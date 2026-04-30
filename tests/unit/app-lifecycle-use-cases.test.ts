import { describe, expect, it, vi } from 'vitest'
import { createAgentLifecycleUseCases } from '../../src/app/agent-lifecycle-use-cases.js'
import type {
	AgentLifecycleDeployResult,
	AgentLifecycleIndexResult,
} from '../../src/core/agent-lifecycle.js'
import type { SQLiteLocalStateStore } from '../../src/core/state/index.js'
import type { AgentLifecycleStatusRecord } from '../../src/core/state/types.js'

function createMockLifecycleUseCases() {
	const status = { agent: { logical_agent_id: 'agent.lifecycle.app' } } as unknown as
		AgentLifecycleStatusRecord
	const registerResult = {
		logical_agent_id: 'agent.lifecycle.app',
		revision: { revision_id: 'draft-revision' },
		status,
	} as unknown as AgentLifecycleIndexResult
	const deployResult = {
		logical_agent_id: 'agent.lifecycle.app',
		revision: { revision_id: 'live-revision' },
		source_revision_id: 'source-revision',
		live_file_path: '/tmp/live-agent.json',
		status,
	} as unknown as AgentLifecycleDeployResult

	return {
		status,
		registerResult,
		deployResult,
		lifecycleService: {
			registerAgentFile: vi.fn(async () => registerResult),
			getAgentStatus: vi.fn(async () => status),
			deployAgentFile: vi.fn(async () => deployResult),
		},
	}
}

describe('app lifecycle use cases', () => {
	it('delegates register, status, and deploy through a constructed lifecycle service', async () => {
		const stateStore = {
			close: vi.fn(),
		} as unknown as SQLiteLocalStateStore
		const createStateStore = vi.fn(async () => stateStore)
		const { lifecycleService, status, registerResult, deployResult } =
			createMockLifecycleUseCases()
		const createLifecycleService = vi.fn(() => lifecycleService)
		const useCases = createAgentLifecycleUseCases({
			createStateStore,
			createLifecycleService,
		})

		await expect(
			useCases.registerAgentFile({
				agentFilePath: 'agent.json',
				stateDbPath: 'state.sqlite',
			}),
		).resolves.toBe(registerResult)
		await expect(
			useCases.getAgentStatus({
				logicalAgentId: 'agent.lifecycle.app',
				stateDbPath: 'state.sqlite',
			}),
		).resolves.toBe(status)
		await expect(
			useCases.deployAgentFile({
				agentFilePath: 'agent.json',
				stateDbPath: 'state.sqlite',
			}),
		).resolves.toBe(deployResult)

		expect(createStateStore).toHaveBeenCalledTimes(3)
		expect(createStateStore).toHaveBeenNthCalledWith(1, 'state.sqlite')
		expect(createLifecycleService).toHaveBeenCalledTimes(3)
		expect(createLifecycleService).toHaveBeenCalledWith(stateStore)
		expect(lifecycleService.registerAgentFile).toHaveBeenCalledWith('agent.json')
		expect(lifecycleService.getAgentStatus).toHaveBeenCalledWith('agent.lifecycle.app')
		expect(lifecycleService.deployAgentFile).toHaveBeenCalledWith('agent.json')
		expect(stateStore.close).toHaveBeenCalledTimes(3)
	})

	it('closes the state store when lifecycle delegation fails', async () => {
		const stateStore = {
			close: vi.fn(),
		} as unknown as SQLiteLocalStateStore
		const expectedError = new Error('delegation failed')
		const lifecycleService = {
			registerAgentFile: vi.fn(async () => {
				throw expectedError
			}),
			getAgentStatus: vi.fn(),
			deployAgentFile: vi.fn(),
		}
		const useCases = createAgentLifecycleUseCases({
			createStateStore: vi.fn(async () => stateStore),
			createLifecycleService: vi.fn(() => lifecycleService),
		})

		await expect(
			useCases.registerAgentFile({
				agentFilePath: 'agent.json',
				stateDbPath: 'state.sqlite',
			}),
		).rejects.toBe(expectedError)

		expect(stateStore.close).toHaveBeenCalledTimes(1)
	})
})
