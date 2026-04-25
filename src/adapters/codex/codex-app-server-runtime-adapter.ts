import { type ChildProcessWithoutNullStreams, spawn } from 'node:child_process'
import { once } from 'node:events'
import path from 'node:path'
import process from 'node:process'
import { createInterface } from 'node:readline'
import { AppError } from '../../core/errors.js'
import type { JsonObject, JsonValue } from '../../core/json.js'
import { validateJsonOutputAgainstSchema } from '../../core/output-schema-validator.js'
import type {
	RuntimeAdapter,
	RuntimeAdapterCapabilities,
	RuntimeAdapterExecutionRequest,
	RuntimeEnvironmentInspectionResult,
	RuntimeEvent,
	RuntimeExecutionSession,
	RuntimeModelCatalogPage,
	RuntimeModelCatalogRequest,
	RuntimePersonality,
	RuntimeRateLimitSummary,
	RuntimeReasoningEffort,
	RuntimeSourceInspectionResult,
	RuntimeSourceSelection,
	RuntimeSpeedTier,
	RuntimeTerminalResult,
	RuntimeUserChatRequestEvent,
	UserChatResponsePayload,
} from '../../ports/runtime.js'

type NativeSessionHandle = JsonObject & {
	threadId: string
}

type LiveRuntimeHandle = JsonObject &
	NativeSessionHandle & {
		turnId: string
	}

interface JsonRpcRequest {
	id: string | number
	method: string
	params?: unknown
}

interface JsonRpcResponse {
	id: string | number
	result?: unknown
	error?: {
		code: number
		message: string
		data?: unknown
	}
}

interface JsonRpcNotification {
	method: string
	params?: unknown
}

type JsonRpcMessage = JsonRpcRequest | JsonRpcResponse | JsonRpcNotification

interface RequestContext {
	resolve: (result: unknown) => void
	reject: (error: Error) => void
}

interface TurnState {
	turnId: string
	turn: AppServerTurn
	settled: boolean
}

interface AppServerThreadStartResponse {
	thread: { id: string }
}

interface AppServerTurnStartResponse {
	turn: AppServerTurn
}

interface AppServerModelListResponse {
	data?: unknown[]
	nextCursor?: unknown
}

interface AppServerAuthStatusResponse {
	authMethod?: unknown
	requiresOpenaiAuth?: unknown
}

interface AppServerAccountReadResponse {
	account?: unknown
	requiresOpenaiAuth?: unknown
}

interface AppServerRateLimitReadResponse {
	rateLimits?: unknown[]
}

interface AppServerConfigReadResponse {
	config?: unknown
}

interface AppServerConfigRequirementsReadResponse {
	requirements?: unknown
}

interface AppServerToolRequestUserInputParams {
	threadId?: unknown
	turnId?: unknown
	itemId?: unknown
	questions?: unknown
}

interface AppServerTurn {
	id: string
	items: AppServerThreadItem[]
	status: 'completed' | 'interrupted' | 'failed' | 'inProgress'
	error?: {
		message: string
		additionalDetails?: string | null
		codexErrorInfo?: unknown | null
	} | null
}

type AppServerThreadItem =
	| {
			type: 'agentMessage'
			id: string
			text: string
			phase?: 'commentary' | 'final_answer' | null
	  }
	| {
			type: 'message'
			id: string
			role?: string
			phase?: 'commentary' | 'final_answer' | null
			content?: Array<{
				type?: string
				text?: string
			}>
			text?: string
	  }
	| {
			type: string
			id: string
			[key: string]: unknown
	  }

interface AppServerLauncher {
	command: string
	args: string[]
}

interface AppServerClientOptions {
	onEvent?: (event: RuntimeEvent) => void
	pendingAutoUserChatReply?: UserChatResponsePayload
	timeoutBudget?: AppServerTimeoutBudget
}

export interface CodexAppServerRuntimeAdapterOptions {
	execution_timeout_ms?: number
	model_catalog_timeout_ms?: number
	environment_timeout_ms?: number
	comment_timeout_ms?: number
	reply_timeout_ms?: number
}

type AppServerTimeoutCode =
	| 'CODEX_APP_SERVER_EXECUTION_TIMEOUT'
	| 'CODEX_APP_SERVER_MODEL_CATALOG_TIMEOUT'
	| 'CODEX_APP_SERVER_ENVIRONMENT_TIMEOUT'
	| 'CODEX_APP_SERVER_COMMENT_TIMEOUT'
	| 'CODEX_APP_SERVER_REPLY_TIMEOUT'

type AppServerTimeoutOperation =
	| 'runtime_execution'
	| 'runtime_model_catalog'
	| 'runtime_environment'
	| 'live_comment'
	| 'prompt_reply'

interface AppServerTimeoutBudget {
	code: AppServerTimeoutCode
	disabled: boolean
	operation: AppServerTimeoutOperation
	startedAtMs: number
	timers: Set<ReturnType<typeof setTimeout>>
	timeoutMs: number
}

const TIMEOUT_MESSAGES: Record<AppServerTimeoutCode, string> = {
	CODEX_APP_SERVER_EXECUTION_TIMEOUT:
		'Codex App Server runtime execution timed out before the active runtime segment completed.',
	CODEX_APP_SERVER_MODEL_CATALOG_TIMEOUT: 'Codex App Server model catalog inspection timed out.',
	CODEX_APP_SERVER_ENVIRONMENT_TIMEOUT: 'Codex App Server environment inspection timed out.',
	CODEX_APP_SERVER_COMMENT_TIMEOUT: 'Codex App Server live comment delivery timed out.',
	CODEX_APP_SERVER_REPLY_TIMEOUT: 'Codex App Server prompt reply delivery timed out.',
}

class AppServerTimeoutError extends AppError {
	constructor(
		readonly timeoutCode: AppServerTimeoutCode,
		readonly operation: AppServerTimeoutOperation,
		readonly timeoutMs: number,
		readonly phase: string,
	) {
		super(
			timeoutCode,
			`${timeoutCode}: ${TIMEOUT_MESSAGES[timeoutCode]} Timeout: ${timeoutMs}ms.`,
			{
				operation,
				phase,
				timeout_ms: timeoutMs,
			},
		)
		this.name = 'AppError'
	}
}

function createTimeoutBudget(
	code: AppServerTimeoutCode,
	operation: AppServerTimeoutOperation,
	timeoutMs: number | undefined,
): AppServerTimeoutBudget | undefined {
	if (timeoutMs === undefined) {
		return undefined
	}
	return {
		code,
		disabled: false,
		operation,
		startedAtMs: Date.now(),
		timers: new Set(),
		timeoutMs,
	}
}

function getRemainingTimeoutMs(budget: AppServerTimeoutBudget | undefined): number | undefined {
	if (!budget || budget.disabled) {
		return undefined
	}
	return Math.max(0, budget.timeoutMs - (Date.now() - budget.startedAtMs))
}

function registerTimeoutBudgetTimer(
	budget: AppServerTimeoutBudget | undefined,
	timer: ReturnType<typeof setTimeout>,
): void {
	budget?.timers.add(timer)
}

function clearTimeoutBudgetTimer(
	budget: AppServerTimeoutBudget | undefined,
	timer: ReturnType<typeof setTimeout> | undefined,
): void {
	if (!timer) {
		return
	}
	clearTimeout(timer)
	budget?.timers.delete(timer)
}

function disarmTimeoutBudget(budget: AppServerTimeoutBudget | undefined): void {
	if (!budget || budget.disabled) {
		return
	}
	budget.disabled = true
	for (const timer of budget.timers) {
		clearTimeout(timer)
	}
	budget.timers.clear()
}

function createTimeoutError(budget: AppServerTimeoutBudget, phase: string): AppServerTimeoutError {
	return new AppServerTimeoutError(budget.code, budget.operation, budget.timeoutMs, phase)
}

async function withAppServerTimeout<T>(
	promise: Promise<T>,
	budget: AppServerTimeoutBudget | undefined,
	phase: string,
): Promise<T> {
	const remainingMs = getRemainingTimeoutMs(budget)
	if (remainingMs === undefined) {
		return await promise
	}
	if (remainingMs <= 0) {
		throw createTimeoutError(budget as AppServerTimeoutBudget, phase)
	}

	let timeout: ReturnType<typeof setTimeout> | undefined
	try {
		return await Promise.race([
			promise,
			new Promise<T>((_, reject) => {
				timeout = setTimeout(() => {
					reject(createTimeoutError(budget as AppServerTimeoutBudget, phase))
				}, remainingMs)
				registerTimeoutBudgetTimer(budget, timeout)
			}),
		])
	} finally {
		clearTimeoutBudgetTimer(budget, timeout)
	}
}

class AsyncEventQueue<T> implements AsyncIterable<T> {
	private readonly values: T[] = []
	private readonly waiters: Array<(result: IteratorResult<T>) => void> = []
	private closed = false

	push(value: T): void {
		if (this.closed) {
			return
		}

		const waiter = this.waiters.shift()
		if (waiter) {
			waiter({
				value,
				done: false,
			})
			return
		}

		this.values.push(value)
	}

	close(): void {
		if (this.closed) {
			return
		}
		this.closed = true
		while (this.waiters.length > 0) {
			this.waiters.shift()?.({
				value: undefined as never,
				done: true,
			})
		}
	}

	async *[Symbol.asyncIterator](): AsyncIterator<T> {
		while (true) {
			if (this.values.length > 0) {
				yield this.values.shift() as T
				continue
			}

			if (this.closed) {
				return
			}

			const result = await new Promise<IteratorResult<T>>((resolve) => {
				this.waiters.push(resolve)
			})

			if (result.done) {
				return
			}

			yield result.value
		}
	}
}

type AppServerPendingUserInputPromptHandle = JsonObject & {
	kind: 'codex_app_server_user_chat_request'
	threadId: string
	turnId: string
	itemId: string
	requestId: string | number
	prompt_id: string
}

interface PendingUserInputRequest {
	handle: AppServerPendingUserInputPromptHandle
	payload:
		| {
				kind: 'text'
				prompt_id: string
				text: string
				require_response: true
				options?: never
		  }
		| {
				kind: 'options'
				prompt_id: string
				text: string
				require_response: true
				options: Array<{
					id: string
					label: string
					value: JsonValue
				}>
		  }
	resolveResponse: (result: unknown) => void
	rejectResponse: (error: Error) => void
	deliveryCompleted: Promise<void>
	resolveDelivery: () => void
	rejectDelivery: (error: Error) => void
}

type NormalizedPendingUserInputRequest = Omit<
	PendingUserInputRequest,
	'resolveResponse' | 'rejectResponse' | 'deliveryCompleted' | 'resolveDelivery' | 'rejectDelivery'
>

function isObject(value: unknown): value is Record<string, unknown> {
	return value !== null && typeof value === 'object' && !Array.isArray(value)
}

function toJsonObject(value: unknown): JsonObject | undefined {
	if (isObject(value)) {
		return value as JsonObject
	}
	return undefined
}

const REASONING_EFFORTS = new Set<RuntimeReasoningEffort>([
	'none',
	'minimal',
	'low',
	'medium',
	'high',
	'xhigh',
])

const SPEED_TIERS = new Set<RuntimeSpeedTier>(['fast', 'flex'])
const PERSONALITIES = new Set<RuntimePersonality>(['none', 'friendly', 'pragmatic'])

function parseStringOption<TValue extends string>(
	value: JsonValue | undefined,
	optionName: string,
	allowedValues?: Set<TValue>,
): TValue | undefined {
	if (value === undefined) {
		return undefined
	}
	if (typeof value !== 'string') {
		throw new Error(`${optionName} must be a string when provided.`)
	}
	if (allowedValues && !allowedValues.has(value as TValue)) {
		throw new Error(`${optionName} must be one of: ${[...allowedValues].sort().join(', ')}.`)
	}
	return value as TValue
}

function parseRuntimeOptions(runtimeOptions: JsonObject): {
	model?: string
	reasoning_effort?: RuntimeReasoningEffort
	speed_tier?: RuntimeSpeedTier
	personality?: RuntimePersonality
} {
	const model = runtimeOptions.model
	return {
		model: parseStringOption(model, 'runtime_options.model'),
		reasoning_effort: parseStringOption(
			runtimeOptions.reasoning_effort,
			'runtime_options.reasoning_effort',
			REASONING_EFFORTS,
		),
		speed_tier: parseStringOption(
			runtimeOptions.speed_tier,
			'runtime_options.speed_tier',
			SPEED_TIERS,
		),
		personality: parseStringOption(
			runtimeOptions.personality,
			'runtime_options.personality',
			PERSONALITIES,
		),
	}
}

function readRequestedModel(runtimeOptions: unknown): unknown {
	if (isObject(runtimeOptions)) {
		return runtimeOptions.model
	}
	return undefined
}

function buildUserChatReplyResult(
	pending: NormalizedPendingUserInputRequest,
	response: UserChatResponsePayload,
): Record<string, unknown> {
	if (pending.payload.kind === 'text') {
		if (response.kind !== 'text') {
			throw new Error('The active App Server prompt requires a text reply.')
		}

		return {
			answers: {
				[pending.handle.prompt_id]: {
					answers: [response.text],
				},
			},
		}
	}

	if (response.kind !== 'option') {
		throw new Error('The active App Server prompt requires an option reply.')
	}

	const selectedOption = pending.payload.options.find((option) => option.id === response.option_id)
	if (!selectedOption) {
		throw new Error(`Option "${response.option_id}" is not valid for the active App Server prompt.`)
	}

	if (response.value !== selectedOption.value) {
		throw new Error(
			`Option "${response.option_id}" must use the declared value for the selected prompt option.`,
		)
	}

	return {
		answers: {
			[pending.handle.prompt_id]: {
				answers: [selectedOption.label],
			},
		},
	}
}

function extractFinalAnswerText(turn: AppServerTurn): string | undefined {
	const agentMessages = turn.items.filter((item) => item.type === 'agentMessage') as Array<{
		type: 'agentMessage'
		text: string
		phase?: 'commentary' | 'final_answer' | null
	}>

	const finalAnswers = agentMessages.filter((item) => item.phase === 'final_answer')
	if (finalAnswers.length > 0) {
		return finalAnswers.map((item) => item.text).join('')
	}

	const messageItems = turn.items.filter((item) => item.type === 'message') as Array<{
		type: 'message'
		phase?: 'commentary' | 'final_answer' | null
		content?: Array<{
			type?: string
			text?: string
		}>
		text?: string
	}>
	const finalMessageItems = messageItems.filter((item) => item.phase === 'final_answer')
	if (finalMessageItems.length > 0) {
		return finalMessageItems
			.flatMap(
				(item) =>
					item.content
						?.filter(
							(contentItem) =>
								contentItem.type === 'output_text' && typeof contentItem.text === 'string',
						)
						.map((contentItem) => contentItem.text) ?? [],
			)
			.join('')
	}

	const textBearingItems = turn.items.filter(
		(item): item is AppServerThreadItem & { text: string } =>
			isObject(item) && typeof item.text === 'string',
	)
	if (textBearingItems.length > 0) {
		return textBearingItems.map((item) => item.text).join('')
	}

	return agentMessages.at(-1)?.text
}

function upsertTurnItem(
	items: AppServerThreadItem[],
	item: AppServerThreadItem,
): AppServerThreadItem[] {
	const existingIndex = items.findIndex((candidate) => candidate.id === item.id)
	if (existingIndex < 0) {
		return [...items, item]
	}

	const updatedItems = [...items]
	updatedItems[existingIndex] = item
	return updatedItems
}

function mergeTurnItems(
	turnItems: AppServerThreadItem[],
	completedItems: AppServerThreadItem[],
): AppServerThreadItem[] {
	return completedItems.reduce((mergedItems, item) => upsertTurnItem(mergedItems, item), turnItems)
}

function toFailure(code: string, message: string, details?: unknown): RuntimeTerminalResult {
	return {
		outcome: 'runtime_error',
		error: {
			code,
			message,
			details,
		},
	}
}

function normalizeNativeHandle(handle: unknown): NativeSessionHandle {
	if (!isObject(handle) || typeof handle.threadId !== 'string') {
		throw new Error('unsupported_request: native_session_handle must include a string threadId.')
	}

	return {
		threadId: handle.threadId,
	}
}

function normalizeLiveHandle(handle: unknown): LiveRuntimeHandle {
	if (
		!isObject(handle) ||
		typeof handle.threadId !== 'string' ||
		typeof handle.turnId !== 'string'
	) {
		throw new Error(
			'unsupported_request: runtime_handle must include string threadId and turnId values.',
		)
	}

	return {
		threadId: handle.threadId,
		turnId: handle.turnId,
	}
}

const MEMORY_CONTEXT_START = '--- BEGIN DENNETT MEMORY CONTEXT ---'
const MEMORY_CONTEXT_END = '--- END DENNETT MEMORY CONTEXT ---'
const MEMORY_CONTEXT_TRUNCATED = '...[truncated]'
const MAX_MEMORY_CONTEXT_BINDINGS = 4
const MAX_MEMORY_CONTEXT_RECORDS_PER_BINDING = 6
const MAX_MEMORY_CONTEXT_CHARS = 12_000
const MAX_MEMORY_FIELD_CHARS = 500
const MAX_MEMORY_RECORD_CONTENT_CHARS = 2_000

function truncateMemoryText(value: string, maxChars: number): string {
	const normalized = value
		.replace(/\r\n?/g, '\n')
		.replace(/\0/g, '')
		.replaceAll(MEMORY_CONTEXT_START, '[memory-context-boundary]')
		.replaceAll(MEMORY_CONTEXT_END, '[memory-context-boundary]')
		.trim()

	if (normalized.length <= maxChars) {
		return normalized
	}

	return `${normalized.slice(0, Math.max(0, maxChars - MEMORY_CONTEXT_TRUNCATED.length))}${MEMORY_CONTEXT_TRUNCATED}`
}

function formatOptionalMemoryField(label: string, value: string | undefined): string | undefined {
	if (!value) {
		return undefined
	}
	return `${label}: ${truncateMemoryText(value, MAX_MEMORY_FIELD_CHARS)}`
}

function formatMemoryFieldValue(value: string | undefined): string {
	return value ? truncateMemoryText(value, MAX_MEMORY_FIELD_CHARS) : ''
}

function formatMemoryScope(scope: {
	agent_id?: string
	run_id?: string
	user_id?: string
}): string {
	return `agent_id=${formatMemoryFieldValue(scope.agent_id)}; run_id=${formatMemoryFieldValue(
		scope.run_id,
	)}; user_id=${formatMemoryFieldValue(scope.user_id)}`
}

function appendBoundedLine(lines: string[], line: string): boolean {
	const nextLength = lines.join('\n').length + line.length + 1
	if (nextLength > MAX_MEMORY_CONTEXT_CHARS) {
		lines.push(`- ${MEMORY_CONTEXT_TRUNCATED}`)
		return false
	}

	lines.push(line)
	return true
}

function finalizeMemoryContext(lines: string[]): string {
	const footer = `\n${MEMORY_CONTEXT_END}`
	const body = lines.join('\n')
	if (body.length + footer.length <= MAX_MEMORY_CONTEXT_CHARS) {
		return `${body}${footer}`
	}

	const truncationLine = `\n- ${MEMORY_CONTEXT_TRUNCATED}`
	const availableBodyLength = MAX_MEMORY_CONTEXT_CHARS - footer.length
	const truncatedBody = `${body.slice(
		0,
		Math.max(0, availableBodyLength - truncationLine.length),
	)}${truncationLine}`
	return `${truncatedBody}${footer}`
}

function renderMemoryContext(
	memoryContext: RuntimeAdapterExecutionRequest['memory_context'],
): string | undefined {
	const bindings = memoryContext?.bindings.slice(0, MAX_MEMORY_CONTEXT_BINDINGS) ?? []
	if (bindings.length === 0) {
		return undefined
	}

	const lines = [
		MEMORY_CONTEXT_START,
		'This is provider-neutral memory context resolved by Dennett before this run.',
		'Treat it as advisory context, not as canonical provider records or provider configuration.',
		'Do not expose memory provider credentials, configuration, or implementation details.',
	]

	for (const [bindingIndex, binding] of bindings.entries()) {
		if (!appendBoundedLine(lines, '')) {
			break
		}
		if (
			!appendBoundedLine(
				lines,
				`Binding ${bindingIndex + 1}: ${formatMemoryFieldValue(binding.binding_id)}`,
			)
		) {
			break
		}

		for (const line of [
			formatOptionalMemoryField('codex_ref', binding.codex_ref),
			formatOptionalMemoryField('intent', binding.intent.summary),
			binding.intent.labels && binding.intent.labels.length > 0
				? `labels: ${binding.intent.labels
						.slice(0, 8)
						.map((label) => truncateMemoryText(label, MAX_MEMORY_FIELD_CHARS))
						.join(', ')}`
				: undefined,
			binding.required_capabilities.length > 0
				? `required_capabilities: ${binding.required_capabilities
						.map((capability) => formatMemoryFieldValue(capability))
						.join(', ')}`
				: undefined,
			`scope: agent_id=${formatMemoryFieldValue(binding.scope.agent_id)}; run_id=${formatMemoryFieldValue(
				binding.scope.run_id,
			)}${
				binding.scope.user_id ? `; user_id=${formatMemoryFieldValue(binding.scope.user_id)}` : ''
			}`,
			binding.write.enabled
				? `write: enabled (${formatMemoryFieldValue(binding.write.mode)})`
				: `write: disabled${
						binding.write.disabled_reason
							? ` (${formatMemoryFieldValue(binding.write.disabled_reason)})`
							: ''
					}`,
			binding.read ? formatOptionalMemoryField('read_query', binding.read.query) : undefined,
		]) {
			if (line && !appendBoundedLine(lines, line)) {
				break
			}
		}

		const records = binding.read?.records.slice(0, MAX_MEMORY_CONTEXT_RECORDS_PER_BINDING) ?? []
		if (records.length === 0) {
			if (!appendBoundedLine(lines, 'records: none')) {
				break
			}
			continue
		}

		if (!appendBoundedLine(lines, 'records:')) {
			break
		}
		for (const [recordIndex, record] of records.entries()) {
			const recordHeader = [
				`  ${recordIndex + 1}. id=${truncateMemoryText(record.id, MAX_MEMORY_FIELD_CHARS)}`,
				record.score === undefined ? undefined : `score=${record.score}`,
				record.created_at
					? `created_at=${truncateMemoryText(record.created_at, MAX_MEMORY_FIELD_CHARS)}`
					: undefined,
				record.updated_at
					? `updated_at=${truncateMemoryText(record.updated_at, MAX_MEMORY_FIELD_CHARS)}`
					: undefined,
			]
				.filter((part): part is string => part !== undefined)
				.join('; ')

			if (!appendBoundedLine(lines, recordHeader)) {
				break
			}
			if (!appendBoundedLine(lines, `     scope: ${formatMemoryScope(record.scope)}`)) {
				break
			}
			if (
				!appendBoundedLine(
					lines,
					`     content: ${truncateMemoryText(record.content, MAX_MEMORY_RECORD_CONTENT_CHARS)}`,
				)
			) {
				break
			}
		}
	}

	if (memoryContext && memoryContext.bindings.length > MAX_MEMORY_CONTEXT_BINDINGS) {
		appendBoundedLine(
			lines,
			`Additional memory bindings omitted: ${
				memoryContext.bindings.length - MAX_MEMORY_CONTEXT_BINDINGS
			}`,
		)
	}

	return finalizeMemoryContext(lines)
}

function renderPromptWithMemoryContext(
	prompt: string,
	memoryContext: RuntimeAdapterExecutionRequest['memory_context'],
): string {
	const renderedMemoryContext = renderMemoryContext(memoryContext)
	if (!renderedMemoryContext) {
		return prompt
	}

	return `${prompt}\n\n${renderedMemoryContext}`
}

function buildThreadStartParams(args: {
	cwd: string
	memoryContext?: RuntimeAdapterExecutionRequest['memory_context']
	model?: string
	prompt: string
	speed_tier?: RuntimeSpeedTier
	personality?: RuntimePersonality
}): Record<string, unknown> {
	return {
		cwd: args.cwd,
		developerInstructions: renderPromptWithMemoryContext(args.prompt, args.memoryContext),
		model: args.model ?? null,
		serviceTier: args.speed_tier ?? null,
		personality: args.personality ?? null,
		sessionStartSource: 'startup',
		sandbox: 'danger-full-access',
		approvalPolicy: 'never',
	}
}

function buildThreadResumeParams(args: {
	cwd: string
	memoryContext?: RuntimeAdapterExecutionRequest['memory_context']
	model?: string
	prompt: string
	speed_tier?: RuntimeSpeedTier
	personality?: RuntimePersonality
	threadId: string
}): Record<string, unknown> {
	return {
		threadId: args.threadId,
		cwd: args.cwd,
		developerInstructions: renderPromptWithMemoryContext(args.prompt, args.memoryContext),
		model: args.model ?? null,
		serviceTier: args.speed_tier ?? null,
		personality: args.personality ?? null,
		sandbox: 'danger-full-access',
		approvalPolicy: 'never',
	}
}

function buildTurnStartParams(args: {
	cwd: string
	model?: string
	personality?: RuntimePersonality
	reasoning_effort?: RuntimeReasoningEffort
	speed_tier?: RuntimeSpeedTier
	output: RuntimeAdapterExecutionRequest['output']
	threadId: string
	userInput: JsonValue
}): Record<string, unknown> {
	const finalAnswerInput =
		typeof args.userInput === 'string' ? args.userInput : JSON.stringify(args.userInput)

	return {
		threadId: args.threadId,
		cwd: args.cwd,
		model: args.model ?? null,
		serviceTier: args.speed_tier ?? null,
		effort: args.reasoning_effort ?? null,
		personality: args.personality ?? null,
		input: [
			{
				type: 'text',
				text: finalAnswerInput,
			},
		],
		outputSchema: args.output.mode === 'json' ? args.output.schema : undefined,
	}
}

function buildSteerParams(args: {
	handle: LiveRuntimeHandle
	text: string
}): Record<string, unknown> {
	return {
		threadId: args.handle.threadId,
		expectedTurnId: args.handle.turnId,
		input: [
			{
				type: 'text',
				text: args.text,
			},
		],
	}
}

function buildInterruptParams(handle: LiveRuntimeHandle): Record<string, unknown> {
	return {
		threadId: handle.threadId,
		turnId: handle.turnId,
	}
}

class AppServerLaunchError extends Error {
	constructor(
		message: string,
		readonly launcher: AppServerLauncher,
		readonly code?: string,
	) {
		super(message)
		this.name = 'AppServerLaunchError'
	}
}

class AppServerJsonRpcClient {
	private readonly pendingUserInputRequests = new Map<string, PendingUserInputRequest>()
	private readonly completedItemsByTurnId = new Map<string, AppServerThreadItem[]>()

	static async createInitialized(
		workingDirectory: string,
		options: AppServerClientOptions = {},
	): Promise<AppServerJsonRpcClient> {
		const launchers = resolveAppServerLaunchers()
		let lastError: unknown

		for (let index = 0; index < launchers.length; index += 1) {
			const launcher = launchers[index]
			if (!launcher) {
				continue
			}
			const client = new AppServerJsonRpcClient(workingDirectory, launcher, options)

			try {
				await client.initialize()
				return client
			} catch (error) {
				lastError = error
				await client.close().catch(() => undefined)

				const canRetry =
					error instanceof AppServerLaunchError &&
					(error.code === 'ENOENT' || error.code === 'EPERM' || error.code === 'EACCES') &&
					index < launchers.length - 1
				if (!canRetry) {
					throw error
				}
			}
		}

		throw lastError instanceof Error ? lastError : new Error('Failed to launch Codex App Server.')
	}

	private readonly child: ChildProcessWithoutNullStreams
	private readonly stdoutLines: ReturnType<typeof createInterface>
	private readonly pendingRequests = new Map<string | number, RequestContext>()
	private readonly turnWaiters = new Map<
		string,
		Array<{
			resolve: (turn: AppServerTurn) => void
			reject: (error: Error) => void
		}>
	>()
	private readonly turnStates = new Map<string, TurnState>()
	private readonly stderrChunks: string[] = []
	private nextRequestId = 1
	private closed = false
	private fatalError: Error | undefined
	private readonly eventSink?: (event: RuntimeEvent) => void
	private pendingAutoUserChatReply: UserChatResponsePayload | undefined
	private readonly timeoutBudget?: AppServerTimeoutBudget

	private constructor(
		private readonly workingDirectory: string,
		private readonly launcher: AppServerLauncher,
		options: AppServerClientOptions,
	) {
		this.eventSink = options.onEvent
		this.pendingAutoUserChatReply = options.pendingAutoUserChatReply
		this.timeoutBudget = options.timeoutBudget
		this.child = spawn(launcher.command, launcher.args, {
			cwd: this.workingDirectory,
			stdio: ['pipe', 'pipe', 'pipe'],
			env: process.env,
			windowsHide: true,
		})

		this.stdoutLines = createInterface({
			input: this.child.stdout,
		})
		this.stdoutLines.on('line', (line) => {
			void this.handleLine(line)
		})
		this.child.stderr.on('data', (chunk: Buffer) => {
			this.stderrChunks.push(chunk.toString('utf8'))
		})
		this.child.once('error', (error: NodeJS.ErrnoException) => {
			this.handleProcessError(error)
		})
		this.child.once('exit', (code, signal) => {
			void this.handleExit(code, signal)
		})
	}

	async initialize(): Promise<void> {
		await this.request('initialize', {
			clientInfo: {
				name: 'dennett-agent-orchestrator',
				title: 'Dennett Agent Orchestrator',
				version: '0.0.0',
			},
			capabilities: {
				experimentalApi: true,
				optOutNotificationMethods: [],
			},
		})
	}

	async startThread(params: Record<string, unknown>): Promise<AppServerThreadStartResponse> {
		return await this.request<AppServerThreadStartResponse>('thread/start', params)
	}

	async resumeThread(params: Record<string, unknown>): Promise<AppServerThreadStartResponse> {
		return await this.request<AppServerThreadStartResponse>('thread/resume', params)
	}

	async startTurn(params: Record<string, unknown>): Promise<AppServerTurnStartResponse> {
		return await this.request<AppServerTurnStartResponse>('turn/start', params)
	}

	async steerTurn(params: Record<string, unknown>): Promise<void> {
		await this.request('turn/steer', params)
	}

	async interruptTurn(params: Record<string, unknown>): Promise<void> {
		await this.request('turn/interrupt', params)
	}

	async listModels(params: Record<string, unknown>): Promise<AppServerModelListResponse> {
		return await this.request<AppServerModelListResponse>('model/list', params)
	}

	async getAuthStatus(): Promise<AppServerAuthStatusResponse> {
		return await this.request<AppServerAuthStatusResponse>('getAuthStatus')
	}

	async readAccount(): Promise<AppServerAccountReadResponse> {
		return await this.request<AppServerAccountReadResponse>('account/read')
	}

	async readAccountRateLimits(): Promise<AppServerRateLimitReadResponse> {
		return await this.request<AppServerRateLimitReadResponse>('account/rateLimits/read')
	}

	async readConfig(): Promise<AppServerConfigReadResponse> {
		return await this.request<AppServerConfigReadResponse>('config/read')
	}

	async readConfigRequirements(): Promise<AppServerConfigRequirementsReadResponse> {
		return await this.request<AppServerConfigRequirementsReadResponse>('configRequirements/read')
	}

	async deliverUserInputResponse(
		handle: unknown,
		response: UserChatResponsePayload,
	): Promise<void> {
		const normalizedHandle = normalizePendingUserInputPromptHandle(handle)
		const key = buildPendingUserInputKey(normalizedHandle)
		const pending = this.pendingUserInputRequests.get(key)
		if (!pending) {
			throw new Error(
				'No matching pending built-in user-chat prompt exists on the active App Server session.',
			)
		}

		if (response.prompt_id && response.prompt_id !== pending.handle.prompt_id) {
			throw new Error(
				`Reply targets prompt "${response.prompt_id}", but the active App Server prompt is "${pending.handle.prompt_id}".`,
			)
		}

		try {
			pending.resolveResponse(buildUserChatReplyResult(pending, response))
			await pending.deliveryCompleted
		} finally {
			this.pendingUserInputRequests.delete(key)
		}
	}

	async close(): Promise<void> {
		if (this.closed) {
			return
		}
		this.closed = true
		this.stdoutLines.close()
		if (this.child.exitCode !== null || this.child.signalCode !== null) {
			this.child.stdin.end()
			return
		}
		this.child.stdin.end()
		this.child.kill()
		await once(this.child, 'close').catch(() => undefined)
	}

	waitForTurnCompletion(turnId: string): Promise<AppServerTurn> {
		const cached = this.turnStates.get(turnId)
		if (cached?.settled) {
			return Promise.resolve(cached.turn)
		}

		return new Promise<AppServerTurn>((resolve, reject) => {
			const existing = this.turnStates.get(turnId)
			if (existing?.settled) {
				resolve(existing.turn)
				return
			}

			const queue = this.turnWaiters.get(turnId) ?? []
			queue.push({ resolve, reject })
			this.turnWaiters.set(turnId, queue)
		})
	}

	private async handleLine(line: string): Promise<void> {
		if (line.trim().length === 0) {
			return
		}

		let message: JsonRpcMessage
		try {
			message = JSON.parse(line) as JsonRpcMessage
		} catch {
			this.failPendingRequests(new Error(`Failed to parse App Server JSON: ${line}`))
			return
		}

		if (this.isResponse(message)) {
			const pending = this.pendingRequests.get(message.id)
			if (!pending) {
				return
			}
			this.pendingRequests.delete(message.id)
			if (message.error) {
				pending.reject(new Error(message.error.message))
				return
			}
			pending.resolve(message.result)
			return
		}

		if (this.isRequest(message)) {
			await this.handleServerRequest(message)
			return
		}

		this.handleNotification(message)
	}

	private isResponse(message: JsonRpcMessage): message is JsonRpcResponse {
		return 'id' in message && !('method' in message)
	}

	private isRequest(message: JsonRpcMessage): message is JsonRpcRequest {
		return 'id' in message && 'method' in message
	}

	private async handleServerRequest(request: JsonRpcRequest): Promise<void> {
		try {
			const result = await this.resolveServerRequest(request)
			await this.writeJsonAndWait({
				id: request.id,
				result,
			})
			this.resolvePendingUserInputDelivery(request)
		} catch (error) {
			const normalizedError =
				error instanceof Error ? error : new Error('Unknown App Server request failure.')
			this.rejectPendingUserInputDelivery(request, normalizedError)
			await this.writeJsonAndWait({
				id: request.id,
				error: {
					code: -32000,
					message: normalizedError.message,
				},
			}).catch(() => undefined)
		}
	}

	private resolveServerRequest(request: JsonRpcRequest): unknown {
		if (request.method === 'item/tool/requestUserInput') {
			return this.createDeferredUserInputResponse(request)
		}

		throw new Error(
			`unsupported_request: App Server request "${request.method}" is not supported by the Codex adapter.`,
		)
	}

	private createDeferredUserInputResponse(request: JsonRpcRequest): Promise<unknown> {
		const normalized = normalizePendingUserInputRequest(request)
		const key = buildPendingUserInputKey(normalized.handle)
		this.emitRuntimeEvent({
			kind: 'user_chat_request',
			request_handle: normalized.handle,
			payload: normalized.payload as RuntimeUserChatRequestEvent['payload'],
		})

		if (this.pendingAutoUserChatReply) {
			const autoReply = this.pendingAutoUserChatReply
			if (autoReply.prompt_id && autoReply.prompt_id !== normalized.handle.prompt_id) {
				throw new Error(
					`Reply targets prompt "${autoReply.prompt_id}", but the active App Server prompt is "${normalized.handle.prompt_id}".`,
				)
			}
			this.pendingAutoUserChatReply = undefined
			return Promise.resolve(buildUserChatReplyResult(normalized, autoReply))
		}

		if (this.pendingUserInputRequests.has(key)) {
			throw new Error(
				`unsupported_request: built-in user-chat prompt "${normalized.handle.prompt_id}" is already pending on this App Server session.`,
			)
		}

		disarmTimeoutBudget(this.timeoutBudget)
		let resolveDelivery: () => void = () => undefined
		let rejectDelivery: (error: Error) => void = () => undefined
		const deliveryCompleted = new Promise<void>((resolve, reject) => {
			resolveDelivery = resolve
			rejectDelivery = reject
		})
		return new Promise<unknown>((resolve, reject) => {
			this.pendingUserInputRequests.set(key, {
				...normalized,
				resolveResponse: resolve,
				rejectResponse: reject,
				deliveryCompleted,
				resolveDelivery,
				rejectDelivery,
			})
		})
	}

	private emitRuntimeEvent(event: RuntimeEvent): void {
		this.eventSink?.(event)
	}

	private handleNotification(message: JsonRpcMessage): void {
		if (!('method' in message)) {
			return
		}

		switch (message.method) {
			case 'item/completed': {
				const params = isObject(message.params) ? message.params : {}
				const turnId = typeof params.turnId === 'string' ? params.turnId : undefined
				const item = toJsonObject(params.item)
				if (!turnId || !item || typeof item.type !== 'string' || typeof item.id !== 'string') {
					return
				}

				const existingItems = this.completedItemsByTurnId.get(turnId) ?? []
				this.completedItemsByTurnId.set(
					turnId,
					upsertTurnItem(existingItems, item as unknown as AppServerThreadItem),
				)
				return
			}
			case 'turn/completed':
			case 'turn/started':
			case 'turn/status/changed': {
				const params = isObject(message.params) ? message.params : {}
				const turn = toJsonObject(params.turn)
				if (
					!turn ||
					typeof turn.id !== 'string' ||
					typeof turn.status !== 'string' ||
					!Array.isArray(turn.items)
				) {
					return
				}
				const completedItems = this.completedItemsByTurnId.get(turn.id) ?? []
				const normalizedTurn = {
					...(turn as unknown as AppServerTurn),
					items: mergeTurnItems(turn.items as AppServerThreadItem[], completedItems),
				}
				const state: TurnState = {
					turnId: normalizedTurn.id,
					turn: normalizedTurn,
					settled: normalizedTurn.status !== 'inProgress',
				}
				this.turnStates.set(normalizedTurn.id, state)
				if (state.settled) {
					const waiters = this.turnWaiters.get(normalizedTurn.id)
					if (waiters) {
						this.turnWaiters.delete(normalizedTurn.id)
						for (const waiter of waiters) {
							waiter.resolve(normalizedTurn)
						}
					}
				}
				return
			}
			default:
				return
		}
	}

	private writeJson(message: JsonRpcMessage): void {
		this.child.stdin.write(`${JSON.stringify(message)}\n`)
	}

	private writeJsonAndWait(message: JsonRpcMessage): Promise<void> {
		return new Promise<void>((resolve, reject) => {
			try {
				this.child.stdin.write(`${JSON.stringify(message)}\n`, (error?: Error | null) => {
					if (error) {
						reject(error)
						return
					}
					resolve()
				})
			} catch (error) {
				reject(error instanceof Error ? error : new Error('Failed to write App Server JSON.'))
			}
		})
	}

	private resolvePendingUserInputDelivery(request: JsonRpcRequest): void {
		const pending = this.getPendingUserInputForServerRequest(request)
		pending?.resolveDelivery()
	}

	private rejectPendingUserInputDelivery(request: JsonRpcRequest, error: Error): void {
		const pending = this.getPendingUserInputForServerRequest(request)
		pending?.rejectDelivery(error)
	}

	private getPendingUserInputForServerRequest(
		request: JsonRpcRequest,
	): PendingUserInputRequest | undefined {
		if (request.method !== 'item/tool/requestUserInput') {
			return undefined
		}
		try {
			const normalized = normalizePendingUserInputRequest(request)
			return this.pendingUserInputRequests.get(buildPendingUserInputKey(normalized.handle))
		} catch {
			return undefined
		}
	}

	private async request<T>(method: string, params?: unknown): Promise<T> {
		if (this.closed) {
			throw new Error('App Server client is already closed.')
		}
		if (this.fatalError) {
			throw this.fatalError
		}

		const id = this.nextRequestId++
		const payload: JsonRpcRequest = {
			id,
			method,
			params: params ?? {},
		}

		let timeout: ReturnType<typeof setTimeout> | undefined
		const remainingMs = getRemainingTimeoutMs(this.timeoutBudget)
		if (remainingMs !== undefined && remainingMs <= 0) {
			throw createTimeoutError(this.timeoutBudget as AppServerTimeoutBudget, method)
		}

		const result = await new Promise<T>((resolve, reject) => {
			this.pendingRequests.set(id, {
				resolve: (value) => resolve(value as T),
				reject,
			})
			if (remainingMs !== undefined) {
				timeout = setTimeout(() => {
					this.pendingRequests.delete(id)
					reject(createTimeoutError(this.timeoutBudget as AppServerTimeoutBudget, method))
				}, remainingMs)
				registerTimeoutBudgetTimer(this.timeoutBudget, timeout)
			}
			this.writeJson(payload)
		}).finally(() => {
			clearTimeoutBudgetTimer(this.timeoutBudget, timeout)
		})

		return result
	}

	private failPendingRequests(error: Error): void {
		this.fatalError = error

		for (const [, pending] of this.pendingRequests) {
			pending.reject(error)
		}
		this.pendingRequests.clear()

		for (const [, pendingUserInput] of this.pendingUserInputRequests) {
			pendingUserInput.rejectResponse(error)
		}
		this.pendingUserInputRequests.clear()

		for (const [, waiters] of this.turnWaiters) {
			for (const waiter of waiters) {
				waiter.reject(error)
			}
		}
		this.turnWaiters.clear()
	}

	private handleProcessError(error: NodeJS.ErrnoException): void {
		if (this.closed) {
			return
		}

		this.failPendingRequests(
			new AppServerLaunchError(
				`Failed to launch App Server via ${formatLauncher(this.launcher)}: ${error.message}`,
				this.launcher,
				error.code,
			),
		)
	}

	private async handleExit(code: number | null, signal: NodeJS.Signals | null): Promise<void> {
		if (this.closed) {
			return
		}

		const stderr = this.stderrChunks.join('')
		const reason =
			stderr.trim().length > 0
				? stderr.trim()
				: `App Server exited with code ${code ?? 'unknown'}${signal ? ` and signal ${signal}` : ''}.`
		this.failPendingRequests(new Error(reason))
	}
}

function formatLauncher(launcher: AppServerLauncher): string {
	return [launcher.command, ...launcher.args].join(' ')
}

function buildPendingUserInputKey(handle: AppServerPendingUserInputPromptHandle): string {
	return `${String(handle.requestId)}:${handle.threadId}:${handle.turnId}:${handle.itemId}:${handle.prompt_id}`
}

function normalizePendingUserInputPromptHandle(
	handle: unknown,
): AppServerPendingUserInputPromptHandle {
	if (
		!isObject(handle) ||
		handle.kind !== 'codex_app_server_user_chat_request' ||
		typeof handle.threadId !== 'string' ||
		typeof handle.turnId !== 'string' ||
		typeof handle.itemId !== 'string' ||
		(typeof handle.requestId !== 'string' && typeof handle.requestId !== 'number') ||
		typeof handle.prompt_id !== 'string'
	) {
		throw new Error(
			'built-in user-chat replies require a live Codex App Server prompt handle from the same adapter session.',
		)
	}

	return {
		kind: 'codex_app_server_user_chat_request',
		threadId: handle.threadId,
		turnId: handle.turnId,
		itemId: handle.itemId,
		requestId: handle.requestId,
		prompt_id: handle.prompt_id,
	}
}

function normalizePendingUserInputRequest(
	request: JsonRpcRequest,
): NormalizedPendingUserInputRequest {
	const params = isObject(request.params)
		? (request.params as AppServerToolRequestUserInputParams)
		: {}
	if (
		typeof params.threadId !== 'string' ||
		typeof params.turnId !== 'string' ||
		typeof params.itemId !== 'string'
	) {
		throw new Error(
			'unsupported_request: App Server user-input request is missing threadId, turnId, or itemId.',
		)
	}
	if (!Array.isArray(params.questions) || params.questions.length !== 1) {
		throw new Error(
			'unsupported_request: The current Codex adapter supports only single-question built-in user-chat prompts.',
		)
	}

	const question = params.questions[0]
	if (!isObject(question)) {
		throw new Error('unsupported_request: App Server user-input question must be an object.')
	}
	if (
		typeof question.id !== 'string' ||
		typeof question.question !== 'string' ||
		typeof question.header !== 'string'
	) {
		throw new Error(
			'unsupported_request: App Server user-input question is missing id, header, or question text.',
		)
	}
	if (question.isSecret === true) {
		throw new Error(
			'unsupported_request: Secret built-in user-chat prompts are not supported by the current Codex adapter.',
		)
	}

	const promptId = question.id
	const handle: AppServerPendingUserInputPromptHandle = {
		kind: 'codex_app_server_user_chat_request',
		threadId: params.threadId,
		turnId: params.turnId,
		itemId: params.itemId,
		requestId: request.id,
		prompt_id: promptId,
	}

	if (question.options === null || question.options === undefined) {
		return {
			handle,
			payload: {
				kind: 'text',
				prompt_id: promptId,
				text: question.question,
				require_response: true,
			},
		}
	}

	if (!Array.isArray(question.options) || question.options.length === 0) {
		throw new Error(
			'unsupported_request: App Server options prompts must include at least one option.',
		)
	}
	if (question.isOther === true) {
		throw new Error(
			'unsupported_request: Mixed options-plus-freeform built-in user-chat prompts are not supported by the current Codex adapter.',
		)
	}

	const normalizedOptions = question.options.map((option, index) => {
		if (!isObject(option) || typeof option.label !== 'string') {
			throw new Error(
				'unsupported_request: App Server prompt options must be objects with string labels.',
			)
		}
		return {
			id: `option-${index + 1}`,
			label: option.label,
			value: option.label,
		}
	})

	return {
		handle,
		payload: {
			kind: 'options',
			prompt_id: promptId,
			text: question.question,
			require_response: true,
			options: normalizedOptions as [
				(typeof normalizedOptions)[number],
				...typeof normalizedOptions,
			],
		},
	}
}

function resolveAppServerLaunchers(): AppServerLauncher[] {
	const launchers: AppServerLauncher[] = []
	if (process.platform === 'win32') {
		launchers.push({
			command: 'cmd.exe',
			args: ['/d', '/s', '/c', 'codex.cmd', 'app-server', '--listen', 'stdio://'],
		})
	} else {
		launchers.push({
			command: 'codex',
			args: ['app-server', '--listen', 'stdio://'],
		})
	}

	const npmExecPath = process.env.npm_execpath
	if (npmExecPath && path.basename(npmExecPath).toLowerCase().includes('pnpm')) {
		launchers.push({
			command: process.execPath,
			args: [npmExecPath, 'exec', 'codex', 'app-server', '--listen', 'stdio://'],
		})
		return launchers
	}

	launchers.push({
		command: process.platform === 'win32' ? 'cmd.exe' : 'pnpm',
		args:
			process.platform === 'win32'
				? ['/d', '/s', '/c', 'pnpm', 'exec', 'codex', 'app-server', '--listen', 'stdio://']
				: ['exec', 'codex', 'app-server', '--listen', 'stdio://'],
	})
	return launchers
}

async function prepareExecution(
	request: RuntimeAdapterExecutionRequest,
	workingDirectory: string,
	adapterOptions: CodexAppServerRuntimeAdapterOptions,
	liveHooks?: {
		registerClient?: (threadId: string, client: AppServerJsonRpcClient) => void
		unregisterClient?: (threadId: string) => void
	},
): Promise<RuntimeExecutionSession> {
	const { model, personality, reasoning_effort, speed_tier } = parseRuntimeOptions(
		request.runtime_options,
	)
	const eventQueue = new AsyncEventQueue<RuntimeEvent>()
	const timeoutBudget = createTimeoutBudget(
		'CODEX_APP_SERVER_EXECUTION_TIMEOUT',
		'runtime_execution',
		adapterOptions.execution_timeout_ms,
	)
	let client: AppServerJsonRpcClient | undefined
	let terminalResultPromise: Promise<RuntimeTerminalResult> | undefined

	try {
		client = await AppServerJsonRpcClient.createInitialized(workingDirectory, {
			onEvent: (event) => {
				eventQueue.push(event)
			},
			pendingAutoUserChatReply: request.interaction.user_chat_reply,
			timeoutBudget,
		})
		const threadResponse =
			request.resume.mode === 'native_resume'
				? await client.resumeThread(
						buildThreadResumeParams({
							cwd: workingDirectory,
							memoryContext: request.memory_context,
							model,
							personality,
							prompt: request.prompt,
							speed_tier,
							threadId: normalizeNativeHandle(request.resume.native_session_handle).threadId,
						}),
					)
				: await client.startThread(
						buildThreadStartParams({
							cwd: workingDirectory,
							memoryContext: request.memory_context,
							model,
							personality,
							prompt: request.prompt,
							speed_tier,
						}),
					)

		const threadId = threadResponse.thread.id
		const turnResponse = await client.startTurn(
			buildTurnStartParams({
				cwd: workingDirectory,
				model,
				personality,
				output: request.output,
				reasoning_effort,
				speed_tier,
				threadId,
				userInput: request.input_message,
			}),
		)

		const runtimeHandle: LiveRuntimeHandle = {
			threadId,
			turnId: turnResponse.turn.id,
		}
		const nativeHandle: NativeSessionHandle = {
			threadId,
		}
		liveHooks?.registerClient?.(threadId, client)

		terminalResultPromise = (async (): Promise<RuntimeTerminalResult> => {
			try {
				const completedTurn =
					turnResponse.turn.status === 'inProgress'
						? await withAppServerTimeout(
								client.waitForTurnCompletion(turnResponse.turn.id),
								timeoutBudget,
								'turn/completion',
							)
						: turnResponse.turn

				if (completedTurn.status === 'completed') {
					const outputText = extractFinalAnswerText(completedTurn)
					if (request.output.mode === 'text') {
						if (outputText === undefined) {
							return {
								outcome: 'invalid_output',
								error: {
									code: 'INVALID_TEXT_OUTPUT',
									message: 'App Server completed the turn without a final text response.',
								},
								native_session_handle: nativeHandle,
							}
						}

						return {
							outcome: 'success',
							output: request.output,
							output_text: outputText,
							native_session_handle: nativeHandle,
						}
					}

					if (outputText === undefined) {
						return {
							outcome: 'invalid_output',
							error: {
								code: 'INVALID_JSON_OUTPUT',
								message: 'App Server completed the turn without a final JSON response.',
							},
							native_session_handle: nativeHandle,
						}
					}

					try {
						const parsed = JSON.parse(outputText) as unknown
						if (!isObject(parsed)) {
							return {
								outcome: 'invalid_output',
								error: {
									code: 'INVALID_JSON_OUTPUT',
									message: 'App Server returned a non-object JSON value for a json output node.',
									details: {
										outputText,
									},
								},
								native_session_handle: nativeHandle,
							}
						}

						const validation = validateJsonOutputAgainstSchema(
							request.output.schema,
							parsed as JsonObject,
						)
						if (!validation.valid) {
							return {
								outcome: 'invalid_output',
								error: {
									code: 'INVALID_JSON_OUTPUT',
									message:
										validation.message ??
										'App Server returned JSON that failed output schema validation.',
									details: {
										outputText,
										issues: validation.issues,
									},
								},
								native_session_handle: nativeHandle,
							}
						}

						return {
							outcome: 'success',
							output: request.output,
							output_json: parsed as JsonObject,
							native_session_handle: nativeHandle,
						}
					} catch {
						return {
							outcome: 'invalid_output',
							error: {
								code: 'INVALID_JSON_OUTPUT',
								message: 'App Server returned output that could not be parsed as JSON.',
								details: {
									outputText,
								},
							},
							native_session_handle: nativeHandle,
						}
					}
				}

				if (completedTurn.status === 'interrupted') {
					return {
						outcome: 'interrupted',
						error: {
							code: 'TURN_INTERRUPTED',
							message: completedTurn.error?.message ?? 'App Server interrupted the active turn.',
							details: completedTurn.error ?? undefined,
						},
						native_session_handle: nativeHandle,
					}
				}

				if (completedTurn.status === 'failed') {
					return {
						outcome: 'runtime_error',
						error: {
							code: 'CODEX_APP_SERVER_FAILED',
							message: completedTurn.error?.message ?? 'App Server reported a failed turn.',
							details: completedTurn.error ?? undefined,
						},
						native_session_handle: nativeHandle,
					}
				}

				return {
					outcome: 'runtime_error',
					error: {
						code: 'CODEX_APP_SERVER_FAILED',
						message: 'App Server returned an unsupported turn state.',
						details: completedTurn,
					},
					native_session_handle: nativeHandle,
				}
			} catch (error) {
				if (error instanceof AppServerTimeoutError) {
					return toFailure(error.code, error.message, error.details)
				}
				const message = error instanceof Error ? error.message : 'Unknown App Server error.'
				if (message.startsWith('unsupported_request:')) {
					return toFailure(
						'UNSUPPORTED_REQUEST',
						'Codex rejected the request as unsupported by the current App Server adapter.',
						{
							requested_model: readRequestedModel(request.runtime_options),
							details: message,
						},
					)
				}
				return toFailure('CODEX_APP_SERVER_FAILED', message, error)
			} finally {
				liveHooks?.unregisterClient?.(threadId)
				await client.close().catch(() => undefined)
				eventQueue.close()
			}
		})()

		return {
			runtime_handle: runtimeHandle,
			native_session_handle: nativeHandle,
			terminal_result: terminalResultPromise,
			events: eventQueue,
		}
	} catch (error) {
		await client?.close().catch(() => undefined)
		eventQueue.close()
		if (error instanceof AppServerTimeoutError) {
			return {
				runtime_handle: null,
				native_session_handle: null,
				terminal_result: Promise.resolve(toFailure(error.code, error.message, error.details)),
				events: eventQueue,
			}
		}
		const message = error instanceof Error ? error.message : 'Unknown App Server error.'
		if (message.startsWith('unsupported_request:')) {
			return {
				runtime_handle: null,
				native_session_handle: null,
				terminal_result: Promise.resolve(
					toFailure(
						'UNSUPPORTED_REQUEST',
						'Codex rejected the request as unsupported by the current App Server adapter.',
						{
							requested_model: readRequestedModel(request.runtime_options),
							details: message,
						},
					),
				),
				events: eventQueue,
			}
		}
		return {
			runtime_handle: null,
			native_session_handle: null,
			terminal_result: Promise.resolve(toFailure('CODEX_APP_SERVER_FAILED', message, error)),
			events: eventQueue,
		}
	}
}

async function performLiveTurnAction(
	workingDirectory: string,
	adapterOptions: CodexAppServerRuntimeAdapterOptions,
	handle: unknown,
	action: 'steer' | 'interrupt',
	text?: string,
): Promise<void> {
	const liveHandle = normalizeLiveHandle(handle)
	const timeoutBudget =
		action === 'steer'
			? createTimeoutBudget(
					'CODEX_APP_SERVER_COMMENT_TIMEOUT',
					'live_comment',
					adapterOptions.comment_timeout_ms,
				)
			: undefined
	const client = await AppServerJsonRpcClient.createInitialized(workingDirectory, {
		timeoutBudget,
	})

	try {
		if (action === 'steer') {
			await client.steerTurn(buildSteerParams({ handle: liveHandle, text: text ?? '' }))
			return
		}
		await client.interruptTurn(buildInterruptParams(liveHandle))
	} finally {
		await client.close().catch(() => undefined)
	}
}

function toStringArray(value: unknown): string[] {
	if (!Array.isArray(value)) {
		return []
	}
	return value.filter((entry): entry is string => typeof entry === 'string')
}

function normalizeModelDescriptor(model: unknown) {
	const modelObject = isObject(model) ? model : {}
	const idCandidate = typeof modelObject.model === 'string' ? modelObject.model : modelObject.id
	if (typeof idCandidate !== 'string' || idCandidate.trim().length === 0) {
		return null
	}

	const supportedReasoningEfforts = toStringArray(modelObject.supportedReasoningEfforts).filter(
		(entry): entry is RuntimeReasoningEffort =>
			REASONING_EFFORTS.has(entry as RuntimeReasoningEffort),
	)
	const additionalSpeedTiers = toStringArray(modelObject.additionalSpeedTiers).filter(
		(entry): entry is RuntimeSpeedTier => SPEED_TIERS.has(entry as RuntimeSpeedTier),
	)
	const defaultReasoningEffort =
		typeof modelObject.defaultReasoningEffort === 'string' &&
		REASONING_EFFORTS.has(modelObject.defaultReasoningEffort as RuntimeReasoningEffort)
			? (modelObject.defaultReasoningEffort as RuntimeReasoningEffort)
			: undefined

	return {
		id: idCandidate,
		...(typeof modelObject.displayName === 'string'
			? { display_name: modelObject.displayName }
			: {}),
		...(typeof modelObject.description === 'string'
			? { description: modelObject.description }
			: {}),
		hidden: modelObject.hidden === true,
		is_default: modelObject.isDefault === true,
		input_modalities: toStringArray(modelObject.inputModalities),
		supports_personality: modelObject.supportsPersonality === true,
		...(defaultReasoningEffort ? { default_reasoning_effort: defaultReasoningEffort } : {}),
		supported_reasoning_efforts: supportedReasoningEfforts,
		additional_speed_tiers: additionalSpeedTiers,
		...(typeof modelObject.upgrade === 'string' ? { upgrade_target: modelObject.upgrade } : {}),
		...(typeof modelObject.upgradeInfo === 'string'
			? { upgrade_info: modelObject.upgradeInfo }
			: {}),
	}
}

function normalizeRateLimit(value: unknown): RuntimeRateLimitSummary | null {
	const rateLimit = isObject(value) ? value : {}
	if (typeof rateLimit.limitId !== 'string' || rateLimit.limitId.trim().length === 0) {
		return null
	}

	return {
		limit_id: rateLimit.limitId,
		...(typeof rateLimit.limitName === 'string' ? { limit_name: rateLimit.limitName } : {}),
		...(typeof rateLimit.planType === 'string' ? { plan_type: rateLimit.planType } : {}),
		...(toJsonObject(rateLimit.primary) ? { primary: toJsonObject(rateLimit.primary) } : {}),
		...(toJsonObject(rateLimit.secondary) ? { secondary: toJsonObject(rateLimit.secondary) } : {}),
		...(toJsonObject(rateLimit.credits) ? { credits: toJsonObject(rateLimit.credits) } : {}),
	}
}

async function listRuntimeModelsViaAppServer(
	workingDirectory: string,
	adapterOptions: CodexAppServerRuntimeAdapterOptions,
	request: RuntimeModelCatalogRequest = {},
): Promise<RuntimeModelCatalogPage> {
	const client = await AppServerJsonRpcClient.createInitialized(workingDirectory, {
		timeoutBudget: createTimeoutBudget(
			'CODEX_APP_SERVER_MODEL_CATALOG_TIMEOUT',
			'runtime_model_catalog',
			adapterOptions.model_catalog_timeout_ms,
		),
	})
	try {
		const response = await client.listModels({
			...(request.cursor ? { cursor: request.cursor } : {}),
			...(request.limit !== undefined ? { limit: request.limit } : {}),
			...(request.include_hidden !== undefined ? { includeHidden: request.include_hidden } : {}),
		})
		const models = Array.isArray(response.data)
			? response.data
					.map((entry) => normalizeModelDescriptor(entry))
					.filter((entry): entry is NonNullable<typeof entry> => entry !== null)
			: []

		return {
			models,
			...(typeof response.nextCursor === 'string' && response.nextCursor.length > 0
				? { next_cursor: response.nextCursor }
				: {}),
		}
	} finally {
		await client.close().catch(() => undefined)
	}
}

async function inspectRuntimeEnvironmentViaAppServer(
	workingDirectory: string,
	adapterOptions: CodexAppServerRuntimeAdapterOptions,
): Promise<RuntimeEnvironmentInspectionResult> {
	const client = await AppServerJsonRpcClient.createInitialized(workingDirectory, {
		timeoutBudget: createTimeoutBudget(
			'CODEX_APP_SERVER_ENVIRONMENT_TIMEOUT',
			'runtime_environment',
			adapterOptions.environment_timeout_ms,
		),
	})
	try {
		const [authStatus, accountRead, rateLimitRead, configRead, configRequirementsRead] =
			await Promise.all([
				client.getAuthStatus(),
				client.readAccount(),
				client.readAccountRateLimits(),
				client.readConfig(),
				client.readConfigRequirements(),
			])

		const account = isObject(accountRead.account) ? accountRead.account : null
		const config = isObject(configRead.config) ? configRead.config : {}
		const requirements = isObject(configRequirementsRead.requirements)
			? configRequirementsRead.requirements
			: null

		const modelReasoningEffort =
			typeof config.model_reasoning_effort === 'string' &&
			REASONING_EFFORTS.has(config.model_reasoning_effort as RuntimeReasoningEffort)
				? (config.model_reasoning_effort as RuntimeReasoningEffort)
				: undefined
		const serviceTier =
			typeof config.service_tier === 'string' &&
			SPEED_TIERS.has(config.service_tier as RuntimeSpeedTier)
				? (config.service_tier as RuntimeSpeedTier)
				: undefined

		return {
			auth: {
				authenticated:
					typeof authStatus.authMethod === 'string' && authStatus.authMethod.length > 0,
				...(typeof authStatus.authMethod === 'string'
					? { auth_method: authStatus.authMethod }
					: {}),
				requires_openai_auth: authStatus.requiresOpenaiAuth === true,
			},
			account: account
				? {
						status: 'available',
						...(typeof account.type === 'string' ? { account_type: account.type } : {}),
						...(typeof account.email === 'string' ? { email: account.email } : {}),
						...(typeof account.planType === 'string' ? { plan_type: account.planType } : {}),
					}
				: {
						status:
							accountRead.requiresOpenaiAuth === true || authStatus.requiresOpenaiAuth === true
								? 'missing'
								: 'unknown',
					},
			rate_limits: Array.isArray(rateLimitRead.rateLimits)
				? rateLimitRead.rateLimits
						.map((entry) => normalizeRateLimit(entry))
						.filter((entry): entry is RuntimeRateLimitSummary => entry !== null)
				: [],
			config: {
				...(typeof config.model === 'string' ? { model: config.model } : {}),
				...(typeof config.review_model === 'string' ? { review_model: config.review_model } : {}),
				...(typeof config.model_provider === 'string'
					? { model_provider: config.model_provider }
					: {}),
				...(typeof config.approval_policy === 'string'
					? { approval_policy: config.approval_policy }
					: {}),
				...(typeof config.sandbox_mode === 'string' ? { sandbox_mode: config.sandbox_mode } : {}),
				...(typeof config.profile === 'string' ? { profile: config.profile } : {}),
				...(modelReasoningEffort ? { model_reasoning_effort: modelReasoningEffort } : {}),
				...(serviceTier ? { service_tier: serviceTier } : {}),
			},
			...(requirements
				? {
						config_requirements: {
							...(Array.isArray(requirements.allowedApprovalPolicies)
								? {
										allowed_approval_policies: toStringArray(requirements.allowedApprovalPolicies),
									}
								: {}),
							...(Array.isArray(requirements.allowedSandboxModes)
								? {
										allowed_sandbox_modes: toStringArray(requirements.allowedSandboxModes),
									}
								: {}),
							...(Array.isArray(requirements.allowedWebSearchModes)
								? {
										allowed_web_search_modes: toStringArray(requirements.allowedWebSearchModes),
									}
								: {}),
							...(typeof requirements.enforceResidency === 'boolean'
								? { enforce_residency: requirements.enforceResidency }
								: {}),
							...(toJsonObject(requirements.featureRequirements)
								? {
										feature_requirements: toJsonObject(requirements.featureRequirements),
									}
								: {}),
						},
					}
				: {}),
		}
	} finally {
		await client.close().catch(() => undefined)
	}
}

export class CodexAppServerRuntimeAdapter implements RuntimeAdapter {
	private readonly liveClientsByThreadId = new Map<string, AppServerJsonRpcClient>()

	constructor(
		private readonly workingDirectory: string,
		private readonly options: CodexAppServerRuntimeAdapterOptions = {},
	) {}

	describeCapabilities(): RuntimeAdapterCapabilities {
		return {
			supports_native_resume: true,
			supports_live_comments: true,
			supports_builtin_user_chat_mcp: true,
			supports_memory_bindings: true,
			supports_model_discovery: true,
			supports_runtime_environment_introspection: true,
			supports_reasoning_effort: true,
			supports_speed_tiers: true,
			supports_personality: true,
			supports_explicit_runtime_source: false,
			supports_runtime_source_introspection: false,
		}
	}

	async listModels(request?: RuntimeModelCatalogRequest): Promise<RuntimeModelCatalogPage> {
		return await listRuntimeModelsViaAppServer(this.workingDirectory, this.options, request)
	}

	async inspectRuntimeEnvironment(): Promise<RuntimeEnvironmentInspectionResult> {
		return await inspectRuntimeEnvironmentViaAppServer(this.workingDirectory, this.options)
	}

	async inspectRuntimeSource(
		_source: RuntimeSourceSelection,
	): Promise<RuntimeSourceInspectionResult> {
		throw new Error('runtime source inspection is not supported by the current App Server adapter.')
	}

	async deliverComment(execution: unknown, text: string): Promise<void> {
		await performLiveTurnAction(this.workingDirectory, this.options, execution, 'steer', text)
	}

	async deliverUserChatResponse(
		execution: unknown,
		response: UserChatResponsePayload,
	): Promise<void> {
		const handle = normalizePendingUserInputPromptHandle(execution)
		const client = this.liveClientsByThreadId.get(handle.threadId)
		if (!client) {
			throw new Error(
				'Built-in user-chat replies require the original live Codex App Server session and cannot be resumed from a fresh adapter process in the current slice.',
			)
		}
		await withAppServerTimeout(
			client.deliverUserInputResponse(handle, response),
			createTimeoutBudget(
				'CODEX_APP_SERVER_REPLY_TIMEOUT',
				'prompt_reply',
				this.options.reply_timeout_ms,
			),
			'item/tool/requestUserInput reply',
		)
	}

	async cancelExecution(execution: unknown): Promise<void> {
		await performLiveTurnAction(this.workingDirectory, this.options, execution, 'interrupt')
	}

	async startExecution(request: RuntimeAdapterExecutionRequest): Promise<RuntimeExecutionSession> {
		return await prepareExecution(request, this.workingDirectory, this.options, {
			registerClient: (threadId, client) => {
				this.liveClientsByThreadId.set(threadId, client)
			},
			unregisterClient: (threadId) => {
				this.liveClientsByThreadId.delete(threadId)
			},
		})
	}
}
