//! Local loopback HTTP runtime adapter for self-hosted model servers
//! (Ollama, LM Studio, and `llama.cpp`).
//!
//! Architectural boundaries (Contract M, Architecture 82 §§1–3):
//! - Implements [`AgentRuntimePort`] with `is_simulated: false` and `auth_profile: LocalEndpoint`.
//! - Enforces loopback-by-default SSRF guard (explicit `allow_non_loopback` opt-in required for
//!   non-loopback hosts), optional Bearer auth from `SecretBroker`, graceful fallback from
//!   `/v1/models` to Ollama `/api/tags`, tool-calling with `PolicyEvaluator` enforcement and
//!   graceful 400/422 plain-text fallback, live cancellation, explicit `ProviderQuotaExceeded`
//!   (429) and `AuthRequired` (401/403) classification, and zero silent provider switching.
//! - ZERO SQL drivers, ZERO UI frameworks, ZERO `dennett-memory-core`, and ZERO deferred
//!   provider crates.

use core::fmt;
use std::collections::HashMap;
use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::Duration;

use dennett_contracts::redact::{REDACTED_PLACEHOLDER, redact_secrets_owned};
use dennett_contracts::{
    DennettError, PolicyEffect, RunStatus, RuntimeAuthProfile, RuntimeCatalogResponse,
    RuntimeModelOption, SessionId, ThinkingLevel, TimestampMs, WorkspaceWritePolicy,
};
use dennett_runtime_antigravity::{
    ActiveSkillPayload, AgentRuntimePort, BoxFuture, PolicyEvaluationRequest, PolicyEvaluator,
    RuntimeDescriptor, RuntimeEvent, RuntimeMcpServerSpec, SessionStartOutcome,
    SessionStartRequest, TurnOutcome, TurnRequest, build_effective_system_prompt_with_skills,
    classify_provider_quota_error, evaluate_binding_resume_compatibility, normalize_workspace_path,
};
use reqwest::StatusCode;
use serde_json::{Value, json};
use tokio::process::Command;
use tokio::sync::{Mutex, mpsc, watch};
use tokio::time;
use uuid::Uuid;

/// Canonical runtime adapter identifier for local HTTP model servers (`local-http-runtime`).
pub const LOCAL_HTTP_ADAPTER_ID: &str = "local-http-runtime";

/// Canonical authentication profile used by [`LOCAL_HTTP_ADAPTER_ID`] (`local-endpoint`).
pub const LOCAL_HTTP_AUTH_PROFILE: RuntimeAuthProfile = RuntimeAuthProfile::LocalEndpoint;

/// Default local model server URL (Ollama default loopback address).
pub const DEFAULT_LOCAL_ENDPOINT_URL: &str = "http://127.0.0.1:11434";

/// Maximum bounded tool-calling rounds executed within a single prompt turn.
const MAX_TOOL_ROUNDS: usize = 8;
/// Default HTTP probe timeout for `/v1/models` and `/api/tags`.
const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(10);
/// Default HTTP prompt turn timeout.
const DEFAULT_TURN_TIMEOUT: Duration = Duration::from_secs(120);
/// Maximum bytes read from a single file by `read_file` tool execution.
const MAX_TOOL_FILE_READ_BYTES: usize = 256 * 1024;
/// Maximum diagnostic output bytes returned from a single tool call to the model.
const MAX_TOOL_OUTPUT_BYTES: usize = 32 * 1024;

/// Validated, normalized local HTTP endpoint metadata with deterministic V29 fingerprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedLocalEndpoint {
    /// Normalized server root URL without trailing `/` or trailing `/v1`.
    pub origin_base: String,
    /// Stable canonical endpoint fingerprint (`scheme://host:port[path]`) for V29 binding checks.
    pub fingerprint: String,
    /// Whether the target host resolves to a loopback address (`localhost`, `127.0.0.1`, `::1`, etc.).
    pub is_loopback: bool,
}

/// Validates a local HTTP runtime URL, enforcing `http://`/`https://`, rejecting embedded
/// credentials/whitespace/control characters, and enforcing loopback-by-default SSRF protection
/// unless `allow_non_loopback` is explicitly set.
///
/// # Errors
///
/// Returns [`DennettError::InvalidInput`] on malformed URLs or
/// [`DennettError::SafetyGuardRejected`] when targeting a non-loopback host without opt-in.
pub fn validate_local_endpoint_url(
    raw: &str,
    allow_non_loopback: bool,
) -> Result<ValidatedLocalEndpoint, DennettError> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.as_bytes().contains(&0)
        || trimmed
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
    {
        return Err(DennettError::InvalidInput {
            field: String::from("endpoint_url"),
            reason: String::from("must be a non-empty URL without whitespace or control bytes"),
        });
    }

    let (scheme, default_port, rest) = if let Some(r) = trimmed.strip_prefix("http://") {
        ("http", 80_u16, r)
    } else if let Some(r) = trimmed.strip_prefix("https://") {
        ("https", 443_u16, r)
    } else {
        return Err(DennettError::InvalidInput {
            field: String::from("endpoint_url"),
            reason: String::from("must start with `http://` or `https://`"),
        });
    };

    if rest.is_empty() {
        return Err(DennettError::InvalidInput {
            field: String::from("endpoint_url"),
            reason: String::from("missing host in endpoint URL"),
        });
    }

    let (authority, raw_path) = match rest.find(['/', '?', '#']) {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, ""),
    };
    if raw_path.contains('?') || raw_path.contains('#') {
        return Err(DennettError::InvalidInput {
            field: String::from("endpoint_url"),
            reason: String::from("endpoint URL must not contain query strings or fragments"),
        });
    }
    if authority.is_empty() || authority.contains('@') {
        return Err(DennettError::InvalidInput {
            field: String::from("endpoint_url"),
            reason: String::from(
                "endpoint URL authority must be non-empty and must not embed userinfo credentials",
            ),
        });
    }

    let (host, port) = parse_authority_host_port(authority, default_port)?;
    let lower_host = host.to_ascii_lowercase();
    let is_loopback = is_loopback_host(&lower_host);
    if !is_loopback && !allow_non_loopback {
        return Err(DennettError::SafetyGuardRejected {
            reason: format!(
                "local runtime endpoint host `{lower_host}` is not loopback; set explicit allow_non_loopback opt-in to connect to remote hosts"
            ),
        });
    }

    let clean_path = raw_path.trim_end_matches('/');
    let stripped_path = clean_path.strip_suffix("/v1").unwrap_or(clean_path);

    let host_formatted = if lower_host.contains(':') {
        format!("[{lower_host}]")
    } else {
        lower_host.clone()
    };
    let origin_base = if port == default_port {
        format!("{scheme}://{host_formatted}{stripped_path}")
    } else {
        format!("{scheme}://{host_formatted}:{port}{stripped_path}")
    };
    let fingerprint = format!("{scheme}://{host_formatted}:{port}{stripped_path}");

    Ok(ValidatedLocalEndpoint {
        origin_base,
        fingerprint,
        is_loopback,
    })
}

fn parse_authority_host_port(
    authority: &str,
    default_port: u16,
) -> Result<(String, u16), DennettError> {
    if let Some(rest) = authority.strip_prefix('[') {
        let Some((ipv6_host, tail)) = rest.split_once(']') else {
            return Err(DennettError::InvalidInput {
                field: String::from("endpoint_url"),
                reason: String::from("unterminated IPv6 host bracket"),
            });
        };
        if ipv6_host.is_empty() || ipv6_host.parse::<IpAddr>().is_err() {
            return Err(DennettError::InvalidInput {
                field: String::from("endpoint_url"),
                reason: format!("invalid IPv6 host `{ipv6_host}`"),
            });
        }
        let port = if tail.is_empty() {
            default_port
        } else if let Some(port_str) = tail.strip_prefix(':') {
            parse_port(port_str)?
        } else {
            return Err(DennettError::InvalidInput {
                field: String::from("endpoint_url"),
                reason: String::from("unexpected characters after IPv6 host"),
            });
        };
        return Ok((ipv6_host.to_owned(), port));
    }

    if let Some((host, port_str)) = authority.rsplit_once(':') {
        if host.is_empty() || host.contains(':') {
            return Err(DennettError::InvalidInput {
                field: String::from("endpoint_url"),
                reason: format!("invalid host authority `{authority}`"),
            });
        }
        let port = parse_port(port_str)?;
        validate_hostname(host)?;
        Ok((host.to_owned(), port))
    } else {
        validate_hostname(authority)?;
        Ok((authority.to_owned(), default_port))
    }
}

fn parse_port(port_str: &str) -> Result<u16, DennettError> {
    let port = port_str
        .parse::<u16>()
        .map_err(|_| DennettError::InvalidInput {
            field: String::from("endpoint_url"),
            reason: format!("invalid TCP port `{port_str}`"),
        })?;
    if port == 0 {
        return Err(DennettError::InvalidInput {
            field: String::from("endpoint_url"),
            reason: String::from("TCP port must be in 1..=65535"),
        });
    }
    Ok(port)
}

fn validate_hostname(host: &str) -> Result<(), DennettError> {
    if host.is_empty()
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
    {
        return Err(DennettError::InvalidInput {
            field: String::from("endpoint_url"),
            reason: format!("invalid endpoint host `{host}`"),
        });
    }
    Ok(())
}

fn is_loopback_host(lower_host: &str) -> bool {
    if lower_host == "localhost" || lower_host.ends_with(".localhost") {
        return true;
    }
    lower_host
        .parse::<IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

/// Configuration for [`LocalHttpRuntime`].
///
/// Custom [`fmt::Debug`] redacts `bearer_token` so secrets never enter logs.
#[derive(Clone)]
pub struct LocalRuntimeConfig {
    /// Base HTTP URL of the local model server (defaults to `http://127.0.0.1:11434`).
    pub endpoint_url: String,
    /// Must be explicitly `true` to permit connecting to non-loopback hosts.
    pub allow_non_loopback: bool,
    /// Optional Bearer API token injected via `SecretBroker` (never logged).
    pub bearer_token: Option<String>,
    /// Optional default model identifier used when a session specifies no explicit model.
    pub default_model: Option<String>,
    /// HTTP timeout applied to `probe()` / `list_models()` calls.
    pub probe_timeout: Duration,
    /// Default HTTP timeout applied to individual prompt completions.
    pub turn_timeout: Duration,
}

impl Default for LocalRuntimeConfig {
    fn default() -> Self {
        Self {
            endpoint_url: String::from(DEFAULT_LOCAL_ENDPOINT_URL),
            allow_non_loopback: false,
            bearer_token: None,
            default_model: None,
            probe_timeout: DEFAULT_PROBE_TIMEOUT,
            turn_timeout: DEFAULT_TURN_TIMEOUT,
        }
    }
}

impl fmt::Debug for LocalRuntimeConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalRuntimeConfig")
            .field("endpoint_url", &self.endpoint_url)
            .field("allow_non_loopback", &self.allow_non_loopback)
            .field(
                "bearer_token",
                &self.bearer_token.as_ref().map(|_| REDACTED_PLACEHOLDER),
            )
            .field("default_model", &self.default_model)
            .field("probe_timeout", &self.probe_timeout)
            .field("turn_timeout", &self.turn_timeout)
            .finish()
    }
}

#[derive(Debug, Clone)]
struct LocalSessionState {
    provider_session_id: String,
    workspace_cwd: String,
    system_prompt: String,
    model_preference: Option<String>,
    thinking_level: Option<ThinkingLevel>,
    write_policy: WorkspaceWritePolicy,
    mcp_servers: Vec<RuntimeMcpServerSpec>,
    active_skills: Vec<ActiveSkillPayload>,
    messages: Vec<Value>,
}

/// Production [`AgentRuntimePort`] targeting local HTTP model servers (Ollama, LM Studio, `llama.cpp`).
pub struct LocalHttpRuntime {
    config: LocalRuntimeConfig,
    endpoint: ValidatedLocalEndpoint,
    client: reqwest::Client,
    bearer_token: StdRwLock<Option<String>>,
    cached_default_model: Mutex<Option<String>>,
    sessions: Mutex<HashMap<SessionId, LocalSessionState>>,
    active_cancels: Mutex<HashMap<SessionId, watch::Sender<bool>>>,
}

impl LocalHttpRuntime {
    /// Creates a new [`LocalHttpRuntime`] after validating `config.endpoint_url` against
    /// loopback-by-default SSRF rules.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::SafetyGuardRejected`], or
    /// [`DennettError::Internal`] if HTTP client initialization fails.
    pub fn new(config: LocalRuntimeConfig) -> Result<Self, DennettError> {
        let endpoint =
            validate_local_endpoint_url(&config.endpoint_url, config.allow_non_loopback)?;
        let mut builder = reqwest::Client::builder();
        if endpoint.is_loopback {
            builder = builder.no_proxy();
        }
        let client = builder.build().map_err(|err| DennettError::Internal {
            detail: format!("failed initializing local runtime HTTP client: {err}"),
        })?;
        let token = config.bearer_token.clone();
        Ok(Self {
            config,
            endpoint,
            client,
            bearer_token: StdRwLock::new(token),
            cached_default_model: Mutex::new(None),
            sessions: Mutex::new(HashMap::new()),
            active_cancels: Mutex::new(HashMap::new()),
        })
    }

    /// Returns the normalized endpoint fingerprint (`scheme://host:port[path]`) bound to this runtime.
    #[must_use]
    pub fn endpoint_fingerprint(&self) -> &str {
        &self.endpoint.fingerprint
    }

    /// Returns the normalized origin base URL (without trailing `/` or `/v1`).
    #[must_use]
    pub fn origin_base(&self) -> &str {
        &self.endpoint.origin_base
    }

    /// Hot-updates or clears the Bearer token supplied by `SecretBroker` without restarting Head.
    pub fn set_bearer_token(&self, token: Option<String>) {
        if let Ok(mut guard) = self.bearer_token.write() {
            *guard = token.filter(|t| !t.trim().is_empty());
        }
    }

    /// Closes and clears every in-memory session held by this runtime (e.g. after credential rotation).
    pub async fn close_all_sessions(&self) {
        let cancels: Vec<watch::Sender<bool>> = {
            let mut guard = self.active_cancels.lock().await;
            guard.drain().map(|(_, tx)| tx).collect()
        };
        for tx in cancels {
            let _ = tx.send(true);
        }
        self.sessions.lock().await.clear();
    }

    /// Probes the local server and returns a [`RuntimeCatalogResponse`] directly consumable by Head/UI.
    ///
    /// # Errors
    ///
    /// Propagates [`DennettError::AuthRequired`], [`DennettError::ProviderQuotaExceeded`],
    /// [`DennettError::Timeout`], or [`DennettError::RuntimeProtocol`].
    pub async fn probe_catalog(&self) -> Result<RuntimeCatalogResponse, DennettError> {
        let desc = self.probe().await?;
        Ok(RuntimeCatalogResponse {
            adapter_id: desc.adapter_id,
            auth_profile: desc.auth_profile,
            is_simulated: false,
            agy_version: None,
            tier: desc.tier,
            account_label: None,
            models: desc.models,
            thinking_options: desc.thinking_options,
            default_model_id: desc.default_model_id,
            default_thinking_level: desc.default_thinking_level,
            quotas: Vec::new(),
            connection_id: None,
            quota_error: None,
            probed_at: TimestampMs::now(),
        })
    }

    /// Queries `GET {base}/v1/models` with graceful fallback to Ollama `GET {base}/api/tags`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::AuthRequired`] on 401/403, [`DennettError::ProviderQuotaExceeded`]
    /// on 429/quota, [`DennettError::Timeout`], or [`DennettError::RuntimeProtocol`].
    pub async fn list_models(&self) -> Result<Vec<RuntimeModelOption>, DennettError> {
        let token = self.current_bearer_token();
        let secrets = token.iter().cloned().collect::<Vec<_>>();
        let v1_url = format!("{}/v1/models", self.endpoint.origin_base);

        let v1_attempt = self
            .fetch_json_get(&v1_url, token.as_deref(), &secrets)
            .await;
        match v1_attempt {
            Ok(body) => {
                let models = parse_v1_models(&body);
                if !models.is_empty() {
                    return Ok(models);
                }
                // Try Ollama `/api/tags` if `/v1/models` returned an empty or unexpected payload.
                let tags_url = format!("{}/api/tags", self.endpoint.origin_base);
                if let Ok(tags_body) = self
                    .fetch_json_get(&tags_url, token.as_deref(), &secrets)
                    .await
                {
                    let ollama_models = parse_ollama_tags(&tags_body);
                    if !ollama_models.is_empty() {
                        return Ok(ollama_models);
                    }
                }
                Ok(models)
            }
            Err(
                err @ (DennettError::AuthRequired { .. }
                | DennettError::ProviderQuotaExceeded { .. }),
            ) => Err(err),
            Err(primary_err) => {
                // Graceful fallback to Ollama `GET {base}/api/tags` if `/v1/models` returned 404/non-2xx.
                let tags_url = format!("{}/api/tags", self.endpoint.origin_base);
                match self
                    .fetch_json_get(&tags_url, token.as_deref(), &secrets)
                    .await
                {
                    Ok(tags_body) => Ok(parse_ollama_tags(&tags_body)),
                    Err(
                        err @ (DennettError::AuthRequired { .. }
                        | DennettError::ProviderQuotaExceeded { .. }),
                    ) => Err(err),
                    Err(_) => Err(primary_err),
                }
            }
        }
    }

    fn current_bearer_token(&self) -> Option<String> {
        self.bearer_token
            .read()
            .ok()
            .and_then(|guard| guard.clone())
            .filter(|t| !t.trim().is_empty())
    }

    async fn fetch_json_get(
        &self,
        url: &str,
        bearer_token: Option<&str>,
        secrets: &[String],
    ) -> Result<Value, DennettError> {
        let mut req = self.client.get(url);
        if let Some(tok) = bearer_token {
            req = req.bearer_auth(tok);
        }
        let resp = time::timeout(self.config.probe_timeout, req.send())
            .await
            .map_err(|_| DennettError::Timeout {
                operation: format!("GET {url}"),
            })?
            .map_err(|err| DennettError::RuntimeProtocol {
                code: None,
                detail: redact_secrets_owned(
                    &format!(
                        "local runtime endpoint `{}` unreachable: {err}",
                        self.endpoint.origin_base
                    ),
                    secrets,
                ),
            })?;

        let status = resp.status();
        let retry_after = parse_retry_after_header(resp.headers());
        let body_text = time::timeout(self.config.probe_timeout, resp.text())
            .await
            .map_err(|_| DennettError::Timeout {
                operation: format!("read {url}"),
            })?
            .map_err(|err| DennettError::RuntimeProtocol {
                code: Some(i64::from(status.as_u16())),
                detail: redact_secrets_owned(
                    &format!("failed reading local runtime response: {err}"),
                    secrets,
                ),
            })?;

        if !status.is_success() {
            return Err(classify_http_error(
                status,
                &body_text,
                retry_after,
                secrets,
            ));
        }

        serde_json::from_str(&body_text).map_err(|err| DennettError::RuntimeProtocol {
            code: Some(i64::from(status.as_u16())),
            detail: redact_secrets_owned(
                &format!("local runtime `{url}` returned invalid JSON: {err}"),
                secrets,
            ),
        })
    }

    async fn resolve_model_for_turn(
        &self,
        preferred: Option<&str>,
    ) -> Result<String, DennettError> {
        if let Some(pref) = preferred.map(str::trim).filter(|s| !s.is_empty()) {
            return Ok(pref.to_owned());
        }
        if let Some(ref configured) = self.config.default_model
            && !configured.trim().is_empty()
        {
            return Ok(configured.trim().to_owned());
        }
        {
            let guard = self.cached_default_model.lock().await;
            if let Some(ref cached) = *guard {
                return Ok(cached.clone());
            }
        }
        let models = self.list_models().await?;
        let chosen = models
            .first()
            .map(|m| m.model_id.clone())
            .ok_or_else(|| DennettError::RuntimeProtocol {
                code: None,
                detail: format!(
                    "local runtime `{}` advertised zero models; load a model or configure default_model",
                    self.endpoint.origin_base
                ),
            })?;
        *self.cached_default_model.lock().await = Some(chosen.clone());
        Ok(chosen)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "sends one chat completion HTTP request with cancellation, deadline, Bearer auth, secret redaction, and optional tools payload"
    )]
    async fn post_chat_completion(
        &self,
        model: &str,
        messages: &[Value],
        include_tools: bool,
        bearer_token: Option<&str>,
        secrets: &[String],
        timeout: Duration,
        cancel_rx: &mut watch::Receiver<bool>,
    ) -> Result<ChatCompletionHttpOutcome, DennettError> {
        let url = format!("{}/v1/chat/completions", self.endpoint.origin_base);
        let mut payload = json!({
            "model": model,
            "messages": messages,
            "stream": false,
        });
        if include_tools && let Some(obj) = payload.as_object_mut() {
            obj.insert(String::from("tools"), local_tool_definitions());
        }

        let mut req = self.client.post(&url).json(&payload);
        if let Some(tok) = bearer_token {
            req = req.bearer_auth(tok);
        }

        let send_fut = async {
            let resp = req
                .send()
                .await
                .map_err(|err| DennettError::RuntimeProtocol {
                    code: None,
                    detail: redact_secrets_owned(
                        &format!(
                            "local runtime `{}` request failed: {err}",
                            self.endpoint.origin_base
                        ),
                        secrets,
                    ),
                })?;
            let status = resp.status();
            let retry_after = parse_retry_after_header(resp.headers());
            let body = resp
                .text()
                .await
                .map_err(|err| DennettError::RuntimeProtocol {
                    code: Some(i64::from(status.as_u16())),
                    detail: redact_secrets_owned(
                        &format!("local runtime failed reading response body: {err}"),
                        secrets,
                    ),
                })?;
            Ok::<(StatusCode, Option<u64>, String), DennettError>((status, retry_after, body))
        };

        let mut cancel_open = true;
        let deadline = time::sleep(timeout);
        tokio::pin!(deadline);
        tokio::pin!(send_fut);

        let (status, retry_after, body) = loop {
            if *cancel_rx.borrow() {
                return Ok(ChatCompletionHttpOutcome::Cancelled);
            }
            tokio::select! {
                biased;
                changed = cancel_rx.changed(), if cancel_open => {
                    if changed.is_ok() {
                        if *cancel_rx.borrow() {
                            return Ok(ChatCompletionHttpOutcome::Cancelled);
                        }
                    } else {
                        cancel_open = false;
                    }
                }
                () = &mut deadline => {
                    return Err(DennettError::Timeout {
                        operation: String::from("local_http_chat_completions"),
                    });
                }
                res = &mut send_fut => break res?,
            }
        };

        if include_tools
            && matches!(
                status,
                StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY
            )
        {
            return Ok(ChatCompletionHttpOutcome::ToolsRejected {
                status,
                body: redact_secrets_owned(&body, secrets),
            });
        }
        if !status.is_success() {
            return Err(classify_http_error(status, &body, retry_after, secrets));
        }

        let parsed = parse_chat_completion_body(&body, secrets)?;
        Ok(ChatCompletionHttpOutcome::Completed(parsed))
    }
}

enum ChatCompletionHttpOutcome {
    Cancelled,
    ToolsRejected { status: StatusCode, body: String },
    Completed(ParsedChatCompletion),
}

#[derive(Debug, Clone)]
struct ParsedToolCall {
    id: String,
    name: String,
    arguments_raw: String,
    arguments_json: Value,
}

#[derive(Debug, Clone)]
struct ParsedChatCompletion {
    content: String,
    tool_calls: Vec<ParsedToolCall>,
    finish_reason: String,
    usage: Option<Value>,
}

impl AgentRuntimePort for LocalHttpRuntime {
    fn adapter_id(&self) -> &str {
        LOCAL_HTTP_ADAPTER_ID
    }

    fn is_simulated(&self) -> bool {
        false
    }

    fn auth_profile(&self) -> RuntimeAuthProfile {
        LOCAL_HTTP_AUTH_PROFILE
    }

    fn probe(&self) -> BoxFuture<'_, Result<RuntimeDescriptor, DennettError>> {
        Box::pin(async move {
            let models = self.list_models().await?;
            let available_models: Vec<String> = models.iter().map(|m| m.model_id.clone()).collect();
            let default_model_id = self
                .config
                .default_model
                .clone()
                .filter(|m| !m.trim().is_empty())
                .or_else(|| available_models.first().cloned());

            if let Some(ref def) = default_model_id {
                *self.cached_default_model.lock().await = Some(def.clone());
            }

            Ok(RuntimeDescriptor {
                adapter_id: String::from(LOCAL_HTTP_ADAPTER_ID),
                auth_profile: LOCAL_HTTP_AUTH_PROFILE,
                is_simulated: false,
                agy_version: None,
                tier: Some(format!("Local HTTP ({})", self.endpoint.fingerprint)),
                protocol_version: Some(1),
                supports_resume: false,
                supports_system_prompt_append: true,
                available_models,
                models,
                thinking_options: Vec::new(),
                default_model_id,
                default_thinking_level: None,
                quotas: Vec::new(),
                capabilities: vec![
                    String::from("local_http_v1"),
                    String::from("ollama_tags_fallback"),
                    String::from("tool_calling_fallback"),
                    String::from("external_permissions"),
                    String::from("live_cancel"),
                ],
            })
        })
    }

    #[expect(
        clippy::too_many_lines,
        reason = "handles V29 resume compatibility check, slot refresh, and explicit rehydration cleanly in one flow"
    )]
    fn start_or_resume_session(
        &self,
        req: SessionStartRequest,
    ) -> BoxFuture<'_, Result<SessionStartOutcome, DennettError>> {
        Box::pin(async move {
            let target_cwd = normalize_workspace_path(&req.workspace_cwd);
            let mut guard = self.sessions.lock().await;
            let mut rehydrated = false;
            let mut rehydrate_reason: Option<String> = None;

            if let Some(ref binding) = req.existing_binding {
                if let Err(msg) = evaluate_binding_resume_compatibility(
                    binding,
                    LOCAL_HTTP_ADAPTER_ID,
                    false,
                    LOCAL_HTTP_AUTH_PROFILE,
                    &target_cwd,
                    None,
                    Some(&self.endpoint.fingerprint),
                ) {
                    if !req.allow_rehydrate {
                        return Err(DennettError::Conflict {
                            resource: String::from("runtime_binding"),
                            expected_revision: None,
                            actual_revision: None,
                            reason: msg,
                        });
                    }
                    rehydrated = true;
                    rehydrate_reason = Some(msg);
                } else if let Some(ref bound_prov_id) = binding.provider_session_id {
                    if let Some(existing) = guard.get_mut(&req.dennett_session_id)
                        && existing.provider_session_id == *bound_prov_id
                    {
                        let skills_for_prompt =
                            if req.system_prompt.contains("[Dennett Active Skills Index]") {
                                &[][..]
                            } else {
                                req.active_skills.as_slice()
                            };
                        let effective_system = build_effective_system_prompt_with_skills(
                            &req.system_prompt,
                            skills_for_prompt,
                            &[],
                            None,
                        );
                        existing.system_prompt.clone_from(&effective_system);
                        existing.model_preference = req.model_preference;
                        existing.thinking_level = req.thinking_level;
                        existing.write_policy = req.workspace_write_policy;
                        existing.mcp_servers = req.mcp_servers;
                        existing.active_skills = req.active_skills;
                        if let Some(first_msg) = existing.messages.first_mut()
                            && first_msg.get("role").and_then(Value::as_str) == Some("system")
                        {
                            *first_msg = json!({
                                "role": "system",
                                "content": effective_system,
                            });
                        }
                        return Ok(SessionStartOutcome {
                            adapter_id: String::from(LOCAL_HTTP_ADAPTER_ID),
                            auth_profile: LOCAL_HTTP_AUTH_PROFILE,
                            tier: Some(format!("Local HTTP ({})", self.endpoint.fingerprint)),
                            agy_version: None,
                            provider_session_id: bound_prov_id.clone(),
                            workspace_cwd: target_cwd,
                            is_simulated: false,
                            rehydrated: false,
                            rehydrate_reason: None,
                            available_models: Vec::new(),
                            endpoint_fingerprint: Some(self.endpoint.fingerprint.clone()),
                        });
                    }

                    let msg = format!(
                        "stateless local HTTP session `{bound_prov_id}` is not active in memory; rehydrating from Dennett history"
                    );
                    if !req.allow_rehydrate {
                        return Err(DennettError::RuntimeProtocol {
                            code: Some(-32002),
                            detail: msg,
                        });
                    }
                    rehydrated = true;
                    rehydrate_reason = Some(msg);
                }
            }

            let provider_session_id = format!("local-sess-{}", Uuid::new_v4().simple());
            let skills_for_prompt = if req.system_prompt.contains("[Dennett Active Skills Index]") {
                &[][..]
            } else {
                req.active_skills.as_slice()
            };
            let effective_system = build_effective_system_prompt_with_skills(
                &req.system_prompt,
                skills_for_prompt,
                &req.history_for_rehydrate,
                rehydrate_reason.as_deref(),
            );
            let mut messages = vec![json!({
                "role": "system",
                "content": effective_system,
            })];
            if rehydrated {
                for m in &req.history_for_rehydrate {
                    let role = match m.role {
                        dennett_contracts::MessageRole::Assistant => "assistant",
                        dennett_contracts::MessageRole::System => "system",
                        _ => "user",
                    };
                    messages.push(json!({
                        "role": role,
                        "content": m.content,
                    }));
                }
            }

            guard.insert(
                req.dennett_session_id,
                LocalSessionState {
                    provider_session_id: provider_session_id.clone(),
                    workspace_cwd: target_cwd.clone(),
                    system_prompt: effective_system,
                    model_preference: req.model_preference,
                    thinking_level: req.thinking_level,
                    write_policy: req.workspace_write_policy,
                    mcp_servers: req.mcp_servers,
                    active_skills: req.active_skills,
                    messages,
                },
            );

            Ok(SessionStartOutcome {
                adapter_id: String::from(LOCAL_HTTP_ADAPTER_ID),
                auth_profile: LOCAL_HTTP_AUTH_PROFILE,
                tier: Some(format!("Local HTTP ({})", self.endpoint.fingerprint)),
                agy_version: None,
                provider_session_id,
                workspace_cwd: target_cwd,
                is_simulated: false,
                rehydrated,
                rehydrate_reason,
                available_models: Vec::new(),
                endpoint_fingerprint: Some(self.endpoint.fingerprint.clone()),
            })
        })
    }

    #[expect(
        clippy::too_many_lines,
        reason = "coordinates live cancellation, bounded tool-calling loop with PolicyEvaluator, 400/422 plain-text fallback, and usage tracking"
    )]
    fn send_prompt_turn(
        &self,
        req: TurnRequest,
        policy: Arc<dyn PolicyEvaluator>,
        event_tx: Option<mpsc::Sender<RuntimeEvent>>,
    ) -> BoxFuture<'_, Result<TurnOutcome, DennettError>> {
        Box::pin(async move {
            let snapshot = {
                let guard = self.sessions.lock().await;
                guard.get(&req.dennett_session_id).cloned()
            }
            .ok_or_else(|| DennettError::NotFound {
                resource: String::from("runtime_session"),
                id: req.dennett_session_id.to_string(),
            })?;

            let mut events = Vec::new();
            let mut human_wait_ms: u64 = 0;

            if *req.cancel_rx.borrow() {
                emit(
                    &mut events,
                    RuntimeEvent::Cancelled {
                        reason: String::from("turn cancelled before dispatch"),
                    },
                    event_tx.as_ref(),
                );
                return Ok(TurnOutcome {
                    status: RunStatus::Cancelled,
                    stop_reason: String::from("cancelled"),
                    assistant_text: String::new(),
                    events,
                    usage: None,
                    human_wait_ms,
                });
            }

            let (combined_cancel_tx, mut combined_cancel_rx) =
                watch::channel(*req.cancel_rx.borrow());
            {
                let mut cancels = self.active_cancels.lock().await;
                cancels.insert(req.dennett_session_id, combined_cancel_tx.clone());
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

            let turn_result = async {
                let model = self
                    .resolve_model_for_turn(snapshot.model_preference.as_deref())
                    .await?;
                let bearer_token = self.current_bearer_token();
                let mut secrets = Vec::new();
                if let Some(ref tok) = bearer_token {
                    secrets.push(tok.clone());
                }
                for srv in &snapshot.mcp_servers {
                    for env_item in &srv.env {
                        let upper = env_item.name.to_ascii_uppercase();
                        if (upper.contains("TOKEN")
                            || upper.contains("SECRET")
                            || upper.contains("KEY")
                            || upper.contains("PASSWORD"))
                            && !env_item.value.is_empty()
                        {
                            secrets.push(env_item.value.clone());
                        }
                    }
                }

                let mut messages = snapshot.messages.clone();
                messages.push(json!({
                    "role": "user",
                    "content": req.prompt_text,
                }));

                let effective_timeout = if req.timeout.is_zero() {
                    self.config.turn_timeout
                } else {
                    req.timeout
                };
                let mut deadline = time::Instant::now() + effective_timeout;
                let mut allow_tools = true;
                let mut last_usage: Option<Value> = None;
                let mut final_text = String::new();
                let mut final_stop_reason = String::from("end_turn");

                for round in 0..MAX_TOOL_ROUNDS {
                    let remaining = deadline.saturating_duration_since(time::Instant::now());
                    if remaining.is_zero() {
                        return Err(DennettError::Timeout {
                            operation: String::from("local_http_chat_completions"),
                        });
                    }

                    let http_outcome = self
                        .post_chat_completion(
                            &model,
                            &messages,
                            allow_tools,
                            bearer_token.as_deref(),
                            &secrets,
                            remaining,
                            &mut combined_cancel_rx,
                        )
                        .await?;

                    let completion = match http_outcome {
                        ChatCompletionHttpOutcome::Cancelled => {
                            emit(
                                &mut events,
                                RuntimeEvent::Cancelled {
                                    reason: String::from("cancelled"),
                                },
                                event_tx.as_ref(),
                            );
                            return Ok(TurnOutcome {
                                status: RunStatus::Cancelled,
                                stop_reason: String::from("cancelled"),
                                assistant_text: final_text,
                                events,
                                usage: last_usage,
                                human_wait_ms,
                            });
                        }
                        ChatCompletionHttpOutcome::ToolsRejected { .. } if allow_tools => {
                            // Graceful fallback to plain-text mode when local model server rejects `tools`.
                            allow_tools = false;
                            let retry_remaining =
                                deadline.saturating_duration_since(time::Instant::now());
                            if retry_remaining.is_zero() {
                                return Err(DennettError::Timeout {
                                    operation: String::from("local_http_chat_completions"),
                                });
                            }
                            match self
                                .post_chat_completion(
                                    &model,
                                    &messages,
                                    false,
                                    bearer_token.as_deref(),
                                    &secrets,
                                    retry_remaining,
                                    &mut combined_cancel_rx,
                                )
                                .await?
                            {
                                ChatCompletionHttpOutcome::Cancelled => {
                                    emit(
                                        &mut events,
                                        RuntimeEvent::Cancelled {
                                            reason: String::from("cancelled"),
                                        },
                                        event_tx.as_ref(),
                                    );
                                    return Ok(TurnOutcome {
                                        status: RunStatus::Cancelled,
                                        stop_reason: String::from("cancelled"),
                                        assistant_text: final_text,
                                        events,
                                        usage: last_usage,
                                        human_wait_ms,
                                    });
                                }
                                ChatCompletionHttpOutcome::Completed(c) => c,
                                ChatCompletionHttpOutcome::ToolsRejected { status, body } => {
                                    return Err(DennettError::RuntimeProtocol {
                                        code: Some(i64::from(status.as_u16())),
                                        detail: body,
                                    });
                                }
                            }
                        }
                        ChatCompletionHttpOutcome::ToolsRejected { status, body } => {
                            return Err(DennettError::RuntimeProtocol {
                                code: Some(i64::from(status.as_u16())),
                                detail: body,
                            });
                        }
                        ChatCompletionHttpOutcome::Completed(c) => c,
                    };

                    if let Some(usage) = completion.usage {
                        let prompt_tokens = usage
                            .get("prompt_tokens")
                            .or_else(|| usage.get("input_tokens"))
                            .and_then(Value::as_u64);
                        let window_tokens = usage
                            .get("context_window_tokens")
                            .or_else(|| usage.get("size"))
                            .and_then(Value::as_u64);
                        if prompt_tokens.is_some() || window_tokens.is_some() {
                            emit(
                                &mut events,
                                RuntimeEvent::ContextUsageUpdated {
                                    used_tokens: prompt_tokens,
                                    window_tokens,
                                },
                                event_tx.as_ref(),
                            );
                        }
                        emit(
                            &mut events,
                            RuntimeEvent::UsageReported {
                                usage: usage.clone(),
                            },
                            event_tx.as_ref(),
                        );
                        last_usage = Some(usage);
                    }

                    if completion.tool_calls.is_empty() || round + 1 == MAX_TOOL_ROUNDS {
                        let safe_round_text = redact_secrets_owned(&completion.content, &secrets);
                        final_stop_reason = completion.finish_reason;
                        if !safe_round_text.is_empty() {
                            if !final_text.is_empty()
                                && !final_text.ends_with('\n')
                                && !safe_round_text.starts_with('\n')
                            {
                                final_text.push_str("\n\n");
                            }
                            final_text.push_str(&safe_round_text);
                            emit(
                                &mut events,
                                RuntimeEvent::TextDelta {
                                    text: safe_round_text,
                                },
                                event_tx.as_ref(),
                            );
                        }
                        messages.push(json!({
                            "role": "assistant",
                            "content": final_text,
                        }));
                        break;
                    }

                    if !completion.content.is_empty() {
                        let safe_preface = redact_secrets_owned(&completion.content, &secrets);
                        if !final_text.is_empty()
                            && !final_text.ends_with('\n')
                            && !safe_preface.starts_with('\n')
                        {
                            final_text.push_str("\n\n");
                        }
                        final_text.push_str(&safe_preface);
                        emit(
                            &mut events,
                            RuntimeEvent::TextDelta { text: safe_preface },
                            event_tx.as_ref(),
                        );
                    }

                    // Record assistant tool_calls message for multi-turn conversation continuity.
                    let wire_tool_calls: Vec<Value> = completion
                        .tool_calls
                        .iter()
                        .map(|tc| {
                            json!({
                                "id": tc.id,
                                "type": "function",
                                "function": {
                                    "name": tc.name,
                                    "arguments": tc.arguments_raw,
                                }
                            })
                        })
                        .collect();
                    messages.push(json!({
                        "role": "assistant",
                        "content": if completion.content.is_empty() { Value::Null } else { Value::String(completion.content.clone()) },
                        "tool_calls": wire_tool_calls,
                    }));

                    for tc in completion.tool_calls {
                        let (tool_kind, target, locations) =
                            classify_local_tool_call(&tc.name, &tc.arguments_json);
                        let title = match &target {
                            Some(t) => format!("{} {t}", tc.name),
                            None => tc.name.clone(),
                        };
                        let eval_req = PolicyEvaluationRequest {
                            session_id: req.dennett_session_id,
                            run_id: req.run_id,
                            tool_call_id: tc.id.clone(),
                            tool_name: tc.name.clone(),
                            tool_kind: tool_kind.to_owned(),
                            title: title.clone(),
                            target: target.clone(),
                            locations,
                            raw_input: tc.arguments_json.clone(),
                        };

                        emit(
                            &mut events,
                            RuntimeEvent::PermissionRequested {
                                tool_call_id: tc.id.clone(),
                                tool_name: tc.name.clone(),
                                target: target.clone(),
                                title: title.clone(),
                            },
                            event_tx.as_ref(),
                        );

                        let wait_started = time::Instant::now();
                        let maybe_decision = {
                            let mut eval_fut = policy.evaluate(&eval_req);
                            let mut cancel_open = true;
                            loop {
                                tokio::select! {
                                    biased;
                                    res = &mut eval_fut => break Some(res),
                                    changed = combined_cancel_rx.changed(), if cancel_open => {
                                        if changed.is_ok() {
                                            if *combined_cancel_rx.borrow() {
                                                break None;
                                            }
                                        } else {
                                            cancel_open = false;
                                        }
                                    }
                                }
                            }
                        };
                        let wait_elapsed = wait_started.elapsed();
                        deadline += wait_elapsed;
                        human_wait_ms = human_wait_ms.saturating_add(
                            u64::try_from(wait_elapsed.as_millis()).unwrap_or(u64::MAX),
                        );

                        let Some(decision) = maybe_decision else {
                            emit(
                                &mut events,
                                RuntimeEvent::PermissionResolved {
                                    tool_call_id: tc.id,
                                    tool_name: tc.name,
                                    target,
                                    effect: PolicyEffect::Deny,
                                    matched_rule_id: None,
                                    reason: String::from(
                                        "abandoned because run was cancelled during approval wait",
                                    ),
                                    scope: format!("session:{}", req.dennett_session_id),
                                    chosen_option_id: None,
                                },
                                event_tx.as_ref(),
                            );
                            emit(
                                &mut events,
                                RuntimeEvent::Cancelled {
                                    reason: String::from("cancelled"),
                                },
                                event_tx.as_ref(),
                            );
                            return Ok(TurnOutcome {
                                status: RunStatus::Cancelled,
                                stop_reason: String::from("cancelled"),
                                assistant_text: final_text,
                                events,
                                usage: last_usage,
                                human_wait_ms,
                            });
                        };

                        if *combined_cancel_rx.borrow() {
                            emit(
                                &mut events,
                                RuntimeEvent::PermissionResolved {
                                    tool_call_id: tc.id,
                                    tool_name: tc.name,
                                    target,
                                    effect: PolicyEffect::Deny,
                                    matched_rule_id: decision.matched_rule_id,
                                    reason: String::from(
                                        "late approval rejected after cancellation",
                                    ),
                                    scope: decision.scope,
                                    chosen_option_id: None,
                                },
                                event_tx.as_ref(),
                            );
                            emit(
                                &mut events,
                                RuntimeEvent::Cancelled {
                                    reason: String::from("cancelled"),
                                },
                                event_tx.as_ref(),
                            );
                            return Ok(TurnOutcome {
                                status: RunStatus::Cancelled,
                                stop_reason: String::from("cancelled"),
                                assistant_text: final_text,
                                events,
                                usage: last_usage,
                                human_wait_ms,
                            });
                        }

                        emit(
                            &mut events,
                            RuntimeEvent::PermissionResolved {
                                tool_call_id: tc.id.clone(),
                                tool_name: tc.name.clone(),
                                target: target.clone(),
                                effect: decision.effect,
                                matched_rule_id: decision.matched_rule_id,
                                reason: redact_secrets_owned(&decision.reason, &secrets),
                                scope: decision.scope.clone(),
                                chosen_option_id: Some(if decision.effect == PolicyEffect::Allow {
                                    String::from("allow_once")
                                } else {
                                    String::from("reject_once")
                                }),
                            },
                            event_tx.as_ref(),
                        );

                        let (tool_status, tool_output) = if decision.effect == PolicyEffect::Allow {
                            match execute_local_tool(
                                &tc.name,
                                &tc.arguments_json,
                                Path::new(&snapshot.workspace_cwd),
                                &secrets,
                            )
                            .await
                            {
                                Ok(out) => (String::from("completed"), out),
                                Err(err) => (String::from("failed"), format!("error: {err}")),
                            }
                        } else {
                            (
                                String::from("denied"),
                                format!("DENIED by Dennett policy: {}", decision.reason),
                            )
                        };

                        let safe_output = redact_secrets_owned(&tool_output, &secrets);
                        emit(
                            &mut events,
                            RuntimeEvent::ToolObserved {
                                tool_call_id: tc.id.clone(),
                                tool_name: tc.name.clone(),
                                title: redact_secrets_owned(&title, &secrets),
                                status: tool_status,
                                output_excerpt: truncate_utf8(&safe_output, 1024),
                            },
                            event_tx.as_ref(),
                        );

                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": tc.id,
                            "content": safe_output,
                        }));
                    }
                }

                emit(
                    &mut events,
                    RuntimeEvent::Completed {
                        stop_reason: final_stop_reason.clone(),
                    },
                    event_tx.as_ref(),
                );

                // Persist updated visible conversation messages in session state.
                {
                    let mut guard = self.sessions.lock().await;
                    if let Some(state) = guard.get_mut(&req.dennett_session_id) {
                        state.messages = messages;
                    }
                }

                Ok(TurnOutcome {
                    status: RunStatus::Succeeded,
                    stop_reason: final_stop_reason,
                    assistant_text: final_text,
                    events,
                    usage: last_usage,
                    human_wait_ms,
                })
            }
            .await;

            forwarder.abort();
            {
                let mut cancels = self.active_cancels.lock().await;
                cancels.remove(&req.dennett_session_id);
            }

            turn_result
        })
    }

    fn cancel_turn(
        &self,
        dennett_session_id: SessionId,
    ) -> BoxFuture<'_, Result<bool, DennettError>> {
        Box::pin(async move {
            let cancels = self.active_cancels.lock().await;
            if let Some(tx) = cancels.get(&dennett_session_id) {
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
            if let Some(tx) = self.active_cancels.lock().await.remove(&dennett_session_id) {
                let _ = tx.send(true);
            }
            self.sessions.lock().await.remove(&dennett_session_id);
            Ok(())
        })
    }
}

fn emit(events: &mut Vec<RuntimeEvent>, ev: RuntimeEvent, tx: Option<&mpsc::Sender<RuntimeEvent>>) {
    if let Some(sink) = tx {
        let _ = sink.try_send(ev.clone());
    }
    events.push(ev);
}

fn parse_retry_after_header(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
}

fn classify_http_error(
    status: StatusCode,
    raw_body: &str,
    retry_after: Option<u64>,
    secrets: &[String],
) -> DennettError {
    let safe_body = redact_secrets_owned(raw_body.trim(), secrets);
    let excerpt = truncate_utf8(&safe_body, 512);
    let detail = if excerpt.is_empty() {
        format!("HTTP {status}")
    } else {
        format!("HTTP {status}: {excerpt}")
    };

    if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
        return DennettError::AuthRequired {
            profile: LOCAL_HTTP_AUTH_PROFILE,
            detail,
        };
    }

    if let Some(quota_err) = classify_provider_quota_error(
        LOCAL_HTTP_ADAPTER_ID,
        Some(i64::from(status.as_u16())),
        &detail,
        retry_after,
    ) {
        return quota_err;
    }

    DennettError::RuntimeProtocol {
        code: Some(i64::from(status.as_u16())),
        detail,
    }
}

fn parse_v1_models(body: &Value) -> Vec<RuntimeModelOption> {
    let mut out = Vec::new();
    let arr = body
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| body.get("models").and_then(Value::as_array));
    let Some(items) = arr else {
        return out;
    };
    for item in items {
        let Some(id) = item
            .get("id")
            .or_else(|| item.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        if out.iter().any(|m: &RuntimeModelOption| m.model_id == id) {
            continue;
        }
        let label = item
            .get("name")
            .or_else(|| item.get("display_name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(id)
            .to_owned();
        out.push(RuntimeModelOption {
            model_id: id.to_owned(),
            label,
            supported_thinking_levels: Vec::new(),
            default_thinking_level: None,
        });
    }
    out
}

fn parse_ollama_tags(body: &Value) -> Vec<RuntimeModelOption> {
    let mut out = Vec::new();
    let Some(items) = body.get("models").and_then(Value::as_array) else {
        return out;
    };
    for item in items {
        let Some(name) = item
            .get("name")
            .or_else(|| item.get("model"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        if out.iter().any(|m: &RuntimeModelOption| m.model_id == name) {
            continue;
        }
        out.push(RuntimeModelOption {
            model_id: name.to_owned(),
            label: name.to_owned(),
            supported_thinking_levels: Vec::new(),
            default_thinking_level: None,
        });
    }
    out
}

fn parse_chat_completion_body(
    raw_body: &str,
    secrets: &[String],
) -> Result<ParsedChatCompletion, DennettError> {
    let trimmed = raw_body.trim();
    if trimmed.starts_with("data:") {
        return parse_sse_chat_completion(trimmed, secrets);
    }
    let val: Value =
        serde_json::from_str(trimmed).map_err(|err| DennettError::RuntimeProtocol {
            code: None,
            detail: redact_secrets_owned(
                &format!("local runtime returned invalid chat completion JSON: {err}"),
                secrets,
            ),
        })?;

    if let Some(err_obj) = val.get("error") {
        let msg = err_obj
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("local runtime returned error payload");
        let safe = redact_secrets_owned(msg, secrets);
        if let Some(quota_err) =
            classify_provider_quota_error(LOCAL_HTTP_ADAPTER_ID, None, &safe, None)
        {
            return Err(quota_err);
        }
        return Err(DennettError::RuntimeProtocol {
            code: err_obj.get("code").and_then(Value::as_i64),
            detail: safe,
        });
    }

    let choice = val
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .ok_or_else(|| DennettError::RuntimeProtocol {
            code: None,
            detail: String::from("local runtime chat completion missing `choices[0]`"),
        })?;
    let message = choice.get("message").unwrap_or(&Value::Null);
    let content = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let finish_reason = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .unwrap_or("end_turn")
        .to_owned();
    let tool_calls = parse_tool_calls_array(message.get("tool_calls"));
    let usage = val.get("usage").cloned();

    Ok(ParsedChatCompletion {
        content,
        tool_calls,
        finish_reason,
        usage,
    })
}

fn parse_sse_chat_completion(
    raw_sse: &str,
    secrets: &[String],
) -> Result<ParsedChatCompletion, DennettError> {
    let mut content = String::new();
    let mut finish_reason = String::from("end_turn");
    let mut usage: Option<Value> = None;

    for line in raw_sse.lines() {
        let Some(data) = line.trim().strip_prefix("data:") else {
            continue;
        };
        let payload = data.trim();
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }
        let chunk: Value =
            serde_json::from_str(payload).map_err(|err| DennettError::RuntimeProtocol {
                code: None,
                detail: redact_secrets_owned(
                    &format!("malformed SSE chunk from local runtime: {err}"),
                    secrets,
                ),
            })?;
        if let Some(u) = chunk.get("usage")
            && !u.is_null()
        {
            usage = Some(u.clone());
        }
        if let Some(choice) = chunk
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
        {
            if let Some(delta_text) = choice
                .get("delta")
                .and_then(|d| d.get("content"))
                .and_then(Value::as_str)
            {
                content.push_str(delta_text);
            }
            if let Some(fr) = choice.get("finish_reason").and_then(Value::as_str) {
                fr.clone_into(&mut finish_reason);
            }
        }
    }

    Ok(ParsedChatCompletion {
        content,
        tool_calls: Vec::new(),
        finish_reason,
        usage,
    })
}

fn parse_tool_calls_array(val: Option<&Value>) -> Vec<ParsedToolCall> {
    let Some(arr) = val.and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (idx, item) in arr.iter().enumerate() {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .map_or_else(|| format!("local-tc-{idx}"), str::to_owned);
        let func = item.get("function").unwrap_or(item);
        let Some(name) = func
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let (arguments_raw, arguments_json) = match func.get("arguments") {
            Some(Value::String(s)) => {
                let parsed =
                    serde_json::from_str::<Value>(s).unwrap_or_else(|_| json!({ "raw": s }));
                (s.clone(), parsed)
            }
            Some(other) => (other.to_string(), other.clone()),
            None => (String::from("{}"), json!({})),
        };
        out.push(ParsedToolCall {
            id,
            name: name.to_owned(),
            arguments_raw,
            arguments_json,
        });
    }
    out
}

fn local_tool_definitions() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read a UTF-8 file inside the project workspace.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Relative or absolute path to read." }
                    },
                    "required": ["path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "write_file",
                "description": "Write UTF-8 text content to a file inside the project workspace.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Relative or absolute file path." },
                        "content": { "type": "string", "description": "UTF-8 file content to write." }
                    },
                    "required": ["path", "content"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_dir",
                "description": "List entries in a workspace directory.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Directory path (defaults to workspace root)." }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "run_command",
                "description": "Execute a shell command inside the workspace directory (requires policy approval).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "Command line to execute." }
                    },
                    "required": ["command"]
                }
            }
        }
    ])
}

fn classify_local_tool_call(
    tool_name: &str,
    args: &Value,
) -> (&'static str, Option<String>, Vec<String>) {
    let path_arg = args
        .get("path")
        .or_else(|| args.get("Path"))
        .or_else(|| args.get("file_path"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let cmd_arg = args
        .get("command")
        .or_else(|| args.get("CommandLine"))
        .or_else(|| args.get("cmd"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);

    match tool_name {
        "write_file" | "edit_file" => {
            let locs = path_arg.iter().cloned().collect();
            ("edit", path_arg, locs)
        }
        "read_file" | "list_dir" => {
            let locs = path_arg.iter().cloned().collect();
            ("read", path_arg, locs)
        }
        "run_command" => ("execute", cmd_arg, Vec::new()),
        _ => ("other", path_arg.or(cmd_arg), Vec::new()),
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "executes all bounded local workspace tools after PolicyEvaluator approval"
)]
async fn execute_local_tool(
    tool_name: &str,
    args: &Value,
    workspace_cwd: &Path,
    secrets: &[String],
) -> Result<String, DennettError> {
    match tool_name {
        "read_file" => {
            let raw_path = args
                .get("path")
                .or_else(|| args.get("Path"))
                .and_then(Value::as_str)
                .ok_or_else(|| DennettError::InvalidInput {
                    field: String::from("path"),
                    reason: String::from("read_file requires string `path`"),
                })?;
            let resolved = resolve_tool_path(workspace_cwd, raw_path);
            let bytes = fs::read(&resolved).map_err(|err| DennettError::Internal {
                detail: format!("failed reading `{}`: {err}", resolved.display()),
            })?;
            let slice = &bytes[..bytes.len().min(MAX_TOOL_FILE_READ_BYTES)];
            let text = String::from_utf8_lossy(slice);
            Ok(truncate_utf8(&text, MAX_TOOL_OUTPUT_BYTES))
        }
        "write_file" => {
            let raw_path = args
                .get("path")
                .or_else(|| args.get("Path"))
                .and_then(Value::as_str)
                .ok_or_else(|| DennettError::InvalidInput {
                    field: String::from("path"),
                    reason: String::from("write_file requires string `path`"),
                })?;
            let content = args
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let resolved = resolve_tool_path(workspace_cwd, raw_path);
            if let Some(parent) = resolved.parent() {
                fs::create_dir_all(parent).map_err(|err| DennettError::Internal {
                    detail: format!("failed creating parent dir `{}`: {err}", parent.display()),
                })?;
            }
            fs::write(&resolved, content.as_bytes()).map_err(|err| DennettError::Internal {
                detail: format!("failed writing `{}`: {err}", resolved.display()),
            })?;
            Ok(format!("wrote {} bytes to {}", content.len(), raw_path))
        }
        "list_dir" => {
            let raw_path = args
                .get("path")
                .or_else(|| args.get("Path"))
                .and_then(Value::as_str)
                .unwrap_or(".");
            let resolved = resolve_tool_path(workspace_cwd, raw_path);
            let mut entries = Vec::new();
            for ent in fs::read_dir(&resolved)
                .map_err(|err| DennettError::Internal {
                    detail: format!("failed reading dir `{}`: {err}", resolved.display()),
                })?
                .flatten()
            {
                entries.push(ent.file_name().to_string_lossy().into_owned());
            }
            entries.sort();
            Ok(truncate_utf8(&entries.join("\n"), MAX_TOOL_OUTPUT_BYTES))
        }
        "run_command" => {
            let command = args
                .get("command")
                .or_else(|| args.get("CommandLine"))
                .and_then(Value::as_str)
                .ok_or_else(|| DennettError::InvalidInput {
                    field: String::from("command"),
                    reason: String::from("run_command requires string `command`"),
                })?;
            let mut cmd = if cfg!(windows) {
                let mut c = Command::new("cmd");
                c.arg("/C").arg(command);
                c
            } else {
                let mut c = Command::new("sh");
                c.arg("-c").arg(command);
                c
            };
            cmd.current_dir(workspace_cwd)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            let output = time::timeout(Duration::from_secs(30), cmd.output())
                .await
                .map_err(|_| DennettError::Timeout {
                    operation: String::from("local_runtime.run_command"),
                })?
                .map_err(|err| DennettError::Internal {
                    detail: redact_secrets_owned(
                        &format!("command execution failed: {err}"),
                        secrets,
                    ),
                })?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let combined = format!(
                "exit={}\nstdout:\n{}\nstderr:\n{}",
                output.status, stdout, stderr
            );
            Ok(truncate_utf8(
                &redact_secrets_owned(&combined, secrets),
                MAX_TOOL_OUTPUT_BYTES,
            ))
        }
        other => Err(DennettError::InvalidInput {
            field: String::from("tool_name"),
            reason: format!("unsupported local runtime tool `{other}`"),
        }),
    }
}

fn resolve_tool_path(workspace_cwd: &Path, raw: &str) -> PathBuf {
    let p = PathBuf::from(raw);
    if p.is_absolute() {
        p
    } else {
        workspace_cwd.join(p)
    }
}

fn truncate_utf8(input: &str, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input.to_owned();
    }
    let mut cut = max_bytes;
    while cut > 0 && !input.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}...[truncated]", &input[..cut])
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<LocalRuntimeConfig>();
    assert_send_sync::<LocalHttpRuntime>();
};

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use dennett_contracts::RunId;
    use dennett_runtime_antigravity::{ConformancePrompts, run_runtime_conformance_suite};
    use tokio::net::TcpListener;

    use super::*;

    #[derive(Clone)]
    struct MockServerState {
        required_bearer: Option<String>,
        v1_models_404: Arc<AtomicBool>,
        reject_tools_400: Arc<AtomicBool>,
        force_429: Arc<AtomicBool>,
        chat_requests: Arc<AtomicUsize>,
    }

    async fn spawn_mock_local_server(
        state: MockServerState,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let app = Router::new()
            .route("/v1/models", get(mock_v1_models))
            .route("/api/tags", get(mock_ollama_tags))
            .route("/v1/chat/completions", post(mock_chat_completions))
            .with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback mock server");
        let addr = listener.local_addr().expect("local addr");
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://{addr}"), handle)
    }

    fn check_auth(headers: &HeaderMap, required: Option<&str>) -> bool {
        let Some(expected) = required else {
            return true;
        };
        headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|h| h == format!("Bearer {expected}"))
    }

    async fn mock_v1_models(
        State(state): State<MockServerState>,
        headers: HeaderMap,
    ) -> (StatusCode, Json<Value>) {
        if !check_auth(&headers, state.required_bearer.as_deref()) {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": { "message": "invalid bearer token" } })),
            );
        }
        if state.v1_models_404.load(Ordering::SeqCst) {
            return (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" })));
        }
        (
            StatusCode::OK,
            Json(json!({
                "object": "list",
                "data": [
                    { "id": "qwen2.5-coder:7b", "object": "model" },
                    { "id": "llama3.2:3b", "object": "model" }
                ]
            })),
        )
    }

    async fn mock_ollama_tags(
        State(state): State<MockServerState>,
        headers: HeaderMap,
    ) -> (StatusCode, Json<Value>) {
        if !check_auth(&headers, state.required_bearer.as_deref()) {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "unauthorized" })),
            );
        }
        (
            StatusCode::OK,
            Json(json!({
                "models": [
                    { "name": "ollama-fallback:8b" }
                ]
            })),
        )
    }

    #[expect(
        clippy::too_many_lines,
        reason = "mock chat completions endpoint simulates normal, tool-call, cancel, 401, and 429 paths for conformance suite"
    )]
    async fn mock_chat_completions(
        State(state): State<MockServerState>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> (StatusCode, HeaderMap, Json<Value>) {
        state.chat_requests.fetch_add(1, Ordering::SeqCst);
        let mut resp_headers = HeaderMap::new();

        if !check_auth(&headers, state.required_bearer.as_deref()) {
            return (
                StatusCode::UNAUTHORIZED,
                resp_headers,
                Json(json!({ "error": { "message": "missing or invalid token" } })),
            );
        }

        if state.force_429.load(Ordering::SeqCst) {
            resp_headers.insert("retry-after", "17".parse().expect("header"));
            return (
                StatusCode::TOO_MANY_REQUESTS,
                resp_headers,
                Json(json!({ "error": { "message": "rate limit exceeded on local gateway" } })),
            );
        }

        if state.reject_tools_400.load(Ordering::SeqCst) && body.get("tools").is_some() {
            return (
                StatusCode::BAD_REQUEST,
                resp_headers,
                Json(json!({ "error": { "message": "model does not support tools" } })),
            );
        }

        let model = body
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let messages = body
            .get("messages")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let last_msg = messages.last().cloned().unwrap_or(Value::Null);
        let last_role = last_msg.get("role").and_then(Value::as_str).unwrap_or("");
        let last_content = last_msg
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or("");

        // If this is the follow-up turn after a tool execution result, summarize it.
        if last_role == "tool" {
            return (
                StatusCode::OK,
                resp_headers,
                Json(json!({
                    "id": "cmpl-tool-done",
                    "choices": [{
                        "index": 0,
                        "finish_reason": "stop",
                        "message": {
                            "role": "assistant",
                            "content": format!("[local:{model}] tool_result={last_content}")
                        }
                    }],
                    "usage": { "prompt_tokens": 12, "completion_tokens": 8, "total_tokens": 20 }
                })),
            );
        }

        if last_content.contains("SLOW_CANCEL") {
            tokio::time::sleep(Duration::from_secs(5)).await;
        }

        if last_content.contains("TRIGGER_TOOL_READ") {
            return (
                StatusCode::OK,
                resp_headers,
                Json(json!({
                    "id": "cmpl-tc-1",
                    "choices": [{
                        "index": 0,
                        "finish_reason": "tool_calls",
                        "message": {
                            "role": "assistant",
                            "content": null,
                            "tool_calls": [{
                                "id": "call-read-1",
                                "type": "function",
                                "function": {
                                    "name": "read_file",
                                    "arguments": "{\"path\":\"hello.txt\"}"
                                }
                            }]
                        }
                    }],
                    "usage": { "total_tokens": 10 }
                })),
            );
        }

        if last_content.contains("TRIGGER_TOOL_DENY") {
            return (
                StatusCode::OK,
                resp_headers,
                Json(json!({
                    "id": "cmpl-tc-2",
                    "choices": [{
                        "index": 0,
                        "finish_reason": "tool_calls",
                        "message": {
                            "role": "assistant",
                            "content": null,
                            "tool_calls": [{
                                "id": "call-cmd-1",
                                "type": "function",
                                "function": {
                                    "name": "run_command",
                                    "arguments": "{\"command\":\"echo forbidden\"}"
                                }
                            }]
                        }
                    }],
                    "usage": { "total_tokens": 10 }
                })),
            );
        }

        (
            StatusCode::OK,
            resp_headers,
            Json(json!({
                "id": "cmpl-text",
                "choices": [{
                    "index": 0,
                    "finish_reason": "stop",
                    "message": {
                        "role": "assistant",
                        "content": format!("[local:{model}] ack: {last_content}")
                    }
                }],
                "usage": { "prompt_tokens": 5, "completion_tokens": 7, "total_tokens": 12 }
            })),
        )
    }

    #[test]
    fn local_runtime_constants_and_loopback_ssrf_guard() {
        assert_eq!(LOCAL_HTTP_ADAPTER_ID, "local-http-runtime");
        assert_eq!(LOCAL_HTTP_AUTH_PROFILE.as_method_id(), "local-endpoint");

        let ok_1 = validate_local_endpoint_url("http://127.0.0.1:11434/v1/", false)
            .expect("loopback IPv4");
        assert_eq!(ok_1.origin_base, "http://127.0.0.1:11434");
        assert_eq!(ok_1.fingerprint, "http://127.0.0.1:11434");
        assert!(ok_1.is_loopback);

        let ok_2 = validate_local_endpoint_url("http://localhost:1234", false).expect("localhost");
        assert!(ok_2.is_loopback);

        let ok_v6 = validate_local_endpoint_url("http://[::1]:8080", false).expect("IPv6 loopback");
        assert!(ok_v6.is_loopback);

        // Non-loopback MUST be rejected by default (SSRF guard).
        let remote_blocked = validate_local_endpoint_url("http://192.168.1.50:11434", false);
        assert!(matches!(
            remote_blocked,
            Err(DennettError::SafetyGuardRejected { .. })
        ));

        // Explicit opt-in allows non-loopback host.
        let remote_ok = validate_local_endpoint_url("https://gpu-box.lan:8443/v1", true)
            .expect("explicit non-loopback opt-in");
        assert!(!remote_ok.is_loopback);
        assert_eq!(remote_ok.origin_base, "https://gpu-box.lan:8443");

        // Embedded credentials must be rejected.
        assert!(matches!(
            validate_local_endpoint_url("http://user:pass@127.0.0.1:11434", false),
            Err(DennettError::InvalidInput { .. })
        ));
    }

    #[tokio::test]
    async fn local_http_runtime_passes_shared_conformance_suite_against_real_axum_server() {
        let ws = std::env::temp_dir().join(format!(
            "dennett-local-conformance-{}",
            Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&ws).expect("create ws");
        fs::write(ws.join("hello.txt"), "hello from workspace file").expect("write hello.txt");

        let state = MockServerState {
            required_bearer: Some(String::from("local-secret-tok-123")),
            v1_models_404: Arc::new(AtomicBool::new(false)),
            reject_tools_400: Arc::new(AtomicBool::new(false)),
            force_429: Arc::new(AtomicBool::new(false)),
            chat_requests: Arc::new(AtomicUsize::new(0)),
        };
        let (base_url, handle) = spawn_mock_local_server(state).await;

        let runtime = LocalHttpRuntime::new(LocalRuntimeConfig {
            endpoint_url: base_url,
            allow_non_loopback: false,
            bearer_token: Some(String::from("local-secret-tok-123")),
            default_model: Some(String::from("qwen2.5-coder:7b")),
            probe_timeout: Duration::from_secs(5),
            turn_timeout: Duration::from_secs(10),
        })
        .expect("build LocalHttpRuntime");

        let prompts = ConformancePrompts {
            basic_prompt: String::from("Say hello"),
            tool_allow_prompt: String::from("TRIGGER_TOOL_READ"),
            tool_deny_prompt: String::from("TRIGGER_TOOL_DENY"),
            slow_cancel_prompt: String::from("SLOW_CANCEL"),
            alt_model: Some(String::from("llama3.2:3b")),
        };

        let res = run_runtime_conformance_suite(&runtime, &ws, &prompts).await;
        handle.abort();
        let _ = fs::remove_dir_all(&ws);
        assert!(
            res.is_ok(),
            "LocalHttpRuntime failed shared conformance suite: {:?}",
            res.err()
        );
    }

    struct DenyAllPolicy;
    impl PolicyEvaluator for DenyAllPolicy {
        fn evaluate<'a>(
            &'a self,
            _req: &'a PolicyEvaluationRequest,
        ) -> BoxFuture<'a, dennett_runtime_antigravity::PolicyEvaluationOutcome> {
            Box::pin(async {
                dennett_runtime_antigravity::PolicyEvaluationOutcome {
                    effect: PolicyEffect::Deny,
                    matched_rule_id: None,
                    reason: String::from("deny"),
                    scope: String::from("test"),
                }
            })
        }
    }

    #[tokio::test]
    async fn local_http_runtime_handles_ollama_fallback_tools_fallback_401_and_429() {
        let ws =
            std::env::temp_dir().join(format!("dennett-local-edges-{}", Uuid::new_v4().simple()));
        fs::create_dir_all(&ws).expect("create ws");

        let state = MockServerState {
            required_bearer: Some(String::from("tok-super-secret-xyz")),
            v1_models_404: Arc::new(AtomicBool::new(true)),
            reject_tools_400: Arc::new(AtomicBool::new(true)),
            force_429: Arc::new(AtomicBool::new(false)),
            chat_requests: Arc::new(AtomicUsize::new(0)),
        };
        let (base_url, handle) = spawn_mock_local_server(state.clone()).await;

        // 1. Missing/wrong bearer token -> AuthRequired(LocalEndpoint), NEVER silent fallback.
        let unauth_rt = LocalHttpRuntime::new(LocalRuntimeConfig {
            endpoint_url: base_url.clone(),
            bearer_token: Some(String::from("wrong-token")),
            ..LocalRuntimeConfig::default()
        })
        .expect("build unauth runtime");
        let auth_err = unauth_rt.probe().await;
        assert!(matches!(
            auth_err,
            Err(DennettError::AuthRequired {
                profile: RuntimeAuthProfile::LocalEndpoint,
                ..
            })
        ));

        // 2. Valid bearer token + `/v1/models` 404 -> graceful fallback to Ollama `/api/tags`.
        unauth_rt.set_bearer_token(Some(String::from("tok-super-secret-xyz")));
        let desc = unauth_rt.probe().await.expect("ollama fallback probe");
        assert_eq!(
            desc.available_models,
            vec![String::from("ollama-fallback:8b")]
        );
        assert!(!desc.is_simulated);

        // 3. Server rejects `tools` with 400 -> graceful fallback to plain-text completion!
        let sid = SessionId::new();
        let start = unauth_rt
            .start_or_resume_session(SessionStartRequest {
                dennett_session_id: sid,
                workspace_cwd: ws.clone(),
                system_prompt: String::from("sys"),
                model_preference: None,
                thinking_level: None,
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                mcp_servers: Vec::new(),
                active_skills: Vec::new(),
                existing_binding: None,
                history_for_rehydrate: Vec::new(),
                allow_rehydrate: true,
            })
            .await
            .expect("start local session");

        let policy: Arc<dyn PolicyEvaluator> = Arc::new(DenyAllPolicy);
        let (_tx, rx) = watch::channel(false);
        let turn = unauth_rt
            .send_prompt_turn(
                TurnRequest {
                    dennett_session_id: sid,
                    provider_session_id: start.provider_session_id.clone(),
                    run_id: RunId::new(),
                    prompt_text: String::from("plain text after tools 400"),
                    timeout: Duration::from_secs(5),
                    cancel_rx: rx,
                },
                Arc::clone(&policy),
                None,
            )
            .await
            .expect("tools fallback turn");
        assert_eq!(turn.status, RunStatus::Succeeded);
        assert!(turn.assistant_text.contains("plain text after tools 400"));

        // 4. HTTP 429 -> ProviderQuotaExceeded with retry_after_secs parsed from Retry-After header!
        state.force_429.store(true, Ordering::SeqCst);
        let (_tx2, rx2) = watch::channel(false);
        let quota_res = unauth_rt
            .send_prompt_turn(
                TurnRequest {
                    dennett_session_id: sid,
                    provider_session_id: start.provider_session_id,
                    run_id: RunId::new(),
                    prompt_text: String::from("trigger 429"),
                    timeout: Duration::from_secs(5),
                    cancel_rx: rx2,
                },
                policy,
                None,
            )
            .await;
        assert!(matches!(
            quota_res,
            Err(DennettError::ProviderQuotaExceeded {
                ref provider,
                retry_after_secs: Some(17),
                ..
            }) if provider == LOCAL_HTTP_ADAPTER_ID
        ));

        handle.abort();
        let _ = fs::remove_dir_all(&ws);
    }
}
