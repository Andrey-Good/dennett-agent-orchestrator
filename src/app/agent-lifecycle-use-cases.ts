import {
	AgentLifecycleService,
	type AgentLifecycleDeployResult,
	type AgentLifecycleIndexResult,
} from '../core/agent-lifecycle.js'
import type { SQLiteLocalStateStore } from '../core/state/index.js'
import type { AgentLifecycleStatusRecord } from '../core/state/types.js'
import { createLocalStateStore } from './local-state.js'

export interface RegisterLifecycleAgentFileInput {
	agentFilePath: string
	stateDbPath: string
}

export interface GetLifecycleAgentStatusInput {
	logicalAgentId: string
	stateDbPath: string
}

export interface DeployLifecycleAgentFileInput {
	agentFilePath: string
	stateDbPath: string
}

export interface AgentLifecycleUseCases {
	registerAgentFile(input: RegisterLifecycleAgentFileInput): Promise<AgentLifecycleIndexResult>
	getAgentStatus(input: GetLifecycleAgentStatusInput): Promise<AgentLifecycleStatusRecord>
	deployAgentFile(input: DeployLifecycleAgentFileInput): Promise<AgentLifecycleDeployResult>
}

export interface AgentLifecycleOperations {
	registerAgentFile(agentFilePath: string): Promise<AgentLifecycleIndexResult>
	getAgentStatus(logicalAgentId: string): Promise<AgentLifecycleStatusRecord>
	deployAgentFile(agentFilePath: string): Promise<AgentLifecycleDeployResult>
}

export interface AgentLifecycleUseCaseDependencies {
	createStateStore?: (stateDbPath: string) => Promise<SQLiteLocalStateStore>
	createLifecycleService?: (stateStore: SQLiteLocalStateStore) => AgentLifecycleOperations
}

function createDefaultLifecycleService(stateStore: SQLiteLocalStateStore): AgentLifecycleOperations {
	return new AgentLifecycleService({
		state_store: stateStore,
	})
}

export function createAgentLifecycleUseCases(
	dependencies: AgentLifecycleUseCaseDependencies = {},
): AgentLifecycleUseCases {
	const createStateStore = dependencies.createStateStore ?? createLocalStateStore
	const createLifecycleService = dependencies.createLifecycleService ?? createDefaultLifecycleService

	return {
		async registerAgentFile(input) {
			const stateStore = await createStateStore(input.stateDbPath)
			try {
				return await createLifecycleService(stateStore).registerAgentFile(input.agentFilePath)
			} finally {
				stateStore.close()
			}
		},

		async getAgentStatus(input) {
			const stateStore = await createStateStore(input.stateDbPath)
			try {
				return await createLifecycleService(stateStore).getAgentStatus(input.logicalAgentId)
			} finally {
				stateStore.close()
			}
		},

		async deployAgentFile(input) {
			const stateStore = await createStateStore(input.stateDbPath)
			try {
				return await createLifecycleService(stateStore).deployAgentFile(input.agentFilePath)
			} finally {
				stateStore.close()
			}
		},
	}
}

const agentLifecycleUseCases = createAgentLifecycleUseCases()

export async function registerLifecycleAgentFile(
	input: RegisterLifecycleAgentFileInput,
): Promise<AgentLifecycleIndexResult> {
	return await agentLifecycleUseCases.registerAgentFile(input)
}

export async function getLifecycleAgentStatus(
	input: GetLifecycleAgentStatusInput,
): Promise<AgentLifecycleStatusRecord> {
	return await agentLifecycleUseCases.getAgentStatus(input)
}

export async function deployLifecycleAgentFile(
	input: DeployLifecycleAgentFileInput,
): Promise<AgentLifecycleDeployResult> {
	return await agentLifecycleUseCases.deployAgentFile(input)
}
