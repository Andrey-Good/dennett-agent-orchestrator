import { createHash } from 'node:crypto'
import process from 'node:process'
import { CodexAppServerRuntimeAdapter } from '../adapters/codex/codex-app-server-runtime-adapter.js'
import type { RuntimeAdapter } from '../ports/runtime.js'
import { loadBuilderSystemAgentResource } from '../resources/builder-system-agent.js'
import type { AgentFile } from './agent-file.js'
import { type AgentLifecycleIndexResult, AgentLifecycleService } from './agent-lifecycle.js'
import {
	auditBuilderCandidate,
	type BuilderCandidateAuditDiagnostics,
} from './builder-candidate-auditor.js'
import { AppError } from './errors.js'
import { runAgentFile } from './graph-runner.js'
import type { JsonObject, JsonValue } from './json.js'
import { loadAndValidateAgentFile, validateAgentFileValue } from './schema.js'
import type {
	AgentLifecycleStatusRecord,
	AgentRevisionRecord,
	SQLiteLocalStateStore,
} from './state/index.js'

export interface BuilderAgentServiceOptions {
	state_store: SQLiteLocalStateStore
	runtime_adapter?: RuntimeAdapter
	lifecycle_root?: string
	working_directory?: string
	builder_agent_resource?: AgentFile
}

export interface BuildAgentDraftInput {
	target_agent_id: string
	request: string
	target_agent_name?: string
	target_agent_description?: string
	revise?: boolean
	run_id?: string
}

export interface ExistingBuilderBaseContext {
	revision: AgentRevisionRecord
	agent_file: AgentFile | null
}

export interface BuildAgentDraftResult {
	operation: 'create' | 'update'
	builder_run_id: string
	draft: AgentLifecycleIndexResult
	candidate_agent_file: AgentFile
	candidate_diagnostics: BuilderCandidateAuditDiagnostics
	base_revision?: AgentRevisionRecord | null
}

function computeSyntheticResolvedRevisionId(resourceId: string, value: unknown): string {
	const digest = createHash('sha256').update(JSON.stringify(value)).digest('hex')
	return `${resourceId}#sha256:${digest}`
}

function isJsonObject(value: JsonValue | unknown): value is JsonObject {
	return value !== null && typeof value === 'object' && !Array.isArray(value)
}

function toJsonObject(value: unknown): JsonObject {
	return JSON.parse(JSON.stringify(value)) as JsonObject
}

function sortRevisionsNewestFirst(revisions: AgentRevisionRecord[]): AgentRevisionRecord[] {
	return [...revisions].sort((left, right) => {
		const updated = right.updated_at.localeCompare(left.updated_at)
		if (updated !== 0) {
			return updated
		}
		return right.created_at.localeCompare(left.created_at)
	})
}

async function selectExistingBaseContext(
	status: AgentLifecycleStatusRecord,
): Promise<ExistingBuilderBaseContext | null> {
	const preferredRevision =
		sortRevisionsNewestFirst(
			status.draft_revisions.filter((revision) => revision.availability_state === 'available'),
		)[0] ?? (status.live_revision?.availability_state === 'available' ? status.live_revision : null)

	if (!preferredRevision) {
		return null
	}

	return {
		revision: preferredRevision,
		agent_file: await loadAndValidateAgentFile(preferredRevision.file_path),
	}
}

function buildBuilderContext(args: {
	input: BuildAgentDraftInput
	operation: 'create' | 'update'
	existing_base: ExistingBuilderBaseContext | null
}): JsonObject {
	return {
		operation: args.operation,
		request: args.input.request,
		target_agent: {
			id: args.input.target_agent_id,
			suggested_name:
				args.input.target_agent_name ?? args.existing_base?.agent_file?.meta.name ?? null,
			suggested_description:
				args.input.target_agent_description ??
				args.existing_base?.agent_file?.meta.description ??
				null,
		},
		existing_revision: args.existing_base
			? {
					revision_id: args.existing_base.revision.revision_id,
					revision_kind: args.existing_base.revision.revision_kind,
					resolved_revision_id: args.existing_base.revision.resolved_revision_id,
				}
			: null,
		existing_agent_file: args.existing_base?.agent_file
			? toJsonObject(args.existing_base.agent_file)
			: null,
		constraints: {
			draft_persistence_default: true,
			explicit_deploy_only: true,
			validate_against_canonical_contract: true,
			public_contract_only: true,
		},
		portable_authoring_guidance: {
			allowed_public_surfaces: [
				'params',
				'initial_vars',
				'skills',
				'mcps',
				'plugins',
				'permissions',
				'memory_bindings',
				'runtime_sources',
				'runtime_options',
				'interaction',
				'chat',
				'orchestrator_agent',
			],
			memory_bindings: {
				required_shape:
					'Use id, kind "runtime_memory", codex_ref, scope, and config with intent plus required_capabilities.',
				provider_extension:
					'Provider-specific hints must stay in portable provider_extension fields. Mem0 supports only provider, optional transport, and config.mem0_config.graph_store with provider plus empty config.',
				forbidden_local_data: [
					'provider registration ids',
					'provider local_config',
					'credentials',
					'api keys',
					'python executables',
					'local package paths',
					'account metadata',
					'rate limits',
				],
			},
			runtime_sources: {
				usage:
					'Declare portable runtime_sources by id, runtime_adapter, source_ref, and optional description; select them from runtime_agent nodes with runtime_source_policy and runtime_source_ids.',
				forbidden_local_data: [
					'auth state',
					'account details',
					'rate limits',
					'config requirements',
					'discovered model catalogs',
				],
			},
			runtime_options: {
				usage:
					'Use node runtime_options for portable hints such as model, reasoning_effort, speed_tier, or personality when requested. speed_tier values are fast or flex.',
				forbidden_local_data: ['provider secrets', 'account quotas', 'runtime inventory dumps'],
			},
			interaction: {
				usage:
					'Use interaction.comments for live comments and interaction.user_mcp with server_name "orchestrator.user_chat" for built-in user chat.',
				child_runs:
					'Do not surface child-run live interaction through orchestrator_agent nodes in this portable contract.',
			},
			managed_subagents: {
				usage:
					'Represent managed-subagent authoring patterns with public orchestrator_agent nodes, agent_ref values, and explicit prompts or handoff text.',
				forbidden_hidden_data: [
					'managed task-package snapshots',
					'write-set ownership internals',
					'lineage records',
					'budgets as hidden lifecycle state',
					'create/send/wait/status/close control payloads',
				],
			},
			lifecycle: {
				builder_result:
					'Candidate output must use the formal wrapper contract {"agent_file": <portable-agent-json>}; the candidate is validated, audited, and saved as a draft only.',
				deployment: 'Deployment remains an explicit separate lifecycle operation.',
			},
		},
	}
}

function extractAgentFileCandidate(output: JsonValue | null): unknown {
	if (!isJsonObject(output)) {
		throw new AppError('BUILDER_INVALID_OUTPUT', 'Builder completed without a JSON object result.')
	}

	const wrapperKeys = Object.keys(output)
	const expectedWrapperProperties = ['agent_file']
	if (!Object.hasOwn(output, 'agent_file')) {
		throw new AppError(
			'BUILDER_INVALID_OUTPUT',
			'Builder output wrapper must contain exactly one property: "agent_file"; missing required "agent_file" property.',
			{
				expected_properties: expectedWrapperProperties,
				actual_properties: wrapperKeys,
			},
		)
	}

	const extraWrapperKeys = wrapperKeys.filter((key) => key !== 'agent_file')
	if (extraWrapperKeys.length > 0) {
		throw new AppError(
			'BUILDER_INVALID_OUTPUT',
			`Builder output wrapper must contain only the "agent_file" property; unexpected wrapper field(s): ${extraWrapperKeys
				.map((key) => `"${key}"`)
				.join(', ')}.`,
			{
				expected_properties: expectedWrapperProperties,
				actual_properties: wrapperKeys,
				extra_properties: extraWrapperKeys,
			},
		)
	}

	const candidate = output.agent_file
	if (!isJsonObject(candidate)) {
		throw new AppError(
			'BUILDER_INVALID_OUTPUT',
			'Builder output wrapper property "agent_file" must be an object.',
			{
				expected_properties: expectedWrapperProperties,
				actual_properties: wrapperKeys,
			},
		)
	}

	return candidate
}

export class BuilderAgentService {
	private readonly stateStore: SQLiteLocalStateStore
	private readonly runtimeAdapter: RuntimeAdapter
	private readonly lifecycleService: AgentLifecycleService
	private readonly workingDirectory: string
	private readonly builderAgentResource?: AgentFile

	constructor(options: BuilderAgentServiceOptions) {
		this.stateStore = options.state_store
		this.workingDirectory = options.working_directory ?? process.cwd()
		this.runtimeAdapter =
			options.runtime_adapter ?? new CodexAppServerRuntimeAdapter(this.workingDirectory)
		this.lifecycleService = new AgentLifecycleService({
			state_store: this.stateStore,
			lifecycle_root: options.lifecycle_root,
		})
		this.builderAgentResource = options.builder_agent_resource
	}

	async buildAgentDraft(input: BuildAgentDraftInput): Promise<BuildAgentDraftResult> {
		const builderAgent = this.builderAgentResource ?? (await loadBuilderSystemAgentResource())
		const builderAgentRevisionId = computeSyntheticResolvedRevisionId(
			builderAgent.meta.id,
			builderAgent,
		)

		const existingStatus = await this.getExistingStatus(input.target_agent_id)
		const existingBase = existingStatus ? await selectExistingBaseContext(existingStatus) : null
		const operation = this.resolveBuilderOperation(input, existingStatus, existingBase)
		const builderContext = buildBuilderContext({
			input,
			operation,
			existing_base: existingBase,
		})

		const builderRun = await runAgentFile(
			builderAgent,
			this.runtimeAdapter,
			{
				context: builderContext,
			},
			{
				state_store: this.stateStore,
				resolved_revision_id: builderAgentRevisionId,
				logical_agent_id: builderAgent.meta.id,
				run_id: input.run_id,
			},
		)

		if (builderRun.status !== 'success') {
			throw new AppError(
				'BUILDER_EXECUTION_FAILED',
				`Builder execution did not complete successfully. ${builderRun.message}`,
				{
					builder_run_id: builderRun.run_id,
					code: builderRun.code,
					operation,
				},
			)
		}

		if (builderRun.final_output_mode !== 'json') {
			throw new AppError(
				'BUILDER_INVALID_OUTPUT',
				'Builder completed without a JSON candidate payload.',
				{
					builder_run_id: builderRun.run_id,
					operation,
				},
			)
		}

		let candidatePayload: unknown
		try {
			candidatePayload = extractAgentFileCandidate(builderRun.final_output)
		} catch (error) {
			if (error instanceof AppError) {
				throw new AppError(error.code, error.message, {
					builder_run_id: builderRun.run_id,
					operation,
					...(isJsonObject(error.details) ? error.details : {}),
				})
			}
			throw error
		}

		let candidateAgentFile: AgentFile
		try {
			candidateAgentFile = await validateAgentFileValue(candidatePayload)
		} catch (error) {
			throw new AppError(
				'BUILDER_CANDIDATE_INVALID',
				`Builder produced an invalid portable agent candidate. ${
					error instanceof Error ? error.message : 'Unknown validation failure.'
				}`,
				{
					builder_run_id: builderRun.run_id,
					operation,
				},
			)
		}

		if (candidateAgentFile.meta.id !== input.target_agent_id) {
			throw new AppError(
				'BUILDER_CANDIDATE_INVALID',
				`Builder returned agent id "${candidateAgentFile.meta.id}" but expected "${input.target_agent_id}".`,
				{
					builder_run_id: builderRun.run_id,
					operation,
				},
			)
		}

		const candidateDiagnostics = auditBuilderCandidate({
			agent_file: candidateAgentFile,
			runtime_adapter: this.runtimeAdapter,
		})
		if (candidateDiagnostics.status === 'rejected') {
			throw new AppError(
				'BUILDER_CANDIDATE_AUDIT_REJECTED',
				`Builder candidate failed deterministic audit: ${candidateDiagnostics.issues
					.filter((issue) => issue.severity === 'error')
					.map((issue) => `${issue.path} ${issue.message}`)
					.join('; ')}`,
				{
					builder_run_id: builderRun.run_id,
					operation,
					candidate_diagnostics: candidateDiagnostics,
				},
			)
		}

		const draft = await this.lifecycleService.saveValidatedDraftAgentFile({
			agent_file: candidateAgentFile,
		})

		return {
			operation,
			builder_run_id: builderRun.run_id,
			draft,
			candidate_agent_file: candidateAgentFile,
			candidate_diagnostics: candidateDiagnostics,
			base_revision: existingBase?.revision ?? null,
		}
	}

	private async getExistingStatus(
		logicalAgentId: string,
	): Promise<AgentLifecycleStatusRecord | null> {
		try {
			return await this.lifecycleService.getAgentStatus(logicalAgentId)
		} catch (error) {
			if (error instanceof AppError && error.code === 'AGENT_NOT_FOUND') {
				return null
			}
			throw error
		}
	}

	private resolveBuilderOperation(
		input: BuildAgentDraftInput,
		existingStatus: AgentLifecycleStatusRecord | null,
		existingBase: ExistingBuilderBaseContext | null,
	): 'create' | 'update' {
		if (input.revise === true) {
			if (!existingBase) {
				throw new AppError(
					'BUILDER_REVISION_BASE_NOT_FOUND',
					`Logical agent "${input.target_agent_id}" does not have an available revision base to revise.`,
				)
			}
			return 'update'
		}

		if (existingStatus) {
			throw new AppError(
				'BUILDER_AGENT_ALREADY_EXISTS',
				`Logical agent "${input.target_agent_id}" already exists. Create requires a new logical agent id; use the explicit revise flow instead.`,
			)
		}

		return 'create'
	}
}
