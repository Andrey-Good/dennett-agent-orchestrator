import { mkdtemp, rm } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { describe, expect, it } from 'vitest'
import type { AgentFile, JsonOutputContract } from '../../src/core/agent-file.js'
import { AppError } from '../../src/core/errors.js'
import { resumeAgentRun } from '../../src/core/graph-runner.js'
import type { JsonObject } from '../../src/core/json.js'
import { SQLiteLocalStateStore } from '../../src/core/state/index.js'
import type {
	RuntimeAdapter,
	RuntimeAdapterCapabilities,
	RuntimeAdapterExecutionRequest,
	RuntimeTerminalResult,
} from '../../src/ports/runtime.js'

const NODE_AFTER_CRASH_OUTPUT: JsonOutputContract = {
	mode: 'json',
	schema: {
		type: 'object',
		properties: {
			status: {
				type: 'string',
			},
			source: {
				type: 'string',
			},
		},
		required: ['status', 'source'],
		additionalProperties: false,
	},
}

const CRASH_RECOVERY_AGENT_FILE: AgentFile = {
	graph_contract_version: '1.0',
	meta: {
		id: 'agent.stage8.crash-recovery',
		name: 'Stage 8 Crash Recovery Test Agent',
	},
	entry_node_id: 'node-stable',
	initial_vars: {
		committed: 'initial',
	},
	final_output: {
		mode: 'last_node_output',
	},
	nodes: [
		{
			id: 'node-stable',
			kind: 'runtime_agent',
			runtime_adapter: 'codex',
			prompt: 'Return stable text.',
			input: {
				parts: [{ type: 'text', text: 'Stable node' }],
			},
			output: {
				mode: 'text',
			},
		},
		{
			id: 'node-after-crash',
			kind: 'runtime_agent',
			runtime_adapter: 'codex',
			prompt: 'Return final recovery json.',
			input: {
				parts: [
					{ type: 'text', text: 'Recover after ' },
					{ type: 'ref', ref: 'node.node-stable.text' },
					{ type: 'text', text: ' with committed=' },
					{ type: 'ref', ref: 'vars.committed' },
				],
			},
			output: NODE_AFTER_CRASH_OUTPUT,
		},
	],
	edges: [
		{
			from: 'node-stable',
			to: 'node-after-crash',
		},
	],
}

function emptyEventStream() {
	return {
		async *[Symbol.asyncIterator]() {
			// No live-provider events are needed for this recovery proof.
		},
	}
}

function createSingleResultAdapter(result: RuntimeTerminalResult): {
	adapter: RuntimeAdapter
	requests: RuntimeAdapterExecutionRequest[]
} {
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
	}
	let consumed = false
	const adapter: RuntimeAdapter = {
		describeCapabilities() {
			return capabilities
		},
		async startExecution(request) {
			if (consumed) {
				throw new Error('Crash recovery adapter result was already consumed.')
			}
			consumed = true
			requests.push(request)
			return {
				runtime_handle: null,
				native_session_handle: null,
				terminal_result: Promise.resolve(result),
				events: emptyEventStream(),
			}
		},
		async listModels() {
			throw new Error('not used in this test')
		},
		async inspectRuntimeEnvironment() {
			throw new Error('not used in this test')
		},
		async inspectRuntimeSource(source) {
			return {
				source_id: source.id,
				availability: 'unknown',
				limit_status: 'unknown',
			}
		},
		async deliverComment() {
			throw new Error('not used in this test')
		},
		async deliverUserChatResponse() {
			throw new Error('not used in this test')
		},
		async cancelExecution() {
			throw new Error('not used in this test')
		},
	}

	return {
		adapter,
		requests,
	}
}

describe('stage 8 crash recovery', () => {
	it('reopens a fresh store with stale in-progress work without fabricating committed success', async () => {
		const tempDir = await mkdtemp(path.join(os.tmpdir(), 'dennett-stage8-crash-recovery-'))
		const databasePath = path.join(tempDir, 'local-state.sqlite')
		let crashedStore: SQLiteLocalStateStore | null = null
		let reopenedStore: SQLiteLocalStateStore | null = null

		try {
			crashedStore = new SQLiteLocalStateStore({
				database_path: databasePath,
			})
			const run = crashedStore.createRun({
				run_id: 'run-crash-reopen-1',
				resolved_revision_id: 'rev-crash-reopen',
				entry_node_id: 'node-stable',
				started_via: 'direct',
				initial_vars: {
					committed: 'initial',
				},
				created_at: '2026-04-24T10:00:00.000Z',
			})

			const committedAttempt = crashedStore.startNodeAttempt({
				attempt_id: 'attempt-stable-1',
				run_id: run.run_id,
				node_id: 'node-stable',
				output_mode: 'text',
				started_at: '2026-04-24T10:00:01.000Z',
			})
			crashedStore.commitNodeSuccess({
				attempt_id: committedAttempt.attempt_id,
				output: {
					mode: 'text',
					text: 'durable output before crash',
				},
				vars: {
					committed: 'safe',
				},
				run_status: 'running',
				resume: {
					native_resume_available: false,
					local_resume_available: true,
					local_context_snapshot: {
						last_safe_node: 'node-stable',
					},
				},
				committed_at: '2026-04-24T10:00:02.000Z',
			})

			const staleAttempt = crashedStore.startNodeAttempt({
				attempt_id: 'attempt-crashed-2',
				run_id: run.run_id,
				node_id: 'node-after-crash',
				output_mode: 'json',
				runtime_handle: {
					process: 'worker-before-crash',
				},
				started_at: '2026-04-24T10:00:03.000Z',
			})

			crashedStore.close()
			crashedStore = null

			reopenedStore = new SQLiteLocalStateStore({
				database_path: databasePath,
			})

			const reopenedSnapshot = reopenedStore.getPersistedRunSnapshot(run.run_id)
			expect(reopenedSnapshot).not.toBeNull()
			expect(reopenedSnapshot?.run).toMatchObject({
				run_id: run.run_id,
				status: 'running',
				last_attempt_sequence: 2,
				last_boundary_sequence: 1,
			})
			expect(reopenedSnapshot?.current_vars).toEqual({
				committed: 'safe',
			})
			expect(reopenedSnapshot?.latest_committed_outputs).toEqual([
				expect.objectContaining({
					attempt_id: committedAttempt.attempt_id,
					node_id: 'node-stable',
					output: {
						mode: 'text',
						text: 'durable output before crash',
					},
					boundary_sequence: 1,
				}),
			])
			expect(reopenedSnapshot?.attempts).toEqual([
				expect.objectContaining({
					attempt_id: committedAttempt.attempt_id,
					state: 'committed_terminal',
					outcome: 'success',
					committed_output_id: expect.any(String),
					resume_boundary_sequence: 1,
				}),
				expect.objectContaining({
					attempt_id: staleAttempt.attempt_id,
					state: 'in_progress',
					outcome: null,
					committed_output_id: null,
					resume_boundary_sequence: null,
					committed_at: null,
					runtime_handle: {
						process: 'worker-before-crash',
					},
				}),
			])
			expect(reopenedSnapshot?.resume).toMatchObject({
				local_resume_available: true,
				last_durable_boundary_sequence: 1,
				last_durable_boundary_kind: 'node_attempt_terminal',
				last_attempt_id: committedAttempt.attempt_id,
				pending_prompt: null,
			})
			expect(reopenedStore.getLatestCommittedNodeOutput(run.run_id, 'node-after-crash')).toBeNull()

			let duplicateStartError: unknown
			try {
				reopenedStore.startNodeAttempt({
					run_id: run.run_id,
					node_id: 'node-after-crash',
					output_mode: 'json',
				})
			} catch (error) {
				duplicateStartError = error
			}

			expect(duplicateStartError).toBeInstanceOf(AppError)
			expect((duplicateStartError as AppError).code).toBe('ACTIVE_NODE_ATTEMPT_EXISTS')

			reopenedStore.commitNodeTerminalOutcome({
				attempt_id: staleAttempt.attempt_id,
				outcome: 'interrupted',
				run_status: 'interrupted',
				resume: {
					native_resume_available: false,
					local_resume_available: true,
					local_context_snapshot: {
						last_safe_node: 'node-stable',
						stale_attempt_id: staleAttempt.attempt_id,
					},
				},
				committed_at: '2026-04-24T10:00:04.000Z',
			})
			const finalOutput: JsonObject = {
				status: 'recovered',
				source: 'retry-after-reopen',
			}
			const { adapter, requests } = createSingleResultAdapter({
				outcome: 'success',
				output: NODE_AFTER_CRASH_OUTPUT,
				output_json: finalOutput,
			})

			const result = await resumeAgentRun(CRASH_RECOVERY_AGENT_FILE, adapter, run.run_id, {
				state_store: reopenedStore,
				resolved_revision_id: 'rev-crash-reopen',
			})

			expect(result).toEqual({
				status: 'success',
				run_id: run.run_id,
				run_status: 'completed',
				final_output: finalOutput,
				final_output_mode: 'json',
				node_outputs: expect.any(Map),
			})
			expect(
				result.status === 'success' ? result.node_outputs.get('node-after-crash') : null,
			).toEqual({
				mode: 'json',
				json: finalOutput,
			})
			expect(requests).toHaveLength(1)
			expect(requests[0]).toMatchObject({
				node_id: 'node-after-crash',
				input_message: 'Recover after durable output before crash with committed=safe',
				resume: {
					mode: 'fresh',
				},
			})

			const completedSnapshot = reopenedStore.getPersistedRunSnapshot(run.run_id)
			expect(completedSnapshot).toMatchObject({
				run: {
					status: 'completed',
					last_attempt_sequence: 3,
					last_boundary_sequence: 3,
				},
				resume: {
					last_durable_boundary_sequence: 3,
					last_durable_boundary_kind: 'node_attempt_terminal',
					local_resume_available: false,
				},
			})
			expect(completedSnapshot?.attempts).toEqual([
				expect.objectContaining({
					attempt_id: committedAttempt.attempt_id,
					state: 'committed_terminal',
					outcome: 'success',
					committed_output_id: expect.any(String),
				}),
				expect.objectContaining({
					attempt_id: staleAttempt.attempt_id,
					state: 'committed_terminal',
					outcome: 'interrupted',
					committed_output_id: null,
				}),
				expect.objectContaining({
					node_id: 'node-after-crash',
					attempt_sequence: 3,
					state: 'committed_terminal',
					outcome: 'success',
					committed_output_id: expect.any(String),
				}),
			])
			expect(completedSnapshot?.latest_committed_outputs).toHaveLength(2)
			expect(completedSnapshot?.latest_committed_outputs).toEqual(
				expect.arrayContaining([
					expect.objectContaining({
						attempt_id: committedAttempt.attempt_id,
						node_id: 'node-stable',
						output: {
							mode: 'text',
							text: 'durable output before crash',
						},
						boundary_sequence: 1,
					}),
					expect.objectContaining({
						node_id: 'node-after-crash',
						output: {
							mode: 'json',
							json: finalOutput,
						},
						boundary_sequence: 3,
					}),
				]),
			)

			const nodeAfterCrashOutputs = completedSnapshot?.latest_committed_outputs.filter(
				(output) => output.node_id === 'node-after-crash',
			)
			expect(nodeAfterCrashOutputs).toHaveLength(1)
			expect(nodeAfterCrashOutputs?.[0]).toMatchObject({
				output: {
					mode: 'json',
					json: finalOutput,
				},
			})
			expect(
				reopenedStore.getLatestCommittedNodeOutput(run.run_id, 'node-after-crash'),
			).toMatchObject({
				output: {
					mode: 'json',
					json: finalOutput,
				},
				boundary_sequence: 3,
			})
		} finally {
			reopenedStore?.close()
			crashedStore?.close()
			await rm(tempDir, { recursive: true, force: true })
		}
	})
})
