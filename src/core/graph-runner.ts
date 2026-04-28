import { createHash } from 'node:crypto'
import {
	ORCHESTRATOR_USER_CHAT_SERVER_NAME,
	type ResolvedMemoryBinding,
	type RuntimeAdapter,
	type RuntimeAdapterCapabilities,
	type RuntimeEvent,
	type RuntimeMemoryContext,
	type RuntimeMemoryOperationScope,
	type RuntimeResumeRequest,
	type RuntimeSourceInspectionResult,
	type RuntimeSourceSelection,
	type RuntimeTerminalResult,
	type RuntimeUserChatRequestEvent,
	type UserChatResponsePayload,
} from '../ports/runtime.js'
import type {
	AgentFile,
	AgentNode,
	MemoryBinding,
	OrchestratorAgentNode,
	OutputContract,
	RuntimeAgentNode,
	RuntimeSourceBinding,
} from './agent-file.js'
import { AgentLifecycleService } from './agent-lifecycle.js'
import { AppError } from './errors.js'
import type { NodeOutputRecord, RunStateSnapshot } from './input-resolution.js'
import { buildNodeLookup, resolveNodeInput } from './input-resolution.js'
import type { JsonObject, JsonValue } from './json.js'
import { MemoryService } from './memory-service.js'
import { validateJsonOutputAgainstSchema } from './output-schema-validator.js'
import type { PersistedRunSnapshot, RunStatus, SQLiteLocalStateStore } from './state/index.js'

export interface RunAgentFileOptions {
	state_store: SQLiteLocalStateStore
	resolved_revision_id: string
	run_id?: string
	logical_agent_id?: string | null
	started_via?: 'direct' | 'event'
	event?: JsonObject | null
	user_runtime_source_ids?: string[]
}

export interface ResumeAgentRunOptions {
	state_store: SQLiteLocalStateStore
	resolved_revision_id: string
}

export interface RunResultSuccess {
	status: 'success'
	run_id: string
	run_status: 'completed'
	final_output: JsonValue | null
	final_output_mode: 'text' | 'json' | null
	node_outputs: Map<string, NodeOutputRecord>
}

export interface RunResultFailure {
	status: 'failure'
	run_id: string
	run_status: Extract<RunStatus, 'failed' | 'cancelled' | 'interrupted'>
	code: string
	message: string
	resume_available: boolean
}

export interface RunResultWaitingForUser {
	status: 'waiting_for_user'
	run_id: string
	run_status: 'waiting_for_user'
	code: 'RUN_WAITING_FOR_USER'
	message: string
	resume_available: true
}

export type RunResult = RunResultSuccess | RunResultFailure | RunResultWaitingForUser

type RuntimeSuccessResult = Extract<RuntimeTerminalResult, { outcome: 'success' }>

type ExecutionCursor =
	| {
			kind: 'execute'
			node: AgentNode
			reuse_attempt_id: string | null
	  }
	| {
			kind: 'completed'
	  }

type ClassifiedExecutionResult =
	| {
			kind: 'success'
			output: NodeOutputRecord
			next_vars: Record<string, JsonValue>
			native_session_handle: unknown | null
	  }
	| {
			kind: 'terminal_failure'
			outcome: 'invalid_output' | 'runtime_error' | 'cancelled' | 'interrupted'
			code: string
			message: string
			native_session_handle: unknown | null
	  }

const supportedRuntimeOptionKeys = new Set([
	'model',
	'reasoning_effort',
	'speed_tier',
	'personality',
])

function createRuntimeRequest(
	node: RuntimeAgentNode,
	input_message: string,
	resume: RuntimeResumeRequest,
	interaction: {
		comments_enabled: boolean
		user_chat_server_name?: typeof ORCHESTRATOR_USER_CHAT_SERVER_NAME
		user_chat_reply?: UserChatResponsePayload
	},
	runtime_context: {
		memory_bindings: ResolvedMemoryBinding[]
		memory_context?: RuntimeMemoryContext
		runtime_source?: RuntimeSourceSelection
	},
) {
	return {
		node_id: node.id,
		runtime_adapter: node.runtime_adapter,
		prompt: node.prompt,
		input_message,
		output: node.output,
		effective_bindings: {
			skills: [],
			mcps: [],
			plugins: [],
			memory_bindings: runtime_context.memory_bindings,
		},
		permissions: {},
		runtime_options: (node.runtime_options ?? {}) as JsonObject,
		...(runtime_context.runtime_source ? { runtime_source: runtime_context.runtime_source } : {}),
		...(runtime_context.memory_context ? { memory_context: runtime_context.memory_context } : {}),
		interaction,
		resume,
	}
}

function isUserChatResponsePayload(value: unknown): value is UserChatResponsePayload {
	if (value === undefined || value === null || typeof value !== 'object' || Array.isArray(value)) {
		return false
	}

	const payload = value as {
		kind?: string
		prompt_id?: unknown
		text?: unknown
		option_id?: unknown
		value?: unknown
	}

	if (payload.kind === 'text') {
		return typeof payload.text === 'string' && payload.option_id === undefined
	}

	if (payload.kind === 'option') {
		return typeof payload.option_id === 'string' && 'value' in payload
	}

	return false
}

function resolvePendingUserChatReply(
	snapshot: PersistedRunSnapshot,
): UserChatResponsePayload | null {
	const pendingPrompt = snapshot.resume.pending_prompt
	const replyPayload = pendingPrompt?.reply?.payload
	if (isUserChatResponsePayload(replyPayload)) {
		return replyPayload
	}

	return null
}

async function waitForBlockingUserChatRequest(
	events: AsyncIterable<RuntimeEvent>,
): Promise<RuntimeUserChatRequestEvent> {
	for await (const event of events) {
		if (event.kind === 'user_chat_request' && event.payload.require_response) {
			return event
		}
	}

	throw new AppError(
		'RUN_STATE_INCONSISTENT',
		'Runtime execution ended before a blocking built-in user-chat prompt was observed.',
	)
}

function isRuntimeSourceUsable(result: RuntimeSourceInspectionResult): boolean {
	return result.availability !== 'unavailable' && result.limit_status !== 'exhausted'
}

function toResolvedMemoryBindings(bindings: MemoryBinding[]): ResolvedMemoryBinding[] {
	return bindings.map((binding) => ({
		id: binding.id,
		kind: binding.kind,
		codex_ref: binding.codex_ref,
		scope: binding.scope,
	}))
}

function buildSelectedMemoryBindings(
	agentFile: AgentFile,
	node: RuntimeAgentNode,
): MemoryBinding[] {
	const memoryBindings = agentFile.memory_bindings ?? []
	const memoryBindingLookup = new Map(
		memoryBindings.map((binding) => [binding.id, binding] as const),
	)
	const selectedIds =
		node.memory_ids === undefined
			? memoryBindings.filter((binding) => binding.scope === 'agent').map((binding) => binding.id)
			: node.memory_ids

	return selectedIds.map((memoryId) => {
		const binding = memoryBindingLookup.get(memoryId)
		if (!binding) {
			throw new AppError(
				'INVALID_RUNTIME_CONTEXT',
				`Node "${node.id}" references unknown memory binding "${memoryId}".`,
			)
		}
		return binding
	})
}

function buildRuntimeMemoryOperationScope(
	agentFile: AgentFile,
	snapshot: PersistedRunSnapshot,
): RuntimeMemoryOperationScope {
	return {
		agent_id: snapshot.run.logical_agent_id ?? agentFile.meta.id,
		run_id: snapshot.run.run_id,
	}
}

async function prepareRuntimeMemoryContext(args: {
	memoryService: MemoryService
	bindings: MemoryBinding[]
	scope: RuntimeMemoryOperationScope
	read_query: string
}): Promise<RuntimeMemoryContext | undefined> {
	if (args.bindings.length === 0) {
		return undefined
	}

	const prepared = []
	for (const binding of args.bindings) {
		const result = await args.memoryService.prepareRuntimeMemoryBindingContext({
			binding,
			scope: args.scope,
			read: {
				query: args.read_query,
			},
		})
		prepared.push(result.context)
	}

	return {
		bindings: prepared,
	}
}

function stableStringify(value: JsonValue): string {
	if (value === null || typeof value !== 'object') {
		return JSON.stringify(value)
	}

	if (Array.isArray(value)) {
		return `[${value.map((item) => stableStringify(item)).join(',')}]`
	}

	const entries = Object.entries(value).sort(([left], [right]) => left.localeCompare(right))
	return `{${entries
		.map(([key, entryValue]) => `${JSON.stringify(key)}:${stableStringify(entryValue)}`)
		.join(',')}}`
}

function serializeNodeOutputContent(output: NodeOutputRecord): string {
	return output.mode === 'text' ? output.text : stableStringify(output.json)
}

function hashNodeOutput(output: NodeOutputRecord): string {
	const hash = createHash('sha256')
	hash.update(stableStringify(output))
	return `sha256:${hash.digest('hex')}`
}

async function writeRuntimeMemoryOnNodeSuccess(args: {
	memoryService: MemoryService
	bindings: MemoryBinding[]
	scope: RuntimeMemoryOperationScope
	node_id: string
	attempt_id: string
	output: NodeOutputRecord
}): Promise<void> {
	if (args.bindings.length === 0) {
		return
	}

	const content = serializeNodeOutputContent(args.output)
	const outputHash = hashNodeOutput(args.output)
	for (const binding of args.bindings) {
		await args.memoryService.writeRuntimeMemoryOnSuccess({
			binding,
			scope: args.scope,
			node_id: args.node_id,
			attempt_id: args.attempt_id,
			output_mode: args.output.mode,
			output_hash: outputHash,
			content,
			outcome: 'success',
		})
	}
}

async function inspectRuntimeSourceIfSupported(
	adapter: RuntimeAdapter,
	capabilities: RuntimeAdapterCapabilities,
	source: RuntimeSourceSelection,
): Promise<RuntimeSourceInspectionResult | null> {
	if (!capabilities.supports_runtime_source_introspection) {
		return null
	}

	return adapter.inspectRuntimeSource(source)
}

function buildRuntimeSourceSelection(source: RuntimeSourceBinding): RuntimeSourceSelection {
	return {
		id: source.id,
		runtime_adapter: source.runtime_adapter,
		source_ref: source.source_ref,
		...(source.description ? { description: source.description } : {}),
	}
}

async function resolveRuntimeSourceSelection(
	agentFile: AgentFile,
	node: RuntimeAgentNode,
	adapter: RuntimeAdapter,
	capabilities: RuntimeAdapterCapabilities,
	userRuntimeSourceIds?: string[],
): Promise<RuntimeSourceSelection | undefined> {
	const matchingSources = (agentFile.runtime_sources ?? [])
		.filter((source) => source.runtime_adapter === node.runtime_adapter)
		.map((source) => buildRuntimeSourceSelection(source))
	const sourceLookup = new Map(matchingSources.map((source) => [source.id, source] as const))
	const policy = node.runtime_source_policy ?? 'inherit'
	const normalizedUserRuntimeSourceIds = userRuntimeSourceIds
		? [...new Set(userRuntimeSourceIds)]
		: undefined

	if (normalizedUserRuntimeSourceIds) {
		for (const sourceId of normalizedUserRuntimeSourceIds) {
			if (!sourceLookup.has(sourceId)) {
				throw new AppError(
					'INVALID_RUNTIME_CONTEXT',
					`Node "${node.id}" received unknown user runtime source "${sourceId}".`,
				)
			}
		}
	}

	if (policy === 'inherit' && node.runtime_source_ids !== undefined) {
		throw new AppError(
			'INVALID_RUNTIME_CONTEXT',
			`Node "${node.id}" cannot combine runtime_source_policy "inherit" with explicit runtime_source_ids.`,
		)
	}

	if (policy === 'inherit') {
		const eligibleSources =
			normalizedUserRuntimeSourceIds === undefined
				? matchingSources
				: matchingSources.filter((source) => normalizedUserRuntimeSourceIds.includes(source.id))

		if (eligibleSources.length === 0) {
			if (matchingSources.length === 0 && normalizedUserRuntimeSourceIds === undefined) {
				return undefined
			}
			throw new AppError(
				'RUNTIME_SOURCE_UNAVAILABLE',
				`Node "${node.id}" has no eligible runtime source after applying user narrowing.`,
			)
		}

		if (!capabilities.supports_explicit_runtime_source) {
			throw new AppError(
				'UNSUPPORTED_RUNTIME_CONTEXT',
				`Node "${node.id}" requires explicit runtime_source selection, but the runtime adapter does not support it.`,
			)
		}

		const sortedSources = [...eligibleSources].sort((left, right) =>
			left.id.localeCompare(right.id),
		)
		if (capabilities.supports_runtime_source_introspection) {
			const usableSources: RuntimeSourceSelection[] = []
			for (const source of sortedSources) {
				const inspection = await inspectRuntimeSourceIfSupported(adapter, capabilities, source)
				if (!inspection || isRuntimeSourceUsable(inspection)) {
					usableSources.push(source)
				}
			}
			if (usableSources.length === 0) {
				throw new AppError(
					'RUNTIME_SOURCE_UNAVAILABLE',
					`Node "${node.id}" has no usable runtime source after introspection.`,
				)
			}
			return usableSources[0]
		}
		return sortedSources[0]
	}

	const requestedIds = node.runtime_source_ids ?? []
	if (requestedIds.length === 0) {
		throw new AppError(
			'INVALID_RUNTIME_CONTEXT',
			`Node "${node.id}" requires a non-empty runtime_source_ids list for policy "${policy}".`,
		)
	}

	const requestedSources = requestedIds.map((sourceId) => {
		const source = sourceLookup.get(sourceId)
		if (!source) {
			throw new AppError(
				'INVALID_RUNTIME_CONTEXT',
				`Node "${node.id}" references unknown runtime source "${sourceId}".`,
			)
		}
		return source
	})
	const eligibleRequestedSources =
		normalizedUserRuntimeSourceIds === undefined
			? requestedSources
			: requestedSources.filter((source) => normalizedUserRuntimeSourceIds.includes(source.id))

	if (eligibleRequestedSources.length === 0) {
		throw new AppError(
			'RUNTIME_SOURCE_UNAVAILABLE',
			`Node "${node.id}" has no eligible runtime source after applying user narrowing.`,
		)
	}

	if (!capabilities.supports_explicit_runtime_source) {
		throw new AppError(
			'UNSUPPORTED_RUNTIME_CONTEXT',
			`Node "${node.id}" requires explicit runtime_source selection, but the runtime adapter does not support it.`,
		)
	}

	if (policy === 'restrict') {
		const usableSources: RuntimeSourceSelection[] = []
		for (const source of eligibleRequestedSources) {
			const inspection = await inspectRuntimeSourceIfSupported(adapter, capabilities, source)
			if (!inspection || isRuntimeSourceUsable(inspection)) {
				usableSources.push(source)
			}
		}
		if (usableSources.length === 0) {
			throw new AppError(
				'RUNTIME_SOURCE_UNAVAILABLE',
				`Node "${node.id}" has no usable runtime source for policy "restrict".`,
			)
		}
		return [...usableSources].sort((left, right) => left.id.localeCompare(right.id))[0]
	}

	for (const source of eligibleRequestedSources) {
		const inspection = await inspectRuntimeSourceIfSupported(adapter, capabilities, source)
		if (!inspection || isRuntimeSourceUsable(inspection)) {
			return source
		}
	}

	throw new AppError(
		'RUNTIME_SOURCE_UNAVAILABLE',
		`Node "${node.id}" has no usable runtime source for policy "prefer_first".`,
	)
}

function shouldUseNativeResume(
	snapshot: PersistedRunSnapshot,
	capabilities: RuntimeAdapterCapabilities,
): boolean {
	return (
		capabilities.supports_native_resume &&
		(snapshot.chat?.policy.prefer_native_resume ?? true) &&
		snapshot.resume.native_resume_available &&
		snapshot.resume.native_session_handle !== null
	)
}

function isBlockedOnUserPrompt(snapshot: PersistedRunSnapshot): boolean {
	return snapshot.run.status === 'waiting_for_user' || snapshot.resume.pending_prompt !== null
}

function reopenRunForNativeResume(stateStore: SQLiteLocalStateStore, runId: string): void {
	const database = (
		stateStore as unknown as {
			database?: {
				prepare(sql: string): {
					run(...params: unknown[]): unknown
				}
			}
		}
	).database

	if (!database) {
		throw new AppError('RUN_NOT_RESUMABLE', `Run "${runId}" is not available for native resume.`)
	}

	database
		.prepare(
			`
        UPDATE runs
        SET status = 'running', updated_at = ?
        WHERE run_id = ?
      `,
		)
		.run(new Date().toISOString(), runId)
}

function resolveRuntimeResumeRequest(
	snapshot: PersistedRunSnapshot,
	capabilities: RuntimeAdapterCapabilities,
): RuntimeResumeRequest {
	return shouldUseNativeResume(snapshot, capabilities)
		? {
				mode: 'native_resume',
				native_session_handle: snapshot.resume.native_session_handle as JsonValue,
			}
		: {
				mode: 'fresh',
			}
}

function hasNonEmptyPermissions(
	permissions: AgentFile['permissions'] | RuntimeAgentNode['permissions'] | undefined,
): boolean {
	if (!permissions) {
		return false
	}

	return (
		permissions.profile !== undefined ||
		(permissions.allow?.length ?? 0) > 0 ||
		(permissions.deny?.length ?? 0) > 0 ||
		Object.keys(permissions.extra ?? {}).length > 0
	)
}

function assertCommentInteractionPolicy(agentFile: AgentFile): void {
	const comments = agentFile.interaction?.comments
	if (comments?.enabled === true && (comments.target_node_ids?.length ?? 0) === 0) {
		throw new AppError(
			'UNSUPPORTED_INTERACTION',
			'interaction.comments.enabled requires at least one target_node_id.',
		)
	}
}

function assertSupportedRuntimeContext(
	agentFile: AgentFile,
	capabilities: RuntimeAdapterCapabilities,
): void {
	if (
		(agentFile.skills?.length ?? 0) > 0 ||
		(agentFile.mcps?.length ?? 0) > 0 ||
		(agentFile.plugins?.length ?? 0) > 0
	) {
		throw new AppError(
			'UNSUPPORTED_RUNTIME_CONTEXT',
			'Top-level skill, MCP, and plugin bindings are not implemented in the current execution slice.',
		)
	}

	if (agentFile.interaction?.user_mcp?.enabled && !capabilities.supports_builtin_user_chat_mcp) {
		throw new AppError(
			'UNSUPPORTED_INTERACTION',
			'Agent file enables interaction.user_mcp, but the runtime adapter does not support built-in user-chat MCP.',
		)
	}

	const commentsPolicy = agentFile.interaction?.comments
	if (
		commentsPolicy?.enabled &&
		(commentsPolicy.target_node_ids?.length ?? 0) > 0 &&
		!capabilities.supports_live_comments
	) {
		throw new AppError(
			'UNSUPPORTED_INTERACTION',
			'Agent file enables interaction.comments, but the runtime adapter does not support live comments.',
		)
	}

	if (hasNonEmptyPermissions(agentFile.permissions)) {
		throw new AppError(
			'UNSUPPORTED_RUNTIME_CONTEXT',
			'Top-level permissions are not implemented in the current execution slice.',
		)
	}

	for (const node of agentFile.nodes) {
		if (node.kind === 'orchestrator_agent') {
			continue
		}

		if (node.runtime_adapter !== 'codex') {
			throw new AppError(
				'UNSUPPORTED_RUNTIME_CONTEXT',
				`Node "${node.id}" targets unsupported runtime adapter "${node.runtime_adapter}" in the current execution slice.`,
			)
		}

		const unsupportedFeatures: string[] = []
		if ((node.skill_ids?.length ?? 0) > 0) {
			unsupportedFeatures.push('skill bindings')
		}
		if ((node.mcp_ids?.length ?? 0) > 0) {
			unsupportedFeatures.push('MCP bindings')
		}
		if ((node.plugin_ids?.length ?? 0) > 0) {
			unsupportedFeatures.push('plugin bindings')
		}
		if (hasNonEmptyPermissions(node.permissions)) {
			unsupportedFeatures.push('permissions')
		}
		for (const runtimeOptionKey of Object.keys(node.runtime_options ?? {})) {
			if (!supportedRuntimeOptionKeys.has(runtimeOptionKey)) {
				unsupportedFeatures.push(`runtime option "${runtimeOptionKey}"`)
			}
		}

		if (unsupportedFeatures.length > 0) {
			throw new AppError(
				'UNSUPPORTED_RUNTIME_CONTEXT',
				`Node "${node.id}" declares ${unsupportedFeatures.join(', ')}, which is not implemented in the current execution slice.`,
			)
		}

		const runtimeOptions = node.runtime_options ?? {}
		if ('reasoning_effort' in runtimeOptions && !capabilities.supports_reasoning_effort) {
			throw new AppError(
				'UNSUPPORTED_RUNTIME_CONTEXT',
				`Node "${node.id}" declares runtime option "reasoning_effort", but the runtime adapter does not support it.`,
			)
		}
		if ('speed_tier' in runtimeOptions && !capabilities.supports_speed_tiers) {
			throw new AppError(
				'UNSUPPORTED_RUNTIME_CONTEXT',
				`Node "${node.id}" declares runtime option "speed_tier", but the runtime adapter does not support it.`,
			)
		}
		if ('personality' in runtimeOptions && !capabilities.supports_personality) {
			throw new AppError(
				'UNSUPPORTED_RUNTIME_CONTEXT',
				`Node "${node.id}" declares runtime option "personality", but the runtime adapter does not support it.`,
			)
		}
	}

	assertCommentInteractionPolicy(agentFile)
}

function assertSupportedMemoryBindings(
	node: RuntimeAgentNode,
	memoryBindings: ResolvedMemoryBinding[],
	capabilities: RuntimeAdapterCapabilities,
): void {
	if (memoryBindings.length > 0 && !capabilities.supports_memory_bindings) {
		throw new AppError(
			'UNSUPPORTED_RUNTIME_CONTEXT',
			`Node "${node.id}" requires memory bindings, but the runtime adapter does not support them.`,
		)
	}
}

function resolveRuntimeInteraction(
	agentFile: AgentFile,
	node: RuntimeAgentNode,
): {
	comments_enabled: boolean
	user_chat_server_name?: typeof ORCHESTRATOR_USER_CHAT_SERVER_NAME
} {
	const commentsPolicy = agentFile.interaction?.comments
	const commentsTargetNodeIds = commentsPolicy?.target_node_ids
		? [...commentsPolicy.target_node_ids]
		: []
	if (commentsPolicy?.enabled === true && commentsTargetNodeIds.length === 0) {
		throw new AppError(
			'UNSUPPORTED_INTERACTION',
			'interaction.comments.enabled requires at least one target_node_id.',
		)
	}
	const commentsEnabled =
		commentsPolicy?.enabled === true && commentsTargetNodeIds.includes(node.id)

	return {
		comments_enabled: commentsEnabled,
		...(agentFile.interaction?.user_mcp?.enabled
			? { user_chat_server_name: ORCHESTRATOR_USER_CHAT_SERVER_NAME }
			: {}),
	}
}

function assertChildLaunchCompatibility(agentFile: AgentFile): void {
	if (agentFile.interaction?.comments?.enabled === true) {
		throw new AppError(
			'UNSUPPORTED_INTERACTION',
			'Child runs launched through orchestrator_agent cannot surface interaction.comments in the base model.',
		)
	}

	if (agentFile.interaction?.user_mcp?.enabled === true) {
		throw new AppError(
			'UNSUPPORTED_INTERACTION',
			'Child runs launched through orchestrator_agent cannot surface interaction.user_mcp in the base model.',
		)
	}
}

function nodeOutputsToMap(
	nodeOutputs: Map<string, NodeOutputRecord>,
): Map<string, NodeOutputRecord> {
	return new Map(nodeOutputs)
}

function evaluateEdgeCondition(conditionCode: string, state: RunStateSnapshot): boolean {
	const nodeProxy = new Proxy(
		{},
		{
			get: (_, key: string | symbol) => {
				if (typeof key !== 'string') {
					return undefined
				}
				const output = state.nodeOutputs.get(key)
				if (!output) {
					return undefined
				}
				return output.mode === 'text' ? { text: output.text } : { json: output.json }
			},
		},
	)

	const evaluator = new Function(
		'params',
		'vars',
		'node',
		'event',
		`"use strict"; return Boolean(${conditionCode});`,
	) as (
		params: Record<string, JsonValue>,
		vars: Record<string, JsonValue>,
		node: unknown,
		event: unknown,
	) => boolean

	return evaluator(state.params, state.vars, nodeProxy, state.event ?? undefined)
}

function selectNextNode(
	agentFile: AgentFile,
	currentNode: AgentNode,
	state: RunStateSnapshot,
	nodeLookup: Map<string, AgentNode>,
): AgentNode | undefined {
	const outgoingEdges = (agentFile.edges ?? []).filter(
		(edge: { from: string; to: string; condition?: { code: string } }) =>
			edge.from === currentNode.id,
	)
	for (const edge of outgoingEdges) {
		if (!edge.condition) {
			return nodeLookup.get(edge.to)
		}
		if (evaluateEdgeCondition(edge.condition.code, state)) {
			return nodeLookup.get(edge.to)
		}
	}
	return undefined
}

function materializeState(snapshot: PersistedRunSnapshot): RunStateSnapshot {
	return {
		params: snapshot.run.params,
		vars: { ...snapshot.current_vars },
		nodeOutputs: new Map(
			snapshot.latest_committed_outputs.map((output) => [output.node_id, output.output]),
		),
		event: snapshot.run.event ?? undefined,
	}
}

function getNodeOrThrow(nodeLookup: Map<string, AgentNode>, nodeId: string): AgentNode {
	const node = nodeLookup.get(nodeId)
	if (!node) {
		throw new AppError(
			'MISSING_NODE',
			`Node "${nodeId}" does not exist in the pinned agent revision.`,
		)
	}
	return node
}

function deriveFinalOutput(
	agentFile: AgentFile,
	snapshot: PersistedRunSnapshot,
): {
	final_output: JsonValue | null
	final_output_mode: 'text' | 'json' | null
} {
	const finalOutputMode = agentFile.final_output?.mode ?? 'last_node_output'
	if (finalOutputMode === 'none') {
		return {
			final_output: null,
			final_output_mode: null,
		}
	}

	const lastSuccessfulOutput = [...snapshot.latest_committed_outputs]
		.sort((left, right) => left.boundary_sequence - right.boundary_sequence)
		.at(-1)
	if (!lastSuccessfulOutput) {
		return {
			final_output: null,
			final_output_mode: null,
		}
	}

	return lastSuccessfulOutput.output.mode === 'text'
		? {
				final_output: lastSuccessfulOutput.output.text,
				final_output_mode: 'text',
			}
		: {
				final_output: lastSuccessfulOutput.output.json,
				final_output_mode: 'json',
			}
}

function buildSuccessResult(
	agentFile: AgentFile,
	snapshot: PersistedRunSnapshot,
): RunResultSuccess {
	const finalOutput = deriveFinalOutput(agentFile, snapshot)
	return {
		status: 'success',
		run_id: snapshot.run.run_id,
		run_status: 'completed',
		final_output: finalOutput.final_output,
		final_output_mode: finalOutput.final_output_mode,
		node_outputs: nodeOutputsToMap(
			new Map(snapshot.latest_committed_outputs.map((output) => [output.node_id, output.output])),
		),
	}
}

function classifyOutputCandidate(args: {
	node_id: string
	output: OutputContract
	candidate_mode: 'text' | 'json' | null
	candidate_value: JsonValue | null
	current_vars: Record<string, JsonValue>
	native_session_handle: unknown | null
	missing_output_code?: string
	missing_output_message?: string
}): ClassifiedExecutionResult {
	if (args.candidate_mode === null) {
		return {
			kind: 'terminal_failure',
			outcome: 'invalid_output',
			code: args.missing_output_code ?? 'INVALID_OUTPUT',
			message:
				args.missing_output_message ??
				`Node "${args.node_id}" did not produce a final response payload.`,
			native_session_handle: args.native_session_handle,
		}
	}

	if (args.output.mode === 'text') {
		if (args.candidate_mode !== 'text' || typeof args.candidate_value !== 'string') {
			return {
				kind: 'terminal_failure',
				outcome: 'invalid_output',
				code: 'INVALID_TEXT_OUTPUT',
				message: `Node "${args.node_id}" declared text output but produced a non-string terminal payload.`,
				native_session_handle: args.native_session_handle,
			}
		}

		return {
			kind: 'success',
			output: {
				mode: 'text',
				text: args.candidate_value,
			},
			next_vars: { ...args.current_vars },
			native_session_handle: args.native_session_handle,
		}
	}

	if (
		args.candidate_mode !== 'json' ||
		args.candidate_value === null ||
		Array.isArray(args.candidate_value) ||
		typeof args.candidate_value !== 'object'
	) {
		return {
			kind: 'terminal_failure',
			outcome: 'invalid_output',
			code: 'INVALID_JSON_OUTPUT',
			message: `Node "${args.node_id}" declared json output but produced a non-object JSON payload.`,
			native_session_handle: args.native_session_handle,
		}
	}

	const candidateJson = args.candidate_value as JsonObject
	const validation = validateJsonOutputAgainstSchema(args.output.schema, candidateJson)
	if (!validation.valid) {
		return {
			kind: 'terminal_failure',
			outcome: 'invalid_output',
			code: 'INVALID_JSON_OUTPUT',
			message:
				`Node "${args.node_id}" produced JSON that failed its declared output schema. ${validation.message ?? ''}`.trim(),
			native_session_handle: args.native_session_handle,
		}
	}

	return {
		kind: 'success',
		output: {
			mode: 'json',
			json: candidateJson,
		},
		next_vars: {
			...args.current_vars,
			...candidateJson,
		},
		native_session_handle: args.native_session_handle,
	}
}

function classifyRuntimeExecutionResult(
	node: RuntimeAgentNode,
	execution: RuntimeTerminalResult,
	currentVars: Record<string, JsonValue>,
): ClassifiedExecutionResult {
	if (execution.outcome !== 'success') {
		return {
			kind: 'terminal_failure',
			outcome: execution.outcome,
			code: execution.error.code,
			message: execution.error.message,
			native_session_handle: execution.native_session_handle ?? null,
		}
	}

	if (execution.output.mode === 'text') {
		const textExecution = execution as RuntimeSuccessResult & {
			output: { mode: 'text' }
			output_text: string
		}
		return classifyOutputCandidate({
			node_id: node.id,
			output: node.output,
			candidate_mode: 'text',
			candidate_value: textExecution.output_text,
			current_vars: currentVars,
			native_session_handle: execution.native_session_handle ?? null,
		})
	}

	const jsonExecution = execution as RuntimeSuccessResult & {
		output: { mode: 'json' }
		output_json: JsonObject
	}
	return classifyOutputCandidate({
		node_id: node.id,
		output: node.output,
		candidate_mode: 'json',
		candidate_value: jsonExecution.output_json,
		current_vars: currentVars,
		native_session_handle: execution.native_session_handle ?? null,
	})
}

function mapTerminalOutcomeToRunStatus(
	outcome: ClassifiedExecutionResult & { kind: 'terminal_failure' },
): Extract<RunStatus, 'failed' | 'cancelled' | 'interrupted'> {
	if (outcome.outcome === 'cancelled') {
		return 'cancelled'
	}
	if (outcome.outcome === 'interrupted') {
		return 'interrupted'
	}
	return 'failed'
}

function shouldAllowLocalResume(
	outcome: ClassifiedExecutionResult & { kind: 'terminal_failure' },
): boolean {
	return outcome.outcome !== 'cancelled'
}

function classifyChildRunFailure(
	node: OrchestratorAgentNode,
	result: RunResultFailure,
	childOutcome: 'invalid_output' | 'runtime_error' | 'cancelled' | 'interrupted',
): ClassifiedExecutionResult & { kind: 'terminal_failure' } {
	return {
		kind: 'terminal_failure',
		outcome: childOutcome,
		code: result.code,
		message: `Child run for node "${node.id}" failed. ${result.message}`,
		native_session_handle: null,
	}
}

function readChildTerminalOutcome(
	stateStore: SQLiteLocalStateStore,
	runId: string,
): 'invalid_output' | 'runtime_error' | 'cancelled' | 'interrupted' {
	const snapshot = stateStore.getPersistedRunSnapshot(runId)
	const outcome = snapshot?.attempts.at(-1)?.outcome
	if (
		outcome === 'invalid_output' ||
		outcome === 'runtime_error' ||
		outcome === 'cancelled' ||
		outcome === 'interrupted'
	) {
		return outcome
	}

	throw new AppError(
		'RUN_STATE_INCONSISTENT',
		`Child run "${runId}" did not persist a terminal outcome that can be mapped back to the parent node.`,
	)
}

function buildEdgeTransitionFailureResult(args: {
	run_id: string
	native_session_handle: unknown | null
	code: string
	message: string
	stateStore: SQLiteLocalStateStore
	attempt_id: string
}): RunResultFailure {
	args.stateStore.commitNodeTerminalOutcome({
		attempt_id: args.attempt_id,
		outcome: 'runtime_error',
		run_status: 'failed',
		resume: {
			native_resume_available: Boolean(args.native_session_handle),
			local_resume_available: true,
			native_session_handle: args.native_session_handle as JsonValue | null,
		},
	})

	return {
		status: 'failure',
		run_id: args.run_id,
		run_status: 'failed',
		code: args.code,
		message: args.message,
		resume_available: true,
	}
}

function resolveExecutionCursor(
	agentFile: AgentFile,
	snapshot: PersistedRunSnapshot,
	state: RunStateSnapshot,
	nodeLookup: Map<string, AgentNode>,
	allow_native_resume_without_local: boolean,
	pendingUserChatReply: UserChatResponsePayload | null,
): ExecutionCursor {
	if (snapshot.resume.pending_prompt && pendingUserChatReply === null) {
		throw new AppError(
			'RUN_WAITING_FOR_USER',
			`Run "${snapshot.run.run_id}" is blocked on unresolved user input and cannot be locally resumed by this slice.`,
		)
	}

	const lastAttempt = snapshot.attempts.at(-1)
	if (!lastAttempt) {
		return {
			kind: 'execute',
			node: getNodeOrThrow(nodeLookup, agentFile.entry_node_id),
			reuse_attempt_id: null,
		}
	}

	if (lastAttempt.state === 'in_progress') {
		return {
			kind: 'execute',
			node: getNodeOrThrow(nodeLookup, lastAttempt.node_id),
			reuse_attempt_id: lastAttempt.attempt_id,
		}
	}

	if (lastAttempt.state === 'blocked_wait') {
		return {
			kind: 'execute',
			node: getNodeOrThrow(nodeLookup, lastAttempt.node_id),
			reuse_attempt_id: null,
		}
	}

	if (lastAttempt.outcome === 'success') {
		if (snapshot.run.status === 'completed') {
			if (snapshot.resume.local_resume_available) {
				throw new AppError(
					'RUN_STATE_INCONSISTENT',
					`Run "${snapshot.run.run_id}" is completed but still marked as locally resumable.`,
				)
			}
			throw new AppError('RUN_NOT_RESUMABLE', `Run "${snapshot.run.run_id}" has already completed.`)
		}

		const currentNode = getNodeOrThrow(nodeLookup, lastAttempt.node_id)
		const nextNode = selectNextNode(agentFile, currentNode, state, nodeLookup)
		if (!nextNode) {
			return {
				kind: 'completed',
			}
		}
		return {
			kind: 'execute',
			node: nextNode,
			reuse_attempt_id: null,
		}
	}

	if (!snapshot.resume.local_resume_available && !allow_native_resume_without_local) {
		throw new AppError(
			'RUN_NOT_RESUMABLE',
			`Run "${snapshot.run.run_id}" is terminal with outcome "${lastAttempt.outcome}" and is not marked for local resume.`,
		)
	}

	return {
		kind: 'execute',
		node: getNodeOrThrow(nodeLookup, lastAttempt.node_id),
		reuse_attempt_id: null,
	}
}

async function executeFromSnapshot(
	agentFile: AgentFile,
	adapter: RuntimeAdapter,
	capabilities: RuntimeAdapterCapabilities,
	stateStore: SQLiteLocalStateStore,
	initialSnapshot: PersistedRunSnapshot,
	allow_native_resume_without_local: boolean,
	pendingUserChatReply: UserChatResponsePayload | null,
	userRuntimeSourceIds?: string[],
): Promise<RunResult> {
	const nodeLookup = buildNodeLookup(agentFile)
	const lifecycleService = new AgentLifecycleService({ state_store: stateStore })
	const memoryService = new MemoryService({ state_store: stateStore })
	const state = materializeState(initialSnapshot)
	let snapshot = initialSnapshot
	let cursor = resolveExecutionCursor(
		agentFile,
		snapshot,
		state,
		nodeLookup,
		allow_native_resume_without_local,
		pendingUserChatReply,
	)

	if (cursor.kind === 'completed') {
		throw new AppError('RUN_NOT_RESUMABLE', `Run "${snapshot.run.run_id}" has already completed.`)
	}

	while (cursor.kind === 'execute') {
		let resolvedInput: string
		try {
			resolvedInput = resolveNodeInput(cursor.node.input.parts, state)
		} catch (error) {
			const code = error instanceof AppError ? error.code : 'NODE_PREPARATION_FAILED'
			const message = error instanceof Error ? error.message : 'Unknown node preparation failure.'
			const attemptId =
				cursor.reuse_attempt_id ??
				stateStore.startNodeAttempt({
					run_id: snapshot.run.run_id,
					node_id: cursor.node.id,
					output_mode: cursor.node.output.mode,
					runtime_handle: null,
				}).attempt_id
			return buildEdgeTransitionFailureResult({
				run_id: snapshot.run.run_id,
				native_session_handle: null,
				code,
				message: `Node "${cursor.node.id}" could not be prepared for execution. ${message}`,
				stateStore,
				attempt_id: attemptId,
			})
		}

		let attemptId: string
		let classified: ClassifiedExecutionResult
		let runtimeMemoryBindings: MemoryBinding[] = []
		let runtimeMemoryScope: RuntimeMemoryOperationScope | null = null

		if (cursor.node.kind === 'runtime_agent') {
			try {
				const interaction = resolveRuntimeInteraction(agentFile, cursor.node)
				runtimeMemoryBindings = buildSelectedMemoryBindings(agentFile, cursor.node)
				const memory_bindings = toResolvedMemoryBindings(runtimeMemoryBindings)
				assertSupportedMemoryBindings(cursor.node, memory_bindings, capabilities)
				runtimeMemoryScope = buildRuntimeMemoryOperationScope(agentFile, snapshot)
				const runtime_context = {
					memory_bindings,
					memory_context: await prepareRuntimeMemoryContext({
						memoryService,
						bindings: runtimeMemoryBindings,
						scope: runtimeMemoryScope,
						read_query: resolvedInput,
					}),
					runtime_source: await resolveRuntimeSourceSelection(
						agentFile,
						cursor.node,
						adapter,
						capabilities,
						userRuntimeSourceIds,
					),
				}
				const runtimeResume = resolveRuntimeResumeRequest(snapshot, capabilities)
				const executionSession = await adapter.startExecution(
					createRuntimeRequest(
						cursor.node,
						resolvedInput,
						runtimeResume,
						{
							...interaction,
							...(pendingUserChatReply ? { user_chat_reply: pendingUserChatReply } : {}),
						},
						runtime_context,
					),
				)
				attemptId =
					cursor.reuse_attempt_id ??
					stateStore.startNodeAttempt({
						run_id: snapshot.run.run_id,
						node_id: cursor.node.id,
						output_mode: cursor.node.output.mode,
						runtime_handle:
							executionSession.runtime_handle ?? executionSession.native_session_handle ?? null,
					}).attempt_id
				if (pendingUserChatReply === null) {
					const blockingPrompt = await Promise.race([
						executionSession.terminal_result.then(
							(result) => ({ kind: 'terminal' as const, result }),
							(error) => {
								throw error
							},
						),
						waitForBlockingUserChatRequest(executionSession.events).then((event) => ({
							kind: 'prompt' as const,
							event,
						})),
					])

					if (blockingPrompt.kind === 'prompt') {
						const promptPayload = blockingPrompt.event.payload as unknown as JsonValue
						stateStore.appendVisibleChatMessage({
							run_id: snapshot.run.run_id,
							kind: 'blocking_prompt',
							payload: promptPayload,
						})
						stateStore.commitBlockedAttempt({
							attempt_id: attemptId,
							pending_prompt: {
								prompt_id: blockingPrompt.event.payload.prompt_id ?? null,
								payload: promptPayload,
								request_handle: blockingPrompt.event.request_handle as JsonValue,
							},
							resume: {
								native_resume_available: Boolean(executionSession.native_session_handle),
								local_resume_available: true,
								native_session_handle: executionSession.native_session_handle ?? null,
							},
						})
						return {
							status: 'waiting_for_user',
							run_id: snapshot.run.run_id,
							run_status: 'waiting_for_user',
							code: 'RUN_WAITING_FOR_USER',
							message: `Run "${snapshot.run.run_id}" is blocked on user input from node "${cursor.node.id}".`,
							resume_available: true,
						}
					}

					const execution = {
						...blockingPrompt.result,
						native_session_handle:
							blockingPrompt.result.native_session_handle ??
							executionSession.native_session_handle ??
							null,
					}
					classified = classifyRuntimeExecutionResult(cursor.node, execution, state.vars)
				} else {
					const terminalExecution = await executionSession.terminal_result
					const execution = {
						...terminalExecution,
						native_session_handle:
							terminalExecution.native_session_handle ??
							executionSession.native_session_handle ??
							null,
					}
					classified = classifyRuntimeExecutionResult(cursor.node, execution, state.vars)
				}
			} catch (error) {
				const code = error instanceof AppError ? error.code : 'NODE_EXECUTION_FAILED'
				const message = error instanceof Error ? error.message : 'Unknown node execution failure.'
				attemptId =
					cursor.reuse_attempt_id ??
					stateStore.startNodeAttempt({
						run_id: snapshot.run.run_id,
						node_id: cursor.node.id,
						output_mode: cursor.node.output.mode,
						runtime_handle: null,
					}).attempt_id
				return buildEdgeTransitionFailureResult({
					run_id: snapshot.run.run_id,
					native_session_handle: null,
					code,
					message: `Node "${cursor.node.id}" failed before terminal classification. ${message}`,
					stateStore,
					attempt_id: attemptId,
				})
			}
		} else {
			attemptId =
				cursor.reuse_attempt_id ??
				stateStore.startNodeAttempt({
					run_id: snapshot.run.run_id,
					node_id: cursor.node.id,
					output_mode: cursor.node.output.mode,
					runtime_handle: null,
				}).attempt_id
			try {
				const childAgent = await lifecycleService.resolveLiveAgentFile(cursor.node.agent_ref)
				assertChildLaunchCompatibility(childAgent.agent_file)
				const childResult = await runAgentFile(
					childAgent.agent_file,
					adapter,
					{
						input: resolvedInput,
					},
					{
						state_store: stateStore,
						resolved_revision_id: childAgent.resolved_revision_id,
						logical_agent_id: childAgent.logical_agent_id,
						started_via: 'direct',
					},
				)
				if (childResult.status === 'failure') {
					classified = classifyChildRunFailure(
						cursor.node,
						childResult,
						readChildTerminalOutcome(stateStore, childResult.run_id),
					)
				} else if (childResult.status === 'waiting_for_user') {
					throw new AppError(
						childResult.code,
						`Child run for node "${cursor.node.id}" did not complete successfully. ${childResult.message}`,
					)
				} else {
					classified = classifyOutputCandidate({
						node_id: cursor.node.id,
						output: cursor.node.output,
						candidate_mode: childResult.final_output_mode,
						candidate_value: childResult.final_output,
						current_vars: state.vars,
						native_session_handle: null,
						missing_output_code: 'CHILD_FINAL_OUTPUT_MISSING',
						missing_output_message: `Child run for node "${cursor.node.id}" completed without a final response payload.`,
					})
				}
			} catch (error) {
				const code = error instanceof AppError ? error.code : 'CHILD_RUN_FAILED'
				const message = error instanceof Error ? error.message : 'Unknown child-run failure.'
				return buildEdgeTransitionFailureResult({
					run_id: snapshot.run.run_id,
					native_session_handle: null,
					code,
					message: `Node "${cursor.node.id}" could not launch child agent "${cursor.node.agent_ref}". ${message}`,
					stateStore,
					attempt_id: attemptId,
				})
			}
		}

		if (classified.kind === 'success') {
			state.nodeOutputs.set(cursor.node.id, classified.output)
			state.vars = { ...classified.next_vars }

			let nextNode: AgentNode | undefined
			try {
				nextNode = selectNextNode(agentFile, cursor.node, state, nodeLookup)
			} catch (error) {
				const code = error instanceof AppError ? error.code : 'EDGE_CONDITION_EVALUATION_FAILED'
				const message =
					error instanceof Error ? error.message : 'Unknown edge condition evaluation failure.'
				return buildEdgeTransitionFailureResult({
					run_id: snapshot.run.run_id,
					native_session_handle: classified.native_session_handle,
					code,
					message: `Edge condition evaluation failed while advancing from node "${cursor.node.id}". ${message}`,
					stateStore,
					attempt_id: attemptId,
				})
			}

			const runStatus = nextNode ? 'running' : 'completed'

			if (cursor.node.kind === 'runtime_agent' && runtimeMemoryScope) {
				try {
					await writeRuntimeMemoryOnNodeSuccess({
						memoryService,
						bindings: runtimeMemoryBindings,
						scope: runtimeMemoryScope,
						node_id: cursor.node.id,
						attempt_id: attemptId,
						output: classified.output,
					})
				} catch (error) {
					const code = error instanceof AppError ? error.code : 'MEMORY_WRITE_FAILED'
					const message =
						error instanceof Error ? error.message : 'Unknown runtime memory write failure.'
					return buildEdgeTransitionFailureResult({
						run_id: snapshot.run.run_id,
						native_session_handle: classified.native_session_handle,
						code,
						message: `Runtime memory write failed while completing node "${cursor.node.id}". ${message}`,
						stateStore,
						attempt_id: attemptId,
					})
				}
			}

			stateStore.commitNodeSuccess({
				attempt_id: attemptId,
				output: classified.output,
				vars: state.vars,
				run_status: runStatus,
				resume: {
					native_resume_available: Boolean(classified.native_session_handle),
					local_resume_available: runStatus !== 'completed',
					native_session_handle: classified.native_session_handle as JsonValue | null,
				},
			})

			const persistedSnapshot = stateStore.getPersistedRunSnapshot(snapshot.run.run_id)
			if (!persistedSnapshot) {
				throw new AppError('RUN_NOT_FOUND', `Run "${attemptId}" disappeared during execution.`)
			}
			snapshot = persistedSnapshot

			if (!nextNode) {
				return buildSuccessResult(agentFile, snapshot)
			}

			cursor = {
				kind: 'execute',
				node: nextNode,
				reuse_attempt_id: null,
			}
			continue
		}

		const runStatus = mapTerminalOutcomeToRunStatus(classified)
		const resumeAvailable = shouldAllowLocalResume(classified)

		stateStore.commitNodeTerminalOutcome({
			attempt_id: attemptId,
			outcome: classified.outcome,
			run_status: runStatus,
			resume: {
				native_resume_available: Boolean(classified.native_session_handle),
				local_resume_available: resumeAvailable,
				native_session_handle: classified.native_session_handle as JsonValue | null,
			},
		})

		return {
			status: 'failure',
			run_id: snapshot.run.run_id,
			run_status: runStatus,
			code: classified.code,
			message: classified.message,
			resume_available: resumeAvailable,
		}
	}

	throw new AppError(
		'RUN_STATE_INCONSISTENT',
		`Run "${snapshot.run.run_id}" resolved to an invalid execution cursor.`,
	)
}

export async function runAgentFile(
	agentFile: AgentFile,
	adapter: RuntimeAdapter,
	params: Record<string, JsonValue>,
	options: RunAgentFileOptions,
): Promise<RunResult> {
	const capabilities = adapter.describeCapabilities()
	assertSupportedRuntimeContext(agentFile, capabilities)

	const run = options.state_store.createRun({
		run_id: options.run_id,
		logical_agent_id: options.logical_agent_id ?? agentFile.meta.id,
		resolved_revision_id: options.resolved_revision_id,
		entry_node_id: agentFile.entry_node_id,
		started_via: options.started_via ?? 'direct',
		params,
		event: options.event ?? null,
		initial_vars: agentFile.initial_vars ?? {},
		chat: agentFile.chat
			? {
					policy: {
						prefer_native_resume: agentFile.chat.prefer_native_resume,
						store_visible_messages: agentFile.chat.store_visible_messages,
						store_context_window: agentFile.chat.store_context_window,
						allow_fresh_start: agentFile.chat.allow_fresh_start,
					},
				}
			: undefined,
		resume: {
			native_resume_available: false,
			local_resume_available: true,
		},
	})

	const snapshot = options.state_store.getPersistedRunSnapshot(run.run_id)
	if (!snapshot) {
		throw new AppError('RUN_NOT_FOUND', `Run "${run.run_id}" does not exist after creation.`)
	}

	return executeFromSnapshot(
		agentFile,
		adapter,
		capabilities,
		options.state_store,
		snapshot,
		false,
		null,
		options.user_runtime_source_ids,
	)
}

export async function resumeAgentRun(
	agentFile: AgentFile,
	adapter: RuntimeAdapter,
	runId: string,
	options: ResumeAgentRunOptions,
): Promise<RunResult> {
	const capabilities = adapter.describeCapabilities()
	assertSupportedRuntimeContext(agentFile, capabilities)

	let snapshot = options.state_store.getPersistedRunSnapshot(runId)
	if (!snapshot) {
		throw new AppError('RUN_NOT_FOUND', `Run "${runId}" does not exist.`)
	}

	if (snapshot.run.resolved_revision_id !== options.resolved_revision_id) {
		throw new AppError(
			'RESUME_REVISION_MISMATCH',
			`Run "${runId}" is pinned to "${snapshot.run.resolved_revision_id}" but resume requested "${options.resolved_revision_id}".`,
		)
	}

	const pendingUserChatReply = resolvePendingUserChatReply(snapshot)
	if (isBlockedOnUserPrompt(snapshot) && pendingUserChatReply === null) {
		throw new AppError(
			'RUN_WAITING_FOR_USER',
			`Run "${runId}" is blocked on unresolved user input and cannot be locally resumed by this slice.`,
		)
	}

	const nativeResumePreferred = shouldUseNativeResume(snapshot, capabilities)
	if (!snapshot.resume.local_resume_available && !nativeResumePreferred) {
		throw new AppError(
			'RUN_NOT_RESUMABLE',
			`Run "${runId}" is not available for explicit local resume.`,
		)
	}

	if (snapshot.run.status === 'completed') {
		throw new AppError('RUN_NOT_RESUMABLE', `Run "${runId}" has already completed.`)
	}

	if (snapshot.run.status !== 'running') {
		if (nativeResumePreferred && snapshot.run.status !== 'cancelled') {
			reopenRunForNativeResume(options.state_store, runId)
		} else if (!snapshot.resume.local_resume_available) {
			throw new AppError(
				'RUN_NOT_RESUMABLE',
				`Run "${runId}" is not available for explicit local resume.`,
			)
		} else {
			options.state_store.reopenRunForExplicitResume(runId, options.resolved_revision_id)
		}
		snapshot = options.state_store.getPersistedRunSnapshot(runId)
		if (!snapshot) {
			throw new AppError(
				'RUN_NOT_FOUND',
				`Run "${runId}" disappeared during explicit local resume.`,
			)
		}
	}

	return executeFromSnapshot(
		agentFile,
		adapter,
		capabilities,
		options.state_store,
		snapshot,
		nativeResumePreferred,
		pendingUserChatReply,
	)
}
