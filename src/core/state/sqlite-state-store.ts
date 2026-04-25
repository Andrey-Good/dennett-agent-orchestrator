import { randomUUID } from 'node:crypto'
import { mkdirSync } from 'node:fs'
import path from 'node:path'
import { DatabaseSync } from 'node:sqlite'
import { AppError } from '../errors.js'
import type { JsonObject, JsonValue } from '../json.js'
import type {
	AgentLifecycleStatusRecord,
	AgentRecord,
	AgentRevisionAvailabilityState,
	AgentRevisionKind,
	AgentRevisionRecord,
	AppendVisibleChatMessageInput,
	ChatPolicySnapshot,
	ChatRecord,
	CloseManagedSubagentInput,
	CommitBlockedAttemptInput,
	CommitNodeSuccessInput,
	CommitNodeTerminalOutcomeInput,
	CreateEventRecordInput,
	CreateManagedSubagentRecordInput,
	CreateRunInput,
	DurableBoundaryKind,
	EnsureChatRecordInput,
	EventDispatchStatus,
	EventRecord,
	ManagedSubagentCloseDisposition,
	ManagedSubagentId,
	ManagedSubagentRecord,
	ManagedSubagentState,
	MarkEventDispatchedInput,
	MarkEventDispatchFailedInput,
	MarkManagedSubagentTerminalInput,
	MemoryProviderCapability,
	MemoryProviderRecord,
	MemoryProviderStatus,
	MemoryProviderTransport,
	NodeAttemptRecord,
	NodeOutputJournalRecord,
	PendingUserPromptRecord,
	PersistedRunSnapshot,
	PromoteAgentRevisionInput,
	ResumeMetadataInput,
	ResumeMetadataRecord,
	RunRecord,
	RunStatus,
	SetAgentLiveRevisionInput,
	StartNodeAttemptInput,
	TriggerRecord,
	UpdateMemoryProviderStatusInput,
	UpsertAgentRecordInput,
	UpsertAgentRevisionInput,
	UpsertMemoryProviderInput,
	UpsertTriggerRecordInput,
	VisibleChatMessageKind,
	VisibleChatMessageRecord,
} from './types.js'

interface ChatRow {
	chat_id: string
	run_id: string
	resolved_revision_id: string
	prefer_native_resume: number
	store_visible_messages: number
	store_context_window: number
	allow_fresh_start: number
	created_at: string
	updated_at: string
}

interface VisibleChatMessageRow {
	message_id: string
	chat_id: string
	run_id: string
	message_sequence: number
	kind: VisibleChatMessageKind
	payload_json: string
	created_at: string
}

interface RunRow {
	run_id: string
	logical_agent_id: string | null
	resolved_revision_id: string
	entry_node_id: string
	started_via: RunRecord['started_via']
	status: RunStatus
	params_json: string
	event_json: string | null
	last_attempt_sequence: number
	last_boundary_sequence: number
	created_at: string
	updated_at: string
}

interface NodeAttemptRow {
	attempt_id: string
	run_id: string
	node_id: string
	attempt_sequence: number
	output_mode: NodeAttemptRecord['output_mode']
	state: NodeAttemptRecord['state']
	outcome: NodeAttemptRecord['outcome']
	blocked_on_user_prompt: number
	runtime_handle_json: string | null
	committed_output_id: string | null
	resume_boundary_sequence: number | null
	started_at: string
	committed_at: string | null
}

interface NodeOutputRow {
	output_id: string
	run_id: string
	node_id: string
	attempt_id: string
	output_mode: NodeOutputJournalRecord['output']['mode']
	output_payload_json: string
	committed_at: string
	boundary_sequence: number
}

interface VarsSnapshotRow {
	run_id: string
	vars_json: string
	boundary_sequence: number
	updated_at: string
}

interface ResumeMetadataRow {
	run_id: string
	resolved_revision_id: string
	native_resume_available: number
	local_resume_available: number
	last_durable_boundary_sequence: number | null
	last_durable_boundary_kind: DurableBoundaryKind | null
	last_attempt_id: string | null
	pending_prompt_json: string | null
	native_session_handle_json: string | null
	local_context_snapshot_json: string | null
	updated_at: string
}

interface AgentRecordRow {
	logical_agent_id: string
	live_revision_id: string | null
	created_at: string
	updated_at: string
}

interface AgentRevisionRow {
	revision_id: string
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

interface TriggerRow {
	trigger_id: string
	logical_agent_id: string
	trigger_ref: string
	created_at: string
	updated_at: string
}

interface EventRow {
	event_id: string
	trigger_id: string
	logical_agent_id: string
	payload_json: string | null
	launch_note: string | null
	dispatch_status: EventDispatchStatus
	run_id: string | null
	resolved_revision_id: string | null
	dispatch_error_code: string | null
	dispatch_error_message: string | null
	created_at: string
	dispatched_at: string | null
	updated_at: string
}

interface MemoryProviderRow {
	provider_id: string
	codex_ref: string
	provider_family: string
	display_name: string | null
	transport: MemoryProviderTransport
	status: MemoryProviderStatus
	supported_capabilities_json: string
	config_json: string
	status_code: string | null
	status_message: string | null
	last_checked_at: string | null
	created_at: string
	updated_at: string
}

interface ManagedSubagentRow {
	subagent_id: string
	child_run_id: string
	child_role: string
	child_logical_agent_id: string
	child_resolved_revision_id: string
	lineage_json: string
	task_package_json: string
	state: ManagedSubagentState
	terminal_result_json: string | null
	close_disposition: string | null
	created_at: string
	updated_at: string
	terminal_at: string | null
	closed_at: string | null
}

function nowIso(): string {
	return new Date().toISOString()
}

function toSqliteBoolean(value: boolean): number {
	return value ? 1 : 0
}

function fromSqliteBoolean(value: number): boolean {
	return value === 1
}

function stringifyJson(value: JsonValue): string {
	return JSON.stringify(value)
}

function stringifyOptionalJson(value: JsonValue | null | undefined): string | null {
	return value === null || value === undefined ? null : JSON.stringify(value)
}

function parseJson<T extends JsonValue>(value: string): T {
	return JSON.parse(value) as T
}

function parseOptionalJson<T extends JsonValue>(value: string | null): T | null {
	return value === null ? null : (JSON.parse(value) as T)
}

function normalizeCapabilityList(
	capabilities: readonly MemoryProviderCapability[] | undefined,
): MemoryProviderCapability[] {
	return [...new Set(capabilities ?? [])]
}

function normalizeManagedSubagentState(state: ManagedSubagentState): ManagedSubagentState {
	switch (state) {
		case 'running':
		case 'cancelling':
		case 'terminal':
		case 'closed':
			return state
	}
}

function normalizeManagedResourceRef(resourceRef: string): string {
	const normalized = resourceRef.replaceAll('\\', '/').trim()
	if (normalized.length <= 1) {
		return normalized
	}
	return normalized.replace(/\/+$/, '')
}

function isSameOrDescendantResource(left: string, right: string): boolean {
	return right === left || right.startsWith(`${left}/`)
}

function normalizeResumeInput(resume?: ResumeMetadataInput) {
	return {
		native_resume_available: resume?.native_resume_available ?? false,
		local_resume_available: resume?.local_resume_available ?? false,
		native_session_handle: resume?.native_session_handle ?? null,
		local_context_snapshot: resume?.local_context_snapshot ?? null,
	}
}

function normalizeStoredLocalContextSnapshot(
	local_context_snapshot: JsonValue | null | undefined,
	store_context_window: boolean,
): JsonValue | null {
	return store_context_window ? (local_context_snapshot ?? null) : null
}

function resolveChatPolicySnapshot(policy?: EnsureChatRecordInput['policy']): ChatPolicySnapshot {
	return {
		prefer_native_resume: policy?.prefer_native_resume ?? true,
		store_visible_messages: policy?.store_visible_messages ?? true,
		store_context_window: policy?.store_context_window ?? true,
		allow_fresh_start: policy?.allow_fresh_start ?? true,
	}
}

function chatPoliciesEqual(left: ChatPolicySnapshot, right: ChatPolicySnapshot): boolean {
	return (
		left.prefer_native_resume === right.prefer_native_resume &&
		left.store_visible_messages === right.store_visible_messages &&
		left.store_context_window === right.store_context_window &&
		left.allow_fresh_start === right.allow_fresh_start
	)
}

export interface SQLiteLocalStateStoreOptions {
	database_path: string
}

export class SQLiteLocalStateStore {
	readonly database_path: string
	private readonly database: DatabaseSync

	constructor(options: SQLiteLocalStateStoreOptions) {
		this.database_path = options.database_path
		mkdirSync(path.dirname(this.database_path), { recursive: true })
		this.database = new DatabaseSync(this.database_path)
		this.database.exec('PRAGMA foreign_keys = ON;')
		this.database.exec('PRAGMA journal_mode = WAL;')
		this.database.exec('PRAGMA synchronous = FULL;')
		this.database.exec('PRAGMA busy_timeout = 5000;')
		this.initializeSchema()
	}

	close(): void {
		this.database.close()
	}

	createRun(input: CreateRunInput): RunRecord {
		const createdAt = input.created_at ?? nowIso()
		const runId = input.run_id ?? randomUUID()
		const params = input.params ?? {}
		const event = input.event ?? null
		const initialVars = input.initial_vars ?? {}
		const resume = normalizeResumeInput(input.resume)
		const chatPolicy = resolveChatPolicySnapshot(input.chat?.policy)
		const chatId = input.chat?.chat_id ?? randomUUID()

		return this.withTransaction(() => {
			this.database
				.prepare(
					`
            INSERT INTO runs (
              run_id,
              logical_agent_id,
              resolved_revision_id,
              entry_node_id,
              started_via,
              status,
              params_json,
              event_json,
              last_attempt_sequence,
              last_boundary_sequence,
              created_at,
              updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, 0, ?, ?)
          `,
				)
				.run(
					runId,
					input.logical_agent_id ?? null,
					input.resolved_revision_id,
					input.entry_node_id,
					input.started_via,
					'running',
					stringifyJson(params),
					stringifyOptionalJson(event),
					createdAt,
					createdAt,
				)

			this.database
				.prepare(
					`
            INSERT INTO vars_snapshots (run_id, vars_json, boundary_sequence, updated_at)
            VALUES (?, ?, 0, ?)
          `,
				)
				.run(runId, stringifyJson(initialVars), createdAt)

			this.database
				.prepare(
					`
            INSERT INTO chat_records (
              chat_id,
              run_id,
              resolved_revision_id,
              prefer_native_resume,
              store_visible_messages,
              store_context_window,
              allow_fresh_start,
              created_at,
              updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
          `,
				)
				.run(
					chatId,
					runId,
					input.resolved_revision_id,
					toSqliteBoolean(chatPolicy.prefer_native_resume),
					toSqliteBoolean(chatPolicy.store_visible_messages),
					toSqliteBoolean(chatPolicy.store_context_window),
					toSqliteBoolean(chatPolicy.allow_fresh_start),
					createdAt,
					createdAt,
				)

			this.database
				.prepare(
					`
            INSERT INTO resume_metadata (
              run_id,
              resolved_revision_id,
              native_resume_available,
              local_resume_available,
              last_durable_boundary_sequence,
              last_durable_boundary_kind,
              last_attempt_id,
              pending_prompt_json,
              native_session_handle_json,
              local_context_snapshot_json,
              updated_at
            ) VALUES (?, ?, ?, ?, NULL, NULL, NULL, NULL, ?, ?, ?)
          `,
				)
				.run(
					runId,
					input.resolved_revision_id,
					toSqliteBoolean(resume.native_resume_available),
					toSqliteBoolean(resume.local_resume_available),
					stringifyOptionalJson(resume.native_session_handle),
					stringifyOptionalJson(
						normalizeStoredLocalContextSnapshot(
							resume.local_context_snapshot,
							chatPolicy.store_context_window,
						),
					),
					createdAt,
				)

			return this.getRunOrThrow(runId)
		})
	}

	startNodeAttempt(input: StartNodeAttemptInput): NodeAttemptRecord {
		const attemptId = input.attempt_id ?? randomUUID()
		const startedAt = input.started_at ?? nowIso()

		return this.withTransaction(() => {
			const run = this.getRunRowOrThrow(input.run_id)
			if (run.status !== 'running') {
				throw new AppError(
					'INVALID_RUN_STATUS',
					`Run "${input.run_id}" is "${run.status}" and cannot start a new node attempt.`,
				)
			}
			const activeAttempt = this.getInProgressAttemptForRun(input.run_id)
			if (activeAttempt) {
				throw new AppError(
					'ACTIVE_NODE_ATTEMPT_EXISTS',
					`Run "${input.run_id}" already has in-progress attempt "${activeAttempt.attempt_id}".`,
				)
			}

			const nextAttemptSequence = run.last_attempt_sequence + 1

			this.database
				.prepare(
					`
            INSERT INTO node_attempts (
              attempt_id,
              run_id,
              node_id,
              attempt_sequence,
              output_mode,
              state,
              outcome,
              blocked_on_user_prompt,
              runtime_handle_json,
              committed_output_id,
              resume_boundary_sequence,
              started_at,
              committed_at
            ) VALUES (?, ?, ?, ?, ?, 'in_progress', NULL, 0, ?, NULL, NULL, ?, NULL)
          `,
				)
				.run(
					attemptId,
					input.run_id,
					input.node_id,
					nextAttemptSequence,
					input.output_mode,
					stringifyOptionalJson(input.runtime_handle ?? null),
					startedAt,
				)

			this.database
				.prepare(
					`
            UPDATE runs
            SET last_attempt_sequence = ?, updated_at = ?
            WHERE run_id = ?
          `,
				)
				.run(nextAttemptSequence, startedAt, input.run_id)

			return this.getNodeAttemptOrThrow(attemptId)
		})
	}

	commitNodeSuccess(input: CommitNodeSuccessInput): NodeAttemptRecord {
		const committedAt = input.committed_at ?? nowIso()
		const resume = normalizeResumeInput(input.resume)

		return this.withTransaction(() => {
			const attempt = this.getMutableAttempt(input.attempt_id)
			if (attempt.output_mode !== input.output.mode) {
				throw new AppError(
					'INVALID_OUTPUT_MODE',
					`Node attempt "${attempt.attempt_id}" expects "${attempt.output_mode}" output, but received "${input.output.mode}".`,
				)
			}
			const boundarySequence = this.nextBoundarySequence(attempt.run_id)
			const outputId = randomUUID()

			this.database
				.prepare(
					`
            INSERT INTO node_output_journal (
              output_id,
              run_id,
              node_id,
              attempt_id,
              output_mode,
              output_payload_json,
              committed_at,
              boundary_sequence
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
          `,
				)
				.run(
					outputId,
					attempt.run_id,
					attempt.node_id,
					attempt.attempt_id,
					input.output.mode,
					input.output.mode === 'text'
						? stringifyJson(input.output.text)
						: stringifyJson(input.output.json),
					committedAt,
					boundarySequence,
				)

			this.database
				.prepare(
					`
            UPDATE vars_snapshots
            SET vars_json = ?, boundary_sequence = ?, updated_at = ?
            WHERE run_id = ?
          `,
				)
				.run(stringifyJson(input.vars), boundarySequence, committedAt, attempt.run_id)

			this.database
				.prepare(
					`
            UPDATE node_attempts
            SET
              state = 'committed_terminal',
              outcome = 'success',
              blocked_on_user_prompt = 0,
              committed_output_id = ?,
              resume_boundary_sequence = ?,
              committed_at = ?
            WHERE attempt_id = ?
          `,
				)
				.run(outputId, boundarySequence, committedAt, attempt.attempt_id)

			this.writeResumeMetadata({
				run_id: attempt.run_id,
				resolved_revision_id: attempt.run.resolved_revision_id,
				boundary_sequence: boundarySequence,
				boundary_kind: 'node_attempt_terminal',
				attempt_id: attempt.attempt_id,
				pending_prompt: null,
				resume,
				updated_at: committedAt,
			})

			this.database
				.prepare(
					`
            UPDATE runs
            SET status = ?, last_boundary_sequence = ?, updated_at = ?
            WHERE run_id = ?
          `,
				)
				.run(input.run_status, boundarySequence, committedAt, attempt.run_id)

			return this.getNodeAttemptOrThrow(attempt.attempt_id)
		})
	}

	commitNodeTerminalOutcome(input: CommitNodeTerminalOutcomeInput): NodeAttemptRecord {
		const committedAt = input.committed_at ?? nowIso()
		const resume = normalizeResumeInput(input.resume)

		return this.withTransaction(() => {
			const attempt = this.getMutableAttempt(input.attempt_id)
			const boundarySequence = this.nextBoundarySequence(attempt.run_id)

			this.database
				.prepare(
					`
            UPDATE node_attempts
            SET
              state = 'committed_terminal',
              outcome = ?,
              blocked_on_user_prompt = 0,
              committed_output_id = NULL,
              resume_boundary_sequence = ?,
              committed_at = ?
            WHERE attempt_id = ?
          `,
				)
				.run(input.outcome, boundarySequence, committedAt, attempt.attempt_id)

			this.writeResumeMetadata({
				run_id: attempt.run_id,
				resolved_revision_id: attempt.run.resolved_revision_id,
				boundary_sequence: boundarySequence,
				boundary_kind: 'node_attempt_terminal',
				attempt_id: attempt.attempt_id,
				pending_prompt: null,
				resume,
				updated_at: committedAt,
			})

			this.database
				.prepare(
					`
            UPDATE runs
            SET status = ?, last_boundary_sequence = ?, updated_at = ?
            WHERE run_id = ?
          `,
				)
				.run(input.run_status, boundarySequence, committedAt, attempt.run_id)

			return this.getNodeAttemptOrThrow(attempt.attempt_id)
		})
	}

	commitBlockedAttempt(input: CommitBlockedAttemptInput): NodeAttemptRecord {
		const committedAt = input.committed_at ?? nowIso()
		const resume = normalizeResumeInput(input.resume)

		return this.withTransaction(() => {
			const attempt = this.getMutableAttempt(input.attempt_id)
			const boundarySequence = this.nextBoundarySequence(attempt.run_id)
			const pendingPrompt: PendingUserPromptRecord = {
				run_id: attempt.run_id,
				attempt_id: attempt.attempt_id,
				prompt_id: input.pending_prompt.prompt_id ?? null,
				payload: input.pending_prompt.payload,
				request_handle: input.pending_prompt.request_handle ?? null,
				unresolved: true,
				blocks_forward_progress: true,
			}

			this.database
				.prepare(
					`
            UPDATE node_attempts
            SET
              state = 'blocked_wait',
              outcome = NULL,
              blocked_on_user_prompt = 1,
              committed_output_id = NULL,
              resume_boundary_sequence = ?,
              committed_at = ?
            WHERE attempt_id = ?
          `,
				)
				.run(boundarySequence, committedAt, attempt.attempt_id)

			this.writeResumeMetadata({
				run_id: attempt.run_id,
				resolved_revision_id: attempt.run.resolved_revision_id,
				boundary_sequence: boundarySequence,
				boundary_kind: 'blocked_prompt_wait',
				attempt_id: attempt.attempt_id,
				pending_prompt: pendingPrompt,
				resume,
				updated_at: committedAt,
			})

			this.database
				.prepare(
					`
            UPDATE runs
            SET status = 'waiting_for_user', last_boundary_sequence = ?, updated_at = ?
            WHERE run_id = ?
          `,
				)
				.run(boundarySequence, committedAt, attempt.run_id)

			return this.getNodeAttemptOrThrow(attempt.attempt_id)
		})
	}

	reopenRunForExplicitResume(runId: string, resolvedRevisionId: string): RunRecord {
		const reopenedAt = nowIso()

		return this.withTransaction(() => {
			const run = this.getRunOrThrow(runId)
			const resume = this.getResumeMetadataOrThrow(runId)

			if (run.resolved_revision_id !== resolvedRevisionId) {
				throw new AppError(
					'RESUME_REVISION_MISMATCH',
					`Run "${runId}" is pinned to "${run.resolved_revision_id}" but resume requested "${resolvedRevisionId}".`,
				)
			}

			if (!resume.local_resume_available) {
				throw new AppError(
					'RUN_NOT_RESUMABLE',
					`Run "${runId}" is not available for explicit local resume.`,
				)
			}

			if (run.status === 'cancelled' || run.status === 'completed') {
				throw new AppError(
					'RUN_NOT_RESUMABLE',
					`Run "${runId}" is "${run.status}" and cannot be explicitly resumed.`,
				)
			}

			if (run.status === 'running') {
				return run
			}

			if (
				run.status !== 'failed' &&
				run.status !== 'interrupted' &&
				run.status !== 'waiting_for_user'
			) {
				throw new AppError(
					'INVALID_RUN_STATUS',
					`Run "${runId}" is "${run.status}" and cannot be reopened for explicit local resume.`,
				)
			}

			this.database
				.prepare(
					`
            UPDATE runs
            SET status = 'running', updated_at = ?
            WHERE run_id = ?
          `,
				)
				.run(reopenedAt, runId)

			return this.getRunOrThrow(runId)
		})
	}

	ensureChatRecord(input: EnsureChatRecordInput): ChatRecord {
		const createdAt = input.created_at ?? nowIso()
		const requestedPolicy = resolveChatPolicySnapshot(input.policy)

		return this.withTransaction(() => {
			const run = this.getRunOrThrow(input.run_id)
			const existing = this.getChatRecord(input.run_id)

			if (existing) {
				if (input.chat_id && existing.chat_id !== input.chat_id) {
					throw new AppError(
						'CHAT_RECORD_CONFLICT',
						`Run "${input.run_id}" is already bound to chat "${existing.chat_id}", not "${input.chat_id}".`,
					)
				}
				if (
					input.resolved_revision_id &&
					existing.resolved_revision_id !== input.resolved_revision_id
				) {
					throw new AppError(
						'RESUME_REVISION_MISMATCH',
						`Run "${input.run_id}" is pinned to "${existing.resolved_revision_id}" but chat requested "${input.resolved_revision_id}".`,
					)
				}
				if (input.policy && !chatPoliciesEqual(existing.policy, requestedPolicy)) {
					throw new AppError(
						'CHAT_POLICY_MISMATCH',
						`Run "${input.run_id}" already has a different persisted chat policy snapshot.`,
					)
				}
				return existing
			}

			const resolvedRevisionId = input.resolved_revision_id ?? run.resolved_revision_id
			if (resolvedRevisionId !== run.resolved_revision_id) {
				throw new AppError(
					'RESUME_REVISION_MISMATCH',
					`Run "${input.run_id}" is pinned to "${run.resolved_revision_id}" but chat requested "${resolvedRevisionId}".`,
				)
			}

			this.database
				.prepare(
					`
            INSERT INTO chat_records (
              chat_id,
              run_id,
              resolved_revision_id,
              prefer_native_resume,
              store_visible_messages,
              store_context_window,
              allow_fresh_start,
              created_at,
              updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
          `,
				)
				.run(
					input.chat_id ?? randomUUID(),
					input.run_id,
					resolvedRevisionId,
					toSqliteBoolean(requestedPolicy.prefer_native_resume),
					toSqliteBoolean(requestedPolicy.store_visible_messages),
					toSqliteBoolean(requestedPolicy.store_context_window),
					toSqliteBoolean(requestedPolicy.allow_fresh_start),
					createdAt,
					createdAt,
				)

			return this.getChatRecordOrThrow(input.run_id)
		})
	}

	appendVisibleChatMessage(input: AppendVisibleChatMessageInput): VisibleChatMessageRecord | null {
		const createdAt = input.created_at ?? nowIso()
		const messageId = input.message_id ?? randomUUID()

		return this.withTransaction(() => {
			const chat = this.getChatRecordOrThrow(input.run_id)
			if (!chat.policy.store_visible_messages) {
				this.database
					.prepare(
						`
              UPDATE chat_records
              SET updated_at = ?
              WHERE run_id = ?
            `,
					)
					.run(createdAt, input.run_id)
				return null
			}

			const nextMessageSequence = this.getNextVisibleMessageSequence(chat.chat_id)

			this.database
				.prepare(
					`
            INSERT INTO visible_chat_messages (
              message_id,
              chat_id,
              run_id,
              message_sequence,
              kind,
              payload_json,
              created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
          `,
				)
				.run(
					messageId,
					chat.chat_id,
					input.run_id,
					nextMessageSequence,
					input.kind,
					stringifyJson(input.payload),
					createdAt,
				)

			this.database
				.prepare(
					`
            UPDATE chat_records
            SET updated_at = ?
            WHERE run_id = ?
          `,
				)
				.run(createdAt, input.run_id)

			return this.getVisibleChatMessageOrThrow(messageId)
		})
	}

	getChatRecord(runId: string): ChatRecord | null {
		const row = this.database
			.prepare(
				`
          SELECT
            chat_id,
            run_id,
            resolved_revision_id,
            prefer_native_resume,
            store_visible_messages,
            store_context_window,
            allow_fresh_start,
            created_at,
            updated_at
          FROM chat_records
          WHERE run_id = ?
        `,
			)
			.get(runId) as ChatRow | undefined

		return row ? this.mapChatRow(row) : null
	}

	listVisibleChatMessages(runId: string): VisibleChatMessageRecord[] {
		const rows = this.database
			.prepare(
				`
          SELECT
            message_id,
            chat_id,
            run_id,
            message_sequence,
            kind,
            payload_json,
            created_at
          FROM visible_chat_messages
          WHERE run_id = ?
          ORDER BY message_sequence ASC
        `,
			)
			.all(runId) as unknown as VisibleChatMessageRow[]

		return rows.map((row) => this.mapVisibleChatMessageRow(row))
	}

	getRun(runId: string): RunRecord | null {
		const row = this.database
			.prepare(
				`
          SELECT
            run_id,
            logical_agent_id,
            resolved_revision_id,
            entry_node_id,
            started_via,
            status,
            params_json,
            event_json,
            last_attempt_sequence,
            last_boundary_sequence,
            created_at,
            updated_at
          FROM runs
          WHERE run_id = ?
        `,
			)
			.get(runId) as RunRow | undefined

		return row ? this.mapRunRow(row) : null
	}

	listNodeAttempts(runId: string): NodeAttemptRecord[] {
		const rows = this.database
			.prepare(
				`
          SELECT
            attempt_id,
            run_id,
            node_id,
            attempt_sequence,
            output_mode,
            state,
            outcome,
            blocked_on_user_prompt,
            runtime_handle_json,
            committed_output_id,
            resume_boundary_sequence,
            started_at,
            committed_at
          FROM node_attempts
          WHERE run_id = ?
          ORDER BY attempt_sequence ASC
        `,
			)
			.all(runId) as unknown as NodeAttemptRow[]

		return rows.map((row) => this.mapNodeAttemptRow(row))
	}

	getManagedSubagentRecord(subagentId: ManagedSubagentId): ManagedSubagentRecord | null {
		const row = this.database
			.prepare(
				`
          SELECT
            subagent_id,
            child_run_id,
            child_role,
            child_logical_agent_id,
            child_resolved_revision_id,
            lineage_json,
            task_package_json,
            state,
            terminal_result_json,
            close_disposition,
            created_at,
            updated_at,
            terminal_at,
            closed_at
          FROM managed_subagents
          WHERE subagent_id = ?
        `,
			)
			.get(subagentId) as ManagedSubagentRow | undefined

		return row ? this.mapManagedSubagentRow(row) : null
	}

	listManagedSubagentRecords(filters?: {
		parent_run_id?: string
		parent_task_id?: string
		state?: ManagedSubagentState
	}): ManagedSubagentRecord[] {
		const whereClauses: string[] = []
		const params: Array<string> = []

		if (filters?.parent_run_id) {
			whereClauses.push("json_extract(lineage_json, '$.parent_run_id') = ?")
			params.push(filters.parent_run_id)
		}
		if (filters?.parent_task_id) {
			whereClauses.push("json_extract(lineage_json, '$.parent_task_id') = ?")
			params.push(filters.parent_task_id)
		}
		if (filters?.state) {
			whereClauses.push('state = ?')
			params.push(filters.state)
		}

		const whereSql = whereClauses.length > 0 ? `WHERE ${whereClauses.join(' AND ')}` : ''
		const rows = this.database
			.prepare(
				`
          SELECT
            subagent_id,
            child_run_id,
            child_role,
            child_logical_agent_id,
            child_resolved_revision_id,
            lineage_json,
            task_package_json,
            state,
            terminal_result_json,
            close_disposition,
            created_at,
            updated_at,
            terminal_at,
            closed_at
          FROM managed_subagents
          ${whereSql}
          ORDER BY created_at ASC
        `,
			)
			.all(...params) as unknown as ManagedSubagentRow[]

		return rows.map((row) => this.mapManagedSubagentRow(row))
	}

	getCurrentVars(runId: string): Record<string, JsonValue> {
		const row = this.database
			.prepare('SELECT vars_json FROM vars_snapshots WHERE run_id = ?')
			.get(runId) as Pick<VarsSnapshotRow, 'vars_json'> | undefined

		if (!row) {
			throw new AppError('RUN_NOT_FOUND', `Run "${runId}" does not exist.`)
		}

		return parseJson<JsonObject>(row.vars_json)
	}

	getResumeMetadata(runId: string): ResumeMetadataRecord | null {
		const row = this.database
			.prepare(
				`
          SELECT
            run_id,
            resolved_revision_id,
            native_resume_available,
            local_resume_available,
            last_durable_boundary_sequence,
            last_durable_boundary_kind,
            last_attempt_id,
            pending_prompt_json,
            native_session_handle_json,
            local_context_snapshot_json,
            updated_at
          FROM resume_metadata
          WHERE run_id = ?
        `,
			)
			.get(runId) as ResumeMetadataRow | undefined

		return row ? this.mapResumeMetadataRow(row) : null
	}

	upsertMemoryProviderRecord(input: UpsertMemoryProviderInput): MemoryProviderRecord {
		const timestamp = input.updated_at ?? input.created_at ?? nowIso()
		const createdAt = input.created_at ?? timestamp
		const providerId = input.provider_id ?? randomUUID()
		const supportedCapabilities = normalizeCapabilityList(input.supported_capabilities)

		return this.withTransaction(() => {
			const existing = this.getMemoryProviderRecord(providerId)
			if (existing) {
				this.database
					.prepare(
						`
              UPDATE memory_provider_registrations
              SET
                codex_ref = ?,
                provider_family = ?,
                display_name = ?,
                transport = ?,
                status = ?,
                supported_capabilities_json = ?,
                config_json = ?,
                status_code = ?,
                status_message = ?,
                last_checked_at = ?,
                updated_at = ?
              WHERE provider_id = ?
            `,
					)
					.run(
						input.codex_ref,
						input.provider_family,
						input.display_name ?? null,
						input.transport,
						input.status ?? existing.status,
						stringifyJson(supportedCapabilities),
						stringifyJson(input.config ?? {}),
						input.status_code ?? null,
						input.status_message ?? null,
						input.last_checked_at ?? null,
						timestamp,
						providerId,
					)
				return this.getMemoryProviderRecordOrThrow(providerId)
			}

			this.database
				.prepare(
					`
            INSERT INTO memory_provider_registrations (
              provider_id,
              codex_ref,
              provider_family,
              display_name,
              transport,
              status,
              supported_capabilities_json,
              config_json,
              status_code,
              status_message,
              last_checked_at,
              created_at,
              updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
          `,
				)
				.run(
					providerId,
					input.codex_ref,
					input.provider_family,
					input.display_name ?? null,
					input.transport,
					input.status ?? 'configured',
					stringifyJson(supportedCapabilities),
					stringifyJson(input.config ?? {}),
					input.status_code ?? null,
					input.status_message ?? null,
					input.last_checked_at ?? null,
					createdAt,
					timestamp,
				)

			return this.getMemoryProviderRecordOrThrow(providerId)
		})
	}

	updateMemoryProviderStatus(input: UpdateMemoryProviderStatusInput): MemoryProviderRecord {
		const timestamp = input.updated_at ?? input.last_checked_at ?? nowIso()
		this.getMemoryProviderRecordOrThrow(input.provider_id)

		this.database
			.prepare(
				`
          UPDATE memory_provider_registrations
          SET
            status = ?,
            status_code = ?,
            status_message = ?,
            last_checked_at = ?,
            updated_at = ?
          WHERE provider_id = ?
        `,
			)
			.run(
				input.status,
				input.status_code ?? null,
				input.status_message ?? null,
				input.last_checked_at ?? timestamp,
				timestamp,
				input.provider_id,
			)

		return this.getMemoryProviderRecordOrThrow(input.provider_id)
	}

	createManagedSubagentRecord(input: CreateManagedSubagentRecordInput): ManagedSubagentRecord {
		const timestamp = input.updated_at ?? input.created_at ?? nowIso()
		const createdAt = input.created_at ?? timestamp
		const subagentId = input.subagent_id ?? randomUUID()

		return this.withTransaction(() => {
			this.getRunOrThrow(input.lineage.parent_run_id)
			const siblingConflicts = this.listManagedSubagentRecords({
				parent_run_id: input.lineage.parent_run_id,
			}).filter(
				(existing) =>
					existing.state !== 'closed' &&
					this.managedWriteSetsConflict(
						existing.task_package.write_set,
						input.task_package.write_set,
					),
			)

			if (siblingConflicts.length > 0) {
				throw new AppError(
					'SUBAGENT_WRITE_SET_CONFLICT',
					`Managed subagent launch conflicts with active sibling subagent "${siblingConflicts[0]?.subagent_id}" for parent run "${input.lineage.parent_run_id}".`,
				)
			}

			this.database
				.prepare(
					`
            INSERT INTO managed_subagents (
              subagent_id,
              child_run_id,
              child_role,
              child_logical_agent_id,
              child_resolved_revision_id,
              lineage_json,
              task_package_json,
              state,
              terminal_result_json,
              close_disposition,
              created_at,
              updated_at,
              terminal_at,
              closed_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, 'running', NULL, NULL, ?, ?, NULL, NULL)
          `,
				)
				.run(
					subagentId,
					input.child_run_id,
					input.child_role,
					input.child_logical_agent_id,
					input.child_resolved_revision_id,
					stringifyJson(input.lineage as unknown as JsonValue),
					stringifyJson(input.task_package as unknown as JsonValue),
					createdAt,
					timestamp,
				)

			return this.getManagedSubagentRecordOrThrow(subagentId)
		})
	}

	updateManagedSubagentTaskPackage(input: {
		subagent_id: ManagedSubagentId
		task_package: ManagedSubagentRecord['task_package']
		updated_at?: string
	}): ManagedSubagentRecord {
		const updatedAt = input.updated_at ?? nowIso()
		this.getManagedSubagentRecordOrThrow(input.subagent_id)

		this.database
			.prepare(
				`
          UPDATE managed_subagents
          SET
            task_package_json = ?,
            updated_at = ?
          WHERE subagent_id = ?
        `,
			)
			.run(stringifyJson(input.task_package as unknown as JsonValue), updatedAt, input.subagent_id)

		return this.getManagedSubagentRecordOrThrow(input.subagent_id)
	}

	markManagedSubagentTerminal(input: MarkManagedSubagentTerminalInput): ManagedSubagentRecord {
		const terminalAt = input.terminal_at ?? nowIso()
		const existing = this.getManagedSubagentRecordOrThrow(input.subagent_id)
		if (existing.state === 'closed') {
			throw new AppError(
				'SUBAGENT_ALREADY_CLOSED',
				`Managed subagent "${input.subagent_id}" is already closed.`,
			)
		}

		this.database
			.prepare(
				`
          UPDATE managed_subagents
          SET
            state = 'terminal',
            terminal_result_json = ?,
            updated_at = ?,
            terminal_at = COALESCE(terminal_at, ?)
          WHERE subagent_id = ?
        `,
			)
			.run(
				stringifyJson(input.terminal_result as unknown as JsonValue),
				terminalAt,
				terminalAt,
				input.subagent_id,
			)

		return this.getManagedSubagentRecordOrThrow(input.subagent_id)
	}

	markManagedSubagentCancelling(input: {
		subagent_id: ManagedSubagentId
		close_disposition: ManagedSubagentCloseDisposition
		updated_at?: string
	}): ManagedSubagentRecord {
		const updatedAt = input.updated_at ?? nowIso()
		const existing = this.getManagedSubagentRecordOrThrow(input.subagent_id)
		if (existing.state === 'closed') {
			return existing
		}

		this.database
			.prepare(
				`
          UPDATE managed_subagents
          SET
            state = 'cancelling',
            close_disposition = ?,
            updated_at = ?
          WHERE subagent_id = ?
        `,
			)
			.run(input.close_disposition, updatedAt, input.subagent_id)

		return this.getManagedSubagentRecordOrThrow(input.subagent_id)
	}

	closeManagedSubagent(input: CloseManagedSubagentInput): ManagedSubagentRecord {
		const closedAt = input.closed_at ?? nowIso()
		const existing = this.getManagedSubagentRecordOrThrow(input.subagent_id)

		if (existing.state === 'closed') {
			return existing
		}

		if (input.close_disposition === 'cancelled_by_parent') {
			if (existing.state !== 'terminal') {
				return this.markManagedSubagentCancelling({
					subagent_id: input.subagent_id,
					close_disposition: input.close_disposition,
					updated_at: closedAt,
				})
			}

			if (existing.terminal_result?.outcome !== 'cancelled') {
				throw new AppError(
					'SUBAGENT_CLOSE_INVALID',
					`Managed subagent "${input.subagent_id}" cannot be cancelled by parent because its terminal outcome is "${existing.terminal_result?.outcome ?? 'unknown'}".`,
				)
			}
		} else {
			if (existing.state !== 'terminal') {
				throw new AppError(
					'SUBAGENT_NOT_TERMINAL',
					`Managed subagent "${input.subagent_id}" must be terminal before it can be closed.`,
				)
			}

			if (
				input.close_disposition === 'accepted_by_parent' &&
				existing.terminal_result?.outcome !== 'accepted'
			) {
				throw new AppError(
					'SUBAGENT_CLOSE_INVALID',
					`Managed subagent "${input.subagent_id}" cannot be accepted by parent because its terminal outcome is "${existing.terminal_result?.outcome ?? 'unknown'}".`,
				)
			}
		}

		this.database
			.prepare(
				`
          UPDATE managed_subagents
          SET
            state = 'closed',
            close_disposition = ?,
            updated_at = ?,
            closed_at = ?
          WHERE subagent_id = ?
        `,
			)
			.run(input.close_disposition, closedAt, closedAt, input.subagent_id)

		return this.getManagedSubagentRecordOrThrow(input.subagent_id)
	}

	upsertAgentRecord(input: UpsertAgentRecordInput): AgentRecord {
		const timestamp = input.updated_at ?? input.created_at ?? nowIso()
		const createdAt = input.created_at ?? timestamp

		return this.withTransaction(() => {
			const existing = this.getAgentRecord(input.logical_agent_id)
			if (existing) {
				this.database
					.prepare(
						`
              UPDATE agent_records
              SET updated_at = ?
              WHERE logical_agent_id = ?
            `,
					)
					.run(timestamp, input.logical_agent_id)
				return this.getAgentRecordOrThrow(input.logical_agent_id)
			}

			this.database
				.prepare(
					`
            INSERT INTO agent_records (
              logical_agent_id,
              live_revision_id,
              created_at,
              updated_at
            ) VALUES (?, NULL, ?, ?)
          `,
				)
				.run(input.logical_agent_id, createdAt, timestamp)

			return this.getAgentRecordOrThrow(input.logical_agent_id)
		})
	}

	upsertTriggerRecord(input: UpsertTriggerRecordInput): TriggerRecord {
		const timestamp = input.updated_at ?? input.created_at ?? nowIso()
		const createdAt = input.created_at ?? timestamp
		const triggerId = input.trigger_id ?? randomUUID()

		return this.withTransaction(() => {
			if (!this.getAgentRecord(input.logical_agent_id)) {
				this.database
					.prepare(
						`
              INSERT INTO agent_records (
                logical_agent_id,
                live_revision_id,
                created_at,
                updated_at
              ) VALUES (?, NULL, ?, ?)
            `,
					)
					.run(input.logical_agent_id, createdAt, timestamp)
			}

			const existing = this.getTriggerRecord(triggerId)
			if (existing) {
				this.database
					.prepare(
						`
              UPDATE triggers
              SET logical_agent_id = ?, trigger_ref = ?, updated_at = ?
              WHERE trigger_id = ?
            `,
					)
					.run(input.logical_agent_id, input.trigger_ref, timestamp, triggerId)
				return this.getTriggerRecordOrThrow(triggerId)
			}

			this.database
				.prepare(
					`
            INSERT INTO triggers (
              trigger_id,
              logical_agent_id,
              trigger_ref,
              created_at,
              updated_at
            ) VALUES (?, ?, ?, ?, ?)
          `,
				)
				.run(triggerId, input.logical_agent_id, input.trigger_ref, createdAt, timestamp)

			return this.getTriggerRecordOrThrow(triggerId)
		})
	}

	createEventRecord(input: CreateEventRecordInput): EventRecord {
		const createdAt = input.created_at ?? nowIso()
		const eventId = input.event_id ?? randomUUID()

		return this.withTransaction(() => {
			const trigger = this.getTriggerRecordOrThrow(input.trigger_id)
			if (trigger.logical_agent_id !== input.logical_agent_id) {
				throw new AppError(
					'INVALID_EVENT_TRIGGER',
					`Trigger "${input.trigger_id}" is bound to "${trigger.logical_agent_id}", not "${input.logical_agent_id}".`,
				)
			}
			this.database
				.prepare(
					`
            INSERT INTO events (
              event_id,
              trigger_id,
              logical_agent_id,
              payload_json,
              launch_note,
              dispatch_status,
              run_id,
              resolved_revision_id,
              dispatch_error_code,
              dispatch_error_message,
              created_at,
              dispatched_at,
              updated_at
            ) VALUES (?, ?, ?, ?, ?, 'pending', NULL, NULL, NULL, NULL, ?, NULL, ?)
          `,
				)
				.run(
					eventId,
					input.trigger_id,
					input.logical_agent_id,
					stringifyOptionalJson(input.payload ?? null),
					input.launch_note ?? null,
					createdAt,
					createdAt,
				)

			return this.getEventRecordOrThrow(eventId)
		})
	}

	markEventDispatched(input: MarkEventDispatchedInput): EventRecord {
		const dispatchedAt = input.dispatched_at ?? nowIso()

		return this.withTransaction(() => {
			this.getEventRecordOrThrow(input.event_id)
			this.database
				.prepare(
					`
            UPDATE events
            SET
              dispatch_status = 'dispatched',
              run_id = ?,
              resolved_revision_id = ?,
              dispatch_error_code = NULL,
              dispatch_error_message = NULL,
              dispatched_at = ?,
              updated_at = ?
            WHERE event_id = ?
          `,
				)
				.run(input.run_id, input.resolved_revision_id, dispatchedAt, dispatchedAt, input.event_id)

			return this.getEventRecordOrThrow(input.event_id)
		})
	}

	markEventDispatchFailed(input: MarkEventDispatchFailedInput): EventRecord {
		const dispatchedAt = input.dispatched_at ?? nowIso()

		return this.withTransaction(() => {
			this.getEventRecordOrThrow(input.event_id)
			this.database
				.prepare(
					`
            UPDATE events
            SET
              dispatch_status = 'failed',
              run_id = NULL,
              resolved_revision_id = NULL,
              dispatch_error_code = ?,
              dispatch_error_message = ?,
              dispatched_at = ?,
              updated_at = ?
            WHERE event_id = ?
          `,
				)
				.run(input.error_code, input.error_message, dispatchedAt, dispatchedAt, input.event_id)

			return this.getEventRecordOrThrow(input.event_id)
		})
	}

	upsertAgentRevision(input: UpsertAgentRevisionInput): AgentRevisionRecord {
		const timestamp = input.updated_at ?? input.created_at ?? nowIso()
		const createdAt = input.created_at ?? timestamp
		const revisionId = input.revision_id ?? randomUUID()

		return this.withTransaction(() => {
			const existingAgent = this.getAgentRecord(input.logical_agent_id)
			if (existingAgent) {
				this.database
					.prepare(
						`
              UPDATE agent_records
              SET updated_at = ?
              WHERE logical_agent_id = ?
            `,
					)
					.run(timestamp, input.logical_agent_id)
			} else {
				this.database
					.prepare(
						`
              INSERT INTO agent_records (
                logical_agent_id,
                live_revision_id,
                created_at,
                updated_at
              ) VALUES (?, NULL, ?, ?)
            `,
					)
					.run(input.logical_agent_id, createdAt, timestamp)
			}

			const existing = this.getAgentRevision(revisionId)
			if (existing) {
				this.database
					.prepare(
						`
              UPDATE agent_revisions
              SET
                revision_kind = ?,
                file_path = ?,
                resolved_revision_id = ?,
                availability_state = ?,
                validation_error = ?,
                validated_at = ?,
                graph_contract_version = ?,
                agent_name = ?,
                agent_description = ?,
                agent_version = ?,
                entry_node_id = ?,
                updated_at = ?
              WHERE revision_id = ?
            `,
					)
					.run(
						input.revision_kind,
						input.file_path,
						input.resolved_revision_id,
						input.availability_state,
						input.validation_error ?? null,
						input.validated_at ?? null,
						input.graph_contract_version ?? null,
						input.agent_name ?? null,
						input.agent_description ?? null,
						input.agent_version ?? null,
						input.entry_node_id ?? null,
						timestamp,
						revisionId,
					)
				return this.getAgentRevisionOrThrow(revisionId)
			}

			this.database
				.prepare(
					`
            INSERT INTO agent_revisions (
              revision_id,
              logical_agent_id,
              revision_kind,
              file_path,
              resolved_revision_id,
              availability_state,
              validation_error,
              validated_at,
              graph_contract_version,
              agent_name,
              agent_description,
              agent_version,
              entry_node_id,
              created_at,
              updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
          `,
				)
				.run(
					revisionId,
					input.logical_agent_id,
					input.revision_kind,
					input.file_path,
					input.resolved_revision_id,
					input.availability_state,
					input.validation_error ?? null,
					input.validated_at ?? null,
					input.graph_contract_version ?? null,
					input.agent_name ?? null,
					input.agent_description ?? null,
					input.agent_version ?? null,
					input.entry_node_id ?? null,
					createdAt,
					timestamp,
				)

			return this.getAgentRevisionOrThrow(revisionId)
		})
	}

	setAgentLiveRevision(input: SetAgentLiveRevisionInput): AgentRecord {
		const timestamp = input.updated_at ?? nowIso()

		return this.withTransaction(() => {
			const existingAgent = this.getAgentRecord(input.logical_agent_id)
			if (existingAgent) {
				this.database
					.prepare(
						`
              UPDATE agent_records
              SET updated_at = ?
              WHERE logical_agent_id = ?
            `,
					)
					.run(timestamp, input.logical_agent_id)
			} else {
				this.database
					.prepare(
						`
              INSERT INTO agent_records (
                logical_agent_id,
                live_revision_id,
                created_at,
                updated_at
              ) VALUES (?, NULL, ?, ?)
            `,
					)
					.run(input.logical_agent_id, timestamp, timestamp)
			}

			this.database
				.prepare(
					`
            UPDATE agent_records
            SET live_revision_id = ?, updated_at = ?
            WHERE logical_agent_id = ?
          `,
				)
				.run(input.live_revision_id, timestamp, input.logical_agent_id)

			return this.getAgentRecordOrThrow(input.logical_agent_id)
		})
	}

	promoteAgentRevision(input: PromoteAgentRevisionInput): {
		agent: AgentRecord
		live_revision: AgentRevisionRecord
	} {
		const timestamp =
			input.updated_at ??
			input.live_revision.updated_at ??
			input.live_revision.created_at ??
			nowIso()
		const liveCreatedAt = input.live_revision.created_at ?? timestamp
		const liveRevisionId = input.live_revision.revision_id ?? randomUUID()

		return this.withTransaction(() => {
			const existingAgent = this.getAgentRecord(input.logical_agent_id)
			if (existingAgent) {
				this.database
					.prepare(
						`
              UPDATE agent_records
              SET updated_at = ?
              WHERE logical_agent_id = ?
            `,
					)
					.run(timestamp, input.logical_agent_id)
			} else {
				this.database
					.prepare(
						`
              INSERT INTO agent_records (
                logical_agent_id,
                live_revision_id,
                created_at,
                updated_at
              ) VALUES (?, NULL, ?, ?)
            `,
					)
					.run(input.logical_agent_id, liveCreatedAt, timestamp)
			}

			if (input.previous_live_revision_id) {
				this.database
					.prepare(
						`
              UPDATE agent_revisions
              SET
                revision_kind = 'historical',
                updated_at = ?
              WHERE revision_id = ? AND logical_agent_id = ?
            `,
					)
					.run(timestamp, input.previous_live_revision_id, input.logical_agent_id)
			}

			const liveRevision = this.getAgentRevision(liveRevisionId)
			if (liveRevision) {
				this.database
					.prepare(
						`
              UPDATE agent_revisions
              SET
                revision_kind = 'live',
                file_path = ?,
                resolved_revision_id = ?,
                availability_state = ?,
                validation_error = ?,
                validated_at = ?,
                graph_contract_version = ?,
                agent_name = ?,
                agent_description = ?,
                agent_version = ?,
                entry_node_id = ?,
                updated_at = ?
              WHERE revision_id = ?
            `,
					)
					.run(
						input.live_revision.file_path,
						input.live_revision.resolved_revision_id,
						input.live_revision.availability_state,
						input.live_revision.validation_error ?? null,
						input.live_revision.validated_at ?? null,
						input.live_revision.graph_contract_version ?? null,
						input.live_revision.agent_name ?? null,
						input.live_revision.agent_description ?? null,
						input.live_revision.agent_version ?? null,
						input.live_revision.entry_node_id ?? null,
						timestamp,
						liveRevisionId,
					)
			} else {
				this.database
					.prepare(
						`
              INSERT INTO agent_revisions (
                revision_id,
                logical_agent_id,
                revision_kind,
                file_path,
                resolved_revision_id,
                availability_state,
                validation_error,
                validated_at,
                graph_contract_version,
                agent_name,
                agent_description,
                agent_version,
                entry_node_id,
                created_at,
                updated_at
              ) VALUES (?, ?, 'live', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            `,
					)
					.run(
						liveRevisionId,
						input.logical_agent_id,
						input.live_revision.file_path,
						input.live_revision.resolved_revision_id,
						input.live_revision.availability_state,
						input.live_revision.validation_error ?? null,
						input.live_revision.validated_at ?? null,
						input.live_revision.graph_contract_version ?? null,
						input.live_revision.agent_name ?? null,
						input.live_revision.agent_description ?? null,
						input.live_revision.agent_version ?? null,
						input.live_revision.entry_node_id ?? null,
						liveCreatedAt,
						timestamp,
					)
			}

			this.database
				.prepare(
					`
            UPDATE agent_records
            SET live_revision_id = ?, updated_at = ?
            WHERE logical_agent_id = ?
          `,
				)
				.run(liveRevisionId, timestamp, input.logical_agent_id)

			return {
				agent: this.getAgentRecordOrThrow(input.logical_agent_id),
				live_revision: this.getAgentRevisionOrThrow(liveRevisionId),
			}
		})
	}

	getAgentRecord(logicalAgentId: string): AgentRecord | null {
		const row = this.database
			.prepare(
				`
          SELECT
            logical_agent_id,
            live_revision_id,
            created_at,
            updated_at
          FROM agent_records
          WHERE logical_agent_id = ?
        `,
			)
			.get(logicalAgentId) as AgentRecordRow | undefined

		return row ? this.mapAgentRecordRow(row) : null
	}

	getMemoryProviderRecord(providerId: string): MemoryProviderRecord | null {
		const row = this.database
			.prepare(
				`
          SELECT
            provider_id,
            codex_ref,
            provider_family,
            display_name,
            transport,
            status,
            supported_capabilities_json,
            config_json,
            status_code,
            status_message,
            last_checked_at,
            created_at,
            updated_at
          FROM memory_provider_registrations
          WHERE provider_id = ?
        `,
			)
			.get(providerId) as MemoryProviderRow | undefined

		return row ? this.mapMemoryProviderRow(row) : null
	}

	getMemoryProviderRecordByCodexRef(codexRef: string): MemoryProviderRecord | null {
		const row = this.database
			.prepare(
				`
          SELECT
            provider_id,
            codex_ref,
            provider_family,
            display_name,
            transport,
            status,
            supported_capabilities_json,
            config_json,
            status_code,
            status_message,
            last_checked_at,
            created_at,
            updated_at
          FROM memory_provider_registrations
          WHERE codex_ref = ?
        `,
			)
			.get(codexRef) as MemoryProviderRow | undefined

		return row ? this.mapMemoryProviderRow(row) : null
	}

	listMemoryProviderRecords(providerFamily?: string): MemoryProviderRecord[] {
		const statement = providerFamily
			? this.database.prepare(
					`
            SELECT
              provider_id,
              codex_ref,
              provider_family,
              display_name,
              transport,
              status,
              supported_capabilities_json,
              config_json,
              status_code,
              status_message,
              last_checked_at,
              created_at,
              updated_at
            FROM memory_provider_registrations
            WHERE provider_family = ?
            ORDER BY created_at ASC
          `,
				)
			: this.database.prepare(
					`
            SELECT
              provider_id,
              codex_ref,
              provider_family,
              display_name,
              transport,
              status,
              supported_capabilities_json,
              config_json,
              status_code,
              status_message,
              last_checked_at,
              created_at,
              updated_at
            FROM memory_provider_registrations
            ORDER BY created_at ASC
          `,
				)

		const rows = (providerFamily
			? statement.all(providerFamily)
			: statement.all()) as unknown as MemoryProviderRow[]

		return rows.map((row) => this.mapMemoryProviderRow(row))
	}

	getAgentRevision(revisionId: string): AgentRevisionRecord | null {
		const row = this.database
			.prepare(
				`
          SELECT
            revision_id,
            logical_agent_id,
            revision_kind,
            file_path,
            resolved_revision_id,
            availability_state,
            validation_error,
            validated_at,
            graph_contract_version,
            agent_name,
            agent_description,
            agent_version,
            entry_node_id,
            created_at,
            updated_at
          FROM agent_revisions
          WHERE revision_id = ?
        `,
			)
			.get(revisionId) as AgentRevisionRow | undefined

		return row ? this.mapAgentRevisionRow(row) : null
	}

	findAgentRevisionByPathAndHash(
		logicalAgentId: string,
		filePath: string,
		resolvedRevisionId: string,
	): AgentRevisionRecord | null {
		const row = this.database
			.prepare(
				`
          SELECT
            revision_id,
            logical_agent_id,
            revision_kind,
            file_path,
            resolved_revision_id,
            availability_state,
            validation_error,
            validated_at,
            graph_contract_version,
            agent_name,
            agent_description,
            agent_version,
            entry_node_id,
            created_at,
            updated_at
          FROM agent_revisions
          WHERE logical_agent_id = ? AND file_path = ? AND resolved_revision_id = ?
          ORDER BY created_at DESC
          LIMIT 1
        `,
			)
			.get(logicalAgentId, filePath, resolvedRevisionId) as AgentRevisionRow | undefined

		return row ? this.mapAgentRevisionRow(row) : null
	}

	listAgentRevisions(logicalAgentId: string): AgentRevisionRecord[] {
		const rows = this.database
			.prepare(
				`
          SELECT
            revision_id,
            logical_agent_id,
            revision_kind,
            file_path,
            resolved_revision_id,
            availability_state,
            validation_error,
            validated_at,
            graph_contract_version,
            agent_name,
            agent_description,
            agent_version,
            entry_node_id,
            created_at,
            updated_at
          FROM agent_revisions
          WHERE logical_agent_id = ?
          ORDER BY created_at ASC
        `,
			)
			.all(logicalAgentId) as unknown as AgentRevisionRow[]

		return rows.map((row) => this.mapAgentRevisionRow(row))
	}

	getAgentLifecycleStatus(logicalAgentId: string): AgentLifecycleStatusRecord | null {
		const agent = this.getAgentRecord(logicalAgentId)
		if (!agent) {
			return null
		}

		const revisions = this.listAgentRevisions(logicalAgentId)
		const liveRevision = agent.live_revision_id
			? (revisions.find((revision) => revision.revision_id === agent.live_revision_id) ?? null)
			: null

		return {
			agent,
			live_revision: liveRevision,
			draft_revisions: revisions.filter((revision) => revision.revision_kind === 'draft'),
			revisions,
		}
	}

	getTriggerRecord(triggerId: string): TriggerRecord | null {
		const row = this.database
			.prepare(
				`
          SELECT
            trigger_id,
            logical_agent_id,
            trigger_ref,
            created_at,
            updated_at
          FROM triggers
          WHERE trigger_id = ?
        `,
			)
			.get(triggerId) as TriggerRow | undefined

		return row ? this.mapTriggerRow(row) : null
	}

	listTriggerRecords(logicalAgentId?: string): TriggerRecord[] {
		const statement = logicalAgentId
			? this.database.prepare(
					`
            SELECT
              trigger_id,
              logical_agent_id,
              trigger_ref,
              created_at,
              updated_at
            FROM triggers
            WHERE logical_agent_id = ?
            ORDER BY created_at ASC
          `,
				)
			: this.database.prepare(
					`
            SELECT
              trigger_id,
              logical_agent_id,
              trigger_ref,
              created_at,
              updated_at
            FROM triggers
            ORDER BY created_at ASC
          `,
				)

		const rows = (logicalAgentId
			? statement.all(logicalAgentId)
			: statement.all()) as unknown as TriggerRow[]

		return rows.map((row) => this.mapTriggerRow(row))
	}

	getEventRecord(eventId: string): EventRecord | null {
		const row = this.database
			.prepare(
				`
          SELECT
            event_id,
            trigger_id,
            logical_agent_id,
            payload_json,
            launch_note,
            dispatch_status,
            run_id,
            resolved_revision_id,
            dispatch_error_code,
            dispatch_error_message,
            created_at,
            dispatched_at,
            updated_at
          FROM events
          WHERE event_id = ?
        `,
			)
			.get(eventId) as EventRow | undefined

		return row ? this.mapEventRow(row) : null
	}

	listEventRecords(filters?: { trigger_id?: string; logical_agent_id?: string }): EventRecord[] {
		const whereClauses: string[] = []
		const params: string[] = []

		if (filters?.trigger_id) {
			whereClauses.push('trigger_id = ?')
			params.push(filters.trigger_id)
		}
		if (filters?.logical_agent_id) {
			whereClauses.push('logical_agent_id = ?')
			params.push(filters.logical_agent_id)
		}

		const whereSql = whereClauses.length > 0 ? `WHERE ${whereClauses.join(' AND ')}` : ''
		const rows = this.database
			.prepare(
				`
          SELECT
            event_id,
            trigger_id,
            logical_agent_id,
            payload_json,
            launch_note,
            dispatch_status,
            run_id,
            resolved_revision_id,
            dispatch_error_code,
            dispatch_error_message,
            created_at,
            dispatched_at,
            updated_at
          FROM events
          ${whereSql}
          ORDER BY created_at ASC
        `,
			)
			.all(...params) as unknown as EventRow[]

		return rows.map((row) => this.mapEventRow(row))
	}

	getLatestCommittedNodeOutput(runId: string, nodeId: string): NodeOutputJournalRecord | null {
		const row = this.database
			.prepare(
				`
          SELECT
            output_id,
            run_id,
            node_id,
            attempt_id,
            output_mode,
            output_payload_json,
            committed_at,
            boundary_sequence
          FROM node_output_journal
          WHERE run_id = ? AND node_id = ?
          ORDER BY boundary_sequence DESC
          LIMIT 1
        `,
			)
			.get(runId, nodeId) as NodeOutputRow | undefined

		return row ? this.mapNodeOutputRow(row) : null
	}

	getPersistedRunSnapshot(runId: string): PersistedRunSnapshot | null {
		const run = this.getRun(runId)
		if (!run) {
			return null
		}

		const latestOutputRows = this.database
			.prepare(
				`
          SELECT
            journal.output_id,
            journal.run_id,
            journal.node_id,
            journal.attempt_id,
            journal.output_mode,
            journal.output_payload_json,
            journal.committed_at,
            journal.boundary_sequence
          FROM node_output_journal AS journal
          INNER JOIN (
            SELECT node_id, MAX(boundary_sequence) AS boundary_sequence
            FROM node_output_journal
            WHERE run_id = ?
            GROUP BY node_id
          ) AS latest
            ON latest.node_id = journal.node_id
           AND latest.boundary_sequence = journal.boundary_sequence
          WHERE journal.run_id = ?
          ORDER BY journal.node_id ASC
        `,
			)
			.all(runId, runId) as unknown as NodeOutputRow[]

		return {
			run,
			chat: this.getChatRecord(runId),
			visible_messages: this.listVisibleChatMessages(runId),
			attempts: this.listNodeAttempts(runId),
			latest_committed_outputs: latestOutputRows.map((row) => this.mapNodeOutputRow(row)),
			current_vars: this.getCurrentVars(runId),
			resume: this.getResumeMetadataOrThrow(runId),
		}
	}

	private initializeSchema(): void {
		this.database.exec(`
      CREATE TABLE IF NOT EXISTS runs (
        run_id TEXT PRIMARY KEY,
        logical_agent_id TEXT,
        resolved_revision_id TEXT NOT NULL,
        entry_node_id TEXT NOT NULL,
        started_via TEXT NOT NULL,
        status TEXT NOT NULL,
        params_json TEXT NOT NULL,
        event_json TEXT,
        last_attempt_sequence INTEGER NOT NULL,
        last_boundary_sequence INTEGER NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
      );

      CREATE TABLE IF NOT EXISTS node_attempts (
        attempt_id TEXT PRIMARY KEY,
        run_id TEXT NOT NULL,
        node_id TEXT NOT NULL,
        attempt_sequence INTEGER NOT NULL,
        output_mode TEXT NOT NULL,
        state TEXT NOT NULL,
        outcome TEXT,
        blocked_on_user_prompt INTEGER NOT NULL,
        runtime_handle_json TEXT,
        committed_output_id TEXT,
        resume_boundary_sequence INTEGER,
        started_at TEXT NOT NULL,
        committed_at TEXT,
        FOREIGN KEY (run_id) REFERENCES runs (run_id) ON DELETE CASCADE,
        UNIQUE (run_id, attempt_sequence)
      );

      CREATE TABLE IF NOT EXISTS node_output_journal (
        output_id TEXT PRIMARY KEY,
        run_id TEXT NOT NULL,
        node_id TEXT NOT NULL,
        attempt_id TEXT NOT NULL,
        output_mode TEXT NOT NULL,
        output_payload_json TEXT NOT NULL,
        committed_at TEXT NOT NULL,
        boundary_sequence INTEGER NOT NULL,
        FOREIGN KEY (run_id) REFERENCES runs (run_id) ON DELETE CASCADE,
        FOREIGN KEY (attempt_id) REFERENCES node_attempts (attempt_id) ON DELETE CASCADE
      );

      CREATE TABLE IF NOT EXISTS vars_snapshots (
        run_id TEXT PRIMARY KEY,
        vars_json TEXT NOT NULL,
        boundary_sequence INTEGER NOT NULL,
        updated_at TEXT NOT NULL,
        FOREIGN KEY (run_id) REFERENCES runs (run_id) ON DELETE CASCADE
      );

      CREATE TABLE IF NOT EXISTS chat_records (
        chat_id TEXT PRIMARY KEY,
        run_id TEXT NOT NULL UNIQUE,
        resolved_revision_id TEXT NOT NULL,
        prefer_native_resume INTEGER NOT NULL,
        store_visible_messages INTEGER NOT NULL,
        store_context_window INTEGER NOT NULL,
        allow_fresh_start INTEGER NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        FOREIGN KEY (run_id) REFERENCES runs (run_id) ON DELETE CASCADE
      );

      CREATE TABLE IF NOT EXISTS visible_chat_messages (
        message_id TEXT PRIMARY KEY,
        chat_id TEXT NOT NULL,
        run_id TEXT NOT NULL,
        message_sequence INTEGER NOT NULL,
        kind TEXT NOT NULL,
        payload_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        FOREIGN KEY (chat_id) REFERENCES chat_records (chat_id) ON DELETE CASCADE,
        FOREIGN KEY (run_id) REFERENCES runs (run_id) ON DELETE CASCADE,
        UNIQUE (chat_id, message_sequence)
      );

      CREATE TABLE IF NOT EXISTS resume_metadata (
        run_id TEXT PRIMARY KEY,
        resolved_revision_id TEXT NOT NULL,
        native_resume_available INTEGER NOT NULL,
        local_resume_available INTEGER NOT NULL,
        last_durable_boundary_sequence INTEGER,
        last_durable_boundary_kind TEXT,
        last_attempt_id TEXT,
        pending_prompt_json TEXT,
        native_session_handle_json TEXT,
        local_context_snapshot_json TEXT,
        updated_at TEXT NOT NULL,
        FOREIGN KEY (run_id) REFERENCES runs (run_id) ON DELETE CASCADE,
        FOREIGN KEY (last_attempt_id) REFERENCES node_attempts (attempt_id) ON DELETE SET NULL
      );

      CREATE TABLE IF NOT EXISTS agent_records (
        logical_agent_id TEXT PRIMARY KEY,
        live_revision_id TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        FOREIGN KEY (live_revision_id) REFERENCES agent_revisions (revision_id) ON DELETE SET NULL
      );

      CREATE TABLE IF NOT EXISTS agent_revisions (
        revision_id TEXT PRIMARY KEY,
        logical_agent_id TEXT NOT NULL,
        revision_kind TEXT NOT NULL,
        file_path TEXT NOT NULL,
        resolved_revision_id TEXT NOT NULL,
        availability_state TEXT NOT NULL,
        validation_error TEXT,
        validated_at TEXT,
        graph_contract_version TEXT,
        agent_name TEXT,
        agent_description TEXT,
        agent_version TEXT,
        entry_node_id TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        FOREIGN KEY (logical_agent_id) REFERENCES agent_records (logical_agent_id) ON DELETE CASCADE
      );

      CREATE TABLE IF NOT EXISTS triggers (
        trigger_id TEXT PRIMARY KEY,
        logical_agent_id TEXT NOT NULL,
        trigger_ref TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        FOREIGN KEY (logical_agent_id) REFERENCES agent_records (logical_agent_id) ON DELETE CASCADE
      );

      CREATE TABLE IF NOT EXISTS events (
        event_id TEXT PRIMARY KEY,
        trigger_id TEXT NOT NULL,
        logical_agent_id TEXT NOT NULL,
        payload_json TEXT,
        launch_note TEXT,
        dispatch_status TEXT NOT NULL,
        run_id TEXT,
        resolved_revision_id TEXT,
        dispatch_error_code TEXT,
        dispatch_error_message TEXT,
        created_at TEXT NOT NULL,
        dispatched_at TEXT,
        updated_at TEXT NOT NULL,
        FOREIGN KEY (trigger_id) REFERENCES triggers (trigger_id) ON DELETE CASCADE,
        FOREIGN KEY (run_id) REFERENCES runs (run_id) ON DELETE SET NULL
      );

      CREATE TABLE IF NOT EXISTS memory_provider_registrations (
        provider_id TEXT PRIMARY KEY,
        codex_ref TEXT NOT NULL UNIQUE,
        provider_family TEXT NOT NULL,
        display_name TEXT,
        transport TEXT NOT NULL,
        status TEXT NOT NULL,
        supported_capabilities_json TEXT NOT NULL,
        config_json TEXT NOT NULL,
        status_code TEXT,
        status_message TEXT,
        last_checked_at TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
      );

      CREATE TABLE IF NOT EXISTS managed_subagents (
        subagent_id TEXT PRIMARY KEY,
        child_run_id TEXT NOT NULL UNIQUE,
        child_role TEXT NOT NULL,
        child_logical_agent_id TEXT NOT NULL,
        child_resolved_revision_id TEXT NOT NULL,
        lineage_json TEXT NOT NULL,
        task_package_json TEXT NOT NULL,
        state TEXT NOT NULL,
        terminal_result_json TEXT,
        close_disposition TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        terminal_at TEXT,
        closed_at TEXT
      );

      CREATE INDEX IF NOT EXISTS node_attempts_run_id_idx
      ON node_attempts (run_id, attempt_sequence);

      CREATE INDEX IF NOT EXISTS node_output_journal_latest_idx
      ON node_output_journal (run_id, node_id, boundary_sequence DESC);

      CREATE INDEX IF NOT EXISTS visible_chat_messages_run_id_idx
      ON visible_chat_messages (run_id, message_sequence ASC);

      CREATE INDEX IF NOT EXISTS agent_revisions_logical_agent_id_idx
      ON agent_revisions (logical_agent_id, created_at DESC);

      CREATE INDEX IF NOT EXISTS agent_revisions_file_path_idx
      ON agent_revisions (logical_agent_id, file_path, resolved_revision_id);

      CREATE INDEX IF NOT EXISTS triggers_logical_agent_id_idx
      ON triggers (logical_agent_id, created_at ASC);

      CREATE INDEX IF NOT EXISTS events_trigger_id_idx
      ON events (trigger_id, created_at ASC);

      CREATE INDEX IF NOT EXISTS events_logical_agent_id_idx
      ON events (logical_agent_id, created_at ASC);

      CREATE INDEX IF NOT EXISTS memory_provider_registrations_family_idx
      ON memory_provider_registrations (provider_family, created_at ASC);

      CREATE INDEX IF NOT EXISTS managed_subagents_parent_run_idx
      ON managed_subagents (state, created_at ASC);
    `)
	}

	private withTransaction<T>(operation: () => T): T {
		this.database.exec('BEGIN IMMEDIATE')
		try {
			const result = operation()
			this.database.exec('COMMIT')
			return result
		} catch (error) {
			try {
				this.database.exec('ROLLBACK')
			} catch {
				// Ignore rollback failures so the original error remains visible.
			}
			throw error
		}
	}

	private nextBoundarySequence(runId: string): number {
		const run = this.getRunRowOrThrow(runId)
		return run.last_boundary_sequence + 1
	}

	private getRunOrThrow(runId: string): RunRecord {
		const run = this.getRun(runId)
		if (!run) {
			throw new AppError('RUN_NOT_FOUND', `Run "${runId}" does not exist.`)
		}
		return run
	}

	private getRunRowOrThrow(runId: string): RunRow {
		const row = this.database
			.prepare(
				`
          SELECT
            run_id,
            logical_agent_id,
            resolved_revision_id,
            entry_node_id,
            started_via,
            status,
            params_json,
            event_json,
            last_attempt_sequence,
            last_boundary_sequence,
            created_at,
            updated_at
          FROM runs
          WHERE run_id = ?
        `,
			)
			.get(runId) as RunRow | undefined

		if (!row) {
			throw new AppError('RUN_NOT_FOUND', `Run "${runId}" does not exist.`)
		}

		return row
	}

	private getNodeAttemptOrThrow(attemptId: string): NodeAttemptRecord {
		const row = this.database
			.prepare(
				`
          SELECT
            attempt_id,
            run_id,
            node_id,
            attempt_sequence,
            output_mode,
            state,
            outcome,
            blocked_on_user_prompt,
            runtime_handle_json,
            committed_output_id,
            resume_boundary_sequence,
            started_at,
            committed_at
          FROM node_attempts
          WHERE attempt_id = ?
        `,
			)
			.get(attemptId) as NodeAttemptRow | undefined

		if (!row) {
			throw new AppError('NODE_ATTEMPT_NOT_FOUND', `Node attempt "${attemptId}" does not exist.`)
		}

		return this.mapNodeAttemptRow(row)
	}

	private getChatRecordOrThrow(runId: string): ChatRecord {
		const chat = this.getChatRecord(runId)
		if (!chat) {
			throw new AppError('CHAT_NOT_FOUND', `Run "${runId}" does not have persisted chat state.`)
		}
		return chat
	}

	private getVisibleChatMessageOrThrow(messageId: string): VisibleChatMessageRecord {
		const row = this.database
			.prepare(
				`
          SELECT
            message_id,
            chat_id,
            run_id,
            message_sequence,
            kind,
            payload_json,
            created_at
          FROM visible_chat_messages
          WHERE message_id = ?
        `,
			)
			.get(messageId) as VisibleChatMessageRow | undefined

		if (!row) {
			throw new AppError(
				'CHAT_MESSAGE_NOT_FOUND',
				`Visible chat message "${messageId}" does not exist.`,
			)
		}

		return this.mapVisibleChatMessageRow(row)
	}

	private getResumeMetadataOrThrow(runId: string): ResumeMetadataRecord {
		const resumeMetadata = this.getResumeMetadata(runId)
		if (!resumeMetadata) {
			throw new AppError('RUN_NOT_FOUND', `Run "${runId}" does not exist.`)
		}
		return resumeMetadata
	}

	private getAgentRecordOrThrow(logicalAgentId: string): AgentRecord {
		const agent = this.getAgentRecord(logicalAgentId)
		if (!agent) {
			throw new AppError('AGENT_NOT_FOUND', `Agent "${logicalAgentId}" does not exist.`)
		}
		return agent
	}

	private getMemoryProviderRecordOrThrow(providerId: string): MemoryProviderRecord {
		const provider = this.getMemoryProviderRecord(providerId)
		if (!provider) {
			throw new AppError(
				'MEMORY_PROVIDER_NOT_FOUND',
				`Memory provider "${providerId}" does not exist.`,
			)
		}
		return provider
	}

	private getManagedSubagentRecordOrThrow(subagentId: ManagedSubagentId): ManagedSubagentRecord {
		const subagent = this.getManagedSubagentRecord(subagentId)
		if (!subagent) {
			throw new AppError('SUBAGENT_NOT_FOUND', `Managed subagent "${subagentId}" does not exist.`)
		}
		return subagent
	}

	private getAgentRevisionOrThrow(revisionId: string): AgentRevisionRecord {
		const revision = this.getAgentRevision(revisionId)
		if (!revision) {
			throw new AppError(
				'AGENT_REVISION_NOT_FOUND',
				`Agent revision "${revisionId}" does not exist.`,
			)
		}
		return revision
	}

	private getTriggerRecordOrThrow(triggerId: string): TriggerRecord {
		const trigger = this.getTriggerRecord(triggerId)
		if (!trigger) {
			throw new AppError('TRIGGER_NOT_FOUND', `Trigger "${triggerId}" does not exist.`)
		}
		return trigger
	}

	private getEventRecordOrThrow(eventId: string): EventRecord {
		const event = this.getEventRecord(eventId)
		if (!event) {
			throw new AppError('EVENT_NOT_FOUND', `Event "${eventId}" does not exist.`)
		}
		return event
	}

	private getMutableAttempt(attemptId: string): NodeAttemptRecord & { run: RunRecord } {
		const attempt = this.getNodeAttemptOrThrow(attemptId)
		if (attempt.state !== 'in_progress') {
			throw new AppError(
				'INVALID_ATTEMPT_STATE',
				`Node attempt "${attemptId}" is already "${attempt.state}" and cannot be committed again.`,
			)
		}
		return {
			...attempt,
			run: this.getRunOrThrow(attempt.run_id),
		}
	}

	private getInProgressAttemptForRun(runId: string): NodeAttemptRecord | null {
		const row = this.database
			.prepare(
				`
          SELECT
            attempt_id,
            run_id,
            node_id,
            attempt_sequence,
            output_mode,
            state,
            outcome,
            blocked_on_user_prompt,
            runtime_handle_json,
            committed_output_id,
            resume_boundary_sequence,
            started_at,
            committed_at
          FROM node_attempts
          WHERE run_id = ? AND state = 'in_progress'
          ORDER BY attempt_sequence ASC
          LIMIT 1
        `,
			)
			.get(runId) as NodeAttemptRow | undefined

		return row ? this.mapNodeAttemptRow(row) : null
	}

	private getNextVisibleMessageSequence(chatId: string): number {
		const row = this.database
			.prepare(
				`
          SELECT COALESCE(MAX(message_sequence), 0) AS max_sequence
          FROM visible_chat_messages
          WHERE chat_id = ?
        `,
			)
			.get(chatId) as { max_sequence: number }

		return row.max_sequence + 1
	}

	private writeResumeMetadata(args: {
		run_id: string
		resolved_revision_id: string
		boundary_sequence: number
		boundary_kind: DurableBoundaryKind
		attempt_id: string
		pending_prompt: PendingUserPromptRecord | null
		resume: ReturnType<typeof normalizeResumeInput>
		updated_at: string
	}): void {
		const chat = this.getChatRecordOrThrow(args.run_id)

		this.database
			.prepare(
				`
          UPDATE resume_metadata
          SET
            resolved_revision_id = ?,
            native_resume_available = ?,
            local_resume_available = ?,
            last_durable_boundary_sequence = ?,
            last_durable_boundary_kind = ?,
            last_attempt_id = ?,
            pending_prompt_json = ?,
            native_session_handle_json = ?,
            local_context_snapshot_json = ?,
            updated_at = ?
          WHERE run_id = ?
        `,
			)
			.run(
				args.resolved_revision_id,
				toSqliteBoolean(args.resume.native_resume_available),
				toSqliteBoolean(args.resume.local_resume_available),
				args.boundary_sequence,
				args.boundary_kind,
				args.attempt_id,
				stringifyOptionalJson(args.pending_prompt as JsonValue | null),
				stringifyOptionalJson(args.resume.native_session_handle),
				stringifyOptionalJson(
					normalizeStoredLocalContextSnapshot(
						args.resume.local_context_snapshot,
						chat.policy.store_context_window,
					),
				),
				args.updated_at,
				args.run_id,
			)
	}

	private mapRunRow(row: RunRow): RunRecord {
		return {
			run_id: row.run_id,
			logical_agent_id: row.logical_agent_id,
			resolved_revision_id: row.resolved_revision_id,
			entry_node_id: row.entry_node_id,
			started_via: row.started_via,
			status: row.status,
			params: parseJson<JsonObject>(row.params_json),
			event: parseOptionalJson<JsonObject>(row.event_json),
			last_attempt_sequence: row.last_attempt_sequence,
			last_boundary_sequence: row.last_boundary_sequence,
			created_at: row.created_at,
			updated_at: row.updated_at,
		}
	}

	private mapChatRow(row: ChatRow): ChatRecord {
		return {
			chat_id: row.chat_id,
			run_id: row.run_id,
			resolved_revision_id: row.resolved_revision_id,
			policy: {
				prefer_native_resume: fromSqliteBoolean(row.prefer_native_resume),
				store_visible_messages: fromSqliteBoolean(row.store_visible_messages),
				store_context_window: fromSqliteBoolean(row.store_context_window),
				allow_fresh_start: fromSqliteBoolean(row.allow_fresh_start),
			},
			created_at: row.created_at,
			updated_at: row.updated_at,
		}
	}

	private mapNodeAttemptRow(row: NodeAttemptRow): NodeAttemptRecord {
		return {
			attempt_id: row.attempt_id,
			run_id: row.run_id,
			node_id: row.node_id,
			attempt_sequence: row.attempt_sequence,
			output_mode: row.output_mode,
			state: row.state,
			outcome: row.outcome,
			blocked_on_user_prompt: fromSqliteBoolean(row.blocked_on_user_prompt),
			runtime_handle: parseOptionalJson<JsonValue>(row.runtime_handle_json),
			committed_output_id: row.committed_output_id,
			resume_boundary_sequence: row.resume_boundary_sequence,
			started_at: row.started_at,
			committed_at: row.committed_at,
		}
	}

	private mapVisibleChatMessageRow(row: VisibleChatMessageRow): VisibleChatMessageRecord {
		return {
			message_id: row.message_id,
			chat_id: row.chat_id,
			run_id: row.run_id,
			message_sequence: row.message_sequence,
			kind: row.kind,
			payload: parseJson<JsonValue>(row.payload_json),
			created_at: row.created_at,
		}
	}

	private mapNodeOutputRow(row: NodeOutputRow): NodeOutputJournalRecord {
		return {
			output_id: row.output_id,
			run_id: row.run_id,
			node_id: row.node_id,
			attempt_id: row.attempt_id,
			output:
				row.output_mode === 'text'
					? { mode: 'text', text: parseJson<string>(row.output_payload_json) }
					: { mode: 'json', json: parseJson<JsonObject>(row.output_payload_json) },
			committed_at: row.committed_at,
			boundary_sequence: row.boundary_sequence,
		}
	}

	private mapResumeMetadataRow(row: ResumeMetadataRow): ResumeMetadataRecord {
		return {
			run_id: row.run_id,
			resolved_revision_id: row.resolved_revision_id,
			native_resume_available: fromSqliteBoolean(row.native_resume_available),
			local_resume_available: fromSqliteBoolean(row.local_resume_available),
			last_durable_boundary_sequence: row.last_durable_boundary_sequence,
			last_durable_boundary_kind: row.last_durable_boundary_kind,
			last_attempt_id: row.last_attempt_id,
			pending_prompt: parseOptionalJson<JsonValue>(
				row.pending_prompt_json,
			) as PendingUserPromptRecord | null,
			native_session_handle: parseOptionalJson<JsonValue>(row.native_session_handle_json),
			local_context_snapshot: parseOptionalJson<JsonValue>(row.local_context_snapshot_json),
			updated_at: row.updated_at,
		}
	}

	private mapAgentRecordRow(row: AgentRecordRow): AgentRecord {
		return {
			logical_agent_id: row.logical_agent_id,
			live_revision_id: row.live_revision_id,
			created_at: row.created_at,
			updated_at: row.updated_at,
		}
	}

	private mapMemoryProviderRow(row: MemoryProviderRow): MemoryProviderRecord {
		return {
			provider_id: row.provider_id,
			codex_ref: row.codex_ref,
			provider_family: row.provider_family,
			display_name: row.display_name,
			transport: row.transport,
			status: row.status,
			supported_capabilities: parseJson<MemoryProviderCapability[]>(
				row.supported_capabilities_json,
			),
			config: parseJson<JsonObject>(row.config_json),
			status_code: row.status_code,
			status_message: row.status_message,
			last_checked_at: row.last_checked_at,
			created_at: row.created_at,
			updated_at: row.updated_at,
		}
	}

	private mapManagedSubagentRow(row: ManagedSubagentRow): ManagedSubagentRecord {
		return {
			subagent_id: row.subagent_id,
			run_id: row.child_run_id,
			child_run_id: row.child_run_id,
			child_role: row.child_role as ManagedSubagentRecord['child_role'],
			child_logical_agent_id: row.child_logical_agent_id,
			child_resolved_revision_id: row.child_resolved_revision_id,
			lineage: parseJson<JsonObject>(
				row.lineage_json,
			) as unknown as ManagedSubagentRecord['lineage'],
			task_package: parseJson<JsonObject>(
				row.task_package_json,
			) as unknown as ManagedSubagentRecord['task_package'],
			state: normalizeManagedSubagentState(row.state),
			terminal_result: parseOptionalJson<JsonObject>(
				row.terminal_result_json,
			) as unknown as ManagedSubagentRecord['terminal_result'],
			close_disposition:
				(row.close_disposition as ManagedSubagentRecord['close_disposition']) ?? null,
			created_at: row.created_at,
			updated_at: row.updated_at,
			terminal_at: row.terminal_at,
			closed_at: row.closed_at,
		}
	}

	private managedWriteSetsConflict(
		left: ManagedSubagentRecord['task_package']['write_set'],
		right: ManagedSubagentRecord['task_package']['write_set'],
	): boolean {
		return left.items.some((leftTarget) =>
			right.items.some((rightTarget) => this.managedWriteTargetsConflict(leftTarget, rightTarget)),
		)
	}

	private managedWriteTargetsConflict(
		left: ManagedSubagentRecord['task_package']['write_set']['items'][number],
		right: ManagedSubagentRecord['task_package']['write_set']['items'][number],
	): boolean {
		const leftRef = normalizeManagedResourceRef(left.resource_ref)
		const rightRef = normalizeManagedResourceRef(right.resource_ref)
		return (
			isSameOrDescendantResource(leftRef, rightRef) || isSameOrDescendantResource(rightRef, leftRef)
		)
	}

	private mapAgentRevisionRow(row: AgentRevisionRow): AgentRevisionRecord {
		return {
			revision_id: row.revision_id,
			logical_agent_id: row.logical_agent_id,
			revision_kind: row.revision_kind,
			file_path: row.file_path,
			resolved_revision_id: row.resolved_revision_id,
			availability_state: row.availability_state,
			validation_error: row.validation_error,
			validated_at: row.validated_at,
			graph_contract_version: row.graph_contract_version,
			agent_name: row.agent_name,
			agent_description: row.agent_description,
			agent_version: row.agent_version,
			entry_node_id: row.entry_node_id,
			created_at: row.created_at,
			updated_at: row.updated_at,
		}
	}

	private mapTriggerRow(row: TriggerRow): TriggerRecord {
		return {
			trigger_id: row.trigger_id,
			logical_agent_id: row.logical_agent_id,
			trigger_ref: row.trigger_ref,
			created_at: row.created_at,
			updated_at: row.updated_at,
		}
	}

	private mapEventRow(row: EventRow): EventRecord {
		return {
			event_id: row.event_id,
			trigger_id: row.trigger_id,
			logical_agent_id: row.logical_agent_id,
			payload: parseOptionalJson<JsonValue>(row.payload_json),
			launch_note: row.launch_note,
			dispatch_status: row.dispatch_status,
			run_id: row.run_id,
			resolved_revision_id: row.resolved_revision_id,
			dispatch_error_code: row.dispatch_error_code,
			dispatch_error_message: row.dispatch_error_message,
			created_at: row.created_at,
			dispatched_at: row.dispatched_at,
			updated_at: row.updated_at,
		}
	}
}
