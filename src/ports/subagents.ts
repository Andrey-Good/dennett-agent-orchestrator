import type { JsonValue } from '../core/json.js'
import type { RunStatus } from '../core/state/types.js'

export type ManagedSubagentId = string

export type ManagedSubagentRole = 'worker' | 'reviewer' | 'final_review'
export type ManagedSubagentState = 'running' | 'cancelling' | 'terminal' | 'closed'
export type ManagedSubagentCloseDisposition =
	| 'accepted_by_parent'
	| 'cancelled_by_parent'
	| 'abandoned_by_parent'
export type ManagedSubagentTerminalOutcome =
	| 'accepted'
	| 'rejected'
	| 'retryable'
	| 'review_required'
	| 'failed'
	| 'cancelled'
export type ManagedSubagentReasonCode =
	| 'invalid_launch_request'
	| 'write_set_conflict'
	| 'missing_required_context'
	| 'budget_exhausted'
	| 'parent_cancelled'
	| 'unsupported_interaction_mode'
	| 'unsupported_nested_spawn'
	| 'invalid_control_message'
	| 'child_runtime_error'
	| 'invalid_child_return'
	| 'review_findings_raised'
	| 'closed_boundary'
export type ManagedSubagentWaitMode = 'terminal_only' | 'terminal_or_update'
export type ManagedSubagentControlMessageKind =
	| 'clarify_scope'
	| 'narrow_constraints'
	| 'update_budget'
	| 'request_status'
	| 'cancel'

export type ManagedSubagentWriteResourceKind = 'file' | 'directory' | 'generic_resource'
export type ManagedSubagentWriteScope = 'exact' | 'descendants'
export type ManagedSubagentWriteAccess =
	| 'modify_existing'
	| 'create_within'
	| 'create_or_modify'
	| 'delete'

export interface ManagedSubagentWriteTarget {
	resource_kind: ManagedSubagentWriteResourceKind
	resource_ref: string
	scope: ManagedSubagentWriteScope
	access: ManagedSubagentWriteAccess
}

export interface ManagedSubagentWriteSet {
	mode: 'allow_list'
	items: [ManagedSubagentWriteTarget, ...ManagedSubagentWriteTarget[]]
}

export interface ManagedSubagentBudgetLimits {
	max_steps?: number
	max_tool_calls?: number
	max_wall_clock_seconds?: number
	max_spawn_depth?: number
	max_children?: number
	max_review_loops?: number
}

export interface ManagedSubagentValidationResult {
	validation_id: string
	status: 'passed' | 'failed' | 'not_run'
	note?: string
}

export interface ManagedSubagentFinalPayload {
	summary: string
	artifact_refs?: string[]
	validation_results?: ManagedSubagentValidationResult[]
}

export interface ManagedSubagentFinding {
	finding_id: string
	severity: 'low' | 'medium' | 'high' | 'critical'
	category: 'correctness' | 'boundary' | 'architecture' | 'validation' | 'quality'
	summary: string
	evidence_refs: [string, ...string[]]
	recommended_action: 'fix' | 'retest' | 'replan' | 'investigate'
}

export interface ManagedSubagentUpdate {
	update_kind: 'progress' | 'needs_parent_input'
	summary: string
}

export interface ManagedSubagentControlMessage {
	message_id: string
	message_kind: ManagedSubagentControlMessageKind
	payload: Record<string, JsonValue>
	created_at: string
}

export interface ManagedSubagentLineage {
	root_run_id: string
	parent_run_id: string
	parent_task_id: string
	depth: number
}

export interface ManagedSubagentTaskPackage {
	agent_ref: string
	objective: string
	input_message: string
	acceptance_criteria: [string, ...string[]]
	prohibitions: string[]
	write_set: ManagedSubagentWriteSet
	budgets?: ManagedSubagentBudgetLimits
	control_messages?: ManagedSubagentControlMessage[]
}

export interface ManagedSubagentTerminalResult {
	outcome: ManagedSubagentTerminalOutcome
	child_run_status: Extract<RunStatus, 'completed' | 'failed' | 'cancelled' | 'interrupted'>
	final_output: JsonValue | null
	final_output_mode: 'text' | 'json' | null
	final_payload: ManagedSubagentFinalPayload | null
	findings: ManagedSubagentFinding[] | null
	reason_code: ManagedSubagentReasonCode | null
	code?: string
	message?: string
}

export interface ManagedSubagentRecord {
	subagent_id: ManagedSubagentId
	run_id: string
	child_run_id: string
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

export interface ManagedSubagentLaunchRequest {
	parent_run_id: string
	parent_task_id: string
	child_role: ManagedSubagentRole
	agent_ref: string
	objective: string
	input_message: string
	acceptance_criteria: [string, ...string[]]
	prohibitions?: string[]
	write_set: ManagedSubagentWriteSet
	budgets?: ManagedSubagentBudgetLimits
}

export interface ManagedSubagentWaitRequest {
	subagent_id: ManagedSubagentId
	wait_mode: ManagedSubagentWaitMode
	timeout_ms?: number
}

export interface ManagedSubagentSendRequest {
	subagent_id: ManagedSubagentId
	message_id: string
	message_kind: ManagedSubagentControlMessageKind
	payload: Record<string, JsonValue>
}

export interface ManagedSubagentCloseRequest {
	subagent_id: ManagedSubagentId
	close_disposition: ManagedSubagentCloseDisposition
}

export interface ManagedSubagentWaitResponse {
	subagent_id: ManagedSubagentId
	state: ManagedSubagentState
	update: ManagedSubagentUpdate | null
	outcome: ManagedSubagentTerminalOutcome | null
	final_payload: ManagedSubagentFinalPayload | null
	findings: ManagedSubagentFinding[] | null
	reason_code: ManagedSubagentReasonCode | null
}

export interface ManagedSubagentSendResponse {
	subagent_id: ManagedSubagentId
	delivery_state: 'accepted' | 'rejected' | 'ignored_terminal'
	state: ManagedSubagentState
	reason_code: ManagedSubagentReasonCode | null
}

export interface ManagedSubagentCloseResponse {
	subagent_id: ManagedSubagentId
	close_status: 'closing' | 'closed' | 'already_closed' | 'rejected'
	state: 'cancelling' | 'terminal' | 'closed'
	outcome: ManagedSubagentTerminalOutcome | null
	reason_code: ManagedSubagentReasonCode | null
}

export interface ManagedSubagentPort {
	launch(request: ManagedSubagentLaunchRequest): Promise<ManagedSubagentRecord>
	wait(request: ManagedSubagentWaitRequest): Promise<ManagedSubagentWaitResponse>
	send(request: ManagedSubagentSendRequest): Promise<ManagedSubagentSendResponse>
	close(request: ManagedSubagentCloseRequest): Promise<ManagedSubagentCloseResponse>
}
