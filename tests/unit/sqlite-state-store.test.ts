import { mkdtemp, rm } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { AppError } from '../../src/core/errors.js'
import { SQLiteLocalStateStore } from '../../src/core/state/index.js'

const storesToClose: SQLiteLocalStateStore[] = []
const tempDirsToRemove: string[] = []

async function createStore(): Promise<SQLiteLocalStateStore> {
	const tempDir = await mkdtemp(path.join(os.tmpdir(), 'dennett-phase7-state-'))
	const store = new SQLiteLocalStateStore({
		database_path: path.join(tempDir, 'local-state.sqlite'),
	})
	storesToClose.push(store)
	tempDirsToRemove.push(tempDir)
	return store
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

describe('SQLiteLocalStateStore', () => {
	it('creates a discoverable run with pinned revision and initial committed vars', async () => {
		const store = await createStore()
		const run = store.createRun({
			run_id: 'run-1',
			logical_agent_id: 'agent.alpha',
			resolved_revision_id: 'rev-2026-04-22',
			entry_node_id: 'entry',
			started_via: 'direct',
			params: {
				topic: 'phase-6',
			},
			initial_vars: {
				seed: 'stable',
			},
			created_at: '2026-04-22T10:00:00.000Z',
		})

		const snapshot = store.getPersistedRunSnapshot(run.run_id)
		expect(snapshot).not.toBeNull()
		expect(snapshot?.run).toEqual({
			run_id: 'run-1',
			logical_agent_id: 'agent.alpha',
			resolved_revision_id: 'rev-2026-04-22',
			entry_node_id: 'entry',
			started_via: 'direct',
			status: 'running',
			params: {
				topic: 'phase-6',
			},
			event: null,
			last_attempt_sequence: 0,
			last_boundary_sequence: 0,
			created_at: '2026-04-22T10:00:00.000Z',
			updated_at: '2026-04-22T10:00:00.000Z',
		})
		expect(snapshot?.current_vars).toEqual({
			seed: 'stable',
		})
		expect(snapshot?.chat).toEqual({
			chat_id: expect.any(String),
			run_id: 'run-1',
			resolved_revision_id: 'rev-2026-04-22',
			policy: {
				prefer_native_resume: true,
				store_visible_messages: true,
				store_context_window: true,
				allow_fresh_start: true,
			},
			created_at: '2026-04-22T10:00:00.000Z',
			updated_at: '2026-04-22T10:00:00.000Z',
		})
		expect(snapshot?.visible_messages).toEqual([])
		expect(snapshot?.resume).toMatchObject({
			resolved_revision_id: 'rev-2026-04-22',
			native_resume_available: false,
			local_resume_available: false,
			last_durable_boundary_sequence: null,
			last_durable_boundary_kind: null,
			last_attempt_id: null,
			pending_prompt: null,
		})
	})

	it('supports repeated attempts for the same node and keeps the latest committed output', async () => {
		const store = await createStore()
		const run = store.createRun({
			run_id: 'run-2',
			resolved_revision_id: 'rev-repeat',
			entry_node_id: 'node-a',
			started_via: 'direct',
		})

		const firstAttempt = store.startNodeAttempt({
			run_id: run.run_id,
			node_id: 'node-a',
			output_mode: 'text',
			started_at: '2026-04-22T10:01:00.000Z',
		})
		store.commitNodeSuccess({
			attempt_id: firstAttempt.attempt_id,
			output: {
				mode: 'text',
				text: 'first output',
			},
			vars: {
				counter: 1,
			},
			run_status: 'running',
			resume: {
				native_resume_available: false,
				local_resume_available: true,
			},
			committed_at: '2026-04-22T10:02:00.000Z',
		})

		const secondAttempt = store.startNodeAttempt({
			run_id: run.run_id,
			node_id: 'node-a',
			output_mode: 'text',
			started_at: '2026-04-22T10:03:00.000Z',
		})
		store.commitNodeSuccess({
			attempt_id: secondAttempt.attempt_id,
			output: {
				mode: 'text',
				text: 'second output',
			},
			vars: {
				counter: 2,
			},
			run_status: 'completed',
			resume: {
				native_resume_available: false,
				local_resume_available: false,
			},
			committed_at: '2026-04-22T10:04:00.000Z',
		})

		const attempts = store.listNodeAttempts(run.run_id)
		const latestOutput = store.getLatestCommittedNodeOutput(run.run_id, 'node-a')
		const persistedRun = store.getRun(run.run_id)

		expect(attempts.map((attempt) => attempt.attempt_sequence)).toEqual([1, 2])
		expect(attempts.map((attempt) => attempt.node_id)).toEqual(['node-a', 'node-a'])
		expect(latestOutput).toMatchObject({
			node_id: 'node-a',
			boundary_sequence: 2,
			output: {
				mode: 'text',
				text: 'second output',
			},
		})
		expect(persistedRun?.status).toBe('completed')
		expect(persistedRun?.last_attempt_sequence).toBe(2)
		expect(persistedRun?.last_boundary_sequence).toBe(2)
	})

	it('materializes the latest committed vars snapshot from successful node commits', async () => {
		const store = await createStore()
		const run = store.createRun({
			run_id: 'run-3',
			resolved_revision_id: 'rev-vars',
			entry_node_id: 'node-json',
			started_via: 'direct',
			initial_vars: {
				counter: 0,
			},
		})

		const attempt = store.startNodeAttempt({
			run_id: run.run_id,
			node_id: 'node-json',
			output_mode: 'json',
		})
		store.commitNodeSuccess({
			attempt_id: attempt.attempt_id,
			output: {
				mode: 'json',
				json: {
					counter: 1,
					status: 'ok',
				},
			},
			vars: {
				counter: 1,
				status: 'ok',
			},
			run_status: 'completed',
			resume: {
				native_resume_available: false,
				local_resume_available: false,
			},
		})

		expect(store.getCurrentVars(run.run_id)).toEqual({
			counter: 1,
			status: 'ok',
		})
		expect(store.getLatestCommittedNodeOutput(run.run_id, 'node-json')).toMatchObject({
			output: {
				mode: 'json',
				json: {
					counter: 1,
					status: 'ok',
				},
			},
		})
	})

	it('persists visible chat messages with explicit kinds and preserves their order', async () => {
		const store = await createStore()
		const run = store.createRun({
			run_id: 'run-chat-1',
			resolved_revision_id: 'rev-chat',
			entry_node_id: 'node-chat',
			started_via: 'direct',
			created_at: '2026-04-22T10:04:00.000Z',
		})

		const userMessage = store.appendVisibleChatMessage({
			run_id: run.run_id,
			message_id: 'msg-user-1',
			kind: 'user_message',
			payload: {
				text: 'Start the run',
			},
			created_at: '2026-04-22T10:04:01.000Z',
		})
		const progressMessage = store.appendVisibleChatMessage({
			run_id: run.run_id,
			message_id: 'msg-agent-1',
			kind: 'agent_progress',
			payload: {
				text: 'Working on it',
			},
			created_at: '2026-04-22T10:04:02.000Z',
		})
		const finalMessage = store.appendVisibleChatMessage({
			run_id: run.run_id,
			message_id: 'msg-agent-2',
			kind: 'agent_final',
			payload: {
				text: 'Completed',
			},
			created_at: '2026-04-22T10:04:03.000Z',
		})

		const messages = store.listVisibleChatMessages(run.run_id)
		const snapshot = store.getPersistedRunSnapshot(run.run_id)

		expect(userMessage).toEqual({
			message_id: 'msg-user-1',
			chat_id: snapshot?.chat?.chat_id,
			run_id: 'run-chat-1',
			message_sequence: 1,
			kind: 'user_message',
			payload: {
				text: 'Start the run',
			},
			created_at: '2026-04-22T10:04:01.000Z',
		})
		expect(progressMessage).toEqual({
			message_id: 'msg-agent-1',
			chat_id: snapshot?.chat?.chat_id,
			run_id: 'run-chat-1',
			message_sequence: 2,
			kind: 'agent_progress',
			payload: {
				text: 'Working on it',
			},
			created_at: '2026-04-22T10:04:02.000Z',
		})
		expect(finalMessage).toEqual({
			message_id: 'msg-agent-2',
			chat_id: snapshot?.chat?.chat_id,
			run_id: 'run-chat-1',
			message_sequence: 3,
			kind: 'agent_final',
			payload: {
				text: 'Completed',
			},
			created_at: '2026-04-22T10:04:03.000Z',
		})
		expect(messages).toEqual([userMessage, progressMessage, finalMessage])
		expect(snapshot?.visible_messages).toEqual(messages)
		expect(snapshot?.chat).toMatchObject({
			run_id: 'run-chat-1',
			resolved_revision_id: 'rev-chat',
			policy: {
				prefer_native_resume: true,
				store_visible_messages: true,
				store_context_window: true,
				allow_fresh_start: true,
			},
			updated_at: '2026-04-22T10:04:03.000Z',
		})
	})

	it('commits blocked wait state and pending prompt metadata as one durable boundary', async () => {
		const store = await createStore()
		const run = store.createRun({
			run_id: 'run-4',
			resolved_revision_id: 'rev-blocked',
			entry_node_id: 'node-chat',
			started_via: 'direct',
		})

		const attempt = store.startNodeAttempt({
			run_id: run.run_id,
			node_id: 'node-chat',
			output_mode: 'text',
		})
		const committedAttempt = store.commitBlockedAttempt({
			attempt_id: attempt.attempt_id,
			pending_prompt: {
				prompt_id: 'prompt-1',
				payload: {
					kind: 'text',
					text: 'Need your answer',
					require_response: true,
				},
				request_handle: {
					execution: 'native-req-1',
				},
			},
			resume: {
				native_resume_available: true,
				local_resume_available: true,
				native_session_handle: {
					session: 'native-session-1',
				},
				local_context_snapshot: {
					messages: ['Need your answer'],
				},
			},
			committed_at: '2026-04-22T10:05:00.000Z',
		})

		const runRecord = store.getRun(run.run_id)
		const resume = store.getResumeMetadata(run.run_id)

		expect(committedAttempt).toMatchObject({
			state: 'blocked_wait',
			outcome: null,
			blocked_on_user_prompt: true,
			resume_boundary_sequence: 1,
			committed_at: '2026-04-22T10:05:00.000Z',
		})
		expect(runRecord?.status).toBe('waiting_for_user')
		expect(resume).toEqual({
			run_id: 'run-4',
			resolved_revision_id: 'rev-blocked',
			native_resume_available: true,
			local_resume_available: true,
			last_durable_boundary_sequence: 1,
			last_durable_boundary_kind: 'blocked_prompt_wait',
			last_attempt_id: attempt.attempt_id,
			pending_prompt: {
				run_id: 'run-4',
				attempt_id: attempt.attempt_id,
				prompt_id: 'prompt-1',
				payload: {
					kind: 'text',
					text: 'Need your answer',
					require_response: true,
				},
				request_handle: {
					execution: 'native-req-1',
				},
				unresolved: true,
				blocks_forward_progress: true,
			},
			native_session_handle: {
				session: 'native-session-1',
			},
			local_context_snapshot: {
				messages: ['Need your answer'],
			},
			updated_at: '2026-04-22T10:05:00.000Z',
		})
		expect(store.getLatestCommittedNodeOutput(run.run_id, 'node-chat')).toBeNull()
	})

	it('suppresses visible transcript persistence when chat policy disables it while keeping blocked prompt state', async () => {
		const store = await createStore()
		const run = store.createRun({
			run_id: 'run-chat-2',
			resolved_revision_id: 'rev-chat-suppressed',
			entry_node_id: 'node-chat',
			started_via: 'direct',
			chat: {
				chat_id: 'chat-suppressed',
				policy: {
					store_visible_messages: false,
					store_context_window: true,
					prefer_native_resume: false,
					allow_fresh_start: false,
				},
			},
			created_at: '2026-04-22T10:05:30.000Z',
		})

		const appendedMessage = store.appendVisibleChatMessage({
			run_id: run.run_id,
			message_id: 'msg-suppressed-1',
			kind: 'blocking_prompt',
			payload: {
				text: 'Answer required',
			},
			created_at: '2026-04-22T10:05:31.000Z',
		})

		const attempt = store.startNodeAttempt({
			run_id: run.run_id,
			node_id: 'node-chat',
			output_mode: 'text',
			started_at: '2026-04-22T10:05:32.000Z',
		})
		store.commitBlockedAttempt({
			attempt_id: attempt.attempt_id,
			pending_prompt: {
				prompt_id: 'prompt-hidden',
				payload: {
					text: 'Answer required',
					require_response: true,
				},
			},
			resume: {
				native_resume_available: false,
				local_resume_available: true,
			},
			committed_at: '2026-04-22T10:05:33.000Z',
		})

		const chat = store.getChatRecord(run.run_id)
		const messages = store.listVisibleChatMessages(run.run_id)
		const snapshot = store.getPersistedRunSnapshot(run.run_id)

		expect(appendedMessage).toBeNull()
		expect(chat).toEqual({
			chat_id: 'chat-suppressed',
			run_id: 'run-chat-2',
			resolved_revision_id: 'rev-chat-suppressed',
			policy: {
				prefer_native_resume: false,
				store_visible_messages: false,
				store_context_window: true,
				allow_fresh_start: false,
			},
			created_at: '2026-04-22T10:05:30.000Z',
			updated_at: '2026-04-22T10:05:31.000Z',
		})
		expect(messages).toEqual([])
		expect(snapshot?.visible_messages).toEqual([])
		expect(snapshot?.resume.pending_prompt).toEqual({
			run_id: 'run-chat-2',
			attempt_id: attempt.attempt_id,
			prompt_id: 'prompt-hidden',
			payload: {
				text: 'Answer required',
				require_response: true,
			},
			request_handle: null,
			unresolved: true,
			blocks_forward_progress: true,
		})
	})

	it('omits durable context snapshots when chat policy disables the context window', async () => {
		const store = await createStore()
		const run = store.createRun({
			run_id: 'run-chat-3',
			resolved_revision_id: 'rev-chat-window-off',
			entry_node_id: 'node-chat',
			started_via: 'direct',
			chat: {
				chat_id: 'chat-window-off',
				policy: {
					store_context_window: false,
				},
			},
			resume: {
				native_resume_available: true,
				local_resume_available: true,
				native_session_handle: {
					session: 'native-session-2',
				},
				local_context_snapshot: {
					messages: ['hidden context'],
				},
			},
			created_at: '2026-04-22T10:05:40.000Z',
		})

		const initialSnapshot = store.getPersistedRunSnapshot(run.run_id)
		expect(initialSnapshot?.resume).toMatchObject({
			native_resume_available: true,
			local_resume_available: true,
			native_session_handle: {
				session: 'native-session-2',
			},
			local_context_snapshot: null,
		})

		const attempt = store.startNodeAttempt({
			run_id: run.run_id,
			node_id: 'node-chat',
			output_mode: 'json',
			started_at: '2026-04-22T10:05:41.000Z',
		})
		store.commitNodeTerminalOutcome({
			attempt_id: attempt.attempt_id,
			outcome: 'runtime_error',
			run_status: 'failed',
			resume: {
				native_resume_available: true,
				local_resume_available: true,
				native_session_handle: {
					session: 'native-session-3',
				},
				local_context_snapshot: {
					messages: ['still hidden'],
				},
			},
			committed_at: '2026-04-22T10:05:42.000Z',
		})

		expect(store.getResumeMetadata(run.run_id)).toMatchObject({
			native_resume_available: true,
			local_resume_available: true,
			native_session_handle: {
				session: 'native-session-3',
			},
			local_context_snapshot: null,
			last_durable_boundary_sequence: 1,
			last_durable_boundary_kind: 'node_attempt_terminal',
			last_attempt_id: attempt.attempt_id,
		})
		expect(store.getPersistedRunSnapshot(run.run_id)?.resume.local_context_snapshot).toBeNull()
	})

	it('does not expose in-flight attempts as committed success', async () => {
		const store = await createStore()
		const run = store.createRun({
			run_id: 'run-5',
			resolved_revision_id: 'rev-inflight',
			entry_node_id: 'node-a',
			started_via: 'direct',
		})

		const committedAttempt = store.startNodeAttempt({
			run_id: run.run_id,
			node_id: 'node-a',
			output_mode: 'text',
		})
		store.commitNodeSuccess({
			attempt_id: committedAttempt.attempt_id,
			output: {
				mode: 'text',
				text: 'stable output',
			},
			vars: {
				stable: true,
			},
			run_status: 'running',
			resume: {
				native_resume_available: false,
				local_resume_available: true,
			},
			committed_at: '2026-04-22T10:06:00.000Z',
		})

		const inflightAttempt = store.startNodeAttempt({
			run_id: run.run_id,
			node_id: 'node-a',
			output_mode: 'text',
			started_at: '2026-04-22T10:07:00.000Z',
		})

		const latestOutput = store.getLatestCommittedNodeOutput(run.run_id, 'node-a')
		const snapshot = store.getPersistedRunSnapshot(run.run_id)

		expect(latestOutput).toMatchObject({
			attempt_id: committedAttempt.attempt_id,
			output: {
				mode: 'text',
				text: 'stable output',
			},
			boundary_sequence: 1,
		})
		expect(snapshot?.attempts).toEqual([
			expect.objectContaining({
				attempt_id: committedAttempt.attempt_id,
				state: 'committed_terminal',
				outcome: 'success',
			}),
			expect.objectContaining({
				attempt_id: inflightAttempt.attempt_id,
				state: 'in_progress',
				outcome: null,
				committed_output_id: null,
			}),
		])
		expect(snapshot?.resume.last_durable_boundary_sequence).toBe(1)
		expect(snapshot?.resume.last_attempt_id).toBe(committedAttempt.attempt_id)
	})

	it('rejects starting a second in-progress attempt in the same run', async () => {
		const store = await createStore()
		const run = store.createRun({
			run_id: 'run-5b',
			resolved_revision_id: 'rev-single-active',
			entry_node_id: 'node-a',
			started_via: 'direct',
		})

		const firstAttempt = store.startNodeAttempt({
			run_id: run.run_id,
			node_id: 'node-a',
			output_mode: 'text',
			started_at: '2026-04-22T10:07:30.000Z',
		})

		let caught: unknown
		try {
			store.startNodeAttempt({
				run_id: run.run_id,
				node_id: 'node-b',
				output_mode: 'json',
				started_at: '2026-04-22T10:07:31.000Z',
			})
		} catch (error) {
			caught = error
		}

		expect(caught).toBeInstanceOf(AppError)
		expect((caught as AppError).code).toBe('ACTIVE_NODE_ATTEMPT_EXISTS')
		expect(store.listNodeAttempts(run.run_id)).toEqual([
			expect.objectContaining({
				attempt_id: firstAttempt.attempt_id,
				node_id: 'node-a',
				state: 'in_progress',
			}),
		])
	})

	it('rejects success commits whose output mode contradicts the attempt', async () => {
		const store = await createStore()
		const run = store.createRun({
			run_id: 'run-5c',
			resolved_revision_id: 'rev-output-mode',
			entry_node_id: 'node-a',
			started_via: 'direct',
			initial_vars: {
				stable: true,
			},
		})

		const attempt = store.startNodeAttempt({
			run_id: run.run_id,
			node_id: 'node-a',
			output_mode: 'text',
			started_at: '2026-04-22T10:07:45.000Z',
		})

		let caught: unknown
		try {
			store.commitNodeSuccess({
				attempt_id: attempt.attempt_id,
				output: {
					mode: 'json',
					json: {
						unexpected: true,
					},
				},
				vars: {
					stable: false,
				},
				run_status: 'completed',
				resume: {
					native_resume_available: false,
					local_resume_available: false,
				},
				committed_at: '2026-04-22T10:07:46.000Z',
			})
		} catch (error) {
			caught = error
		}

		expect(caught).toBeInstanceOf(AppError)
		expect((caught as AppError).code).toBe('INVALID_OUTPUT_MODE')
		expect(store.getLatestCommittedNodeOutput(run.run_id, 'node-a')).toBeNull()
		expect(store.getCurrentVars(run.run_id)).toEqual({
			stable: true,
		})
		expect(store.getRun(run.run_id)).toMatchObject({
			status: 'running',
			last_boundary_sequence: 0,
		})
		expect(store.listNodeAttempts(run.run_id)).toEqual([
			expect.objectContaining({
				attempt_id: attempt.attempt_id,
				output_mode: 'text',
				state: 'in_progress',
				committed_output_id: null,
				committed_at: null,
			}),
		])
	})

	it('stores terminal non-success outcomes without fabricating committed outputs', async () => {
		const store = await createStore()
		const run = store.createRun({
			run_id: 'run-6',
			resolved_revision_id: 'rev-failure',
			entry_node_id: 'node-fail',
			started_via: 'direct',
			initial_vars: {
				untouched: true,
			},
		})

		const attempt = store.startNodeAttempt({
			run_id: run.run_id,
			node_id: 'node-fail',
			output_mode: 'json',
		})
		const committedAttempt = store.commitNodeTerminalOutcome({
			attempt_id: attempt.attempt_id,
			outcome: 'runtime_error',
			run_status: 'failed',
			resume: {
				native_resume_available: false,
				local_resume_available: true,
				local_context_snapshot: {
					last_safe_node: 'entry',
				},
			},
			committed_at: '2026-04-22T10:08:00.000Z',
		})

		expect(committedAttempt).toMatchObject({
			state: 'committed_terminal',
			outcome: 'runtime_error',
			committed_output_id: null,
			resume_boundary_sequence: 1,
		})
		expect(store.getLatestCommittedNodeOutput(run.run_id, 'node-fail')).toBeNull()
		expect(store.getCurrentVars(run.run_id)).toEqual({
			untouched: true,
		})
		expect(store.getRun(run.run_id)?.status).toBe('failed')
		expect(store.getResumeMetadata(run.run_id)).toMatchObject({
			local_resume_available: true,
			last_durable_boundary_sequence: 1,
			last_durable_boundary_kind: 'node_attempt_terminal',
			last_attempt_id: attempt.attempt_id,
			pending_prompt: null,
		})
	})

	it('reopens a resumable terminal run for explicit local resume without losing its durable boundary', async () => {
		const store = await createStore()
		const run = store.createRun({
			run_id: 'run-7',
			resolved_revision_id: 'rev-resume-terminal',
			entry_node_id: 'node-fail',
			started_via: 'direct',
			initial_vars: {
				untouched: true,
			},
		})

		const failedAttempt = store.startNodeAttempt({
			run_id: run.run_id,
			node_id: 'node-fail',
			output_mode: 'json',
			started_at: '2026-04-22T10:09:00.000Z',
		})
		store.commitNodeTerminalOutcome({
			attempt_id: failedAttempt.attempt_id,
			outcome: 'runtime_error',
			run_status: 'failed',
			resume: {
				native_resume_available: false,
				local_resume_available: true,
			},
			committed_at: '2026-04-22T10:09:10.000Z',
		})

		const reopenedRun = store.reopenRunForExplicitResume(run.run_id, 'rev-resume-terminal')

		expect(reopenedRun).toMatchObject({
			run_id: 'run-7',
			status: 'running',
		})
		expect(store.getResumeMetadata(run.run_id)).toMatchObject({
			local_resume_available: true,
			last_durable_boundary_sequence: 1,
			last_durable_boundary_kind: 'node_attempt_terminal',
			last_attempt_id: failedAttempt.attempt_id,
		})
		expect(store.getCurrentVars(run.run_id)).toEqual({
			untouched: true,
		})
		expect(store.getLatestCommittedNodeOutput(run.run_id, 'node-fail')).toBeNull()

		const resumedAttempt = store.startNodeAttempt({
			run_id: run.run_id,
			node_id: 'node-fail',
			output_mode: 'json',
			started_at: '2026-04-22T10:09:20.000Z',
		})

		expect(resumedAttempt).toMatchObject({
			node_id: 'node-fail',
			attempt_sequence: 2,
			state: 'in_progress',
		})
		expect(store.getRun(run.run_id)).toMatchObject({
			status: 'running',
			last_attempt_sequence: 2,
			last_boundary_sequence: 1,
		})
	})

	it('persists trigger records and event dispatch outcomes independently from graph-visible run state', async () => {
		const store = await createStore()

		const trigger = store.upsertTriggerRecord({
			trigger_id: 'trigger-1',
			logical_agent_id: 'agent.events',
			trigger_ref: 'mailbox://triage',
			created_at: '2026-04-22T10:10:00.000Z',
			updated_at: '2026-04-22T10:10:00.000Z',
		})
		const event = store.createEventRecord({
			event_id: 'event-1',
			trigger_id: trigger.trigger_id,
			logical_agent_id: 'agent.events',
			payload: {
				topic: 'phase-9',
			},
			launch_note: 'operator note',
			created_at: '2026-04-22T10:10:01.000Z',
		})

		const run = store.createRun({
			run_id: 'run-event-1',
			logical_agent_id: 'agent.events',
			resolved_revision_id: 'rev-event',
			entry_node_id: 'start',
			started_via: 'event',
			event: {
				payload: {
					topic: 'phase-9',
				},
				launch_note: 'operator note',
			},
			created_at: '2026-04-22T10:10:02.000Z',
		})

		const dispatched = store.markEventDispatched({
			event_id: event.event_id,
			run_id: run.run_id,
			resolved_revision_id: 'rev-event',
			dispatched_at: '2026-04-22T10:10:03.000Z',
		})

		expect(store.getTriggerRecord(trigger.trigger_id)).toEqual(trigger)
		expect(store.listTriggerRecords('agent.events')).toEqual([trigger])
		expect(dispatched).toEqual({
			event_id: 'event-1',
			trigger_id: 'trigger-1',
			logical_agent_id: 'agent.events',
			payload: {
				topic: 'phase-9',
			},
			launch_note: 'operator note',
			dispatch_status: 'dispatched',
			run_id: 'run-event-1',
			resolved_revision_id: 'rev-event',
			dispatch_error_code: null,
			dispatch_error_message: null,
			created_at: '2026-04-22T10:10:01.000Z',
			dispatched_at: '2026-04-22T10:10:03.000Z',
			updated_at: '2026-04-22T10:10:03.000Z',
		})
		expect(store.listEventRecords({ trigger_id: trigger.trigger_id })).toEqual([dispatched])
		expect(store.getPersistedRunSnapshot(run.run_id)?.run.event).toEqual({
			payload: {
				topic: 'phase-9',
			},
			launch_note: 'operator note',
		})
	})

	it('records failed event dispatch attempts without inventing a run', async () => {
		const store = await createStore()

		store.upsertTriggerRecord({
			trigger_id: 'trigger-2',
			logical_agent_id: 'agent.events.fail',
			trigger_ref: 'mailbox://fail',
			created_at: '2026-04-22T10:11:00.000Z',
			updated_at: '2026-04-22T10:11:00.000Z',
		})
		store.createEventRecord({
			event_id: 'event-2',
			trigger_id: 'trigger-2',
			logical_agent_id: 'agent.events.fail',
			payload: {
				topic: 'phase-9',
			},
			created_at: '2026-04-22T10:11:01.000Z',
		})

		const failed = store.markEventDispatchFailed({
			event_id: 'event-2',
			error_code: 'AGENT_NOT_FOUND',
			error_message: 'Agent "agent.events.fail" does not exist.',
			dispatched_at: '2026-04-22T10:11:02.000Z',
		})

		expect(failed).toMatchObject({
			event_id: 'event-2',
			dispatch_status: 'failed',
			run_id: null,
			resolved_revision_id: null,
			dispatch_error_code: 'AGENT_NOT_FOUND',
			dispatch_error_message: 'Agent "agent.events.fail" does not exist.',
			dispatched_at: '2026-04-22T10:11:02.000Z',
		})
	})

	it('persists local memory provider registrations with stable typed fields and update semantics', async () => {
		const store = await createStore()

		const firstRecord = store.upsertMemoryProviderRecord({
			provider_id: 'mem0-local',
			codex_ref: 'primary_memory',
			provider_family: 'mem0',
			display_name: 'Primary Mem0',
			transport: 'api',
			supported_capabilities: ['read', 'write', 'entity_scoped'],
			config: {
				base_url: 'http://127.0.0.1:8000',
			},
			created_at: '2026-04-23T08:00:00.000Z',
			updated_at: '2026-04-23T08:00:00.000Z',
		})

		const updatedRecord = store.upsertMemoryProviderRecord({
			provider_id: 'mem0-local',
			codex_ref: 'primary_memory',
			provider_family: 'mem0',
			display_name: 'Primary Mem0 v2',
			transport: 'mcp',
			status: 'available',
			supported_capabilities: ['read', 'write', 'entity_scoped', 'mcp_transport'],
			config: {
				server_name: 'mem0',
			},
			status_code: 'HEALTHY',
			status_message: 'Connected',
			last_checked_at: '2026-04-23T08:05:00.000Z',
			updated_at: '2026-04-23T08:05:00.000Z',
		})

		expect(firstRecord).toEqual({
			provider_id: 'mem0-local',
			codex_ref: 'primary_memory',
			provider_family: 'mem0',
			display_name: 'Primary Mem0',
			transport: 'api',
			status: 'configured',
			supported_capabilities: ['read', 'write', 'entity_scoped'],
			config: {
				base_url: 'http://127.0.0.1:8000',
			},
			status_code: null,
			status_message: null,
			last_checked_at: null,
			created_at: '2026-04-23T08:00:00.000Z',
			updated_at: '2026-04-23T08:00:00.000Z',
		})
		expect(updatedRecord).toEqual({
			provider_id: 'mem0-local',
			codex_ref: 'primary_memory',
			provider_family: 'mem0',
			display_name: 'Primary Mem0 v2',
			transport: 'mcp',
			status: 'available',
			supported_capabilities: ['read', 'write', 'entity_scoped', 'mcp_transport'],
			config: {
				server_name: 'mem0',
			},
			status_code: 'HEALTHY',
			status_message: 'Connected',
			last_checked_at: '2026-04-23T08:05:00.000Z',
			created_at: '2026-04-23T08:00:00.000Z',
			updated_at: '2026-04-23T08:05:00.000Z',
		})
		expect(store.getMemoryProviderRecord('mem0-local')).toEqual(updatedRecord)
		expect(store.getMemoryProviderRecordByCodexRef('primary_memory')).toEqual(updatedRecord)
		expect(store.listMemoryProviderRecords()).toEqual([updatedRecord])
		expect(store.listMemoryProviderRecords('mem0')).toEqual([updatedRecord])
	})

	it('updates local memory provider status metadata without rewriting its registration identity', async () => {
		const store = await createStore()

		store.upsertMemoryProviderRecord({
			provider_id: 'mem0-local',
			codex_ref: 'primary_memory',
			provider_family: 'mem0',
			transport: 'api',
			supported_capabilities: ['read'],
			config: {},
			created_at: '2026-04-23T08:10:00.000Z',
			updated_at: '2026-04-23T08:10:00.000Z',
		})

		const updated = store.updateMemoryProviderStatus({
			provider_id: 'mem0-local',
			status: 'error',
			status_code: 'CONNECTION_FAILED',
			status_message: 'Dial failed',
			last_checked_at: '2026-04-23T08:11:00.000Z',
			updated_at: '2026-04-23T08:11:00.000Z',
		})

		expect(updated).toMatchObject({
			provider_id: 'mem0-local',
			codex_ref: 'primary_memory',
			provider_family: 'mem0',
			status: 'error',
			status_code: 'CONNECTION_FAILED',
			status_message: 'Dial failed',
			last_checked_at: '2026-04-23T08:11:00.000Z',
			created_at: '2026-04-23T08:10:00.000Z',
			updated_at: '2026-04-23T08:11:00.000Z',
		})
	})

	it('persists managed subagent records with durable lineage, task-package snapshot, terminal result, and close disposition', async () => {
		const store = await createStore()
		store.createRun({
			run_id: 'parent-run-managed',
			resolved_revision_id: 'rev-parent-managed',
			entry_node_id: 'entry',
			started_via: 'direct',
			created_at: '2026-04-23T09:00:00.000Z',
		})

		const created = store.createManagedSubagentRecord({
			subagent_id: 'subagent-1',
			child_run_id: 'child-run-1',
			child_role: 'worker',
			child_logical_agent_id: 'agent.child',
			child_resolved_revision_id: 'rev-child',
			lineage: {
				root_run_id: 'parent-run-managed',
				parent_run_id: 'parent-run-managed',
				parent_task_id: 'task-managed',
				depth: 1,
			},
			task_package: {
				agent_ref: 'agent.child',
				objective: 'Do the delegated work',
				input_message: 'child input',
				acceptance_criteria: ['Return a terminal payload'],
				prohibitions: ['Do not leave write_set'],
				write_set: {
					mode: 'allow_list',
					items: [
						{
							resource_kind: 'file',
							resource_ref: 'src/core/example.ts',
							scope: 'exact',
							access: 'create_or_modify',
						},
					],
				},
			},
			created_at: '2026-04-23T09:00:01.000Z',
			updated_at: '2026-04-23T09:00:01.000Z',
		})

		const terminal = store.markManagedSubagentTerminal({
			subagent_id: created.subagent_id,
			terminal_result: {
				outcome: 'accepted',
				child_run_status: 'completed',
				final_output: 'done',
				final_output_mode: 'text',
				final_payload: {
					summary: 'done',
				},
				findings: null,
				reason_code: null,
			},
			terminal_at: '2026-04-23T09:00:02.000Z',
		})

		const closed = store.closeManagedSubagent({
			subagent_id: created.subagent_id,
			close_disposition: 'accepted_by_parent',
			closed_at: '2026-04-23T09:00:03.000Z',
		})

		expect(created).toMatchObject({
			subagent_id: 'subagent-1',
			child_run_id: 'child-run-1',
			state: 'running',
			terminal_result: null,
			close_disposition: null,
		})
		expect(terminal).toMatchObject({
			subagent_id: 'subagent-1',
			state: 'terminal',
			terminal_result: {
				outcome: 'accepted',
				child_run_status: 'completed',
				final_output: 'done',
				final_output_mode: 'text',
			},
			terminal_at: '2026-04-23T09:00:02.000Z',
		})
		expect(closed).toMatchObject({
			subagent_id: 'subagent-1',
			state: 'closed',
			close_disposition: 'accepted_by_parent',
			closed_at: '2026-04-23T09:00:03.000Z',
		})
		expect(store.listManagedSubagentRecords({ parent_run_id: 'parent-run-managed' })).toEqual([
			closed,
		])
	})

	it('rejects sibling managed subagent launches with overlapping write sets before the second child starts', async () => {
		const store = await createStore()
		store.createRun({
			run_id: 'parent-run-overlap',
			resolved_revision_id: 'rev-parent-overlap',
			entry_node_id: 'entry',
			started_via: 'direct',
		})

		store.createManagedSubagentRecord({
			subagent_id: 'subagent-overlap-1',
			child_run_id: 'child-run-overlap-1',
			child_role: 'worker',
			child_logical_agent_id: 'agent.child',
			child_resolved_revision_id: 'rev-child',
			lineage: {
				root_run_id: 'parent-run-overlap',
				parent_run_id: 'parent-run-overlap',
				parent_task_id: 'task-overlap',
				depth: 1,
			},
			task_package: {
				agent_ref: 'agent.child',
				objective: 'First worker',
				input_message: 'first',
				acceptance_criteria: ['done'],
				prohibitions: [],
				write_set: {
					mode: 'allow_list',
					items: [
						{
							resource_kind: 'directory',
							resource_ref: 'src/core/subagents',
							scope: 'descendants',
							access: 'create_or_modify',
						},
					],
				},
			},
		})

		let caught: unknown
		try {
			store.createManagedSubagentRecord({
				subagent_id: 'subagent-overlap-2',
				child_run_id: 'child-run-overlap-2',
				child_role: 'worker',
				child_logical_agent_id: 'agent.child',
				child_resolved_revision_id: 'rev-child',
				lineage: {
					root_run_id: 'parent-run-overlap',
					parent_run_id: 'parent-run-overlap',
					parent_task_id: 'task-overlap',
					depth: 1,
				},
				task_package: {
					agent_ref: 'agent.child',
					objective: 'Second worker',
					input_message: 'second',
					acceptance_criteria: ['done'],
					prohibitions: [],
					write_set: {
						mode: 'allow_list',
						items: [
							{
								resource_kind: 'file',
								resource_ref: 'src/core/subagents/example.ts',
								scope: 'exact',
								access: 'modify_existing',
							},
						],
					},
				},
			})
		} catch (error) {
			caught = error
		}

		expect(caught).toBeInstanceOf(AppError)
		expect((caught as AppError).code).toBe('SUBAGENT_WRITE_SET_CONFLICT')
	})
})
