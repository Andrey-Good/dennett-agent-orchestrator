//! Production Google Antigravity (`agy acp`) runtime adapter and narrow [`AgentRuntimePort`].
//!
//! Architectural boundaries (Contract M v2.0, Architecture 82 §§1–3):
//! - Depends strictly on `dennett-contracts`, `serde`, `serde_json`, `tokio`, and `uuid`.
//! - ZERO SQL drivers, ZERO UI frameworks, ZERO `dennett-memory-core`, and ZERO deferred
//!   provider crates (Claude Code / Codex / `OpenAI` / Anthropic).
//! - Explicitly separates live `AntigravityAcpRuntime` (`is_simulated: false`) from the
//!   deterministic CI [`SimulatedRuntime`] (`is_simulated: true`).

pub mod antigravity;
pub mod port;
pub mod profile;
pub mod simulated;
pub mod transport;

pub use antigravity::{ANTIGRAVITY_ADAPTER_ID, AntigravityAcpRuntime, AntigravityRuntimeConfig};
pub use port::{
    AgentRuntimePort, BoxFuture, PolicyEvaluationOutcome, PolicyEvaluationRequest, PolicyEvaluator,
    RuntimeDescriptor, RuntimeEvent, RuntimeMcpEnvEntry, RuntimeMcpServerSpec, SessionStartOutcome,
    SessionStartRequest, TurnOutcome, TurnRequest,
};
pub use profile::{
    ISOLATED_APP_DATA_REL, IsolatedProfilePaths, MIN_AGY_VERSION, Semver,
    build_effective_system_prompt, build_settings_json, build_write_gate_hooks_json,
    detect_agy_version, ensure_agy_version_supported, normalize_workspace_path, parse_agy_version,
    prepare_isolated_profile, resolve_agy_binary, workspace_paths_match,
};
pub use simulated::{SIMULATED_ADAPTER_ID, SimulatedRuntime};
pub use transport::{
    AcpInitializeInfo, AcpPermissionOption, AcpPermissionRequest, AcpProcess, AcpProcessConfig,
    AcpSessionInfo, AcpTurnParams, MAX_ACP_STDOUT_LINE_BYTES, parse_permission_request,
    select_permission_option,
};
