import { describe, expect, it, vi } from 'vitest'
import {
	buildRunInteractionStatus,
	createRunInteractionUseCases,
} from '../../src/app/run-interaction-use-cases.js'
import { AppError } from '../../src/core/errors.js'
import type { SQLiteLocalStateStore } from '../../src/core/state/index.js'
import type { PersistedRunSnapshot } from '../../src/core/state/types.js'

function createSnapshot(): PersistedRunSnapshot {
	return {
		run: {
			run_id: 'run-app-status',
			logical_agent_id: 'agent.app',
			resolved_revision_id: 'rev-app',
			entry_node_id: 'start',
			started_via: 'direct',
			status: 'waiting_for_user',
			params: {},
			event: null,
			last_attempt_sequence: 1,
			last_boundary_sequence: 1,
			created_at: '2026-04-30T08:00:00.000Z',
			updated_at: '2026-04-30T08:00:01.000Z',
		},
		chat: null,
		visible_messages: [
			{
				message_id: 'msg-1',
				chat_id: 'chat-1',
				run_id: 'run-app-status',
				message_sequence: 1,
				kind: 'blocking_prompt',
				payload: { text: 'Continue?' },
				created_at: '2026-04-30T08:00:01.000Z',
			},
		],
		attempts: [
			{
				attempt_id: 'attempt-1',
				run_id: 'run-app-status',
				node_id: 'start',
				attempt_sequence: 1,
				output_mode: 'text',
				state: 'blocked_wait',
				outcome: null,
				blocked_on_user_prompt: true,
				runtime_handle: { thread_id: 'thread-1' },
				committed_output_id: null,
				resume_boundary_sequence: 1,
				started_at: '2026-04-30T08:00:00.000Z',
				committed_at: null,
			},
		],
		latest_committed_outputs: [],
		current_vars: {},
		resume: {
			run_id: 'run-app-status',
			resolved_revision_id: 'rev-app',
			native_resume_available: true,
			local_resume_available: true,
			last_durable_boundary_sequence: 1,
			last_durable_boundary_kind: 'blocked_prompt_wait',
			last_attempt_id: 'attempt-1',
			pending_prompt: {
				run_id: 'run-app-status',
				attempt_id: 'attempt-1',
				prompt_id: 'prompt-1',
				payload: {
					kind: 'text',
					require_response: true,
					text: 'Continue?',
				},
				request_handle: { request_id: 'request-1' },
				unresolved: true,
				blocks_forward_progress: true,
				reply: null,
			},
			native_session_handle: { thread_id: 'thread-1' },
			local_context_snapshot: null,
			updated_at: '2026-04-30T08:00:01.000Z',
		},
	}
}

function createStateStore(snapshot: PersistedRunSnapshot | null): SQLiteLocalStateStore {
	return {
		getPersistedRunSnapshot: vi.fn(() => snapshot),
		close: vi.fn(),
	} as unknown as SQLiteLocalStateStore
}

describe('run interaction app use cases', () => {
	it('returns the existing redacted run-status projection and closes the store', async () => {
		const snapshot = createSnapshot()
		const stateStore = createStateStore(snapshot)
		const createStateStoreDependency = vi.fn(async () => stateStore)
		const useCases = createRunInteractionUseCases({
			createStateStore: createStateStoreDependency,
		})

		await expect(
			useCases.getRunStatus({
				runId: 'run-app-status',
				stateDbPath: 'state.sqlite',
			}),
		).resolves.toEqual(buildRunInteractionStatus(snapshot))

		expect(createStateStoreDependency).toHaveBeenCalledWith('state.sqlite')
		expect(stateStore.getPersistedRunSnapshot).toHaveBeenCalledWith('run-app-status')
		expect(stateStore.close).toHaveBeenCalledTimes(1)
	})

	it('throws RUN_NOT_FOUND for a missing run and closes the store', async () => {
		const stateStore = createStateStore(null)
		const useCases = createRunInteractionUseCases({
			createStateStore: vi.fn(async () => stateStore),
		})

		await expect(
			useCases.getRunStatus({
				runId: 'missing-run',
				stateDbPath: 'state.sqlite',
			}),
		).rejects.toMatchObject({
			code: 'RUN_NOT_FOUND',
			message: 'Run "missing-run" does not exist.',
		} satisfies Partial<AppError>)

		expect(stateStore.close).toHaveBeenCalledTimes(1)
	})

	it('closes the store when snapshot lookup fails', async () => {
		const expectedError = new Error('lookup failed')
		const stateStore = {
			getPersistedRunSnapshot: vi.fn(() => {
				throw expectedError
			}),
			close: vi.fn(),
		} as unknown as SQLiteLocalStateStore
		const useCases = createRunInteractionUseCases({
			createStateStore: vi.fn(async () => stateStore),
		})

		await expect(
			useCases.getRunStatus({
				runId: 'run-app-status',
				stateDbPath: 'state.sqlite',
			}),
		).rejects.toBe(expectedError)

		expect(stateStore.close).toHaveBeenCalledTimes(1)
	})
})
