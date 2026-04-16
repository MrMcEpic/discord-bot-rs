//! Postgres-backed integration tests for the auto-role promotion claim.
//!
//! The Tier 2.6 fix made `try_claim_promotion` atomic via a single
//! conditional UPDATE that returns the row only if it was previously
//! `promoted = FALSE`. The race we're guarding against: the message
//! handler and the periodic background scanner can both try to promote
//! the same user simultaneously. Without the atomic claim, both would
//! observe `promoted = FALSE`, both would call `add_role`, and Discord
//! would log two duplicate audit events (or worse — diverge if a role
//! removal sat between the two).
//!
//! The contract under test: across N concurrent `try_claim_promotion`
//! calls for the same `(guild_id, user_id)`, exactly one returns `true`
//! and the rest return `false`.

use sqlx::PgPool;
use std::sync::Arc;

use discord_bot::db::queries;

#[sqlx::test(migrations = "./migrations")]
async fn try_claim_promotion_only_one_winner_under_concurrency(pool: PgPool) {
	const ITERATIONS: usize = 5;
	const PARALLELISM: usize = 16;

	let pool = Arc::new(pool);

	for i in 0..ITERATIONS {
		let guild = format!("g-{i}");
		let user = format!("u-{i}");

		// Seed the activity row so there's something to claim.
		queries::increment_message_count(&pool, &guild, &user)
			.await
			.unwrap();

		let mut handles = Vec::with_capacity(PARALLELISM);
		for _ in 0..PARALLELISM {
			let pool = pool.clone();
			let guild = guild.clone();
			let user = user.clone();
			handles.push(tokio::spawn(async move {
				queries::try_claim_promotion(&pool, &guild, &user)
					.await
					.unwrap()
			}));
		}

		let mut wins = 0;
		for h in handles {
			if h.await.unwrap() {
				wins += 1;
			}
		}
		assert_eq!(
			wins, 1,
			"iter {i}: expected exactly one winner from {PARALLELISM} concurrent claims, got {wins}"
		);
	}
}

#[sqlx::test(migrations = "./migrations")]
async fn try_claim_promotion_returns_false_when_no_row(pool: PgPool) {
	// No `increment_message_count` was ever called, so there's no row
	// to claim. The query should return `Ok(false)` rather than err.
	let claimed = queries::try_claim_promotion(&pool, "ghost-guild", "ghost-user")
		.await
		.unwrap();
	assert!(!claimed);
}

#[sqlx::test(migrations = "./migrations")]
async fn try_claim_promotion_subsequent_calls_return_false(pool: PgPool) {
	queries::increment_message_count(&pool, "g", "u")
		.await
		.unwrap();
	assert!(queries::try_claim_promotion(&pool, "g", "u").await.unwrap());
	assert!(!queries::try_claim_promotion(&pool, "g", "u").await.unwrap());
	assert!(!queries::try_claim_promotion(&pool, "g", "u").await.unwrap());
}

#[sqlx::test(migrations = "./migrations")]
async fn increment_message_count_accumulates(pool: PgPool) {
	let a = queries::increment_message_count(&pool, "g", "u")
		.await
		.unwrap();
	assert_eq!(a.message_count, 1);
	let b = queries::increment_message_count(&pool, "g", "u")
		.await
		.unwrap();
	assert_eq!(b.message_count, 2);
	let c = queries::increment_message_count(&pool, "g", "u")
		.await
		.unwrap();
	assert_eq!(c.message_count, 3);
	assert!(!c.promoted);
}
