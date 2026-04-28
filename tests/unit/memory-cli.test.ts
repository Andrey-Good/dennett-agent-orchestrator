import { mkdtemp, rm } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { AppError } from '../../src/core/errors.js'
import { MemoryService } from '../../src/core/memory-service.js'
import {
	buildCliProgram,
	deleteRegisteredMemory,
	deleteRegisteredMemoryCleanup,
	listRegisteredMemory,
	previewRegisteredMemoryCleanup,
	readRegisteredMemory,
	registerMemoryProvider,
	searchRegisteredMemory,
	showRegisteredMemoryProvider,
	updateRegisteredMemory,
	writeRegisteredMemory,
} from '../../src/interfaces/cli.js'
import type { MemoryScope } from '../../src/ports/memory.js'
import { MEM0_LOCAL_PYTHON, shouldRunLocalMem0Tests } from './mem0-test-helpers.js'

const tempDirsToRemove: string[] = []
const localMem0It = shouldRunLocalMem0Tests() ? it : it.skip

async function createHarness(prefix: string): Promise<{
	stateDbPath: string
	scope: MemoryScope
}> {
	const tempDir = await mkdtemp(path.join(os.tmpdir(), prefix))
	tempDirsToRemove.push(tempDir)

	const stateDbPath = path.join(tempDir, 'local-state.sqlite')
	await registerMemoryProvider(
		{
			providerId: 'mem0-local',
			codexRef: 'primary_memory',
			providerFamily: 'mem0',
			displayName: 'Primary Mem0',
			config: {
				python_executable: MEM0_LOCAL_PYTHON,
				working_directory: process.cwd(),
				mem0_config: {
					vector_store: {
						provider: 'chroma',
						config: {
							path: path.join(tempDir, 'chroma'),
							collection_name: `phase13-cli-${path.basename(tempDir)}`,
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
				bridge_timeout_ms: 120000,
			},
		},
		stateDbPath,
	)

	return {
		stateDbPath,
		scope: {
			user_id: 'cli-user',
		},
	}
}

async function createStatePath(prefix: string): Promise<string> {
	const tempDir = await mkdtemp(path.join(os.tmpdir(), prefix))
	tempDirsToRemove.push(tempDir)
	return path.join(tempDir, 'local-state.sqlite')
}

afterEach(async () => {
	vi.restoreAllMocks()
	while (tempDirsToRemove.length > 0) {
		const tempDir = tempDirsToRemove.pop()
		if (tempDir) {
			await rm(tempDir, { recursive: true, force: true })
		}
	}
})

describe('memory CLI helpers', () => {
	it('rejects cleanup preview without explicit scope before calling MemoryService', async () => {
		const stateDbPath = await createStatePath('dennett-task355-memory-cli-unscoped-preview-')
		const preview = vi.spyOn(MemoryService.prototype, 'previewMemoryCleanupForCodexRef')

		await expect(
			previewRegisteredMemoryCleanup('primary_memory', { scope: {} }, stateDbPath),
		).rejects.toMatchObject({
			code: 'MEMORY_CLEANUP_SCOPE_REQUIRED',
			message:
				'Memory cleanup requires at least one explicit scope option: --user-id, --agent-id, or --run-id.',
		})
		expect(preview).not.toHaveBeenCalled()
	})

	it('returns cleanup preview safety fields and a confirmation token', async () => {
		const stateDbPath = await createStatePath('dennett-task355-memory-cli-preview-')
		const scope = { user_id: 'cli-user' }
		const preview = vi
			.spyOn(MemoryService.prototype, 'previewMemoryCleanupForCodexRef')
			.mockResolvedValue({
				namespace_id: 'namespace-a',
				candidate_ids: ['memory-a', 'memory-b'],
				candidate_count: 2,
				limit: 25,
				truncated: false,
			})

		const result = await previewRegisteredMemoryCleanup(
			'primary_memory',
			{
				scope,
				limit: 25,
			},
			stateDbPath,
		)

		expect(preview).toHaveBeenCalledWith('primary_memory', {
			scope,
			limit: 25,
		})
		expect(result).toEqual({
			namespace_id: 'namespace-a',
			scope,
			candidate_count: 2,
			candidate_ids: ['memory-a', 'memory-b'],
			limit: 25,
			truncated: false,
			confirmation_token: expect.stringMatching(/^cleanup:[a-f0-9]{24}$/),
			verification: {
				status: 'preview_only',
				required_command: 'memory-cleanup-verified-delete',
			},
		})
	})

	it('requires matching confirmation before verified cleanup delete', async () => {
		const stateDbPath = await createStatePath('dennett-task355-memory-cli-token-mismatch-')
		const scope = { user_id: 'cli-user' }
		vi.spyOn(MemoryService.prototype, 'previewMemoryCleanupForCodexRef').mockResolvedValue({
			namespace_id: 'namespace-a',
			candidate_ids: ['memory-a'],
			candidate_count: 1,
			limit: 10,
			truncated: false,
		})
		const cleanup = vi.spyOn(MemoryService.prototype, 'deleteMemoryCleanupForCodexRef')

		await expect(
			deleteRegisteredMemoryCleanup(
				'primary_memory',
				{
					scope,
					confirmationToken: 'cleanup:not-the-preview-token',
					limit: 10,
				},
				stateDbPath,
			),
		).rejects.toMatchObject({
			code: 'MEMORY_CLEANUP_CONFIRMATION_MISMATCH',
			message:
				'Confirmation token does not match the current cleanup preview for the requested scope.',
		})
		expect(cleanup).not.toHaveBeenCalled()
	})

	it('passes preview candidate IDs through verified cleanup delete', async () => {
		const stateDbPath = await createStatePath('dennett-task355-memory-cli-delete-')
		const scope = { user_id: 'cli-user', agent_id: 'agent-a' }
		const preview = vi
			.spyOn(MemoryService.prototype, 'previewMemoryCleanupForCodexRef')
			.mockResolvedValue({
				namespace_id: 'namespace-a',
				candidate_ids: ['memory-a'],
				candidate_count: 1,
				limit: 10,
				truncated: false,
			})
		const cleanup = vi
			.spyOn(MemoryService.prototype, 'deleteMemoryCleanupForCodexRef')
			.mockResolvedValue({
				namespace_id: 'namespace-a',
				limit: 10,
				requested_ids: ['memory-a'],
				deleted_ids: ['memory-a'],
				skipped_ids: [],
				remaining_ids: [],
				requested_truncated: false,
				remaining_truncated: false,
				verified_empty: true,
			})

		const previewResult = await previewRegisteredMemoryCleanup(
			'primary_memory',
			{
				scope,
				limit: 10,
			},
			stateDbPath,
		)
		const result = await deleteRegisteredMemoryCleanup(
			'primary_memory',
			{
				scope,
				confirmationToken: previewResult.confirmation_token,
				limit: 10,
			},
			stateDbPath,
		)

		expect(preview).toHaveBeenCalledTimes(2)
		expect(cleanup).toHaveBeenCalledWith('primary_memory', {
			scope,
			candidate_ids: ['memory-a'],
			limit: 10,
		})
		expect(result).toEqual({
			namespace_id: 'namespace-a',
			scope,
			candidate_count: 1,
			candidate_ids: ['memory-a'],
			preview_truncated: false,
			limit: 10,
			requested_ids: ['memory-a'],
			deleted_ids: ['memory-a'],
			skipped_ids: [],
			remaining_ids: [],
			requested_truncated: false,
			remaining_truncated: false,
			verified_empty: true,
			verification: {
				status: 'verified_empty',
				confirmation_token: previewResult.confirmation_token,
			},
		})
	})

	it('surfaces MemoryService cleanup errors without swallowing them', async () => {
		const stateDbPath = await createStatePath('dennett-task355-memory-cli-service-error-')
		vi.spyOn(MemoryService.prototype, 'previewMemoryCleanupForCodexRef').mockRejectedValue(
			new AppError(
				'MEMORY_PROVIDER_CLEANUP_UNSUPPORTED',
				'Memory provider "mem0-local" does not expose adapter cleanup method "previewMemoryCleanup".',
			),
		)

		await expect(
			previewRegisteredMemoryCleanup(
				'primary_memory',
				{
					scope: { user_id: 'cli-user' },
				},
				stateDbPath,
			),
		).rejects.toMatchObject({
			code: 'MEMORY_PROVIDER_CLEANUP_UNSUPPORTED',
			message:
				'Memory provider "mem0-local" does not expose adapter cleanup method "previewMemoryCleanup".',
		})
	})

	it('requires a confirmation token for the verified delete command', () => {
		const program = buildCliProgram()
		const command = program.commands.find(
			(entry) => entry.name() === 'memory-cleanup-verified-delete',
		)

		expect(command).toBeDefined()
		expect(
			command?.options.some(
				(option) => option.flags === '--confirm-token <token>' && option.required === true,
			),
		).toBe(true)
	})

	it('redacts provider config from default memory-provider CLI output', async () => {
		const stateDbPath = await createStatePath('dennett-task534-memory-cli-redaction-')
		const rawConfig = {
			python_executable: 'C:/Users/Alice/private/python.exe',
			api_key: 'super-secret-token',
			mem0_config: {
				vector_store: {
					provider: 'chroma',
					config: {
						path: 'C:/Users/Alice/private/chroma',
					},
				},
			},
		}
		let stdout = ''
		const stdoutSpy = vi.spyOn(process.stdout, 'write').mockImplementation((chunk) => {
			stdout += String(chunk)
			return true
		})

		try {
			const program = buildCliProgram()
			program.exitOverride()

			await program.parseAsync(
				[
					'memory-provider-register',
					'mem0-local',
					'--family',
					'mem0',
					'--codex-ref',
					'primary_memory',
					'--display-name',
					'Primary Mem0',
					'--config',
					JSON.stringify(rawConfig),
					'--state-db',
					stateDbPath,
				],
				{ from: 'user' },
			)
			await program.parseAsync(['memory-provider-show', 'mem0-local', '--state-db', stateDbPath], {
				from: 'user',
			})
			await program.parseAsync(['memory-provider-list', '--state-db', stateDbPath], {
				from: 'user',
			})
		} finally {
			stdoutSpy.mockRestore()
		}

		expect(stdout).not.toContain('super-secret-token')
		expect(stdout).not.toContain('C:/Users/Alice/private/python.exe')
		expect(stdout).not.toContain('C:/Users/Alice/private/chroma')
		expect(stdout).not.toContain('api_key')
		expect(stdout).toContain('"redacted": true')
		expect(stdout).toContain(
			'Provider configuration is local/private and omitted from default CLI output.',
		)

		const rawShown = await showRegisteredMemoryProvider('mem0-local', stateDbPath)
		expect(rawShown.config).toEqual(rawConfig)
	})

	localMem0It(
		'performs a full registered Mem0 round-trip through the CLI helper surface',
		async () => {
			const harness = await createHarness('dennett-phase13-memory-cli-')

			const writeResult = await writeRegisteredMemory(
				'primary_memory',
				{
					text: 'Phase 13 CLI helper memory',
					scope: harness.scope,
					metadata: {
						source: 'cli-helper',
					},
					infer: false,
				},
				harness.stateDbPath,
			)

			expect(writeResult.records).toHaveLength(1)
			const memoryId = writeResult.records[0]?.id ?? ''
			expect(memoryId).not.toBe('')

			const searched = await searchRegisteredMemory(
				'primary_memory',
				{
					query: 'CLI helper memory',
					scope: harness.scope,
					limit: 5,
				},
				harness.stateDbPath,
			)
			expect(searched.records).toHaveLength(1)
			expect(searched.records[0]?.id).toBe(memoryId)

			const readBack = await readRegisteredMemory('primary_memory', memoryId, harness.stateDbPath)
			expect(readBack).toMatchObject({
				id: memoryId,
				content: 'Phase 13 CLI helper memory',
				scope: {
					user_id: 'cli-user',
				},
			})

			const listed = await listRegisteredMemory(
				'primary_memory',
				{
					scope: harness.scope,
					limit: 10,
				},
				harness.stateDbPath,
			)
			expect(listed).toHaveLength(1)
			expect(listed[0]?.id).toBe(memoryId)

			const updated = await updateRegisteredMemory(
				'primary_memory',
				{
					memoryId,
					text: 'Phase 13 CLI helper memory updated',
					metadata: {
						source: 'cli-helper-updated',
					},
				},
				harness.stateDbPath,
			)
			expect(updated).toMatchObject({
				id: memoryId,
				content: 'Phase 13 CLI helper memory updated',
				metadata: {
					source: 'cli-helper-updated',
				},
			})

			const deleted = await deleteRegisteredMemory('primary_memory', memoryId, harness.stateDbPath)
			expect(deleted).toEqual({
				deleted: true,
			})

			await expect(
				readRegisteredMemory('primary_memory', memoryId, harness.stateDbPath),
			).resolves.toBeNull()
		},
		120000,
	)
})
