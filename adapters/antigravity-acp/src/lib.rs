//! Production Google Antigravity (`agy acp`) runtime adapter and narrow [`AgentRuntimePort`].
//!
//! Architectural boundaries (Contract M v2.0, Architecture 82 §§1–3):
//! - Depends strictly on `dennett-contracts`, `serde`, `serde_json`, `tokio`, and `uuid`.
//! - ZERO SQL drivers, ZERO UI frameworks, ZERO `dennett-memory-core`, and ZERO deferred
//!   provider crates (Claude Code / Codex / `OpenAI` / Anthropic).
//! - Explicitly separates live `AntigravityAcpRuntime` (`is_simulated: false`) from the
//!   deterministic CI [`SimulatedRuntime`] (`is_simulated: true`).

pub mod antigravity;
pub mod conformance;
pub mod port;
pub mod process_tree;
pub mod profile;
pub mod simulated;
pub mod transport;

pub use antigravity::{
    ANTIGRAVITY_ADAPTER_ID, AntigravityAcpRuntime, AntigravityRuntimeConfig,
    compute_skills_fingerprint,
};
pub use conformance::{ConformancePrompts, run_runtime_conformance_suite};
pub use port::{
    ActiveSkillPayload, AgentRuntimePort, BoxFuture, PolicyEvaluationOutcome,
    PolicyEvaluationRequest, PolicyEvaluator, RuntimeDescriptor, RuntimeEvent, RuntimeMcpEnvEntry,
    RuntimeMcpServerSpec, SessionStartOutcome, SessionStartRequest, TurnOutcome, TurnRequest,
    classify_provider_quota_error, evaluate_binding_resume_compatibility,
};
pub use process_tree::ProcessTreeGuard;
pub use profile::{
    ISOLATED_APP_DATA_REL, IsolatedProfilePaths, MIN_AGY_VERSION, Semver,
    build_effective_system_prompt, build_effective_system_prompt_with_skills, build_settings_json,
    build_write_gate_hooks_json, detect_agy_version, ensure_agy_version_supported,
    materialize_isolated_skills, normalize_workspace_path, parse_agy_version,
    prepare_isolated_profile, prepare_isolated_profile_with_skills, resolve_agy_binary,
    safe_skill_dir_name, workspace_paths_match,
};
pub use simulated::{SIMULATED_ADAPTER_ID, SimulatedRuntime};
pub use transport::{
    AcpInitializeInfo, AcpPermissionOption, AcpPermissionRequest, AcpProcess, AcpProcessConfig,
    AcpSessionInfo, AcpTurnParams, MAX_ACP_STDOUT_LINE_BYTES, parse_permission_request,
    select_permission_option,
};
