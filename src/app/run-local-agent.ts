import path from 'node:path'
import {
	runAgentFile as runAgentFileCore,
	type RunResult,
} from '../core/graph-runner.js'
import type { AgentFile } from '../core/agent-file.js'
import type { JsonValue } from '../core/json.js'
import { computeResolvedRevisionId as computeResolvedRevisionIdCore } from '../core/resolved-revision.js'
import { loadAndValidateAgentFile as loadAndValidateAgentFileCore } from '../core/schema.js'
import type { SQLiteLocalStateStore } from '../core/state/index.js'
import type { RuntimeAdapter } from '../ports/runtime.js'
import { createLocalStateStore } from './local-state.js'

export interface RunLocalAgentInput {
	agentFilePath: string
	cwd: string
	stateDbPath: string
	adapter: RuntimeAdapter
	params: Record<string, JsonValue>
	runId?: string
	runtimeSourceIds?: string[]
}

export interface RunLocalAgentDependencies {
	createStateStore?: (stateDbPath: string) => Promise<SQLiteLocalStateStore>
	loadAndValidateAgentFile?: (agentFilePath: string) => Promise<AgentFile>
	computeResolvedRevisionId?: (agentFilePath: string) => Promise<string>
	runAgentFile?: typeof runAgentFileCore
}

export function createRunLocalAgent(
	dependencies: RunLocalAgentDependencies = {},
): (input: RunLocalAgentInput) => Promise<RunResult> {
	const createStateStore = dependencies.createStateStore ?? createLocalStateStore
	const loadAndValidateAgentFile =
		dependencies.loadAndValidateAgentFile ?? loadAndValidateAgentFileCore
	const computeResolvedRevisionId =
		dependencies.computeResolvedRevisionId ?? computeResolvedRevisionIdCore
	const runAgentFile = dependencies.runAgentFile ?? runAgentFileCore

	return async function runLocalAgent(input) {
		const resolvedAgentFilePath = path.resolve(input.cwd, input.agentFilePath)
		const [agentFile, resolvedRevisionId] = await Promise.all([
			loadAndValidateAgentFile(resolvedAgentFilePath),
			computeResolvedRevisionId(resolvedAgentFilePath),
		])
		const stateStore = await createStateStore(path.resolve(input.cwd, input.stateDbPath))

		try {
			return await runAgentFile(agentFile, input.adapter, input.params, {
				state_store: stateStore,
				resolved_revision_id: resolvedRevisionId,
				run_id: input.runId,
				user_runtime_source_ids: input.runtimeSourceIds,
			})
		} finally {
			stateStore.close()
		}
	}
}

const defaultRunLocalAgent = createRunLocalAgent()

export async function runLocalAgent(input: RunLocalAgentInput): Promise<RunResult> {
	return await defaultRunLocalAgent(input)
}
