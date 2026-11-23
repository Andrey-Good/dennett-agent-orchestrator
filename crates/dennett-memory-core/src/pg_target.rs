//! Validated PostgreSQL connection target shared by `dennett-memory-core` and `dennett-memoryd`.

use core::fmt;

use dennett_contracts::redact::{
    REDACTED_PLACEHOLDER, sanitize_pg_error, sanitize_pg_error_with_password,
};
use dennett_contracts::{DennettError, ProfileMode};
use tokio_postgres::Config as PgConfig;

/// Validated PostgreSQL configuration parsed once by [`tokio_postgres::Config`] to eliminate
/// parser differentials between safety validation and connection establishment.
#[derive(Clone)]
pub struct ValidatedPgTarget {
    raw_url: String,
    config: PgConfig,
}

impl fmt::Debug for ValidatedPgTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ValidatedPgTarget")
            .field("dbname", &self.config.get_dbname())
            .field("user", &self.config.get_user())
            .field("password", &REDACTED_PLACEHOLDER)
            .finish_non_exhaustive()
    }
}

impl ValidatedPgTarget {
    /// Parses and validates `raw_url` under `profile_mode`.
    ///
    /// In [`ProfileMode::TestIsolated`], the parsed `dbname` must explicitly end with `_test`
    /// and have a non-empty prefix before `_test`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::SafetyGuardRejected`] when an isolated test profile receives an
    /// invalid DSN or a non-`*_test` database name, or [`DennettError::DatabaseUnavailable`]
    /// when a production profile receives a malformed connection string.
    pub fn parse(raw_url: &str, profile_mode: ProfileMode) -> Result<Self, DennettError> {
        let config: PgConfig = raw_url.parse().map_err(|err| {
            let safe = sanitize_pg_error(&format!("{err}"), Some(raw_url));
            if profile_mode == ProfileMode::TestIsolated {
                DennettError::SafetyGuardRejected {
                    reason: format!("invalid PostgreSQL connection string: {safe}"),
                }
            } else {
                DennettError::DatabaseUnavailable {
                    detail: format!("invalid PostgreSQL connection string: {safe}"),
                }
            }
        })?;

        if profile_mode == ProfileMode::TestIsolated {
            let Some(db_name) = config.get_dbname() else {
                return Err(DennettError::SafetyGuardRejected {
                    reason: String::from(
                        "test_isolated profile requires an explicit `*_test` PostgreSQL database",
                    ),
                });
            };
            if db_name.len() <= "_test".len() || !db_name.ends_with("_test") {
                return Err(DennettError::SafetyGuardRejected {
                    reason: format!(
                        "test_isolated profile requires a `*_test` PostgreSQL database, got `{db_name}`"
                    ),
                });
            }
        }

        Ok(Self {
            raw_url: raw_url.to_owned(),
            config,
        })
    }

    /// Returns the parsed [`tokio_postgres::Config`].
    #[must_use]
    pub const fn config(&self) -> &PgConfig {
        &self.config
    }

    /// Returns the raw connection string for secret-extraction during error redaction.
    #[must_use]
    pub fn raw_url(&self) -> &str {
        &self.raw_url
    }

    /// Extracts the driver-parsed password bytes as UTF-8 when present.
    #[must_use]
    pub fn parsed_password(&self) -> Option<&str> {
        self.config
            .get_password()
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
    }

    /// Redacts connection credentials and parsed password from a PostgreSQL error message.
    #[must_use]
    pub fn sanitize_error(&self, raw_error: &str) -> String {
        sanitize_pg_error_with_password(raw_error, Some(&self.raw_url), self.parsed_password())
    }
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<ValidatedPgTarget>();
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validated_pg_target_blocks_non_test_and_override_exploits_in_test_isolated() {
        for bad in [
            "postgres://u:TOP_SECRET_PW@127.0.0.1:5432/prod_db",
            "postgres://u:TOP_SECRET_PW@127.0.0.1:5432/fake_test?dbname=prod_db",
            "host=127.0.0.1 user=u password=TOP_SECRET_PW dbname=fake_test dbname=prod_db",
            "postgres://u:TOP_SECRET_PW@127.0.0.1:5432/_test",
        ] {
            let res = ValidatedPgTarget::parse(bad, ProfileMode::TestIsolated);
            assert!(
                matches!(res, Err(DennettError::SafetyGuardRejected { .. })),
                "expected SafetyGuardRejected for `{bad}`, got {res:?}"
            );
        }

        let valid = ValidatedPgTarget::parse(
            "postgres://u:TOP_SECRET_PW@127.0.0.1:55432/dennett_memory_test",
            ProfileMode::TestIsolated,
        );
        let Ok(target) = valid else {
            panic!("valid test DSN must parse");
        };
        let dbg = format!("{target:?}");
        assert!(
            !dbg.contains("TOP_SECRET_PW"),
            "Debug leaked password: {dbg}"
        );
        let sanitized = target.sanitize_error("auth failed with TOP_SECRET_PW");
        assert!(!sanitized.contains("TOP_SECRET_PW"));
    }
}
