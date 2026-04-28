import { randomUUID } from 'node:crypto'
import { isDeepStrictEqual } from 'node:util'
import type { RuntimeAdapter } from '../ports/runtime.js'
import type {
	ManagedSubagentBudgetLimits,
	ManagedSubagentCloseRequest,
	ManagedSubagentCloseResponse,
	ManagedSubagentControlMessage,
	ManagedSubagentFinalPayload,
	ManagedSubagentFinding,
	ManagedSubagentLaunchRequest,
	ManagedSubagentPort,
	ManagedSubagentReasonCode,
	ManagedSubagentRecord,
	ManagedSubagentSendRequest,
	ManagedSubagentSendResponse,
	ManagedSubagentTaskPackage,
	ManagedSubagentTerminalOutcome,
	ManagedSubagentTerminalResult,
	ManagedSubagentWaitRequest,
	ManagedSubagentWaitResponse,
	ManagedSubagentWriteSet,
} from '../ports/subagents.js'
import type { AgentFile } from './agent-file.js'
import { AgentLifecycleService } from './agent-lifecycle.js'
import { AppError } from './errors.js'
import { type RunResult, runAgentFile } from './graph-runner.js'
import type { JsonValue } from './json.js'
import type { SQLiteLocalStateStore } from './state/index.js'

export interface ManagedSubagentServiceOptions {
	state_store: SQLiteLocalStateStore
	runtime_adapter: RuntimeAdapter
	lifecycle_service?: AgentLifecycleService
}

function nowIso(): string {
	return new Date().toISOString()
}

const managedSubagentBudgetKeys = [
	'max_steps',
	'max_tool_calls',
	'max_wall_clock_seconds',
	'max_spawn_depth',
	'max_children',
	'max_review_loops',
] as const

const managedSubagentWriteSetKeys = ['mode', 'items'] as const
const managedSubagentWriteTargetKeys = ['resource_kind', 'resource_ref', 'scope', 'access'] as const

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function assertClosedObjectKeys(
	value: Record<string, unknown>,
	allowedKeys: readonly string[],
	errorCode: string,
	context: string,
): void {
	const allowedKeySet = new Set(allowedKeys)
	const unknownKeys = Object.keys(value).filter((key) => !allowedKeySet.has(key))
	if (unknownKeys.length > 0) {
		throw new AppError(
			errorCode,
			`${context} does not allow unknown keys: ${unknownKeys.join(', ')}.`,
		)
	}
}

function normalizeManagedSubagentBudgets(
	budgets: ManagedSubagentBudgetLimits | undefined,
): ManagedSubagentBudgetLimits {
	const normalized: ManagedSubagentBudgetLimits = {}
	if (budgets && isRecord(budgets)) {
		assertClosedObjectKeys(
			budgets,
			managedSubagentBudgetKeys,
			'INVALID_SUBAGENT_REQUEST',
			'Managed subagent budget object',
		)
	}
	for (const key of managedSubagentBudgetKeys) {
		const value = budgets?.[key]
		if (value === undefined) {
			continue
		}
		if (!Number.isInteger(value) || value <= 0) {
			throw new AppError(
				'INVALID_SUBAGENT_REQUEST',
				`Managed subagent budget "${key}" must be a positive integer when present.`,
			)
		}
		normalized[key] = value
	}
	return normalized
}

export function normalizeManagedSubagentWriteSet(writeSet: unknown): ManagedSubagentWriteSet {
	if (!isRecord(writeSet)) {
		throw new AppError(
			'INVALID_SUBAGENT_REQUEST',
			'Managed subagent write_set must be a JSON object.',
		)
	}
	assertClosedObjectKeys(
		writeSet,
		managedSubagentWriteSetKeys,
		'INVALID_SUBAGENT_REQUEST',
		'Managed subagent write_set',
	)
	if (writeSet.mode !== 'allow_list') {
		throw new AppError(
			'INVALID_SUBAGENT_REQUEST',
			'Managed subagent write_set must use allow_list mode.',
		)
	}
	if (!Array.isArray(writeSet.items) || writeSet.items.length === 0) {
		throw new AppError(
			'INVALID_SUBAGENT_REQUEST',
			'Managed subagent write_set must include at least one item.',
		)
	}

	const items: ManagedSubagentWriteSet['items'] = writeSet.items.map((entry, index) => {
		if (!isRecord(entry)) {
			throw new AppError(
				'INVALID_SUBAGENT_REQUEST',
				`Managed subagent write_set item ${index + 1} must be a JSON object.`,
			)
		}
		assertClosedObjectKeys(
			entry,
			managedSubagentWriteTargetKeys,
			'INVALID_SUBAGENT_REQUEST',
			`Managed subagent write_set item ${index + 1}`,
		)
		if (
			entry.resource_kind !== 'file' &&
			entry.resource_kind !== 'directory' &&
			entry.resource_kind !== 'generic_resource'
		) {
			throw new AppError(
				'INVALID_SUBAGENT_REQUEST',
				`Managed subagent write_set item ${index + 1} resource_kind must be one of: file, directory, generic_resource.`,
			)
		}
		if (typeof entry.resource_ref !== 'string' || entry.resource_ref.trim().length === 0) {
			throw new AppError(
				'INVALID_SUBAGENT_REQUEST',
				`Managed subagent write_set item ${index + 1} resource_ref must be a non-empty string.`,
			)
		}
		if (entry.scope !== 'exact' && entry.scope !== 'descendants') {
			throw new AppError(
				'INVALID_SUBAGENT_REQUEST',
				`Managed subagent write_set item ${index + 1} scope must be one of: exact, descendants.`,
			)
		}
		if (
			entry.access !== 'modify_existing' &&
			entry.access !== 'create_within' &&
			entry.access !== 'create_or_modify' &&
			entry.access !== 'delete'
		) {
			throw new AppError(
				'INVALID_SUBAGENT_REQUEST',
				`Managed subagent write_set item ${index + 1} access must be one of: modify_existing, create_within, create_or_modify, delete.`,
			)
		}
		return {
			resource_kind: entry.resource_kind,
			resource_ref: entry.resource_ref.trim(),
			scope: entry.scope,
			access: entry.access,
		}
	}) as ManagedSubagentWriteSet['items']

	return {
		mode: 'allow_list',
		items,
	}
}

function isManagedSubagentBudgetTighterOrEqual(
	current: ManagedSubagentBudgetLimits | undefined,
	next: ManagedSubagentBudgetLimits,
): boolean {
	for (const key of managedSubagentBudgetKeys) {
		const currentValue = current?.[key]
		const nextValue = next[key]
		if (nextValue === undefined) {
			continue
		}
		if (currentValue !== undefined && nextValue > currentValue) {
			return false
		}
	}
	return true
}

function isManagedSubagentRoleReviewerLike(
	role: ManagedSubagentLaunchRequest['child_role'],
): boolean {
	return role === 'reviewer' || role === 'final_review'
}

function assertManagedSubagentLaunchRequest(request: ManagedSubagentLaunchRequest): void {
	if (
		request.child_role !== 'worker' &&
		request.child_role !== 'reviewer' &&
		request.child_role !== 'final_review'
	) {
		throw new AppError(
			'UNSUPPORTED_SUBAGENT_ROLE',
			`Managed subagent role "${request.child_role}" is not implemented in the current Phase 16 slice.`,
		)
	}
	if (request.parent_run_id.trim().length === 0 || request.parent_task_id.trim().length === 0) {
		throw new AppError(
			'INVALID_SUBAGENT_REQUEST',
			'Managed subagent parent_run_id and parent_task_id must be non-empty strings.',
		)
	}
	if (request.agent_ref.trim().length === 0) {
		throw new AppError(
			'INVALID_SUBAGENT_REQUEST',
			'Managed subagent agent_ref must be a non-empty string.',
		)
	}
	if (request.objective.trim().length === 0) {
		throw new AppError(
			'INVALID_SUBAGENT_REQUEST',
			'Managed subagent objective must be a non-empty string.',
		)
	}
	if (request.input_message.trim().length === 0) {
		throw new AppError(
			'INVALID_SUBAGENT_REQUEST',
			'Managed subagent input_message must be a non-empty string.',
		)
	}
	normalizeManagedSubagentWriteSet(request.write_set)
	for (const prohibition of request.prohibitions ?? []) {
		if (prohibition.trim().length === 0) {
			throw new AppError(
				'INVALID_SUBAGENT_REQUEST',
				'Managed subagent prohibitions must be non-empty strings when present.',
			)
		}
	}
	normalizeManagedSubagentBudgets(request.budgets)
}

function assertChildLaunchCompatibility(agentFile: AgentFile): void {
	if (agentFile.interaction?.comments?.enabled === true) {
		throw new AppError(
			'UNSUPPORTED_INTERACTION',
			'Managed subagents cannot surface interaction.comments through the parent boundary in the current slice.',
		)
	}

	if (agentFile.interaction?.user_mcp?.enabled === true) {
		throw new AppError(
			'UNSUPPORTED_INTERACTION',
			'Managed subagents cannot surface interaction.user_mcp through the parent boundary in the current slice.',
		)
	}
}

function normalizeManagedSubagentFinalPayload(
	value: JsonValue | null,
): ManagedSubagentFinalPayload | null {
	if (!isRecord(value)) {
		if (typeof value === 'string' && value.trim().length > 0) {
			return { summary: value }
		}
		return null
	}

	const summary = value.summary
	if (typeof summary !== 'string' || summary.trim().length === 0) {
		return null
	}

	const payload: ManagedSubagentFinalPayload = {
		summary,
	}

	if (
		Array.isArray(value.artifact_refs) &&
		value.artifact_refs.every((entry) => typeof entry === 'string')
	) {
		payload.artifact_refs = value.artifact_refs
	}

	if (Array.isArray(value.validation_results)) {
		const validation_results: NonNullable<ManagedSubagentFinalPayload['validation_results']> = []
		for (const entry of value.validation_results) {
			if (!isRecord(entry)) {
				return null
			}
			const validationId = entry.validation_id
			const status = entry.status
			if (
				typeof validationId !== 'string' ||
				validationId.trim().length === 0 ||
				(status !== 'passed' && status !== 'failed' && status !== 'not_run')
			) {
				return null
			}
			const note = entry.note
			validation_results.push({
				validation_id: validationId,
				status,
				...(typeof note === 'string' ? { note } : {}),
			})
		}
		if (validation_results.length > 0) {
			payload.validation_results = validation_results
		}
	}

	return payload
}

function normalizeManagedSubagentFindings(
	value: JsonValue | null,
): ManagedSubagentFinding[] | null {
	if (!Array.isArray(value)) {
		return null
	}

	const findings: ManagedSubagentFinding[] = []
	for (const entry of value) {
		if (!isRecord(entry)) {
			return null
		}
		const findingId = entry.finding_id
		const severity = entry.severity
		const category = entry.category
		const summary = entry.summary
		const evidenceRefs = entry.evidence_refs
		const recommendedAction = entry.recommended_action
		if (
			typeof findingId !== 'string' ||
			findingId.trim().length === 0 ||
			typeof summary !== 'string' ||
			summary.trim().length === 0 ||
			!Array.isArray(evidenceRefs) ||
			evidenceRefs.length === 0 ||
			evidenceRefs.some((ref) => typeof ref !== 'string' || ref.trim().length === 0) ||
			(severity !== 'low' &&
				severity !== 'medium' &&
				severity !== 'high' &&
				severity !== 'critical') ||
			(category !== 'correctness' &&
				category !== 'boundary' &&
				category !== 'architecture' &&
				category !== 'validation' &&
				category !== 'quality') ||
			(recommendedAction !== 'fix' &&
				recommendedAction !== 'retest' &&
				recommendedAction !== 'replan' &&
				recommendedAction !== 'investigate')
		) {
			return null
		}
		findings.push({
			finding_id: findingId,
			severity,
			category,
			summary,
			evidence_refs: evidenceRefs as [string, ...string[]],
			recommended_action: recommendedAction,
		})
	}

	return findings
}

function normalizeManagedSubagentReasonCode(
	value: JsonValue | undefined,
): ManagedSubagentReasonCode | null {
	if (
		value === 'invalid_launch_request' ||
		value === 'write_set_conflict' ||
		value === 'missing_required_context' ||
		value === 'budget_exhausted' ||
		value === 'parent_cancelled' ||
		value === 'unsupported_interaction_mode' ||
		value === 'unsupported_nested_spawn' ||
		value === 'invalid_control_message' ||
		value === 'child_runtime_error' ||
		value === 'invalid_child_return' ||
		value === 'review_findings_raised' ||
		value === 'closed_boundary'
	) {
		return value
	}

	return null
}

function deriveManagedSubagentFinalResult(
	result: RunResult,
	childRole: ManagedSubagentRecord['child_role'],
): ManagedSubagentTerminalResult {
	if (result.status === 'success') {
		const finalOutputPayload = normalizeManagedSubagentFinalPayload(result.final_output)
		const findings = isManagedSubagentRoleReviewerLike(childRole)
			? normalizeManagedSubagentFindings(
					isRecord(result.final_output)
						? ((result.final_output.findings as JsonValue | undefined) ?? null)
						: null,
				)
			: null
		const hasFindings = findings !== null && findings.length > 0
		const outcome: ManagedSubagentTerminalOutcome = hasFindings ? 'review_required' : 'accepted'
		return {
			outcome,
			child_run_status: 'completed',
			final_output: result.final_output,
			final_output_mode: result.final_output_mode,
			final_payload: hasFindings ? finalOutputPayload : finalOutputPayload,
			findings: hasFindings ? findings : null,
			reason_code: hasFindings ? 'review_findings_raised' : null,
		}
	}

	if (result.status === 'waiting_for_user') {
		return {
			outcome: 'failed',
			child_run_status: 'failed',
			final_output: null,
			final_output_mode: null,
			final_payload: null,
			findings: null,
			reason_code: 'unsupported_interaction_mode',
			code: result.code,
			message: result.message,
		}
	}

	return {
		outcome: result.run_status === 'cancelled' ? 'cancelled' : 'failed',
		child_run_status: result.run_status,
		final_output: null,
		final_output_mode: null,
		final_payload: null,
		findings: null,
		reason_code: normalizeManagedSubagentReasonCode(
			result.run_status === 'cancelled' ? 'parent_cancelled' : 'child_runtime_error',
		),
		code: result.code,
		message: result.message,
	}
}

function buildManagedSubagentWaitResponse(
	record: ManagedSubagentRecord,
	waitMode: ManagedSubagentWaitRequest['wait_mode'],
): ManagedSubagentWaitResponse {
	if (record.state === 'terminal' || record.state === 'closed') {
		return {
			subagent_id: record.subagent_id,
			state: record.state,
			update: null,
			outcome: record.terminal_result?.outcome ?? null,
			final_payload:
				record.terminal_result?.final_payload ??
				normalizeManagedSubagentFinalPayload(record.terminal_result?.final_output ?? null),
			findings: record.terminal_result?.findings ?? null,
			reason_code: record.terminal_result?.reason_code ?? null,
		}
	}

	return {
		subagent_id: record.subagent_id,
		state: record.state,
		update:
			waitMode === 'terminal_or_update'
				? {
						update_kind: record.state === 'cancelling' ? 'needs_parent_input' : 'progress',
						summary:
							record.state === 'cancelling'
								? 'Cancellation is in progress and the child has not reached a terminal state yet.'
								: `Managed ${record.child_role} child is still running.`,
					}
				: null,
		outcome: null,
		final_payload: null,
		findings: null,
		reason_code: null,
	}
}

function buildManagedSubagentCloseResponse(
	record: ManagedSubagentRecord,
	closeStatus: ManagedSubagentCloseResponse['close_status'],
): ManagedSubagentCloseResponse {
	return {
		subagent_id: record.subagent_id,
		close_status: closeStatus,
		state: record.state as ManagedSubagentCloseResponse['state'],
		outcome: record.terminal_result?.outcome ?? null,
		reason_code: record.terminal_result?.reason_code ?? null,
	}
}

function isSameManagedControlMessageIntent(
	message: ManagedSubagentControlMessage,
	request: ManagedSubagentSendRequest,
): boolean {
	return (
		message.message_kind === request.message_kind &&
		isDeepStrictEqual(message.payload, request.payload)
	)
}

export class ManagedSubagentService implements ManagedSubagentPort {
	private readonly stateStore: SQLiteLocalStateStore
	private readonly runtimeAdapter: RuntimeAdapter
	private readonly lifecycleService: AgentLifecycleService
	private readonly liveExecutions = new Map<string, Promise<void>>()

	constructor(options: ManagedSubagentServiceOptions) {
		this.stateStore = options.state_store
		this.runtimeAdapter = options.runtime_adapter
		this.lifecycleService =
			options.lifecycle_service ?? new AgentLifecycleService({ state_store: options.state_store })
	}

	async launch(request: ManagedSubagentLaunchRequest): Promise<ManagedSubagentRecord> {
		assertManagedSubagentLaunchRequest(request)
		const resolvedChild = await this.lifecycleService.resolveLiveAgentFile(request.agent_ref)
		assertChildLaunchCompatibility(resolvedChild.agent_file)
		this.assertLaunchBudgetCaps(request)
		const writeSet = normalizeManagedSubagentWriteSet(request.write_set)

		const timestamp = nowIso()
		const taskPackage: ManagedSubagentTaskPackage = {
			agent_ref: request.agent_ref,
			objective: request.objective.trim(),
			input_message: request.input_message,
			acceptance_criteria: request.acceptance_criteria,
			prohibitions: request.prohibitions ?? [],
			write_set: writeSet,
			budgets: normalizeManagedSubagentBudgets(request.budgets),
			control_messages: [],
		}
		const subagent = this.stateStore.createManagedSubagentRecord({
			subagent_id: randomUUID(),
			child_run_id: randomUUID(),
			child_role: request.child_role,
			child_logical_agent_id: resolvedChild.logical_agent_id,
			child_resolved_revision_id: resolvedChild.resolved_revision_id,
			lineage: {
				root_run_id: request.parent_run_id,
				parent_run_id: request.parent_run_id,
				parent_task_id: request.parent_task_id,
				depth: 1,
			},
			task_package: taskPackage,
			created_at: timestamp,
			updated_at: timestamp,
		})

		const execution = this.executeManagedChild(subagent.subagent_id, resolvedChild)
		this.liveExecutions.set(subagent.subagent_id, execution)
		void execution.finally(() => {
			this.liveExecutions.delete(subagent.subagent_id)
		})

		return subagent
	}

	async wait(request: ManagedSubagentWaitRequest): Promise<ManagedSubagentWaitResponse> {
		const current = this.stateStore.getManagedSubagentRecord(request.subagent_id)
		if (!current) {
			throw new AppError(
				'SUBAGENT_NOT_FOUND',
				`Managed subagent "${request.subagent_id}" does not exist.`,
			)
		}
		if (current.state === 'terminal' || current.state === 'closed') {
			return buildManagedSubagentWaitResponse(current, request.wait_mode)
		}

		const liveExecution = this.liveExecutions.get(request.subagent_id)
		if (liveExecution) {
			if (request.timeout_ms !== undefined) {
				await Promise.race([
					liveExecution,
					new Promise<void>((resolve) => setTimeout(resolve, request.timeout_ms)),
				])
			} else {
				await liveExecution
			}
			const next = this.stateStore.getManagedSubagentRecord(request.subagent_id) ?? current
			return buildManagedSubagentWaitResponse(next, request.wait_mode)
		}

		const childSnapshot = this.stateStore.getPersistedRunSnapshot(current.child_run_id)
		if (!childSnapshot) {
			return buildManagedSubagentWaitResponse(current, request.wait_mode)
		}
		if (
			childSnapshot.run.status === 'completed' ||
			childSnapshot.run.status === 'failed' ||
			childSnapshot.run.status === 'cancelled' ||
			childSnapshot.run.status === 'interrupted'
		) {
			const terminalResult =
				current.state === 'cancelling'
					? this.buildCancelledByParentTerminalResult(current)
					: this.deriveTerminalResultFromChildRun(current.child_run_id, current.child_role)
			return buildManagedSubagentWaitResponse(
				this.stateStore.markManagedSubagentTerminal({
					subagent_id: current.subagent_id,
					terminal_result: terminalResult,
				}),
				request.wait_mode,
			)
		}

		return buildManagedSubagentWaitResponse(current, request.wait_mode)
	}

	async send(request: ManagedSubagentSendRequest): Promise<ManagedSubagentSendResponse> {
		const current = this.stateStore.getManagedSubagentRecord(request.subagent_id)
		if (!current) {
			throw new AppError(
				'SUBAGENT_NOT_FOUND',
				`Managed subagent "${request.subagent_id}" does not exist.`,
			)
		}
		const existingControlMessage = current.task_package.control_messages?.find(
			(message) => message.message_id === request.message_id,
		)
		if (existingControlMessage) {
			if (!isSameManagedControlMessageIntent(existingControlMessage, request)) {
				return this.rejectManagedControlMessage(current, 'invalid_control_message')
			}
			return {
				subagent_id: current.subagent_id,
				delivery_state: 'accepted',
				state: current.state,
				reason_code: null,
			}
		}
		if (current.state === 'closed' || current.state === 'terminal') {
			return {
				subagent_id: current.subagent_id,
				delivery_state: 'ignored_terminal',
				state: current.state,
				reason_code: null,
			}
		}

		const timestamp = nowIso()
		const taskPackage = current.task_package
		const controlMessage: ManagedSubagentControlMessage = {
			message_id: request.message_id,
			message_kind: request.message_kind,
			payload: request.payload,
			created_at: timestamp,
		}

		try {
			switch (request.message_kind) {
				case 'request_status': {
					assertClosedObjectKeys(
						request.payload,
						[],
						'INVALID_SUBAGENT_REQUEST',
						'Managed subagent request_status payload',
					)
					if (Object.keys(request.payload).length !== 0) {
						return this.rejectManagedControlMessage(current, 'invalid_control_message')
					}
					this.persistManagedControlMessage(current, controlMessage, taskPackage, timestamp)
					return {
						subagent_id: current.subagent_id,
						delivery_state: 'accepted',
						state: current.state,
						reason_code: null,
					}
				}
				case 'clarify_scope': {
					assertClosedObjectKeys(
						request.payload,
						['summary', 'references'],
						'INVALID_SUBAGENT_REQUEST',
						'Managed subagent clarify_scope payload',
					)
					if (
						request.payload.summary !== undefined &&
						typeof request.payload.summary !== 'string'
					) {
						return this.rejectManagedControlMessage(current, 'invalid_control_message')
					}
					if (
						request.payload.references !== undefined &&
						(!Array.isArray(request.payload.references) ||
							request.payload.references.some((entry) => typeof entry !== 'string'))
					) {
						return this.rejectManagedControlMessage(current, 'invalid_control_message')
					}
					this.persistManagedControlMessage(current, controlMessage, taskPackage, timestamp)
					return {
						subagent_id: current.subagent_id,
						delivery_state: 'accepted',
						state: current.state,
						reason_code: null,
					}
				}
				case 'narrow_constraints': {
					assertClosedObjectKeys(
						request.payload,
						['prohibitions'],
						'INVALID_SUBAGENT_REQUEST',
						'Managed subagent narrow_constraints payload',
					)
					if (
						request.payload.write_set !== undefined ||
						request.payload.read_context !== undefined
					) {
						return this.rejectManagedControlMessage(current, 'invalid_control_message')
					}
					const nextTaskPackage: ManagedSubagentTaskPackage = {
						...taskPackage,
						prohibitions: [
							...taskPackage.prohibitions,
							...this.readStringArray(request.payload.prohibitions),
						],
					}
					this.persistManagedControlMessage(current, controlMessage, nextTaskPackage, timestamp)
					return {
						subagent_id: current.subagent_id,
						delivery_state: 'accepted',
						state: current.state,
						reason_code: null,
					}
				}
				case 'update_budget': {
					assertClosedObjectKeys(
						request.payload,
						['budgets'],
						'INVALID_SUBAGENT_REQUEST',
						'Managed subagent update_budget payload',
					)
					if (!isRecord(request.payload.budgets)) {
						return this.rejectManagedControlMessage(current, 'invalid_control_message')
					}
					const nextBudgets = normalizeManagedSubagentBudgets(
						request.payload.budgets as ManagedSubagentBudgetLimits,
					)
					const mergedBudgets = {
						...(taskPackage.budgets ?? {}),
						...nextBudgets,
					}
					if (!isManagedSubagentBudgetTighterOrEqual(taskPackage.budgets, mergedBudgets)) {
						return this.rejectManagedControlMessage(current, 'budget_exhausted')
					}
					if (!this.isBudgetEnvelopeStillValid(current, mergedBudgets)) {
						return this.rejectManagedControlMessage(current, 'budget_exhausted')
					}
					this.persistManagedControlMessage(
						current,
						controlMessage,
						{
							...taskPackage,
							budgets: mergedBudgets,
						},
						timestamp,
					)
					return {
						subagent_id: current.subagent_id,
						delivery_state: 'accepted',
						state: current.state,
						reason_code: null,
					}
				}
				case 'cancel': {
					assertClosedObjectKeys(
						request.payload,
						['reason'],
						'INVALID_SUBAGENT_REQUEST',
						'Managed subagent cancel payload',
					)
					if (request.payload.reason !== undefined && typeof request.payload.reason !== 'string') {
						return this.rejectManagedControlMessage(current, 'invalid_control_message')
					}
					this.persistManagedControlMessage(current, controlMessage, taskPackage, timestamp)
					this.stateStore.markManagedSubagentCancelling({
						subagent_id: current.subagent_id,
						close_disposition: 'cancelled_by_parent',
						updated_at: timestamp,
					})
					return {
						subagent_id: current.subagent_id,
						delivery_state: 'accepted',
						state: 'cancelling',
						reason_code: null,
					}
				}
			}
		} catch (error) {
			if (error instanceof AppError) {
				return this.rejectManagedControlMessage(
					current,
					error.code === 'SUBAGENT_BUDGET_EXHAUSTED'
						? 'budget_exhausted'
						: 'invalid_control_message',
				)
			}
			throw error
		}
	}

	async close(request: ManagedSubagentCloseRequest): Promise<ManagedSubagentCloseResponse> {
		const current = this.stateStore.getManagedSubagentRecord(request.subagent_id)
		if (!current) {
			throw new AppError(
				'SUBAGENT_NOT_FOUND',
				`Managed subagent "${request.subagent_id}" does not exist.`,
			)
		}
		if (current.state === 'closed') {
			return buildManagedSubagentCloseResponse(current, 'already_closed')
		}

		if (request.close_disposition === 'cancelled_by_parent') {
			if (current.state !== 'terminal') {
				const cancelling = this.stateStore.markManagedSubagentCancelling({
					subagent_id: current.subagent_id,
					close_disposition: 'cancelled_by_parent',
				})
				return buildManagedSubagentCloseResponse(cancelling, 'closing')
			}

			if (current.terminal_result?.outcome !== 'cancelled') {
				throw new AppError(
					'SUBAGENT_CLOSE_INVALID',
					`Managed subagent "${request.subagent_id}" cannot be cancelled by parent because its terminal outcome is "${current.terminal_result?.outcome ?? 'unknown'}".`,
				)
			}
		}

		const closed = this.stateStore.closeManagedSubagent({
			subagent_id: request.subagent_id,
			close_disposition: request.close_disposition,
		})
		return buildManagedSubagentCloseResponse(closed, 'closed')
	}

	private assertLaunchBudgetCaps(request: ManagedSubagentLaunchRequest): void {
		const budgets = normalizeManagedSubagentBudgets(request.budgets)
		if (budgets.max_spawn_depth !== undefined && budgets.max_spawn_depth < 1) {
			throw new AppError(
				'INVALID_SUBAGENT_REQUEST',
				'Managed subagent max_spawn_depth must be a positive integer.',
			)
		}

		if (budgets.max_children !== undefined) {
			const activeSiblingCount = this.countOutstandingSiblingChildren(request.parent_run_id)
			if (activeSiblingCount >= budgets.max_children) {
				throw new AppError(
					'SUBAGENT_BUDGET_EXHAUSTED',
					`Managed subagent launch would exceed the max_children budget for parent run "${request.parent_run_id}".`,
				)
			}
		}

		if (
			isManagedSubagentRoleReviewerLike(request.child_role) &&
			budgets.max_review_loops !== undefined
		) {
			const launchedReviews = this.countReviewLoopLaunches(request.parent_task_id)
			if (launchedReviews >= budgets.max_review_loops) {
				throw new AppError(
					'SUBAGENT_BUDGET_EXHAUSTED',
					`Managed subagent launch would exceed the max_review_loops budget for parent task "${request.parent_task_id}".`,
				)
			}
		}
	}

	private isBudgetEnvelopeStillValid(
		record: ManagedSubagentRecord,
		nextBudgets: ManagedSubagentBudgetLimits,
	): boolean {
		if (
			nextBudgets.max_spawn_depth !== undefined &&
			nextBudgets.max_spawn_depth < record.lineage.depth
		) {
			return false
		}
		if (nextBudgets.max_children !== undefined) {
			const activeSiblingCount = this.countOutstandingSiblingChildren(record.lineage.parent_run_id)
			if (activeSiblingCount > nextBudgets.max_children) {
				return false
			}
		}
		if (
			nextBudgets.max_review_loops !== undefined &&
			isManagedSubagentRoleReviewerLike(record.child_role)
		) {
			const launchedReviews = this.countReviewLoopLaunches(record.lineage.parent_task_id)
			if (launchedReviews > nextBudgets.max_review_loops) {
				return false
			}
		}
		return true
	}

	private persistManagedControlMessage(
		record: ManagedSubagentRecord,
		message: ManagedSubagentControlMessage,
		taskPackage: ManagedSubagentTaskPackage,
		timestamp: string,
	): ManagedSubagentRecord {
		return this.stateStore.updateManagedSubagentTaskPackage({
			subagent_id: record.subagent_id,
			task_package: {
				...taskPackage,
				control_messages: [...(taskPackage.control_messages ?? []), message],
			},
			updated_at: timestamp,
		})
	}

	private rejectManagedControlMessage(
		record: ManagedSubagentRecord,
		reasonCode: ManagedSubagentReasonCode,
	): ManagedSubagentSendResponse {
		return {
			subagent_id: record.subagent_id,
			delivery_state: 'rejected',
			state: record.state,
			reason_code: reasonCode,
		}
	}

	private readStringArray(value: JsonValue | undefined): string[] {
		if (value === undefined) {
			return []
		}
		if (
			!Array.isArray(value) ||
			value.some((item) => typeof item !== 'string' || item.trim().length === 0)
		) {
			throw new AppError(
				'INVALID_SUBAGENT_REQUEST',
				'Managed subagent control payload must use non-empty string arrays.',
			)
		}
		return value as string[]
	}

	private countOutstandingSiblingChildren(parentRunId: string): number {
		return this.stateStore
			.listManagedSubagentRecords({ parent_run_id: parentRunId })
			.filter((record) => record.state !== 'closed').length
	}

	private countReviewLoopLaunches(parentTaskId: string): number {
		return this.stateStore
			.listManagedSubagentRecords({ parent_task_id: parentTaskId })
			.filter((record) =>
				isManagedSubagentRoleReviewerLike(
					record.child_role as ManagedSubagentLaunchRequest['child_role'],
				),
			).length
	}

	private async executeManagedChild(
		subagentId: string,
		resolvedChild: Awaited<ReturnType<AgentLifecycleService['resolveLiveAgentFile']>>,
	): Promise<void> {
		const subagent = this.stateStore.getManagedSubagentRecord(subagentId)
		if (!subagent) {
			return
		}

		try {
			const result = await runAgentFile(
				resolvedChild.agent_file,
				this.runtimeAdapter,
				{
					input: subagent.task_package.input_message,
				},
				{
					state_store: this.stateStore,
					resolved_revision_id: resolvedChild.resolved_revision_id,
					logical_agent_id: resolvedChild.logical_agent_id,
					run_id: subagent.child_run_id,
					started_via: 'direct',
				},
			)
			const current = this.stateStore.getManagedSubagentRecord(subagentId)

			this.stateStore.markManagedSubagentTerminal({
				subagent_id: subagent.subagent_id,
				terminal_result:
					current?.state === 'cancelling'
						? this.buildCancelledByParentTerminalResult(subagent)
						: deriveManagedSubagentFinalResult(result, subagent.child_role),
			})
		} catch (error) {
			this.stateStore.markManagedSubagentTerminal({
				subagent_id: subagent.subagent_id,
				terminal_result: {
					outcome: 'failed',
					child_run_status: 'failed',
					final_output: null,
					final_output_mode: null,
					final_payload: null,
					findings: null,
					reason_code: 'child_runtime_error',
					code: error instanceof AppError ? error.code : 'SUBAGENT_EXECUTION_FAILED',
					message:
						error instanceof Error ? error.message : 'Unknown managed subagent execution failure.',
				},
			})
		}
	}

	private buildCancelledByParentTerminalResult(
		subagent: ManagedSubagentRecord,
	): ManagedSubagentTerminalResult {
		return {
			outcome: 'cancelled',
			child_run_status: 'cancelled',
			final_output: null,
			final_output_mode: null,
			final_payload: null,
			findings: null,
			reason_code: 'parent_cancelled',
			code: 'PARENT_CANCELLED',
			message: `Managed subagent "${subagent.subagent_id}" was cancelled by the parent boundary.`,
		}
	}

	private deriveTerminalResultFromChildRun(
		childRunId: string,
		childRole: ManagedSubagentRecord['child_role'],
	): ManagedSubagentTerminalResult {
		const snapshot = this.stateStore.getPersistedRunSnapshot(childRunId)
		if (!snapshot) {
			throw new AppError(
				'RUN_NOT_FOUND',
				`Child run "${childRunId}" does not exist for managed subagent reconciliation.`,
			)
		}

		const latestAttempt = snapshot.attempts.at(-1)
		const latestOutput = snapshot.latest_committed_outputs.at(-1)?.output
		if (snapshot.run.status === 'completed') {
			const outputValue =
				latestOutput?.mode === 'text'
					? latestOutput.text
					: latestOutput?.mode === 'json'
						? latestOutput.json
						: null
			return {
				outcome:
					isManagedSubagentRoleReviewerLike(childRole) &&
					isRecord(outputValue) &&
					Array.isArray(outputValue.findings) &&
					outputValue.findings.length > 0
						? 'review_required'
						: 'accepted',
				child_run_status: 'completed',
				final_output: outputValue,
				final_output_mode: latestOutput?.mode ?? null,
				final_payload: normalizeManagedSubagentFinalPayload(outputValue),
				findings:
					isManagedSubagentRoleReviewerLike(childRole) && isRecord(outputValue)
						? normalizeManagedSubagentFindings(
								(outputValue.findings as JsonValue | undefined) ?? null,
							)
						: null,
				reason_code:
					isManagedSubagentRoleReviewerLike(childRole) &&
					isRecord(outputValue) &&
					Array.isArray(outputValue.findings) &&
					outputValue.findings.length > 0
						? 'review_findings_raised'
						: null,
			}
		}

		return {
			outcome: snapshot.run.status === 'cancelled' ? 'cancelled' : 'failed',
			child_run_status:
				snapshot.run.status === 'running' || snapshot.run.status === 'waiting_for_user'
					? 'failed'
					: snapshot.run.status,
			final_output: null,
			final_output_mode: null,
			final_payload: null,
			findings: null,
			reason_code:
				snapshot.run.status === 'waiting_for_user'
					? 'unsupported_interaction_mode'
					: snapshot.run.status === 'cancelled'
						? 'parent_cancelled'
						: latestAttempt?.outcome === 'invalid_output'
							? 'invalid_child_return'
							: 'child_runtime_error',
			code:
				latestAttempt?.outcome === 'invalid_output'
					? 'INVALID_OUTPUT'
					: latestAttempt?.outcome === 'cancelled'
						? 'CANCELLED'
						: latestAttempt?.outcome === 'interrupted'
							? 'INTERRUPTED'
							: 'RUNTIME_ERROR',
			message: `Child run "${childRunId}" completed managed reconciliation in state "${snapshot.run.status}".`,
		}
	}
}
