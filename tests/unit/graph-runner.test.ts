import { createHash } from 'node:crypto'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { AgentFile, MemoryBinding, OutputContract } from '../../src/core/agent-file.js'
import { AgentLifecycleService } from '../../src/core/agent-lifecycle.js'
import { AppError } from '../../src/core/errors.js'
import { resumeAgentRun, runAgentFile } from '../../src/core/graph-runner.js'
import type { JsonValue } from '../../src/core/json.js'
import { MemoryService } from '../../src/core/memory-service.js'
import { SQLiteLocalStateStore } from '../../src/core/state/index.js'
import {
	buildOptionReplyPayload,
	resolveCommentExecutionHandle,
	resolveReplyExecutionHandle,
} from '../../src/interfaces/cli.js'
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
const STRICT_JSON_OBJECT_OUTPUT: Extract<OutputContract, { mode: 'json' }> = {
	mode: 'json',
	schema: {
		type: 'object',
		properties: {
			summary: {
				type: 'string',
			},
			count: {
				type: 'number',
			},
		},
		required: ['summary', 'count'],
		additionalProperties: false,
	},
}

type StubExecutionDescriptor =
	| RuntimeTerminalResult
	| {
			runtime_handle?: JsonValue | null
			native_session_handle?: JsonValue | null
			terminal_result: RuntimeTerminalResult | Promise<RuntimeTerminalResult>
			events?: AsyncIterable<RuntimeEvent>
	  }

function emptyEventStream(): AsyncIterable<RuntimeEvent> {
	return {
		async *[Symbol.asyncIterator]() {
			// Intentionally empty.
		},
	}
}

function singleEventStream(event: RuntimeEvent): AsyncIterable<RuntimeEvent> {
	return {
		async *[Symbol.asyncIterator]() {
			yield event
		},
	}
}

async function createStore(): Promise<SQLiteLocalStateStore> {
	const tempDir = await mkdtemp(path.join(os.tmpdir(), 'dennett-phase6-runner-'))
	const store = new SQLiteLocalStateStore({
		database_path: path.join(tempDir, 'local-state.sqlite'),
	})
	storesToClose.push(store)
	tempDirsToRemove.push(tempDir)
	return store
}

function buildAgentFile(nodeAOutput: OutputContract = JSON_OBJECT_OUTPUT): AgentFile {
	return {
		graph_contract_version: '1.0',
		meta: {
			id: 'agent.phase6.runner',
			name: 'Phase 6 Runner Test Agent',
		},
		entry_node_id: 'node-a',
		initial_vars: {
			seed: 'stable',
		},
		final_output: {
			mode: 'last_node_output',
		},
		nodes: [
			{
				id: 'node-a',
				kind: 'runtime_agent',
				runtime_adapter: 'codex',
				prompt: 'Return summary json.',
				input: {
					parts: [
						{ type: 'text', text: 'Topic: ' },
						{ type: 'ref', ref: 'params.topic' },
					],
				},
				output: nodeAOutput,
			},
			{
				id: 'node-b',
				kind: 'runtime_agent',
				runtime_adapter: 'codex',
				prompt: 'Return final text.',
				input: {
					parts: [
						{ type: 'text', text: 'Summary: ' },
						{ type: 'ref', ref: 'node.node-a.json.summary' },
						{ type: 'text', text: '; Count: ' },
						{ type: 'ref', ref: 'vars.count' },
					],
				},
				output: TEXT_OUTPUT,
			},
		],
		edges: [
			{
				from: 'node-a',
				to: 'node-b',
			},
		],
	}
}

function buildRuntimeMemoryBinding(overrides: Partial<MemoryBinding> = {}): MemoryBinding {
	return {
		id: 'agent-memory',
		kind: 'runtime_memory',
		codex_ref: 'primary_memory',
		scope: 'agent',
		config: {
			intent: {
				summary: 'Memory available to runtime nodes.',
			},
			required_capabilities: ['read', 'write', 'user_scoped'],
		},
		...overrides,
	}
}

function stableStringifyForExpectedHash(value: JsonValue): string {
	if (value === null || typeof value !== 'object') {
		return JSON.stringify(value)
	}
	if (Array.isArray(value)) {
		return `[${value.map((item) => stableStringifyForExpectedHash(item)).join(',')}]`
	}
	const entries = Object.entries(value).sort(([left], [right]) => left.localeCompare(right))
	return `{${entries
		.map(
			([key, entryValue]) => `${JSON.stringify(key)}:${stableStringifyForExpectedHash(entryValue)}`,
		)
		.join(',')}}`
}

function expectedOutputHash(output: JsonValue): string {
	return `sha256:${createHash('sha256').update(stableStringifyForExpectedHash(output)).digest('hex')}`
}

function createStubAdapter(
	results: StubExecutionDescriptor[],
	capabilityOverrides: Partial<RuntimeAdapterCapabilities> = {},
	inspectResults: Record<string, RuntimeSourceInspectionResult> = {},
) {
	const requests: RuntimeAdapterExecutionRequest[] = []
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
		...capabilityOverrides,
	}
	const adapter: RuntimeAdapter = {
		describeCapabilities() {
			return capabilities
		},
		async startExecution(request) {
			requests.push(request)
			const next = results.shift()
			if (!next) {
				throw new Error('Stub adapter has no remaining terminal result.')
			}
			const executionSession: RuntimeExecutionSession =
				'terminal_result' in next
					? {
							runtime_handle: next.runtime_handle ?? null,
							native_session_handle: next.native_session_handle ?? null,
							terminal_result: Promise.resolve(next.terminal_result),
							events: next.events ?? emptyEventStream(),
						}
					: {
							runtime_handle: null,
							native_session_handle: null,
							terminal_result: Promise.resolve(next),
							events: emptyEventStream(),
						}
			return executionSession
		},
		async listModels() {
			throw new Error('not used in tests')
		},
		async inspectRuntimeEnvironment() {
			throw new Error('not used in tests')
		},
		async inspectRuntimeSource(source) {
			return (
				inspectResults[source.id] ?? {
					source_id: source.id,
					availability: 'unknown',
					limit_status: 'unknown',
				}
			)
		},
		async deliverComment() {
			throw new Error('not used in tests')
		},
		async deliverUserChatResponse() {
			throw new Error('not used in tests')
		},
		async cancelExecution() {
			throw new Error('not used in tests')
		},
	}

	return {
		adapter,
		requests,
	}
}

async function writeAgentFile(
	tempDir: string,
	fileName: string,
	agentFile: AgentFile,
): Promise<string> {
	const filePath = path.join(tempDir, fileName)
	await writeFile(filePath, `${JSON.stringify(agentFile, null, 2)}\n`, 'utf8')
	return filePath
}

function buildOrchestratorEntryAgentFile(
	childAgentRef: string,
	output: OutputContract = TEXT_OUTPUT,
): AgentFile {
	return {
		graph_contract_version: '1.0',
		meta: {
			id: 'agent.phase9.parent',
			name: 'Phase 9 Parent Agent',
		},
		entry_node_id: 'delegate',
		final_output: {
			mode: 'last_node_output',
		},
		nodes: [
			{
				id: 'delegate',
				kind: 'orchestrator_agent',
				agent_ref: childAgentRef,
				input: {
					parts: [
						{ type: 'text', text: 'Review request: ' },
						{ type: 'ref', ref: 'params.topic' },
					],
				},
				output,
			},
		],
	}
}

afterEach(async () => {
	vi.restoreAllMocks()

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

describe('graph-runner durable execution', () => {
	it('commits successful progression and derives the final response from the last successful node', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		const { adapter, requests } = createStubAdapter([
			{
				outcome: 'success',
				output: JSON_OBJECT_OUTPUT,
				output_json: {
					summary: 'phase-6',
					count: 2,
				},
			},
			{
				outcome: 'success',
				output: TEXT_OUTPUT,
				output_text: 'done',
			},
		])

		const result = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'execution' },
			{
				state_store: store,
				resolved_revision_id: 'rev-success',
				run_id: 'run-success',
			},
		)

		expect(result).toEqual({
			status: 'success',
			run_id: 'run-success',
			run_status: 'completed',
			final_output: 'done',
			final_output_mode: 'text',
			node_outputs: expect.any(Map),
		})
		expect(requests.map((request) => request.input_message)).toEqual([
			'Topic: execution',
			'Summary: phase-6; Count: 2',
		])

		const snapshot = store.getPersistedRunSnapshot('run-success')
		expect(snapshot?.run.status).toBe('completed')
		expect(snapshot?.current_vars).toEqual({
			seed: 'stable',
			summary: 'phase-6',
			count: 2,
		})
		expect(snapshot?.attempts).toEqual([
			expect.objectContaining({
				node_id: 'node-a',
				attempt_sequence: 1,
				state: 'committed_terminal',
				outcome: 'success',
			}),
			expect.objectContaining({
				node_id: 'node-b',
				attempt_sequence: 2,
				state: 'committed_terminal',
				outcome: 'success',
			}),
		])
		expect(snapshot?.resume).toMatchObject({
			local_resume_available: false,
			last_durable_boundary_sequence: 2,
			last_attempt_id: snapshot?.attempts[1]?.attempt_id,
		})
	})

	it('rejects schema-invalid json success results before committing them', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile(STRICT_JSON_OBJECT_OUTPUT)
		const { adapter, requests } = createStubAdapter([
			{
				outcome: 'success',
				output: STRICT_JSON_OBJECT_OUTPUT,
				output_json: {
					summary: 'phase-6',
					count: 2,
					extra: true,
				},
			},
		])

		const result = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'execution' },
			{
				state_store: store,
				resolved_revision_id: 'rev-schema-failure',
				run_id: 'run-schema-failure',
			},
		)

		expect(result).toEqual({
			status: 'failure',
			run_id: 'run-schema-failure',
			run_status: 'failed',
			code: 'INVALID_JSON_OUTPUT',
			message: expect.stringContaining('failed its declared output schema'),
			resume_available: true,
		})
		expect(requests).toHaveLength(1)

		const snapshot = store.getPersistedRunSnapshot('run-schema-failure')
		expect(snapshot?.run.status).toBe('failed')
		expect(snapshot?.current_vars).toEqual({
			seed: 'stable',
		})
		expect(snapshot?.latest_committed_outputs).toEqual([])
		expect(snapshot?.attempts).toEqual([
			expect.objectContaining({
				node_id: 'node-a',
				state: 'committed_terminal',
				outcome: 'invalid_output',
				committed_output_id: null,
			}),
		])
	})

	it('keeps terminal failure state distinct from explicit local resume capability', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		const { adapter } = createStubAdapter([
			{
				outcome: 'invalid_output',
				error: {
					code: 'INVALID_JSON_OUTPUT',
					message: 'Runtime returned invalid JSON.',
				},
			},
		])

		const result = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'bad-output' },
			{
				state_store: store,
				resolved_revision_id: 'rev-failure',
				run_id: 'run-failure',
			},
		)

		expect(result).toEqual({
			status: 'failure',
			run_id: 'run-failure',
			run_status: 'failed',
			code: 'INVALID_JSON_OUTPUT',
			message: 'Runtime returned invalid JSON.',
			resume_available: true,
		})

		const snapshot = store.getPersistedRunSnapshot('run-failure')
		expect(snapshot?.run.status).toBe('failed')
		expect(snapshot?.current_vars).toEqual({
			seed: 'stable',
		})
		expect(snapshot?.latest_committed_outputs).toEqual([])
		expect(snapshot?.attempts).toEqual([
			expect.objectContaining({
				node_id: 'node-a',
				state: 'committed_terminal',
				outcome: 'invalid_output',
				committed_output_id: null,
			}),
		])
		expect(snapshot?.resume).toMatchObject({
			local_resume_available: true,
			last_durable_boundary_kind: 'node_attempt_terminal',
			pending_prompt: null,
		})

		const { adapter: resumeAdapter, requests: resumeRequests } = createStubAdapter([
			{
				outcome: 'success',
				output: JSON_OBJECT_OUTPUT,
				output_json: {
					summary: 'resumed',
					count: 5,
				},
			},
			{
				outcome: 'success',
				output: TEXT_OUTPUT,
				output_text: 'resumed-after-failure',
			},
		])

		const resumed = await resumeAgentRun(agentFile, resumeAdapter, 'run-failure', {
			state_store: store,
			resolved_revision_id: 'rev-failure',
		})

		expect(resumed).toEqual({
			status: 'success',
			run_id: 'run-failure',
			run_status: 'completed',
			final_output: 'resumed-after-failure',
			final_output_mode: 'text',
			node_outputs: expect.any(Map),
		})
		expect(resumeRequests.map((request) => request.node_id)).toEqual(['node-a', 'node-b'])
		expect(resumeRequests.map((request) => request.input_message)).toEqual([
			'Topic: bad-output',
			'Summary: resumed; Count: 5',
		])

		const resumedSnapshot = store.getPersistedRunSnapshot('run-failure')
		expect(resumedSnapshot?.attempts).toEqual([
			expect.objectContaining({
				node_id: 'node-a',
				attempt_sequence: 1,
				state: 'committed_terminal',
				outcome: 'invalid_output',
				committed_output_id: null,
			}),
			expect.objectContaining({
				node_id: 'node-a',
				attempt_sequence: 2,
				state: 'committed_terminal',
				outcome: 'success',
			}),
			expect.objectContaining({
				node_id: 'node-b',
				attempt_sequence: 3,
				state: 'committed_terminal',
				outcome: 'success',
			}),
		])
		expect(resumedSnapshot?.run.status).toBe('completed')
		expect(resumedSnapshot?.resume).toMatchObject({
			local_resume_available: false,
			last_durable_boundary_sequence: 3,
		})
	})

	it('keeps cancelled runs non-resumable', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		const { adapter } = createStubAdapter([
			{
				outcome: 'cancelled',
				error: {
					code: 'CANCELLED',
					message: 'Execution was cancelled.',
				},
			},
		])

		const result = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'cancelled' },
			{
				state_store: store,
				resolved_revision_id: 'rev-cancelled',
				run_id: 'run-cancelled',
			},
		)

		expect(result).toEqual({
			status: 'failure',
			run_id: 'run-cancelled',
			run_status: 'cancelled',
			code: 'CANCELLED',
			message: 'Execution was cancelled.',
			resume_available: false,
		})
		expect(store.getPersistedRunSnapshot('run-cancelled')?.resume).toMatchObject({
			local_resume_available: false,
		})
	})

	it('resumes from the last committed success boundary and continues with the next node', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		const run = store.createRun({
			run_id: 'run-resume-boundary',
			logical_agent_id: agentFile.meta.id,
			resolved_revision_id: 'rev-resume-boundary',
			entry_node_id: agentFile.entry_node_id,
			started_via: 'direct',
			params: {
				topic: 'resume',
			},
			initial_vars: agentFile.initial_vars ?? {},
			resume: {
				native_resume_available: false,
				local_resume_available: true,
			},
		})
		const firstAttempt = store.startNodeAttempt({
			run_id: run.run_id,
			node_id: 'node-a',
			output_mode: 'json',
		})
		store.commitNodeSuccess({
			attempt_id: firstAttempt.attempt_id,
			output: {
				mode: 'json',
				json: {
					summary: 'committed',
					count: 3,
				},
			},
			vars: {
				seed: 'stable',
				summary: 'committed',
				count: 3,
			},
			run_status: 'running',
			resume: {
				native_resume_available: false,
				local_resume_available: true,
			},
		})

		const { adapter, requests } = createStubAdapter([
			{
				outcome: 'success',
				output: TEXT_OUTPUT,
				output_text: 'resumed-final',
			},
		])

		const result = await resumeAgentRun(agentFile, adapter, run.run_id, {
			state_store: store,
			resolved_revision_id: 'rev-resume-boundary',
		})

		expect(result).toEqual({
			status: 'success',
			run_id: 'run-resume-boundary',
			run_status: 'completed',
			final_output: 'resumed-final',
			final_output_mode: 'text',
			node_outputs: expect.any(Map),
		})
		expect(requests).toHaveLength(1)
		expect(requests[0]?.node_id).toBe('node-b')
		expect(requests[0]?.input_message).toBe('Summary: committed; Count: 3')

		const snapshot = store.getPersistedRunSnapshot(run.run_id)
		expect(snapshot?.attempts).toEqual([
			expect.objectContaining({
				attempt_id: firstAttempt.attempt_id,
				node_id: 'node-a',
				state: 'committed_terminal',
				outcome: 'success',
			}),
			expect.objectContaining({
				node_id: 'node-b',
				attempt_sequence: 2,
				state: 'committed_terminal',
				outcome: 'success',
			}),
		])
	})

	it('prefers native resume when the adapter supports it and stored metadata allows it', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		agentFile.chat = {
			prefer_native_resume: true,
			store_visible_messages: true,
			store_context_window: true,
			allow_fresh_start: true,
		}

		const run = store.createRun({
			run_id: 'run-native-resume',
			logical_agent_id: agentFile.meta.id,
			resolved_revision_id: 'rev-native-resume',
			entry_node_id: agentFile.entry_node_id,
			started_via: 'direct',
			params: {
				topic: 'native-resume',
			},
			initial_vars: agentFile.initial_vars ?? {},
			chat: {
				policy: agentFile.chat,
			},
			resume: {
				native_resume_available: false,
				local_resume_available: true,
			},
		})
		const failedAttempt = store.startNodeAttempt({
			run_id: run.run_id,
			node_id: 'node-a',
			output_mode: 'json',
			started_at: '2026-04-22T10:10:00.000Z',
		})
		store.commitNodeTerminalOutcome({
			attempt_id: failedAttempt.attempt_id,
			outcome: 'runtime_error',
			run_status: 'failed',
			resume: {
				native_resume_available: true,
				local_resume_available: true,
				native_session_handle: {
					session: 'native-session-1',
				},
			},
			committed_at: '2026-04-22T10:10:01.000Z',
		})

		const { adapter, requests } = createStubAdapter(
			[
				{
					runtime_handle: {
						runtime: 'native-handle-1',
					},
					native_session_handle: {
						session: 'native-session-2',
					},
					terminal_result: {
						outcome: 'success',
						output: JSON_OBJECT_OUTPUT,
						output_json: {
							summary: 'native',
							count: 7,
						},
					},
				},
				{
					runtime_handle: {
						runtime: 'native-handle-2',
					},
					native_session_handle: {
						session: 'native-session-3',
					},
					terminal_result: {
						outcome: 'success',
						output: TEXT_OUTPUT,
						output_text: 'native-done',
					},
				},
			],
			{
				supports_native_resume: true,
			},
		)

		const result = await resumeAgentRun(agentFile, adapter, run.run_id, {
			state_store: store,
			resolved_revision_id: 'rev-native-resume',
		})

		expect(result).toEqual({
			status: 'success',
			run_id: 'run-native-resume',
			run_status: 'completed',
			final_output: 'native-done',
			final_output_mode: 'text',
			node_outputs: expect.any(Map),
		})
		expect(requests.map((request) => request.resume)).toEqual([
			{
				mode: 'native_resume',
				native_session_handle: {
					session: 'native-session-1',
				},
			},
			{
				mode: 'native_resume',
				native_session_handle: {
					session: 'native-session-2',
				},
			},
		])

		const snapshot = store.getPersistedRunSnapshot(run.run_id)
		expect(snapshot?.attempts).toEqual([
			expect.objectContaining({
				node_id: 'node-a',
				state: 'committed_terminal',
				outcome: 'runtime_error',
			}),
			expect.objectContaining({
				node_id: 'node-a',
				runtime_handle: {
					runtime: 'native-handle-1',
				},
			}),
			expect.objectContaining({
				node_id: 'node-b',
				runtime_handle: {
					runtime: 'native-handle-2',
				},
			}),
		])
		expect(snapshot?.resume).toMatchObject({
			native_resume_available: true,
			native_session_handle: {
				session: 'native-session-3',
			},
		})
	})

	it('permits native resume from a terminal run even when local resume is unavailable', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		agentFile.chat = {
			prefer_native_resume: true,
			store_visible_messages: true,
			store_context_window: true,
			allow_fresh_start: true,
		}

		const run = store.createRun({
			run_id: 'run-native-only-resume',
			logical_agent_id: agentFile.meta.id,
			resolved_revision_id: 'rev-native-only-resume',
			entry_node_id: agentFile.entry_node_id,
			started_via: 'direct',
			params: {
				topic: 'native-only-resume',
			},
			initial_vars: agentFile.initial_vars ?? {},
			chat: {
				policy: agentFile.chat,
			},
			resume: {
				native_resume_available: false,
				local_resume_available: true,
			},
		})
		const failedAttempt = store.startNodeAttempt({
			run_id: run.run_id,
			node_id: 'node-a',
			output_mode: 'json',
			started_at: '2026-04-22T10:30:00.000Z',
		})
		store.commitNodeTerminalOutcome({
			attempt_id: failedAttempt.attempt_id,
			outcome: 'runtime_error',
			run_status: 'failed',
			resume: {
				native_resume_available: true,
				local_resume_available: false,
				native_session_handle: {
					session: 'native-session-1',
				},
			},
			committed_at: '2026-04-22T10:30:01.000Z',
		})

		const { adapter, requests } = createStubAdapter(
			[
				{
					runtime_handle: {
						runtime: 'native-handle-1',
					},
					native_session_handle: {
						session: 'native-session-2',
					},
					terminal_result: {
						outcome: 'success',
						output: JSON_OBJECT_OUTPUT,
						output_json: {
							summary: 'native-only',
							count: 8,
						},
					},
				},
				{
					runtime_handle: {
						runtime: 'native-handle-2',
					},
					native_session_handle: {
						session: 'native-session-3',
					},
					terminal_result: {
						outcome: 'success',
						output: TEXT_OUTPUT,
						output_text: 'native-only-done',
					},
				},
			],
			{
				supports_native_resume: true,
			},
		)

		const result = await resumeAgentRun(agentFile, adapter, run.run_id, {
			state_store: store,
			resolved_revision_id: 'rev-native-only-resume',
		})

		expect(result).toEqual({
			status: 'success',
			run_id: 'run-native-only-resume',
			run_status: 'completed',
			final_output: 'native-only-done',
			final_output_mode: 'text',
			node_outputs: expect.any(Map),
		})
		expect(requests.map((request) => request.resume)).toEqual([
			{
				mode: 'native_resume',
				native_session_handle: {
					session: 'native-session-1',
				},
			},
			{
				mode: 'native_resume',
				native_session_handle: {
					session: 'native-session-2',
				},
			},
		])
		expect(store.getPersistedRunSnapshot(run.run_id)?.run.status).toBe('completed')
	})

	it('keeps waiting_for_user runs blocked even when native resume metadata is present', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		agentFile.chat = {
			prefer_native_resume: true,
			store_visible_messages: true,
			store_context_window: true,
			allow_fresh_start: true,
		}

		const run = store.createRun({
			run_id: 'run-native-blocked-prompt',
			logical_agent_id: agentFile.meta.id,
			resolved_revision_id: 'rev-native-blocked-prompt',
			entry_node_id: agentFile.entry_node_id,
			started_via: 'direct',
			params: {
				topic: 'blocked-prompt',
			},
			initial_vars: agentFile.initial_vars ?? {},
			chat: {
				policy: agentFile.chat,
			},
			resume: {
				native_resume_available: false,
				local_resume_available: true,
			},
		})
		const attempt = store.startNodeAttempt({
			run_id: run.run_id,
			node_id: 'node-a',
			output_mode: 'json',
			started_at: '2026-04-22T10:35:00.000Z',
		})
		store.commitBlockedAttempt({
			attempt_id: attempt.attempt_id,
			pending_prompt: {
				prompt_id: 'prompt-1',
				payload: {
					kind: 'text',
					text: 'Need a follow-up answer.',
					require_response: true,
				},
				request_handle: {
					execution: 'native-prompt-1',
				},
			},
			resume: {
				native_resume_available: true,
				local_resume_available: true,
				native_session_handle: {
					session: 'native-session-blocked',
				},
			},
			committed_at: '2026-04-22T10:35:01.000Z',
		})

		const { adapter, requests } = createStubAdapter([], {
			supports_native_resume: true,
		})

		await expect(
			resumeAgentRun(agentFile, adapter, run.run_id, {
				state_store: store,
				resolved_revision_id: 'rev-native-blocked-prompt',
			}),
		).rejects.toMatchObject({
			code: 'RUN_WAITING_FOR_USER',
			message: `Run "${run.run_id}" is blocked on unresolved user input and cannot be locally resumed by this slice.`,
		} satisfies Pick<AppError, 'code' | 'message'>)
		expect(requests).toHaveLength(0)
		expect(store.getPersistedRunSnapshot(run.run_id)).toMatchObject({
			run: {
				status: 'waiting_for_user',
			},
			resume: {
				pending_prompt: expect.objectContaining({
					unresolved: true,
				}),
				native_resume_available: true,
			},
		})
	})

	it('rejects native resume for completed runs without reopening them', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		agentFile.chat = {
			prefer_native_resume: true,
			store_visible_messages: true,
			store_context_window: true,
			allow_fresh_start: true,
		}
		agentFile.edges = []

		const run = store.createRun({
			run_id: 'run-completed-native-resume',
			logical_agent_id: agentFile.meta.id,
			resolved_revision_id: 'rev-completed-native-resume',
			entry_node_id: agentFile.entry_node_id,
			started_via: 'direct',
			params: {
				topic: 'completed-native-resume',
			},
			initial_vars: agentFile.initial_vars ?? {},
			chat: {
				policy: agentFile.chat,
			},
			resume: {
				native_resume_available: false,
				local_resume_available: true,
			},
		})
		const attempt = store.startNodeAttempt({
			run_id: run.run_id,
			node_id: 'node-a',
			output_mode: 'json',
			started_at: '2026-04-22T10:40:00.000Z',
		})
		store.commitNodeSuccess({
			attempt_id: attempt.attempt_id,
			output: {
				mode: 'json',
				json: {
					summary: 'completed',
					count: 9,
				},
			},
			vars: {
				seed: 'stable',
				summary: 'completed',
				count: 9,
			},
			run_status: 'completed',
			resume: {
				native_resume_available: true,
				local_resume_available: false,
				native_session_handle: {
					session: 'native-session-completed',
				},
			},
			committed_at: '2026-04-22T10:40:01.000Z',
		})

		const { adapter, requests } = createStubAdapter(
			[
				{
					runtime_handle: {
						runtime: 'native-handle-completed',
					},
					native_session_handle: {
						session: 'native-session-resume',
					},
					terminal_result: {
						outcome: 'success',
						output: TEXT_OUTPUT,
						output_text: 'should-not-run',
					},
				},
			],
			{
				supports_native_resume: true,
			},
		)

		await expect(
			resumeAgentRun(agentFile, adapter, run.run_id, {
				state_store: store,
				resolved_revision_id: 'rev-completed-native-resume',
			}),
		).rejects.toMatchObject({
			code: 'RUN_NOT_RESUMABLE',
			message: `Run "${run.run_id}" has already completed.`,
		} satisfies Pick<AppError, 'code' | 'message'>)
		expect(requests).toHaveLength(0)
		expect(store.getPersistedRunSnapshot(run.run_id)?.run.status).toBe('completed')
	})

	it('does not fabricate success for an unfinished attempt and reuses that attempt on local resume', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		const run = store.createRun({
			run_id: 'run-resume-inflight',
			logical_agent_id: agentFile.meta.id,
			resolved_revision_id: 'rev-resume-inflight',
			entry_node_id: agentFile.entry_node_id,
			started_via: 'direct',
			params: {
				topic: 'resume',
			},
			initial_vars: agentFile.initial_vars ?? {},
			resume: {
				native_resume_available: false,
				local_resume_available: true,
			},
		})
		const firstAttempt = store.startNodeAttempt({
			run_id: run.run_id,
			node_id: 'node-a',
			output_mode: 'json',
		})
		store.commitNodeSuccess({
			attempt_id: firstAttempt.attempt_id,
			output: {
				mode: 'json',
				json: {
					summary: 'stable',
					count: 4,
				},
			},
			vars: {
				seed: 'stable',
				summary: 'stable',
				count: 4,
			},
			run_status: 'running',
			resume: {
				native_resume_available: false,
				local_resume_available: true,
			},
		})
		const inflightAttempt = store.startNodeAttempt({
			run_id: run.run_id,
			node_id: 'node-b',
			output_mode: 'text',
		})

		expect(store.getLatestCommittedNodeOutput(run.run_id, 'node-b')).toBeNull()

		const { adapter, requests } = createStubAdapter([
			{
				outcome: 'success',
				output: TEXT_OUTPUT,
				output_text: 'finished-after-resume',
			},
		])

		const result = await resumeAgentRun(agentFile, adapter, run.run_id, {
			state_store: store,
			resolved_revision_id: 'rev-resume-inflight',
		})

		expect(result).toEqual({
			status: 'success',
			run_id: 'run-resume-inflight',
			run_status: 'completed',
			final_output: 'finished-after-resume',
			final_output_mode: 'text',
			node_outputs: expect.any(Map),
		})
		expect(requests).toHaveLength(1)
		expect(requests[0]?.node_id).toBe('node-b')
		expect(requests[0]?.resume.mode).toBe('fresh')

		const snapshot = store.getPersistedRunSnapshot(run.run_id)
		expect(snapshot?.attempts).toEqual([
			expect.objectContaining({
				attempt_id: firstAttempt.attempt_id,
				node_id: 'node-a',
				state: 'committed_terminal',
				outcome: 'success',
			}),
			expect.objectContaining({
				attempt_id: inflightAttempt.attempt_id,
				node_id: 'node-b',
				state: 'committed_terminal',
				outcome: 'success',
			}),
		])
		expect(snapshot?.latest_committed_outputs).toEqual([
			expect.objectContaining({
				node_id: 'node-a',
			}),
			expect.objectContaining({
				attempt_id: inflightAttempt.attempt_id,
				node_id: 'node-b',
				output: {
					mode: 'text',
					text: 'finished-after-resume',
				},
			}),
		])
	})

	it('rejects local resume when stored resume metadata disables it, even if the run is still marked running', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		const run = store.createRun({
			run_id: 'run-resume-disabled',
			logical_agent_id: agentFile.meta.id,
			resolved_revision_id: 'rev-resume-disabled',
			entry_node_id: agentFile.entry_node_id,
			started_via: 'direct',
			params: {
				topic: 'resume-disabled',
			},
			initial_vars: agentFile.initial_vars ?? {},
			resume: {
				native_resume_available: false,
				local_resume_available: false,
			},
		})

		const { adapter, requests } = createStubAdapter([
			{
				outcome: 'success',
				output: JSON_OBJECT_OUTPUT,
				output_json: {
					summary: 'should-not-run',
					count: 1,
				},
			},
		])

		await expect(
			resumeAgentRun(agentFile, adapter, run.run_id, {
				state_store: store,
				resolved_revision_id: 'rev-resume-disabled',
			}),
		).rejects.toMatchObject({
			code: 'RUN_NOT_RESUMABLE',
			message: `Run "${run.run_id}" is not available for explicit local resume.`,
		} satisfies Pick<AppError, 'code' | 'message'>)
		expect(requests).toHaveLength(0)
	})

	it('fails fast when a runtime node declares unsupported bindings, permissions, or runtime-source intent', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()

		const firstNode = agentFile.nodes[0]
		if (firstNode?.kind !== 'runtime_agent') {
			throw new Error('expected runtime_agent test node')
		}
		firstNode.skill_ids = ['skill-a']
		firstNode.permissions = {
			profile: 'strict',
		}
		firstNode.runtime_source_policy = 'restrict'
		firstNode.runtime_source_ids = ['source-a']

		const { adapter, requests } = createStubAdapter([
			{
				outcome: 'success',
				output: JSON_OBJECT_OUTPUT,
				output_json: {
					summary: 'should-not-run',
					count: 1,
				},
			},
		])

		await expect(
			runAgentFile(
				agentFile,
				adapter,
				{ topic: 'unsupported-runtime-context' },
				{
					state_store: store,
					resolved_revision_id: 'rev-unsupported-runtime-context',
					run_id: 'run-unsupported-runtime-context',
				},
			),
		).rejects.toMatchObject({
			code: 'UNSUPPORTED_RUNTIME_CONTEXT',
			message:
				'Node "node-a" declares skill bindings, permissions, which is not implemented in the current execution slice.',
		} satisfies Pick<AppError, 'code' | 'message'>)
		expect(requests).toHaveLength(0)
		expect(store.getPersistedRunSnapshot('run-unsupported-runtime-context')).toBeNull()
	})

	it('accepts the phase-14 runtime option allowlist and still rejects unknown keys', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()

		const firstNode = agentFile.nodes[0]
		if (firstNode?.kind !== 'runtime_agent') {
			throw new Error('expected runtime_agent test node')
		}
		firstNode.runtime_options = {
			model: 'gpt-5.3-codex',
			reasoning_effort: 'medium',
			speed_tier: 'fast',
			personality: 'pragmatic',
			temperature: 0.1,
		}

		const { adapter, requests } = createStubAdapter([
			{
				outcome: 'success',
				output: JSON_OBJECT_OUTPUT,
				output_json: {
					summary: 'should-not-run',
					count: 1,
				},
			},
		])

		await expect(
			runAgentFile(
				agentFile,
				adapter,
				{ topic: 'runtime-options' },
				{
					state_store: store,
					resolved_revision_id: 'rev-runtime-options',
					run_id: 'run-runtime-options',
				},
			),
		).rejects.toMatchObject({
			code: 'UNSUPPORTED_RUNTIME_CONTEXT',
			message:
				'Node "node-a" declares runtime option "temperature", which is not implemented in the current execution slice.',
		} satisfies Pick<AppError, 'code' | 'message'>)
		expect(requests).toHaveLength(0)
		expect(store.getPersistedRunSnapshot('run-runtime-options')).toBeNull()
	})

	it('rejects supported runtime option keys when the selected adapter lacks the matching capability', async () => {
		const cases = [
			{
				runId: 'run-runtime-options-no-reasoning-effort',
				runtimeOptions: {
					reasoning_effort: 'medium',
				},
				message:
					'Node "node-a" declares runtime option "reasoning_effort", but the runtime adapter does not support it.',
			},
			{
				runId: 'run-runtime-options-no-speed-tier',
				runtimeOptions: {
					speed_tier: 'fast',
				},
				message:
					'Node "node-a" declares runtime option "speed_tier", but the runtime adapter does not support it.',
			},
			{
				runId: 'run-runtime-options-no-personality',
				runtimeOptions: {
					personality: 'pragmatic',
				},
				message:
					'Node "node-a" declares runtime option "personality", but the runtime adapter does not support it.',
			},
		] as const

		for (const testCase of cases) {
			const store = await createStore()
			const agentFile = buildAgentFile()

			const firstNode = agentFile.nodes[0]
			if (firstNode?.kind !== 'runtime_agent') {
				throw new Error('expected runtime_agent test node')
			}
			firstNode.runtime_options = testCase.runtimeOptions

			const { adapter, requests } = createStubAdapter([
				{
					outcome: 'success',
					output: JSON_OBJECT_OUTPUT,
					output_json: {
						summary: 'should-not-run',
						count: 1,
					},
				},
			])

			await expect(
				runAgentFile(
					agentFile,
					adapter,
					{ topic: 'runtime-option-capabilities' },
					{
						state_store: store,
						resolved_revision_id: 'rev-runtime-option-capabilities',
						run_id: testCase.runId,
					},
				),
			).rejects.toMatchObject({
				code: 'UNSUPPORTED_RUNTIME_CONTEXT',
				message: testCase.message,
			} satisfies Pick<AppError, 'code' | 'message'>)
			expect(requests).toHaveLength(0)
			expect(store.getPersistedRunSnapshot(testCase.runId)).toBeNull()
		}
	})

	it('rejects unsupported top-level skill bindings before creating a run and before resuming one', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		agentFile.skills = [
			{
				id: 'skill-a',
				inline_text: 'Use the bound skill.',
			},
		]

		const { adapter, requests } = createStubAdapter([
			{
				outcome: 'success',
				output: JSON_OBJECT_OUTPUT,
				output_json: {
					summary: 'should-not-run',
					count: 1,
				},
			},
		])

		await expect(
			runAgentFile(
				agentFile,
				adapter,
				{ topic: 'top-level-bindings' },
				{
					state_store: store,
					resolved_revision_id: 'rev-top-level-bindings',
					run_id: 'run-top-level-bindings',
				},
			),
		).rejects.toMatchObject({
			code: 'UNSUPPORTED_RUNTIME_CONTEXT',
			message:
				'Top-level skill, MCP, and plugin bindings are not implemented in the current execution slice.',
		} satisfies Pick<AppError, 'code' | 'message'>)
		expect(requests).toHaveLength(0)
		expect(store.getPersistedRunSnapshot('run-top-level-bindings')).toBeNull()

		const resumableRun = store.createRun({
			run_id: 'run-top-level-bindings-resume',
			logical_agent_id: agentFile.meta.id,
			resolved_revision_id: 'rev-top-level-bindings',
			entry_node_id: agentFile.entry_node_id,
			started_via: 'direct',
			params: {
				topic: 'top-level-bindings',
			},
			initial_vars: agentFile.initial_vars ?? {},
			resume: {
				native_resume_available: false,
				local_resume_available: true,
			},
		})

		await expect(
			resumeAgentRun(agentFile, adapter, resumableRun.run_id, {
				state_store: store,
				resolved_revision_id: 'rev-top-level-bindings',
			}),
		).rejects.toMatchObject({
			code: 'UNSUPPORTED_RUNTIME_CONTEXT',
			message:
				'Top-level skill, MCP, and plugin bindings are not implemented in the current execution slice.',
		} satisfies Pick<AppError, 'code' | 'message'>)
		expect(requests).toHaveLength(0)
	})

	it('resolves runtime memory bindings and narrowed runtime sources for runtime_agent nodes', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		agentFile.memory_bindings = [
			buildRuntimeMemoryBinding({
				id: 'agent-memory',
				codex_ref: 'memory://agent',
				scope: 'agent',
			}),
			buildRuntimeMemoryBinding({
				id: 'node-memory',
				codex_ref: 'memory://node',
				scope: 'node',
			}),
		]
		agentFile.runtime_sources = [
			{
				id: 'source-a',
				runtime_adapter: 'codex',
				source_ref: 'workspace://source-a',
			},
			{
				id: 'source-b',
				runtime_adapter: 'codex',
				source_ref: 'workspace://source-b',
			},
		]
		const firstNode = agentFile.nodes[0]
		if (firstNode?.kind !== 'runtime_agent') {
			throw new Error('expected runtime_agent test node')
		}
		firstNode.memory_ids = ['node-memory']
		firstNode.runtime_source_policy = 'restrict'
		firstNode.runtime_source_ids = ['source-a', 'source-b']
		const prepareMemoryContext = vi
			.spyOn(MemoryService.prototype, 'prepareRuntimeMemoryBindingContext')
			.mockImplementation(async ({ binding, scope, read }) => ({
				context: {
					binding_id: binding.id,
					codex_ref: binding.codex_ref,
					intent: {
						summary: `Prepared ${binding.id}`,
					},
					required_capabilities: ['read', 'write', 'user_scoped'],
					scope,
					read: read
						? {
								query: read.query,
								records: [
									{
										id: `${binding.id}-record`,
										content: `memory for ${read.query}`,
										scope,
									},
								],
							}
						: undefined,
					write: {
						enabled: true,
						mode: 'node_success_output',
					},
				},
				provider: {
					provider_id: `${binding.id}-provider`,
				} as never,
				read_enabled: true,
				write_enabled: true,
				required_capabilities: ['read', 'write', 'user_scoped'],
			}))
		const writeMemory = vi
			.spyOn(MemoryService.prototype, 'writeRuntimeMemoryOnSuccess')
			.mockResolvedValue({
				status: 'written',
				dennett_write_key: 'test-write-key',
				metadata: {},
				result: {
					records: [],
				},
			})

		const { adapter, requests } = createStubAdapter(
			[
				{
					outcome: 'success',
					output: JSON_OBJECT_OUTPUT,
					output_json: {
						summary: 'phase-9',
						count: 1,
					},
				},
				{
					outcome: 'success',
					output: TEXT_OUTPUT,
					output_text: 'done',
				},
			],
			{
				supports_memory_bindings: true,
				supports_explicit_runtime_source: true,
				supports_runtime_source_introspection: true,
			},
			{
				'source-a': {
					source_id: 'source-a',
					availability: 'unavailable',
					limit_status: 'ok',
				},
				'source-b': {
					source_id: 'source-b',
					availability: 'available',
					limit_status: 'ok',
				},
			},
		)

		const result = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'runtime-sources' },
			{
				state_store: store,
				resolved_revision_id: 'rev-top-level-runtime-sources',
				run_id: 'run-top-level-runtime-sources',
			},
		)

		expect(result.status).toBe('success')
		expect(requests[0]?.effective_bindings.memory_bindings).toEqual([
			{
				id: 'node-memory',
				kind: 'runtime_memory',
				codex_ref: 'memory://node',
				scope: 'node',
			},
		])
		expect(requests[0]?.memory_context).toMatchObject({
			bindings: [
				{
					binding_id: 'node-memory',
					codex_ref: 'memory://node',
					read: {
						query: 'Topic: runtime-sources',
					},
					write: {
						enabled: true,
						mode: 'node_success_output',
					},
				},
			],
		})
		expect(requests[0]?.runtime_source).toEqual({
			id: 'source-b',
			runtime_adapter: 'codex',
			source_ref: 'workspace://source-b',
		})
		expect(requests[1]?.runtime_source).toEqual({
			id: 'source-b',
			runtime_adapter: 'codex',
			source_ref: 'workspace://source-b',
		})
		expect(prepareMemoryContext).toHaveBeenCalledTimes(2)
		expect(writeMemory).toHaveBeenCalledTimes(2)
	})

	it('applies user runtime-source narrowing on top of file and node constraints', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		agentFile.runtime_sources = [
			{
				id: 'source-a',
				runtime_adapter: 'codex',
				source_ref: 'workspace://source-a',
			},
			{
				id: 'source-b',
				runtime_adapter: 'codex',
				source_ref: 'workspace://source-b',
			},
		]
		const firstNode = agentFile.nodes[0]
		if (firstNode?.kind !== 'runtime_agent') {
			throw new Error('expected runtime_agent test node')
		}
		firstNode.runtime_source_policy = 'restrict'
		firstNode.runtime_source_ids = ['source-a', 'source-b']

		const { adapter, requests } = createStubAdapter(
			[
				{
					outcome: 'success',
					output: JSON_OBJECT_OUTPUT,
					output_json: {
						summary: 'phase-9',
						count: 1,
					},
				},
				{
					outcome: 'success',
					output: TEXT_OUTPUT,
					output_text: 'done',
				},
			],
			{
				supports_explicit_runtime_source: true,
			},
		)

		const result = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'user-narrowing' },
			{
				state_store: store,
				resolved_revision_id: 'rev-user-runtime-source',
				run_id: 'run-user-runtime-source',
				user_runtime_source_ids: ['source-b'],
			},
		)

		expect(result.status).toBe('success')
		expect(requests[0]?.runtime_source).toEqual({
			id: 'source-b',
			runtime_adapter: 'codex',
			source_ref: 'workspace://source-b',
		})
		expect(requests[1]?.runtime_source).toEqual({
			id: 'source-b',
			runtime_adapter: 'codex',
			source_ref: 'workspace://source-b',
		})
	})

	it('writes successful runtime outputs to write-eligible memory before node success is committed', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		agentFile.edges = []
		agentFile.memory_bindings = [buildRuntimeMemoryBinding()]
		const outputJson = {
			count: 4,
			summary: 'remembered',
		}
		vi.spyOn(MemoryService.prototype, 'prepareRuntimeMemoryBindingContext').mockImplementation(
			async ({ binding, scope, read }) => ({
				context: {
					binding_id: binding.id,
					codex_ref: binding.codex_ref,
					intent: {
						summary: 'Prepared memory context',
					},
					required_capabilities: ['read', 'write', 'user_scoped'],
					scope,
					read: {
						query: read?.query ?? '',
						records: [],
					},
					write: {
						enabled: true,
						mode: 'node_success_output',
					},
				},
				provider: {
					provider_id: 'mem-provider',
				} as never,
				read_enabled: true,
				write_enabled: true,
				required_capabilities: ['read', 'write', 'user_scoped'],
			}),
		)
		const writeMemory = vi
			.spyOn(MemoryService.prototype, 'writeRuntimeMemoryOnSuccess')
			.mockImplementation(async () => {
				const snapshot = store.getPersistedRunSnapshot('run-memory-write-success')
				expect(snapshot?.attempts.at(-1)).toMatchObject({
					node_id: 'node-a',
					state: 'in_progress',
					outcome: null,
				})
				expect(snapshot?.latest_committed_outputs).toEqual([])
				return {
					status: 'written',
					dennett_write_key: 'test-write-key',
					metadata: {},
					result: {
						records: [],
					},
				}
			})

		const { adapter, requests } = createStubAdapter(
			[
				{
					outcome: 'success',
					output: JSON_OBJECT_OUTPUT,
					output_json: outputJson,
				},
			],
			{
				supports_memory_bindings: true,
			},
		)

		const result = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'memory write' },
			{
				state_store: store,
				resolved_revision_id: 'rev-memory-write-success',
				run_id: 'run-memory-write-success',
			},
		)

		expect(result.status).toBe('success')
		expect(requests[0]?.memory_context).toMatchObject({
			bindings: [
				{
					binding_id: 'agent-memory',
					read: {
						query: 'Topic: memory write',
					},
					write: {
						enabled: true,
						mode: 'node_success_output',
					},
				},
			],
		})
		expect(writeMemory).toHaveBeenCalledOnce()
		expect(writeMemory).toHaveBeenCalledWith(
			expect.objectContaining({
				binding: expect.objectContaining({
					id: 'agent-memory',
				}),
				scope: {
					agent_id: 'agent.phase6.runner',
					run_id: 'run-memory-write-success',
				},
				node_id: 'node-a',
				output_mode: 'json',
				output_hash: expectedOutputHash({
					json: outputJson,
					mode: 'json',
				}),
				content: '{"count":4,"summary":"remembered"}',
				outcome: 'success',
			}),
		)
		expect(
			store.getPersistedRunSnapshot('run-memory-write-success')?.attempts.at(-1),
		).toMatchObject({
			node_id: 'node-a',
			state: 'committed_terminal',
			outcome: 'success',
		})
	})

	it('fails before node completion and downstream execution when runtime memory success-write fails', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		agentFile.memory_bindings = [buildRuntimeMemoryBinding()]
		vi.spyOn(MemoryService.prototype, 'prepareRuntimeMemoryBindingContext').mockImplementation(
			async ({ binding, scope, read }) => ({
				context: {
					binding_id: binding.id,
					codex_ref: binding.codex_ref,
					intent: {
						summary: 'Prepared memory context',
					},
					required_capabilities: ['read', 'write', 'user_scoped'],
					scope,
					read: {
						query: read?.query ?? '',
						records: [],
					},
					write: {
						enabled: true,
						mode: 'node_success_output',
					},
				},
				provider: {
					provider_id: 'mem-provider',
				} as never,
				read_enabled: true,
				write_enabled: true,
				required_capabilities: ['read', 'write', 'user_scoped'],
			}),
		)
		const writeMemory = vi
			.spyOn(MemoryService.prototype, 'writeRuntimeMemoryOnSuccess')
			.mockRejectedValue(
				new AppError('MEMORY_PROVIDER_OPERATION_FAILED', 'Memory provider rejected the write.'),
			)

		const { adapter, requests } = createStubAdapter(
			[
				{
					outcome: 'success',
					output: JSON_OBJECT_OUTPUT,
					output_json: {
						summary: 'should-not-commit',
						count: 9,
					},
				},
				{
					outcome: 'success',
					output: TEXT_OUTPUT,
					output_text: 'should-not-run',
				},
			],
			{
				supports_memory_bindings: true,
			},
		)

		const result = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'memory write failure' },
			{
				state_store: store,
				resolved_revision_id: 'rev-memory-write-failure',
				run_id: 'run-memory-write-failure',
			},
		)

		expect(result).toEqual({
			status: 'failure',
			run_id: 'run-memory-write-failure',
			run_status: 'failed',
			code: 'MEMORY_PROVIDER_OPERATION_FAILED',
			message: expect.stringContaining(
				'Runtime memory write failed while completing node "node-a".',
			),
			resume_available: true,
		})
		expect(requests.map((request) => request.node_id)).toEqual(['node-a'])
		expect(writeMemory).toHaveBeenCalledOnce()

		const snapshot = store.getPersistedRunSnapshot('run-memory-write-failure')
		expect(snapshot?.run.status).toBe('failed')
		expect(snapshot?.current_vars).toEqual({
			seed: 'stable',
		})
		expect(snapshot?.latest_committed_outputs).toEqual([])
		expect(snapshot?.attempts).toEqual([
			expect.objectContaining({
				node_id: 'node-a',
				state: 'committed_terminal',
				outcome: 'runtime_error',
				committed_output_id: null,
			}),
		])
		expect(snapshot?.resume).toMatchObject({
			local_resume_available: true,
			last_durable_boundary_kind: 'node_attempt_terminal',
			pending_prompt: null,
		})
	})

	it('retries the same runtime node after runtime memory success-write failure and then proceeds', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		agentFile.memory_bindings = [buildRuntimeMemoryBinding()]
		vi.spyOn(MemoryService.prototype, 'prepareRuntimeMemoryBindingContext').mockImplementation(
			async ({ binding, scope, read }) => ({
				context: {
					binding_id: binding.id,
					codex_ref: binding.codex_ref,
					intent: {
						summary: 'Prepared memory context',
					},
					required_capabilities: ['read', 'write', 'user_scoped'],
					scope,
					read: {
						query: read?.query ?? '',
						records: [],
					},
					write: {
						enabled: true,
						mode: 'node_success_output',
					},
				},
				provider: {
					provider_id: 'mem-provider',
				} as never,
				read_enabled: true,
				write_enabled: true,
				required_capabilities: ['read', 'write', 'user_scoped'],
			}),
		)
		const writeMemory = vi
			.spyOn(MemoryService.prototype, 'writeRuntimeMemoryOnSuccess')
			.mockRejectedValueOnce(
				new AppError('MEMORY_PROVIDER_TRANSIENT_WRITE_FAILED', 'Transient memory write failure.'),
			)
			.mockResolvedValue({
				status: 'written',
				dennett_write_key: 'test-write-key',
				metadata: {},
				result: {
					records: [],
				},
			})

		const { adapter, requests } = createStubAdapter(
			[
				{
					outcome: 'success',
					output: JSON_OBJECT_OUTPUT,
					output_json: {
						summary: 'not-committed',
						count: 1,
					},
				},
				{
					outcome: 'success',
					output: JSON_OBJECT_OUTPUT,
					output_json: {
						summary: 'committed-after-resume',
						count: 2,
					},
				},
				{
					outcome: 'success',
					output: TEXT_OUTPUT,
					output_text: 'done-after-memory-retry',
				},
			],
			{
				supports_memory_bindings: true,
			},
		)

		const failed = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'memory retry' },
			{
				state_store: store,
				resolved_revision_id: 'rev-memory-retry',
				run_id: 'run-memory-retry',
			},
		)

		expect(failed).toEqual({
			status: 'failure',
			run_id: 'run-memory-retry',
			run_status: 'failed',
			code: 'MEMORY_PROVIDER_TRANSIENT_WRITE_FAILED',
			message: expect.stringContaining(
				'Runtime memory write failed while completing node "node-a".',
			),
			resume_available: true,
		})
		expect(requests.map((request) => request.node_id)).toEqual(['node-a'])

		const failedSnapshot = store.getPersistedRunSnapshot('run-memory-retry')
		expect(failedSnapshot?.run.status).toBe('failed')
		expect(failedSnapshot?.current_vars).toEqual({
			seed: 'stable',
		})
		expect(failedSnapshot?.latest_committed_outputs).toEqual([])
		expect(failedSnapshot?.attempts).toEqual([
			expect.objectContaining({
				node_id: 'node-a',
				state: 'committed_terminal',
				outcome: 'runtime_error',
				committed_output_id: null,
			}),
		])

		const resumed = await resumeAgentRun(agentFile, adapter, 'run-memory-retry', {
			state_store: store,
			resolved_revision_id: 'rev-memory-retry',
		})

		expect(resumed).toEqual({
			status: 'success',
			run_id: 'run-memory-retry',
			run_status: 'completed',
			final_output: 'done-after-memory-retry',
			final_output_mode: 'text',
			node_outputs: expect.any(Map),
		})
		expect(requests.map((request) => request.node_id)).toEqual(['node-a', 'node-a', 'node-b'])
		expect(requests.map((request) => request.input_message)).toEqual([
			'Topic: memory retry',
			'Topic: memory retry',
			'Summary: committed-after-resume; Count: 2',
		])
		expect(writeMemory).toHaveBeenCalledTimes(3)

		const resumedSnapshot = store.getPersistedRunSnapshot('run-memory-retry')
		expect(resumedSnapshot?.attempts).toEqual([
			expect.objectContaining({
				node_id: 'node-a',
				attempt_sequence: 1,
				state: 'committed_terminal',
				outcome: 'runtime_error',
				committed_output_id: null,
			}),
			expect.objectContaining({
				node_id: 'node-a',
				attempt_sequence: 2,
				state: 'committed_terminal',
				outcome: 'success',
			}),
			expect.objectContaining({
				node_id: 'node-b',
				attempt_sequence: 3,
				state: 'committed_terminal',
				outcome: 'success',
			}),
		])
		expect(resumedSnapshot?.current_vars).toEqual({
			seed: 'stable',
			summary: 'committed-after-resume',
			count: 2,
		})
		expect(resumedSnapshot?.latest_committed_outputs).toHaveLength(2)
		expect(resumedSnapshot?.run.status).toBe('completed')
		expect(resumedSnapshot?.resume).toMatchObject({
			local_resume_available: false,
			last_durable_boundary_sequence: 3,
		})
	})

	it('fails before runtime launch when memory context preparation fails', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		agentFile.memory_bindings = [buildRuntimeMemoryBinding()]
		vi.spyOn(MemoryService.prototype, 'prepareRuntimeMemoryBindingContext').mockRejectedValue(
			new AppError(
				'MEMORY_PROVIDER_NOT_FOUND',
				'Memory provider for codex_ref "primary_memory" could not be resolved.',
			),
		)
		const writeMemory = vi.spyOn(MemoryService.prototype, 'writeRuntimeMemoryOnSuccess')
		const { adapter, requests } = createStubAdapter(
			[
				{
					outcome: 'success',
					output: JSON_OBJECT_OUTPUT,
					output_json: {
						summary: 'should-not-run',
						count: 1,
					},
				},
			],
			{
				supports_memory_bindings: true,
			},
		)

		const result = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'memory prepare failure' },
			{
				state_store: store,
				resolved_revision_id: 'rev-memory-prepare-failure',
				run_id: 'run-memory-prepare-failure',
			},
		)

		expect(result).toMatchObject({
			status: 'failure',
			run_status: 'failed',
			code: 'MEMORY_PROVIDER_NOT_FOUND',
			message: expect.stringContaining('failed before terminal classification'),
		})
		expect(requests).toHaveLength(0)
		expect(writeMemory).not.toHaveBeenCalled()
		expect(
			store.getPersistedRunSnapshot('run-memory-prepare-failure')?.attempts.at(-1),
		).toMatchObject({
			node_id: 'node-a',
			outcome: 'runtime_error',
		})
	})

	it('does not write runtime memory for terminal non-success outcomes', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		agentFile.memory_bindings = [buildRuntimeMemoryBinding()]
		vi.spyOn(MemoryService.prototype, 'prepareRuntimeMemoryBindingContext').mockImplementation(
			async ({ binding, scope, read }) => ({
				context: {
					binding_id: binding.id,
					codex_ref: binding.codex_ref,
					intent: {
						summary: 'Prepared memory context',
					},
					required_capabilities: ['read', 'write', 'user_scoped'],
					scope,
					read: {
						query: read?.query ?? '',
						records: [],
					},
					write: {
						enabled: true,
						mode: 'node_success_output',
					},
				},
				provider: {
					provider_id: 'mem-provider',
				} as never,
				read_enabled: true,
				write_enabled: true,
				required_capabilities: ['read', 'write', 'user_scoped'],
			}),
		)
		const writeMemory = vi.spyOn(MemoryService.prototype, 'writeRuntimeMemoryOnSuccess')
		const { adapter, requests } = createStubAdapter(
			[
				{
					outcome: 'interrupted',
					error: {
						code: 'INTERRUPTED',
						message: 'Runtime interrupted before final output.',
					},
				},
			],
			{
				supports_memory_bindings: true,
			},
		)

		const result = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'interrupted memory' },
			{
				state_store: store,
				resolved_revision_id: 'rev-memory-no-write',
				run_id: 'run-memory-no-write',
			},
		)

		expect(result).toMatchObject({
			status: 'failure',
			run_status: 'interrupted',
			code: 'INTERRUPTED',
		})
		expect(requests).toHaveLength(1)
		expect(writeMemory).not.toHaveBeenCalled()
	})

	it('fails before launch when user runtime-source narrowing references an unknown source id', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		agentFile.runtime_sources = [
			{
				id: 'source-a',
				runtime_adapter: 'codex',
				source_ref: 'workspace://source-a',
			},
		]

		const { adapter, requests } = createStubAdapter([
			{
				outcome: 'success',
				output: JSON_OBJECT_OUTPUT,
				output_json: {
					summary: 'should-not-run',
					count: 1,
				},
			},
		])

		const result = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'unknown-user-source' },
			{
				state_store: store,
				resolved_revision_id: 'rev-unknown-user-source',
				run_id: 'run-unknown-user-source',
				user_runtime_source_ids: ['source-missing'],
			},
		)

		expect(result).toMatchObject({
			status: 'failure',
			run_status: 'failed',
			code: 'INVALID_RUNTIME_CONTEXT',
			message: expect.stringContaining('received unknown user runtime source "source-missing"'),
		})
		expect(requests).toHaveLength(0)
	})

	it('fails before launch when user runtime-source narrowing removes every source allowed by node policy', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		agentFile.runtime_sources = [
			{
				id: 'source-a',
				runtime_adapter: 'codex',
				source_ref: 'workspace://source-a',
			},
			{
				id: 'source-b',
				runtime_adapter: 'codex',
				source_ref: 'workspace://source-b',
			},
		]
		const firstNode = agentFile.nodes[0]
		if (firstNode?.kind !== 'runtime_agent') {
			throw new Error('expected runtime_agent test node')
		}
		firstNode.runtime_source_policy = 'restrict'
		firstNode.runtime_source_ids = ['source-a']

		const { adapter, requests } = createStubAdapter([
			{
				outcome: 'success',
				output: JSON_OBJECT_OUTPUT,
				output_json: {
					summary: 'should-not-run',
					count: 1,
				},
			},
		])

		const result = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'excluded-user-source' },
			{
				state_store: store,
				resolved_revision_id: 'rev-excluded-user-source',
				run_id: 'run-excluded-user-source',
				user_runtime_source_ids: ['source-b'],
			},
		)

		expect(result).toMatchObject({
			status: 'failure',
			run_status: 'failed',
			code: 'RUNTIME_SOURCE_UNAVAILABLE',
			message: expect.stringContaining(
				'has no eligible runtime source after applying user narrowing',
			),
		})
		expect(requests).toHaveLength(0)
	})

	it('fails before launch when a runtime node requires memory bindings the adapter does not support', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		agentFile.memory_bindings = [
			{
				id: 'agent-memory',
				kind: 'runtime_memory',
				codex_ref: 'memory://agent',
				scope: 'agent',
			},
		]

		const { adapter, requests } = createStubAdapter([
			{
				outcome: 'success',
				output: JSON_OBJECT_OUTPUT,
				output_json: {
					summary: 'should-not-run',
					count: 1,
				},
			},
		])

		const result = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'memory-bindings' },
			{
				state_store: store,
				resolved_revision_id: 'rev-memory-bindings',
				run_id: 'run-memory-bindings',
			},
		)

		expect(result).toMatchObject({
			status: 'failure',
			run_status: 'failed',
			code: 'UNSUPPORTED_RUNTIME_CONTEXT',
			message: expect.stringContaining(
				'requires memory bindings, but the runtime adapter does not support them.',
			),
		})
		expect(requests).toHaveLength(0)
		expect(store.getPersistedRunSnapshot('run-memory-bindings')?.attempts.at(-1)).toMatchObject({
			node_id: 'node-a',
			outcome: 'runtime_error',
		})
	})

	it('fails when runtime source narrowing requires explicit source selection but the adapter cannot honor it', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		agentFile.runtime_sources = [
			{
				id: 'source-a',
				runtime_adapter: 'codex',
				source_ref: 'workspace://source-a',
			},
		]

		const { adapter, requests } = createStubAdapter([
			{
				outcome: 'success',
				output: JSON_OBJECT_OUTPUT,
				output_json: {
					summary: 'should-not-run',
					count: 1,
				},
			},
		])

		const result = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'narrowed-source' },
			{
				state_store: store,
				resolved_revision_id: 'rev-narrowed-source',
				run_id: 'run-narrowed-source',
			},
		)

		expect(result).toMatchObject({
			status: 'failure',
			run_status: 'failed',
			code: 'UNSUPPORTED_RUNTIME_CONTEXT',
			message: expect.stringContaining(
				'requires explicit runtime_source selection, but the runtime adapter does not support it.',
			),
		})
		expect(requests).toHaveLength(0)
		expect(store.getPersistedRunSnapshot('run-narrowed-source')?.attempts.at(-1)).toMatchObject({
			node_id: 'node-a',
			outcome: 'runtime_error',
		})
	})

	it('executes orchestrator_agent nodes through live child runs', async () => {
		const store = await createStore()
		const tempDir = path.dirname(store.database_path)
		const childAgentId = 'agent.phase9.child'
		const childAgentFile: AgentFile = {
			graph_contract_version: '1.0',
			meta: {
				id: childAgentId,
				name: 'Phase 9 Child Agent',
			},
			entry_node_id: 'child-start',
			final_output: {
				mode: 'last_node_output',
			},
			nodes: [
				{
					id: 'child-start',
					kind: 'runtime_agent',
					runtime_adapter: 'codex',
					prompt: 'Return a child response.',
					input: {
						parts: [
							{ type: 'text', text: 'Child launch: ' },
							{ type: 'ref', ref: 'params.input' },
						],
					},
					output: TEXT_OUTPUT,
				},
			],
		}
		const childPath = await writeAgentFile(tempDir, 'child-agent.json', childAgentFile)
		const lifecycle = new AgentLifecycleService({ state_store: store })
		await lifecycle.registerAgentFile(childPath)
		await lifecycle.deployAgentFile(childPath)

		const parentAgentFile = buildOrchestratorEntryAgentFile(childAgentId)
		const { adapter, requests } = createStubAdapter([
			{
				outcome: 'success',
				output: TEXT_OUTPUT,
				output_text: 'child-final',
			},
		])

		const result = await runAgentFile(
			parentAgentFile,
			adapter,
			{ topic: 'nested execution' },
			{
				state_store: store,
				resolved_revision_id: 'rev-parent',
				run_id: 'run-parent',
			},
		)

		expect(result).toMatchObject({
			status: 'success',
			run_status: 'completed',
			final_output: 'child-final',
			final_output_mode: 'text',
		})
		expect(requests).toHaveLength(1)
		expect(requests[0]?.input_message).toBe('Child launch: Review request: nested execution')
		expect(store.getPersistedRunSnapshot('run-parent')?.run.status).toBe('completed')
	})

	it('maps a child run without final payload to invalid_output on the parent orchestrator_agent node', async () => {
		const store = await createStore()
		const tempDir = path.dirname(store.database_path)
		const childAgentId = 'agent.phase9.child.none'
		const childAgentFile: AgentFile = {
			graph_contract_version: '1.0',
			meta: {
				id: childAgentId,
				name: 'Phase 9 Child Without Final Output',
			},
			entry_node_id: 'child-start',
			final_output: {
				mode: 'none',
			},
			nodes: [
				{
					id: 'child-start',
					kind: 'runtime_agent',
					runtime_adapter: 'codex',
					prompt: 'Return a child response.',
					input: {
						parts: [{ type: 'ref', ref: 'params.input' }],
					},
					output: TEXT_OUTPUT,
				},
			],
		}
		const childPath = await writeAgentFile(tempDir, 'child-agent-none.json', childAgentFile)
		const lifecycle = new AgentLifecycleService({ state_store: store })
		await lifecycle.registerAgentFile(childPath)
		await lifecycle.deployAgentFile(childPath)

		const parentAgentFile = buildOrchestratorEntryAgentFile(childAgentId)
		const { adapter } = createStubAdapter([
			{
				outcome: 'success',
				output: TEXT_OUTPUT,
				output_text: 'child-final',
			},
		])

		const result = await runAgentFile(
			parentAgentFile,
			adapter,
			{ topic: 'missing payload' },
			{
				state_store: store,
				resolved_revision_id: 'rev-parent-none',
				run_id: 'run-parent-none',
			},
		)

		expect(result).toMatchObject({
			status: 'failure',
			run_status: 'failed',
			code: 'CHILD_FINAL_OUTPUT_MISSING',
		})
		expect(store.getPersistedRunSnapshot('run-parent-none')?.attempts.at(-1)).toMatchObject({
			node_id: 'delegate',
			outcome: 'invalid_output',
		})
	})

	it('mirrors child invalid_output directly instead of inferring it from failure-code prefixes', async () => {
		const store = await createStore()
		const tempDir = path.dirname(store.database_path)
		const childAgentId = 'agent.phase9.child.invalid'
		const childAgentFile: AgentFile = {
			graph_contract_version: '1.0',
			meta: {
				id: childAgentId,
				name: 'Phase 9 Child Invalid Output',
			},
			entry_node_id: 'child-start',
			final_output: {
				mode: 'last_node_output',
			},
			nodes: [
				{
					id: 'child-start',
					kind: 'runtime_agent',
					runtime_adapter: 'codex',
					prompt: 'Return a child response.',
					input: {
						parts: [{ type: 'ref', ref: 'params.input' }],
					},
					output: TEXT_OUTPUT,
				},
			],
		}
		const childPath = await writeAgentFile(tempDir, 'child-agent-invalid.json', childAgentFile)
		const lifecycle = new AgentLifecycleService({ state_store: store })
		await lifecycle.registerAgentFile(childPath)
		await lifecycle.deployAgentFile(childPath)

		const parentAgentFile = buildOrchestratorEntryAgentFile(childAgentId)
		const { adapter } = createStubAdapter([
			{
				outcome: 'invalid_output',
				error: {
					code: 'CHILD_OUTPUT_REJECTED',
					message: 'Child payload failed validation.',
				},
			},
		])

		const result = await runAgentFile(
			parentAgentFile,
			adapter,
			{ topic: 'invalid child payload' },
			{
				state_store: store,
				resolved_revision_id: 'rev-parent-invalid-child',
				run_id: 'run-parent-invalid-child',
			},
		)

		expect(result).toMatchObject({
			status: 'failure',
			run_status: 'failed',
			code: 'CHILD_OUTPUT_REJECTED',
			message: expect.stringContaining('Child payload failed validation.'),
		})
		expect(
			store.getPersistedRunSnapshot('run-parent-invalid-child')?.attempts.at(-1),
		).toMatchObject({
			node_id: 'delegate',
			outcome: 'invalid_output',
		})
	})

	it('fails the parent orchestrator_agent node when the child logical agent has no live revision', async () => {
		const store = await createStore()
		const parentAgentFile = buildOrchestratorEntryAgentFile('agent.missing.live')
		const { adapter, requests } = createStubAdapter([])

		const result = await runAgentFile(
			parentAgentFile,
			adapter,
			{ topic: 'missing child' },
			{
				state_store: store,
				resolved_revision_id: 'rev-parent-missing',
				run_id: 'run-parent-missing',
			},
		)

		expect(result).toMatchObject({
			status: 'failure',
			run_status: 'failed',
			code: 'AGENT_NOT_FOUND',
		})
		expect(requests).toHaveLength(0)
		expect(store.getPersistedRunSnapshot('run-parent-missing')?.attempts.at(-1)).toMatchObject({
			node_id: 'delegate',
			outcome: 'runtime_error',
		})
	})

	it('rejects comments.enabled when no target nodes are declared', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		agentFile.interaction = {
			comments: {
				enabled: true,
			},
		}

		const { adapter, requests } = createStubAdapter([
			{
				outcome: 'success',
				output: JSON_OBJECT_OUTPUT,
				output_json: {
					summary: 'should-not-run',
					count: 1,
				},
			},
		])

		await expect(
			runAgentFile(
				agentFile,
				adapter,
				{ topic: 'interaction-contract' },
				{
					state_store: store,
					resolved_revision_id: 'rev-interaction-contract',
					run_id: 'run-interaction-contract',
				},
			),
		).rejects.toMatchObject({
			code: 'UNSUPPORTED_INTERACTION',
			message: 'interaction.comments.enabled requires at least one target_node_id.',
		} satisfies Pick<AppError, 'code' | 'message'>)
		expect(requests).toHaveLength(0)
		expect(store.getPersistedRunSnapshot('run-interaction-contract')).toBeNull()
	})

	it('routes live comments to targeted runtime nodes and threads the built-in MCP server name', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		agentFile.interaction = {
			comments: {
				enabled: true,
				target_node_ids: ['node-b'],
			},
			user_mcp: {
				enabled: true,
				server_name: 'orchestrator.user_chat',
			},
		}
		agentFile.chat = {
			prefer_native_resume: false,
			store_visible_messages: false,
			store_context_window: true,
			allow_fresh_start: false,
		}

		const { adapter, requests } = createStubAdapter(
			[
				{
					runtime_handle: {
						runtime: 'handle-node-a',
					},
					native_session_handle: {
						session: 'native-node-a',
					},
					terminal_result: {
						outcome: 'success',
						output: JSON_OBJECT_OUTPUT,
						output_json: {
							summary: 'phase-7',
							count: 2,
						},
					},
				},
				{
					runtime_handle: {
						runtime: 'handle-node-b',
					},
					native_session_handle: {
						session: 'native-node-b',
					},
					terminal_result: {
						outcome: 'success',
						output: TEXT_OUTPUT,
						output_text: 'done',
					},
				},
			],
			{
				supports_live_comments: true,
				supports_builtin_user_chat_mcp: true,
			},
		)

		const result = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'interaction-routing' },
			{
				state_store: store,
				resolved_revision_id: 'rev-interaction-routing',
				run_id: 'run-interaction-routing',
			},
		)

		expect(result).toEqual({
			status: 'success',
			run_id: 'run-interaction-routing',
			run_status: 'completed',
			final_output: 'done',
			final_output_mode: 'text',
			node_outputs: expect.any(Map),
		})
		expect(requests.map((request) => request.interaction)).toEqual([
			{
				comments_enabled: false,
				user_chat_server_name: 'orchestrator.user_chat',
			},
			{
				comments_enabled: true,
				user_chat_server_name: 'orchestrator.user_chat',
			},
		])
		expect(store.getPersistedRunSnapshot('run-interaction-routing')?.chat).toMatchObject({
			policy: {
				prefer_native_resume: false,
				store_visible_messages: false,
				store_context_window: true,
				allow_fresh_start: false,
			},
		})
		expect(store.getPersistedRunSnapshot('run-interaction-routing')?.attempts).toEqual([
			expect.objectContaining({
				node_id: 'node-a',
				runtime_handle: {
					runtime: 'handle-node-a',
				},
			}),
			expect.objectContaining({
				node_id: 'node-b',
				runtime_handle: {
					runtime: 'handle-node-b',
				},
			}),
		])
	})

	it('blocks on a real built-in user-chat prompt event and resumes from the stored reply', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		agentFile.interaction = {
			user_mcp: {
				enabled: true,
				server_name: 'orchestrator.user_chat',
			},
		}

		const { adapter, requests } = createStubAdapter(
			[
				{
					runtime_handle: {
						runtime: 'handle-prompt',
					},
					native_session_handle: {
						session: 'native-prompt',
					},
					terminal_result: new Promise<RuntimeTerminalResult>(() => undefined),
					events: singleEventStream({
						kind: 'user_chat_request',
						request_handle: {
							kind: 'codex_app_server_user_chat_request',
							threadId: 'thread-1',
							turnId: 'turn-1',
							itemId: 'tool-1',
							requestId: 42,
							prompt_id: 'prompt-1',
						},
						payload: {
							kind: 'text',
							prompt_id: 'prompt-1',
							text: 'Need approval?',
							require_response: true,
						},
					}),
				},
				{
					runtime_handle: {
						runtime: 'handle-resume-json',
					},
					native_session_handle: {
						session: 'native-resume-json',
					},
					terminal_result: {
						outcome: 'success',
						output: JSON_OBJECT_OUTPUT,
						output_json: {
							summary: 'resumed',
							count: 5,
						},
					},
				},
				{
					runtime_handle: {
						runtime: 'handle-resume-text',
					},
					native_session_handle: {
						session: 'native-resume-text',
					},
					terminal_result: {
						outcome: 'success',
						output: TEXT_OUTPUT,
						output_text: 'resumed-final',
					},
				},
			],
			{
				supports_builtin_user_chat_mcp: true,
			},
		)

		const blocked = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'prompt flow' },
			{
				state_store: store,
				resolved_revision_id: 'rev-prompt-flow',
				run_id: 'run-prompt-flow',
			},
		)

		expect(blocked).toEqual({
			status: 'waiting_for_user',
			run_id: 'run-prompt-flow',
			run_status: 'waiting_for_user',
			code: 'RUN_WAITING_FOR_USER',
			message: 'Run "run-prompt-flow" is blocked on user input from node "node-a".',
			resume_available: true,
		})

		const snapshot = store.getPersistedRunSnapshot('run-prompt-flow')
		expect(snapshot?.run.status).toBe('waiting_for_user')
		expect(snapshot?.resume.pending_prompt).toMatchObject({
			prompt_id: 'prompt-1',
			request_handle: {
				kind: 'codex_app_server_user_chat_request',
				threadId: 'thread-1',
				turnId: 'turn-1',
				itemId: 'tool-1',
				requestId: 42,
				prompt_id: 'prompt-1',
			},
		})

		store.recordUserPromptReply({
			run_id: 'run-prompt-flow',
			payload: {
				kind: 'text',
				prompt_id: 'prompt-1',
				text: 'Approved',
			},
		})

		const resumed = await resumeAgentRun(agentFile, adapter, 'run-prompt-flow', {
			state_store: store,
			resolved_revision_id: 'rev-prompt-flow',
		})

		expect(resumed).toEqual({
			status: 'success',
			run_id: 'run-prompt-flow',
			run_status: 'completed',
			final_output: 'resumed-final',
			final_output_mode: 'text',
			node_outputs: expect.any(Map),
		})
		expect(requests[1]?.interaction).toMatchObject({
			comments_enabled: false,
			user_chat_server_name: 'orchestrator.user_chat',
			user_chat_reply: {
				kind: 'text',
				prompt_id: 'prompt-1',
				text: 'Approved',
			},
		})
		expect(requests[1]?.input_message).toBe('Topic: prompt flow')
		expect(requests[2]?.input_message).toBe('Summary: resumed; Count: 5')
		expect(store.getPersistedRunSnapshot('run-prompt-flow')?.run.status).toBe('completed')
	})

	it('fails durably when edge condition code throws after a successful runtime turn', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		agentFile.edges = [
			{
				from: 'node-a',
				to: 'node-b',
				condition: {
					code: '(() => { throw new Error("edge boom"); })()',
				},
			},
		]

		const { adapter, requests } = createStubAdapter([
			{
				outcome: 'success',
				output: JSON_OBJECT_OUTPUT,
				output_json: {
					summary: 'phase-7',
					count: 2,
				},
			},
		])

		const result = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'edge-failure' },
			{
				state_store: store,
				resolved_revision_id: 'rev-edge-failure',
				run_id: 'run-edge-failure',
			},
		)

		expect(result).toEqual({
			status: 'failure',
			run_id: 'run-edge-failure',
			run_status: 'failed',
			code: 'EDGE_CONDITION_EVALUATION_FAILED',
			message: expect.stringContaining('edge boom'),
			resume_available: true,
		})
		expect(requests).toHaveLength(1)

		const snapshot = store.getPersistedRunSnapshot('run-edge-failure')
		expect(snapshot?.run.status).toBe('failed')
		expect(snapshot?.current_vars).toEqual({
			seed: 'stable',
		})
		expect(snapshot?.latest_committed_outputs).toEqual([])
		expect(snapshot?.attempts).toEqual([
			expect.objectContaining({
				node_id: 'node-a',
				state: 'committed_terminal',
				outcome: 'runtime_error',
			}),
		])
		expect(snapshot?.resume).toMatchObject({
			local_resume_available: true,
			last_durable_boundary_kind: 'node_attempt_terminal',
			pending_prompt: null,
		})
	})

	it('fails durably when node input resolution throws before any runtime execution begins', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		const firstNode = agentFile.nodes[0]
		if (firstNode?.kind !== 'runtime_agent') {
			throw new Error('expected runtime_agent test node')
		}
		firstNode.input = {
			parts: [
				{
					type: 'ref',
					ref: 'params.missing',
				},
			],
		}

		const { adapter, requests } = createStubAdapter([
			{
				outcome: 'success',
				output: JSON_OBJECT_OUTPUT,
				output_json: {
					summary: 'should-not-run',
					count: 1,
				},
			},
		])

		const result = await runAgentFile(
			agentFile,
			adapter,
			{ topic: 'pre-attempt-failure' },
			{
				state_store: store,
				resolved_revision_id: 'rev-pre-attempt-failure',
				run_id: 'run-pre-attempt-failure',
			},
		)

		expect(result).toEqual({
			status: 'failure',
			run_id: 'run-pre-attempt-failure',
			run_status: 'failed',
			code: 'RESOLUTION_ERROR',
			message: 'Node "node-a" could not be prepared for execution. Missing parameter "missing".',
			resume_available: true,
		})
		expect(requests).toHaveLength(0)

		const snapshot = store.getPersistedRunSnapshot('run-pre-attempt-failure')
		expect(snapshot?.run.status).toBe('failed')
		expect(snapshot?.current_vars).toEqual({
			seed: 'stable',
		})
		expect(snapshot?.latest_committed_outputs).toEqual([])
		expect(snapshot?.attempts).toEqual([
			expect.objectContaining({
				node_id: 'node-a',
				attempt_sequence: 1,
				state: 'committed_terminal',
				outcome: 'runtime_error',
			}),
		])
		expect(snapshot?.resume).toMatchObject({
			local_resume_available: true,
			last_durable_boundary_kind: 'node_attempt_terminal',
			pending_prompt: null,
		})
	})

	it('rejects built-in user-chat interaction when the adapter cannot support it', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		agentFile.interaction = {
			user_mcp: {
				enabled: true,
				server_name: 'orchestrator.user_chat',
			},
		}

		const { adapter, requests } = createStubAdapter([
			{
				outcome: 'success',
				output: JSON_OBJECT_OUTPUT,
				output_json: {
					summary: 'should-not-run',
					count: 1,
				},
			},
		])

		await expect(
			runAgentFile(
				agentFile,
				adapter,
				{ topic: 'interaction-policy' },
				{
					state_store: store,
					resolved_revision_id: 'rev-interaction-policy',
					run_id: 'run-interaction-policy',
				},
			),
		).rejects.toMatchObject({
			code: 'UNSUPPORTED_INTERACTION',
			message:
				'Agent file enables interaction.user_mcp, but the runtime adapter does not support built-in user-chat MCP.',
		} satisfies Pick<AppError, 'code' | 'message'>)
		expect(requests).toHaveLength(0)
		expect(store.getPersistedRunSnapshot('run-interaction-policy')).toBeNull()
	})

	it('rejects comments-enabled graphs on adapters without live-comment support before any node runs', async () => {
		const store = await createStore()
		const agentFile = buildAgentFile()
		agentFile.interaction = {
			comments: {
				enabled: true,
				target_node_ids: ['node-b'],
			},
		}

		const { adapter, requests } = createStubAdapter([
			{
				outcome: 'success',
				output: JSON_OBJECT_OUTPUT,
				output_json: {
					summary: 'should-not-run',
					count: 1,
				},
			},
		])

		await expect(
			runAgentFile(
				agentFile,
				adapter,
				{ topic: 'comments-unsupported' },
				{
					state_store: store,
					resolved_revision_id: 'rev-comments-unsupported',
					run_id: 'run-comments-unsupported',
				},
			),
		).rejects.toMatchObject({
			code: 'UNSUPPORTED_INTERACTION',
			message:
				'Agent file enables interaction.comments, but the runtime adapter does not support live comments.',
		} satisfies Pick<AppError, 'code' | 'message'>)
		expect(requests).toHaveLength(0)
		expect(store.getPersistedRunSnapshot('run-comments-unsupported')).toBeNull()
	})

	it('keeps comment delivery on the live runtime handle and reply delivery on the prompt handle', () => {
		const liveHandle = {
			kind: 'live-runtime-handle',
		}
		const replyHandle = {
			kind: 'reply-handle',
		}

		const snapshot = {
			resume: {
				native_session_handle: null,
				pending_prompt: {
					request_handle: replyHandle,
				},
			},
			attempts: [
				{
					node_id: 'node-a',
					state: 'blocked_wait',
					runtime_handle: liveHandle,
				},
			],
		}

		expect(resolveCommentExecutionHandle(snapshot)).toBe(liveHandle)
		expect(resolveReplyExecutionHandle(snapshot)).toBe(replyHandle)
	})

	it('rejects option replies whose value contradicts the selected option declaration', () => {
		try {
			buildOptionReplyPayload({
				runId: 'run-reply',
				promptId: 'prompt-1',
				promptPayload: {
					kind: 'options',
					options: [
						{
							id: 'option-a',
							value: {
								answer: 42,
							},
						},
					],
				},
				optionId: 'option-a',
				value: '{"answer":7}',
			})
			throw new Error('expected option reply validation to fail')
		} catch (caught) {
			expect(caught).toBeInstanceOf(AppError)
			expect(caught).toMatchObject({
				code: 'INVALID_REPLY',
				message: 'Option "option-a" must use the declared value for the selected prompt option.',
			})
		}
	})
})
