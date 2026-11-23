//! Validated PostgreSQL connection target for `dennett-head` (`dennett_control`).
//!
//! Strictly independent of `dennett-memory-core` so Head never imports Memory Core.

use core::fmt;

use dennett_contracts::redact::{
    REDACTED_PLACEHOLDER, sanitize_pg_error, sanitize_pg_error_with_password,
};
use dennett_contracts::{DennettError, ProfileMode};
use tokio_postgres::Config as PgConfig;

/// Validated PostgreSQL target parsed once by [`tokio_postgres::Config`] to eliminate
/// parser differentials between safety validation and connection pooling.
#[derive(Clone)]
pub struct ValidatedControlPgTarget {
    raw_url: String,
    config: PgConfig,
}

impl fmt::Debug for ValidatedControlPgTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ValidatedControlPgTarget")
            .field("dbname", &self.config.get_dbname())
            .field("user", &self.config.get_user())
            .field("password", &REDACTED_PLACEHOLDER)
            .finish_non_exhaustive()
    }
}

impl ValidatedControlPgTarget {
    /// Parses and validates `raw_url` under `profile_mode`.
    ///
    /// In [`ProfileMode::TestIsolated`], the parsed `dbname` MUST end with `_test` and have a
    /// non-empty prefix before `_test`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::SafetyGuardRejected`] in [`ProfileMode::TestIsolated`] on invalid
    /// DSN or non-`*_test` database name, or [`DennettError::DatabaseUnavailable`] in production.
    pub fn parse(raw_url: &str, profile_mode: ProfileMode) -> Result<Self, DennettError> {
        let config: PgConfig = raw_url.parse().map_err(|err| {
            let safe = sanitize_pg_error(&format!("{err}"), Some(raw_url));
            if profile_mode == ProfileMode::TestIsolated {
                DennettError::SafetyGuardRejected {
                    reason: format!("invalid control PostgreSQL connection string: {safe}"),
                }
            } else {
                DennettError::DatabaseUnavailable {
                    detail: format!("invalid control PostgreSQL connection string: {safe}"),
                }
            }
        })?;

        if profile_mode == ProfileMode::TestIsolated {
            let Some(db_name) = config.get_dbname() else {
                return Err(DennettError::SafetyGuardRejected {
                    reason: String::from(
                        "test_isolated profile requires an explicit `*_test` control PostgreSQL database",
                    ),
                });
            };
            if db_name.len() <= "_test".len() || !db_name.ends_with("_test") {
                return Err(DennettError::SafetyGuardRejected {
                    reason: format!(
                        "test_isolated profile requires a `*_test` control PostgreSQL database, got `{db_name}`"
                    ),
                });
            }
        }

        Ok(Self {
            raw_url: raw_url.to_owned(),
            config,
        })
    }

    /// Resolves the control PG target from CLI override and env vars, guaranteeing that
    /// [`ProfileMode::TestIsolated`] NEVER falls back to production `DENNETT_CONTROL_PG_URL`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::SafetyGuardRejected`] or [`DennettError::DatabaseUnavailable`].
    pub fn resolve_from_sources(
        cli_override: Option<&str>,
        test_env: Option<&str>,
        prod_env: Option<&str>,
        profile_mode: ProfileMode,
    ) -> Result<Option<Self>, DennettError> {
        fn non_empty(v: Option<&str>) -> Option<&str> {
            v.map(str::trim).filter(|s| !s.is_empty())
        }
        let chosen = match profile_mode {
            ProfileMode::TestIsolated => non_empty(cli_override).or_else(|| non_empty(test_env)),
            ProfileMode::Production => non_empty(cli_override).or_else(|| non_empty(prod_env)),
        };
        chosen.map(|url| Self::parse(url, profile_mode)).transpose()
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

    /// Extracts the driver-parsed password as UTF-8 when present.
    #[must_use]
    pub fn parsed_password(&self) -> Option<&str> {
        self.config
            .get_password()
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
    }

    /// Redacts connection credentials and parsed password from a PostgreSQL error string.
    #[must_use]
    pub fn sanitize_error(&self, raw_error: &str) -> String {
        sanitize_pg_error_with_password(raw_error, Some(&self.raw_url), self.parsed_password())
    }
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<ValidatedControlPgTarget>();
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_isolated_blocks_prod_fallback_and_non_test_dbs() {
        let no_fallback = ValidatedControlPgTarget::resolve_from_sources(
            None,
            None,
            Some("postgres://u:secret@127.0.0.1:5432/dennett_control_prod"),
            ProfileMode::TestIsolated,
        )
        .expect("None when test env unset");
        assert!(no_fallback.is_none());

        for bad in [
            "postgres://u:PW_SECRET@127.0.0.1:5432/dennett_control",
            "postgres://u:PW_SECRET@127.0.0.1:5432/fake_test?dbname=prod_db",
            "host=127.0.0.1 user=u password=PW_SECRET dbname=fake_test dbname=prod_db",
            "postgres://u:PW_SECRET@127.0.0.1:5432/_test",
        ] {
            let err = ValidatedControlPgTarget::parse(bad, ProfileMode::TestIsolated);
            assert!(
                matches!(err, Err(DennettError::SafetyGuardRejected { .. })),
                "expected SafetyGuardRejected for `{bad}`, got {err:?}"
            );
        }
    }
}
