import type {
	ManagedSubagentBudgetLimits as ManagedSubagentBudgetLimitsPort,
	ManagedSubagentCloseDisposition as ManagedSubagentCloseDispositionPort,
	ManagedSubagentCloseResponse as ManagedSubagentCloseResponsePort,
	ManagedSubagentControlMessage as ManagedSubagentControlMessagePort,
	ManagedSubagentFinalPayload as ManagedSubagentFinalPayloadPort,
	ManagedSubagentFinding as ManagedSubagentFindingPort,
	ManagedSubagentId as ManagedSubagentIdPort,
	ManagedSubagentLineage as ManagedSubagentLineagePort,
	ManagedSubagentReasonCode as ManagedSubagentReasonCodePort,
	ManagedSubagentRole as ManagedSubagentRolePort,
	ManagedSubagentSendResponse as ManagedSubagentSendResponsePort,
	ManagedSubagentState as ManagedSubagentStatePort,
	ManagedSubagentTaskPackage as ManagedSubagentTaskPackagePort,
	ManagedSubagentTerminalResult as ManagedSubagentTerminalResultPort,
	ManagedSubagentUpdate as ManagedSubagentUpdatePort,
	ManagedSubagentWaitMode as ManagedSubagentWaitModePort,
	ManagedSubagentWaitResponse as ManagedSubagentWaitResponsePort,
} from '../../ports/subagents.js'
import type { NodeId, OutputMode } from '../agent-file.js'
import type { JsonObject, JsonValue } from '../json.js'

export type RunId = string
export type NodeAttemptId = string
export type NodeOutputId = string
export type ChatId = string
export type VisibleChatMessageId = string
export type AgentRevisionId = string
export type AgentRevisionKind = 'draft' | 'live' | 'historical'
export type AgentRevisionAvailabilityState = 'available' | 'missing' | 'invalid' | 'conflicted'
export type TriggerId = string
export type EventId = string
export type MemoryProviderId = string
export type MemoryProviderTransport = 'api' | 'sdk' | 'mcp'
export type MemoryProviderStatus = 'configured' | 'available' | 'error' | 'disabled'
export type MemoryProviderCapability = string
export type ManagedSubagentWaitMode = ManagedSubagentWaitModePort
export type ManagedSubagentId = ManagedSubagentIdPort
export type ManagedSubagentRole = ManagedSubagentRolePort
export type ManagedSubagentState = ManagedSubagentStatePort
export type ManagedSubagentCloseDisposition = ManagedSubagentCloseDispositionPort
export type ManagedSubagentReasonCode = ManagedSubagentReasonCodePort
export type ManagedSubagentLineage = ManagedSubagentLineagePort
export type ManagedSubagentBudgetLimits = ManagedSubagentBudgetLimitsPort
export type ManagedSubagentControlMessage = ManagedSubagentControlMessagePort
export type ManagedSubagentFinding = ManagedSubagentFindingPort
export type ManagedSubagentFinalPayload = ManagedSubagentFinalPayloadPort
export type ManagedSubagentUpdate = ManagedSubagentUpdatePort
export type ManagedSubagentWaitResponse = ManagedSubagentWaitResponsePort
export type ManagedSubagentSendResponse = ManagedSubagentSendResponsePort
export type ManagedSubagentCloseResponse = ManagedSubagentCloseResponsePort
export type ManagedSubagentTaskPackage = ManagedSubagentTaskPackagePort
export type ManagedSubagentTerminalResult = ManagedSubagentTerminalResultPort

export type RunStartKind = 'direct' | 'event' | 'explicit_resume'
export type EventDispatchStatus = 'pending' | 'dispatched' | 'failed'

export type RunStatus =
	| 'running'
	| 'waiting_for_user'
	| 'completed'
	| 'failed'
	| 'cancelled'
	| 'interrupted'

export type DurableBoundaryKind = 'node_attempt_terminal' | 'blocked_prompt_wait'

export type NodeAttemptState = 'in_progress' | 'committed_terminal' | 'blocked_wait'

export type TerminalNodeOutcome =
	| 'success'
	| 'invalid_output'
	| 'runtime_error'
	| 'cancelled'
	| 'interrupted'

export type VisibleChatMessageKind =
	| 'user_message'
	| 'agent_progress'
	| 'agent_final'
	| 'runtime_status'
	| 'blocking_prompt'

export type UserPromptReplyDeliveryStatus = 'recorded' | 'delivered_live' | 'delivery_failed'

export type NodeOutputRecord = { mode: 'text'; text: string } | { mode: 'json'; json: JsonObject }

export interface ChatPolicySnapshot {
	prefer_native_resume: boolean
	store_visible_messages: boolean
	store_context_window: boolean
	allow_fresh_start: boolean
}

export interface ChatPolicySnapshotInput {
	prefer_native_resume?: boolean
	store_visible_messages?: boolean
	store_context_window?: boolean
	allow_fresh_start?: boolean
}

export interface ChatRecord {
	chat_id: ChatId
	run_id: RunId
	resolved_revision_id: string
	policy: ChatPolicySnapshot
	created_at: string
	updated_at: string
}

export interface VisibleChatMessageRecord {
	message_id: VisibleChatMessageId
	chat_id: ChatId
	run_id: RunId
	message_sequence: number
	kind: VisibleChatMessageKind
	payload: JsonValue
	created_at: string
}

export interface RunRecord {
	run_id: RunId
	logical_agent_id: string | null
	resolved_revision_id: string
	entry_node_id: NodeId
	started_via: RunStartKind
	status: RunStatus
	params: Record<string, JsonValue>
	event: JsonObject | null
	last_attempt_sequence: number
	last_boundary_sequence: number
	created_at: string
	updated_at: string
}

export interface NodeAttemptRecord {
	attempt_id: NodeAttemptId
	run_id: RunId
	node_id: NodeId
	attempt_sequence: number
	output_mode: OutputMode
	state: NodeAttemptState
	outcome: TerminalNodeOutcome | null
	blocked_on_user_prompt: boolean
	runtime_handle: JsonValue | null
	committed_output_id: NodeOutputId | null
	resume_boundary_sequence: number | null
	started_at: string
	committed_at: string | null
}

export interface NodeOutputJournalRecord {
	output_id: NodeOutputId
	run_id: RunId
	node_id: NodeId
	attempt_id: NodeAttemptId
	output: NodeOutputRecord
	committed_at: string
	boundary_sequence: number
}

export interface PendingUserPromptRecord {
	run_id: RunId
	attempt_id: NodeAttemptId
	prompt_id: string | null
	payload: JsonValue
	request_handle: JsonValue | null
	unresolved: true
	blocks_forward_progress: true
	reply?: UserPromptReplyRecord | null
}

export interface UserPromptReplyRecord {
	reply_id: string
	run_id: RunId
	attempt_id: NodeAttemptId
	prompt_id: string | null
	payload: JsonValue
	idempotency_key: string
	delivery_status: UserPromptReplyDeliveryStatus
	delivery_error_message: string | null
	recorded_at: string
	delivered_at: string | null
}

export interface ResumeMetadataRecord {
	run_id: RunId
	resolved_revision_id: string
	native_resume_available: boolean
	local_resume_available: boolean
	last_durable_boundary_sequence: number | null
	last_durable_boundary_kind: DurableBoundaryKind | null
	last_attempt_id: NodeAttemptId | null
	pending_prompt: PendingUserPromptRecord | null
	native_session_handle: JsonValue | null
	local_context_snapshot: JsonValue | null
	updated_at: string
}

export interface AgentRecord {
	logical_agent_id: string
	live_revision_id: AgentRevisionId | null
	created_at: string
	updated_at: string
}

export interface AgentRevisionRecord {
	revision_id: AgentRevisionId
	logical_agent_id: string
	revision_kind: AgentRevisionKind
	file_path: string
	resolved_revision_id: string
	availability_state: AgentRevisionAvailabilityState
	validation_error: string | null
	validated_at: string | null
	graph_contract_version: string | null
	agent_name: string | null
	agent_description: string | null
	agent_version: string | null
	entry_node_id: string | null
	created_at: string
	updated_at: string
}

export interface AgentLifecycleStatusRecord {
	agent: AgentRecord
	live_revision: AgentRevisionRecord | null
	draft_revisions: AgentRevisionRecord[]
	revisions: AgentRevisionRecord[]
}

export interface TriggerRecord {
	trigger_id: TriggerId
	logical_agent_id: string
	trigger_ref: string
	created_at: string
	updated_at: string
}

export interface EventRecord {
	event_id: EventId
	trigger_id: TriggerId
	logical_agent_id: string
	payload: JsonValue | null
	launch_note: string | null
	dispatch_status: EventDispatchStatus
	run_id: RunId | null
	resolved_revision_id: string | null
	dispatch_error_code: string | null
	dispatch_error_message: string | null
	created_at: string
	dispatched_at: string | null
	updated_at: string
}

export interface MemoryProviderRecord {
	provider_id: MemoryProviderId
	codex_ref: string
	provider_family: string
	display_name: string | null
	transport: MemoryProviderTransport
	status: MemoryProviderStatus
	supported_capabilities: MemoryProviderCapability[]
	config: JsonObject
	status_code: string | null
	status_message: string | null
	last_checked_at: string | null
	created_at: string
	updated_at: string
}

export interface ManagedSubagentRecord {
	subagent_id: ManagedSubagentId
	run_id: RunId
	child_run_id: RunId
	child_role: ManagedSubagentRole
	child_logical_agent_id: string
	child_resolved_revision_id: string
	lineage: ManagedSubagentLineage
	task_package: ManagedSubagentTaskPackage
	state: ManagedSubagentState
	terminal_result: ManagedSubagentTerminalResult | null
	close_disposition: ManagedSubagentCloseDisposition | null
	created_at: string
	updated_at: string
	terminal_at: string | null
	closed_at: string | null
}

export interface PersistedRunSnapshot {
	run: RunRecord
	chat: ChatRecord | null
	visible_messages: VisibleChatMessageRecord[]
	attempts: NodeAttemptRecord[]
	latest_committed_outputs: NodeOutputJournalRecord[]
	current_vars: Record<string, JsonValue>
	resume: ResumeMetadataRecord
}

export interface ResumeMetadataInput {
	native_resume_available: boolean
	local_resume_available: boolean
	native_session_handle?: JsonValue | null
	local_context_snapshot?: JsonValue | null
}

export interface CreateRunInput {
	run_id?: RunId
	logical_agent_id?: string | null
	resolved_revision_id: string
	entry_node_id: NodeId
	started_via: RunStartKind
	params?: Record<string, JsonValue>
	event?: JsonObject | null
	initial_vars?: Record<string, JsonValue>
	resume?: ResumeMetadataInput
	chat?: {
		chat_id?: ChatId
		policy?: ChatPolicySnapshotInput
	}
	created_at?: string
}

export interface UpsertAgentRecordInput {
	logical_agent_id: string
	created_at?: string
	updated_at?: string
}

export interface UpsertTriggerRecordInput {
	trigger_id?: TriggerId
	logical_agent_id: string
	trigger_ref: string
	created_at?: string
	updated_at?: string
}

export interface CreateEventRecordInput {
	event_id?: EventId
	trigger_id: TriggerId
	logical_agent_id: string
	payload?: JsonValue | null
	launch_note?: string | null
	created_at?: string
}

export interface MarkEventDispatchedInput {
	event_id: EventId
	run_id: RunId
	resolved_revision_id: string
	dispatched_at?: string
}

export interface MarkEventDispatchFailedInput {
	event_id: EventId
	error_code: string
	error_message: string
	dispatched_at?: string
}

export interface UpsertMemoryProviderInput {
	provider_id?: MemoryProviderId
	codex_ref: string
	provider_family: string
	display_name?: string | null
	transport: MemoryProviderTransport
	status?: MemoryProviderStatus
	supported_capabilities?: MemoryProviderCapability[]
	config?: JsonObject
	status_code?: string | null
	status_message?: string | null
	last_checked_at?: string | null
	created_at?: string
	updated_at?: string
}

export interface UpdateMemoryProviderStatusInput {
	provider_id: MemoryProviderId
	status: MemoryProviderStatus
	status_code?: string | null
	status_message?: string | null
	last_checked_at?: string | null
	updated_at?: string
}

export interface CreateManagedSubagentRecordInput {
	subagent_id?: ManagedSubagentId
	child_run_id: RunId
	child_role: ManagedSubagentRole
	child_logical_agent_id: string
	child_resolved_revision_id: string
	lineage: ManagedSubagentLineage
	task_package: ManagedSubagentTaskPackage
	created_at?: string
	updated_at?: string
}

export interface MarkManagedSubagentTerminalInput {
	subagent_id: ManagedSubagentId
	terminal_result: ManagedSubagentTerminalResult
	terminal_at?: string
}

export interface CloseManagedSubagentInput {
	subagent_id: ManagedSubagentId
	close_disposition: ManagedSubagentCloseDisposition
	closed_at?: string
}

export interface UpsertAgentRevisionInput {
	revision_id?: AgentRevisionId
	logical_agent_id: string
	revision_kind: AgentRevisionKind
	file_path: string
	resolved_revision_id: string
	availability_state: AgentRevisionAvailabilityState
	validation_error?: string | null
	validated_at?: string | null
	graph_contract_version?: string | null
	agent_name?: string | null
	agent_description?: string | null
	agent_version?: string | null
	entry_node_id?: string | null
	created_at?: string
	updated_at?: string
}

export interface UpdateAgentRevisionStateInput {
	revision_id: AgentRevisionId
	revision_kind?: AgentRevisionKind
	file_path?: string
	resolved_revision_id?: string
	availability_state: AgentRevisionAvailabilityState
	validation_error?: string | null
	validated_at?: string | null
	graph_contract_version?: string | null
	agent_name?: string | null
	agent_description?: string | null
	agent_version?: string | null
	entry_node_id?: string | null
	updated_at?: string
}

export interface SetAgentLiveRevisionInput {
	logical_agent_id: string
	live_revision_id: AgentRevisionId | null
	updated_at?: string
}

export interface PromoteAgentRevisionInput {
	logical_agent_id: string
	previous_live_revision_id?: AgentRevisionId | null
	live_revision: UpsertAgentRevisionInput
	updated_at?: string
}

export interface EnsureChatRecordInput {
	run_id: RunId
	chat_id?: ChatId
	resolved_revision_id?: string
	policy?: ChatPolicySnapshotInput
	created_at?: string
}

export interface AppendVisibleChatMessageInput {
	run_id: RunId
	message_id?: VisibleChatMessageId
	kind: VisibleChatMessageKind
	payload: JsonValue
	created_at?: string
}

export interface RecordUserPromptReplyInput {
	run_id: RunId
	prompt_id?: string | null
	payload: JsonValue
	reply_id?: string
	recorded_at?: string
}

export interface RecordUserPromptReplyResult {
	reply: UserPromptReplyRecord
	accepted: boolean
}

export interface MarkUserPromptReplyDeliveryInput {
	run_id: RunId
	reply_id: string
	delivered_at?: string
	error_message?: string | null
}

export interface StartNodeAttemptInput {
	attempt_id?: NodeAttemptId
	run_id: RunId
	node_id: NodeId
	output_mode: OutputMode
	runtime_handle?: JsonValue | null
	started_at?: string
}

export interface CommitNodeSuccessInput {
	attempt_id: NodeAttemptId
	output: NodeOutputRecord
	vars: Record<string, JsonValue>
	run_status: Extract<RunStatus, 'running' | 'completed'>
	resume: ResumeMetadataInput
	committed_at?: string
}

export interface CommitNodeTerminalOutcomeInput {
	attempt_id: NodeAttemptId
	outcome: Exclude<TerminalNodeOutcome, 'success'>
	run_status: Extract<RunStatus, 'failed' | 'cancelled' | 'interrupted'>
	resume: ResumeMetadataInput
	committed_at?: string
}

export interface CommitBlockedAttemptInput {
	attempt_id: NodeAttemptId
	pending_prompt: {
		prompt_id?: string | null
		payload: JsonValue
		request_handle?: JsonValue | null
	}
	resume: ResumeMetadataInput
	committed_at?: string
}
