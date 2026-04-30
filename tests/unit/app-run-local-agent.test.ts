import path from 'node:path'
import { describe, expect, it, vi } from 'vitest'
import { createRunLocalAgent } from '../../src/app/run-local-agent.js'
import type { AgentFile } from '../../src/core/agent-file.js'
import type { RunResult } from '../../src/core/graph-runner.js'
import type { SQLiteLocalStateStore } from '../../src/core/state/index.js'
import type { RuntimeAdapter } from '../../src/ports/runtime.js'

function createRuntimeAdapter(): RuntimeAdapter {
	return {
		describeCapabilities: vi.fn(),
		startExecution: vi.fn(),
		listModels: vi.fn(),
		inspectRuntimeEnvironment: vi.fn(),
		inspectRuntimeSource: vi.fn(),
		deliverComment: vi.fn(),
		deliverUserChatResponse: vi.fn(),
		cancelExecution: vi.fn(),
	} as unknown as RuntimeAdapter
}

describe('runLocalAgent app facade', () => {
	it('loads the local agent, creates a store, and delegates the run to core', async () => {
		const cwd = path.resolve('workspace')
		const agentFile = {
			meta: { id: 'agent.run-local' },
		} as unknown as AgentFile
		const stateStore = {
			close: vi.fn(),
		} as unknown as SQLiteLocalStateStore
		const adapter = createRuntimeAdapter()
		const result = {
			status: 'success',
			run_id: 'run-1',
			run_status: 'completed',
			final_output: 'done',
			final_output_mode: 'text',
			node_outputs: new Map(),
		} satisfies RunResult
		const createStateStore = vi.fn(async () => stateStore)
		const loadAndValidateAgentFile = vi.fn(async () => agentFile)
		const computeResolvedRevisionId = vi.fn(async () => 'rev-1')
		const runAgentFile = vi.fn(async () => result)
		const runLocalAgent = createRunLocalAgent({
			createStateStore,
			loadAndValidateAgentFile,
			computeResolvedRevisionId,
			runAgentFile,
		})

		await expect(
			runLocalAgent({
				agentFilePath: 'agents/example.json',
				cwd,
				stateDbPath: '.dennett/local-state.sqlite',
				adapter,
				params: { city: 'Paris' },
				runId: 'run-1',
				runtimeSourceIds: ['source-1'],
			}),
		).resolves.toBe(result)

		const resolvedAgentFilePath = path.resolve(cwd, 'agents/example.json')
		expect(loadAndValidateAgentFile).toHaveBeenCalledWith(resolvedAgentFilePath)
		expect(computeResolvedRevisionId).toHaveBeenCalledWith(resolvedAgentFilePath)
		expect(createStateStore).toHaveBeenCalledWith(
			path.resolve(cwd, '.dennett/local-state.sqlite'),
		)
		expect(runAgentFile).toHaveBeenCalledWith(agentFile, adapter, { city: 'Paris' }, {
			state_store: stateStore,
			resolved_revision_id: 'rev-1',
			run_id: 'run-1',
			user_runtime_source_ids: ['source-1'],
		})
		expect(stateStore.close).toHaveBeenCalledTimes(1)
	})

	it('closes the state store when core run delegation fails', async () => {
		const stateStore = {
			close: vi.fn(),
		} as unknown as SQLiteLocalStateStore
		const expectedError = new Error('run failed')
		const runLocalAgent = createRunLocalAgent({
			createStateStore: vi.fn(async () => stateStore),
			loadAndValidateAgentFile: vi.fn(async () => ({}) as AgentFile),
			computeResolvedRevisionId: vi.fn(async () => 'rev-1'),
			runAgentFile: vi.fn(async () => {
				throw expectedError
			}),
		})

		await expect(
			runLocalAgent({
				agentFilePath: 'agent.json',
				cwd: path.resolve('workspace'),
				stateDbPath: 'state.sqlite',
				adapter: createRuntimeAdapter(),
				params: {},
			}),
		).rejects.toBe(expectedError)

		expect(stateStore.close).toHaveBeenCalledTimes(1)
	})
})
