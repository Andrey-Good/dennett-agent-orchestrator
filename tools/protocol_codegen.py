"""Generate and verify Dennett's committed Protobuf client artifacts."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
from tempfile import TemporaryDirectory
from typing import Mapping, Sequence


ROOT = Path(__file__).resolve().parents[1]
PROTOCOLS = ROOT / "protocols"
GENERATED = ROOT / "generated"
GENERATOR_TEMPLATE = PROTOCOLS / "buf.gen.yaml"
DO_NOT_EDIT_HEADER = b"// DO NOT EDIT. Generated from protocols/proto by Buf.\n"
GENERATED_LANGUAGES = ("rust", "ts")
APPROVED_BUF_CONFIG_SHA256 = "c6c396e445f7d4296c2bec35ceee630878767fad405d656eacc7c3f270302609"
EPOCH_MIGRATION_MANIFEST = PROTOCOLS / "epoch-migrations" / "m00-to-m01.json"
APPROVED_EPOCH_MIGRATION_SHA256 = (
    "211f06392875667913d7dcccda8ef2dce3b25774788e0682621844874f4d9546"
)
M02_INITIAL_DESCRIPTOR_FILES = (
    "dennett/control/v1/project.proto",
    "dennett/control/v1/workspace.proto",
)
# This approval checksum locks every initial M02 field, enum, oneof, option and
# service method. Buf remains the wire-compatibility authority; additive future
# changes intentionally update this checksum during review instead of escaping
# detection merely because they are absent from origin/main today.
APPROVED_M02_INITIAL_DESCRIPTOR_SHA256 = (
    "43c715e8e0fe743fdac38a3551280cf2dfd172dad5d253dd4ce258ab869ca275"
)
COMPARISON_BUF_CONFIG = """version: v2
modules:
  - path: proto
breaking:
  use: [WIRE_JSON]
"""
LintViolation = tuple[str, str, str]
FieldContract = tuple[str, int, str, str, str | None, int | None, bool]
MethodContract = tuple[str, str, str, bool, bool]


def _field(
    name: str,
    number: int,
    field_type: str,
    *,
    label: str = "LABEL_OPTIONAL",
    type_name: str | None = None,
    oneof_index: int | None = None,
    proto3_optional: bool = False,
) -> FieldContract:
    return (
        name,
        number,
        label,
        field_type,
        type_name,
        oneof_index,
        proto3_optional,
    )


def _workspace_outcome(
    success_name: str,
    success_type: str,
) -> tuple[FieldContract, ...]:
    return (
        _field(
            success_name,
            1,
            "TYPE_MESSAGE",
            type_name=success_type,
            oneof_index=0,
        ),
        _field(
            "error",
            2,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspaceFailure",
            oneof_index=0,
        ),
    )


EXPECTED_DESCRIPTOR_FILES = {
    "dennett/common/v1/common.proto",
    "dennett/control/v1/project.proto",
    "dennett/control/v1/session.proto",
    "dennett/control/v1/system.proto",
    "dennett/control/v1/workspace.proto",
    "dennett/sync/v1/watch.proto",
}
EXPECTED_ENUM_VALUES: dict[str, tuple[tuple[str, int], ...]] = {
    ".dennett.sync.v1.ResyncReason": (
        ("RESYNC_REASON_UNSPECIFIED", 0),
        ("RESYNC_REASON_SEQUENCE_GAP", 1),
        ("RESYNC_REASON_REVISION_GAP", 2),
        ("RESYNC_REASON_AUTHORITY_EPOCH_CHANGED", 3),
        ("RESYNC_REASON_STREAM_REPLACED", 4),
        ("RESYNC_REASON_SNAPSHOT_INVALID", 5),
    ),
    ".dennett.control.v1.ComposerDraftWriteState": (
        ("COMPOSER_DRAFT_WRITE_STATE_UNSPECIFIED", 0),
        ("COMPOSER_DRAFT_WRITE_STATE_SAVED", 1),
        ("COMPOSER_DRAFT_WRITE_STATE_ALREADY_ACCEPTED", 2),
    ),
    ".dennett.control.v1.TurnActivityStatus": (
        ("TURN_ACTIVITY_STATUS_UNSPECIFIED", 0),
        ("TURN_ACTIVITY_STATUS_STARTED", 1),
        ("TURN_ACTIVITY_STATUS_UPDATED", 2),
        ("TURN_ACTIVITY_STATUS_COMPLETED", 3),
        ("TURN_ACTIVITY_STATUS_FAILED", 4),
    ),
    ".dennett.control.v1.TurnDeliveryMode": (
        ("TURN_DELIVERY_MODE_UNSPECIFIED", 0),
        ("TURN_DELIVERY_MODE_NEW_TURN", 1),
        ("TURN_DELIVERY_MODE_STEER_NOW", 2),
    ),
    ".dennett.control.v1.ProjectRegistrationKind": (
        ("PROJECT_REGISTRATION_KIND_UNSPECIFIED", 0),
        ("PROJECT_REGISTRATION_KIND_CREATE_EMPTY", 1),
        ("PROJECT_REGISTRATION_KIND_ATTACH_EXISTING", 2),
    ),
    ".dennett.control.v1.ProjectSourceFeature": (
        ("PROJECT_SOURCE_FEATURE_UNSPECIFIED", 0),
        ("PROJECT_SOURCE_FEATURE_VERSIONED_REPOSITORY", 1),
        ("PROJECT_SOURCE_FEATURE_INSTRUCTION_FILES", 2),
        ("PROJECT_SOURCE_FEATURE_PORTABLE_PROJECT_METADATA", 3),
        ("PROJECT_SOURCE_FEATURE_SHARED_PROJECT_MEMORY", 4),
    ),
    ".dennett.control.v1.WorkspaceKind": (
        ("WORKSPACE_KIND_UNSPECIFIED", 0),
        ("WORKSPACE_KIND_FOLDER", 1),
        ("WORKSPACE_KIND_VERSIONED_CHECKOUT", 2),
        ("WORKSPACE_KIND_ISOLATED_CHECKOUT", 3),
        ("WORKSPACE_KIND_REMOTE", 4),
    ),
    ".dennett.control.v1.WorkspaceAvailability": (
        ("WORKSPACE_AVAILABILITY_UNSPECIFIED", 0),
        ("WORKSPACE_AVAILABILITY_AVAILABLE", 1),
        ("WORKSPACE_AVAILABILITY_MISSING", 2),
        ("WORKSPACE_AVAILABILITY_INACCESSIBLE", 3),
        ("WORKSPACE_AVAILABILITY_DETACHED", 4),
    ),
    ".dennett.control.v1.WorkspaceAccessMode": (
        ("WORKSPACE_ACCESS_MODE_UNSPECIFIED", 0),
        ("WORKSPACE_ACCESS_MODE_READ_ONLY", 1),
        ("WORKSPACE_ACCESS_MODE_READ_WRITE", 2),
    ),
    ".dennett.control.v1.ProjectTrustState": (
        ("PROJECT_TRUST_STATE_UNSPECIFIED", 0),
        ("PROJECT_TRUST_STATE_RESTRICTED", 1),
        ("PROJECT_TRUST_STATE_TRUSTED_BOUNDED", 2),
        ("PROJECT_TRUST_STATE_REVOKED", 3),
    ),
    ".dennett.control.v1.PortableProjectMetadataState": (
        ("PORTABLE_PROJECT_METADATA_STATE_UNSPECIFIED", 0),
        ("PORTABLE_PROJECT_METADATA_STATE_ABSENT", 1),
        ("PORTABLE_PROJECT_METADATA_STATE_PRESENT_VALID", 2),
        ("PORTABLE_PROJECT_METADATA_STATE_INVALID", 3),
        ("PORTABLE_PROJECT_METADATA_STATE_IDENTITY_CONFLICT", 4),
        ("PORTABLE_PROJECT_METADATA_STATE_UNSUPPORTED_VERSION", 5),
    ),
    ".dennett.control.v1.SharedProjectMemoryState": (
        ("SHARED_PROJECT_MEMORY_STATE_UNSPECIFIED", 0),
        ("SHARED_PROJECT_MEMORY_STATE_ABSENT", 1),
        ("SHARED_PROJECT_MEMORY_STATE_PRESENT", 2),
        ("SHARED_PROJECT_MEMORY_STATE_INVALID", 3),
    ),
    ".dennett.control.v1.PortableMetadataAction": (
        ("PORTABLE_METADATA_ACTION_UNSPECIFIED", 0),
        ("PORTABLE_METADATA_ACTION_LEAVE_ABSENT", 1),
        ("PORTABLE_METADATA_ACTION_USE_EXISTING", 2),
        ("PORTABLE_METADATA_ACTION_CREATE_MINIMAL", 3),
        ("PORTABLE_METADATA_ACTION_FORK_WITH_NEW_IDENTITY", 4),
    ),
    ".dennett.control.v1.RebindPortableMetadataAction": (
        ("REBIND_PORTABLE_METADATA_ACTION_UNSPECIFIED", 0),
        ("REBIND_PORTABLE_METADATA_ACTION_LEAVE_ABSENT", 1),
        ("REBIND_PORTABLE_METADATA_ACTION_USE_EXISTING", 2),
        ("REBIND_PORTABLE_METADATA_ACTION_CREATE_MINIMAL", 3),
    ),
    ".dennett.control.v1.WorkspaceProjectionState": (
        ("WORKSPACE_PROJECTION_STATE_UNSPECIFIED", 0),
        ("WORKSPACE_PROJECTION_STATE_READY", 1),
        ("WORKSPACE_PROJECTION_STATE_STALE", 2),
        ("WORKSPACE_PROJECTION_STATE_CONFLICT", 3),
        ("WORKSPACE_PROJECTION_STATE_RECOVERY_REQUIRED", 4),
    ),
    ".dennett.control.v1.FileChangeKind": (
        ("FILE_CHANGE_KIND_UNSPECIFIED", 0),
        ("FILE_CHANGE_KIND_ADDED", 1),
        ("FILE_CHANGE_KIND_MODIFIED", 2),
        ("FILE_CHANGE_KIND_DELETED", 3),
        ("FILE_CHANGE_KIND_RENAMED", 4),
    ),
    ".dennett.control.v1.FileReviewState": (
        ("FILE_REVIEW_STATE_UNSPECIFIED", 0),
        ("FILE_REVIEW_STATE_UNREVIEWED", 1),
        ("FILE_REVIEW_STATE_REVIEWED", 2),
        ("FILE_REVIEW_STATE_CHANGES_REQUESTED", 3),
        ("FILE_REVIEW_STATE_ACCEPTED", 4),
    ),
    ".dennett.control.v1.WorkspaceOperationKind": (
        ("WORKSPACE_OPERATION_KIND_UNSPECIFIED", 0),
        ("WORKSPACE_OPERATION_KIND_APPLY_FILE_CHANGES", 1),
        ("WORKSPACE_OPERATION_KIND_RUN_COMMAND", 2),
        ("WORKSPACE_OPERATION_KIND_RUN_TEST", 3),
        ("WORKSPACE_OPERATION_KIND_CREATE_CHECKPOINT", 4),
        ("WORKSPACE_OPERATION_KIND_RESTORE_CHECKPOINT", 5),
        ("WORKSPACE_OPERATION_KIND_REVIEW_ACTION", 6),
    ),
    ".dennett.control.v1.WorkspaceOperationState": (
        ("WORKSPACE_OPERATION_STATE_UNSPECIFIED", 0),
        ("WORKSPACE_OPERATION_STATE_ACCEPTED", 1),
        ("WORKSPACE_OPERATION_STATE_RUNNING", 2),
        ("WORKSPACE_OPERATION_STATE_SUCCEEDED", 3),
        ("WORKSPACE_OPERATION_STATE_FAILED", 4),
        ("WORKSPACE_OPERATION_STATE_CANCELLED", 5),
        ("WORKSPACE_OPERATION_STATE_TIMED_OUT", 6),
        ("WORKSPACE_OPERATION_STATE_RECOVERY_REQUIRED", 7),
    ),
    ".dennett.control.v1.WorkspaceFailureKind": (
        ("WORKSPACE_FAILURE_KIND_UNSPECIFIED", 0),
        ("WORKSPACE_FAILURE_KIND_STALE_SNAPSHOT", 1),
        ("WORKSPACE_FAILURE_KIND_SCOPE_DENIED", 2),
        ("WORKSPACE_FAILURE_KIND_CONFLICT", 3),
        ("WORKSPACE_FAILURE_KIND_CANCELLED", 4),
        ("WORKSPACE_FAILURE_KIND_LOCATION_MISSING", 5),
        ("WORKSPACE_FAILURE_KIND_ADAPTER_RETRYABLE", 6),
        ("WORKSPACE_FAILURE_KIND_ADAPTER_TERMINAL", 7),
        ("WORKSPACE_FAILURE_KIND_VALIDATION", 8),
        ("WORKSPACE_FAILURE_KIND_RECOVERY_REQUIRED", 9),
    ),
    ".dennett.control.v1.ExecutionKind": (
        ("EXECUTION_KIND_UNSPECIFIED", 0),
        ("EXECUTION_KIND_COMMAND", 1),
        ("EXECUTION_KIND_TEST", 2),
    ),
    ".dennett.control.v1.ExecutionTerminalKind": (
        ("EXECUTION_TERMINAL_KIND_UNSPECIFIED", 0),
        ("EXECUTION_TERMINAL_KIND_SUCCEEDED", 1),
        ("EXECUTION_TERMINAL_KIND_FAILED", 2),
        ("EXECUTION_TERMINAL_KIND_TIMED_OUT", 3),
        ("EXECUTION_TERMINAL_KIND_CANCELLED", 4),
        ("EXECUTION_TERMINAL_KIND_SPAWN_FAILED", 5),
        ("EXECUTION_TERMINAL_KIND_RECOVERY_REQUIRED", 6),
    ),
    ".dennett.control.v1.TestOutcome": (
        ("TEST_OUTCOME_UNSPECIFIED", 0),
        ("TEST_OUTCOME_PASSED", 1),
        ("TEST_OUTCOME_FAILED", 2),
        ("TEST_OUTCOME_TIMED_OUT", 3),
        ("TEST_OUTCOME_CANCELLED", 4),
        ("TEST_OUTCOME_SPAWN_FAILED", 5),
        ("TEST_OUTCOME_RECOVERY_REQUIRED", 6),
    ),
    ".dennett.control.v1.ArtifactState": (
        ("ARTIFACT_STATE_UNSPECIFIED", 0),
        ("ARTIFACT_STATE_AVAILABLE", 1),
        ("ARTIFACT_STATE_MISSING", 2),
        ("ARTIFACT_STATE_OVERSIZED", 3),
        ("ARTIFACT_STATE_OUT_OF_SCOPE", 4),
        ("ARTIFACT_STATE_UNSUPPORTED", 5),
    ),
    ".dennett.control.v1.ArtifactKind": (
        ("ARTIFACT_KIND_UNSPECIFIED", 0),
        ("ARTIFACT_KIND_FILE", 1),
        ("ARTIFACT_KIND_DIRECTORY", 2),
        ("ARTIFACT_KIND_REPORT", 3),
        ("ARTIFACT_KIND_BINARY", 4),
        ("ARTIFACT_KIND_PATCH", 5),
        ("ARTIFACT_KIND_OTHER", 6),
    ),
    ".dennett.control.v1.CheckpointState": (
        ("CHECKPOINT_STATE_UNSPECIFIED", 0),
        ("CHECKPOINT_STATE_AVAILABLE", 1),
        ("CHECKPOINT_STATE_RESTORING", 2),
        ("CHECKPOINT_STATE_RESTORED", 3),
        ("CHECKPOINT_STATE_PARTIALLY_APPLIED", 4),
        ("CHECKPOINT_STATE_RECOVERY_REQUIRED", 5),
    ),
    ".dennett.control.v1.ReviewState": (
        ("REVIEW_STATE_UNSPECIFIED", 0),
        ("REVIEW_STATE_PENDING", 1),
        ("REVIEW_STATE_IN_REVIEW", 2),
        ("REVIEW_STATE_CHANGES_REQUESTED", 3),
        ("REVIEW_STATE_APPROVED", 4),
        ("REVIEW_STATE_STALE", 5),
    ),
    ".dennett.control.v1.ReviewActionKind": (
        ("REVIEW_ACTION_KIND_UNSPECIFIED", 0),
        ("REVIEW_ACTION_KIND_REQUEST_CHANGES", 1),
        ("REVIEW_ACTION_KIND_APPROVE", 2),
        ("REVIEW_ACTION_KIND_CONTINUE_REVIEWING", 3),
    ),
    ".dennett.control.v1.ReviewCommentState": (
        ("REVIEW_COMMENT_STATE_UNSPECIFIED", 0),
        ("REVIEW_COMMENT_STATE_OPEN", 1),
        ("REVIEW_COMMENT_STATE_RESOLVED", 2),
        ("REVIEW_COMMENT_STATE_OUTDATED", 3),
    ),
}
EXPECTED_SERVICE_METHODS: dict[str, tuple[MethodContract, ...]] = {
    ".dennett.control.v1.SystemService": (
        (
            "Handshake",
            ".dennett.control.v1.HandshakeRequest",
            ".dennett.control.v1.HandshakeResponse",
            False,
            False,
        ),
        (
            "Bootstrap",
            ".dennett.control.v1.BootstrapRequest",
            ".dennett.control.v1.BootstrapResponse",
            False,
            False,
        ),
        (
            "Watch",
            ".dennett.control.v1.WatchRequest",
            ".dennett.control.v1.WatchResponse",
            False,
            True,
        ),
        (
            "GetHealth",
            ".dennett.control.v1.GetHealthRequest",
            ".dennett.control.v1.GetHealthResponse",
            False,
            False,
        ),
    ),
    ".dennett.control.v1.ProjectService": (
        (
            "CreateProject",
            ".dennett.control.v1.CreateProjectRequest",
            ".dennett.control.v1.CreateProjectResponse",
            False,
            False,
        ),
        (
            "ListProjects",
            ".dennett.control.v1.ListProjectsRequest",
            ".dennett.control.v1.ListProjectsResponse",
            False,
            False,
        ),
        (
            "GetProject",
            ".dennett.control.v1.GetProjectRequest",
            ".dennett.control.v1.GetProjectResponse",
            False,
            False,
        ),
        (
            "InspectProjectLocation",
            ".dennett.control.v1.InspectProjectLocationRequest",
            ".dennett.control.v1.InspectProjectLocationResponse",
            False,
            False,
        ),
        (
            "RegisterProject",
            ".dennett.control.v1.RegisterProjectRequest",
            ".dennett.control.v1.RegisterProjectResponse",
            False,
            False,
        ),
        (
            "RebindProjectWorkspace",
            ".dennett.control.v1.RebindProjectWorkspaceRequest",
            ".dennett.control.v1.RebindProjectWorkspaceResponse",
            False,
            False,
        ),
        (
            "SetProjectTrust",
            ".dennett.control.v1.SetProjectTrustRequest",
            ".dennett.control.v1.SetProjectTrustResponse",
            False,
            False,
        ),
    ),
    ".dennett.control.v1.SessionService": (
        (
            "CreateSession",
            ".dennett.control.v1.CreateSessionRequest",
            ".dennett.control.v1.CreateSessionResponse",
            False,
            False,
        ),
        (
            "SendTurn",
            ".dennett.control.v1.SendTurnRequest",
            ".dennett.control.v1.SendTurnResponse",
            False,
            False,
        ),
        (
            "CancelTurn",
            ".dennett.control.v1.CancelTurnRequest",
            ".dennett.control.v1.CancelTurnResponse",
            False,
            False,
        ),
        (
            "WatchSession",
            ".dennett.control.v1.WatchSessionRequest",
            ".dennett.control.v1.WatchSessionResponse",
            False,
            True,
        ),
        (
            "GetComposerDraft",
            ".dennett.control.v1.GetComposerDraftRequest",
            ".dennett.control.v1.GetComposerDraftResponse",
            False,
            False,
        ),
        (
            "SaveComposerDraft",
            ".dennett.control.v1.SaveComposerDraftRequest",
            ".dennett.control.v1.SaveComposerDraftResponse",
            False,
            False,
        ),
        (
            "DiscardComposerDraft",
            ".dennett.control.v1.DiscardComposerDraftRequest",
            ".dennett.control.v1.DiscardComposerDraftResponse",
            False,
            False,
        ),
    ),
    ".dennett.control.v1.WorkspaceService": (
        (
            "GetWorkspace",
            ".dennett.control.v1.GetWorkspaceRequest",
            ".dennett.control.v1.GetWorkspaceResponse",
            False,
            False,
        ),
        (
            "WatchWorkspace",
            ".dennett.control.v1.WatchWorkspaceRequest",
            ".dennett.control.v1.WatchWorkspaceResponse",
            False,
            True,
        ),
        (
            "GetWorkspaceDiff",
            ".dennett.control.v1.GetWorkspaceDiffRequest",
            ".dennett.control.v1.GetWorkspaceDiffResponse",
            False,
            False,
        ),
        (
            "ApplyFileChanges",
            ".dennett.control.v1.ApplyFileChangesRequest",
            ".dennett.control.v1.ApplyFileChangesResponse",
            False,
            False,
        ),
        (
            "RunWorkspaceCommand",
            ".dennett.control.v1.RunWorkspaceCommandRequest",
            ".dennett.control.v1.RunWorkspaceCommandResponse",
            False,
            False,
        ),
        (
            "CancelWorkspaceOperation",
            ".dennett.control.v1.CancelWorkspaceOperationRequest",
            ".dennett.control.v1.CancelWorkspaceOperationResponse",
            False,
            False,
        ),
        (
            "CreateCheckpoint",
            ".dennett.control.v1.CreateCheckpointRequest",
            ".dennett.control.v1.CreateCheckpointResponse",
            False,
            False,
        ),
        (
            "RestoreCheckpoint",
            ".dennett.control.v1.RestoreCheckpointRequest",
            ".dennett.control.v1.RestoreCheckpointResponse",
            False,
            False,
        ),
        (
            "SubmitReviewAction",
            ".dennett.control.v1.SubmitReviewActionRequest",
            ".dennett.control.v1.SubmitReviewActionResponse",
            False,
            False,
        ),
    ),
}
EXPECTED_MESSAGE_FIELDS: dict[str, tuple[FieldContract, ...]] = {
    ".dennett.common.v1.CommandMetadata": (
        _field("command_id", 1, "TYPE_STRING"),
        _field("idempotency_key", 2, "TYPE_STRING"),
        _field("correlation_id", 3, "TYPE_STRING"),
        _field("authority_epoch_seen", 4, "TYPE_UINT64"),
        _field("created_at", 5, "TYPE_MESSAGE", type_name=".google.protobuf.Timestamp"),
        _field(
            "expected_revision",
            6,
            "TYPE_UINT64",
            oneof_index=0,
            proto3_optional=True,
        ),
        _field("client_session_id", 7, "TYPE_STRING"),
    ),
    ".dennett.common.v1.CommandAccepted": (
        _field("command_id", 1, "TYPE_STRING"),
        _field("correlation_id", 2, "TYPE_STRING"),
        _field("operation_id", 3, "TYPE_STRING"),
        _field("accepted_revision", 4, "TYPE_UINT64"),
    ),
    ".dennett.common.v1.ErrorEnvelope": (
        _field("code", 1, "TYPE_STRING"),
        _field("message_key", 2, "TYPE_STRING"),
        _field("correlation_id", 3, "TYPE_STRING"),
        _field("retryable", 4, "TYPE_BOOL"),
        _field("user_action_required", 5, "TYPE_BOOL"),
        _field("details_handle", 6, "TYPE_STRING"),
        _field(
            "current_revision",
            7,
            "TYPE_UINT64",
            oneof_index=0,
            proto3_optional=True,
        ),
    ),
    ".dennett.common.v1.CommandResult": (
        _field("completed_revision", 1, "TYPE_UINT64"),
        _field(
            "canonical_refs",
            2,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.common.v1.StableRef",
        ),
        _field("message_key", 3, "TYPE_STRING"),
        _field("partial", 4, "TYPE_BOOL"),
    ),
    ".dennett.common.v1.CommandTerminal": (
        _field("command_id", 1, "TYPE_STRING"),
        _field("operation_id", 2, "TYPE_STRING"),
        _field(
            "result",
            3,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.CommandResult",
            oneof_index=0,
        ),
        _field(
            "error",
            4,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.ErrorEnvelope",
            oneof_index=0,
        ),
    ),
    ".dennett.sync.v1.WatchCursor": (
        _field("stream_id", 1, "TYPE_STRING"),
        _field("sequence", 2, "TYPE_UINT64"),
        _field("authority_epoch", 3, "TYPE_UINT64"),
    ),
    ".dennett.sync.v1.ResyncRequired": (
        _field(
            "reason",
            1,
            "TYPE_ENUM",
            type_name=".dennett.sync.v1.ResyncReason",
        ),
        _field("current_revision", 2, "TYPE_UINT64"),
        _field("snapshot_required", 3, "TYPE_BOOL"),
    ),
    ".dennett.control.v1.CreateProjectAccepted": (
        _field(
            "command",
            1,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.CommandAccepted",
        ),
        _field("project_id", 2, "TYPE_STRING"),
    ),
    ".dennett.control.v1.ListProjectsRequest": (
        _field("page_size", 1, "TYPE_UINT32"),
        _field("page_token", 2, "TYPE_STRING"),
        _field("client_session_id", 3, "TYPE_STRING"),
    ),
    ".dennett.control.v1.GetProjectRequest": (
        _field("project_id", 1, "TYPE_STRING"),
        _field("client_session_id", 2, "TYPE_STRING"),
    ),
    ".dennett.control.v1.CreateSessionAccepted": (
        _field(
            "command",
            1,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.CommandAccepted",
        ),
        _field("session_id", 2, "TYPE_STRING"),
    ),
    ".dennett.control.v1.SendTurnAccepted": (
        _field(
            "command",
            1,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.CommandAccepted",
        ),
        _field("turn_id", 2, "TYPE_STRING"),
    ),
    ".dennett.control.v1.WatchSessionRequest": (
        _field("session_id", 1, "TYPE_STRING"),
        _field(
            "known_revision",
            2,
            "TYPE_UINT64",
            oneof_index=0,
            proto3_optional=True,
        ),
        _field("client_session_id", 3, "TYPE_STRING"),
    ),
    ".dennett.control.v1.ComposerDraft": (
        _field("project_id", 1, "TYPE_STRING"),
        _field("session_id", 2, "TYPE_STRING"),
        _field("command_id", 3, "TYPE_STRING"),
        _field("text", 4, "TYPE_STRING"),
        _field("updated_at", 5, "TYPE_MESSAGE", type_name=".google.protobuf.Timestamp"),
        _field("revision", 6, "TYPE_UINT64"),
    ),
    ".dennett.control.v1.GetComposerDraftRequest": (
        _field("project_id", 1, "TYPE_STRING"),
        _field("session_id", 2, "TYPE_STRING"),
        _field("client_session_id", 3, "TYPE_STRING"),
    ),
    ".dennett.control.v1.ComposerDraftMissing": (
        _field("session_id", 1, "TYPE_STRING"),
    ),
    ".dennett.control.v1.SaveComposerDraftRequest": (
        _field(
            "operation",
            1,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.CommandMetadata",
        ),
        _field(
            "draft",
            2,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.ComposerDraft",
        ),
    ),
    ".dennett.control.v1.ComposerDraftWriteReceipt": (
        _field("session_id", 1, "TYPE_STRING"),
        _field("command_id", 2, "TYPE_STRING"),
        _field("persisted_at", 3, "TYPE_MESSAGE", type_name=".google.protobuf.Timestamp"),
        _field(
            "state",
            4,
            "TYPE_ENUM",
            type_name=".dennett.control.v1.ComposerDraftWriteState",
        ),
    ),
    ".dennett.control.v1.DiscardComposerDraftRequest": (
        _field(
            "operation",
            1,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.CommandMetadata",
        ),
        _field("project_id", 2, "TYPE_STRING"),
        _field("session_id", 3, "TYPE_STRING"),
        _field("draft_command_id", 4, "TYPE_STRING"),
    ),
    ".dennett.control.v1.ComposerDraftDiscarded": (
        _field("session_id", 1, "TYPE_STRING"),
        _field("existed", 2, "TYPE_BOOL"),
    ),
    ".dennett.control.v1.SessionDelta": (
        _field("base_revision", 1, "TYPE_UINT64"),
        _field("new_revision", 2, "TYPE_UINT64"),
        _field(
            "mutations",
            3,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.control.v1.SessionMutation",
        ),
        _field("committed_at", 4, "TYPE_MESSAGE", type_name=".google.protobuf.Timestamp"),
    ),
    ".dennett.control.v1.SessionSnapshot": (
        _field(
            "session",
            1,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.SessionSummary",
        ),
        _field("snapshot_fingerprint", 2, "TYPE_BYTES"),
        _field(
            "turns",
            3,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.control.v1.TurnSnapshot",
        ),
    ),
    ".dennett.control.v1.TurnSnapshot": (
        _field("turn_id", 1, "TYPE_STRING"),
        _field("command_id", 2, "TYPE_STRING"),
        _field("role", 3, "TYPE_ENUM", type_name=".dennett.control.v1.TurnRole"),
        _field("state", 4, "TYPE_ENUM", type_name=".dennett.control.v1.TurnState"),
        _field("text", 5, "TYPE_STRING"),
        _field(
            "result",
            6,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.ResultEnvelope",
            oneof_index=0,
        ),
        _field(
            "error",
            7,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.ErrorEnvelope",
            oneof_index=0,
        ),
        _field("created_at", 8, "TYPE_MESSAGE", type_name=".google.protobuf.Timestamp"),
        _field("completed_at", 9, "TYPE_MESSAGE", type_name=".google.protobuf.Timestamp"),
        _field(
            "activities",
            10,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.control.v1.TurnActivitySnapshot",
        ),
        _field("created_revision", 11, "TYPE_UINT64"),
    ),
    ".dennett.control.v1.TurnActivitySnapshot": (
        _field("activity_id", 1, "TYPE_STRING"),
        _field("phase", 2, "TYPE_STRING"),
        _field(
            "message",
            3,
            "TYPE_STRING",
            oneof_index=0,
            proto3_optional=True,
        ),
        _field(
            "status",
            4,
            "TYPE_ENUM",
            type_name=".dennett.control.v1.TurnActivityStatus",
        ),
        _field("updated_at", 5, "TYPE_MESSAGE", type_name=".google.protobuf.Timestamp"),
        _field(
            "native_extensions",
            6,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.control.v1.NativeExtensionPayload",
        ),
        _field("created_at", 7, "TYPE_MESSAGE", type_name=".google.protobuf.Timestamp"),
        _field("created_revision", 8, "TYPE_UINT64"),
    ),
    ".dennett.control.v1.NativeExtensionPayload": (
        _field("namespace", 1, "TYPE_STRING"),
        _field("schema_version", 2, "TYPE_STRING"),
        _field("json_value", 3, "TYPE_STRING"),
    ),
    ".dennett.control.v1.TurnActivityUpsert": (
        _field("turn_id", 1, "TYPE_STRING"),
        _field(
            "activity",
            2,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.TurnActivitySnapshot",
        ),
    ),
    ".dennett.control.v1.ServerWelcome": (
        _field("protocol_version", 1, "TYPE_UINT32"),
        _field("node_version", 2, "TYPE_STRING"),
        _field("authority_epoch_seen", 3, "TYPE_UINT64"),
        _field("enabled_features", 4, "TYPE_STRING", label="LABEL_REPEATED"),
        _field("session_proof", 5, "TYPE_BYTES"),
        _field("resync_required", 6, "TYPE_BOOL"),
        _field(
            "compatibility_mode",
            7,
            "TYPE_ENUM",
            type_name=".dennett.control.v1.CompatibilityMode",
        ),
        _field("max_message_bytes", 8, "TYPE_UINT64"),
        _field("client_session_id", 9, "TYPE_STRING"),
    ),
    ".dennett.control.v1.BootstrapRequest": (
        _field("session_proof", 1, "TYPE_BYTES"),
        _field(
            "known_revision",
            2,
            "TYPE_UINT64",
            oneof_index=0,
            proto3_optional=True,
        ),
        _field("client_session_id", 3, "TYPE_STRING"),
    ),
    ".dennett.control.v1.BootstrapSnapshot": (
        _field("revision", 1, "TYPE_UINT64"),
        _field("authority_epoch", 2, "TYPE_UINT64"),
        _field("observed_at", 3, "TYPE_MESSAGE", type_name=".google.protobuf.Timestamp"),
        _field(
            "projects",
            4,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.control.v1.ProjectSummary",
        ),
        _field(
            "recent_sessions",
            5,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.control.v1.SessionSummary",
        ),
        _field("active_project_id", 6, "TYPE_STRING"),
        _field("active_session_id", 7, "TYPE_STRING"),
        _field(
            "node_state",
            8,
            "TYPE_ENUM",
            type_name=".dennett.control.v1.HealthState",
        ),
        _field(
            "runtime",
            9,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.RuntimeSummary",
        ),
    ),
    ".dennett.control.v1.RuntimeSummary": (
        _field("adapter_id", 1, "TYPE_STRING"),
        _field("runtime_kind", 2, "TYPE_STRING"),
        _field("streaming", 3, "TYPE_BOOL"),
        _field("continuation", 4, "TYPE_BOOL"),
        _field("scoped_cancellation", 5, "TYPE_BOOL"),
        _field("deadlines", 6, "TYPE_BOOL"),
        _field("native_extension_schemas", 7, "TYPE_STRING", label="LABEL_REPEATED"),
        _field(
            "controls",
            8,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.control.v1.RuntimeControlDescriptor",
        ),
        _field("steering", 9, "TYPE_STRING"),
    ),
    ".dennett.control.v1.RuntimeControlCondition": (
        _field("control_id", 1, "TYPE_STRING"),
        _field("choice_ids", 2, "TYPE_STRING", label="LABEL_REPEATED"),
    ),
    ".dennett.control.v1.RuntimeControlChoice": (
        _field("id", 1, "TYPE_STRING"),
        _field("label", 2, "TYPE_STRING"),
        _field("description", 3, "TYPE_STRING", oneof_index=0, proto3_optional=True),
        _field(
            "available_when",
            4,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.control.v1.RuntimeControlCondition",
        ),
    ),
    ".dennett.control.v1.RuntimeControlDescriptor": (
        _field("id", 1, "TYPE_STRING"),
        _field("label", 2, "TYPE_STRING"),
        _field("default_choice_id", 3, "TYPE_STRING"),
        _field(
            "choices",
            4,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.control.v1.RuntimeControlChoice",
        ),
    ),
    ".dennett.control.v1.SendTurnRequest": (
        _field(
            "command",
            1,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.CommandMetadata",
        ),
        _field("project_id", 2, "TYPE_STRING"),
        _field("session_id", 3, "TYPE_STRING"),
        _field("text", 4, "TYPE_STRING"),
        _field(
            "attachments",
            5,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.control.v1.ContextAttachment",
        ),
        _field(
            "runtime_controls",
            6,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.control.v1.RuntimeControlSelection",
        ),
        _field(
            "delivery_mode",
            7,
            "TYPE_ENUM",
            type_name=".dennett.control.v1.TurnDeliveryMode",
        ),
        _field("expected_active_turn_id", 8, "TYPE_STRING"),
    ),
    ".dennett.control.v1.RuntimeControlSelection": (
        _field("control_id", 1, "TYPE_STRING"),
        _field("choice_id", 2, "TYPE_STRING"),
    ),
    ".dennett.control.v1.SystemSnapshot": (
        _field(
            "bootstrap",
            1,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.BootstrapSnapshot",
        ),
        _field("snapshot_fingerprint", 2, "TYPE_BYTES"),
    ),
    ".dennett.control.v1.SystemDelta": (
        _field("base_revision", 1, "TYPE_UINT64"),
        _field("new_revision", 2, "TYPE_UINT64"),
        _field(
            "mutations",
            3,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.control.v1.SystemMutation",
        ),
    ),
    ".dennett.control.v1.WatchRequest": (
        _field("client_session_id", 1, "TYPE_STRING"),
        _field(
            "known_revision",
            2,
            "TYPE_UINT64",
            oneof_index=0,
            proto3_optional=True,
        ),
    ),
}
EXPECTED_ONEOFS: dict[str, dict[str, tuple[FieldContract, ...]]] = {
    ".dennett.common.v1.CommandTerminal": {
        "outcome": (
            _field(
                "result",
                3,
                "TYPE_MESSAGE",
                type_name=".dennett.common.v1.CommandResult",
                oneof_index=0,
            ),
            _field(
                "error",
                4,
                "TYPE_MESSAGE",
                type_name=".dennett.common.v1.ErrorEnvelope",
                oneof_index=0,
            ),
        )
    },
    ".dennett.control.v1.HandshakeResponse": {
        "outcome": (
            _field(
                "welcome",
                1,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.ServerWelcome",
                oneof_index=0,
            ),
            _field(
                "error",
                2,
                "TYPE_MESSAGE",
                type_name=".dennett.common.v1.ErrorEnvelope",
                oneof_index=0,
            ),
        )
    },
    ".dennett.control.v1.BootstrapResponse": {
        "outcome": (
            _field(
                "snapshot",
                1,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.BootstrapSnapshot",
                oneof_index=0,
            ),
            _field(
                "error",
                2,
                "TYPE_MESSAGE",
                type_name=".dennett.common.v1.ErrorEnvelope",
                oneof_index=0,
            ),
        )
    },
    ".dennett.control.v1.GetHealthResponse": {
        "outcome": (
            _field(
                "health",
                1,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.GetHealthResult",
                oneof_index=0,
            ),
            _field(
                "error",
                2,
                "TYPE_MESSAGE",
                type_name=".dennett.common.v1.ErrorEnvelope",
                oneof_index=0,
            ),
        )
    },
    ".dennett.control.v1.CreateProjectResponse": {
        "outcome": (
            _field(
                "accepted",
                1,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.CreateProjectAccepted",
                oneof_index=0,
            ),
            _field(
                "error",
                2,
                "TYPE_MESSAGE",
                type_name=".dennett.common.v1.ErrorEnvelope",
                oneof_index=0,
            ),
        )
    },
    ".dennett.control.v1.ListProjectsResponse": {
        "outcome": (
            _field(
                "result",
                1,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.ListProjectsResult",
                oneof_index=0,
            ),
            _field(
                "error",
                2,
                "TYPE_MESSAGE",
                type_name=".dennett.common.v1.ErrorEnvelope",
                oneof_index=0,
            ),
        )
    },
    ".dennett.control.v1.GetProjectResponse": {
        "outcome": (
            _field(
                "project",
                1,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.Project",
                oneof_index=0,
            ),
            _field(
                "error",
                2,
                "TYPE_MESSAGE",
                type_name=".dennett.common.v1.ErrorEnvelope",
                oneof_index=0,
            ),
        )
    },
    ".dennett.control.v1.CreateSessionResponse": {
        "outcome": (
            _field(
                "accepted",
                1,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.CreateSessionAccepted",
                oneof_index=0,
            ),
            _field(
                "error",
                2,
                "TYPE_MESSAGE",
                type_name=".dennett.common.v1.ErrorEnvelope",
                oneof_index=0,
            ),
        )
    },
    ".dennett.control.v1.SendTurnResponse": {
        "outcome": (
            _field(
                "accepted",
                1,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.SendTurnAccepted",
                oneof_index=0,
            ),
            _field(
                "error",
                2,
                "TYPE_MESSAGE",
                type_name=".dennett.common.v1.ErrorEnvelope",
                oneof_index=0,
            ),
        )
    },
    ".dennett.control.v1.CancelTurnResponse": {
        "outcome": (
            _field(
                "accepted",
                1,
                "TYPE_MESSAGE",
                type_name=".dennett.common.v1.CommandAccepted",
                oneof_index=0,
            ),
            _field(
                "error",
                2,
                "TYPE_MESSAGE",
                type_name=".dennett.common.v1.ErrorEnvelope",
                oneof_index=0,
            ),
        )
    },
    ".dennett.control.v1.GetComposerDraftResponse": {
        "outcome": (
            _field(
                "draft",
                1,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.ComposerDraft",
                oneof_index=0,
            ),
            _field(
                "missing",
                2,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.ComposerDraftMissing",
                oneof_index=0,
            ),
            _field(
                "error",
                3,
                "TYPE_MESSAGE",
                type_name=".dennett.common.v1.ErrorEnvelope",
                oneof_index=0,
            ),
        )
    },
    ".dennett.control.v1.SaveComposerDraftResponse": {
        "outcome": (
            _field(
                "saved",
                1,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.ComposerDraftWriteReceipt",
                oneof_index=0,
            ),
            _field(
                "error",
                2,
                "TYPE_MESSAGE",
                type_name=".dennett.common.v1.ErrorEnvelope",
                oneof_index=0,
            ),
        )
    },
    ".dennett.control.v1.DiscardComposerDraftResponse": {
        "outcome": (
            _field(
                "discarded",
                1,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.ComposerDraftDiscarded",
                oneof_index=0,
            ),
            _field(
                "error",
                2,
                "TYPE_MESSAGE",
                type_name=".dennett.common.v1.ErrorEnvelope",
                oneof_index=0,
            ),
        )
    },
    ".dennett.control.v1.TurnSnapshot": {
        "outcome": (
            _field(
                "result",
                6,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.ResultEnvelope",
                oneof_index=0,
            ),
            _field(
                "error",
                7,
                "TYPE_MESSAGE",
                type_name=".dennett.common.v1.ErrorEnvelope",
                oneof_index=0,
            ),
        )
    },
    ".dennett.control.v1.TurnTerminal": {
        "outcome": (
            _field(
                "result",
                3,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.ResultEnvelope",
                oneof_index=0,
            ),
            _field(
                "error",
                4,
                "TYPE_MESSAGE",
                type_name=".dennett.common.v1.ErrorEnvelope",
                oneof_index=0,
            ),
        )
    },
    ".dennett.control.v1.SessionMutation": {
        "mutation": (
            _field(
                "upsert_turn",
                1,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.TurnSnapshot",
                oneof_index=0,
            ),
            _field(
                "append_turn_text",
                2,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.TurnTextAppend",
                oneof_index=0,
            ),
            _field(
                "finish_turn",
                3,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.TurnTerminal",
                oneof_index=0,
            ),
            _field(
                "update_session",
                4,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.SessionMetadataUpdate",
                oneof_index=0,
            ),
            _field(
                "upsert_turn_activity",
                5,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.TurnActivityUpsert",
                oneof_index=0,
            ),
        )
    },
    ".dennett.control.v1.SessionWatchFrame": {
        "frame": (
            _field(
                "snapshot",
                2,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.SessionSnapshot",
                oneof_index=0,
            ),
            _field(
                "delta",
                3,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.SessionDelta",
                oneof_index=0,
            ),
            _field(
                "heartbeat",
                4,
                "TYPE_MESSAGE",
                type_name=".dennett.sync.v1.WatchHeartbeat",
                oneof_index=0,
            ),
            _field(
                "resync_required",
                5,
                "TYPE_MESSAGE",
                type_name=".dennett.sync.v1.ResyncRequired",
                oneof_index=0,
            ),
            _field(
                "error",
                6,
                "TYPE_MESSAGE",
                type_name=".dennett.common.v1.ErrorEnvelope",
                oneof_index=0,
            ),
        )
    },
    ".dennett.control.v1.SystemMutation": {
        "mutation": (
            _field(
                "upsert_project",
                1,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.ProjectSummary",
                oneof_index=0,
            ),
            _field("remove_project_id", 2, "TYPE_STRING", oneof_index=0),
            _field(
                "upsert_session",
                3,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.SessionSummary",
                oneof_index=0,
            ),
            _field("remove_session_id", 4, "TYPE_STRING", oneof_index=0),
            _field(
                "update_selection",
                5,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.SystemSelectionUpdate",
                oneof_index=0,
            ),
            _field(
                "update_health",
                6,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.SystemHealthUpdate",
                oneof_index=0,
            ),
            _field(
                "finish_command",
                7,
                "TYPE_MESSAGE",
                type_name=".dennett.common.v1.CommandTerminal",
                oneof_index=0,
            ),
        )
    },
    ".dennett.control.v1.SystemWatchFrame": {
        "frame": (
            _field(
                "snapshot",
                2,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.SystemSnapshot",
                oneof_index=0,
            ),
            _field(
                "delta",
                3,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.SystemDelta",
                oneof_index=0,
            ),
            _field(
                "heartbeat",
                4,
                "TYPE_MESSAGE",
                type_name=".dennett.sync.v1.WatchHeartbeat",
                oneof_index=0,
            ),
            _field(
                "resync_required",
                5,
                "TYPE_MESSAGE",
                type_name=".dennett.sync.v1.ResyncRequired",
                oneof_index=0,
            ),
            _field(
                "error",
                6,
                "TYPE_MESSAGE",
                type_name=".dennett.common.v1.ErrorEnvelope",
                oneof_index=0,
            ),
        )
    },
}

# M02 extends the accepted M01 epoch additively. These are semantic minimums:
# Buf owns wire compatibility, while this list prevents a superficially
# additive schema from omitting identity, exact revisions, receipts or errors.
M02_REQUIRED_MESSAGE_FIELDS: dict[str, tuple[FieldContract, ...]] = {
    ".dennett.control.v1.ProjectLocationInspection": (
        _field("inspection_id", 1, "TYPE_STRING"),
        _field("root_uri", 3, "TYPE_STRING"),
        _field(
            "portable_metadata",
            7,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.PortableProjectMetadata",
        ),
        _field(
            "location_identity",
            9,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.StableRef",
        ),
    ),
    ".dennett.control.v1.RegisterProjectRequest": (
        _field(
            "command",
            1,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.CommandMetadata",
        ),
        _field("inspection_id", 2, "TYPE_STRING"),
        _field(
            "portable_metadata_action",
            4,
            "TYPE_ENUM",
            type_name=".dennett.control.v1.PortableMetadataAction",
        ),
        _field(
            "initial_trust_state",
            5,
            "TYPE_ENUM",
            type_name=".dennett.control.v1.ProjectTrustState",
        ),
        _field(
            "trust_decision",
            6,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.StableRef",
        ),
    ),
    ".dennett.control.v1.RegisterProjectAccepted": (
        _field(
            "command",
            1,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.CommandAccepted",
        ),
        _field("project_id", 2, "TYPE_STRING"),
        _field("workspace_binding_id", 3, "TYPE_STRING"),
    ),
    ".dennett.control.v1.RebindProjectWorkspaceRequest": (
        _field(
            "command",
            1,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.CommandMetadata",
        ),
        _field("project_id", 2, "TYPE_STRING"),
        _field("current_workspace_binding_id", 3, "TYPE_STRING"),
        _field("inspection_id", 4, "TYPE_STRING"),
        _field(
            "portable_metadata_action",
            5,
            "TYPE_ENUM",
            type_name=".dennett.control.v1.RebindPortableMetadataAction",
        ),
    ),
    ".dennett.control.v1.SetProjectTrustRequest": (
        _field(
            "command",
            1,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.CommandMetadata",
        ),
        _field("project_id", 2, "TYPE_STRING"),
        _field(
            "trust_state",
            3,
            "TYPE_ENUM",
            type_name=".dennett.control.v1.ProjectTrustState",
        ),
        _field(
            "expected_policy_revision",
            4,
            "TYPE_UINT64",
        ),
        _field(
            "trust_decision",
            5,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.StableRef",
        ),
    ),
    ".dennett.control.v1.PortableProjectMetadata": (
        _field(
            "state",
            1,
            "TYPE_ENUM",
            type_name=".dennett.control.v1.PortableProjectMetadataState",
        ),
        _field("project_id", 2, "TYPE_STRING"),
        _field("schema_version", 3, "TYPE_STRING"),
        _field(
            "shared_memory_state",
            4,
            "TYPE_ENUM",
            type_name=".dennett.control.v1.SharedProjectMemoryState",
        ),
        _field("minimal_structure_creation_available", 5, "TYPE_BOOL"),
    ),
    ".dennett.control.v1.ProjectAccessPolicy": (
        _field("project_id", 1, "TYPE_STRING"),
        _field(
            "trust_state",
            2,
            "TYPE_ENUM",
            type_name=".dennett.control.v1.ProjectTrustState",
        ),
        _field("revision", 3, "TYPE_UINT64"),
        _field(
            "policy_ref",
            4,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.StableRef",
        ),
    ),
    ".dennett.control.v1.WorkspaceBinding": (
        _field("workspace_binding_id", 1, "TYPE_STRING"),
        _field("project_id", 2, "TYPE_STRING"),
        _field("device_id", 3, "TYPE_STRING"),
        _field("location_uri", 4, "TYPE_STRING"),
        _field(
            "availability",
            6,
            "TYPE_ENUM",
            type_name=".dennett.control.v1.WorkspaceAvailability",
        ),
        _field(
            "access_mode",
            7,
            "TYPE_ENUM",
            type_name=".dennett.control.v1.WorkspaceAccessMode",
        ),
        _field(
            "portable_metadata",
            9,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.PortableProjectMetadata",
        ),
        _field("record_revision", 11, "TYPE_UINT64"),
    ),
    ".dennett.control.v1.WorkspaceRevision": (
        _field("workspace_binding_id", 1, "TYPE_STRING"),
        _field("sequence", 2, "TYPE_UINT64"),
        _field("snapshot_id", 3, "TYPE_STRING"),
        _field("observed_at", 4, "TYPE_MESSAGE", type_name=".google.protobuf.Timestamp"),
    ),
    ".dennett.control.v1.WorkspaceFailure": (
        _field(
            "kind",
            1,
            "TYPE_ENUM",
            type_name=".dennett.control.v1.WorkspaceFailureKind",
        ),
        _field(
            "error",
            2,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.ErrorEnvelope",
        ),
        _field(
            "current_revision",
            3,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspaceRevision",
        ),
        _field(
            "conflicting_paths",
            4,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.control.v1.WorkspacePath",
        ),
    ),
    ".dennett.control.v1.FileChange": (
        _field("file_change_id", 1, "TYPE_STRING"),
        _field(
            "path",
            3,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspacePath",
        ),
        _field(
            "base_revision",
            5,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspaceRevision",
        ),
        _field(
            "resulting_revision",
            6,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspaceRevision",
        ),
        _field("conflict", 9, "TYPE_BOOL"),
    ),
    ".dennett.control.v1.WorkspaceDiff": (
        _field("diff_id", 1, "TYPE_STRING"),
        _field("project_id", 2, "TYPE_STRING"),
        _field("workspace_binding_id", 3, "TYPE_STRING"),
        _field(
            "from_revision",
            4,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspaceRevision",
        ),
        _field(
            "to_revision",
            5,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspaceRevision",
        ),
        _field(
            "content",
            7,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.StableRef",
        ),
    ),
    ".dennett.control.v1.WorkspaceOperationReceipt": (
        _field("workspace_operation_id", 1, "TYPE_STRING"),
        _field("command_id", 2, "TYPE_STRING"),
        _field("correlation_id", 3, "TYPE_STRING"),
        _field("project_id", 4, "TYPE_STRING"),
        _field("workspace_binding_id", 5, "TYPE_STRING"),
        _field(
            "state",
            7,
            "TYPE_ENUM",
            type_name=".dennett.control.v1.WorkspaceOperationState",
        ),
        _field(
            "base_revision",
            8,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspaceRevision",
        ),
        _field(
            "resulting_revision",
            9,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspaceRevision",
        ),
    ),
    ".dennett.control.v1.CommandReceipt": (
        _field("command_receipt_id", 1, "TYPE_STRING"),
        _field("workspace_operation_id", 2, "TYPE_STRING"),
        _field("command_id", 3, "TYPE_STRING"),
        _field("correlation_id", 4, "TYPE_STRING"),
        _field(
            "observed_revision",
            7,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspaceRevision",
        ),
        _field(
            "terminal_kind",
            10,
            "TYPE_ENUM",
            type_name=".dennett.control.v1.ExecutionTerminalKind",
        ),
        _field(
            "output_evidence",
            14,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.common.v1.StableRef",
        ),
        _field(
            "failure",
            16,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspaceFailure",
        ),
    ),
    ".dennett.control.v1.TestReceipt": (
        _field("test_receipt_id", 1, "TYPE_STRING"),
        _field("command_receipt_id", 2, "TYPE_STRING"),
        _field(
            "verified_revision",
            5,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspaceRevision",
        ),
        _field(
            "outcome",
            7,
            "TYPE_ENUM",
            type_name=".dennett.control.v1.TestOutcome",
        ),
        _field("stale", 12, "TYPE_BOOL"),
        _field(
            "failure",
            14,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspaceFailure",
        ),
    ),
    ".dennett.control.v1.ArtifactDescriptor": (
        _field("artifact_id", 1, "TYPE_STRING"),
        _field("project_id", 2, "TYPE_STRING"),
        _field("workspace_binding_id", 3, "TYPE_STRING"),
        _field(
            "produced_revision",
            4,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspaceRevision",
        ),
        _field("source_command_id", 5, "TYPE_STRING"),
        _field(
            "state",
            7,
            "TYPE_ENUM",
            type_name=".dennett.control.v1.ArtifactState",
        ),
        _field(
            "path",
            8,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspacePath",
        ),
        _field(
            "content",
            9,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.StableRef",
        ),
    ),
    ".dennett.control.v1.CheckpointDescriptor": (
        _field("checkpoint_id", 1, "TYPE_STRING"),
        _field("project_id", 2, "TYPE_STRING"),
        _field("workspace_binding_id", 3, "TYPE_STRING"),
        _field(
            "base_revision",
            4,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspaceRevision",
        ),
        _field(
            "captured_revision",
            5,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspaceRevision",
        ),
        _field(
            "touched_paths",
            9,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.control.v1.WorkspacePath",
        ),
        _field(
            "external_effects",
            11,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.common.v1.StableRef",
        ),
        _field(
            "provider_continuation",
            12,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.StableRef",
        ),
    ),
    ".dennett.control.v1.ReviewRecord": (
        _field("review_id", 1, "TYPE_STRING"),
        _field("project_id", 2, "TYPE_STRING"),
        _field("workspace_binding_id", 3, "TYPE_STRING"),
        _field(
            "revision",
            4,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspaceRevision",
        ),
        _field(
            "state",
            5,
            "TYPE_ENUM",
            type_name=".dennett.control.v1.ReviewState",
        ),
        _field(
            "comments",
            6,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.control.v1.ReviewComment",
        ),
        _field("test_receipt_ids", 7, "TYPE_STRING", label="LABEL_REPEATED"),
    ),
    ".dennett.control.v1.WorkspaceSnapshot": (
        _field("project_id", 1, "TYPE_STRING"),
        _field(
            "binding",
            2,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspaceBinding",
        ),
        _field(
            "access_policy",
            3,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.ProjectAccessPolicy",
        ),
        _field(
            "workspace_revision",
            4,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspaceRevision",
        ),
        _field("projection_revision", 5, "TYPE_UINT64"),
        _field(
            "changes",
            7,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.control.v1.FileChange",
        ),
        _field(
            "command_receipts",
            9,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.control.v1.CommandReceipt",
        ),
        _field(
            "test_receipts",
            10,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.control.v1.TestReceipt",
        ),
        _field(
            "artifacts",
            11,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.control.v1.ArtifactDescriptor",
        ),
        _field(
            "checkpoints",
            12,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.control.v1.CheckpointDescriptor",
        ),
        _field(
            "review",
            13,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.ReviewRecord",
        ),
    ),
    ".dennett.control.v1.WorkspaceDelta": (
        _field("base_projection_revision", 1, "TYPE_UINT64"),
        _field("new_projection_revision", 2, "TYPE_UINT64"),
        _field(
            "mutations",
            3,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.control.v1.WorkspaceMutation",
        ),
        _field("committed_at", 4, "TYPE_MESSAGE", type_name=".google.protobuf.Timestamp"),
    ),
    ".dennett.control.v1.WorkspaceOperationAccepted": (
        _field(
            "command",
            1,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.CommandAccepted",
        ),
        _field(
            "allocated_refs",
            2,
            "TYPE_MESSAGE",
            label="LABEL_REPEATED",
            type_name=".dennett.common.v1.StableRef",
        ),
    ),
    ".dennett.control.v1.ApplyFileChangesRequest": (
        _field(
            "command",
            1,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.CommandMetadata",
        ),
        _field("project_id", 2, "TYPE_STRING"),
        _field("workspace_binding_id", 3, "TYPE_STRING"),
        _field(
            "base_revision",
            4,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspaceRevision",
        ),
    ),
    ".dennett.control.v1.RunWorkspaceCommandRequest": (
        _field(
            "command",
            1,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.CommandMetadata",
        ),
        _field("project_id", 2, "TYPE_STRING"),
        _field("workspace_binding_id", 3, "TYPE_STRING"),
        _field(
            "base_revision",
            4,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspaceRevision",
        ),
    ),
    ".dennett.control.v1.CancelWorkspaceOperationRequest": (
        _field(
            "command",
            1,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.CommandMetadata",
        ),
        _field("project_id", 2, "TYPE_STRING"),
        _field("workspace_binding_id", 3, "TYPE_STRING"),
        _field("workspace_operation_id", 4, "TYPE_STRING"),
    ),
    ".dennett.control.v1.CancelWorkspaceOperationAccepted": (
        _field(
            "command",
            1,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.CommandAccepted",
        ),
        _field("target_workspace_operation_id", 2, "TYPE_STRING"),
    ),
    ".dennett.control.v1.CreateCheckpointRequest": (
        _field(
            "command",
            1,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.CommandMetadata",
        ),
        _field("project_id", 2, "TYPE_STRING"),
        _field("workspace_binding_id", 3, "TYPE_STRING"),
        _field(
            "base_revision",
            4,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspaceRevision",
        ),
    ),
    ".dennett.control.v1.RestoreCheckpointRequest": (
        _field(
            "command",
            1,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.CommandMetadata",
        ),
        _field("project_id", 2, "TYPE_STRING"),
        _field("workspace_binding_id", 3, "TYPE_STRING"),
        _field("checkpoint_id", 4, "TYPE_STRING"),
        _field(
            "expected_current_revision",
            5,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspaceRevision",
        ),
    ),
    ".dennett.control.v1.SubmitReviewActionRequest": (
        _field(
            "command",
            1,
            "TYPE_MESSAGE",
            type_name=".dennett.common.v1.CommandMetadata",
        ),
        _field("project_id", 2, "TYPE_STRING"),
        _field("workspace_binding_id", 3, "TYPE_STRING"),
        _field(
            "expected_revision",
            5,
            "TYPE_MESSAGE",
            type_name=".dennett.control.v1.WorkspaceRevision",
        ),
        _field(
            "action",
            6,
            "TYPE_ENUM",
            type_name=".dennett.control.v1.ReviewActionKind",
        ),
    ),
}

M02_REQUIRED_ONEOFS: dict[str, dict[str, tuple[FieldContract, ...]]] = {
    ".dennett.control.v1.InspectProjectLocationResponse": {
        "outcome": _workspace_outcome(
            "inspection", ".dennett.control.v1.ProjectLocationInspection"
        )
    },
    ".dennett.control.v1.RegisterProjectResponse": {
        "outcome": _workspace_outcome(
            "accepted", ".dennett.control.v1.RegisterProjectAccepted"
        )
    },
    ".dennett.control.v1.RebindProjectWorkspaceResponse": {
        "outcome": _workspace_outcome(
            "accepted", ".dennett.control.v1.RebindProjectWorkspaceAccepted"
        )
    },
    ".dennett.control.v1.SetProjectTrustResponse": {
        "outcome": _workspace_outcome(
            "accepted", ".dennett.common.v1.CommandAccepted"
        )
    },
    ".dennett.control.v1.WorkspaceOperationReceipt": {
        "terminal": (
            _field(
                "success",
                12,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.WorkspaceOperationSuccess",
                oneof_index=0,
            ),
            _field(
                "failure",
                13,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.WorkspaceFailure",
                oneof_index=0,
            ),
        )
    },
    ".dennett.control.v1.WorkspaceMutation": {
        "mutation": (
            _field(
                "upsert_binding",
                1,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.WorkspaceBinding",
                oneof_index=0,
            ),
            _field(
                "update_access_policy",
                2,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.ProjectAccessPolicy",
                oneof_index=0,
            ),
            _field(
                "update_projection_state",
                3,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.WorkspaceProjectionStateUpdate",
                oneof_index=0,
            ),
            _field(
                "upsert_file_change",
                4,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.FileChange",
                oneof_index=0,
            ),
            _field("remove_file_change_id", 5, "TYPE_STRING", oneof_index=0),
            _field(
                "upsert_operation",
                6,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.WorkspaceOperationReceipt",
                oneof_index=0,
            ),
            _field(
                "upsert_command_receipt",
                7,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.CommandReceipt",
                oneof_index=0,
            ),
            _field(
                "upsert_test_receipt",
                8,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.TestReceipt",
                oneof_index=0,
            ),
            _field(
                "upsert_artifact",
                9,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.ArtifactDescriptor",
                oneof_index=0,
            ),
            _field("remove_artifact_id", 10, "TYPE_STRING", oneof_index=0),
            _field(
                "upsert_checkpoint",
                11,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.CheckpointDescriptor",
                oneof_index=0,
            ),
            _field(
                "upsert_review",
                12,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.ReviewRecord",
                oneof_index=0,
            ),
        )
    },
    ".dennett.control.v1.WorkspaceWatchFrame": {
        "frame": (
            _field(
                "snapshot",
                2,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.WorkspaceSnapshot",
                oneof_index=0,
            ),
            _field(
                "delta",
                3,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.WorkspaceDelta",
                oneof_index=0,
            ),
            _field(
                "heartbeat",
                4,
                "TYPE_MESSAGE",
                type_name=".dennett.sync.v1.WatchHeartbeat",
                oneof_index=0,
            ),
            _field(
                "resync_required",
                5,
                "TYPE_MESSAGE",
                type_name=".dennett.sync.v1.ResyncRequired",
                oneof_index=0,
            ),
            _field(
                "error",
                6,
                "TYPE_MESSAGE",
                type_name=".dennett.control.v1.WorkspaceFailure",
                oneof_index=0,
            ),
        )
    },
    ".dennett.control.v1.GetWorkspaceResponse": {
        "outcome": _workspace_outcome(
            "snapshot", ".dennett.control.v1.WorkspaceSnapshot"
        )
    },
    ".dennett.control.v1.GetWorkspaceDiffResponse": {
        "outcome": _workspace_outcome("diff", ".dennett.control.v1.WorkspaceDiff")
    },
    ".dennett.control.v1.ApplyFileChangesResponse": {
        "outcome": _workspace_outcome(
            "accepted", ".dennett.control.v1.WorkspaceOperationAccepted"
        )
    },
    ".dennett.control.v1.RunWorkspaceCommandResponse": {
        "outcome": _workspace_outcome(
            "accepted", ".dennett.control.v1.WorkspaceOperationAccepted"
        )
    },
    ".dennett.control.v1.CancelWorkspaceOperationResponse": {
        "outcome": _workspace_outcome(
            "accepted", ".dennett.control.v1.CancelWorkspaceOperationAccepted"
        )
    },
    ".dennett.control.v1.CreateCheckpointResponse": {
        "outcome": _workspace_outcome(
            "accepted", ".dennett.control.v1.WorkspaceOperationAccepted"
        )
    },
    ".dennett.control.v1.RestoreCheckpointResponse": {
        "outcome": _workspace_outcome(
            "accepted", ".dennett.control.v1.WorkspaceOperationAccepted"
        )
    },
    ".dennett.control.v1.SubmitReviewActionResponse": {
        "outcome": _workspace_outcome(
            "accepted", ".dennett.control.v1.WorkspaceOperationAccepted"
        )
    },
}

EXPECTED_ONEOFS.update(M02_REQUIRED_ONEOFS)


@dataclass(frozen=True)
class EpochMigration:
    migration_id: str
    previous_epoch: str
    current_epoch: str
    base_module_sha256: str
    candidate_module_sha256: str
    retired_packages: tuple[str, ...]
    introduced_packages: tuple[str, ...]
    retired_symbol_families: tuple[str, ...]
    introduced_symbol_families: tuple[str, ...]
    decision_ref: str
    owner_gate: str


class ProtocolCheckError(RuntimeError):
    """A protocol contract check failed with an actionable message."""


def run(
    command: Sequence[str],
    *,
    cwd: Path = ROOT,
    capture_output: bool = False,
    check: bool = True,
    announce: bool = True,
) -> subprocess.CompletedProcess[bytes]:
    if announce:
        print(f"+ {subprocess.list2cmdline(command)}", flush=True)
    executable = shutil.which(command[0])
    if executable is None:
        raise FileNotFoundError(command[0])
    result = subprocess.run(
        [executable, *command[1:]],
        cwd=cwd,
        check=False,
        capture_output=capture_output,
    )
    if check and result.returncode != 0:
        if capture_output:
            _print_process_output(result)
        raise subprocess.CalledProcessError(
            result.returncode,
            command,
            output=result.stdout,
            stderr=result.stderr,
        )
    return result


def _print_process_output(result: subprocess.CompletedProcess[bytes]) -> None:
    for stream in (result.stdout, result.stderr):
        if stream:
            print(stream.decode("utf-8", errors="replace"), file=sys.stderr, end="")


def proto_files(module: Path = PROTOCOLS) -> list[Path]:
    return sorted((module / "proto").rglob("*.proto"))


def add_do_not_edit_headers(root: Path) -> list[Path]:
    changed: list[Path] = []
    for path in sorted(root.rglob("*")):
        if not path.is_file() or path.suffix not in {".rs", ".ts"}:
            continue
        content = path.read_bytes()
        body = (
            content[len(DO_NOT_EDIT_HEADER) :]
            if content.startswith(DO_NOT_EDIT_HEADER)
            else content
        )
        normalized = DO_NOT_EDIT_HEADER + body.rstrip(b"\r\n") + b"\n"
        if content != normalized:
            path.write_bytes(normalized)
            changed.append(path)
    return changed


def tree_differences(actual: Path, expected: Path, label: str) -> list[str]:
    actual_files = _relative_files(actual)
    expected_files = _relative_files(expected)
    differences: list[str] = []
    for relative in sorted(actual_files.keys() | expected_files.keys()):
        display = f"{label}/{relative.as_posix()}"
        if relative not in actual_files:
            differences.append(f"missing: {display}")
        elif relative not in expected_files:
            differences.append(f"unexpected: {display}")
        elif actual_files[relative] != expected_files[relative]:
            differences.append(f"stale: {display}")
    return differences


def _relative_files(root: Path) -> dict[Path, bytes]:
    if not root.exists():
        return {}
    return {
        path.relative_to(root): path.read_bytes()
        for path in sorted(root.rglob("*"))
        if path.is_file()
    }


def check_approved_buf_configuration() -> None:
    digest = hashlib.sha256((PROTOCOLS / "buf.yaml").read_bytes()).hexdigest()
    if digest != APPROVED_BUF_CONFIG_SHA256:
        raise ProtocolCheckError(
            "protocols/buf.yaml changed without an explicit checker approval; "
            f"expected {APPROVED_BUF_CONFIG_SHA256}, got {digest}"
        )
    print("Approved Buf module configuration is unchanged.")


def snapshot_protocol_module(
    source: Path,
    destination: Path,
    config: str,
) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source / "proto", destination / "proto")
    (destination / "buf.yaml").write_text(config, encoding="utf-8", newline="\n")


def base_ref_candidates(
    explicit: str | None,
    environment: Mapping[str, str] = os.environ,
) -> list[str]:
    if explicit:
        return [explicit]
    github_base = environment.get("GITHUB_BASE_REF", "").strip()
    if github_base:
        return [f"origin/{github_base}", github_base]
    return ["origin/main", "main"]


def resolve_base_ref(explicit: str | None) -> tuple[str, str]:
    for candidate in base_ref_candidates(explicit):
        result = run(
            ["git", "rev-parse", "--verify", "--quiet", f"{candidate}^{{commit}}"],
            capture_output=True,
            check=False,
            announce=False,
        )
        if result.returncode == 0:
            return candidate, result.stdout.decode("ascii").strip()
    attempted = ", ".join(base_ref_candidates(explicit))
    raise ProtocolCheckError(f"cannot resolve protocol comparison base; tried: {attempted}")


def extract_protocol_baseline(ref: str, destination: Path) -> None:
    listing = run(
        [
            "git",
            "ls-tree",
            "-r",
            "--name-only",
            "-z",
            ref,
            "--",
            "protocols/proto",
        ],
        capture_output=True,
        announce=False,
    )
    paths = [
        path
        for path in listing.stdout.decode("utf-8").split("\0")
        if path.endswith(".proto")
    ]
    if not paths:
        raise ProtocolCheckError(f"no canonical .proto files found at {ref}")

    destination.mkdir(parents=True, exist_ok=True)
    (destination / "buf.yaml").write_text(
        COMPARISON_BUF_CONFIG,
        encoding="utf-8",
        newline="\n",
    )
    for repository_path in paths:
        relative = Path(repository_path).relative_to("protocols")
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        content = run(
            ["git", "show", f"{ref}:{repository_path}"],
            capture_output=True,
            announce=False,
        ).stdout
        target.write_bytes(content)


def protocol_module_sha256(module: Path) -> str:
    digest = hashlib.sha256()
    files = sorted((module / "proto").rglob("*.proto"))
    if not files:
        raise ProtocolCheckError(f"no protocol sources found in {module}")
    for path in files:
        relative = path.relative_to(module).as_posix().encode("utf-8")
        content = path.read_bytes()
        digest.update(len(relative).to_bytes(8, "big"))
        digest.update(relative)
        digest.update(len(content).to_bytes(8, "big"))
        digest.update(content)
    return digest.hexdigest()


def protocol_packages(module: Path) -> tuple[str, ...]:
    packages: set[str] = set()
    package_pattern = re.compile(r"^package\s+([a-zA-Z0-9_.]+);$", re.MULTILINE)
    for path in sorted((module / "proto").rglob("*.proto")):
        match = package_pattern.search(path.read_text(encoding="utf-8"))
        if match is None:
            raise ProtocolCheckError(
                f"protocol source has no package declaration: {path.relative_to(module)}"
            )
        packages.add(match.group(1))
    return tuple(sorted(packages))


def protocol_epoch_changed(baseline: Path, candidate: Path) -> bool:
    return protocol_packages(baseline) != protocol_packages(candidate)


def build_descriptor_set(module: Path = PROTOCOLS) -> dict[str, object]:
    with TemporaryDirectory(prefix="dennett-protocol-descriptor-") as directory:
        output = Path(directory) / "descriptor.json"
        run(
            [
                "buf",
                "build",
                str(module),
                "--as-file-descriptor-set",
                "--exclude-source-info",
                "-o",
                str(output),
            ]
        )
        try:
            payload = json.loads(output.read_text(encoding="utf-8"))
        except (OSError, UnicodeError, json.JSONDecodeError) as error:
            raise ProtocolCheckError(f"Buf descriptor set is unreadable: {error}") from error
    if not isinstance(payload, dict):
        raise ProtocolCheckError("Buf descriptor set root is not a mapping")
    return payload


def _descriptor_field_contract(field: Mapping[str, object]) -> FieldContract:
    number = field.get("number")
    oneof_index = field.get("oneofIndex")
    return (
        str(field.get("name", "")),
        number if isinstance(number, int) and not isinstance(number, bool) else -1,
        str(field.get("label", "")),
        str(field.get("type", "")),
        str(field["typeName"]) if isinstance(field.get("typeName"), str) else None,
        (
            oneof_index
            if isinstance(oneof_index, int) and not isinstance(oneof_index, bool)
            else None
        ),
        field.get("proto3Optional") is True,
    )


def m02_initial_descriptor_sha256(
    files: Mapping[str, Mapping[str, object]],
) -> str | None:
    if any(name not in files for name in M02_INITIAL_DESCRIPTOR_FILES):
        return None
    selected = [files[name] for name in M02_INITIAL_DESCRIPTOR_FILES]
    canonical = json.dumps(
        selected,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    return hashlib.sha256(canonical).hexdigest()


def descriptor_contract_differences(payload: Mapping[str, object]) -> list[str]:
    raw_files = payload.get("file")
    if not isinstance(raw_files, list):
        return ["descriptor set has no file list"]

    differences: list[str] = []
    files: dict[str, Mapping[str, object]] = {}
    messages: dict[str, Mapping[str, object]] = {}
    enums: dict[str, Mapping[str, object]] = {}
    services: dict[str, Mapping[str, object]] = {}
    for raw_file in raw_files:
        if not isinstance(raw_file, dict):
            differences.append("descriptor set contains a non-mapping file record")
            continue
        package = raw_file.get("package")
        name = raw_file.get("name")
        if not isinstance(package, str) or not package.startswith("dennett."):
            continue
        if not isinstance(name, str):
            differences.append(f"descriptor package {package} has no source name")
            continue
        files[name] = raw_file
        for raw_message in raw_file.get("messageType", []):
            if not isinstance(raw_message, dict) or not isinstance(
                raw_message.get("name"), str
            ):
                differences.append(f"{name}: malformed message descriptor")
                continue
            messages[f".{package}.{raw_message['name']}"] = raw_message
        for raw_enum in raw_file.get("enumType", []):
            if not isinstance(raw_enum, dict) or not isinstance(
                raw_enum.get("name"), str
            ):
                differences.append(f"{name}: malformed enum descriptor")
                continue
            enums[f".{package}.{raw_enum['name']}"] = raw_enum
        for raw_service in raw_file.get("service", []):
            if not isinstance(raw_service, dict) or not isinstance(
                raw_service.get("name"), str
            ):
                differences.append(f"{name}: malformed service descriptor")
                continue
            services[f".{package}.{raw_service['name']}"] = raw_service

    m02_hash = m02_initial_descriptor_sha256(files)
    if (
        m02_hash is not None
        and m02_hash != APPROVED_M02_INITIAL_DESCRIPTOR_SHA256
    ):
        differences.append(
            "M02 initial descriptor approval hash is "
            f"{m02_hash}, expected {APPROVED_M02_INITIAL_DESCRIPTOR_SHA256}"
        )

    if set(files) != EXPECTED_DESCRIPTOR_FILES:
        differences.append(
            f"Dennett descriptor files are {sorted(files)}, "
            f"expected {sorted(EXPECTED_DESCRIPTOR_FILES)}"
        )
    packages = {str(file.get("package")) for file in files.values()}
    expected_packages = {"dennett.common.v1", "dennett.control.v1", "dennett.sync.v1"}
    if packages != expected_packages:
        differences.append(
            f"Dennett descriptor packages are {sorted(packages)}, "
            f"expected {sorted(expected_packages)}"
        )

    for enum_name, expected in EXPECTED_ENUM_VALUES.items():
        enum = enums.get(enum_name)
        if enum is None:
            differences.append(f"missing critical enum {enum_name}")
            continue
        raw_values = enum.get("value", [])
        if not isinstance(raw_values, list):
            differences.append(f"{enum_name} has no value list")
            continue
        actual = tuple(
            (
                str(value.get("name", "")),
                value.get("number")
                if isinstance(value.get("number"), int)
                and not isinstance(value.get("number"), bool)
                else -1,
            )
            for value in raw_values
            if isinstance(value, dict)
        )
        if actual != expected:
            differences.append(f"{enum_name} values are {actual}, expected {expected}")

    if set(services) != set(EXPECTED_SERVICE_METHODS):
        differences.append(
            f"services are {sorted(services)}, expected {sorted(EXPECTED_SERVICE_METHODS)}"
        )
    for service_name, expected in EXPECTED_SERVICE_METHODS.items():
        service = services.get(service_name)
        if service is None:
            continue
        raw_methods = service.get("method", [])
        if not isinstance(raw_methods, list):
            differences.append(f"{service_name} has no method list")
            continue
        actual: tuple[MethodContract, ...] = tuple(
            (
                str(method.get("name", "")),
                str(method.get("inputType", "")),
                str(method.get("outputType", "")),
                method.get("clientStreaming") is True,
                method.get("serverStreaming") is True,
            )
            for method in raw_methods
            if isinstance(method, dict)
        )
        if actual != expected:
            differences.append(f"{service_name} methods are {actual}, expected {expected}")

    for message_name, expected in EXPECTED_MESSAGE_FIELDS.items():
        message = messages.get(message_name)
        if message is None:
            differences.append(f"missing critical message {message_name}")
            continue
        raw_fields = message.get("field", [])
        if not isinstance(raw_fields, list):
            differences.append(f"{message_name} has no field list")
            continue
        actual = tuple(
            _descriptor_field_contract(field)
            for field in raw_fields
            if isinstance(field, dict)
        )
        if actual != expected:
            differences.append(f"{message_name} fields are {actual}, expected {expected}")

    for message_name, required in M02_REQUIRED_MESSAGE_FIELDS.items():
        message = messages.get(message_name)
        if message is None:
            differences.append(f"missing M02 contract message {message_name}")
            continue
        raw_fields = message.get("field", [])
        if not isinstance(raw_fields, list):
            differences.append(f"{message_name} has no field list")
            continue
        actual = {
            _descriptor_field_contract(field)
            for field in raw_fields
            if isinstance(field, dict)
        }
        missing = tuple(field for field in required if field not in actual)
        if missing:
            differences.append(f"{message_name} is missing required M02 fields {missing}")

    for message_name, expected_oneofs in EXPECTED_ONEOFS.items():
        message = messages.get(message_name)
        if message is None:
            differences.append(f"missing oneof-bearing message {message_name}")
            continue
        raw_oneofs = message.get("oneofDecl", [])
        raw_fields = message.get("field", [])
        if not isinstance(raw_oneofs, list) or not isinstance(raw_fields, list):
            differences.append(f"{message_name} has malformed oneof descriptors")
            continue
        oneof_names = [
            oneof.get("name") if isinstance(oneof, dict) else None
            for oneof in raw_oneofs
        ]
        for oneof_name, expected_fields in expected_oneofs.items():
            if oneof_name not in oneof_names:
                differences.append(f"{message_name} is missing oneof {oneof_name}")
                continue
            oneof_index = oneof_names.index(oneof_name)
            actual_fields = tuple(
                _descriptor_field_contract(field)
                for field in raw_fields
                if isinstance(field, dict) and field.get("oneofIndex") == oneof_index
            )
            if actual_fields != expected_fields:
                differences.append(
                    f"{message_name}.{oneof_name} fields are {actual_fields}, "
                    f"expected {expected_fields}"
                )
        if message_name.endswith(("Response", "Mutation")):
            expected_all = tuple(
                field
                for expected_fields in expected_oneofs.values()
                for field in expected_fields
            )
            actual_all = tuple(
                _descriptor_field_contract(field)
                for field in raw_fields
                if isinstance(field, dict)
            )
            if actual_all != expected_all:
                differences.append(
                    f"{message_name} complete fields are {actual_all}, "
                    f"expected {expected_all}"
                )

    for message_name in (
        ".dennett.control.v1.SessionWatchFrame",
        ".dennett.control.v1.SystemWatchFrame",
    ):
        message = messages.get(message_name)
        if message is None:
            continue
        raw_fields = message.get("field", [])
        actual_fields = tuple(
            _descriptor_field_contract(field)
            for field in raw_fields
            if isinstance(field, dict)
        )
        expected_fields = (
            _field(
                "cursor",
                1,
                "TYPE_MESSAGE",
                type_name=".dennett.sync.v1.WatchCursor",
            ),
            *EXPECTED_ONEOFS[message_name]["frame"],
        )
        if actual_fields != expected_fields:
            differences.append(
                f"{message_name} complete fields are {actual_fields}, "
                f"expected {expected_fields}"
            )

    for message_name, message in messages.items():
        raw_fields = message.get("field", [])
        if not isinstance(raw_fields, list):
            continue
        for field in raw_fields:
            if not isinstance(field, dict):
                continue
            if field.get("typeName") == ".google.protobuf.Any":
                differences.append(f"{message_name} uses forbidden google.protobuf.Any")
            if field.get("name") == "payload" and field.get("type") == "TYPE_BYTES":
                differences.append(f"{message_name} has a forbidden generic bytes payload")
    return differences


def check_descriptor_contract() -> None:
    differences = descriptor_contract_differences(build_descriptor_set())
    if differences:
        details = "\n".join(f"- {difference}" for difference in differences)
        raise ProtocolCheckError(f"accepted descriptor contract differs:\n{details}")
    print(
        "Accepted M01 surface and required M02 workspace contracts match "
        "services, fields, oneofs and streams."
    )


def _manifest_string_list(payload: object, field: str) -> tuple[str, ...]:
    if not isinstance(payload, list) or not payload:
        raise ProtocolCheckError(f"epoch migration field {field} must be a non-empty list")
    if not all(isinstance(item, str) and item for item in payload):
        raise ProtocolCheckError(f"epoch migration field {field} contains an invalid value")
    values = tuple(payload)
    if len(values) != len(set(values)):
        raise ProtocolCheckError(f"epoch migration field {field} contains duplicates")
    return values


def load_epoch_migration(path: Path = EPOCH_MIGRATION_MANIFEST) -> EpochMigration:
    if not path.is_file():
        raise ProtocolCheckError(f"protocol epoch migration manifest is missing: {path}")
    try:
        raw_manifest = path.read_bytes()
    except OSError as error:
        raise ProtocolCheckError(
            f"protocol epoch migration manifest is unreadable: {error}"
        ) from error
    digest = hashlib.sha256(raw_manifest).hexdigest()
    if digest != APPROVED_EPOCH_MIGRATION_SHA256:
        raise ProtocolCheckError(
            "protocol epoch migration manifest changed without checker approval; "
            f"expected {APPROVED_EPOCH_MIGRATION_SHA256}, got {digest}"
        )
    try:
        payload = json.loads(raw_manifest.decode("utf-8"))
    except (UnicodeError, json.JSONDecodeError) as error:
        raise ProtocolCheckError(
            f"protocol epoch migration manifest is unreadable: {error}"
        ) from error
    expected_fields = {
        "version",
        "migration_id",
        "previous_epoch",
        "current_epoch",
        "base_module_sha256",
        "candidate_module_sha256",
        "retired_packages",
        "introduced_packages",
        "retired_symbol_families",
        "introduced_symbol_families",
        "decision_ref",
        "owner_gate",
    }
    if not isinstance(payload, dict) or set(payload) != expected_fields:
        raise ProtocolCheckError("epoch migration manifest fields are not canonical")
    if payload["version"] != 1:
        raise ProtocolCheckError("unsupported protocol epoch migration manifest version")
    for field in (
        "migration_id",
        "previous_epoch",
        "current_epoch",
        "decision_ref",
        "owner_gate",
    ):
        if not isinstance(payload[field], str) or not payload[field]:
            raise ProtocolCheckError(f"epoch migration field {field} must be non-empty")
    for field in ("base_module_sha256", "candidate_module_sha256"):
        value = payload[field]
        if not isinstance(value, str) or re.fullmatch(r"[0-9a-f]{64}", value) is None:
            raise ProtocolCheckError(f"epoch migration field {field} is not a SHA-256")
    decision_root = (ROOT / "docs" / "decisions").resolve()
    decision_path = (ROOT / str(payload["decision_ref"])).resolve()
    if not decision_path.is_relative_to(decision_root):
        raise ProtocolCheckError(
            "epoch migration decision_ref must stay under docs/decisions"
        )
    if not decision_path.is_file():
        raise ProtocolCheckError(
            f"epoch migration decision does not exist: {payload['decision_ref']}"
        )
    return EpochMigration(
        migration_id=str(payload["migration_id"]),
        previous_epoch=str(payload["previous_epoch"]),
        current_epoch=str(payload["current_epoch"]),
        base_module_sha256=str(payload["base_module_sha256"]),
        candidate_module_sha256=str(payload["candidate_module_sha256"]),
        retired_packages=_manifest_string_list(
            payload["retired_packages"], "retired_packages"
        ),
        introduced_packages=_manifest_string_list(
            payload["introduced_packages"], "introduced_packages"
        ),
        retired_symbol_families=_manifest_string_list(
            payload["retired_symbol_families"], "retired_symbol_families"
        ),
        introduced_symbol_families=_manifest_string_list(
            payload["introduced_symbol_families"], "introduced_symbol_families"
        ),
        decision_ref=str(payload["decision_ref"]),
        owner_gate=str(payload["owner_gate"]),
    )


def epoch_migration_differences(
    baseline: Path,
    candidate: Path,
    migration: EpochMigration,
) -> list[str]:
    differences: list[str] = []
    actual_base_hash = protocol_module_sha256(baseline)
    actual_candidate_hash = protocol_module_sha256(candidate)
    base_packages = set(protocol_packages(baseline))
    candidate_packages = set(protocol_packages(candidate))
    actual_retired = tuple(sorted(base_packages - candidate_packages))
    actual_introduced = tuple(sorted(candidate_packages - base_packages))

    if migration.previous_epoch == migration.current_epoch:
        differences.append("previous and current epochs are identical")
    if actual_base_hash != migration.base_module_sha256:
        differences.append(
            f"base module hash is {actual_base_hash}, expected {migration.base_module_sha256}"
        )
    if actual_candidate_hash != migration.candidate_module_sha256:
        differences.append(
            "candidate module hash is "
            f"{actual_candidate_hash}, expected {migration.candidate_module_sha256}"
        )
    if actual_retired != tuple(sorted(migration.retired_packages)):
        differences.append(
            f"retired packages are {actual_retired}, expected {migration.retired_packages}"
        )
    if actual_introduced != tuple(sorted(migration.introduced_packages)):
        differences.append(
            "introduced packages are "
            f"{actual_introduced}, expected {migration.introduced_packages}"
        )
    return differences


def _normalise_lint_path(raw_path: str) -> str:
    path = raw_path.replace("\\", "/")
    if path.startswith("proto/"):
        return path
    marker = "/proto/"
    if marker in path:
        return f"proto/{path.split(marker, 1)[1]}"
    raise ProtocolCheckError(f"strict Buf lint returned an unexpected path: {raw_path}")


def parse_lint_violations(
    result: subprocess.CompletedProcess[bytes],
) -> frozenset[LintViolation]:
    violations: set[LintViolation] = set()
    output = b"\n".join(stream for stream in (result.stdout, result.stderr) if stream)
    for line in output.decode("utf-8", errors="replace").splitlines():
        if not line.strip():
            continue
        try:
            payload = json.loads(line)
            violations.add(
                (
                    _normalise_lint_path(str(payload["path"])),
                    str(payload["type"]),
                    str(payload["message"]),
                )
            )
        except (json.JSONDecodeError, KeyError, TypeError) as error:
            raise ProtocolCheckError(
                f"strict Buf lint returned non-violation output: {line}"
            ) from error
    if result.returncode != 0 and not violations:
        raise ProtocolCheckError("strict Buf lint failed without structured violations")
    return frozenset(violations)


def check_strict_standard_lint() -> None:
    run(["buf", "lint", str(PROTOCOLS)])
    print("Buf STANDARD lint passed with no ignores or grandfathered findings.")


def check_negative_lint_probe() -> None:
    with TemporaryDirectory(prefix="dennett-protocol-lint-probe-") as directory:
        candidate = Path(directory) / "protocols"
        shutil.copytree(PROTOCOLS, candidate)
        target = (
            candidate
            / "proto"
            / "dennett"
            / "control"
            / "v1"
            / "session.proto"
        )
        with target.open("a", encoding="utf-8", newline="\n") as stream:
            stream.write(
                "\nmessage ProbeRequest {}\n"
                "message ProbeResponse {}\n"
                "service NewLegacy {\n"
                "  rpc Probe(ProbeRequest) returns (ProbeResponse);\n"
                "}\n"
            )
        run(["buf", "build", str(candidate)])
        result = run(
            ["buf", "lint", str(candidate), "--error-format", "json"],
            capture_output=True,
            check=False,
        )
        violations = parse_lint_violations(result)
        expected = (
            "proto/dennett/control/v1/session.proto",
            "SERVICE_SUFFIX",
        )
        if result.returncode == 0 or not any(
            (path, rule) == expected for path, rule, _message in violations
        ):
            raise ProtocolCheckError(
                "strict lint negative probe accepted a service without the required suffix"
            )
    print("Negative lint probe rejected a newly introduced STANDARD violation.")


def generate_into(cwd: Path) -> Path:
    run(
        [
            "buf",
            "generate",
            str(PROTOCOLS),
            "--template",
            str(GENERATOR_TEMPLATE),
        ],
        cwd=cwd,
    )
    output = cwd / "generated"
    add_do_not_edit_headers(output)
    return output


def generate() -> None:
    generate_into(ROOT)
    files = sum(len(_relative_files(GENERATED / language)) for language in GENERATED_LANGUAGES)
    print(f"Generated {files} committed protocol client artifacts.")


def check_format() -> None:
    stale: list[str] = []
    files = proto_files()
    if not files:
        raise ProtocolCheckError("no canonical Protobuf sources found under protocols/proto")
    for path in files:
        formatted = run(
            ["buf", "format", str(path.relative_to(ROOT))],
            capture_output=True,
            announce=False,
        ).stdout
        if formatted != path.read_bytes():
            stale.append(path.relative_to(ROOT).as_posix())
    if stale:
        details = "\n".join(f"- {path}" for path in stale)
        raise ProtocolCheckError(f"stale protocol formatting:\n{details}")
    print(f"Buf format check passed ({len(files)} files).")


def check_generated() -> None:
    with TemporaryDirectory(prefix="dennett-protocol-generation-") as directory:
        expected = generate_into(Path(directory))
        differences: list[str] = []
        for language in GENERATED_LANGUAGES:
            differences.extend(
                tree_differences(
                    GENERATED / language,
                    expected / language,
                    f"generated/{language}",
                )
            )
    if differences:
        details = "\n".join(f"- {difference}" for difference in differences)
        raise ProtocolCheckError(f"generated protocol artifacts are not current:\n{details}")
    print("Committed Rust and TypeScript protocol artifacts are current.")


def check_against_main(explicit_base_ref: str | None) -> None:
    base_ref, commit = resolve_base_ref(explicit_base_ref)
    migration_used: EpochMigration | None = None
    with TemporaryDirectory(prefix="dennett-protocol-baseline-") as directory:
        root = Path(directory)
        baseline = root / "baseline"
        candidate = root / "candidate"
        extract_protocol_baseline(base_ref, baseline)
        snapshot_protocol_module(PROTOCOLS, candidate, COMPARISON_BUF_CONFIG)
        result = run(
            ["buf", "breaking", str(candidate), "--against", str(baseline)],
            capture_output=True,
            check=False,
        )
        package_epoch_changed = protocol_epoch_changed(baseline, candidate)
        if package_epoch_changed:
            migration = load_epoch_migration()
            differences = epoch_migration_differences(baseline, candidate, migration)
            if differences:
                _print_process_output(result)
                details = "\n".join(f"- {difference}" for difference in differences)
                raise ProtocolCheckError(
                    "breaking protocol change does not match the approved epoch "
                    f"migration {migration.migration_id}:\n{details}"
                )
            migration_used = migration
        elif result.returncode != 0:
            _print_process_output(result)
            raise ProtocolCheckError(
                f"protocol compatibility failed against {base_ref} ({commit[:12]})"
            )
    if migration_used is None:
        print(f"Protocol compatibility passed against {base_ref} ({commit[:12]}).")
    else:
        print(
            f"Protocol epoch migration {migration_used.migration_id} exactly matches "
            f"{base_ref} ({commit[:12]}); owner gate {migration_used.owner_gate} remains."
        )


def check_negative_breaking_probe() -> None:
    with TemporaryDirectory(prefix="dennett-protocol-breaking-") as directory:
        root = Path(directory)
        baseline = root / "baseline"
        candidate = root / "candidate"
        snapshot_protocol_module(PROTOCOLS, baseline, COMPARISON_BUF_CONFIG)
        snapshot_protocol_module(PROTOCOLS, candidate, COMPARISON_BUF_CONFIG)
        target = (
            candidate
            / "proto"
            / "dennett"
            / "common"
            / "v1"
            / "common.proto"
        )
        content = target.read_text(encoding="utf-8")
        original = "string kind = 1;"
        incompatible = "int64 kind = 1;"
        if content.count(original) != 1:
            raise ProtocolCheckError(
                "negative compatibility fixture cannot find StableRef.kind field"
            )
        target.write_text(
            content.replace(original, incompatible, 1),
            encoding="utf-8",
            newline="\n",
        )
        run(["buf", "build", str(candidate)])
        result = run(
            ["buf", "breaking", str(candidate), "--against", str(baseline)],
            capture_output=True,
            check=False,
        )
        if result.returncode == 0:
            raise ProtocolCheckError(
                "Buf accepted an incompatible field-type reuse in the negative probe"
            )
    print("Negative compatibility probe rejected incompatible field-number reuse.")


def check(explicit_base_ref: str | None) -> None:
    check_approved_buf_configuration()
    check_strict_standard_lint()
    check_descriptor_contract()
    check_negative_lint_probe()
    check_format()
    check_generated()
    check_against_main(explicit_base_ref)
    check_negative_breaking_probe()
    print("Protocol contract checks passed.")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    subcommands = parser.add_subparsers(dest="command", required=True)
    subcommands.add_parser("generate", help="regenerate committed clients")
    check_parser = subcommands.add_parser("check", help="run protocol contract gates")
    check_parser.add_argument(
        "--base-ref",
        help="Git ref to compare against (defaults to the PR base or main)",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.command == "generate":
        generate()
    else:
        check(args.base_ref)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ProtocolCheckError as error:
        print(f"protocol check failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
    except FileNotFoundError as error:
        missing = error.filename or (error.args[0] if error.args else "unknown")
        print(f"required protocol tool is missing: {missing}", file=sys.stderr)
        raise SystemExit(1) from error
    except subprocess.CalledProcessError as error:
        print(
            f"protocol command failed with exit code {error.returncode}: "
            f"{subprocess.list2cmdline(error.cmd)}",
            file=sys.stderr,
        )
        raise SystemExit(error.returncode) from error
