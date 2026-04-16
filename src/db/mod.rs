pub mod models;
pub mod queries;

use sqlx::postgres::PgPoolOptions;
use sqlx::{Connection, Executor, PgPool};

pub async fn init_pool(database_url: &str, schema: &str) -> Result<PgPool, sqlx::Error> {
	// Create schema on a one-off connection
	let mut conn = sqlx::postgres::PgConnection::connect(database_url).await?;
	conn.execute(format!("CREATE SCHEMA IF NOT EXISTS \"{}\"", schema).as_str())
		.await?;
	drop(conn);

	// Build pool with after_connect that sets search_path on every new connection.
	// The migration runner below then runs inside the instance's schema, and the
	// `_sqlx_migrations` tracking table is created there too — one per instance.
	let schema_owned = schema.to_string();
	let pool = PgPoolOptions::new()
		.after_connect(move |conn, _meta| {
			let schema = schema_owned.clone();
			Box::pin(async move {
				conn.execute(format!("SET search_path TO \"{}\"", schema).as_str())
					.await?;
				Ok(())
			})
		})
		.connect(database_url)
		.await?;

	sqlx::migrate!("./migrations")
		.run(&pool)
		.await
		.map_err(|e| sqlx::Error::Migrate(Box::new(e)))?;

	tracing::info!("Database initialized (schema: {}).", schema);
	Ok(pool)
}
