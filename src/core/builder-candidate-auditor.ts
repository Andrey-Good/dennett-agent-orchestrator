import type { RuntimeAdapter, RuntimeAdapterCapabilities } from '../ports/runtime.js'
import type { AgentFile, RuntimeAgentNode } from './agent-file.js'
import type { JsonObject, JsonValue } from './json.js'
import { createAjv2020Validator } from './output-schema-validator.js'

export type BuilderCandidateAuditSeverity = 'error' | 'warning'

export interface BuilderCandidateAuditIssue {
	severity: BuilderCandidateAuditSeverity
	code: string
	path: string
	message: string
}

export interface BuilderCandidateAuditDiagnostics {
	status: 'accepted' | 'rejected'
	issues: BuilderCandidateAuditIssue[]
	capabilities: RuntimeAdapterCapabilities
}

const supportedRuntimeOptionKeys = new Set([
	'model',
	'reasoning_effort',
	'speed_tier',
	'personality',
])

const supportedReasoningEfforts = new Set(['none', 'minimal', 'low', 'medium', 'high', 'xhigh'])
const supportedSpeedTiers = new Set(['fast', 'flex'])
const supportedPersonalities = new Set(['none', 'friendly', 'pragmatic'])

const forbiddenSecretOrLocalConfigKeys = new Set([
	'api_key',
	'apikey',
	'auth',
	'auth_state',
	'credentials',
	'credential',
	'env',
	'local_config',
	'package_path',
	'provider_id',
	'provider_registration_id',
	'python_executable',
	'rate_limit',
	'rate_limits',
	'secret',
	'secrets',
	'token',
])

const forbiddenManagedSubagentKeys = new Set([
	'budget',
	'budgets',
	'control_message',
	'control_messages',
	'lineage',
	'managed_subagent',
	'managed_subagents',
	'subagent_task_package',
	'task_package',
	'write_scope',
	'write_set',
])

function normalizeKey(key: string): string {
	return key.replace(/[-\s]/g, '_').toLowerCase()
}

function addError(
	issues: BuilderCandidateAuditIssue[],
	code: string,
	path: string,
	message: string,
): void {
	issues.push({
		severity: 'error',
		code,
		path,
		message,
	})
}

function joinPath(parent: string, key: string | number): string {
	return `${parent}/${String(key).replace(/~/g, '~0').replace(/\//g, '~1')}`
}

function isJsonObject(value: JsonValue | unknown): value is JsonObject {
	return value !== null && typeof value === 'object' && !Array.isArray(value)
}

function auditJsonOutputSchemas(agentFile: AgentFile, issues: BuilderCandidateAuditIssue[]): void {
	for (let index = 0; index < agentFile.nodes.length; index += 1) {
		const node = agentFile.nodes[index]
		if (node.output.mode !== 'json') {
			continue
		}

		try {
			createAjv2020Validator().compile(node.output.schema as never)
		} catch (error) {
			addError(
				issues,
				'JSON_OUTPUT_SCHEMA_INVALID',
				`/nodes/${index}/output/schema`,
				`Node "${node.id}" output JSON schema could not be compiled: ${
					error instanceof Error ? error.message : 'Unknown schema compilation error.'
				}`,
			)
		}
	}
}

function auditRuntimeOptions(
	node: RuntimeAgentNode,
	nodePath: string,
	capabilities: RuntimeAdapterCapabilities,
	issues: BuilderCandidateAuditIssue[],
): void {
	const runtimeOptions = node.runtime_options ?? {}

	for (const [key, value] of Object.entries(runtimeOptions)) {
		const path = `${nodePath}/runtime_options/${key}`
		if (!supportedRuntimeOptionKeys.has(key)) {
			addError(
				issues,
				'RUNTIME_OPTION_UNKNOWN',
				path,
				`Node "${node.id}" declares unsupported runtime option "${key}".`,
			)
			continue
		}

		if (key === 'model' && typeof value !== 'string') {
			addError(
				issues,
				'RUNTIME_OPTION_INVALID_TYPE',
				path,
				'runtime_options.model must be a string.',
			)
		}

		if (key === 'reasoning_effort') {
			if (typeof value !== 'string' || !supportedReasoningEfforts.has(value)) {
				addError(
					issues,
					'RUNTIME_OPTION_INVALID_VALUE',
					path,
					'runtime_options.reasoning_effort must be one of: none, minimal, low, medium, high, xhigh.',
				)
			}
			if (!capabilities.supports_reasoning_effort) {
				addError(
					issues,
					'RUNTIME_CAPABILITY_UNSUPPORTED',
					path,
					`Node "${node.id}" declares runtime option "reasoning_effort", but the selected runtime adapter does not support it.`,
				)
			}
		}

		if (key === 'speed_tier') {
			if (typeof value !== 'string' || !supportedSpeedTiers.has(value)) {
				addError(
					issues,
					'RUNTIME_OPTION_INVALID_VALUE',
					path,
					'runtime_options.speed_tier must be one of: fast, flex.',
				)
			}
			if (!capabilities.supports_speed_tiers) {
				addError(
					issues,
					'RUNTIME_CAPABILITY_UNSUPPORTED',
					path,
					`Node "${node.id}" declares runtime option "speed_tier", but the selected runtime adapter does not support it.`,
				)
			}
		}

		if (key === 'personality') {
			if (typeof value !== 'string' || !supportedPersonalities.has(value)) {
				addError(
					issues,
					'RUNTIME_OPTION_INVALID_VALUE',
					path,
					'runtime_options.personality must be one of: none, friendly, pragmatic.',
				)
			}
			if (!capabilities.supports_personality) {
				addError(
					issues,
					'RUNTIME_CAPABILITY_UNSUPPORTED',
					path,
					`Node "${node.id}" declares runtime option "personality", but the selected runtime adapter does not support it.`,
				)
			}
		}
	}
}

function auditRuntimeCapabilities(
	agentFile: AgentFile,
	capabilities: RuntimeAdapterCapabilities,
	issues: BuilderCandidateAuditIssue[],
): void {
	if (
		(agentFile.runtime_sources?.length ?? 0) > 0 &&
		!capabilities.supports_explicit_runtime_source
	) {
		addError(
			issues,
			'RUNTIME_CAPABILITY_UNSUPPORTED',
			'/runtime_sources',
			'Agent declares runtime_sources, but the selected runtime adapter does not support explicit runtime sources.',
		)
	}

	if ((agentFile.memory_bindings?.length ?? 0) > 0 && !capabilities.supports_memory_bindings) {
		addError(
			issues,
			'RUNTIME_CAPABILITY_UNSUPPORTED',
			'/memory_bindings',
			'Agent declares memory_bindings, but the selected runtime adapter does not support memory bindings.',
		)
	}

	if (agentFile.interaction?.comments?.enabled === true && !capabilities.supports_live_comments) {
		addError(
			issues,
			'RUNTIME_CAPABILITY_UNSUPPORTED',
			'/interaction/comments',
			'Agent enables interaction.comments, but the selected runtime adapter does not support live comments.',
		)
	}

	if (
		agentFile.interaction?.user_mcp?.enabled === true &&
		!capabilities.supports_builtin_user_chat_mcp
	) {
		addError(
			issues,
			'RUNTIME_CAPABILITY_UNSUPPORTED',
			'/interaction/user_mcp',
			'Agent enables interaction.user_mcp, but the selected runtime adapter does not support built-in user chat MCP.',
		)
	}

	for (let index = 0; index < agentFile.nodes.length; index += 1) {
		const node = agentFile.nodes[index]
		if (node.kind !== 'runtime_agent') {
			continue
		}

		const nodePath = `/nodes/${index}`
		auditRuntimeOptions(node, nodePath, capabilities, issues)

		if (
			node.runtime_source_policy !== undefined &&
			node.runtime_source_policy !== 'inherit' &&
			!capabilities.supports_explicit_runtime_source
		) {
			addError(
				issues,
				'RUNTIME_CAPABILITY_UNSUPPORTED',
				`${nodePath}/runtime_source_policy`,
				`Node "${node.id}" requires explicit runtime source selection, but the selected runtime adapter does not support it.`,
			)
		}

		if ((node.memory_ids?.length ?? 0) > 0 && !capabilities.supports_memory_bindings) {
			addError(
				issues,
				'RUNTIME_CAPABILITY_UNSUPPORTED',
				`${nodePath}/memory_ids`,
				`Node "${node.id}" requires memory bindings, but the selected runtime adapter does not support them.`,
			)
		}
	}
}

function auditForbiddenObjectKeys(
	value: JsonValue,
	path: string,
	issues: BuilderCandidateAuditIssue[],
): void {
	if (Array.isArray(value)) {
		for (const [index, item] of value.entries()) {
			auditForbiddenObjectKeys(item, joinPath(path, index), issues)
		}
		return
	}

	if (!isJsonObject(value)) {
		return
	}

	for (const [key, nestedValue] of Object.entries(value)) {
		const normalizedKey = normalizeKey(key)
		if (forbiddenManagedSubagentKeys.has(normalizedKey)) {
			addError(
				issues,
				'HIDDEN_MANAGED_SUBAGENT_FIELD',
				joinPath(path, key),
				`Agent JSON must not contain hidden managed-subagent field "${key}". Use public orchestrator_agent nodes instead.`,
			)
		}

		if (forbiddenSecretOrLocalConfigKeys.has(normalizedKey)) {
			addError(
				issues,
				'LOCAL_PROVIDER_DATA_FORBIDDEN',
				joinPath(path, key),
				`Agent JSON must not contain local provider configuration or secret-like field "${key}".`,
			)
		}

		auditForbiddenObjectKeys(nestedValue, joinPath(path, key), issues)
	}
}

function auditMemoryProviderExtensions(
	agentFile: AgentFile,
	issues: BuilderCandidateAuditIssue[],
): void {
	for (let index = 0; index < (agentFile.memory_bindings ?? []).length; index += 1) {
		const binding = agentFile.memory_bindings?.[index]
		const providerExtension = binding?.config?.provider_extension
		if (!isJsonObject(providerExtension)) {
			continue
		}

		const provider = providerExtension.provider
		if (
			provider !== 'mem0' &&
			isJsonObject(providerExtension.config) &&
			Object.keys(providerExtension.config).length > 0
		) {
			addError(
				issues,
				'LOCAL_PROVIDER_DATA_FORBIDDEN',
				`/memory_bindings/${index}/config/provider_extension/config`,
				'Non-Mem0 memory provider_extension.config is not accepted from builder candidates because it can smuggle local provider configuration.',
			)
		}
	}
}

export function auditBuilderCandidate(args: {
	agent_file: AgentFile
	runtime_adapter: RuntimeAdapter
}): BuilderCandidateAuditDiagnostics {
	const capabilities = args.runtime_adapter.describeCapabilities()
	const issues: BuilderCandidateAuditIssue[] = []

	auditRuntimeCapabilities(args.agent_file, capabilities, issues)
	auditJsonOutputSchemas(args.agent_file, issues)
	auditForbiddenObjectKeys(args.agent_file as unknown as JsonValue, '', issues)
	auditMemoryProviderExtensions(args.agent_file, issues)

	return {
		status: issues.some((issue) => issue.severity === 'error') ? 'rejected' : 'accepted',
		issues,
		capabilities,
	}
}
