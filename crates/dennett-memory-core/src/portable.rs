//! Portable, deterministic, secret-stripped Export & untrusted Import (Contract J).
//!
//! Security and compatibility guarantees:
//! - Exports strictly filter by a single authorized scope (`forgotten_at IS NULL`) and scrub
//!   all secret patterns (`dnt1.`, `dct1.`, `GEMINI_API_KEY`, `Bearer `, `AIza...`, `sk-...`,
//!   `ghp_...`, private keys, URI passwords, etc.) idempotently before computing SHA-256.
//! - Deterministic canonical JSON + embedded payload `manifest_sha256` guarantees byte-identical
//!   repeated exports and `export -> import -> export` roundtrip SHA-256 identity.
//! - Imports enforce hard bounds (`<= 8 MiB`, JSON depth `<= 32`, strict field length/control
//!   character/path-traversal checks, `manifest_sha256` integrity verification), NEVER panic on
//!   arbitrary bytes, and ALWAYS tag every imported item `trust_level = ExternalUntrusted` with
//!   `instruction_active = false`.

use std::collections::BTreeMap;

use dennett_contracts::redact::{REDACTED_PLACEHOLDER, redact_secrets};
use dennett_contracts::{
    AdmissionStatus, AuthPrincipal, ClaimId, CorrelationId, DennettError, DocumentId,
    DocumentVersionId, EpistemicStatus, ExportPackageId, ExportRequest, ExportResponse,
    IdempotencyKey, ImportRecordId, ImportRequest, ImportResponse, MAX_CAVEAT_BYTES,
    MAX_CAVEATS_PER_CLAIM, MAX_CLAIM_KEY_LEN, MAX_CLAIM_TEXT_BYTES, MAX_DOCUMENT_BODY_BYTES,
    MAX_DOCUMENT_KEY_LEN, MAX_DOCUMENT_TITLE_BYTES, MAX_METADATA_ENTRIES, MAX_METADATA_KEY_LEN,
    MAX_METADATA_VALUE_LEN, MAX_NOTE_BODY_BYTES, MAX_NOTE_SALIENCE, MAX_NOTE_TITLE_BYTES,
    MAX_RATIONALE_BYTES, MAX_TAG_LEN, MAX_TAGS_PER_NOTE, MAX_TOPIC_KEY_LEN, MemoryCapability,
    MemoryEventId, MemoryEventKind, MemoryNoteId, NoteRevisionId, TimestampMs, TrustLevel,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tokio::time;

use crate::claims::{NewClaimRow, insert_claim_row};
use crate::fingerprint::sha256_hex;
use crate::search::{
    maybe_advance_watermark_only, maybe_index_claim_in_tx, maybe_index_document_in_tx,
    maybe_index_note_in_tx,
};
use crate::service::{
    MemoryService, NewMemoryEvent, insert_memory_event, require_scope_capability,
};

/// Supported Contract J portable memory package format version.
pub const PORTABLE_FORMAT_VERSION: &str = "1.0";
/// Strict maximum accepted byte length for imported portable memory packages (8 MiB).
pub const MAX_IMPORT_SAFE_BYTES: usize = 8 * 1024 * 1024;
/// Strict maximum JSON nesting depth accepted by the portable package parser.
pub const MAX_IMPORT_JSON_DEPTH: usize = 32;
/// Maximum number of items accepted per section (`notes`, `claims`, `documents`) in one import.
pub const MAX_IMPORT_ITEMS_PER_SECTION: usize = 10_000;
/// Maximum number of validation errors accumulated in an [`ImportResponse`].
pub const MAX_VALIDATION_ERRORS: usize = 64;

/// Top-level deterministic portable memory package envelope (Contract J).
///
/// Field names are ordered alphabetically so canonical JSON serialization is deterministic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortablePackageEnvelope {
    /// Package schema format version (`"1.0"`).
    pub format_version: String,
    /// Lowercase hex SHA-256 of the canonical JSON serialization of `payload`.
    pub manifest_sha256: String,
    /// Canonical sorted semantic payload.
    pub payload: PortablePackagePayload,
    /// Confirms secret patterns were stripped during export.
    pub secrets_stripped: bool,
}

/// Canonical sorted payload inside [`PortablePackageEnvelope`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortablePackagePayload {
    /// Deterministic sorted list of exported structured claims.
    #[serde(default)]
    pub claims: Vec<ExportedClaim>,
    /// Deterministic sorted list of exported dossier documents.
    #[serde(default)]
    pub documents: Vec<ExportedDocument>,
    /// Deterministic sorted list of exported notes.
    #[serde(default)]
    pub notes: Vec<ExportedNote>,
}

/// Portable representation of one [`dennett_contracts::MemoryNote`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportedNote {
    /// Admission disposition (`admitted`, `ephemeral`, `rejected`).
    pub admission_status: AdmissionStatus,
    /// Note body (secret-scrubbed).
    pub body: String,
    /// Optional latest rationale (secret-scrubbed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    /// Salience score `0..=100`.
    pub salience: u8,
    /// Note tags (secret-scrubbed).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Note title (secret-scrubbed).
    pub title: String,
    /// Optional normalized topic key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic_key: Option<String>,
}

/// Portable representation of one [`dennett_contracts::MemoryClaim`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportedClaim {
    /// Essential caveats/exceptions (secret-scrubbed).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub caveats: Vec<String>,
    /// Optional predicate sub-key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_key: Option<String>,
    /// Epistemic status label.
    pub epistemic_status: EpistemicStatus,
    /// Natural-language claim text (secret-scrubbed).
    pub natural_language: String,
    /// Provenance metadata map (secret-scrubbed).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provenance: BTreeMap<String, String>,
    /// Normalized topic key.
    pub topic_key: String,
    /// Optional validity start timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<TimestampMs>,
    /// Optional validity end timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<TimestampMs>,
}

/// Portable representation of one [`dennett_contracts::MemoryDocument`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ExportedDocument {
    /// Current dossier body (secret-scrubbed).
    pub body: String,
    /// Scope-unique logical document key.
    pub document_key: String,
    /// Human-readable title (secret-scrubbed).
    pub title: String,
}

/// Result of parsing and validating an untrusted portable memory package string.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedImportPackage {
    /// Validated notes ready for `external_untrusted` insertion.
    pub valid_notes: Vec<ExportedNote>,
    /// Validated claims ready for `external_untrusted` insertion.
    pub valid_claims: Vec<ExportedClaim>,
    /// Validated documents ready for `external_untrusted` insertion.
    pub valid_documents: Vec<ExportedDocument>,
    /// Count of rejected items (or 1 when the top-level envelope itself failed validation).
    pub items_rejected: u64,
    /// Safe, bounded human-readable validation error strings.
    pub validation_errors: Vec<String>,
}

/// Thoroughly and idempotently scrubs secret tokens, API keys, and sensitive credential markers
/// from text destined for a portable export package (Contract J).
///
/// Guarantees that none of `dnt1.`, `dct1.`, `GEMINI_API_KEY`, `GOOGLE_API_KEY`, `Bearer `,
/// `AIza...`, `sk-...`, `ghp_...`, `gho_...`, `AKIA...`, PEM private keys, or URI userinfo
/// secrets survive in the returned string.
#[must_use]
pub fn scrub_export_text(input: &str) -> String {
    let mut out = input.to_owned();
    for _ in 0..4 {
        let next = scrub_export_once(&out);
        if next == out {
            break;
        }
        out = next;
    }
    out
}

fn scrub_export_once(input: &str) -> String {
    let mut out = redact_secrets(input, &[]);
    out = scrub_pem_blocks(&out);
    out = scrub_prefixed_secret(&out, "dnt1.", 1, is_token_char);
    out = scrub_prefixed_secret(&out, "dct1.", 1, is_token_char);
    out = scrub_prefixed_secret(&out, "AIza", 12, is_token_char);
    out = scrub_prefixed_secret(&out, "sk-", 12, is_token_char);
    out = scrub_prefixed_secret(&out, "ghp_", 12, is_token_char);
    out = scrub_prefixed_secret(&out, "gho_", 12, is_token_char);
    out = scrub_prefixed_secret(&out, "ghu_", 12, is_token_char);
    out = scrub_prefixed_secret(&out, "ghs_", 12, is_token_char);
    out = scrub_prefixed_secret(&out, "ghr_", 12, is_token_char);
    out = scrub_prefixed_secret(&out, "xoxb-", 10, is_token_char);
    out = scrub_prefixed_secret(&out, "xoxp-", 10, is_token_char);
    out = scrub_prefixed_secret(&out, "AKIA", 12, is_upper_alnum);

    for marker in [
        "GEMINI_API_KEY",
        "GOOGLE_API_KEY",
        "DENNETT_MEMORY_TOKEN",
        "DENNETT_HEAD_TOKEN",
        "DENNETT_CONTROL_TOKEN",
        "DENNETT_VAULT_MASTER_KEY",
    ] {
        out = scrub_case_insensitive_marker(&out, marker);
    }
    out = scrub_bearer_phrase(&out);
    out
}

fn scrub_required_export_text(input: &str) -> String {
    let scrubbed = scrub_export_text(input);
    if scrubbed.trim().is_empty() {
        String::from(REDACTED_PLACEHOLDER)
    } else {
        scrubbed
    }
}

fn scrub_pem_blocks(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("-----BEGIN ") {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        if let Some(end_rel) = after.find("PRIVATE KEY-----") {
            let after_hdr = &after[end_rel + "PRIVATE KEY-----".len()..];
            if let Some(footer_pos) = after_hdr.find("-----END ") {
                let tail = &after_hdr[footer_pos..];
                let end_line = tail.find('\n').map_or(tail.len(), |p| p + 1);
                out.push_str(REDACTED_PLACEHOLDER);
                rest = &tail[end_line..];
                continue;
            }
            out.push_str(REDACTED_PLACEHOLDER);
            return out;
        }
        out.push_str("-----BEGIN ");
        rest = &after["-----BEGIN ".len()..];
    }
    out.push_str(rest);
    out
}

fn scrub_prefixed_secret(
    input: &str,
    prefix: &str,
    min_tail: usize,
    is_char: fn(u8) -> bool,
) -> String {
    let mut out = String::with_capacity(input.len());
    let mut idx = 0;
    while let Some(rel) = input[idx..].find(prefix) {
        let start = idx + rel;
        let tail_start = start + prefix.len();
        let tail_len = input.as_bytes()[tail_start..]
            .iter()
            .take_while(|&&b| is_char(b))
            .count();
        if tail_len >= min_tail {
            out.push_str(&input[idx..start]);
            out.push_str(REDACTED_PLACEHOLDER);
            idx = tail_start + tail_len;
        } else {
            // Even bare `dnt1.` / `dct1.` markers must not leak in portable exports.
            if prefix == "dnt1." || prefix == "dct1." {
                out.push_str(&input[idx..start]);
                out.push_str(REDACTED_PLACEHOLDER);
                idx = tail_start;
            } else {
                out.push_str(&input[idx..tail_start]);
                idx = tail_start;
            }
        }
    }
    out.push_str(&input[idx..]);
    out
}

fn scrub_case_insensitive_marker(input: &str, marker: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let needle = marker.to_ascii_lowercase();
    let mut out = String::with_capacity(input.len());
    let mut idx = 0;

    while let Some(rel) = lower[idx..].find(&needle) {
        let start = idx + rel;
        out.push_str(&input[idx..start]);
        out.push_str(REDACTED_PLACEHOLDER);
        let mut cursor = start + needle.len();
        let bytes = input.as_bytes();
        // Also swallow optional `= [REDACTED]` / `: <secret>` suffix attached to the env var name.
        while bytes.get(cursor).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            cursor += 1;
        }
        if bytes.get(cursor).is_some_and(|b| matches!(b, b'=' | b':')) {
            cursor += 1;
            while bytes.get(cursor).is_some_and(|b| matches!(b, b' ' | b'\t')) {
                cursor += 1;
            }
            if input[cursor..].starts_with(REDACTED_PLACEHOLDER) {
                cursor += REDACTED_PLACEHOLDER.len();
            } else {
                while bytes.get(cursor).is_some_and(|b| {
                    !b.is_ascii_whitespace()
                        && !matches!(b, b',' | b';' | b'"' | b'\'' | b'}' | b']')
                }) {
                    cursor += 1;
                }
            }
        }
        idx = cursor;
    }

    out.push_str(&input[idx..]);
    out
}

fn scrub_bearer_phrase(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut idx = 0;

    while let Some(rel) = lower[idx..].find("bearer") {
        let start = idx + rel;
        let after = start + "bearer".len();
        let left_ok =
            start == 0 || !(bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_');
        let right_space = bytes.get(after).is_some_and(|b| matches!(b, b' ' | b'\t'));
        if !left_ok || !right_space {
            out.push_str(&input[idx..after]);
            idx = after;
            continue;
        }
        out.push_str(&input[idx..start]);
        out.push_str(REDACTED_PLACEHOLDER);
        let mut cursor = after;
        while bytes.get(cursor).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            cursor += 1;
        }
        if input[cursor..].starts_with(REDACTED_PLACEHOLDER) {
            cursor += REDACTED_PLACEHOLDER.len();
        } else {
            while bytes.get(cursor).is_some_and(|b| {
                !b.is_ascii_whitespace() && !matches!(b, b',' | b';' | b'"' | b'\'' | b'}' | b']')
            }) {
                cursor += 1;
            }
        }
        idx = cursor;
    }

    out.push_str(&input[idx..]);
    out
}

const fn is_token_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')
}

const fn is_upper_alnum(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'0'..=b'9')
}

/// Iteratively checks JSON nesting depth without recursion so deeply nested adversarial input
/// can never overflow the stack.
///
/// # Errors
///
/// Returns an error description if container nesting (`{` or `[`) exceeds `max_depth`.
pub fn check_json_depth(raw: &str, max_depth: usize) -> Result<(), String> {
    let mut depth: usize = 0;
    let mut in_string = false;
    let mut escaped = false;

    for b in raw.bytes() {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth = depth.saturating_add(1);
                if depth > max_depth {
                    return Err(format!(
                        "JSON nesting depth exceeded maximum limit of {max_depth}"
                    ));
                }
            }
            b'}' | b']' => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    Ok(())
}

/// Returns `true` if `value` contains path-traversal sequences (`..`, absolute root/drive paths)
/// or unsafe characters disallowed in keys/tags/paths (Contract J §J.8).
#[must_use]
pub fn has_unsafe_path_or_traversal(value: &str) -> bool {
    if value.contains("..") || value.contains('\0') || value.contains('\\') {
        return true;
    }
    if value.starts_with('/') {
        return true;
    }
    let bytes = value.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
}

/// Parses and validates an untrusted portable memory package JSON string without panicking on
/// arbitrary input.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "single-pass defensive parser validates size, depth, manifest SHA-256, and every note/claim/document item"
)]
pub fn parse_and_validate_import_package(raw_json: &str) -> ParsedImportPackage {
    let mut result = ParsedImportPackage::default();

    if raw_json.as_bytes().contains(&0) {
        push_err(&mut result, "package payload contains NUL (\\u0000) bytes");
        result.items_rejected = 1;
        return result;
    }
    if raw_json.len() > MAX_IMPORT_SAFE_BYTES {
        push_err(
            &mut result,
            format!(
                "package size {} bytes exceeds maximum import limit of {MAX_IMPORT_SAFE_BYTES} bytes",
                raw_json.len()
            ),
        );
        result.items_rejected = 1;
        return result;
    }
    if let Err(depth_err) = check_json_depth(raw_json, MAX_IMPORT_JSON_DEPTH) {
        push_err(&mut result, depth_err);
        result.items_rejected = 1;
        return result;
    }

    let raw_val: JsonValue = match serde_json::from_str(raw_json) {
        Ok(v) => v,
        Err(err) => {
            push_err(
                &mut result,
                format!(
                    "malformed JSON package: {}",
                    redact_secrets(&err.to_string(), &[])
                ),
            );
            result.items_rejected = 1;
            return result;
        }
    };

    let Some(root_obj) = raw_val.as_object() else {
        push_err(&mut result, "package root must be a JSON object");
        result.items_rejected = 1;
        return result;
    };

    let format_version = root_obj
        .get("format_version")
        .and_then(JsonValue::as_str)
        .unwrap_or_default();
    if format_version != PORTABLE_FORMAT_VERSION {
        push_err(
            &mut result,
            format!(
                "unsupported package format_version `{}` (expected `{PORTABLE_FORMAT_VERSION}`)",
                redact_secrets(format_version, &[])
            ),
        );
        result.items_rejected = 1;
        return result;
    }

    let Some(declared_manifest) = root_obj.get("manifest_sha256").and_then(JsonValue::as_str)
    else {
        push_err(&mut result, "missing required `manifest_sha256` field");
        result.items_rejected = 1;
        return result;
    };

    let Some(payload_val) = root_obj.get("payload") else {
        push_err(&mut result, "missing required `payload` object");
        result.items_rejected = 1;
        return result;
    };

    let Some(payload_obj) = payload_val.as_object() else {
        push_err(&mut result, "`payload` must be a JSON object");
        result.items_rejected = 1;
        return result;
    };

    // Verify embedded manifest_sha256 against canonical serialization of `payload`.
    let canonical_payload_bytes = match canonical_json_bytes(payload_val) {
        Ok(bytes) => bytes,
        Err(err) => {
            push_err(&mut result, err);
            result.items_rejected = 1;
            return result;
        }
    };
    let computed_manifest = sha256_hex(&canonical_payload_bytes);
    if declared_manifest != computed_manifest.as_str() {
        push_err(
            &mut result,
            "manifest_sha256 verification failed: package payload digest mismatch",
        );
        result.items_rejected = 1;
        return result;
    }

    // Validate notes array.
    if let Some(notes_val) = payload_obj.get("notes") {
        if let Some(arr) = notes_val.as_array() {
            if arr.len() > MAX_IMPORT_ITEMS_PER_SECTION {
                push_err(
                    &mut result,
                    format!("notes array exceeds maximum of {MAX_IMPORT_ITEMS_PER_SECTION} items"),
                );
                result.items_rejected = result.items_rejected.saturating_add(1);
            } else {
                for (idx, item) in arr.iter().enumerate() {
                    match serde_json::from_value::<ExportedNote>(item.clone()) {
                        Ok(note) => match validate_imported_note(&note) {
                            Ok(()) => result.valid_notes.push(note),
                            Err(msg) => {
                                result.items_rejected = result.items_rejected.saturating_add(1);
                                push_err(&mut result, format!("notes[{idx}]: {msg}"));
                            }
                        },
                        Err(err) => {
                            result.items_rejected = result.items_rejected.saturating_add(1);
                            push_err(&mut result, format!("notes[{idx}]: invalid schema ({err})"));
                        }
                    }
                }
            }
        } else {
            result.items_rejected = result.items_rejected.saturating_add(1);
            push_err(&mut result, "`payload.notes` must be an array");
        }
    }

    // Validate claims array.
    if let Some(claims_val) = payload_obj.get("claims") {
        if let Some(arr) = claims_val.as_array() {
            if arr.len() > MAX_IMPORT_ITEMS_PER_SECTION {
                push_err(
                    &mut result,
                    format!("claims array exceeds maximum of {MAX_IMPORT_ITEMS_PER_SECTION} items"),
                );
                result.items_rejected = result.items_rejected.saturating_add(1);
            } else {
                for (idx, item) in arr.iter().enumerate() {
                    match serde_json::from_value::<ExportedClaim>(item.clone()) {
                        Ok(claim) => match validate_imported_claim(&claim) {
                            Ok(()) => result.valid_claims.push(claim),
                            Err(msg) => {
                                result.items_rejected = result.items_rejected.saturating_add(1);
                                push_err(&mut result, format!("claims[{idx}]: {msg}"));
                            }
                        },
                        Err(err) => {
                            result.items_rejected = result.items_rejected.saturating_add(1);
                            push_err(
                                &mut result,
                                format!("claims[{idx}]: invalid schema ({err})"),
                            );
                        }
                    }
                }
            }
        } else {
            result.items_rejected = result.items_rejected.saturating_add(1);
            push_err(&mut result, "`payload.claims` must be an array");
        }
    }

    // Validate documents array.
    if let Some(docs_val) = payload_obj.get("documents") {
        if let Some(arr) = docs_val.as_array() {
            if arr.len() > MAX_IMPORT_ITEMS_PER_SECTION {
                push_err(
                    &mut result,
                    format!(
                        "documents array exceeds maximum of {MAX_IMPORT_ITEMS_PER_SECTION} items"
                    ),
                );
                result.items_rejected = result.items_rejected.saturating_add(1);
            } else {
                for (idx, item) in arr.iter().enumerate() {
                    match serde_json::from_value::<ExportedDocument>(item.clone()) {
                        Ok(doc) => match validate_imported_document(&doc) {
                            Ok(()) => result.valid_documents.push(doc),
                            Err(msg) => {
                                result.items_rejected = result.items_rejected.saturating_add(1);
                                push_err(&mut result, format!("documents[{idx}]: {msg}"));
                            }
                        },
                        Err(err) => {
                            result.items_rejected = result.items_rejected.saturating_add(1);
                            push_err(
                                &mut result,
                                format!("documents[{idx}]: invalid schema ({err})"),
                            );
                        }
                    }
                }
            }
        } else {
            result.items_rejected = result.items_rejected.saturating_add(1);
            push_err(&mut result, "`payload.documents` must be an array");
        }
    }

    result
}

/// Builds a canonical serialized [`PortablePackageEnvelope`] string from a payload.
///
/// # Errors
///
/// Returns [`DennettError::Internal`] if JSON serialization fails.
pub fn serialize_canonical_package(
    mut payload: PortablePackagePayload,
) -> Result<String, DennettError> {
    payload.notes.sort_by(|a, b| {
        (
            &a.title,
            &a.body,
            &a.topic_key,
            &a.tags,
            a.admission_status.as_str(),
            a.salience,
            &a.rationale,
        )
            .cmp(&(
                &b.title,
                &b.body,
                &b.topic_key,
                &b.tags,
                b.admission_status.as_str(),
                b.salience,
                &b.rationale,
            ))
    });
    payload.claims.sort_by(|a, b| {
        (
            &a.topic_key,
            &a.claim_key,
            &a.natural_language,
            a.epistemic_status.as_str(),
            a.valid_from,
            a.valid_to,
            &a.caveats,
            &a.provenance,
        )
            .cmp(&(
                &b.topic_key,
                &b.claim_key,
                &b.natural_language,
                b.epistemic_status.as_str(),
                b.valid_from,
                b.valid_to,
                &b.caveats,
                &b.provenance,
            ))
    });
    payload.documents.sort();

    let payload_val = serde_json::to_value(&payload).map_err(|err| DennettError::Internal {
        detail: format!("failed to convert portable payload to JSON value: {err}"),
    })?;
    let payload_bytes =
        canonical_json_bytes(&payload_val).map_err(|detail| DennettError::Internal { detail })?;
    let payload_sha256 = sha256_hex(&payload_bytes);

    let envelope = PortablePackageEnvelope {
        format_version: String::from(PORTABLE_FORMAT_VERSION),
        manifest_sha256: payload_sha256.as_str().to_owned(),
        payload,
        secrets_stripped: true,
    };
    let env_val = serde_json::to_value(&envelope).map_err(|err| DennettError::Internal {
        detail: format!("failed to convert portable envelope to JSON value: {err}"),
    })?;
    let env_bytes =
        canonical_json_bytes(&env_val).map_err(|detail| DennettError::Internal { detail })?;
    String::from_utf8(env_bytes).map_err(|err| DennettError::Internal {
        detail: format!("canonical portable JSON was not valid UTF-8: {err}"),
    })
}

fn canonical_json_bytes(value: &JsonValue) -> Result<Vec<u8>, String> {
    let normalized = sort_json_keys(value);
    serde_json::to_vec(&normalized).map_err(|err| format!("failed to encode canonical JSON: {err}"))
}

fn sort_json_keys(value: &JsonValue) -> JsonValue {
    match value {
        JsonValue::Object(map) => {
            let mut entries: Vec<(&String, &JsonValue)> = map.iter().collect();
            entries.sort_by_key(|(a, _)| *a);
            let mut out = serde_json::Map::with_capacity(entries.len());
            for (k, v) in entries {
                out.insert(k.clone(), sort_json_keys(v));
            }
            JsonValue::Object(out)
        }
        JsonValue::Array(items) => JsonValue::Array(items.iter().map(sort_json_keys).collect()),
        other => other.clone(),
    }
}

fn push_err(target: &mut ParsedImportPackage, msg: impl Into<String>) {
    if target.validation_errors.len() < MAX_VALIDATION_ERRORS {
        target.validation_errors.push(msg.into());
    }
}

fn validate_safe_key(value: &str, field: &str, max_len: usize) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > max_len {
        return Err(format!(
            "{field} must be non-empty and at most {max_len} bytes"
        ));
    }
    if value.bytes().any(|b| b.is_ascii_control()) {
        return Err(format!("{field} must not contain ASCII control characters"));
    }
    if has_unsafe_path_or_traversal(value) {
        return Err(format!(
            "{field} contains forbidden path traversal or unsafe path characters"
        ));
    }
    Ok(())
}

fn validate_safe_text(value: &str, field: &str, max_len: usize) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > max_len {
        return Err(format!(
            "{field} must be non-empty and at most {max_len} bytes"
        ));
    }
    if value
        .bytes()
        .any(|b| b == 0 || (b.is_ascii_control() && !matches!(b, b'\n' | b'\r' | b'\t')))
    {
        return Err(format!(
            "{field} must not contain NUL or disallowed control characters"
        ));
    }
    Ok(())
}

fn validate_imported_note(note: &ExportedNote) -> Result<(), String> {
    validate_safe_text(&note.title, "title", MAX_NOTE_TITLE_BYTES)?;
    validate_safe_text(&note.body, "body", MAX_NOTE_BODY_BYTES)?;
    if note.salience > MAX_NOTE_SALIENCE {
        return Err(format!(
            "salience {} exceeds {MAX_NOTE_SALIENCE}",
            note.salience
        ));
    }
    if let Some(ref topic) = note.topic_key {
        validate_safe_key(topic, "topic_key", MAX_TOPIC_KEY_LEN)?;
    }
    if let Some(ref rat) = note.rationale {
        validate_safe_text(rat, "rationale", MAX_RATIONALE_BYTES)?;
    }
    if note.tags.len() > MAX_TAGS_PER_NOTE {
        return Err(format!("tags exceeds maximum of {MAX_TAGS_PER_NOTE}"));
    }
    for tag in &note.tags {
        validate_safe_key(tag, "tags", MAX_TAG_LEN)?;
    }
    Ok(())
}

fn validate_imported_claim(claim: &ExportedClaim) -> Result<(), String> {
    validate_safe_key(&claim.topic_key, "topic_key", MAX_TOPIC_KEY_LEN)?;
    if let Some(ref ck) = claim.claim_key {
        validate_safe_key(ck, "claim_key", MAX_CLAIM_KEY_LEN)?;
    }
    validate_safe_text(
        &claim.natural_language,
        "natural_language",
        MAX_CLAIM_TEXT_BYTES,
    )?;
    if let Some(vf) = claim.valid_from
        && !vf.is_in_range()
    {
        return Err(String::from("valid_from timestamp is out of range"));
    }
    if let Some(vt) = claim.valid_to
        && !vt.is_in_range()
    {
        return Err(String::from("valid_to timestamp is out of range"));
    }
    if let (Some(vf), Some(vt)) = (claim.valid_from, claim.valid_to)
        && vf > vt
    {
        return Err(String::from("valid_to must be >= valid_from"));
    }
    if claim.caveats.len() > MAX_CAVEATS_PER_CLAIM {
        return Err(format!(
            "caveats exceeds maximum of {MAX_CAVEATS_PER_CLAIM}"
        ));
    }
    for caveat in &claim.caveats {
        validate_safe_text(caveat, "caveats", MAX_CAVEAT_BYTES)?;
    }
    if claim.provenance.len() > MAX_METADATA_ENTRIES {
        return Err(format!(
            "provenance exceeds maximum of {MAX_METADATA_ENTRIES} entries"
        ));
    }
    for (k, v) in &claim.provenance {
        validate_safe_key(k, "provenance.key", MAX_METADATA_KEY_LEN)?;
        if v.len() > MAX_METADATA_VALUE_LEN
            || v.bytes()
                .any(|b| b == 0 || (b.is_ascii_control() && !matches!(b, b'\n' | b'\r' | b'\t')))
        {
            return Err(String::from("provenance value invalid or oversized"));
        }
    }
    Ok(())
}

fn validate_imported_document(doc: &ExportedDocument) -> Result<(), String> {
    validate_safe_key(&doc.document_key, "document_key", MAX_DOCUMENT_KEY_LEN)?;
    validate_safe_text(&doc.title, "title", MAX_DOCUMENT_TITLE_BYTES)?;
    validate_safe_text(&doc.body, "body", MAX_DOCUMENT_BODY_BYTES)?;
    Ok(())
}

/// Exports one authorized scope as a deterministic, secret-free Contract J package.
#[expect(
    clippy::too_many_lines,
    reason = "export collects notes, claims, and dossiers for one scope, scrubs secrets, serializes canonical JSON, and audits export atomically"
)]
pub(crate) async fn execute_export_package(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    req: ExportRequest,
) -> Result<ExportResponse, DennettError> {
    require_scope_capability(principal, &req.scope, MemoryCapability::Export)?;
    if let Some(ref fmt_ver) = req.format_version
        && fmt_ver.trim() != PORTABLE_FORMAT_VERSION
    {
        return Err(DennettError::InvalidInput {
            field: String::from("format_version"),
            reason: format!(
                "unsupported format_version `{fmt_ver}`; only `{PORTABLE_FORMAT_VERSION}` is supported"
            ),
        });
    }
    svc.ensure_migrated().await?;

    let op_fut = async {
        let mut conn = svc.get_conn(svc.op_timeout()).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

        let note_rows = tx
            .query(
                "SELECT title, body, tags, rationale, admission_status, salience, topic_key
                 FROM memory_notes
                 WHERE scope = $1 AND forgotten_at IS NULL",
                &[&req.scope.as_str()],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

        let mut notes = Vec::with_capacity(note_rows.len());
        for row in note_rows {
            let raw_tags: Vec<String> =
                serde_json::from_value(row.get::<_, JsonValue>(2)).unwrap_or_default();
            let mut tags: Vec<String> = raw_tags
                .into_iter()
                .map(|t| scrub_required_export_text(&t))
                .collect();
            tags.sort();
            let raw_rationale: Option<String> = row.get(3);
            let rationale = raw_rationale.map(|r| scrub_required_export_text(&r));
            let salience_i16: i16 = row.get(5);
            let salience = u8::try_from(salience_i16.clamp(0, i16::from(MAX_NOTE_SALIENCE)))
                .unwrap_or(dennett_contracts::DEFAULT_NOTE_SALIENCE);
            let raw_topic: Option<String> = row.get(6);
            notes.push(ExportedNote {
                admission_status: row.get::<_, String>(4).parse()?,
                body: scrub_required_export_text(&row.get::<_, String>(1)),
                rationale,
                salience,
                tags,
                title: scrub_required_export_text(&row.get::<_, String>(0)),
                topic_key: raw_topic.map(|t| scrub_required_export_text(&t)),
            });
        }

        let claim_rows = tx
            .query(
                "SELECT topic_key, claim_key, natural_language, epistemic_status,
                        TRUNC(EXTRACT(EPOCH FROM valid_from) * 1000)::bigint,
                        TRUNC(EXTRACT(EPOCH FROM valid_to) * 1000)::bigint,
                        caveats, provenance
                 FROM memory_claims
                 WHERE scope = $1 AND forgotten_at IS NULL",
                &[&req.scope.as_str()],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

        let mut claims = Vec::with_capacity(claim_rows.len());
        for row in claim_rows {
            let raw_caveats: Vec<String> =
                serde_json::from_value(row.get::<_, JsonValue>(6)).unwrap_or_default();
            let caveats: Vec<String> = raw_caveats
                .into_iter()
                .map(|c| scrub_required_export_text(&c))
                .collect();
            let raw_prov: BTreeMap<String, String> =
                serde_json::from_value(row.get::<_, JsonValue>(7)).unwrap_or_default();
            let provenance: BTreeMap<String, String> = raw_prov
                .into_iter()
                .map(|(k, v)| (scrub_required_export_text(&k), scrub_export_text(&v)))
                .collect();
            let raw_ck: Option<String> = row.get(1);
            claims.push(ExportedClaim {
                caveats,
                claim_key: raw_ck.map(|k| scrub_required_export_text(&k)),
                epistemic_status: row.get::<_, String>(3).parse()?,
                natural_language: scrub_required_export_text(&row.get::<_, String>(2)),
                provenance,
                topic_key: scrub_required_export_text(&row.get::<_, String>(0)),
                valid_from: row.get::<_, Option<i64>>(4).map(TimestampMs::from_millis),
                valid_to: row.get::<_, Option<i64>>(5).map(TimestampMs::from_millis),
            });
        }

        let doc_rows = tx
            .query(
                "SELECT document_key, title, current_body
                 FROM memory_documents
                 WHERE scope = $1 AND forgotten_at IS NULL",
                &[&req.scope.as_str()],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

        let mut documents = Vec::with_capacity(doc_rows.len());
        for row in doc_rows {
            documents.push(ExportedDocument {
                body: scrub_required_export_text(&row.get::<_, String>(2)),
                document_key: scrub_required_export_text(&row.get::<_, String>(0)),
                title: scrub_required_export_text(&row.get::<_, String>(1)),
            });
        }

        let item_count = u64::try_from(notes.len() + claims.len() + documents.len()).unwrap_or(0);
        let package_json = serialize_canonical_package(PortablePackagePayload {
            claims,
            documents,
            notes,
        })?;
        let manifest_sha256 = sha256_hex(package_json.as_bytes());
        let byte_size = u64::try_from(package_json.len()).unwrap_or(0);

        let package_id = ExportPackageId::new();
        let item_count_i32 = i32::try_from(item_count).unwrap_or(i32::MAX);
        let byte_size_i64 = i64::try_from(byte_size).unwrap_or(i64::MAX);

        tx.execute(
            "INSERT INTO memory_export_packages (
                package_id, scope, manifest_sha256, format_version,
                exported_by, item_count, byte_size, secrets_stripped
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, true)",
            &[
                &package_id.as_uuid(),
                &req.scope.as_str(),
                &manifest_sha256.as_str(),
                &PORTABLE_FORMAT_VERSION,
                &principal.principal_id.as_str(),
                &item_count_i32,
                &byte_size_i64,
            ],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

        let event_id = MemoryEventId::new();
        let idem = IdempotencyKey::try_new(format!("export-{package_id}"))?;
        if let Some(stream_seq) = insert_memory_event(
            &tx,
            NewMemoryEvent {
                event_id,
                scope: &req.scope,
                kind: MemoryEventKind::PackageExported,
                principal_id: &principal.principal_id,
                correlation_id,
                idempotency_key: &idem,
                fingerprint: &manifest_sha256,
                target_entity_id: package_id.as_uuid(),
                target_revision: None,
                observed_ms: None,
            },
            svc,
        )
        .await?
        {
            maybe_advance_watermark_only(&tx, &req.scope, event_id, stream_seq, svc).await?;
        }

        tx.commit().await.map_err(|err| svc.map_pg_err(&err))?;

        Ok(ExportResponse {
            package_id,
            scope: req.scope,
            format_version: String::from(PORTABLE_FORMAT_VERSION),
            manifest_sha256,
            item_count,
            byte_size,
            secrets_stripped: true,
            package_json,
        })
    };

    time::timeout(svc.op_timeout(), op_fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("memory_service::export_package"),
        })?
}

/// Imports an external Contract J package into `req.scope` strictly as `external_untrusted`
/// data with `instruction_active = false`.
#[expect(
    clippy::too_many_lines,
    reason = "transactional import persists validated notes, claims, documents, audit row, and PackageImported event atomically"
)]
pub(crate) async fn execute_import_package(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    req: ImportRequest,
) -> Result<ImportResponse, DennettError> {
    req.validate()?;
    require_scope_capability(principal, &req.scope, MemoryCapability::Write)?;
    svc.ensure_migrated().await?;

    let source_package_sha256 = sha256_hex(req.package_json.as_bytes());
    let ParsedImportPackage {
        valid_notes,
        valid_claims,
        valid_documents,
        mut items_rejected,
        mut validation_errors,
    } = parse_and_validate_import_package(&req.package_json);

    let op_fut = async {
        let mut conn = svc.get_conn(svc.op_timeout()).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

        let import_id = ImportRecordId::new();
        let mut items_imported = 0_u64;

        // 1. Import validated notes strictly as `external_untrusted` + `instruction_active = false`.
        for note in &valid_notes {
            let note_id = MemoryNoteId::new();
            let rev_id = NoteRevisionId::new();
            let event_id = MemoryEventId::new();
            let idem = IdempotencyKey::try_new(format!("imp-note-{event_id}"))?;
            let fp = sha256_hex(format!("{}:{}", note.title, note.body).as_bytes());
            let tags_json =
                serde_json::to_value(&note.tags).map_err(|err| DennettError::Internal {
                    detail: format!("failed to serialize imported tags: {err}"),
                })?;
            let salience_i16 = i16::from(note.salience);

            let stream_seq = insert_memory_event(
                &tx,
                NewMemoryEvent {
                    event_id,
                    scope: &req.scope,
                    kind: MemoryEventKind::NoteCreated,
                    principal_id: &principal.principal_id,
                    correlation_id,
                    idempotency_key: &idem,
                    fingerprint: &fp,
                    target_entity_id: note_id.as_uuid(),
                    target_revision: Some(1),
                    observed_ms: None,
                },
                svc,
            )
            .await?
            .unwrap_or(0);

            tx.execute(
                "INSERT INTO memory_notes (
                    note_id, scope, current_revision, current_revision_id,
                    title, body, tags, rationale, last_event_id, recorded_by, author_label,
                    admission_status, salience, topic_key, trust_level, instruction_active
                 ) VALUES ($1, $2, 1, $3, $4, $5, $6, $7, $8, $9, NULL, $10, $11, $12, 'external_untrusted', false)",
                &[
                    &note_id.as_uuid(),
                    &req.scope.as_str(),
                    &rev_id.as_uuid(),
                    &note.title,
                    &note.body,
                    &tags_json,
                    &note.rationale,
                    &event_id.as_uuid(),
                    &principal.principal_id.as_str(),
                    &note.admission_status.as_str(),
                    &salience_i16,
                    &note.topic_key,
                ],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

            tx.execute(
                "INSERT INTO memory_note_revisions (
                    revision_id, note_id, revision, title, body, tags, rationale,
                    event_id, recorded_by, author_label
                 ) VALUES ($1, $2, 1, $3, $4, $5, $6, $7, $8, NULL)",
                &[
                    &rev_id.as_uuid(),
                    &note_id.as_uuid(),
                    &note.title,
                    &note.body,
                    &tags_json,
                    &note.rationale,
                    &event_id.as_uuid(),
                    &principal.principal_id.as_str(),
                ],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

            if stream_seq > 0 {
                maybe_index_note_in_tx(
                    &tx,
                    &req.scope,
                    note_id.as_uuid(),
                    note.admission_status,
                    event_id,
                    stream_seq,
                    svc,
                )
                .await?;
            }
            items_imported = items_imported.saturating_add(1);
        }

        // 2. Import validated claims strictly as `external_untrusted` + `instruction_active = false`.
        for claim in &valid_claims {
            let claim_id = ClaimId::new();
            let event_id = MemoryEventId::new();
            let idem = IdempotencyKey::try_new(format!("imp-claim-{event_id}"))?;
            let fp =
                sha256_hex(format!("{}:{}", claim.topic_key, claim.natural_language).as_bytes());

            let stream_seq = insert_memory_event(
                &tx,
                NewMemoryEvent {
                    event_id,
                    scope: &req.scope,
                    kind: MemoryEventKind::ClaimCreated,
                    principal_id: &principal.principal_id,
                    correlation_id,
                    idempotency_key: &idem,
                    fingerprint: &fp,
                    target_entity_id: claim_id.as_uuid(),
                    target_revision: None,
                    observed_ms: claim.valid_from.map(TimestampMs::as_millis),
                },
                svc,
            )
            .await?
            .unwrap_or(0);

            insert_claim_row(
                &tx,
                NewClaimRow {
                    claim_id,
                    scope: &req.scope,
                    topic_key: &claim.topic_key,
                    claim_key: claim.claim_key.as_deref(),
                    natural_language: &claim.natural_language,
                    epistemic_status: claim.epistemic_status,
                    supersedes_claim_id: None,
                    valid_from: claim.valid_from,
                    valid_to: claim.valid_to,
                    source_event_id: event_id,
                    source_note_id: None,
                    caveats: &claim.caveats,
                    provenance: &claim.provenance,
                    trust_level: TrustLevel::ExternalUntrusted,
                    instruction_active: false,
                },
                svc,
            )
            .await?;

            if stream_seq > 0 {
                maybe_index_claim_in_tx(
                    &tx,
                    &req.scope,
                    claim_id.as_uuid(),
                    event_id,
                    stream_seq,
                    svc,
                )
                .await?;
            }
            items_imported = items_imported.saturating_add(1);
        }

        // 3. Import validated documents strictly as `external_untrusted` + `instruction_active = false`.
        //    Never silently overwrite an existing active dossier with the same document_key.
        for doc in &valid_documents {
            let exists = tx
                .query_opt(
                    "SELECT 1 FROM memory_documents
                     WHERE scope = $1 AND document_key = $2 AND forgotten_at IS NULL",
                    &[&req.scope.as_str(), &doc.document_key],
                )
                .await
                .map_err(|err| svc.map_pg_err(&err))?;

            if exists.is_some() {
                items_rejected = items_rejected.saturating_add(1);
                if validation_errors.len() < MAX_VALIDATION_ERRORS {
                    validation_errors.push(format!(
                        "document `{}` already exists in scope; untrusted import will not overwrite",
                        doc.document_key
                    ));
                }
                continue;
            }

            let doc_id = DocumentId::new();
            let ver_id = DocumentVersionId::new();
            let event_id = MemoryEventId::new();
            let idem = IdempotencyKey::try_new(format!("imp-doc-{event_id}"))?;
            let fp = sha256_hex(format!("{}:{}", doc.document_key, doc.body).as_bytes());
            let empty_uuids: Vec<uuid::Uuid> = Vec::new();

            let stream_seq = insert_memory_event(
                &tx,
                NewMemoryEvent {
                    event_id,
                    scope: &req.scope,
                    kind: MemoryEventKind::DocumentCreated,
                    principal_id: &principal.principal_id,
                    correlation_id,
                    idempotency_key: &idem,
                    fingerprint: &fp,
                    target_entity_id: doc_id.as_uuid(),
                    target_revision: Some(1),
                    observed_ms: None,
                },
                svc,
            )
            .await?
            .unwrap_or(0);

            tx.execute(
                "INSERT INTO memory_documents (
                    document_id, scope, document_key, title, current_version, current_body,
                    provenance_claim_ids, provenance_note_ids, source_event_id, principal_id,
                    trust_level, instruction_active
                 ) VALUES ($1, $2, $3, $4, 1, $5, $6, $6, $7, $8, 'external_untrusted', false)",
                &[
                    &doc_id.as_uuid(),
                    &req.scope.as_str(),
                    &doc.document_key,
                    &doc.title,
                    &doc.body,
                    &empty_uuids,
                    &event_id.as_uuid(),
                    &principal.principal_id.as_str(),
                ],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

            tx.execute(
                "INSERT INTO memory_document_versions (
                    version_id, document_id, version, body, provenance_claim_ids, diff_summary, event_id
                 ) VALUES ($1, $2, 1, $3, $4, $5, $6)",
                &[
                    &ver_id.as_uuid(),
                    &doc_id.as_uuid(),
                    &doc.body,
                    &empty_uuids,
                    &Some("Imported external_untrusted package"),
                    &event_id.as_uuid(),
                ],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

            if stream_seq > 0 {
                maybe_index_document_in_tx(
                    &tx,
                    &req.scope,
                    doc_id.as_uuid(),
                    event_id,
                    stream_seq,
                    svc,
                )
                .await?;
            }
            items_imported = items_imported.saturating_add(1);
        }

        let errors_json =
            serde_json::to_value(&validation_errors).map_err(|err| DennettError::Internal {
                detail: format!("failed to serialize import validation_errors: {err}"),
            })?;
        let imported_i32 = i32::try_from(items_imported).unwrap_or(i32::MAX);
        let rejected_i32 = i32::try_from(items_rejected).unwrap_or(i32::MAX);

        tx.execute(
            "INSERT INTO memory_import_records (
                import_id, scope, source_package_sha256, trust_level, instruction_active,
                items_imported, items_rejected, validation_errors, imported_by
             ) VALUES ($1, $2, $3, 'external_untrusted', false, $4, $5, $6, $7)",
            &[
                &import_id.as_uuid(),
                &req.scope.as_str(),
                &source_package_sha256.as_str(),
                &imported_i32,
                &rejected_i32,
                &errors_json,
                &principal.principal_id.as_str(),
            ],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

        let imp_event_id = MemoryEventId::new();
        let imp_idem = IdempotencyKey::try_new(format!("import-{import_id}"))?;
        if let Some(stream_seq) = insert_memory_event(
            &tx,
            NewMemoryEvent {
                event_id: imp_event_id,
                scope: &req.scope,
                kind: MemoryEventKind::PackageImported,
                principal_id: &principal.principal_id,
                correlation_id,
                idempotency_key: &imp_idem,
                fingerprint: &source_package_sha256,
                target_entity_id: import_id.as_uuid(),
                target_revision: None,
                observed_ms: None,
            },
            svc,
        )
        .await?
        {
            maybe_advance_watermark_only(&tx, &req.scope, imp_event_id, stream_seq, svc).await?;
        }

        tx.commit().await.map_err(|err| svc.map_pg_err(&err))?;

        Ok(ImportResponse {
            import_id,
            scope: req.scope,
            source_package_sha256,
            trust_level: TrustLevel::ExternalUntrusted,
            instruction_active: false,
            items_imported,
            items_rejected,
            validation_errors,
        })
    };

    time::timeout(svc.op_timeout(), op_fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("memory_service::import_package"),
        })?
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn scrub_export_text_strips_all_secret_patterns_idempotently() {
        let raw = "tok=dnt1.eyJ0b2tlbl9pZCI6IjEifQ.AbCdEf0123456789 \
                   ctrl=dct1.payload12345.sig12345 \
                   GEMINI_API_KEY=AIzaSySecretKey123456789012345 \
                   Authorization: Bearer super_secret_bearer_token \
                   gh=ghp_1234567890abcdef1234567890 \
                   openai=sk-1234567890abcdefghij";
        let scrubbed = scrub_export_text(raw);
        for forbidden in [
            "dnt1.",
            "dct1.",
            "GEMINI_API_KEY",
            "AIza",
            "Bearer ",
            "super_secret_bearer_token",
            "ghp_",
            "sk-123456",
        ] {
            assert!(
                !scrubbed.contains(forbidden),
                "forbidden secret marker `{forbidden}` leaked into export: {scrubbed}"
            );
        }
        assert_eq!(
            scrub_export_text(&scrubbed),
            scrubbed,
            "export secret scrubbing must be strictly idempotent"
        );
    }

    #[test]
    fn import_parser_rejects_excessive_depth_oversized_traversal_and_tampered_manifest() {
        let deep = format!("{}{}", "[".repeat(40), "]".repeat(40));
        let res_deep = parse_and_validate_import_package(&deep);
        assert!(res_deep.items_rejected > 0);
        assert_ne!(res_deep.validation_errors, Vec::<String>::new());

        let tampered_payload = PortablePackagePayload {
            notes: vec![ExportedNote {
                admission_status: AdmissionStatus::Admitted,
                body: String::from("ok body"),
                rationale: None,
                salience: 50,
                tags: vec![],
                title: String::from("ok title"),
                topic_key: None,
            }],
            claims: vec![],
            documents: vec![],
        };
        let valid_json = serialize_canonical_package(tampered_payload).expect("serialize");
        let mutated = valid_json.replace("ok body", "tampered body");
        let res_tampered = parse_and_validate_import_package(&mutated);
        assert_eq!(res_tampered.valid_notes.len(), 0);
        assert!(res_tampered.items_rejected > 0);

        // Path traversal inside document_key with valid manifest must reject that document cleanly.
        let bad_doc_payload = PortablePackagePayload {
            notes: vec![],
            claims: vec![],
            documents: vec![ExportedDocument {
                body: String::from("content"),
                document_key: String::from("../../etc/passwd"),
                title: String::from("evil"),
            }],
        };
        let bad_doc_json = serialize_canonical_package(bad_doc_payload).expect("serialize");
        let res_bad_doc = parse_and_validate_import_package(&bad_doc_json);
        assert_eq!(res_bad_doc.valid_documents.len(), 0);
        assert_eq!(res_bad_doc.items_rejected, 1);
        assert_ne!(res_bad_doc.validation_errors, Vec::<String>::new());
    }

    proptest! {
        #[test]
        fn proptest_import_parser_never_panics_on_arbitrary_input(raw in "\\PC{0,1024}") {
            let parsed = parse_and_validate_import_package(&raw);
            // Arbitrary non-package strings must either parse cleanly or reject safely.
            prop_assert!(
                parsed.items_rejected > 0
                    || !parsed.valid_notes.is_empty()
                    || !parsed.valid_claims.is_empty()
                    || !parsed.valid_documents.is_empty()
                    || parsed.validation_errors.is_empty()
            );
        }

        #[test]
        fn proptest_canonical_package_roundtrip_is_sha256_identical(
            title in "[A-Za-z0-9 _-]{1,40}",
            body in "[A-Za-z0-9 _.,-]{1,80}",
            topic in "[a-z0-9-]{1,20}",
            claim_text in "[A-Za-z0-9 _.,-]{1,80}",
            caveat in "[A-Za-z0-9 _.,-]{1,40}",
        ) {
            prop_assume!(!title.trim().is_empty() && !body.trim().is_empty() && !claim_text.trim().is_empty() && !caveat.trim().is_empty());
            let payload = PortablePackagePayload {
                notes: vec![ExportedNote {
                    admission_status: AdmissionStatus::Admitted,
                    body: scrub_required_export_text(&body),
                    rationale: None,
                    salience: 50,
                    tags: vec![String::from("tag-a")],
                    title: scrub_required_export_text(&title),
                    topic_key: Some(topic.clone()),
                }],
                claims: vec![ExportedClaim {
                    caveats: vec![scrub_required_export_text(&caveat)],
                    claim_key: None,
                    epistemic_status: EpistemicStatus::Asserted,
                    natural_language: scrub_required_export_text(&claim_text),
                    provenance: BTreeMap::new(),
                    topic_key: topic.clone(),
                    valid_from: None,
                    valid_to: None,
                }],
                documents: vec![ExportedDocument {
                    body: scrub_required_export_text(&body),
                    document_key: format!("dossier:{topic}"),
                    title: scrub_required_export_text(&title),
                }],
            };

            let json1 = serialize_canonical_package(payload).expect("serialize 1");
            let parsed = parse_and_validate_import_package(&json1);
            prop_assert_eq!(parsed.items_rejected, 0);
            prop_assert_eq!(parsed.valid_notes.len(), 1);
            prop_assert_eq!(parsed.valid_claims.len(), 1);
            prop_assert_eq!(parsed.valid_documents.len(), 1);

            let json2 = serialize_canonical_package(PortablePackagePayload {
                notes: parsed.valid_notes,
                claims: parsed.valid_claims,
                documents: parsed.valid_documents,
            }).expect("serialize 2");
            prop_assert_eq!(sha256_hex(json1.as_bytes()), sha256_hex(json2.as_bytes()));
        }
    }
}
