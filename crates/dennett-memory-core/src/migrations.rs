//! Versioned, idempotent PostgreSQL schema migrator for `dennett-memoryd`.

use std::collections::BTreeMap;

use dennett_contracts::DennettError;
use dennett_contracts::redact::sanitize_pg_error_with_password;
use tokio_postgres::Client;

use crate::fingerprint::sha256_hex;

/// Stable PostgreSQL advisory lock key serializing concurrent `dennett-memoryd` schema migrations.
const MIGRATION_ADVISORY_LOCK_KEY: i64 = 728_391_045_123_456_789;

/// Latest Memory Fabric schema migration version bundled in this build.
pub const LATEST_SCHEMA_VERSION: i64 = 1;

/// One versioned SQL migration bundled into `dennett-memory-core`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Migration {
    /// Positive monotonically increasing migration version.
    pub version: i64,
    /// Stable human-readable migration name.
    pub name: &'static str,
    /// Forward-only PostgreSQL DDL script.
    pub sql: &'static str,
}

/// Ordered list of all Memory Fabric schema migrations owned by `dennett-memoryd`.
pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "0001_memory_core_v1",
    sql: include_str!("../migrations/0001_memory_core_v1.sql"),
}];

/// Runs all pending Memory Fabric migrations atomically and idempotently.
///
/// Uses a transaction-scoped PostgreSQL advisory lock (`pg_advisory_xact_lock`) together with
/// `lock_timeout = '5s'` and `statement_timeout = '30s'`, verifies stored SHA-256 checksums for
/// previously applied versions, and fails closed if the database is on a newer schema version
/// than this binary understands.
///
/// # Errors
///
/// - [`DennettError::SafetyGuardRejected`] if an applied migration checksum mismatches or the
///   database schema version exceeds [`LATEST_SCHEMA_VERSION`].
/// - [`DennettError::DatabaseUnavailable`] if PostgreSQL rejects the transaction or DDL.
pub async fn run_migrations(
    client: &mut Client,
    pg_url: Option<&str>,
    parsed_password: Option<&str>,
) -> Result<i64, DennettError> {
    let map_pg = |err: tokio_postgres::Error| DennettError::DatabaseUnavailable {
        detail: sanitize_pg_error_with_password(&err.to_string(), pg_url, parsed_password),
    };

    let tx = client.transaction().await.map_err(map_pg)?;
    tx.batch_execute("SET LOCAL lock_timeout = '5s'; SET LOCAL statement_timeout = '30s';")
        .await
        .map_err(map_pg)?;

    tx.execute(
        "SELECT pg_advisory_xact_lock($1)",
        &[&MIGRATION_ADVISORY_LOCK_KEY],
    )
    .await
    .map_err(map_pg)?;

    tx.batch_execute(
        "CREATE TABLE IF NOT EXISTS memory_schema_migrations (
            version BIGINT PRIMARY KEY CHECK (version > 0),
            name TEXT NOT NULL CHECK (octet_length(name) > 0),
            checksum TEXT NOT NULL CHECK (checksum ~ '^[0-9a-f]{64}$'),
            applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
        );",
    )
    .await
    .map_err(map_pg)?;

    let existing_rows = tx
        .query(
            "SELECT version, checksum FROM memory_schema_migrations ORDER BY version ASC",
            &[],
        )
        .await
        .map_err(map_pg)?;

    let mut applied: BTreeMap<i64, String> = BTreeMap::new();
    for row in existing_rows {
        let version: i64 = row.get(0);
        let checksum: String = row.get(1);
        if version > LATEST_SCHEMA_VERSION {
            return Err(DennettError::SafetyGuardRejected {
                reason: format!(
                    "database memory schema version {version} is newer than supported version {LATEST_SCHEMA_VERSION}"
                ),
            });
        }
        applied.insert(version, checksum);
    }

    for migration in MIGRATIONS {
        let expected_checksum = sha256_hex(migration.sql.as_bytes());
        if let Some(stored_checksum) = applied.get(&migration.version) {
            if stored_checksum != expected_checksum.as_str() {
                return Err(DennettError::SafetyGuardRejected {
                    reason: format!(
                        "migration {} ({}) checksum mismatch: database does not match binary DDL",
                        migration.version, migration.name
                    ),
                });
            }
            continue;
        }

        tx.batch_execute(migration.sql).await.map_err(map_pg)?;
        tx.execute(
            "INSERT INTO memory_schema_migrations (version, name, checksum) VALUES ($1, $2, $3)",
            &[
                &migration.version,
                &migration.name,
                &expected_checksum.as_str(),
            ],
        )
        .await
        .map_err(map_pg)?;
    }

    tx.commit().await.map_err(map_pg)?;
    Ok(LATEST_SCHEMA_VERSION)
}

/// Performs a strictly read-only lookup of the highest applied Memory Fabric schema version.
///
/// Safe to execute against read-only PostgreSQL standbys and health probes (never writes).
///
/// # Errors
///
/// Returns [`DennettError::DatabaseUnavailable`] with credentials redacted if the query fails.
pub async fn query_schema_version(
    client: &Client,
    pg_url: Option<&str>,
    parsed_password: Option<&str>,
) -> Result<Option<i64>, DennettError> {
    let map_pg = |err: tokio_postgres::Error| DennettError::DatabaseUnavailable {
        detail: sanitize_pg_error_with_password(&err.to_string(), pg_url, parsed_password),
    };

    let regclass: Option<String> = client
        .query_one("SELECT to_regclass('memory_schema_migrations')::text", &[])
        .await
        .map_err(map_pg)?
        .get(0);

    if regclass.is_none() {
        return Ok(None);
    }

    let version: Option<i64> = client
        .query_one(
            "SELECT MAX(version)::bigint FROM memory_schema_migrations",
            &[],
        )
        .await
        .map_err(map_pg)?
        .get(0);

    Ok(version)
}
