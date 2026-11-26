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
/// Default salience score assigned to newly admitted notes (`0..=100`).
pub const DEFAULT_NOTE_SALIENCE: u8 = 50;
/// Maximum permitted salience score (`0..=100`).
pub const MAX_NOTE_SALIENCE: u8 = 100;
/// Maximum permitted byte length of a normalized topic key.
pub const MAX_TOPIC_KEY_LEN: usize = 128;
/// Maximum permitted byte length of an optional claim predicate sub-key.
pub const MAX_CLAIM_KEY_LEN: usize = 128;
/// Maximum permitted byte length of a natural-language claim statement (16 KiB).
pub const MAX_CLAIM_TEXT_BYTES: usize = 16 * 1024;
/// Maximum number of explicit caveats attached to a single [`MemoryClaim`].
pub const MAX_CAVEATS_PER_CLAIM: usize = 32;
/// Maximum byte length of a single claim caveat string.
pub const MAX_CAVEAT_BYTES: usize = 1024;
/// Maximum permitted byte length of a logical [`MemoryDocument`] key.
pub const MAX_DOCUMENT_KEY_LEN: usize = 128;
/// Maximum permitted byte length of a [`MemoryDocument`] title.
pub const MAX_DOCUMENT_TITLE_BYTES: usize = 256;
/// Maximum permitted byte length of a [`MemoryDocument`] body (512 KiB).
pub const MAX_DOCUMENT_BODY_BYTES: usize = 512 * 1024;
/// Maximum permitted byte length of a forget/conflict resolution rationale.
pub const MAX_FORGET_REASON_BYTES: usize = 4 * 1024;
/// Maximum permitted byte length of an imported/exported memory package JSON payload (16 MiB).
pub const MAX_PACKAGE_JSON_BYTES: usize = 16 * 1024 * 1024;
/// Maximum number of provenance claim or note identifiers attached to a single document version.
pub const MAX_PROVENANCE_IDS: usize = 256;

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

define_uuid_id! {
    /// Stable UUID identifying a structured [`MemoryClaim`].
    pub struct ClaimId;
    field = "claim_id";
}

define_uuid_id! {
    /// Stable UUID identifying a tracked [`MemoryConflict`].
    pub struct ConflictId;
    field = "conflict_id";
}

define_uuid_id! {
    /// Stable UUID identifying a versioned [`MemoryDocument`] dossier.
    pub struct DocumentId;
    field = "document_id";
}

define_uuid_id! {
    /// Stable UUID identifying one immutable [`DocumentVersion`] snapshot.
    pub struct DocumentVersionId;
    field = "version_id";
}

define_uuid_id! {
    /// Stable UUID identifying an interruptible [`ConsolidationJob`].
    pub struct ConsolidationJobId;
    field = "job_id";
}

define_uuid_id! {
    /// Stable UUID identifying a content-free [`ForgetTombstone`] audit record.
    pub struct TombstoneId;
    field = "tombstone_id";
}

define_uuid_id! {
    /// Stable UUID identifying a recorded [`ExportResponse`] package.
    pub struct ExportPackageId;
    field = "package_id";
}

define_uuid_id! {
    /// Stable UUID identifying a recorded [`ImportResponse`] ingestion audit row.
    pub struct ImportRecordId;
    field = "import_id";
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
    /// Allows reading notes, claims, conflicts, documents, history, events, evidence, stats, and scoped search.
    Read,
    /// Allows ingesting evidence, creating/correcting notes, claims, conflicts, documents, consolidation jobs, and untrusted imports.
    Write,
    /// Allows administrative memory maintenance operations such as full search-index rebuilds.
    Admin,
    /// Allows exporting deterministic, secret-stripped memory packages (Contract J).
    Export,
    /// Allows forgetting/deleting memory entities with search-index purge and audit tombstones.
    Forget,
}

impl MemoryCapability {
    /// Returns the canonical snake-case capability label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Admin => "admin",
            Self::Export => "export",
            Self::Forget => "forget",
        }
    }
}

impl fmt::Display for MemoryCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MemoryCapability {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "read" => Ok(Self::Read),
            "write" => Ok(Self::Write),
            "admin" => Ok(Self::Admin),
            "export" => Ok(Self::Export),
            "forget" => Ok(Self::Forget),
            other => Err(DennettError::InvalidInput {
                field: String::from("capability"),
                reason: format!("unknown memory capability `{other}`"),
            }),
        }
    }
}

/// Admission disposition assigned to a [`MemoryNote`] upon ingestion.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AdmissionStatus {
    /// Admitted into durable long-term memory projections.
    #[default]
    Admitted,
    /// Ephemeral short-lived note subject to retention sweeping.
    Ephemeral,
    /// Rejected from active retrieval projections while preserving audit lineage.
    Rejected,
}

impl AdmissionStatus {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Admitted => "admitted",
            Self::Ephemeral => "ephemeral",
            Self::Rejected => "rejected",
        }
    }
}

impl fmt::Display for AdmissionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AdmissionStatus {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "admitted" => Ok(Self::Admitted),
            "ephemeral" => Ok(Self::Ephemeral),
            "rejected" => Ok(Self::Rejected),
            other => Err(DennettError::InvalidInput {
                field: String::from("admission_status"),
                reason: format!("unknown admission status `{other}`"),
            }),
        }
    }
}

/// Provenance trust tier distinguishing native memory from imported external material (Contract J).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TrustLevel {
    /// Authored directly inside this Dennett instance.
    #[default]
    Native,
    /// Imported from an external package and untrusted by default (`instruction_active = false`).
    ExternalUntrusted,
    /// External material explicitly reviewed by the owner.
    Reviewed,
    /// Explicitly promoted to trusted material by the owner.
    Trusted,
}

impl TrustLevel {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::ExternalUntrusted => "external_untrusted",
            Self::Reviewed => "reviewed",
            Self::Trusted => "trusted",
        }
    }
}

impl fmt::Display for TrustLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TrustLevel {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "native" => Ok(Self::Native),
            "external_untrusted" => Ok(Self::ExternalUntrusted),
            "reviewed" => Ok(Self::Reviewed),
            "trusted" => Ok(Self::Trusted),
            other => Err(DennettError::InvalidInput {
                field: String::from("trust_level"),
                reason: format!("unknown trust level `{other}`"),
            }),
        }
    }
}

const fn default_note_salience() -> u8 {
    DEFAULT_NOTE_SALIENCE
}

const fn default_true() -> bool {
    true
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
    /// Optional admission disposition (defaults to [`AdmissionStatus::Admitted`] on the server).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admission_status: Option<AdmissionStatus>,
    /// Optional salience score in `0..=100` (defaults to [`DEFAULT_NOTE_SALIENCE`] on the server).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub salience: Option<u8>,
    /// Optional normalized topic key grouping related notes and claims.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic_key: Option<String>,
    /// Caller idempotency key preventing duplicate creation on retries.
    pub idempotency_key: IdempotencyKey,
}

impl CreateNoteRequest {
    /// Validates note creation fields, admission/salience bounds, topic key, observed timestamp, and evidence link inputs.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] on empty/oversized/NUL-containing fields,
    /// out-of-range salience or timestamps, or invalid links.
    pub fn validate(&self) -> Result<(), DennettError> {
        if let Some(ts) = self.observed_at {
            ts.validate("observed_at")?;
        }
        if let Some(salience) = self.salience
            && salience > MAX_NOTE_SALIENCE
        {
            return Err(DennettError::InvalidInput {
                field: String::from("salience"),
                reason: format!("must be between 0 and {MAX_NOTE_SALIENCE}"),
            });
        }
        if let Some(ref topic_key) = self.topic_key {
            validate_bounded_key(topic_key, "topic_key", MAX_TOPIC_KEY_LEN)?;
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
    /// Admission disposition of this note (`admitted`, `ephemeral`, or `rejected`).
    #[serde(default)]
    pub admission_status: AdmissionStatus,
    /// Salience score in `0..=100` (defaults to [`DEFAULT_NOTE_SALIENCE`]).
    #[serde(default = "default_note_salience")]
    pub salience: u8,
    /// Optional normalized topic key grouping related notes and claims.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic_key: Option<String>,
    /// Provenance trust tier (`native` vs `external_untrusted`/`reviewed`/`trusted`).
    #[serde(default)]
    pub trust_level: TrustLevel,
    /// Whether this note is permitted to act as executable instructions (always `false` on untrusted imports).
    #[serde(default)]
    pub instruction_active: bool,
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
    /// When `true`, includes historical/superseded matches alongside current state where supported.
    #[serde(default)]
    pub include_history: bool,
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

/// Scoped search response containing ranked notes, claims, documents, truncation metadata, and live index status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchNotesResponse {
    /// Matching current notes visible within the principal's authorized scopes.
    pub notes: Vec<MemoryNote>,
    /// Matching structured claims visible within the principal's authorized scopes.
    #[serde(default)]
    pub claims: Vec<MemoryClaim>,
    /// Matching versioned dossier documents visible within the principal's authorized scopes.
    #[serde(default)]
    pub documents: Vec<MemoryDocument>,
    /// Total number of items matching the scoped query.
    pub total_matched: u64,
    /// `true` when additional matching items exist beyond the returned slice.
    pub truncated: bool,
    /// Live freshness/health status of the separable search index at query time.
    #[serde(default)]
    pub index_status: IndexStatus,
    /// Honest notice when the index is lagging, rebuilding, or degraded (fresh tail overlay active).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_warning: Option<String>,
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
    /// A structured [`MemoryClaim`] was created.
    ClaimCreated,
    /// An existing [`MemoryClaim`] was superseded by a newer claim.
    ClaimSuperseded,
    /// A contradiction between candidate claims was recorded as a [`MemoryConflict`].
    ConflictDetected,
    /// An open [`MemoryConflict`] was resolved or explicitly adjudicated.
    ConflictResolved,
    /// A new versioned [`MemoryDocument`] dossier was created.
    DocumentCreated,
    /// An existing [`MemoryDocument`] received a new version.
    DocumentUpdated,
    /// A [`MemoryNote`] was forgotten with tombstone and search-index purge.
    NoteForgotten,
    /// An [`EvidenceObject`] was forgotten with tombstone and search-index purge.
    EvidenceForgotten,
    /// A [`MemoryClaim`] was forgotten with tombstone and search-index purge.
    ClaimForgotten,
    /// A [`MemoryDocument`] was forgotten with tombstone and search-index purge.
    DocumentForgotten,
    /// A deterministic, secret-free memory package was exported (Contract J).
    PackageExported,
    /// An external memory package was imported as `external_untrusted` data (Contract J).
    PackageImported,
}

impl MemoryEventKind {
    /// Returns the canonical snake-case SQL/wire representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EvidenceIngested => "evidence_ingested",
            Self::NoteCreated => "note_created",
            Self::NoteCorrected => "note_corrected",
            Self::ClaimCreated => "claim_created",
            Self::ClaimSuperseded => "claim_superseded",
            Self::ConflictDetected => "conflict_detected",
            Self::ConflictResolved => "conflict_resolved",
            Self::DocumentCreated => "document_created",
            Self::DocumentUpdated => "document_updated",
            Self::NoteForgotten => "note_forgotten",
            Self::EvidenceForgotten => "evidence_forgotten",
            Self::ClaimForgotten => "claim_forgotten",
            Self::DocumentForgotten => "document_forgotten",
            Self::PackageExported => "package_exported",
            Self::PackageImported => "package_imported",
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
            "claim_created" => Ok(Self::ClaimCreated),
            "claim_superseded" => Ok(Self::ClaimSuperseded),
            "conflict_detected" => Ok(Self::ConflictDetected),
            "conflict_resolved" => Ok(Self::ConflictResolved),
            "document_created" => Ok(Self::DocumentCreated),
            "document_updated" => Ok(Self::DocumentUpdated),
            "note_forgotten" => Ok(Self::NoteForgotten),
            "evidence_forgotten" => Ok(Self::EvidenceForgotten),
            "claim_forgotten" => Ok(Self::ClaimForgotten),
            "document_forgotten" => Ok(Self::DocumentForgotten),
            "package_exported" => Ok(Self::PackageExported),
            "package_imported" => Ok(Self::PackageImported),
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

// ============================================================================
// Phase D3 Memory Fabric: Claims, Conflicts, Documents, Consolidation,
// Separable Search Index, Forgetting, and Portable Export/Import (Contract J)
// ============================================================================

/// Epistemic lifecycle status of a structured [`MemoryClaim`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EpistemicStatus {
    /// Directly observed from primary evidence.
    Observed,
    /// Explicitly asserted as current best-known state.
    #[default]
    Asserted,
    /// Derived or synthesized from multiple observations.
    Inferred,
    /// Tentative hypothesis awaiting confirmation.
    Hypothesis,
    /// Actively disputed by an open [`MemoryConflict`].
    Contested,
    /// Replaced by a newer claim in a supersession chain.
    Superseded,
    /// Explicitly withdrawn without a replacement claim.
    Retracted,
}

impl EpistemicStatus {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Observed => "observed",
            Self::Asserted => "asserted",
            Self::Inferred => "inferred",
            Self::Hypothesis => "hypothesis",
            Self::Contested => "contested",
            Self::Superseded => "superseded",
            Self::Retracted => "retracted",
        }
    }
}

impl fmt::Display for EpistemicStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for EpistemicStatus {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "observed" => Ok(Self::Observed),
            "asserted" => Ok(Self::Asserted),
            "inferred" => Ok(Self::Inferred),
            "hypothesis" => Ok(Self::Hypothesis),
            "contested" => Ok(Self::Contested),
            "superseded" => Ok(Self::Superseded),
            "retracted" => Ok(Self::Retracted),
            other => Err(DennettError::InvalidInput {
                field: String::from("epistemic_status"),
                reason: format!("unknown epistemic status `{other}`"),
            }),
        }
    }
}

/// Structured claim representing best-known current or historical state for a topic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryClaim {
    /// Stable claim identifier.
    pub claim_id: ClaimId,
    /// Scope isolating this claim.
    pub scope: MemoryScope,
    /// Normalized topic key grouping related claims.
    pub topic_key: String,
    /// Optional predicate sub-key within `topic_key`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_key: Option<String>,
    /// Natural-language statement of the claim.
    pub natural_language: String,
    /// Current epistemic status of this claim.
    pub epistemic_status: EpistemicStatus,
    /// Previous claim explicitly superseded by this claim, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes_claim_id: Option<ClaimId>,
    /// Optional start of the claim's validity interval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<TimestampMs>,
    /// Optional end of the claim's validity interval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<TimestampMs>,
    /// Append-only event that created this claim.
    pub source_event_id: MemoryEventId,
    /// Optional source note from which this claim was derived.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_note_id: Option<MemoryNoteId>,
    /// Essential caveats, exceptions, or qualifiers that consolidation must preserve.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub caveats: Vec<String>,
    /// Deterministic provenance metadata map (supporting evidence ids, confidence, model version).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provenance: BTreeMap<String, String>,
    /// Provenance trust tier (`native` vs `external_untrusted`/`reviewed`/`trusted`).
    #[serde(default)]
    pub trust_level: TrustLevel,
    /// Whether this claim may act as executable instructions (strictly `false` on untrusted imports).
    #[serde(default)]
    pub instruction_active: bool,
    /// Evidence links attached to `source_event_id`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_links: Vec<EventEvidenceLink>,
    /// Server commit timestamp.
    pub created_at: TimestampMs,
}

/// Request to create a new structured [`MemoryClaim`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateClaimRequest {
    /// Scope owning the claim.
    pub scope: MemoryScope,
    /// Normalized topic key grouping related claims.
    pub topic_key: String,
    /// Optional predicate sub-key within `topic_key`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_key: Option<String>,
    /// Natural-language statement of the claim.
    pub natural_language: String,
    /// Initial epistemic status (defaults to [`EpistemicStatus::Asserted`]).
    #[serde(default)]
    pub epistemic_status: EpistemicStatus,
    /// Optional earlier claim superseded by this claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes_claim_id: Option<ClaimId>,
    /// Optional start of the claim's validity interval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<TimestampMs>,
    /// Optional end of the claim's validity interval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<TimestampMs>,
    /// Optional source note from which this claim was extracted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_note_id: Option<MemoryNoteId>,
    /// Essential caveats or exceptions attached to this claim.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub caveats: Vec<String>,
    /// Optional provenance metadata map.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provenance: BTreeMap<String, String>,
    /// Optional links to supporting evidence objects in the same scope.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_links: Vec<EvidenceLinkInput>,
    /// Caller idempotency key preventing duplicate claim creation on retries.
    pub idempotency_key: IdempotencyKey,
}

impl CreateClaimRequest {
    /// Validates claim creation fields, validity interval, caveats, provenance, and evidence links.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] on empty/oversized/NUL-containing fields,
    /// invalid timestamps (`valid_from > valid_to`), or invalid caveats/links.
    pub fn validate(&self) -> Result<(), DennettError> {
        validate_bounded_key(&self.topic_key, "topic_key", MAX_TOPIC_KEY_LEN)?;
        if let Some(ref key) = self.claim_key {
            validate_bounded_key(key, "claim_key", MAX_CLAIM_KEY_LEN)?;
        }
        reject_nul(&self.natural_language, "natural_language")?;
        if self.natural_language.trim().is_empty()
            || self.natural_language.len() > MAX_CLAIM_TEXT_BYTES
        {
            return Err(DennettError::InvalidInput {
                field: String::from("natural_language"),
                reason: format!("must be non-empty and at most {MAX_CLAIM_TEXT_BYTES} bytes"),
            });
        }
        if let Some(vf) = self.valid_from {
            vf.validate("valid_from")?;
        }
        if let Some(vt) = self.valid_to {
            vt.validate("valid_to")?;
        }
        if let (Some(vf), Some(vt)) = (self.valid_from, self.valid_to)
            && vf > vt
        {
            return Err(DennettError::InvalidInput {
                field: String::from("valid_to"),
                reason: String::from("valid_to must be greater than or equal to valid_from"),
            });
        }
        if self.caveats.len() > MAX_CAVEATS_PER_CLAIM {
            return Err(DennettError::InvalidInput {
                field: String::from("caveats"),
                reason: format!("exceeds maximum of {MAX_CAVEATS_PER_CLAIM} caveats"),
            });
        }
        for caveat in &self.caveats {
            reject_nul(caveat, "caveats")?;
            if caveat.trim().is_empty() || caveat.len() > MAX_CAVEAT_BYTES {
                return Err(DennettError::InvalidInput {
                    field: String::from("caveats"),
                    reason: format!(
                        "each caveat must be non-empty and at most {MAX_CAVEAT_BYTES} bytes"
                    ),
                });
            }
        }
        validate_metadata_map(&self.provenance)?;
        if self.evidence_links.len() > MAX_LINKS_PER_EVENT {
            return Err(DennettError::InvalidInput {
                field: String::from("evidence_links"),
                reason: format!("exceeds maximum of {MAX_LINKS_PER_EVENT} links"),
            });
        }
        for link in &self.evidence_links {
            link.validate()?;
        }
        Ok(())
    }
}

/// Request to supersede an existing [`MemoryClaim`] with a newer replacement claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupersedeClaimRequest {
    /// Existing claim identifier to be superseded.
    pub claim_id: ClaimId,
    /// Expected owning scope of both the superseded claim and `new_claim`.
    pub scope: MemoryScope,
    /// Replacement claim payload.
    pub new_claim: CreateClaimRequest,
    /// Optional explicit conflict resolution event authorizing supersession across older timestamps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution_event_id: Option<MemoryEventId>,
}

impl SupersedeClaimRequest {
    /// Validates replacement claim fields and scope consistency.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `new_claim` is invalid or targets a different scope.
    pub fn validate(&self) -> Result<(), DennettError> {
        if self.new_claim.scope != self.scope {
            return Err(DennettError::InvalidInput {
                field: String::from("new_claim.scope"),
                reason: String::from("replacement claim scope must match target scope"),
            });
        }
        self.new_claim.validate()
    }
}

/// Response listing [`MemoryClaim`] records for a scope or topic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListClaimsResponse {
    /// Matching claims visible within authorized scopes.
    pub claims: Vec<MemoryClaim>,
}

/// Category of contradiction tracked by [`MemoryConflict`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConflictType {
    /// Claims disagree across overlapping or inverted validity intervals.
    Temporal,
    /// Distinct evidence sources contradict one another.
    Source,
    /// Concurrent writers updated the same topic or predicate simultaneously.
    ConcurrentEdit,
    /// Semantically incompatible statements on the same topic.
    Semantic,
    /// Policy or constraint contradiction requiring explicit adjudication.
    Policy,
}

impl ConflictType {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Temporal => "temporal",
            Self::Source => "source",
            Self::ConcurrentEdit => "concurrent_edit",
            Self::Semantic => "semantic",
            Self::Policy => "policy",
        }
    }
}

impl fmt::Display for ConflictType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ConflictType {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "temporal" => Ok(Self::Temporal),
            "source" => Ok(Self::Source),
            "concurrent_edit" => Ok(Self::ConcurrentEdit),
            "semantic" => Ok(Self::Semantic),
            "policy" => Ok(Self::Policy),
            other => Err(DennettError::InvalidInput {
                field: String::from("conflict_type"),
                reason: format!("unknown conflict type `{other}`"),
            }),
        }
    }
}

/// Lifecycle state of a tracked [`MemoryConflict`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConflictStatus {
    /// Unresolved conflict actively marking candidate claims as `contested`.
    #[default]
    Open,
    /// Explicitly resolved with an audit event and optional winning claim.
    Resolved,
    /// Ambiguity intentionally accepted while keeping candidate positions documented.
    AcceptedAmbiguity,
    /// Conflict became moot because underlying claims were retracted or forgotten.
    Invalidated,
}

impl ConflictStatus {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Resolved => "resolved",
            Self::AcceptedAmbiguity => "accepted_ambiguity",
            Self::Invalidated => "invalidated",
        }
    }
}

impl fmt::Display for ConflictStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ConflictStatus {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "open" => Ok(Self::Open),
            "resolved" => Ok(Self::Resolved),
            "accepted_ambiguity" => Ok(Self::AcceptedAmbiguity),
            "invalidated" => Ok(Self::Invalidated),
            other => Err(DennettError::InvalidInput {
                field: String::from("conflict_status"),
                reason: format!("unknown conflict status `{other}`"),
            }),
        }
    }
}

/// Explicit tracked contradiction between candidate claims in a [`MemoryScope`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryConflict {
    /// Stable conflict identifier.
    pub conflict_id: ConflictId,
    /// Scope isolating this conflict.
    pub scope: MemoryScope,
    /// Optional normalized topic key shared by the conflicting claims.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic_key: Option<String>,
    /// Category of contradiction detected.
    pub conflict_type: ConflictType,
    /// Candidate claim identifiers participating in this conflict.
    pub candidate_claim_ids: Vec<ClaimId>,
    /// Whether this conflict blocks automatic decisions until resolved.
    #[serde(default)]
    pub blocking: bool,
    /// Current lifecycle status of the conflict.
    pub status: ConflictStatus,
    /// Append-only event that resolved or adjudicated this conflict, if resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution_event_id: Option<MemoryEventId>,
    /// Optional human- or operator-supplied resolution rationale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution_note: Option<String>,
    /// UTC timestamp when the conflict was detected.
    pub detected_at: TimestampMs,
    /// UTC timestamp when the conflict was resolved, if terminal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<TimestampMs>,
}

/// Response listing [`MemoryConflict`] records for a scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListConflictsResponse {
    /// Matching conflict records visible within authorized scopes.
    pub conflicts: Vec<MemoryConflict>,
}

/// Request to resolve or adjudicate an open [`MemoryConflict`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveConflictRequest {
    /// Target conflict identifier.
    pub conflict_id: ConflictId,
    /// Expected owning scope of the conflict.
    pub scope: MemoryScope,
    /// Required human-readable rationale documenting the resolution.
    pub resolution_note: String,
    /// Optional winning claim identifier whose competitors become `superseded`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub winning_claim_id: Option<ClaimId>,
    /// Optional explicit target status (defaults to [`ConflictStatus::Resolved`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_status: Option<ConflictStatus>,
    /// Caller idempotency key preventing duplicate resolution events on retries.
    pub idempotency_key: IdempotencyKey,
}

impl ResolveConflictRequest {
    /// Validates conflict resolution rationale and target status.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `resolution_note` is empty/oversized/contains NUL,
    /// or if `target_status` attempts to transition back to `Open`.
    pub fn validate(&self) -> Result<(), DennettError> {
        reject_nul(&self.resolution_note, "resolution_note")?;
        if self.resolution_note.trim().is_empty()
            || self.resolution_note.len() > MAX_FORGET_REASON_BYTES
        {
            return Err(DennettError::InvalidInput {
                field: String::from("resolution_note"),
                reason: format!("must be non-empty and at most {MAX_FORGET_REASON_BYTES} bytes"),
            });
        }
        if matches!(self.target_status, Some(ConflictStatus::Open)) {
            return Err(DennettError::InvalidInput {
                field: String::from("target_status"),
                reason: String::from("cannot resolve a conflict into `open` status"),
            });
        }
        Ok(())
    }
}

/// Human-readable versioned dossier synthesized from claims and notes in a [`MemoryScope`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryDocument {
    /// Stable document identifier.
    pub document_id: DocumentId,
    /// Scope isolating this document.
    pub scope: MemoryScope,
    /// Scope-unique logical key (e.g. `"dossier:user-preferences"`).
    pub document_key: String,
    /// Human-readable dossier title.
    pub title: String,
    /// Current monotonically increasing version number (`>= 1`).
    pub current_version: i32,
    /// Current markdown/text body of the dossier.
    pub current_body: String,
    /// Provenance claim identifiers backing this dossier version.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance_claim_ids: Vec<ClaimId>,
    /// Provenance note identifiers backing this dossier version.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance_note_ids: Vec<MemoryNoteId>,
    /// Append-only event that produced `current_version`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_event_id: Option<MemoryEventId>,
    /// Verified principal who wrote `current_version`.
    pub principal_id: PrincipalId,
    /// Provenance trust tier (`native` vs `external_untrusted`/`reviewed`/`trusted`).
    #[serde(default)]
    pub trust_level: TrustLevel,
    /// Whether this document may act as executable instructions (always `false` on untrusted imports).
    #[serde(default)]
    pub instruction_active: bool,
    /// UTC creation timestamp.
    pub created_at: TimestampMs,
    /// UTC timestamp of the latest version update.
    pub updated_at: TimestampMs,
}

/// One immutable historical version snapshot of a [`MemoryDocument`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentVersion {
    /// Stable version row identifier.
    pub version_id: DocumentVersionId,
    /// Parent logical document identifier.
    pub document_id: DocumentId,
    /// Monotonic version number (`1`, `2`, ...).
    pub version: i32,
    /// Full dossier body captured at this version.
    pub body: String,
    /// Provenance claim identifiers recorded for this version.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance_claim_ids: Vec<ClaimId>,
    /// Optional summary of changes compared to the previous version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_summary: Option<String>,
    /// Append-only event that committed this version.
    pub event_id: MemoryEventId,
    /// UTC commit timestamp.
    pub created_at: TimestampMs,
}

/// Request to create a version-1 [`MemoryDocument`] dossier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateDocumentRequest {
    /// Scope owning the document.
    pub scope: MemoryScope,
    /// Scope-unique logical document key (e.g. `"dossier:project-architecture"`).
    pub document_key: String,
    /// Human-readable document title.
    pub title: String,
    /// Markdown/text body for version 1.
    pub body: String,
    /// Optional provenance claim identifiers supporting this dossier.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance_claim_ids: Vec<ClaimId>,
    /// Optional provenance note identifiers supporting this dossier.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance_note_ids: Vec<MemoryNoteId>,
    /// Caller idempotency key preventing duplicate creation on retries.
    pub idempotency_key: IdempotencyKey,
}

impl CreateDocumentRequest {
    /// Validates document key, title, body, and provenance list bounds.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] on empty/oversized/NUL-containing fields.
    pub fn validate(&self) -> Result<(), DennettError> {
        validate_bounded_key(&self.document_key, "document_key", MAX_DOCUMENT_KEY_LEN)?;
        validate_document_fields(
            &self.title,
            &self.body,
            &self.provenance_claim_ids,
            &self.provenance_note_ids,
        )
    }
}

/// Request to update an existing [`MemoryDocument`] with optimistic version concurrency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateDocumentRequest {
    /// Target document identifier.
    pub document_id: DocumentId,
    /// Expected owning scope of the document.
    pub scope: MemoryScope,
    /// Optimistic concurrency version expected on `memory_documents.current_version`.
    pub expected_version: i32,
    /// Updated human-readable title.
    pub title: String,
    /// Updated markdown/text body for the new version.
    pub body: String,
    /// Provenance claim identifiers supporting the new version.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance_claim_ids: Vec<ClaimId>,
    /// Provenance note identifiers supporting the new version.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance_note_ids: Vec<MemoryNoteId>,
    /// Optional human-readable diff summary explaining the revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_summary: Option<String>,
    /// Caller idempotency key preventing duplicate version commits on retries.
    pub idempotency_key: IdempotencyKey,
}

impl UpdateDocumentRequest {
    /// Validates optimistic version bound, title, body, diff summary, and provenance bounds.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `expected_version < 1` or any field fails validation.
    pub fn validate(&self) -> Result<(), DennettError> {
        if self.expected_version < 1 {
            return Err(DennettError::InvalidInput {
                field: String::from("expected_version"),
                reason: String::from("must be at least 1"),
            });
        }
        if let Some(ref summary) = self.diff_summary {
            reject_nul(summary, "diff_summary")?;
            if summary.len() > MAX_RATIONALE_BYTES {
                return Err(DennettError::InvalidInput {
                    field: String::from("diff_summary"),
                    reason: format!("exceeds maximum length of {MAX_RATIONALE_BYTES} bytes"),
                });
            }
        }
        validate_document_fields(
            &self.title,
            &self.body,
            &self.provenance_claim_ids,
            &self.provenance_note_ids,
        )
    }
}

/// Response listing [`MemoryDocument`] dossiers for a scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListDocumentsResponse {
    /// Matching documents visible within authorized scopes.
    pub documents: Vec<MemoryDocument>,
}

/// Full immutable version history of a [`MemoryDocument`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetDocumentVersionsResponse {
    /// Logical document identifier.
    pub document_id: DocumentId,
    /// Scope owning this document.
    pub scope: MemoryScope,
    /// Latest committed version number.
    pub current_version: i32,
    /// Ordered list of immutable document versions (`1..=current_version`).
    pub versions: Vec<DocumentVersion>,
}

/// Kind of interruptible background memory consolidation job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConsolidationJobType {
    /// Summarizes raw episode events while preserving caveats and provenance.
    EpisodeSummary,
    /// Refreshes versioned dossiers from active claims and notes.
    DossierUpdate,
    /// Extracts structured claims from admitted notes and evidence.
    ClaimExtraction,
    /// Rebuilds the separable search index from canonical tables.
    IndexRebuild,
    /// Scans active claims for temporal, source, or semantic contradictions.
    ConflictScan,
    /// Sweeps expired ephemeral notes according to retention policy.
    RetentionSweep,
}

impl ConsolidationJobType {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EpisodeSummary => "episode_summary",
            Self::DossierUpdate => "dossier_update",
            Self::ClaimExtraction => "claim_extraction",
            Self::IndexRebuild => "index_rebuild",
            Self::ConflictScan => "conflict_scan",
            Self::RetentionSweep => "retention_sweep",
        }
    }
}

impl fmt::Display for ConsolidationJobType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ConsolidationJobType {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "episode_summary" => Ok(Self::EpisodeSummary),
            "dossier_update" => Ok(Self::DossierUpdate),
            "claim_extraction" => Ok(Self::ClaimExtraction),
            "index_rebuild" => Ok(Self::IndexRebuild),
            "conflict_scan" => Ok(Self::ConflictScan),
            "retention_sweep" => Ok(Self::RetentionSweep),
            other => Err(DennettError::InvalidInput {
                field: String::from("job_type"),
                reason: format!("unknown consolidation job type `{other}`"),
            }),
        }
    }
}

/// Lifecycle state of an interruptible [`ConsolidationJob`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConsolidationJobStatus {
    /// Queued and awaiting execution.
    #[default]
    Pending,
    /// Actively executing against canonical memory storage.
    Running,
    /// Explicitly paused at a durable checkpoint for later resume.
    Paused,
    /// Finished cleanly without losing canonical data or caveats.
    Completed,
    /// Terminated due to a runtime or database fault (checkpoint preserved for recovery).
    Failed,
    /// Explicitly cancelled before completion.
    Cancelled,
}

impl ConsolidationJobStatus {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl fmt::Display for ConsolidationJobStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ConsolidationJobStatus {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "paused" => Ok(Self::Paused),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(DennettError::InvalidInput {
                field: String::from("job_status"),
                reason: format!("unknown consolidation job status `{other}`"),
            }),
        }
    }
}

/// Interruptible background memory consolidation job with durable checkpoint/resume state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolidationJob {
    /// Stable consolidation job identifier.
    pub job_id: ConsolidationJobId,
    /// Scope targeted by this consolidation job.
    pub scope: MemoryScope,
    /// Kind of consolidation pass to run.
    pub job_type: ConsolidationJobType,
    /// Current lifecycle status.
    pub status: ConsolidationJobStatus,
    /// Optional serialized JSON checkpoint payload enabling clean pause/resume recovery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_json: Option<String>,
    /// Last canonical memory event incorporated into this job's watermark.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watermark_event_id: Option<MemoryEventId>,
    /// Confirms whether compression/summarization preserved essential source caveats.
    #[serde(default = "default_true")]
    pub caveats_preserved: bool,
    /// Secret-redacted failure diagnostic when `status == ConsolidationJobStatus::Failed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_detail: Option<String>,
    /// UTC creation timestamp.
    pub created_at: TimestampMs,
    /// UTC timestamp of latest status or checkpoint update.
    pub updated_at: TimestampMs,
    /// UTC timestamp when execution started, if started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<TimestampMs>,
    /// UTC timestamp when execution finished, if terminal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<TimestampMs>,
}

/// Request to schedule or start an interruptible [`ConsolidationJob`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateConsolidationJobRequest {
    /// Target memory scope.
    pub scope: MemoryScope,
    /// Consolidation job type to execute.
    pub job_type: ConsolidationJobType,
    /// Optional caller idempotency key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<IdempotencyKey>,
}

/// Response listing [`ConsolidationJob`] records for a scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListConsolidationJobsResponse {
    /// Consolidation jobs ordered newest first.
    pub jobs: Vec<ConsolidationJob>,
}

/// Live health and freshness state of the separable, rebuildable search index.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum IndexStatus {
    /// Search index watermark is caught up with canonical memory events.
    #[default]
    Healthy,
    /// Index watermark trails the canonical stream head; fresh tail overlay bridges the gap.
    Lagging,
    /// Full index rebuild is actively running; fresh tail overlay keeps recent writes searchable.
    Rebuilding,
    /// Search index is degraded or truncated; fresh tail overlay keeps memory honestly searchable.
    Degraded,
}

impl IndexStatus {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Lagging => "lagging",
            Self::Rebuilding => "rebuilding",
            Self::Degraded => "degraded",
        }
    }
}

impl fmt::Display for IndexStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for IndexStatus {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "healthy" => Ok(Self::Healthy),
            "lagging" => Ok(Self::Lagging),
            "rebuilding" => Ok(Self::Rebuilding),
            "degraded" => Ok(Self::Degraded),
            other => Err(DennettError::InvalidInput {
                field: String::from("index_status"),
                reason: format!("unknown index status `{other}`"),
            }),
        }
    }
}

/// Persisted state of the separable search index for one [`MemoryScope`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexState {
    /// Scope covered by this index state row.
    pub scope: MemoryScope,
    /// Optional identifier of the latest indexed canonical event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watermark_event_id: Option<MemoryEventId>,
    /// Highest canonical stream sequence reflected in `memory_search_index`.
    pub watermark_seq: i64,
    /// Search tokenizer/ranking model version (e.g. `"fts-v1"`).
    pub model_version: String,
    /// Current health/freshness status of the search index.
    pub status: IndexStatus,
    /// UTC timestamp of the latest completed full rebuild, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_rebuild_at: Option<TimestampMs>,
    /// UTC timestamp of the latest state update, if recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<TimestampMs>,
}

/// Request to trigger a full rebuild of `memory_search_index` for a scope (`Admin` capability).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerIndexRebuildRequest {
    /// Scope whose separable search index should be rebuilt from canonical storage.
    pub scope: MemoryScope,
}

/// Result of triggering or completing a full search index rebuild.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerIndexRebuildResponse {
    /// Updated search index state after rebuild.
    pub index_state: IndexState,
    /// Number of search index entries materialized during the rebuild.
    pub entries_rebuilt: u64,
}

/// Live scoped summary statistics returned by `GET /v1/memory/stats` for UI and diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryStatsSummary {
    /// Optional scope represented by these counters (`None` when aggregated across authorized scopes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<MemoryScope>,
    /// Number of stored evidence source objects.
    pub sources_count: u64,
    /// Number of active (non-forgotten, non-rejected) notes.
    pub active_notes_count: u64,
    /// Number of active (non-superseded, non-retracted) claims.
    pub active_claims_count: u64,
    /// Number of versioned dossiers/documents.
    pub documents_count: u64,
    /// Number of currently open conflicts requiring attention.
    pub open_conflicts_count: u64,
    /// Number of items tagged `external_untrusted` from external package imports.
    pub imported_untrusted_count: u64,
    /// Worst-case or scope-specific search index status.
    #[serde(default)]
    pub index_status: IndexStatus,
    /// Highest indexed stream sequence watermark.
    #[serde(default)]
    pub watermark_seq: i64,
    /// UTC timestamp of the last full search index rebuild, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_rebuild_at: Option<TimestampMs>,
}

/// Target entity kind accepted by [`ForgetRequest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ForgetTargetType {
    /// Active memory note (`memory_notes`).
    Note,
    /// Immutable evidence object (`evidence_objects`).
    Evidence,
    /// Structured claim (`memory_claims`).
    Claim,
    /// Versioned dossier document (`memory_documents`).
    Document,
}

impl ForgetTargetType {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Evidence => "evidence",
            Self::Claim => "claim",
            Self::Document => "document",
        }
    }
}

impl fmt::Display for ForgetTargetType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ForgetTargetType {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "note" => Ok(Self::Note),
            "evidence" => Ok(Self::Evidence),
            "claim" => Ok(Self::Claim),
            "document" => Ok(Self::Document),
            other => Err(DennettError::InvalidInput {
                field: String::from("target_type"),
                reason: format!("unknown forget target type `{other}`"),
            }),
        }
    }
}

/// Propagation progress of a forget operation across active projections and search indexes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ForgetPropagationStatus {
    /// Tombstone recorded; purge still pending.
    #[default]
    Pending,
    /// Target removed from `memory_search_index`.
    IndexCleared,
    /// Target removed from active projections (`notes`/`claims`/`documents`).
    ProjectionsCleared,
    /// Target fully purged from active projections and search indexes.
    Completed,
    /// Propagation encountered an error and requires administrative retry.
    Failed,
}

impl ForgetPropagationStatus {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::IndexCleared => "index_cleared",
            Self::ProjectionsCleared => "projections_cleared",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

impl fmt::Display for ForgetPropagationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ForgetPropagationStatus {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "pending" => Ok(Self::Pending),
            "index_cleared" => Ok(Self::IndexCleared),
            "projections_cleared" => Ok(Self::ProjectionsCleared),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            other => Err(DennettError::InvalidInput {
                field: String::from("propagation_status"),
                reason: format!("unknown forget propagation status `{other}`"),
            }),
        }
    }
}

/// Request to forget/delete a memory entity with search-index purge and audit tombstone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgetRequest {
    /// Scope owning the target entity.
    pub scope: MemoryScope,
    /// Kind of entity being forgotten.
    pub target_type: ForgetTargetType,
    /// UUID of the target entity (`note_id`, `evidence_id`, `claim_id`, or `document_id`).
    pub target_id: Uuid,
    /// Required human-readable audit reason (never stores the forgotten content).
    pub reason: String,
    /// Caller idempotency key preventing duplicate forget tombstones on retries.
    pub idempotency_key: IdempotencyKey,
}

impl ForgetRequest {
    /// Validates forget audit reason bounds.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `reason` is empty, oversized, or contains NUL.
    pub fn validate(&self) -> Result<(), DennettError> {
        reject_nul(&self.reason, "reason")?;
        if self.reason.trim().is_empty() || self.reason.len() > MAX_FORGET_REASON_BYTES {
            return Err(DennettError::InvalidInput {
                field: String::from("reason"),
                reason: format!("must be non-empty and at most {MAX_FORGET_REASON_BYTES} bytes"),
            });
        }
        Ok(())
    }
}

/// Result of forgetting a memory entity, including honest backup-retention disclosure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgetResponse {
    /// Stable identifier of the recorded content-free audit tombstone.
    pub tombstone_id: TombstoneId,
    /// Append-only forget event identifier.
    pub forget_event_id: MemoryEventId,
    /// Honest disclosure noting that forgotten content may still exist in offline backups until rotated.
    pub residual_backup_notice: String,
    /// Propagation status across active projections and separable search indexes.
    pub propagation_status: ForgetPropagationStatus,
    /// `true` when this response was served from an existing matching idempotency record.
    #[serde(default)]
    pub idempotent_replay: bool,
}

/// Content-free audit tombstone proving a memory entity was forgotten.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgetTombstone {
    /// Stable tombstone identifier.
    pub tombstone_id: TombstoneId,
    /// Monotonic positive sequence number for V15-safe cursor-based tombstone streaming.
    #[serde(default)]
    pub tombstone_seq: i64,
    /// Scope from which the entity was forgotten.
    pub scope: MemoryScope,
    /// Entity kind that was forgotten.
    pub target_type: ForgetTargetType,
    /// UUID of the forgotten entity.
    pub target_id: Uuid,
    /// Append-only event recording the forget action.
    pub forget_event_id: MemoryEventId,
    /// Verified principal who initiated the forget action.
    pub principal_id: PrincipalId,
    /// Audit reason for forgetting (must NEVER contain the forgotten payload).
    pub reason: String,
    /// Honest disclosure regarding offline backup retention.
    pub residual_backup_notice: String,
    /// Current propagation status across projections and search indexes.
    pub propagation_status: ForgetPropagationStatus,
    /// UTC timestamp when the tombstone was recorded.
    pub created_at: TimestampMs,
}

/// Response listing [`ForgetTombstone`] audit records for a scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListTombstonesResponse {
    /// Recorded content-free tombstones (ordered `tombstone_seq DESC` by default, or
    /// `tombstone_seq ASC` when streaming with `after_seq`).
    pub tombstones: Vec<ForgetTombstone>,
    /// Highest monotonic `tombstone_seq` observed in this page (or echoed `after_seq` cursor),
    /// used for incremental V15-safe tombstone polling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_after_seq: Option<i64>,
}

/// Request to export a deterministic, secret-free memory package for one [`MemoryScope`] (Contract J).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportRequest {
    /// Scope whose memory records should be exported.
    pub scope: MemoryScope,
    /// Optional requested export format version (defaults to `"1.0"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format_version: Option<String>,
}

/// Deterministic, secret-stripped exported memory package (Contract J).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportResponse {
    /// Stable export package audit identifier.
    pub package_id: ExportPackageId,
    /// Scope covered by this export.
    pub scope: MemoryScope,
    /// Export format version (e.g. `"1.0"`).
    pub format_version: String,
    /// Deterministic lowercase SHA-256 hex digest of `package_json`.
    pub manifest_sha256: Sha256Hex,
    /// Total number of exported domain items (notes, claims, documents, etc.).
    pub item_count: u64,
    /// Exact UTF-8 byte length of `package_json`.
    pub byte_size: u64,
    /// Confirms secret patterns were stripped prior to export (always `true`).
    #[serde(default = "default_true")]
    pub secrets_stripped: bool,
    /// Deterministic canonical UTF-8 JSON representation of the exported package.
    pub package_json: String,
}

/// Request to import an external memory package into a [`MemoryScope`] as untrusted data (Contract J).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportRequest {
    /// Destination scope for imported items.
    pub scope: MemoryScope,
    /// Canonical UTF-8 JSON package payload to validate and import.
    pub package_json: String,
}

impl ImportRequest {
    /// Validates non-empty bounded UTF-8 package JSON payload without NUL bytes.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `package_json` is empty, exceeds
    /// [`MAX_PACKAGE_JSON_BYTES`], or contains NUL bytes.
    pub fn validate(&self) -> Result<(), DennettError> {
        reject_nul(&self.package_json, "package_json")?;
        if self.package_json.trim().is_empty() || self.package_json.len() > MAX_PACKAGE_JSON_BYTES {
            return Err(DennettError::InvalidInput {
                field: String::from("package_json"),
                reason: format!("must be non-empty and at most {MAX_PACKAGE_JSON_BYTES} bytes"),
            });
        }
        Ok(())
    }
}

/// Result of importing an external memory package (Contract J).
///
/// Imported material ALWAYS enters with `trust_level = TrustLevel::ExternalUntrusted`
/// and `instruction_active = false`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportResponse {
    /// Stable import audit record identifier.
    pub import_id: ImportRecordId,
    /// Destination scope.
    pub scope: MemoryScope,
    /// SHA-256 digest of the submitted package payload.
    pub source_package_sha256: Sha256Hex,
    /// Assigned provenance trust tier (always [`TrustLevel::ExternalUntrusted`] on import).
    pub trust_level: TrustLevel,
    /// Confirms imported content cannot execute as instructions (always `false` on import).
    pub instruction_active: bool,
    /// Number of valid items accepted into storage.
    pub items_imported: u64,
    /// Number of invalid or unsafe items rejected during validation.
    pub items_rejected: u64,
    /// Safe, bounded validation errors explaining rejected items.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validation_errors: Vec<String>,
}

fn validate_bounded_key(value: &str, field: &str, max_len: usize) -> Result<(), DennettError> {
    reject_nul(value, field)?;
    let trimmed = value.trim();
    if trimmed.is_empty() || value.len() > max_len {
        return Err(DennettError::InvalidInput {
            field: field.to_owned(),
            reason: format!("must be non-empty and at most {max_len} bytes"),
        });
    }
    if value.bytes().any(|b| b.is_ascii_control()) {
        return Err(DennettError::InvalidInput {
            field: field.to_owned(),
            reason: String::from("must not contain ASCII control characters"),
        });
    }
    Ok(())
}

fn validate_document_fields(
    title: &str,
    body: &str,
    provenance_claim_ids: &[ClaimId],
    provenance_note_ids: &[MemoryNoteId],
) -> Result<(), DennettError> {
    reject_nul(title, "title")?;
    if title.trim().is_empty() || title.len() > MAX_DOCUMENT_TITLE_BYTES {
        return Err(DennettError::InvalidInput {
            field: String::from("title"),
            reason: format!("must be non-empty and at most {MAX_DOCUMENT_TITLE_BYTES} bytes"),
        });
    }
    reject_nul(body, "body")?;
    if body.trim().is_empty() || body.len() > MAX_DOCUMENT_BODY_BYTES {
        return Err(DennettError::InvalidInput {
            field: String::from("body"),
            reason: format!("must be non-empty and at most {MAX_DOCUMENT_BODY_BYTES} bytes"),
        });
    }
    if provenance_claim_ids.len() > MAX_PROVENANCE_IDS {
        return Err(DennettError::InvalidInput {
            field: String::from("provenance_claim_ids"),
            reason: format!("exceeds maximum of {MAX_PROVENANCE_IDS} claim IDs"),
        });
    }
    if provenance_note_ids.len() > MAX_PROVENANCE_IDS {
        return Err(DennettError::InvalidInput {
            field: String::from("provenance_note_ids"),
            reason: format!("exceeds maximum of {MAX_PROVENANCE_IDS} note IDs"),
        });
    }
    Ok(())
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
    assert_send_sync::<AdmissionStatus>();
    assert_send_sync::<TrustLevel>();
    assert_send_sync::<ClaimId>();
    assert_send_sync::<EpistemicStatus>();
    assert_send_sync::<MemoryClaim>();
    assert_send_sync::<CreateClaimRequest>();
    assert_send_sync::<SupersedeClaimRequest>();
    assert_send_sync::<ListClaimsResponse>();
    assert_send_sync::<ConflictId>();
    assert_send_sync::<ConflictType>();
    assert_send_sync::<ConflictStatus>();
    assert_send_sync::<MemoryConflict>();
    assert_send_sync::<ListConflictsResponse>();
    assert_send_sync::<ResolveConflictRequest>();
    assert_send_sync::<DocumentId>();
    assert_send_sync::<DocumentVersionId>();
    assert_send_sync::<MemoryDocument>();
    assert_send_sync::<DocumentVersion>();
    assert_send_sync::<CreateDocumentRequest>();
    assert_send_sync::<UpdateDocumentRequest>();
    assert_send_sync::<ListDocumentsResponse>();
    assert_send_sync::<GetDocumentVersionsResponse>();
    assert_send_sync::<ConsolidationJobId>();
    assert_send_sync::<ConsolidationJobType>();
    assert_send_sync::<ConsolidationJobStatus>();
    assert_send_sync::<ConsolidationJob>();
    assert_send_sync::<CreateConsolidationJobRequest>();
    assert_send_sync::<ListConsolidationJobsResponse>();
    assert_send_sync::<IndexStatus>();
    assert_send_sync::<IndexState>();
    assert_send_sync::<TriggerIndexRebuildRequest>();
    assert_send_sync::<TriggerIndexRebuildResponse>();
    assert_send_sync::<MemoryStatsSummary>();
    assert_send_sync::<TombstoneId>();
    assert_send_sync::<ForgetTargetType>();
    assert_send_sync::<ForgetPropagationStatus>();
    assert_send_sync::<ForgetRequest>();
    assert_send_sync::<ForgetResponse>();
    assert_send_sync::<ForgetTombstone>();
    assert_send_sync::<ListTombstonesResponse>();
    assert_send_sync::<ExportPackageId>();
    assert_send_sync::<ImportRecordId>();
    assert_send_sync::<ExportRequest>();
    assert_send_sync::<ExportResponse>();
    assert_send_sync::<ImportRequest>();
    assert_send_sync::<ImportResponse>();
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
            admission_status: None,
            salience: None,
            topic_key: None,
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
            include_history: false,
        };
        assert!(matches!(
            bad_search.validated_limit(),
            Err(DennettError::InvalidInput { .. })
        ));
    }

    #[test]
    fn d3_memory_fabric_contracts_default_and_validate_cleanly() {
        let scope = MemoryScope::try_new("project-d3").expect("valid scope");
        let key = IdempotencyKey::try_new("op-d3-1").expect("valid key");

        // Backward-compatible deserialization of D1 CreateNoteRequest / SearchNotesRequest / SearchNotesResponse.
        let legacy_note_json =
            format!(r#"{{"scope":"{scope}","title":"T","body":"B","idempotency_key":"{key}"}}"#);
        let decoded_note: CreateNoteRequest =
            serde_json::from_str(&legacy_note_json).expect("legacy CreateNoteRequest");
        assert!(decoded_note.admission_status.is_none());
        assert!(decoded_note.salience.is_none());
        assert!(decoded_note.topic_key.is_none());
        assert!(decoded_note.validate().is_ok());

        let bad_salience = CreateNoteRequest {
            salience: Some(101),
            ..decoded_note
        };
        assert!(bad_salience.validate().is_err());

        let legacy_search_resp: SearchNotesResponse =
            serde_json::from_str(r#"{"notes":[],"total_matched":0,"truncated":false}"#)
                .expect("legacy SearchNotesResponse");
        assert_eq!(legacy_search_resp.index_status, IndexStatus::Healthy);
        assert!(legacy_search_resp.stale_warning.is_none());
        assert_eq!(legacy_search_resp.claims, Vec::new());
        assert_eq!(legacy_search_resp.documents, Vec::new());
        let encoded_unified =
            serde_json::to_string(&legacy_search_resp).expect("serialize unified search response");
        assert!(encoded_unified.contains(r#""claims":[]"#));
        assert!(encoded_unified.contains(r#""documents":[]"#));

        let claim_req = CreateClaimRequest {
            scope: scope.clone(),
            topic_key: String::from("architecture:memory"),
            claim_key: Some(String::from("search_backend")),
            natural_language: String::from("Stock PG16 FTS is used without pgvector"),
            epistemic_status: EpistemicStatus::Observed,
            supersedes_claim_id: None,
            valid_from: Some(TimestampMs::from_millis(1_700_000_000_000)),
            valid_to: None,
            source_note_id: None,
            caveats: vec![String::from("Fresh tail overlay bridges lagging indexes")],
            provenance: BTreeMap::from([(String::from("source"), String::from("spec-10"))]),
            evidence_links: vec![],
            idempotency_key: key.clone(),
        };
        assert!(claim_req.validate().is_ok());

        let inverted_claim = CreateClaimRequest {
            valid_from: Some(TimestampMs::from_millis(2_000)),
            valid_to: Some(TimestampMs::from_millis(1_000)),
            ..claim_req
        };
        assert!(inverted_claim.validate().is_err());

        let forget_req = ForgetRequest {
            scope,
            target_type: ForgetTargetType::Note,
            target_id: Uuid::new_v4(),
            reason: String::from("Owner requested removal"),
            idempotency_key: key,
        };
        assert!(forget_req.validate().is_ok());
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
