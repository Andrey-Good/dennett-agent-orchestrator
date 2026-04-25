import { AppError } from './errors.js'
import type { JsonObject } from './json.js'
import type {
	MemoryProviderCapability,
	MemoryProviderRecord,
	MemoryProviderStatus,
	MemoryProviderTransport,
	SQLiteLocalStateStore,
	UpsertMemoryProviderInput,
} from './state/index.js'

export const MEM0_PROVIDER_FAMILY = 'mem0'

export interface MemoryTransportPreferences {
	preferred?: MemoryProviderTransport[]
	forbid?: MemoryProviderTransport[]
}

export interface RegisterMemoryProviderInput {
	provider_id: string
	codex_ref?: string
	provider_family: string
	display_name?: string | null
	transport?: MemoryProviderTransport
	supported_capabilities?: MemoryProviderCapability[]
	config?: JsonObject
}

export interface UpdateMemoryProviderStatusArgs {
	provider_id: string
	status: MemoryProviderStatus
	status_code?: string | null
	status_message?: string | null
	last_checked_at?: string | null
}

export interface ResolveMemoryProviderArgs {
	codex_ref: string
	provider_family?: string
	required_capabilities?: MemoryProviderCapability[]
	transport_preferences?: MemoryTransportPreferences
}

export interface MemoryProviderRegistryServiceOptions {
	state_store: SQLiteLocalStateStore
	supported_families?: string[]
}

function uniqueItems<T>(items: readonly T[] | undefined): T[] {
	return [...new Set(items ?? [])]
}

function defaultTransportForFamily(providerFamily: string): MemoryProviderTransport {
	switch (providerFamily) {
		case MEM0_PROVIDER_FAMILY:
			return 'sdk'
		default:
			return 'api'
	}
}

function validateTransportPreferences(preferences?: MemoryTransportPreferences): void {
	if (!preferences) {
		return
	}

	const preferred = uniqueItems(preferences.preferred)
	const forbid = new Set(uniqueItems(preferences.forbid))
	for (const transport of preferred) {
		if (forbid.has(transport)) {
			throw new AppError(
				'INVALID_MEMORY_PROVIDER_TRANSPORT_PREFERENCES',
				`Transport "${transport}" cannot be both preferred and forbidden.`,
			)
		}
	}
}

export class MemoryProviderRegistryService {
	private readonly supportedFamilies: ReadonlySet<string>

	constructor(private readonly options: MemoryProviderRegistryServiceOptions) {
		this.supportedFamilies = new Set(options.supported_families ?? [MEM0_PROVIDER_FAMILY])
	}

	registerProvider(input: RegisterMemoryProviderInput): MemoryProviderRecord {
		this.assertFamilySupported(input.provider_family)
		return this.options.state_store.upsertMemoryProviderRecord({
			provider_id: input.provider_id,
			codex_ref: input.codex_ref ?? input.provider_id,
			provider_family: input.provider_family,
			display_name: input.display_name ?? null,
			transport: input.transport ?? defaultTransportForFamily(input.provider_family),
			status: 'configured',
			supported_capabilities: uniqueItems(input.supported_capabilities),
			config: input.config ?? {},
		})
	}

	updateProviderStatus(input: UpdateMemoryProviderStatusArgs): MemoryProviderRecord {
		return this.options.state_store.updateMemoryProviderStatus(input)
	}

	getProvider(providerId: string): MemoryProviderRecord | null {
		return this.options.state_store.getMemoryProviderRecord(providerId)
	}

	getProviderOrThrow(providerId: string): MemoryProviderRecord {
		const provider = this.getProvider(providerId)
		if (!provider) {
			throw new AppError(
				'MEMORY_PROVIDER_NOT_FOUND',
				`Memory provider "${providerId}" is not registered locally.`,
			)
		}
		return provider
	}

	listProviders(providerFamily?: string): MemoryProviderRecord[] {
		if (providerFamily) {
			this.assertFamilySupported(providerFamily)
		}
		return this.options.state_store.listMemoryProviderRecords(providerFamily)
	}

	resolveProvider(args: ResolveMemoryProviderArgs): MemoryProviderRecord {
		validateTransportPreferences(args.transport_preferences)

		const provider = this.options.state_store.getMemoryProviderRecordByCodexRef(args.codex_ref)
		if (!provider) {
			throw new AppError(
				'MEMORY_PROVIDER_NOT_FOUND',
				`Memory provider codex_ref "${args.codex_ref}" is not registered locally.`,
			)
		}

		if (provider.status === 'disabled') {
			throw new AppError(
				'MEMORY_PROVIDER_DISABLED',
				`Memory provider "${provider.provider_id}" is disabled.`,
			)
		}

		if (args.provider_family && provider.provider_family !== args.provider_family) {
			throw new AppError(
				'MEMORY_PROVIDER_FAMILY_MISMATCH',
				`Memory provider "${provider.provider_id}" is "${provider.provider_family}", not "${args.provider_family}".`,
			)
		}

		const preferred = uniqueItems(args.transport_preferences?.preferred)
		const forbidden = new Set(uniqueItems(args.transport_preferences?.forbid))

		if (forbidden.has(provider.transport)) {
			throw new AppError(
				'MEMORY_PROVIDER_TRANSPORT_FORBIDDEN',
				`Memory provider "${provider.provider_id}" uses forbidden transport "${provider.transport}".`,
			)
		}

		if (preferred.length > 0 && !preferred.includes(provider.transport)) {
			throw new AppError(
				'MEMORY_PROVIDER_TRANSPORT_MISMATCH',
				`Memory provider "${provider.provider_id}" uses transport "${provider.transport}", which is outside the preferred set.`,
			)
		}

		const missingCapabilities = uniqueItems(args.required_capabilities).filter(
			(capability) => !provider.supported_capabilities.includes(capability),
		)
		if (missingCapabilities.length > 0) {
			throw new AppError(
				'MEMORY_PROVIDER_CAPABILITY_MISSING',
				`Memory provider "${provider.provider_id}" is missing required capabilities: ${missingCapabilities.join(', ')}.`,
			)
		}

		return provider
	}

	private assertFamilySupported(providerFamily: string): void {
		if (!this.supportedFamilies.has(providerFamily)) {
			throw new AppError(
				'UNSUPPORTED_MEMORY_PROVIDER_FAMILY',
				`Memory provider family "${providerFamily}" is not supported in this product slice.`,
			)
		}
	}
}

export function buildMemoryProviderRecordForRegistration(
	input: RegisterMemoryProviderInput,
): UpsertMemoryProviderInput {
	return {
		provider_id: input.provider_id,
		codex_ref: input.codex_ref ?? input.provider_id,
		provider_family: input.provider_family,
		display_name: input.display_name ?? null,
		transport: input.transport ?? defaultTransportForFamily(input.provider_family),
		status: 'configured',
		supported_capabilities: uniqueItems(input.supported_capabilities),
		config: input.config ?? {},
	}
}
