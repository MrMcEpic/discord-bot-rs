use regex::Regex;
use std::sync::LazyLock;

static DURATION_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d+)([smhdw])$").unwrap());

/// Maximum duration: 365 days in milliseconds.
const MAX_DURATION_MS: i64 = 365 * 86_400_000;

/// Parse a short duration string like "3d", "2h", "30m" into milliseconds.
/// Returns None on overflow or if the duration exceeds 365 days.
pub fn parse_duration(input: &str) -> Option<i64> {
    let caps = DURATION_RE.captures(input)?;
    let amount: i64 = caps[1].parse().ok()?;
    let unit_ms: i64 = match &caps[2] {
        "s" => 1_000,
        "m" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        "w" => 604_800_000,
        _ => return None,
    };
    let total = amount.checked_mul(unit_ms)?;
    if total > MAX_DURATION_MS {
        return None;
    }
    Some(total)
}

/// Format milliseconds into the largest fitting unit, e.g. "3d", "2h".
pub fn format_duration_ms(ms: i64) -> String {
    let seconds = ms / 1000;
    if seconds < 60 {
        return format!("{seconds}s");
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h");
    }
    let days = hours / 24;
    if days < 7 {
        return format!("{days}d");
    }
    let weeks = days / 7;
    format!("{weeks}w")
}

/// Format seconds into H:MM:SS or M:SS for track durations.
pub fn format_track_duration(seconds: u64) -> String {
    let h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    let s = seconds % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}
