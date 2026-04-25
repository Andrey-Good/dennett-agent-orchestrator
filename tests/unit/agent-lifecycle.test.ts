import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { AgentLifecycleService } from '../../src/core/agent-lifecycle.js'
import type { AppError } from '../../src/core/errors.js'
import { SQLiteLocalStateStore } from '../../src/core/state/index.js'
import {
	buildCliProgram,
	dispatchTriggerEvent,
	registerLifecycleTrigger,
	runLiveAgentByLogicalId,
} from '../../src/interfaces/cli.js'
import type {
	RuntimeAdapter,
	RuntimeAdapterCapabilities,
	RuntimeAdapterExecutionRequest,
	RuntimeEvent,
	RuntimeExecutionSession,
	RuntimeTerminalResult,
} from '../../src/ports/runtime.js'

const storesToClose: SQLiteLocalStateStore[] = []
const tempDirsToRemove: string[] = []

function emptyEventStream(): AsyncIterable<RuntimeEvent> {
	return {
		async *[Symbol.asyncIterator]() {
			// Intentionally empty.
		},
	}
}

async function createStore(): Promise<SQLiteLocalStateStore> {
	const tempDir = await mkdtemp(path.join(os.tmpdir(), 'dennett-phase8-lifecycle-'))
	const store = new SQLiteLocalStateStore({
		database_path: path.join(tempDir, 'local-state.sqlite'),
	})
	storesToClose.push(store)
	tempDirsToRemove.push(tempDir)
	return store
}

async function createAgentFile(
	tempDir: string,
	agentId = 'agent.lifecycle.phase8',
	inputPrefix = 'Live input: ',
): Promise<string> {
	const filePath = path.join(tempDir, 'agent.json')
	const agentFile = {
		graph_contract_version: '1.0',
		meta: {
			id: agentId,
			name: 'Phase 8 Lifecycle Test Agent',
			description: 'Lifecycle test agent',
			agent_version: '1.0.0',
		},
		entry_node_id: 'start',
		final_output: {
			mode: 'last_node_output',
		},
		nodes: [
			{
				id: 'start',
				kind: 'runtime_agent',
				runtime_adapter: 'codex',
				prompt: 'Return a text response.',
				input: {
					parts: [
						{
							type: 'text',
							text: inputPrefix,
						},
						{
							type: 'ref',
							ref: 'params.topic',
						},
					],
				},
				output: {
					mode: 'text',
				},
			},
		],
	}

	await writeFile(filePath, `${JSON.stringify(agentFile, null, 2)}\n`, 'utf8')
	return filePath
}

async function writeAgentJson(
	tempDir: string,
	fileName: string,
	agentFile: unknown,
): Promise<string> {
	const filePath = path.join(tempDir, fileName)
	await writeFile(filePath, `${JSON.stringify(agentFile, null, 2)}\n`, 'utf8')
	return filePath
}

function createStubAdapter(
	results: RuntimeTerminalResult[],
	capabilityOverrides: Partial<RuntimeAdapterCapabilities> = {},
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
				throw new Error('No more results configured for test adapter.')
			}
			const executionSession: RuntimeExecutionSession = {
				runtime_handle: null,
				native_session_handle: null,
				terminal_result: Promise.resolve(next),
				events: emptyEventStream(),
			}
			return executionSession
		},
		async listModels() {
			throw new Error('not used')
		},
		async inspectRuntimeEnvironment() {
			throw new Error('not used')
		},
		async inspectRuntimeSource() {
			throw new Error('not used')
		},
		async deliverComment() {
			throw new Error('not used')
		},
		async deliverUserChatResponse() {
			throw new Error('not used')
		},
		async cancelExecution() {
			throw new Error('not used')
		},
	}

	return { adapter, requests }
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

describe('AgentLifecycleService', () => {
	it('keeps omitted --runtime-source-id undefined on CLI launch commands and still collects explicit ids', async () => {
		const cases = [
			{
				commandName: 'run',
				omittedArgs: ['run', 'agent.json'],
				explicitArgs: [
					'run',
					'agent.json',
					'--runtime-source-id',
					'source-a',
					'--runtime-source-id',
					'source-b',
				],
			},
			{
				commandName: 'run-live',
				omittedArgs: ['run-live', 'agent.logical'],
				explicitArgs: [
					'run-live',
					'agent.logical',
					'--runtime-source-id',
					'source-a',
					'--runtime-source-id',
					'source-b',
				],
			},
			{
				commandName: 'event-dispatch',
				omittedArgs: ['event-dispatch', 'trigger.id'],
				explicitArgs: [
					'event-dispatch',
					'trigger.id',
					'--runtime-source-id',
					'source-a',
					'--runtime-source-id',
					'source-b',
				],
			},
		] as const

		for (const testCase of cases) {
			let omittedOptions:
				| {
						runtimeSourceId?: string[]
				  }
				| undefined
			const omittedProgram = buildCliProgram()
			omittedProgram.exitOverride()
			const omittedCommand = omittedProgram.commands.find(
				(entry) => entry.name() === testCase.commandName,
			)
			if (!omittedCommand) {
				throw new Error(`expected CLI command ${testCase.commandName}`)
			}
			omittedCommand.action(async (...args: unknown[]) => {
				omittedOptions = args.at(-2) as { runtimeSourceId?: string[] }
			})

			await omittedProgram.parseAsync(testCase.omittedArgs, { from: 'user' })
			expect(omittedOptions?.runtimeSourceId).toBeUndefined()

			let explicitOptions:
				| {
						runtimeSourceId?: string[]
				  }
				| undefined
			const explicitProgram = buildCliProgram()
			explicitProgram.exitOverride()
			const explicitCommand = explicitProgram.commands.find(
				(entry) => entry.name() === testCase.commandName,
			)
			if (!explicitCommand) {
				throw new Error(`expected CLI command ${testCase.commandName}`)
			}
			explicitCommand.action(async (...args: unknown[]) => {
				explicitOptions = args.at(-2) as { runtimeSourceId?: string[] }
			})

			await explicitProgram.parseAsync(testCase.explicitArgs, { from: 'user' })
			expect(explicitOptions?.runtimeSourceId).toEqual(['source-a', 'source-b'])
		}
	})

	it('registers a valid agent under its logical id without making it live', async () => {
		const store = await createStore()
		const tempDir = path.dirname(store.database_path)
		const sourcePath = await createAgentFile(tempDir, 'agent.lifecycle.register')
		const lifecycle = new AgentLifecycleService({
			state_store: store,
		})

		const result = await lifecycle.registerAgentFile(sourcePath)

		expect(result.logical_agent_id).toBe('agent.lifecycle.register')
		expect(result.revision.revision_kind).toBe('draft')
		expect(result.revision.availability_state).toBe('available')
		expect(result.status.agent.live_revision_id).toBeNull()
		expect(result.status.draft_revisions).toHaveLength(1)
		expect(result.status.draft_revisions[0]).toMatchObject({
			logical_agent_id: 'agent.lifecycle.register',
			revision_kind: 'draft',
			availability_state: 'available',
		})
	})

	it('deploys a live revision and resolves logical-id runs from live bytes, not later draft edits', async () => {
		const store = await createStore()
		const tempDir = path.dirname(store.database_path)
		const sourcePath = await createAgentFile(tempDir, 'agent.lifecycle.deploy', 'Live input: ')
		const lifecycle = new AgentLifecycleService({
			state_store: store,
		})

		await lifecycle.registerAgentFile(sourcePath)
		const deployResult = await lifecycle.deployAgentFile(sourcePath)
		const deployedLivePath = deployResult.live_file_path

		await writeFile(
			sourcePath,
			`${JSON.stringify(
				{
					graph_contract_version: '1.0',
					meta: {
						id: 'agent.lifecycle.deploy',
						name: 'Phase 8 Lifecycle Test Agent',
						description: 'Lifecycle test agent',
						agent_version: '1.0.0',
					},
					entry_node_id: 'start',
					final_output: {
						mode: 'last_node_output',
					},
					nodes: [
						{
							id: 'start',
							kind: 'runtime_agent',
							runtime_adapter: 'codex',
							prompt: 'Return a text response.',
							input: {
								parts: [
									{
										type: 'text',
										text: 'Draft input: ',
									},
									{
										type: 'ref',
										ref: 'params.topic',
									},
								],
							},
							output: {
								mode: 'text',
							},
						},
					],
				},
				null,
				2,
			)}\n`,
			'utf8',
		)

		const { adapter, requests } = createStubAdapter([
			{
				outcome: 'success',
				output: {
					mode: 'text',
				},
				output_text: 'live-result',
			},
		])

		const result = await runLiveAgentByLogicalId(
			'agent.lifecycle.deploy',
			path.join(tempDir, 'local-state.sqlite'),
			{
				topic: 'phase-8',
			},
			{
				adapter,
			},
		)

		expect(result).toMatchObject({
			status: 'success',
			run_status: 'completed',
			final_output: 'live-result',
			final_output_mode: 'text',
		})
		expect(requests).toHaveLength(1)
		expect(requests[0]?.input_message).toBe('Live input: phase-8')

		const liveFileText = await readFile(deployedLivePath, 'utf8')
		expect(liveFileText).toContain('Live input: ')
		expect(liveFileText).not.toContain('Draft input: ')
	})

	it('threads user runtime-source narrowing through runLiveAgentByLogicalId before launch', async () => {
		const store = await createStore()
		const tempDir = path.dirname(store.database_path)
		const agentId = 'agent.lifecycle.runtime-source'
		const sourcePath = await writeAgentJson(tempDir, 'runtime-source-agent.json', {
			graph_contract_version: '1.0',
			meta: {
				id: agentId,
				name: 'Runtime Source Agent',
			},
			entry_node_id: 'start',
			final_output: {
				mode: 'last_node_output',
			},
			runtime_sources: [
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
			],
			nodes: [
				{
					id: 'start',
					kind: 'runtime_agent',
					runtime_adapter: 'codex',
					prompt: 'Return a text response.',
					input: {
						parts: [
							{
								type: 'text',
								text: 'Live input: ',
							},
							{
								type: 'ref',
								ref: 'params.topic',
							},
						],
					},
					output: {
						mode: 'text',
					},
				},
			],
		})
		const lifecycle = new AgentLifecycleService({
			state_store: store,
		})

		await lifecycle.registerAgentFile(sourcePath)
		await lifecycle.deployAgentFile(sourcePath)

		const { adapter, requests } = createStubAdapter(
			[
				{
					outcome: 'success',
					output: {
						mode: 'text',
					},
					output_text: 'live-result',
				},
			],
			{
				supports_explicit_runtime_source: true,
			},
		)

		const result = await runLiveAgentByLogicalId(
			agentId,
			store.database_path,
			{
				topic: 'phase-9',
			},
			{
				adapter,
				runtimeSourceIds: ['source-b'],
			},
		)

		expect(result).toMatchObject({
			status: 'success',
			final_output: 'live-result',
		})
		expect(requests[0]?.runtime_source).toEqual({
			id: 'source-b',
			runtime_adapter: 'codex',
			source_ref: 'workspace://source-b',
		})
	})

	it('reports out-of-band live mutation as conflicted and preserves the prior live after a failed deploy', async () => {
		const store = await createStore()
		const tempDir = path.dirname(store.database_path)
		const sourcePath = await createAgentFile(tempDir, 'agent.lifecycle.conflict', 'Live input: ')
		const lifecycle = new AgentLifecycleService({
			state_store: store,
		})

		await lifecycle.registerAgentFile(sourcePath)
		const firstDeploy = await lifecycle.deployAgentFile(sourcePath)

		await writeFile(
			firstDeploy.live_file_path,
			`${JSON.stringify(
				{
					graph_contract_version: '1.0',
					meta: {
						id: 'agent.lifecycle.conflict',
						name: 'Phase 8 Lifecycle Test Agent',
						description: 'Mutated live agent',
						agent_version: '1.0.1',
					},
					entry_node_id: 'start',
					final_output: {
						mode: 'last_node_output',
					},
					nodes: [
						{
							id: 'start',
							kind: 'runtime_agent',
							runtime_adapter: 'codex',
							prompt: 'Return a text response.',
							input: {
								parts: [
									{
										type: 'text',
										text: 'Mutated live input: ',
									},
									{
										type: 'ref',
										ref: 'params.topic',
									},
								],
							},
							output: {
								mode: 'text',
							},
						},
					],
				},
				null,
				2,
			)}\n`,
			'utf8',
		)

		const conflictedStatus = await lifecycle.getAgentStatus('agent.lifecycle.conflict')
		expect(conflictedStatus.live_revision?.availability_state).toBe('conflicted')
		await expect(lifecycle.resolveLiveAgentFile('agent.lifecycle.conflict')).rejects.toMatchObject({
			code: 'AGENT_LIVE_UNAVAILABLE',
		} satisfies Partial<AppError>)

		await writeFile(
			sourcePath,
			`${JSON.stringify(
				{
					graph_contract_version: '1.0',
					meta: {
						id: 'agent.lifecycle.conflict',
						name: 'Phase 8 Lifecycle Test Agent',
						description: 'Broken draft',
						agent_version: '1.0.2',
					},
					entry_node_id: 'start',
					final_output: {
						mode: 'last_node_output',
					},
					nodes: [],
				},
				null,
				2,
			)}\n`,
			'utf8',
		)

		await expect(lifecycle.deployAgentFile(sourcePath)).rejects.toThrow()
		const statusAfterFailedDeploy = await lifecycle.getAgentStatus('agent.lifecycle.conflict')
		expect(statusAfterFailedDeploy.agent.live_revision_id).toBe(firstDeploy.revision.revision_id)
		expect(statusAfterFailedDeploy.live_revision?.availability_state).toBe('conflicted')
	})

	it('marks a previously live revision as missing when the published file disappears', async () => {
		const store = await createStore()
		const tempDir = path.dirname(store.database_path)
		const sourcePath = await createAgentFile(
			tempDir,
			'agent.lifecycle.missing-live',
			'Live input: ',
		)
		const lifecycle = new AgentLifecycleService({
			state_store: store,
		})

		await lifecycle.registerAgentFile(sourcePath)
		const deployResult = await lifecycle.deployAgentFile(sourcePath)

		await rm(deployResult.live_file_path, { force: true })

		const status = await lifecycle.getAgentStatus('agent.lifecycle.missing-live')
		expect(status.live_revision?.availability_state).toBe('missing')
		expect(status.live_revision?.validation_error).toContain(deployResult.live_file_path)
		await expect(
			lifecycle.resolveLiveAgentFile('agent.lifecycle.missing-live'),
		).rejects.toMatchObject({
			code: 'AGENT_LIVE_UNAVAILABLE',
		} satisfies Partial<AppError>)
	})

	it('marks a previously live revision as invalid when the published file stops validating', async () => {
		const store = await createStore()
		const tempDir = path.dirname(store.database_path)
		const sourcePath = await createAgentFile(
			tempDir,
			'agent.lifecycle.invalid-live',
			'Live input: ',
		)
		const lifecycle = new AgentLifecycleService({
			state_store: store,
		})

		await lifecycle.registerAgentFile(sourcePath)
		const deployResult = await lifecycle.deployAgentFile(sourcePath)

		await writeFile(
			deployResult.live_file_path,
			`${JSON.stringify(
				{
					graph_contract_version: '1.0',
					meta: {
						id: 'agent.lifecycle.invalid-live',
						name: 'Broken Live Agent',
					},
					entry_node_id: 'missing-node',
					final_output: {
						mode: 'last_node_output',
					},
					nodes: [],
				},
				null,
				2,
			)}\n`,
			'utf8',
		)

		const status = await lifecycle.getAgentStatus('agent.lifecycle.invalid-live')
		expect(status.live_revision?.availability_state).toBe('invalid')
		expect(status.live_revision?.validation_error).toBeTruthy()
		await expect(
			lifecycle.resolveLiveAgentFile('agent.lifecycle.invalid-live'),
		).rejects.toMatchObject({
			code: 'AGENT_LIVE_UNAVAILABLE',
		} satisfies Partial<AppError>)
	})

	it('dispatches a trigger-backed event through the current live revision and persists event metadata', async () => {
		const store = await createStore()
		const tempDir = path.dirname(store.database_path)
		const agentId = 'agent.lifecycle.event'
		const sourcePath = await writeAgentJson(tempDir, 'event-agent.json', {
			graph_contract_version: '1.0',
			meta: {
				id: agentId,
				name: 'Event Launch Agent',
			},
			entry_node_id: 'start',
			final_output: {
				mode: 'last_node_output',
			},
			nodes: [
				{
					id: 'start',
					kind: 'runtime_agent',
					runtime_adapter: 'codex',
					prompt: 'Return a text response.',
					input: {
						parts: [
							{ type: 'text', text: 'Event input: ' },
							{ type: 'ref', ref: 'event.payload.topic' },
							{ type: 'text', text: ' / ' },
							{ type: 'ref', ref: 'event.launch_note' },
						],
					},
					output: {
						mode: 'text',
					},
				},
			],
		})
		const lifecycle = new AgentLifecycleService({
			state_store: store,
		})

		await lifecycle.registerAgentFile(sourcePath)
		await lifecycle.deployAgentFile(sourcePath)
		const trigger = await registerLifecycleTrigger(
			'trigger.lifecycle.event',
			agentId,
			'mailbox://triage',
			store.database_path,
		)

		const { adapter, requests } = createStubAdapter([
			{
				outcome: 'success',
				output: {
					mode: 'text',
				},
				output_text: 'event-result',
			},
		])

		const dispatch = await dispatchTriggerEvent(trigger.trigger_id, store.database_path, {
			eventId: 'event-1',
			payload: {
				topic: 'phase-9',
			},
			launchNote: 'operator note',
			runId: 'run-event-1',
			adapter,
		})

		expect(dispatch.event).toMatchObject({
			event_id: 'event-1',
			trigger_id: trigger.trigger_id,
			logical_agent_id: agentId,
			dispatch_status: 'dispatched',
			run_id: 'run-event-1',
		})
		expect(dispatch.result).toMatchObject({
			status: 'success',
			run_id: 'run-event-1',
			run_status: 'completed',
			final_output: 'event-result',
		})
		expect(requests).toHaveLength(1)
		expect(requests[0]?.input_message).toBe('Event input: phase-9 / operator note')

		const persistedEvent = lifecycle.listEvents({ trigger_id: trigger.trigger_id })
		expect(persistedEvent).toEqual([
			expect.objectContaining({
				event_id: 'event-1',
				dispatch_status: 'dispatched',
				run_id: 'run-event-1',
				payload: {
					topic: 'phase-9',
				},
				launch_note: 'operator note',
			}),
		])

		const snapshot = store.getPersistedRunSnapshot('run-event-1')
		expect(snapshot?.run.started_via).toBe('event')
		expect(snapshot?.run.event).toEqual({
			payload: {
				topic: 'phase-9',
			},
			launch_note: 'operator note',
		})
	})

	it('threads user runtime-source narrowing through dispatchTriggerEvent before launch', async () => {
		const store = await createStore()
		const tempDir = path.dirname(store.database_path)
		const agentId = 'agent.lifecycle.event.runtime-source'
		const sourcePath = await writeAgentJson(tempDir, 'event-runtime-source-agent.json', {
			graph_contract_version: '1.0',
			meta: {
				id: agentId,
				name: 'Event Runtime Source Agent',
			},
			entry_node_id: 'start',
			final_output: {
				mode: 'last_node_output',
			},
			runtime_sources: [
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
			],
			nodes: [
				{
					id: 'start',
					kind: 'runtime_agent',
					runtime_adapter: 'codex',
					prompt: 'Return a text response.',
					input: {
						parts: [
							{ type: 'text', text: 'Event input: ' },
							{ type: 'ref', ref: 'event.payload.topic' },
						],
					},
					output: {
						mode: 'text',
					},
				},
			],
		})
		const lifecycle = new AgentLifecycleService({
			state_store: store,
		})

		await lifecycle.registerAgentFile(sourcePath)
		await lifecycle.deployAgentFile(sourcePath)
		const trigger = await registerLifecycleTrigger(
			'trigger.lifecycle.event.runtime-source',
			agentId,
			'mailbox://triage',
			store.database_path,
		)

		const { adapter, requests } = createStubAdapter(
			[
				{
					outcome: 'success',
					output: {
						mode: 'text',
					},
					output_text: 'event-result',
				},
			],
			{
				supports_explicit_runtime_source: true,
			},
		)

		const dispatch = await dispatchTriggerEvent(trigger.trigger_id, store.database_path, {
			eventId: 'event-runtime-source',
			payload: {
				topic: 'phase-9',
			},
			runtimeSourceIds: ['source-b'],
			adapter,
		})

		expect(dispatch.result).toMatchObject({
			status: 'success',
			final_output: 'event-result',
		})
		expect(requests[0]?.runtime_source).toEqual({
			id: 'source-b',
			runtime_adapter: 'codex',
			source_ref: 'workspace://source-b',
		})
	})

	it('marks event dispatch failed when the trigger points at a logical agent without a live revision', async () => {
		const store = await createStore()
		const lifecycle = new AgentLifecycleService({
			state_store: store,
		})

		const trigger = await registerLifecycleTrigger(
			'trigger.lifecycle.missing-live',
			'agent.lifecycle.missing-live',
			'mailbox://missing',
			store.database_path,
		)

		await expect(
			dispatchTriggerEvent(trigger.trigger_id, store.database_path, {
				eventId: 'event-missing-live',
				payload: {
					topic: 'phase-9',
				},
			}),
		).rejects.toMatchObject({
			code: 'AGENT_LIVE_NOT_FOUND',
		} satisfies Partial<AppError>)

		expect(lifecycle.listEvents({ trigger_id: trigger.trigger_id })).toEqual([
			expect.objectContaining({
				event_id: 'event-missing-live',
				dispatch_status: 'failed',
				run_id: null,
				dispatch_error_code: 'AGENT_LIVE_NOT_FOUND',
			}),
		])
	})

	it('preserves repeated and near-concurrent event dispatch records without corrupting lifecycle state', async () => {
		const store = await createStore()
		const tempDir = path.dirname(store.database_path)
		const agentId = 'agent.lifecycle.event.dispatch-regression'
		const sourcePath = await writeAgentJson(tempDir, 'event-dispatch-regression-agent.json', {
			graph_contract_version: '1.0',
			meta: {
				id: agentId,
				name: 'Event Dispatch Regression Agent',
			},
			entry_node_id: 'start',
			final_output: {
				mode: 'last_node_output',
			},
			nodes: [
				{
					id: 'start',
					kind: 'runtime_agent',
					runtime_adapter: 'codex',
					prompt: 'Return a text response.',
					input: {
						parts: [
							{ type: 'text', text: 'Dispatch input: ' },
							{ type: 'ref', ref: 'event.payload.topic' },
							{ type: 'text', text: ' / ' },
							{ type: 'ref', ref: 'event.launch_note' },
						],
					},
					output: {
						mode: 'text',
					},
				},
			],
		})
		const lifecycle = new AgentLifecycleService({
			state_store: store,
		})

		await lifecycle.registerAgentFile(sourcePath)
		const deployResult = await lifecycle.deployAgentFile(sourcePath)
		const trigger = await registerLifecycleTrigger(
			'trigger.lifecycle.event.dispatch-regression',
			agentId,
			'mailbox://dispatch-regression',
			store.database_path,
		)
		const missingLiveTrigger = await registerLifecycleTrigger(
			'trigger.lifecycle.event.dispatch-regression.missing-live',
			'agent.lifecycle.event.dispatch-regression.missing-live',
			'mailbox://dispatch-regression-missing-live',
			store.database_path,
		)

		const { adapter, requests } = createStubAdapter([
			{
				outcome: 'success',
				output: {
					mode: 'text',
				},
				output_text: 'first-result',
			},
			{
				outcome: 'runtime_error',
				error: {
					code: 'TEST_RUNTIME_FAILURE',
					message: 'deterministic runtime failure',
				},
			},
			{
				outcome: 'success',
				output: {
					mode: 'text',
				},
				output_text: 'concurrent-result-a',
			},
			{
				outcome: 'success',
				output: {
					mode: 'text',
				},
				output_text: 'concurrent-result-b',
			},
		])

		const firstDispatch = await dispatchTriggerEvent(trigger.trigger_id, store.database_path, {
			eventId: 'event-dispatch-regression-1',
			payload: {
				topic: 'first',
			},
			launchNote: 'sequential success',
			runId: 'run-dispatch-regression-1',
			adapter,
		})
		const runtimeFailureDispatch = await dispatchTriggerEvent(
			trigger.trigger_id,
			store.database_path,
			{
				eventId: 'event-dispatch-regression-runtime-failure',
				payload: {
					topic: 'runtime-failure',
				},
				launchNote: 'sequential runtime failure',
				runId: 'run-dispatch-regression-runtime-failure',
				adapter,
			},
		)
		const [concurrentDispatchA, concurrentDispatchB] = await Promise.all([
			dispatchTriggerEvent(trigger.trigger_id, store.database_path, {
				eventId: 'event-dispatch-regression-concurrent-a',
				payload: {
					topic: 'concurrent-a',
				},
				launchNote: 'near-concurrent success a',
				runId: 'run-dispatch-regression-concurrent-a',
				adapter,
			}),
			dispatchTriggerEvent(trigger.trigger_id, store.database_path, {
				eventId: 'event-dispatch-regression-concurrent-b',
				payload: {
					topic: 'concurrent-b',
				},
				launchNote: 'near-concurrent success b',
				runId: 'run-dispatch-regression-concurrent-b',
				adapter,
			}),
		])

		await expect(
			dispatchTriggerEvent(missingLiveTrigger.trigger_id, store.database_path, {
				eventId: 'event-dispatch-regression-missing-live',
				payload: {
					topic: 'missing-live',
				},
			}),
		).rejects.toMatchObject({
			code: 'AGENT_LIVE_NOT_FOUND',
		} satisfies Partial<AppError>)

		expect(firstDispatch.event).toMatchObject({
			event_id: 'event-dispatch-regression-1',
			dispatch_status: 'dispatched',
			run_id: 'run-dispatch-regression-1',
			resolved_revision_id: deployResult.revision.resolved_revision_id,
		})
		expect(firstDispatch.result).toMatchObject({
			status: 'success',
			run_status: 'completed',
			final_output: 'first-result',
		})
		expect(runtimeFailureDispatch.event).toMatchObject({
			event_id: 'event-dispatch-regression-runtime-failure',
			dispatch_status: 'dispatched',
			run_id: 'run-dispatch-regression-runtime-failure',
			dispatch_error_code: null,
		})
		expect(runtimeFailureDispatch.result).toMatchObject({
			status: 'failure',
			run_status: 'failed',
			code: 'TEST_RUNTIME_FAILURE',
			message: 'deterministic runtime failure',
		})
		expect([concurrentDispatchA.result.status, concurrentDispatchB.result.status]).toEqual([
			'success',
			'success',
		])
		expect(requests.slice(0, 2).map((request) => request.input_message)).toEqual([
			'Dispatch input: first / sequential success',
			'Dispatch input: runtime-failure / sequential runtime failure',
		])
		expect(requests.slice(2).map((request) => request.input_message)).toEqual([
			expect.stringMatching(/^Dispatch input: concurrent-[ab] \/ near-concurrent success [ab]$/),
			expect.stringMatching(/^Dispatch input: concurrent-[ab] \/ near-concurrent success [ab]$/),
		])
		expect(new Set(requests.slice(2).map((request) => request.input_message))).toEqual(
			new Set([
				'Dispatch input: concurrent-a / near-concurrent success a',
				'Dispatch input: concurrent-b / near-concurrent success b',
			]),
		)

		const persistedEvents = lifecycle.listEvents({ trigger_id: trigger.trigger_id })
		expect(persistedEvents).toHaveLength(4)
		expect(persistedEvents).toEqual(
			expect.arrayContaining([
				expect.objectContaining({
					event_id: 'event-dispatch-regression-1',
					dispatch_status: 'dispatched',
					run_id: 'run-dispatch-regression-1',
					payload: {
						topic: 'first',
					},
					launch_note: 'sequential success',
				}),
				expect.objectContaining({
					event_id: 'event-dispatch-regression-runtime-failure',
					dispatch_status: 'dispatched',
					run_id: 'run-dispatch-regression-runtime-failure',
					dispatch_error_code: null,
					dispatch_error_message: null,
				}),
				expect.objectContaining({
					event_id: 'event-dispatch-regression-concurrent-a',
					dispatch_status: 'dispatched',
					run_id: 'run-dispatch-regression-concurrent-a',
				}),
				expect.objectContaining({
					event_id: 'event-dispatch-regression-concurrent-b',
					dispatch_status: 'dispatched',
					run_id: 'run-dispatch-regression-concurrent-b',
				}),
			]),
		)
		expect(lifecycle.listEvents({ trigger_id: missingLiveTrigger.trigger_id })).toEqual([
			expect.objectContaining({
				event_id: 'event-dispatch-regression-missing-live',
				dispatch_status: 'failed',
				run_id: null,
				resolved_revision_id: null,
				dispatch_error_code: 'AGENT_LIVE_NOT_FOUND',
			}),
		])

		for (const runId of [
			'run-dispatch-regression-1',
			'run-dispatch-regression-concurrent-a',
			'run-dispatch-regression-concurrent-b',
		]) {
			const snapshot = store.getPersistedRunSnapshot(runId)
			expect(snapshot?.run).toMatchObject({
				run_id: runId,
				logical_agent_id: agentId,
				started_via: 'event',
				status: 'completed',
				resolved_revision_id: deployResult.revision.resolved_revision_id,
			})
			expect(snapshot?.attempts).toEqual([
				expect.objectContaining({
					node_id: 'start',
					state: 'committed_terminal',
					outcome: 'success',
				}),
			])
		}

		const failedRunSnapshot = store.getPersistedRunSnapshot(
			'run-dispatch-regression-runtime-failure',
		)
		expect(failedRunSnapshot?.run).toMatchObject({
			run_id: 'run-dispatch-regression-runtime-failure',
			logical_agent_id: agentId,
			started_via: 'event',
			status: 'failed',
			resolved_revision_id: deployResult.revision.resolved_revision_id,
		})
		expect(failedRunSnapshot?.attempts).toEqual([
			expect.objectContaining({
				node_id: 'start',
				state: 'committed_terminal',
				outcome: 'runtime_error',
			}),
		])

		const status = await lifecycle.getAgentStatus(agentId)
		expect(status.agent.live_revision_id).toBe(deployResult.revision.revision_id)
		expect(status.live_revision).toMatchObject({
			revision_id: deployResult.revision.revision_id,
			availability_state: 'available',
			resolved_revision_id: deployResult.revision.resolved_revision_id,
		})
		expect(status.draft_revisions).toHaveLength(1)
	})
})
