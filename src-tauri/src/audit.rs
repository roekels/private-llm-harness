//! Audit export: one session as a CSV file, one row per message, reasoning
//! block, tool call and tool result, in order. Includes the system prompt and
//! the session memory. Images are noted, not embedded.

use std::path::{Path, PathBuf};

use crate::clock;
use crate::error::{Result, msg};
use crate::store::{MessageRow, SessionRow};

const HEADER: [&str; 15] = [
    "session", "seq", "time", "role", "kind", "tool", "call_id", "approval", "outcome", "exit_code", "duration_ms",
    "prompt_tokens", "cached_tokens", "output_tokens", "content",
];

#[derive(Default)]
struct Row {
    seq: String,
    time: String,
    role: String,
    kind: &'static str,
    tool: String,
    call_id: String,
    approval: String,
    outcome: String,
    exit_code: String,
    duration_ms: String,
    prompt_tokens: String,
    cached_tokens: String,
    output_tokens: String,
    content: String,
}

/// The CSV text for a session.
pub fn csv(session: &SessionRow, system_prompt: &str, messages: &[MessageRow]) -> String {
    let mut rows = vec![Row { kind: "system_prompt", role: "system".into(), content: system_prompt.into(), ..Row::default() }];
    if !session.memory.trim().is_empty() {
        rows.push(Row { kind: "session_memory", role: "system".into(), content: session.memory.clone(), ..Row::default() });
    }
    for m in messages {
        let base = || Row { seq: m.seq.to_string(), time: clock::iso_stamp(m.created_at), role: m.role.clone(), ..Row::default() };
        let meta = m.meta.as_ref();
        let num = |k: &str| meta.and_then(|v| v.get(k)).filter(|v| !v.is_null()).map(|v| v.to_string()).unwrap_or_default();
        let text = |k: &str| meta.and_then(|v| v.get(k)).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let images = if m.images.is_empty() { String::new() } else { format!("\n[{} image(s) attached]", m.images.len()) };
        match m.role.as_str() {
            "tool" => rows.push(Row {
                kind: "tool_result",
                tool: text("name"),
                call_id: m.tool_call_id.clone().unwrap_or_default(),
                approval: text("approval"),
                outcome: text("outcome"),
                exit_code: num("exitCode"),
                duration_ms: num("durationMs"),
                content: format!("{}{images}", m.content),
                ..base()
            }),
            "assistant" => {
                if let Some(r) = m.reasoning.as_ref().filter(|r| !r.trim().is_empty()) {
                    rows.push(Row { kind: "reasoning", content: r.clone(), ..base() });
                }
                if !m.content.is_empty() || m.tool_calls.as_ref().is_none_or(|c| c.is_empty()) {
                    rows.push(Row {
                        kind: "message",
                        prompt_tokens: num("promptTokens"),
                        cached_tokens: num("cachedTokens"),
                        output_tokens: num("completionTokens"),
                        content: m.content.clone(),
                        ..base()
                    });
                }
                for c in m.tool_calls.iter().flatten() {
                    rows.push(Row { kind: "tool_call", tool: c.name.clone(), call_id: c.id.clone(), content: c.arguments.clone(), ..base() });
                }
            }
            _ => rows.push(Row { kind: "message", content: format!("{}{images}", m.content), ..base() }),
        }
    }

    // UTF-8 BOM so spreadsheet apps read accents and symbols correctly.
    let mut out = String::from("\u{feff}");
    out.push_str(&line(HEADER.iter().map(|h| h.to_string()).collect()));
    for r in rows {
        out.push_str(&line(vec![
            session.id.clone(),
            r.seq,
            r.time,
            r.role,
            r.kind.into(),
            r.tool,
            r.call_id,
            r.approval,
            r.outcome,
            r.exit_code,
            r.duration_ms,
            r.prompt_tokens,
            r.cached_tokens,
            r.output_tokens,
            defuse(&r.content),
        ]));
    }
    out
}

/// Write the CSV to `downloads`. Returns the file's path.
pub fn export(downloads: &Path, session: &SessionRow, system_prompt: &str, messages: &[MessageRow]) -> Result<PathBuf> {
    std::fs::create_dir_all(downloads)?;
    let slug: String = session
        .title
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-");
    let slug = if slug.is_empty() { "session".to_string() } else { slug };
    let path = downloads.join(format!("harness-audit-{slug}-{}.csv", clock::file_stamp(clock::now_ms())));
    if path.exists() {
        return Err(msg("An export with this name already exists. Try again in a second."));
    }
    std::fs::write(&path, csv(session, system_prompt, messages))?;
    Ok(path)
}

/// RFC 4180: every field quoted, quotes doubled, CRLF line ends.
fn line(fields: Vec<String>) -> String {
    let mut s = fields.iter().map(|f| format!("\"{}\"", f.replace('"', "\"\""))).collect::<Vec<_>>().join(",");
    s.push_str("\r\n");
    s
}

/// Model and web text can start with "=", "+", "-" or "@", which spreadsheet
/// apps run as a formula. A leading apostrophe keeps it as text.
fn defuse(s: &str) -> String {
    if s.starts_with(['=', '+', '-', '@', '\t', '\r']) { format!("'{s}") } else { s.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolCall;

    fn row(seq: i64, role: &str, content: &str) -> MessageRow {
        let mut m = MessageRow::new(role, content.to_string());
        m.seq = seq;
        m.created_at = 1_792_756_800_000;
        m
    }

    #[test]
    fn rows_for_every_part() {
        let s = SessionRow { id: "s1".into(), title: "Fix the \"build\"".into(), profile_id: None, trim_seq: 0, created_at: 0, updated_at: 0, strip_seq: 0, compactions: 0, memory: "Goal: x".into() };
        let mut a = row(2, "assistant", "");
        a.reasoning = Some("thinking".into());
        a.tool_calls = Some(vec![ToolCall { id: "c1".into(), name: "sh".into(), arguments: r#"{"cmd":"ls"}"#.into() }]);
        let mut t = row(3, "tool", "=HYPERLINK(\"x\")");
        t.tool_call_id = Some("c1".into());
        t.meta = Some(serde_json::json!({"name":"sh","outcome":"ok","exitCode":0,"durationMs":12,"approval":"turn"}));
        let mut f = row(4, "assistant", "Done, \"quoted\"\nnext line");
        f.meta = Some(serde_json::json!({"promptTokens":100,"cachedTokens":90,"completionTokens":5}));
        let mut u = row(1, "user", "list files");
        u.images = vec!["data:x".into()];
        let out = csv(&s, "SYSTEM", &[u, a, t, f]);
        let lines: Vec<&str> = out.trim_start_matches('\u{feff}').split("\r\n").filter(|l| !l.is_empty()).collect();
        assert!(lines[0].starts_with("\"session\",\"seq\""));
        for k in ["system_prompt", "session_memory", "message", "reasoning", "tool_call", "tool_result"] {
            assert!(out.contains(&format!("\"{k}\"")), "{k}");
        }
        assert_eq!(lines.iter().filter(|l| l.starts_with("\"s1\"")).count(), 7, "one row per part");
        assert!(out.contains("[1 image(s) attached]"));
        assert!(out.contains("\"'=HYPERLINK(\"\"x\"\")\""), "formula defused and quotes doubled");
        assert!(out.contains("\"turn\""));
        assert!(out.contains("\"Done, \"\"quoted\"\"\nnext line\""), "newlines stay inside the quoted field");
        assert!(out.contains("\"100\",\"90\",\"5\""));
    }
}
