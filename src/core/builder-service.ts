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

type BuilderCandidateGate =
	| 'output_json'
	| 'wrapper_extraction'
	| 'schema_validation'
	| 'identity_check'
	| 'deterministic_candidate_audit'

interface BuilderAttemptAccepted {
	status: 'accepted'
	builder_run_id: string
	candidate_agent_file: AgentFile
	candidate_diagnostics: BuilderCandidateAuditDiagnostics
}

interface BuilderAttemptRejected {
	status: 'rejected'
	error: AppError
	diagnostics: JsonObject
}

type BuilderAttemptResult = BuilderAttemptAccepted | BuilderAttemptRejected

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

function getJsonObjectProperty(value: JsonObject, key: string): JsonObject | null {
	const property = value[key]
	return isJsonObject(property) ? property : null
}

function getStringProperty(value: JsonObject, key: string): string | null {
	const property = value[key]
	return typeof property === 'string' ? property : null
}

function getStringArrayProperty(value: JsonObject, key: string): string[] | null {
	const property = value[key]
	if (!Array.isArray(property) || !property.every((entry) => typeof entry === 'string')) {
		return null
	}
	return property
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

function buildFailureDiagnostics(args: { attempt_number: number; error: AppError }): JsonObject {
	const details = isJsonObject(args.error.details) ? args.error.details : null
	const diagnostics: JsonObject = {
		attempt_number: args.attempt_number,
		code: args.error.code,
		message: args.error.message,
	}

	if (details) {
		const gate = getStringProperty(details, 'gate')
		if (gate) {
			diagnostics.gate = gate
		}

		const builderRunId = getStringProperty(details, 'builder_run_id')
		if (builderRunId) {
			diagnostics.builder_run_id = builderRunId
		}

		const expectedProperties = getStringArrayProperty(details, 'expected_properties')
		if (expectedProperties) {
			diagnostics.expected_properties = expectedProperties
		}

		const actualProperties = getStringArrayProperty(details, 'actual_properties')
		if (actualProperties) {
			diagnostics.actual_properties = actualProperties
		}

		const extraProperties = getStringArrayProperty(details, 'extra_properties')
		if (extraProperties) {
			diagnostics.extra_properties = extraProperties
		}

		const candidateDiagnostics = getJsonObjectProperty(details, 'candidate_diagnostics')
		if (candidateDiagnostics) {
			const status = getStringProperty(candidateDiagnostics, 'status')
			const issues = Array.isArray(candidateDiagnostics.issues)
				? candidateDiagnostics.issues
						.filter(isJsonObject)
						.map((issue) => ({
							severity: getStringProperty(issue, 'severity') ?? 'error',
							code: getStringProperty(issue, 'code') ?? 'UNKNOWN',
							path: getStringProperty(issue, 'path') ?? '',
							message: getStringProperty(issue, 'message') ?? 'No message provided.',
						}))
				: []
			diagnostics.candidate_diagnostics = {
				status: status ?? 'rejected',
				issues,
			}
		}
	}

	return diagnostics
}

function isRepairableBuilderFailure(error: AppError): boolean {
	const details = isJsonObject(error.details) ? error.details : null
	const gate = getStringProperty(details ?? {}, 'gate') as BuilderCandidateGate | null
	return (
		error.code === 'BUILDER_INVALID_OUTPUT' ||
		error.code === 'BUILDER_CANDIDATE_INVALID' ||
		error.code === 'BUILDER_CANDIDATE_AUDIT_REJECTED' ||
		gate !== null
	)
}

function buildRepairBuilderContext(args: {
	base_context: JsonObject
	first_failure: JsonObject
}): JsonObject {
	return {
		...args.base_context,
		repair_attempt: {
			attempt_number: 2,
			max_attempts: 2,
			reason:
				'The first builder candidate failed existing extraction, validation, identity, or deterministic audit gates and was not persisted.',
			previous_failure: args.first_failure,
			requirements: [
				'Return only the formal {"agent_file": <portable-agent-json>} wrapper.',
				'Keep meta.id exactly equal to the requested target agent id.',
				'Use only the public portable agent contract fields described in this context.',
				'Correct every reported diagnostic before returning the repaired candidate.',
			],
		},
	}
}

function buildRepairFailureError(args: {
	first_failure: BuilderAttemptRejected
	second_failure: BuilderAttemptRejected
}): AppError {
	const secondDetails = isJsonObject(args.second_failure.error.details)
		? args.second_failure.error.details
		: {}

	return new AppError(
		args.second_failure.error.code,
		`Builder repair attempt failed after the initial candidate was rejected. ${args.second_failure.error.message}`,
		{
			...secondDetails,
			builder_repair: {
				attempts: [args.first_failure.diagnostics, args.second_failure.diagnostics],
				max_attempts: 2,
				persisted: false,
			},
		},
	)
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

		const firstAttempt = await this.runBuilderAttempt({
			builder_agent: builderAgent,
			builder_agent_revision_id: builderAgentRevisionId,
			context: builderContext,
			target_agent_id: input.target_agent_id,
			operation,
			run_id: input.run_id,
			attempt_number: 1,
		})
		const acceptedAttempt =
			firstAttempt.status === 'accepted'
				? firstAttempt
				: await this.runRepairAttemptOrThrow({
						builder_agent: builderAgent,
						builder_agent_revision_id: builderAgentRevisionId,
						base_context: builderContext,
						target_agent_id: input.target_agent_id,
						operation,
						initial_run_id: input.run_id,
						first_failure: firstAttempt,
					})

		const draft = await this.lifecycleService.saveValidatedDraftAgentFile({
			agent_file: acceptedAttempt.candidate_agent_file,
		})

		return {
			operation,
			builder_run_id: acceptedAttempt.builder_run_id,
			draft,
			candidate_agent_file: acceptedAttempt.candidate_agent_file,
			candidate_diagnostics: acceptedAttempt.candidate_diagnostics,
			base_revision: existingBase?.revision ?? null,
		}
	}

	private async runRepairAttemptOrThrow(args: {
		builder_agent: AgentFile
		builder_agent_revision_id: string
		base_context: JsonObject
		target_agent_id: string
		operation: 'create' | 'update'
		initial_run_id?: string
		first_failure: BuilderAttemptRejected
	}): Promise<BuilderAttemptAccepted> {
		if (!isRepairableBuilderFailure(args.first_failure.error)) {
			throw args.first_failure.error
		}

		const repairContext = buildRepairBuilderContext({
			base_context: args.base_context,
			first_failure: args.first_failure.diagnostics,
		})
		const repairAttempt = await this.runBuilderAttempt({
			builder_agent: args.builder_agent,
			builder_agent_revision_id: args.builder_agent_revision_id,
			context: repairContext,
			target_agent_id: args.target_agent_id,
			operation: args.operation,
			run_id: args.initial_run_id ? `${args.initial_run_id}:repair` : undefined,
			attempt_number: 2,
		})

		if (repairAttempt.status === 'accepted') {
			return repairAttempt
		}

		throw buildRepairFailureError({
			first_failure: args.first_failure,
			second_failure: repairAttempt,
		})
	}

	private async runBuilderAttempt(args: {
		builder_agent: AgentFile
		builder_agent_revision_id: string
		context: JsonObject
		target_agent_id: string
		operation: 'create' | 'update'
		run_id?: string
		attempt_number: number
	}): Promise<BuilderAttemptResult> {
		try {
			return await this.executeBuilderAttempt(args)
		} catch (error) {
			if (error instanceof AppError) {
				return {
					status: 'rejected',
					error,
					diagnostics: buildFailureDiagnostics({
						attempt_number: args.attempt_number,
						error,
					}),
				}
			}
			throw error
		}
	}

	private async executeBuilderAttempt(args: {
		builder_agent: AgentFile
		builder_agent_revision_id: string
		context: JsonObject
		target_agent_id: string
		operation: 'create' | 'update'
		run_id?: string
		attempt_number: number
	}): Promise<BuilderAttemptAccepted> {
		const builderRun = await runAgentFile(
			args.builder_agent,
			this.runtimeAdapter,
			{
				context: args.context,
			},
			{
				state_store: this.stateStore,
				resolved_revision_id: args.builder_agent_revision_id,
				logical_agent_id: args.builder_agent.meta.id,
				run_id: args.run_id,
			},
		)

		if (builderRun.status !== 'success') {
			throw new AppError(
				'BUILDER_EXECUTION_FAILED',
				`Builder execution did not complete successfully. ${builderRun.message}`,
				{
					builder_run_id: builderRun.run_id,
					code: builderRun.code,
					operation: args.operation,
					attempt_number: args.attempt_number,
				},
			)
		}

		if (builderRun.final_output_mode !== 'json') {
			throw new AppError(
				'BUILDER_INVALID_OUTPUT',
				'Builder completed without a JSON candidate payload.',
				{
					builder_run_id: builderRun.run_id,
					operation: args.operation,
					attempt_number: args.attempt_number,
					gate: 'output_json' satisfies BuilderCandidateGate,
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
					operation: args.operation,
					attempt_number: args.attempt_number,
					gate: 'wrapper_extraction' satisfies BuilderCandidateGate,
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
					operation: args.operation,
					attempt_number: args.attempt_number,
					gate: 'schema_validation' satisfies BuilderCandidateGate,
				},
			)
		}

		if (candidateAgentFile.meta.id !== args.target_agent_id) {
			throw new AppError(
				'BUILDER_CANDIDATE_INVALID',
				`Builder returned agent id "${candidateAgentFile.meta.id}" but expected "${args.target_agent_id}".`,
				{
					builder_run_id: builderRun.run_id,
					operation: args.operation,
					attempt_number: args.attempt_number,
					gate: 'identity_check' satisfies BuilderCandidateGate,
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
					operation: args.operation,
					attempt_number: args.attempt_number,
					gate: 'deterministic_candidate_audit' satisfies BuilderCandidateGate,
					candidate_diagnostics: candidateDiagnostics,
				},
			)
		}

		return {
			status: 'accepted',
			builder_run_id: builderRun.run_id,
			candidate_agent_file: candidateAgentFile,
			candidate_diagnostics: candidateDiagnostics,
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
