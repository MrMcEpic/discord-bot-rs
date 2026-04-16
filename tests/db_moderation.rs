//! Postgres-backed integration tests for the tempban moderation queries.
//!
//! Covers the full lifecycle: `create_tempban` writes a row,
//! `get_active_bans` returns it while it's active, `mark_unbanned`
//! flips the flag, and the active-bans query no longer returns it
//! afterwards. Also pins down `get_expired_bans` against the wall
//! clock (via a tiny duration) and `mark_unbanned_by_id`.

use sqlx::PgPool;
use std::time::Duration;

use discord_bot::db::queries;

const G: &str = "guild";
const U: &str = "user";
const M: &str = "mod";

#[sqlx::test(migrations = "./migrations")]
async fn tempban_lifecycle_create_then_unban(pool: PgPool) {
	let expires = queries::create_tempban(&pool, G, U, M, 60_000, Some("test reason"))
		.await
		.unwrap();
	assert!(expires > chrono::Utc::now());

	let active = queries::get_active_bans(&pool, G).await.unwrap();
	assert_eq!(active.len(), 1, "ban should be active");
	let ban = &active[0];
	assert_eq!(ban.user_id, U);
	assert_eq!(ban.moderator_id, M);
	assert_eq!(ban.reason.as_deref(), Some("test reason"));
	assert!(!ban.unbanned);

	let did_unban = queries::mark_unbanned(&pool, G, U).await.unwrap();
	assert!(did_unban, "first mark_unbanned should report a row update");

	let active = queries::get_active_bans(&pool, G).await.unwrap();
	assert!(active.is_empty(), "no active bans after mark_unbanned");

	// Idempotency: marking again on an already-unbanned row returns false.
	let did_again = queries::mark_unbanned(&pool, G, U).await.unwrap();
	assert!(!did_again, "second mark_unbanned should be a no-op");
}

#[sqlx::test(migrations = "./migrations")]
async fn get_active_bans_excludes_other_guilds(pool: PgPool) {
	queries::create_tempban(&pool, "g1", U, M, 60_000, None)
		.await
		.unwrap();
	queries::create_tempban(&pool, "g2", U, M, 60_000, None)
		.await
		.unwrap();

	let g1 = queries::get_active_bans(&pool, "g1").await.unwrap();
	assert_eq!(g1.len(), 1);
	let g2 = queries::get_active_bans(&pool, "g2").await.unwrap();
	assert_eq!(g2.len(), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn get_expired_bans_returns_only_past_expiry(pool: PgPool) {
	// 50ms expiry — by the time we query in a moment, it should be expired.
	queries::create_tempban(&pool, G, "expired-user", M, 50, None)
		.await
		.unwrap();
	// Far-future expiry — still active.
	queries::create_tempban(&pool, G, "active-user", M, 600_000, None)
		.await
		.unwrap();

	tokio::time::sleep(Duration::from_millis(150)).await;

	let expired = queries::get_expired_bans(&pool).await.unwrap();
	assert_eq!(expired.len(), 1, "exactly one ban should be expired");
	assert_eq!(expired[0].user_id, "expired-user");

	// And the active list for this guild should now only show the future one.
	let active = queries::get_active_bans(&pool, G).await.unwrap();
	assert_eq!(active.len(), 1);
	assert_eq!(active[0].user_id, "active-user");

	// mark_unbanned_by_id removes it from get_expired_bans.
	let id = expired[0].id;
	queries::mark_unbanned_by_id(&pool, id).await.unwrap();
	let expired = queries::get_expired_bans(&pool).await.unwrap();
	assert!(expired.is_empty());
}
