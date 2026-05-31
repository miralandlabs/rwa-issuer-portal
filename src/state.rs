use crate::{config::Config, db::Db, error::Error};

/// Shared application state. Records-only: a Postgres handle + config. No
/// Solana RPC, no keypair — the portal never touches the chain.
pub struct AppState {
    pub config: Config,
    pub db: Db,
}

impl AppState {
    /// Connect the DB and apply the idempotent schema migration.
    pub async fn cold_start(config: Config) -> Result<Self, Error> {
        let db = Db::connect(&config.database_url)?;
        db.migrate(include_str!("../migrations/init.sql")).await?;
        tracing::info!(target: "server_log", "rwa-issuer-portal migration applied");
        Ok(Self { config, db })
    }
}
