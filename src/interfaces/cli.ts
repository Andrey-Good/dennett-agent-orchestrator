#!/usr/bin/env node
import { createHash, randomUUID } from 'node:crypto'
import path from 'node:path'
import process from 'node:process'
import { pathToFileURL } from 'node:url'
import { isDeepStrictEqual } from 'node:util'
import { Command } from 'commander'
import {
	CodexAppServerRuntimeAdapter,
	type CodexAppServerRuntimeAdapterOptions,
} from '../adapters/codex/codex-app-server-runtime-adapter.js'
import { AgentLifecycleService } from '../core/agent-lifecycle.js'
import { BuilderAgentService } from '../core/builder-service.js'
import { AppError, isAppError } from '../core/errors.js'
import { resumeAgentRun, runAgentFile } from '../core/graph-runner.js'
import type { JsonObject, JsonValue } from '../core/json.js'
import { MemoryProviderRegistryService } from '../core/memory-provider-registry.js'
import { MemoryService } from '../core/memory-service.js'
import { computeResolvedRevisionId } from '../core/resolved-revision.js'
import { loadAndValidateAgentFile } from '../core/schema.js'
import type {
	AgentLifecycleStatusRecord,
	ManagedSubagentCloseDisposition,
	ManagedSubagentRecord,
	ManagedSubagentState,
	MemoryProviderCapability,
	MemoryProviderRecord,
	MemoryProviderTransport,
	PersistedRunSnapshot,
} from '../core/state/types.js'
import {
	ManagedSubagentService,
	normalizeManagedSubagentWriteSet,
} from '../core/subagent-service.js'
import type {
	MemoryCleanupPreviewResult,
	MemoryScope,
	MemoryVerifiedCleanupResult,
} from '../ports/memory.js'
import type {
	RuntimeAdapter,
	RuntimeEnvironmentInspectionResult,
	RuntimeModelCatalogPage,
} from '../ports/runtime.js'
import type {
	ManagedSubagentControlMessageKind,
	ManagedSubagentLaunchRequest,
	ManagedSubagentWaitMode,
} from '../ports/subagents.js'

type SQLiteLocalStateStore = import('../core/state/index.js').SQLiteLocalStateStore
type SQLiteLocalStateStoreConstructor =
	typeof import('../core/state/index.js').SQLiteLocalStateStore

let sqliteLocalStateStoreConstructor: SQLiteLocalStateStoreConstructor | null = null

async function getSQLiteLocalStateStoreConstructor(): Promise<SQLiteLocalStateStoreConstructor> {
	if (sqliteLocalStateStoreConstructor === null) {
		const stateModule = await import('../core/state/index.js')
		sqliteLocalStateStoreConstructor = stateModule.SQLiteLocalStateStore
	}
	return sqliteLocalStateStoreConstructor
}

function parseParamValue(rawValue: string): JsonValue {
	const trimmed = rawValue.trim()
	if (trimmed === 'true') {
		return true
	}
	if (trimmed === 'false') {
		return false
	}
	if (trimmed === 'null') {
		return null
	}
	if (trimmed !== '' && !Number.isNaN(Number(trimmed)) && String(Number(trimmed)) === trimmed) {
		return Number(trimmed)
	}
	if (
		(trimmed.startsWith('{') && trimmed.endsWith('}')) ||
		(trimmed.startsWith('[') && trimmed.endsWith(']')) ||
		(trimmed.startsWith('"') && trimmed.endsWith('"'))
	) {
		try {
			return JSON.parse(trimmed) as JsonValue
		} catch {
			return rawValue
		}
	}
	return rawValue
}

function parseParams(paramPairs: string[]): Record<string, JsonValue> {
	const params: Record<string, JsonValue> = {}
	for (const pair of paramPairs) {
		const separatorIndex = pair.indexOf('=')
		if (separatorIndex <= 0) {
			throw new Error(`Invalid --param value "${pair}". Expected key=value.`)
		}
		const key = pair.slice(0, separatorIndex).trim()
		const value = pair.slice(separatorIndex + 1)
		if (!key) {
			throw new Error(`Invalid --param value "${pair}". Parameter name is required.`)
		}
		params[key] = parseParamValue(value)
	}
	return params
}

function parseJsonObjectOption(rawValue: string, optionName: string): JsonObject {
	const parsedValue = parseParamValue(rawValue)
	if (parsedValue === null || Array.isArray(parsedValue) || typeof parsedValue !== 'object') {
		throw new AppError('INVALID_JSON_OPTION', `Option "${optionName}" must be a JSON object.`)
	}
	return parsedValue as JsonObject
}

function printFailure(code: string, message: string): void {
	process.stderr.write(`${code}: ${message}\n`)
}

function defaultStateDatabasePath(): string {
	return path.resolve(process.cwd(), '.dennett', 'local-state.sqlite')
}

function printRunId(runId: string): void {
	process.stderr.write(`Run ID: ${runId}\n`)
}

async function createStateStore(stateDbPath: string): Promise<SQLiteLocalStateStore> {
	const SQLiteLocalStateStore = await getSQLiteLocalStateStoreConstructor()
	return new SQLiteLocalStateStore({
		database_path: path.resolve(process.cwd(), stateDbPath),
	})
}

async function createLifecycleService(stateDbPath: string): Promise<{
	stateStore: SQLiteLocalStateStore
	lifecycleService: AgentLifecycleService
}> {
	const stateStore = await createStateStore(stateDbPath)
	return {
		stateStore,
		lifecycleService: new AgentLifecycleService({
			state_store: stateStore,
		}),
	}
}

async function createMemoryProviderRegistry(stateDbPath: string): Promise<{
	stateStore: SQLiteLocalStateStore
	registryService: MemoryProviderRegistryService
}> {
	const stateStore = await createStateStore(stateDbPath)
	return {
		stateStore,
		registryService: new MemoryProviderRegistryService({
			state_store: stateStore,
		}),
	}
}

async function createMemoryService(stateDbPath: string): Promise<{
	stateStore: SQLiteLocalStateStore
	memoryService: MemoryService
}> {
	const stateStore = await createStateStore(stateDbPath)
	return {
		stateStore,
		memoryService: new MemoryService({
			state_store: stateStore,
		}),
	}
}

function printJson(value: unknown): void {
	process.stdout.write(`${JSON.stringify(value, null, 2)}\n`)
}

type RedactedMemoryProviderRecord = Omit<MemoryProviderRecord, 'config'> & {
	config: {
		redacted: true
		reason: string
	}
}

function redactMemoryProviderRecord(record: MemoryProviderRecord): RedactedMemoryProviderRecord {
	return {
		...record,
		config: {
			redacted: true,
			reason: 'Provider configuration is local/private and omitted from default CLI output.',
		},
	}
}

function printLifecycleStatus(status: AgentLifecycleStatusRecord): void {
	printJson({
		agent: status.agent,
		live_revision: status.live_revision,
		draft_revisions: status.draft_revisions,
		revisions: status.revisions,
	})
}

function printMemoryProviderRecord(record: MemoryProviderRecord): void {
	printJson(redactMemoryProviderRecord(record))
}

function printMemoryProviderRecords(records: MemoryProviderRecord[]): void {
	printJson(records.map((record) => redactMemoryProviderRecord(record)))
}

function buildMemoryScope(args: {
	userId?: string
	agentId?: string
	runId?: string
}): MemoryScope {
	const scope: MemoryScope = {}
	if (args.userId?.trim()) {
		scope.user_id = args.userId.trim()
	}
	if (args.agentId?.trim()) {
		scope.agent_id = args.agentId.trim()
	}
	if (args.runId?.trim()) {
		scope.run_id = args.runId.trim()
	}
	return scope
}

function assertExplicitMemoryCleanupScope(scope: MemoryScope): void {
	if (!scope.user_id?.trim() && !scope.agent_id?.trim() && !scope.run_id?.trim()) {
		throw new AppError(
			'MEMORY_CLEANUP_SCOPE_REQUIRED',
			'Memory cleanup requires at least one explicit scope option: --user-id, --agent-id, or --run-id.',
		)
	}
}

function parsePositiveIntegerOption(
	rawValue: string | undefined,
	optionName: string,
): number | undefined {
	if (rawValue === undefined) {
		return undefined
	}
	const parsed = Number.parseInt(rawValue, 10)
	if (!Number.isSafeInteger(parsed) || parsed <= 0 || String(parsed) !== rawValue.trim()) {
		throw new AppError('INVALID_CLI_OPTION', `Option "${optionName}" must be a positive integer.`)
	}
	return parsed
}

function parseTimeoutOption(rawValue: string | undefined, optionName: string): number | undefined {
	return parsePositiveIntegerOption(rawValue, optionName)
}

function parseManagedSubagentStateOption(
	rawValue: string | undefined,
): ManagedSubagentState | undefined {
	if (rawValue === undefined) {
		return undefined
	}
	if (
		rawValue === 'running' ||
		rawValue === 'cancelling' ||
		rawValue === 'terminal' ||
		rawValue === 'closed'
	) {
		return rawValue
	}
	throw new AppError(
		'INVALID_CLI_OPTION',
		'Option "--state" must be one of: running, cancelling, terminal, closed.',
	)
}

function parseManagedSubagentRole(rawValue: string): ManagedSubagentLaunchRequest['child_role'] {
	if (rawValue === 'worker' || rawValue === 'reviewer' || rawValue === 'final_review') {
		return rawValue
	}
	throw new AppError(
		'INVALID_CLI_OPTION',
		'Option "--role" must be one of: worker, reviewer, final_review.',
	)
}

function parseManagedSubagentWaitMode(rawValue: string | undefined): ManagedSubagentWaitMode {
	if (rawValue === undefined || rawValue === 'terminal_only' || rawValue === 'terminal_or_update') {
		return rawValue ?? 'terminal_or_update'
	}
	throw new AppError(
		'INVALID_CLI_OPTION',
		'Option "--wait-mode" must be one of: terminal_only, terminal_or_update.',
	)
}

function parseManagedSubagentControlKind(rawValue: string): ManagedSubagentControlMessageKind {
	if (
		rawValue === 'clarify_scope' ||
		rawValue === 'narrow_constraints' ||
		rawValue === 'update_budget' ||
		rawValue === 'request_status' ||
		rawValue === 'cancel'
	) {
		return rawValue
	}
	throw new AppError(
		'INVALID_CLI_OPTION',
		'Option "--kind" must be one of: clarify_scope, narrow_constraints, update_budget, request_status, cancel.',
	)
}

function parseManagedSubagentCloseDisposition(rawValue: string): ManagedSubagentCloseDisposition {
	if (
		rawValue === 'accepted_by_parent' ||
		rawValue === 'cancelled_by_parent' ||
		rawValue === 'abandoned_by_parent'
	) {
		return rawValue
	}
	throw new AppError(
		'INVALID_CLI_OPTION',
		'Option "--disposition" must be one of: accepted_by_parent, cancelled_by_parent, abandoned_by_parent.',
	)
}

function parseManagedSubagentWriteSet(rawValue: string): ManagedSubagentLaunchRequest['write_set'] {
	const parsed = parseJsonObjectOption(rawValue, '--write-set')
	return normalizeManagedSubagentWriteSet(parsed)
}

function parseManagedSubagentBudgets(
	rawValue: string | undefined,
): ManagedSubagentLaunchRequest['budgets'] {
	if (rawValue === undefined) {
		return undefined
	}
	const parsed = parseJsonObjectOption(rawValue, '--budgets')
	return parsed as unknown as ManagedSubagentLaunchRequest['budgets']
}

function createCodexAppServerAdapter(
	options: CodexAppServerRuntimeAdapterOptions = {},
): CodexAppServerRuntimeAdapter {
	return new CodexAppServerRuntimeAdapter(process.cwd(), options)
}

function stableJson(value: unknown): string {
	if (Array.isArray(value)) {
		return `[${value.map((entry) => stableJson(entry)).join(',')}]`
	}
	if (value !== null && typeof value === 'object') {
		return `{${Object.entries(value as Record<string, unknown>)
			.sort(([leftKey], [rightKey]) => leftKey.localeCompare(rightKey))
			.map(([key, entry]) => `${JSON.stringify(key)}:${stableJson(entry)}`)
			.join(',')}}`
	}
	return JSON.stringify(value)
}

function buildMemoryCleanupConfirmationToken(args: {
	codexRef: string
	scope: MemoryScope
	preview: MemoryCleanupPreviewResult
}): string {
	const tokenPayload = {
		codex_ref: args.codexRef,
		scope: args.scope,
		namespace_id: args.preview.namespace_id,
		candidate_ids: args.preview.candidate_ids,
		candidate_count: args.preview.candidate_count,
		limit: args.preview.limit,
		truncated: args.preview.truncated,
	}
	return `cleanup:${createHash('sha256').update(stableJson(tokenPayload)).digest('hex').slice(0, 24)}`
}

function buildMemoryCleanupPreviewOutput(args: {
	codexRef: string
	scope: MemoryScope
	preview: MemoryCleanupPreviewResult
}) {
	return {
		namespace_id: args.preview.namespace_id,
		scope: args.scope,
		candidate_count: args.preview.candidate_count,
		candidate_ids: args.preview.candidate_ids,
		limit: args.preview.limit,
		truncated: args.preview.truncated,
		confirmation_token: buildMemoryCleanupConfirmationToken(args),
		verification: {
			status: 'preview_only',
			required_command: 'memory-cleanup-verified-delete',
		},
	}
}

function getLatestActiveAttempt(snapshot: {
	attempts: Array<{ node_id: string; state: string; runtime_handle: JsonValue | null }>
}): { node_id: string; state: string; runtime_handle: JsonValue | null } | null {
	return (
		[...snapshot.attempts]
			.reverse()
			.find((attempt) => attempt.state === 'in_progress' || attempt.state === 'blocked_wait') ??
		null
	)
}

export function resolveCommentExecutionHandle(snapshot: {
	resume: { native_session_handle: JsonValue | null }
	attempts: Array<{ node_id: string; runtime_handle: JsonValue | null; state: string }>
}): JsonValue | null {
	const activeAttempt = getLatestActiveAttempt(snapshot)
	return activeAttempt?.runtime_handle ?? snapshot.resume.native_session_handle ?? null
}

export function resolveReplyExecutionHandle(snapshot: {
	resume: {
		native_session_handle: JsonValue | null
		pending_prompt: { request_handle: JsonValue | null } | null
	}
	attempts: Array<{ node_id: string; runtime_handle: JsonValue | null; state: string }>
}): JsonValue | null {
	const pendingPrompt = snapshot.resume.pending_prompt
	if (pendingPrompt !== null && pendingPrompt.request_handle !== null) {
		return pendingPrompt.request_handle
	}
	if (snapshot.resume.native_session_handle !== null) {
		return snapshot.resume.native_session_handle
	}
	const activeAttempt = getLatestActiveAttempt(snapshot)
	return activeAttempt?.runtime_handle ?? null
}

export function buildOptionReplyPayload(args: {
	runId: string
	promptId?: string
	promptPayload: {
		kind?: string
		options?: Array<{ id?: string; value: JsonValue }>
	}
	optionId: string
	value: string
}): {
	kind: 'option'
	prompt_id?: string
	option_id: string
	value: JsonValue
} {
	if (args.promptPayload.kind !== 'options') {
		throw new AppError(
			'UNSUPPORTED_INTERACTION',
			`Run "${args.runId}" is not waiting on an options prompt.`,
		)
	}

	const matchedOption = args.promptPayload.options?.find((option) => option.id === args.optionId)
	if (!matchedOption) {
		throw new AppError(
			'UNSUPPORTED_INTERACTION',
			`Prompt "${args.promptId ?? '<unknown>'}" does not define option "${args.optionId}".`,
		)
	}

	const parsedValue = parseParamValue(args.value)
	if (!isDeepStrictEqual(parsedValue, matchedOption.value)) {
		throw new AppError(
			'INVALID_REPLY',
			`Option "${args.optionId}" must use the declared value for the selected prompt option.`,
		)
	}

	return {
		kind: 'option',
		...(args.promptId ? { prompt_id: args.promptId } : {}),
		option_id: args.optionId,
		value: matchedOption.value,
	}
}

export function buildRunInteractionStatus(snapshot: PersistedRunSnapshot) {
	const activeAttempt = getLatestActiveAttempt(snapshot)
	const pendingPrompt = snapshot.resume.pending_prompt
	const promptPayload =
		pendingPrompt?.payload !== null && typeof pendingPrompt?.payload === 'object'
			? (pendingPrompt.payload as { kind?: unknown; require_response?: unknown })
			: null
	const reply = pendingPrompt?.reply ?? null

	return {
		run: {
			run_id: snapshot.run.run_id,
			status: snapshot.run.status,
			resolved_revision_id: snapshot.run.resolved_revision_id,
			entry_node_id: snapshot.run.entry_node_id,
			last_boundary_sequence: snapshot.run.last_boundary_sequence,
		},
		active_attempt: activeAttempt
			? {
					node_id: activeAttempt.node_id,
					state: activeAttempt.state,
					has_runtime_handle: activeAttempt.runtime_handle !== null,
				}
			: null,
		interaction: {
			waiting_for_user: snapshot.run.status === 'waiting_for_user',
			pending_prompt: pendingPrompt
				? {
						prompt_id: pendingPrompt.prompt_id,
						attempt_id: pendingPrompt.attempt_id,
						kind: typeof promptPayload?.kind === 'string' ? promptPayload.kind : null,
						require_response:
							typeof promptPayload?.require_response === 'boolean'
								? promptPayload.require_response
								: null,
						has_request_handle: pendingPrompt.request_handle !== null,
						reply: reply
							? {
									reply_id: reply.reply_id,
									prompt_id: reply.prompt_id,
									delivery_status: reply.delivery_status,
									recorded_at: reply.recorded_at,
									delivered_at: reply.delivered_at,
								}
							: null,
					}
				: null,
			visible_transcript_messages: snapshot.visible_messages.length,
		},
		resume: {
			native_resume_available: snapshot.resume.native_resume_available,
			local_resume_available: snapshot.resume.local_resume_available,
			last_durable_boundary_sequence: snapshot.resume.last_durable_boundary_sequence,
			last_durable_boundary_kind: snapshot.resume.last_durable_boundary_kind,
			has_native_session_handle: snapshot.resume.native_session_handle !== null,
		},
		redaction: {
			prompt_payload_omitted: pendingPrompt !== null,
			reply_payload_omitted: reply !== null,
			reason:
				'run-status omits prompt and reply payload content; use the local state database only under the project data-retention policy.',
		},
	}
}

export function buildManagedSubagentOperatorView(record: ManagedSubagentRecord) {
	const latestControlMessage = record.task_package.control_messages?.at(-1) ?? null
	const cancellationRequested =
		record.close_disposition === 'cancelled_by_parent' ||
		latestControlMessage?.message_kind === 'cancel'

	return {
		subagent_id: record.subagent_id,
		state: record.state,
		child_role: record.child_role,
		child_run_id: record.child_run_id,
		child_agent: {
			logical_agent_id: record.child_logical_agent_id,
			resolved_revision_id: record.child_resolved_revision_id,
		},
		lineage: record.lineage,
		task: {
			agent_ref: record.task_package.agent_ref,
			objective: record.task_package.objective,
			acceptance_criteria: record.task_package.acceptance_criteria,
			prohibitions: record.task_package.prohibitions,
			write_set: record.task_package.write_set,
			budgets: record.task_package.budgets ?? {},
			control_message_count: record.task_package.control_messages?.length ?? 0,
			latest_control_message: latestControlMessage
				? {
						message_id: latestControlMessage.message_id,
						message_kind: latestControlMessage.message_kind,
						created_at: latestControlMessage.created_at,
					}
				: null,
		},
		terminal_result: record.terminal_result,
		findings: record.terminal_result?.findings ?? null,
		close_disposition: record.close_disposition,
		timestamps: {
			created_at: record.created_at,
			updated_at: record.updated_at,
			terminal_at: record.terminal_at,
			closed_at: record.closed_at,
		},
		operator_semantics: {
			write_scope_enforcement:
				'metadata_conflict_checked_not_filesystem_sandbox; sibling write-set conflicts are rejected, but filesystem writes are not sandboxed by this surface.',
			control_messages:
				'recorded_in_task_package; this CLI surface does not live-deliver messages into an already running child process.',
			cancellation: cancellationRequested
				? 'cancel_requested_in_state_not_runtime_cancel; child terminal reconciliation reports parent_cancelled, but no runtime cancel signal is claimed.'
				: 'not_requested',
		},
	}
}

export async function listManagedSubagentsForOperators(
	stateDbPath: string,
	filters: {
		parentRunId?: string
		parentTaskId?: string
		state?: ManagedSubagentState
	} = {},
) {
	const stateStore = await createStateStore(stateDbPath)
	try {
		return stateStore
			.listManagedSubagentRecords({
				parent_run_id: filters.parentRunId,
				parent_task_id: filters.parentTaskId,
				state: filters.state,
			})
			.map((record) => buildManagedSubagentOperatorView(record))
	} finally {
		stateStore.close()
	}
}

export async function showManagedSubagentForOperators(subagentId: string, stateDbPath: string) {
	const stateStore = await createStateStore(stateDbPath)
	try {
		const record = stateStore.getManagedSubagentRecord(subagentId)
		if (!record) {
			throw new AppError('SUBAGENT_NOT_FOUND', `Managed subagent "${subagentId}" does not exist.`)
		}
		return buildManagedSubagentOperatorView(record)
	} finally {
		stateStore.close()
	}
}

function createManagedSubagentCliService(
	stateStore: SQLiteLocalStateStore,
	adapter: RuntimeAdapter = createCodexAppServerAdapter(),
): ManagedSubagentService {
	return new ManagedSubagentService({
		state_store: stateStore,
		runtime_adapter: adapter,
	})
}

export async function launchManagedSubagentForOperators(
	request: ManagedSubagentLaunchRequest,
	stateDbPath: string,
	adapter: RuntimeAdapter = createCodexAppServerAdapter(),
) {
	const stateStore = await createStateStore(stateDbPath)
	try {
		const lifecycleService = new AgentLifecycleService({
			state_store: stateStore,
		})
		const service = new ManagedSubagentService({
			state_store: stateStore,
			runtime_adapter: adapter,
			lifecycle_service: lifecycleService,
		})
		const launched = await service.launch(request)
		const wait = await service.wait({
			subagent_id: launched.subagent_id,
			wait_mode: 'terminal_only',
		})
		const record = stateStore.getManagedSubagentRecord(launched.subagent_id) ?? launched
		return {
			launched: buildManagedSubagentOperatorView(launched),
			wait,
			record: buildManagedSubagentOperatorView(record),
			launch_semantics: {
				background_execution: false,
				waited_in_process: true,
				note: 'subagent-launch starts the child and waits in the same CLI process; it does not create a durable background worker.',
			},
		}
	} finally {
		stateStore.close()
	}
}

export async function waitManagedSubagentForOperators(
	subagentId: string,
	stateDbPath: string,
	options: {
		waitMode?: ManagedSubagentWaitMode
		timeoutMs?: number
	} = {},
) {
	const stateStore = await createStateStore(stateDbPath)
	try {
		const service = createManagedSubagentCliService(stateStore)
		const response = await service.wait({
			subagent_id: subagentId,
			wait_mode: options.waitMode ?? 'terminal_or_update',
		})
		const record = stateStore.getManagedSubagentRecord(subagentId)
		return {
			...response,
			record: record ? buildManagedSubagentOperatorView(record) : null,
			wait_semantics: {
				durable_reconciliation: true,
				live_execution_wait: false,
				timeout_ms_requested: options.timeoutMs ?? null,
				timeout_ms_applied: false,
				note: 'This CLI can reconcile persisted child-run terminal state; it does not attach to a live in-process subagent launched by another process.',
			},
		}
	} finally {
		stateStore.close()
	}
}

export async function recordManagedSubagentControlForOperators(
	subagentId: string,
	stateDbPath: string,
	input: {
		messageId?: string
		messageKind: ManagedSubagentControlMessageKind
		payload: JsonObject
	},
) {
	const stateStore = await createStateStore(stateDbPath)
	try {
		const beforeRecord = stateStore.getManagedSubagentRecord(subagentId)
		const existingMessage = beforeRecord?.task_package.control_messages?.find(
			(message) => message.message_id === input.messageId,
		)
		const beforeControlMessageCount = beforeRecord?.task_package.control_messages?.length ?? 0
		const service = createManagedSubagentCliService(stateStore)
		const messageId = input.messageId ?? randomUUID()
		const response = await service.send({
			subagent_id: subagentId,
			message_id: messageId,
			message_kind: input.messageKind,
			payload: input.payload,
		})
		const record = stateStore.getManagedSubagentRecord(subagentId)
		const afterControlMessageCount = record?.task_package.control_messages?.length ?? 0
		const wroteNewControlMessage = afterControlMessageCount > beforeControlMessageCount
		return {
			...response,
			record: record ? buildManagedSubagentOperatorView(record) : null,
			delivery_semantics: {
				recorded_in_state: wroteNewControlMessage,
				idempotent_replay:
					existingMessage !== undefined &&
					response.delivery_state === 'accepted' &&
					!wroteNewControlMessage,
				duplicate_id_conflict:
					existingMessage !== undefined && response.delivery_state === 'rejected',
				live_delivery: false,
				runtime_cancellation_delivered: false,
				note:
					existingMessage !== undefined && response.delivery_state === 'rejected'
						? `Control message id "${messageId}" already exists with different kind or payload; no new control message was recorded.`
						: input.messageKind === 'cancel'
							? 'Cancel is recorded and marks the managed subagent cancelling; no runtime cancellation signal is delivered by this CLI surface.'
							: 'Control messages are recorded in the managed task package for durable operator visibility; no live child delivery is claimed.',
			},
		}
	} finally {
		stateStore.close()
	}
}

export async function closeManagedSubagentForOperators(
	subagentId: string,
	stateDbPath: string,
	closeDisposition: ManagedSubagentCloseDisposition,
) {
	const stateStore = await createStateStore(stateDbPath)
	try {
		const service = createManagedSubagentCliService(stateStore)
		const response = await service.close({
			subagent_id: subagentId,
			close_disposition: closeDisposition,
		})
		const record = stateStore.getManagedSubagentRecord(subagentId)
		return {
			...response,
			record: record ? buildManagedSubagentOperatorView(record) : null,
			close_semantics: {
				runtime_cancellation_delivered: false,
				note:
					closeDisposition === 'cancelled_by_parent'
						? 'cancelled_by_parent records parent intent and marks state; this CLI surface does not claim runtime cancellation delivery.'
						: 'Close records parent disposition on the managed subagent boundary.',
			},
		}
	} finally {
		stateStore.close()
	}
}

async function loadAgentContext(agentFilePath: string): Promise<{
	agentFile: Awaited<ReturnType<typeof loadAndValidateAgentFile>>
	resolvedRevisionId: string
}> {
	const resolvedAgentFilePath = path.resolve(process.cwd(), agentFilePath)
	const [agentFile, resolvedRevisionId] = await Promise.all([
		loadAndValidateAgentFile(resolvedAgentFilePath),
		computeResolvedRevisionId(resolvedAgentFilePath),
	])
	return { agentFile, resolvedRevisionId }
}

export async function registerAgentLifecycleFile(agentFilePath: string, stateDbPath: string) {
	const { stateStore, lifecycleService } = await createLifecycleService(stateDbPath)
	try {
		return await lifecycleService.registerAgentFile(agentFilePath)
	} finally {
		stateStore.close()
	}
}

function collectOptionalListOption(value: string, previous?: string[]): string[] {
	return [...(previous ?? []), value]
}

export async function deployAgentLifecycleFile(agentFilePath: string, stateDbPath: string) {
	const { stateStore, lifecycleService } = await createLifecycleService(stateDbPath)
	try {
		return await lifecycleService.deployAgentFile(agentFilePath)
	} finally {
		stateStore.close()
	}
}

export async function getAgentLifecycleStatus(logicalAgentId: string, stateDbPath: string) {
	const { stateStore, lifecycleService } = await createLifecycleService(stateDbPath)
	try {
		return await lifecycleService.getAgentStatus(logicalAgentId)
	} finally {
		stateStore.close()
	}
}

export async function buildAgentDraftWithSystemBuilder(
	input: {
		targetAgentId: string
		request: string
		targetAgentName?: string
		targetAgentDescription?: string
		revise?: boolean
		runId?: string
	},
	stateDbPath: string,
	adapter?: RuntimeAdapter,
) {
	const { stateStore } = await createLifecycleService(stateDbPath)
	const builderService = new BuilderAgentService({
		state_store: stateStore,
		...(adapter ? { runtime_adapter: adapter } : {}),
	})

	try {
		return await builderService.buildAgentDraft({
			target_agent_id: input.targetAgentId,
			request: input.request,
			target_agent_name: input.targetAgentName,
			target_agent_description: input.targetAgentDescription,
			revise: input.revise,
			run_id: input.runId,
		})
	} finally {
		stateStore.close()
	}
}

export async function registerLifecycleTrigger(
	triggerId: string,
	logicalAgentId: string,
	triggerRef: string,
	stateDbPath: string,
) {
	const { stateStore, lifecycleService } = await createLifecycleService(stateDbPath)
	try {
		return lifecycleService.registerTrigger({
			trigger_id: triggerId,
			logical_agent_id: logicalAgentId,
			trigger_ref: triggerRef,
		})
	} finally {
		stateStore.close()
	}
}

export async function listLifecycleTriggers(stateDbPath: string, logicalAgentId?: string) {
	const { stateStore, lifecycleService } = await createLifecycleService(stateDbPath)
	try {
		return lifecycleService.listTriggers(logicalAgentId)
	} finally {
		stateStore.close()
	}
}

export async function registerMemoryProvider(
	input: {
		providerId: string
		codexRef?: string
		providerFamily: string
		displayName?: string
		transport?: MemoryProviderTransport
		config?: JsonObject
		supportedCapabilities?: MemoryProviderCapability[]
	},
	stateDbPath: string,
) {
	const { stateStore, registryService } = await createMemoryProviderRegistry(stateDbPath)
	try {
		return registryService.registerProvider({
			provider_id: input.providerId,
			codex_ref: input.codexRef,
			provider_family: input.providerFamily,
			display_name: input.displayName,
			transport: input.transport,
			config: input.config,
			supported_capabilities: input.supportedCapabilities,
		})
	} finally {
		stateStore.close()
	}
}

export async function listRegisteredMemoryProviders(stateDbPath: string, providerFamily?: string) {
	const { stateStore, registryService } = await createMemoryProviderRegistry(stateDbPath)
	try {
		return registryService.listProviders(providerFamily)
	} finally {
		stateStore.close()
	}
}

export async function showRegisteredMemoryProvider(providerId: string, stateDbPath: string) {
	const { stateStore, registryService } = await createMemoryProviderRegistry(stateDbPath)
	try {
		return registryService.getProviderOrThrow(providerId)
	} finally {
		stateStore.close()
	}
}

export async function writeRegisteredMemory(
	codexRef: string,
	input: {
		text: string
		scope: MemoryScope
		metadata?: JsonObject
		infer?: boolean
	},
	stateDbPath: string,
) {
	const { stateStore, memoryService } = await createMemoryService(stateDbPath)
	try {
		return await memoryService.writeForCodexRef(codexRef, {
			content: input.text,
			scope: input.scope,
			metadata: input.metadata,
			infer: input.infer,
		})
	} finally {
		stateStore.close()
	}
}

export async function readRegisteredMemory(
	codexRef: string,
	memoryId: string,
	stateDbPath: string,
) {
	const { stateStore, memoryService } = await createMemoryService(stateDbPath)
	try {
		return await memoryService.readForCodexRef(codexRef, {
			memory_id: memoryId,
		})
	} finally {
		stateStore.close()
	}
}

export async function searchRegisteredMemory(
	codexRef: string,
	input: {
		query: string
		scope: MemoryScope
		limit?: number
		threshold?: number
	},
	stateDbPath: string,
) {
	const { stateStore, memoryService } = await createMemoryService(stateDbPath)
	try {
		return await memoryService.searchForCodexRef(codexRef, {
			query: input.query,
			scope: input.scope,
			limit: input.limit,
			threshold: input.threshold,
		})
	} finally {
		stateStore.close()
	}
}

export async function listRegisteredMemory(
	codexRef: string,
	input: {
		scope: MemoryScope
		limit?: number
	},
	stateDbPath: string,
) {
	const { stateStore, memoryService } = await createMemoryService(stateDbPath)
	try {
		return await memoryService.listForCodexRef(codexRef, {
			scope: input.scope,
			limit: input.limit,
		})
	} finally {
		stateStore.close()
	}
}

export async function updateRegisteredMemory(
	codexRef: string,
	input: {
		memoryId: string
		text: string
		metadata?: JsonObject
	},
	stateDbPath: string,
) {
	const { stateStore, memoryService } = await createMemoryService(stateDbPath)
	try {
		return await memoryService.updateForCodexRef(codexRef, {
			memory_id: input.memoryId,
			content: input.text,
			metadata: input.metadata,
		})
	} finally {
		stateStore.close()
	}
}

export async function deleteRegisteredMemory(
	codexRef: string,
	memoryId: string,
	stateDbPath: string,
) {
	const { stateStore, memoryService } = await createMemoryService(stateDbPath)
	try {
		return await memoryService.deleteForCodexRef(codexRef, {
			memory_id: memoryId,
		})
	} finally {
		stateStore.close()
	}
}

export async function previewRegisteredMemoryCleanup(
	codexRef: string,
	input: {
		scope: MemoryScope
		limit?: number
	},
	stateDbPath: string,
) {
	const scope = buildMemoryScope({
		userId: input.scope.user_id,
		agentId: input.scope.agent_id,
		runId: input.scope.run_id,
	})
	assertExplicitMemoryCleanupScope(scope)
	const { stateStore, memoryService } = await createMemoryService(stateDbPath)
	try {
		const preview = await memoryService.previewMemoryCleanupForCodexRef(codexRef, {
			scope,
			limit: input.limit,
		})
		return buildMemoryCleanupPreviewOutput({
			codexRef,
			scope,
			preview,
		})
	} finally {
		stateStore.close()
	}
}

export async function deleteRegisteredMemoryCleanup(
	codexRef: string,
	input: {
		scope: MemoryScope
		confirmationToken: string
		limit?: number
	},
	stateDbPath: string,
): Promise<
	MemoryVerifiedCleanupResult & {
		scope: MemoryScope
		candidate_count: number
		candidate_ids: string[]
		preview_truncated: boolean
		verification: {
			status: 'verified_empty' | 'remaining_candidates'
			confirmation_token: string
		}
	}
> {
	const scope = buildMemoryScope({
		userId: input.scope.user_id,
		agentId: input.scope.agent_id,
		runId: input.scope.run_id,
	})
	assertExplicitMemoryCleanupScope(scope)
	const { stateStore, memoryService } = await createMemoryService(stateDbPath)
	try {
		const preview = await memoryService.previewMemoryCleanupForCodexRef(codexRef, {
			scope,
			limit: input.limit,
		})
		const expectedToken = buildMemoryCleanupConfirmationToken({
			codexRef,
			scope,
			preview,
		})
		if (input.confirmationToken !== expectedToken) {
			throw new AppError(
				'MEMORY_CLEANUP_CONFIRMATION_MISMATCH',
				'Confirmation token does not match the current cleanup preview for the requested scope.',
			)
		}

		const cleanup = await memoryService.deleteMemoryCleanupForCodexRef(codexRef, {
			scope,
			candidate_ids: preview.candidate_ids,
			limit: preview.limit,
		})
		return {
			...cleanup,
			scope,
			candidate_count: preview.candidate_count,
			candidate_ids: preview.candidate_ids,
			preview_truncated: preview.truncated,
			verification: {
				status: cleanup.verified_empty ? 'verified_empty' : 'remaining_candidates',
				confirmation_token: input.confirmationToken,
			},
		}
	} finally {
		stateStore.close()
	}
}

export async function dispatchTriggerEvent(
	triggerId: string,
	stateDbPath: string,
	options?: {
		eventId?: string
		payload?: JsonValue | null
		launchNote?: string
		runId?: string
		runtimeSourceIds?: string[]
		adapter?: RuntimeAdapter
		codexAppServerTimeouts?: CodexAppServerRuntimeAdapterOptions
	},
) {
	const { stateStore, lifecycleService } = await createLifecycleService(stateDbPath)
	const adapter = options?.adapter ?? createCodexAppServerAdapter(options?.codexAppServerTimeouts)

	try {
		const prepared = await lifecycleService.prepareEventDispatch({
			trigger_id: triggerId,
			event_id: options?.eventId,
			payload: options?.payload ?? null,
			launch_note: options?.launchNote,
		})

		try {
			const result = await runAgentFile(
				prepared.live_agent.agent_file,
				adapter,
				{},
				{
					state_store: stateStore,
					resolved_revision_id: prepared.live_agent.resolved_revision_id,
					logical_agent_id: prepared.live_agent.logical_agent_id,
					run_id: options?.runId,
					started_via: 'event',
					event: prepared.graph_event,
					user_runtime_source_ids: options?.runtimeSourceIds,
				},
			)
			const event = lifecycleService.markEventDispatched(
				prepared.event.event_id,
				result.run_id,
				prepared.live_agent.resolved_revision_id,
			)
			return {
				event,
				result,
			}
		} catch (error) {
			const appError =
				error instanceof AppError
					? error
					: new AppError(
							'EVENT_DISPATCH_FAILED',
							error instanceof Error ? error.message : 'Unknown event dispatch failure.',
						)

			lifecycleService.markEventDispatchFailed(prepared.event.event_id, appError)
			throw appError
		}
	} finally {
		stateStore.close()
	}
}

export async function runLiveAgentByLogicalId(
	logicalAgentId: string,
	stateDbPath: string,
	params: Record<string, JsonValue>,
	options?: {
		runId?: string
		runtimeSourceIds?: string[]
		adapter?: RuntimeAdapter
		codexAppServerTimeouts?: CodexAppServerRuntimeAdapterOptions
	},
) {
	const { stateStore, lifecycleService } = await createLifecycleService(stateDbPath)
	const adapter = options?.adapter ?? createCodexAppServerAdapter(options?.codexAppServerTimeouts)

	try {
		const liveAgent = await lifecycleService.resolveLiveAgentFile(logicalAgentId)
		return await runAgentFile(liveAgent.agent_file, adapter, params, {
			state_store: stateStore,
			resolved_revision_id: liveAgent.resolved_revision_id,
			run_id: options?.runId,
			user_runtime_source_ids: options?.runtimeSourceIds,
		})
	} finally {
		stateStore.close()
	}
}

export async function listRuntimeModels(
	input: {
		cursor?: string
		limit?: number
		includeHidden?: boolean
	},
	adapter?: RuntimeAdapter,
): Promise<RuntimeModelCatalogPage> {
	const runtimeAdapter = adapter ?? createCodexAppServerAdapter()
	const capabilities = runtimeAdapter.describeCapabilities()
	if (!capabilities.supports_model_discovery) {
		throw new AppError(
			'UNSUPPORTED_RUNTIME_SURFACE',
			'The current runtime adapter does not support model discovery.',
		)
	}

	return await runtimeAdapter.listModels({
		...(input.cursor ? { cursor: input.cursor } : {}),
		...(input.limit !== undefined ? { limit: input.limit } : {}),
		...(input.includeHidden !== undefined ? { include_hidden: input.includeHidden } : {}),
	})
}

export async function inspectRuntimeEnvironment(
	adapter?: RuntimeAdapter,
): Promise<RuntimeEnvironmentInspectionResult> {
	const runtimeAdapter = adapter ?? createCodexAppServerAdapter()
	const capabilities = runtimeAdapter.describeCapabilities()
	if (!capabilities.supports_runtime_environment_introspection) {
		throw new AppError(
			'UNSUPPORTED_RUNTIME_SURFACE',
			'The current runtime adapter does not support runtime environment introspection.',
		)
	}

	return await runtimeAdapter.inspectRuntimeEnvironment()
}

function assertActiveRunCompatibility(
	snapshot: {
		run: { run_id: string; resolved_revision_id: string; status: string }
		attempts: Array<{ node_id: string; state: string }>
		resume: {
			pending_prompt: {
				request_handle: JsonValue | null
				prompt_id: string | null
				payload: JsonValue
			} | null
		}
	},
	resolvedRevisionId: string,
): void {
	if (snapshot.run.resolved_revision_id !== resolvedRevisionId) {
		throw new AppError(
			'RESUME_REVISION_MISMATCH',
			`Run "${snapshot.run.run_id}" is pinned to "${snapshot.run.resolved_revision_id}" but the supplied agent file resolves to "${resolvedRevisionId}".`,
		)
	}
	if (snapshot.run.status !== 'running' && snapshot.run.status !== 'waiting_for_user') {
		throw new AppError(
			'RUN_NOT_ACTIVE',
			`Run "${snapshot.run.run_id}" is "${snapshot.run.status}" and cannot accept live interaction.`,
		)
	}
}

export function buildCliProgram(): Command {
	const program = new Command()
	program
		.name('dennett-agent-orchestrator')
		.description('Phase 8 agent lifecycle, live resolution, and durable runtime_agent execution.')

	program
		.command('runtime-model-list')
		.option('--cursor <cursor>', 'pagination cursor from a previous model-list response')
		.option('--limit <count>', 'maximum number of models to return')
		.option('--include-hidden', 'include hidden models in the returned catalog')
		.option(
			'--codex-app-server-model-catalog-timeout-ms <ms>',
			'Codex App Server model catalog timeout in milliseconds',
		)
		.action(
			async (options: {
				cursor?: string
				limit?: string
				includeHidden?: boolean
				codexAppServerModelCatalogTimeoutMs?: string
			}) => {
				const parsedLimit =
					options.limit === undefined ? undefined : Number.parseInt(options.limit, 10)
				if (options.limit !== undefined && Number.isNaN(parsedLimit)) {
					throw new AppError('INVALID_CLI_OPTION', 'Option "--limit" must be an integer.')
				}
				const result = await listRuntimeModels(
					{
						cursor: options.cursor,
						limit: parsedLimit,
						includeHidden: options.includeHidden,
					},
					createCodexAppServerAdapter({
						model_catalog_timeout_ms: parseTimeoutOption(
							options.codexAppServerModelCatalogTimeoutMs,
							'--codex-app-server-model-catalog-timeout-ms',
						),
					}),
				)
				printJson(result)
			},
		)

	program
		.command('runtime-env-inspect')
		.option(
			'--codex-app-server-environment-timeout-ms <ms>',
			'Codex App Server environment inspection timeout in milliseconds',
		)
		.action(async (options: { codexAppServerEnvironmentTimeoutMs?: string }) => {
			const result = await inspectRuntimeEnvironment(
				createCodexAppServerAdapter({
					environment_timeout_ms: parseTimeoutOption(
						options.codexAppServerEnvironmentTimeoutMs,
						'--codex-app-server-environment-timeout-ms',
					),
				}),
			)
			printJson(result)
		})

	program
		.command('memory-provider-register')
		.argument('<provider-id>', 'stable local memory provider id')
		.requiredOption('--family <family>', 'local provider family, for example mem0')
		.option('--codex-ref <ref>', 'codex_ref used by portable memory bindings')
		.option('--display-name <name>', 'human-readable local display name')
		.option('--transport <kind>', 'provider transport: api, sdk, or mcp')
		.option('--config <json>', 'provider config as a JSON object', '{}')
		.option('--capability <token>', 'supported memory capability token', collectOptionalListOption)
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.action(
			async (
				providerId: string,
				options: {
					family: string
					codexRef?: string
					displayName?: string
					transport?: MemoryProviderTransport
					config: string
					capability?: string[]
					stateDb: string
				},
			) => {
				const record = await registerMemoryProvider(
					{
						providerId,
						codexRef: options.codexRef,
						providerFamily: options.family,
						displayName: options.displayName,
						transport: options.transport,
						config: parseJsonObjectOption(options.config, '--config'),
						supportedCapabilities: options.capability,
					},
					options.stateDb,
				)
				printMemoryProviderRecord(record)
			},
		)

	program
		.command('memory-provider-list')
		.option('--family <family>', 'optional provider family filter')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.action(async (options: { family?: string; stateDb: string }) => {
			const records = await listRegisteredMemoryProviders(options.stateDb, options.family)
			printMemoryProviderRecords(records)
		})

	program
		.command('memory-provider-show')
		.argument('<provider-id>', 'stable local memory provider id')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.action(async (providerId: string, options: { stateDb: string }) => {
			const record = await showRegisteredMemoryProvider(providerId, options.stateDb)
			printMemoryProviderRecord(record)
		})

	program
		.command('memory-write')
		.argument(
			'<codex-ref>',
			'portable memory binding codex_ref resolved through the local registry',
		)
		.requiredOption('--text <text>', 'memory text to write')
		.option('--user-id <id>', 'optional user scope id')
		.option('--agent-id <id>', 'optional agent scope id')
		.option('--run-id <id>', 'optional run scope id')
		.option('--metadata <json>', 'metadata JSON object', '{}')
		.option('--infer', 'enable provider-side inference when supported', false)
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.action(
			async (
				codexRef: string,
				options: {
					text: string
					userId?: string
					agentId?: string
					runId?: string
					metadata: string
					infer: boolean
					stateDb: string
				},
			) => {
				const result = await writeRegisteredMemory(
					codexRef,
					{
						text: options.text,
						scope: buildMemoryScope(options),
						metadata: parseJsonObjectOption(options.metadata, '--metadata'),
						infer: options.infer,
					},
					options.stateDb,
				)
				printJson(result)
			},
		)

	program
		.command('memory-read')
		.argument(
			'<codex-ref>',
			'portable memory binding codex_ref resolved through the local registry',
		)
		.argument('<memory-id>', 'provider memory identifier')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.action(async (codexRef: string, memoryId: string, options: { stateDb: string }) => {
			const result = await readRegisteredMemory(codexRef, memoryId, options.stateDb)
			printJson(result)
		})

	program
		.command('memory-search')
		.argument(
			'<codex-ref>',
			'portable memory binding codex_ref resolved through the local registry',
		)
		.requiredOption('--query <text>', 'semantic search query')
		.option('--user-id <id>', 'optional user scope id')
		.option('--agent-id <id>', 'optional agent scope id')
		.option('--run-id <id>', 'optional run scope id')
		.option('--limit <count>', 'max result count', '10')
		.option('--threshold <score>', 'minimum score threshold', '0.1')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.action(
			async (
				codexRef: string,
				options: {
					query: string
					userId?: string
					agentId?: string
					runId?: string
					limit: string
					threshold: string
					stateDb: string
				},
			) => {
				const result = await searchRegisteredMemory(
					codexRef,
					{
						query: options.query,
						scope: buildMemoryScope(options),
						limit: Number(options.limit),
						threshold: Number(options.threshold),
					},
					options.stateDb,
				)
				printJson(result)
			},
		)

	program
		.command('memory-list')
		.argument(
			'<codex-ref>',
			'portable memory binding codex_ref resolved through the local registry',
		)
		.option('--user-id <id>', 'optional user scope id')
		.option('--agent-id <id>', 'optional agent scope id')
		.option('--run-id <id>', 'optional run scope id')
		.option('--limit <count>', 'max result count', '20')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.action(
			async (
				codexRef: string,
				options: {
					userId?: string
					agentId?: string
					runId?: string
					limit: string
					stateDb: string
				},
			) => {
				const result = await listRegisteredMemory(
					codexRef,
					{
						scope: buildMemoryScope(options),
						limit: Number(options.limit),
					},
					options.stateDb,
				)
				printJson(result)
			},
		)

	program
		.command('memory-update')
		.argument(
			'<codex-ref>',
			'portable memory binding codex_ref resolved through the local registry',
		)
		.argument('<memory-id>', 'provider memory identifier')
		.requiredOption('--text <text>', 'replacement memory text')
		.option('--metadata <json>', 'metadata JSON object', '{}')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.action(
			async (
				codexRef: string,
				memoryId: string,
				options: {
					text: string
					metadata: string
					stateDb: string
				},
			) => {
				const result = await updateRegisteredMemory(
					codexRef,
					{
						memoryId,
						text: options.text,
						metadata: parseJsonObjectOption(options.metadata, '--metadata'),
					},
					options.stateDb,
				)
				printJson(result)
			},
		)

	program
		.command('memory-delete')
		.argument(
			'<codex-ref>',
			'portable memory binding codex_ref resolved through the local registry',
		)
		.argument('<memory-id>', 'provider memory identifier')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.action(async (codexRef: string, memoryId: string, options: { stateDb: string }) => {
			const result = await deleteRegisteredMemory(codexRef, memoryId, options.stateDb)
			printJson(result)
		})

	program
		.command('memory-cleanup-preview')
		.argument(
			'<codex-ref>',
			'portable memory binding codex_ref resolved through the local registry',
		)
		.option('--user-id <id>', 'required cleanup scope user id unless another scope is set')
		.option('--agent-id <id>', 'required cleanup scope agent id unless another scope is set')
		.option('--run-id <id>', 'required cleanup scope run id unless another scope is set')
		.option('--limit <count>', 'maximum number of cleanup candidates to preview')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.action(
			async (
				codexRef: string,
				options: {
					userId?: string
					agentId?: string
					runId?: string
					limit?: string
					stateDb: string
				},
			) => {
				const result = await previewRegisteredMemoryCleanup(
					codexRef,
					{
						scope: buildMemoryScope(options),
						limit: parsePositiveIntegerOption(options.limit, '--limit'),
					},
					options.stateDb,
				)
				printJson(result)
			},
		)

	program
		.command('memory-cleanup-verified-delete')
		.argument(
			'<codex-ref>',
			'portable memory binding codex_ref resolved through the local registry',
		)
		.requiredOption(
			'--confirm-token <token>',
			'confirmation token copied from memory-cleanup-preview output',
		)
		.option('--user-id <id>', 'required cleanup scope user id unless another scope is set')
		.option('--agent-id <id>', 'required cleanup scope agent id unless another scope is set')
		.option('--run-id <id>', 'required cleanup scope run id unless another scope is set')
		.option('--limit <count>', 'same candidate limit used for the confirmed preview')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.action(
			async (
				codexRef: string,
				options: {
					confirmToken: string
					userId?: string
					agentId?: string
					runId?: string
					limit?: string
					stateDb: string
				},
			) => {
				const result = await deleteRegisteredMemoryCleanup(
					codexRef,
					{
						scope: buildMemoryScope(options),
						confirmationToken: options.confirmToken,
						limit: parsePositiveIntegerOption(options.limit, '--limit'),
					},
					options.stateDb,
				)
				printJson(result)
			},
		)

	program
		.command('subagent-launch')
		.argument('<agent-ref>', 'live logical agent id to run as a managed child')
		.requiredOption('--parent-run-id <id>', 'parent run id that owns the managed child')
		.requiredOption('--parent-task-id <id>', 'parent task id that owns the managed child')
		.requiredOption('--role <role>', 'worker, reviewer, or final_review')
		.requiredOption('--objective <text>', 'bounded delegated objective')
		.requiredOption('--input-message <text>', 'input message passed to the child run')
		.requiredOption(
			'--acceptance-criterion <text>',
			'acceptance criterion; repeat for multiple criteria',
			collectOptionalListOption,
		)
		.requiredOption('--write-set <json>', 'managed write_set JSON object')
		.option(
			'--prohibition <text>',
			'prohibition; repeat for multiple prohibitions',
			collectOptionalListOption,
		)
		.option('--budgets <json>', 'budget limits JSON object')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.option(
			'--codex-app-server-execution-timeout-ms <ms>',
			'Codex App Server runtime execution timeout in milliseconds',
		)
		.action(
			async (
				agentRef: string,
				options: {
					parentRunId: string
					parentTaskId: string
					role: string
					objective: string
					inputMessage: string
					acceptanceCriterion: string[]
					writeSet: string
					prohibition?: string[]
					budgets?: string
					stateDb: string
					codexAppServerExecutionTimeoutMs?: string
				},
			) => {
				const result = await launchManagedSubagentForOperators(
					{
						parent_run_id: options.parentRunId,
						parent_task_id: options.parentTaskId,
						child_role: parseManagedSubagentRole(options.role),
						agent_ref: agentRef,
						objective: options.objective,
						input_message: options.inputMessage,
						acceptance_criteria: options.acceptanceCriterion as [string, ...string[]],
						prohibitions: options.prohibition ?? [],
						write_set: parseManagedSubagentWriteSet(options.writeSet),
						budgets: parseManagedSubagentBudgets(options.budgets),
					},
					options.stateDb,
					createCodexAppServerAdapter({
						execution_timeout_ms: parseTimeoutOption(
							options.codexAppServerExecutionTimeoutMs,
							'--codex-app-server-execution-timeout-ms',
						),
					}),
				)
				printJson(result)
			},
		)

	program
		.command('subagent-list')
		.option('--parent-run-id <id>', 'filter by parent run id')
		.option('--parent-task-id <id>', 'filter by parent task id')
		.option('--state <state>', 'filter by state: running, cancelling, terminal, closed')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.action(
			async (options: {
				parentRunId?: string
				parentTaskId?: string
				state?: string
				stateDb: string
			}) => {
				const records = await listManagedSubagentsForOperators(options.stateDb, {
					parentRunId: options.parentRunId,
					parentTaskId: options.parentTaskId,
					state: parseManagedSubagentStateOption(options.state),
				})
				printJson(records)
			},
		)

	program
		.command('subagent-show')
		.argument('<subagent-id>', 'managed subagent id to inspect')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.action(async (subagentId: string, options: { stateDb: string }) => {
			const record = await showManagedSubagentForOperators(subagentId, options.stateDb)
			printJson(record)
		})

	program
		.command('subagent-wait')
		.argument('<subagent-id>', 'managed subagent id to reconcile or inspect')
		.option('--wait-mode <mode>', 'terminal_only or terminal_or_update', 'terminal_or_update')
		.option(
			'--timeout-ms <ms>',
			'recorded in output only; this CLI reconciles persisted state and cannot bound another process live wait',
		)
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.action(
			async (
				subagentId: string,
				options: {
					waitMode?: string
					timeoutMs?: string
					stateDb: string
				},
			) => {
				const result = await waitManagedSubagentForOperators(subagentId, options.stateDb, {
					waitMode: parseManagedSubagentWaitMode(options.waitMode),
					timeoutMs: parseTimeoutOption(options.timeoutMs, '--timeout-ms'),
				})
				printJson(result)
			},
		)

	program
		.command('subagent-record-control')
		.argument('<subagent-id>', 'managed subagent id to record a bounded control message for')
		.requiredOption(
			'--kind <kind>',
			'control kind: clarify_scope, narrow_constraints, update_budget, request_status, cancel',
		)
		.option('--message-id <id>', 'explicit idempotency key/control message id')
		.option('--payload <json>', 'control payload JSON object', '{}')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.action(
			async (
				subagentId: string,
				options: {
					kind: string
					messageId?: string
					payload: string
					stateDb: string
				},
			) => {
				const result = await recordManagedSubagentControlForOperators(subagentId, options.stateDb, {
					messageId: options.messageId,
					messageKind: parseManagedSubagentControlKind(options.kind),
					payload: parseJsonObjectOption(options.payload, '--payload'),
				})
				printJson(result)
			},
		)

	program
		.command('subagent-close')
		.argument('<subagent-id>', 'managed subagent id to close')
		.requiredOption(
			'--disposition <disposition>',
			'accepted_by_parent, cancelled_by_parent, or abandoned_by_parent',
		)
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.action(
			async (
				subagentId: string,
				options: {
					disposition: string
					stateDb: string
				},
			) => {
				const result = await closeManagedSubagentForOperators(
					subagentId,
					options.stateDb,
					parseManagedSubagentCloseDisposition(options.disposition),
				)
				printJson(result)
			},
		)

	program
		.command('register')
		.argument('<agent-file>', 'path to a portable agent JSON file')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.action(async (agentFilePath: string, options: { stateDb: string }) => {
			const stateStore = await createStateStore(options.stateDb)
			const lifecycleService = new AgentLifecycleService({
				state_store: stateStore,
			})

			try {
				const result = await lifecycleService.registerAgentFile(agentFilePath)
				printLifecycleStatus(result.status)
				printJson({
					logical_agent_id: result.logical_agent_id,
					revision: result.revision,
				})
			} finally {
				stateStore.close()
			}
		})

	program
		.command('status')
		.argument('<agent-id>', 'logical agent id to inspect')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.action(async (logicalAgentId: string, options: { stateDb: string }) => {
			const stateStore = await createStateStore(options.stateDb)
			const lifecycleService = new AgentLifecycleService({
				state_store: stateStore,
			})

			try {
				const status = await lifecycleService.getAgentStatus(logicalAgentId)
				printLifecycleStatus(status)
			} finally {
				stateStore.close()
			}
		})

	program
		.command('deploy')
		.argument('<agent-file>', 'path to a portable agent JSON file')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.action(async (agentFilePath: string, options: { stateDb: string }) => {
			const stateStore = await createStateStore(options.stateDb)
			const lifecycleService = new AgentLifecycleService({
				state_store: stateStore,
			})

			try {
				const result = await lifecycleService.deployAgentFile(agentFilePath)
				printLifecycleStatus(result.status)
				printJson({
					logical_agent_id: result.logical_agent_id,
					source_revision_id: result.source_revision_id,
					live_file_path: result.live_file_path,
					revision: result.revision,
				})
			} finally {
				stateStore.close()
			}
		})

	program
		.command('builder')
		.argument('<agent-id>', 'logical agent id to create or revise')
		.requiredOption('--request <text>', 'builder request describing the desired draft')
		.option('--name <name>', 'suggested display name for newly created drafts')
		.option('--description <text>', 'suggested description for newly created drafts')
		.option('--revise', 'revise an existing logical agent from its latest available revision base')
		.option('--run-id <id>', 'explicit run id to use for the builder execution')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.option(
			'--codex-app-server-execution-timeout-ms <ms>',
			'Codex App Server runtime execution timeout in milliseconds',
		)
		.action(
			async (
				logicalAgentId: string,
				options: {
					request: string
					name?: string
					description?: string
					revise?: boolean
					runId?: string
					stateDb: string
					codexAppServerExecutionTimeoutMs?: string
				},
			) => {
				const result = await buildAgentDraftWithSystemBuilder(
					{
						targetAgentId: logicalAgentId,
						request: options.request,
						targetAgentName: options.name,
						targetAgentDescription: options.description,
						revise: options.revise,
						runId: options.runId,
					},
					options.stateDb,
					createCodexAppServerAdapter({
						execution_timeout_ms: parseTimeoutOption(
							options.codexAppServerExecutionTimeoutMs,
							'--codex-app-server-execution-timeout-ms',
						),
					}),
				)

				printJson({
					operation: result.operation,
					builder_run_id: result.builder_run_id,
					base_revision: result.base_revision,
					draft_revision: result.draft.revision,
					draft_status: result.draft.status,
				})
			},
		)

	program
		.command('trigger-register')
		.argument('<trigger-id>', 'stable trigger id')
		.argument('<agent-id>', 'logical agent id to bind')
		.requiredOption('--trigger-ref <ref>', 'trigger reference label')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.action(
			async (
				triggerId: string,
				logicalAgentId: string,
				options: { triggerRef: string; stateDb: string },
			) => {
				const trigger = await registerLifecycleTrigger(
					triggerId,
					logicalAgentId,
					options.triggerRef,
					options.stateDb,
				)
				printJson(trigger)
			},
		)

	program
		.command('trigger-list')
		.argument('[agent-id]', 'optional logical agent id filter')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.action(async (logicalAgentId: string | undefined, options: { stateDb: string }) => {
			const triggers = await listLifecycleTriggers(options.stateDb, logicalAgentId)
			printJson(triggers)
		})

	program
		.command('event-dispatch')
		.argument('<trigger-id>', 'trigger id to dispatch')
		.option('--event-id <id>', 'explicit event id')
		.option('--payload <json>', 'event payload as JSON or plain scalar text')
		.option('--launch-note <text>', 'launch note surfaced as event.launch_note')
		.option(
			'--runtime-source-id <id>',
			'narrow execution to a declared runtime source id',
			collectOptionalListOption,
		)
		.option('--run-id <id>', 'explicit run id for the created run')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.option(
			'--codex-app-server-execution-timeout-ms <ms>',
			'Codex App Server runtime execution timeout in milliseconds',
		)
		.action(
			async (
				triggerId: string,
				options: {
					eventId?: string
					payload?: string
					launchNote?: string
					runtimeSourceId?: string[]
					runId?: string
					stateDb: string
					codexAppServerExecutionTimeoutMs?: string
				},
			) => {
				const dispatch = await dispatchTriggerEvent(triggerId, options.stateDb, {
					eventId: options.eventId,
					payload: options.payload !== undefined ? parseParamValue(options.payload) : null,
					launchNote: options.launchNote,
					runtimeSourceIds: options.runtimeSourceId,
					runId: options.runId,
					codexAppServerTimeouts: {
						execution_timeout_ms: parseTimeoutOption(
							options.codexAppServerExecutionTimeoutMs,
							'--codex-app-server-execution-timeout-ms',
						),
					},
				})

				printJson({
					event: dispatch.event,
					run: {
						run_id: dispatch.result.run_id,
						status: dispatch.result.status,
						run_status: dispatch.result.run_status,
						...(dispatch.result.status === 'success'
							? {
									final_output: dispatch.result.final_output,
									final_output_mode: dispatch.result.final_output_mode,
								}
							: {
									code: dispatch.result.code,
									message: dispatch.result.message,
									resume_available: dispatch.result.resume_available,
								}),
					},
				})

				if (dispatch.result.status !== 'success') {
					process.exitCode = 1
				}
			},
		)

	program
		.command('run-live')
		.argument('<agent-id>', 'logical agent id to run using the current live revision')
		.option(
			'--param <key=value>',
			'parameter override',
			(value, previous: string[] = []) => [...previous, value],
			[],
		)
		.option(
			'--runtime-source-id <id>',
			'narrow execution to a declared runtime source id',
			collectOptionalListOption,
		)
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.option('--run-id <id>', 'explicit run id to use for this execution')
		.option(
			'--codex-app-server-execution-timeout-ms <ms>',
			'Codex App Server runtime execution timeout in milliseconds',
		)
		.action(
			async (
				logicalAgentId: string,
				options: {
					param?: string[]
					runtimeSourceId?: string[]
					stateDb: string
					runId?: string
					codexAppServerExecutionTimeoutMs?: string
				},
			) => {
				const params = parseParams(options.param ?? [])
				const stateStore = await createStateStore(options.stateDb)
				const lifecycleService = new AgentLifecycleService({
					state_store: stateStore,
				})
				const adapter = createCodexAppServerAdapter({
					execution_timeout_ms: parseTimeoutOption(
						options.codexAppServerExecutionTimeoutMs,
						'--codex-app-server-execution-timeout-ms',
					),
				})

				try {
					const liveAgent = await lifecycleService.resolveLiveAgentFile(logicalAgentId)
					const result = await runAgentFile(liveAgent.agent_file, adapter, params, {
						state_store: stateStore,
						resolved_revision_id: liveAgent.resolved_revision_id,
						run_id: options.runId,
						user_runtime_source_ids: options.runtimeSourceId,
					})

					printRunId(result.run_id)
					if (result.status !== 'success') {
						if (result.resume_available) {
							process.stderr.write('Local resume remains available.\n')
						}
						printFailure(result.code, result.message)
						process.exitCode = 1
						return
					}

					if (result.final_output_mode === 'text') {
						process.stdout.write(`${result.final_output}\n`)
					} else if (result.final_output_mode === 'json') {
						process.stdout.write(`${JSON.stringify(result.final_output, null, 2)}\n`)
					}
				} finally {
					stateStore.close()
				}
			},
		)

	program
		.command('run')
		.argument('<agent-file>', 'path to the agent JSON file')
		.option(
			'--param <key=value>',
			'parameter override',
			(value, previous: string[] = []) => [...previous, value],
			[],
		)
		.option(
			'--runtime-source-id <id>',
			'narrow execution to a declared runtime source id',
			collectOptionalListOption,
		)
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.option('--run-id <id>', 'explicit run id to use for this execution')
		.option(
			'--codex-app-server-execution-timeout-ms <ms>',
			'Codex App Server runtime execution timeout in milliseconds',
		)
		.action(
			async (
				agentFilePath: string,
				options: {
					param?: string[]
					runtimeSourceId?: string[]
					stateDb: string
					runId?: string
					codexAppServerExecutionTimeoutMs?: string
				},
			) => {
				const resolvedAgentFilePath = path.resolve(process.cwd(), agentFilePath)
				const params = parseParams(options.param ?? [])
				const [agentFile, resolvedRevisionId] = await Promise.all([
					loadAndValidateAgentFile(resolvedAgentFilePath),
					computeResolvedRevisionId(resolvedAgentFilePath),
				])
				const adapter = createCodexAppServerAdapter({
					execution_timeout_ms: parseTimeoutOption(
						options.codexAppServerExecutionTimeoutMs,
						'--codex-app-server-execution-timeout-ms',
					),
				})
				const stateStore = await createStateStore(options.stateDb)

				try {
					const result = await runAgentFile(agentFile, adapter, params, {
						state_store: stateStore,
						resolved_revision_id: resolvedRevisionId,
						run_id: options.runId,
						user_runtime_source_ids: options.runtimeSourceId,
					})

					printRunId(result.run_id)
					if (result.status !== 'success') {
						if (result.resume_available) {
							process.stderr.write('Local resume remains available.\n')
						}
						printFailure(result.code, result.message)
						process.exitCode = 1
						return
					}

					if (result.final_output_mode === 'text') {
						process.stdout.write(`${result.final_output}\n`)
					} else if (result.final_output_mode === 'json') {
						process.stdout.write(`${JSON.stringify(result.final_output, null, 2)}\n`)
					}
				} finally {
					stateStore.close()
				}
			},
		)

	program
		.command('run-status')
		.requiredOption('--run-id <id>', 'run id to inspect')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.action(async (options: { runId: string; stateDb: string }) => {
			const stateStore = await createStateStore(options.stateDb)

			try {
				const snapshot = stateStore.getPersistedRunSnapshot(options.runId)
				if (!snapshot) {
					throw new AppError('RUN_NOT_FOUND', `Run "${options.runId}" does not exist.`)
				}
				printJson(buildRunInteractionStatus(snapshot))
			} finally {
				stateStore.close()
			}
		})

	program
		.command('comment')
		.argument('<agent-file>', 'path to the same pinned agent JSON file')
		.requiredOption('--run-id <id>', 'run id to comment on')
		.requiredOption('--text <text>', 'comment text to inject')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.option(
			'--codex-app-server-comment-timeout-ms <ms>',
			'Codex App Server live comment delivery timeout in milliseconds',
		)
		.action(
			async (
				agentFilePath: string,
				options: {
					runId: string
					text: string
					stateDb: string
					codexAppServerCommentTimeoutMs?: string
				},
			) => {
				const { agentFile, resolvedRevisionId } = await loadAgentContext(agentFilePath)
				const adapter = createCodexAppServerAdapter({
					comment_timeout_ms: parseTimeoutOption(
						options.codexAppServerCommentTimeoutMs,
						'--codex-app-server-comment-timeout-ms',
					),
				})
				const stateStore = await createStateStore(options.stateDb)

				try {
					const snapshot = stateStore.getPersistedRunSnapshot(options.runId)
					if (!snapshot) {
						throw new AppError('RUN_NOT_FOUND', `Run "${options.runId}" does not exist.`)
					}
					assertActiveRunCompatibility(snapshot, resolvedRevisionId)

					if (agentFile.interaction?.comments?.enabled !== true) {
						throw new AppError(
							'UNSUPPORTED_INTERACTION',
							`Agent file does not enable interaction.comments for run "${options.runId}".`,
						)
					}

					const activeAttempt = getLatestActiveAttempt(snapshot)
					const commentTargetNodeIds = agentFile.interaction.comments.target_node_ids
						? [...agentFile.interaction.comments.target_node_ids]
						: []
					if (!activeAttempt || !commentTargetNodeIds.includes(activeAttempt.node_id)) {
						throw new AppError(
							'UNSUPPORTED_INTERACTION',
							`Run "${options.runId}" is not currently on a comment-targeted runtime node.`,
						)
					}

					const capabilities = adapter.describeCapabilities()
					if (!capabilities.supports_live_comments) {
						throw new AppError(
							'UNSUPPORTED_INTERACTION',
							'The current runtime adapter does not support live comments.',
						)
					}

					const execution = resolveCommentExecutionHandle(snapshot)
					if (execution === null) {
						throw new AppError(
							'RUN_NOT_INTERACTABLE',
							`Run "${options.runId}" does not have a live runtime handle for comment delivery.`,
						)
					}

					await adapter.deliverComment(execution, options.text)
					stateStore.appendVisibleChatMessage({
						run_id: options.runId,
						kind: 'user_message',
						payload: { text: options.text },
					})
					process.stdout.write('Comment delivered.\n')
				} finally {
					stateStore.close()
				}
			},
		)

	program
		.command('reply')
		.argument('<agent-file>', 'path to the same pinned agent JSON file')
		.requiredOption('--run-id <id>', 'run id to reply on')
		.option('--prompt-id <id>', 'explicit prompt id to answer')
		.option('--text <text>', 'text response to the pending prompt')
		.option('--option-id <id>', 'option id to return for an options prompt')
		.option('--value <json>', 'value for an option response')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.option(
			'--codex-app-server-reply-timeout-ms <ms>',
			'Codex App Server prompt reply delivery timeout in milliseconds',
		)
		.action(
			async (
				agentFilePath: string,
				options: {
					runId: string
					promptId?: string
					text?: string
					optionId?: string
					value?: string
					stateDb: string
					codexAppServerReplyTimeoutMs?: string
				},
			) => {
				const { agentFile, resolvedRevisionId } = await loadAgentContext(agentFilePath)
				const adapter = createCodexAppServerAdapter({
					reply_timeout_ms: parseTimeoutOption(
						options.codexAppServerReplyTimeoutMs,
						'--codex-app-server-reply-timeout-ms',
					),
				})
				const stateStore = await createStateStore(options.stateDb)

				try {
					const snapshot = stateStore.getPersistedRunSnapshot(options.runId)
					if (!snapshot) {
						throw new AppError('RUN_NOT_FOUND', `Run "${options.runId}" does not exist.`)
					}
					assertActiveRunCompatibility(snapshot, resolvedRevisionId)

					if (agentFile.interaction?.user_mcp?.enabled !== true) {
						throw new AppError(
							'UNSUPPORTED_INTERACTION',
							`Agent file does not enable interaction.user_mcp for run "${options.runId}".`,
						)
					}

					const pendingPrompt = snapshot.resume.pending_prompt
					if (!pendingPrompt) {
						throw new AppError(
							'RUN_NOT_WAITING_FOR_USER',
							`Run "${options.runId}" is not waiting on a built-in user-chat prompt.`,
						)
					}

					const promptPayload = pendingPrompt.payload as {
						kind?: string
						options?: Array<{ id?: string; value: JsonValue }>
					}
					const promptId = options.promptId ?? pendingPrompt.prompt_id ?? undefined
					if (pendingPrompt.prompt_id && promptId !== pendingPrompt.prompt_id) {
						throw new AppError(
							'UNSUPPORTED_INTERACTION',
							`Run "${options.runId}" is waiting for prompt "${pendingPrompt.prompt_id}", not "${promptId}".`,
						)
					}

					let response:
						| {
								kind: 'text'
								prompt_id?: string
								text: string
						  }
						| {
								kind: 'option'
								prompt_id?: string
								option_id: string
								value: JsonValue
						  }

					if (options.optionId !== undefined || options.value !== undefined) {
						if (options.optionId === undefined || options.value === undefined) {
							throw new AppError(
								'INVALID_REPLY',
								'Option replies require both --option-id and --value.',
							)
						}
						response = buildOptionReplyPayload({
							runId: options.runId,
							promptId,
							promptPayload,
							optionId: options.optionId,
							value: options.value,
						})
					} else {
						if (options.text === undefined) {
							throw new AppError('INVALID_REPLY', 'Text replies require --text.')
						}
						if (promptPayload.kind === 'options') {
							throw new AppError(
								'UNSUPPORTED_INTERACTION',
								`Run "${options.runId}" requires an explicit option response.`,
							)
						}
						response = {
							kind: 'text',
							...(promptId ? { prompt_id: promptId } : {}),
							text: options.text,
						}
					}

					const capabilities = adapter.describeCapabilities()
					const execution = resolveReplyExecutionHandle(snapshot)
					const recordedReply = stateStore.recordUserPromptReply({
						run_id: options.runId,
						prompt_id: promptId ?? null,
						payload: response,
					})
					let deliveredLive = false
					if (
						recordedReply.accepted &&
						capabilities.supports_builtin_user_chat_mcp &&
						execution !== null
					) {
						try {
							await adapter.deliverUserChatResponse(execution, response)
							stateStore.markUserPromptReplyDelivered({
								run_id: options.runId,
								reply_id: recordedReply.reply.reply_id,
							})
							deliveredLive = true
						} catch (error) {
							stateStore.markUserPromptReplyDeliveryFailed({
								run_id: options.runId,
								reply_id: recordedReply.reply.reply_id,
								error_message: error instanceof Error ? error.message : 'Unknown error.',
							})
							process.stderr.write(
								`Live prompt delivery was unavailable: ${
									error instanceof Error ? error.message : 'Unknown error.'
								}\n`,
							)
						}
					}

					if (recordedReply.accepted) {
						stateStore.appendVisibleChatMessage({
							run_id: options.runId,
							kind: 'user_message',
							payload: response,
						})
					}
					process.stdout.write(
						recordedReply.accepted
							? deliveredLive
								? 'Prompt reply delivered.\n'
								: 'Prompt reply recorded for resume.\n'
							: 'Prompt reply already recorded.\n',
					)
				} finally {
					stateStore.close()
				}
			},
		)

	program
		.command('resume')
		.argument('<agent-file>', 'path to the same pinned agent JSON file')
		.requiredOption('--run-id <id>', 'run id to resume')
		.option('--state-db <path>', 'path to the local state database', defaultStateDatabasePath())
		.option(
			'--codex-app-server-execution-timeout-ms <ms>',
			'Codex App Server runtime execution timeout in milliseconds',
		)
		.action(
			async (
				agentFilePath: string,
				options: {
					runId: string
					stateDb: string
					codexAppServerExecutionTimeoutMs?: string
				},
			) => {
				const resolvedAgentFilePath = path.resolve(process.cwd(), agentFilePath)
				const [agentFile, resolvedRevisionId] = await Promise.all([
					loadAndValidateAgentFile(resolvedAgentFilePath),
					computeResolvedRevisionId(resolvedAgentFilePath),
				])
				const adapter = createCodexAppServerAdapter({
					execution_timeout_ms: parseTimeoutOption(
						options.codexAppServerExecutionTimeoutMs,
						'--codex-app-server-execution-timeout-ms',
					),
				})
				const stateStore = await createStateStore(options.stateDb)

				try {
					const result = await resumeAgentRun(agentFile, adapter, options.runId, {
						state_store: stateStore,
						resolved_revision_id: resolvedRevisionId,
					})

					printRunId(result.run_id)
					if (result.status !== 'success') {
						if (result.resume_available) {
							process.stderr.write('Local resume remains available.\n')
						}
						printFailure(result.code, result.message)
						process.exitCode = 1
						return
					}

					if (result.final_output_mode === 'text') {
						process.stdout.write(`${result.final_output}\n`)
					} else if (result.final_output_mode === 'json') {
						process.stdout.write(`${JSON.stringify(result.final_output, null, 2)}\n`)
					}
				} finally {
					stateStore.close()
				}
			},
		)

	return program
}

async function main(): Promise<void> {
	const program = buildCliProgram()
	await program.parseAsync(process.argv)
}

const isMainModule =
	process.argv[1] !== undefined &&
	pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url

if (isMainModule) {
	void main().catch((caught: unknown) => {
		if (isAppError(caught)) {
			printFailure(caught.code, caught.message)
		} else if (caught instanceof Error) {
			printFailure('UNHANDLED_ERROR', caught.message)
		} else {
			printFailure('UNHANDLED_ERROR', 'Unknown error.')
		}
		process.exitCode = 1
	})
}
