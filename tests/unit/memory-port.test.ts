import { describe, expect, it } from 'vitest'
import {
	assertMemoryNegotiation,
	MemoryCapabilityError,
	type MemoryProviderDescriptor,
	negotiateMemoryProviderSupport,
} from '../../src/ports/memory.js'

const BASE_DESCRIPTOR: MemoryProviderDescriptor = {
	provider: 'mem0',
	display_name: 'Mem0',
	supported_capabilities: ['read', 'write', 'user_scoped', 'entity_scoped', 'infer_extract'],
	supported_transports: ['sdk'],
	default_transport: 'sdk',
}

describe('memory port negotiation', () => {
	it('accepts supported capabilities and selects the default transport', () => {
		const result = negotiateMemoryProviderSupport(BASE_DESCRIPTOR, {
			required_capabilities: ['read', 'write'],
		})

		expect(result).toEqual({
			ok: true,
			selected_transport: 'sdk',
			missing_capabilities: [],
			forbidden_transport_conflicts: [],
		})
	})

	it('returns an explicit failure when a required capability is missing', () => {
		const result = negotiateMemoryProviderSupport(BASE_DESCRIPTOR, {
			required_capabilities: ['read', 'graph_context'],
		})

		expect(result.ok).toBe(false)
		expect(result.missing_capabilities).toEqual(['graph_context'])
		expect(result.reason).toContain('does not support every required memory capability')
	})

	it('returns an explicit failure when all supported transports are forbidden', () => {
		const result = negotiateMemoryProviderSupport(BASE_DESCRIPTOR, {
			required_capabilities: ['read'],
			transport_preferences: {
				forbid: ['sdk'],
			},
		})

		expect(result).toEqual({
			ok: false,
			missing_capabilities: [],
			forbidden_transport_conflicts: ['sdk'],
			reason: 'No supported transport remains after applying transport_preferences.forbid.',
		})
	})

	it('prefers an allowed transport from the preferred list when available', () => {
		const descriptor: MemoryProviderDescriptor = {
			...BASE_DESCRIPTOR,
			supported_transports: ['api', 'sdk', 'mcp'],
			default_transport: 'sdk',
		}

		const result = negotiateMemoryProviderSupport(descriptor, {
			required_capabilities: ['read'],
			transport_preferences: {
				preferred: ['mcp', 'api'],
			},
		})

		expect(result).toEqual({
			ok: true,
			selected_transport: 'mcp',
			missing_capabilities: [],
			forbidden_transport_conflicts: [],
		})
	})

	it('throws a MemoryCapabilityError when asserted against an unsupported requirement', () => {
		expect(() =>
			assertMemoryNegotiation(BASE_DESCRIPTOR, {
				required_capabilities: ['profile_synthesis'],
			}),
		).toThrow(MemoryCapabilityError)
	})
})
