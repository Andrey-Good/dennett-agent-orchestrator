//! Thin stdio JSON-RPC 2.0 MCP adapter for standalone `dennett-memoryd`.
//!
//! Architectural boundary (Architecture 80 §6, 82 §7, V06):
//! - Depends ONLY on `dennett-contracts` and `dennett-memory-client` (NEVER on
//!   `dennett-memory-core` or PostgreSQL drivers).
//! - Delegates every memory operation to `dennett-memoryd` via [`MemoryClient`] using
//!   a scoped bearer token read from environment.
//! - Enforces a strict maximum stdin line size ([`MAX_MCP_STDIN_LINE_BYTES`]) to bound memory.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, BufRead};
use std::str::FromStr;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use dennett_contracts::redact::{REDACTED_PLACEHOLDER, redact_secrets};
use dennett_contracts::{
    AdmissionStatus, ApiErrorResponse, ClaimId, ConflictId, ConflictStatus, CorrectNoteRequest,
    CorrelationId, CreateClaimRequest, CreateDocumentRequest, CreateNoteRequest, DennettError,
    DocumentId, EpistemicStatus, EvidenceLinkInput, ExportRequest, ForgetRequest, ForgetTargetType,
    IdempotencyKey, ImportRequest, IngestEvidenceRequest, MemoryCapability, MemoryEventId,
    MemoryNoteId, MemoryScope, PrincipalId, ResolveConflictRequest, SearchNotesRequest,
    SupersedeClaimRequest, TimestampMs, UpdateDocumentRequest,
};
use dennett_memory_client::{
    DEFAULT_CLIENT_TIMEOUT, DEFAULT_CONNECT_TIMEOUT, MAX_RESPONSE_BODY_BYTES, MemoryClient,
};
use hmac::{Hmac, Mac};
use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// Default MCP protocol version advertised when a client omits `protocolVersion`.
pub const DEFAULT_PROTOCOL_VERSION: &str = "2024-11-05";
/// Maximum permitted byte length of a single JSON-RPC line on stdin (256 KiB).
pub const MAX_MCP_STDIN_LINE_BYTES: usize = 256 * 1024;
/// Default `dennett-memoryd` URL used when `DENNETT_MEMORY_URL` is unset.
pub const DEFAULT_MEMORY_URL: &str = "http://127.0.0.1:18080";
/// Environment variable providing the `dennett-memoryd` base URL.
pub const ENV_MEMORY_URL: &str = "DENNETT_MEMORY_URL";
/// Environment variable providing the scoped bearer credential for `dennett-memoryd`.
pub const ENV_MEMORY_TOKEN: &str = "DENNETT_MEMORY_TOKEN";
/// Environment variable providing the shared HMAC-SHA256 secret for minting `dnt1.` tokens.
pub const ENV_MEMORY_HMAC_SECRET: &str = "DENNETT_MEMORY_HMAC_SECRET";
/// Optional comma-separated scope allowlist used when minting `dnt1.` tokens via HMAC secret.
pub const ENV_MEMORY_SCOPES: &str = "DENNETT_MEMORY_SCOPES";
/// Optional principal identifier used when minting `dnt1.` tokens via HMAC secret.
pub const ENV_MEMORY_PRINCIPAL_ID: &str = "DENNETT_MEMORY_PRINCIPAL_ID";
/// Optional comma-separated capabilities (`read,write,export,forget,admin`) used with HMAC auth.
pub const ENV_MEMORY_CAPABILITIES: &str = "DENNETT_MEMORY_CAPABILITIES";
/// Minimum required byte length for `DENNETT_MEMORY_HMAC_SECRET` / `--memory-hmac-secret`.
pub const MIN_HMAC_SECRET_BYTES: usize = 16;

const DEFAULT_MEMORY_AUDIENCE: &str = "dennett-memoryd";
const DEFAULT_MEMORY_ISSUER: &str = "dennett-standalone";
const SIGNED_TOKEN_PREFIX: &str = "dnt1";
const DEFAULT_TOKEN_TTL_MS: i64 = 2 * 60 * 60 * 1000;
const MAX_EVIDENCE_LIST_LIMIT: u32 = 200;

static HEADER_CORRELATION_ID: HeaderName = HeaderName::from_static("x-correlation-id");

#[derive(Debug, Serialize)]
struct McpSignedTokenClaims {
    token_id: String,
    principal_id: PrincipalId,
    issuer: String,
    audience: String,
    allowed_scopes: BTreeSet<MemoryScope>,
    capabilities: BTreeSet<MemoryCapability>,
    issued_at: TimestampMs,
    expires_at: TimestampMs,
}

/// Cryptographic HMAC-SHA256 signer that mints `dnt1.` bearer tokens for `dennett-memoryd`
/// when `DENNETT_MEMORY_HMAC_SECRET` (or `--memory-hmac-secret`) is configured.
///
/// Custom [`fmt::Debug`] redacts the signing secret so it never enters logs or diagnostics.
#[derive(Clone)]
pub struct HmacTokenMinter {
    secret: Vec<u8>,
    secret_raw: String,
    principal_id: PrincipalId,
    allowed_scopes: BTreeSet<MemoryScope>,
    capabilities: BTreeSet<MemoryCapability>,
    issuer: String,
    audience: String,
}

impl fmt::Debug for HmacTokenMinter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HmacTokenMinter")
            .field("secret", &REDACTED_PLACEHOLDER)
            .field("secret_raw", &REDACTED_PLACEHOLDER)
            .field("principal_id", &self.principal_id)
            .field("allowed_scopes", &self.allowed_scopes)
            .field("capabilities", &self.capabilities)
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .finish()
    }
}

impl HmacTokenMinter {
    /// Constructs a new [`HmacTokenMinter`] from a shared HMAC-SHA256 secret (at least 16 bytes).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `secret` is shorter than [`MIN_HMAC_SECRET_BYTES`].
    pub fn new(
        secret: impl Into<String>,
        allowed_scopes: BTreeSet<MemoryScope>,
        principal_id: Option<PrincipalId>,
        capabilities: Option<BTreeSet<MemoryCapability>>,
    ) -> Result<Self, DennettError> {
        let secret_raw = secret.into();
        if secret_raw.len() < MIN_HMAC_SECRET_BYTES {
            return Err(DennettError::InvalidInput {
                field: String::from("memory_hmac_secret"),
                reason: format!("must be at least {MIN_HMAC_SECRET_BYTES} bytes"),
            });
        }
        let principal = match principal_id {
            Some(p) => p,
            None => PrincipalId::try_new("agent:memory-mcp")?,
        };
        let caps = match capabilities {
            Some(c) if !c.is_empty() => c,
            _ => BTreeSet::from([
                MemoryCapability::Read,
                MemoryCapability::Write,
                MemoryCapability::Export,
                MemoryCapability::Forget,
                MemoryCapability::Admin,
            ]),
        };
        Ok(Self {
            secret: secret_raw.as_bytes().to_vec(),
            secret_raw,
            principal_id: principal,
            allowed_scopes,
            capabilities: caps,
            issuer: String::from(DEFAULT_MEMORY_ISSUER),
            audience: String::from(DEFAULT_MEMORY_AUDIENCE),
        })
    }

    /// Returns the raw secret string strictly for redaction registration.
    #[must_use]
    pub fn secret_for_redaction(&self) -> &str {
        &self.secret_raw
    }

    /// Mints a short-lived `dnt1.` bearer token authorized for configured scopes (or
    /// `requested_scope` when no static allowlist was configured).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::Internal`] on serialization or HMAC signing errors.
    pub fn mint_for_scope(
        &self,
        requested_scope: Option<&MemoryScope>,
    ) -> Result<String, DennettError> {
        let effective_scopes = if !self.allowed_scopes.is_empty() {
            self.allowed_scopes.clone()
        } else if let Some(sc) = requested_scope {
            BTreeSet::from([sc.clone()])
        } else {
            BTreeSet::from([MemoryScope::try_new("default")?])
        };

        let issued_at = TimestampMs::now();
        let expires_at =
            TimestampMs::from_millis(issued_at.as_millis().saturating_add(DEFAULT_TOKEN_TTL_MS));
        let claims = McpSignedTokenClaims {
            token_id: format!("mcp-{}", Uuid::new_v4().simple()),
            principal_id: self.principal_id.clone(),
            issuer: self.issuer.clone(),
            audience: self.audience.clone(),
            allowed_scopes: effective_scopes,
            capabilities: self.capabilities.clone(),
            issued_at,
            expires_at,
        };
        let payload_json = serde_json::to_vec(&claims).map_err(|err| DennettError::Internal {
            detail: format!("failed serializing MCP signed token claims: {err}"),
        })?;
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload_json);
        let signing_input = format!("{SIGNED_TOKEN_PREFIX}.{payload_b64}");

        let mut mac =
            HmacSha256::new_from_slice(&self.secret).map_err(|_| DennettError::Internal {
                detail: String::from("failed initializing HMAC-SHA256 signer"),
            })?;
        mac.update(signing_input.as_bytes());
        let tag = mac.finalize().into_bytes();
        let tag_b64 = URL_SAFE_NO_PAD.encode(tag);
        Ok(format!("{signing_input}.{tag_b64}"))
    }
}

/// Result of reading one bounded newline-delimited frame from an input stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundedRead {
    /// Clean end-of-file before any bytes of a new frame arrived.
    Eof,
    /// Complete valid UTF-8 frame within the configured byte limit.
    Line(String),
    /// Frame exceeded `max_bytes` and was safely drained up to the next newline (or EOF).
    TooLarge,
}

/// Reads one newline-delimited UTF-8 line without allocating beyond `max_bytes`.
///
/// When a line exceeds `max_bytes`, all remaining bytes up to and including `\n` are
/// discarded so subsequent lines remain aligned.
///
/// # Errors
///
/// Returns `io::Error` on underlying I/O failure or invalid UTF-8 bytes within the limit.
pub fn read_bounded_line(reader: &mut impl BufRead, max_bytes: usize) -> io::Result<BoundedRead> {
    let mut raw = Vec::new();
    let mut exceeded = false;

    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if exceeded {
                return Ok(BoundedRead::TooLarge);
            }
            if raw.is_empty() {
                return Ok(BoundedRead::Eof);
            }
            break;
        }

        let newline_pos = available.iter().position(|&b| b == b'\n');
        let take_len = newline_pos.map_or(available.len(), |pos| pos + 1);

        if !exceeded {
            if raw.len() + take_len > max_bytes {
                exceeded = true;
                raw.clear();
            } else {
                raw.extend_from_slice(&available[..take_len]);
            }
        }
        reader.consume(take_len);

        if newline_pos.is_some() {
            if exceeded {
                return Ok(BoundedRead::TooLarge);
            }
            break;
        }
    }

    let text =
        String::from_utf8(raw).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    Ok(BoundedRead::Line(text))
}

/// Parsed and validated MCP tool invocation ready to dispatch via [`MemoryClient`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedToolCall {
    /// Scoped search over current memory notes (`memory_search`).
    Search(SearchNotesRequest),
    /// Fetch current state of a memory note (`memory_get_current`).
    GetCurrent(MemoryNoteId),
    /// Fetch complete immutable revision history of a note (`memory_get_history`).
    GetHistory(MemoryNoteId),
    /// Create a revision-1 memory note (`memory_write_note`).
    WriteNote(CreateNoteRequest),
    /// Append an immutable correction revision to a note (`memory_correct_note`).
    CorrectNote(CorrectNoteRequest),
    /// Create a structured claim (`memory_claim_create`).
    ClaimCreate(CreateClaimRequest),
    /// Lookup claims by scope and/or topic (`memory_claim_lookup`).
    ClaimLookup {
        /// Optional scope filter.
        scope: Option<MemoryScope>,
        /// Optional topic key filter.
        topic_key: Option<String>,
    },
    /// Supersede an existing claim (`memory_claim_supersede`).
    ClaimSupersede(SupersedeClaimRequest),
    /// List open conflicts (`memory_conflict_list`).
    ConflictList(Option<MemoryScope>),
    /// Resolve an open conflict (`memory_conflict_resolve`).
    ConflictResolve(ResolveConflictRequest),
    /// Read a document dossier (or list dossiers when `document_id` omitted) (`memory_document_read`).
    DocumentRead {
        /// Optional document identifier to fetch.
        document_id: Option<DocumentId>,
        /// Optional scope filter when listing documents.
        scope: Option<MemoryScope>,
        /// When `true` and `document_id` is provided, fetch full version history.
        include_versions: bool,
    },
    /// Create a version-1 dossier (`memory_document_write` without `document_id`).
    DocumentCreate(CreateDocumentRequest),
    /// Update an existing dossier (`memory_document_write` with `document_id`).
    DocumentUpdate(UpdateDocumentRequest),
    /// Forget a memory entity with search purge and audit tombstone (`memory_forget`).
    Forget(ForgetRequest),
    /// Export a deterministic, secret-stripped Contract J package (`memory_export`).
    Export(ExportRequest),
    /// Import an external package as `external_untrusted` (`memory_import`).
    Import(ImportRequest),
    /// Query separable search index freshness/health status (`memory_index_status`).
    IndexStatus(Option<MemoryScope>),
    /// Ingest an immutable inline evidence object (`memory_evidence_create`).
    EvidenceCreate(IngestEvidenceRequest),
    /// List active evidence objects across authorized scopes (`memory_evidence_list`).
    EvidenceList {
        /// Optional scope filter.
        scope: Option<MemoryScope>,
        /// Optional maximum items returned (`1..=200`).
        limit: Option<u32>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArgs {
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    include_history: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NoteIdArgs {
    note_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteNoteArgs {
    scope: String,
    title: String,
    body: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    evidence_links: Vec<EvidenceLinkInput>,
    #[serde(default)]
    author_label: Option<String>,
    #[serde(default)]
    admission_status: Option<AdmissionStatus>,
    #[serde(default)]
    salience: Option<u8>,
    #[serde(default)]
    topic_key: Option<String>,
    #[serde(default)]
    idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CorrectNoteArgs {
    note_id: String,
    scope: String,
    expected_revision: i64,
    title: String,
    body: String,
    rationale: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    evidence_links: Vec<EvidenceLinkInput>,
    #[serde(default)]
    author_label: Option<String>,
    #[serde(default)]
    idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClaimCreateArgs {
    scope: String,
    topic_key: String,
    #[serde(default)]
    claim_key: Option<String>,
    natural_language: String,
    #[serde(default)]
    epistemic_status: EpistemicStatus,
    #[serde(default)]
    supersedes_claim_id: Option<String>,
    #[serde(default)]
    valid_from: Option<TimestampMs>,
    #[serde(default)]
    valid_to: Option<TimestampMs>,
    #[serde(default)]
    source_note_id: Option<String>,
    #[serde(default)]
    caveats: Vec<String>,
    #[serde(default)]
    provenance: BTreeMap<String, String>,
    #[serde(default)]
    evidence_links: Vec<EvidenceLinkInput>,
    #[serde(default)]
    idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClaimLookupArgs {
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    topic_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClaimSupersedeArgs {
    claim_id: String,
    scope: String,
    topic_key: String,
    #[serde(default)]
    claim_key: Option<String>,
    natural_language: String,
    #[serde(default)]
    epistemic_status: EpistemicStatus,
    #[serde(default)]
    valid_from: Option<TimestampMs>,
    #[serde(default)]
    valid_to: Option<TimestampMs>,
    #[serde(default)]
    source_note_id: Option<String>,
    #[serde(default)]
    caveats: Vec<String>,
    #[serde(default)]
    provenance: BTreeMap<String, String>,
    #[serde(default)]
    evidence_links: Vec<EvidenceLinkInput>,
    #[serde(default)]
    resolution_event_id: Option<String>,
    #[serde(default)]
    idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OptionalScopeArgs {
    #[serde(default)]
    scope: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConflictResolveArgs {
    conflict_id: String,
    scope: String,
    resolution_note: String,
    #[serde(default)]
    winning_claim_id: Option<String>,
    #[serde(default)]
    target_status: Option<ConflictStatus>,
    #[serde(default)]
    idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DocumentReadArgs {
    #[serde(default)]
    document_id: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    include_versions: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DocumentWriteArgs {
    scope: String,
    #[serde(default)]
    document_id: Option<String>,
    #[serde(default)]
    document_key: Option<String>,
    #[serde(default)]
    expected_version: Option<i32>,
    title: String,
    body: String,
    #[serde(default)]
    provenance_claim_ids: Vec<String>,
    #[serde(default)]
    provenance_note_ids: Vec<String>,
    #[serde(default)]
    diff_summary: Option<String>,
    #[serde(default)]
    idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ForgetArgs {
    scope: String,
    target_type: ForgetTargetType,
    target_id: String,
    reason: String,
    #[serde(default)]
    idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportArgs {
    scope: String,
    #[serde(default)]
    format_version: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportArgs {
    scope: String,
    package_json: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceCreateArgs {
    scope: String,
    content: String,
    #[serde(default = "default_evidence_media_type")]
    media_type: String,
    #[serde(default)]
    source_uri: Option<String>,
    #[serde(default)]
    observed_at: Option<TimestampMs>,
    #[serde(default)]
    metadata: BTreeMap<String, String>,
    #[serde(default)]
    idempotency_key: Option<String>,
}

fn default_evidence_media_type() -> String {
    String::from("text/plain")
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceListArgs {
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
}

/// Parses and validates an MCP tool name + JSON arguments object into a typed [`ParsedToolCall`].
///
/// # Errors
///
/// Returns `Ok(None)` if `tool_name` is unknown (so caller can emit JSON-RPC `-32601`),
/// or `Err(DennettError::InvalidInput)` if arguments fail validation.
#[expect(
    clippy::too_many_lines,
    reason = "single validated match table covering all 18 Memory Fabric MCP tools"
)]
pub fn parse_tool_call(
    tool_name: &str,
    raw_args: &Value,
) -> Result<Option<ParsedToolCall>, DennettError> {
    let allows_null = matches!(
        tool_name,
        "memory_search"
            | "memory_claim_lookup"
            | "memory_conflict_list"
            | "memory_document_read"
            | "memory_index_status"
            | "memory_evidence_list"
    );
    let args_obj = match raw_args {
        Value::Object(_) => raw_args.clone(),
        Value::Null if allows_null => json!({}),
        _ => {
            return Err(DennettError::InvalidInput {
                field: String::from("arguments"),
                reason: String::from("tool arguments must be a JSON object"),
            });
        }
    };

    match tool_name {
        "memory_search" => {
            let parsed: SearchArgs = deserialize_args(args_obj)?;
            let scope = parsed.scope.map(MemoryScope::try_new).transpose()?;
            let req = SearchNotesRequest {
                scope,
                query: parsed.query,
                limit: parsed.limit,
                include_history: parsed.include_history,
            };
            let _ = req.validated_limit()?;
            Ok(Some(ParsedToolCall::Search(req)))
        }
        "memory_get_current" => {
            let parsed: NoteIdArgs = deserialize_args(args_obj)?;
            let note_id = MemoryNoteId::from_str(&parsed.note_id)?;
            Ok(Some(ParsedToolCall::GetCurrent(note_id)))
        }
        "memory_get_history" => {
            let parsed: NoteIdArgs = deserialize_args(args_obj)?;
            let note_id = MemoryNoteId::from_str(&parsed.note_id)?;
            Ok(Some(ParsedToolCall::GetHistory(note_id)))
        }
        "memory_write_note" => {
            let parsed: WriteNoteArgs = deserialize_args(args_obj)?;
            let scope = MemoryScope::try_new(parsed.scope)?;
            let idempotency_key = parse_opt_idempotency(parsed.idempotency_key)?;
            let req = CreateNoteRequest {
                scope,
                title: parsed.title,
                body: parsed.body,
                tags: parsed.tags,
                observed_at: None,
                evidence_links: parsed.evidence_links,
                author_label: parsed.author_label,
                admission_status: parsed.admission_status,
                salience: parsed.salience,
                topic_key: parsed.topic_key,
                idempotency_key,
            };
            req.validate()?;
            Ok(Some(ParsedToolCall::WriteNote(req)))
        }
        "memory_correct_note" => {
            let parsed: CorrectNoteArgs = deserialize_args(args_obj)?;
            let note_id = MemoryNoteId::from_str(&parsed.note_id)?;
            let scope = MemoryScope::try_new(parsed.scope)?;
            let idempotency_key = parse_opt_idempotency(parsed.idempotency_key)?;
            let req = CorrectNoteRequest {
                note_id,
                scope,
                expected_revision: parsed.expected_revision,
                title: parsed.title,
                body: parsed.body,
                tags: parsed.tags,
                rationale: parsed.rationale,
                evidence_links: parsed.evidence_links,
                author_label: parsed.author_label,
                idempotency_key,
            };
            req.validate()?;
            Ok(Some(ParsedToolCall::CorrectNote(req)))
        }
        "memory_claim_create" => {
            let parsed: ClaimCreateArgs = deserialize_args(args_obj)?;
            let scope = MemoryScope::try_new(parsed.scope)?;
            let supersedes_claim_id = parsed
                .supersedes_claim_id
                .as_deref()
                .map(ClaimId::from_str)
                .transpose()?;
            let source_note_id = parsed
                .source_note_id
                .as_deref()
                .map(MemoryNoteId::from_str)
                .transpose()?;
            let idempotency_key = parse_opt_idempotency(parsed.idempotency_key)?;
            let req = CreateClaimRequest {
                scope,
                topic_key: parsed.topic_key,
                claim_key: parsed.claim_key,
                natural_language: parsed.natural_language,
                epistemic_status: parsed.epistemic_status,
                supersedes_claim_id,
                valid_from: parsed.valid_from,
                valid_to: parsed.valid_to,
                source_note_id,
                caveats: parsed.caveats,
                provenance: parsed.provenance,
                evidence_links: parsed.evidence_links,
                idempotency_key,
            };
            req.validate()?;
            Ok(Some(ParsedToolCall::ClaimCreate(req)))
        }
        "memory_claim_lookup" => {
            let parsed: ClaimLookupArgs = deserialize_args(args_obj)?;
            let scope = parsed.scope.map(MemoryScope::try_new).transpose()?;
            Ok(Some(ParsedToolCall::ClaimLookup {
                scope,
                topic_key: parsed.topic_key,
            }))
        }
        "memory_claim_supersede" => {
            let parsed: ClaimSupersedeArgs = deserialize_args(args_obj)?;
            let claim_id = ClaimId::from_str(&parsed.claim_id)?;
            let scope = MemoryScope::try_new(parsed.scope)?;
            let source_note_id = parsed
                .source_note_id
                .as_deref()
                .map(MemoryNoteId::from_str)
                .transpose()?;
            let resolution_event_id = parsed
                .resolution_event_id
                .as_deref()
                .map(MemoryEventId::from_str)
                .transpose()?;
            let idempotency_key = parse_opt_idempotency(parsed.idempotency_key)?;
            let req = SupersedeClaimRequest {
                claim_id,
                scope: scope.clone(),
                new_claim: CreateClaimRequest {
                    scope,
                    topic_key: parsed.topic_key,
                    claim_key: parsed.claim_key,
                    natural_language: parsed.natural_language,
                    epistemic_status: parsed.epistemic_status,
                    supersedes_claim_id: Some(claim_id),
                    valid_from: parsed.valid_from,
                    valid_to: parsed.valid_to,
                    source_note_id,
                    caveats: parsed.caveats,
                    provenance: parsed.provenance,
                    evidence_links: parsed.evidence_links,
                    idempotency_key,
                },
                resolution_event_id,
            };
            req.validate()?;
            Ok(Some(ParsedToolCall::ClaimSupersede(req)))
        }
        "memory_conflict_list" => {
            let parsed: OptionalScopeArgs = deserialize_args(args_obj)?;
            let scope = parsed.scope.map(MemoryScope::try_new).transpose()?;
            Ok(Some(ParsedToolCall::ConflictList(scope)))
        }
        "memory_conflict_resolve" => {
            let parsed: ConflictResolveArgs = deserialize_args(args_obj)?;
            let conflict_id = ConflictId::from_str(&parsed.conflict_id)?;
            let scope = MemoryScope::try_new(parsed.scope)?;
            let winning_claim_id = parsed
                .winning_claim_id
                .as_deref()
                .map(ClaimId::from_str)
                .transpose()?;
            let idempotency_key = parse_opt_idempotency(parsed.idempotency_key)?;
            let req = ResolveConflictRequest {
                conflict_id,
                scope,
                resolution_note: parsed.resolution_note,
                winning_claim_id,
                target_status: parsed.target_status,
                idempotency_key,
            };
            req.validate()?;
            Ok(Some(ParsedToolCall::ConflictResolve(req)))
        }
        "memory_document_read" => {
            let parsed: DocumentReadArgs = deserialize_args(args_obj)?;
            let document_id = parsed
                .document_id
                .as_deref()
                .map(DocumentId::from_str)
                .transpose()?;
            let scope = parsed.scope.map(MemoryScope::try_new).transpose()?;
            Ok(Some(ParsedToolCall::DocumentRead {
                document_id,
                scope,
                include_versions: parsed.include_versions,
            }))
        }
        "memory_document_write" => {
            let parsed: DocumentWriteArgs = deserialize_args(args_obj)?;
            let scope = MemoryScope::try_new(parsed.scope)?;
            let idempotency_key = parse_opt_idempotency(parsed.idempotency_key)?;
            let provenance_claim_ids: Vec<ClaimId> = parsed
                .provenance_claim_ids
                .iter()
                .map(|s| ClaimId::from_str(s))
                .collect::<Result<_, _>>()?;
            let provenance_note_ids: Vec<MemoryNoteId> = parsed
                .provenance_note_ids
                .iter()
                .map(|s| MemoryNoteId::from_str(s))
                .collect::<Result<_, _>>()?;

            if let Some(raw_doc_id) = parsed.document_id {
                let document_id = DocumentId::from_str(&raw_doc_id)?;
                let expected_version =
                    parsed
                        .expected_version
                        .ok_or_else(|| DennettError::InvalidInput {
                            field: String::from("expected_version"),
                            reason: String::from(
                                "expected_version is required when updating an existing document",
                            ),
                        })?;
                let req = UpdateDocumentRequest {
                    document_id,
                    scope,
                    expected_version,
                    title: parsed.title,
                    body: parsed.body,
                    provenance_claim_ids,
                    provenance_note_ids,
                    diff_summary: parsed.diff_summary,
                    idempotency_key,
                };
                req.validate()?;
                Ok(Some(ParsedToolCall::DocumentUpdate(req)))
            } else {
                let document_key =
                    parsed
                        .document_key
                        .ok_or_else(|| DennettError::InvalidInput {
                            field: String::from("document_key"),
                            reason: String::from(
                                "document_key is required when creating a new document",
                            ),
                        })?;
                let req = CreateDocumentRequest {
                    scope,
                    document_key,
                    title: parsed.title,
                    body: parsed.body,
                    provenance_claim_ids,
                    provenance_note_ids,
                    idempotency_key,
                };
                req.validate()?;
                Ok(Some(ParsedToolCall::DocumentCreate(req)))
            }
        }
        "memory_forget" => {
            let parsed: ForgetArgs = deserialize_args(args_obj)?;
            let scope = MemoryScope::try_new(parsed.scope)?;
            let target_id =
                Uuid::parse_str(&parsed.target_id).map_err(|_| DennettError::InvalidInput {
                    field: String::from("target_id"),
                    reason: String::from("must be a valid UUID"),
                })?;
            let idempotency_key = parse_opt_idempotency(parsed.idempotency_key)?;
            let req = ForgetRequest {
                scope,
                target_type: parsed.target_type,
                target_id,
                reason: parsed.reason,
                idempotency_key,
            };
            req.validate()?;
            Ok(Some(ParsedToolCall::Forget(req)))
        }
        "memory_export" => {
            let parsed: ExportArgs = deserialize_args(args_obj)?;
            let scope = MemoryScope::try_new(parsed.scope)?;
            Ok(Some(ParsedToolCall::Export(ExportRequest {
                scope,
                format_version: parsed.format_version,
            })))
        }
        "memory_import" => {
            let parsed: ImportArgs = deserialize_args(args_obj)?;
            let scope = MemoryScope::try_new(parsed.scope)?;
            let req = ImportRequest {
                scope,
                package_json: parsed.package_json,
            };
            req.validate()?;
            Ok(Some(ParsedToolCall::Import(req)))
        }
        "memory_index_status" => {
            let parsed: OptionalScopeArgs = deserialize_args(args_obj)?;
            let scope = parsed.scope.map(MemoryScope::try_new).transpose()?;
            Ok(Some(ParsedToolCall::IndexStatus(scope)))
        }
        "memory_evidence_create" => {
            let parsed: EvidenceCreateArgs = deserialize_args(args_obj)?;
            let scope = MemoryScope::try_new(parsed.scope)?;
            let idempotency_key = parse_opt_idempotency(parsed.idempotency_key)?;
            let req = IngestEvidenceRequest {
                scope,
                media_type: parsed.media_type,
                content: parsed.content,
                source_uri: parsed.source_uri,
                observed_at: parsed.observed_at,
                metadata: parsed.metadata,
                idempotency_key,
            };
            req.validate()?;
            Ok(Some(ParsedToolCall::EvidenceCreate(req)))
        }
        "memory_evidence_list" => {
            let parsed: EvidenceListArgs = deserialize_args(args_obj)?;
            let scope = parsed.scope.map(MemoryScope::try_new).transpose()?;
            if let Some(lim) = parsed.limit
                && (lim == 0 || lim > MAX_EVIDENCE_LIST_LIMIT)
            {
                return Err(DennettError::InvalidInput {
                    field: String::from("limit"),
                    reason: format!("must be between 1 and {MAX_EVIDENCE_LIST_LIMIT}"),
                });
            }
            Ok(Some(ParsedToolCall::EvidenceList {
                scope,
                limit: parsed.limit,
            }))
        }
        _ => Ok(None),
    }
}

fn parse_opt_idempotency(raw: Option<String>) -> Result<IdempotencyKey, DennettError> {
    match raw {
        Some(s) => IdempotencyKey::try_new(s),
        None => Ok(IdempotencyKey::generate()),
    }
}

fn deserialize_args<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, DennettError> {
    serde_json::from_value(value).map_err(|err| DennettError::InvalidInput {
        field: String::from("arguments"),
        reason: redact_secrets(&err.to_string(), &[]),
    })
}

/// Thin MCP JSON-RPC dispatcher backed by [`MemoryClient`] and optional [`HmacTokenMinter`].
#[derive(Clone)]
pub struct MemoryMcpServer {
    client: MemoryClient,
    base_url: String,
    static_token: Option<String>,
    hmac_minter: Option<HmacTokenMinter>,
    extra_secrets: Vec<String>,
    http: reqwest::Client,
}

impl fmt::Debug for MemoryMcpServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryMcpServer")
            .field("base_url", &self.base_url)
            .field(
                "static_token",
                &self.static_token.as_ref().map(|_| REDACTED_PLACEHOLDER),
            )
            .field("hmac_minter", &self.hmac_minter)
            .finish_non_exhaustive()
    }
}

impl MemoryMcpServer {
    /// Creates a new [`MemoryMcpServer`] wrapping a configured [`MemoryClient`].
    #[must_use]
    pub fn new(client: MemoryClient) -> Self {
        let base_url = client.base_url().to_owned();
        let http = reqwest::Client::builder()
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
            .timeout(DEFAULT_CLIENT_TIMEOUT)
            .no_proxy()
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            client,
            base_url,
            static_token: None,
            hmac_minter: None,
            extra_secrets: Vec::new(),
            http,
        }
    }

    /// Creates a [`MemoryMcpServer`] with explicit URL, optional static/pre-minted bearer token,
    /// optional [`HmacTokenMinter`] (`DENNETT_MEMORY_HMAC_SECRET` / `--memory-hmac-secret`), and
    /// additional secret strings to redact from every diagnostic path.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `base_url` or `bearer_token` is malformed, or
    /// [`DennettError::Internal`] if HTTP client initialization fails.
    pub fn with_auth(
        base_url: &str,
        bearer_token: Option<String>,
        hmac_minter: Option<HmacTokenMinter>,
        mut extra_secrets: Vec<String>,
    ) -> Result<Self, DennettError> {
        let clean_token = bearer_token
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty());
        let client = match clean_token.as_deref() {
            Some(tok) => MemoryClient::new(base_url, tok)?,
            None => MemoryClient::unauthenticated(base_url)?,
        };
        if let Some(ref minter) = hmac_minter {
            let sec = minter.secret_for_redaction();
            if !sec.is_empty() && !extra_secrets.iter().any(|s| s == sec) {
                extra_secrets.push(sec.to_owned());
            }
        }
        let normalized_url = client.base_url().to_owned();
        let http = reqwest::Client::builder()
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
            .timeout(DEFAULT_CLIENT_TIMEOUT)
            .no_proxy()
            .build()
            .map_err(|err| DennettError::Internal {
                detail: format!("failed to construct HTTP client: {err}"),
            })?;
        Ok(Self {
            client,
            base_url: normalized_url,
            static_token: clean_token,
            hmac_minter,
            extra_secrets,
            http,
        })
    }

    /// Redacts generic secrets, the configured bearer credential, and any HMAC signing secret.
    #[must_use]
    pub fn redact_text(&self, text: &str) -> String {
        let first = self.client.redact_text(text);
        if self.extra_secrets.is_empty() {
            return first;
        }
        let refs: Vec<&str> = self.extra_secrets.iter().map(String::as_str).collect();
        redact_secrets(&first, &refs)
    }

    fn client_and_token_for(
        &self,
        scope: Option<&MemoryScope>,
    ) -> Result<(MemoryClient, Option<String>), DennettError> {
        if self.static_token.is_some() {
            return Ok((self.client.clone(), self.static_token.clone()));
        }
        if let Some(ref minter) = self.hmac_minter {
            let tok = minter.mint_for_scope(scope)?;
            let cli = MemoryClient::new(&self.base_url, tok.clone())?;
            return Ok((cli, Some(tok)));
        }
        Ok((self.client.clone(), None))
    }

    /// Dispatches one parsed JSON-RPC 2.0 message and returns an optional response payload.
    ///
    /// Only a JSON object carrying a string `"method"` and omitting `"id"` is treated as a
    /// notification (`None`). Non-object frames or objects missing a valid string `"method"`
    /// return JSON-RPC `-32600 Invalid Request`.
    pub async fn handle_message(&self, msg: &Value) -> Option<Value> {
        let Some(obj) = msg.as_object() else {
            return Some(invalid_request(
                &Value::Null,
                "invalid JSON-RPC request: frame must be a JSON object",
            ));
        };

        let has_id = obj.contains_key("id");
        let id = obj.get("id").cloned().unwrap_or(Value::Null);
        let Some(method) = obj.get("method").and_then(Value::as_str) else {
            return Some(invalid_request(
                &id,
                "invalid JSON-RPC request: missing string `method`",
            ));
        };

        if !has_id {
            return None;
        }

        let params = obj.get("params").unwrap_or(&Value::Null);

        match method {
            "initialize" => {
                let client_protocol = params
                    .get("protocolVersion")
                    .and_then(Value::as_str)
                    .unwrap_or(DEFAULT_PROTOCOL_VERSION);
                Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": client_protocol,
                        "capabilities": {
                            "tools": {}
                        },
                        "serverInfo": {
                            "name": "dennett-memory-mcp",
                            "version": env!("CARGO_PKG_VERSION")
                        }
                    }
                }))
            }
            "ping" => Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {}
            })),
            "tools/list" => Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": tool_descriptors()
                }
            })),
            "tools/call" => Some(self.handle_tool_call(&id, params).await),
            _ => Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!("method `{method}` not found")
                }
            })),
        }
    }

    async fn handle_tool_call(&self, id: &Value, params: &Value) -> Value {
        let tool_name = params
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let raw_args = params.get("arguments").unwrap_or(&Value::Null);

        let parsed = match parse_tool_call(tool_name, raw_args) {
            Ok(Some(call)) => call,
            Ok(None) => {
                return json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": format!("unknown tool `{tool_name}`")
                    }
                });
            }
            Err(err) => return self.tool_error_redacted(id, &err),
        };

        let result_json = self.dispatch_tool_call(parsed).await;

        match result_json {
            Ok(text) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{
                        "type": "text",
                        "text": text
                    }],
                    "isError": false
                }
            }),
            Err(err) => self.tool_error_redacted(id, &err),
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "exhaustive match dispatches all 18 D1+D3 MCP memory tools through one authenticated memoryd boundary"
    )]
    async fn dispatch_tool_call(&self, parsed: ParsedToolCall) -> Result<String, DennettError> {
        match parsed {
            ParsedToolCall::Search(req) => {
                let (cli, _) = self.client_and_token_for(req.scope.as_ref())?;
                cli.search_notes(&req, None)
                    .await
                    .and_then(|resp| to_json_string(&resp))
            }
            ParsedToolCall::GetCurrent(note_id) => {
                let (cli, _) = self.client_and_token_for(None)?;
                cli.get_note_current(note_id, None)
                    .await
                    .and_then(|resp| to_json_string(&resp))
            }
            ParsedToolCall::GetHistory(note_id) => {
                let (cli, _) = self.client_and_token_for(None)?;
                cli.get_note_history(note_id, None)
                    .await
                    .and_then(|resp| to_json_string(&resp))
            }
            ParsedToolCall::WriteNote(req) => {
                let (cli, _) = self.client_and_token_for(Some(&req.scope))?;
                cli.create_note(&req, None)
                    .await
                    .and_then(|resp| to_json_string(&resp))
            }
            ParsedToolCall::CorrectNote(req) => {
                let (cli, _) = self.client_and_token_for(Some(&req.scope))?;
                cli.correct_note(&req, None)
                    .await
                    .and_then(|resp| to_json_string(&resp))
            }
            ParsedToolCall::ClaimCreate(req) => {
                let (cli, _) = self.client_and_token_for(Some(&req.scope))?;
                cli.create_claim(&req, None)
                    .await
                    .and_then(|resp| to_json_string(&resp))
            }
            ParsedToolCall::ClaimLookup { scope, topic_key } => {
                let (cli, _) = self.client_and_token_for(scope.as_ref())?;
                match (scope.as_ref(), topic_key.as_deref()) {
                    (Some(sc), Some(topic)) if !topic.trim().is_empty() => cli
                        .list_claims_by_topic(sc, topic, None)
                        .await
                        .and_then(|resp| to_json_string(&resp)),
                    _ => cli
                        .list_claims(scope.as_ref(), topic_key.as_deref(), None)
                        .await
                        .and_then(|resp| to_json_string(&resp)),
                }
            }
            ParsedToolCall::ClaimSupersede(req) => {
                let (cli, _) = self.client_and_token_for(Some(&req.scope))?;
                cli.supersede_claim(&req, None)
                    .await
                    .and_then(|resp| to_json_string(&resp))
            }
            ParsedToolCall::ConflictList(scope) => {
                let (cli, _) = self.client_and_token_for(scope.as_ref())?;
                cli.list_open_conflicts(scope.as_ref(), None)
                    .await
                    .and_then(|resp| to_json_string(&resp))
            }
            ParsedToolCall::ConflictResolve(req) => {
                let (cli, _) = self.client_and_token_for(Some(&req.scope))?;
                cli.resolve_conflict(&req, None)
                    .await
                    .and_then(|resp| to_json_string(&resp))
            }
            ParsedToolCall::DocumentRead {
                document_id,
                scope,
                include_versions,
            } => {
                let (cli, _) = self.client_and_token_for(scope.as_ref())?;
                match document_id {
                    Some(doc_id) if include_versions => cli
                        .get_document_versions(doc_id, None)
                        .await
                        .and_then(|resp| to_json_string(&resp)),
                    Some(doc_id) => cli
                        .get_document(doc_id, None)
                        .await
                        .and_then(|resp| to_json_string(&resp)),
                    None => cli
                        .list_documents(scope.as_ref(), None)
                        .await
                        .and_then(|resp| to_json_string(&resp)),
                }
            }
            ParsedToolCall::DocumentCreate(req) => {
                let (cli, _) = self.client_and_token_for(Some(&req.scope))?;
                cli.create_document(&req, None)
                    .await
                    .and_then(|resp| to_json_string(&resp))
            }
            ParsedToolCall::DocumentUpdate(req) => {
                let (cli, _) = self.client_and_token_for(Some(&req.scope))?;
                cli.update_document(&req, None)
                    .await
                    .and_then(|resp| to_json_string(&resp))
            }
            ParsedToolCall::Forget(req) => {
                let (cli, _) = self.client_and_token_for(Some(&req.scope))?;
                cli.forget(&req, None)
                    .await
                    .and_then(|resp| to_json_string(&resp))
            }
            ParsedToolCall::Export(req) => {
                let (cli, _) = self.client_and_token_for(Some(&req.scope))?;
                cli.export_package(&req, None)
                    .await
                    .and_then(|resp| to_json_string(&resp))
            }
            ParsedToolCall::Import(req) => {
                let (cli, _) = self.client_and_token_for(Some(&req.scope))?;
                cli.import_package(&req, None)
                    .await
                    .and_then(|resp| to_json_string(&resp))
            }
            ParsedToolCall::IndexStatus(scope) => {
                let (cli, _) = self.client_and_token_for(scope.as_ref())?;
                cli.get_index_status(scope.as_ref(), None)
                    .await
                    .and_then(|resp| to_json_string(&resp))
            }
            ParsedToolCall::EvidenceCreate(req) => {
                let (cli, _) = self.client_and_token_for(Some(&req.scope))?;
                cli.ingest_evidence(&req, None)
                    .await
                    .and_then(|resp| to_json_string(&resp))
            }
            ParsedToolCall::EvidenceList { scope, limit } => {
                let resp = self.fetch_evidence_list(scope.as_ref(), limit).await?;
                to_json_string(&resp)
            }
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "sends authenticated bounded GET /v1/memory/evidence request and maps HTTP/domain errors cleanly"
    )]
    async fn fetch_evidence_list(
        &self,
        scope: Option<&MemoryScope>,
        limit: Option<u32>,
    ) -> Result<Value, DennettError> {
        let (_, token) = self.client_and_token_for(scope)?;
        let mut params = Vec::new();
        if let Some(sc) = scope {
            params.push(format!("scope={}", encode_query_component(sc.as_str())));
        }
        if let Some(lim) = limit {
            params.push(format!("limit={lim}"));
        }
        let url = if params.is_empty() {
            format!("{}/v1/memory/evidence", self.base_url)
        } else {
            format!("{}/v1/memory/evidence?{}", self.base_url, params.join("&"))
        };

        let mut headers = HeaderMap::new();
        let cid = CorrelationId::generate();
        if let Ok(cid_val) = HeaderValue::from_str(cid.as_str()) {
            headers.insert(HEADER_CORRELATION_ID.clone(), cid_val);
        }
        if let Some(ref tok) = token {
            let auth_header = format!("Bearer {tok}");
            let auth_val =
                HeaderValue::from_str(&auth_header).map_err(|_| DennettError::InvalidInput {
                    field: String::from("bearer_token"),
                    reason: String::from("bearer token contains disallowed HTTP header characters"),
                })?;
            headers.insert(AUTHORIZATION, auth_val);
        }

        let mut resp = self
            .http
            .get(&url)
            .headers(headers)
            .send()
            .await
            .map_err(|err| {
                if err.is_timeout() {
                    DennettError::Timeout {
                        operation: String::from("memory_mcp::evidence_list"),
                    }
                } else {
                    DennettError::DatabaseUnavailable {
                        detail: self.redact_text(&format!("memoryd request failed: {err}")),
                    }
                }
            })?;

        let status = resp.status();
        let mut body_bytes = Vec::new();
        while let Some(chunk) =
            resp.chunk()
                .await
                .map_err(|err| DennettError::DatabaseUnavailable {
                    detail: self.redact_text(&format!("memoryd response read failed: {err}")),
                })?
        {
            if body_bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BODY_BYTES {
                return Err(DennettError::Internal {
                    detail: format!(
                        "memoryd response body exceeded limit of {MAX_RESPONSE_BODY_BYTES} bytes"
                    ),
                });
            }
            body_bytes.extend_from_slice(&chunk);
        }

        if status.is_success() {
            return serde_json::from_slice(&body_bytes).map_err(|err| DennettError::Internal {
                detail: self.redact_text(&format!(
                    "failed to decode evidence list JSON from memoryd: {err}"
                )),
            });
        }

        if let Ok(envelope) = serde_json::from_slice::<ApiErrorResponse>(&body_bytes) {
            return Err(envelope.error);
        }
        if let Ok(val) = serde_json::from_slice::<Value>(&body_bytes)
            && let Some(err_val) = val.get("error")
            && let Ok(parsed_err) = serde_json::from_value::<DennettError>(err_val.clone())
        {
            return Err(parsed_err);
        }
        let safe_body = self.redact_text(&String::from_utf8_lossy(&body_bytes));
        Err(match status {
            StatusCode::UNAUTHORIZED => DennettError::Unauthorized {
                reason: if safe_body.is_empty() {
                    String::from("unauthorized")
                } else {
                    safe_body
                },
            },
            StatusCode::FORBIDDEN => DennettError::ForbiddenScope {
                scope: scope.map_or_else(|| String::from("unknown"), ToString::to_string),
                reason: if safe_body.is_empty() {
                    String::from("forbidden")
                } else {
                    safe_body
                },
            },
            StatusCode::BAD_REQUEST => DennettError::InvalidInput {
                field: String::from("request"),
                reason: safe_body,
            },
            StatusCode::SERVICE_UNAVAILABLE => {
                DennettError::DatabaseUnavailable { detail: safe_body }
            }
            _ => DennettError::Internal {
                detail: format!("unexpected HTTP {}: {safe_body}", status.as_u16()),
            },
        })
    }

    fn tool_error_redacted(&self, id: &Value, err: &DennettError) -> Value {
        let safe_message = self.redact_text(&err.to_string());
        let payload = json!({
            "code": err.code(),
            "error": err,
            "message": safe_message,
        });
        let safe_text = self.redact_text(&payload.to_string());
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{
                    "type": "text",
                    "text": safe_text
                }],
                "isError": true
            }
        })
    }
}

fn encode_query_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(char::from(byte));
        } else {
            use core::fmt::Write as _;
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

fn invalid_request(id: &Value, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": -32600,
            "message": message
        }
    })
}

fn to_json_string<T: serde::Serialize>(value: &T) -> Result<String, DennettError> {
    serde_json::to_string(value).map_err(|err| DennettError::Internal {
        detail: format!("failed to serialize MCP tool response: {err}"),
    })
}

/// Formats a [`DennettError`] into a structured, secret-redacted MCP tool error payload.
#[must_use]
pub fn tool_error(id: &Value, err: &DennettError) -> Value {
    let safe_message = redact_secrets(&err.to_string(), &[]);
    let payload = json!({
        "code": err.code(),
        "error": err,
        "message": safe_message,
    });
    let safe_text = redact_secrets(&payload.to_string(), &[]);
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [{
                "type": "text",
                "text": safe_text
            }],
            "isError": true
        }
    })
}

/// Returns the JSON Schema descriptors for all 18 tools exposed by `dennett-memory-mcp`.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "complete JSON Schema manifest for all 18 Memory Fabric MCP tools"
)]
pub fn tool_descriptors() -> Value {
    let evidence_link_schema = json!({
        "type": "object",
        "properties": {
            "evidence_id": { "type": "string", "description": "UUID of an existing evidence object in the same scope." },
            "relationship": { "type": "string", "description": "Link relationship (e.g. supports, excerpt, correction_basis)." },
            "ordinal": { "type": ["integer", "null"], "minimum": 0, "description": "Optional non-negative position hint." },
            "anchor": {
                "type": ["object", "null"],
                "properties": {
                    "kind": { "type": "string" },
                    "start": { "type": "integer", "minimum": 0 },
                    "end": { "type": "integer", "minimum": 0 },
                    "locator": { "type": "string" },
                    "attributes": { "type": "object", "additionalProperties": { "type": "string" } }
                },
                "required": ["kind"]
            }
        },
        "required": ["evidence_id", "relationship"]
    });

    json!([
        {
            "name": "memory_search",
            "description": "Unified ranked search over current memory notes, structured claims, and dossier documents within authorized scopes with fresh-tail overlay and live index status.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "scope": { "type": "string", "description": "Optional scope filter (must be allowed by the active credential)." },
                    "query": { "type": "string", "description": "Optional text search query over notes, claims, and documents." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100 },
                    "include_history": { "type": "boolean", "description": "Include historical revisions and superseded claims in query matching." }
                }
            }
        },
        {
            "name": "memory_get_current",
            "description": "Fetch the current revision and evidence links of a memory note.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "note_id": { "type": "string", "description": "UUID of the memory note." }
                },
                "required": ["note_id"]
            }
        },
        {
            "name": "memory_get_history",
            "description": "Fetch the complete immutable revision history of a memory note.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "note_id": { "type": "string", "description": "UUID of the memory note." }
                },
                "required": ["note_id"]
            }
        },
        {
            "name": "memory_write_note",
            "description": "Create a new revision-1 memory note with optional evidence links, admission status, salience, and topic key.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "scope": { "type": "string" },
                    "title": { "type": "string" },
                    "body": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "evidence_links": { "type": "array", "items": evidence_link_schema.clone() },
                    "author_label": { "type": "string", "description": "Optional display label; never used for authorization." },
                    "admission_status": { "type": "string", "enum": ["admitted", "ephemeral", "rejected"] },
                    "salience": { "type": "integer", "minimum": 0, "maximum": 100 },
                    "topic_key": { "type": "string" },
                    "idempotency_key": { "type": "string" }
                },
                "required": ["scope", "title", "body"]
            }
        },
        {
            "name": "memory_correct_note",
            "description": "Append an immutable correction revision to an existing memory note using optimistic revision locking.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "note_id": { "type": "string" },
                    "scope": { "type": "string" },
                    "expected_revision": { "type": "integer", "minimum": 1 },
                    "title": { "type": "string" },
                    "body": { "type": "string" },
                    "rationale": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "evidence_links": { "type": "array", "items": evidence_link_schema.clone() },
                    "author_label": { "type": "string", "description": "Optional display label; never used for authorization." },
                    "idempotency_key": { "type": "string" }
                },
                "required": ["note_id", "scope", "expected_revision", "title", "body", "rationale"]
            }
        },
        {
            "name": "memory_claim_create",
            "description": "Create a structured claim with topic_key, optional caveats, and automatic conflict detection.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "scope": { "type": "string" },
                    "topic_key": { "type": "string" },
                    "claim_key": { "type": "string" },
                    "natural_language": { "type": "string" },
                    "epistemic_status": { "type": "string", "enum": ["observed", "asserted", "inferred", "hypothesis", "contested", "superseded", "retracted"] },
                    "supersedes_claim_id": { "type": "string" },
                    "valid_from": { "type": "integer" },
                    "valid_to": { "type": "integer" },
                    "source_note_id": { "type": "string" },
                    "caveats": { "type": "array", "items": { "type": "string" } },
                    "provenance": { "type": "object", "additionalProperties": { "type": "string" } },
                    "evidence_links": { "type": "array", "items": evidence_link_schema.clone() },
                    "idempotency_key": { "type": "string" }
                },
                "required": ["scope", "topic_key", "natural_language"]
            }
        },
        {
            "name": "memory_claim_lookup",
            "description": "List structured claims filtered by authorized scope and optional topic_key.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "scope": { "type": "string" },
                    "topic_key": { "type": "string" }
                }
            }
        },
        {
            "name": "memory_claim_supersede",
            "description": "Atomically supersede an active claim with a newer claim (respecting temporal and concurrency guards).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "claim_id": { "type": "string" },
                    "scope": { "type": "string" },
                    "topic_key": { "type": "string" },
                    "claim_key": { "type": "string" },
                    "natural_language": { "type": "string" },
                    "epistemic_status": { "type": "string", "enum": ["observed", "asserted", "inferred", "hypothesis"] },
                    "valid_from": { "type": "integer" },
                    "valid_to": { "type": "integer" },
                    "source_note_id": { "type": "string" },
                    "caveats": { "type": "array", "items": { "type": "string" } },
                    "provenance": { "type": "object", "additionalProperties": { "type": "string" } },
                    "evidence_links": { "type": "array", "items": evidence_link_schema },
                    "resolution_event_id": { "type": "string" },
                    "idempotency_key": { "type": "string" }
                },
                "required": ["claim_id", "scope", "topic_key", "natural_language"]
            }
        },
        {
            "name": "memory_conflict_list",
            "description": "List open conflicts across authorized scopes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "scope": { "type": "string" }
                }
            }
        },
        {
            "name": "memory_conflict_resolve",
            "description": "Resolve or adjudicate an open memory conflict with an audit rationale and optional winning claim.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "conflict_id": { "type": "string" },
                    "scope": { "type": "string" },
                    "resolution_note": { "type": "string" },
                    "winning_claim_id": { "type": "string" },
                    "target_status": { "type": "string", "enum": ["resolved", "accepted_ambiguity", "invalidated"] },
                    "idempotency_key": { "type": "string" }
                },
                "required": ["conflict_id", "scope", "resolution_note"]
            }
        },
        {
            "name": "memory_document_read",
            "description": "Read a human-readable dossier document (or its version history) by ID, or list dossiers in a scope.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "document_id": { "type": "string" },
                    "scope": { "type": "string" },
                    "include_versions": { "type": "boolean" }
                }
            }
        },
        {
            "name": "memory_document_write",
            "description": "Create a new dossier document (with document_key) or update an existing one (with document_id and expected_version).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "scope": { "type": "string" },
                    "document_id": { "type": "string" },
                    "document_key": { "type": "string" },
                    "expected_version": { "type": "integer", "minimum": 1 },
                    "title": { "type": "string" },
                    "body": { "type": "string" },
                    "provenance_claim_ids": { "type": "array", "items": { "type": "string" } },
                    "provenance_note_ids": { "type": "array", "items": { "type": "string" } },
                    "diff_summary": { "type": "string" },
                    "idempotency_key": { "type": "string" }
                },
                "required": ["scope", "title", "body"]
            }
        },
        {
            "name": "memory_forget",
            "description": "Forget a memory entity (note, evidence, claim, or document) with immediate scrub, index purge, and audit tombstone.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "scope": { "type": "string" },
                    "target_type": { "type": "string", "enum": ["note", "evidence", "claim", "document"] },
                    "target_id": { "type": "string" },
                    "reason": { "type": "string" },
                    "idempotency_key": { "type": "string" }
                },
                "required": ["scope", "target_type", "target_id", "reason"]
            }
        },
        {
            "name": "memory_export",
            "description": "Export a deterministic, secret-stripped portable memory package for one authorized scope (Contract J).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "scope": { "type": "string" },
                    "format_version": { "type": "string" }
                },
                "required": ["scope"]
            }
        },
        {
            "name": "memory_import",
            "description": "Import a portable memory package strictly as external_untrusted data with instruction_active=false (Contract J).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "scope": { "type": "string" },
                    "package_json": { "type": "string" }
                },
                "required": ["scope", "package_json"]
            }
        },
        {
            "name": "memory_index_status",
            "description": "Query live search index health and watermark status for an authorized scope.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "scope": { "type": "string" }
                }
            }
        },
        {
            "name": "memory_evidence_create",
            "description": "Ingest an immutable inline evidence object into an authorized scope.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "scope": { "type": "string" },
                    "content": { "type": "string" },
                    "media_type": { "type": "string", "description": "MIME type (defaults to text/plain)." },
                    "source_uri": { "type": "string" },
                    "observed_at": { "type": "integer" },
                    "metadata": { "type": "object", "additionalProperties": { "type": "string" } },
                    "idempotency_key": { "type": "string" }
                },
                "required": ["scope", "content"]
            }
        },
        {
            "name": "memory_evidence_list",
            "description": "List active evidence objects across authorized scopes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "scope": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
                }
            }
        }
    ])
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use proptest::prelude::*;

    use super::*;

    #[test]
    fn bounded_line_reader_enforces_exact_limit_and_recovers_on_next_line() {
        let input = b"12345\n1234567890\nok\n";
        let mut cursor = Cursor::new(&input[..]);

        assert_eq!(
            read_bounded_line(&mut cursor, 6).expect("line 1"),
            BoundedRead::Line(String::from("12345\n"))
        );
        assert_eq!(
            read_bounded_line(&mut cursor, 6).expect("line 2 oversized"),
            BoundedRead::TooLarge
        );
        assert_eq!(
            read_bounded_line(&mut cursor, 6).expect("line 3 recovered"),
            BoundedRead::Line(String::from("ok\n"))
        );
        assert_eq!(
            read_bounded_line(&mut cursor, 6).expect("eof"),
            BoundedRead::Eof
        );
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "unit test validates schema parsing and rejection paths across all 16 MCP memory tools"
    )]
    fn parse_tool_call_validates_all_tools_and_rejects_bad_inputs() {
        let search = parse_tool_call(
            "memory_search",
            &json!({"scope": "project-alpha", "limit": 5, "include_history": true}),
        );
        assert!(matches!(search, Ok(Some(ParsedToolCall::Search(_)))));

        let bad_scope = parse_tool_call("memory_search", &json!({"scope": "../escape"}));
        assert!(matches!(bad_scope, Err(DennettError::InvalidInput { .. })));

        let bad_limit = parse_tool_call("memory_search", &json!({"limit": 0}));
        assert!(matches!(bad_limit, Err(DennettError::InvalidInput { .. })));

        let bad_uuid = parse_tool_call("memory_get_current", &json!({"note_id": "not-a-uuid"}));
        assert!(matches!(bad_uuid, Err(DennettError::InvalidInput { .. })));

        let bad_rev = parse_tool_call(
            "memory_correct_note",
            &json!({
                "note_id": MemoryNoteId::new().to_string(),
                "scope": "project-alpha",
                "expected_revision": 0,
                "title": "t",
                "body": "b",
                "rationale": "r"
            }),
        );
        assert!(matches!(bad_rev, Err(DennettError::InvalidInput { .. })));

        assert!(matches!(
            parse_tool_call(
                "memory_claim_create",
                &json!({
                    "scope": "project-alpha",
                    "topic_key": "runtime",
                    "natural_language": "Use PG16",
                    "caveats": ["Except client cache"]
                })
            ),
            Ok(Some(ParsedToolCall::ClaimCreate(_)))
        ));
        assert!(matches!(
            parse_tool_call(
                "memory_claim_lookup",
                &json!({"scope": "project-alpha", "topic_key": "runtime"})
            ),
            Ok(Some(ParsedToolCall::ClaimLookup { .. }))
        ));
        assert!(matches!(
            parse_tool_call(
                "memory_claim_supersede",
                &json!({
                    "claim_id": ClaimId::new().to_string(),
                    "scope": "project-alpha",
                    "topic_key": "runtime",
                    "natural_language": "Use PG17"
                })
            ),
            Ok(Some(ParsedToolCall::ClaimSupersede(_)))
        ));
        assert!(matches!(
            parse_tool_call("memory_conflict_list", &Value::Null),
            Ok(Some(ParsedToolCall::ConflictList(None)))
        ));
        assert!(matches!(
            parse_tool_call(
                "memory_conflict_resolve",
                &json!({
                    "conflict_id": ConflictId::new().to_string(),
                    "scope": "project-alpha",
                    "resolution_note": "Resolved by owner"
                })
            ),
            Ok(Some(ParsedToolCall::ConflictResolve(_)))
        ));
        assert!(matches!(
            parse_tool_call(
                "memory_document_write",
                &json!({
                    "scope": "project-alpha",
                    "document_key": "dossier:arch",
                    "title": "Arch",
                    "body": "Body"
                })
            ),
            Ok(Some(ParsedToolCall::DocumentCreate(_)))
        ));
        assert!(matches!(
            parse_tool_call(
                "memory_document_write",
                &json!({
                    "scope": "project-alpha",
                    "document_id": DocumentId::new().to_string(),
                    "expected_version": 1,
                    "title": "Arch",
                    "body": "Updated"
                })
            ),
            Ok(Some(ParsedToolCall::DocumentUpdate(_)))
        ));
        assert!(matches!(
            parse_tool_call(
                "memory_forget",
                &json!({
                    "scope": "project-alpha",
                    "target_type": "note",
                    "target_id": Uuid::new_v4().to_string(),
                    "reason": "owner requested deletion"
                })
            ),
            Ok(Some(ParsedToolCall::Forget(_)))
        ));
        assert!(matches!(
            parse_tool_call("memory_export", &json!({"scope": "project-alpha"})),
            Ok(Some(ParsedToolCall::Export(_)))
        ));
        assert!(matches!(
            parse_tool_call(
                "memory_import",
                &json!({"scope": "project-alpha", "package_json": "{}"})
            ),
            Ok(Some(ParsedToolCall::Import(_)))
        ));
        assert!(matches!(
            parse_tool_call("memory_index_status", &Value::Null),
            Ok(Some(ParsedToolCall::IndexStatus(None)))
        ));
        assert!(matches!(
            parse_tool_call(
                "memory_evidence_create",
                &json!({
                    "scope": "project-alpha",
                    "content": "Primary benchmark log",
                    "media_type": "text/plain",
                    "source_uri": "https://example.invalid/bench"
                })
            ),
            Ok(Some(ParsedToolCall::EvidenceCreate(_)))
        ));
        assert!(matches!(
            parse_tool_call("memory_evidence_list", &Value::Null),
            Ok(Some(ParsedToolCall::EvidenceList {
                scope: None,
                limit: None
            }))
        ));
        assert!(matches!(
            parse_tool_call("memory_evidence_list", &json!({"limit": 0})),
            Err(DennettError::InvalidInput { .. })
        ));

        let unknown = parse_tool_call("memory_drop_database", &json!({}));
        assert_eq!(unknown, Ok(None));
    }

    #[tokio::test]
    async fn mcp_protocol_handles_initialize_ping_list_and_notifications() {
        let client = MemoryClient::unauthenticated("http://127.0.0.1:1").expect("valid client");
        let server = MemoryMcpServer::new(client);

        // Notification without `id` produces no response.
        assert!(
            server
                .handle_message(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
                .await
                .is_none()
        );

        let init = server
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {"protocolVersion": "2024-11-05"}
            }))
            .await
            .expect("initialize response");
        assert_eq!(init["result"]["serverInfo"]["name"], "dennett-memory-mcp");

        let list = server
            .handle_message(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}))
            .await
            .expect("tools/list response");
        let tools = list["result"]["tools"].as_array().expect("tools array");
        assert_eq!(tools.len(), 18);

        let unknown = server
            .handle_message(&json!({"jsonrpc": "2.0", "id": 3, "method": "does/not/exist"}))
            .await
            .expect("unknown method error");
        assert_eq!(unknown["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn mcp_rejects_non_object_and_missing_method_frames_with_32600_and_redacts_token() {
        let secret = "dnt_static_secret_token_for_redaction_check_9911";
        let client = MemoryClient::new("http://127.0.0.1:1", secret).expect("valid client");
        let server = MemoryMcpServer::new(client);

        for invalid in [
            json!(123),
            json!([]),
            json!("raw-string"),
            json!({}),
            json!({"method": 42}),
        ] {
            let resp = server.handle_message(&invalid).await.unwrap_or_else(|| {
                panic!("invalid frame `{invalid}` must return -32600, not None")
            });
            assert_eq!(resp["error"]["code"], -32600);
            assert_eq!(resp["id"], Value::Null);
        }

        let missing_method_with_id = server
            .handle_message(&json!({"jsonrpc": "2.0", "id": 9}))
            .await
            .expect("missing method with id must return -32600");
        assert_eq!(missing_method_with_id["error"]["code"], -32600);
        assert_eq!(missing_method_with_id["id"], 9);

        // Pre-HTTP tool validation error echoing the static token string MUST redact it.
        let leak_attempt = server
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "id": 10,
                "method": "tools/call",
                "params": {
                    "name": "memory_search",
                    "arguments": {
                        secret: "unexpected_field_with_secret_name"
                    }
                }
            }))
            .await
            .expect("tool error response");
        assert_eq!(leak_attempt["result"]["isError"], true);
        let text = leak_attempt["result"]["content"][0]["text"]
            .as_str()
            .expect("text content");
        assert!(
            !text.contains(secret),
            "configured bearer token leaked in pre-HTTP MCP tool error: {text}"
        );
    }

    proptest! {
        #[test]
        fn prop_bounded_reader_never_exceeds_max_bytes_on_arbitrary_ascii(
            payload in "[A-Za-z0-9 ]{0,200}",
            limit in 1_usize..64_usize,
        ) {
            let line = format!("{payload}\n");
            let mut cursor = Cursor::new(line.as_bytes());
            let res = read_bounded_line(&mut cursor, limit).expect("valid ascii");
            let eof = read_bounded_line(&mut cursor, limit).expect("eof");
            prop_assert_eq!(eof, BoundedRead::Eof);
            if line.len() <= limit {
                prop_assert_eq!(res, BoundedRead::Line(line));
            } else {
                prop_assert_eq!(res, BoundedRead::TooLarge);
            }
        }
    }
}
