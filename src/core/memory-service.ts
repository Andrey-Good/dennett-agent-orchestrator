import { Mem0MemoryAdapter } from '../adapters/memory/mem0-memory-adapter.js'
import {
	MEMORY_CAPABILITIES,
	MEMORY_TRANSPORTS,
	type MemoryAdapter,
	type MemoryCapability,
	type MemoryCapabilityRequirement,
	type MemoryCleanupPreviewRequest,
	type MemoryCleanupPreviewResult,
	MemoryConfigurationError,
	type MemoryDeleteRequest,
	type MemoryDeleteResult,
	type MemoryListRequest,
	type MemoryNegotiationResult,
	type MemoryReadRequest,
	type MemoryRecord,
	type MemoryScope,
	type MemorySearchRequest,
	type MemorySearchResult,
	type MemoryTransport,
	type MemoryTransportPreferences,
	type MemoryUpdateRequest,
	type MemoryVerifiedCleanupRequest,
	type MemoryVerifiedCleanupResult,
	type MemoryWriteRequest,
	type MemoryWriteResult,
} from '../ports/memory.js'
import type {
	RuntimeMemoryBindingContext,
	RuntimeMemoryOperationScope,
	RuntimeMemoryRecord,
	RuntimeMemoryWriteContext,
} from '../ports/runtime.js'
import type { MemoryBinding, OutputMode } from './agent-file.js'
import { AppError } from './errors.js'
import type { JsonObject, JsonValue } from './json.js'
import {
	MEM0_PROVIDER_FAMILY,
	MemoryProviderRegistryService,
	type MemoryTransportPreferences as RegistryTransportPreferences,
} from './memory-provider-registry.js'
import type {
	MemoryProviderCapability,
	MemoryProviderRecord,
	SQLiteLocalStateStore,
} from './state/index.js'

interface MemoryBindingPortableConfig {
	intent: {
		summary: string
		labels?: string[]
	}
	required_capabilities: MemoryCapability[]
	transport_preferences?: MemoryTransportPreferences
	provider_extension?: {
		provider: string
		transport?: MemoryTransport
		config?: JsonObject
	}
	local_notes?: string
}

interface Mem0ProviderExtensionConfig {
	mem0_config?: {
		graph_store?: {
			provider?: string
			config?: JsonObject
		}
	}
}

export interface ResolveMemoryAdapterForBindingResult {
	binding: MemoryBinding
	provider: MemoryProviderRecord
	adapter: MemoryAdapter
	requirement: MemoryBindingPortableConfig
}

export interface ResolveMemoryAdapterForCodexRefOptions {
	provider_family?: string
	required_capabilities?: MemoryProviderCapability[]
	transport_preferences?: MemoryTransportPreferences
}

export interface PrepareRuntimeMemoryBindingContextOptions {
	binding: MemoryBinding
	scope: RuntimeMemoryOperationScope
	read?: {
		query: string
		limit?: number
		threshold?: number
	}
}

export interface PrepareRuntimeMemoryBindingContextResult {
	context: RuntimeMemoryBindingContext
	provider: MemoryProviderRecord
	read_enabled: boolean
	write_enabled: boolean
	required_capabilities: MemoryCapability[]
}

export interface PlanRuntimeMemorySuccessWriteOptions {
	binding: MemoryBinding
	scope: RuntimeMemoryOperationScope
	node_id: string
	content: string
	outcome: string
	attempt_id: string
	output_mode: OutputMode
	output_hash: string
	metadata?: JsonObject
	infer?: boolean
}

export type RuntimeMemorySuccessWritePlan =
	| {
			should_write: true
			dennett_write_key: string
			metadata: JsonObject
			request: MemoryWriteRequest
	  }
	| {
			should_write: false
			disabled_reason: string
	  }

export type RuntimeMemorySuccessWriteResult =
	| {
			status: 'written'
			dennett_write_key: string
			metadata: JsonObject
			result: MemoryWriteResult
	  }
	| {
			status: 'skipped'
			disabled_reason: string
	  }

export interface MemoryServiceOptions {
	state_store: SQLiteLocalStateStore
}

function isJsonObject(value: JsonValue | unknown): value is JsonObject {
	return value !== null && typeof value === 'object' && !Array.isArray(value)
}

function toRegistryTransportPreferences(
	preferences?: MemoryTransportPreferences,
): RegistryTransportPreferences | undefined {
	if (!preferences) {
		return undefined
	}

	return {
		...(preferences.preferred ? { preferred: [...preferences.preferred] } : {}),
		...(preferences.forbid ? { forbid: [...preferences.forbid] } : {}),
	}
}

function parseCapabilityToken(rawValue: string): MemoryCapability {
	if ((MEMORY_CAPABILITIES as readonly string[]).includes(rawValue)) {
		return rawValue as MemoryCapability
	}

	throw new AppError(
		'INVALID_MEMORY_BINDING_CONFIG',
		`Unknown memory capability token "${rawValue}".`,
	)
}

function parseTransportToken(rawValue: string): MemoryTransport {
	if ((MEMORY_TRANSPORTS as readonly string[]).includes(rawValue)) {
		return rawValue as MemoryTransport
	}

	throw new AppError(
		'INVALID_MEMORY_BINDING_CONFIG',
		`Unknown memory transport token "${rawValue}".`,
	)
}

function parseTransportList(
	rawValue: JsonValue | undefined,
	fieldName: string,
): MemoryTransport[] | undefined {
	if (rawValue === undefined) {
		return undefined
	}
	if (!Array.isArray(rawValue)) {
		throw new AppError(
			'INVALID_MEMORY_BINDING_CONFIG',
			`memory binding "${fieldName}" must be an array of transport tokens.`,
		)
	}

	return [...new Set(rawValue.map((item) => parseTransportToken(String(item))))]
}

function parseStringList(rawValue: JsonValue | undefined, fieldName: string): string[] | undefined {
	if (rawValue === undefined) {
		return undefined
	}
	if (!Array.isArray(rawValue)) {
		throw new AppError(
			'INVALID_MEMORY_BINDING_CONFIG',
			`memory binding "${fieldName}" must be an array of strings.`,
		)
	}

	return [...new Set(rawValue.map((item) => String(item)))]
}

function parsePortableMemoryConfig(binding: MemoryBinding): MemoryBindingPortableConfig {
	const config = binding.config
	if (config === undefined) {
		throw new AppError(
			'INVALID_MEMORY_BINDING_CONFIG',
			`Memory binding "${binding.id}" must define a config object for runtime_memory bindings.`,
		)
	}
	if (!isJsonObject(config)) {
		throw new AppError(
			'INVALID_MEMORY_BINDING_CONFIG',
			`Memory binding "${binding.id}" config must be a JSON object.`,
		)
	}

	if (!isJsonObject(config.intent)) {
		throw new AppError(
			'INVALID_MEMORY_BINDING_CONFIG',
			`Memory binding "${binding.id}" intent must be a JSON object.`,
		)
	}
	if (typeof config.intent.summary !== 'string' || config.intent.summary.trim().length === 0) {
		throw new AppError(
			'INVALID_MEMORY_BINDING_CONFIG',
			`Memory binding "${binding.id}" intent.summary must be a non-empty string.`,
		)
	}
	const intent = {
		summary: config.intent.summary.trim(),
		...(config.intent.labels !== undefined
			? { labels: parseStringList(config.intent.labels, 'intent.labels') }
			: {}),
	}

	const rawCapabilities = config.required_capabilities
	if (!Array.isArray(rawCapabilities)) {
		throw new AppError(
			'INVALID_MEMORY_BINDING_CONFIG',
			`Memory binding "${binding.id}" required_capabilities must be an array.`,
		)
	}
	const requiredCapabilities = [
		...new Set(rawCapabilities.map((value) => parseCapabilityToken(String(value)))),
	]

	let transportPreferences: MemoryTransportPreferences | undefined
	if (config.transport_preferences !== undefined) {
		if (!isJsonObject(config.transport_preferences)) {
			throw new AppError(
				'INVALID_MEMORY_BINDING_CONFIG',
				`Memory binding "${binding.id}" transport_preferences must be a JSON object.`,
			)
		}
		transportPreferences = {
			...(config.transport_preferences.preferred !== undefined
				? {
						preferred: parseTransportList(
							config.transport_preferences.preferred,
							'transport_preferences.preferred',
						),
					}
				: {}),
			...(config.transport_preferences.forbid !== undefined
				? {
						forbid: parseTransportList(
							config.transport_preferences.forbid,
							'transport_preferences.forbid',
						),
					}
				: {}),
		}
	}

	let providerExtension: MemoryBindingPortableConfig['provider_extension']
	if (config.provider_extension !== undefined) {
		if (!isJsonObject(config.provider_extension)) {
			throw new AppError(
				'INVALID_MEMORY_BINDING_CONFIG',
				`Memory binding "${binding.id}" provider_extension must be a JSON object.`,
			)
		}
		if (typeof config.provider_extension.provider !== 'string') {
			throw new AppError(
				'INVALID_MEMORY_BINDING_CONFIG',
				`Memory binding "${binding.id}" provider_extension.provider must be a string.`,
			)
		}
		providerExtension = {
			provider: config.provider_extension.provider,
			...(config.provider_extension.transport !== undefined
				? {
						transport: parseTransportToken(String(config.provider_extension.transport)),
					}
				: {}),
		}
		if (config.provider_extension.config !== undefined) {
			if (!isJsonObject(config.provider_extension.config)) {
				throw new AppError(
					'INVALID_MEMORY_BINDING_CONFIG',
					`Memory binding "${binding.id}" provider_extension.config must be a JSON object.`,
				)
			}
			providerExtension.config = config.provider_extension.config
		}
	}

	if (providerExtension?.transport) {
		const existingPreferred = transportPreferences?.preferred ?? []
		transportPreferences = {
			...(transportPreferences ?? {}),
			preferred: [...new Set([providerExtension.transport, ...existingPreferred])],
		}
	}

	return {
		intent,
		required_capabilities: requiredCapabilities,
		...(transportPreferences ? { transport_preferences: transportPreferences } : {}),
		...(providerExtension ? { provider_extension: providerExtension } : {}),
		...(typeof config.local_notes === 'string' && config.local_notes.length > 0
			? { local_notes: config.local_notes }
			: {}),
	}
}

function assertValidProviderConfig(
	provider: MemoryProviderRecord,
	requiredKey: string,
	value: JsonValue | undefined,
): string {
	if (typeof value === 'string' && value.trim().length > 0) {
		return value
	}

	throw new AppError(
		'INVALID_MEMORY_PROVIDER_CONFIG',
		`Memory provider "${provider.provider_id}" must define "${requiredKey}" in local config.`,
	)
}

function assertObjectProviderConfig(
	provider: MemoryProviderRecord,
	requiredKey: string,
	value: JsonValue | undefined,
): JsonObject {
	if (isJsonObject(value)) {
		return value
	}

	throw new AppError(
		'INVALID_MEMORY_PROVIDER_CONFIG',
		`Memory provider "${provider.provider_id}" must define "${requiredKey}" as a JSON object in local config.`,
	)
}

function normalizeScope(scope: MemoryScope): MemoryScope {
	const normalized: MemoryScope = {}
	if (typeof scope.user_id === 'string' && scope.user_id.trim().length > 0) {
		normalized.user_id = scope.user_id.trim()
	}
	if (typeof scope.agent_id === 'string' && scope.agent_id.trim().length > 0) {
		normalized.agent_id = scope.agent_id.trim()
	}
	if (typeof scope.run_id === 'string' && scope.run_id.trim().length > 0) {
		normalized.run_id = scope.run_id.trim()
	}
	return normalized
}

function normalizeRuntimeMemoryScope(scope: RuntimeMemoryOperationScope): MemoryScope {
	return normalizeScope({
		agent_id: scope.agent_id,
		run_id: scope.run_id,
		...(scope.user_id ? { user_id: scope.user_id } : {}),
	})
}

function toRuntimeMemoryRecord(record: MemoryRecord): RuntimeMemoryRecord {
	return {
		id: record.id,
		content: record.content,
		scope: normalizeScope(record.scope),
		...(record.metadata ? { metadata: record.metadata } : {}),
		...(typeof record.score === 'number' ? { score: record.score } : {}),
		...(record.created_at ? { created_at: record.created_at } : {}),
		...(record.updated_at ? { updated_at: record.updated_at } : {}),
		...(record.provider_data ? { provider_data: record.provider_data } : {}),
	}
}

function buildWriteDisabledContext(reason: string): RuntimeMemoryWriteContext {
	return {
		enabled: false,
		disabled_reason: reason,
	}
}

function buildDennettWriteKey(options: {
	binding_id: string
	run_id: string
	node_id: string
	output_hash: string
}): string {
	const parts = [options.run_id, options.node_id, options.binding_id, options.output_hash].map(
		(part) => encodeURIComponent(part),
	)
	return `dennett:${parts.join(':')}`
}

function buildRuntimeWriteMetadata(
	options: PlanRuntimeMemorySuccessWriteOptions,
	dennettWriteKey: string,
): JsonObject {
	const systemMetadata: JsonObject = {
		dennett_kind: 'runtime_node_output',
		agent_id: options.scope.agent_id,
		run_id: options.scope.run_id,
		node_id: options.node_id,
		binding_id: options.binding.id,
		attempt_id: options.attempt_id,
		output_mode: options.output_mode,
		output_hash: options.output_hash,
		dennett_write_key: dennettWriteKey,
		dennett_write_mode: 'node_success_output',
		dennett_binding_id: options.binding.id,
		dennett_codex_ref: options.binding.codex_ref,
		dennett_agent_id: options.scope.agent_id,
		dennett_run_id: options.scope.run_id,
		dennett_node_id: options.node_id,
		dennett_attempt_id: options.attempt_id,
	}

	return {
		...(options.metadata ?? {}),
		...systemMetadata,
	}
}

function deepMergeJsonObjects(base: JsonObject, override: JsonObject): JsonObject {
	const merged: JsonObject = { ...base }
	for (const [key, overrideValue] of Object.entries(override)) {
		const baseValue = merged[key]
		if (isJsonObject(baseValue) && isJsonObject(overrideValue)) {
			merged[key] = deepMergeJsonObjects(baseValue, overrideValue)
			continue
		}
		merged[key] = overrideValue
	}
	return merged
}

function sanitizeMem0ProviderOverrideConfig(
	bindingId: string,
	overrideConfig: JsonObject,
): Mem0ProviderExtensionConfig {
	const allowedKeys = new Set(['mem0_config'])
	for (const key of Object.keys(overrideConfig)) {
		if (!allowedKeys.has(key)) {
			throw new AppError(
				'INVALID_MEMORY_BINDING_CONFIG',
				`Memory binding "${bindingId}" provider_extension.config may only override "mem0_config" for provider "mem0". Local provider registration fields such as "${key}" are not portable override inputs.`,
			)
		}
	}

	if (overrideConfig.mem0_config !== undefined && !isJsonObject(overrideConfig.mem0_config)) {
		throw new AppError(
			'INVALID_MEMORY_BINDING_CONFIG',
			`Memory binding "${bindingId}" provider_extension.config.mem0_config must be a JSON object.`,
		)
	}

	const mem0Config = overrideConfig.mem0_config
	if (isJsonObject(mem0Config)) {
		const allowedMem0ConfigKeys = new Set(['graph_store'])
		for (const key of Object.keys(mem0Config)) {
			if (!allowedMem0ConfigKeys.has(key)) {
				throw new AppError(
					'INVALID_MEMORY_BINDING_CONFIG',
					`Memory binding "${bindingId}" provider_extension.config.mem0_config may only override "graph_store" in the current Mem0 slice. Sensitive or local-only fields such as "${key}" remain owned by local provider registration.`,
				)
			}
		}

		if (mem0Config.graph_store !== undefined && !isJsonObject(mem0Config.graph_store)) {
			throw new AppError(
				'INVALID_MEMORY_BINDING_CONFIG',
				`Memory binding "${bindingId}" provider_extension.config.mem0_config.graph_store must be a JSON object.`,
			)
		}

		const graphStore = mem0Config.graph_store
		if (isJsonObject(graphStore)) {
			const allowedGraphStoreKeys = new Set(['provider', 'config'])
			for (const key of Object.keys(graphStore)) {
				if (!allowedGraphStoreKeys.has(key)) {
					throw new AppError(
						'INVALID_MEMORY_BINDING_CONFIG',
						`Memory binding "${bindingId}" provider_extension.config.mem0_config.graph_store may only include "provider" and optional empty "config" in the current Mem0 slice. Field "${key}" is not portable.`,
					)
				}
			}

			if (typeof graphStore.provider !== 'string' || graphStore.provider.trim().length === 0) {
				throw new AppError(
					'INVALID_MEMORY_BINDING_CONFIG',
					`Memory binding "${bindingId}" provider_extension.config.mem0_config.graph_store.provider must be a non-empty string when graph_store is present.`,
				)
			}

			if (graphStore.config !== undefined) {
				if (!isJsonObject(graphStore.config)) {
					throw new AppError(
						'INVALID_MEMORY_BINDING_CONFIG',
						`Memory binding "${bindingId}" provider_extension.config.mem0_config.graph_store.config must be a JSON object.`,
					)
				}
				if (Object.keys(graphStore.config).length > 0) {
					const nestedKeys = Object.keys(graphStore.config).join(', ')
					throw new AppError(
						'INVALID_MEMORY_BINDING_CONFIG',
						`Memory binding "${bindingId}" provider_extension.config.mem0_config.graph_store.config must stay empty in the current Mem0 slice. Nested override keys such as "${nestedKeys}" are local-only.`,
					)
				}
			}
		}
	}

	const graphStore =
		isJsonObject(mem0Config) && isJsonObject(mem0Config.graph_store)
			? mem0Config.graph_store
			: undefined
	const graphStoreProvider =
		graphStore && typeof graphStore.provider === 'string' ? graphStore.provider.trim() : undefined

	return {
		...(isJsonObject(mem0Config)
			? {
					mem0_config: {
						...(graphStore && graphStoreProvider
							? {
									graph_store: {
										provider: graphStoreProvider,
										...(isJsonObject(graphStore.config) ? { config: graphStore.config } : {}),
									},
								}
							: {}),
					},
				}
			: {}),
	}
}

export class MemoryService {
	private readonly registryService: MemoryProviderRegistryService

	constructor(options: MemoryServiceOptions) {
		this.registryService = new MemoryProviderRegistryService({
			state_store: options.state_store,
		})
	}

	resolveAdapterForBinding(binding: MemoryBinding): ResolveMemoryAdapterForBindingResult {
		const requirement = parsePortableMemoryConfig(binding)
		const provider = this.registryService.resolveProvider({
			codex_ref: binding.codex_ref,
			provider_family: requirement.provider_extension?.provider,
			transport_preferences: toRegistryTransportPreferences(requirement.transport_preferences),
		})
		try {
			const adapter = this.instantiateProviderAdapter(
				provider,
				binding.id,
				requirement.provider_extension?.config,
			)
			this.assertProviderAdapterCompatibility(provider, adapter, requirement)
			return {
				binding,
				provider,
				adapter,
				requirement,
			}
		} catch (error) {
			if (this.isProviderConfigurationError(error)) {
				this.markProviderError(provider, error)
			}
			throw error
		}
	}

	resolveAdapterForCodexRef(
		codexRef: string,
		options?: ResolveMemoryAdapterForCodexRefOptions,
	): { provider: MemoryProviderRecord; adapter: MemoryAdapter } {
		const provider = this.registryService.resolveProvider({
			codex_ref: codexRef,
			provider_family: options?.provider_family,
			transport_preferences: toRegistryTransportPreferences(options?.transport_preferences),
		})
		try {
			const adapter = this.instantiateProviderAdapter(provider)
			this.assertProviderAdapterCompatibility(provider, adapter, {
				required_capabilities: (options?.required_capabilities ?? []) as MemoryCapability[],
				...(options?.transport_preferences
					? { transport_preferences: options.transport_preferences }
					: {}),
			})
			return {
				provider,
				adapter,
			}
		} catch (error) {
			if (this.isProviderConfigurationError(error)) {
				this.markProviderError(provider, error)
			}
			throw error
		}
	}

	async writeForBinding(
		binding: MemoryBinding,
		request: MemoryWriteRequest,
	): Promise<MemoryWriteResult> {
		const resolved = this.resolveAdapterForBinding(binding)
		return this.runWithProviderStatus(resolved.provider, () =>
			resolved.adapter.writeMemory({
				...request,
				scope: normalizeScope(request.scope),
			}),
		)
	}

	async readForBinding(
		binding: MemoryBinding,
		request: MemoryReadRequest,
	): Promise<MemoryRecord | null> {
		const resolved = this.resolveAdapterForBinding(binding)
		return this.runWithProviderStatus(resolved.provider, () => resolved.adapter.readMemory(request))
	}

	async searchForBinding(
		binding: MemoryBinding,
		request: MemorySearchRequest,
	): Promise<MemorySearchResult> {
		const resolved = this.resolveAdapterForBinding(binding)
		return this.runWithProviderStatus(resolved.provider, () =>
			resolved.adapter.searchMemory({
				...request,
				scope: normalizeScope(request.scope),
			}),
		)
	}

	async prepareRuntimeMemoryBindingContext(
		options: PrepareRuntimeMemoryBindingContextOptions,
	): Promise<PrepareRuntimeMemoryBindingContextResult> {
		const resolved = this.resolveAdapterForBinding(options.binding)
		const readEnabled = resolved.requirement.required_capabilities.includes('read')
		const writeEnabled = resolved.requirement.required_capabilities.includes('write')
		const context: RuntimeMemoryBindingContext = {
			binding_id: options.binding.id,
			codex_ref: options.binding.codex_ref,
			intent: resolved.requirement.intent,
			required_capabilities: [...resolved.requirement.required_capabilities],
			scope: options.scope,
			write: writeEnabled
				? {
						enabled: true,
						mode: 'node_success_output',
					}
				: buildWriteDisabledContext('Memory binding does not declare the "write" capability.'),
		}

		if (options.read && readEnabled) {
			const searchResult = await this.runWithProviderStatus(resolved.provider, () =>
				resolved.adapter.searchMemory({
					query: options.read?.query ?? '',
					scope: normalizeRuntimeMemoryScope(options.scope),
					...(options.read?.limit !== undefined ? { limit: options.read.limit } : {}),
					...(options.read?.threshold !== undefined ? { threshold: options.read.threshold } : {}),
				}),
			)
			context.read = {
				query: options.read.query,
				records: searchResult.records.map((record) => toRuntimeMemoryRecord(record)),
			}
		}

		return {
			context,
			provider: resolved.provider,
			read_enabled: readEnabled,
			write_enabled: writeEnabled,
			required_capabilities: [...resolved.requirement.required_capabilities],
		}
	}

	planRuntimeMemorySuccessWrite(
		options: PlanRuntimeMemorySuccessWriteOptions,
	): RuntimeMemorySuccessWritePlan {
		if (options.outcome !== 'success') {
			return {
				should_write: false,
				disabled_reason: 'Runtime memory writes only run for successful node outcomes.',
			}
		}

		const requirement = parsePortableMemoryConfig(options.binding)
		if (!requirement.required_capabilities.includes('write')) {
			return {
				should_write: false,
				disabled_reason: 'Memory binding does not declare the "write" capability.',
			}
		}

		const dennettWriteKey = buildDennettWriteKey({
			binding_id: options.binding.id,
			run_id: options.scope.run_id,
			node_id: options.node_id,
			output_hash: options.output_hash,
		})
		const metadata = buildRuntimeWriteMetadata(options, dennettWriteKey)

		return {
			should_write: true,
			dennett_write_key: dennettWriteKey,
			metadata,
			request: {
				content: options.content,
				scope: normalizeRuntimeMemoryScope(options.scope),
				metadata,
				...(options.infer !== undefined ? { infer: options.infer } : {}),
			},
		}
	}

	async writeRuntimeMemoryOnSuccess(
		options: PlanRuntimeMemorySuccessWriteOptions,
	): Promise<RuntimeMemorySuccessWriteResult> {
		const plan = this.planRuntimeMemorySuccessWrite(options)
		if (!plan.should_write) {
			return {
				status: 'skipped',
				disabled_reason: plan.disabled_reason,
			}
		}

		const result = await this.writeForBinding(options.binding, plan.request)
		return {
			status: 'written',
			dennett_write_key: plan.dennett_write_key,
			metadata: plan.metadata,
			result,
		}
	}

	async writeForCodexRef(
		codexRef: string,
		request: MemoryWriteRequest,
		options?: ResolveMemoryAdapterForCodexRefOptions,
	): Promise<MemoryWriteResult> {
		const resolved = this.resolveAdapterForCodexRef(codexRef, options)
		return this.runWithProviderStatus(resolved.provider, () =>
			resolved.adapter.writeMemory({
				...request,
				scope: normalizeScope(request.scope),
			}),
		)
	}

	async readForCodexRef(
		codexRef: string,
		request: MemoryReadRequest,
		options?: ResolveMemoryAdapterForCodexRefOptions,
	): Promise<MemoryRecord | null> {
		const resolved = this.resolveAdapterForCodexRef(codexRef, options)
		return this.runWithProviderStatus(resolved.provider, () => resolved.adapter.readMemory(request))
	}

	async searchForCodexRef(
		codexRef: string,
		request: MemorySearchRequest,
		options?: ResolveMemoryAdapterForCodexRefOptions,
	): Promise<MemorySearchResult> {
		const resolved = this.resolveAdapterForCodexRef(codexRef, options)
		return this.runWithProviderStatus(resolved.provider, () =>
			resolved.adapter.searchMemory({
				...request,
				scope: normalizeScope(request.scope),
			}),
		)
	}

	async listForCodexRef(
		codexRef: string,
		request: MemoryListRequest,
		options?: ResolveMemoryAdapterForCodexRefOptions,
	): Promise<MemoryRecord[]> {
		const resolved = this.resolveAdapterForCodexRef(codexRef, options)
		return this.runWithProviderStatus(resolved.provider, () =>
			resolved.adapter.listMemories({
				...request,
				scope: normalizeScope(request.scope),
			}),
		)
	}

	async updateForCodexRef(
		codexRef: string,
		request: MemoryUpdateRequest,
		options?: ResolveMemoryAdapterForCodexRefOptions,
	): Promise<MemoryRecord | null> {
		const resolved = this.resolveAdapterForCodexRef(codexRef, options)
		return this.runWithProviderStatus(resolved.provider, () =>
			resolved.adapter.updateMemory(request),
		)
	}

	async deleteForCodexRef(
		codexRef: string,
		request: MemoryDeleteRequest,
		options?: ResolveMemoryAdapterForCodexRefOptions,
	): Promise<MemoryDeleteResult> {
		const resolved = this.resolveAdapterForCodexRef(codexRef, options)
		return this.runWithProviderStatus(resolved.provider, () =>
			resolved.adapter.deleteMemory(request),
		)
	}

	async previewMemoryCleanupForCodexRef(
		codexRef: string,
		request: MemoryCleanupPreviewRequest,
		options?: ResolveMemoryAdapterForCodexRefOptions,
	): Promise<MemoryCleanupPreviewResult> {
		const resolved = this.resolveAdapterForCodexRef(codexRef, options)
		return this.runWithProviderStatus(resolved.provider, () => {
			this.assertCleanupMethodSupported(resolved.provider, resolved.adapter, 'previewMemoryCleanup')
			return resolved.adapter.previewMemoryCleanup({
				...request,
				scope: normalizeScope(request.scope),
			})
		})
	}

	async deleteMemoryCleanupForCodexRef(
		codexRef: string,
		request: MemoryVerifiedCleanupRequest,
		options?: ResolveMemoryAdapterForCodexRefOptions,
	): Promise<MemoryVerifiedCleanupResult> {
		const resolved = this.resolveAdapterForCodexRef(codexRef, options)
		return this.runWithProviderStatus(resolved.provider, () => {
			this.assertCleanupMethodSupported(resolved.provider, resolved.adapter, 'deleteMemoryCleanup')
			return resolved.adapter.deleteMemoryCleanup({
				...request,
				scope: normalizeScope(request.scope),
			})
		})
	}

	private instantiateProviderAdapter(
		provider: MemoryProviderRecord,
		bindingId?: string,
		providerConfigOverride?: JsonObject,
	): MemoryAdapter {
		switch (provider.provider_family) {
			case MEM0_PROVIDER_FAMILY: {
				const mem0Override =
					providerConfigOverride && bindingId
						? sanitizeMem0ProviderOverrideConfig(bindingId, providerConfigOverride)
						: undefined
				const effectiveMem0Config =
					mem0Override?.mem0_config?.graph_store !== undefined
						? deepMergeJsonObjects(
								assertObjectProviderConfig(provider, 'mem0_config', provider.config.mem0_config),
								{
									graph_store: mem0Override.mem0_config.graph_store,
								},
							)
						: assertObjectProviderConfig(provider, 'mem0_config', provider.config.mem0_config)
				return new Mem0MemoryAdapter({
					python_executable: assertValidProviderConfig(
						provider,
						'python_executable',
						provider.config.python_executable,
					),
					mem0_config: effectiveMem0Config,
					...(typeof provider.config.working_directory === 'string' &&
					provider.config.working_directory.trim().length > 0
						? { working_directory: provider.config.working_directory }
						: {}),
					...(typeof provider.config.bridge_timeout_ms === 'number' &&
					Number.isFinite(provider.config.bridge_timeout_ms) &&
					provider.config.bridge_timeout_ms > 0
						? { bridge_timeout_ms: provider.config.bridge_timeout_ms }
						: {}),
				})
			}
			default:
				throw new AppError(
					'UNSUPPORTED_MEMORY_PROVIDER_FAMILY',
					`Memory provider family "${provider.provider_family}" is not implemented in the current product slice.`,
				)
		}
	}

	private assertCleanupMethodSupported(
		provider: MemoryProviderRecord,
		adapter: MemoryAdapter,
		methodName: 'previewMemoryCleanup' | 'deleteMemoryCleanup',
	): void {
		if (typeof adapter[methodName] === 'function') {
			return
		}

		throw new AppError(
			'MEMORY_PROVIDER_CLEANUP_UNSUPPORTED',
			`Memory provider "${provider.provider_id}" does not expose adapter cleanup method "${methodName}".`,
		)
	}

	private assertProviderAdapterCompatibility(
		provider: MemoryProviderRecord,
		adapter: MemoryAdapter,
		requirement: MemoryCapabilityRequirement,
	): void {
		const descriptor = adapter.describeProvider()

		if (descriptor.provider !== provider.provider_family) {
			throw new AppError(
				'MEMORY_PROVIDER_FAMILY_MISMATCH',
				`Memory provider "${provider.provider_id}" is registered as "${provider.provider_family}", but the instantiated adapter reports "${descriptor.provider}".`,
			)
		}

		if (!descriptor.supported_transports.includes(provider.transport)) {
			throw new AppError(
				'MEMORY_PROVIDER_TRANSPORT_MISMATCH',
				`Memory provider "${provider.provider_id}" is registered with transport "${provider.transport}", but adapter "${descriptor.provider}" only supports: ${descriptor.supported_transports.join(', ')}.`,
			)
		}

		const adapterRequirement = this.withRegisteredTransportPreference(
			requirement,
			provider.transport,
		)
		const negotiation = adapter.negotiate(adapterRequirement)
		this.assertNegotiationSucceeded(provider, descriptor.provider, negotiation)
	}

	private withRegisteredTransportPreference(
		requirement: MemoryCapabilityRequirement,
		registeredTransport: MemoryTransport,
	): MemoryCapabilityRequirement {
		const existingPreferred = requirement.transport_preferences?.preferred ?? []
		const preferred = [
			registeredTransport,
			...existingPreferred.filter((transport) => transport !== registeredTransport),
		]

		return {
			required_capabilities: [...requirement.required_capabilities],
			transport_preferences: {
				...(requirement.transport_preferences?.forbid
					? { forbid: [...requirement.transport_preferences.forbid] }
					: {}),
				preferred,
			},
		}
	}

	private assertNegotiationSucceeded(
		provider: MemoryProviderRecord,
		adapterProvider: string,
		negotiation: MemoryNegotiationResult,
	): void {
		if (negotiation.ok && negotiation.selected_transport === provider.transport) {
			return
		}

		const details: string[] = []
		if (negotiation.missing_capabilities.length > 0) {
			details.push(`missing capabilities: ${negotiation.missing_capabilities.join(', ')}`)
		}
		if (negotiation.forbidden_transport_conflicts.length > 0) {
			details.push(
				`forbidden transport conflicts: ${negotiation.forbidden_transport_conflicts.join(', ')}`,
			)
		}
		if (negotiation.selected_transport && negotiation.selected_transport !== provider.transport) {
			details.push(
				`selected transport "${negotiation.selected_transport}" does not match registered transport "${provider.transport}"`,
			)
		}
		if (negotiation.reason) {
			details.push(negotiation.reason)
		}

		throw new AppError(
			'MEMORY_PROVIDER_ADAPTER_NEGOTIATION_FAILED',
			`Memory provider "${provider.provider_id}" could not be negotiated against adapter "${adapterProvider}".${details.length > 0 ? ` ${details.join('; ')}.` : ''}`,
		)
	}

	private async runWithProviderStatus<T>(
		provider: MemoryProviderRecord,
		action: () => Promise<T>,
	): Promise<T> {
		try {
			const result = await action()
			this.registryService.updateProviderStatus({
				provider_id: provider.provider_id,
				status: 'available',
				status_code: null,
				status_message: null,
				last_checked_at: new Date().toISOString(),
			})
			return result
		} catch (error) {
			this.markProviderError(provider, error)
			throw error
		}
	}

	private isProviderConfigurationError(error: unknown): boolean {
		return (
			error instanceof MemoryConfigurationError ||
			(error instanceof AppError && error.code === 'INVALID_MEMORY_PROVIDER_CONFIG')
		)
	}

	private markProviderError(provider: MemoryProviderRecord, error: unknown): void {
		this.registryService.updateProviderStatus({
			provider_id: provider.provider_id,
			status: 'error',
			status_code: error instanceof AppError ? error.code : 'MEMORY_PROVIDER_OPERATION_FAILED',
			status_message: error instanceof Error ? error.message : 'Unknown memory provider failure.',
			last_checked_at: new Date().toISOString(),
		})
	}
}
