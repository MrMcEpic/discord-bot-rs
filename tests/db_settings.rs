//! Postgres-backed integration tests for `upsert_guild_setting`.
//!
//! `upsert_guild_setting` interpolates the column name straight into
//! the SQL string, which would normally be a SQL injection landmine.
//! The defence is the hard-coded `ALLOWED_COLUMNS` allowlist: any key
//! not on that list is rejected at the Rust layer before a query is
//! ever sent. These tests pin that allowlist down — both that the
//! documented keys work, and that anything else (including obvious
//! injection attempts) is bounced.

use sqlx::PgPool;

use discord_bot::db::queries;

const G: &str = "guild";

#[sqlx::test(migrations = "./migrations")]
async fn upsert_guild_setting_accepts_audit_log_channel_id(pool: PgPool) {
	queries::upsert_guild_setting(&pool, G, "audit_log_channel_id", "12345")
		.await
		.expect("audit_log_channel_id is a documented key");

	let s = queries::get_guild_settings(&pool, G).await.unwrap();
	assert_eq!(s.audit_log_channel_id.as_deref(), Some("12345"));

	// Upsert again with a new value — same row, updated.
	queries::upsert_guild_setting(&pool, G, "audit_log_channel_id", "67890")
		.await
		.unwrap();
	let s = queries::get_guild_settings(&pool, G).await.unwrap();
	assert_eq!(s.audit_log_channel_id.as_deref(), Some("67890"));
}

#[sqlx::test(migrations = "./migrations")]
async fn upsert_guild_setting_accepts_dj_role_id(pool: PgPool) {
	queries::upsert_guild_setting(&pool, G, "dj_role_id", "role-1")
		.await
		.unwrap();
	let s = queries::get_guild_settings(&pool, G).await.unwrap();
	assert_eq!(s.dj_role_id.as_deref(), Some("role-1"));
}

#[sqlx::test(migrations = "./migrations")]
async fn upsert_guild_setting_bool_accepts_dj_mode_enabled(pool: PgPool) {
	queries::upsert_guild_setting_bool(&pool, G, "dj_mode_enabled", true)
		.await
		.unwrap();
	let s = queries::get_guild_settings(&pool, G).await.unwrap();
	assert!(s.dj_mode_enabled);

	queries::upsert_guild_setting_bool(&pool, G, "dj_mode_enabled", false)
		.await
		.unwrap();
	let s = queries::get_guild_settings(&pool, G).await.unwrap();
	assert!(!s.dj_mode_enabled);
}

#[sqlx::test(migrations = "./migrations")]
async fn upsert_guild_setting_rejects_unknown_keys(pool: PgPool) {
	for bad_key in [
		"created_at",
		"guild_id",                 // PK — can't update via this helper
		"foo",                      // total nonsense
		"audit_log_channel_id; --", // SQL injection attempt
		"audit_log_channel_id,dj_role_id",
		"",
	] {
		let err = queries::upsert_guild_setting(&pool, G, bad_key, "value")
			.await
			.expect_err(&format!("key {bad_key:?} should be rejected"));
		let msg = format!("{err}");
		assert!(
			msg.contains("Invalid setting key"),
			"key {bad_key:?}: unexpected error: {msg}"
		);
	}

	// And the bool variant has the same allowlist.
	let err = queries::upsert_guild_setting_bool(&pool, G, "made_up_flag", true)
		.await
		.expect_err("unknown bool key should be rejected");
	assert!(format!("{err}").contains("Invalid setting key"));
}

#[sqlx::test(migrations = "./migrations")]
async fn get_guild_settings_returns_none_for_missing_guild(pool: PgPool) {
	let s = queries::get_guild_settings(&pool, "no-such-guild").await;
	assert!(s.is_none());
}
