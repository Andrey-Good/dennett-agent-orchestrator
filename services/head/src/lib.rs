//! Dennett Head orchestration control library and service (`dennett-head`).
//!
//! Architectural boundary (Architecture 80 §3.1, §6):
//! - Owns `dennett_control` PostgreSQL schema, agents/config versions, projects, sessions,
//!   messages, runs, artifacts, durable tasks/timers/signals, interactive approvals/grants,
//!   external effect claims (`LostAck -> UNKNOWN`), ordered delivery journal, external policy
//!   enforcement, durable memory outbox, and diagnostics.
//! - Communicates with standalone `dennett-memoryd` strictly over HTTP via `dennett-memory-client`
//!   (NEVER depends on `dennett-memory-core`).

pub mod approvals;
pub mod capabilities;
pub mod connections;
pub mod effects;
pub mod events;
pub mod http;
pub mod lifecycle;
pub mod memory_auth;
pub mod memory_proxy;
pub mod migrations;
pub mod pg_target;
pub mod policy;
pub mod secrets;
pub mod service;
pub mod store;
pub mod tasks;
pub mod workspace_files;

pub use approvals::{
    CapturedPermissionDecision, DEFAULT_APPROVAL_WAIT_TIMEOUT, InteractivePolicyEvaluator,
};
pub use capabilities::{compute_capability_sha256, executable_surface_expanded};
pub use connections::static_planned_providers;
pub use effects::{
    ExternalDispatchInput, ExternalDispatchOutcome, ExternalEffectPort, ExternalReconcileResult,
    ProviderEffectReceipt, SIMULATED_EXTERNAL_CONNECTOR_ID, SimulatedExternalConnector,
};
pub use events::{
    ControlEventAppend, DEFAULT_EVENTS_LIMIT, MAX_EVENTS_LIMIT, append_control_event_in_tx,
    enqueue_ordered_outbox_in_tx,
};
pub use http::{HeadAppState, MAX_CONCURRENT_REQUESTS, MAX_HTTP_BODY_BYTES, build_head_router};
pub use memory_auth::{
    CONTROL_TOKEN_PREFIX, ControlTokenClaims, ControlTokenSigner, MemoryTokenSigner,
};
pub use migrations::{
    CONTROL_MIGRATIONS, ControlMigration, LATEST_CONTROL_SCHEMA_VERSION,
    query_control_schema_version, run_control_migrations, sha256_hex,
};
pub use pg_target::ValidatedControlPgTarget;
pub use policy::{
    PolicyContext, PolicyEngine, PolicyEvaluationDecision, RecordingPolicyEvaluator,
    resolve_within_workspace,
};
pub use secrets::{
    GEMINI_API_KEY_SLOT, SECRET_AEAD_ALGORITHM, SecretCipher, VaultMasterKey,
    compute_secret_fingerprint,
};
pub use service::{
    ArtifactContentBlob, DEFAULT_CONTROL_POOL_SIZE, DEFAULT_PG_OP_TIMEOUT, DEFAULT_PROBE_TIMEOUT,
    DEFAULT_TURN_TIMEOUT, HEAD_STAGE_LABEL, HeadService, HeadServiceExtras, MemoryMcpWiring,
    RUNTIME_CATALOG_CACHE_TTL, d1_head_capabilities, d2_head_capabilities, d3_head_capabilities,
};
pub use tasks::{
    BACKGROUND_SCHEDULER_POLL_INTERVAL, MISFIRE_GRACE_MS, TIMER_CLAIM_LEASE_SECS,
    compute_next_fire_at_pg, verify_pg_timezone,
};
pub use workspace_files::{list_project_workspace_files, validate_safe_workspace_root};
