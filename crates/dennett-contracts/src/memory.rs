//! Pure domain contracts for Dennett standalone Memory Fabric (`dennett-memoryd`).
//!
//! All identifiers, scopes, anchors, requests, and responses stay strictly independent
//! of SQL drivers, HTTP stacks, and UI frameworks so `dennett-memoryd`, `dennett-memory-client`,
//! `dennett-memory-mcp`, and future Head services share one canonical wire contract.

use core::fmt;
use core::str::FromStr;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use crate::{CorrelationId, DennettError};

/// Maximum permitted byte length of a [`MemoryScope`].
pub const MAX_SCOPE_LEN: usize = 128;
/// Maximum permitted byte length of a [`PrincipalId`].
pub const MAX_PRINCIPAL_ID_LEN: usize = 128;
/// Maximum permitted byte length of an [`IdempotencyKey`].
pub const MAX_IDEMPOTENCY_KEY_LEN: usize = 128;
/// Maximum permitted byte length of an [`EvidenceRelationship`].
pub const MAX_RELATIONSHIP_LEN: usize = 64;
/// Exact ASCII byte length of a lowercase hex SHA-256 digest.
pub const SHA256_HEX_LEN: usize = 64;
/// Maximum permitted byte length of an evidence MIME / media type.
pub const MAX_MEDIA_TYPE_LEN: usize = 128;
/// Maximum inline evidence payload size (256 KiB) accepted directly by the Memory API.
pub const MAX_EVIDENCE_CONTENT_BYTES: usize = 256 * 1024;
/// Maximum permitted byte length of an evidence provenance source URI.
pub const MAX_SOURCE_URI_LEN: usize = 1024;
/// Maximum permitted byte length of a memory note title.
pub const MAX_NOTE_TITLE_BYTES: usize = 256;
/// Maximum permitted byte length of a memory note body (128 KiB).
pub const MAX_NOTE_BODY_BYTES: usize = 128 * 1024;
/// Maximum permitted byte length of a note correction rationale.
pub const MAX_RATIONALE_BYTES: usize = 4 * 1024;
/// Maximum permitted byte length of an untrusted caller-supplied author display label.
pub const MAX_AUTHOR_LABEL_BYTES: usize = 128;
/// Maximum permitted byte length of a single note tag.
pub const MAX_TAG_LEN: usize = 64;
/// Maximum number of tags attached to a single note revision.
pub const MAX_TAGS_PER_NOTE: usize = 32;
/// Maximum number of evidence links attached to a single memory event.
pub const MAX_LINKS_PER_EVENT: usize = 64;
/// Maximum number of metadata entries on an evidence object.
pub const MAX_METADATA_ENTRIES: usize = 64;
/// Maximum byte length of a metadata key.
pub const MAX_METADATA_KEY_LEN: usize = 128;
/// Maximum byte length of a metadata value.
pub const MAX_METADATA_VALUE_LEN: usize = 1024;
/// Maximum byte length of an [`EvidenceAnchor`] kind label.
pub const MAX_ANCHOR_KIND_LEN: usize = 64;
/// Maximum byte length of an [`EvidenceAnchor`] locator string.
pub const MAX_ANCHOR_LOCATOR_LEN: usize = 1024;
/// Maximum number of extra attributes on an [`EvidenceAnchor`].
pub const MAX_ANCHOR_ATTRIBUTES: usize = 32;
/// Default number of notes returned by [`SearchNotesRequest`] when `limit` is omitted.
pub const DEFAULT_SEARCH_LIMIT: u32 = 20;
/// Maximum number of notes returned by a single [`SearchNotesRequest`].
pub const MAX_SEARCH_LIMIT: u32 = 100;
/// Maximum byte length of a text search query.
pub const MAX_SEARCH_QUERY_BYTES: usize = 512;
/// Minimum valid UTC timestamp in milliseconds (`0001-01-01T00:00:00Z`), safely within PostgreSQL `TIMESTAMPTZ`.
pub const MIN_TIMESTAMP_MS: i64 = -62_135_596_800_000;
/// Maximum valid UTC timestamp in milliseconds (`9999-12-31T23:59:59.999Z`), safely within PostgreSQL `TIMESTAMPTZ`.
pub const MAX_TIMESTAMP_MS: i64 = 253_402_300_799_999;

macro_rules! define_uuid_id {
    (
        $(#[$meta:meta])*
        pub struct $name:ident;
        field = $field:literal;
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        #[repr(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Generates a fresh random UUID v4 identifier.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Wraps an existing [`Uuid`] into this typed identifier.
            #[must_use]
            pub const fn from_uuid(id: Uuid) -> Self {
                Self(id)
            }

            /// Returns the underlying [`Uuid`].
            #[must_use]
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = DennettError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(s).map(Self).map_err(|_| DennettError::InvalidInput {
                    field: String::from($field),
                    reason: String::from("must be a valid UUID"),
                })
            }
        }
    };
}

define_uuid_id! {
    /// Stable UUID identifying an immutable [`EvidenceObject`].
    pub struct EvidenceId;
    field = "evidence_id";
}

define_uuid_id! {
    /// Stable UUID identifying an append-only [`MemoryEvent`].
    pub struct MemoryEventId;
    field = "event_id";
}

define_uuid_id! {
    /// Stable UUID identifying a single [`EventEvidenceLink`] row (Architecture 81 §3.3 / V19).
    pub struct EvidenceLinkId;
    field = "link_id";
}

define_uuid_id! {
    /// Stable UUID identifying a logical [`MemoryNote`] across revisions.
    pub struct MemoryNoteId;
    field = "note_id";
}

define_uuid_id! {
    /// Stable UUID identifying an immutable [`NoteRevision`] snapshot.
    pub struct NoteRevisionId;
    field = "revision_id";
}

/// UTC timestamp stored as signed milliseconds since Unix epoch (`1970-01-01T00:00:00Z`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct TimestampMs(i64);

impl TimestampMs {
    /// Captures the current system wall-clock time in UTC milliseconds.
    #[must_use]
    pub fn now() -> Self {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
        Self(millis)
    }

    /// Constructs a [`TimestampMs`] from signed Unix epoch milliseconds.
    #[must_use]
    pub const fn from_millis(millis: i64) -> Self {
        Self(millis)
    }

    /// Returns the signed Unix epoch millisecond value.
    #[must_use]
    pub const fn as_millis(self) -> i64 {
        self.0
    }

    /// Returns `true` if the timestamp lies within the valid canonical range
    /// `[MIN_TIMESTAMP_MS, MAX_TIMESTAMP_MS]` (`0001-01-01T00:00:00Z` through `9999-12-31T23:59:59.999Z`).
    #[must_use]
    pub const fn is_in_range(self) -> bool {
        self.0 >= MIN_TIMESTAMP_MS && self.0 <= MAX_TIMESTAMP_MS
    }

    /// Validates that this timestamp fits safely in canonical PostgreSQL `TIMESTAMPTZ` range.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if outside `[MIN_TIMESTAMP_MS, MAX_TIMESTAMP_MS]`.
    pub fn validate(self, field: &str) -> Result<(), DennettError> {
        if !self.is_in_range() {
            return Err(DennettError::InvalidInput {
                field: field.to_owned(),
                reason: format!(
                    "timestamp {} ms is outside supported range [{MIN_TIMESTAMP_MS}, {MAX_TIMESTAMP_MS}]",
                    self.0
                ),
            });
        }
        Ok(())
    }
}

impl fmt::Display for TimestampMs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Validated memory authorization and isolation scope (e.g. `"project-alpha"`, `"personal:notes"`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct MemoryScope(String);

impl MemoryScope {
    /// Validates and constructs a [`MemoryScope`].
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `value` is empty, exceeds [`MAX_SCOPE_LEN`],
    /// contains disallowed characters or traversal sequences (`..`, `//`, `::`), or has
    /// leading/trailing separator punctuation.
    pub fn try_new(value: impl Into<String>) -> Result<Self, DennettError> {
        let candidate = value.into();
        validate_scope(&candidate)?;
        Ok(Self(candidate))
    }

    /// Borrows the validated scope string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MemoryScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for MemoryScope {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_new(s)
    }
}

impl TryFrom<String> for MemoryScope {
    type Error = DennettError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl<'de> Deserialize<'de> for MemoryScope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::try_new(raw).map_err(serde::de::Error::custom)
    }
}

fn validate_scope(candidate: &str) -> Result<(), DennettError> {
    if candidate.is_empty() {
        return Err(DennettError::InvalidInput {
            field: String::from("scope"),
            reason: String::from("must not be empty"),
        });
    }
    if candidate.len() > MAX_SCOPE_LEN {
        return Err(DennettError::InvalidInput {
            field: String::from("scope"),
            reason: format!("exceeds maximum length of {MAX_SCOPE_LEN} bytes"),
        });
    }
    let bytes = candidate.as_bytes();
    if !bytes
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':' | b'/'))
    {
        return Err(DennettError::InvalidInput {
            field: String::from("scope"),
            reason: String::from(
                "must contain only ASCII alphanumeric characters, '-', '_', '.', ':', or '/'",
            ),
        });
    }
    if matches!(bytes.first(), Some(b'/' | b':' | b'.'))
        || matches!(bytes.last(), Some(b'/' | b':' | b'.'))
        || candidate.contains("..")
        || candidate.contains("//")
        || candidate.contains("::")
    {
        return Err(DennettError::InvalidInput {
            field: String::from("scope"),
            reason: String::from(
                "must not start/end with '/', ':', '.' or contain '..', '//', or '::'",
            ),
        });
    }
    Ok(())
}

/// Validated principal identifier verified from a cryptographic or registry credential.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct PrincipalId(String);

impl PrincipalId {
    /// Validates and constructs a [`PrincipalId`].
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `value` is empty, exceeds
    /// [`MAX_PRINCIPAL_ID_LEN`], or contains whitespace, control characters, or disallowed punctuation.
    pub fn try_new(value: impl Into<String>) -> Result<Self, DennettError> {
        let candidate = value.into();
        if candidate.is_empty() {
            return Err(DennettError::InvalidInput {
                field: String::from("principal_id"),
                reason: String::from("must not be empty"),
            });
        }
        if candidate.len() > MAX_PRINCIPAL_ID_LEN {
            return Err(DennettError::InvalidInput {
                field: String::from("principal_id"),
                reason: format!("exceeds maximum length of {MAX_PRINCIPAL_ID_LEN} bytes"),
            });
        }
        if !candidate.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':' | b'@' | b'/')
        }) || candidate.contains("..")
        {
            return Err(DennettError::InvalidInput {
                field: String::from("principal_id"),
                reason: String::from(
                    "must contain only ASCII alphanumeric characters, '-', '_', '.', ':', '@', or '/' without '..'",
                ),
            });
        }
        Ok(Self(candidate))
    }

    /// Borrows the validated principal identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PrincipalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for PrincipalId {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_new(s)
    }
}

impl TryFrom<String> for PrincipalId {
    type Error = DennettError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl<'de> Deserialize<'de> for PrincipalId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::try_new(raw).map_err(serde::de::Error::custom)
    }
}

/// Validated caller-supplied idempotency key guarding against duplicate mutations.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    /// Generates a fresh UUID-backed [`IdempotencyKey`].
    #[must_use]
    pub fn generate() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Validates and constructs an [`IdempotencyKey`].
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `value` is empty, exceeds
    /// [`MAX_IDEMPOTENCY_KEY_LEN`], or contains disallowed characters.
    pub fn try_new(value: impl Into<String>) -> Result<Self, DennettError> {
        let candidate = value.into();
        if candidate.is_empty() {
            return Err(DennettError::InvalidInput {
                field: String::from("idempotency_key"),
                reason: String::from("must not be empty"),
            });
        }
        if candidate.len() > MAX_IDEMPOTENCY_KEY_LEN {
            return Err(DennettError::InvalidInput {
                field: String::from("idempotency_key"),
                reason: format!("exceeds maximum length of {MAX_IDEMPOTENCY_KEY_LEN} bytes"),
            });
        }
        if !candidate
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
        {
            return Err(DennettError::InvalidInput {
                field: String::from("idempotency_key"),
                reason: String::from(
                    "must contain only ASCII alphanumeric characters, '-', '_', '.', or ':'",
                ),
            });
        }
        Ok(Self(candidate))
    }

    /// Borrows the validated idempotency key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IdempotencyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for IdempotencyKey {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_new(s)
    }
}

impl TryFrom<String> for IdempotencyKey {
    type Error = DennettError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl<'de> Deserialize<'de> for IdempotencyKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::try_new(raw).map_err(serde::de::Error::custom)
    }
}

/// Validated lowercase hexadecimal SHA-256 digest (64 ASCII hex digits).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Sha256Hex(String);

impl Sha256Hex {
    /// Encodes a raw 32-byte SHA-256 digest into a guaranteed-valid lowercase [`Sha256Hex`].
    #[must_use]
    pub fn from_digest_bytes(digest: [u8; 32]) -> Self {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(SHA256_HEX_LEN);
        for byte in digest {
            out.push(char::from(HEX[usize::from(byte >> 4)]));
            out.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        Self(out)
    }

    /// Validates and constructs a lowercase 64-character [`Sha256Hex`].
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `value` is not exactly 64 lowercase hex digits.
    pub fn try_new(value: impl Into<String>) -> Result<Self, DennettError> {
        let candidate = value.into();
        if candidate.len() != SHA256_HEX_LEN
            || !candidate
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        {
            return Err(DennettError::InvalidInput {
                field: String::from("sha256"),
                reason: format!(
                    "must be exactly {SHA256_HEX_LEN} lowercase hexadecimal characters"
                ),
            });
        }
        Ok(Self(candidate))
    }

    /// Borrows the validated lowercase hex string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Sha256Hex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Sha256Hex {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_new(s)
    }
}

impl<'de> Deserialize<'de> for Sha256Hex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::try_new(raw).map_err(serde::de::Error::custom)
    }
}

/// Validated relationship label linking a [`MemoryEvent`] to an [`EvidenceObject`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct EvidenceRelationship(String);

impl EvidenceRelationship {
    /// Validates and constructs an [`EvidenceRelationship`].
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `value` is empty, exceeds
    /// [`MAX_RELATIONSHIP_LEN`], or contains disallowed characters.
    pub fn try_new(value: impl Into<String>) -> Result<Self, DennettError> {
        let candidate = value.into();
        if candidate.is_empty() {
            return Err(DennettError::InvalidInput {
                field: String::from("relationship"),
                reason: String::from("must not be empty"),
            });
        }
        if candidate.len() > MAX_RELATIONSHIP_LEN {
            return Err(DennettError::InvalidInput {
                field: String::from("relationship"),
                reason: format!("exceeds maximum length of {MAX_RELATIONSHIP_LEN} bytes"),
            });
        }
        if !candidate
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':'))
        {
            return Err(DennettError::InvalidInput {
                field: String::from("relationship"),
                reason: String::from(
                    "must contain only ASCII alphanumeric characters, '-', '_', or ':'",
                ),
            });
        }
        Ok(Self(candidate))
    }

    /// Borrows the validated relationship string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EvidenceRelationship {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for EvidenceRelationship {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_new(s)
    }
}

impl<'de> Deserialize<'de> for EvidenceRelationship {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::try_new(raw).map_err(serde::de::Error::custom)
    }
}

/// Capability granted to an authenticated [`AuthPrincipal`] within its allowed scopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MemoryCapability {
    /// Allows reading notes, history, events, evidence, and scoped search.
    Read,
    /// Allows ingesting evidence, creating notes, and appending corrections.
    Write,
}

impl MemoryCapability {
    /// Returns the canonical snake-case capability label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }
}

impl fmt::Display for MemoryCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Authenticated principal verified by `AuthVerifier` from a bearer credential.
///
/// Never carries raw secret material; `token_id` is a non-secret audit/revocation identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthPrincipal {
    /// Verified principal identity recorded on events and revisions.
    pub principal_id: PrincipalId,
    /// Trusted issuer identifier (e.g. `"dennett-standalone"`).
    pub issuer: String,
    /// Non-secret token identifier used for revocation and audit correlation.
    pub token_id: String,
    /// Explicit allowlist of memory scopes this principal may access.
    pub allowed_scopes: BTreeSet<MemoryScope>,
    /// Explicit set of capabilities granted within `allowed_scopes`.
    pub capabilities: BTreeSet<MemoryCapability>,
    /// Optional UTC expiry timestamp in epoch milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<TimestampMs>,
}

impl AuthPrincipal {
    /// Checks whether this principal holds `capability`.
    #[must_use]
    pub fn has_capability(&self, capability: MemoryCapability) -> bool {
        self.capabilities.contains(&capability)
    }

    /// Checks whether this principal may perform `capability` within `scope`.
    #[must_use]
    pub fn can_access(&self, scope: &MemoryScope, capability: MemoryCapability) -> bool {
        self.has_capability(capability) && self.allowed_scopes.contains(scope)
    }

    /// Returns `true` if the principal's credential has expired at or before `now`.
    #[must_use]
    pub fn is_expired_at(&self, now: TimestampMs) -> bool {
        self.expires_at.is_some_and(|exp| now >= exp)
    }
}

/// Structured provenance anchor locating a span or fragment inside an [`EvidenceObject`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceAnchor {
    /// Anchor category (e.g. `"line_range"`, `"page"`, `"time_range"`, `"git_fragment"`).
    pub kind: String,
    /// Optional start offset (line number, byte offset, page, or timestamp ms).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<u64>,
    /// Optional inclusive end offset (`>= start` when both are provided).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<u64>,
    /// Optional human- or machine-readable locator (heading, selector, git path, excerpt).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<String>,
    /// Optional deterministic extra anchor attributes.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, String>,
}

impl EvidenceAnchor {
    /// Validates anchor bounds and invariants.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `kind` is empty/too long, `start > end`,
    /// `locator` exceeds [`MAX_ANCHOR_LOCATOR_LEN`], `attributes` exceed configured limits,
    /// or any string contains NUL (`\0`) bytes.
    pub fn validate(&self) -> Result<(), DennettError> {
        reject_nul(&self.kind, "anchor.kind")?;
        let kind = self.kind.trim();
        if kind.is_empty() || self.kind.len() > MAX_ANCHOR_KIND_LEN {
            return Err(DennettError::InvalidInput {
                field: String::from("anchor.kind"),
                reason: format!("must be non-empty and at most {MAX_ANCHOR_KIND_LEN} bytes"),
            });
        }
        if let (Some(start), Some(end)) = (self.start, self.end)
            && start > end
        {
            return Err(DennettError::InvalidInput {
                field: String::from("anchor.end"),
                reason: format!("end ({end}) must be greater than or equal to start ({start})"),
            });
        }
        if let Some(locator) = &self.locator {
            reject_nul(locator, "anchor.locator")?;
            if locator.len() > MAX_ANCHOR_LOCATOR_LEN {
                return Err(DennettError::InvalidInput {
                    field: String::from("anchor.locator"),
                    reason: format!("exceeds maximum length of {MAX_ANCHOR_LOCATOR_LEN} bytes"),
                });
            }
        }
        if self.attributes.len() > MAX_ANCHOR_ATTRIBUTES {
            return Err(DennettError::InvalidInput {
                field: String::from("anchor.attributes"),
                reason: format!("exceeds maximum of {MAX_ANCHOR_ATTRIBUTES} attributes"),
            });
        }
        for (key, value) in &self.attributes {
            reject_nul(key, "anchor.attributes")?;
            reject_nul(value, "anchor.attributes")?;
            if key.is_empty() || key.len() > MAX_METADATA_KEY_LEN {
                return Err(DennettError::InvalidInput {
                    field: String::from("anchor.attributes"),
                    reason: format!(
                        "attribute key must be non-empty and at most {MAX_METADATA_KEY_LEN} bytes"
                    ),
                });
            }
            if value.len() > MAX_METADATA_VALUE_LEN {
                return Err(DennettError::InvalidInput {
                    field: String::from("anchor.attributes"),
                    reason: format!(
                        "attribute value exceeds maximum length of {MAX_METADATA_VALUE_LEN} bytes"
                    ),
                });
            }
        }
        Ok(())
    }
}

/// Caller input describing one link between a memory event and an [`EvidenceObject`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceLinkInput {
    /// Target evidence object identifier (must reside in the same [`MemoryScope`]).
    pub evidence_id: EvidenceId,
    /// Semantic link relationship (e.g. `"supports"`, `"excerpt"`, `"correction_basis"`).
    pub relationship: EvidenceRelationship,
    /// Optional structured anchor inside the evidence object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<EvidenceAnchor>,
    /// Optional non-negative ordering hint; `None` is explicitly valid per V19 / Architecture 81 §3.3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ordinal: Option<i32>,
}

impl EvidenceLinkInput {
    /// Validates link invariants (`ordinal >= 0` when present and nested anchor bounds).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `ordinal` is negative or `anchor` is invalid.
    pub fn validate(&self) -> Result<(), DennettError> {
        if let Some(ord) = self.ordinal
            && ord < 0
        {
            return Err(DennettError::InvalidInput {
                field: String::from("evidence_links.ordinal"),
                reason: String::from("ordinal must be non-negative when present"),
            });
        }
        if let Some(anchor) = &self.anchor {
            anchor.validate()?;
        }
        Ok(())
    }
}

/// Stored row from `event_evidence_links` (Architecture 81 §3.3 / V19).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEvidenceLink {
    /// Stable primary-key identifier of this specific evidence link.
    pub link_id: EvidenceLinkId,
    /// Owning append-only memory event identifier.
    pub event_id: MemoryEventId,
    /// Referenced evidence object identifier.
    pub evidence_id: EvidenceId,
    /// Relationship label between event and evidence.
    pub relationship: EvidenceRelationship,
    /// Optional structured anchor inside the evidence object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<EvidenceAnchor>,
    /// Optional non-negative position hint (nullable and excluded from primary key per V19).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ordinal: Option<i32>,
    /// Positive revision number of the link (`>= 1`).
    pub revision: i64,
}

/// Request to ingest an immutable inline [`EvidenceObject`] into a [`MemoryScope`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestEvidenceRequest {
    /// Scope owning the ingested evidence.
    pub scope: MemoryScope,
    /// Media/MIME type describing `content` (e.g. `"text/plain"`, `"text/markdown"`).
    pub media_type: String,
    /// Inline UTF-8 content (bounded by [`MAX_EVIDENCE_CONTENT_BYTES`]).
    pub content: String,
    /// Optional provenance URI, file path, or external locator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_uri: Option<String>,
    /// Optional time the source was originally observed; omitted when unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<TimestampMs>,
    /// Deterministic string metadata map.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
    /// Caller idempotency key preventing duplicate ingestion on retries.
    pub idempotency_key: IdempotencyKey,
}

impl IngestEvidenceRequest {
    /// Validates request size and field invariants before hitting PostgreSQL.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] on empty, oversized, NUL-containing, or
    /// out-of-range timestamp fields.
    pub fn validate(&self) -> Result<(), DennettError> {
        reject_nul(&self.media_type, "media_type")?;
        if self.media_type.trim().is_empty() || self.media_type.len() > MAX_MEDIA_TYPE_LEN {
            return Err(DennettError::InvalidInput {
                field: String::from("media_type"),
                reason: format!("must be non-empty and at most {MAX_MEDIA_TYPE_LEN} bytes"),
            });
        }
        reject_nul(&self.content, "content")?;
        if self.content.is_empty() || self.content.len() > MAX_EVIDENCE_CONTENT_BYTES {
            return Err(DennettError::InvalidInput {
                field: String::from("content"),
                reason: format!("must be non-empty and at most {MAX_EVIDENCE_CONTENT_BYTES} bytes"),
            });
        }
        if let Some(uri) = &self.source_uri {
            reject_nul(uri, "source_uri")?;
            if uri.len() > MAX_SOURCE_URI_LEN {
                return Err(DennettError::InvalidInput {
                    field: String::from("source_uri"),
                    reason: format!("exceeds maximum length of {MAX_SOURCE_URI_LEN} bytes"),
                });
            }
        }
        if let Some(ts) = self.observed_at {
            ts.validate("observed_at")?;
        }
        validate_metadata_map(&self.metadata)?;
        Ok(())
    }
}

/// Stored immutable evidence object returned by the Memory API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceObject {
    /// Stable identifier of this evidence object.
    pub evidence_id: EvidenceId,
    /// Scope isolating this evidence object.
    pub scope: MemoryScope,
    /// Lowercase hex SHA-256 hash of `content`.
    pub sha256: Sha256Hex,
    /// Media/MIME type of `content`.
    pub media_type: String,
    /// Exact UTF-8 byte length of `content`.
    pub byte_len: u64,
    /// Stored UTF-8 content.
    pub content: String,
    /// Optional provenance URI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_uri: Option<String>,
    /// Optional observed timestamp when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<TimestampMs>,
    /// Server-assigned ingestion timestamp.
    pub ingested_at: TimestampMs,
    /// Verified principal who ingested this evidence.
    pub ingested_by: PrincipalId,
    /// Deterministic string metadata map.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

/// Response returned after ingesting (or idempotently replaying) an evidence object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestEvidenceResponse {
    /// Persisted evidence object.
    pub evidence: EvidenceObject,
    /// Append-only memory event recording the ingestion.
    pub event_id: MemoryEventId,
    /// `true` when this response was served from an existing matching idempotency record.
    pub idempotent_replay: bool,
}

/// Request to create a new revision-1 [`MemoryNote`] in a [`MemoryScope`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateNoteRequest {
    /// Scope owning the note.
    pub scope: MemoryScope,
    /// Concise human-readable title.
    pub title: String,
    /// Full editable note content.
    pub body: String,
    /// Optional tags/facets associated with the note.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Optional observed timestamp when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<TimestampMs>,
    /// Optional links to supporting/contextual evidence objects in the same scope.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_links: Vec<EvidenceLinkInput>,
    /// Optional untrusted display label (for example CLI `--agent`); never used for authorization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_label: Option<String>,
    /// Caller idempotency key preventing duplicate creation on retries.
    pub idempotency_key: IdempotencyKey,
}

impl CreateNoteRequest {
    /// Validates note creation fields, tags, observed timestamp, and evidence link inputs.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] on empty/oversized/NUL-containing fields,
    /// out-of-range timestamps, or invalid links.
    pub fn validate(&self) -> Result<(), DennettError> {
        if let Some(ts) = self.observed_at {
            ts.validate("observed_at")?;
        }
        validate_note_fields(
            &self.title,
            &self.body,
            &self.tags,
            self.author_label.as_deref(),
            &self.evidence_links,
        )
    }
}

/// Request to append an immutable correction revision to an existing [`MemoryNote`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrectNoteRequest {
    /// Logical note identifier to correct.
    pub note_id: MemoryNoteId,
    /// Expected owning scope of the note (verified against both principal and stored row).
    pub scope: MemoryScope,
    /// Optimistic concurrency revision expected on `memory_notes.current_revision`.
    pub expected_revision: i64,
    /// Updated title for the new current revision.
    pub title: String,
    /// Updated body for the new current revision.
    pub body: String,
    /// Updated tags/facets for the new current revision.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Required human-readable rationale explaining why the note was corrected.
    pub rationale: String,
    /// Optional evidence links supporting the correction.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_links: Vec<EvidenceLinkInput>,
    /// Optional untrusted display label (for example CLI `--agent`); never used for authorization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_label: Option<String>,
    /// Caller idempotency key preventing duplicate corrections on retries.
    pub idempotency_key: IdempotencyKey,
}

impl CorrectNoteRequest {
    /// Validates note correction fields, optimistic revision bound, rationale, and links.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `expected_revision < 1`, rationale is empty,
    /// or any note/link field fails validation.
    pub fn validate(&self) -> Result<(), DennettError> {
        if self.expected_revision < 1 {
            return Err(DennettError::InvalidInput {
                field: String::from("expected_revision"),
                reason: String::from("must be at least 1"),
            });
        }
        reject_nul(&self.rationale, "rationale")?;
        if self.rationale.trim().is_empty() || self.rationale.len() > MAX_RATIONALE_BYTES {
            return Err(DennettError::InvalidInput {
                field: String::from("rationale"),
                reason: format!("must be non-empty and at most {MAX_RATIONALE_BYTES} bytes"),
            });
        }
        validate_note_fields(
            &self.title,
            &self.body,
            &self.tags,
            self.author_label.as_deref(),
            &self.evidence_links,
        )
    }
}

/// Current-state projection of a [`MemoryNote`] together with its latest evidence links.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryNote {
    /// Stable logical identifier of this note.
    pub note_id: MemoryNoteId,
    /// Scope owning this note.
    pub scope: MemoryScope,
    /// Current monotonically increasing revision number (`>= 1`).
    pub current_revision: i64,
    /// Identifier of the immutable [`NoteRevision`] currently active.
    pub current_revision_id: NoteRevisionId,
    /// Current note title.
    pub title: String,
    /// Current note body.
    pub body: String,
    /// Current tags/facets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Rationale for the latest revision (`None` on revision 1, `Some` on corrections).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    /// Append-only memory event that produced `current_revision`.
    pub last_event_id: MemoryEventId,
    /// Verified principal who wrote `current_revision`.
    pub recorded_by: PrincipalId,
    /// Optional untrusted caller display label recorded alongside the revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_label: Option<String>,
    /// Timestamp when revision 1 was created.
    pub created_at: TimestampMs,
    /// Timestamp when `current_revision` was committed.
    pub updated_at: TimestampMs,
    /// Evidence links attached to `last_event_id`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_links: Vec<EventEvidenceLink>,
}

/// One immutable historical version of a [`MemoryNote`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteRevision {
    /// Stable identifier of this revision row.
    pub revision_id: NoteRevisionId,
    /// Parent logical note identifier.
    pub note_id: MemoryNoteId,
    /// Positive sequential revision number (`1`, `2`, ...).
    pub revision: i64,
    /// Title captured at this revision.
    pub title: String,
    /// Body captured at this revision.
    pub body: String,
    /// Tags captured at this revision.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Rationale supplied for this revision (`None` for revision 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    /// Append-only memory event that created this revision.
    pub event_id: MemoryEventId,
    /// Verified principal who recorded this revision.
    pub recorded_by: PrincipalId,
    /// Optional untrusted caller display label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_label: Option<String>,
    /// Timestamp when this revision was committed.
    pub created_at: TimestampMs,
    /// Evidence links attached to `event_id`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_links: Vec<EventEvidenceLink>,
}

/// Response returned after creating or correcting a [`MemoryNote`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteMutationResponse {
    /// Resulting note snapshot for the created or replayed revision.
    pub note: MemoryNote,
    /// Append-only event recording this mutation.
    pub event_id: MemoryEventId,
    /// `true` when this response was served from an existing matching idempotency record.
    pub idempotent_replay: bool,
}

/// Full immutable revision history of a [`MemoryNote`], ordered from revision 1 upward.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteHistoryResponse {
    /// Logical note identifier.
    pub note_id: MemoryNoteId,
    /// Scope owning this note.
    pub scope: MemoryScope,
    /// Latest committed revision number on the note.
    pub current_revision: i64,
    /// Ordered list of every immutable revision (`1..=current_revision`).
    pub revisions: Vec<NoteRevision>,
}

/// Scoped search / listing query over current [`MemoryNote`] projections.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchNotesRequest {
    /// Optional scope filter. When omitted, search is bounded strictly to the principal's allowed scopes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<MemoryScope>,
    /// Optional free-text search query matching titles, bodies, and tags.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// Optional maximum number of results (`1..=MAX_SEARCH_LIMIT`, defaults to [`DEFAULT_SEARCH_LIMIT`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

impl SearchNotesRequest {
    /// Validates search query length and result limit bounds, returning the effective limit.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `limit` is 0 or exceeds [`MAX_SEARCH_LIMIT`],
    /// or if `query` exceeds [`MAX_SEARCH_QUERY_BYTES`].
    pub fn validated_limit(&self) -> Result<u32, DennettError> {
        if let Some(q) = &self.query {
            reject_nul(q, "query")?;
            if q.len() > MAX_SEARCH_QUERY_BYTES {
                return Err(DennettError::InvalidInput {
                    field: String::from("query"),
                    reason: format!("exceeds maximum length of {MAX_SEARCH_QUERY_BYTES} bytes"),
                });
            }
        }
        let limit = self.limit.unwrap_or(DEFAULT_SEARCH_LIMIT);
        if limit == 0 || limit > MAX_SEARCH_LIMIT {
            return Err(DennettError::InvalidInput {
                field: String::from("limit"),
                reason: format!("must be between 1 and {MAX_SEARCH_LIMIT}"),
            });
        }
        Ok(limit)
    }
}

/// Scoped search response containing current notes and truncation metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchNotesResponse {
    /// Matching current notes visible within the principal's authorized scopes.
    pub notes: Vec<MemoryNote>,
    /// Total number of notes matching the scoped query.
    pub total_matched: u64,
    /// `true` when additional matching notes exist beyond `notes.len()`.
    pub truncated: bool,
}

/// Discriminator for append-only rows in `memory_events`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MemoryEventKind {
    /// Evidence object was ingested into a scope.
    EvidenceIngested,
    /// A new revision-1 memory note was created.
    NoteCreated,
    /// An existing memory note received a new immutable correction revision.
    NoteCorrected,
}

impl MemoryEventKind {
    /// Returns the canonical snake-case SQL/wire representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EvidenceIngested => "evidence_ingested",
            Self::NoteCreated => "note_created",
            Self::NoteCorrected => "note_corrected",
        }
    }
}

impl fmt::Display for MemoryEventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MemoryEventKind {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "evidence_ingested" => Ok(Self::EvidenceIngested),
            "note_created" => Ok(Self::NoteCreated),
            "note_corrected" => Ok(Self::NoteCorrected),
            other => Err(DennettError::InvalidInput {
                field: String::from("event_kind"),
                reason: format!("unknown memory event kind `{other}`"),
            }),
        }
    }
}

/// Stored append-only row from `memory_events` together with its [`EventEvidenceLink`]s.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryEvent {
    /// Stable event identifier.
    pub event_id: MemoryEventId,
    /// Monotonic stream identifier allocated via `stream_counters`.
    pub stream_id: String,
    /// Gapless commit-ordered sequence number within `stream_id`.
    pub stream_seq: i64,
    /// Scope isolating this event.
    pub scope: MemoryScope,
    /// Event discriminator.
    pub kind: MemoryEventKind,
    /// Verified principal who originated this event.
    pub principal_id: PrincipalId,
    /// Correlation identifier propagated from the caller or generated at ingress.
    pub correlation_id: CorrelationId,
    /// Caller idempotency key associated with this event.
    pub idempotency_key: IdempotencyKey,
    /// Canonical SHA-256 payload fingerprint used to detect conflicting idempotency replays.
    pub payload_fingerprint: Sha256Hex,
    /// Primary target entity UUID (`evidence_id` or `note_id`).
    pub target_entity_id: Uuid,
    /// Optional target revision (`Some(rev)` for note events, `None` for evidence ingestion).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_revision: Option<i64>,
    /// Optional observed timestamp when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<TimestampMs>,
    /// Server commit timestamp.
    pub created_at: TimestampMs,
    /// Evidence links associated with this event.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_links: Vec<EventEvidenceLink>,
}

fn reject_nul(value: &str, field: &str) -> Result<(), DennettError> {
    if value.as_bytes().contains(&0) {
        return Err(DennettError::InvalidInput {
            field: field.to_owned(),
            reason: String::from("must not contain NUL (\\u0000) bytes"),
        });
    }
    Ok(())
}

fn validate_metadata_map(metadata: &BTreeMap<String, String>) -> Result<(), DennettError> {
    if metadata.len() > MAX_METADATA_ENTRIES {
        return Err(DennettError::InvalidInput {
            field: String::from("metadata"),
            reason: format!("exceeds maximum of {MAX_METADATA_ENTRIES} entries"),
        });
    }
    for (key, value) in metadata {
        reject_nul(key, "metadata")?;
        reject_nul(value, "metadata")?;
        if key.is_empty() || key.len() > MAX_METADATA_KEY_LEN {
            return Err(DennettError::InvalidInput {
                field: String::from("metadata"),
                reason: format!("key must be non-empty and at most {MAX_METADATA_KEY_LEN} bytes"),
            });
        }
        if value.len() > MAX_METADATA_VALUE_LEN {
            return Err(DennettError::InvalidInput {
                field: String::from("metadata"),
                reason: format!("value exceeds maximum length of {MAX_METADATA_VALUE_LEN} bytes"),
            });
        }
    }
    Ok(())
}

fn validate_note_fields(
    title: &str,
    body: &str,
    tags: &[String],
    author_label: Option<&str>,
    evidence_links: &[EvidenceLinkInput],
) -> Result<(), DennettError> {
    reject_nul(title, "title")?;
    if title.trim().is_empty() || title.len() > MAX_NOTE_TITLE_BYTES {
        return Err(DennettError::InvalidInput {
            field: String::from("title"),
            reason: format!("must be non-empty and at most {MAX_NOTE_TITLE_BYTES} bytes"),
        });
    }
    reject_nul(body, "body")?;
    if body.trim().is_empty() || body.len() > MAX_NOTE_BODY_BYTES {
        return Err(DennettError::InvalidInput {
            field: String::from("body"),
            reason: format!("must be non-empty and at most {MAX_NOTE_BODY_BYTES} bytes"),
        });
    }
    if tags.len() > MAX_TAGS_PER_NOTE {
        return Err(DennettError::InvalidInput {
            field: String::from("tags"),
            reason: format!("exceeds maximum of {MAX_TAGS_PER_NOTE} tags"),
        });
    }
    for tag in tags {
        reject_nul(tag, "tags")?;
        if tag.trim().is_empty() || tag.len() > MAX_TAG_LEN {
            return Err(DennettError::InvalidInput {
                field: String::from("tags"),
                reason: format!("each tag must be non-empty and at most {MAX_TAG_LEN} bytes"),
            });
        }
    }
    if let Some(label) = author_label {
        reject_nul(label, "author_label")?;
        if label.trim().is_empty() || label.len() > MAX_AUTHOR_LABEL_BYTES {
            return Err(DennettError::InvalidInput {
                field: String::from("author_label"),
                reason: format!("must be non-empty and at most {MAX_AUTHOR_LABEL_BYTES} bytes"),
            });
        }
    }
    if evidence_links.len() > MAX_LINKS_PER_EVENT {
        return Err(DennettError::InvalidInput {
            field: String::from("evidence_links"),
            reason: format!("exceeds maximum of {MAX_LINKS_PER_EVENT} links"),
        });
    }
    for link in evidence_links {
        link.validate()?;
    }
    Ok(())
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<EvidenceId>();
    assert_send_sync::<MemoryEventId>();
    assert_send_sync::<EvidenceLinkId>();
    assert_send_sync::<MemoryNoteId>();
    assert_send_sync::<NoteRevisionId>();
    assert_send_sync::<TimestampMs>();
    assert_send_sync::<MemoryScope>();
    assert_send_sync::<PrincipalId>();
    assert_send_sync::<IdempotencyKey>();
    assert_send_sync::<Sha256Hex>();
    assert_send_sync::<EvidenceRelationship>();
    assert_send_sync::<MemoryCapability>();
    assert_send_sync::<AuthPrincipal>();
    assert_send_sync::<EvidenceAnchor>();
    assert_send_sync::<EvidenceLinkInput>();
    assert_send_sync::<EventEvidenceLink>();
    assert_send_sync::<IngestEvidenceRequest>();
    assert_send_sync::<EvidenceObject>();
    assert_send_sync::<IngestEvidenceResponse>();
    assert_send_sync::<CreateNoteRequest>();
    assert_send_sync::<CorrectNoteRequest>();
    assert_send_sync::<MemoryNote>();
    assert_send_sync::<NoteRevision>();
    assert_send_sync::<NoteMutationResponse>();
    assert_send_sync::<NoteHistoryResponse>();
    assert_send_sync::<SearchNotesRequest>();
    assert_send_sync::<SearchNotesResponse>();
    assert_send_sync::<MemoryEventKind>();
    assert_send_sync::<MemoryEvent>();
};

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn memory_scope_accepts_valid_and_rejects_traversal_and_wildcards() {
        for valid in [
            "project-alpha",
            "personal",
            "project:alpha/docs",
            "v1.0_test",
        ] {
            assert!(
                MemoryScope::try_new(valid).is_ok(),
                "expected valid scope `{valid}`"
            );
        }
        for invalid in [
            "",
            " ",
            "project*",
            "../escape",
            "a/../b",
            "a//b",
            "a::b",
            "/leading",
            "trailing/",
            ":leading",
            "trailing:",
            ".leading",
            "trailing.",
            "has space",
            "has\nnewline",
        ] {
            assert!(
                MemoryScope::try_new(invalid).is_err(),
                "expected invalid scope `{invalid}`"
            );
            let json = serde_json::to_string(invalid).unwrap_or_default();
            let de: Result<MemoryScope, _> = serde_json::from_str(&json);
            assert!(de.is_err(), "serde must reject invalid scope `{invalid}`");
        }
    }

    #[test]
    fn auth_principal_checks_scope_capability_and_expiry_boundaries() {
        let alpha = MemoryScope::try_new("project-alpha").ok();
        let beta = MemoryScope::try_new("project-beta").ok();
        let principal_id = PrincipalId::try_new("agent:writer").ok();
        let (Some(alpha), Some(beta), Some(principal_id)) = (alpha, beta, principal_id) else {
            panic!("valid fixtures must construct");
        };

        let principal = AuthPrincipal {
            principal_id,
            issuer: String::from("dennett-standalone"),
            token_id: String::from("tok-1"),
            allowed_scopes: BTreeSet::from([alpha.clone()]),
            capabilities: BTreeSet::from([MemoryCapability::Read]),
            expires_at: Some(TimestampMs::from_millis(1_000)),
        };

        assert!(principal.can_access(&alpha, MemoryCapability::Read));
        assert!(!principal.can_access(&alpha, MemoryCapability::Write));
        assert!(!principal.can_access(&beta, MemoryCapability::Read));
        assert!(!principal.is_expired_at(TimestampMs::from_millis(999)));
        assert!(principal.is_expired_at(TimestampMs::from_millis(1_000)));
        assert!(principal.is_expired_at(TimestampMs::from_millis(1_001)));
    }

    #[test]
    fn evidence_link_and_anchor_validate_non_negative_ordinal_and_range() {
        let Ok(rel) = EvidenceRelationship::try_new("supports") else {
            panic!("valid relationship");
        };

        let valid_null_ordinal = EvidenceLinkInput {
            evidence_id: EvidenceId::new(),
            relationship: rel.clone(),
            anchor: Some(EvidenceAnchor {
                kind: String::from("line_range"),
                start: Some(10),
                end: Some(20),
                locator: Some(String::from("src/lib.rs")),
                attributes: BTreeMap::new(),
            }),
            ordinal: None,
        };
        assert!(valid_null_ordinal.validate().is_ok());

        let neg_ordinal = EvidenceLinkInput {
            ordinal: Some(-1),
            ..valid_null_ordinal.clone()
        };
        assert!(neg_ordinal.validate().is_err());

        let inverted_anchor = EvidenceLinkInput {
            anchor: Some(EvidenceAnchor {
                kind: String::from("line_range"),
                start: Some(30),
                end: Some(10),
                locator: None,
                attributes: BTreeMap::new(),
            }),
            ..valid_null_ordinal
        };
        assert!(inverted_anchor.validate().is_err());
    }

    #[test]
    fn timestamp_ms_enforces_postgresql_safe_bounds() {
        assert!(
            TimestampMs::from_millis(MIN_TIMESTAMP_MS)
                .validate("ts")
                .is_ok()
        );
        assert!(
            TimestampMs::from_millis(MAX_TIMESTAMP_MS)
                .validate("ts")
                .is_ok()
        );
        for bad_ts in [
            MIN_TIMESTAMP_MS - 1,
            MAX_TIMESTAMP_MS + 1,
            i64::MIN,
            i64::MAX,
        ] {
            assert!(
                matches!(
                    TimestampMs::from_millis(bad_ts).validate("observed_at"),
                    Err(DennettError::InvalidInput { .. })
                ),
                "expected InvalidInput for timestamp {bad_ts}"
            );
        }
    }

    #[test]
    fn nul_bytes_and_extreme_timestamps_are_rejected_across_evidence_requests() {
        let scope = MemoryScope::try_new("project-alpha").expect("valid scope");
        let key = IdempotencyKey::try_new("op-nul-1").expect("valid key");

        let base_ev = IngestEvidenceRequest {
            scope,
            media_type: String::from("text/plain"),
            content: String::from("ok"),
            source_uri: Some(String::from("file://ok")),
            observed_at: Some(TimestampMs::from_millis(1_700_000_000_000)),
            metadata: BTreeMap::from([(String::from("k"), String::from("v"))]),
            idempotency_key: key,
        };
        assert!(base_ev.validate().is_ok());
        for bad in [
            IngestEvidenceRequest {
                media_type: String::from("text/\0plain"),
                ..base_ev.clone()
            },
            IngestEvidenceRequest {
                content: String::from("bad\0content"),
                ..base_ev.clone()
            },
            IngestEvidenceRequest {
                source_uri: Some(String::from("file://bad\0uri")),
                ..base_ev.clone()
            },
            IngestEvidenceRequest {
                metadata: BTreeMap::from([(String::from("k\0"), String::from("v"))]),
                ..base_ev.clone()
            },
            IngestEvidenceRequest {
                metadata: BTreeMap::from([(String::from("k"), String::from("v\0"))]),
                ..base_ev.clone()
            },
            IngestEvidenceRequest {
                observed_at: Some(TimestampMs::from_millis(i64::MAX)),
                ..base_ev
            },
        ] {
            assert!(matches!(
                bad.validate(),
                Err(DennettError::InvalidInput { .. })
            ));
        }
    }

    #[test]
    fn nul_bytes_and_extreme_timestamps_are_rejected_across_note_and_search_requests() {
        let scope = MemoryScope::try_new("project-alpha").expect("valid scope");
        let key = IdempotencyKey::try_new("op-nul-2").expect("valid key");
        let rel = EvidenceRelationship::try_new("supports").expect("valid rel");

        let base_note = CreateNoteRequest {
            scope: scope.clone(),
            title: String::from("ok"),
            body: String::from("ok"),
            tags: vec![String::from("ok")],
            observed_at: None,
            evidence_links: vec![],
            author_label: Some(String::from("ok")),
            idempotency_key: key.clone(),
        };
        assert!(base_note.validate().is_ok());
        for bad in [
            CreateNoteRequest {
                title: String::from("bad\0title"),
                ..base_note.clone()
            },
            CreateNoteRequest {
                body: String::from("bad\0body"),
                ..base_note.clone()
            },
            CreateNoteRequest {
                tags: vec![String::from("bad\0tag")],
                ..base_note.clone()
            },
            CreateNoteRequest {
                author_label: Some(String::from("bad\0label")),
                ..base_note.clone()
            },
            CreateNoteRequest {
                observed_at: Some(TimestampMs::from_millis(i64::MIN)),
                ..base_note.clone()
            },
            CreateNoteRequest {
                evidence_links: vec![EvidenceLinkInput {
                    evidence_id: EvidenceId::new(),
                    relationship: rel,
                    anchor: Some(EvidenceAnchor {
                        kind: String::from("line_range"),
                        start: Some(1),
                        end: Some(2),
                        locator: Some(String::from("bad\0loc")),
                        attributes: BTreeMap::new(),
                    }),
                    ordinal: None,
                }],
                ..base_note
            },
        ] {
            assert!(matches!(
                bad.validate(),
                Err(DennettError::InvalidInput { .. })
            ));
        }

        let bad_corr = CorrectNoteRequest {
            note_id: MemoryNoteId::new(),
            scope,
            expected_revision: 1,
            title: String::from("ok"),
            body: String::from("ok"),
            tags: vec![],
            rationale: String::from("bad\0rationale"),
            evidence_links: vec![],
            author_label: None,
            idempotency_key: key,
        };
        assert!(matches!(
            bad_corr.validate(),
            Err(DennettError::InvalidInput { .. })
        ));

        let bad_search = SearchNotesRequest {
            scope: None,
            query: Some(String::from("bad\0query")),
            limit: Some(10),
        };
        assert!(matches!(
            bad_search.validated_limit(),
            Err(DennettError::InvalidInput { .. })
        ));
    }

    proptest! {
        #[test]
        fn prop_memory_scope_matches_spec_oracle(raw in any::<String>()) {
            let bytes = raw.as_bytes();
            let expected_valid = !raw.is_empty()
                && raw.len() <= MAX_SCOPE_LEN
                && bytes.iter().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':' | b'/'))
                && !matches!(bytes.first(), Some(b'/' | b':' | b'.'))
                && !matches!(bytes.last(), Some(b'/' | b':' | b'.'))
                && !raw.contains("..")
                && !raw.contains("//")
                && !raw.contains("::");

            prop_assert_eq!(MemoryScope::try_new(raw.clone()).is_ok(), expected_valid);
            if let Ok(json) = serde_json::to_string(&raw) {
                let decoded: Result<MemoryScope, _> = serde_json::from_str(&json);
                prop_assert_eq!(decoded.is_ok(), expected_valid);
            }
        }

        #[test]
        fn prop_idempotency_key_roundtrips_valid_ascii(raw in "[A-Za-z0-9._:-]{1,128}") {
            let parsed = IdempotencyKey::try_new(raw.clone());
            prop_assert!(parsed.is_ok());
            if let Ok(key) = parsed {
                prop_assert_eq!(key.as_str(), raw.as_str());
                let json = serde_json::to_string(&key);
                prop_assert!(json.is_ok());
                if let Ok(encoded) = json {
                    let decoded: Result<IdempotencyKey, _> = serde_json::from_str(&encoded);
                    prop_assert_eq!(decoded.ok(), Some(key));
                }
            }
        }

        #[test]
        fn prop_sha256_hex_accepts_only_64_lowercase_hex(raw in any::<String>()) {
            let expected = raw.len() == SHA256_HEX_LEN
                && raw.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
            prop_assert_eq!(Sha256Hex::try_new(raw).is_ok(), expected);
        }
    }
}
