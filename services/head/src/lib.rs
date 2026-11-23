//! Dennett Head orchestration control library and service (`dennett-head`).
//!
//! Architectural boundary (Architecture 80 §3.1, §6):
//! - Owns `dennett_control` PostgreSQL schema, agents/config versions, projects, sessions,
//!   messages, runs, artifacts, external policy enforcement, durable memory outbox, and diagnostics.
//! - Communicates with standalone `dennett-memoryd` strictly over HTTP via `dennett-memory-client`
//!   (NEVER depends on `dennett-memory-core`).

pub mod http;
pub mod migrations;
pub mod pg_target;
pub mod policy;
pub mod service;
pub mod store;

pub use http::{HeadAppState, MAX_CONCURRENT_REQUESTS, MAX_HTTP_BODY_BYTES, build_head_router};
pub use migrations::{
    CONTROL_MIGRATIONS, ControlMigration, LATEST_CONTROL_SCHEMA_VERSION,
    query_control_schema_version, run_control_migrations, sha256_hex,
};
pub use pg_target::ValidatedControlPgTarget;
pub use policy::{PolicyContext, PolicyEngine, RecordingPolicyEvaluator, resolve_within_workspace};
pub use service::{
    DEFAULT_CONTROL_POOL_SIZE, DEFAULT_PG_OP_TIMEOUT, DEFAULT_PROBE_TIMEOUT, DEFAULT_TURN_TIMEOUT,
    HEAD_STAGE_LABEL, HeadService, MemoryMcpWiring, d1_head_capabilities,
};
