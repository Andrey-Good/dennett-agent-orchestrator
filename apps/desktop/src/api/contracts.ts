export type ProfileMode = "production" | "test_isolated";
export type RuntimeAuthProfile = "oauth-personal" | "gemini-api-key" | "local-endpoint";
export type ServiceKind = "memoryd" | "head" | "node";
export type MemoryStatus = "healthy" | "degraded_unreachable";
export type MemoryOutboxStatus = "pending" | "delivered" | "failed";
export type ThinkingLevel = "low" | "medium" | "high" | "xhigh";
export type SessionStatus = "active" | "archived" | "closed";
export type SessionKind = "user_chat" | "task_run";
export type MessageRole = "user" | "assistant" | "system";
export type RunStatus =
  | "running"
  | "succeeded"
  | "cancelled"
  | "failed"
  | "degraded"
  | "superseded"
  | "unknown_external";
export type PolicyEffect = "allow" | "deny";
export type WorkspaceFileKind = "file" | "directory";

// ============================================================================
// Phase D2 Lifecycle, Durable Tasks, Approvals, Inbox & External Effects Enums
// ============================================================================

export type ProjectLifecycleState = "active" | "archived" | "detached" | "deleted_record";
export type ArtifactCompleteness = "partial" | "final";
export type ArtifactLifecycleState = "active" | "archived" | "removed" | "deleted";
export type ArtifactLifecycleAction = "finalize" | "archive" | "unarchive" | "remove" | "delete";

export type TaskStatus =
  | "queued"
  | "running"
  | "waiting_timer"
  | "waiting_signal"
  | "waiting_clarification"
  | "waiting_approval"
  | "succeeded"
  | "failed"
  | "cancelled"
  | "unknown_external";

export type ScheduleKind =
  | { kind: "once_at"; fire_at: number }
  | { kind: "interval"; interval_secs: number }
  | { kind: "daily_wall_time"; hour: number; minute: number }
  | { kind: "weekly_wall_time"; weekdays_mask: number; hour: number; minute: number };

export type MissedRunPolicy = "execute_once" | "skip" | "ask";

export interface TaskSchedule {
  timezone: string;
  original_expression?: string | null;
  kind: ScheduleKind;
  missed_run_policy?: MissedRunPolicy;
  next_fire_at?: number | null;
}

export type BudgetExceedPolicy = "ask_approval" | "fail";

export interface TaskBudget {
  max_turns?: number | null;
  max_tokens?: number | null;
  max_wall_secs?: number | null;
  exceed_policy?: BudgetExceedPolicy;
}

export type UsageConfidence = "provider_reported" | "estimated" | "unknown";

export interface TaskUsage {
  turns_used: number;
  tokens_used: number;
  wall_ms_used: number;
  confidence: UsageConfidence;
}

export type TaskTimerStatus = "pending" | "claimed" | "fired" | "paused" | "skipped" | "cancelled";
export type TimerPauseReason = "archived_project" | "misfire_ask";

export interface TaskTimerRecord {
  timer_id: string;
  task_id: string;
  task_generation: number;
  idempotency_key: string;
  fire_at: number;
  timezone: string;
  original_expression?: string | null;
  recurrence_kind: string;
  missed_run_policy: MissedRunPolicy;
  status: TaskTimerStatus;
  pause_reason?: TimerPauseReason | null;
  lease_owner?: string | null;
  lease_generation: number;
  lease_expires_at?: number | null;
  created_at: number;
  fired_at?: number | null;
}

export type TaskSignalOutcome = "woke_task" | "queued_on_task" | "rejected_stopped";

export interface TaskSignalRecord {
  signal_id: string;
  task_id: string;
  idempotency_key: string;
  signal_name: string;
  payload_text: string;
  payload_sha256: string;
  applied_generation?: number | null;
  outcome_status: TaskSignalOutcome;
  correlation_id: string;
  created_at: number;
}

export type PermissionRequestStatus =
  | "pending"
  | "approved_once"
  | "approved_scope"
  | "denied"
  | "expired";

export type PermissionResolutionDecision =
  | "approve_once"
  | "approve_project"
  | "approve_task"
  | "deny";

export interface PermissionRequestRecord {
  request_id: string;
  project_id: string;
  session_id: string;
  run_id: string;
  task_id?: string | null;
  agent_id: string;
  tool_call_id: string;
  tool_name: string;
  tool_kind: string;
  target_summary: string;
  normalized_targets: string[];
  parameters_sha256: string;
  preview_snippet?: string | null;
  reason: string;
  status: PermissionRequestStatus;
  issued_grant_id?: string | null;
  decision_id?: string | null;
  revision: number;
  expires_at: number;
  resolved_at?: number | null;
  correlation_id: string;
  created_at: number;
}

export interface CapabilityGrantRecord {
  grant_id: string;
  scope: string;
  project_id: string;
  agent_id?: string | null;
  session_id?: string | null;
  task_id?: string | null;
  tool_pattern: string;
  target_prefixes: string[];
  max_uses?: number | null;
  uses_consumed: number;
  reason: string;
  expires_at?: number | null;
  revoked_at?: number | null;
  revoked_reason?: string | null;
  created_at: number;
}

export type InboxCardKind =
  | "permission_approval"
  | "clarification"
  | "unknown_effect"
  | "budget_exceeded"
  | "missed_schedule";

export type InboxCardStatus = "open" | "resolved" | "expired" | "superseded" | "withdrawn";
export type InboxCardUrgency = "low" | "normal" | "high" | "critical";

export interface InboxCardRecord {
  card_id: string;
  project_id: string;
  session_id?: string | null;
  task_id?: string | null;
  run_id?: string | null;
  agent_id?: string | null;
  kind: InboxCardKind;
  status: InboxCardStatus;
  urgency: InboxCardUrgency;
  subject: string;
  detail: string;
  permission_request_id?: string | null;
  effect_id?: string | null;
  resolution_summary?: string | null;
  revision: number;
  created_at: number;
  updated_at: number;
  resolved_at?: number | null;
}

export type ExternalEffectStatus =
  | "prepared"
  | "dispatching"
  | "confirmed"
  | "denied"
  | "failed"
  | "unknown";

export type SimEffectBehavior = "success" | "denied" | "failed_before_accept" | "lost_ack";
export type ManualEffectResolution = "mark_confirmed" | "mark_not_executed";

export interface EffectClaimRecord {
  effect_id: string;
  project_id: string;
  session_id: string;
  task_id?: string | null;
  run_id?: string | null;
  agent_id: string;
  effect_idempotency_key: string;
  connector_id: string;
  operation: string;
  target_recipient: string;
  parameters_json: string;
  parameters_sha256: string;
  status: ExternalEffectStatus;
  provider_operation_id?: string | null;
  receipt_json?: string | null;
  error_summary?: string | null;
  reconciled_at?: number | null;
  correlation_id: string;
  created_at: number;
  updated_at: number;
}

export interface AgentMessageRecord {
  agent_message_id: string;
  sender_agent_id?: string | null;
  recipient_agent_id: string;
  project_id: string;
  related_task_id?: string | null;
  idempotency_key: string;
  body_text: string;
  delivered_in_run_id?: string | null;
  correlation_id: string;
  created_at: number;
  delivered_at?: number | null;
}

export interface StreamCursor {
  authority_epoch: number;
  committed_seq: number;
  retained_from_seq: number;
}

export interface ControlEventRecord {
  cursor_seq: number;
  event_id: string;
  authority_epoch: number;
  entity_key: string;
  entity_seq: number;
  event_type: string;
  project_id?: string | null;
  session_id?: string | null;
  task_id?: string | null;
  correlation_id: string;
  payload_json: string;
  created_at: number;
}

export interface TaskRecord {
  task_id: string;
  project_id: string;
  session_id: string;
  agent_id: string;
  parent_task_id?: string | null;
  title: string;
  goal: string;
  generation: number;
  status: TaskStatus;
  wait_detail?: string | null;
  expected_signal_name?: string | null;
  partial_result_summary?: string | null;
  error_code?: string | null;
  error_summary?: string | null;
  active_run_id?: string | null;
  schedule?: TaskSchedule | null;
  budget: TaskBudget;
  usage: TaskUsage;
  attached_skills: string[];
  auth_profile_override?: RuntimeAuthProfile | null;
  created_at: number;
  updated_at: number;
  finished_at?: number | null;
}

export type WorkspaceWritePolicy =
  | { mode: "deny_all" }
  | { mode: "ask_before_write" }
  | { mode: "allow_workspace" }
  | { mode: "allow_paths"; allowed_prefixes: string[] };

export interface RuntimeThinkingOption {
  value: ThinkingLevel;
  label: string;
}

export interface RuntimeModelOption {
  model_id: string;
  label: string;
  supported_thinking_levels: ThinkingLevel[];
  default_thinking_level?: ThinkingLevel | null;
}

export interface RuntimeQuotaWindow {
  label: string;
  remaining_percent: number;
}

export interface RuntimeCatalogResponse {
  adapter_id: string;
  auth_profile: RuntimeAuthProfile;
  is_simulated: boolean;
  agy_version?: string | null;
  tier?: string | null;
  account_label?: string | null;
  default_model_id?: string | null;
  default_thinking_level?: ThinkingLevel | null;
  models: RuntimeModelOption[];
  thinking_options: RuntimeThinkingOption[];
  quotas: RuntimeQuotaWindow[];
  connection_id?: string | null;
  quota_error?: ProviderQuotaError | null;
  probed_at: number;
}

export interface AgentConfigVersion {
  config_version_id: string;
  agent_id: string;
  version_number: number;
  system_prompt: string;
  model_preference?: string | null;
  thinking_level?: ThinkingLevel | null;
  allowed_memory_scopes: string[];
  workspace_write_policy: WorkspaceWritePolicy;
  default_budget?: TaskBudget | null;
  attached_skills?: string[];
  preferred_connection_id?: string | null;
  created_at: number;
}

export interface AgentRecord {
  agent_id: string;
  name: string;
  description: string;
  current_config_version_id: string;
  current_config: AgentConfigVersion;
  created_at: number;
  updated_at: number;
}

export interface ProjectRecord {
  project_id: string;
  name: string;
  workspace_path: string | null;
  memory_scope: string;
  lifecycle_state?: ProjectLifecycleState;
  archived_at?: number | null;
  pinned: boolean;
  pinned_at?: number | null;
  created_at: number;
}

export interface RuntimeBinding {
  binding_id: string;
  session_id: string;
  adapter_id: string;
  auth_profile: RuntimeAuthProfile;
  tier?: string | null;
  agy_version?: string | null;
  provider_session_id?: string | null;
  workspace_cwd: string;
  is_simulated: boolean;
  rehydrate_count: number;
  connection_id?: string | null;
  endpoint_fingerprint?: string | null;
  created_at: number;
  updated_at: number;
}

export interface SessionRecord {
  session_id: string;
  agent_id: string;
  project_id: string | null;
  title: string;
  session_kind?: SessionKind;
  runtime_binding?: RuntimeBinding | null;
  status: SessionStatus;
  pinned: boolean;
  pinned_at?: number | null;
  created_at: number;
  updated_at: number;
}

export interface MessageRecord {
  message_id: string;
  session_id: string;
  role: MessageRole;
  content: string;
  run_id?: string | null;
  client_message_id?: string | null;
  correlation_id: string;
  created_at: number;
}

export interface RunRecord {
  run_id: string;
  session_id: string;
  agent_id: string;
  agent_config_version_id: string;
  user_message_id: string;
  assistant_message_id?: string | null;
  status: RunStatus;
  stop_reason?: string | null;
  error_code?: string | null;
  error_summary?: string | null;
  rehydrated: boolean;
  task_id?: string | null;
  task_generation?: number | null;
  tokens_used?: number | null;
  wall_ms?: number | null;
  correlation_id: string;
  created_at: number;
  finished_at?: number | null;
}

export interface ArtifactRecord {
  artifact_id: string;
  session_id: string;
  project_id: string | null;
  run_id?: string | null;
  name: string;
  mime_type: string;
  revision: number;
  content?: string | null;
  relative_path?: string | null;
  sha256: string;
  completeness_status?: ArtifactCompleteness;
  lifecycle_state?: ArtifactLifecycleState;
  created_at: number;
}

export interface PermissionDecisionRecord {
  decision_id: string;
  session_id: string;
  run_id?: string | null;
  tool_call_id: string;
  tool_name: string;
  target_pattern?: string | null;
  scope: string;
  effect: PolicyEffect;
  matched_rule_id?: string | null;
  reason: string;
  evaluated_outside_model: boolean;
  correlation_id: string;
  created_at: number;
}

export interface MemoryOutboxItem {
  outbox_id: string;
  session_id?: string | null;
  run_id?: string | null;
  idempotency_key: string;
  scope: string;
  operation: string;
  payload_json: string;
  status: MemoryOutboxStatus;
  attempts: number;
  last_error?: string | null;
  delivered_event_id?: string | null;
  delivered_note_id?: string | null;
  correlation_id: string;
  created_at: number;
  updated_at: number;
}

export interface DiagnosticEvent {
  event_id: string;
  correlation_id: string;
  session_id?: string | null;
  run_id?: string | null;
  component: string;
  event_kind: string;
  safe_summary: string;
  created_at: number;
}

export interface HeadHealth {
  service: ServiceKind;
  stage: string;
  profile_mode: ProfileMode;
  postgres_connected: boolean;
  postgres_role?: string | null;
  postgres_database?: string | null;
  schema_version?: number | null;
  memory_status: MemoryStatus;
  memory_detail?: string | null;
  runtime_adapter_id: string;
  runtime_is_simulated: boolean;
  pending_outbox_count: number;
  capabilities: string[];
  correlation_id: string;
}

export interface DesktopHealthSnapshot {
  head_url: string;
  connected: boolean;
  health?: HeadHealth | null;
  error_code?: string | null;
  error?: string | null;
}

export interface ListProjectsResponse {
  projects: ProjectRecord[];
}

export interface CreateProjectRequest {
  name: string;
  workspace_path?: string | null;
  memory_scope?: string | null;
}

export interface UpdateProjectRequest {
  name?: string | null;
  workspace_path?: string | null;
  pinned?: boolean;
}

export interface RebindProjectRequest {
  workspace_path: string;
  reason?: string | null;
}

export interface ProjectLifecycleResponse {
  project: ProjectRecord;
  affected_timers: number;
  user_workspace_files_deleted: boolean;
}

export interface WorkspaceFileEntry {
  relative_path: string;
  name: string;
  kind: WorkspaceFileKind;
  size_bytes?: number | null;
  depth: number;
}

export interface ListProjectFilesResponse {
  project_id: string;
  workspace_bound: boolean;
  workspace_path: string | null;
  entries: WorkspaceFileEntry[];
  truncated: boolean;
}

export interface ListAgentsResponse {
  agents: AgentRecord[];
}

export interface CreateAgentRequest {
  name: string;
  description: string;
  system_prompt: string;
  model_preference?: string | null;
  thinking_level?: ThinkingLevel | null;
  allowed_memory_scopes: string[];
  workspace_write_policy: WorkspaceWritePolicy;
  default_budget?: TaskBudget | null;
  attached_skills?: string[];
  preferred_connection_id?: string | null;
}

export interface UpdateAgentConfigRequest {
  system_prompt: string;
  model_preference?: string | null;
  thinking_level?: ThinkingLevel | null;
  allowed_memory_scopes: string[];
  workspace_write_policy: WorkspaceWritePolicy;
  default_budget?: TaskBudget | null;
  attached_skills?: string[];
  preferred_connection_id?: string | null;
}

export interface ListSessionsResponse {
  sessions: SessionRecord[];
}

export interface CreateSessionRequest {
  agent_id: string;
  project_id?: string | null;
  title: string;
  session_kind?: SessionKind;
}

export interface UpdateSessionRequest {
  title?: string;
  pinned?: boolean;
}

export interface SessionDetailResponse {
  session: SessionRecord;
  messages: MessageRecord[];
  artifacts: ArtifactRecord[];
  runs: RunRecord[];
  permission_decisions: PermissionDecisionRecord[];
}

export interface SendMessageRequest {
  content: string;
  timeout_ms?: number | null;
  client_message_id?: string | null;
}

export interface SendMessageResponse {
  user_message: MessageRecord;
  assistant_message?: MessageRecord | null;
  run: RunRecord;
  runtime_binding: RuntimeBinding;
  artifacts: ArtifactRecord[];
  permission_decisions: PermissionDecisionRecord[];
  memory_status: MemoryStatus;
  session_title?: string;
  deduplicated?: boolean;
}

export interface CancelRunResponse {
  session_id: string;
  run_id?: string | null;
  cancelled: boolean;
  status?: RunStatus | null;
  memory_status: MemoryStatus;
  external_outcome_unknown?: boolean;
  committed_effects?: EffectClaimRecord[];
  unknown_effects?: EffectClaimRecord[];
}

export interface CreateArtifactRequest {
  run_id?: string | null;
  name: string;
  mime_type: string;
  content?: string | null;
  relative_path?: string | null;
  completeness_status?: ArtifactCompleteness;
}

export interface ListArtifactsResponse {
  artifacts: ArtifactRecord[];
}

export interface ArtifactLifecycleRequest {
  action: ArtifactLifecycleAction;
  reason?: string | null;
}

export interface ArtifactLifecycleResponse {
  artifact: ArtifactRecord;
  user_workspace_files_deleted: boolean;
}

export interface FlushOutboxResponse {
  attempted: number;
  delivered: number;
  pending: number;
  failed: number;
  memory_status: MemoryStatus;
  items: MemoryOutboxItem[];
}

export interface DiagnosticsResponse {
  events: DiagnosticEvent[];
  permission_decisions: PermissionDecisionRecord[];
  outbox_items: MemoryOutboxItem[];
}

export type AdmissionStatus = "admitted" | "ephemeral" | "rejected";
export type TrustLevel = "native" | "external_untrusted" | "reviewed" | "trusted";
export type EpistemicStatus =
  | "observed"
  | "asserted"
  | "inferred"
  | "hypothesis"
  | "contested"
  | "superseded"
  | "retracted";
export type ConflictType = "temporal" | "source" | "concurrent_edit" | "semantic" | "policy";
export type ConflictStatus = "open" | "resolved" | "accepted_ambiguity" | "invalidated";
export type ConsolidationJobType =
  | "episode_summary"
  | "dossier_update"
  | "claim_extraction"
  | "index_rebuild"
  | "conflict_scan"
  | "retention_sweep";
export type ConsolidationJobStatus =
  | "pending"
  | "running"
  | "paused"
  | "completed"
  | "failed"
  | "cancelled";
export type IndexStatus = "healthy" | "lagging" | "rebuilding" | "degraded";
export type ForgetTargetType = "note" | "evidence" | "claim" | "document";
export type ForgetPropagationStatus =
  | "pending"
  | "index_cleared"
  | "projections_cleared"
  | "completed"
  | "failed";

export interface EvidenceAnchor {
  kind: string;
  start?: number | null;
  end?: number | null;
  locator?: string | null;
  attributes?: Record<string, string>;
}

export interface EvidenceLinkInput {
  evidence_id: string;
  relationship: string;
  anchor?: EvidenceAnchor | null;
  ordinal?: number | null;
}

export interface EventEvidenceLink {
  link_id: string;
  event_id: string;
  evidence_id: string;
  relationship: string;
  anchor?: EvidenceAnchor | null;
  ordinal?: number | null;
  revision: number;
}

export interface MemoryNote {
  note_id: string;
  scope: string;
  current_revision?: number;
  revision?: number;
  current_revision_id?: string;
  title: string;
  body: string;
  tags?: string[];
  rationale?: string | null;
  last_event_id?: string;
  recorded_by?: string;
  author_label?: string | null;
  created_at: number;
  updated_at: number;
  admission_status?: AdmissionStatus;
  salience?: number;
  topic_key?: string | null;
  trust_level?: TrustLevel;
  instruction_active?: boolean;
  evidence_links?: EventEvidenceLink[];
}

export interface NoteRevision {
  revision_id: string;
  note_id: string;
  revision: number;
  title: string;
  body: string;
  tags?: string[];
  rationale?: string | null;
  event_id: string;
  recorded_by: string;
  author_label?: string | null;
  created_at: number;
  evidence_links?: EventEvidenceLink[];
}

export interface NoteMutationResponse {
  note: MemoryNote;
  event_id: string;
  idempotent_replay: boolean;
}

export interface NoteHistoryResponse {
  note_id: string;
  scope: string;
  current_revision: number;
  revisions: NoteRevision[];
}

export interface CommitMemoryNoteRequest {
  session_id?: string | null;
  run_id?: string | null;
  scope: string;
  title: string;
  body: string;
  tags?: string[];
  idempotency_key?: string | null;
}

export interface CommitMemoryNoteResponse {
  outbox_item: MemoryOutboxItem;
  delivered_immediately: boolean;
  memory_status: MemoryStatus;
}

export interface CorrectNoteRequest {
  note_id: string;
  scope: string;
  expected_revision: number;
  title: string;
  body: string;
  tags?: string[];
  rationale: string;
  evidence_links?: EvidenceLinkInput[];
  author_label?: string | null;
  idempotency_key: string;
}

export interface HeadMemorySearchRequest {
  scope?: string | null;
  query?: string | null;
  limit?: number | null;
  include_history?: boolean;
}

export interface HeadMemorySearchResponse {
  memory_status: MemoryStatus;
  degraded_reason?: string | null;
  notes: MemoryNote[];
  index_status?: IndexStatus;
  stale_warning?: string | null;
}

export interface MemoryClaim {
  claim_id: string;
  scope: string;
  topic_key: string;
  claim_key?: string | null;
  natural_language: string;
  epistemic_status: EpistemicStatus;
  supersedes_claim_id?: string | null;
  valid_from?: number | null;
  valid_to?: number | null;
  source_event_id: string;
  source_note_id?: string | null;
  caveats?: string[];
  provenance?: Record<string, string>;
  trust_level?: TrustLevel;
  instruction_active?: boolean;
  evidence_links?: EventEvidenceLink[];
  created_at: number;
}

export interface CreateClaimRequest {
  scope: string;
  topic_key: string;
  claim_key?: string | null;
  natural_language: string;
  epistemic_status?: EpistemicStatus;
  supersedes_claim_id?: string | null;
  valid_from?: number | null;
  valid_to?: number | null;
  source_note_id?: string | null;
  caveats?: string[];
  provenance?: Record<string, string>;
  evidence_links?: EvidenceLinkInput[];
  idempotency_key: string;
}

export interface SupersedeClaimRequest {
  claim_id: string;
  scope: string;
  new_claim: CreateClaimRequest;
  resolution_event_id?: string | null;
}

export interface ListClaimsResponse {
  claims: MemoryClaim[];
}

export interface MemoryConflict {
  conflict_id: string;
  scope: string;
  topic_key?: string | null;
  conflict_type: ConflictType;
  candidate_claim_ids: string[];
  blocking?: boolean;
  status: ConflictStatus;
  resolution_event_id?: string | null;
  resolution_note?: string | null;
  detected_at: number;
  resolved_at?: number | null;
}

export interface ListConflictsResponse {
  conflicts: MemoryConflict[];
}

export interface ResolveConflictRequest {
  conflict_id: string;
  scope: string;
  resolution_note: string;
  winning_claim_id?: string | null;
  target_status?: ConflictStatus | null;
  idempotency_key: string;
}

export interface MemoryDocument {
  document_id: string;
  scope: string;
  document_key: string;
  title: string;
  current_version: number;
  current_body: string;
  provenance_claim_ids?: string[];
  provenance_note_ids?: string[];
  source_event_id?: string | null;
  principal_id: string;
  trust_level?: TrustLevel;
  instruction_active?: boolean;
  created_at: number;
  updated_at: number;
}

export interface DocumentVersion {
  version_id: string;
  document_id: string;
  version: number;
  body: string;
  provenance_claim_ids?: string[];
  diff_summary?: string | null;
  event_id: string;
  created_at: number;
}

export interface CreateDocumentRequest {
  scope: string;
  document_key: string;
  title: string;
  body: string;
  provenance_claim_ids?: string[];
  provenance_note_ids?: string[];
  idempotency_key: string;
}

export interface UpdateDocumentRequest {
  document_id: string;
  scope: string;
  expected_version: number;
  title: string;
  body: string;
  provenance_claim_ids?: string[];
  provenance_note_ids?: string[];
  diff_summary?: string | null;
  idempotency_key: string;
}

export interface ListDocumentsResponse {
  documents: MemoryDocument[];
}

export interface GetDocumentVersionsResponse {
  document_id: string;
  scope: string;
  current_version: number;
  versions: DocumentVersion[];
}

export interface ConsolidationJob {
  job_id: string;
  scope: string;
  job_type: ConsolidationJobType;
  status: ConsolidationJobStatus;
  checkpoint_json?: string | null;
  watermark_event_id?: string | null;
  caveats_preserved: boolean;
  error_detail?: string | null;
  created_at: number;
  updated_at: number;
  started_at?: number | null;
  completed_at?: number | null;
}

export interface CreateConsolidationJobRequest {
  scope: string;
  job_type: ConsolidationJobType;
  idempotency_key?: string | null;
}

export interface ListConsolidationJobsResponse {
  jobs: ConsolidationJob[];
}

export interface IndexState {
  scope: string;
  watermark_event_id?: string | null;
  watermark_seq: number;
  model_version: string;
  status: IndexStatus;
  last_rebuild_at?: number | null;
  updated_at?: number | null;
}

export interface TriggerIndexRebuildRequest {
  scope: string;
}

export interface TriggerIndexRebuildResponse {
  index_state: IndexState;
  entries_rebuilt: number;
}

export interface MemoryStatsSummary {
  scope?: string | null;
  sources_count: number;
  active_notes_count: number;
  active_claims_count: number;
  documents_count: number;
  open_conflicts_count: number;
  imported_untrusted_count: number;
  index_status: IndexStatus;
  watermark_seq: number;
  last_rebuild_at?: number | null;
}

export interface ForgetRequest {
  scope: string;
  target_type: ForgetTargetType;
  target_id: string;
  reason: string;
  idempotency_key: string;
}

export interface ForgetResponse {
  tombstone_id: string;
  forget_event_id: string;
  residual_backup_notice: string;
  propagation_status: ForgetPropagationStatus;
  idempotent_replay?: boolean;
}

export interface ForgetTombstone {
  tombstone_id: string;
  scope: string;
  target_type: ForgetTargetType;
  target_id: string;
  forget_event_id: string;
  principal_id: string;
  reason: string;
  residual_backup_notice: string;
  propagation_status: ForgetPropagationStatus;
  created_at: number;
}

export interface ListTombstonesResponse {
  tombstones: ForgetTombstone[];
}

export interface ExportRequest {
  scope: string;
  format_version?: string | null;
}

export interface ExportResponse {
  package_id: string;
  scope: string;
  format_version: string;
  manifest_sha256: string;
  item_count: number;
  byte_size: number;
  secrets_stripped: boolean;
  package_json: string;
}

export interface ImportRequest {
  scope: string;
  package_json: string;
}

export interface ImportResponse {
  import_id: string;
  scope: string;
  source_package_sha256: string;
  trust_level: TrustLevel;
  instruction_active: boolean;
  items_imported: number;
  items_rejected: number;
  validation_errors?: string[];
}

export interface MemoryOverviewResponse {
  memory_status: MemoryStatus;
  degraded_reason?: string | null;
  scope?: string | null;
  stats?: MemoryStatsSummary | null;
  notes: MemoryNote[];
  claims: MemoryClaim[];
  conflicts: MemoryConflict[];
  documents: MemoryDocument[];
  consolidation_jobs: ConsolidationJob[];
  tombstones: ForgetTombstone[];
}

// ============================================================================
// Phase D3: SecretBroker, Provider Connections, Capabilities Registry (A08)
// ============================================================================

export type SecretSource = "vault" | "env_fallback" | "unset";

export interface SetSecretRequest {
  value: string;
  scope?: string;
  description?: string;
}

export interface SecretMetadataRecord {
  slot_key: string;
  scope: string;
  description: string;
  configured: boolean;
  version: number;
  fingerprint_short?: string | null;
  source: SecretSource;
  updated_at?: number | null;
}

export interface ListSecretsResponse {
  secrets: SecretMetadataRecord[];
}

export type ProviderKind = "antigravity_acp" | "local_http";
export type ProviderHealthStatus =
  | "healthy"
  | "auth_required"
  | "unreachable"
  | "incompatible"
  | "quota_limited"
  | "unknown";

export interface ProviderQuotaError {
  provider: string;
  auth_profile: RuntimeAuthProfile;
  detail: string;
  retry_after_secs?: number | null;
}

export interface ProviderConnectionRecord {
  connection_id: string;
  provider_kind: ProviderKind;
  display_name: string;
  adapter_id: string;
  auth_profile: RuntimeAuthProfile;
  endpoint_url?: string | null;
  secret_slot_key?: string | null;
  is_default_interactive: boolean;
  is_default_background: boolean;
  enabled: boolean;
  last_health_status: ProviderHealthStatus;
  last_health_detail?: string | null;
  last_probed_at?: number | null;
  created_at: number;
  updated_at: number;
}

export interface PlannedProviderInfo {
  id: string;
  label: string;
  status: string;
  note?: string | null;
}

export interface ListProviderConnectionsResponse {
  connections: ProviderConnectionRecord[];
  planned_providers: PlannedProviderInfo[];
}

export interface UpsertProviderConnectionRequest {
  provider_kind: ProviderKind;
  display_name: string;
  adapter_id?: string | null;
  auth_profile: RuntimeAuthProfile;
  endpoint_url?: string | null;
  secret_slot_key?: string | null;
  is_default_interactive?: boolean | null;
  is_default_background?: boolean | null;
  enabled?: boolean | null;
}

export type ConnectionActivationRole = "interactive" | "background" | "both";

export interface ActivateConnectionRequest {
  role?: ConnectionActivationRole;
}

export interface ProbeConnectionResponse {
  connection: ProviderConnectionRecord;
  catalog?: RuntimeCatalogResponse | null;
}

export interface DeleteProviderConnectionResponse {
  connection_id: string;
  deleted: boolean;
}

export type CapabilityKind = "skill" | "mcp_server" | "plugin";
export type CapabilityOrigin =
  | "user_manual"
  | "project_local"
  | "personal_discovered"
  | "builtin";
export type CapabilityOwnership = "user" | "project" | "provider" | "dennett";
export type CapabilityScopeKind = "global" | "project" | "candidate_unattached";
export type CapabilityPossession = "discovered" | "installed" | "connected";
export type CapabilityTrust = "unreviewed" | "restricted" | "trusted" | "revoked";
export type CapabilityActivation = "disabled" | "on_demand" | "active" | "pinned" | "forbidden";
export type CompatibilityStatus = "compatible" | "warning" | "incompatible";

export interface PluginMcpServerEntry {
  name: string;
  command: string;
  args?: string[];
  env_vars?: Record<string, string>;
  secret_slots?: Record<string, string>;
}

export type CapabilitySpec =
  | {
      kind: "skill";
      skill_markdown: string;
      has_scripts?: boolean;
      entry_files?: string[];
    }
  | {
      kind: "mcp_server";
      command: string;
      args?: string[];
      env_vars?: Record<string, string>;
      secret_slots?: Record<string, string>;
    }
  | {
      kind: "plugin";
      manifest_version: string;
      skills?: string[];
      mcp_servers?: PluginMcpServerEntry[];
      has_hooks?: boolean;
      enabled_components?: string[];
    };

export interface CapabilityProjectBinding {
  capability_id: string;
  project_id: string;
  activation_state: CapabilityActivation;
  updated_at: number;
}

export interface CapabilityRecord {
  capability_id: string;
  slug: string;
  kind: CapabilityKind;
  display_name: string;
  description: string;
  version: string;
  origin_kind: CapabilityOrigin;
  ownership: CapabilityOwnership;
  scope_kind: CapabilityScopeKind;
  project_id?: string | null;
  possession_state: CapabilityPossession;
  trust_state: CapabilityTrust;
  activation_state: CapabilityActivation;
  compatibility_status: CompatibilityStatus;
  compatibility_detail?: string | null;
  source_path?: string | null;
  content_sha256: string;
  spec: CapabilitySpec;
  can_rollback: boolean;
  revision: number;
  project_bindings?: CapabilityProjectBinding[];
  created_at: number;
  updated_at: number;
}

export interface ListCapabilitiesResponse {
  capabilities: CapabilityRecord[];
}

export interface DiscoverCapabilitiesRequest {
  project_id?: string | null;
  scan_personal_gemini_readonly?: boolean;
}

export interface DiscoverCapabilitiesResponse {
  discovered: CapabilityRecord[];
  newly_registered: number;
  updated_existing: number;
  warnings?: string[];
}

export interface CreateCapabilityRequest {
  slug: string;
  display_name: string;
  description?: string;
  version: string;
  origin_kind?: CapabilityOrigin;
  ownership?: CapabilityOwnership;
  scope_kind: CapabilityScopeKind;
  project_id?: string | null;
  trust_state?: CapabilityTrust;
  activation_state?: CapabilityActivation;
  source_path?: string | null;
  spec: CapabilitySpec;
}

export interface UpdateCapabilityRequest {
  display_name?: string | null;
  description?: string | null;
  version?: string | null;
  scope_kind?: CapabilityScopeKind | null;
  project_id?: string | null;
  trust_state?: CapabilityTrust | null;
  activation_state?: CapabilityActivation | null;
  spec?: CapabilitySpec | null;
}

export interface SetProjectCapabilityBindingRequest {
  activation_state: CapabilityActivation;
}

export interface DeleteCapabilityResponse {
  capability_id: string;
  deleted: boolean;
}

// ============================================================================
// Phase D2 Envelopes
// ============================================================================

export interface StateSnapshotResponse {
  snapshot_cursor: StreamCursor;
  projects: ProjectRecord[];
  agents: AgentRecord[];
  sessions: SessionRecord[];
  tasks: TaskRecord[];
  inbox_cards: InboxCardRecord[];
  active_grants: CapabilityGrantRecord[];
  unresolved_inbox_count: number;
  running_session_ids?: string[];
}

export interface EventStreamResponse {
  authority_epoch: number;
  retained_from_seq: number;
  after_cursor: number;
  scanned_through_seq: number;
  next_cursor: number;
  has_more: boolean;
  events: ControlEventRecord[];
}

export interface CreateTaskRequest {
  project_id: string;
  session_id?: string | null;
  agent_id: string;
  parent_task_id?: string | null;
  title?: string | null;
  goal: string;
  schedule?: TaskSchedule | null;
  wait_for_signal?: string | null;
  budget?: TaskBudget | null;
  attached_skills?: string[];
  auth_profile_override?: RuntimeAuthProfile | null;
  execute_immediately?: boolean | null;
}

export interface ListTasksResponse {
  tasks: TaskRecord[];
}

export interface TaskDetailResponse {
  task: TaskRecord;
  runs: RunRecord[];
  timers: TaskTimerRecord[];
  signals: TaskSignalRecord[];
  effects: EffectClaimRecord[];
  open_inbox_cards: InboxCardRecord[];
}

export interface UpdateTaskRequest {
  title?: string | null;
  goal?: string | null;
  schedule?: TaskSchedule | null;
  clear_schedule?: boolean;
  budget?: TaskBudget | null;
  execute_immediately?: boolean | null;
}

export interface TaskSignalRequest {
  idempotency_key: string;
  signal_name: string;
  payload_text?: string;
}

export interface TaskSignalResponse {
  signal: TaskSignalRecord;
  task: TaskRecord;
  deduplicated: boolean;
}

export interface ReplyTaskRequest {
  answer_text: string;
}

export interface CancelTaskResponse {
  task: TaskRecord;
  cancelled_run_id?: string | null;
  external_outcome_unknown: boolean;
  committed_effects: EffectClaimRecord[];
  unknown_effects: EffectClaimRecord[];
  memory_status: MemoryStatus;
}

export interface ListInboxResponse {
  cards: InboxCardRecord[];
  unresolved_count: number;
}

export type InboxResolutionAction =
  | {
      action: "permission";
      decision: PermissionResolutionDecision;
      reason?: string | null;
      max_uses?: number | null;
      expires_in_secs?: number | null;
    }
  | {
      action: "clarification";
      answer_text: string;
    }
  | {
      action: "increase_budget";
      additional_turns?: number | null;
      additional_tokens?: number | null;
      additional_wall_secs?: number | null;
    }
  | {
      action: "stop_task";
      reason?: string | null;
    }
  | {
      action: "missed_schedule";
      run_now: boolean;
    }
  | {
      action: "reconcile_effect";
      manual_resolution?: ManualEffectResolution | null;
      reason?: string | null;
    };

export interface ResolveInboxCardRequest {
  expected_revision: number;
  resolution: InboxResolutionAction;
}

export interface ResolveInboxCardResponse {
  card: InboxCardRecord;
  task?: TaskRecord | null;
  permission_request?: PermissionRequestRecord | null;
  issued_grant?: CapabilityGrantRecord | null;
  effect?: EffectClaimRecord | null;
}

export interface ListPermissionRequestsResponse {
  requests: PermissionRequestRecord[];
}

export interface ResolvePermissionRequest {
  expected_revision?: number | null;
  decision: PermissionResolutionDecision;
  reason?: string | null;
  max_uses?: number | null;
  expires_in_secs?: number | null;
}

export interface ResolvePermissionResponse {
  request: PermissionRequestRecord;
  decision: PermissionDecisionRecord;
  issued_grant?: CapabilityGrantRecord | null;
}

export interface ListCapabilityGrantsResponse {
  grants: CapabilityGrantRecord[];
}

export interface RevokeGrantRequest {
  reason?: string | null;
}

export interface SendAgentMessageRequest {
  sender_agent_id?: string | null;
  project_id: string;
  related_task_id?: string | null;
  idempotency_key: string;
  body_text: string;
}

export interface SendAgentMessageResponse {
  message: AgentMessageRecord;
  deduplicated: boolean;
}

export interface ListAgentMessagesResponse {
  messages: AgentMessageRecord[];
}

export interface DispatchEffectRequest {
  project_id: string;
  session_id: string;
  task_id?: string | null;
  run_id?: string | null;
  agent_id: string;
  effect_idempotency_key: string;
  connector_id?: string | null;
  operation: string;
  target_recipient: string;
  parameters_json?: string;
  simulated_behavior?: SimEffectBehavior | null;
}

export interface DispatchEffectResponse {
  effect: EffectClaimRecord;
  deduplicated: boolean;
  external_outcome_unknown: boolean;
}

export interface ListEffectsResponse {
  effects: EffectClaimRecord[];
}

export interface ReconcileEffectRequest {
  manual_resolution?: ManualEffectResolution | null;
  reason?: string | null;
}

export interface ReconcileEffectResponse {
  effect: EffectClaimRecord;
  task?: TaskRecord | null;
  provider_found: boolean;
  provider_execution_count: number;
  resolution_summary?: string;
}

export interface QueuedOfflineMessage {
  client_message_id: string;
  session_id: string;
  project_id: string | null;
  content: string;
  created_at: number;
}

export function comparePinnedRecords(
  aPinned: boolean | undefined,
  aPinnedAt: number | null | undefined,
  bPinned: boolean | undefined,
  bPinnedAt: number | null | undefined,
  fallbackA: number,
  fallbackB: number,
): number {
  const ap = Boolean(aPinned);
  const bp = Boolean(bPinned);
  if (ap !== bp) {
    return ap ? -1 : 1;
  }
  if (ap && bp) {
    const aTs = aPinnedAt ?? -Infinity;
    const bTs = bPinnedAt ?? -Infinity;
    if (aTs !== bTs) {
      return bTs - aTs;
    }
  }
  return fallbackB - fallbackA;
}

export function sortProjectsByPin(projects: ProjectRecord[]): ProjectRecord[] {
  return [...projects].sort((a, b) =>
    comparePinnedRecords(a.pinned, a.pinned_at, b.pinned, b.pinned_at, a.created_at, b.created_at),
  );
}

export function sortSessionsByPin(sessions: SessionRecord[]): SessionRecord[] {
  return [...sessions].sort((a, b) =>
    comparePinnedRecords(a.pinned, a.pinned_at, b.pinned, b.pinned_at, a.updated_at, b.updated_at),
  );
}

export interface BridgeErrorPayload {
  code: string;
  message: string;
}

export function isStreamResyncConflict(err: BridgeErrorPayload): boolean {
  const code = err.code.toUpperCase();
  if (code !== "CONFLICT" && code !== "HTTP_409") {
    return false;
  }
  const msg = err.message.toLowerCase();
  return (
    msg.includes("stream_epoch") ||
    msg.includes("stream_cursor_expired") ||
    msg.includes("epoch") ||
    msg.includes("retained_from_seq")
  );
}

export function normalizeBridgeError(err: unknown): BridgeErrorPayload {
  if (typeof err === "object" && err !== null) {
    const record = err as Record<string, unknown>;
    if (typeof record.message === "string") {
      return {
        code: typeof record.code === "string" ? record.code : "BRIDGE_ERROR",
        message: record.message,
      };
    }
    if (record.error && typeof record.error === "object") {
      const inner = record.error as Record<string, unknown>;
      const topCode =
        typeof record.code === "string"
          ? record.code
          : typeof inner.code === "string"
            ? inner.code
            : "HEAD_API";
      const resourcePrefix =
        typeof inner.resource === "string" && inner.resource.length > 0
          ? `conflict on ${inner.resource}: `
          : "";
      const rawText =
        typeof inner.message === "string"
          ? inner.message
          : typeof inner.reason === "string"
            ? `${resourcePrefix}${inner.reason}`
            : typeof inner.detail === "string"
              ? inner.detail
              : null;
      if (rawText) {
        return {
          code: topCode,
          message: rawText,
        };
      }
    }
  }
  if (err instanceof Error) {
    return { code: "CLIENT_ERROR", message: err.message };
  }
  if (typeof err === "string") {
    return { code: "BRIDGE_ERROR", message: err };
  }
  return { code: "UNKNOWN_ERROR", message: "Непредвиденная ошибка взаимодействия с dennett-head." };
}
