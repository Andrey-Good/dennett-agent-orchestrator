//! Re-exports the canonical secret redaction helpers from [`dennett_contracts::redact`].

pub use dennett_contracts::redact::{
    REDACTED_PLACEHOLDER, extract_connection_secrets, redact_secrets, redact_secrets_owned,
    sanitize_cli_arg, sanitize_pg_error, sanitize_pg_error_with_password,
};
