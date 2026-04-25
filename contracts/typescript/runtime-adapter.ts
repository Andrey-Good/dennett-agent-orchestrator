import type { JsonObject, JsonValue } from "./json";
import type {
  BindingId,
  JsonOutputContract,
  MemoryBinding,
  NodeId,
  OutputContract,
  Permissions,
  RuntimeAdapterId,
  RuntimeSourceId,
  SkillBinding,
  TextOutputContract,
} from "./agent-file";
import type {
  UserChatRequestPayload,
  UserChatResponsePayload,
  UserChatServerName,
} from "./orchestrator-user-chat-mcp";

export interface RuntimeAdapterCapabilities {
  supports_native_resume: boolean;
  supports_live_comments: boolean;
  supports_builtin_user_chat_mcp: boolean;
  supports_memory_bindings: boolean;
  supports_model_discovery: boolean;
  supports_runtime_environment_introspection: boolean;
  supports_reasoning_effort: boolean;
  supports_speed_tiers: boolean;
  supports_personality: boolean;
  supports_explicit_runtime_source: boolean;
  supports_runtime_source_introspection: boolean;
}

export type RuntimeReasoningEffort =
  | "none"
  | "minimal"
  | "low"
  | "medium"
  | "high"
  | "xhigh";

export type RuntimeSpeedTier = "fast" | "flex";

export type RuntimePersonality = "none" | "friendly" | "pragmatic";

export interface RuntimeModelCatalogRequest {
  cursor?: string;
  limit?: number;
  include_hidden?: boolean;
}

export interface RuntimeModelDescriptor {
  id: string;
  display_name?: string;
  description?: string;
  hidden: boolean;
  is_default: boolean;
  input_modalities: string[];
  supports_personality: boolean;
  default_reasoning_effort?: RuntimeReasoningEffort;
  supported_reasoning_efforts: RuntimeReasoningEffort[];
  additional_speed_tiers: RuntimeSpeedTier[];
  upgrade_target?: string;
  upgrade_info?: string;
}

export interface RuntimeModelCatalogPage {
  models: RuntimeModelDescriptor[];
  next_cursor?: string;
}

export interface RuntimeAuthStatus {
  authenticated: boolean;
  auth_method?: string;
  requires_openai_auth: boolean;
}

export interface RuntimeAccountSummary {
  status: "available" | "missing" | "unknown";
  account_type?: string;
  email?: string;
  plan_type?: string;
}

export interface RuntimeRateLimitSummary {
  limit_id: string;
  limit_name?: string;
  plan_type?: string;
  primary?: JsonObject;
  secondary?: JsonObject;
  credits?: JsonObject;
}

export interface RuntimeConfigSnapshot {
  model?: string;
  review_model?: string;
  model_provider?: string;
  approval_policy?: string;
  sandbox_mode?: string;
  profile?: string;
  model_reasoning_effort?: RuntimeReasoningEffort;
  service_tier?: RuntimeSpeedTier;
}

export interface RuntimeConfigRequirementsSnapshot {
  allowed_approval_policies?: string[];
  allowed_sandbox_modes?: string[];
  allowed_web_search_modes?: string[];
  enforce_residency?: boolean;
  feature_requirements?: JsonObject;
}

export interface RuntimeEnvironmentInspectionResult {
  auth: RuntimeAuthStatus;
  account: RuntimeAccountSummary;
  rate_limits: RuntimeRateLimitSummary[];
  config: RuntimeConfigSnapshot;
  config_requirements?: RuntimeConfigRequirementsSnapshot;
}

export interface ResolvedSkillBindingWithCodexRef {
  id: BindingId;
  codex_ref: string;
  inline_text?: string;
  frozen?: false;
}

export interface ResolvedSkillBindingWithInlineText {
  id: BindingId;
  codex_ref?: string;
  inline_text: string;
  frozen?: false;
}

export interface FrozenResolvedSkillBindingWithCodexRef {
  id: BindingId;
  codex_ref: string;
  inline_text: string;
  frozen: true;
}

export interface FrozenResolvedSkillBindingWithInlineText {
  id: BindingId;
  codex_ref?: string;
  inline_text: string;
  frozen: true;
}

export type ResolvedSkillBinding =
  | ResolvedSkillBindingWithCodexRef
  | ResolvedSkillBindingWithInlineText
  | FrozenResolvedSkillBindingWithCodexRef
  | FrozenResolvedSkillBindingWithInlineText;

export interface ResolvedMcpBinding {
  id: BindingId;
  codex_ref: string;
  config?: JsonObject;
}

export interface ResolvedPluginBinding {
  id: BindingId;
  codex_ref: string;
  config?: JsonObject;
}

export interface ResolvedMemoryBinding {
  id: BindingId;
  kind: "runtime_memory";
  codex_ref: string;
  config?: JsonObject;
  scope: "agent" | "node";
}

export interface RuntimeAdapterEffectiveBindings {
  skills: ResolvedSkillBinding[];
  mcps: ResolvedMcpBinding[];
  plugins: ResolvedPluginBinding[];
  memory_bindings?: ResolvedMemoryBinding[];
}

export type RuntimeMemoryCapability =
  | "read"
  | "write"
  | "entity_scoped"
  | "user_scoped"
  | "group_scoped"
  | "session_scoped"
  | "graph_context"
  | "temporal_index"
  | "profile_synthesis"
  | "rag_retrieval"
  | "infer_extract"
  | "versioned_write"
  | "mcp_transport";

export interface RuntimeMemoryBindingIntent {
  summary: string;
  labels?: string[];
}

export interface RuntimeMemoryOperationScope {
  agent_id: string;
  run_id: string;
  user_id?: string;
}

export interface RuntimeMemoryRecordScope {
  agent_id?: string;
  run_id?: string;
  user_id?: string;
}

export interface RuntimeMemoryRecord {
  id: string;
  content: string;
  scope: RuntimeMemoryRecordScope;
  metadata?: JsonObject;
  score?: number;
  created_at?: string;
  updated_at?: string;
  provider_data?: JsonObject;
}

export interface RuntimeMemoryReadContext {
  query: string;
  records: RuntimeMemoryRecord[];
}

export type RuntimeMemoryWriteContext =
  | {
      enabled: true;
      mode: "node_success_output";
      disabled_reason?: never;
    }
  | {
      enabled: false;
      mode?: never;
      disabled_reason?: string;
    };

export interface RuntimeMemoryBindingContext {
  binding_id: BindingId;
  codex_ref: string;
  intent: RuntimeMemoryBindingIntent;
  required_capabilities: RuntimeMemoryCapability[];
  scope: RuntimeMemoryOperationScope;
  read?: RuntimeMemoryReadContext;
  write: RuntimeMemoryWriteContext;
}

export interface RuntimeMemoryContext {
  bindings: RuntimeMemoryBindingContext[];
}

export type ResolvedInputMessage = JsonValue;

export type RuntimeResumeRequest =
  | {
      mode: "fresh";
      native_session_handle?: never;
    }
  | {
      mode: "native_resume";
      native_session_handle: unknown;
    };

export interface RuntimeSourceSelection {
  id: RuntimeSourceId;
  runtime_adapter: RuntimeAdapterId;
  source_ref: string;
  description?: string;
}

export interface RuntimeAdapterExecutionRequest {
  node_id: NodeId;
  runtime_adapter: RuntimeAdapterId;
  prompt: string;
  input_message: ResolvedInputMessage;
  output: OutputContract;
  effective_bindings: RuntimeAdapterEffectiveBindings;
  permissions: Permissions;
  runtime_options: JsonObject;
  runtime_source?: RuntimeSourceSelection;
  memory_context?: RuntimeMemoryContext;
  interaction: {
    comments_enabled: boolean;
    user_chat_server_name?: UserChatServerName;
  };
  resume: RuntimeResumeRequest;
}

export interface RuntimeSourceInspectionResult {
  source_id: RuntimeSourceId;
  availability: "available" | "unavailable" | "unknown";
  limit_status: "ok" | "limited" | "exhausted" | "unknown";
  status_message?: string;
}

export interface RuntimeCommentEvent {
  kind: "comment";
  payload: {
    text: string;
  };
}

export interface RuntimeUserChatRequestEvent {
  kind: "user_chat_request";
  payload: UserChatRequestPayload;
}

export type RuntimeEvent = RuntimeCommentEvent | RuntimeUserChatRequestEvent;

export interface RuntimeTerminalSuccessTextResult {
  outcome: "success";
  output: TextOutputContract;
  output_text: string;
  native_session_handle?: unknown;
  error?: never;
}

export interface RuntimeTerminalSuccessJsonResult {
  outcome: "success";
  output: JsonOutputContract;
  output_json: JsonObject;
  native_session_handle?: unknown;
  error?: never;
}

export interface RuntimeTerminalFailureResult {
  outcome: "invalid_output" | "runtime_error" | "interrupted" | "cancelled";
  error: {
    code: string;
    message: string;
    details?: unknown;
  };
  native_session_handle?: unknown;
  output?: never;
  output_text?: never;
  output_json?: never;
}

export type RuntimeTerminalResult =
  | RuntimeTerminalSuccessTextResult
  | RuntimeTerminalSuccessJsonResult
  | RuntimeTerminalFailureResult;

export interface RuntimeAdapterPorts {
  describeCapabilities(): RuntimeAdapterCapabilities;
  startExecution(request: RuntimeAdapterExecutionRequest): Promise<{
    runtime_handle: JsonValue | null;
    native_session_handle: JsonValue | null;
    terminal_result: Promise<RuntimeTerminalResult>;
  }>;
  listModels(
    request?: RuntimeModelCatalogRequest,
  ): Promise<RuntimeModelCatalogPage>;
  inspectRuntimeEnvironment(): Promise<RuntimeEnvironmentInspectionResult>;
  inspectRuntimeSource(
    source: RuntimeSourceSelection,
  ): Promise<RuntimeSourceInspectionResult>;
  deliverComment(execution: unknown, text: string): Promise<void>;
  deliverUserChatResponse(
    execution: unknown,
    response: UserChatResponsePayload,
  ): Promise<void>;
  cancelExecution(execution: unknown): Promise<void>;
}

export type {
  MemoryBinding,
  Permissions,
  RuntimeAdapterId,
  SkillBinding,
};
