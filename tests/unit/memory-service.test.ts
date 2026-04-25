import { mkdtemp } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import type { MemoryBinding } from '../../src/core/agent-file.js'
import { AppError } from '../../src/core/errors.js'
import {
	MEM0_PROVIDER_FAMILY,
	MemoryProviderRegistryService,
} from '../../src/core/memory-provider-registry.js'
import { MemoryService } from '../../src/core/memory-service.js'
import { SQLiteLocalStateStore } from '../../src/core/state/index.js'
import type { MemoryAdapter } from '../../src/ports/memory.js'
import { acquireMem0ChromaTestLock, cleanupMem0TempDir } from './mem0-test-helpers.js'

const MEM0_PYTHON = path.resolve(process.cwd(), '.local', 'mem0-venv', 'Scripts', 'python.exe')

const storesToClose: SQLiteLocalStateStore[] = []
const tempDirsToRemove: string[] = []
const mem0LocksToRelease: Array<() => Promise<void>> = []

async function createHarness(prefix: string): Promise<{
	store: SQLiteLocalStateStore
	registry: MemoryProviderRegistryService
	memoryService: MemoryService
	tempDir: string
}> {
	const mem0Lock = await acquireMem0ChromaTestLock(prefix)
	mem0LocksToRelease.push(mem0Lock.release)
	const tempDir = await mkdtemp(path.join(os.tmpdir(), prefix))
	const store = new SQLiteLocalStateStore({
		database_path: path.join(tempDir, 'local-state.sqlite'),
	})
	storesToClose.push(store)
	tempDirsToRemove.push(tempDir)

	const registry = new MemoryProviderRegistryService({
		state_store: store,
	})

	registry.registerProvider({
		provider_id: 'mem0-local',
		codex_ref: 'primary_memory',
		provider_family: MEM0_PROVIDER_FAMILY,
		display_name: 'Primary Mem0',
		transport: 'sdk',
		supported_capabilities: ['read', 'write', 'user_scoped', 'infer_extract'],
		config: {
			python_executable: MEM0_PYTHON,
			working_directory: process.cwd(),
			mem0_config: {
				vector_store: {
					provider: 'chroma',
					config: {
						path: path.join(tempDir, 'chroma'),
						collection_name: `phase13-${path.basename(tempDir)}`,
					},
				},
				embedder: {
					provider: 'fastembed',
					config: {
						model: 'BAAI/bge-small-en-v1.5',
					},
				},
				llm: {
					provider: 'ollama',
					config: {
						model: 'qwen2.5:0.5b-instruct',
					},
				},
				history_db_path: path.join(tempDir, 'history.db'),
				version: 'v1.1',
			},
		},
	})

	return {
		store,
		registry,
		memoryService: new MemoryService({
			state_store: store,
		}),
		tempDir,
	}
}

function createPortableBinding(overrides: Partial<MemoryBinding> = {}): MemoryBinding {
	return {
		id: 'primary_memory_binding',
		kind: 'runtime_memory',
		codex_ref: 'primary_memory',
		scope: 'agent',
		config: {
			intent: {
				summary: 'Primary user memory for live Phase 13 proof.',
			},
			required_capabilities: ['read', 'write', 'user_scoped'],
			transport_preferences: {
				preferred: ['sdk'],
			},
			provider_extension: {
				provider: 'mem0',
			},
		},
		...overrides,
	}
}

function createFakeMemoryAdapter(overrides: Partial<MemoryAdapter> = {}): MemoryAdapter {
	return {
		describeProvider() {
			return {
				provider: MEM0_PROVIDER_FAMILY,
				display_name: 'Fake Mem0',
				supported_capabilities: ['read', 'write', 'user_scoped'],
				supported_transports: ['sdk'],
				default_transport: 'sdk',
			}
		},
		negotiate(requirement) {
			return {
				ok: true,
				selected_transport: requirement.transport_preferences?.preferred?.[0] ?? 'sdk',
				missing_capabilities: [],
				forbidden_transport_conflicts: [],
			}
		},
		async writeMemory() {
			throw new Error('Unexpected fake adapter writeMemory call.')
		},
		async readMemory() {
			throw new Error('Unexpected fake adapter readMemory call.')
		},
		async searchMemory() {
			throw new Error('Unexpected fake adapter searchMemory call.')
		},
		async listMemories() {
			throw new Error('Unexpected fake adapter listMemories call.')
		},
		async updateMemory() {
			throw new Error('Unexpected fake adapter updateMemory call.')
		},
		async deleteMemory() {
			throw new Error('Unexpected fake adapter deleteMemory call.')
		},
		async previewMemoryCleanup() {
			throw new Error('Unexpected fake adapter previewMemoryCleanup call.')
		},
		async deleteMemoryCleanup() {
			throw new Error('Unexpected fake adapter deleteMemoryCleanup call.')
		},
		...overrides,
	}
}

async function withFakeMemoryAdapter<T>(
	service: MemoryService,
	adapter: MemoryAdapter,
	action: () => Promise<T>,
): Promise<T> {
	const serviceWithPrivate = service as unknown as {
		instantiateProviderAdapter: () => MemoryAdapter
	}
	const originalInstantiateProviderAdapter = serviceWithPrivate.instantiateProviderAdapter
	serviceWithPrivate.instantiateProviderAdapter = () => adapter
	try {
		return await action()
	} finally {
		serviceWithPrivate.instantiateProviderAdapter = originalInstantiateProviderAdapter
	}
}

afterEach(async () => {
	let cleanupError: unknown
	try {
		while (storesToClose.length > 0) {
			storesToClose.pop()?.close()
		}

		while (tempDirsToRemove.length > 0) {
			const tempDir = tempDirsToRemove.pop()
			if (tempDir) {
				await cleanupMem0TempDir(tempDir)
			}
		}
	} catch (error) {
		cleanupError = error
	} finally {
		while (mem0LocksToRelease.length > 0) {
			await mem0LocksToRelease.pop()?.()
		}
	}

	if (cleanupError) {
		throw cleanupError
	}
})

describe('MemoryService', () => {
	it('resolves a portable memory binding to the registered Mem0 adapter and performs a live round-trip', async () => {
		const harness = await createHarness('dennett-phase13-memory-service-binding-')
		const binding = createPortableBinding()

		const writeResult = await harness.memoryService.writeForBinding(binding, {
			content: 'Phase 13 service binding memory',
			scope: {
				user_id: 'binding-user',
			},
			infer: false,
			metadata: {
				source: 'memory-service-binding-test',
			},
		})

		expect(writeResult.records).toHaveLength(1)
		const memoryId = writeResult.records[0]?.id
		expect(memoryId).toBeTruthy()

		const searchResult = await harness.memoryService.searchForBinding(binding, {
			query: 'service binding memory',
			scope: {
				user_id: 'binding-user',
			},
			limit: 5,
		})

		expect(searchResult.records).toHaveLength(1)
		expect(searchResult.records[0]?.id).toBe(memoryId)

		const readResult = await harness.memoryService.readForBinding(binding, {
			memory_id: memoryId ?? '',
		})
		expect(readResult).toMatchObject({
			id: memoryId,
			content: 'Phase 13 service binding memory',
			scope: {
				user_id: 'binding-user',
			},
		})

		const provider = harness.registry.getProviderOrThrow('mem0-local')
		expect(provider.status).toBe('available')
	}, 180000)

	it('rejects portable bindings whose provider_extension.config is not a JSON object', async () => {
		const harness = await createHarness('dennett-phase13-memory-service-invalid-config-')
		const binding = createPortableBinding({
			config: {
				intent: {
					summary: 'Bad provider extension config test.',
				},
				required_capabilities: ['read'],
				provider_extension: {
					provider: 'mem0',
					config: 'invalid-provider-extension-config' as unknown as never,
				},
			},
		})

		expect(() => harness.memoryService.resolveAdapterForBinding(binding)).toThrow(
			/provider_extension\.config must be a JSON object/i,
		)
	})

	it('rejects providers whose registered transport is unsupported by the instantiated adapter', async () => {
		const harness = await createHarness('dennett-phase13-memory-service-transport-mismatch-')
		harness.registry.registerProvider({
			provider_id: 'mem0-api-mismatch',
			codex_ref: 'api_memory',
			provider_family: MEM0_PROVIDER_FAMILY,
			display_name: 'Mem0 API Mismatch',
			transport: 'api',
			supported_capabilities: ['read', 'write', 'user_scoped'],
			config: {
				python_executable: MEM0_PYTHON,
				working_directory: process.cwd(),
				mem0_config: {
					vector_store: {
						provider: 'chroma',
						config: {
							path: path.join(harness.tempDir, 'mismatch-chroma'),
							collection_name: `phase13-mismatch-${path.basename(harness.tempDir)}`,
						},
					},
					embedder: {
						provider: 'fastembed',
						config: {
							model: 'BAAI/bge-small-en-v1.5',
						},
					},
					history_db_path: path.join(harness.tempDir, 'mismatch-history.db'),
					version: 'v1.1',
				},
			},
		})

		expect(() =>
			harness.memoryService.resolveAdapterForCodexRef('api_memory', {
				required_capabilities: ['read'],
			}),
		).toThrow(/registered with transport "api".*only supports: sdk/i)
	})

	it('uses adapter-side capability negotiation instead of trusting registry capability claims', async () => {
		const harness = await createHarness('dennett-phase13-memory-service-capability-negotiation-')
		harness.registry.registerProvider({
			provider_id: 'mem0-overclaimed',
			codex_ref: 'overclaimed_memory',
			provider_family: MEM0_PROVIDER_FAMILY,
			display_name: 'Mem0 Overclaimed',
			transport: 'sdk',
			supported_capabilities: ['read', 'write', 'user_scoped', 'graph_context'],
			config: {
				python_executable: MEM0_PYTHON,
				working_directory: process.cwd(),
				mem0_config: {
					vector_store: {
						provider: 'chroma',
						config: {
							path: path.join(harness.tempDir, 'overclaimed-chroma'),
							collection_name: `phase13-overclaimed-${path.basename(harness.tempDir)}`,
						},
					},
					embedder: {
						provider: 'fastembed',
						config: {
							model: 'BAAI/bge-small-en-v1.5',
						},
					},
					history_db_path: path.join(harness.tempDir, 'overclaimed-history.db'),
					version: 'v1.1',
				},
			},
		})

		expect(() =>
			harness.memoryService.resolveAdapterForCodexRef('overclaimed_memory', {
				required_capabilities: ['graph_context'],
			}),
		).toThrow(/missing capabilities: graph_context/i)
	})

	it('applies provider_extension.config overrides before adapter-side negotiation', async () => {
		const harness = await createHarness('dennett-phase13-memory-service-provider-override-')
		const binding = createPortableBinding({
			config: {
				intent: {
					summary: 'Graph-capable memory binding via provider override.',
				},
				required_capabilities: ['read', 'graph_context'],
				transport_preferences: {
					preferred: ['sdk'],
				},
				provider_extension: {
					provider: 'mem0',
					config: {
						mem0_config: {
							graph_store: {
								provider: 'networkx',
								config: {},
							},
						},
					},
				},
			},
		})

		expect(() => harness.memoryService.resolveAdapterForBinding(binding)).not.toThrow()
	})

	it('rejects graph_store overrides without provider before adapter-side graph capability negotiation', async () => {
		const harness = await createHarness('dennett-phase13-memory-service-missing-graph-provider-')
		const binding = createPortableBinding({
			config: {
				intent: {
					summary: 'Graph memory binding with invalid empty graph_store override.',
				},
				required_capabilities: ['read', 'graph_context'],
				transport_preferences: {
					preferred: ['sdk'],
				},
				provider_extension: {
					provider: 'mem0',
					config: {
						mem0_config: {
							graph_store: {},
						},
					},
				},
			},
		})

		expect(() => harness.memoryService.resolveAdapterForBinding(binding)).toThrow(
			/graph_store\.provider must be a non-empty string when graph_store is present/i,
		)
	})

	it('rejects provider_extension.config attempts to override local Mem0 registration fields', async () => {
		const harness = await createHarness('dennett-phase13-memory-service-forbidden-override-')
		const binding = createPortableBinding({
			config: {
				intent: {
					summary: 'Attempt to override local Mem0 bridge settings.',
				},
				required_capabilities: ['read'],
				provider_extension: {
					provider: 'mem0',
					config: {
						python_executable: 'C:/forbidden/python.exe',
					},
				},
			},
		})

		expect(() => harness.memoryService.resolveAdapterForBinding(binding)).toThrow(
			/may only override "mem0_config".*python_executable/i,
		)
	})

	it('rejects nested sensitive Mem0 overrides under llm, embedder, and vector_store', async () => {
		const harness = await createHarness('dennett-phase13-memory-service-forbidden-nested-override-')

		const llmBinding = createPortableBinding({
			config: {
				intent: { summary: 'Attempt to override Mem0 llm credentials.' },
				required_capabilities: ['read'],
				provider_extension: {
					provider: 'mem0',
					config: {
						mem0_config: {
							llm: {
								config: {
									api_key: 'forbidden-key',
								},
							},
						},
					},
				},
			},
		})
		expect(() => harness.memoryService.resolveAdapterForBinding(llmBinding)).toThrow(
			/mem0_config may only override "graph_store".*llm/i,
		)

		const embedderBinding = createPortableBinding({
			config: {
				intent: { summary: 'Attempt to override Mem0 embedder credentials.' },
				required_capabilities: ['read'],
				provider_extension: {
					provider: 'mem0',
					config: {
						mem0_config: {
							embedder: {
								config: {
									api_key: 'forbidden-key',
								},
							},
						},
					},
				},
			},
		})
		expect(() => harness.memoryService.resolveAdapterForBinding(embedderBinding)).toThrow(
			/mem0_config may only override "graph_store".*embedder/i,
		)

		const vectorStoreBinding = createPortableBinding({
			config: {
				intent: { summary: 'Attempt to override Mem0 vector-store credentials.' },
				required_capabilities: ['read'],
				provider_extension: {
					provider: 'mem0',
					config: {
						mem0_config: {
							vector_store: {
								config: {
									api_key: 'forbidden-key',
								},
							},
						},
					},
				},
			},
		})
		expect(() => harness.memoryService.resolveAdapterForBinding(vectorStoreBinding)).toThrow(
			/mem0_config may only override "graph_store".*vector_store/i,
		)
	})

	it('rejects nested graph_store config overrides such as api_key', async () => {
		const harness = await createHarness(
			'dennett-phase13-memory-service-forbidden-graph-store-config-',
		)
		const binding = createPortableBinding({
			config: {
				intent: { summary: 'Attempt to override graph_store nested credentials.' },
				required_capabilities: ['read'],
				provider_extension: {
					provider: 'mem0',
					config: {
						mem0_config: {
							graph_store: {
								provider: 'networkx',
								config: {
									api_key: 'forbidden-key',
								},
							},
						},
					},
				},
			},
		})

		expect(() => harness.memoryService.resolveAdapterForBinding(binding)).toThrow(
			/graph_store\.config must stay empty.*api_key/i,
		)
	})

	it('prepares provider-neutral runtime memory context with searched records and write eligibility', async () => {
		const harness = await createHarness('dennett-task321-memory-service-runtime-context-')
		const binding = createPortableBinding()
		const scope = {
			agent_id: 'agent-runtime',
			run_id: 'run-runtime',
			user_id: 'runtime-user',
		}

		const writeResult = await harness.memoryService.writeForBinding(binding, {
			content: 'Runtime helper should retrieve this memory',
			scope,
			infer: false,
			metadata: {
				source: 'runtime-context-test',
			},
		})
		const memoryId = writeResult.records[0]?.id
		expect(memoryId).toBeTruthy()

		const prepared = await harness.memoryService.prepareRuntimeMemoryBindingContext({
			binding,
			scope,
			read: {
				query: 'retrieve this memory',
				limit: 5,
			},
		})

		expect(prepared.provider.provider_id).toBe('mem0-local')
		expect(prepared.read_enabled).toBe(true)
		expect(prepared.write_enabled).toBe(true)
		expect(prepared.required_capabilities).toEqual(['read', 'write', 'user_scoped'])
		expect(prepared.context).toMatchObject({
			binding_id: 'primary_memory_binding',
			codex_ref: 'primary_memory',
			intent: {
				summary: 'Primary user memory for live Phase 13 proof.',
			},
			required_capabilities: ['read', 'write', 'user_scoped'],
			scope,
			read: {
				query: 'retrieve this memory',
			},
			write: {
				enabled: true,
				mode: 'node_success_output',
			},
		})
		expect(prepared.context.read?.records.some((record) => record.id === memoryId)).toBe(true)
		expect(prepared.context.read?.records[0]?.scope).toMatchObject(scope)
	}, 180000)

	it('prepares runtime memory context without read or write when capabilities are absent', async () => {
		const harness = await createHarness('dennett-task321-memory-service-runtime-eligibility-')
		const binding = createPortableBinding({
			config: {
				intent: {
					summary: 'Runtime binding that only exposes user scoping.',
				},
				required_capabilities: ['user_scoped'],
			},
		})

		const prepared = await harness.memoryService.prepareRuntimeMemoryBindingContext({
			binding,
			scope: {
				agent_id: 'agent-runtime',
				run_id: 'run-runtime',
				user_id: 'runtime-user',
			},
			read: {
				query: 'should not call provider search',
			},
		})

		expect(prepared.read_enabled).toBe(false)
		expect(prepared.write_enabled).toBe(false)
		expect(prepared.context.read).toBeUndefined()
		expect(prepared.context.write).toEqual({
			enabled: false,
			disabled_reason: 'Memory binding does not declare the "write" capability.',
		})
	})

	it('plans success-only runtime memory writes with deterministic Dennett metadata', async () => {
		const harness = await createHarness('dennett-task321-memory-service-write-plan-')
		const binding = createPortableBinding()
		const outputHash = 'sha256:output-a'
		const expectedWriteKey = 'dennett:run-a:node-a:primary_memory_binding:sha256%3Aoutput-a'

		const plan = harness.memoryService.planRuntimeMemorySuccessWrite({
			binding,
			scope: {
				agent_id: 'agent-a',
				run_id: 'run-a',
				user_id: 'user-a',
			},
			node_id: 'node-a',
			attempt_id: 'attempt-a',
			output_mode: 'text',
			output_hash: outputHash,
			content: 'Remember this successful node output.',
			outcome: 'success',
			infer: false,
			metadata: {
				dennett_kind: 'caller-cannot-override',
				dennett_write_key: 'caller-cannot-override',
				output_hash: 'caller-cannot-override',
				source: 'caller',
			},
		})

		expect(plan.should_write).toBe(true)
		if (!plan.should_write) {
			throw new Error('Expected write plan.')
		}
		expect(plan.dennett_write_key).toBe(expectedWriteKey)
		expect(plan.metadata).toEqual({
			source: 'caller',
			dennett_kind: 'runtime_node_output',
			agent_id: 'agent-a',
			run_id: 'run-a',
			node_id: 'node-a',
			binding_id: 'primary_memory_binding',
			attempt_id: 'attempt-a',
			output_mode: 'text',
			output_hash: outputHash,
			dennett_write_key: expectedWriteKey,
			dennett_write_mode: 'node_success_output',
			dennett_binding_id: 'primary_memory_binding',
			dennett_codex_ref: 'primary_memory',
			dennett_agent_id: 'agent-a',
			dennett_run_id: 'run-a',
			dennett_node_id: 'node-a',
			dennett_attempt_id: 'attempt-a',
		})
		expect(plan.request).toEqual({
			content: 'Remember this successful node output.',
			scope: {
				agent_id: 'agent-a',
				run_id: 'run-a',
				user_id: 'user-a',
			},
			metadata: plan.metadata,
			infer: false,
		})

		const retryPlan = harness.memoryService.planRuntimeMemorySuccessWrite({
			binding,
			scope: {
				agent_id: 'agent-a',
				run_id: 'run-a',
				user_id: 'user-a',
			},
			node_id: 'node-a',
			attempt_id: 'attempt-b',
			output_mode: 'text',
			output_hash: outputHash,
			content: 'Remember this successful node output.',
			outcome: 'success',
			infer: false,
		})
		const changedOutputPlan = harness.memoryService.planRuntimeMemorySuccessWrite({
			binding,
			scope: {
				agent_id: 'agent-a',
				run_id: 'run-a',
				user_id: 'user-a',
			},
			node_id: 'node-a',
			attempt_id: 'attempt-b',
			output_mode: 'text',
			output_hash: 'sha256:output-b',
			content: 'Remember changed successful node output.',
			outcome: 'success',
			infer: false,
		})

		expect(retryPlan.should_write).toBe(true)
		expect(changedOutputPlan.should_write).toBe(true)
		if (!retryPlan.should_write || !changedOutputPlan.should_write) {
			throw new Error('Expected write plans.')
		}
		expect(retryPlan.dennett_write_key).toBe(expectedWriteKey)
		expect(retryPlan.metadata).toMatchObject({
			attempt_id: 'attempt-b',
			output_hash: outputHash,
			dennett_write_key: expectedWriteKey,
		})
		expect(changedOutputPlan.dennett_write_key).toBe(
			'dennett:run-a:node-a:primary_memory_binding:sha256%3Aoutput-b',
		)
		expect(changedOutputPlan.dennett_write_key).not.toBe(plan.dennett_write_key)
	})

	it('skips runtime memory writes for non-success outcomes without resolving a provider', async () => {
		const harness = await createHarness('dennett-task321-memory-service-non-success-skip-')
		const result = await harness.memoryService.writeRuntimeMemoryOnSuccess({
			binding: createPortableBinding({
				codex_ref: 'missing-provider-ref',
			}),
			scope: {
				agent_id: 'agent-a',
				run_id: 'run-a',
			},
			node_id: 'node-a',
			attempt_id: 'attempt-a',
			output_mode: 'text',
			output_hash: 'sha256:skipped-output',
			content: 'This failed output must not be written.',
			outcome: 'interrupted',
		})

		expect(result).toEqual({
			status: 'skipped',
			disabled_reason: 'Runtime memory writes only run for successful node outcomes.',
		})
	})

	it('writes successful runtime memory output and records Dennett write metadata', async () => {
		const harness = await createHarness('dennett-task321-memory-service-success-write-')
		const binding = createPortableBinding()
		const scope = {
			agent_id: 'agent-success',
			run_id: 'run-success',
			user_id: 'success-user',
		}

		const result = await harness.memoryService.writeRuntimeMemoryOnSuccess({
			binding,
			scope,
			node_id: 'node-success',
			attempt_id: 'attempt-success',
			output_mode: 'text',
			output_hash: 'sha256:success-output',
			content: 'Successful runtime output to remember',
			outcome: 'success',
			infer: false,
		})

		expect(result.status).toBe('written')
		if (result.status !== 'written') {
			throw new Error('Expected runtime memory write.')
		}
		expect(result.dennett_write_key).toBe(
			'dennett:run-success:node-success:primary_memory_binding:sha256%3Asuccess-output',
		)
		expect(result.result.records).toHaveLength(1)
		expect(result.metadata).toMatchObject({
			dennett_kind: 'runtime_node_output',
			agent_id: 'agent-success',
			run_id: 'run-success',
			node_id: 'node-success',
			binding_id: 'primary_memory_binding',
			attempt_id: 'attempt-success',
			output_mode: 'text',
			output_hash: 'sha256:success-output',
			dennett_write_key:
				'dennett:run-success:node-success:primary_memory_binding:sha256%3Asuccess-output',
			dennett_write_mode: 'node_success_output',
			dennett_binding_id: 'primary_memory_binding',
			dennett_codex_ref: 'primary_memory',
			dennett_agent_id: 'agent-success',
			dennett_run_id: 'run-success',
			dennett_node_id: 'node-success',
			dennett_attempt_id: 'attempt-success',
		})
	}, 180000)

	it('propagates provider write errors through runtime success writes and marks provider status', async () => {
		const harness = await createHarness('dennett-task321-memory-service-provider-error-')
		harness.registry.registerProvider({
			provider_id: 'mem0-bad-python',
			codex_ref: 'bad_memory',
			provider_family: MEM0_PROVIDER_FAMILY,
			display_name: 'Bad Mem0',
			transport: 'sdk',
			supported_capabilities: ['read', 'write', 'user_scoped'],
			config: {
				python_executable: path.join(harness.tempDir, 'missing-python.exe'),
				working_directory: process.cwd(),
				mem0_config: {
					vector_store: {
						provider: 'chroma',
						config: {
							path: path.join(harness.tempDir, 'bad-chroma'),
							collection_name: `phase13-bad-${path.basename(harness.tempDir)}`,
						},
					},
					embedder: {
						provider: 'fastembed',
						config: {
							model: 'BAAI/bge-small-en-v1.5',
						},
					},
					history_db_path: path.join(harness.tempDir, 'bad-history.db'),
					version: 'v1.1',
				},
			},
		})

		await expect(
			harness.memoryService.writeRuntimeMemoryOnSuccess({
				binding: createPortableBinding({
					codex_ref: 'bad_memory',
				}),
				scope: {
					agent_id: 'agent-bad',
					run_id: 'run-bad',
					user_id: 'bad-user',
				},
				node_id: 'node-bad',
				attempt_id: 'attempt-bad',
				output_mode: 'text',
				output_hash: 'sha256:bad-output',
				content: 'This write should fail at provider execution.',
				outcome: 'success',
				infer: false,
			}),
		).rejects.toThrow()

		const provider = harness.registry.getProviderOrThrow('mem0-bad-python')
		expect(provider.status).toBe('error')
		expect(provider.status_code).toBeTruthy()
	})

	it('propagates transient adapter read, search, and write failures and records provider status', async () => {
		const writeHarness = await createHarness('dennett-task378-memory-service-write-transient-')
		let writeShouldFail = true
		const writeAdapter = createFakeMemoryAdapter({
			async writeMemory(request) {
				if (writeShouldFail) {
					throw new AppError('MEMORY_PROVIDER_TRANSIENT_WRITE_FAILED', 'Transient write failure.')
				}
				return {
					records: [
						{
							id: 'memory-write-ok',
							content: request.content,
							scope: request.scope,
							metadata: request.metadata,
						},
					],
				}
			},
		})

		await expect(
			withFakeMemoryAdapter(writeHarness.memoryService, writeAdapter, () =>
				writeHarness.memoryService.writeForBinding(createPortableBinding(), {
					content: 'Transient write content',
					scope: {
						user_id: ' user-write ',
						agent_id: '',
					},
				}),
			),
		).rejects.toMatchObject({
			code: 'MEMORY_PROVIDER_TRANSIENT_WRITE_FAILED',
			message: 'Transient write failure.',
		})
		let provider = writeHarness.registry.getProviderOrThrow('mem0-local')
		expect(provider.status).toBe('error')
		expect(provider.status_code).toBe('MEMORY_PROVIDER_TRANSIENT_WRITE_FAILED')

		writeShouldFail = false
		await expect(
			withFakeMemoryAdapter(writeHarness.memoryService, writeAdapter, () =>
				writeHarness.memoryService.writeForBinding(createPortableBinding(), {
					content: 'Recovered write content',
					scope: {
						user_id: ' user-write ',
						agent_id: '',
					},
				}),
			),
		).resolves.toMatchObject({
			records: [
				{
					id: 'memory-write-ok',
					content: 'Recovered write content',
					scope: {
						user_id: 'user-write',
					},
				},
			],
		})
		provider = writeHarness.registry.getProviderOrThrow('mem0-local')
		expect(provider.status).toBe('available')
		expect(provider.status_code).toBeNull()

		const readHarness = await createHarness('dennett-task378-memory-service-read-transient-')
		const readAdapter = createFakeMemoryAdapter({
			async readMemory() {
				throw new AppError('MEMORY_PROVIDER_TRANSIENT_READ_FAILED', 'Transient read failure.')
			},
		})

		await expect(
			withFakeMemoryAdapter(readHarness.memoryService, readAdapter, () =>
				readHarness.memoryService.readForBinding(createPortableBinding(), {
					memory_id: 'memory-read-fail',
				}),
			),
		).rejects.toMatchObject({
			code: 'MEMORY_PROVIDER_TRANSIENT_READ_FAILED',
			message: 'Transient read failure.',
		})
		provider = readHarness.registry.getProviderOrThrow('mem0-local')
		expect(provider.status).toBe('error')
		expect(provider.status_code).toBe('MEMORY_PROVIDER_TRANSIENT_READ_FAILED')

		const searchHarness = await createHarness('dennett-task378-memory-service-search-transient-')
		const searchAdapter = createFakeMemoryAdapter({
			async searchMemory() {
				throw new AppError('MEMORY_PROVIDER_TRANSIENT_SEARCH_FAILED', 'Transient search failure.')
			},
		})

		await expect(
			withFakeMemoryAdapter(searchHarness.memoryService, searchAdapter, () =>
				searchHarness.memoryService.searchForBinding(createPortableBinding(), {
					query: 'transient search',
					scope: {
						user_id: ' user-search ',
						run_id: '',
					},
				}),
			),
		).rejects.toMatchObject({
			code: 'MEMORY_PROVIDER_TRANSIENT_SEARCH_FAILED',
			message: 'Transient search failure.',
		})
		provider = searchHarness.registry.getProviderOrThrow('mem0-local')
		expect(provider.status).toBe('error')
		expect(provider.status_code).toBe('MEMORY_PROVIDER_TRANSIENT_SEARCH_FAILED')
	})

	it('uses direct codex_ref operations for read, list, update, and delete', async () => {
		const harness = await createHarness('dennett-phase13-memory-service-direct-')

		const writeResult = await harness.memoryService.writeForCodexRef('primary_memory', {
			content: 'Phase 13 direct codex ref memory',
			scope: {
				user_id: 'direct-user',
			},
			infer: false,
		})

		const memoryId = writeResult.records[0]?.id ?? ''
		expect(memoryId).not.toBe('')

		const readBack = await harness.memoryService.readForCodexRef('primary_memory', {
			memory_id: memoryId,
		})
		expect(readBack).toMatchObject({
			id: memoryId,
			content: 'Phase 13 direct codex ref memory',
			scope: {
				user_id: 'direct-user',
			},
		})

		const listed = await harness.memoryService.listForCodexRef('primary_memory', {
			scope: {
				user_id: 'direct-user',
			},
			limit: 10,
		})
		expect(listed).toHaveLength(1)
		expect(listed[0]?.id).toBe(memoryId)

		const updated = await harness.memoryService.updateForCodexRef('primary_memory', {
			memory_id: memoryId,
			content: 'Phase 13 updated direct memory',
			metadata: {
				source: 'updated',
			},
		})
		expect(updated).toMatchObject({
			id: memoryId,
			content: 'Phase 13 updated direct memory',
			metadata: {
				source: 'updated',
			},
		})

		const deleted = await harness.memoryService.deleteForCodexRef('primary_memory', {
			memory_id: memoryId,
		})
		expect(deleted).toEqual({
			deleted: true,
		})

		await expect(
			harness.memoryService.readForCodexRef('primary_memory', {
				memory_id: memoryId,
			}),
		).resolves.toBeNull()
	}, 180000)

	it('previews cleanup through a registry-resolved codex_ref adapter and normalizes scope', async () => {
		const harness = await createHarness('dennett-task353-memory-service-preview-cleanup-')
		let observedScope: unknown
		const adapter = createFakeMemoryAdapter({
			async previewMemoryCleanup(request) {
				observedScope = request.scope
				return {
					namespace_id: 'namespace-a',
					candidate_ids: ['memory-a'],
					candidate_count: 1,
					limit: request.limit ?? 10000,
					truncated: false,
				}
			},
		})

		const preview = await withFakeMemoryAdapter(harness.memoryService, adapter, () =>
			harness.memoryService.previewMemoryCleanupForCodexRef('primary_memory', {
				scope: {
					user_id: ' user-a ',
					agent_id: '',
					run_id: 'run-a',
				},
				limit: 25,
			}),
		)

		expect(preview).toEqual({
			namespace_id: 'namespace-a',
			candidate_ids: ['memory-a'],
			candidate_count: 1,
			limit: 25,
			truncated: false,
		})
		expect(observedScope).toEqual({
			user_id: 'user-a',
			run_id: 'run-a',
		})
		expect(harness.registry.getProviderOrThrow('mem0-local').status).toBe('available')
	})

	it('runs verified cleanup delete through a registry-resolved codex_ref adapter and normalizes scope', async () => {
		const harness = await createHarness('dennett-task353-memory-service-delete-cleanup-')
		let observedRequest: unknown
		const adapter = createFakeMemoryAdapter({
			async deleteMemoryCleanup(request) {
				observedRequest = request
				return {
					namespace_id: 'namespace-a',
					limit: request.limit ?? 10000,
					requested_ids: request.candidate_ids ?? [],
					deleted_ids: request.candidate_ids ?? [],
					skipped_ids: [],
					remaining_ids: [],
					requested_truncated: false,
					remaining_truncated: false,
					verified_empty: true,
				}
			},
		})

		const cleanup = await withFakeMemoryAdapter(harness.memoryService, adapter, () =>
			harness.memoryService.deleteMemoryCleanupForCodexRef('primary_memory', {
				scope: {
					user_id: ' user-a ',
					agent_id: 'agent-a',
					run_id: '',
				},
				candidate_ids: ['memory-a'],
				limit: 25,
			}),
		)

		expect(cleanup).toEqual({
			namespace_id: 'namespace-a',
			limit: 25,
			requested_ids: ['memory-a'],
			deleted_ids: ['memory-a'],
			skipped_ids: [],
			remaining_ids: [],
			requested_truncated: false,
			remaining_truncated: false,
			verified_empty: true,
		})
		expect(observedRequest).toEqual({
			scope: {
				user_id: 'user-a',
				agent_id: 'agent-a',
			},
			candidate_ids: ['memory-a'],
			limit: 25,
		})
		expect(harness.registry.getProviderOrThrow('mem0-local').status).toBe('available')
	})

	it('fails clearly and marks provider status when a resolved adapter lacks cleanup support', async () => {
		const harness = await createHarness('dennett-task353-memory-service-unsupported-cleanup-')
		const adapterWithoutCleanup = createFakeMemoryAdapter() as Partial<MemoryAdapter>
		delete adapterWithoutCleanup.previewMemoryCleanup

		await expect(
			withFakeMemoryAdapter(harness.memoryService, adapterWithoutCleanup as MemoryAdapter, () =>
				harness.memoryService.previewMemoryCleanupForCodexRef('primary_memory', {
					scope: {
						user_id: 'user-a',
					},
				}),
			),
		).rejects.toMatchObject({
			code: 'MEMORY_PROVIDER_CLEANUP_UNSUPPORTED',
			message:
				'Memory provider "mem0-local" does not expose adapter cleanup method "previewMemoryCleanup".',
		})

		const provider = harness.registry.getProviderOrThrow('mem0-local')
		expect(provider.status).toBe('error')
		expect(provider.status_code).toBe('MEMORY_PROVIDER_CLEANUP_UNSUPPORTED')
	})

	it('propagates adapter cleanup errors and marks provider status', async () => {
		const harness = await createHarness('dennett-task353-memory-service-cleanup-error-')
		const adapter = createFakeMemoryAdapter({
			async deleteMemoryCleanup() {
				throw new AppError(
					'MEMORY_CLEANUP_NAMESPACE_REQUIRED',
					'Mem0 namespace cleanup requires mem0_config.dennett_namespace_id.',
				)
			},
		})

		await expect(
			withFakeMemoryAdapter(harness.memoryService, adapter, () =>
				harness.memoryService.deleteMemoryCleanupForCodexRef('primary_memory', {
					scope: {
						user_id: 'user-a',
					},
				}),
			),
		).rejects.toMatchObject({
			code: 'MEMORY_CLEANUP_NAMESPACE_REQUIRED',
			message: 'Mem0 namespace cleanup requires mem0_config.dennett_namespace_id.',
		})

		const provider = harness.registry.getProviderOrThrow('mem0-local')
		expect(provider.status).toBe('error')
		expect(provider.status_code).toBe('MEMORY_CLEANUP_NAMESPACE_REQUIRED')
	})
})
