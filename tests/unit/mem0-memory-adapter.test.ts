import { mkdtemp } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { describe, expect, it } from 'vitest'
import {
	type Mem0BridgeRunner,
	Mem0MemoryAdapter,
} from '../../src/adapters/memory/mem0-memory-adapter.js'
import {
	MemoryConfigurationError,
	MemoryExecutionError,
	type MemoryRecord,
} from '../../src/ports/memory.js'
import { acquireMem0ChromaTestLock, cleanupMem0TempDir } from './mem0-test-helpers.js'

const MEM0_PYTHON = path.resolve(process.cwd(), '.local', 'mem0-venv', 'Scripts', 'python.exe')
const DENNETT_NAMESPACE_METADATA_KEY = 'dennett_namespace_id'

interface AdapterHarnessOptions {
	acquireMem0Lock?: boolean
}

function createMem0Config(
	chromaPath: string,
	historyDbPath: string,
	collectionName: string,
	namespaceId?: string,
) {
	return {
		...(namespaceId ? { dennett_namespace_id: namespaceId } : {}),
		vector_store: {
			provider: 'chroma',
			config: {
				path: chromaPath,
				collection_name: collectionName,
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
		history_db_path: historyDbPath,
		version: 'v1.1',
	}
}

async function createAdapter(
	prefix: string,
	namespaceId?: string,
	options: AdapterHarnessOptions = {},
) {
	const mem0Lock = options.acquireMem0Lock ? await acquireMem0ChromaTestLock(prefix) : undefined
	const tempDir = await mkdtemp(path.join(os.tmpdir(), prefix))
	const chromaPath = path.join(tempDir, 'chroma')
	const historyDbPath = path.join(tempDir, 'history.db')
	const collectionName = `phase13-${path.basename(tempDir)}`

	const adapter = new Mem0MemoryAdapter({
		python_executable: MEM0_PYTHON,
		working_directory: process.cwd(),
		mem0_config: createMem0Config(chromaPath, historyDbPath, collectionName, namespaceId),
	})

	return {
		adapter,
		tempDir,
		async cleanup() {
			try {
				await cleanupMem0TempDir(tempDir)
			} finally {
				await mem0Lock?.release()
			}
		},
	}
}

async function createSharedNamespaceHarness(prefix: string) {
	const mem0Lock = await acquireMem0ChromaTestLock(prefix)
	const tempDir = await mkdtemp(path.join(os.tmpdir(), prefix))
	const chromaPath = path.join(tempDir, 'chroma')
	const collectionName = `phase13-${path.basename(tempDir)}`

	function createNamespacedAdapter(namespaceId: string, historyDbName: string): Mem0MemoryAdapter {
		return new Mem0MemoryAdapter({
			python_executable: MEM0_PYTHON,
			working_directory: process.cwd(),
			mem0_config: createMem0Config(
				chromaPath,
				path.join(tempDir, historyDbName),
				collectionName,
				namespaceId,
			),
		})
	}

	return {
		target: createNamespacedAdapter('target-namespace', 'target-history.db'),
		control: createNamespacedAdapter('control-namespace', 'control-history.db'),
		tempDir,
		async cleanup() {
			try {
				await cleanupMem0TempDir(tempDir)
			} finally {
				await mem0Lock.release()
			}
		},
	}
}

function requireRecord(records: MemoryRecord[], index: number): MemoryRecord {
	const record = records[index]
	expect(record).toBeDefined()
	return record as MemoryRecord
}

function createScriptedCleanupAdapter(namespaceId: string): Mem0MemoryAdapter {
	let nextRecord = 1
	const records = new Map<
		string,
		{
			id: string
			memory: string
			user_id?: string
			agent_id?: string
			run_id?: string
			metadata?: Record<string, unknown>
		}
	>()

	function matchesFilters(
		record: NonNullable<ReturnType<typeof records.get>>,
		filters: unknown,
	): boolean {
		if (filters === null || typeof filters !== 'object' || Array.isArray(filters)) {
			return true
		}

		const typedFilters = filters as Record<string, unknown>
		for (const scopeKey of ['user_id', 'agent_id', 'run_id'] as const) {
			if (
				typeof typedFilters[scopeKey] === 'string' &&
				record[scopeKey] !== typedFilters[scopeKey]
			) {
				return false
			}
		}

		return (
			typeof typedFilters[DENNETT_NAMESPACE_METADATA_KEY] !== 'string' ||
			record.metadata?.[DENNETT_NAMESPACE_METADATA_KEY] ===
				typedFilters[DENNETT_NAMESPACE_METADATA_KEY]
		)
	}

	const bridgeRunner: Mem0BridgeRunner = async (request) => {
		let result: unknown
		if (request.action === 'write') {
			const id = `scripted-memory-${nextRecord}`
			nextRecord += 1
			const record = {
				id,
				memory: String(request.content ?? ''),
				...(typeof request.user_id === 'string' ? { user_id: request.user_id } : {}),
				...(typeof request.agent_id === 'string' ? { agent_id: request.agent_id } : {}),
				...(typeof request.run_id === 'string' ? { run_id: request.run_id } : {}),
				...(request.metadata !== null &&
				typeof request.metadata === 'object' &&
				!Array.isArray(request.metadata)
					? { metadata: request.metadata as Record<string, unknown> }
					: {}),
			}
			records.set(id, record)
			result = { results: [record] }
		} else if (request.action === 'list') {
			const topK = typeof request.top_k === 'number' ? request.top_k : 20
			result = {
				results: [...records.values()]
					.filter((record) => matchesFilters(record, request.filters))
					.slice(0, topK),
			}
		} else if (request.action === 'read') {
			result =
				typeof request.memory_id === 'string' ? (records.get(request.memory_id) ?? null) : null
		} else if (request.action === 'delete') {
			if (typeof request.memory_id === 'string') {
				records.delete(request.memory_id)
			}
			result = { deleted: true }
		} else {
			throw new Error(`Unsupported scripted cleanup action: ${String(request.action)}`)
		}

		return {
			exit_code: 0,
			stderr: '',
			stdout: JSON.stringify({
				ok: true,
				result,
			}),
		}
	}

	return new Mem0MemoryAdapter({
		python_executable: process.execPath,
		mem0_config: {
			dennett_namespace_id: namespaceId,
			version: 'scripted-cleanup-test',
		},
		bridge_runner: bridgeRunner,
	})
}

describe('Mem0MemoryAdapter', () => {
	it('reports the provider shape and supported capabilities', async () => {
		const harness = await createAdapter('dennett-mem0-capabilities-')

		try {
			expect(harness.adapter.describeProvider()).toEqual({
				provider: 'mem0',
				display_name: 'Mem0',
				supported_capabilities: [
					'read',
					'write',
					'entity_scoped',
					'user_scoped',
					'session_scoped',
					'infer_extract',
				],
				supported_transports: ['sdk'],
				default_transport: 'sdk',
			})
			expect(
				harness.adapter.negotiate({
					required_capabilities: ['read', 'write'],
				}),
			).toEqual({
				ok: true,
				selected_transport: 'sdk',
				missing_capabilities: [],
				forbidden_transport_conflicts: [],
			})
		} finally {
			await harness.cleanup()
		}
	})

	it('performs real local Mem0 write, read, search, list, update, and delete operations', async () => {
		const harness = await createAdapter('dennett-mem0-live-', undefined, {
			acquireMem0Lock: true,
		})
		const scope = { user_id: 'phase13-live-user' }

		try {
			const writeResult = await harness.adapter.writeMemory({
				content: 'Phase 13 live memory record',
				scope,
				metadata: {
					source: 'vitest',
					stage: 13,
				},
				infer: false,
			})

			expect(writeResult.records).toHaveLength(1)
			const writtenRecord = requireRecord(writeResult.records, 0)
			const recordId = writtenRecord.id
			expect(recordId).toBeTruthy()
			expect(writtenRecord).toMatchObject({
				content: 'Phase 13 live memory record',
				scope: {
					user_id: 'phase13-live-user',
				},
				provider_data: {
					event: 'ADD',
					role: 'user',
					actor_id: null,
				},
			})

			const readBack = await harness.adapter.readMemory({
				memory_id: recordId ?? '',
			})
			expect(readBack).toMatchObject({
				id: recordId,
				content: 'Phase 13 live memory record',
				scope: {
					user_id: 'phase13-live-user',
				},
				metadata: {
					source: 'vitest',
					stage: 13,
				},
			})

			const searchResult = await harness.adapter.searchMemory({
				query: 'live memory record',
				scope,
				limit: 5,
			})
			expect(searchResult.records).toHaveLength(1)
			expect(searchResult.records[0]?.id).toBe(recordId)
			expect(searchResult.records[0]?.score).toEqual(expect.any(Number))

			const listed = await harness.adapter.listMemories({
				scope,
				limit: 10,
			})
			expect(listed).toHaveLength(1)
			expect(listed[0]?.id).toBe(recordId)

			const updated = await harness.adapter.updateMemory({
				memory_id: recordId ?? '',
				content: 'Phase 13 updated memory record',
				metadata: {
					source: 'vitest-updated',
				},
			})
			expect(updated).toMatchObject({
				id: recordId,
				content: 'Phase 13 updated memory record',
				scope: {
					user_id: 'phase13-live-user',
				},
				metadata: {
					source: 'vitest-updated',
				},
			})

			const deleted = await harness.adapter.deleteMemory({
				memory_id: recordId ?? '',
			})
			expect(deleted).toEqual({
				deleted: true,
			})

			await expect(
				harness.adapter.readMemory({
					memory_id: recordId ?? '',
				}),
			).resolves.toBeNull()
		} finally {
			await harness.cleanup()
		}
	}, 180000)

	it('injects namespace metadata, filters CRUD by namespace, and performs verified scoped cleanup', async () => {
		const harness = await createSharedNamespaceHarness('dennett-mem0-namespace-cleanup-')
		const scope = { user_id: 'phase13-namespace-user' }

		try {
			const targetOne = await harness.target.writeMemory({
				content: 'Phase 13 namespace cleanup target alpha',
				scope,
				metadata: {
					source: 'target-one',
					dennett_namespace_id: 'spoofed-by-request',
				},
				infer: false,
			})
			const targetWrongScope = await harness.target.writeMemory({
				content: 'Phase 13 namespace cleanup wrong scope beta',
				scope: { user_id: 'phase13-namespace-other-user' },
				metadata: {
					source: 'target-wrong-scope',
				},
				infer: false,
			})
			const control = await harness.control.writeMemory({
				content: 'Phase 13 namespace cleanup control gamma',
				scope,
				metadata: {
					source: 'control',
				},
				infer: false,
			})

			const targetOneId = requireRecord(targetOne.records, 0).id
			const targetWrongScopeId = requireRecord(targetWrongScope.records, 0).id
			const controlId = requireRecord(control.records, 0).id

			await expect(harness.target.readMemory({ memory_id: controlId })).resolves.toBeNull()
			await expect(
				harness.target.updateMemory({
					memory_id: controlId,
					content: 'Phase 13 namespace cleanup should not update control',
				}),
			).resolves.toBeNull()
			await expect(harness.target.deleteMemory({ memory_id: controlId })).resolves.toEqual({
				deleted: false,
			})

			const targetReadBack = await harness.target.readMemory({ memory_id: targetOneId })
			expect(targetReadBack).toMatchObject({
				id: targetOneId,
				metadata: {
					source: 'target-one',
					dennett_namespace_id: 'target-namespace',
				},
			})

			const targetList = await harness.target.listMemories({ scope, limit: 10 })
			expect(targetList.map((record) => record.id)).toEqual([targetOneId])
			expect(
				targetList.every((record) => record.metadata?.dennett_namespace_id === 'target-namespace'),
			).toBe(true)

			const targetSearch = await harness.target.searchMemory({
				query: 'namespace cleanup target',
				scope,
				limit: 10,
			})
			expect(targetSearch.records.map((record) => record.id)).toEqual([targetOneId])

			const preview = await harness.target.previewMemoryCleanup({ scope })
			expect(preview).toEqual({
				namespace_id: 'target-namespace',
				candidate_ids: [targetOneId],
				candidate_count: 1,
				limit: 10000,
				truncated: false,
			})

			const cleanup = await harness.target.deleteMemoryCleanup({
				scope,
				candidate_ids: [...preview.candidate_ids, targetWrongScopeId, controlId],
			})
			expect(cleanup).toEqual({
				namespace_id: 'target-namespace',
				limit: 10000,
				requested_ids: [...preview.candidate_ids, targetWrongScopeId, controlId],
				deleted_ids: [targetOneId],
				skipped_ids: [targetWrongScopeId, controlId],
				remaining_ids: [],
				requested_truncated: false,
				remaining_truncated: false,
				verified_empty: true,
			})

			await expect(harness.target.readMemory({ memory_id: targetOneId })).resolves.toBeNull()
			await expect(
				harness.target.readMemory({ memory_id: targetWrongScopeId }),
			).resolves.toMatchObject({
				id: targetWrongScopeId,
				content: 'Phase 13 namespace cleanup wrong scope beta',
				scope: {
					user_id: 'phase13-namespace-other-user',
				},
				metadata: {
					source: 'target-wrong-scope',
					dennett_namespace_id: 'target-namespace',
				},
			})

			const controlReadBack = await harness.control.readMemory({ memory_id: controlId })
			expect(controlReadBack).toMatchObject({
				id: controlId,
				content: 'Phase 13 namespace cleanup control gamma',
				metadata: {
					source: 'control',
					dennett_namespace_id: 'control-namespace',
				},
			})
			expect(await harness.control.listMemories({ scope, limit: 10 })).toHaveLength(1)
		} finally {
			await harness.cleanup()
		}
	}, 150000)

	it('reports cleanup bounds explicitly and rejects non-positive cleanup limits', async () => {
		const adapter = createScriptedCleanupAdapter('bounded-namespace')
		const scope = { user_id: 'phase13-cleanup-limit-user' }

		await expect(
			adapter.previewMemoryCleanup({
				scope,
				limit: 0,
			}),
		).rejects.toThrow(MemoryConfigurationError)
		await expect(
			adapter.deleteMemoryCleanup({
				scope,
				limit: -1,
			}),
		).rejects.toThrow(MemoryConfigurationError)

		const first = await adapter.writeMemory({
			content: 'Phase 13 cleanup limit first',
			scope,
			infer: false,
		})
		const second = await adapter.writeMemory({
			content: 'Phase 13 cleanup limit second',
			scope,
			infer: false,
		})
		const recordIds = [requireRecord(first.records, 0).id, requireRecord(second.records, 0).id]

		const preview = await adapter.previewMemoryCleanup({
			scope,
			limit: 1,
		})
		expect(preview).toMatchObject({
			namespace_id: 'bounded-namespace',
			candidate_count: 1,
			limit: 1,
			truncated: true,
		})
		expect(preview.candidate_ids).toHaveLength(1)
		expect(recordIds).toContain(preview.candidate_ids[0])

		const cleanup = await adapter.deleteMemoryCleanup({
			scope,
			limit: 1,
		})
		expect(cleanup).toMatchObject({
			namespace_id: 'bounded-namespace',
			limit: 1,
			skipped_ids: [],
			requested_truncated: true,
			remaining_truncated: false,
			verified_empty: false,
		})
		expect(cleanup.requested_ids).toHaveLength(1)
		expect(cleanup.deleted_ids).toHaveLength(1)
		expect(cleanup.remaining_ids).toHaveLength(1)
		expect(recordIds).toContain(cleanup.deleted_ids[0])
		expect(recordIds).toContain(cleanup.remaining_ids[0])
	})

	it('makes cleanup unavailable when no namespace is configured', async () => {
		const harness = await createAdapter('dennett-mem0-cleanup-without-namespace-')

		try {
			await expect(
				harness.adapter.previewMemoryCleanup({
					scope: { user_id: 'phase13-cleanup-without-namespace-user' },
				}),
			).rejects.toThrow(MemoryConfigurationError)
			await expect(
				harness.adapter.deleteMemoryCleanup({
					scope: { user_id: 'phase13-cleanup-without-namespace-user' },
				}),
			).rejects.toThrow(MemoryConfigurationError)
		} finally {
			await harness.cleanup()
		}
	})

	it('fails fast when the configured python executable does not exist', () => {
		expect(
			() =>
				new Mem0MemoryAdapter({
					python_executable: path.join(process.cwd(), '.local', 'missing-python.exe'),
					mem0_config: {
						vector_store: {
							provider: 'chroma',
							config: {
								path: 'unused',
								collection_name: 'unused',
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
						history_db_path: 'unused',
						version: 'v1.1',
					},
				}),
		).toThrow(MemoryConfigurationError)
	})

	it('rejects unsupported capability requests explicitly', async () => {
		const harness = await createAdapter('dennett-mem0-capability-error-')

		try {
			expect(
				harness.adapter.negotiate({
					required_capabilities: ['graph_context'],
				}),
			).toMatchObject({
				ok: false,
				missing_capabilities: ['graph_context'],
			})
			await expect(
				harness.adapter.searchMemory({
					query: 'anything',
					scope: {},
				}),
			).rejects.toThrow(MemoryConfigurationError)
			expect(
				harness.adapter.negotiate({
					required_capabilities: ['read'],
					transport_preferences: {
						forbid: ['sdk'],
					},
				}),
			).toMatchObject({
				ok: false,
				forbidden_transport_conflicts: ['sdk'],
			})
		} finally {
			await harness.cleanup()
		}
	})

	it('surfaces provider execution failures clearly for invalid delete operations', async () => {
		const harness = await createAdapter('dennett-mem0-provider-error-', undefined, {
			acquireMem0Lock: true,
		})

		try {
			await expect(
				harness.adapter.deleteMemory({
					memory_id: 'missing-memory-id',
				}),
			).rejects.toThrow(MemoryExecutionError)
		} finally {
			await harness.cleanup()
		}
	}, 15000)
})
