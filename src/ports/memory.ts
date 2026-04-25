import type { JsonObject, JsonValue } from '../core/json.js'

export const MEMORY_CAPABILITIES = [
	'read',
	'write',
	'entity_scoped',
	'user_scoped',
	'group_scoped',
	'session_scoped',
	'graph_context',
	'temporal_index',
	'profile_synthesis',
	'rag_retrieval',
	'infer_extract',
	'versioned_write',
	'mcp_transport',
] as const

export type MemoryCapability = (typeof MEMORY_CAPABILITIES)[number]

export const MEMORY_TRANSPORTS = ['api', 'sdk', 'mcp'] as const

export type MemoryTransport = (typeof MEMORY_TRANSPORTS)[number]

export interface MemoryScope {
	user_id?: string
	agent_id?: string
	run_id?: string
}

export interface MemoryTransportPreferences {
	preferred?: MemoryTransport[]
	forbid?: MemoryTransport[]
}

export interface MemoryCapabilityRequirement {
	required_capabilities: MemoryCapability[]
	transport_preferences?: MemoryTransportPreferences
}

export interface MemoryProviderDescriptor {
	provider: string
	display_name: string
	supported_capabilities: MemoryCapability[]
	supported_transports: MemoryTransport[]
	default_transport: MemoryTransport
}

export interface MemoryNegotiationResult {
	ok: boolean
	selected_transport?: MemoryTransport
	missing_capabilities: MemoryCapability[]
	forbidden_transport_conflicts: MemoryTransport[]
	reason?: string
}

export interface MemoryRecord {
	id: string
	content: string
	scope: MemoryScope
	metadata?: JsonObject
	score?: number
	created_at?: string
	updated_at?: string
	provider_data?: JsonObject
}

export interface MemoryWriteRequest {
	content: string
	scope: MemoryScope
	metadata?: JsonObject
	infer?: boolean
}

export interface MemoryWriteResult {
	records: MemoryRecord[]
}

export interface MemoryReadRequest {
	memory_id: string
}

export interface MemorySearchRequest {
	query: string
	scope: MemoryScope
	limit?: number
	threshold?: number
}

export interface MemorySearchResult {
	records: MemoryRecord[]
}

export interface MemoryListRequest {
	scope: MemoryScope
	limit?: number
}

export interface MemoryUpdateRequest {
	memory_id: string
	content: string
	metadata?: JsonObject
}

export interface MemoryDeleteRequest {
	memory_id: string
}

export interface MemoryDeleteResult {
	deleted: boolean
}

export interface MemoryCleanupPreviewRequest {
	scope: MemoryScope
	limit?: number
}

export interface MemoryCleanupPreviewResult {
	namespace_id: string
	candidate_ids: string[]
	candidate_count: number
	limit: number
	truncated: boolean
}

export interface MemoryVerifiedCleanupRequest {
	scope: MemoryScope
	candidate_ids?: string[]
	limit?: number
}

export interface MemoryVerifiedCleanupResult {
	namespace_id: string
	limit: number
	requested_ids: string[]
	deleted_ids: string[]
	skipped_ids: string[]
	remaining_ids: string[]
	requested_truncated: boolean
	remaining_truncated: boolean
	verified_empty: boolean
}

export class MemoryCapabilityError extends Error {
	readonly name = 'MemoryCapabilityError'

	constructor(
		message: string,
		readonly details: {
			required_capabilities?: MemoryCapability[]
			missing_capabilities?: MemoryCapability[]
			transport_preferences?: MemoryTransportPreferences
			supported_transports?: MemoryTransport[]
		} = {},
	) {
		super(message)
	}
}

export class MemoryConfigurationError extends Error {
	readonly name = 'MemoryConfigurationError'

	constructor(
		message: string,
		readonly details: JsonObject = {},
	) {
		super(message)
	}
}

export class MemoryExecutionError extends Error {
	readonly name = 'MemoryExecutionError'

	constructor(
		message: string,
		readonly details: JsonObject = {},
	) {
		super(message)
	}
}

export interface MemoryAdapter {
	describeProvider(): MemoryProviderDescriptor
	negotiate(requirement: MemoryCapabilityRequirement): MemoryNegotiationResult
	writeMemory(request: MemoryWriteRequest): Promise<MemoryWriteResult>
	readMemory(request: MemoryReadRequest): Promise<MemoryRecord | null>
	searchMemory(request: MemorySearchRequest): Promise<MemorySearchResult>
	listMemories(request: MemoryListRequest): Promise<MemoryRecord[]>
	updateMemory(request: MemoryUpdateRequest): Promise<MemoryRecord | null>
	deleteMemory(request: MemoryDeleteRequest): Promise<MemoryDeleteResult>
	previewMemoryCleanup(request: MemoryCleanupPreviewRequest): Promise<MemoryCleanupPreviewResult>
	deleteMemoryCleanup(request: MemoryVerifiedCleanupRequest): Promise<MemoryVerifiedCleanupResult>
}

function dedupeCapabilities(capabilities: MemoryCapability[]): MemoryCapability[] {
	return [...new Set(capabilities)]
}

function dedupeTransports(transports: MemoryTransport[] | undefined): MemoryTransport[] {
	return transports ? [...new Set(transports)] : []
}

function pickTransport(
	supported: MemoryTransport[],
	defaultTransport: MemoryTransport,
	preferences?: MemoryTransportPreferences,
): {
	selected_transport?: MemoryTransport
	forbidden_transport_conflicts: MemoryTransport[]
	reason?: string
} {
	const forbidden = new Set(dedupeTransports(preferences?.forbid))
	const preferred = dedupeTransports(preferences?.preferred)
	const forbiddenTransportConflicts = supported.filter((transport) => forbidden.has(transport))
	const allowed = supported.filter((transport) => !forbidden.has(transport))

	if (allowed.length === 0) {
		return {
			forbidden_transport_conflicts: forbiddenTransportConflicts,
			reason: 'No supported transport remains after applying transport_preferences.forbid.',
		}
	}

	const preferredAllowed = preferred.filter((transport) => allowed.includes(transport))
	if (preferredAllowed.length > 0) {
		return {
			selected_transport: preferredAllowed[0],
			forbidden_transport_conflicts: forbiddenTransportConflicts,
		}
	}

	if (allowed.includes(defaultTransport)) {
		return {
			selected_transport: defaultTransport,
			forbidden_transport_conflicts: forbiddenTransportConflicts,
		}
	}

	return {
		selected_transport: allowed[0],
		forbidden_transport_conflicts: forbiddenTransportConflicts,
	}
}

export function negotiateMemoryProviderSupport(
	descriptor: MemoryProviderDescriptor,
	requirement: MemoryCapabilityRequirement,
): MemoryNegotiationResult {
	const requiredCapabilities = dedupeCapabilities(requirement.required_capabilities)
	const supportedCapabilities = new Set(descriptor.supported_capabilities)
	const missingCapabilities = requiredCapabilities.filter(
		(capability) => !supportedCapabilities.has(capability),
	)

	const transportSelection = pickTransport(
		descriptor.supported_transports,
		descriptor.default_transport,
		requirement.transport_preferences,
	)

	if (missingCapabilities.length > 0) {
		return {
			ok: false,
			missing_capabilities: missingCapabilities,
			forbidden_transport_conflicts: transportSelection.forbidden_transport_conflicts,
			selected_transport: transportSelection.selected_transport,
			reason: `Provider "${descriptor.provider}" does not support every required memory capability.`,
		}
	}

	if (!transportSelection.selected_transport) {
		return {
			ok: false,
			missing_capabilities: [],
			forbidden_transport_conflicts: transportSelection.forbidden_transport_conflicts,
			reason: transportSelection.reason,
		}
	}

	return {
		ok: true,
		selected_transport: transportSelection.selected_transport,
		missing_capabilities: [],
		forbidden_transport_conflicts: transportSelection.forbidden_transport_conflicts,
	}
}

export function assertMemoryNegotiation(
	descriptor: MemoryProviderDescriptor,
	requirement: MemoryCapabilityRequirement,
): MemoryTransport {
	const negotiation = negotiateMemoryProviderSupport(descriptor, requirement)
	if (!negotiation.ok || !negotiation.selected_transport) {
		throw new MemoryCapabilityError(
			negotiation.reason ??
				`Provider "${descriptor.provider}" cannot satisfy the requested memory capabilities.`,
			{
				required_capabilities: requirement.required_capabilities,
				missing_capabilities: negotiation.missing_capabilities,
				transport_preferences: requirement.transport_preferences,
				supported_transports: descriptor.supported_transports,
			},
		)
	}

	return negotiation.selected_transport
}

export function isMemoryRecord(value: JsonValue): value is JsonObject {
	return (
		value !== null &&
		typeof value === 'object' &&
		!Array.isArray(value) &&
		typeof value.id === 'string'
	)
}
