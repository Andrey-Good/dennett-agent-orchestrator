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

async function createStore(): Promise<SQLiteLocalStateStore> {
	const tempDir = await mkdtemp(path.join(os.tmpdir(), 'dennett-phase19-stress-'))
	const store = new SQLiteLocalStateStore({
		database_path: path.join(tempDir, 'local-state.sqlite'),
	})
	storesToClose.push(store)
	tempDirsToRemove.push(tempDir)
	return store
}

function nonCompletingEventStream() {
	return {
		async *[Symbol.asyncIterator]() {
			await new Promise<never>(() => {
				// Keep the prompt side of Promise.race pending until terminal_result wins.
			})
		},
	}
}

function createStartBarrier(expectedStarts: number) {
	let started = 0
	let release: (() => void) | undefined
	const allStarted = new Promise<void>((resolve) => {
		release = resolve
	})

	return {
		markStarted(): void {
			started += 1
			if (started === expectedStarts) {
				release?.()
			}
		},
		waitForAllStarts(): Promise<void> {
			return allStarted
		},
		get started() {
			return started
		},
	}
}

function buildStressAgentFile(): AgentFile {
	return {
		graph_contract_version: '1.0',
		meta: {
			id: 'agent.phase19.stress',
			name: 'Phase 19 Local Stress Agent',
		},
		entry_node_id: 'draft',
		params: {
			topic: {
				type: 'string',
				required: true,
			},
		},
		initial_vars: {
			release_phase: 'phase19',
		},
		final_output: {
			mode: 'last_node_output',
		},
		nodes: [
			{
				id: 'draft',
				kind: 'runtime_agent',
				runtime_adapter: 'codex',
				prompt: 'Return a release stress JSON summary.',
				input: {
					parts: [{ type: 'ref', ref: 'params.topic' }],
				},
				output: JSON_OBJECT_OUTPUT,
			},
			{
				id: 'finalize',
				kind: 'runtime_agent',
				runtime_adapter: 'codex',
				prompt: 'Return the final release stress text.',
				input: {
					parts: [
						{ type: 'text', text: 'Summary: ' },
						{ type: 'ref', ref: 'node.draft.json.summary' },
						{ type: 'text', text: '; phase: ' },
						{ type: 'ref', ref: 'vars.release_phase' },
					],
				},
				output: TEXT_OUTPUT,
			},
		],
		edges: [
			{
				from: 'draft',
				to: 'finalize',
			},
		],
	}
}

function createStressRuntimeAdapter(expectedInitialDraftStarts: number) {
	const requests: RuntimeAdapterExecutionRequest[] = []
	const failedProviderTopics = new Set<string>()
	const interruptedTopics = new Set<string>()
	const outcomes = new Map<string, number>()
	const startedByNode = new Map<string, number>()
	const completedByNode = new Map<string, number>()
	const firstWaveDraftStartBarrier = createStartBarrier(expectedInitialDraftStarts)
	let gatedDraftStarts = 0
	let activeExecutions = 0
	let maxConcurrentExecutions = 0

	function recordNodeStart(nodeId: string): void {
		startedByNode.set(nodeId, (startedByNode.get(nodeId) ?? 0) + 1)
	}

	function recordNodeCompletion(nodeId: string): void {
		completedByNode.set(nodeId, (completedByNode.get(nodeId) ?? 0) + 1)
	}

	function recordOutcome(outcome: RuntimeTerminalResult['outcome']): void {
		outcomes.set(outcome, (outcomes.get(outcome) ?? 0) + 1)
	}

	function terminalStartGate(request: RuntimeAdapterExecutionRequest): Promise<void> {
		if (request.node_id !== 'draft' || gatedDraftStarts >= expectedInitialDraftStarts) {
			return Promise.resolve()
		}

		gatedDraftStarts += 1
		firstWaveDraftStartBarrier.markStarted()
		return firstWaveDraftStartBarrier.waitForAllStarts()
	}

	function resolveTopic(request: RuntimeAdapterExecutionRequest): string {
		if (request.node_id === 'draft' && typeof request.input_message === 'string') {
			return request.input_message
		}
		if (typeof request.input_message === 'string') {
			const match = /^Summary: (.*); phase: phase19$/.exec(request.input_message)
			return match?.[1] ?? request.input_message
		}
		return JSON.stringify(request.input_message)
	}

	function buildTerminalResult(request: RuntimeAdapterExecutionRequest): RuntimeTerminalResult {
		const topic = resolveTopic(request)
		if (
			request.node_id === 'draft' &&
			topic === 'provider-failure-once' &&
			!failedProviderTopics.has(topic)
		) {
			failedProviderTopics.add(topic)
			return {
				outcome: 'runtime_error',
				error: {
					code: 'PROVIDER_RATE_LIMIT',
					message: 'Synthetic provider throttling for Phase 19 stress proof.',
				},
			}
		}

		if (
			request.node_id === 'draft' &&
			topic === 'interruption-once' &&
			!interruptedTopics.has(topic)
		) {
			interruptedTopics.add(topic)
			return {
				outcome: 'interrupted',
				error: {
					code: 'PROVIDER_STREAM_INTERRUPTED',
					message: 'Synthetic runtime stream interruption for Phase 19 stress proof.',
				},
			}
		}

		if (request.output.mode === 'json') {
			return {
				outcome: 'success',
				output: request.output,
				output_json: {
					summary: topic,
					request_index: requests.length,
				},
			}
		}

		return {
			outcome: 'success',
			output: request.output,
			output_text: `${request.input_message}`,
		}
	}

	const capabilities: RuntimeAdapterCapabilities = {
		supports_native_resume: false,
		supports_live_comments: false,
		supports_builtin_user_chat_mcp: false,
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
			recordNodeStart(request.node_id)
			activeExecutions += 1
			maxConcurrentExecutions = Math.max(maxConcurrentExecutions, activeExecutions)
			const startGate = terminalStartGate(request)

			const terminal_result = (async () => {
				try {
					await startGate
					const result = buildTerminalResult(request)
					recordOutcome(result.outcome)
					return result
				} finally {
					recordNodeCompletion(request.node_id)
					activeExecutions -= 1
				}
			})()

			return {
				runtime_handle: {
					request_index: requests.length,
				},
				native_session_handle: null,
				terminal_result,
				events: nonCompletingEventStream(),
			} satisfies RuntimeExecutionSession
		},
		async listModels() {
			throw new Error('not used in Phase 19 stress proof')
		},
		async inspectRuntimeEnvironment() {
			throw new Error('not used in Phase 19 stress proof')
		},
		async inspectRuntimeSource(source): Promise<RuntimeSourceInspectionResult> {
			return {
				source_id: source.id,
				availability: 'unknown',
				limit_status: 'unknown',
			}
		},
		async deliverComment() {
			throw new Error('not used in Phase 19 stress proof')
		},
		async deliverUserChatResponse() {
			throw new Error('not used in Phase 19 stress proof')
		},
		async cancelExecution() {
			throw new Error('not used in Phase 19 stress proof')
		},
	}

	return {
		adapter,
		metrics: {
			requests,
			outcomes,
			startedByNode,
			completedByNode,
			get firstWaveDraftStarts() {
				return firstWaveDraftStartBarrier.started
			},
			get maxConcurrentExecutions() {
				return maxConcurrentExecutions
			},
			get activeExecutions() {
				return activeExecutions
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

describe('Phase 19 stress and regression proof', () => {
	it('keeps shared storage consistent under concurrent runs, provider failure, interruption, and resume', async () => {
		const store = await createStore()
		const agentFile = buildStressAgentFile()
		const successRunIds = Array.from({ length: 10 }, (_, index) => `phase19-stress-ok-${index}`)
		const failureRunId = 'phase19-stress-provider-failure'
		const interruptedRunId = 'phase19-stress-interrupted'
		const runSpecs = [
			...successRunIds.map((runId, index) => ({
				runId,
				topic: `concurrent-${index}`,
			})),
			{
				runId: failureRunId,
				topic: 'provider-failure-once',
			},
			{
				runId: interruptedRunId,
				topic: 'interruption-once',
			},
		]
		const { adapter, metrics } = createStressRuntimeAdapter(runSpecs.length)

		const initialResults = await Promise.all(
			runSpecs.map((spec) =>
				runAgentFile(
					agentFile,
					adapter,
					{ topic: spec.topic },
					{
						state_store: store,
						resolved_revision_id: 'rev-phase19-stress',
						run_id: spec.runId,
					},
				),
			),
		)

		const successfulInitialResults = initialResults.filter((result) => result.status === 'success')
		const providerFailure = initialResults.find((result) => result.run_id === failureRunId)
		const interrupted = initialResults.find((result) => result.run_id === interruptedRunId)

		expect(successfulInitialResults).toHaveLength(10)
		expect(providerFailure).toMatchObject({
			status: 'failure',
			run_status: 'failed',
			code: 'PROVIDER_RATE_LIMIT',
			resume_available: true,
		})
		expect(interrupted).toMatchObject({
			status: 'failure',
			run_status: 'interrupted',
			code: 'PROVIDER_STREAM_INTERRUPTED',
			resume_available: true,
		})
		expect(metrics.firstWaveDraftStarts).toBe(runSpecs.length)
		expect(metrics.maxConcurrentExecutions).toBe(runSpecs.length)
		expect(metrics.activeExecutions).toBe(0)
		expect(metrics.startedByNode.get('draft')).toBe(runSpecs.length)
		expect(metrics.completedByNode.get('draft')).toBe(runSpecs.length)
		expect(metrics.startedByNode.get('finalize')).toBe(successRunIds.length)
		expect(metrics.completedByNode.get('finalize')).toBe(successRunIds.length)

		for (const runId of successRunIds) {
			const snapshot = store.getPersistedRunSnapshot(runId)
			expect(snapshot?.run.status).toBe('completed')
			expect(snapshot?.attempts).toHaveLength(2)
			expect(snapshot?.latest_committed_outputs).toHaveLength(2)
			expect(snapshot?.resume.local_resume_available).toBe(false)
		}

		expect(store.getPersistedRunSnapshot(failureRunId)).toMatchObject({
			run: {
				status: 'failed',
			},
			resume: {
				local_resume_available: true,
			},
		})
		expect(store.getPersistedRunSnapshot(interruptedRunId)).toMatchObject({
			run: {
				status: 'interrupted',
			},
			resume: {
				local_resume_available: true,
			},
		})

		const resumedResults = await Promise.all([
			resumeAgentRun(agentFile, adapter, failureRunId, {
				state_store: store,
				resolved_revision_id: 'rev-phase19-stress',
			}),
			resumeAgentRun(agentFile, adapter, interruptedRunId, {
				state_store: store,
				resolved_revision_id: 'rev-phase19-stress',
			}),
		])

		expect(resumedResults).toEqual([
			expect.objectContaining({
				status: 'success',
				run_id: failureRunId,
				run_status: 'completed',
				final_output: 'Summary: provider-failure-once; phase: phase19',
			}),
			expect.objectContaining({
				status: 'success',
				run_id: interruptedRunId,
				run_status: 'completed',
				final_output: 'Summary: interruption-once; phase: phase19',
			}),
		])
		expect(metrics.activeExecutions).toBe(0)
		expect(metrics.startedByNode.get('draft')).toBe(runSpecs.length + 2)
		expect(metrics.completedByNode.get('draft')).toBe(runSpecs.length + 2)
		expect(metrics.startedByNode.get('finalize')).toBe(successRunIds.length + 2)
		expect(metrics.completedByNode.get('finalize')).toBe(successRunIds.length + 2)

		for (const runId of [failureRunId, interruptedRunId]) {
			const snapshot = store.getPersistedRunSnapshot(runId)
			expect(snapshot?.run.status).toBe('completed')
			expect(snapshot?.attempts).toHaveLength(3)
			expect(snapshot?.attempts.map((attempt) => attempt.state)).toEqual([
				'committed_terminal',
				'committed_terminal',
				'committed_terminal',
			])
			expect(snapshot?.attempts[0]?.outcome).not.toBe('success')
			expect(snapshot?.attempts.slice(1).map((attempt) => attempt.outcome)).toEqual([
				'success',
				'success',
			])
			expect(snapshot?.latest_committed_outputs).toHaveLength(2)
			expect(snapshot?.resume.local_resume_available).toBe(false)
		}

		expect(metrics.requests).toHaveLength(26)
		expect(metrics.outcomes.get('success')).toBe(24)
		expect(metrics.outcomes.get('runtime_error')).toBe(1)
		expect(metrics.outcomes.get('interrupted')).toBe(1)
	})
})
