//! Versioned, idempotent PostgreSQL schema migrator for `dennett_control`.

use std::collections::BTreeMap;

use dennett_contracts::redact::sanitize_pg_error_with_password;
use dennett_contracts::{DennettError, Sha256Hex};
use sha2::{Digest, Sha256};
use tokio_postgres::Client;

/// Stable PostgreSQL advisory lock key serializing concurrent `dennett-head` schema migrations.
const CONTROL_MIGRATION_ADVISORY_LOCK_KEY: i64 = 842_190_537_612_049_311;

/// Latest Control schema migration version bundled in this build.
pub const LATEST_CONTROL_SCHEMA_VERSION: i64 = 4;

/// One forward-only PostgreSQL migration owned by `dennett-head`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlMigration {
    /// Positive monotonically increasing migration version.
    pub version: i64,
    /// Stable human-readable migration name.
    pub name: &'static str,
    /// Forward-only PostgreSQL DDL script.
    pub sql: &'static str,
}

/// Ordered list of all Control schema migrations owned by `dennett-head`.
pub const CONTROL_MIGRATIONS: &[ControlMigration] = &[
    ControlMigration {
        version: 1,
        name: "0001_control_core_v1",
        sql: include_str!("../migrations/0001_control_core_v1.sql"),
    },
    ControlMigration {
        version: 2,
        name: "0002_projects_nullable_workspace",
        sql: include_str!("../migrations/0002_projects_nullable_workspace.sql"),
    },
    ControlMigration {
        version: 3,
        name: "0003_artifacts_unique_revision",
        sql: include_str!("../migrations/0003_artifacts_unique_revision.sql"),
    },
    ControlMigration {
        version: 4,
        name: "0004_d2_durable_tasks_and_lifecycle",
        sql: include_str!("../migrations/0004_d2_durable_tasks_and_lifecycle.sql"),
    },
];

/// Computes lowercase SHA-256 hex digest of `bytes`.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> Sha256Hex {
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    Sha256Hex::from_digest_bytes(digest)
}

/// Runs all pending `dennett_control` migrations atomically and idempotently.
///
/// Uses `pg_advisory_xact_lock`, `lock_timeout = '5s'`, `statement_timeout = '30s'`, verifies
/// SHA-256 checksums of previously applied versions, and fails closed if the database is on
/// a newer schema version than this binary supports.
///
/// # Errors
///
/// Returns [`DennettError::SafetyGuardRejected`] on checksum mismatch or newer DB schema,
/// or [`DennettError::DatabaseUnavailable`] on PostgreSQL execution errors.
pub async fn run_control_migrations(
    client: &mut Client,
    pg_url: Option<&str>,
    parsed_password: Option<&str>,
) -> Result<i64, DennettError> {
    let map_pg = |err: tokio_postgres::Error| DennettError::DatabaseUnavailable {
        detail: sanitize_pg_error_with_password(
            &crate::store::format_pg_error_detail(&err),
            pg_url,
            parsed_password,
        ),
    };

    let tx = client.transaction().await.map_err(map_pg)?;
    tx.batch_execute("SET LOCAL lock_timeout = '5s'; SET LOCAL statement_timeout = '30s';")
        .await
        .map_err(map_pg)?;

    tx.execute(
        "SELECT pg_advisory_xact_lock($1)",
        &[&CONTROL_MIGRATION_ADVISORY_LOCK_KEY],
    )
    .await
    .map_err(map_pg)?;

    tx.batch_execute(
        "CREATE TABLE IF NOT EXISTS control_schema_migrations (
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
            "SELECT version, checksum FROM control_schema_migrations ORDER BY version ASC",
            &[],
        )
        .await
        .map_err(map_pg)?;

    let mut applied: BTreeMap<i64, String> = BTreeMap::new();
    for row in existing_rows {
        let version: i64 = row.get(0);
        let checksum: String = row.get(1);
        if version > LATEST_CONTROL_SCHEMA_VERSION {
            return Err(DennettError::SafetyGuardRejected {
                reason: format!(
                    "database control schema version {version} is newer than supported version {LATEST_CONTROL_SCHEMA_VERSION}"
                ),
            });
        }
        applied.insert(version, checksum);
    }

    for migration in CONTROL_MIGRATIONS {
        let expected_checksum = sha256_hex(migration.sql.as_bytes());
        if let Some(stored_checksum) = applied.get(&migration.version) {
            if stored_checksum != expected_checksum.as_str() {
                return Err(DennettError::SafetyGuardRejected {
                    reason: format!(
                        "control migration {} ({}) checksum mismatch: database does not match binary DDL",
                        migration.version, migration.name
                    ),
                });
            }
            continue;
        }

        tx.batch_execute(migration.sql).await.map_err(map_pg)?;
        tx.execute(
            "INSERT INTO control_schema_migrations (version, name, checksum) VALUES ($1, $2, $3)",
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
    Ok(LATEST_CONTROL_SCHEMA_VERSION)
}

/// Performs a strictly read-only lookup of the highest applied Control schema version.
///
/// # Errors
///
/// Returns [`DennettError::DatabaseUnavailable`] with credentials redacted on query failure.
pub async fn query_control_schema_version(
    client: &Client,
    pg_url: Option<&str>,
    parsed_password: Option<&str>,
) -> Result<Option<i64>, DennettError> {
    let map_pg = |err: tokio_postgres::Error| DennettError::DatabaseUnavailable {
        detail: sanitize_pg_error_with_password(
            &crate::store::format_pg_error_detail(&err),
            pg_url,
            parsed_password,
        ),
    };

    let regclass: Option<String> = client
        .query_one("SELECT to_regclass('control_schema_migrations')::text", &[])
        .await
        .map_err(map_pg)?
        .get(0);

    if regclass.is_none() {
        return Ok(None);
    }

    let version: Option<i64> = client
        .query_one(
            "SELECT MAX(version)::bigint FROM control_schema_migrations",
            &[],
        )
        .await
        .map_err(map_pg)?
        .get(0);

    Ok(version)
}
