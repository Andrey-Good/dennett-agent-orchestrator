//! Asynchronous PostgreSQL verification harness for isolated D0 tests.

use std::time::Duration;

use dennett_contracts::DennettError;
use tokio::task::JoinHandle;
use tokio::time;
use tokio_postgres::NoTls;
use tokio_postgres::error::SqlState;
use uuid::Uuid;

use crate::profile::FaultInjectionGuard;
use crate::redact::{extract_connection_secrets, redact_secrets_owned};

/// Default timeout for each PostgreSQL probe operation.
const PG_OP_TIMEOUT: Duration = Duration::from_secs(5);
/// Timeout dedicated to dropping disposable probe schemas even if the main probe failed.
const PG_CLEANUP_TIMEOUT: Duration = Duration::from_secs(3);

/// RAII guard that aborts a spawned connection task on early return, timeout, or panic.
struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Active PostgreSQL identity confirmed via `SELECT current_user, current_database()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PgConnectionInfo {
    /// Active PostgreSQL role (`current_user`).
    pub role: String,
    /// Active PostgreSQL database (`current_database()`).
    pub database: String,
}

/// Connects to PostgreSQL and verifies the active role and database name.
///
/// # Errors
///
/// Returns [`DennettError::DatabaseUnavailable`] with all credentials redacted if
/// connecting or querying fails or times out.
pub async fn probe_connection(pg_url: &str) -> Result<PgConnectionInfo, DennettError> {
    let secrets = extract_connection_secrets(pg_url);
    let fut = async {
        let (client, connection) = tokio_postgres::connect(pg_url, NoTls)
            .await
            .map_err(|err| DennettError::DatabaseUnavailable {
                detail: redact_secrets_owned(&err.to_string(), &secrets),
            })?;

        let _conn_guard = AbortOnDrop(tokio::spawn(async move {
            let _ = connection.await;
        }));

        let row = client
            .query_one("SELECT current_user::text, current_database()::text", &[])
            .await
            .map_err(|err| DennettError::DatabaseUnavailable {
                detail: redact_secrets_owned(&err.to_string(), &secrets),
            })?;

        let role: String = row.get(0);
        let database: String = row.get(1);
        drop(client);

        Ok(PgConnectionInfo { role, database })
    };

    time::timeout(PG_OP_TIMEOUT, fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("pg_harness::probe_connection"),
        })?
}

/// Verifies transactional rollback and commit inside a disposable `d0_probe_<hex>` schema,
/// cleaning up the schema afterward even when intermediate statements fail or time out.
///
/// Requires an acquired [`FaultInjectionGuard`] so write probes cannot run against
/// unisolated or non-`_test` databases.
///
/// # Errors
///
/// Returns [`DennettError::SafetyGuardRejected`] if the connected database does not match
/// the isolated guard's `_test` databases, or [`DennettError::DatabaseUnavailable`] /
/// [`DennettError::Internal`] on SQL or assertion failures.
pub async fn verify_transactional_roundtrip(
    pg_url: &str,
    guard: &FaultInjectionGuard,
) -> Result<(), DennettError> {
    let secrets = extract_connection_secrets(pg_url);

    let (mut client, connection) =
        time::timeout(PG_OP_TIMEOUT, tokio_postgres::connect(pg_url, NoTls))
            .await
            .map_err(|_| DennettError::Timeout {
                operation: String::from("pg_harness::verify_transactional_roundtrip.connect"),
            })?
            .map_err(|err| DennettError::DatabaseUnavailable {
                detail: redact_secrets_owned(&err.to_string(), &secrets),
            })?;

    let _conn_guard = AbortOnDrop(tokio::spawn(async move {
        let _ = connection.await;
    }));

    let active_db: String = time::timeout(
        PG_OP_TIMEOUT,
        client.query_one("SELECT current_database()::text", &[]),
    )
    .await
    .map_err(|_| DennettError::Timeout {
        operation: String::from("pg_harness::verify_transactional_roundtrip.current_database"),
    })?
    .map_err(|err| DennettError::DatabaseUnavailable {
        detail: redact_secrets_owned(&err.to_string(), &secrets),
    })?
    .get(0);

    if !active_db.ends_with("_test")
        || (active_db != guard.memory_db() && active_db != guard.control_db())
    {
        return Err(DennettError::SafetyGuardRejected {
            reason: format!("refusing write probe on database `{active_db}` outside guard scope"),
        });
    }

    let suffix = &Uuid::new_v4().simple().to_string()[..12];
    let schema = format!("d0_probe_{suffix}");
    let table = format!("{schema}.items");

    let outcome = match time::timeout(
        PG_OP_TIMEOUT,
        run_roundtrip_sql(&mut client, &schema, &table, &secrets),
    )
    .await
    {
        Ok(res) => res,
        Err(_) => Err(DennettError::Timeout {
            operation: String::from("pg_harness::verify_transactional_roundtrip.sql"),
        }),
    };

    // Always attempt cleanup of the disposable schema even if `run_roundtrip_sql` failed or timed out.
    let drop_sql = format!("DROP SCHEMA IF EXISTS {schema} CASCADE");
    let cleanup_res = time::timeout(PG_CLEANUP_TIMEOUT, client.batch_execute(&drop_sql)).await;
    drop(client);

    outcome?;
    match cleanup_res {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => Err(DennettError::DatabaseUnavailable {
            detail: redact_secrets_owned(&err.to_string(), &secrets),
        }),
        Err(_) => Err(DennettError::Timeout {
            operation: String::from("pg_harness::verify_transactional_roundtrip.cleanup"),
        }),
    }
}

async fn run_roundtrip_sql(
    client: &mut tokio_postgres::Client,
    schema: &str,
    table: &str,
    secrets: &[String],
) -> Result<(), DennettError> {
    let setup_sql = format!(
        "CREATE SCHEMA {schema}; \
         CREATE TABLE {table} (id INT PRIMARY KEY, note TEXT NOT NULL);"
    );
    client
        .batch_execute(&setup_sql)
        .await
        .map_err(|err| DennettError::DatabaseUnavailable {
            detail: redact_secrets_owned(&err.to_string(), secrets),
        })?;

    // 1. Verify ROLLBACK leaves zero rows.
    let insert_sql = format!("INSERT INTO {table} (id, note) VALUES ($1, $2)");
    {
        let tx = client
            .transaction()
            .await
            .map_err(|err| DennettError::DatabaseUnavailable {
                detail: redact_secrets_owned(&err.to_string(), secrets),
            })?;
        tx.execute(&insert_sql, &[&1_i32, &"rolled-back"])
            .await
            .map_err(|err| DennettError::DatabaseUnavailable {
                detail: redact_secrets_owned(&err.to_string(), secrets),
            })?;
        tx.rollback()
            .await
            .map_err(|err| DennettError::DatabaseUnavailable {
                detail: redact_secrets_owned(&err.to_string(), secrets),
            })?;
    }

    let count_sql = format!("SELECT COUNT(*)::int8 FROM {table} WHERE id = $1");
    let rolled_back_count: i64 = client
        .query_one(&count_sql, &[&1_i32])
        .await
        .map_err(|err| DennettError::DatabaseUnavailable {
            detail: redact_secrets_owned(&err.to_string(), secrets),
        })?
        .get(0);
    if rolled_back_count != 0 {
        return Err(DennettError::Internal {
            detail: format!("rollback probe expected 0 rows, observed {rolled_back_count}"),
        });
    }

    // 2. Verify COMMIT persists the row.
    {
        let tx = client
            .transaction()
            .await
            .map_err(|err| DennettError::DatabaseUnavailable {
                detail: redact_secrets_owned(&err.to_string(), secrets),
            })?;
        tx.execute(&insert_sql, &[&2_i32, &"committed"])
            .await
            .map_err(|err| DennettError::DatabaseUnavailable {
                detail: redact_secrets_owned(&err.to_string(), secrets),
            })?;
        tx.commit()
            .await
            .map_err(|err| DennettError::DatabaseUnavailable {
                detail: redact_secrets_owned(&err.to_string(), secrets),
            })?;
    }

    let select_sql = format!("SELECT note FROM {table} WHERE id = $1");
    let committed_note: String = client
        .query_one(&select_sql, &[&2_i32])
        .await
        .map_err(|err| DennettError::DatabaseUnavailable {
            detail: redact_secrets_owned(&err.to_string(), secrets),
        })?
        .get(0);
    if committed_note != "committed" {
        return Err(DennettError::Internal {
            detail: format!("commit probe expected `committed`, observed `{committed_note}`"),
        });
    }

    Ok(())
}

/// Verifies that PostgreSQL explicitly rejects a cross-domain connection with
/// SQLSTATE `42501` (`insufficient_privilege`), rather than merely being unreachable.
///
/// # Errors
///
/// - [`DennettError::SafetyGuardRejected`] if the cross-domain connection unexpectedly succeeded.
/// - [`DennettError::DatabaseUnavailable`] if PostgreSQL failed for any reason other than
///   `INSUFFICIENT_PRIVILEGE` (e.g. server down or wrong credentials).
/// - [`DennettError::Timeout`] if the connection attempt timed out.
pub async fn verify_cross_domain_connect_denied(pg_url: &str) -> Result<(), DennettError> {
    let secrets = extract_connection_secrets(pg_url);
    let fut = async {
        match tokio_postgres::connect(pg_url, NoTls).await {
            Ok((client, connection)) => {
                let _conn_guard = AbortOnDrop(tokio::spawn(async move {
                    let _ = connection.await;
                }));
                drop(client);
                Err(DennettError::SafetyGuardRejected {
                    reason: String::from(
                        "cross-domain PostgreSQL connection unexpectedly succeeded",
                    ),
                })
            }
            Err(err) => {
                if err.code() == Some(&SqlState::INSUFFICIENT_PRIVILEGE) {
                    Ok(())
                } else {
                    Err(DennettError::DatabaseUnavailable {
                        detail: format!(
                            "expected SQLSTATE 42501 (insufficient_privilege), got: {}",
                            redact_secrets_owned(&err.to_string(), &secrets)
                        ),
                    })
                }
            }
        }
    };

    time::timeout(PG_OP_TIMEOUT, fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("pg_harness::verify_cross_domain_connect_denied"),
        })?
}

/// Replaces the database path segment in a `postgres://` or `postgresql://` URI while
/// preserving credentials, host, port, and query parameters.
///
/// # Errors
///
/// Returns [`DennettError::InvalidInput`] if `new_database` is not a safe identifier or
/// if `pg_url` is not a valid PostgreSQL URI.
pub fn replace_url_database(pg_url: &str, new_database: &str) -> Result<String, DennettError> {
    if new_database.is_empty()
        || !new_database
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(DennettError::InvalidInput {
            field: String::from("new_database"),
            reason: String::from("must be non-empty ASCII alphanumeric or '_'"),
        });
    }

    let Some(scheme_pos) = pg_url.find("://") else {
        return Err(DennettError::InvalidInput {
            field: String::from("pg_url"),
            reason: String::from("expected URI with scheme (postgres:// or postgresql://)"),
        });
    };
    let scheme = &pg_url[..scheme_pos];
    if scheme != "postgres" && scheme != "postgresql" {
        return Err(DennettError::InvalidInput {
            field: String::from("pg_url"),
            reason: format!("unsupported URI scheme `{scheme}`"),
        });
    }

    let after_scheme = &pg_url[scheme_pos + 3..];
    let Some(slash_rel) = after_scheme.find('/') else {
        return Err(DennettError::InvalidInput {
            field: String::from("pg_url"),
            reason: String::from("URI is missing database path segment"),
        });
    };

    let prefix = &pg_url[..=(scheme_pos + 3 + slash_rel)];
    let tail = &after_scheme[slash_rel + 1..];
    let query_suffix = tail.find(['?', '#']).map_or("", |pos| &tail[pos..]);

    Ok(format!("{prefix}{new_database}{query_suffix}"))
}
