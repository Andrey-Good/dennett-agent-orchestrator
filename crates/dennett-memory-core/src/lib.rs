//! Canonical PostgreSQL Memory Fabric engine and standalone [`AuthVerifier`] for `dennett-memoryd`.
//!
//! Architectural boundary (Architecture 80 §3.3, §6): this crate is ONLY linked by
//! `services/memoryd` and isolated tests. `dennett-head`, `dennett-memory-client`,
//! `dennett-memory-mcp`, and UI layers MUST NEVER depend on `dennett-memory-core`.

pub mod auth;
pub mod claims;
pub mod consolidation;
pub mod documents;
pub mod fingerprint;
pub mod forget;
pub mod migrations;
pub mod pg_target;
pub mod portable;
pub mod search;
pub mod service;

pub use auth::{
    AuthConfig, AuthVerifier, DEFAULT_AUDIENCE, DEFAULT_ISSUER, MAX_BEARER_TOKEN_BYTES,
    MIN_HMAC_SECRET_BYTES, SIGNED_TOKEN_PREFIX, SignedTokenClaims, StandaloneAuthVerifier,
    StaticTokenEntryConfig, parse_bearer_header,
};
pub use consolidation::ConsolidationCheckpoint;
pub use fingerprint::{
    fingerprint_correct_note, fingerprint_create_claim, fingerprint_create_document,
    fingerprint_create_note, fingerprint_forget, fingerprint_ingest_evidence,
    fingerprint_resolve_conflict, fingerprint_supersede_claim, fingerprint_update_document,
    sha256_bytes, sha256_hex,
};
pub use forget::RESIDUAL_BACKUP_NOTICE;
pub use migrations::{
    LATEST_MEMORY_SCHEMA_VERSION, LATEST_SCHEMA_VERSION, MIGRATIONS, Migration,
    query_schema_version, run_migrations,
};
pub use pg_target::ValidatedPgTarget;
pub use portable::{
    ExportedClaim, ExportedDocument, ExportedNote, MAX_IMPORT_JSON_DEPTH, MAX_IMPORT_SAFE_BYTES,
    PORTABLE_FORMAT_VERSION, ParsedImportPackage, PortablePackageEnvelope, PortablePackagePayload,
    check_json_depth, has_unsafe_path_or_traversal, parse_and_validate_import_package,
    scrub_export_text, serialize_canonical_package,
};
pub use search::{
    PreparedSearchQuery, SEARCH_MODEL_VERSION, escape_like_pattern, prepare_search_query,
};
pub use service::{
    DEFAULT_PG_OP_TIMEOUT, DEFAULT_PG_PROBE_TIMEOUT, DEFAULT_POOL_MAX_SIZE, MEMORYD_STAGE_LABEL,
    MemoryService, d1_memoryd_capabilities,
};
