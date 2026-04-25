import { createHash, randomUUID } from 'node:crypto'
import { constants as fsConstants } from 'node:fs'
import { access } from 'node:fs/promises'
import path from 'node:path'
import type { AgentFile } from './agent-file.js'
import { writeTextFileAtomically } from './atomic-write.js'
import { AppError } from './errors.js'
import type { JsonObject, JsonValue } from './json.js'
import { computeResolvedRevisionId } from './resolved-revision.js'
import { loadAndValidateAgentFile } from './schema.js'
import type { SQLiteLocalStateStore } from './state/index.js'
import type {
	AgentLifecycleStatusRecord,
	AgentRevisionAvailabilityState,
	AgentRevisionRecord,
	EventRecord,
	TriggerRecord,
} from './state/types.js'

function nowIso(): string {
	return new Date().toISOString()
}

function sha256Hex(value: string): string {
	return createHash('sha256').update(value).digest('hex')
}

async function pathExists(filePath: string): Promise<boolean> {
	try {
		await access(filePath, fsConstants.R_OK)
		return true
	} catch {
		return false
	}
}

function agentMetadataFromFile(
	agentFile: AgentFile,
): Pick<
	AgentRevisionRecord,
	'graph_contract_version' | 'agent_name' | 'agent_description' | 'agent_version' | 'entry_node_id'
> {
	return {
		graph_contract_version: agentFile.graph_contract_version,
		agent_name: agentFile.meta.name,
		agent_description: agentFile.meta.description ?? null,
		agent_version: agentFile.meta.agent_version ?? null,
		entry_node_id: agentFile.entry_node_id,
	}
}

function serializeAgentFile(agentFile: AgentFile): string {
	return `${JSON.stringify(agentFile, null, 2)}\n`
}

export function buildGraphVisibleEventEnvelope(input: {
	payload?: JsonValue | null
	launch_note?: string | null
}): JsonObject | null {
	const eventEnvelope: JsonObject = {}

	if (input.payload !== undefined && input.payload !== null) {
		eventEnvelope.payload = input.payload
	}
	if (input.launch_note !== undefined && input.launch_note !== null) {
		eventEnvelope.launch_note = input.launch_note
	}

	return Object.keys(eventEnvelope).length > 0 ? eventEnvelope : null
}

export interface AgentLifecycleServiceOptions {
	state_store: SQLiteLocalStateStore
	lifecycle_root?: string
}

export interface AgentLifecycleIndexResult {
	logical_agent_id: string
	revision: AgentRevisionRecord
	status: AgentLifecycleStatusRecord
}

export interface AgentLifecycleDeployResult extends AgentLifecycleIndexResult {
	live_file_path: string
	source_revision_id: string
}

export interface SaveValidatedDraftAgentFileInput {
	agent_file: AgentFile
	draft_file_path?: string
}

export interface ResolvedLiveAgentFile {
	logical_agent_id: string
	resolved_revision_id: string
	live_file_path: string
	revision: AgentRevisionRecord
	agent_file: AgentFile
}

export interface RegisterTriggerInput {
	trigger_id?: string
	logical_agent_id: string
	trigger_ref: string
}

export interface CreateDispatchEventInput {
	trigger_id: string
	event_id?: string
	payload?: JsonValue | null
	launch_note?: string | null
}

export interface PreparedEventDispatch {
	trigger: TriggerRecord
	event: EventRecord
	live_agent: ResolvedLiveAgentFile
	graph_event: JsonObject | null
}

export class AgentLifecycleService {
	private readonly stateStore: SQLiteLocalStateStore
	private readonly lifecycleRoot: string

	constructor(options: AgentLifecycleServiceOptions) {
		this.stateStore = options.state_store
		this.lifecycleRoot =
			options.lifecycle_root ??
			path.join(path.dirname(this.stateStore.database_path), 'agent-lifecycle')
	}

	async registerAgentFile(agentFilePath: string): Promise<AgentLifecycleIndexResult> {
		const { agentFile, resolvedRevisionId, resolvedAgentFilePath } =
			await this.readSourceAgentFile(agentFilePath)
		const revision = this.stateStore.findAgentRevisionByPathAndHash(
			agentFile.meta.id,
			resolvedAgentFilePath,
			resolvedRevisionId,
		)
		const timestamp = nowIso()

		const storedRevision = this.stateStore.upsertAgentRevision({
			revision_id: revision?.revision_id,
			logical_agent_id: agentFile.meta.id,
			revision_kind: revision?.revision_kind === 'live' ? 'live' : 'draft',
			file_path: resolvedAgentFilePath,
			resolved_revision_id: resolvedRevisionId,
			availability_state: 'available',
			validation_error: null,
			validated_at: timestamp,
			...agentMetadataFromFile(agentFile),
			created_at: revision?.created_at ?? timestamp,
			updated_at: timestamp,
		})

		const status = await this.refreshAgentStatus(agentFile.meta.id)
		if (!status) {
			throw new AppError(
				'AGENT_NOT_FOUND',
				`Agent "${agentFile.meta.id}" does not exist after registration.`,
			)
		}

		return {
			logical_agent_id: agentFile.meta.id,
			revision: storedRevision,
			status,
		}
	}

	async saveValidatedDraftAgentFile(
		input: SaveValidatedDraftAgentFileInput,
	): Promise<AgentLifecycleIndexResult> {
		const draftFilePath =
			input.draft_file_path ?? this.buildDraftRevisionPath(input.agent_file.meta.id, randomUUID())
		await writeTextFileAtomically(draftFilePath, serializeAgentFile(input.agent_file))
		return await this.registerAgentFile(draftFilePath)
	}

	async deployAgentFile(agentFilePath: string): Promise<AgentLifecycleDeployResult> {
		const { agentFile, resolvedRevisionId, resolvedAgentFilePath } =
			await this.readSourceAgentFile(agentFilePath)
		const timestamp = nowIso()
		const agentId = agentFile.meta.id
		const sourceRevision = this.stateStore.findAgentRevisionByPathAndHash(
			agentId,
			resolvedAgentFilePath,
			resolvedRevisionId,
		)

		if (!sourceRevision) {
			this.stateStore.upsertAgentRevision({
				logical_agent_id: agentId,
				revision_kind: 'draft',
				file_path: resolvedAgentFilePath,
				resolved_revision_id: resolvedRevisionId,
				availability_state: 'available',
				validation_error: null,
				validated_at: timestamp,
				...agentMetadataFromFile(agentFile),
				created_at: timestamp,
				updated_at: timestamp,
			})
		} else {
			this.stateStore.upsertAgentRevision({
				revision_id: sourceRevision.revision_id,
				logical_agent_id: agentId,
				revision_kind: sourceRevision.revision_kind === 'live' ? 'live' : 'draft',
				file_path: resolvedAgentFilePath,
				resolved_revision_id: resolvedRevisionId,
				availability_state: 'available',
				validation_error: null,
				validated_at: timestamp,
				...agentMetadataFromFile(agentFile),
				created_at: sourceRevision.created_at,
				updated_at: timestamp,
			})
		}

		const liveRevisionId = randomUUID()
		const liveFilePath = this.buildLiveRevisionPath(agentId, liveRevisionId)
		await writeTextFileAtomically(liveFilePath, serializeAgentFile(agentFile))

		const liveAgentFile = await loadAndValidateAgentFile(liveFilePath)
		const liveResolvedRevisionId = await computeResolvedRevisionId(liveFilePath)
		if (liveResolvedRevisionId.length === 0) {
			throw new AppError(
				'AGENT_DEPLOY_FAILED',
				`Live revision for agent "${agentId}" could not be resolved.`,
			)
		}

		const previousLiveRevisionId = this.stateStore.getAgentRecord(agentId)?.live_revision_id ?? null
		const liveRevision = this.stateStore.promoteAgentRevision({
			logical_agent_id: agentId,
			previous_live_revision_id: previousLiveRevisionId,
			live_revision: {
				revision_id: liveRevisionId,
				logical_agent_id: agentId,
				revision_kind: 'live',
				file_path: liveFilePath,
				resolved_revision_id: liveResolvedRevisionId,
				availability_state: 'available',
				validation_error: null,
				validated_at: timestamp,
				...agentMetadataFromFile(liveAgentFile),
				created_at: timestamp,
				updated_at: timestamp,
			},
			updated_at: timestamp,
		})

		const status = await this.refreshAgentStatus(agentId)
		if (!status) {
			throw new AppError('AGENT_NOT_FOUND', `Agent "${agentId}" does not exist after deploy.`)
		}

		return {
			logical_agent_id: agentId,
			revision: liveRevision.live_revision,
			live_file_path: liveFilePath,
			source_revision_id: resolvedRevisionId,
			status,
		}
	}

	async resolveLiveAgentFile(logicalAgentId: string): Promise<ResolvedLiveAgentFile> {
		const status = await this.refreshAgentStatus(logicalAgentId)
		if (!status) {
			throw new AppError('AGENT_NOT_FOUND', `Agent "${logicalAgentId}" does not exist.`)
		}

		const liveRevision = status.live_revision
		if (!liveRevision) {
			throw new AppError(
				'AGENT_LIVE_NOT_FOUND',
				`Agent "${logicalAgentId}" does not have a live revision.`,
			)
		}

		const inspection = await this.inspectRevision(liveRevision)
		if (inspection.availability_state !== 'available' || !inspection.agent_file) {
			throw new AppError(
				'AGENT_LIVE_UNAVAILABLE',
				`Agent "${logicalAgentId}" live revision "${liveRevision.revision_id}" is "${inspection.availability_state}".`,
			)
		}

		return {
			logical_agent_id: logicalAgentId,
			resolved_revision_id: liveRevision.resolved_revision_id,
			live_file_path: liveRevision.file_path,
			revision: liveRevision,
			agent_file: inspection.agent_file,
		}
	}

	async getAgentStatus(logicalAgentId: string): Promise<AgentLifecycleStatusRecord> {
		const status = await this.refreshAgentStatus(logicalAgentId)
		if (!status) {
			throw new AppError('AGENT_NOT_FOUND', `Agent "${logicalAgentId}" does not exist.`)
		}
		return status
	}

	registerTrigger(input: RegisterTriggerInput): TriggerRecord {
		return this.stateStore.upsertTriggerRecord(input)
	}

	getTrigger(triggerId: string): TriggerRecord {
		const trigger = this.stateStore.getTriggerRecord(triggerId)
		if (!trigger) {
			throw new AppError('TRIGGER_NOT_FOUND', `Trigger "${triggerId}" does not exist.`)
		}
		return trigger
	}

	listTriggers(logicalAgentId?: string): TriggerRecord[] {
		return this.stateStore.listTriggerRecords(logicalAgentId)
	}

	listEvents(filters?: { trigger_id?: string; logical_agent_id?: string }): EventRecord[] {
		return this.stateStore.listEventRecords(filters)
	}

	async prepareEventDispatch(input: CreateDispatchEventInput): Promise<PreparedEventDispatch> {
		const trigger = this.getTrigger(input.trigger_id)
		const event = this.stateStore.createEventRecord({
			event_id: input.event_id,
			trigger_id: trigger.trigger_id,
			logical_agent_id: trigger.logical_agent_id,
			payload: input.payload ?? null,
			launch_note: input.launch_note ?? null,
		})

		try {
			const liveAgent = await this.resolveLiveAgentFile(trigger.logical_agent_id)
			return {
				trigger,
				event,
				live_agent: liveAgent,
				graph_event: buildGraphVisibleEventEnvelope({
					payload: event.payload,
					launch_note: event.launch_note,
				}),
			}
		} catch (error) {
			const appError =
				error instanceof AppError
					? error
					: new AppError(
							'EVENT_DISPATCH_FAILED',
							error instanceof Error ? error.message : 'Unknown event dispatch failure.',
						)

			this.stateStore.markEventDispatchFailed({
				event_id: event.event_id,
				error_code: appError.code,
				error_message: appError.message,
			})
			throw appError
		}
	}

	markEventDispatched(eventId: string, runId: string, resolvedRevisionId: string): EventRecord {
		return this.stateStore.markEventDispatched({
			event_id: eventId,
			run_id: runId,
			resolved_revision_id: resolvedRevisionId,
		})
	}

	markEventDispatchFailed(eventId: string, error: Pick<AppError, 'code' | 'message'>): EventRecord {
		return this.stateStore.markEventDispatchFailed({
			event_id: eventId,
			error_code: error.code,
			error_message: error.message,
		})
	}

	private async refreshAgentStatus(
		logicalAgentId: string,
	): Promise<AgentLifecycleStatusRecord | null> {
		const status = this.stateStore.getAgentLifecycleStatus(logicalAgentId)
		if (!status) {
			return null
		}

		const refreshedRevisions: AgentRevisionRecord[] = []
		for (const revision of status.revisions) {
			refreshedRevisions.push(await this.refreshRevisionRecord(revision))
		}

		const liveRevisionId = status.agent.live_revision_id
		const liveRevision = liveRevisionId
			? (refreshedRevisions.find((revision) => revision.revision_id === liveRevisionId) ?? null)
			: null

		return {
			agent: this.stateStore.getAgentRecord(logicalAgentId) ?? status.agent,
			live_revision: liveRevision,
			draft_revisions: refreshedRevisions.filter((revision) => revision.revision_kind === 'draft'),
			revisions: refreshedRevisions,
		}
	}

	private async refreshRevisionRecord(revision: AgentRevisionRecord): Promise<AgentRevisionRecord> {
		const inspection = await this.inspectRevision(revision)
		return this.stateStore.upsertAgentRevision({
			revision_id: revision.revision_id,
			logical_agent_id: revision.logical_agent_id,
			revision_kind: revision.revision_kind,
			file_path: revision.file_path,
			resolved_revision_id: revision.resolved_revision_id,
			availability_state: inspection.availability_state,
			validation_error: inspection.validation_error,
			validated_at: inspection.validated_at,
			graph_contract_version:
				inspection.agent_file?.graph_contract_version ?? revision.graph_contract_version,
			agent_name: inspection.agent_file?.meta.name ?? revision.agent_name,
			agent_description: inspection.agent_file?.meta.description ?? revision.agent_description,
			agent_version: inspection.agent_file?.meta.agent_version ?? revision.agent_version,
			entry_node_id: inspection.agent_file?.entry_node_id ?? revision.entry_node_id,
			created_at: revision.created_at,
			updated_at: inspection.validated_at,
		})
	}

	private async inspectRevision(revision: AgentRevisionRecord): Promise<{
		availability_state: AgentRevisionAvailabilityState
		validation_error: string | null
		validated_at: string
		agent_file: AgentFile | null
	}> {
		const validatedAt = nowIso()

		if (!(await pathExists(revision.file_path))) {
			return {
				availability_state: 'missing',
				validation_error: `File "${revision.file_path}" does not exist.`,
				validated_at: validatedAt,
				agent_file: null,
			}
		}

		try {
			const agentFile = await loadAndValidateAgentFile(revision.file_path)
			const resolvedRevisionId = await computeResolvedRevisionId(revision.file_path)

			if (resolvedRevisionId !== revision.resolved_revision_id) {
				return {
					availability_state: 'conflicted',
					validation_error: `File "${revision.file_path}" no longer matches revision "${revision.revision_id}".`,
					validated_at: validatedAt,
					agent_file: agentFile,
				}
			}

			return {
				availability_state: 'available',
				validation_error: null,
				validated_at: validatedAt,
				agent_file: agentFile,
			}
		} catch (error) {
			return {
				availability_state: 'invalid',
				validation_error: error instanceof Error ? error.message : 'Unknown validation failure.',
				validated_at: validatedAt,
				agent_file: null,
			}
		}
	}

	private async readSourceAgentFile(agentFilePath: string): Promise<{
		agentFile: AgentFile
		resolvedRevisionId: string
		resolvedAgentFilePath: string
	}> {
		const resolvedAgentFilePath = path.resolve(agentFilePath)
		const agentFile = await loadAndValidateAgentFile(resolvedAgentFilePath)
		const resolvedRevisionId = await computeResolvedRevisionId(resolvedAgentFilePath)
		return {
			agentFile,
			resolvedRevisionId,
			resolvedAgentFilePath,
		}
	}

	private buildLiveRevisionPath(logicalAgentId: string, revisionId: string): string {
		return path.join(
			this.lifecycleRoot,
			'agents',
			sha256Hex(logicalAgentId),
			'live',
			`${revisionId}.json`,
		)
	}

	private buildDraftRevisionPath(logicalAgentId: string, revisionId: string): string {
		return path.join(
			this.lifecycleRoot,
			'agents',
			sha256Hex(logicalAgentId),
			'drafts',
			`${revisionId}.json`,
		)
	}
}
