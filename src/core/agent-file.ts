import type { RuntimeAdapterId, RuntimeSourceId } from '../ports/runtime.js'
import type { JsonArray, JsonObject, JsonObjectSchema, JsonValue, NonEmptyArray } from './json.js'

export type GraphContractVersion = string
export type AgentId = string
export type NodeId = string
export type BindingId = string

export type ParameterType = 'string' | 'number' | 'boolean' | 'object' | 'array' | 'null'

export interface StringParameterConstraints {
	min_length?: number
	max_length?: number
	pattern?: string
}

export interface NumberParameterConstraints {
	minimum?: number
	maximum?: number
}

export interface ArrayParameterConstraints {
	min_items?: number
	max_items?: number
}

interface ParameterDescriptorBase<TType extends ParameterType, TValue extends JsonValue> {
	type: TType
	required: boolean
	default?: TValue
	description?: string
	mutable_in_ui?: boolean
	allowed_values?: NonEmptyArray<TValue>
}

export interface StringParameterDescriptor extends ParameterDescriptorBase<'string', string> {
	constraints?: StringParameterConstraints
}

export interface NumberParameterDescriptor extends ParameterDescriptorBase<'number', number> {
	constraints?: NumberParameterConstraints
}

export interface BooleanParameterDescriptor extends ParameterDescriptorBase<'boolean', boolean> {}

export interface ObjectParameterDescriptor extends ParameterDescriptorBase<'object', JsonObject> {}

export interface ArrayParameterDescriptor extends ParameterDescriptorBase<'array', JsonArray> {
	constraints?: ArrayParameterConstraints
}

export interface NullParameterDescriptor extends ParameterDescriptorBase<'null', null> {}

export type ParameterDescriptor =
	| StringParameterDescriptor
	| NumberParameterDescriptor
	| BooleanParameterDescriptor
	| ObjectParameterDescriptor
	| ArrayParameterDescriptor
	| NullParameterDescriptor

export type ParameterMap = Record<string, ParameterDescriptor>
export type InitialVarsMap = Record<string, JsonValue>

export interface SkillBinding {
	id: BindingId
	codex_ref?: string
	inline_text?: string
	frozen?: boolean
}

export interface McpBinding {
	id: BindingId
	codex_ref: string
	config?: JsonObject
}

export interface PluginBinding {
	id: BindingId
	codex_ref: string
	config?: JsonObject
}

export interface MemoryBinding {
	id: BindingId
	kind: 'runtime_memory'
	codex_ref: string
	config?: JsonObject
	scope: 'agent' | 'node'
}

export interface RuntimeSourceBinding {
	id: RuntimeSourceId
	runtime_adapter: RuntimeAdapterId
	source_ref: string
	description?: string
}

export interface Permissions {
	profile?: string
	allow?: string[]
	deny?: string[]
	extra?: JsonObject
}

export interface FinalOutputPolicy {
	mode: 'last_node_output' | 'none'
}

export interface InteractionPolicy {
	comments?: {
		enabled: boolean
		target_node_ids?: NonEmptyArray<NodeId>
	}
	user_mcp?: {
		enabled: boolean
		server_name?: 'orchestrator.user_chat'
	}
}

export interface ChatPolicy {
	prefer_native_resume?: boolean
	store_visible_messages?: boolean
	store_context_window?: boolean
	allow_fresh_start?: boolean
	secret_markers?: {
		enabled: boolean
		open_marker?: string
		close_marker?: string
	}
}

export type InputReference =
	| `params.${string}`
	| `vars.${string}`
	| `node.${string}.text`
	| `node.${string}.json.${string}`
	| `event.${string}`

export interface TextInputPart {
	type: 'text'
	text: string
}

export interface ReferenceInputPart {
	type: 'ref'
	ref: InputReference
}

export type InputPart = TextInputPart | ReferenceInputPart

export interface NodeInput {
	parts: InputPart[]
}

export interface TextOutputContract {
	mode: 'text'
	schema?: never
}

export interface JsonOutputContract {
	mode: 'json'
	schema: JsonObjectSchema
}

export type OutputContract = TextOutputContract | JsonOutputContract
export type OutputMode = OutputContract['mode']
export type NodeKind = 'runtime_agent' | 'orchestrator_agent'

export interface NodeBase {
	id: NodeId
	title?: string
	input: NodeInput
	output: OutputContract
}

export interface RuntimeAgentNode extends NodeBase {
	kind: 'runtime_agent'
	runtime_adapter: RuntimeAdapterId
	prompt: string
	skill_ids?: BindingId[]
	mcp_ids?: BindingId[]
	plugin_ids?: BindingId[]
	memory_ids?: BindingId[]
	permissions?: Permissions
	runtime_options?: JsonObject
	runtime_source_policy?: 'inherit' | 'restrict' | 'prefer_first'
	runtime_source_ids?: NonEmptyArray<RuntimeSourceId>
}

export interface OrchestratorAgentNode extends NodeBase {
	kind: 'orchestrator_agent'
	agent_ref: AgentId
}

export type AgentNode = RuntimeAgentNode | OrchestratorAgentNode

export interface EdgeCondition {
	code: string
}

export interface Edge {
	from: NodeId
	to: NodeId
	condition?: EdgeCondition
}

export interface AgentMeta {
	id: AgentId
	name: string
	description?: string
	agent_version?: string
}

export interface AgentFile {
	graph_contract_version: GraphContractVersion
	meta: AgentMeta
	entry_node_id: NodeId
	params?: ParameterMap
	initial_vars?: InitialVarsMap
	skills?: SkillBinding[]
	mcps?: McpBinding[]
	plugins?: PluginBinding[]
	permissions?: Permissions
	interaction?: InteractionPolicy
	chat?: ChatPolicy
	final_output?: FinalOutputPolicy
	nodes: NonEmptyArray<AgentNode>
	edges?: Edge[]
	memory_bindings?: MemoryBinding[]
	runtime_sources?: RuntimeSourceBinding[]
}
