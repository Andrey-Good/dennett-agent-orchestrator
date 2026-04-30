import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { describe, expect, it } from 'vitest'

type ArchitectureBoundaryException = {
	ruleId: string
	importer: string
	imported: string
	reason: string
}

type ArchitectureBoundaryViolation = {
	ruleId: string
	importer: string
	imported: string
	line: number
	message: string
	specifier: string
}

type ArchitectureBoundaryCheckResult = {
	allowlistedViolations: ArchitectureBoundaryViolation[]
	scannedFiles: string[]
	staleAllowlistEntries: ArchitectureBoundaryException[]
	violations: ArchitectureBoundaryViolation[]
}

type ArchitectureBoundaryChecker = {
	checkArchitectureBoundaries(options?: {
		allowlist?: ArchitectureBoundaryException[]
		rootDir?: string
		sourceRoot?: string
	}): Promise<ArchitectureBoundaryCheckResult>
}

async function loadArchitectureBoundaryChecker(): Promise<ArchitectureBoundaryChecker> {
	// @ts-expect-error The architecture boundary checker is a Node ESM script, not TS source.
	return (await import('../../scripts/check-architecture-boundaries.js')) as ArchitectureBoundaryChecker
}

async function withFixture(
	files: Record<string, string>,
	run: (rootDir: string) => Promise<void>,
): Promise<void> {
	const rootDir = await mkdtemp(path.join(os.tmpdir(), 'dennett-architecture-boundaries-'))
	try {
		for (const [filePath, content] of Object.entries(files)) {
			const absolutePath = path.join(rootDir, filePath)
			await mkdir(path.dirname(absolutePath), { recursive: true })
			await writeFile(absolutePath, content, 'utf8')
		}
		await run(rootDir)
	} finally {
		await rm(rootDir, { force: true, recursive: true })
	}
}

describe('architecture boundary checker', () => {
	it('fails on unknown core-to-interface imports', async () => {
		const checker = await loadArchitectureBoundaryChecker()

		await withFixture(
			{
				'src/core/domain-service.ts': "import { buildCliProgram } from '../interfaces/cli.js'\n",
				'src/interfaces/cli.ts': 'export function buildCliProgram() {}\n',
			},
			async (rootDir) => {
				const result = await checker.checkArchitectureBoundaries({ allowlist: [], rootDir })

				expect(result.violations).toEqual([
					expect.objectContaining({
						imported: 'src/interfaces/cli.ts',
						importer: 'src/core/domain-service.ts',
						line: 1,
						ruleId: 'core-must-not-import-interfaces',
						specifier: '../interfaces/cli.js',
					}),
				])
			},
		)
	})

	it('allows explicitly documented current exceptions', async () => {
		const checker = await loadArchitectureBoundaryChecker()
		const allowlist = [
			{
				imported: 'src/adapters/codex/runtime-adapter.ts',
				importer: 'src/core/builder-service.ts',
				reason: 'Fixture baseline exception.',
				ruleId: 'core-must-not-import-adapters',
			},
		]

		await withFixture(
			{
				'src/adapters/codex/runtime-adapter.ts': 'export class RuntimeAdapter {}\n',
				'src/core/builder-service.ts':
					"import { RuntimeAdapter } from '../adapters/codex/runtime-adapter.js'\n",
			},
			async (rootDir) => {
				const result = await checker.checkArchitectureBoundaries({ allowlist, rootDir })

				expect(result.violations).toEqual([])
				expect(result.staleAllowlistEntries).toEqual([])
				expect(result.allowlistedViolations).toEqual([
					expect.objectContaining({
						imported: 'src/adapters/codex/runtime-adapter.ts',
						importer: 'src/core/builder-service.ts',
						ruleId: 'core-must-not-import-adapters',
					}),
				])
			},
		)
	})

	it('rejects stale allowlist entries', async () => {
		const checker = await loadArchitectureBoundaryChecker()
		const allowlist = [
			{
				imported: 'src/interfaces/cli.ts',
				importer: 'src/core/domain-service.ts',
				reason: 'Fixture stale exception.',
				ruleId: 'core-must-not-import-interfaces',
			},
		]

		await withFixture(
			{
				'src/core/domain-service.ts': 'export function runDomainService() {}\n',
				'src/interfaces/cli.ts': 'export function buildCliProgram() {}\n',
			},
			async (rootDir) => {
				const result = await checker.checkArchitectureBoundaries({ allowlist, rootDir })

				expect(result.violations).toEqual([])
				expect(result.staleAllowlistEntries).toEqual(allowlist)
			},
		)
	})

	it('fails on concrete technology imports outside adapters and interface startup', async () => {
		const checker = await loadArchitectureBoundaryChecker()

		await withFixture(
			{
				'src/core/runtime-client.ts': "import OpenAI from 'openai'\n",
			},
			async (rootDir) => {
				const result = await checker.checkArchitectureBoundaries({ allowlist: [], rootDir })

				expect(result.violations).toEqual([
					expect.objectContaining({
						imported: 'openai',
						importer: 'src/core/runtime-client.ts',
						line: 1,
						ruleId: 'concrete-technology-imports-stay-in-adapters-or-interface-startup',
						specifier: 'openai',
					}),
				])
			},
		)
	})

	it('fails on unresolved imports that cross forbidden architecture boundaries', async () => {
		const checker = await loadArchitectureBoundaryChecker()

		await withFixture(
			{
				'src/core/domain-service.ts': "import { buildCliProgram } from '../interfaces/missing-cli.js'\n",
			},
			async (rootDir) => {
				const result = await checker.checkArchitectureBoundaries({ allowlist: [], rootDir })

				expect(result.violations).toEqual([
					expect.objectContaining({
						imported: 'src/interfaces/missing-cli.ts',
						importer: 'src/core/domain-service.ts',
						line: 1,
						ruleId: 'core-must-not-import-interfaces',
						specifier: '../interfaces/missing-cli.js',
					}),
				])
			},
		)
	})
})
