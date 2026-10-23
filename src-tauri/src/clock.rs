//! Local date and time stamps for prompts (no extra crates: libc localtime_r).

use std::ffi::CStr;

const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November",
    "December",
];

struct Local {
    wday: usize,
    day: i32,
    month: usize,
    year: i32,
    hour: i32,
    min: i32,
    sec: i32,
    zone: String,
}

fn local(ms: i64) -> Local {
    let secs = (ms / 1000) as libc::time_t;
    // SAFETY: localtime_r fills the struct we own; tm_zone points to static tz data.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&secs, &mut tm) };
    let zone = if tm.tm_zone.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(tm.tm_zone) }.to_string_lossy().into_owned()
    };
    Local {
        wday: tm.tm_wday.clamp(0, 6) as usize,
        day: tm.tm_mday,
        month: tm.tm_mon.clamp(0, 11) as usize,
        year: 1900 + tm.tm_year,
        hour: tm.tm_hour,
        min: tm.tm_min,
        sec: tm.tm_sec,
        zone,
    }
}

/// "Thursday 23 October 2026"
pub fn long_date(ms: i64) -> String {
    let t = local(ms);
    format!("{} {} {} {}", DAYS[t.wday], t.day, MONTHS[t.month], t.year)
}

/// "Thu 23 Oct 2026 10:42 CEST", for the start of each user message.
pub fn short_stamp(ms: i64) -> String {
    let t = local(ms);
    let zone = if t.zone.is_empty() { String::new() } else { format!(" {}", t.zone) };
    format!("{} {} {} {} {:02}:{:02}{zone}", &DAYS[t.wday][..3], t.day, &MONTHS[t.month][..3], t.year, t.hour, t.min)
}

/// "2026-10-23 14:02:39 CEST", for exports.
pub fn iso_stamp(ms: i64) -> String {
    let t = local(ms);
    let zone = if t.zone.is_empty() { String::new() } else { format!(" {}", t.zone) };
    format!("{}-{:02}-{:02} {:02}:{:02}:{:02}{zone}", t.year, t.month + 1, t.day, t.hour, t.min, t.sec)
}

/// "20261023-140239", for file names.
pub fn file_stamp(ms: i64) -> String {
    let t = local(ms);
    format!("{}{:02}{:02}-{:02}{:02}{:02}", t.year, t.month + 1, t.day, t.hour, t.min, t.sec)
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        // 2026-10-23 08:42:00 UTC
        let ms = 1_792_744_920_000;
        assert!(long_date(ms).ends_with("October 2026"), "{}", long_date(ms));
        let s = short_stamp(ms);
        assert!(s.contains("Oct 2026") && s.contains(':'), "{s}");
        assert_eq!(short_stamp(ms), short_stamp(ms), "deterministic, so history renders the same every turn");
    }
}
