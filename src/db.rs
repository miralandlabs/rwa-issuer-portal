//! Postgres access for the issuer portal.
//!
//! Reuses the SAME Supabase database as `aethervane` / `spl-token-balance`
//! (one DB for preview, one for production), namespaced by this crate's own
//! tables (`issuers`, `issuer_kyc_records`). Every wire step is wrapped in a
//! wall-clock `timeout` for the same reason as the sibling crates: Vercel +
//! Supabase pooled connections can go half-open and hang the socket. See
//! `spl-token-balance-serverless/src/db.rs`.

use deadpool_postgres::{Client, Config as PgConfig, Pool, PoolConfig, Runtime};
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
use postgres_openssl::MakeTlsConnector;
use std::time::Duration;
use tokio::time::timeout;
use tokio_postgres::types::ToSql;
use tracing::error;

use crate::error::Error;

#[derive(Clone)]
pub struct Db {
    pool: Pool,
}

impl Db {
    const WAIT: Duration = Duration::from_secs(15);
    const CREATE: Duration = Duration::from_secs(10);
    const RECYCLE: Duration = Duration::from_secs(30);
    const POOL_GET_TIMEOUT: Duration = Duration::from_secs(20);
    const TX_BEGIN_TIMEOUT: Duration = Duration::from_secs(20);
    const SET_LOCAL_CMD_TIMEOUT: Duration = Duration::from_secs(5);
    const QUERY_TIMEOUT: Duration = Duration::from_secs(30);
    const DEALLOCATE_TIMEOUT: Duration = Duration::from_secs(5);
    const PG_STATEMENT_TIMEOUT: &'static str = "20s";

    pub fn connect(database_url: &str) -> Result<Self, Error> {
        let mut cfg = PgConfig::new();
        cfg.url = Some(database_url.to_string());
        cfg.pool = Some(PoolConfig {
            max_size: 5,
            timeouts: deadpool_postgres::Timeouts {
                wait: Some(Self::WAIT),
                create: Some(Self::CREATE),
                recycle: Some(Self::RECYCLE),
            },
            ..Default::default()
        });
        let mut builder =
            SslConnector::builder(SslMethod::tls()).map_err(|e| Error::Internal(e.to_string()))?;
        builder.set_verify(SslVerifyMode::NONE);
        let tls = MakeTlsConnector::new(builder.build());
        let pool = cfg
            .create_pool(Some(Runtime::Tokio1), tls)
            .map_err(|e| Error::Internal(format!("db pool: {e}")))?;
        Ok(Self { pool })
    }

    async fn conn(&self) -> Result<Client, Error> {
        timeout(Self::POOL_GET_TIMEOUT, self.pool.get())
            .await
            .map_err(|_| Error::Internal("db pool get timed out".into()))?
            .map_err(|e| Error::Internal(format!("db pool: {e}")))
    }

    /// Apply the idempotent schema migration. Safe to run on every cold start.
    pub async fn migrate(&self, sql: &str) -> Result<(), Error> {
        let client = self.conn().await?;
        let wrapped = format!(
            "BEGIN; SET LOCAL statement_timeout = '{}'; {} COMMIT;",
            Self::PG_STATEMENT_TIMEOUT,
            sql
        );
        timeout(Self::QUERY_TIMEOUT, client.batch_execute(&wrapped))
            .await
            .map_err(|_| Error::Internal("migration timed out".into()))?
            .map_err(|e| Error::Internal(format!("migration failed: {e}")))?;
        Ok(())
    }

    pub(crate) async fn query_opt(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
        label: &str,
    ) -> Result<Option<tokio_postgres::Row>, Error> {
        let client = self.conn().await?;
        self.query_opt_in_tx(client, sql, params, label).await
    }

    pub(crate) async fn query(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
        label: &str,
    ) -> Result<Vec<tokio_postgres::Row>, Error> {
        let client = self.conn().await?;
        self.query_in_tx(client, sql, params, label).await
    }

    async fn begin_transaction<'a>(
        client: &'a mut Client,
        label: &str,
    ) -> Result<deadpool_postgres::Transaction<'a>, Error> {
        timeout(Self::TX_BEGIN_TIMEOUT, client.transaction())
            .await
            .map_err(|_| {
                Error::Internal(format!(
                    "{label} transaction start timed out (pool connection may be stale)"
                ))
            })?
            .map_err(|e| Error::Internal(format!("{label} transaction start failed: {e}")))
    }

    async fn set_statement_timeout_local(tx: &deadpool_postgres::Transaction<'_>) {
        let sql = format!(
            "SET LOCAL statement_timeout = '{}'",
            Self::PG_STATEMENT_TIMEOUT
        );
        match timeout(Self::SET_LOCAL_CMD_TIMEOUT, tx.execute(sql.as_str(), &[])).await {
            Ok(Ok(_)) => (),
            Ok(Err(e)) => error!(error = %e, "SET LOCAL statement_timeout failed"),
            Err(_) => error!(
                "SET LOCAL statement_timeout timed out after {:?}",
                Self::SET_LOCAL_CMD_TIMEOUT
            ),
        }
    }

    async fn deallocate_prepared(tx: &deadpool_postgres::Transaction<'_>) {
        let _ = timeout(Self::DEALLOCATE_TIMEOUT, tx.execute("DEALLOCATE ALL", &[])).await;
    }

    async fn commit_tx(tx: deadpool_postgres::Transaction<'_>, label: &str) -> Result<(), Error> {
        timeout(Self::QUERY_TIMEOUT, tx.commit())
            .await
            .map_err(|_| Error::Internal(format!("{label} commit timed out")))?
            .map_err(|e| Error::Internal(format!("{label} commit failed: {e}")))
    }

    async fn query_opt_in_tx(
        &self,
        mut client: Client,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
        label: &str,
    ) -> Result<Option<tokio_postgres::Row>, Error> {
        let tx = Self::begin_transaction(&mut client, label).await?;
        Self::set_statement_timeout_local(&tx).await;
        Self::deallocate_prepared(&tx).await;

        let row = timeout(Self::QUERY_TIMEOUT, tx.query_opt(sql, params))
            .await
            .map_err(|_| Error::Internal(format!("{label} timed out")))?
            .map_err(|e| Error::Internal(format!("{label} failed: {e}")))?;

        Self::commit_tx(tx, label).await?;
        Ok(row)
    }

    async fn query_in_tx(
        &self,
        mut client: Client,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
        label: &str,
    ) -> Result<Vec<tokio_postgres::Row>, Error> {
        let tx = Self::begin_transaction(&mut client, label).await?;
        Self::set_statement_timeout_local(&tx).await;
        Self::deallocate_prepared(&tx).await;

        let rows = timeout(Self::QUERY_TIMEOUT, tx.query(sql, params))
            .await
            .map_err(|_| Error::Internal(format!("{label} timed out")))?
            .map_err(|e| Error::Internal(format!("{label} failed: {e}")))?;

        Self::commit_tx(tx, label).await?;
        Ok(rows)
    }
}
