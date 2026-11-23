export type ProfileMode = "production" | "test_isolated";
export type RuntimeAuthProfile = "oauth-personal" | "gemini-api-key";
export type ServiceKind = "memoryd" | "head" | "node";
export type MemoryStatus = "healthy" | "degraded_unreachable";
export type MemoryOutboxStatus = "pending" | "delivered" | "failed";
export type ThinkingLevel = "low" | "medium" | "high" | "xhigh";
export type SessionStatus = "active" | "archived" | "closed";
export type MessageRole = "user" | "assistant" | "system";
export type RunStatus = "running" | "succeeded" | "cancelled" | "failed" | "degraded";
export type PolicyEffect = "allow" | "deny";

export type WorkspaceWritePolicy =
  | { mode: "deny_all" }
  | { mode: "ask_before_write" }
  | { mode: "allow_workspace" }
  | { mode: "allow_paths"; allowed_prefixes: string[] };

export interface AgentConfigVersion {
  config_version_id: string;
  agent_id: string;
  version_number: number;
  system_prompt: string;
  model_preference?: string | null;
  thinking_level?: ThinkingLevel | null;
  allowed_memory_scopes: string[];
  workspace_write_policy: WorkspaceWritePolicy;
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
  workspace_path: string;
  memory_scope: string;
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
  created_at: number;
  updated_at: number;
}

export interface SessionRecord {
  session_id: string;
  agent_id: string;
  project_id: string;
  title: string;
  runtime_binding?: RuntimeBinding | null;
  status: SessionStatus;
  created_at: number;
  updated_at: number;
}

export interface MessageRecord {
  message_id: string;
  session_id: string;
  role: MessageRole;
  content: string;
  run_id?: string | null;
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
  correlation_id: string;
  created_at: number;
  finished_at?: number | null;
}

export interface ArtifactRecord {
  artifact_id: string;
  session_id: string;
  project_id: string;
  run_id?: string | null;
  name: string;
  mime_type: string;
  revision: number;
  content?: string | null;
  relative_path?: string | null;
  sha256: string;
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
  workspace_path: string;
  memory_scope: string;
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
}

export interface UpdateAgentConfigRequest {
  system_prompt: string;
  model_preference?: string | null;
  thinking_level?: ThinkingLevel | null;
  allowed_memory_scopes: string[];
  workspace_write_policy: WorkspaceWritePolicy;
}

export interface ListSessionsResponse {
  sessions: SessionRecord[];
}

export interface CreateSessionRequest {
  agent_id: string;
  project_id: string;
  title: string;
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
}

export interface SendMessageResponse {
  user_message: MessageRecord;
  assistant_message?: MessageRecord | null;
  run: RunRecord;
  runtime_binding: RuntimeBinding;
  artifacts: ArtifactRecord[];
  permission_decisions: PermissionDecisionRecord[];
  memory_status: MemoryStatus;
}

export interface CancelRunResponse {
  session_id: string;
  run_id?: string | null;
  cancelled: boolean;
  status?: RunStatus | null;
  memory_status: MemoryStatus;
}

export interface ListArtifactsResponse {
  artifacts: ArtifactRecord[];
}

export interface CreateArtifactRequest {
  run_id?: string | null;
  name: string;
  mime_type: string;
  content?: string | null;
  relative_path?: string | null;
}

export interface DiagnosticsResponse {
  events: DiagnosticEvent[];
  permission_decisions: PermissionDecisionRecord[];
  outbox_items: MemoryOutboxItem[];
}

export interface FlushOutboxResponse {
  attempted: number;
  delivered: number;
  pending: number;
  failed: number;
  memory_status: MemoryStatus;
  items: MemoryOutboxItem[];
}

export interface BridgeErrorPayload {
  code: string;
  message: string;
}

export function normalizeBridgeError(err: unknown): BridgeErrorPayload {
  if (typeof err === "object" && err !== null) {
    const record = err as Record<string, unknown>;
    if (typeof record.code === "string" && typeof record.message === "string") {
      return { code: record.code, message: record.message };
    }
    if (typeof record.message === "string") {
      return { code: "BRIDGE_ERROR", message: record.message };
    }
  }
  if (typeof err === "string" && err.trim().length > 0) {
    return { code: "BRIDGE_ERROR", message: err };
  }
  return { code: "BRIDGE_ERROR", message: "Неизвестная ошибка связи с dennett-head" };
}
