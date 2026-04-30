import { AppError } from '../core/errors.js'
import type { JsonValue } from '../core/json.js'
import type { SQLiteLocalStateStore } from '../core/state/index.js'
import type { PersistedRunSnapshot } from '../core/state/types.js'
import { createLocalStateStore } from './local-state.js'

export interface GetRunInteractionStatusInput {
	runId: string
	stateDbPath: string
}

export interface RunInteractionUseCases {
	getRunStatus(input: GetRunInteractionStatusInput): Promise<RunInteractionStatus>
}

export interface RunInteractionUseCaseDependencies {
	createStateStore?: (stateDbPath: string) => Promise<SQLiteLocalStateStore>
}

function getLatestActiveAttempt(snapshot: {
	attempts: Array<{ node_id: string; state: string; runtime_handle: JsonValue | null }>
}): { node_id: string; state: string; runtime_handle: JsonValue | null } | null {
	return (
		[...snapshot.attempts]
			.reverse()
			.find((attempt) => attempt.state === 'in_progress' || attempt.state === 'blocked_wait') ??
		null
	)
}

export function buildRunInteractionStatus(snapshot: PersistedRunSnapshot) {
	const activeAttempt = getLatestActiveAttempt(snapshot)
	const pendingPrompt = snapshot.resume.pending_prompt
	const promptPayload =
		pendingPrompt?.payload !== null && typeof pendingPrompt?.payload === 'object'
			? (pendingPrompt.payload as { kind?: unknown; require_response?: unknown })
			: null
	const reply = pendingPrompt?.reply ?? null

	return {
		run: {
			run_id: snapshot.run.run_id,
			status: snapshot.run.status,
			resolved_revision_id: snapshot.run.resolved_revision_id,
			entry_node_id: snapshot.run.entry_node_id,
			last_boundary_sequence: snapshot.run.last_boundary_sequence,
		},
		active_attempt: activeAttempt
			? {
					node_id: activeAttempt.node_id,
					state: activeAttempt.state,
					has_runtime_handle: activeAttempt.runtime_handle !== null,
				}
			: null,
		interaction: {
			waiting_for_user: snapshot.run.status === 'waiting_for_user',
			pending_prompt: pendingPrompt
				? {
						prompt_id: pendingPrompt.prompt_id,
						attempt_id: pendingPrompt.attempt_id,
						kind: typeof promptPayload?.kind === 'string' ? promptPayload.kind : null,
						require_response:
							typeof promptPayload?.require_response === 'boolean'
								? promptPayload.require_response
								: null,
						has_request_handle: pendingPrompt.request_handle !== null,
						reply: reply
							? {
									reply_id: reply.reply_id,
									prompt_id: reply.prompt_id,
									delivery_status: reply.delivery_status,
									recorded_at: reply.recorded_at,
									delivered_at: reply.delivered_at,
								}
							: null,
					}
				: null,
			visible_transcript_messages: snapshot.visible_messages.length,
		},
		resume: {
			native_resume_available: snapshot.resume.native_resume_available,
			local_resume_available: snapshot.resume.local_resume_available,
			last_durable_boundary_sequence: snapshot.resume.last_durable_boundary_sequence,
			last_durable_boundary_kind: snapshot.resume.last_durable_boundary_kind,
			has_native_session_handle: snapshot.resume.native_session_handle !== null,
		},
		redaction: {
			prompt_payload_omitted: pendingPrompt !== null,
			reply_payload_omitted: reply !== null,
			reason:
				'run-status omits prompt and reply payload content; use the local state database only under the project data-retention policy.',
		},
	}
}

export type RunInteractionStatus = ReturnType<typeof buildRunInteractionStatus>

export function createRunInteractionUseCases(
	dependencies: RunInteractionUseCaseDependencies = {},
): RunInteractionUseCases {
	const createStateStore = dependencies.createStateStore ?? createLocalStateStore

	return {
		async getRunStatus(input) {
			const stateStore = await createStateStore(input.stateDbPath)
			try {
				const snapshot = stateStore.getPersistedRunSnapshot(input.runId)
				if (!snapshot) {
					throw new AppError('RUN_NOT_FOUND', `Run "${input.runId}" does not exist.`)
				}
				return buildRunInteractionStatus(snapshot)
			} finally {
				stateStore.close()
			}
		},
	}
}

const runInteractionUseCases = createRunInteractionUseCases()

export async function getRunInteractionStatus(
	input: GetRunInteractionStatusInput,
): Promise<RunInteractionStatus> {
	return await runInteractionUseCases.getRunStatus(input)
}
