use super::SqliteControlStore;
use async_trait::async_trait;
use dennett_contracts::{
    CheckpointId, CommandId, ProjectId, WorkspaceBindingId, WorkspaceOperationId,
    WorkspaceRevision, WorkspaceSnapshotId,
};
use dennett_effect_core::workspace::{
    ContentSha256, DurableCheckpointState, DurableWorkspaceFailureKind,
    DurableWorkspaceOperationState, MAX_STAGED_OPERATION_BYTES, SnapshotCommitOutcome,
    StagedContentRef, WorkspaceBlob, WorkspaceCheckpointRecord, WorkspaceJournalError,
    WorkspaceJournalPort, WorkspaceManifest, WorkspaceOperationRecord,
    WorkspaceSnapshotPublication, WorkspaceSnapshotRecord,
};
use sha2::{Digest, Sha256};
use sqlx::{Executor, Row, Sqlite, Transaction, sqlite::SqliteRow};
use std::collections::BTreeMap;
use uuid::Uuid;

const JSON_HASH_DOMAIN: &[u8] = b"dennett.workspace-journal-json.v1\0";

fn storage_error(_: sqlx::Error) -> WorkspaceJournalError {
    WorkspaceJournalError::Unavailable
}

fn integrity_error(_: sqlx::Error) -> WorkspaceJournalError {
    WorkspaceJournalError::Integrity
}

fn to_i64(value: u64) -> Result<i64, WorkspaceJournalError> {
    i64::try_from(value).map_err(|_| WorkspaceJournalError::Integrity)
}

fn from_i64(value: i64) -> Result<u64, WorkspaceJournalError> {
    u64::try_from(value).map_err(|_| WorkspaceJournalError::Integrity)
}

fn parse_uuid(value: &str) -> Result<Uuid, WorkspaceJournalError> {
    Uuid::parse_str(value).map_err(|_| WorkspaceJournalError::Integrity)
}

fn json_hash(json: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(JSON_HASH_DOMAIN);
    hasher.update(json.as_bytes());
    hasher.finalize().into()
}

fn verify_record_hash(json: &str, stored_hash: &[u8]) -> Result<(), WorkspaceJournalError> {
    if stored_hash != json_hash(json) {
        return Err(WorkspaceJournalError::Integrity);
    }
    Ok(())
}

macro_rules! encode_record {
    ($record:expr) => {{
        let json = serde_json::to_string($record).map_err(|_| WorkspaceJournalError::Integrity)?;
        let hash = json_hash(&json);
        Ok::<_, WorkspaceJournalError>((json, hash))
    }};
}

fn validate_snapshot(record: &WorkspaceSnapshotRecord) -> Result<(), WorkspaceJournalError> {
    let reconstructed = WorkspaceManifest::new(
        record.manifest.revision,
        record.manifest.scope_sha256,
        record.manifest.complete,
        record.manifest.entries().to_vec(),
    )
    .map_err(|_| WorkspaceJournalError::Integrity)?;
    if reconstructed != record.manifest {
        return Err(WorkspaceJournalError::Integrity);
    }
    Ok(())
}

fn validate_operation(record: &WorkspaceOperationRecord) -> Result<(), WorkspaceJournalError> {
    record
        .validate()
        .map_err(|_| WorkspaceJournalError::Integrity)?;
    if record.plan.base_revision.binding_id() != record.plan.binding_id
        || record.plan.correlation_id.is_empty()
        || record.plan.correlation_id.len() > 256
        || record.plan.changes.is_empty()
        || record.plan.transitions.is_empty()
    {
        return Err(WorkspaceJournalError::Integrity);
    }
    Ok(())
}

fn validate_checkpoint(record: &WorkspaceCheckpointRecord) -> Result<(), WorkspaceJournalError> {
    record
        .validate()
        .map_err(|_| WorkspaceJournalError::Integrity)
}

fn checkpoint_state_name(state: DurableCheckpointState) -> &'static str {
    match state {
        DurableCheckpointState::Available => "available",
        DurableCheckpointState::Restored => "restored",
        DurableCheckpointState::RecoveryRequired => "recovery_required",
    }
}

fn operation_state_name(state: DurableWorkspaceOperationState) -> &'static str {
    match state {
        DurableWorkspaceOperationState::Prepared => "prepared",
        DurableWorkspaceOperationState::FilesystemApplied => "filesystem_applied",
        DurableWorkspaceOperationState::Succeeded => "succeeded",
        DurableWorkspaceOperationState::Failed => "failed",
        DurableWorkspaceOperationState::RecoveryRequired => "recovery_required",
    }
}

fn failure_kind_name(kind: DurableWorkspaceFailureKind) -> &'static str {
    match kind {
        DurableWorkspaceFailureKind::Conflict => "conflict",
        DurableWorkspaceFailureKind::ScopeDenied => "scope_denied",
        DurableWorkspaceFailureKind::AdapterFailure => "adapter_failure",
        DurableWorkspaceFailureKind::RecoveryRequired => "recovery_required",
    }
}

fn parse_snapshot_row(row: &SqliteRow) -> Result<WorkspaceSnapshotRecord, WorkspaceJournalError> {
    let json: String = row.try_get("record_json").map_err(integrity_error)?;
    let hash: Vec<u8> = row.try_get("record_sha256").map_err(integrity_error)?;
    verify_record_hash(&json, &hash)?;
    let record: WorkspaceSnapshotRecord =
        serde_json::from_str(&json).map_err(|_| WorkspaceJournalError::Integrity)?;
    validate_snapshot(&record)?;

    let snapshot_id: String = row.try_get("snapshot_id").map_err(integrity_error)?;
    let binding_id: String = row.try_get("binding_id").map_err(integrity_error)?;
    let project_id: String = row.try_get("project_id").map_err(integrity_error)?;
    let sequence: i64 = row.try_get("sequence").map_err(integrity_error)?;
    let scope_sha256: Vec<u8> = row.try_get("scope_sha256").map_err(integrity_error)?;
    let complete: i64 = row.try_get("manifest_complete").map_err(integrity_error)?;
    let observed_at: i64 = row
        .try_get("observed_at_unix_ms")
        .map_err(integrity_error)?;
    if WorkspaceSnapshotId(parse_uuid(&snapshot_id)?) != record.manifest.revision.snapshot_id()
        || WorkspaceBindingId(parse_uuid(&binding_id)?) != record.manifest.revision.binding_id()
        || ProjectId(parse_uuid(&project_id)?) != record.project_id
        || from_i64(sequence)? != record.manifest.revision.sequence()
        || scope_sha256.as_slice() != record.manifest.scope_sha256.0
        || complete != i64::from(record.manifest.complete)
        || from_i64(observed_at)? != record.observed_at_unix_ms
    {
        return Err(WorkspaceJournalError::Integrity);
    }
    Ok(record)
}

fn parse_checkpoint_row(
    row: &SqliteRow,
) -> Result<WorkspaceCheckpointRecord, WorkspaceJournalError> {
    let json: String = row.try_get("record_json").map_err(integrity_error)?;
    let hash: Vec<u8> = row.try_get("record_sha256").map_err(integrity_error)?;
    verify_record_hash(&json, &hash)?;
    let record: WorkspaceCheckpointRecord =
        serde_json::from_str(&json).map_err(|_| WorkspaceJournalError::Integrity)?;
    validate_checkpoint(&record)?;

    let checkpoint_id: String = row.try_get("checkpoint_id").map_err(integrity_error)?;
    let project_id: String = row.try_get("project_id").map_err(integrity_error)?;
    let binding_id: String = row.try_get("binding_id").map_err(integrity_error)?;
    let base_snapshot_id: String = row.try_get("base_snapshot_id").map_err(integrity_error)?;
    let base_sequence: i64 = row.try_get("base_sequence").map_err(integrity_error)?;
    let captured_snapshot_id: String = row
        .try_get("captured_snapshot_id")
        .map_err(integrity_error)?;
    let captured_sequence: i64 = row.try_get("captured_sequence").map_err(integrity_error)?;
    let state: String = row.try_get("state").map_err(integrity_error)?;
    let created_at: i64 = row.try_get("created_at_unix_ms").map_err(integrity_error)?;
    if CheckpointId(parse_uuid(&checkpoint_id)?) != record.checkpoint_id
        || ProjectId(parse_uuid(&project_id)?) != record.project_id
        || WorkspaceBindingId(parse_uuid(&binding_id)?) != record.binding_id
        || WorkspaceSnapshotId(parse_uuid(&base_snapshot_id)?) != record.base_revision.snapshot_id()
        || from_i64(base_sequence)? != record.base_revision.sequence()
        || WorkspaceSnapshotId(parse_uuid(&captured_snapshot_id)?)
            != record.captured_revision.snapshot_id()
        || from_i64(captured_sequence)? != record.captured_revision.sequence()
        || state != checkpoint_state_name(record.state)
        || from_i64(created_at)? != record.created_at_unix_ms
    {
        return Err(WorkspaceJournalError::Integrity);
    }
    Ok(record)
}

fn parse_operation_row(row: &SqliteRow) -> Result<WorkspaceOperationRecord, WorkspaceJournalError> {
    let json: String = row.try_get("record_json").map_err(integrity_error)?;
    let hash: Vec<u8> = row.try_get("record_sha256").map_err(integrity_error)?;
    verify_record_hash(&json, &hash)?;
    let record: WorkspaceOperationRecord =
        serde_json::from_str(&json).map_err(|_| WorkspaceJournalError::Integrity)?;
    validate_operation(&record)?;

    let operation_id: String = row.try_get("operation_id").map_err(integrity_error)?;
    let command_id: String = row.try_get("command_id").map_err(integrity_error)?;
    let project_id: String = row.try_get("project_id").map_err(integrity_error)?;
    let binding_id: String = row.try_get("binding_id").map_err(integrity_error)?;
    let base_snapshot_id: String = row.try_get("base_snapshot_id").map_err(integrity_error)?;
    let base_sequence: i64 = row.try_get("base_sequence").map_err(integrity_error)?;
    let safety_checkpoint_id: String = row
        .try_get("safety_checkpoint_id")
        .map_err(integrity_error)?;
    let state: String = row.try_get("state").map_err(integrity_error)?;
    let intent_sha256: Vec<u8> = row.try_get("intent_sha256").map_err(integrity_error)?;
    let resulting_snapshot_id: Option<String> = row
        .try_get("resulting_snapshot_id")
        .map_err(integrity_error)?;
    let resulting_sequence: Option<i64> =
        row.try_get("resulting_sequence").map_err(integrity_error)?;
    let failure_kind: Option<String> = row.try_get("failure_kind").map_err(integrity_error)?;
    let failure_safe_code: Option<String> =
        row.try_get("failure_safe_code").map_err(integrity_error)?;
    let prepared_at: i64 = row
        .try_get("prepared_at_unix_ms")
        .map_err(integrity_error)?;
    let completed_at: Option<i64> = row
        .try_get("completed_at_unix_ms")
        .map_err(integrity_error)?;

    let expected_result = record
        .resulting_revision
        .map(|revision| (revision.snapshot_id().0.to_string(), revision.sequence()));
    let stored_result = resulting_snapshot_id
        .map(|snapshot_id| {
            Ok((
                snapshot_id,
                from_i64(resulting_sequence.ok_or(WorkspaceJournalError::Integrity)?)?,
            ))
        })
        .transpose()?;
    let expected_failure = record.failure.as_ref().map(|failure| {
        (
            failure_kind_name(failure.kind).to_owned(),
            failure.safe_code.clone(),
        )
    });
    let stored_failure = failure_kind.map(|kind| (kind, failure_safe_code.unwrap_or_default()));
    if WorkspaceOperationId(parse_uuid(&operation_id)?) != record.plan.operation_id
        || CommandId(parse_uuid(&command_id)?) != record.plan.command_id
        || ProjectId(parse_uuid(&project_id)?) != record.plan.project_id
        || WorkspaceBindingId(parse_uuid(&binding_id)?) != record.plan.binding_id
        || WorkspaceSnapshotId(parse_uuid(&base_snapshot_id)?)
            != record.plan.base_revision.snapshot_id()
        || from_i64(base_sequence)? != record.plan.base_revision.sequence()
        || CheckpointId(parse_uuid(&safety_checkpoint_id)?) != record.plan.safety_checkpoint_id
        || state != operation_state_name(record.state)
        || intent_sha256.as_slice() != record.plan.intent_sha256.0
        || stored_result != expected_result
        || stored_failure != expected_failure
        || from_i64(prepared_at)? != record.plan.prepared_at_unix_ms
        || completed_at.map(from_i64).transpose()? != record.completed_at_unix_ms
    {
        return Err(WorkspaceJournalError::Integrity);
    }
    Ok(record)
}

fn expected_operation_refs(
    operation: &WorkspaceOperationRecord,
) -> Result<BTreeMap<String, StagedContentRef>, WorkspaceJournalError> {
    let mut refs = BTreeMap::new();
    for reference in operation
        .plan
        .transitions
        .iter()
        .filter_map(|transition| transition.content.clone())
    {
        reference
            .validate()
            .map_err(|_| WorkspaceJournalError::Integrity)?;
        match refs.insert(reference.content_id.clone(), reference.clone()) {
            Some(existing) if existing != reference => {
                return Err(WorkspaceJournalError::Integrity);
            }
            _ => {}
        }
    }
    Ok(refs)
}

fn expected_checkpoint_refs(
    checkpoint: &WorkspaceCheckpointRecord,
) -> Result<BTreeMap<String, StagedContentRef>, WorkspaceJournalError> {
    let mut refs = BTreeMap::new();
    for reference in checkpoint
        .entries
        .iter()
        .filter_map(|entry| entry.content.clone())
    {
        reference
            .validate()
            .map_err(|_| WorkspaceJournalError::Integrity)?;
        match refs.insert(reference.content_id.clone(), reference.clone()) {
            Some(existing) if existing != reference => {
                return Err(WorkspaceJournalError::Integrity);
            }
            _ => {}
        }
    }
    Ok(refs)
}

fn supplied_blobs(
    blobs: Vec<WorkspaceBlob>,
) -> Result<BTreeMap<String, WorkspaceBlob>, WorkspaceJournalError> {
    let mut supplied = BTreeMap::new();
    let mut staged_bytes = 0_u64;
    for blob in blobs {
        blob.validate()
            .map_err(|_| WorkspaceJournalError::Integrity)?;
        staged_bytes = staged_bytes
            .checked_add(blob.reference.byte_size)
            .filter(|value| *value <= MAX_STAGED_OPERATION_BYTES)
            .ok_or(WorkspaceJournalError::Integrity)?;
        if supplied
            .insert(blob.reference.content_id.clone(), blob)
            .is_some()
        {
            return Err(WorkspaceJournalError::Integrity);
        }
    }
    Ok(supplied)
}

fn selected_blobs(
    refs: &BTreeMap<String, StagedContentRef>,
    supplied: &BTreeMap<String, WorkspaceBlob>,
) -> Result<BTreeMap<String, WorkspaceBlob>, WorkspaceJournalError> {
    refs.iter()
        .map(|(content_id, reference)| {
            let blob = supplied
                .get(content_id)
                .ok_or(WorkspaceJournalError::Integrity)?;
            if &blob.reference != reference {
                return Err(WorkspaceJournalError::Integrity);
            }
            Ok((content_id.clone(), blob.clone()))
        })
        .collect()
}

async fn load_snapshot_from<'e, E>(
    executor: E,
    revision: WorkspaceRevision,
) -> Result<Option<WorkspaceSnapshotRecord>, WorkspaceJournalError>
where
    E: Executor<'e, Database = Sqlite>,
{
    sqlx::query("SELECT * FROM workspace_snapshots WHERE snapshot_id = ?")
        .bind(revision.snapshot_id().0.to_string())
        .fetch_optional(executor)
        .await
        .map_err(storage_error)?
        .as_ref()
        .map(parse_snapshot_row)
        .transpose()
        .and_then(|record| match record {
            Some(record) if record.manifest.revision != revision => {
                Err(WorkspaceJournalError::Integrity)
            }
            other => Ok(other),
        })
}

async fn load_head_from<'e, E>(
    executor: E,
    binding_id: WorkspaceBindingId,
) -> Result<Option<WorkspaceSnapshotRecord>, WorkspaceJournalError>
where
    E: Executor<'e, Database = Sqlite>,
{
    sqlx::query(
        "SELECT s.* FROM workspace_snapshot_heads h \
         JOIN workspace_snapshots s ON s.snapshot_id = h.snapshot_id \
         WHERE h.binding_id = ?",
    )
    .bind(binding_id.0.to_string())
    .fetch_optional(executor)
    .await
    .map_err(storage_error)?
    .as_ref()
    .map(parse_snapshot_row)
    .transpose()
}

async fn load_operation_from<'e, E>(
    executor: E,
    selector: OperationSelector,
) -> Result<Option<WorkspaceOperationRecord>, WorkspaceJournalError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let (query, value) = match selector {
        OperationSelector::Operation(id) => (
            "SELECT * FROM workspace_operations WHERE operation_id = ?",
            id.0.to_string(),
        ),
        OperationSelector::Command(id) => (
            "SELECT * FROM workspace_operations WHERE command_id = ?",
            id.0.to_string(),
        ),
        OperationSelector::Checkpoint(id) => (
            "SELECT * FROM workspace_operations WHERE safety_checkpoint_id = ?",
            id.0.to_string(),
        ),
    };
    sqlx::query(query)
        .bind(value)
        .fetch_optional(executor)
        .await
        .map_err(storage_error)?
        .as_ref()
        .map(parse_operation_row)
        .transpose()
}

enum OperationSelector {
    Operation(WorkspaceOperationId),
    Command(CommandId),
    Checkpoint(CheckpointId),
}

async fn load_checkpoint_from<'e, E>(
    executor: E,
    checkpoint_id: CheckpointId,
) -> Result<Option<WorkspaceCheckpointRecord>, WorkspaceJournalError>
where
    E: Executor<'e, Database = Sqlite>,
{
    sqlx::query("SELECT * FROM workspace_checkpoints WHERE checkpoint_id = ?")
        .bind(checkpoint_id.0.to_string())
        .fetch_optional(executor)
        .await
        .map_err(storage_error)?
        .as_ref()
        .map(parse_checkpoint_row)
        .transpose()
}

async fn load_blobs_from<'e, E>(
    executor: E,
    owner: BlobOwner,
) -> Result<Vec<WorkspaceBlob>, WorkspaceJournalError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let (query, owner_id) = match owner {
        BlobOwner::Operation(id) => (
            "SELECT r.content_id, r.content_sha256, r.byte_size, d.bytes \
             FROM workspace_operation_blobs r \
             JOIN workspace_blob_data d ON d.content_sha256 = r.content_sha256 \
                AND d.byte_size = r.byte_size \
             WHERE r.operation_id = ? ORDER BY r.content_id",
            id.0.to_string(),
        ),
        BlobOwner::Checkpoint(id) => (
            "SELECT r.content_id, r.content_sha256, r.byte_size, d.bytes \
             FROM workspace_checkpoint_blobs r \
             JOIN workspace_blob_data d ON d.content_sha256 = r.content_sha256 \
                AND d.byte_size = r.byte_size \
             WHERE r.checkpoint_id = ? ORDER BY r.content_id",
            id.0.to_string(),
        ),
    };
    sqlx::query(query)
        .bind(owner_id)
        .fetch_all(executor)
        .await
        .map_err(storage_error)?
        .iter()
        .map(parse_blob_row)
        .collect()
}

enum BlobOwner {
    Operation(WorkspaceOperationId),
    Checkpoint(CheckpointId),
}

fn parse_blob_row(row: &SqliteRow) -> Result<WorkspaceBlob, WorkspaceJournalError> {
    let content_id: String = row.try_get("content_id").map_err(integrity_error)?;
    let hash: Vec<u8> = row.try_get("content_sha256").map_err(integrity_error)?;
    let byte_size: i64 = row.try_get("byte_size").map_err(integrity_error)?;
    let bytes: Vec<u8> = row.try_get("bytes").map_err(integrity_error)?;
    let content_sha256 = ContentSha256(
        hash.try_into()
            .map_err(|_| WorkspaceJournalError::Integrity)?,
    );
    let blob = WorkspaceBlob {
        reference: StagedContentRef {
            content_id,
            content_sha256,
            byte_size: from_i64(byte_size)?,
        },
        bytes,
    };
    blob.validate()
        .map_err(|_| WorkspaceJournalError::Integrity)?;
    Ok(blob)
}

fn blob_map(
    blobs: Vec<WorkspaceBlob>,
) -> Result<BTreeMap<String, WorkspaceBlob>, WorkspaceJournalError> {
    supplied_blobs(blobs)
}

fn validate_safety_checkpoint(
    operation: &WorkspaceOperationRecord,
    checkpoint: &WorkspaceCheckpointRecord,
) -> Result<(), WorkspaceJournalError> {
    if checkpoint.state != DurableCheckpointState::Available
        || operation.plan.safety_checkpoint_id != checkpoint.checkpoint_id
        || operation.plan.project_id != checkpoint.project_id
        || operation.plan.binding_id != checkpoint.binding_id
        || operation.plan.base_revision != checkpoint.base_revision
        || operation.plan.transitions.len() != checkpoint.entries.len()
    {
        return Err(WorkspaceJournalError::Integrity);
    }
    let entries = checkpoint
        .entries
        .iter()
        .map(|entry| (entry.path.as_str(), &entry.state))
        .collect::<BTreeMap<_, _>>();
    if operation.plan.transitions.iter().any(|transition| {
        entries.get(transition.path.as_str()).copied() != Some(&transition.before)
    }) {
        return Err(WorkspaceJournalError::Integrity);
    }
    Ok(())
}

async fn load_snapshot_id_from<'e, E>(
    executor: E,
    snapshot_id: WorkspaceSnapshotId,
) -> Result<Option<WorkspaceSnapshotRecord>, WorkspaceJournalError>
where
    E: Executor<'e, Database = Sqlite>,
{
    sqlx::query("SELECT * FROM workspace_snapshots WHERE snapshot_id = ?")
        .bind(snapshot_id.0.to_string())
        .fetch_optional(executor)
        .await
        .map_err(storage_error)?
        .as_ref()
        .map(parse_snapshot_row)
        .transpose()
}

async fn commit_snapshot_tx(
    transaction: &mut Transaction<'_, Sqlite>,
    expected_head: Option<WorkspaceRevision>,
    snapshot: &WorkspaceSnapshotRecord,
) -> Result<SnapshotCommitOutcome, WorkspaceJournalError> {
    validate_snapshot(snapshot)?;
    let revision = snapshot.manifest.revision;
    if let Some(expected) = expected_head
        && expected.binding_id() != revision.binding_id()
    {
        return Err(WorkspaceJournalError::RevisionConflict);
    }

    let current = load_head_from(&mut **transaction, revision.binding_id()).await?;
    if let Some(current) = &current
        && current.manifest.revision == revision
    {
        return if current == snapshot {
            Ok(SnapshotCommitOutcome::AlreadyCurrent)
        } else {
            Err(WorkspaceJournalError::IdempotencyConflict)
        };
    }
    if current.as_ref().map(|record| record.manifest.revision) != expected_head {
        return Err(WorkspaceJournalError::RevisionConflict);
    }

    if let Some(existing) =
        load_snapshot_id_from(&mut **transaction, revision.snapshot_id()).await?
    {
        return if existing == *snapshot {
            Err(WorkspaceJournalError::RevisionConflict)
        } else {
            Err(WorkspaceJournalError::IdempotencyConflict)
        };
    }
    let expected_sequence = match current.as_ref() {
        Some(record) => record
            .manifest
            .revision
            .sequence()
            .checked_add(1)
            .ok_or(WorkspaceJournalError::Integrity)?,
        None => 1,
    };
    if revision.sequence() != expected_sequence {
        return Err(WorkspaceJournalError::RevisionConflict);
    }

    let (json, hash) = encode_record!(snapshot)?;
    sqlx::query(
        "INSERT INTO workspace_snapshots(\
            snapshot_id, binding_id, project_id, sequence, scope_sha256, manifest_complete, \
            record_json, record_sha256, observed_at_unix_ms\
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(revision.snapshot_id().0.to_string())
    .bind(revision.binding_id().0.to_string())
    .bind(snapshot.project_id.0.to_string())
    .bind(to_i64(revision.sequence())?)
    .bind(snapshot.manifest.scope_sha256.0.as_slice())
    .bind(i64::from(snapshot.manifest.complete))
    .bind(json)
    .bind(hash.as_slice())
    .bind(to_i64(snapshot.observed_at_unix_ms)?)
    .execute(&mut **transaction)
    .await
    .map_err(storage_error)?;

    match current {
        Some(previous) => {
            let updated = sqlx::query(
                "UPDATE workspace_snapshot_heads SET project_id = ?, snapshot_id = ?, sequence = ? \
                 WHERE binding_id = ? AND snapshot_id = ? AND sequence = ?",
            )
            .bind(snapshot.project_id.0.to_string())
            .bind(revision.snapshot_id().0.to_string())
            .bind(to_i64(revision.sequence())?)
            .bind(revision.binding_id().0.to_string())
            .bind(previous.manifest.revision.snapshot_id().0.to_string())
            .bind(to_i64(previous.manifest.revision.sequence())?)
            .execute(&mut **transaction)
            .await
            .map_err(storage_error)?;
            if updated.rows_affected() != 1 {
                return Err(WorkspaceJournalError::RevisionConflict);
            }
        }
        None => {
            sqlx::query(
                "INSERT INTO workspace_snapshot_heads(binding_id, project_id, snapshot_id, sequence) \
                 VALUES (?, ?, ?, ?)",
            )
            .bind(revision.binding_id().0.to_string())
            .bind(snapshot.project_id.0.to_string())
            .bind(revision.snapshot_id().0.to_string())
            .bind(to_i64(revision.sequence())?)
            .execute(&mut **transaction)
            .await
            .map_err(storage_error)?;
        }
    }
    Ok(SnapshotCommitOutcome::Inserted)
}

async fn persist_blob_data_tx(
    transaction: &mut Transaction<'_, Sqlite>,
    blob: &WorkspaceBlob,
) -> Result<(), WorkspaceJournalError> {
    blob.validate()
        .map_err(|_| WorkspaceJournalError::Integrity)?;
    let existing =
        sqlx::query("SELECT byte_size, bytes FROM workspace_blob_data WHERE content_sha256 = ?")
            .bind(blob.reference.content_sha256.0.as_slice())
            .fetch_optional(&mut **transaction)
            .await
            .map_err(storage_error)?;
    if let Some(row) = existing {
        let byte_size: i64 = row.try_get("byte_size").map_err(integrity_error)?;
        let bytes: Vec<u8> = row.try_get("bytes").map_err(integrity_error)?;
        if from_i64(byte_size)? != blob.reference.byte_size || bytes != blob.bytes {
            return Err(WorkspaceJournalError::Integrity);
        }
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO workspace_blob_data(content_sha256, byte_size, bytes) VALUES (?, ?, ?)",
    )
    .bind(blob.reference.content_sha256.0.as_slice())
    .bind(to_i64(blob.reference.byte_size)?)
    .bind(blob.bytes.as_slice())
    .execute(&mut **transaction)
    .await
    .map_err(storage_error)?;
    Ok(())
}

async fn persist_checkpoint_blobs_tx(
    transaction: &mut Transaction<'_, Sqlite>,
    checkpoint_id: CheckpointId,
    blobs: &BTreeMap<String, WorkspaceBlob>,
) -> Result<(), WorkspaceJournalError> {
    for blob in blobs.values() {
        persist_blob_data_tx(transaction, blob).await?;
        sqlx::query(
            "INSERT INTO workspace_checkpoint_blobs(\
                checkpoint_id, content_id, content_sha256, byte_size\
             ) VALUES (?, ?, ?, ?)",
        )
        .bind(checkpoint_id.0.to_string())
        .bind(&blob.reference.content_id)
        .bind(blob.reference.content_sha256.0.as_slice())
        .bind(to_i64(blob.reference.byte_size)?)
        .execute(&mut **transaction)
        .await
        .map_err(storage_error)?;
    }
    Ok(())
}

async fn persist_operation_blobs_tx(
    transaction: &mut Transaction<'_, Sqlite>,
    operation_id: WorkspaceOperationId,
    blobs: &BTreeMap<String, WorkspaceBlob>,
) -> Result<(), WorkspaceJournalError> {
    for blob in blobs.values() {
        persist_blob_data_tx(transaction, blob).await?;
        sqlx::query(
            "INSERT INTO workspace_operation_blobs(\
                operation_id, content_id, content_sha256, byte_size\
             ) VALUES (?, ?, ?, ?)",
        )
        .bind(operation_id.0.to_string())
        .bind(&blob.reference.content_id)
        .bind(blob.reference.content_sha256.0.as_slice())
        .bind(to_i64(blob.reference.byte_size)?)
        .execute(&mut **transaction)
        .await
        .map_err(storage_error)?;
    }
    Ok(())
}

async fn save_checkpoint_tx(
    transaction: &mut Transaction<'_, Sqlite>,
    checkpoint: &WorkspaceCheckpointRecord,
    blobs: &BTreeMap<String, WorkspaceBlob>,
) -> Result<WorkspaceCheckpointRecord, WorkspaceJournalError> {
    validate_checkpoint(checkpoint)?;
    let refs = expected_checkpoint_refs(checkpoint)?;
    let selected = selected_blobs(&refs, blobs)?;
    if selected.len() != blobs.len() {
        return Err(WorkspaceJournalError::Integrity);
    }
    if let Some(existing) =
        load_checkpoint_from(&mut **transaction, checkpoint.checkpoint_id).await?
    {
        if existing != *checkpoint {
            return Err(WorkspaceJournalError::IdempotencyConflict);
        }
        let stored = blob_map(
            load_blobs_from(
                &mut **transaction,
                BlobOwner::Checkpoint(checkpoint.checkpoint_id),
            )
            .await?,
        )?;
        return if stored == selected {
            Ok(existing)
        } else {
            Err(WorkspaceJournalError::IdempotencyConflict)
        };
    }

    let (json, hash) = encode_record!(checkpoint)?;
    sqlx::query(
        "INSERT INTO workspace_checkpoints(\
            checkpoint_id, project_id, binding_id, base_snapshot_id, base_sequence, \
            captured_snapshot_id, captured_sequence, state, record_json, record_sha256, \
            created_at_unix_ms\
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(checkpoint.checkpoint_id.0.to_string())
    .bind(checkpoint.project_id.0.to_string())
    .bind(checkpoint.binding_id.0.to_string())
    .bind(checkpoint.base_revision.snapshot_id().0.to_string())
    .bind(to_i64(checkpoint.base_revision.sequence())?)
    .bind(checkpoint.captured_revision.snapshot_id().0.to_string())
    .bind(to_i64(checkpoint.captured_revision.sequence())?)
    .bind(checkpoint_state_name(checkpoint.state))
    .bind(json)
    .bind(hash.as_slice())
    .bind(to_i64(checkpoint.created_at_unix_ms)?)
    .execute(&mut **transaction)
    .await
    .map_err(storage_error)?;
    persist_checkpoint_blobs_tx(transaction, checkpoint.checkpoint_id, &selected).await?;
    Ok(checkpoint.clone())
}

async fn insert_operation_tx(
    transaction: &mut Transaction<'_, Sqlite>,
    operation: &WorkspaceOperationRecord,
) -> Result<(), WorkspaceJournalError> {
    let (json, hash) = encode_record!(operation)?;
    let resulting = operation.resulting_revision;
    let failure = operation.failure.as_ref();
    sqlx::query(
        "INSERT INTO workspace_operations(\
            operation_id, command_id, project_id, binding_id, base_snapshot_id, base_sequence, \
            safety_checkpoint_id, state, intent_sha256, resulting_snapshot_id, resulting_sequence, \
            failure_kind, failure_safe_code, record_json, record_sha256, prepared_at_unix_ms, \
            completed_at_unix_ms\
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(operation.plan.operation_id.0.to_string())
    .bind(operation.plan.command_id.0.to_string())
    .bind(operation.plan.project_id.0.to_string())
    .bind(operation.plan.binding_id.0.to_string())
    .bind(operation.plan.base_revision.snapshot_id().0.to_string())
    .bind(to_i64(operation.plan.base_revision.sequence())?)
    .bind(operation.plan.safety_checkpoint_id.0.to_string())
    .bind(operation_state_name(operation.state))
    .bind(operation.plan.intent_sha256.0.as_slice())
    .bind(resulting.map(|revision| revision.snapshot_id().0.to_string()))
    .bind(
        resulting
            .map(|revision| to_i64(revision.sequence()))
            .transpose()?,
    )
    .bind(failure.map(|failure| failure_kind_name(failure.kind)))
    .bind(failure.map(|failure| failure.safe_code.as_str()))
    .bind(json)
    .bind(hash.as_slice())
    .bind(to_i64(operation.plan.prepared_at_unix_ms)?)
    .bind(operation.completed_at_unix_ms.map(to_i64).transpose()?)
    .execute(&mut **transaction)
    .await
    .map_err(storage_error)?;
    Ok(())
}

async fn verify_replay_tx(
    transaction: &mut Transaction<'_, Sqlite>,
    existing: WorkspaceOperationRecord,
    requested: &WorkspaceOperationRecord,
    checkpoint: &WorkspaceCheckpointRecord,
    operation_blobs: &BTreeMap<String, WorkspaceBlob>,
    checkpoint_blobs: &BTreeMap<String, WorkspaceBlob>,
) -> Result<WorkspaceOperationRecord, WorkspaceJournalError> {
    if existing.plan != requested.plan {
        return Err(WorkspaceJournalError::IdempotencyConflict);
    }
    let stored_checkpoint =
        load_checkpoint_from(&mut **transaction, requested.plan.safety_checkpoint_id)
            .await?
            .ok_or(WorkspaceJournalError::Integrity)?;
    if stored_checkpoint != *checkpoint {
        return Err(WorkspaceJournalError::IdempotencyConflict);
    }
    let stored_operation_blobs = blob_map(
        load_blobs_from(
            &mut **transaction,
            BlobOwner::Operation(requested.plan.operation_id),
        )
        .await?,
    )?;
    let stored_checkpoint_blobs = blob_map(
        load_blobs_from(
            &mut **transaction,
            BlobOwner::Checkpoint(checkpoint.checkpoint_id),
        )
        .await?,
    )?;
    if stored_operation_blobs != *operation_blobs || stored_checkpoint_blobs != *checkpoint_blobs {
        return Err(WorkspaceJournalError::IdempotencyConflict);
    }
    Ok(existing)
}

async fn update_operation_tx(
    transaction: &mut Transaction<'_, Sqlite>,
    expected_state: DurableWorkspaceOperationState,
    operation: &WorkspaceOperationRecord,
) -> Result<(), WorkspaceJournalError> {
    let (json, hash) = encode_record!(operation)?;
    let resulting = operation.resulting_revision;
    let failure = operation.failure.as_ref();
    let result = sqlx::query(
        "UPDATE workspace_operations SET \
            state = ?, resulting_snapshot_id = ?, resulting_sequence = ?, failure_kind = ?, \
            failure_safe_code = ?, record_json = ?, record_sha256 = ?, completed_at_unix_ms = ? \
         WHERE operation_id = ? AND state = ?",
    )
    .bind(operation_state_name(operation.state))
    .bind(resulting.map(|revision| revision.snapshot_id().0.to_string()))
    .bind(
        resulting
            .map(|revision| to_i64(revision.sequence()))
            .transpose()?,
    )
    .bind(failure.map(|failure| failure_kind_name(failure.kind)))
    .bind(failure.map(|failure| failure.safe_code.as_str()))
    .bind(json)
    .bind(hash.as_slice())
    .bind(operation.completed_at_unix_ms.map(to_i64).transpose()?)
    .bind(operation.plan.operation_id.0.to_string())
    .bind(operation_state_name(expected_state))
    .execute(&mut **transaction)
    .await
    .map_err(storage_error)?;
    if result.rows_affected() != 1 {
        return Err(WorkspaceJournalError::RevisionConflict);
    }
    Ok(())
}

#[async_trait]
impl WorkspaceJournalPort for SqliteControlStore {
    async fn load_head(
        &self,
        binding_id: WorkspaceBindingId,
    ) -> Result<Option<WorkspaceSnapshotRecord>, WorkspaceJournalError> {
        load_head_from(&self.pool, binding_id).await
    }

    async fn load_snapshot(
        &self,
        revision: WorkspaceRevision,
    ) -> Result<Option<WorkspaceSnapshotRecord>, WorkspaceJournalError> {
        load_snapshot_from(&self.pool, revision).await
    }

    async fn commit_snapshot(
        &self,
        expected_head: Option<WorkspaceRevision>,
        snapshot: WorkspaceSnapshotRecord,
    ) -> Result<SnapshotCommitOutcome, WorkspaceJournalError> {
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        let outcome = commit_snapshot_tx(&mut transaction, expected_head, &snapshot).await?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(outcome)
    }

    async fn prepare_file_effect(
        &self,
        operation: WorkspaceOperationRecord,
        safety_checkpoint: WorkspaceCheckpointRecord,
        blobs: Vec<WorkspaceBlob>,
    ) -> Result<WorkspaceOperationRecord, WorkspaceJournalError> {
        validate_operation(&operation)?;
        validate_checkpoint(&safety_checkpoint)?;
        if operation.state != DurableWorkspaceOperationState::Prepared {
            return Err(WorkspaceJournalError::InvalidTransition);
        }
        validate_safety_checkpoint(&operation, &safety_checkpoint)?;

        let operation_refs = expected_operation_refs(&operation)?;
        let checkpoint_refs = expected_checkpoint_refs(&safety_checkpoint)?;
        let supplied = supplied_blobs(blobs)?;
        let operation_blobs = selected_blobs(&operation_refs, &supplied)?;
        let checkpoint_blobs = selected_blobs(&checkpoint_refs, &supplied)?;
        let mut union = operation_refs;
        for (content_id, reference) in checkpoint_refs {
            match union.insert(content_id, reference.clone()) {
                Some(existing) if existing != reference => {
                    return Err(WorkspaceJournalError::Integrity);
                }
                _ => {}
            }
        }
        if union.len() != supplied.len() {
            return Err(WorkspaceJournalError::Integrity);
        }

        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        if let Some(existing) = load_operation_from(
            &mut *transaction,
            OperationSelector::Command(operation.plan.command_id),
        )
        .await?
        {
            let result = verify_replay_tx(
                &mut transaction,
                existing,
                &operation,
                &safety_checkpoint,
                &operation_blobs,
                &checkpoint_blobs,
            )
            .await?;
            transaction.commit().await.map_err(storage_error)?;
            return Ok(result);
        }
        if load_operation_from(
            &mut *transaction,
            OperationSelector::Operation(operation.plan.operation_id),
        )
        .await?
        .is_some()
        {
            return Err(WorkspaceJournalError::IdempotencyConflict);
        }

        save_checkpoint_tx(&mut transaction, &safety_checkpoint, &checkpoint_blobs).await?;
        insert_operation_tx(&mut transaction, &operation).await?;
        persist_operation_blobs_tx(
            &mut transaction,
            operation.plan.operation_id,
            &operation_blobs,
        )
        .await?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(operation)
    }

    async fn load_operation(
        &self,
        operation_id: WorkspaceOperationId,
    ) -> Result<Option<WorkspaceOperationRecord>, WorkspaceJournalError> {
        load_operation_from(&self.pool, OperationSelector::Operation(operation_id)).await
    }

    async fn load_operation_by_command(
        &self,
        command_id: CommandId,
    ) -> Result<Option<WorkspaceOperationRecord>, WorkspaceJournalError> {
        load_operation_from(&self.pool, OperationSelector::Command(command_id)).await
    }

    async fn load_operation_by_checkpoint(
        &self,
        checkpoint_id: CheckpointId,
    ) -> Result<Option<WorkspaceOperationRecord>, WorkspaceJournalError> {
        load_operation_from(&self.pool, OperationSelector::Checkpoint(checkpoint_id)).await
    }

    async fn load_unfinished_operations(
        &self,
    ) -> Result<Vec<WorkspaceOperationRecord>, WorkspaceJournalError> {
        sqlx::query(
            "SELECT * FROM workspace_operations \
             WHERE state IN ('prepared', 'filesystem_applied') \
             ORDER BY prepared_at_unix_ms, operation_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?
        .iter()
        .map(parse_operation_row)
        .collect()
    }

    async fn load_operation_blobs(
        &self,
        operation_id: WorkspaceOperationId,
    ) -> Result<Vec<WorkspaceBlob>, WorkspaceJournalError> {
        if load_operation_from(&self.pool, OperationSelector::Operation(operation_id))
            .await?
            .is_none()
        {
            return Err(WorkspaceJournalError::NotFound);
        }
        load_blobs_from(&self.pool, BlobOwner::Operation(operation_id)).await
    }

    async fn transition_operation(
        &self,
        expected_state: DurableWorkspaceOperationState,
        operation: WorkspaceOperationRecord,
        resulting_snapshot: Option<WorkspaceSnapshotPublication>,
    ) -> Result<WorkspaceOperationRecord, WorkspaceJournalError> {
        validate_operation(&operation)?;
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        let current = load_operation_from(
            &mut *transaction,
            OperationSelector::Operation(operation.plan.operation_id),
        )
        .await?
        .ok_or(WorkspaceJournalError::NotFound)?;
        if current.plan != operation.plan {
            return Err(WorkspaceJournalError::IdempotencyConflict);
        }

        if current == operation {
            match (operation.resulting_revision, resulting_snapshot.as_ref()) {
                (Some(revision), Some(publication))
                    if publication.snapshot.manifest.revision == revision
                        && publication.snapshot.project_id == operation.plan.project_id =>
                {
                    let stored = load_snapshot_from(&mut *transaction, revision)
                        .await?
                        .ok_or(WorkspaceJournalError::Integrity)?;
                    if stored != publication.snapshot {
                        return Err(WorkspaceJournalError::IdempotencyConflict);
                    }
                }
                (Some(revision), None)
                    if operation.state == DurableWorkspaceOperationState::Succeeded =>
                {
                    let stored = load_snapshot_from(&mut *transaction, revision)
                        .await?
                        .ok_or(WorkspaceJournalError::Integrity)?;
                    if stored.project_id != operation.plan.project_id {
                        return Err(WorkspaceJournalError::Integrity);
                    }
                }
                (None, None) => {}
                _ => return Err(WorkspaceJournalError::InvalidTransition),
            }
            transaction.commit().await.map_err(storage_error)?;
            return Ok(current);
        }
        if current.state.is_terminal() {
            return Err(WorkspaceJournalError::InvalidTransition);
        }
        if current.state != expected_state {
            return Err(WorkspaceJournalError::RevisionConflict);
        }
        if !expected_state.can_transition_to(operation.state) {
            return Err(WorkspaceJournalError::InvalidTransition);
        }

        match (operation.resulting_revision, resulting_snapshot.as_ref()) {
            (Some(revision), Some(publication))
                if operation.state == DurableWorkspaceOperationState::Succeeded
                    && publication.snapshot.manifest.revision == revision
                    && publication.snapshot.project_id == operation.plan.project_id
                    && revision.binding_id() == operation.plan.binding_id =>
            {
                commit_snapshot_tx(
                    &mut transaction,
                    Some(publication.expected_head),
                    &publication.snapshot,
                )
                .await?;
            }
            (Some(revision), None)
                if operation.state == DurableWorkspaceOperationState::Succeeded
                    && revision.binding_id() == operation.plan.binding_id =>
            {
                let stored = load_snapshot_from(&mut *transaction, revision)
                    .await?
                    .ok_or(WorkspaceJournalError::Integrity)?;
                if stored.project_id != operation.plan.project_id {
                    return Err(WorkspaceJournalError::Integrity);
                }
            }
            (None, None) if operation.state != DurableWorkspaceOperationState::Succeeded => {}
            _ => return Err(WorkspaceJournalError::InvalidTransition),
        }
        update_operation_tx(&mut transaction, expected_state, &operation).await?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(operation)
    }

    async fn save_checkpoint(
        &self,
        checkpoint: WorkspaceCheckpointRecord,
        blobs: Vec<WorkspaceBlob>,
    ) -> Result<WorkspaceCheckpointRecord, WorkspaceJournalError> {
        let supplied = supplied_blobs(blobs)?;
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        let result = save_checkpoint_tx(&mut transaction, &checkpoint, &supplied).await?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(result)
    }

    async fn load_checkpoint(
        &self,
        checkpoint_id: CheckpointId,
    ) -> Result<Option<WorkspaceCheckpointRecord>, WorkspaceJournalError> {
        load_checkpoint_from(&self.pool, checkpoint_id).await
    }

    async fn load_checkpoint_blobs(
        &self,
        checkpoint_id: CheckpointId,
    ) -> Result<Vec<WorkspaceBlob>, WorkspaceJournalError> {
        if load_checkpoint_from(&self.pool, checkpoint_id)
            .await?
            .is_none()
        {
            return Err(WorkspaceJournalError::NotFound);
        }
        load_blobs_from(&self.pool, BlobOwner::Checkpoint(checkpoint_id)).await
    }
}

impl SqliteControlStore {
    pub(crate) async fn verify_workspace_journal_integrity(
        &self,
    ) -> Result<(), WorkspaceJournalError> {
        for row in sqlx::query("SELECT * FROM workspace_snapshots ORDER BY binding_id, sequence")
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?
        {
            parse_snapshot_row(&row)?;
        }

        for head in
            sqlx::query("SELECT binding_id, snapshot_id, sequence FROM workspace_snapshot_heads")
                .fetch_all(&self.pool)
                .await
                .map_err(storage_error)?
        {
            let binding_id: String = head.try_get("binding_id").map_err(integrity_error)?;
            let snapshot_id: String = head.try_get("snapshot_id").map_err(integrity_error)?;
            let sequence: i64 = head.try_get("sequence").map_err(integrity_error)?;
            let revision = WorkspaceRevision::new(
                WorkspaceBindingId(parse_uuid(&binding_id)?),
                WorkspaceSnapshotId(parse_uuid(&snapshot_id)?),
                from_i64(sequence)?,
            )
            .map_err(|_| WorkspaceJournalError::Integrity)?;
            load_snapshot_from(&self.pool, revision)
                .await?
                .ok_or(WorkspaceJournalError::Integrity)?;
            let maximum = sqlx::query_scalar::<_, i64>(
                "SELECT MAX(sequence) FROM workspace_snapshots WHERE binding_id = ?",
            )
            .bind(&binding_id)
            .fetch_one(&self.pool)
            .await
            .map_err(storage_error)?;
            if maximum != sequence {
                return Err(WorkspaceJournalError::Integrity);
            }
        }
        let snapshot_bindings = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(DISTINCT binding_id) FROM workspace_snapshots",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)?;
        let heads = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workspace_snapshot_heads")
            .fetch_one(&self.pool)
            .await
            .map_err(storage_error)?;
        if snapshot_bindings != heads {
            return Err(WorkspaceJournalError::Integrity);
        }

        for row in sqlx::query("SELECT * FROM workspace_checkpoints ORDER BY checkpoint_id")
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?
        {
            let checkpoint = parse_checkpoint_row(&row)?;
            let expected = expected_checkpoint_refs(&checkpoint)?;
            let stored = blob_map(
                load_blobs_from(&self.pool, BlobOwner::Checkpoint(checkpoint.checkpoint_id))
                    .await?,
            )?;
            if selected_blobs(&expected, &stored)? != stored || expected.len() != stored.len() {
                return Err(WorkspaceJournalError::Integrity);
            }
        }

        for row in sqlx::query("SELECT * FROM workspace_operations ORDER BY operation_id")
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?
        {
            let operation = parse_operation_row(&row)?;
            let checkpoint = load_checkpoint_from(&self.pool, operation.plan.safety_checkpoint_id)
                .await?
                .ok_or(WorkspaceJournalError::Integrity)?;
            validate_safety_checkpoint(&operation, &checkpoint)?;
            let expected = expected_operation_refs(&operation)?;
            let stored = blob_map(
                load_blobs_from(
                    &self.pool,
                    BlobOwner::Operation(operation.plan.operation_id),
                )
                .await?,
            )?;
            if selected_blobs(&expected, &stored)? != stored || expected.len() != stored.len() {
                return Err(WorkspaceJournalError::Integrity);
            }
            if let Some(revision) = operation.resulting_revision {
                load_snapshot_from(&self.pool, revision)
                    .await?
                    .ok_or(WorkspaceJournalError::Integrity)?;
            }
        }

        for row in sqlx::query("SELECT content_sha256, byte_size, bytes FROM workspace_blob_data")
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?
        {
            let hash: Vec<u8> = row.try_get("content_sha256").map_err(integrity_error)?;
            let byte_size: i64 = row.try_get("byte_size").map_err(integrity_error)?;
            let bytes: Vec<u8> = row.try_get("bytes").map_err(integrity_error)?;
            let actual_hash: [u8; 32] = Sha256::digest(&bytes).into();
            if hash.as_slice() != actual_hash
                || from_i64(byte_size)?
                    != u64::try_from(bytes.len()).map_err(|_| WorkspaceJournalError::Integrity)?
            {
                return Err(WorkspaceJournalError::Integrity);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dennett_contracts::{ArtifactId, EffectId, ProjectRelativePath};
    use dennett_effect_core::workspace::{
        CanonicalObjectRef, DurableWorkspaceFailure, FileMutationKind, PortableFilePermissions,
        ResolvedFileChangeProposal, WorkspaceCheckpointEntry, WorkspaceFileEffectPlan,
        WorkspaceFileEffectRequest, WorkspaceManifestEntry, WorkspacePathState,
    };
    use dennett_memory_core::session::SessionJournalError;
    use tempfile::TempDir;

    const SCOPE: ContentSha256 = ContentSha256([7; 32]);
    const PERMISSIONS: PortableFilePermissions = PortableFilePermissions {
        read_only: false,
        executable: false,
    };

    fn path(value: &str) -> ProjectRelativePath {
        ProjectRelativePath::try_from(value).expect("valid project-relative path")
    }

    fn revision(binding_id: WorkspaceBindingId, sequence: u64) -> WorkspaceRevision {
        WorkspaceRevision::new(binding_id, WorkspaceSnapshotId::new(), sequence)
            .expect("positive workspace revision")
    }

    fn snapshot(
        project_id: ProjectId,
        revision: WorkspaceRevision,
        entries: Vec<WorkspaceManifestEntry>,
        observed_at_unix_ms: u64,
    ) -> WorkspaceSnapshotRecord {
        WorkspaceSnapshotRecord {
            project_id,
            manifest: WorkspaceManifest::new(revision, SCOPE, true, entries)
                .expect("valid manifest"),
            observed_at_unix_ms,
        }
    }

    fn file_state(blob: &WorkspaceBlob) -> WorkspacePathState {
        WorkspacePathState::RegularFile {
            content_sha256: blob.reference.content_sha256,
            metadata_sha256: PERMISSIONS.metadata_sha256(),
            byte_size: blob.reference.byte_size,
        }
    }

    async fn seed_binding(
        store: &SqliteControlStore,
        project_id: ProjectId,
        binding_id: WorkspaceBindingId,
        root: &std::path::Path,
    ) {
        sqlx::query(
            "INSERT INTO projects(\
                project_id, display_name, primary_binding_id, revision, created_at_unix_ms, \
                updated_at_unix_ms\
             ) VALUES (?, 'workspace journal test', ?, 1, 1, 1)",
        )
        .bind(project_id.0.to_string())
        .bind(binding_id.0.to_string())
        .execute(&store.pool)
        .await
        .expect("seed project");
        sqlx::query(
            "INSERT INTO project_access_policies(\
                project_id, trust_state, revision, updated_at_unix_ms\
             ) VALUES (?, 'restricted', 1, 1)",
        )
        .bind(project_id.0.to_string())
        .execute(&store.pool)
        .await
        .expect("seed policy");
        sqlx::query(
            "INSERT INTO workspace_bindings(\
                binding_id, project_id, canonical_path, canonical_location_key, source_identity, \
                workspace_kind, availability, access_mode, portable_metadata_state, \
                portable_project_id, is_primary, record_revision, created_at_unix_ms, \
                last_verified_at_unix_ms\
             ) VALUES (?, ?, ?, ?, ?, 'folder', 'available', 'read_write', 'absent', \
                NULL, 1, 1, 1, 1)",
        )
        .bind(binding_id.0.to_string())
        .bind(project_id.0.to_string())
        .bind(root.to_string_lossy().into_owned())
        .bind([4_u8; 32].as_slice())
        .bind([5_u8; 32].as_slice())
        .execute(&store.pool)
        .await
        .expect("seed binding");
    }

    async fn store_fixture() -> (
        TempDir,
        std::path::PathBuf,
        SqliteControlStore,
        ProjectId,
        WorkspaceBindingId,
    ) {
        let temp = tempfile::tempdir().expect("temporary directory");
        let database = temp.path().join("control.sqlite");
        let store = SqliteControlStore::open(&database)
            .await
            .expect("open sqlite control store");
        let project_id = ProjectId::new();
        let binding_id = WorkspaceBindingId::new();
        seed_binding(&store, project_id, binding_id, temp.path()).await;
        (temp, database, store, project_id, binding_id)
    }

    fn add_effect(
        project_id: ProjectId,
        binding_id: WorkspaceBindingId,
        base_revision: WorkspaceRevision,
        command_id: CommandId,
        relative_path: &str,
        prepared_at_unix_ms: u64,
    ) -> (
        WorkspaceOperationRecord,
        WorkspaceCheckpointRecord,
        Vec<WorkspaceBlob>,
    ) {
        let blob = WorkspaceBlob::from_bytes(
            format!("after-{relative_path}"),
            format!("new content for {relative_path}").into_bytes(),
        )
        .expect("valid staged content");
        let manifest = WorkspaceManifest::new(base_revision, SCOPE, true, vec![])
            .expect("valid empty manifest");
        let checkpoint_id = CheckpointId::new();
        let operation_id = WorkspaceOperationId::new();
        let target = path(relative_path);
        let plan = WorkspaceFileEffectPlan::build(
            &manifest,
            WorkspaceFileEffectRequest {
                operation_id,
                command_id,
                correlation_id: format!("test.{relative_path}"),
                project_id,
                binding_id,
                base_revision,
                intent_sha256: ContentSha256([9; 32]),
                safety_checkpoint_id: checkpoint_id,
                prepared_at_unix_ms,
                changes: vec![ResolvedFileChangeProposal {
                    kind: FileMutationKind::Add,
                    path: target.clone(),
                    previous_path: None,
                    content: Some(blob.reference.clone()),
                    expected_content_sha256: None,
                    resulting_permissions: Some(PERMISSIONS),
                }],
            },
        )
        .expect("valid file effect plan");
        let operation = WorkspaceOperationRecord {
            plan,
            state: DurableWorkspaceOperationState::Prepared,
            resulting_revision: None,
            failure: None,
            completed_at_unix_ms: None,
        };
        let checkpoint = WorkspaceCheckpointRecord {
            checkpoint_id,
            project_id,
            binding_id,
            base_revision,
            captured_revision: base_revision,
            state: DurableCheckpointState::Available,
            label: "Before file effect".to_owned(),
            request_summary: "Safety checkpoint".to_owned(),
            entries: vec![WorkspaceCheckpointEntry {
                path: target,
                state: WorkspacePathState::Absent,
                content: None,
            }],
            artifacts: vec![],
            external_effects: vec![],
            provider_continuation: None,
            created_at_unix_ms: prepared_at_unix_ms,
        };
        (operation, checkpoint, vec![blob])
    }

    #[tokio::test]
    async fn snapshot_commit_is_cas_bound_and_idempotent() {
        let (_temp, _database, store, project_id, binding_id) = store_fixture().await;
        let first_revision = revision(binding_id, 1);
        let first = snapshot(project_id, first_revision, vec![], 10);
        assert_eq!(
            store.commit_snapshot(None, first.clone()).await.unwrap(),
            SnapshotCommitOutcome::Inserted
        );
        assert_eq!(
            store.commit_snapshot(None, first.clone()).await.unwrap(),
            SnapshotCommitOutcome::AlreadyCurrent
        );

        let second_revision = revision(binding_id, 2);
        let second = snapshot(project_id, second_revision, vec![], 20);
        assert_eq!(
            store.commit_snapshot(None, second.clone()).await,
            Err(WorkspaceJournalError::RevisionConflict)
        );
        assert_eq!(
            store
                .commit_snapshot(Some(first_revision), second.clone())
                .await
                .unwrap(),
            SnapshotCommitOutcome::Inserted
        );
        assert_eq!(store.load_head(binding_id).await.unwrap(), Some(second));
        assert_eq!(
            store.load_snapshot(first_revision).await.unwrap(),
            Some(first)
        );
    }

    #[tokio::test]
    async fn prepare_atomically_persists_operation_checkpoint_and_blobs() {
        let (_temp, database, store, project_id, binding_id) = store_fixture().await;
        let base_revision = revision(binding_id, 1);
        store
            .commit_snapshot(None, snapshot(project_id, base_revision, vec![], 10))
            .await
            .unwrap();
        let (operation, checkpoint, blobs) = add_effect(
            project_id,
            binding_id,
            base_revision,
            CommandId::new(),
            "new.txt",
            20,
        );
        assert_eq!(
            store
                .prepare_file_effect(operation.clone(), checkpoint.clone(), blobs.clone())
                .await
                .unwrap(),
            operation
        );
        assert_eq!(
            store
                .load_checkpoint(checkpoint.checkpoint_id)
                .await
                .unwrap(),
            Some(checkpoint.clone())
        );
        assert_eq!(
            store
                .load_operation_blobs(operation.plan.operation_id)
                .await
                .unwrap(),
            blobs
        );
        store.close().await;

        let reopened = SqliteControlStore::open(database).await.unwrap();
        assert_eq!(
            reopened
                .prepare_file_effect(operation.clone(), checkpoint, blobs)
                .await
                .unwrap(),
            operation
        );
    }

    #[tokio::test]
    async fn failed_prepare_rolls_back_checkpoint_and_blob_rows() {
        let (_temp, _database, store, project_id, binding_id) = store_fixture().await;
        let base_revision = revision(binding_id, 1);
        store
            .commit_snapshot(None, snapshot(project_id, base_revision, vec![], 10))
            .await
            .unwrap();
        let (mut operation, checkpoint, blobs) = add_effect(
            project_id,
            binding_id,
            base_revision,
            CommandId::new(),
            "rollback.txt",
            20,
        );
        operation.plan.prepared_at_unix_ms = u64::MAX;

        assert_eq!(
            store
                .prepare_file_effect(operation.clone(), checkpoint.clone(), blobs)
                .await,
            Err(WorkspaceJournalError::Integrity)
        );
        assert_eq!(
            store
                .load_operation(operation.plan.operation_id)
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            store
                .load_checkpoint(checkpoint.checkpoint_id)
                .await
                .unwrap(),
            None
        );
        let blob_rows = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workspace_blob_data")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(blob_rows, 0);
    }

    #[tokio::test]
    async fn command_reuse_with_changed_intent_is_rejected() {
        let (_temp, _database, store, project_id, binding_id) = store_fixture().await;
        let base_revision = revision(binding_id, 1);
        store
            .commit_snapshot(None, snapshot(project_id, base_revision, vec![], 10))
            .await
            .unwrap();
        let command_id = CommandId::new();
        let (operation, checkpoint, blobs) = add_effect(
            project_id,
            binding_id,
            base_revision,
            command_id,
            "one.txt",
            20,
        );
        store
            .prepare_file_effect(operation.clone(), checkpoint.clone(), blobs.clone())
            .await
            .unwrap();
        let mut changed = operation.clone();
        changed.plan.intent_sha256 = ContentSha256([3; 32]);
        assert_eq!(
            store.prepare_file_effect(changed, checkpoint, blobs).await,
            Err(WorkspaceJournalError::IdempotencyConflict)
        );
        assert_eq!(
            store.load_operation_by_command(command_id).await.unwrap(),
            Some(operation)
        );
    }

    #[tokio::test]
    async fn terminal_operation_transitions_are_forward_only_and_replay_safe() {
        let (_temp, _database, store, project_id, binding_id) = store_fixture().await;
        let base_revision = revision(binding_id, 1);
        store
            .commit_snapshot(None, snapshot(project_id, base_revision, vec![], 10))
            .await
            .unwrap();
        let (prepared, checkpoint, blobs) = add_effect(
            project_id,
            binding_id,
            base_revision,
            CommandId::new(),
            "result.txt",
            20,
        );
        store
            .prepare_file_effect(prepared.clone(), checkpoint, blobs)
            .await
            .unwrap();
        let mut applied = prepared.clone();
        applied.state = DurableWorkspaceOperationState::FilesystemApplied;
        store
            .transition_operation(
                DurableWorkspaceOperationState::Prepared,
                applied.clone(),
                None,
            )
            .await
            .unwrap();

        let mut invalid_failure = applied.clone();
        invalid_failure.state = DurableWorkspaceOperationState::Failed;
        invalid_failure.failure = Some(DurableWorkspaceFailure {
            kind: DurableWorkspaceFailureKind::AdapterFailure,
            safe_code: "workspace.adapter_failed".to_owned(),
            conflicting_paths: vec![],
        });
        invalid_failure.completed_at_unix_ms = Some(30);
        assert_eq!(
            store
                .transition_operation(
                    DurableWorkspaceOperationState::FilesystemApplied,
                    invalid_failure,
                    None,
                )
                .await,
            Err(WorkspaceJournalError::InvalidTransition)
        );

        let resulting_revision = revision(binding_id, 2);
        let transition = &applied.plan.transitions[0];
        let resulting_snapshot = snapshot(
            project_id,
            resulting_revision,
            vec![WorkspaceManifestEntry {
                path: transition.path.clone(),
                state: transition.after.clone(),
            }],
            40,
        );
        let publication = WorkspaceSnapshotPublication {
            expected_head: base_revision,
            snapshot: resulting_snapshot,
        };
        let mut succeeded = applied.clone();
        succeeded.state = DurableWorkspaceOperationState::Succeeded;
        succeeded.resulting_revision = Some(resulting_revision);
        succeeded.completed_at_unix_ms = Some(40);
        assert_eq!(
            store
                .transition_operation(
                    DurableWorkspaceOperationState::FilesystemApplied,
                    succeeded.clone(),
                    Some(publication.clone()),
                )
                .await
                .unwrap(),
            succeeded
        );
        assert_eq!(
            store
                .transition_operation(
                    DurableWorkspaceOperationState::FilesystemApplied,
                    succeeded.clone(),
                    Some(publication),
                )
                .await
                .unwrap(),
            succeeded
        );
        assert_eq!(
            store
                .transition_operation(DurableWorkspaceOperationState::Succeeded, prepared, None,)
                .await,
            Err(WorkspaceJournalError::InvalidTransition)
        );
        assert_eq!(
            store
                .load_operation(succeeded.plan.operation_id)
                .await
                .unwrap(),
            Some(succeeded)
        );
    }

    #[tokio::test]
    async fn unfinished_recovery_loads_only_prepared_and_filesystem_applied() {
        let (_temp, _database, store, project_id, binding_id) = store_fixture().await;
        let base_revision = revision(binding_id, 1);
        store
            .commit_snapshot(None, snapshot(project_id, base_revision, vec![], 10))
            .await
            .unwrap();
        let (prepared, prepared_checkpoint, prepared_blobs) = add_effect(
            project_id,
            binding_id,
            base_revision,
            CommandId::new(),
            "prepared.txt",
            20,
        );
        let (mut applied, applied_checkpoint, applied_blobs) = add_effect(
            project_id,
            binding_id,
            base_revision,
            CommandId::new(),
            "applied.txt",
            30,
        );
        let (mut failed, failed_checkpoint, failed_blobs) = add_effect(
            project_id,
            binding_id,
            base_revision,
            CommandId::new(),
            "failed.txt",
            40,
        );
        for (operation, checkpoint, blobs) in [
            (&prepared, prepared_checkpoint, prepared_blobs),
            (&applied, applied_checkpoint, applied_blobs),
            (&failed, failed_checkpoint, failed_blobs),
        ] {
            store
                .prepare_file_effect(operation.clone(), checkpoint, blobs)
                .await
                .unwrap();
        }
        applied.state = DurableWorkspaceOperationState::FilesystemApplied;
        store
            .transition_operation(
                DurableWorkspaceOperationState::Prepared,
                applied.clone(),
                None,
            )
            .await
            .unwrap();
        failed.state = DurableWorkspaceOperationState::Failed;
        failed.failure = Some(DurableWorkspaceFailure {
            kind: DurableWorkspaceFailureKind::Conflict,
            safe_code: "workspace.path_conflict".to_owned(),
            conflicting_paths: vec![path("failed.txt")],
        });
        failed.completed_at_unix_ms = Some(50);
        store
            .transition_operation(DurableWorkspaceOperationState::Prepared, failed, None)
            .await
            .unwrap();

        let unfinished = store.load_unfinished_operations().await.unwrap();
        assert_eq!(unfinished, vec![prepared, applied]);
    }

    #[tokio::test]
    async fn checkpoint_and_raw_blob_round_trip_without_debug_disclosure() {
        let (_temp, database, store, project_id, binding_id) = store_fixture().await;
        let content = WorkspaceBlob::from_bytes("before-file", b"private bytes".to_vec())
            .expect("valid checkpoint blob");
        assert!(!format!("{content:?}").contains("private bytes"));
        let base_revision = revision(binding_id, 1);
        let entry = WorkspaceManifestEntry {
            path: path("existing.txt"),
            state: file_state(&content),
        };
        store
            .commit_snapshot(
                None,
                snapshot(project_id, base_revision, vec![entry.clone()], 10),
            )
            .await
            .unwrap();
        let checkpoint = WorkspaceCheckpointRecord {
            checkpoint_id: CheckpointId::new(),
            project_id,
            binding_id,
            base_revision,
            captured_revision: base_revision,
            state: DurableCheckpointState::Available,
            label: "Named checkpoint".to_owned(),
            request_summary: "Preserve the current file".to_owned(),
            entries: vec![WorkspaceCheckpointEntry {
                path: entry.path,
                state: entry.state,
                content: Some(content.reference.clone()),
            }],
            artifacts: vec![ArtifactId::new()],
            external_effects: vec![EffectId::new()],
            provider_continuation: Some(CanonicalObjectRef {
                kind: "codex_thread".to_owned(),
                id: "thread-safe-reference".to_owned(),
            }),
            created_at_unix_ms: 20,
        };
        store
            .save_checkpoint(checkpoint.clone(), vec![content.clone()])
            .await
            .unwrap();
        store.close().await;

        let reopened = SqliteControlStore::open(database).await.unwrap();
        assert_eq!(
            reopened
                .load_checkpoint(checkpoint.checkpoint_id)
                .await
                .unwrap(),
            Some(checkpoint.clone())
        );
        assert_eq!(
            reopened
                .load_checkpoint_blobs(checkpoint.checkpoint_id)
                .await
                .unwrap(),
            vec![content.clone()]
        );
        assert_eq!(
            reopened
                .save_checkpoint(checkpoint.clone(), vec![content])
                .await
                .unwrap(),
            checkpoint
        );
    }

    #[tokio::test]
    async fn json_record_corruption_stops_reopen() {
        let (_temp, database, store, project_id, binding_id) = store_fixture().await;
        let base_revision = revision(binding_id, 1);
        store
            .commit_snapshot(None, snapshot(project_id, base_revision, vec![], 10))
            .await
            .unwrap();
        sqlx::query("UPDATE workspace_snapshots SET record_json = '{}'")
            .execute(&store.pool)
            .await
            .expect("inject json corruption");
        store.close().await;

        assert!(matches!(
            SqliteControlStore::open(database).await,
            Err(SessionJournalError::IntegrityFailure(_))
        ));
    }

    #[tokio::test]
    async fn raw_blob_corruption_is_rejected() {
        let (_temp, _database, store, project_id, binding_id) = store_fixture().await;
        let content = WorkspaceBlob::from_bytes("checkpoint-content", b"secret data".to_vec())
            .expect("valid checkpoint blob");
        let base_revision = revision(binding_id, 1);
        let state = file_state(&content);
        store
            .commit_snapshot(
                None,
                snapshot(
                    project_id,
                    base_revision,
                    vec![WorkspaceManifestEntry {
                        path: path("secret.txt"),
                        state: state.clone(),
                    }],
                    10,
                ),
            )
            .await
            .unwrap();
        let checkpoint = WorkspaceCheckpointRecord {
            checkpoint_id: CheckpointId::new(),
            project_id,
            binding_id,
            base_revision,
            captured_revision: base_revision,
            state: DurableCheckpointState::Available,
            label: "Corruption test".to_owned(),
            request_summary: String::new(),
            entries: vec![WorkspaceCheckpointEntry {
                path: path("secret.txt"),
                state,
                content: Some(content.reference.clone()),
            }],
            artifacts: vec![],
            external_effects: vec![],
            provider_continuation: None,
            created_at_unix_ms: 20,
        };
        store
            .save_checkpoint(checkpoint, vec![content])
            .await
            .unwrap();
        sqlx::query("UPDATE workspace_blob_data SET bytes = zeroblob(byte_size)")
            .execute(&store.pool)
            .await
            .expect("inject same-length blob corruption");
        assert_eq!(
            store.verify_workspace_journal_integrity().await,
            Err(WorkspaceJournalError::Integrity)
        );
    }
}
