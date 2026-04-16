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

#[cfg(test)]
mod tests {
	use super::*;

	// --- parse_duration ---

	#[test]
	fn parse_duration_basic_units() {
		assert_eq!(parse_duration("1s"), Some(1_000));
		assert_eq!(parse_duration("30s"), Some(30_000));
		assert_eq!(parse_duration("1m"), Some(60_000));
		assert_eq!(parse_duration("30m"), Some(1_800_000));
		assert_eq!(parse_duration("1h"), Some(3_600_000));
		assert_eq!(parse_duration("2h"), Some(7_200_000));
		assert_eq!(parse_duration("1d"), Some(86_400_000));
		assert_eq!(parse_duration("3d"), Some(259_200_000));
		assert_eq!(parse_duration("1w"), Some(604_800_000));
	}

	#[test]
	fn parse_duration_rejects_uppercase() {
		// Regex uses lowercase [smhdw], so uppercase units are rejected.
		assert_eq!(parse_duration("1S"), None);
		assert_eq!(parse_duration("30M"), None);
		assert_eq!(parse_duration("2H"), None);
		assert_eq!(parse_duration("3D"), None);
		assert_eq!(parse_duration("1W"), None);
	}

	#[test]
	fn parse_duration_rejects_invalid_strings() {
		assert_eq!(parse_duration(""), None);
		assert_eq!(parse_duration("1y"), None);
		assert_eq!(parse_duration("abc"), None);
		assert_eq!(parse_duration("1.5d"), None);
		assert_eq!(parse_duration("-1h"), None);
		assert_eq!(parse_duration("1"), None);
		assert_eq!(parse_duration("s"), None);
		assert_eq!(parse_duration("1 h"), None);
		assert_eq!(parse_duration("1h30m"), None);
		assert_eq!(parse_duration(" 1h"), None);
		assert_eq!(parse_duration("1h "), None);
	}

	#[test]
	fn parse_duration_zero_is_allowed() {
		// Current behavior: the regex accepts "0d", "0h", etc. and they return 0 ms.
		// (The audit flagged this — zero durations pass through. Noting for the report.)
		assert_eq!(parse_duration("0s"), Some(0));
		assert_eq!(parse_duration("0d"), Some(0));
	}

	#[test]
	fn parse_duration_max_boundary() {
		// 365 days is the documented max.
		assert_eq!(parse_duration("365d"), Some(365 * 86_400_000));
		// 366 days exceeds the max.
		assert_eq!(parse_duration("366d"), None);
		// 52 weeks = 364 days, within limit.
		assert_eq!(parse_duration("52w"), Some(52 * 604_800_000));
		// 53 weeks = 371 days, exceeds limit.
		assert_eq!(parse_duration("53w"), None);
		// Seconds boundary: 365 days in seconds.
		assert_eq!(parse_duration("31536000s"), Some(365 * 86_400 * 1_000));
	}

	#[test]
	fn parse_duration_overflow_returns_none() {
		// Very large numbers that would overflow i64 when multiplied — must not panic.
		assert_eq!(parse_duration("99999999999999999999s"), None);
		// Still valid regex match but multiplication would overflow.
		assert_eq!(parse_duration("9223372036854775807w"), None);
	}

	// --- format_duration_ms ---

	#[test]
	fn format_duration_ms_basic() {
		assert_eq!(format_duration_ms(1_000), "1s");
		assert_eq!(format_duration_ms(30_000), "30s");
		assert_eq!(format_duration_ms(59_000), "59s");
		assert_eq!(format_duration_ms(60_000), "1m");
		assert_eq!(format_duration_ms(3_600_000), "1h");
		assert_eq!(format_duration_ms(86_400_000), "1d");
		assert_eq!(format_duration_ms(604_800_000), "1w");
	}

	#[test]
	fn format_duration_ms_uses_largest_fitting_unit() {
		// 1h30m = 5_400_000 ms → 90m → "1h" (integer division, drops remainder).
		assert_eq!(format_duration_ms(5_400_000), "1h");
		// 2d12h → 2.5 days → "2d".
		assert_eq!(format_duration_ms(86_400_000 * 2 + 3_600_000 * 12), "2d");
		// 10 days → "1w" (70 days / 7 = 10 weeks? No — 10 days = 1 week 3 days → "1w").
		assert_eq!(format_duration_ms(86_400_000 * 10), "1w");
	}

	#[test]
	fn format_duration_ms_zero_and_sub_second() {
		assert_eq!(format_duration_ms(0), "0s");
		assert_eq!(format_duration_ms(500), "0s"); // sub-second truncates to 0s.
	}

	#[test]
	fn format_duration_ms_negative() {
		// Current behavior: the function uses signed `<` comparisons, so any
		// negative value trivially falls into the first branch (`seconds < 60`)
		// and is formatted as "<N>s" with a negative number. Document this
		// quirk — the function isn't really designed for negatives, and an
		// audit might want to clamp to 0.
		assert_eq!(format_duration_ms(-500), "0s");
		assert_eq!(format_duration_ms(-1_000), "-1s");
		assert_eq!(format_duration_ms(-60_000), "-60s");
		assert_eq!(format_duration_ms(-3_600_000), "-3600s");
	}

	// --- format_track_duration ---

	#[test]
	fn format_track_duration_mmss_and_hmmss() {
		assert_eq!(format_track_duration(0), "0:00");
		assert_eq!(format_track_duration(5), "0:05");
		assert_eq!(format_track_duration(65), "1:05");
		assert_eq!(format_track_duration(3_599), "59:59");
		assert_eq!(format_track_duration(3_600), "1:00:00");
		assert_eq!(format_track_duration(3_665), "1:01:05");
	}
}
