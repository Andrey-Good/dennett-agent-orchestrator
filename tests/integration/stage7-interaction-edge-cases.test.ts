import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { CodexAppServerRuntimeAdapter } from '../../src/adapters/codex/codex-app-server-runtime-adapter.js'
import type { AgentFile } from '../../src/core/agent-file.js'
import { AppError } from '../../src/core/errors.js'
import { resumeAgentRun, runAgentFile } from '../../src/core/graph-runner.js'
import type { JsonObject, JsonValue } from '../../src/core/json.js'
import { computeResolvedRevisionId } from '../../src/core/resolved-revision.js'
import { SQLiteLocalStateStore } from '../../src/core/state/index.js'
import { buildCliProgram } from '../../src/interfaces/cli.js'
import type {
	RuntimeAdapter,
	RuntimeAdapterCapabilities,
	RuntimeAdapterExecutionRequest,
	RuntimeEnvironmentInspectionResult,
	RuntimeEvent,
	RuntimeExecutionSession,
	RuntimeModelCatalogPage,
	RuntimeSourceInspectionResult,
	RuntimeSourceSelection,
	RuntimeTerminalResult,
	UserChatResponsePayload,
} from '../../src/ports/runtime.js'

const TEXT_OUTPUT = { mode: 'text' } as const
const TARGET_AGENT_ID = 'agent.stage7.interaction.edge'
const RUN_ID = 'run-stage7-interaction-edge'
const PROMPT_ID = 'stage7-edge-approval'
const TOPIC = 'Stage 7 interaction edge case'

const tempDirsToRemove: string[] = []
const storesToClose: SQLiteLocalStateStore[] = []

const DEFAULT_CAPABILITIES = {
	supports_native_resume: true,
	supports_live_comments: true,
	supports_builtin_user_chat_mcp: true,
	supports_memory_bindings: true,
	supports_model_discovery: false,
	supports_runtime_environment_introspection: false,
	supports_reasoning_effort: true,
	supports_speed_tiers: true,
	supports_personality: true,
	supports_explicit_runtime_source: false,
	supports_runtime_source_introspection: false,
} satisfies RuntimeAdapterCapabilities

type CliResult = {
	stdout: string
	stderr: string
	exitCode: string | number | undefined
}

interface StubExecutionDescriptor {
	runtime_handle?: JsonValue | null
	native_session_handle?: JsonValue | null
	terminal_result: RuntimeTerminalResult | Promise<RuntimeTerminalResult>
	events?: AsyncIterable<RuntimeEvent>
}

class StubRuntimeAdapter implements RuntimeAdapter {
	readonly requests: RuntimeAdapterExecutionRequest[] = []

	readonly deliveredReplies: Array<{ execution: unknown; response: UserChatResponsePayload }> = []

	constructor(
		private readonly sessions: StubExecutionDescriptor[],
		private readonly capabilities: RuntimeAdapterCapabilities = DEFAULT_CAPABILITIES,
	) {}

	describeCapabilities(): RuntimeAdapterCapabilities {
		return this.capabilities
	}

	async startExecution(request: RuntimeAdapterExecutionRequest): Promise<RuntimeExecutionSession> {
		this.requests.push(request)
		const next = this.sessions.shift()
		if (!next) {
			throw new Error('Unexpected runtime execution launch.')
		}

		return {
			runtime_handle: next.runtime_handle ?? null,
			native_session_handle: next.native_session_handle ?? null,
			terminal_result: Promise.resolve(next.terminal_result),
			events: next.events ?? emptyEventStream(),
		}
	}

	async deliverUserChatResponse(
		execution: unknown,
		response: UserChatResponsePayload,
	): Promise<void> {
		this.deliveredReplies.push({ execution, response })
	}

	async deliverComment(): Promise<void> {
		throw new Error('Comments are not used by this offline interaction edge-case fixture.')
	}

	async cancelExecution(): Promise<void> {
		throw new Error('Cancellation is not used by this offline interaction edge-case fixture.')
	}

	async listModels(): Promise<RuntimeModelCatalogPage> {
		throw new Error('Model discovery is not used by this offline interaction edge-case fixture.')
	}

	async inspectRuntimeEnvironment(): Promise<RuntimeEnvironmentInspectionResult> {
		throw new Error('Runtime inspection is not used by this offline interaction edge-case fixture.')
	}

	async inspectRuntimeSource(
		_source: RuntimeSourceSelection,
	): Promise<RuntimeSourceInspectionResult> {
		throw new Error('Runtime source inspection is not used by this offline edge-case fixture.')
	}

	enqueueSession(session: StubExecutionDescriptor): void {
		this.sessions.push(session)
	}
}

function buildInteractionAgent(
	runtimeOptions: JsonObject = { model: 'edge-model-stable' },
): AgentFile {
	return {
		graph_contract_version: '1.0',
		meta: {
			id: TARGET_AGENT_ID,
			name: 'Stage 7 Interaction Edge Cases',
			description: 'Offline fixture for deterministic interaction edge-case coverage.',
		},
		entry_node_id: 'ask',
		params: {
			topic: {
				type: 'string',
				required: true,
			},
		},
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
		final_output: {
			mode: 'last_node_output',
		},
		nodes: [
			{
				id: 'ask',
				kind: 'runtime_agent',
				runtime_adapter: 'codex',
				prompt: 'Ask for approval, then return the approved edge-case result.',
				runtime_options: runtimeOptions,
				input: {
					parts: [
						{
							type: 'ref',
							ref: 'params.topic',
						},
					],
				},
				output: TEXT_OUTPUT,
			},
		],
	}
}

function emptyEventStream(): AsyncIterable<RuntimeEvent> {
	return {
		async *[Symbol.asyncIterator]() {
			// Offline fixture intentionally emits no runtime events.
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

function pendingTerminalResult(): Promise<RuntimeTerminalResult> {
	return new Promise<RuntimeTerminalResult>(() => undefined)
}

function promptSession(promptId = PROMPT_ID): StubExecutionDescriptor {
	return {
		runtime_handle: {
			threadId: 'stage7-edge-thread',
			turnId: 'stage7-edge-turn',
		},
		native_session_handle: {
			threadId: 'stage7-edge-thread',
		},
		terminal_result: pendingTerminalResult(),
		events: singleEventStream({
			kind: 'user_chat_request',
			request_handle: {
				kind: 'codex_app_server_user_chat_request',
				threadId: 'stage7-edge-thread',
				turnId: 'stage7-edge-turn',
				itemId: 'stage7-edge-tool',
				requestId: 428,
				prompt_id: promptId,
			},
			payload: {
				kind: 'text',
				prompt_id: promptId,
				text: 'Approve the deterministic Stage 7 edge-case flow?',
				require_response: true,
			},
		}),
	}
}

function successSession(outputText: string): StubExecutionDescriptor {
	return {
		runtime_handle: {
			threadId: 'stage7-edge-thread',
			turnId: 'stage7-edge-resume-turn',
		},
		native_session_handle: {
			threadId: 'stage7-edge-thread',
		},
		terminal_result: {
			outcome: 'success',
			output: TEXT_OUTPUT,
			output_text: outputText,
		},
	}
}

async function createHarness(agentFile = buildInteractionAgent()): Promise<{
	agentFile: AgentFile
	agentFilePath: string
	resolvedRevisionId: string
	stateDbPath: string
	stateStore: SQLiteLocalStateStore
}> {
	const tempDir = await mkdtemp(path.join(os.tmpdir(), 'dennett-stage7-edge-cases-'))
	tempDirsToRemove.push(tempDir)
	const agentFilePath = path.join(tempDir, 'agent.json')
	await writeAgentFile(agentFilePath, agentFile)
	const resolvedRevisionId = await computeResolvedRevisionId(agentFilePath)
	const stateDbPath = path.join(tempDir, 'local-state.sqlite')
	const stateStore = new SQLiteLocalStateStore({ database_path: stateDbPath })
	storesToClose.push(stateStore)

	return {
		agentFile,
		agentFilePath,
		resolvedRevisionId,
		stateDbPath,
		stateStore,
	}
}

async function writeAgentFile(agentFilePath: string, agentFile: AgentFile): Promise<void> {
	await writeFile(agentFilePath, `${JSON.stringify(agentFile, null, '\t')}\n`)
}

async function startWaitingRun(args: {
	agentFile: AgentFile
	resolvedRevisionId: string
	stateStore: SQLiteLocalStateStore
}): Promise<StubRuntimeAdapter> {
	const adapter = new StubRuntimeAdapter([promptSession()])
	const result = await runAgentFile(
		args.agentFile,
		adapter,
		{
			topic: TOPIC,
		},
		{
			state_store: args.stateStore,
			resolved_revision_id: args.resolvedRevisionId,
			run_id: RUN_ID,
		},
	)

	expect(result).toMatchObject({
		status: 'waiting_for_user',
		run_id: RUN_ID,
		code: 'RUN_WAITING_FOR_USER',
	})
	expect(args.stateStore.getPersistedRunSnapshot(RUN_ID)?.resume.pending_prompt).toMatchObject({
		prompt_id: PROMPT_ID,
	})

	return adapter
}

async function runCli(args: string[]): Promise<CliResult> {
	const originalExitCode = process.exitCode
	let stdout = ''
	let stderr = ''
	const stdoutSpy = vi.spyOn(process.stdout, 'write').mockImplementation((chunk) => {
		stdout += String(chunk)
		return true
	})
	const stderrSpy = vi.spyOn(process.stderr, 'write').mockImplementation((chunk) => {
		stderr += String(chunk)
		return true
	})

	try {
		process.exitCode = undefined
		const program = buildCliProgram()
		program.exitOverride()
		await program.parseAsync(args, { from: 'user' })

		return {
			stdout,
			stderr,
			exitCode: process.exitCode,
		}
	} finally {
		stdoutSpy.mockRestore()
		stderrSpy.mockRestore()
		process.exitCode = originalExitCode
	}
}

async function expectCliAppError(args: string[]): Promise<AppError> {
	try {
		await runCli(args)
	} catch (error) {
		if (error instanceof AppError) {
			return error
		}
		throw error
	}
	throw new Error('Expected CLI command to fail with AppError.')
}

async function recordCliTextReply(args: {
	agentFilePath: string
	stateDbPath: string
	text: string
}): Promise<CliResult> {
	return await runCli([
		'reply',
		args.agentFilePath,
		'--run-id',
		RUN_ID,
		'--prompt-id',
		PROMPT_ID,
		'--text',
		args.text,
		'--state-db',
		args.stateDbPath,
	])
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

describe('Stage 7 interaction edge cases', () => {
	it('rejects late replies after the prompt run has completed', async () => {
		const harness = await createHarness()
		const adapter = await startWaitingRun(harness)
		harness.stateStore.recordUserPromptReply({
			run_id: RUN_ID,
			payload: {
				kind: 'text',
				prompt_id: PROMPT_ID,
				text: 'Approve before completion.',
			},
		})
		adapter.enqueueSession(successSession('Completed before the late reply.'))

		const resumeResult = await resumeAgentRun(harness.agentFile, adapter, RUN_ID, {
			state_store: harness.stateStore,
			resolved_revision_id: harness.resolvedRevisionId,
		})
		expect(resumeResult).toMatchObject({
			status: 'success',
			run_status: 'completed',
			final_output: 'Completed before the late reply.',
		})

		const messagesBeforeLateReply = harness.stateStore.listVisibleChatMessages(RUN_ID)
		const error = await expectCliAppError([
			'reply',
			harness.agentFilePath,
			'--run-id',
			RUN_ID,
			'--prompt-id',
			PROMPT_ID,
			'--text',
			'This answer arrived too late.',
			'--state-db',
			harness.stateDbPath,
		])

		expect(error.code).toBe('RUN_NOT_ACTIVE')
		expect(error.message).toContain(`Run "${RUN_ID}" is "completed"`)
		expect(harness.stateStore.listVisibleChatMessages(RUN_ID)).toEqual(messagesBeforeLateReply)
		expect(harness.stateStore.getPersistedRunSnapshot(RUN_ID)?.resume.pending_prompt).toBeNull()
	})

	it('records duplicate prompt replies idempotently until resume consumes the first-class reply', async () => {
		const harness = await createHarness()
		const adapter = await startWaitingRun(harness)
		const deliveredReplies: Array<{ execution: unknown; response: UserChatResponsePayload }> = []
		vi.spyOn(CodexAppServerRuntimeAdapter.prototype, 'deliverUserChatResponse').mockImplementation(
			async (execution, response) => {
				deliveredReplies.push({ execution, response })
			},
		)

		const firstReply = await recordCliTextReply({
			agentFilePath: harness.agentFilePath,
			stateDbPath: harness.stateDbPath,
			text: 'Duplicate approval.',
		})
		const duplicateReply = await recordCliTextReply({
			agentFilePath: harness.agentFilePath,
			stateDbPath: harness.stateDbPath,
			text: 'Duplicate approval.',
		})
		adapter.enqueueSession(successSession('Completed after duplicate approval.'))

		const resumeResult = await resumeAgentRun(harness.agentFile, adapter, RUN_ID, {
			state_store: harness.stateStore,
			resolved_revision_id: harness.resolvedRevisionId,
		})
		const snapshot = harness.stateStore.getPersistedRunSnapshot(RUN_ID)
		const userMessages = snapshot?.visible_messages.filter(
			(message) => message.kind === 'user_message',
		)

		expect(firstReply.stdout).toBe('Prompt reply delivered.\n')
		expect(duplicateReply.stdout).toBe('Prompt reply already recorded.\n')
		expect(deliveredReplies).toHaveLength(1)
		expect(userMessages).toEqual([
			expect.objectContaining({
				message_sequence: 2,
				payload: {
					kind: 'text',
					prompt_id: PROMPT_ID,
					text: 'Duplicate approval.',
				},
			}),
		])
		expect(adapter.requests[1]?.interaction.user_chat_reply).toEqual({
			kind: 'text',
			prompt_id: PROMPT_ID,
			text: 'Duplicate approval.',
		})
		expect(resumeResult).toMatchObject({
			status: 'success',
			final_output: 'Completed after duplicate approval.',
		})
		expect(snapshot?.resume.pending_prompt).toBeNull()
	})

	it('rejects conflicting duplicate replies before resume', async () => {
		const harness = await createHarness()
		const adapter = await startWaitingRun(harness)
		vi.spyOn(CodexAppServerRuntimeAdapter.prototype, 'deliverUserChatResponse').mockImplementation(
			async () => undefined,
		)

		await recordCliTextReply({
			agentFilePath: harness.agentFilePath,
			stateDbPath: harness.stateDbPath,
			text: 'Older approval.',
		})
		const error = await expectCliAppError([
			'reply',
			harness.agentFilePath,
			'--run-id',
			RUN_ID,
			'--prompt-id',
			PROMPT_ID,
			'--text',
			'Newer approval.',
			'--state-db',
			harness.stateDbPath,
		])
		adapter.enqueueSession(successSession('Completed after superseded reply.'))

		const resumeResult = await resumeAgentRun(harness.agentFile, adapter, RUN_ID, {
			state_store: harness.stateStore,
			resolved_revision_id: harness.resolvedRevisionId,
		})
		const snapshot = harness.stateStore.getPersistedRunSnapshot(RUN_ID)

		expect(adapter.requests[1]?.interaction.user_chat_reply).toEqual({
			kind: 'text',
			prompt_id: PROMPT_ID,
			text: 'Older approval.',
		})
		expect(error.code).toBe('PROMPT_REPLY_ALREADY_RECORDED')
		expect(snapshot?.visible_messages.map((message) => message.payload)).toEqual([
			expect.objectContaining({
				kind: 'text',
				prompt_id: PROMPT_ID,
				require_response: true,
			}),
			{
				kind: 'text',
				prompt_id: PROMPT_ID,
				text: 'Older approval.',
			},
		])
		expect(resumeResult).toMatchObject({
			status: 'success',
			final_output: 'Completed after superseded reply.',
		})
		expect(snapshot?.resume.pending_prompt).toBeNull()
	})

	it('defers risky mid-run model changes by rejecting changed revision resumes', async () => {
		const originalAgent = buildInteractionAgent({ model: 'edge-model-original' })
		const harness = await createHarness(originalAgent)
		const adapter = await startWaitingRun(harness)
		const changedAgent = buildInteractionAgent({ model: 'edge-model-changed' })
		await writeAgentFile(harness.agentFilePath, changedAgent)
		const changedResolvedRevisionId = await computeResolvedRevisionId(harness.agentFilePath)

		await expect(
			resumeAgentRun(changedAgent, adapter, RUN_ID, {
				state_store: harness.stateStore,
				resolved_revision_id: changedResolvedRevisionId,
			}),
		).rejects.toMatchObject({
			code: 'RESUME_REVISION_MISMATCH',
		})

		const snapshot = harness.stateStore.getPersistedRunSnapshot(RUN_ID)
		expect(changedResolvedRevisionId).not.toBe(harness.resolvedRevisionId)
		expect(adapter.requests).toHaveLength(1)
		expect(snapshot?.run).toMatchObject({
			status: 'waiting_for_user',
			resolved_revision_id: harness.resolvedRevisionId,
		})
		expect(snapshot?.resume.pending_prompt).toMatchObject({
			prompt_id: PROMPT_ID,
		})
	})

	it('reports changed revision resumes through the CLI without launching runtime work', async () => {
		const originalAgent = buildInteractionAgent({ model: 'edge-model-original' })
		const harness = await createHarness(originalAgent)
		await startWaitingRun(harness)
		const changedAgent = buildInteractionAgent({ model: 'edge-model-changed' })
		await writeAgentFile(harness.agentFilePath, changedAgent)
		const startExecutionSpy = vi.spyOn(
			CodexAppServerRuntimeAdapter.prototype,
			'startExecution',
		)

		const error = await expectCliAppError([
			'resume',
			harness.agentFilePath,
			'--run-id',
			RUN_ID,
			'--state-db',
			harness.stateDbPath,
		])

		expect(error.code).toBe('RESUME_REVISION_MISMATCH')
		expect(error.message).toContain(`Run "${RUN_ID}" is pinned to`)
		expect(startExecutionSpy).not.toHaveBeenCalled()
		expect(harness.stateStore.getPersistedRunSnapshot(RUN_ID)?.run).toMatchObject({
			status: 'waiting_for_user',
			resolved_revision_id: harness.resolvedRevisionId,
		})
	})
})
