//! Production Google Antigravity (`agy acp`) implementation of [`AgentRuntimePort`].
//!
//! Enforces:
//! - Strict separation between `OAuthPersonal` (Google AI Ultra via OS keyring) and
//!   `GeminiApiKey` (isolated `modelProvider: "gemini"` with fail-closed auth and zero fallback).
//! - Isolated per-session `--gemini-dir` / `--app-data-dir` / `--include-default-customizations=false`
//!   / `--disable-telemetry` / `--log-file` profiles (never touching `~/.gemini`).
//! - Explicit session isolation and resume by exact `provider_session_id` + `workspace_cwd`
//!   (never `--continue`), with honest Dennett-side rehydration via `systemPrompt.append`.
//! - Real `dennett-memory-mcp` wiring via ACP `mcpServers` with scoped token redaction.
//! - Live cancellation via `session/cancel` and external `PolicyEvaluator` callbacks.

use core::fmt;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use dennett_contracts::redact::{REDACTED_PLACEHOLDER, redact_secrets_owned};
use dennett_contracts::{
    DennettError, RuntimeAuthProfile, RuntimeModelOption, SessionId, ThinkingLevel, TimestampMs,
};
use serde_json::Value;
use tokio::sync::{Mutex, mpsc, watch};

use crate::port::{
    ActiveSkillPayload, AgentRuntimePort, BoxFuture, PolicyEvaluator, RuntimeDescriptor,
    RuntimeEvent, RuntimeMcpServerSpec, SessionStartOutcome, SessionStartRequest, TurnOutcome,
    TurnRequest, evaluate_binding_resume_compatibility,
};
use crate::profile::{
    Semver, build_effective_system_prompt, detect_agy_version, normalize_workspace_path,
    prepare_isolated_profile_with_skills, resolve_agy_binary, workspace_paths_match,
};
use crate::transport::{
    AcpProcess, AcpProcessConfig, AcpTurnParams, extract_thinking_options_from_config,
};

/// Canonical adapter identifier for production Google Antigravity ACP runtime.
pub const ANTIGRAVITY_ADAPTER_ID: &str = "antigravity-acp";
/// Default timeout for ACP control handshakes (`initialize`, `authenticate`, `session/*`).
const DEFAULT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

/// Configuration for [`AntigravityAcpRuntime`].
///
/// `Debug` strictly redacts any configured `gemini_api_key`.
#[derive(Clone)]
pub struct AntigravityRuntimeConfig {
    /// Path to the `agy` binary (defaults to `DENNETT_AGY_BIN` or `"agy"` on `PATH`).
    pub binary: PathBuf,
    /// Root directory under which isolated per-session `--gemini-dir` profiles are stored.
    pub profile_root: PathBuf,
    /// Explicitly chosen authentication profile (`OAuthPersonal` vs `GeminiApiKey`).
    pub auth_profile: RuntimeAuthProfile,
    /// Explicit Gemini API key used ONLY when `auth_profile == RuntimeAuthProfile::GeminiApiKey`.
    pub gemini_api_key: Option<String>,
    /// Whether to pass `--sandbox` to `agy acp` for terminal restrictions.
    pub enable_sandbox: bool,
    /// Handshake RPC timeout.
    pub handshake_timeout: Duration,
}

impl fmt::Debug for AntigravityRuntimeConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AntigravityRuntimeConfig")
            .field("binary", &self.binary)
            .field("profile_root", &self.profile_root)
            .field("auth_profile", &self.auth_profile)
            .field(
                "gemini_api_key",
                &self.gemini_api_key.as_ref().map(|_| REDACTED_PLACEHOLDER),
            )
            .field("enable_sandbox", &self.enable_sandbox)
            .field("handshake_timeout", &self.handshake_timeout)
            .finish()
    }
}

impl AntigravityRuntimeConfig {
    /// Creates an interactive `OAuthPersonal` (Google AI Ultra) configuration storing isolated
    /// profiles under `profile_root`.
    #[must_use]
    pub fn oauth_personal(profile_root: PathBuf) -> Self {
        Self {
            binary: resolve_agy_binary(),
            profile_root,
            auth_profile: RuntimeAuthProfile::OAuthPersonal,
            gemini_api_key: None,
            enable_sandbox: false,
            handshake_timeout: DEFAULT_HANDSHAKE_TIMEOUT,
        }
    }

    /// Creates an isolated headless `GeminiApiKey` configuration (fails closed if `api_key` is `None`).
    #[must_use]
    pub fn gemini_api_key(profile_root: PathBuf, api_key: Option<String>) -> Self {
        Self {
            binary: resolve_agy_binary(),
            profile_root,
            auth_profile: RuntimeAuthProfile::GeminiApiKey,
            gemini_api_key: api_key,
            enable_sandbox: false,
            handshake_timeout: DEFAULT_HANDSHAKE_TIMEOUT,
        }
    }
}

/// Renewal safety margin before MCP signed token expiry (`dnt1.` / `dct1.`) triggering respawn (10 minutes).
const MCP_TOKEN_EXPIRY_MARGIN_MS: i64 = 10 * 60 * 1_000;

struct ActiveSessionSlot {
    process: Mutex<AcpProcess>,
    provider_session_id: String,
    workspace_cwd: String,
    tier: Option<String>,
    agy_version: String,
    available_models: Vec<String>,
    models: Vec<RuntimeModelOption>,
    applied_write_policy: dennett_contracts::WorkspaceWritePolicy,
    applied_system_prompt: String,
    applied_mcp_digest: u64,
    applied_skills_digest: u64,
    mcp_token_expires_at_ms: Option<i64>,
    applied_model: Mutex<Option<String>>,
    applied_thinking: Mutex<Option<ThinkingLevel>>,
    active_thinking_levels: Mutex<Vec<ThinkingLevel>>,
    active_cancel_tx: Mutex<Option<watch::Sender<bool>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SlotReuseAction {
    ReuseLive,
    RespawnAndResume,
    RespawnAndRehydrate { reason: String },
}

#[expect(
    clippy::too_many_arguments,
    reason = "slot reuse evaluates cwd, prompt, write policy, process health, MCP scope digest, active skills digest, and token expiry margin together"
)]
fn decide_slot_reuse(
    slot_cwd: &str,
    target_cwd: &str,
    slot_write_policy: &dennett_contracts::WorkspaceWritePolicy,
    req_write_policy: &dennett_contracts::WorkspaceWritePolicy,
    slot_system_prompt: &str,
    req_system_prompt: &str,
    slot_mcp_digest: u64,
    req_mcp_digest: u64,
    slot_skills_digest: u64,
    req_skills_digest: u64,
    slot_token_expires_at_ms: Option<i64>,
    now_ms: i64,
    process_exited: bool,
) -> SlotReuseAction {
    if !workspace_paths_match(slot_cwd, target_cwd) {
        return SlotReuseAction::RespawnAndRehydrate {
            reason: format!("workspace cwd mismatch (live `{slot_cwd}`, requested `{target_cwd}`)"),
        };
    }
    if slot_system_prompt != req_system_prompt {
        return SlotReuseAction::RespawnAndRehydrate {
            reason: String::from(
                "agent system_prompt changed in updated config version; rehydrating session with new prompt",
            ),
        };
    }
    let token_near_expiry = slot_token_expires_at_ms
        .is_some_and(|exp_ms| exp_ms.saturating_sub(now_ms) < MCP_TOKEN_EXPIRY_MARGIN_MS);
    if process_exited
        || slot_write_policy != req_write_policy
        || slot_mcp_digest != req_mcp_digest
        || slot_skills_digest != req_skills_digest
        || token_near_expiry
    {
        return SlotReuseAction::RespawnAndResume;
    }
    SlotReuseAction::ReuseLive
}

/// Computes a deterministic digest of active A08 skills so adding, updating, or disabling any
/// skill triggers a clean isolated profile rematerialization and respawn.
#[must_use]
pub fn compute_skills_fingerprint(active_skills: &[ActiveSkillPayload]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mix_bytes = |h: &mut u64, bytes: &[u8]| {
        for &b in bytes {
            *h ^= u64::from(b);
            *h = h.wrapping_mul(0x0100_0000_01b3);
        }
        *h ^= 0xfe;
        *h = h.wrapping_mul(0x0100_0000_01b3);
    };

    let mut sorted: Vec<&ActiveSkillPayload> = active_skills.iter().collect();
    sorted.sort_by(|a, b| a.slug.cmp(&b.slug).then_with(|| a.version.cmp(&b.version)));
    for skill in sorted {
        mix_bytes(&mut hash, b"skill");
        mix_bytes(&mut hash, skill.slug.as_bytes());
        mix_bytes(&mut hash, skill.version.as_bytes());
        mix_bytes(&mut hash, skill.content_sha256.as_bytes());
        mix_bytes(&mut hash, skill.skill_markdown.as_bytes());
    }
    hash
}

/// Computes a deterministic semantic digest of `mcp_servers` plus the earliest `expires_at` (ms)
/// observed in any `dnt1.` or `dct1.` signed MCP token (D2-SEC-01).
///
/// For `dnt1.` and `dct1.` tokens, hashes authorization-bearing claims (`allowed_scopes`,
/// `capabilities`, `project_id`, `agent_id`, `session_id`, `session_kind`, `principal_id`,
/// `issuer`, `audience`) rather than per-turn random `token_id`/`issued_at`, so normal
/// multi-turn reuse stays fast while any scope change, MCP server removal/addition, or
/// approaching token expiry (`< 10m`) forces a clean `RespawnAndResume`.
fn compute_mcp_fingerprint(mcp_servers: &[RuntimeMcpServerSpec]) -> (u64, Option<i64>) {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mix_bytes = |h: &mut u64, bytes: &[u8]| {
        for &b in bytes {
            *h ^= u64::from(b);
            *h = h.wrapping_mul(0x0100_0000_01b3);
        }
        *h ^= 0xff;
        *h = h.wrapping_mul(0x0100_0000_01b3);
    };

    let mut earliest_exp_ms: Option<i64> = None;
    let mut sorted_servers: Vec<&RuntimeMcpServerSpec> = mcp_servers.iter().collect();
    sorted_servers.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.command.cmp(&b.command)));

    for srv in sorted_servers {
        mix_bytes(&mut hash, b"server");
        mix_bytes(&mut hash, srv.name.as_bytes());
        mix_bytes(&mut hash, srv.command.as_bytes());
        for arg in &srv.args {
            mix_bytes(&mut hash, b"arg");
            mix_bytes(&mut hash, arg.as_bytes());
        }
        let mut sorted_env: Vec<(&str, &str)> = srv
            .env
            .iter()
            .map(|e| (e.name.as_str(), e.value.as_str()))
            .collect();
        sorted_env.sort_unstable();
        for (k, v) in sorted_env {
            mix_bytes(&mut hash, b"env");
            mix_bytes(&mut hash, k.as_bytes());
            if let Some((claims_summary, exp_opt)) = inspect_dennett_signed_token(v) {
                mix_bytes(&mut hash, claims_summary.as_bytes());
                if let Some(exp) = exp_opt {
                    earliest_exp_ms = Some(match earliest_exp_ms {
                        Some(cur) => cur.min(exp),
                        None => exp,
                    });
                }
            } else {
                mix_bytes(&mut hash, v.as_bytes());
            }
        }
    }

    (hash, earliest_exp_ms)
}

fn inspect_dennett_signed_token(raw: &str) -> Option<(String, Option<i64>)> {
    let (prefix, rest) = if let Some(r) = raw.strip_prefix("dnt1.") {
        ("dnt1", r)
    } else {
        let r = raw.strip_prefix("dct1.")?;
        ("dct1", r)
    };
    let (payload_b64, _sig) = rest.split_once('.')?;
    let bytes = decode_base64url_no_pad(payload_b64)?;
    let parsed: Value = serde_json::from_slice(&bytes).ok()?;
    let exp = parsed.get("expires_at").and_then(Value::as_i64);

    let mut summary = String::from(prefix);
    if prefix == "dnt1" {
        if let Some(p) = parsed.get("principal_id").and_then(Value::as_str) {
            summary.push('|');
            summary.push_str(p);
        }
        if let Some(iss) = parsed.get("issuer").and_then(Value::as_str) {
            summary.push('|');
            summary.push_str(iss);
        }
        if let Some(aud) = parsed.get("audience").and_then(Value::as_str) {
            summary.push('|');
            summary.push_str(aud);
        }
        if let Some(scopes) = parsed.get("allowed_scopes").and_then(Value::as_array) {
            let mut list: Vec<&str> = scopes.iter().filter_map(Value::as_str).collect();
            list.sort_unstable();
            summary.push_str("|scopes=");
            summary.push_str(&list.join(","));
        }
        if let Some(caps) = parsed.get("capabilities").and_then(Value::as_array) {
            let mut list: Vec<&str> = caps.iter().filter_map(Value::as_str).collect();
            list.sort_unstable();
            summary.push_str("|caps=");
            summary.push_str(&list.join(","));
        }
    } else {
        for field in ["project_id", "agent_id", "session_id", "session_kind"] {
            if let Some(val) = parsed.get(field).and_then(Value::as_str) {
                summary.push('|');
                summary.push_str(field);
                summary.push('=');
                summary.push_str(val);
            }
        }
    }
    Some((summary, exp))
}

fn decode_base64url_no_pad(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity((input.len() * 3) / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for byte in input.bytes() {
        let val = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            b'=' => continue,
            _ => return None,
        };
        buf = (buf << 6) | u32::from(val);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

/// Production [`AgentRuntimePort`] backed by local `agy acp` subprocesses over stdio JSON-RPC 2.0.
pub struct AntigravityAcpRuntime {
    config: AntigravityRuntimeConfig,
    sessions: Mutex<HashMap<SessionId, Arc<ActiveSessionSlot>>>,
}

impl AntigravityAcpRuntime {
    /// Creates a new production [`AntigravityAcpRuntime`].
    #[must_use]
    pub fn new(config: AntigravityRuntimeConfig) -> Self {
        Self {
            config,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    fn collect_secrets(&self, req: &SessionStartRequest) -> Vec<String> {
        let mut secrets = Vec::new();
        if let Some(ref key) = self.config.gemini_api_key
            && !key.is_empty()
        {
            secrets.push(key.clone());
        }
        for srv in &req.mcp_servers {
            for entry in &srv.env {
                let upper = entry.name.to_ascii_uppercase();
                if (upper.contains("TOKEN")
                    || upper.contains("SECRET")
                    || upper.contains("KEY")
                    || upper.contains("PASSWORD"))
                    && !entry.value.is_empty()
                {
                    secrets.push(entry.value.clone());
                }
            }
        }
        secrets
    }

    fn spawn_isolated_process(
        &self,
        session_id: SessionId,
        workspace_cwd: &std::path::Path,
        write_policy: &dennett_contracts::WorkspaceWritePolicy,
        active_skills: &[ActiveSkillPayload],
        secrets: Vec<String>,
    ) -> Result<AcpProcess, DennettError> {
        let profile = prepare_isolated_profile_with_skills(
            &self.config.profile_root,
            session_id,
            self.config.auth_profile,
            write_policy,
            active_skills,
        )?;

        let mut extra_args = vec![
            String::from("--gemini-dir"),
            profile.gemini_dir.to_string_lossy().into_owned(),
            String::from("--app-data-dir"),
            profile.app_data_rel,
            String::from("--include-default-customizations=false"),
            String::from("--disable-telemetry"),
            String::from("--log-file"),
            profile.log_file.to_string_lossy().into_owned(),
        ];
        if self.config.enable_sandbox {
            extra_args.push(String::from("--sandbox"));
        }

        let mut env_overrides = Vec::new();
        if self.config.auth_profile == RuntimeAuthProfile::GeminiApiKey
            && let Some(ref api_key) = self.config.gemini_api_key
            && !api_key.trim().is_empty()
        {
            env_overrides.push((String::from("GEMINI_API_KEY"), api_key.clone()));
        }

        AcpProcess::spawn(&AcpProcessConfig {
            binary: self.config.binary.clone(),
            extra_args,
            working_dir: workspace_cwd.to_path_buf(),
            env_overrides,
            secrets_to_redact: secrets,
        })
    }

    async fn initialize_and_authenticate(
        &self,
        proc: &mut AcpProcess,
    ) -> Result<(crate::transport::AcpInitializeInfo, Semver), DennettError> {
        let version = detect_agy_version(&self.config.binary).await?;
        let init = proc.initialize(self.config.handshake_timeout).await?;
        if init.protocol_version != 1 {
            return Err(DennettError::RuntimeProtocol {
                code: None,
                detail: format!(
                    "expected ACP protocolVersion 1, got {}",
                    init.protocol_version
                ),
            });
        }

        if self.config.auth_profile == RuntimeAuthProfile::GeminiApiKey {
            // Explicitly authenticate under `gemini-api-key` so missing/invalid keys fail closed
            // (`-32602` / `-32000`) and NEVER fall back to OAuthPersonal.
            proc.authenticate(
                RuntimeAuthProfile::GeminiApiKey,
                self.config.handshake_timeout,
            )
            .await
            .map_err(|err| map_auth_error(RuntimeAuthProfile::GeminiApiKey, err))?;
        }

        Ok((init, version))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "coordinates model normalization, per-model configOptions inspection, and thought_level clamping"
    )]
    async fn apply_model_and_thinking(
        &self,
        proc: &mut AcpProcess,
        provider_session_id: &str,
        available_models: &[String],
        models: &[RuntimeModelOption],
        initial_thinking_levels: &[ThinkingLevel],
        model_preference: Option<&str>,
        thinking_level: Option<ThinkingLevel>,
    ) -> Result<(Option<String>, Option<ThinkingLevel>, Vec<ThinkingLevel>), DennettError> {
        let (normalized_model, effective_requested_thinking) =
            match model_preference.map(str::trim).filter(|s| !s.is_empty()) {
                Some(pref) => {
                    let (chosen, inferred_tl) =
                        split_model_and_thinking(pref, thinking_level, available_models, models);
                    (Some(chosen), inferred_tl)
                }
                None => (None, thinking_level),
            };

        let mut supported_thinking = initial_thinking_levels.to_vec();
        let mut applied_model = None;
        if let Some(chosen) = normalized_model {
            let resp = proc
                .set_config_option(
                    provider_session_id,
                    "model",
                    &chosen,
                    self.config.handshake_timeout,
                )
                .await?;
            if let Some((updated_opts, _)) = extract_thinking_options_from_config(&resp) {
                supported_thinking = updated_opts.into_iter().map(|opt| opt.value).collect();
            } else if let Some(model_meta) = models.iter().find(|m| m.model_id == chosen) {
                supported_thinking.clone_from(&model_meta.supported_thinking_levels);
            }
            applied_model = Some(chosen);
        }

        let mut applied_thinking = None;
        if let Some(requested_level) = effective_requested_thinking
            && let Some(clamped) = clamp_thinking_level(requested_level, &supported_thinking)
        {
            proc.set_config_option(
                provider_session_id,
                "thought_level",
                clamped.as_str(),
                self.config.handshake_timeout,
            )
            .await?;
            applied_thinking = Some(clamped);
        }

        Ok((applied_model, applied_thinking, supported_thinking))
    }
}

impl AgentRuntimePort for AntigravityAcpRuntime {
    fn adapter_id(&self) -> &str {
        ANTIGRAVITY_ADAPTER_ID
    }

    fn is_simulated(&self) -> bool {
        false
    }

    fn auth_profile(&self) -> RuntimeAuthProfile {
        self.config.auth_profile
    }

    fn probe(&self) -> BoxFuture<'_, Result<RuntimeDescriptor, DennettError>> {
        Box::pin(async move {
            let version = detect_agy_version(&self.config.binary).await?;
            let probe_session = SessionId::new();
            let probe_ws = self
                .config
                .profile_root
                .join(format!("probe-{probe_session}"));
            std::fs::create_dir_all(&probe_ws).map_err(|err| DennettError::Internal {
                detail: format!("failed creating probe workspace dir: {err}"),
            })?;

            let mut secrets = Vec::new();
            if let Some(ref key) = self.config.gemini_api_key
                && !key.is_empty()
            {
                secrets.push(key.clone());
            }

            let mut proc = self.spawn_isolated_process(
                probe_session,
                &probe_ws,
                &dennett_contracts::WorkspaceWritePolicy::DenyAll,
                &[],
                secrets,
            )?;
            let init = proc.initialize(self.config.handshake_timeout).await?;

            if self.config.auth_profile == RuntimeAuthProfile::GeminiApiKey {
                let auth_res = proc
                    .authenticate(
                        RuntimeAuthProfile::GeminiApiKey,
                        self.config.handshake_timeout,
                    )
                    .await;
                if let Err(err) = auth_res {
                    proc.shutdown().await;
                    let _ = std::fs::remove_dir_all(&probe_ws);
                    let _ = std::fs::remove_dir_all(
                        self.config
                            .profile_root
                            .join(format!("session-{probe_session}")),
                    );
                    return Err(map_auth_error(RuntimeAuthProfile::GeminiApiKey, err));
                }
            }

            let sess_res = proc
                .session_new(&probe_ws, &[], None, self.config.handshake_timeout)
                .await;
            proc.shutdown().await;
            let _ = std::fs::remove_dir_all(&probe_ws);
            let _ = std::fs::remove_dir_all(
                self.config
                    .profile_root
                    .join(format!("session-{probe_session}")),
            );

            let sess = sess_res.map_err(|err| map_auth_error(self.config.auth_profile, err))?;
            if self.config.auth_profile == RuntimeAuthProfile::OAuthPersonal
                && sess.tier.as_deref().is_none_or(|t| t.trim().is_empty())
            {
                return Err(DennettError::AuthRequired {
                    profile: RuntimeAuthProfile::OAuthPersonal,
                    detail: String::from(
                        "agy acp did not report a non-empty `_meta.antigravity.tier` for oauth-personal profile",
                    ),
                });
            }

            Ok(RuntimeDescriptor {
                adapter_id: String::from(ANTIGRAVITY_ADAPTER_ID),
                auth_profile: self.config.auth_profile,
                is_simulated: false,
                agy_version: Some(version.to_string()),
                tier: sess.tier,
                protocol_version: Some(init.protocol_version),
                supports_resume: init.supports_load_session,
                supports_system_prompt_append: init.supports_system_prompt_append,
                available_models: sess.available_models,
                models: sess.models,
                thinking_options: sess.thinking_options,
                default_model_id: sess.default_model_id,
                default_thinking_level: sess.default_thinking_level,
                quotas: sess.quotas,
                capabilities: vec![
                    String::from("acp_v1_stdio"),
                    String::from("isolated_profile"),
                    String::from("mcp_servers"),
                    String::from("external_permissions"),
                    String::from("live_cancel"),
                ],
            })
        })
    }

    #[expect(
        clippy::too_many_lines,
        reason = "start_or_resume_session keeps live-slot reuse, explicit resume, and honest rehydrate transitions together"
    )]
    fn start_or_resume_session(
        &self,
        req: SessionStartRequest,
    ) -> BoxFuture<'_, Result<SessionStartOutcome, DennettError>> {
        Box::pin(async move {
            let target_cwd = normalize_workspace_path(&req.workspace_cwd);
            let secrets = self.collect_secrets(&req);
            let (req_mcp_digest, req_mcp_token_expires_at_ms) =
                compute_mcp_fingerprint(&req.mcp_servers);
            let req_skills_digest = compute_skills_fingerprint(&req.active_skills);
            let now_ms = TimestampMs::now().as_millis();

            // 0. V29 fail-fast static compatibility check before touching any live process or spawning
            //    `agy acp` when explicit resume is requested with `allow_rehydrate == false`.
            if !req.allow_rehydrate
                && let Some(ref binding) = req.existing_binding
                && let Err(reason) = evaluate_binding_resume_compatibility(
                    binding,
                    ANTIGRAVITY_ADAPTER_ID,
                    false,
                    self.config.auth_profile,
                    &target_cwd,
                    None,
                    None,
                )
            {
                return Err(DennettError::Conflict {
                    resource: String::from("runtime_binding"),
                    expected_revision: None,
                    actual_revision: None,
                    reason,
                });
            }

            // 1. Check if we already hold a live process for this Dennett session in the exact workspace
            //    with matching `workspace_write_policy`, `system_prompt`, MCP scope digest, active skills
            //    digest, and fresh token.
            let existing_slot = {
                let guard = self.sessions.lock().await;
                guard.get(&req.dennett_session_id).cloned()
            };
            let mut forced_rehydrate_reason: Option<String> = None;
            let mut prior_slot_provider_id: Option<String> = None;
            let mut prior_slot_tier: Option<String> = None;

            if let Some(slot) = existing_slot {
                let mut proc = slot.process.lock().await;
                let action = decide_slot_reuse(
                    &slot.workspace_cwd,
                    &target_cwd,
                    &slot.applied_write_policy,
                    &req.workspace_write_policy,
                    &slot.applied_system_prompt,
                    &req.system_prompt,
                    slot.applied_mcp_digest,
                    req_mcp_digest,
                    slot.applied_skills_digest,
                    req_skills_digest,
                    slot.mcp_token_expires_at_ms,
                    now_ms,
                    proc.has_exited(),
                );
                if action == SlotReuseAction::ReuseLive {
                    proc.add_secrets(secrets.clone());

                    // Apply live model/thinking changes on the existing session (A04: same AgentId & session).
                    let (wanted_model, effective_thinking) = match req
                        .model_preference
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                    {
                        Some(pref) => {
                            let (chosen, inferred_tl) = split_model_and_thinking(
                                pref,
                                req.thinking_level,
                                &slot.available_models,
                                &slot.models,
                            );
                            (Some(chosen), inferred_tl)
                        }
                        None => (None, req.thinking_level),
                    };

                    let mut applied_model_guard = slot.applied_model.lock().await;
                    let mut supported_guard = slot.active_thinking_levels.lock().await;
                    let mut applied_thinking_guard = slot.applied_thinking.lock().await;

                    let mut model_switched = false;
                    if wanted_model.is_some()
                        && *applied_model_guard != wanted_model
                        && let Some(ref model_val) = wanted_model
                    {
                        let resp = proc
                            .set_config_option(
                                &slot.provider_session_id,
                                "model",
                                model_val,
                                self.config.handshake_timeout,
                            )
                            .await?;
                        if let Some((updated_opts, _)) = extract_thinking_options_from_config(&resp)
                        {
                            *supported_guard =
                                updated_opts.into_iter().map(|opt| opt.value).collect();
                        } else if let Some(model_meta) =
                            slot.models.iter().find(|m| m.model_id == *model_val)
                        {
                            supported_guard.clone_from(&model_meta.supported_thinking_levels);
                        }
                        *applied_model_guard = Some(model_val.clone());
                        *applied_thinking_guard = None;
                        model_switched = true;
                    }

                    if let Some(requested_tl) = effective_thinking {
                        let clamped = clamp_thinking_level(requested_tl, &supported_guard);
                        if let Some(target_tl) = clamped
                            && (model_switched || *applied_thinking_guard != Some(target_tl))
                        {
                            proc.set_config_option(
                                &slot.provider_session_id,
                                "thought_level",
                                target_tl.as_str(),
                                self.config.handshake_timeout,
                            )
                            .await?;
                            *applied_thinking_guard = Some(target_tl);
                        } else if clamped.is_none() {
                            *applied_thinking_guard = None;
                        }
                    }

                    return Ok(SessionStartOutcome {
                        adapter_id: String::from(ANTIGRAVITY_ADAPTER_ID),
                        auth_profile: self.config.auth_profile,
                        tier: slot.tier.clone(),
                        agy_version: Some(slot.agy_version.clone()),
                        provider_session_id: slot.provider_session_id.clone(),
                        workspace_cwd: slot.workspace_cwd.clone(),
                        is_simulated: false,
                        rehydrated: false,
                        rehydrate_reason: None,
                        available_models: slot.available_models.clone(),
                        endpoint_fingerprint: None,
                    });
                }

                // Process exited OR `workspace_write_policy` / `system_prompt` / `cwd` changed:
                // drop our handle to `slot` BEFORE `close_session` so `Arc::try_unwrap` shuts down
                // the old `agy acp` process cleanly before we rewrite `hooks.json`/`settings.json`.
                prior_slot_provider_id = Some(slot.provider_session_id.clone());
                prior_slot_tier = slot.tier.clone();
                if let SlotReuseAction::RespawnAndRehydrate { reason } = action {
                    forced_rehydrate_reason = Some(reason);
                }
                drop(proc);
                drop(slot);
                let _ = self.close_session(req.dennett_session_id).await;
            }

            if let Some(ref reason) = forced_rehydrate_reason
                && !req.allow_rehydrate
            {
                return Err(DennettError::Conflict {
                    resource: String::from("runtime_binding"),
                    expected_revision: None,
                    actual_revision: None,
                    reason: reason.clone(),
                });
            }

            // 2. Spawn fresh isolated `agy acp` process for this Dennett session (re-materializes
            //    `settings.json`, `<gemini-dir>/config/hooks.json`, and `<gemini-dir>/config/skills/*`).
            let mut proc = self.spawn_isolated_process(
                req.dennett_session_id,
                &req.workspace_cwd,
                &req.workspace_write_policy,
                &req.active_skills,
                secrets.clone(),
            )?;
            let (_init, version) = self.initialize_and_authenticate(&mut proc).await?;

            // 3. Explicit resume check against persisted RuntimeBinding (A04 / V29).
            let mut rehydrated = forced_rehydrate_reason.is_some();
            let mut rehydrate_reason: Option<String> = forced_rehydrate_reason;
            let mut session_info = None;

            if !rehydrated {
                let resume_candidate: Option<(String, Option<String>)> =
                    if let Some(ref binding) = req.existing_binding {
                        if let Err(msg) = evaluate_binding_resume_compatibility(
                            binding,
                            ANTIGRAVITY_ADAPTER_ID,
                            false,
                            self.config.auth_profile,
                            &target_cwd,
                            Some(version),
                            None,
                        ) {
                            if !req.allow_rehydrate {
                                proc.shutdown().await;
                                return Err(DennettError::Conflict {
                                    resource: String::from("runtime_binding"),
                                    expected_revision: None,
                                    actual_revision: None,
                                    reason: msg,
                                });
                            }
                            rehydrated = true;
                            rehydrate_reason = Some(msg);
                            None
                        } else {
                            let carried_tier = binding.tier.clone().or(prior_slot_tier);
                            binding
                                .provider_session_id
                                .clone()
                                .or(prior_slot_provider_id)
                                .map(|id| (id, carried_tier))
                        }
                    } else {
                        prior_slot_provider_id.map(|id| (id, prior_slot_tier))
                    };

                if let Some((prov_id, carried_tier)) = resume_candidate {
                    match proc
                        .session_load(
                            &prov_id,
                            &req.workspace_cwd,
                            &req.mcp_servers,
                            self.config.handshake_timeout,
                        )
                        .await
                    {
                        Ok(mut loaded) => {
                            if loaded.tier.as_deref().is_none_or(|t| t.trim().is_empty()) {
                                loaded.tier = carried_tier;
                            }
                            session_info = Some(loaded);
                        }
                        Err(err) => {
                            let safe_reason = redact_secrets_owned(
                                &format!("explicit session/load failed for `{prov_id}`: {err}"),
                                &secrets,
                            );
                            if !req.allow_rehydrate {
                                proc.shutdown().await;
                                return Err(DennettError::RuntimeProtocol {
                                    code: match err {
                                        DennettError::RuntimeProtocol { code, .. } => code,
                                        _ => None,
                                    },
                                    detail: safe_reason,
                                });
                            }
                            rehydrated = true;
                            rehydrate_reason = Some(safe_reason);
                        }
                    }
                }
            }

            // 4. Start new ACP session (fresh or explicit honest rehydrate from persisted history).
            let info = if let Some(loaded) = session_info {
                loaded
            } else {
                let effective_prompt = build_effective_system_prompt(
                    &req.system_prompt,
                    &req.history_for_rehydrate,
                    rehydrate_reason.as_deref(),
                );
                match proc
                    .session_new(
                        &req.workspace_cwd,
                        &req.mcp_servers,
                        Some(&effective_prompt),
                        self.config.handshake_timeout,
                    )
                    .await
                {
                    Ok(created) => created,
                    Err(err) => {
                        proc.shutdown().await;
                        return Err(map_auth_error(self.config.auth_profile, err));
                    }
                }
            };

            if self.config.auth_profile == RuntimeAuthProfile::OAuthPersonal
                && info.tier.as_deref().is_none_or(|t| t.trim().is_empty())
            {
                proc.shutdown().await;
                return Err(DennettError::AuthRequired {
                    profile: RuntimeAuthProfile::OAuthPersonal,
                    detail: String::from(
                        "oauth-personal session missing non-empty `_meta.antigravity.tier`",
                    ),
                });
            }

            let initial_thinking_levels =
                info.active_thinking_levels.clone().unwrap_or_else(|| {
                    info.default_model_id
                        .as_deref()
                        .and_then(|mid| info.models.iter().find(|m| m.model_id == mid))
                        .map_or_default(|m| m.supported_thinking_levels.clone())
                });

            let (applied_model, applied_thinking, active_thinking_levels) = self
                .apply_model_and_thinking(
                    &mut proc,
                    &info.session_id,
                    &info.available_models,
                    &info.models,
                    &initial_thinking_levels,
                    req.model_preference.as_deref(),
                    req.thinking_level,
                )
                .await?;

            let provider_session_id = info.session_id.clone();
            let tier = info.tier.clone();
            let available_models = info.available_models.clone();
            let models = info.models.clone();
            let agy_version = version.to_string();

            let slot = Arc::new(ActiveSessionSlot {
                process: Mutex::new(proc),
                provider_session_id: provider_session_id.clone(),
                workspace_cwd: target_cwd.clone(),
                tier: tier.clone(),
                agy_version: agy_version.clone(),
                available_models: available_models.clone(),
                models,
                applied_write_policy: req.workspace_write_policy,
                applied_system_prompt: req.system_prompt,
                applied_mcp_digest: req_mcp_digest,
                applied_skills_digest: req_skills_digest,
                mcp_token_expires_at_ms: req_mcp_token_expires_at_ms,
                applied_model: Mutex::new(applied_model),
                applied_thinking: Mutex::new(applied_thinking),
                active_thinking_levels: Mutex::new(active_thinking_levels),
                active_cancel_tx: Mutex::new(None),
            });

            {
                let mut guard = self.sessions.lock().await;
                guard.insert(req.dennett_session_id, slot);
            }

            Ok(SessionStartOutcome {
                adapter_id: String::from(ANTIGRAVITY_ADAPTER_ID),
                auth_profile: self.config.auth_profile,
                tier,
                agy_version: Some(agy_version),
                provider_session_id,
                workspace_cwd: target_cwd,
                is_simulated: false,
                rehydrated,
                rehydrate_reason,
                available_models,
                endpoint_fingerprint: None,
            })
        })
    }

    fn send_prompt_turn(
        &self,
        req: TurnRequest,
        policy: Arc<dyn PolicyEvaluator>,
        event_tx: Option<mpsc::Sender<RuntimeEvent>>,
    ) -> BoxFuture<'_, Result<TurnOutcome, DennettError>> {
        Box::pin(async move {
            let slot = {
                let guard = self.sessions.lock().await;
                guard.get(&req.dennett_session_id).cloned()
            }
            .ok_or_else(|| DennettError::NotFound {
                resource: String::from("runtime_session"),
                id: req.dennett_session_id.to_string(),
            })?;

            // Bridge both caller's `req.cancel_rx` and runtime-level `cancel_turn(session_id)`.
            let (combined_cancel_tx, combined_cancel_rx) = watch::channel(*req.cancel_rx.borrow());
            {
                let mut active = slot.active_cancel_tx.lock().await;
                *active = Some(combined_cancel_tx.clone());
            }
            let mut caller_rx = req.cancel_rx;
            let forward_tx = combined_cancel_tx.clone();
            let forwarder = tokio::spawn(async move {
                while !*caller_rx.borrow() {
                    if caller_rx.changed().await.is_err() {
                        break;
                    }
                }
                if *caller_rx.borrow() {
                    let _ = forward_tx.send(true);
                }
            });

            let turn_res = {
                let mut proc = slot.process.lock().await;
                proc.run_prompt_turn(AcpTurnParams {
                    dennett_session_id: req.dennett_session_id,
                    provider_session_id: &req.provider_session_id,
                    run_id: req.run_id,
                    prompt_text: &req.prompt_text,
                    timeout: req.timeout,
                    cancel_rx: combined_cancel_rx,
                    policy,
                    event_tx,
                })
                .await
            };

            forwarder.abort();
            {
                let mut active = slot.active_cancel_tx.lock().await;
                *active = None;
            }
            drop(slot);

            match turn_res {
                Ok((outcome, needs_reset)) => {
                    if needs_reset {
                        let _ = self.close_session(req.dennett_session_id).await;
                    }
                    Ok(outcome)
                }
                Err(err) => {
                    // Reset broken/crashed process so subsequent turns cleanly recover/rehydrate.
                    let _ = self.close_session(req.dennett_session_id).await;
                    Err(err)
                }
            }
        })
    }

    fn cancel_turn(
        &self,
        dennett_session_id: SessionId,
    ) -> BoxFuture<'_, Result<bool, DennettError>> {
        Box::pin(async move {
            let slot = {
                let guard = self.sessions.lock().await;
                guard.get(&dennett_session_id).cloned()
            };
            let Some(slot) = slot else {
                return Ok(false);
            };
            let active = slot.active_cancel_tx.lock().await;
            if let Some(ref tx) = *active {
                let _ = tx.send(true);
                return Ok(true);
            }
            Ok(false)
        })
    }

    fn close_session(
        &self,
        dennett_session_id: SessionId,
    ) -> BoxFuture<'_, Result<(), DennettError>> {
        Box::pin(async move {
            let removed = {
                let mut guard = self.sessions.lock().await;
                guard.remove(&dennett_session_id)
            };
            if let Some(slot) = removed {
                if let Some(tx) = slot.active_cancel_tx.lock().await.take() {
                    let _ = tx.send(true);
                }
                slot.process.lock().await.shutdown().await;
            }
            Ok(())
        })
    }
}

fn resolve_preferred_model(
    preference: &str,
    available_models: &[String],
    models: &[RuntimeModelOption],
) -> String {
    if models.iter().any(|m| m.model_id == preference)
        || available_models.iter().any(|m| m == preference)
    {
        return preference.to_owned();
    }
    if preference.eq_ignore_ascii_case("flash")
        && let Some(found) = models
            .iter()
            .map(|m| &m.model_id)
            .chain(available_models.iter())
            .find(|m| m.to_ascii_lowercase().contains("flash"))
    {
        return found.clone();
    }
    preference.to_owned()
}

fn split_model_and_thinking(
    preference: &str,
    explicit_thinking: Option<ThinkingLevel>,
    available_models: &[String],
    models: &[RuntimeModelOption],
) -> (String, Option<ThinkingLevel>) {
    if models.iter().any(|m| m.model_id == preference) {
        return (preference.to_owned(), explicit_thinking);
    }
    for (suffix, level) in [
        ("-xhigh", ThinkingLevel::XHigh),
        ("-high", ThinkingLevel::High),
        ("-medium", ThinkingLevel::Medium),
        ("-low", ThinkingLevel::Low),
    ] {
        if let Some(base) = preference.strip_suffix(suffix)
            && !base.is_empty()
            && (models.iter().any(|m| m.model_id == base)
                || available_models.iter().any(|m| m == base))
        {
            return (base.to_owned(), explicit_thinking.or(Some(level)));
        }
    }
    (
        resolve_preferred_model(preference, available_models, models),
        explicit_thinking,
    )
}

fn clamp_thinking_level(
    requested: ThinkingLevel,
    supported: &[ThinkingLevel],
) -> Option<ThinkingLevel> {
    if supported.is_empty() {
        return None;
    }
    if supported.contains(&requested) {
        return Some(requested);
    }
    supported
        .iter()
        .copied()
        .filter(|lvl| *lvl <= requested)
        .max()
        .or_else(|| supported.iter().copied().min())
}

fn map_auth_error(profile: RuntimeAuthProfile, err: DennettError) -> DennettError {
    match err {
        DennettError::RuntimeProtocol {
            code: Some(code @ (-32602 | -32000)),
            detail,
        } => DennettError::AuthRequired {
            profile,
            detail: format!("JSON-RPC {code}: {detail}"),
        },
        other => other,
    }
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<AntigravityRuntimeConfig>();
    assert_send_sync::<AntigravityAcpRuntime>();
};

#[cfg(test)]
mod tests {
    use dennett_contracts::{
        DennettError, RuntimeAuthProfile, RuntimeBinding, RuntimeBindingId, RuntimeModelOption,
        SessionId, ThinkingLevel, TimestampMs, WorkspaceWritePolicy,
    };

    use super::{
        ANTIGRAVITY_ADAPTER_ID, AntigravityAcpRuntime, AntigravityRuntimeConfig,
        MCP_TOKEN_EXPIRY_MARGIN_MS, SlotReuseAction, clamp_thinking_level, compute_mcp_fingerprint,
        compute_skills_fingerprint, decide_slot_reuse, split_model_and_thinking,
    };
    use crate::port::{
        ActiveSkillPayload, AgentRuntimePort, RuntimeMcpEnvEntry, RuntimeMcpServerSpec,
        SessionStartRequest, evaluate_binding_resume_compatibility,
    };
    use crate::profile::Semver;

    #[test]
    fn split_and_clamp_handles_carbon_xhigh_and_claude_builtin_thinking() {
        let models = vec![
            RuntimeModelOption {
                model_id: String::from("gemini-4-neon"),
                label: String::from("Gemini 4 Neon"),
                supported_thinking_levels: vec![
                    ThinkingLevel::Low,
                    ThinkingLevel::Medium,
                    ThinkingLevel::High,
                    ThinkingLevel::XHigh,
                ],
                default_thinking_level: Some(ThinkingLevel::Medium),
            },
            RuntimeModelOption {
                model_id: String::from("gemini-4-carbon"),
                label: String::from("Gemini 4 Carbon"),
                supported_thinking_levels: vec![
                    ThinkingLevel::Low,
                    ThinkingLevel::Medium,
                    ThinkingLevel::High,
                ],
                default_thinking_level: Some(ThinkingLevel::Medium),
            },
            RuntimeModelOption {
                model_id: String::from("claude-opus-4-6-thinking"),
                label: String::from("Claude Opus 4.6 (Thinking)"),
                supported_thinking_levels: Vec::new(),
                default_thinking_level: None,
            },
        ];
        let avail: Vec<String> = models.iter().map(|m| m.model_id.clone()).collect();

        assert_eq!(
            split_model_and_thinking("gemini-4-carbon-high", None, &avail, &models),
            (String::from("gemini-4-carbon"), Some(ThinkingLevel::High))
        );
        assert_eq!(
            split_model_and_thinking(
                "gemini-4-carbon-high",
                Some(ThinkingLevel::Low),
                &avail,
                &models
            ),
            (String::from("gemini-4-carbon"), Some(ThinkingLevel::Low))
        );
        assert_eq!(
            clamp_thinking_level(ThinkingLevel::XHigh, &models[1].supported_thinking_levels),
            Some(ThinkingLevel::High)
        );
        assert_eq!(
            clamp_thinking_level(ThinkingLevel::High, &models[2].supported_thinking_levels),
            None
        );
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "exhaustively verifies all decide_slot_reuse dimensions including MCP digest and token expiry margin"
    )]
    fn slot_reuse_restarts_on_write_policy_mcp_scope_or_expiring_token_and_rehydrates_on_prompt_change()
     {
        let allow_docs = WorkspaceWritePolicy::AllowPaths {
            allowed_prefixes: vec![String::from("docs")],
        };
        let now_ms: i64 = 1_750_000_000_000;
        let fresh_exp = Some(now_ms + MCP_TOKEN_EXPIRY_MARGIN_MS + 60_000);
        let expiring_exp = Some(now_ms + MCP_TOKEN_EXPIRY_MARGIN_MS - 1_000);

        assert_eq!(
            decide_slot_reuse(
                "C:/ws",
                "C:/ws",
                &WorkspaceWritePolicy::DenyAll,
                &WorkspaceWritePolicy::DenyAll,
                "prompt v1",
                "prompt v1",
                42,
                42,
                7,
                7,
                fresh_exp,
                now_ms,
                false
            ),
            SlotReuseAction::ReuseLive
        );

        // Changing DenyAll -> AllowPaths or AllowWorkspace -> DenyAll MUST respawn to rewrite hooks.json/settings.json!
        assert_eq!(
            decide_slot_reuse(
                "C:/ws",
                "C:/ws",
                &WorkspaceWritePolicy::DenyAll,
                &allow_docs,
                "prompt v1",
                "prompt v1",
                42,
                42,
                7,
                7,
                fresh_exp,
                now_ms,
                false
            ),
            SlotReuseAction::RespawnAndResume
        );
        assert_eq!(
            decide_slot_reuse(
                "C:/ws",
                "C:/ws",
                &WorkspaceWritePolicy::AllowWorkspace,
                &WorkspaceWritePolicy::DenyAll,
                "prompt v1",
                "prompt v1",
                42,
                42,
                7,
                7,
                fresh_exp,
                now_ms,
                false
            ),
            SlotReuseAction::RespawnAndResume
        );

        // D2-SEC-01: MCP scope digest change MUST trigger RespawnAndResume!
        assert_eq!(
            decide_slot_reuse(
                "C:/ws",
                "C:/ws",
                &allow_docs,
                &allow_docs,
                "prompt v1",
                "prompt v1",
                42,
                99,
                7,
                7,
                fresh_exp,
                now_ms,
                false
            ),
            SlotReuseAction::RespawnAndResume
        );

        // A08: Active skills digest change MUST trigger RespawnAndResume!
        assert_eq!(
            decide_slot_reuse(
                "C:/ws",
                "C:/ws",
                &allow_docs,
                &allow_docs,
                "prompt v1",
                "prompt v1",
                42,
                42,
                7,
                8,
                fresh_exp,
                now_ms,
                false
            ),
            SlotReuseAction::RespawnAndResume
        );

        // D2-SEC-01: MCP token within < 10m of expiry MUST trigger RespawnAndResume!
        assert_eq!(
            decide_slot_reuse(
                "C:/ws",
                "C:/ws",
                &allow_docs,
                &allow_docs,
                "prompt v1",
                "prompt v1",
                42,
                42,
                7,
                7,
                expiring_exp,
                now_ms,
                false
            ),
            SlotReuseAction::RespawnAndResume
        );

        // Exited process respawns and attempts explicit session/load resume.
        assert_eq!(
            decide_slot_reuse(
                "C:/ws",
                "C:/ws",
                &allow_docs,
                &allow_docs,
                "prompt v1",
                "prompt v1",
                42,
                42,
                7,
                7,
                fresh_exp,
                now_ms,
                true
            ),
            SlotReuseAction::RespawnAndResume
        );

        // System prompt change MUST rehydrate via session/new (session/load cannot update systemPrompt.append).
        assert!(matches!(
            decide_slot_reuse(
                "C:/ws",
                "C:/ws",
                &allow_docs,
                &allow_docs,
                "prompt v1",
                "prompt v2 (updated)",
                42,
                42,
                7,
                7,
                fresh_exp,
                now_ms,
                false
            ),
            SlotReuseAction::RespawnAndRehydrate { .. }
        ));

        // Cwd mismatch MUST rehydrate.
        assert!(matches!(
            decide_slot_reuse(
                "C:/ws-a",
                "C:/ws-b",
                &allow_docs,
                &allow_docs,
                "prompt v1",
                "prompt v1",
                42,
                42,
                7,
                7,
                fresh_exp,
                now_ms,
                false
            ),
            SlotReuseAction::RespawnAndRehydrate { .. }
        ));
    }

    #[tokio::test]
    async fn v29_rejects_adapter_profile_major_version_and_endpoint_mismatches() {
        let now = TimestampMs::now();
        let base_binding = RuntimeBinding {
            binding_id: RuntimeBindingId::new(),
            session_id: SessionId::new(),
            adapter_id: String::from(ANTIGRAVITY_ADAPTER_ID),
            auth_profile: RuntimeAuthProfile::OAuthPersonal,
            tier: Some(String::from("Google AI Ultra")),
            agy_version: Some(String::from("1.5.15")),
            provider_session_id: Some(String::from("prov-v29")),
            workspace_cwd: String::from("C:/ws"),
            is_simulated: false,
            rehydrate_count: 0,
            connection_id: None,
            endpoint_fingerprint: None,
            created_at: now,
            updated_at: now,
        };

        // Exact match passes (even on minor upgrade 1.5.15 -> 1.6.0).
        assert!(
            evaluate_binding_resume_compatibility(
                &base_binding,
                ANTIGRAVITY_ADAPTER_ID,
                false,
                RuntimeAuthProfile::OAuthPersonal,
                "C:/ws",
                Some(Semver::new(1, 6, 0)),
                None,
            )
            .is_ok()
        );

        // Auth profile mismatch (OAuthPersonal -> GeminiApiKey) MUST fail V29.
        assert!(
            evaluate_binding_resume_compatibility(
                &base_binding,
                ANTIGRAVITY_ADAPTER_ID,
                false,
                RuntimeAuthProfile::GeminiApiKey,
                "C:/ws",
                Some(Semver::new(1, 5, 15)),
                None,
            )
            .is_err()
        );

        // Adapter mismatch (antigravity-acp -> local-http-runtime) MUST fail V29.
        assert!(
            evaluate_binding_resume_compatibility(
                &base_binding,
                "local-http-runtime",
                false,
                RuntimeAuthProfile::LocalEndpoint,
                "C:/ws",
                None,
                Some("http://127.0.0.1:11434"),
            )
            .is_err()
        );

        // Major version change (1.5.15 -> 2.0.0) MUST fail V29.
        assert!(
            evaluate_binding_resume_compatibility(
                &base_binding,
                ANTIGRAVITY_ADAPTER_ID,
                false,
                RuntimeAuthProfile::OAuthPersonal,
                "C:/ws",
                Some(Semver::new(2, 0, 0)),
                None,
            )
            .is_err()
        );

        // AntigravityAcpRuntime must reject mismatched profile/adapter with Conflict before spawning agy!
        let rt = AntigravityAcpRuntime::new(AntigravityRuntimeConfig::gemini_api_key(
            std::env::temp_dir(),
            None,
        ));
        let conflict = rt
            .start_or_resume_session(SessionStartRequest {
                dennett_session_id: base_binding.session_id,
                workspace_cwd: std::path::PathBuf::from("C:/ws"),
                system_prompt: String::from("sys"),
                model_preference: None,
                thinking_level: None,
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                mcp_servers: Vec::new(),
                active_skills: Vec::new(),
                existing_binding: Some(base_binding),
                history_for_rehydrate: Vec::new(),
                allow_rehydrate: false,
            })
            .await;
        assert!(matches!(conflict, Err(DennettError::Conflict { .. })));

        // Active skills digest must change when version or content changes.
        let s1 = ActiveSkillPayload {
            slug: String::from("audit"),
            version: String::from("1.0.0"),
            content_sha256: "a".repeat(64),
            skill_markdown: String::from("# v1"),
        };
        let s2 = ActiveSkillPayload {
            version: String::from("1.0.1"),
            ..s1.clone()
        };
        assert_ne!(
            compute_skills_fingerprint(&[s1]),
            compute_skills_fingerprint(&[s2])
        );
    }

    #[test]
    fn d2_sec_01_mcp_fingerprint_tracks_scopes_and_earliest_expiry_ignoring_token_nonce() {
        fn b64url(s: &str) -> String {
            const TABLE: &[u8; 64] =
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
            let bytes = s.as_bytes();
            let mut out = String::new();
            let mut i = 0;
            while i < bytes.len() {
                let b0 = bytes[i];
                let b1 = if i + 1 < bytes.len() { bytes[i + 1] } else { 0 };
                let b2 = if i + 2 < bytes.len() { bytes[i + 2] } else { 0 };
                let n = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
                out.push(TABLE[((n >> 18) & 63) as usize] as char);
                out.push(TABLE[((n >> 12) & 63) as usize] as char);
                if i + 1 < bytes.len() {
                    out.push(TABLE[((n >> 6) & 63) as usize] as char);
                }
                if i + 2 < bytes.len() {
                    out.push(TABLE[(n & 63) as usize] as char);
                }
                i += 3;
            }
            out
        }

        let mk_spec = |token: String| {
            vec![RuntimeMcpServerSpec {
                name: String::from("dennett-memory"),
                command: String::from("dennett-memory-mcp"),
                args: vec![],
                env: vec![RuntimeMcpEnvEntry {
                    name: String::from("DENNETT_MEMORY_TOKEN"),
                    value: token,
                }],
            }]
        };

        let tok_a1 = format!(
            "dnt1.{}.sig1",
            b64url(
                r#"{"token_id":"t1","principal_id":"head","allowed_scopes":["project:a"],"capabilities":["read","write"],"expires_at":2000000000000}"#
            )
        );
        let tok_a2 = format!(
            "dnt1.{}.sig2",
            b64url(
                r#"{"token_id":"t2","principal_id":"head","allowed_scopes":["project:a"],"capabilities":["write","read"],"expires_at":2000000099999}"#
            )
        );
        let tok_b = format!(
            "dnt1.{}.sig3",
            b64url(
                r#"{"token_id":"t3","principal_id":"head","allowed_scopes":["project:b"],"capabilities":["read","write"],"expires_at":2000000000000}"#
            )
        );

        let (d_a1, exp_a1) = compute_mcp_fingerprint(&mk_spec(tok_a1));
        let (d_a2, exp_a2) = compute_mcp_fingerprint(&mk_spec(tok_a2));
        let (d_b, _) = compute_mcp_fingerprint(&mk_spec(tok_b));
        let (d_empty, exp_empty) = compute_mcp_fingerprint(&[]);

        assert_eq!(
            d_a1, d_a2,
            "same scopes with refreshed token_id/exp keep digest stable"
        );
        assert_eq!(exp_a1, Some(2_000_000_000_000));
        assert_eq!(exp_a2, Some(2_000_000_099_999));
        assert_ne!(d_a1, d_b, "scope change MUST change MCP digest");
        assert_ne!(d_a1, d_empty, "removing MCP server MUST change digest");
        assert_eq!(exp_empty, None);
    }
}
