import { mkdtemp, rm } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { describe, expect, it } from 'vitest'
import { AppError } from '../../src/core/errors.js'
import { SQLiteLocalStateStore } from '../../src/core/state/index.js'

const runId = 'run-sqlite-pressure'
const resolvedRevisionId = 'rev-sqlite-pressure'
const attemptCount = 12
const storeCount = 4

function deterministicTimestamp(step: number): string {
	return `2026-04-25T10:${String(step).padStart(2, '0')}:00.000Z`
}

function expectAppErrorCode(error: unknown, code: string): void {
	expect(error).toBeInstanceOf(AppError)
	expect((error as AppError).code).toBe(code)
}

describe('stage 8 SQLite pressure', () => {
	it('keeps same-file run state consistent across multiple store instances', async () => {
		const tempDir = await mkdtemp(path.join(os.tmpdir(), 'dennett-stage8-sqlite-pressure-'))
		const databasePath = path.join(tempDir, 'local-state.sqlite')
		const stores: SQLiteLocalStateStore[] = []

		try {
			for (let index = 0; index < storeCount; index += 1) {
				stores.push(
					new SQLiteLocalStateStore({
						database_path: databasePath,
					}),
				)
			}

			stores[0].createRun({
				run_id: runId,
				logical_agent_id: 'agent.sqlite-pressure',
				resolved_revision_id: resolvedRevisionId,
				entry_node_id: 'node-01',
				started_via: 'direct',
				initial_vars: {
					committed_count: 0,
					last_committed_node_id: null,
				},
				created_at: deterministicTimestamp(0),
			})

			for (let step = 1; step <= attemptCount; step += 1) {
				const nodeId = `node-${String(step).padStart(2, '0')}`
				const attemptId = `attempt-sqlite-pressure-${String(step).padStart(2, '0')}`
				const startStore = stores[step % storeCount]
				const competingStore = stores[(step + 1) % storeCount]
				const observingStore = stores[(step + 2) % storeCount]
				const committingStore = stores[(step + 3) % storeCount]

				const attempt = startStore.startNodeAttempt({
					attempt_id: attemptId,
					run_id: runId,
					node_id: nodeId,
					output_mode: step % 2 === 0 ? 'json' : 'text',
					runtime_handle: {
						start_store_index: step % storeCount,
						step,
					},
					started_at: deterministicTimestamp(step),
				})

				expect(attempt).toMatchObject({
					attempt_id: attemptId,
					run_id: runId,
					node_id: nodeId,
					attempt_sequence: step,
					state: 'in_progress',
					committed_output_id: null,
				})

				let competingStartError: unknown
				try {
					competingStore.startNodeAttempt({
						run_id: runId,
						node_id: `${nodeId}-duplicate-active`,
						output_mode: 'text',
						started_at: deterministicTimestamp(step),
					})
				} catch (error) {
					competingStartError = error
				}

				expectAppErrorCode(competingStartError, 'ACTIVE_NODE_ATTEMPT_EXISTS')
				expect(
					observingStore.listNodeAttempts(runId).filter(({ state }) => state === 'in_progress'),
				).toEqual([
					expect.objectContaining({
						attempt_id: attemptId,
						attempt_sequence: step,
					}),
				])

				committingStore.commitNodeSuccess({
					attempt_id: attemptId,
					output:
						step % 2 === 0
							? {
									mode: 'json',
									json: {
										node_id: nodeId,
										committed_count: step,
									},
								}
							: {
									mode: 'text',
									text: `committed ${nodeId}`,
								},
					vars: {
						committed_count: step,
						last_committed_node_id: nodeId,
					},
					run_status: step === attemptCount ? 'completed' : 'running',
					resume: {
						native_resume_available: false,
						local_resume_available: step !== attemptCount,
						local_context_snapshot: {
							last_committed_node_id: nodeId,
							committed_count: step,
						},
					},
					committed_at: deterministicTimestamp(step + attemptCount),
				})
			}

			for (const store of stores) {
				const snapshot = store.getPersistedRunSnapshot(runId)
				expect(snapshot).not.toBeNull()
				expect(snapshot?.run).toMatchObject({
					run_id: runId,
					status: 'completed',
					last_attempt_sequence: attemptCount,
					last_boundary_sequence: attemptCount,
				})
				expect(snapshot?.current_vars).toEqual({
					committed_count: attemptCount,
					last_committed_node_id: `node-${String(attemptCount).padStart(2, '0')}`,
				})
				expect(snapshot?.resume).toMatchObject({
					local_resume_available: false,
					last_durable_boundary_sequence: attemptCount,
					last_durable_boundary_kind: 'node_attempt_terminal',
					last_attempt_id: `attempt-sqlite-pressure-${String(attemptCount).padStart(2, '0')}`,
					pending_prompt: null,
				})

				const attempts = snapshot?.attempts ?? []
				const committedOutputs = snapshot?.latest_committed_outputs ?? []
				expect(attempts).toHaveLength(attemptCount)
				expect(committedOutputs).toHaveLength(attemptCount)
				expect(attempts.map(({ attempt_sequence }) => attempt_sequence)).toEqual(
					Array.from({ length: attemptCount }, (_, index) => index + 1),
				)
				expect(attempts.filter(({ state }) => state === 'in_progress')).toEqual([])
				expect(
					attempts.every(
						({ state, outcome }) => state === 'committed_terminal' && outcome === 'success',
					),
				).toBe(true)

				const committedOutputIds = attempts.map(({ committed_output_id }) => committed_output_id)
				expect(committedOutputIds.every((outputId) => outputId !== null)).toBe(true)
				expect(new Set(committedOutputIds).size).toBe(attemptCount)
				expect(
					committedOutputs
						.map(({ boundary_sequence }) => boundary_sequence)
						.sort((left, right) => left - right),
				).toEqual(Array.from({ length: attemptCount }, (_, index) => index + 1))
				expect(new Set(committedOutputs.map(({ output_id }) => output_id)).size).toBe(attemptCount)

				for (let step = 1; step <= attemptCount; step += 1) {
					const nodeId = `node-${String(step).padStart(2, '0')}`
					expect(store.getLatestCommittedNodeOutput(runId, nodeId)).toMatchObject({
						run_id: runId,
						node_id: nodeId,
						attempt_id: `attempt-sqlite-pressure-${String(step).padStart(2, '0')}`,
						boundary_sequence: step,
					})
				}
			}
		} finally {
			while (stores.length > 0) {
				stores.pop()?.close()
			}
			await rm(tempDir, { recursive: true, force: true })
		}
	})
})
