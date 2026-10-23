use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::error::Result;
use crate::profile::Profile;
use crate::tools::{ToolCall, ToolSettings};
use crate::vm::{Patch, PatchRun, builtin_patches};

const ACTIVE_PROFILE: &str = "active_profile";
const PRESET_VERSION: &str = "preset_version";

/// Local SQLite store. Holds key references, never key values.
pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        let version: i64 = self.conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version < 1 {
            self.conn.execute_batch(
                "BEGIN;
                 CREATE TABLE profiles (
                   id TEXT PRIMARY KEY,
                   data TEXT NOT NULL,
                   updated_at INTEGER NOT NULL
                 );
                 CREATE TABLE settings (
                   key TEXT PRIMARY KEY,
                   value TEXT NOT NULL
                 );
                 PRAGMA user_version = 1;
                 COMMIT;",
            )?;
        }
        if version < 2 {
            // trim_seq: messages below it are kept for display but left out of
            // requests. It only moves forward, so the prompt prefix stays stable.
            self.conn.execute_batch(
                "BEGIN;
                 CREATE TABLE sessions (
                   id TEXT PRIMARY KEY,
                   title TEXT NOT NULL,
                   profile_id TEXT,
                   system_prompt TEXT NOT NULL,
                   trim_seq INTEGER NOT NULL DEFAULT 0,
                   created_at INTEGER NOT NULL,
                   updated_at INTEGER NOT NULL
                 );
                 CREATE TABLE messages (
                   id TEXT PRIMARY KEY,
                   session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                   seq INTEGER NOT NULL,
                   role TEXT NOT NULL,
                   content TEXT NOT NULL,
                   images TEXT,
                   reasoning TEXT,
                   meta TEXT,
                   created_at INTEGER NOT NULL,
                   UNIQUE (session_id, seq)
                 );
                 PRAGMA user_version = 2;
                 COMMIT;",
            )?;
        }
        if version < 3 {
            self.conn.execute_batch(
                "BEGIN;
                 ALTER TABLE messages ADD COLUMN tool_calls TEXT;
                 ALTER TABLE messages ADD COLUMN tool_call_id TEXT;
                 PRAGMA user_version = 3;
                 COMMIT;",
            )?;
        }
        if version < 4 {
            // title_source: "first" (from the first message), "auto" (model) or "user".
            self.conn.execute_batch(
                "BEGIN;
                 ALTER TABLE sessions ADD COLUMN title_source TEXT NOT NULL DEFAULT 'first';
                 PRAGMA user_version = 4;
                 COMMIT;",
            )?;
        }
        if version < 5 {
            // Compaction: memory is the session's running summary; messages
            // below strip_seq are sent shortened. Both only move forward.
            self.conn.execute_batch(
                "BEGIN;
                 ALTER TABLE sessions ADD COLUMN memory TEXT NOT NULL DEFAULT '';
                 ALTER TABLE sessions ADD COLUMN strip_seq INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE sessions ADD COLUMN compactions INTEGER NOT NULL DEFAULT 0;
                 PRAGMA user_version = 5;
                 COMMIT;",
            )?;
        }
        Ok(())
    }

    pub fn tool_settings(&self) -> Result<ToolSettings> {
        Ok(match self.setting(TOOL_SETTINGS)? {
            Some(s) => serde_json::from_str(&s).unwrap_or_default(),
            None => ToolSettings::default(),
        })
    }

    pub fn set_tool_settings(&self, t: &ToolSettings) -> Result<()> {
        self.set_setting(TOOL_SETTINGS, &serde_json::to_string(t)?)
    }

    /// Built-in patches first (their script always from the app), then custom ones.
    pub fn patches(&self) -> Result<Vec<Patch>> {
        let stored: Vec<Patch> = match self.setting(PATCHES)? {
            Some(s) => serde_json::from_str(&s).unwrap_or_default(),
            None => Vec::new(),
        };
        let mut out = builtin_patches();
        for b in &mut out {
            if let Some(s) = stored.iter().find(|s| s.id == b.id) {
                b.auto_run = s.auto_run;
                b.last_run = s.last_run.clone();
            }
        }
        let builtin_ids: Vec<String> = out.iter().map(|p| p.id.clone()).collect();
        out.extend(stored.into_iter().filter(|s| !builtin_ids.contains(&s.id)).map(|mut s| {
            s.builtin = false;
            s
        }));
        Ok(out)
    }

    pub fn save_patches(&self, list: &[Patch]) -> Result<()> {
        self.set_setting(PATCHES, &serde_json::to_string(list)?)
    }

    pub fn record_patch_runs(&self, runs: &[(String, PatchRun)]) -> Result<()> {
        if runs.is_empty() {
            return Ok(());
        }
        let mut list = self.patches()?;
        for (id, run) in runs {
            if let Some(p) = list.iter_mut().find(|p| &p.id == id) {
                p.last_run = Some(run.clone());
            }
        }
        self.save_patches(&list)
    }

    // ---- Sessions and messages (M2) ----

    pub fn sessions(&self) -> Result<Vec<SessionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, profile_id, trim_seq, created_at, updated_at, strip_seq, compactions, memory
             FROM sessions ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], SessionRow::from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn session(&self, id: &str) -> Result<Option<(SessionRow, String)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, title, profile_id, trim_seq, created_at, updated_at, strip_seq, compactions, memory, system_prompt
                 FROM sessions WHERE id = ?1",
                [id],
                |r| Ok((SessionRow::from_row(r)?, r.get(9)?)),
            )
            .optional()?)
    }

    pub fn create_session(&self, id: &str, profile_id: Option<&str>, system_prompt: &str) -> Result<SessionRow> {
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO sessions (id, title, profile_id, system_prompt, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![id, NEW_SESSION_TITLE, profile_id, system_prompt, now],
        )?;
        Ok(SessionRow {
            id: id.into(),
            title: NEW_SESSION_TITLE.into(),
            profile_id: profile_id.map(Into::into),
            trim_seq: 0,
            created_at: now,
            updated_at: now,
            strip_seq: 0,
            compactions: 0,
            memory: String::new(),
        })
    }

    /// A rename by the user. Auto titles never replace it.
    pub fn rename_session(&self, id: &str, title: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET title = ?2, title_source = 'user', updated_at = ?3 WHERE id = ?1",
            params![id, title, now_ms()],
        )?;
        Ok(())
    }

    /// True when the title still comes from the first message.
    pub fn wants_auto_title(&self, id: &str) -> Result<bool> {
        Ok(self
            .conn
            .query_row("SELECT title_source = 'first' FROM sessions WHERE id = ?1", [id], |r| r.get(0))
            .optional()?
            .unwrap_or(false))
    }

    pub fn set_auto_title(&self, id: &str, title: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET title = ?2, title_source = 'auto' WHERE id = ?1 AND title_source = 'first'",
            params![id, title],
        )?;
        Ok(())
    }

    pub fn delete_session(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM sessions WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn set_trim_seq(&self, id: &str, seq: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET trim_seq = MAX(trim_seq, ?2) WHERE id = ?1",
            params![id, seq],
        )?;
        Ok(())
    }

    /// Record a compaction: move the window and strip point (never back) and
    /// replace the memory.
    pub fn compact(&self, id: &str, trim_seq: i64, strip_seq: i64, memory: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET trim_seq = MAX(trim_seq, ?2), strip_seq = MAX(strip_seq, ?3),
               memory = ?4, compactions = compactions + 1 WHERE id = ?1",
            params![id, trim_seq, strip_seq, memory],
        )?;
        Ok(())
    }

    /// Edited by the user in the Memory pane.
    pub fn set_memory(&self, id: &str, memory: &str) -> Result<()> {
        self.conn.execute("UPDATE sessions SET memory = ?2 WHERE id = ?1", params![id, memory])?;
        Ok(())
    }

    /// Messages with seq >= `from`, in order.
    pub fn messages(&self, session_id: &str, from: i64) -> Result<Vec<MessageRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, seq, role, content, images, reasoning, meta, created_at, tool_calls, tool_call_id
             FROM messages WHERE session_id = ?1 AND seq >= ?2 ORDER BY seq",
        )?;
        let rows = stmt.query_map(params![session_id, from], |r| {
            let images: Option<String> = r.get(4)?;
            let meta: Option<String> = r.get(6)?;
            let calls: Option<String> = r.get(8)?;
            Ok(MessageRow {
                id: r.get(0)?,
                seq: r.get(1)?,
                role: r.get(2)?,
                content: r.get(3)?,
                images: images.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
                reasoning: r.get(5)?,
                meta: meta.and_then(|s| serde_json::from_str(&s).ok()),
                created_at: r.get(7)?,
                tool_calls: calls.and_then(|s| serde_json::from_str(&s).ok()),
                tool_call_id: r.get(9)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Append a message, assigning the next seq. Returns the stored row.
    pub fn add_message(&self, session_id: &str, mut m: MessageRow) -> Result<MessageRow> {
        let tx = self.conn.unchecked_transaction()?;
        m.seq = tx.query_row(
            "SELECT COALESCE(MAX(seq), -1) + 1 FROM messages WHERE session_id = ?1",
            [session_id],
            |r| r.get(0),
        )?;
        m.created_at = now_ms();
        let images = (!m.images.is_empty()).then(|| serde_json::to_string(&m.images)).transpose()?;
        let meta = m.meta.as_ref().map(serde_json::to_string).transpose()?;
        let calls = m.tool_calls.as_ref().map(serde_json::to_string).transpose()?;
        tx.execute(
            "INSERT INTO messages (id, session_id, seq, role, content, images, reasoning, meta, created_at, tool_calls, tool_call_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![m.id, session_id, m.seq, m.role, m.content, images, m.reasoning, meta, m.created_at, calls, m.tool_call_id],
        )?;
        // First user message names the session.
        if m.role == "user" {
            tx.execute(
                "UPDATE sessions SET title = ?2 WHERE id = ?1 AND title = ?3",
                params![session_id, title_from(&m), NEW_SESSION_TITLE],
            )?;
        }
        tx.execute("UPDATE sessions SET updated_at = ?2 WHERE id = ?1", params![session_id, m.created_at])?;
        tx.commit()?;
        Ok(m)
    }

    /// First run: add the local preset and make it active. Later runs:
    /// one-time fixes for older installs.
    pub fn seed(&self) -> Result<()> {
        if self.profiles()?.is_empty() {
            let p = Profile::local_default();
            self.save_profile(&p)?;
            self.set_setting(ACTIVE_PROFILE, &p.id)?;
            self.set_setting(PRESET_VERSION, "3")?;
            return Ok(());
        }
        if self.setting(PRESET_VERSION)?.as_deref() != Some("3") {
            // 1Password was removed in 0.7: point those profiles at the Keychain.
            for mut p in self.profiles()? {
                if p.key_ref.as_deref().is_some_and(|k| k.starts_with("op://")) {
                    p.key_ref = Some(format!("keychain:{}/{}", crate::secrets::KEYCHAIN_SERVICE, p.id));
                    self.save_profile(&p)?;
                }
            }
            self.set_setting(PRESET_VERSION, "3")?;
        }
        Ok(())
    }

    pub fn profiles(&self) -> Result<Vec<Profile>> {
        let mut stmt = self.conn.prepare("SELECT data FROM profiles ORDER BY rowid")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_str(&row?)?);
        }
        Ok(out)
    }

    pub fn profile(&self, id: &str) -> Result<Option<Profile>> {
        let data: Option<String> = self
            .conn
            .query_row("SELECT data FROM profiles WHERE id = ?1", [id], |r| r.get(0))
            .optional()?;
        Ok(match data {
            Some(d) => Some(serde_json::from_str(&d)?),
            None => None,
        })
    }

    pub fn save_profile(&self, p: &Profile) -> Result<()> {
        self.conn.execute(
            "INSERT INTO profiles (id, data, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET data = excluded.data, updated_at = excluded.updated_at",
            params![p.id, serde_json::to_string(p)?, now_ms()],
        )?;
        Ok(())
    }

    pub fn delete_profile(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM profiles WHERE id = ?1", [id])?;
        if self.active_profile_id()?.as_deref() == Some(id) {
            match self.profiles()?.first() {
                Some(p) => self.set_setting(ACTIVE_PROFILE, &p.id)?,
                None => {
                    self.conn.execute("DELETE FROM settings WHERE key = ?1", [ACTIVE_PROFILE])?;
                }
            }
        }
        Ok(())
    }

    pub fn active_profile_id(&self) -> Result<Option<String>> {
        self.setting(ACTIVE_PROFILE)
    }

    pub fn set_active_profile(&self, id: &str) -> Result<()> {
        self.set_setting(ACTIVE_PROFILE, id)
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

pub const NEW_SESSION_TITLE: &str = "New session";
const TOOL_SETTINGS: &str = "tools";
const PATCHES: &str = "patches";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRow {
    pub id: String,
    pub title: String,
    pub profile_id: Option<String>,
    pub trim_seq: i64,
    pub created_at: i64,
    pub updated_at: i64,
    /// Messages below this seq are sent shortened (heavy parts removed).
    pub strip_seq: i64,
    pub compactions: i64,
    /// Session memory: the running summary that replaces dropped history.
    pub memory: String,
}

impl SessionRow {
    fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: r.get(0)?,
            title: r.get(1)?,
            profile_id: r.get(2)?,
            trim_seq: r.get(3)?,
            created_at: r.get(4)?,
            updated_at: r.get(5)?,
            strip_seq: r.get(6)?,
            compactions: r.get(7)?,
            memory: r.get(8)?,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageRow {
    pub id: String,
    pub seq: i64,
    /// "user", "assistant" or "tool".
    pub role: String,
    pub content: String,
    /// Image data URIs, already downscaled by the UI.
    pub images: Vec<String>,
    pub reasoning: Option<String>,
    /// Usage, timings and flags for assistant messages.
    pub meta: Option<serde_json::Value>,
    pub created_at: i64,
    /// Calls an assistant message asked for.
    pub tool_calls: Option<Vec<ToolCall>>,
    /// For role "tool": the call this answers.
    pub tool_call_id: Option<String>,
}

impl MessageRow {
    pub fn new(role: &str, content: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            seq: 0,
            role: role.into(),
            content,
            images: Vec::new(),
            reasoning: None,
            meta: None,
            created_at: 0,
            tool_calls: None,
            tool_call_id: None,
        }
    }
}

fn title_from(m: &MessageRow) -> String {
    let line = m.content.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    if line.is_empty() {
        return if m.images.is_empty() { NEW_SESSION_TITLE.into() } else { "Image".into() };
    }
    let mut t: String = line.chars().take(48).collect();
    if line.chars().count() > 48 {
        t.push('…');
    }
    t
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_save_delete() {
        let s = Store::open_in_memory().unwrap();
        s.seed().unwrap();
        assert_eq!(s.profiles().unwrap().len(), 1);
        assert_eq!(s.active_profile_id().unwrap().as_deref(), Some("local"));

        let mut p = Profile::local_default();
        p.id = "second".into();
        p.name = "Second".into();
        s.save_profile(&p).unwrap();
        p.name = "Local 2".into();
        s.save_profile(&p).unwrap();
        let all = s.profiles().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[1].name, "Local 2");

        s.delete_profile("local").unwrap();
        assert_eq!(s.active_profile_id().unwrap().as_deref(), Some("second"));
    }

    #[test]
    fn op_refs_move_to_keychain() {
        let s = Store::open_in_memory().unwrap();
        let mut p = Profile::local_default();
        p.key_ref = Some("op://vault/item/field".into());
        s.save_profile(&p).unwrap();
        s.set_setting(PRESET_VERSION, "2").unwrap();
        s.seed().unwrap();
        assert_eq!(s.profile("local").unwrap().unwrap().key_ref.as_deref(), Some("keychain:qwen35-harness/local"));
    }

    #[test]
    fn sessions_and_messages() {
        let s = Store::open_in_memory().unwrap();
        s.create_session("s1", Some("local"), "sys").unwrap();

        let u = s.add_message("s1", MessageRow::new("user", "  \nHello there, model\nmore".into())).unwrap();
        let a = s.add_message("s1", MessageRow::new("assistant", "Hi".into())).unwrap();
        assert_eq!((u.seq, a.seq), (0, 1));

        let (row, sys) = s.session("s1").unwrap().unwrap();
        assert_eq!(row.title, "Hello there, model");
        assert_eq!(sys, "sys");

        // A second user message does not rename.
        s.add_message("s1", MessageRow::new("user", "Next".into())).unwrap();
        assert_eq!(s.session("s1").unwrap().unwrap().0.title, "Hello there, model");

        assert!(s.wants_auto_title("s1").unwrap());
        s.set_auto_title("s1", "Auto title").unwrap();
        assert!(!s.wants_auto_title("s1").unwrap());
        s.rename_session("s1", "Mine").unwrap();
        s.set_auto_title("s1", "Ignored").unwrap();
        assert_eq!(s.session("s1").unwrap().unwrap().0.title, "Mine");
        s.compact("s1", 1, 2, "## Goal\n- test").unwrap();
        s.compact("s1", 0, 1, "## Goal\n- newer").unwrap(); // window and strip never move back
        let (row, _) = s.session("s1").unwrap().unwrap();
        assert_eq!((row.trim_seq, row.strip_seq, row.compactions, row.memory.as_str()), (1, 2, 2, "## Goal\n- newer"));
        s.set_trim_seq("s1", 2).unwrap();
        s.set_trim_seq("s1", 1).unwrap(); // never moves back
        assert_eq!(s.session("s1").unwrap().unwrap().0.trim_seq, 2);
        assert_eq!(s.messages("s1", 2).unwrap().len(), 1);

        let mut a = MessageRow::new("assistant", String::new());
        a.tool_calls = Some(vec![ToolCall { id: "c1".into(), name: "sh".into(), arguments: "{}".into() }]);
        s.add_message("s1", a).unwrap();
        let mut tmsg = MessageRow::new("tool", "exit=0".into());
        tmsg.tool_call_id = Some("c1".into());
        s.add_message("s1", tmsg).unwrap();
        let all = s.messages("s1", 0).unwrap();
        assert_eq!(all[all.len() - 2].tool_calls.as_ref().unwrap()[0].name, "sh");
        assert_eq!(all[all.len() - 1].tool_call_id.as_deref(), Some("c1"));

        assert!(!s.tool_settings().unwrap().auto_approve);
        s.set_tool_settings(&ToolSettings { auto_approve: true, ..Default::default() }).unwrap();
        assert!(s.tool_settings().unwrap().auto_approve);

        // Patches: built-ins always present; their script can't be overridden.
        let mut list = s.patches().unwrap();
        assert_eq!(list[0].id, "system-updates");
        list[0].script = "rm -rf /".into();
        list[0].auto_run = true;
        list.push(Patch { id: "ffmpeg".into(), name: "ffmpeg".into(), script: "apt-get install -y ffmpeg".into(), auto_run: true, builtin: false, last_run: None });
        s.save_patches(&list).unwrap();
        let list = s.patches().unwrap();
        assert!(list[0].auto_run && list[0].script.contains("apt-get update"));
        assert_eq!(list.len(), 2);
        s.record_patch_runs(&[("ffmpeg".into(), PatchRun { at: 1, ok: true, ms: 5, summary: "Done".into() })]).unwrap();
        assert!(s.patches().unwrap()[1].last_run.as_ref().unwrap().ok);

        s.delete_session("s1").unwrap();
        assert!(s.messages("s1", 0).unwrap().is_empty(), "messages cascade");
    }
}
