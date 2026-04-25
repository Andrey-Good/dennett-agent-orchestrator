import { readFile } from 'node:fs/promises'
import { describe, expect, it } from 'vitest'

type PackageJson = {
	private?: boolean
	engines?: {
		node?: string
	}
	files?: string[]
	scripts?: Record<string, string>
}

type PacklistValidator = {
	validatePacklistFiles(files: string[]): string[]
}

type ReleaseCandidateValidator = {
	validateReleaseCandidate(candidate: {
		candidateFiles: string[]
		untrackedFiles: string[]
	}): string[]
}

async function readPackageJson(): Promise<PackageJson> {
	return JSON.parse(await readFile('package.json', 'utf8')) as PackageJson
}

async function loadPacklistValidator(): Promise<PacklistValidator> {
	// @ts-expect-error The distribution validator is a Node ESM script, not TS source.
	return (await import('../../scripts/check-packlist.js')) as PacklistValidator
}

async function loadReleaseCandidateValidator(): Promise<ReleaseCandidateValidator> {
	// @ts-expect-error The release candidate validator is a Node ESM script, not TS source.
	return (await import('../../scripts/check-release-candidate.js')) as ReleaseCandidateValidator
}

describe('package distribution metadata', () => {
	it('keeps the package private with an explicit node:sqlite engine floor', async () => {
		const packageJson = await readPackageJson()

		expect(packageJson.private).toBe(true)
		expect(packageJson.engines?.node).toBe('>=22.13.0')
	})

	it('keeps package inventory constrained to built CLI and JSON schema contracts', async () => {
		const packageJson = await readPackageJson()

		expect(packageJson.files).toEqual(['dist/src/**', 'contracts/json-schema/*.schema.json'])
	})

	it('exposes stable local distribution validation scripts', async () => {
		const packageJson = await readPackageJson()

		expect(packageJson.scripts).toMatchObject({
			build: 'node scripts/clean-dist.js && tsc -p tsconfig.build.json',
			'dist:clean': 'node scripts/clean-dist.js',
			'dist:check': 'node scripts/check-distribution.js',
			'packlist:check': 'node scripts/check-packlist.js',
			'release-candidate:check': 'node scripts/check-release-candidate.js',
			'package:check': 'pnpm build && pnpm dist:check && pnpm packlist:check',
		})
	})

	it('rejects missing required files and forbidden package inventory entries', async () => {
		const { validatePacklistFiles } = await loadPacklistValidator()

		expect(
			validatePacklistFiles([
				'package.json',
				'README.md',
				'LICENSE',
				'contracts/json-schema/agent-file.schema.json',
				'contracts/json-schema/agent-json.defs.schema.json',
				'tests/unit/leaked.test.ts',
				'dist/vitest.config.js',
			]),
		).toEqual([
			'Package inventory is missing required file: dist/src/interfaces/cli.js',
			'Package inventory contains non-allowlisted file: tests/unit/leaked.test.ts',
			'Package inventory contains forbidden file: tests/unit/leaked.test.ts',
			'Package inventory contains non-allowlisted file: dist/vitest.config.js',
			'Package inventory contains forbidden file: dist/vitest.config.js',
		])
	})

	it('rejects forbidden git candidate and staging hazard artifacts', async () => {
		const { validateReleaseCandidate } = await loadReleaseCandidateValidator()
		const requiredCandidateFiles = [
			'.github/workflows/ci.yml',
			'.gitignore',
			'AGENTS.md',
			'README.md',
			'agent_orchestrator_final_spec_v2.md',
			'biome.json',
			'package.json',
			'pnpm-lock.yaml',
			'tsconfig.build.json',
			'tsconfig.json',
			'vitest.config.ts',
			'contracts/invariants/README.md',
			'contracts/json-schema/agent-file.schema.json',
			'contracts/typescript/agent-file.ts',
			'docs/11-hardening/release-gates.md',
			'examples/agents/README.md',
			'scripts/check-distribution.js',
			'scripts/check-packlist.js',
			'scripts/check-release-candidate.js',
			'scripts/clean-dist.js',
			'src/core/agent-file.ts',
			'tests/unit/distribution-package.test.ts',
		]

		expect(
			validateReleaseCandidate({
				candidateFiles: [
					...requiredCandidateFiles,
					'dist/src/interfaces/cli.js',
					'contracts/typescript/agent-file.js',
				],
				untrackedFiles: [
					'subagent_tasks/TASK-466-git-hygiene-guard-worker.md',
					'.local/run.sqlite',
				],
			}),
		).toEqual([
			'Release candidate includes forbidden tracked/staged path (stale generated TypeScript contract JavaScript): contracts/typescript/agent-file.js',
			'Release candidate includes forbidden tracked/staged path (dist build output): dist/src/interfaces/cli.js',
			'Forbidden untracked artifact is still visible to git status (local state): .local/run.sqlite',
			'Forbidden untracked artifact is still visible to git status (subagent_tasks orchestration scratch documents): subagent_tasks/TASK-466-git-hygiene-guard-worker.md',
		])
	})

	it('requires product paths to be tracked or staged instead of untracked locally', async () => {
		const { validateReleaseCandidate } = await loadReleaseCandidateValidator()

		expect(
			validateReleaseCandidate({
				candidateFiles: ['LICENSE'],
				untrackedFiles: ['src/core/agent-file.ts'],
			}),
		).toContain(
			'Product path is visible but not tracked or staged (src/**): src/core/agent-file.ts',
		)
	})
})
