import type {
  JsonObject,
  JsonObjectSchema,
  JsonValue,
  NonEmptyArray,
} from "./json";
import type { UserChatServerName } from "./orchestrator-user-chat-mcp";

export type GraphContractVersion = string;
export type AgentId = string;
export type NodeId = string;
export type BindingId = string;
export type RuntimeAdapterId = string;
export type RuntimeSourceId = string;

export type ParameterType = "string" | "number" | "boolean" | "object" | "array" | "null";

export interface StringParameterDescriptor {
  type: "string";
  required: boolean;
  default?: string;
  description?: string;
  mutable_in_ui?: boolean;
}

export interface NumberParameterDescriptor {
  type: "number";
  required: boolean;
  default?: number;
  description?: string;
  mutable_in_ui?: boolean;
}

export interface BooleanParameterDescriptor {
  type: "boolean";
  required: boolean;
  default?: boolean;
  description?: string;
  mutable_in_ui?: boolean;
}

export interface ObjectParameterDescriptor {
  type: "object";
  required: boolean;
  default?: JsonObject;
  description?: string;
  mutable_in_ui?: boolean;
}

export interface ArrayParameterDescriptor {
  type: "array";
  required: boolean;
  default?: JsonValue[];
  description?: string;
  mutable_in_ui?: boolean;
}

export interface NullParameterDescriptor {
  type: "null";
  required: boolean;
  default?: null;
  description?: string;
  mutable_in_ui?: boolean;
}

export type ParameterDescriptor =
  | StringParameterDescriptor
  | NumberParameterDescriptor
  | BooleanParameterDescriptor
  | ObjectParameterDescriptor
  | ArrayParameterDescriptor
  | NullParameterDescriptor;

export type ParameterMap = Record<string, ParameterDescriptor>;
export type InitialVarsMap = Record<string, JsonValue>;

export interface SkillBindingWithCodexRef {
  id: BindingId;
  codex_ref: string;
  inline_text?: string;
  frozen?: false;
}

export interface SkillBindingWithInlineText {
  id: BindingId;
  codex_ref?: string;
  inline_text: string;
  frozen?: false;
}

export interface FrozenSkillBindingWithCodexRef {
  id: BindingId;
  codex_ref: string;
  inline_text: string;
  frozen: true;
}

export interface FrozenSkillBindingWithInlineText {
  id: BindingId;
  codex_ref?: string;
  inline_text: string;
  frozen: true;
}

export type SkillBinding =
  | SkillBindingWithCodexRef
  | SkillBindingWithInlineText
  | FrozenSkillBindingWithCodexRef
  | FrozenSkillBindingWithInlineText;

export interface McpBinding {
  id: BindingId;
  codex_ref: string;
  config?: JsonObject;
}

export interface PluginBinding {
  id: BindingId;
  codex_ref: string;
  config?: JsonObject;
}

export type MemoryScope = "agent" | "node";

export interface MemoryBinding {
  id: BindingId;
  kind: "runtime_memory";
  codex_ref: string;
  config?: JsonObject;
  scope: MemoryScope;
}

export interface RuntimeSourceBinding {
  id: RuntimeSourceId;
  runtime_adapter: RuntimeAdapterId;
  source_ref: string;
  description?: string;
}

export interface Permissions {
  profile?: string;
  allow?: string[];
  deny?: string[];
  extra?: JsonObject;
}

export interface FinalOutputPolicy {
  mode: "last_node_output" | "none";
}

export interface InteractionCommentsEnabled {
  enabled: true;
  target_node_ids: NonEmptyArray<NodeId>;
}

export interface InteractionCommentsDisabled {
  enabled: false;
  target_node_ids?: never;
}

export type InteractionComments = InteractionCommentsEnabled | InteractionCommentsDisabled;

export interface InteractionUserMcpEnabled {
  enabled: true;
  server_name?: UserChatServerName;
}

export interface InteractionUserMcpDisabled {
  enabled: false;
  server_name?: UserChatServerName;
}

export type InteractionUserMcp = InteractionUserMcpEnabled | InteractionUserMcpDisabled;

export interface InteractionPolicy {
  comments?: InteractionComments;
  user_mcp?: InteractionUserMcp;
}

export interface ChatSecretMarkersEnabled {
  enabled: true;
  open_marker?: string;
  close_marker?: string;
}

export interface ChatSecretMarkersDisabled {
  enabled: false;
  open_marker?: string;
  close_marker?: string;
}

export type ChatSecretMarkers = ChatSecretMarkersEnabled | ChatSecretMarkersDisabled;

export interface ChatPolicy {
  prefer_native_resume?: boolean;
  store_visible_messages?: boolean;
  store_context_window?: boolean;
  allow_fresh_start?: boolean;
  secret_markers?: ChatSecretMarkers;
}

export type InputReference =
  | `params.${string}`
  | `vars.${string}`
  | `node.${string}.text`
  | `node.${string}.json.${string}`
  | `event.${string}`;

export interface TextInputPart {
  type: "text";
  text: string;
  ref?: never;
}

export interface ReferenceInputPart {
  type: "ref";
  ref: InputReference;
  text?: never;
}

export type InputPart = TextInputPart | ReferenceInputPart;

export interface NodeInput {
  parts: InputPart[];
}

export interface TextOutputContract {
  mode: "text";
  schema?: never;
}

export interface JsonOutputContract {
  mode: "json";
  schema: JsonObjectSchema;
}

export type OutputContract = TextOutputContract | JsonOutputContract;
export type OutputMode = OutputContract["mode"];
export type NodeKind = "runtime_agent" | "orchestrator_agent";

export interface NodeBase {
  id: NodeId;
  title?: string;
  input: NodeInput;
  output: OutputContract;
}

export type RuntimeSourcePolicy =
  | { runtime_source_policy?: undefined; runtime_source_ids?: never }
  | { runtime_source_policy: "inherit"; runtime_source_ids?: never }
  | { runtime_source_policy: "restrict"; runtime_source_ids: NonEmptyArray<RuntimeSourceId> }
  | { runtime_source_policy: "prefer_first"; runtime_source_ids: NonEmptyArray<RuntimeSourceId> };

export interface RuntimeAgentNodeBase extends NodeBase {
  kind: "runtime_agent";
  runtime_adapter: RuntimeAdapterId;
  prompt: string;
  skill_ids?: BindingId[];
  mcp_ids?: BindingId[];
  plugin_ids?: BindingId[];
  memory_ids?: BindingId[];
  permissions?: Permissions;
  runtime_options?: JsonObject;
  agent_ref?: never;
}

export type RuntimeAgentNode = RuntimeAgentNodeBase & RuntimeSourcePolicy;

export interface OrchestratorAgentNode extends NodeBase {
  kind: "orchestrator_agent";
  agent_ref: AgentId;
  runtime_adapter?: never;
  prompt?: never;
  skill_ids?: never;
  mcp_ids?: never;
  plugin_ids?: never;
  memory_ids?: never;
  permissions?: never;
  runtime_options?: never;
  runtime_source_ids?: never;
  runtime_source_policy?: never;
}

export type AgentNode = RuntimeAgentNode | OrchestratorAgentNode;

export interface EdgeCondition {
  code: string;
}

export interface Edge {
  from: NodeId;
  to: NodeId;
  condition?: EdgeCondition;
}

export interface AgentMeta {
  id: AgentId;
  name: string;
  description?: string;
  agent_version?: string;
}

export interface InteractionPolicyContainer {
  interaction?: InteractionPolicy;
}

export interface AgentFile extends InteractionPolicyContainer {
  graph_contract_version: GraphContractVersion;
  meta: AgentMeta;
  entry_node_id: NodeId;
  params?: ParameterMap;
  initial_vars?: InitialVarsMap;
  skills?: SkillBinding[];
  mcps?: McpBinding[];
  plugins?: PluginBinding[];
  permissions?: Permissions;
  chat?: ChatPolicy;
  final_output?: FinalOutputPolicy;
  nodes: NonEmptyArray<AgentNode>;
  edges?: Edge[];
  memory_bindings?: MemoryBinding[];
  runtime_sources?: RuntimeSourceBinding[];
}
