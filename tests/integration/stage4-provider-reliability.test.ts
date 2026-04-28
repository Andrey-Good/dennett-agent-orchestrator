import { mkdtemp, rm } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import type { AgentFile } from '../../src/core/agent-file.js'
import { resumeAgentRun, runAgentFile } from '../../src/core/graph-runner.js'
import { SQLiteLocalStateStore } from '../../src/core/state/index.js'
import type {
	RuntimeAdapter,
	RuntimeAdapterCapabilities,
	RuntimeAdapterExecutionRequest,
	RuntimeEvent,
	RuntimeExecutionSession,
	RuntimeSourceInspectionResult,
	RuntimeTerminalResult,
} from '../../src/ports/runtime.js'

const storesToClose: SQLiteLocalStateStore[] = []
const tempDirsToRemove: string[] = []

const TEXT_OUTPUT = { mode: 'text' } as const
const JSON_OBJECT_OUTPUT = {
	mode: 'json',
	schema: {
		type: 'object',
		additionalProperties: true,
	},
} as const

interface Deferred<T> {
	promise: Promise<T>
	resolve(value: T): void
	reject(error: unknown): void
}

function createDeferred<T>(): Deferred<T> {
	let resolve!: (value: T) => void
	let reject!: (error: unknown) => void
	const promise = new Promise<T>((innerResolve, innerReject) => {
		resolve = innerResolve
		reject = innerReject
	})

	return {
		promise,
		resolve,
		reject,
	}
}

async function createStore(): Promise<SQLiteLocalStateStore> {
	const tempDir = await mkdtemp(path.join(os.tmpdir(), 'dennett-stage4-provider-'))
	const store = new SQLiteLocalStateStore({
		database_path: path.join(tempDir, 'local-state.sqlite'),
	})
	storesToClose.push(store)
	tempDirsToRemove.push(tempDir)
	return store
}

function nonCompletingEventStream(): AsyncIterable<RuntimeEvent> {
	return {
		async *[Symbol.asyncIterator]() {
			await new Promise<never>(() => {
				// Keep terminal_result as the deterministic winner for non-interaction tests.
			})
		},
	}
}

function promptEventStream(onClosed: () => void): AsyncIterable<RuntimeEvent> {
	return {
		async *[Symbol.asyncIterator]() {
			try {
				yield {
					kind: 'user_chat_request',
					request_handle: {
						kind: 'stage4_stub_prompt',
						prompt_id: 'stage4-approval',
					},
					payload: {
						kind: 'text',
						prompt_id: 'stage4-approval',
						text: 'Approve deterministic Stage 4 resume?',
						require_response: true,
					},
				}
			} finally {
				onClosed()
			}
		},
	}
}

function buildProviderMatrixAgent(): AgentFile {
	return {
		graph_contract_version: '1.0',
		meta: {
			id: 'agent.stage4.provider-reliability',
			name: 'Stage 4 Deterministic Provider Reliability Agent',
		},
		entry_node_id: 'prepare',
		params: {
			topic: {
				type: 'string',
				required: true,
			},
		},
		final_output: {
			mode: 'last_node_output',
		},
		nodes: [
			{
				id: 'prepare',
				kind: 'runtime_agent',
				runtime_adapter: 'codex',
				prompt: 'Prepare deterministic provider reliability JSON.',
				input: {
					parts: [{ type: 'ref', ref: 'params.topic' }],
				},
				output: JSON_OBJECT_OUTPUT,
			},
			{
				id: 'finalize',
				kind: 'runtime_agent',
				runtime_adapter: 'codex',
				prompt: 'Finalize deterministic provider reliability text.',
				input: {
					parts: [
						{ type: 'text', text: 'Prepared: ' },
						{ type: 'ref', ref: 'node.prepare.json.topic' },
					],
				},
				output: TEXT_OUTPUT,
			},
		],
		edges: [
			{
				from: 'prepare',
				to: 'finalize',
			},
		],
	}
}

function buildProviderMatrixInteractionAgent(): AgentFile {
	return {
		...buildProviderMatrixAgent(),
		interaction: {
			user_mcp: {
				enabled: true,
				server_name: 'orchestrator.user_chat',
			},
		},
		chat: {
			prefer_native_resume: true,
			store_visible_messages: true,
			store_context_window: true,
			allow_fresh_start: true,
		},
	}
}

function topicForRequest(request: RuntimeAdapterExecutionRequest): string {
	if (request.node_id === 'prepare') {
		return String(request.input_message)
	}

	const match = /^Prepared: (.*)$/.exec(String(request.input_message))
	return match?.[1] ?? String(request.input_message)
}

function successResultForRequest(
	request: RuntimeAdapterExecutionRequest,
	requestIndex: number,
): RuntimeTerminalResult {
	const topic = topicForRequest(request)
	if (request.output.mode === 'json') {
		return {
			outcome: 'success',
			output: request.output,
			output_json: {
				topic,
				request_index: requestIndex,
			},
			native_session_handle: {
				session: `${topic}-${request.node_id}-success`,
			},
		}
	}

	return {
		outcome: 'success',
		output: request.output,
		output_text: `final:${request.input_message}`,
		native_session_handle: {
			session: `${topic}-${request.node_id}-success`,
		},
	}
}

function failureResult(
	code: string,
	message: string,
	outcome: 'runtime_error' | 'interrupted' = 'runtime_error',
): RuntimeTerminalResult {
	return {
		outcome,
		error: {
			code,
			message,
		},
		native_session_handle: {
			session: `${code.toLowerCase()}-session`,
		},
	}
}

function createStage4MatrixAdapter(options?: { volumeBarrierTopics?: string[] }) {
	const requests: RuntimeAdapterExecutionRequest[] = []
	const outcomes = new Map<RuntimeTerminalResult['outcome'], number>()
	const attemptsByNodeAndTopic = new Map<string, number>()
	const volumeTopics = new Set(options?.volumeBarrierTopics ?? [])
	const pendingVolumePrepareResults: {
		deferred: Deferred<RuntimeTerminalResult>
		request: RuntimeAdapterExecutionRequest
	}[] = []
	const expectedVolumePrepareCount = volumeTopics.size
	let activeExecutions = 0
	let maxConcurrentExecutions = 0

	function recordOutcome(outcome: RuntimeTerminalResult['outcome']): void {
		outcomes.set(outcome, (outcomes.get(outcome) ?? 0) + 1)
	}

	function incrementActive(): void {
		activeExecutions += 1
		maxConcurrentExecutions = Math.max(maxConcurrentExecutions, activeExecutions)
	}

	function decrementActive(): void {
		activeExecutions -= 1
	}

	function attemptNumber(request: RuntimeAdapterExecutionRequest): number {
		const key = `${topicForRequest(request)}:${request.node_id}`
		const next = (attemptsByNodeAndTopic.get(key) ?? 0) + 1
		attemptsByNodeAndTopic.set(key, next)
		return next
	}

	function makeTerminalSession(
		result: RuntimeTerminalResult | Promise<RuntimeTerminalResult>,
	): RuntimeExecutionSession {
		const terminal_result = Promise.resolve(result).then((terminal) => {
			recordOutcome(terminal.outcome)
			decrementActive()
			return terminal
		})

		return {
			runtime_handle: {
				request_index: requests.length,
			},
			native_session_handle: null,
			terminal_result,
			events: nonCompletingEventStream(),
		}
	}

	function makePromptSession(): RuntimeExecutionSession {
		return {
			runtime_handle: {
				request_index: requests.length,
			},
			native_session_handle: {
				session: 'stage4-approval-session',
			},
			terminal_result: new Promise<RuntimeTerminalResult>(() => undefined),
			events: promptEventStream(decrementActive),
		}
	}

	const capabilities: RuntimeAdapterCapabilities = {
		supports_native_resume: true,
		supports_live_comments: false,
		supports_builtin_user_chat_mcp: true,
		supports_memory_bindings: false,
		supports_model_discovery: false,
		supports_runtime_environment_introspection: false,
		supports_reasoning_effort: false,
		supports_speed_tiers: false,
		supports_personality: false,
		supports_explicit_runtime_source: false,
		supports_runtime_source_introspection: false,
	}

	const adapter: RuntimeAdapter = {
		describeCapabilities() {
			return capabilities
		},
		async startExecution(request) {
			requests.push(request)
			incrementActive()
			const topic = topicForRequest(request)
			const attempt = attemptNumber(request)

			if (topic === 'rate-limit-once' && request.node_id === 'prepare' && attempt === 1) {
				return makeTerminalSession(
					failureResult('PROVIDER_RATE_LIMIT', 'Deterministic provider throttling failure.'),
				)
			}

			if (topic === 'transient-once' && request.node_id === 'prepare' && attempt === 1) {
				return makeTerminalSession(
					failureResult('PROVIDER_TRANSIENT_FAILURE', 'Deterministic transient provider failure.'),
				)
			}

			if (topic === 'interrupted-once' && request.node_id === 'prepare' && attempt === 1) {
				return makeTerminalSession(
					failureResult(
						'PROVIDER_STREAM_INTERRUPTED',
						'Deterministic stream interruption.',
						'interrupted',
					),
				)
			}

			if (topic === 'finalize-failure-once' && request.node_id === 'finalize' && attempt === 1) {
				return makeTerminalSession(
					failureResult(
						'PROVIDER_TRANSIENT_FINALIZE',
						'Deterministic finalizer failure after prepare succeeded.',
					),
				)
			}

			if (
				topic === 'needs-approval' &&
				request.node_id === 'prepare' &&
				request.interaction.user_chat_reply === undefined
			) {
				return makePromptSession()
			}

			if (volumeTopics.has(topic) && request.node_id === 'prepare') {
				const deferred = createDeferred<RuntimeTerminalResult>()
				pendingVolumePrepareResults.push({
					deferred,
					request,
				})
				if (pendingVolumePrepareResults.length === expectedVolumePrepareCount) {
					pendingVolumePrepareResults.forEach((pending, index) => {
						pending.deferred.resolve(successResultForRequest(pending.request, index + 1))
					})
				}
				return makeTerminalSession(deferred.promise)
			}

			return makeTerminalSession(successResultForRequest(request, requests.length))
		},
		async listModels() {
			throw new Error('not used by deterministic Stage 4 matrix')
		},
		async inspectRuntimeEnvironment() {
			throw new Error('not used by deterministic Stage 4 matrix')
		},
		async inspectRuntimeSource(source): Promise<RuntimeSourceInspectionResult> {
			return {
				source_id: source.id,
				availability: 'unknown',
				limit_status: 'unknown',
			}
		},
		async deliverComment() {
			throw new Error('not used by deterministic Stage 4 matrix')
		},
		async deliverUserChatResponse() {
			throw new Error('not used by deterministic Stage 4 matrix')
		},
		async cancelExecution() {
			throw new Error('not used by deterministic Stage 4 matrix')
		},
	}

	return {
		adapter,
		metrics: {
			requests,
			outcomes,
			attemptsByNodeAndTopic,
			get activeExecutions() {
				return activeExecutions
			},
			get maxConcurrentExecutions() {
				return maxConcurrentExecutions
			},
		},
	}
}

afterEach(async () => {
	while (storesToClose.length > 0) {
		storesToClose.pop()?.close()
	}

	while (tempDirsToRemove.length > 0) {
		const tempDir = tempDirsToRemove.pop()
		if (tempDir) {
			await rm(tempDir, { recursive: true, force: true })
		}
	}
})

describe('Stage 4 deterministic provider reliability matrix', () => {
	it('keeps throttling and transient runtime failures visible, resumable, and final-output safe', async () => {
		const store = await createStore()
		const agentFile = buildProviderMatrixAgent()
		const { adapter, metrics } = createStage4MatrixAdapter()

		const rateLimitFailure = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'rate-limit-once' },
			{
				state_store: store,
				resolved_revision_id: 'rev-stage4-provider',
				run_id: 'stage4-rate-limit',
			},
		)

		expect(rateLimitFailure).toMatchObject({
			status: 'failure',
			run_status: 'failed',
			code: 'PROVIDER_RATE_LIMIT',
			resume_available: true,
		})
		expect(store.getPersistedRunSnapshot('stage4-rate-limit')).toMatchObject({
			run: {
				status: 'failed',
			},
			attempts: [
				expect.objectContaining({
					node_id: 'prepare',
					state: 'committed_terminal',
					outcome: 'runtime_error',
					committed_output_id: null,
				}),
			],
			latest_committed_outputs: [],
			resume: {
				local_resume_available: true,
				last_durable_boundary_kind: 'node_attempt_terminal',
			},
		})

		const resumedRateLimit = await resumeAgentRun(agentFile, adapter, 'stage4-rate-limit', {
			state_store: store,
			resolved_revision_id: 'rev-stage4-provider',
		})

		expect(resumedRateLimit).toMatchObject({
			status: 'success',
			run_status: 'completed',
			final_output: 'final:Prepared: rate-limit-once',
			final_output_mode: 'text',
		})

		const transientFailure = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'transient-once' },
			{
				state_store: store,
				resolved_revision_id: 'rev-stage4-provider',
				run_id: 'stage4-transient',
			},
		)

		expect(transientFailure).toMatchObject({
			status: 'failure',
			run_status: 'failed',
			code: 'PROVIDER_TRANSIENT_FAILURE',
			resume_available: true,
		})

		const resumedTransient = await resumeAgentRun(agentFile, adapter, 'stage4-transient', {
			state_store: store,
			resolved_revision_id: 'rev-stage4-provider',
		})

		expect(resumedTransient).toMatchObject({
			status: 'success',
			run_status: 'completed',
			final_output: 'final:Prepared: transient-once',
			final_output_mode: 'text',
		})

		const transientSnapshot = store.getPersistedRunSnapshot('stage4-transient')
		expect(transientSnapshot?.attempts.map((attempt) => attempt.outcome)).toEqual([
			'runtime_error',
			'success',
			'success',
		])
		expect(
			transientSnapshot?.latest_committed_outputs.filter((output) => output.node_id === 'finalize'),
		).toHaveLength(1)
		expect(metrics.attemptsByNodeAndTopic.get('transient-once:finalize')).toBe(1)
		expect(metrics.outcomes.get('runtime_error')).toBe(2)
		expect(metrics.outcomes.get('success')).toBe(4)
		expect(metrics.activeExecutions).toBe(0)
	})

	it('treats interruption and waiting-for-user boundaries as resumable, not cancellation', async () => {
		const store = await createStore()
		const agentFile = buildProviderMatrixInteractionAgent()
		const { adapter, metrics } = createStage4MatrixAdapter()

		const interrupted = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'interrupted-once' },
			{
				state_store: store,
				resolved_revision_id: 'rev-stage4-interaction',
				run_id: 'stage4-interrupted',
			},
		)

		expect(interrupted).toMatchObject({
			status: 'failure',
			run_status: 'interrupted',
			code: 'PROVIDER_STREAM_INTERRUPTED',
			resume_available: true,
		})
		expect(store.getPersistedRunSnapshot('stage4-interrupted')?.attempts).toEqual([
			expect.objectContaining({
				node_id: 'prepare',
				state: 'committed_terminal',
				outcome: 'interrupted',
			}),
		])

		const resumedInterrupted = await resumeAgentRun(agentFile, adapter, 'stage4-interrupted', {
			state_store: store,
			resolved_revision_id: 'rev-stage4-interaction',
		})

		expect(resumedInterrupted).toMatchObject({
			status: 'success',
			run_status: 'completed',
			final_output: 'final:Prepared: interrupted-once',
		})

		const waiting = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'needs-approval' },
			{
				state_store: store,
				resolved_revision_id: 'rev-stage4-interaction',
				run_id: 'stage4-waiting',
			},
		)

		expect(waiting).toMatchObject({
			status: 'waiting_for_user',
			run_status: 'waiting_for_user',
			code: 'RUN_WAITING_FOR_USER',
			resume_available: true,
		})
		expect(store.getPersistedRunSnapshot('stage4-waiting')).toMatchObject({
			run: {
				status: 'waiting_for_user',
			},
			attempts: [
				expect.objectContaining({
					node_id: 'prepare',
					state: 'blocked_wait',
					outcome: null,
					blocked_on_user_prompt: true,
				}),
			],
			resume: {
				local_resume_available: true,
				native_resume_available: true,
				last_durable_boundary_kind: 'blocked_prompt_wait',
				pending_prompt: expect.objectContaining({
					prompt_id: 'stage4-approval',
				}),
			},
		})

		const approvalReply = {
			kind: 'text',
			prompt_id: 'stage4-approval',
			text: 'Approved',
		} as const
		const recordedReply = store.recordUserPromptReply({
			run_id: 'stage4-waiting',
			prompt_id: 'stage4-approval',
			payload: approvalReply,
		})
		expect(recordedReply).toMatchObject({
			accepted: true,
			reply: {
				prompt_id: 'stage4-approval',
				payload: approvalReply,
				delivery_status: 'recorded',
			},
		})
		expect(
			store.getPersistedRunSnapshot('stage4-waiting')?.resume.pending_prompt?.reply,
		).toMatchObject({
			prompt_id: 'stage4-approval',
			payload: approvalReply,
			delivery_status: 'recorded',
		})

		const resumedWaiting = await resumeAgentRun(agentFile, adapter, 'stage4-waiting', {
			state_store: store,
			resolved_revision_id: 'rev-stage4-interaction',
		})

		expect(resumedWaiting).toMatchObject({
			status: 'success',
			run_status: 'completed',
			final_output: 'final:Prepared: needs-approval',
		})
		expect(
			store
				.getPersistedRunSnapshot('stage4-waiting')
				?.attempts.some((attempt) => attempt.outcome === 'cancelled'),
		).toBe(false)
		expect(
			metrics.requests.find(
				(request) =>
					topicForRequest(request) === 'needs-approval' &&
					request.node_id === 'prepare' &&
					request.interaction.user_chat_reply !== undefined,
			),
		).toMatchObject({
			resume: {
				mode: 'native_resume',
				native_session_handle: {
					session: 'stage4-approval-session',
				},
			},
			interaction: {
				user_chat_reply: {
					kind: 'text',
					prompt_id: 'stage4-approval',
					text: 'Approved',
				},
			},
		})
		expect(metrics.outcomes.get('interrupted')).toBe(1)
		expect(metrics.activeExecutions).toBe(0)
	})

	it('drains all active executions for bounded concurrent volume without latency gates', async () => {
		const store = await createStore()
		const agentFile = buildProviderMatrixAgent()
		const topics = Array.from({ length: 8 }, (_, index) => `volume-${index}`)
		const { adapter, metrics } = createStage4MatrixAdapter({
			volumeBarrierTopics: topics,
		})

		const results = await Promise.all(
			topics.map((topic) =>
				runAgentFile(
					agentFile,
					adapter,
					{ topic },
					{
						state_store: store,
						resolved_revision_id: 'rev-stage4-volume',
						run_id: `stage4-${topic}`,
					},
				),
			),
		)

		expect(results).toEqual(
			topics.map((topic) =>
				expect.objectContaining({
					status: 'success',
					run_status: 'completed',
					final_output: `final:Prepared: ${topic}`,
				}),
			),
		)
		expect(metrics.requests).toHaveLength(topics.length * 2)
		expect(metrics.outcomes.get('success')).toBe(topics.length * 2)
		expect(metrics.maxConcurrentExecutions).toBe(topics.length)
		expect(metrics.activeExecutions).toBe(0)

		for (const topic of topics) {
			expect(store.getPersistedRunSnapshot(`stage4-${topic}`)).toMatchObject({
				run: {
					status: 'completed',
				},
				attempts: [
					expect.objectContaining({
						node_id: 'prepare',
						outcome: 'success',
					}),
					expect.objectContaining({
						node_id: 'finalize',
						outcome: 'success',
					}),
				],
				latest_committed_outputs: expect.arrayContaining([
					expect.objectContaining({
						node_id: 'prepare',
					}),
					expect.objectContaining({
						node_id: 'finalize',
					}),
				]),
				resume: {
					local_resume_available: false,
				},
			})
		}
	})

	it('does not expose final output until every required node succeeds', async () => {
		const store = await createStore()
		const agentFile = buildProviderMatrixAgent()
		const { adapter, metrics } = createStage4MatrixAdapter()

		const failed = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'finalize-failure-once' },
			{
				state_store: store,
				resolved_revision_id: 'rev-stage4-final-output',
				run_id: 'stage4-final-output',
			},
		)

		expect(failed).toMatchObject({
			status: 'failure',
			run_status: 'failed',
			code: 'PROVIDER_TRANSIENT_FINALIZE',
			resume_available: true,
		})
		expect('final_output' in failed).toBe(false)
		expect(store.getPersistedRunSnapshot('stage4-final-output')).toMatchObject({
			run: {
				status: 'failed',
			},
			attempts: [
				expect.objectContaining({
					node_id: 'prepare',
					outcome: 'success',
				}),
				expect.objectContaining({
					node_id: 'finalize',
					outcome: 'runtime_error',
					committed_output_id: null,
				}),
			],
			latest_committed_outputs: [
				expect.objectContaining({
					node_id: 'prepare',
				}),
			],
			resume: {
				local_resume_available: true,
			},
		})

		const resumed = await resumeAgentRun(agentFile, adapter, 'stage4-final-output', {
			state_store: store,
			resolved_revision_id: 'rev-stage4-final-output',
		})

		expect(resumed).toMatchObject({
			status: 'success',
			run_status: 'completed',
			final_output: 'final:Prepared: finalize-failure-once',
			final_output_mode: 'text',
		})
		const finalSnapshot = store.getPersistedRunSnapshot('stage4-final-output')
		expect(finalSnapshot?.attempts.map((attempt) => attempt.outcome)).toEqual([
			'success',
			'runtime_error',
			'success',
		])
		expect(
			finalSnapshot?.latest_committed_outputs.filter((output) => output.node_id === 'finalize'),
		).toHaveLength(1)
		expect(metrics.attemptsByNodeAndTopic.get('finalize-failure-once:prepare')).toBe(1)
		expect(metrics.attemptsByNodeAndTopic.get('finalize-failure-once:finalize')).toBe(2)
		expect(metrics.activeExecutions).toBe(0)
	})
})
