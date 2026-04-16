use std::fmt;

#[derive(Debug)]
pub enum BotError {
	Serenity(serenity::Error),
	Sqlx(sqlx::Error),
	Reqwest(reqwest::Error),
	SerdeJson(serde_json::Error),
	Other(String),
}

impl fmt::Display for BotError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Serenity(e) => write!(f, "Discord error: {e}"),
			Self::Sqlx(e) => write!(f, "Database error: {e}"),
			Self::Reqwest(e) => write!(f, "HTTP error: {e}"),
			Self::SerdeJson(e) => write!(f, "JSON error: {e}"),
			Self::Other(s) => write!(f, "{s}"),
		}
	}
}

impl BotError {
	/// Returns a clean, user-facing message safe to show in Discord.
	///
	/// Internal error details (SQL fragments, URLs, stack traces, etc.) are
	/// kept out of this string. The full error is still logged via
	/// `tracing::error!` for operators.
	pub fn user_message(&self) -> String {
		match self {
			Self::Serenity(_) => "Discord API hiccup. Please try again.".to_string(),
			Self::Sqlx(_) => {
				"Something went wrong talking to the database. Please try again later.".to_string()
			}
			Self::Reqwest(_) => "Couldn't reach an external service. Please try again.".to_string(),
			Self::SerdeJson(_) => {
				"Failed to parse a response from an external service. Please try again.".to_string()
			}
			// `Other` is used throughout the codebase for short curated
			// messages (e.g. "Not in a guild", validation errors). Safe to
			// surface directly.
			Self::Other(s) => s.clone(),
		}
	}
}

impl std::error::Error for BotError {}

impl From<serenity::Error> for BotError {
	fn from(e: serenity::Error) -> Self {
		Self::Serenity(e)
	}
}

impl From<sqlx::Error> for BotError {
	fn from(e: sqlx::Error) -> Self {
		Self::Sqlx(e)
	}
}

impl From<reqwest::Error> for BotError {
	fn from(e: reqwest::Error) -> Self {
		Self::Reqwest(e)
	}
}

impl From<serde_json::Error> for BotError {
	fn from(e: serde_json::Error) -> Self {
		Self::SerdeJson(e)
	}
}

impl From<String> for BotError {
	fn from(s: String) -> Self {
		Self::Other(s)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn other_user_message_is_passthrough() {
		let e = BotError::Other("Not in a guild".to_string());
		assert_eq!(e.user_message(), "Not in a guild");
	}

	#[test]
	fn sqlx_user_message_hides_internals() {
		// Construct an `sqlx::Error` via the `Protocol` variant — no DB needed.
		let e = BotError::Sqlx(sqlx::Error::Protocol("FOO BAR baz_table".into()));
		let msg = e.user_message();
		// Must NOT leak the internal protocol detail.
		assert!(!msg.contains("FOO BAR"));
		assert!(!msg.contains("baz_table"));
		assert!(msg.to_lowercase().contains("database"));
	}

	#[test]
	fn serdejson_user_message_hides_internals() {
		// Force a JSON parse error so we don't have to fabricate a `serde_json::Error`.
		let inner = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
		let e = BotError::SerdeJson(inner);
		let msg = e.user_message();
		assert!(msg.to_lowercase().contains("parse"));
		assert!(!msg.contains("not json"));
	}

	#[test]
	fn from_string_yields_other_variant() {
		let e: BotError = "validation failed".to_string().into();
		match e {
			BotError::Other(s) => assert_eq!(s, "validation failed"),
			_ => panic!("expected Other variant"),
		}
	}

	#[test]
	fn display_includes_inner_for_other() {
		let e = BotError::Other("boom".to_string());
		assert_eq!(format!("{e}"), "boom");
	}

	#[test]
	fn user_message_strings_have_no_trailing_whitespace() {
		// Sanity: every fixed user-facing message should be tidy.
		let cases = [
			BotError::Sqlx(sqlx::Error::Protocol("x".into())),
			BotError::SerdeJson(serde_json::from_str::<serde_json::Value>("x").unwrap_err()),
		];
		for c in &cases {
			let m = c.user_message();
			assert_eq!(m.trim(), m, "user_message has stray whitespace: {m:?}");
			assert!(!m.is_empty());
		}
	}
}
