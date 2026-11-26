//! Write-only encrypted `SecretBroker` backed by pure-Rust ChaCha20-Poly1305 AEAD
//! and PostgreSQL `secret_entries` (Spec 30 §17, D3).
//!
//! Security guarantees:
//! - Master key (32 bytes) is loaded from `DENNETT_VAULT_MASTER_KEY` (hex or base64) or
//!   auto-generated outside PostgreSQL in `<vault_dir>/master.key` (`0o600` on Unix) and
//!   wrapped in `ZeroizeOnDrop`.
//! - Every secret is encrypted with a fresh random 12-byte nonce and AAD bound to `slot_key`,
//!   preventing cross-slot ciphertext swaps in the database.
//! - Public API (`list_secrets`, `set_secret`, `delete_secret`) is strictly **write-only**:
//!   it NEVER returns plaintext secret material, only `SecretMetadataRecord` with a non-reversible
//!   16-hex-character `fingerprint_short` and monotonic `version`.
//! - Plaintext material is resolved strictly internally via `resolve_secret_plaintext` and is
//!   immediately registered in `HeadService`'s live secret redactor so errors, audit events,
//!   and tool excerpts can never leak credentials.

use core::fmt;
use core::str::FromStr as _;
use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use dennett_contracts::redact::REDACTED_PLACEHOLDER;
use dennett_contracts::{
    CorrelationId, DennettError, ListSecretsResponse, ProjectId, SecretMetadataRecord,
    SecretSource, SetSecretRequest, TimestampMs, validate_secret_slot_key,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

use crate::events::{ControlEventAppend, append_control_event_in_tx};
use crate::service::{DEFAULT_PG_OP_TIMEOUT, HeadService};
use crate::store::{append_audit_event, map_pg_err};

/// Canonical AEAD algorithm identifier stored in `secret_entries.algorithm`.
pub const SECRET_AEAD_ALGORITHM: &str = "chacha20poly1305-v1";
/// Well-known secret slot key for headless `gemini-api-key` execution.
pub const GEMINI_API_KEY_SLOT: &str = "gemini_api_key";
/// Environment variable supplying an optional master key override (64-hex or base64 32-byte key).
pub const VAULT_MASTER_KEY_ENV: &str = "DENNETT_VAULT_MASTER_KEY";

/// Zeroized 32-byte symmetric master key used by [`SecretCipher`].
#[derive(Clone)]
pub struct VaultMasterKey {
    bytes: [u8; 32],
}

impl Drop for VaultMasterKey {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

impl fmt::Debug for VaultMasterKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VaultMasterKey")
            .field("bytes", &REDACTED_PLACEHOLDER)
            .finish()
    }
}

impl VaultMasterKey {
    /// Constructs a master key from exact 32 raw bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }

    /// Loads the 32-byte vault master key from `DENNETT_VAULT_MASTER_KEY` (if set and non-empty),
    /// or loads/auto-generates `<vault_dir>/master.key` outside PostgreSQL (`0o600` on Unix).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::SafetyGuardRejected`] if the configured key is malformed, or
    /// [`DennettError::Internal`] if creating/reading `<vault_dir>/master.key` fails.
    pub fn load_or_create(
        vault_dir: &Path,
        explicit_master_key: Option<&str>,
    ) -> Result<Self, DennettError> {
        if let Some(raw_explicit) = explicit_master_key.map(str::trim).filter(|s| !s.is_empty()) {
            let bytes = decode_master_key_material(raw_explicit).map_err(|reason| {
                DennettError::SafetyGuardRejected {
                    reason: format!("invalid explicit vault master key: {reason}"),
                }
            })?;
            return Ok(Self::from_bytes(bytes));
        }

        if let Ok(raw_env) = std::env::var(VAULT_MASTER_KEY_ENV) {
            let trimmed = raw_env.trim();
            if !trimmed.is_empty() {
                let bytes = decode_master_key_material(trimmed).map_err(|reason| {
                    DennettError::SafetyGuardRejected {
                        reason: format!("invalid `{VAULT_MASTER_KEY_ENV}`: {reason}"),
                    }
                })?;
                return Ok(Self::from_bytes(bytes));
            }
        }

        let key_path = vault_dir.join("master.key");
        if key_path.exists() {
            let content = fs::read_to_string(&key_path).map_err(|err| DennettError::Internal {
                detail: format!(
                    "failed reading vault master key `{}`: {err}",
                    key_path.display()
                ),
            })?;
            let bytes = decode_master_key_material(content.trim()).map_err(|reason| {
                DennettError::SafetyGuardRejected {
                    reason: format!(
                        "corrupt vault master key `{}`: {reason}",
                        key_path.display()
                    ),
                }
            })?;
            return Ok(Self::from_bytes(bytes));
        }

        fs::create_dir_all(vault_dir).map_err(|err| DennettError::Internal {
            detail: format!(
                "failed creating vault directory `{}`: {err}",
                vault_dir.display()
            ),
        })?;

        let generated = ChaCha20Poly1305::generate_key(&mut OsRng);
        let mut raw = [0_u8; 32];
        raw.copy_from_slice(&generated);
        let encoded = Zeroizing::new(encode_hex(&raw));
        write_master_key_file(&key_path, encoded.as_bytes())?;
        Ok(Self::from_bytes(raw))
    }
}

/// Pure-Rust ChaCha20-Poly1305 AEAD cipher bound to a [`VaultMasterKey`].
#[derive(Clone, Debug)]
pub struct SecretCipher {
    master_key: VaultMasterKey,
}

/// Encrypted envelope produced by [`SecretCipher::encrypt`].
#[derive(Debug, Clone)]
pub struct EncryptedSecret {
    /// 12-byte random AEAD nonce.
    pub nonce: Vec<u8>,
    /// Authenticated ciphertext bytes (including 16-byte Poly1305 tag).
    pub ciphertext: Vec<u8>,
    /// Non-reversible 16-hex-character fingerprint prefix for rotation verification.
    pub fingerprint_short: String,
}

impl SecretCipher {
    /// Creates a new [`SecretCipher`] from a loaded [`VaultMasterKey`].
    #[must_use]
    pub const fn new(master_key: VaultMasterKey) -> Self {
        Self { master_key }
    }

    /// Encrypts `plaintext` for `slot_key` with a random 12-byte nonce and `slot_key` as AAD.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::Internal`] if AEAD encryption fails.
    pub fn encrypt(
        &self,
        slot_key: &str,
        plaintext: &str,
    ) -> Result<EncryptedSecret, DennettError> {
        let key = Key::from_slice(&self.master_key.bytes);
        let cipher = ChaCha20Poly1305::new(key);
        let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext.as_bytes(),
                    aad: slot_key.as_bytes(),
                },
            )
            .map_err(|_| DennettError::Internal {
                detail: String::from("ChaCha20-Poly1305 secret encryption failed"),
            })?;
        let fingerprint_short = compute_secret_fingerprint(slot_key, plaintext);
        Ok(EncryptedSecret {
            nonce: nonce.to_vec(),
            ciphertext,
            fingerprint_short,
        })
    }

    /// Decrypts and authenticates ciphertext stored for `slot_key`, returning a [`Zeroizing`] string.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::SafetyGuardRejected`] on nonce mismatch, authentication tag failure,
    /// cross-slot tampering, or invalid UTF-8 plaintext.
    pub fn decrypt(
        &self,
        slot_key: &str,
        nonce: &[u8],
        ciphertext: &[u8],
    ) -> Result<Zeroizing<String>, DennettError> {
        if nonce.len() != 12 {
            return Err(DennettError::SafetyGuardRejected {
                reason: format!(
                    "secret slot `{slot_key}` has invalid AEAD nonce length ({} != 12)",
                    nonce.len()
                ),
            });
        }
        let key = Key::from_slice(&self.master_key.bytes);
        let cipher = ChaCha20Poly1305::new(key);
        let plaintext_bytes = Zeroizing::new(
            cipher
                .decrypt(
                    Nonce::from_slice(nonce),
                    Payload {
                        msg: ciphertext,
                        aad: slot_key.as_bytes(),
                    },
                )
                .map_err(|_| DennettError::SafetyGuardRejected {
                    reason: format!(
                        "failed authenticating encrypted secret slot `{slot_key}` (wrong master key or tampered ciphertext)"
                    ),
                })?,
        );
        let utf8 = String::from_utf8(plaintext_bytes.to_vec()).map_err(|_| {
            DennettError::SafetyGuardRejected {
                reason: format!("decrypted secret slot `{slot_key}` is not valid UTF-8"),
            }
        })?;
        Ok(Zeroizing::new(utf8))
    }
}

/// Computes a non-reversible 16-hex-character fingerprint prefix for a `(slot_key, plaintext)` pair.
#[must_use]
pub fn compute_secret_fingerprint(slot_key: &str, plaintext: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"dennett-secret-fp-v1:");
    hasher.update(slot_key.as_bytes());
    hasher.update(b":");
    hasher.update(plaintext.as_bytes());
    let digest = hasher.finalize();
    encode_hex(&digest[..8])
}

/// Resolves the default vault directory outside PostgreSQL when not explicitly provided.
#[must_use]
pub fn default_vault_dir(detached_projects_root: &Path) -> PathBuf {
    detached_projects_root.join("vault")
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn decode_hex_32(input: &str) -> Option<[u8; 32]> {
    if input.len() != 64 {
        return None;
    }
    let mut out = [0_u8; 32];
    let bytes = input.as_bytes();
    for i in 0..32 {
        let hi = hex_nibble(bytes[i * 2])?;
        let lo = hex_nibble(bytes[i * 2 + 1])?;
        out[i] = (hi << 4) | lo;
    }
    Some(out)
}

const fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn decode_master_key_material(raw: &str) -> Result<[u8; 32], String> {
    if let Some(hex_bytes) = decode_hex_32(raw) {
        return Ok(hex_bytes);
    }
    if let Ok(b64) = STANDARD.decode(raw)
        && b64.len() == 32
    {
        let mut out = [0_u8; 32];
        out.copy_from_slice(&b64);
        return Ok(out);
    }
    if let Ok(b64u) = URL_SAFE_NO_PAD.decode(raw)
        && b64u.len() == 32
    {
        let mut out = [0_u8; 32];
        out.copy_from_slice(&b64u);
        return Ok(out);
    }
    Err(String::from(
        "must be 64 hex characters or base64 encoding of exactly 32 bytes",
    ))
}

/// Writes sensitive key or token material to `path` with strict owner-only filesystem permissions:
/// `0o600` on Unix and an owner-only DACL (inheritance stripped) on Windows (`A-SEC-1`, `A-SEC-3`).
///
/// # Errors
///
/// Returns [`DennettError::Internal`] if writing the file or applying restrictive ACLs fails.
pub fn write_owner_only_file(path: &Path, content: &[u8]) -> Result<(), DennettError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|err| DennettError::Internal {
            detail: format!(
                "failed creating parent directory for `{}`: {err}",
                path.display()
            ),
        })?;
    }

    #[cfg(unix)]
    {
        use std::io::Write as _;
        use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};

        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|err| DennettError::Internal {
                detail: format!("failed creating secret file `{}`: {err}", path.display()),
            })?;
        file.write_all(content)
            .map_err(|err| DennettError::Internal {
                detail: format!("failed writing secret file `{}`: {err}", path.display()),
            })?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|err| {
            DennettError::Internal {
                detail: format!("failed tightening permissions on `{}`: {err}", path.display()),
            }
        })?;
        Ok(())
    }

    #[cfg(windows)]
    {
        fs::write(path, content).map_err(|err| DennettError::Internal {
            detail: format!("failed writing secret file `{}`: {err}", path.display()),
        })?;
        apply_windows_owner_only_dacl(path)?;
        Ok(())
    }

    #[cfg(not(any(unix, windows)))]
    {
        fs::write(path, content).map_err(|err| DennettError::Internal {
            detail: format!("failed writing secret file `{}`: {err}", path.display()),
        })
    }
}

#[cfg(windows)]
fn resolve_current_windows_principal() -> Option<String> {
    use std::sync::OnceLock;
    static PRINCIPAL: OnceLock<Option<String>> = OnceLock::new();
    PRINCIPAL
        .get_or_init(|| {
            if let Ok(output) = std::process::Command::new("whoami")
                .args(["/user", "/fo", "csv", "/nh"])
                .output()
                && output.status.success()
            {
                let text = String::from_utf8_lossy(&output.stdout);
                if let Some(line) = text.lines().map(str::trim).find(|l| !l.is_empty())
                    && let Some((_, sid_part)) = line.rsplit_once(',')
                {
                    let sid = sid_part.trim().trim_matches('"');
                    if sid.starts_with("S-1-")
                        && sid
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                    {
                        return Some(format!("*{sid}"));
                    }
                }
            }
            let user = std::env::var("USERNAME").ok()?;
            let trimmed = user.trim();
            if trimmed.is_empty() {
                return None;
            }
            if let Ok(domain) = std::env::var("USERDOMAIN") {
                let d = domain.trim();
                if !d.is_empty() {
                    return Some(format!("{d}\\{trimmed}"));
                }
            }
            Some(trimmed.to_owned())
        })
        .clone()
}

#[cfg(windows)]
fn apply_windows_owner_only_dacl(path: &Path) -> Result<(), DennettError> {
    let Some(principal) = resolve_current_windows_principal() else {
        return Err(DennettError::Internal {
            detail: format!(
                "unable to determine current Windows user SID to protect `{}`",
                path.display()
            ),
        });
    };
    let grant_spec = format!("{principal}:(F)");
    let output = std::process::Command::new("icacls")
        .arg(path)
        .args(["/inheritance:r", "/grant:r", &grant_spec])
        .output()
        .map_err(|err| DennettError::Internal {
            detail: format!(
                "failed invoking icacls to protect `{}`: {err}",
                path.display()
            ),
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(DennettError::Internal {
            detail: format!(
                "icacls failed protecting `{}` (status={}): {} {}",
                path.display(),
                output.status,
                stdout.trim(),
                stderr.trim()
            ),
        });
    }
    Ok(())
}

fn write_master_key_file(path: &Path, content: &[u8]) -> Result<(), DennettError> {
    write_owner_only_file(path, content)
}

/// Enforces `secret_entries.scope` containment (`"global"` vs `"project:<uuid>"`, `A-SEC-3`).
///
/// Fails closed on cross-project access, standalone access to project secrets, or malformed scopes.
pub(crate) fn enforce_secret_scope(
    slot_key: &str,
    entry_scope: &str,
    caller_project_id: Option<ProjectId>,
) -> Result<(), DennettError> {
    let trimmed = entry_scope.trim();
    if trimmed == "global" {
        return Ok(());
    }
    if let Some(raw_pid) = trimmed.strip_prefix("project:") {
        let target_pid = ProjectId::from_str(raw_pid).map_err(|_| DennettError::AccessDenied {
            scope: trimmed.to_owned(),
            reason: format!(
                "secret slot `{slot_key}` has malformed project scope `{trimmed}`; failing closed"
            ),
        })?;
        if caller_project_id == Some(target_pid) {
            return Ok(());
        }
        return Err(DennettError::AccessDenied {
            scope: trimmed.to_owned(),
            reason: format!(
                "secret slot `{slot_key}` is restricted to project `{target_pid}` and cannot be resolved for caller project {caller_project_id:?}"
            ),
        });
    }
    Err(DennettError::AccessDenied {
        scope: trimmed.to_owned(),
        reason: format!(
            "secret slot `{slot_key}` has unrecognized scope `{trimmed}`; failing closed"
        ),
    })
}

impl HeadService {
    /// Lists write-only metadata for all configured secret slots (`GET /v1/secrets`).
    ///
    /// NEVER selects or returns plaintext credentials. Includes `"gemini_api_key"` status
    /// (`vault`, `env_fallback`, or `unset`) so Settings always displays current key provenance.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] on PostgreSQL failure.
    pub async fn list_secrets(&self) -> Result<ListSecretsResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let rows = conn
            .query(
                "SELECT slot_key, scope, description, fingerprint_short, version,
                        TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint
                 FROM secret_entries
                 ORDER BY slot_key ASC",
                &[],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let mut secrets = Vec::with_capacity(rows.len() + 1);
        let mut saw_gemini = false;
        for row in rows {
            let slot_key: String = row.get(0);
            if slot_key == GEMINI_API_KEY_SLOT {
                saw_gemini = true;
            }
            secrets.push(SecretMetadataRecord {
                slot_key,
                scope: row.get(1),
                description: row.get(2),
                configured: true,
                version: row.get(4),
                fingerprint_short: Some(row.get(3)),
                source: SecretSource::Vault,
                updated_at: Some(TimestampMs::from_millis(row.get(5))),
            });
        }

        if !saw_gemini {
            secrets.push(self.build_gemini_env_or_unset_metadata());
        }

        secrets.sort_by(|a, b| a.slot_key.cmp(&b.slot_key));
        Ok(ListSecretsResponse { secrets })
    }

    /// Encrypts and stores or rotates a write-only secret slot (`PUT /v1/secrets/{slot_key}`).
    ///
    /// Registers plaintext immediately in Head's secret redactor, encrypts via ChaCha20-Poly1305
    /// AEAD bound to `slot_key`, persists ciphertext into `secret_entries`, and hot-invalidates
    /// cached runtime adapters referencing `slot_key`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] or [`DennettError::DatabaseUnavailable`].
    pub async fn set_secret(
        &self,
        slot_key: &str,
        req: SetSecretRequest,
        correlation_id: &CorrelationId,
    ) -> Result<SecretMetadataRecord, DennettError> {
        validate_secret_slot_key(slot_key)?;
        req.validate()?;
        let _ = self.ensure_migrated().await?;

        // Register plaintext BEFORE any DB or audit call so errors can NEVER leak it.
        self.register_secret_for_redaction(&req.value);

        let encrypted = self.secret_cipher().encrypt(slot_key, &req.value)?;

        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let row = tx
            .query_one(
                "INSERT INTO secret_entries (
                    slot_key, scope, description, algorithm, nonce, ciphertext,
                    fingerprint_short, version
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, 1)
                 ON CONFLICT (slot_key)
                 DO UPDATE SET
                    scope = EXCLUDED.scope,
                    description = EXCLUDED.description,
                    algorithm = EXCLUDED.algorithm,
                    nonce = EXCLUDED.nonce,
                    ciphertext = EXCLUDED.ciphertext,
                    fingerprint_short = EXCLUDED.fingerprint_short,
                    version = secret_entries.version + 1,
                    updated_at = now()
                 RETURNING version, TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint",
                &[
                    &slot_key,
                    &req.scope.as_str(),
                    &req.description.trim(),
                    &SECRET_AEAD_ALGORITHM,
                    &encrypted.nonce,
                    &encrypted.ciphertext,
                    &encrypted.fingerprint_short.as_str(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let version: i64 = row.get(0);
        let updated_at = TimestampMs::from_millis(row.get(1));

        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            None,
            None,
            "secret_broker",
            "secret_rotated",
            &format!(
                "updated encrypted secret slot `{slot_key}` scope={} version={version} fp={}",
                req.scope, encrypted.fingerprint_short
            ),
        )
        .await?;

        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("secret:{slot_key}"),
                event_type: "secret.updated",
                project_id: None,
                session_id: None,
                task_id: None,
                correlation_id,
                payload: json!({
                    "slot_key": slot_key,
                    "scope": req.scope,
                    "version": version,
                    "fingerprint_short": encrypted.fingerprint_short,
                    "source": SecretSource::Vault,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        self.invalidate_runtimes_for_secret_slot(slot_key).await;

        Ok(SecretMetadataRecord {
            slot_key: slot_key.to_owned(),
            scope: req.scope,
            description: req.description.trim().to_owned(),
            configured: true,
            version,
            fingerprint_short: Some(encrypted.fingerprint_short),
            source: SecretSource::Vault,
            updated_at: Some(updated_at),
        })
    }

    /// Removes a secret slot from `secret_entries`, resets cached runtimes referencing it,
    /// and returns post-delete write-only metadata (`DELETE /v1/secrets/{slot_key}`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::NotFound`], or
    /// [`DennettError::DatabaseUnavailable`].
    pub async fn delete_secret(
        &self,
        slot_key: &str,
        correlation_id: &CorrelationId,
    ) -> Result<SecretMetadataRecord, DennettError> {
        validate_secret_slot_key(slot_key)?;
        let _ = self.ensure_migrated().await?;

        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let deleted = tx
            .query_opt(
                "DELETE FROM secret_entries
                 WHERE slot_key = $1
                 RETURNING scope, description",
                &[&slot_key],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let Some(row) = deleted else {
            return Err(DennettError::NotFound {
                resource: String::from("secret"),
                id: slot_key.to_owned(),
            });
        };
        let scope: String = row.get(0);
        let description: String = row.get(1);

        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            None,
            None,
            "secret_broker",
            "secret_deleted",
            &format!("deleted encrypted secret slot `{slot_key}`"),
        )
        .await?;

        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("secret:{slot_key}"),
                event_type: "secret.deleted",
                project_id: None,
                session_id: None,
                task_id: None,
                correlation_id,
                payload: json!({
                    "slot_key": slot_key,
                    "deleted": true,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        self.invalidate_runtimes_for_secret_slot(slot_key).await;

        if slot_key == GEMINI_API_KEY_SLOT {
            return Ok(self.build_gemini_env_or_unset_metadata());
        }

        Ok(SecretMetadataRecord {
            slot_key: slot_key.to_owned(),
            scope,
            description,
            configured: false,
            version: 0,
            fingerprint_short: None,
            source: SecretSource::Unset,
            updated_at: None,
        })
    }

    /// Resolves plaintext secret material strictly internally for runtime/MCP env injection,
    /// enforcing `secret_entries.scope` containment against `caller_project_id` (`A-SEC-3`).
    ///
    /// Automatically registers resolved plaintext in `HeadService`'s secret redactor so no
    /// downstream error or diagnostic message can ever leak it.
    pub(crate) async fn resolve_secret_plaintext(
        &self,
        slot_key: &str,
        caller_project_id: Option<ProjectId>,
    ) -> Result<Option<(Zeroizing<String>, SecretSource, i64)>, DennettError> {
        validate_secret_slot_key(slot_key)?;
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let row = conn
            .query_opt(
                "SELECT scope, nonce, ciphertext, version FROM secret_entries WHERE slot_key = $1",
                &[&slot_key],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        if let Some(row) = row {
            let entry_scope: String = row.get(0);
            enforce_secret_scope(slot_key, &entry_scope, caller_project_id)?;
            let nonce: Vec<u8> = row.get(1);
            let ciphertext: Vec<u8> = row.get(2);
            let version: i64 = row.get(3);
            let plaintext = self
                .secret_cipher()
                .decrypt(slot_key, &nonce, &ciphertext)?;
            self.register_secret_for_redaction(&plaintext);
            return Ok(Some((plaintext, SecretSource::Vault, version)));
        }

        if slot_key == GEMINI_API_KEY_SLOT
            && let Some(env_val) = self.env_gemini_api_key()
        {
            self.register_secret_for_redaction(&env_val);
            return Ok(Some((
                Zeroizing::new(env_val),
                SecretSource::EnvFallback,
                0,
            )));
        }

        Ok(None)
    }

    fn build_gemini_env_or_unset_metadata(&self) -> SecretMetadataRecord {
        if let Some(env_val) = self.env_gemini_api_key() {
            self.register_secret_for_redaction(&env_val);
            let fp = compute_secret_fingerprint(GEMINI_API_KEY_SLOT, &env_val);
            SecretMetadataRecord {
                slot_key: String::from(GEMINI_API_KEY_SLOT),
                scope: String::from("global"),
                description: String::from("Headless Gemini API key (environment fallback)"),
                configured: true,
                version: 0,
                fingerprint_short: Some(fp),
                source: SecretSource::EnvFallback,
                updated_at: None,
            }
        } else {
            SecretMetadataRecord {
                slot_key: String::from(GEMINI_API_KEY_SLOT),
                scope: String::from("global"),
                description: String::from("Headless Gemini API key for background tasks"),
                configured: false,
                version: 0,
                fingerprint_short: None,
                source: SecretSource::Unset,
                updated_at: None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_cipher_encrypts_authenticates_and_rejects_cross_slot_tampering() {
        let key = VaultMasterKey::from_bytes([0x42; 32]);
        let cipher = SecretCipher::new(key);

        let plaintext = "AIzaSy-super-secret-test-value-9999";
        let enc1 = cipher
            .encrypt("gemini_api_key", plaintext)
            .expect("encrypt 1");
        let enc2 = cipher
            .encrypt("gemini_api_key", plaintext)
            .expect("encrypt 2");

        assert_eq!(enc1.nonce.len(), 12);
        assert_ne!(enc1.nonce, enc2.nonce, "fresh random nonce per encryption");
        assert_ne!(enc1.ciphertext, plaintext.as_bytes());
        assert!(
            !String::from_utf8_lossy(&enc1.ciphertext).contains("super-secret"),
            "ciphertext must not contain plaintext"
        );
        assert_eq!(enc1.fingerprint_short.len(), 16);
        assert_eq!(enc1.fingerprint_short, enc2.fingerprint_short);

        let dec = cipher
            .decrypt("gemini_api_key", &enc1.nonce, &enc1.ciphertext)
            .expect("decrypt valid");
        assert_eq!(dec.as_str(), plaintext);

        // Cross-slot AAD swap MUST fail authentication!
        let cross_slot = cipher.decrypt("local_runtime_token", &enc1.nonce, &enc1.ciphertext);
        assert!(matches!(
            cross_slot,
            Err(DennettError::SafetyGuardRejected { .. })
        ));

        // Wrong master key MUST fail authentication!
        let other = SecretCipher::new(VaultMasterKey::from_bytes([0x99; 32]));
        assert!(matches!(
            other.decrypt("gemini_api_key", &enc1.nonce, &enc1.ciphertext),
            Err(DennettError::SafetyGuardRejected { .. })
        ));
    }

    #[test]
    fn master_key_auto_generates_and_reloads_from_vault_dir() {
        let tmp = std::env::temp_dir().join(format!(
            "dennett-vault-key-test-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let k1 = VaultMasterKey::load_or_create(&tmp, None).expect("generate key");
        assert!(tmp.join("master.key").exists());

        let cipher1 = SecretCipher::new(k1);
        let enc = cipher1.encrypt("slot_a", "secret-val-123").expect("enc");

        let k2 = VaultMasterKey::load_or_create(&tmp, None).expect("reload key");
        let cipher2 = SecretCipher::new(k2);
        let dec = cipher2
            .decrypt("slot_a", &enc.nonce, &enc.ciphertext)
            .expect("dec with reloaded key");
        assert_eq!(dec.as_str(), "secret-val-123");

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn secret_scope_isolation_blocks_cross_project_and_malformed_scopes() {
        let proj_a = ProjectId::new();
        let proj_b = ProjectId::new();
        let scope_a = format!("project:{proj_a}");

        assert!(enforce_secret_scope("global_key", "global", None).is_ok());
        assert!(enforce_secret_scope("global_key", "global", Some(proj_a)).is_ok());
        assert!(enforce_secret_scope("proj_a_key", &scope_a, Some(proj_a)).is_ok());

        // Cross-project MUST fail with AccessDenied!
        assert!(matches!(
            enforce_secret_scope("proj_a_key", &scope_a, Some(proj_b)),
            Err(DennettError::AccessDenied { .. })
        ));
        // Standalone / global caller MUST NOT read project-scoped secret!
        assert!(matches!(
            enforce_secret_scope("proj_a_key", &scope_a, None),
            Err(DennettError::AccessDenied { .. })
        ));
        // Malformed or unknown scope MUST fail closed!
        assert!(matches!(
            enforce_secret_scope("bad_key", "project:not-a-uuid", Some(proj_a)),
            Err(DennettError::AccessDenied { .. })
        ));
        assert!(matches!(
            enforce_secret_scope("bad_key", "custom:foo", Some(proj_a)),
            Err(DennettError::AccessDenied { .. })
        ));
    }
}
