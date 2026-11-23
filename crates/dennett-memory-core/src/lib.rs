//! Canonical PostgreSQL Memory Fabric engine and standalone [`AuthVerifier`] for `dennett-memoryd`.
//!
//! Architectural boundary (Architecture 80 §3.3, §6): this crate is ONLY linked by
//! `services/memoryd` and isolated tests. `dennett-head`, `dennett-memory-client`,
//! `dennett-memory-mcp`, and UI layers MUST NEVER depend on `dennett-memory-core`.

pub mod auth;
pub mod fingerprint;
pub mod migrations;
pub mod pg_target;
pub mod service;

pub use auth::{
    AuthConfig, AuthVerifier, DEFAULT_AUDIENCE, DEFAULT_ISSUER, MAX_BEARER_TOKEN_BYTES,
    MIN_HMAC_SECRET_BYTES, SIGNED_TOKEN_PREFIX, SignedTokenClaims, StandaloneAuthVerifier,
    StaticTokenEntryConfig, parse_bearer_header,
};
pub use fingerprint::{
    fingerprint_correct_note, fingerprint_create_note, fingerprint_ingest_evidence, sha256_bytes,
    sha256_hex,
};
pub use migrations::{
    LATEST_SCHEMA_VERSION, MIGRATIONS, Migration, query_schema_version, run_migrations,
};
pub use pg_target::ValidatedPgTarget;
pub use service::{
    DEFAULT_PG_OP_TIMEOUT, DEFAULT_PG_PROBE_TIMEOUT, DEFAULT_POOL_MAX_SIZE, MEMORYD_STAGE_LABEL,
    MemoryService, d1_memoryd_capabilities,
};
