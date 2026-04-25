import { mkdtemp, rm } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { AppError } from '../../src/core/errors.js'
import {
	MEM0_PROVIDER_FAMILY,
	MemoryProviderRegistryService,
} from '../../src/core/memory-provider-registry.js'
import { SQLiteLocalStateStore } from '../../src/core/state/index.js'
import {
	listRegisteredMemoryProviders,
	registerMemoryProvider,
	showRegisteredMemoryProvider,
} from '../../src/interfaces/cli.js'

const storesToClose: SQLiteLocalStateStore[] = []
const tempDirsToRemove: string[] = []

async function createStore(): Promise<SQLiteLocalStateStore> {
	const tempDir = await mkdtemp(path.join(os.tmpdir(), 'dennett-phase13-memory-'))
	const store = new SQLiteLocalStateStore({
		database_path: path.join(tempDir, 'local-state.sqlite'),
	})
	storesToClose.push(store)
	tempDirsToRemove.push(tempDir)
	return store
}

async function createStateDbPath(): Promise<string> {
	const tempDir = await mkdtemp(path.join(os.tmpdir(), 'dennett-phase13-memory-cli-'))
	tempDirsToRemove.push(tempDir)
	return path.join(tempDir, 'local-state.sqlite')
}

afterEach(async () => {
	while (storesToClose.length > 0) {
		storesToClose.pop()?.close()
	}

	while (tempDirsToRemove.length > 0) {
		const tempDir = tempDirsToRemove.pop()
		if (tempDir) {
			await rm(tempDir, { recursive: true, force: true })
		}
	}
})

describe('MemoryProviderRegistryService', () => {
	it('registers and resolves a Mem0 provider with capability and transport checks', async () => {
		const store = await createStore()
		const registry = new MemoryProviderRegistryService({
			state_store: store,
		})

		const registered = registry.registerProvider({
			provider_id: 'mem0-local',
			codex_ref: 'primary_memory',
			provider_family: MEM0_PROVIDER_FAMILY,
			display_name: 'Primary Mem0',
			transport: 'mcp',
			supported_capabilities: ['read', 'write', 'mcp_transport'],
			config: {
				server_name: 'mem0',
			},
		})

		const resolved = registry.resolveProvider({
			codex_ref: 'primary_memory',
			provider_family: MEM0_PROVIDER_FAMILY,
			required_capabilities: ['read', 'mcp_transport'],
			transport_preferences: {
				preferred: ['mcp'],
			},
		})

		expect(registered).toMatchObject({
			provider_id: 'mem0-local',
			codex_ref: 'primary_memory',
			provider_family: MEM0_PROVIDER_FAMILY,
			transport: 'mcp',
			status: 'configured',
			supported_capabilities: ['read', 'write', 'mcp_transport'],
		})
		expect(resolved).toEqual(registered)
		expect(registry.listProviders(MEM0_PROVIDER_FAMILY)).toEqual([registered])
	})

	it('fails explicitly for unsupported families, unknown providers, and missing capabilities', async () => {
		const store = await createStore()
		const registry = new MemoryProviderRegistryService({
			state_store: store,
		})

		registry.registerProvider({
			provider_id: 'mem0-local',
			codex_ref: 'primary_memory',
			provider_family: MEM0_PROVIDER_FAMILY,
			transport: 'api',
			supported_capabilities: ['read'],
			config: {},
		})

		expect(() =>
			registry.resolveProvider({
				codex_ref: 'missing_memory',
			}),
		).toThrowError(AppError)

		expect(() =>
			registry.registerProvider({
				provider_id: 'graphiti-local',
				provider_family: 'graphiti',
				config: {},
			}),
		).toThrowError(AppError)

		let missingCapabilityError: unknown
		try {
			registry.resolveProvider({
				codex_ref: 'primary_memory',
				required_capabilities: ['write'],
			})
		} catch (error) {
			missingCapabilityError = error
		}

		expect(missingCapabilityError).toBeInstanceOf(AppError)
		expect((missingCapabilityError as AppError).code).toBe('MEMORY_PROVIDER_CAPABILITY_MISSING')
	})

	it('honors disabled status and forbidden transport preferences', async () => {
		const store = await createStore()
		const registry = new MemoryProviderRegistryService({
			state_store: store,
		})

		registry.registerProvider({
			provider_id: 'mem0-local',
			codex_ref: 'primary_memory',
			provider_family: MEM0_PROVIDER_FAMILY,
			transport: 'mcp',
			supported_capabilities: ['read', 'mcp_transport'],
			config: {},
		})
		registry.updateProviderStatus({
			provider_id: 'mem0-local',
			status: 'disabled',
			status_code: 'MANUALLY_DISABLED',
		})

		let disabledError: unknown
		try {
			registry.resolveProvider({
				codex_ref: 'primary_memory',
			})
		} catch (error) {
			disabledError = error
		}
		expect(disabledError).toBeInstanceOf(AppError)
		expect((disabledError as AppError).code).toBe('MEMORY_PROVIDER_DISABLED')

		registry.updateProviderStatus({
			provider_id: 'mem0-local',
			status: 'available',
		})

		let transportError: unknown
		try {
			registry.resolveProvider({
				codex_ref: 'primary_memory',
				transport_preferences: {
					forbid: ['mcp'],
				},
			})
		} catch (error) {
			transportError = error
		}
		expect(transportError).toBeInstanceOf(AppError)
		expect((transportError as AppError).code).toBe('MEMORY_PROVIDER_TRANSPORT_FORBIDDEN')
	})
})

describe('memory provider CLI helpers', () => {
	it('registers, lists, and shows local providers through the CLI-adjacent surface', async () => {
		const stateDbPath = await createStateDbPath()

		const registered = await registerMemoryProvider(
			{
				providerId: 'mem0-local',
				codexRef: 'primary_memory',
				providerFamily: MEM0_PROVIDER_FAMILY,
				displayName: 'Primary Mem0',
				transport: 'api',
				supportedCapabilities: ['read', 'write'],
				config: {
					base_url: 'http://127.0.0.1:8000',
				},
			},
			stateDbPath,
		)

		const listed = await listRegisteredMemoryProviders(stateDbPath)
		const shown = await showRegisteredMemoryProvider('mem0-local', stateDbPath)

		expect(registered).toMatchObject({
			provider_id: 'mem0-local',
			codex_ref: 'primary_memory',
			provider_family: MEM0_PROVIDER_FAMILY,
			display_name: 'Primary Mem0',
			transport: 'api',
			status: 'configured',
		})
		expect(listed).toEqual([registered])
		expect(shown).toEqual(registered)
	})

	it('defaults Mem0 registrations to sdk transport when transport is omitted', async () => {
		const stateDbPath = await createStateDbPath()

		const registered = await registerMemoryProvider(
			{
				providerId: 'mem0-local-default',
				codexRef: 'primary_memory_default',
				providerFamily: MEM0_PROVIDER_FAMILY,
				displayName: 'Primary Mem0 Default',
				supportedCapabilities: ['read', 'write'],
				config: {
					python_executable: 'C:/python.exe',
					mem0_config: {},
				},
			},
			stateDbPath,
		)

		expect(registered).toMatchObject({
			provider_id: 'mem0-local-default',
			transport: 'sdk',
		})
	})
})
