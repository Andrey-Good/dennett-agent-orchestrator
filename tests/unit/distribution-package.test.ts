import { readFile } from 'node:fs/promises'
import path from 'node:path'
import { describe, expect, it } from 'vitest'
import type { PersistedRunSnapshot } from '../../src/core/state/types.js'
import {
	buildCliProgram,
	buildRunInteractionStatus,
	getCliCommandContracts,
} from '../../src/interfaces/cli.js'

type PackageJson = {
	description?: string
	version?: string
	private?: boolean
	repository?: {
		type?: string
		url?: string
	}
	bugs?: {
		url?: string
	}
	homepage?: string
	keywords?: string[]
	engines?: {
		node?: string
	}
	exports?: Record<string, string>
	files?: string[]
	scripts?: Record<string, string>
}

type PacklistValidator = {
	validatePackageMetadata(packageJson: PackageJson): string[]
	validatePacklistFiles(files: string[]): string[]
}

type ReleaseCandidateValidator = {
	validateReleaseCandidate(candidate: {
		candidateFiles: string[]
		untrackedFiles: string[]
	}): string[]
}

type DistributionValidator = {
	getSupplyChainDeferrals(): string[]
	parseLocalInstallProofArgs(argv: string[]): { keepTemp: boolean }
	parseLocalSbomProofArgs(
		argv: string[],
		cwd?: string,
	): { fromTgz: string | undefined; output: string | undefined; keepTemp: boolean }
	parseUpgradeRollbackProofArgs(
		argv: string[],
		cwd?: string,
	): { fromTgz: string; toTgz: string; keepTemp: boolean }
	validateSbomDocument(sbomDocument: unknown): string[]
}

type PublicReleaseFoundationValidator = {
	collectOssLaunchBlockers(input: {
		externalBetaDoc: string
		finalGateDoc: string
		packageIdentityDoc: string
		packageJson: PackageJson
		supplyChainDoc: string
	}): Array<{ label: string; blockers: string[] }>
	hasMissingPublicArtifactProof(
		launchBlockers: Array<{ label: string; blockers: string[] }>,
	): boolean
	validatePublicClaims(input: {
		documents: Array<{ filePath: string; text: string }>
		externalBetaNotRun: boolean
		hostedOrProductionNotApproved: boolean
		missingPublicArtifactProof: boolean
		ossLaunchNotApproved?: boolean
		packagePrivate?: boolean
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

async function loadDistributionValidator(): Promise<DistributionValidator> {
	// @ts-expect-error The distribution validator is a Node ESM script, not TS source.
	return (await import('../../scripts/check-distribution.js')) as DistributionValidator
}

async function loadPublicReleaseFoundationValidator(): Promise<PublicReleaseFoundationValidator> {
	// @ts-expect-error The public release validator is a Node ESM script, not TS source.
	const validator = await import('../../scripts/check-public-release-foundation.js')

	return validator as PublicReleaseFoundationValidator
}

describe('package distribution metadata', () => {
	it('keeps the package private with an explicit prepared release-candidate version', async () => {
		const packageJson = await readPackageJson()

		expect(packageJson.private).toBe(true)
		expect(packageJson.version).toBe('0.1.0-rc.1')
		expect(packageJson.engines?.node).toBe('>=22.13.0')
	})

	it('sets public-ready package metadata while retaining private publication blocking', async () => {
		const packageJson = await readPackageJson()

		expect(packageJson.description).toBe('Codex-first orchestrator for portable agent runs.')
		expect(packageJson.repository).toEqual({
			type: 'git',
			url: 'https://github.com/Andrey-Good/dennett-agent-orchestrator',
		})
		expect(packageJson.bugs).toEqual({
			url: 'https://github.com/Andrey-Good/dennett-agent-orchestrator/issues',
		})
		expect(packageJson.homepage).toBe(
			'https://github.com/Andrey-Good/dennett-agent-orchestrator#readme',
		)
		expect(packageJson.keywords).toEqual([
			'agent-orchestration',
			'agent-runtime',
			'codex',
			'cli',
			'workflow',
		])
	})

	it('keeps package inventory constrained to built CLI and JSON schema contracts', async () => {
		const packageJson = await readPackageJson()

		expect(packageJson.files).toEqual(['dist/src/**', 'contracts/json-schema/*.schema.json'])
	})

	it('keeps public-readiness baseline docs aligned with the private release candidate version', async () => {
		const packageJson = await readPackageJson()
		const baselineGapDoc = await readFile(
			'docs/21-public-launch-readiness/baseline-gap-and-forbidden-claims.md',
			'utf8',
		)
		const finalGateDoc = await readFile(
			'docs/21-public-launch-readiness/final-public-launch-gate-decision.md',
			'utf8',
		)

		expect(packageJson.private).toBe(true)
		expect(baselineGapDoc).toContain(
			'version is the prepared private release candidate `0.1.0-rc.1`',
		)
		expect(baselineGapDoc).not.toContain('version is `0.0.0`')
		expect(finalGateDoc).toContain(`Package version: \`${packageJson.version}\``)
	})

	it('exports only package metadata and public schema files', async () => {
		const packageJson = await readPackageJson()

		expect(packageJson.exports).toEqual({
			'./package.json': './package.json',
			'./contracts/json-schema/*.schema.json': './contracts/json-schema/*.schema.json',
		})
		expect(Object.hasOwn(packageJson.exports ?? {}, '.')).toBe(false)
		expect(
			Object.values(packageJson.exports ?? {}).some((exportTarget) =>
				exportTarget.startsWith('./dist/src/'),
			),
		).toBe(false)
	})

	it('exposes stable local distribution validation scripts', async () => {
		const packageJson = await readPackageJson()

		expect(packageJson.scripts).toMatchObject({
			build: 'node scripts/clean-dist.js && tsc -p tsconfig.build.json',
			dennett: 'node ./dist/src/interfaces/cli.js',
			'dist:clean': 'node scripts/clean-dist.js',
			'dist:check': 'node scripts/check-distribution.js',
			'packlist:check': 'node scripts/check-packlist.js',
			'release-candidate:check': 'node scripts/check-release-candidate.js',
			'public-release-foundation:check': 'node scripts/check-public-release-foundation.js',
			'package:local-install:proof': 'node scripts/check-distribution.js local-install-proof',
			'package:upgrade-rollback:proof': 'node scripts/check-distribution.js upgrade-rollback-proof',
			'supply-chain:local:proof': 'node scripts/check-distribution.js local-sbom-proof',
			'stage15:integrated-flow:proof': 'node scripts/check-stage15-integrated-flow.js',
			'test:mem0': 'node scripts/run-mem0-tests.js',
			'package:check':
				'pnpm build && pnpm dist:check && pnpm packlist:check && pnpm public-release-foundation:check',
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

	it('rejects package metadata when public routing and discovery fields drift', async () => {
		const { validatePackageMetadata } = await loadPacklistValidator()
		const packageJson = await readPackageJson()

		expect(
			validatePackageMetadata({
				...packageJson,
				version: '0.0.0',
				bugs: { url: 'https://example.invalid/issues' },
				homepage: 'https://example.invalid',
				keywords: ['agent'],
				scripts: { ...(packageJson.scripts ?? {}), dennett: 'node dist/src/interfaces/cli.js' },
			}),
		).toEqual([
			'package.json version must be 0.1.0-rc.1 for the prepared private release candidate.',
			'package.json bugs must be {"url":"https://github.com/Andrey-Good/dennett-agent-orchestrator/issues"} for public issue routing metadata.',
			'package.json homepage must be https://github.com/Andrey-Good/dennett-agent-orchestrator#readme.',
			'package.json keywords must be ["agent-orchestration","agent-runtime","codex","cli","workflow"] for public package discovery metadata.',
			'package.json script "dennett" must be "node ./dist/src/interfaces/cli.js" for source-checkout CLI usage.',
		])
	})

	it('rejects forbidden git candidate and staging hazard artifacts', async () => {
		const { validateReleaseCandidate } = await loadReleaseCandidateValidator()
		const requiredCandidateFiles = [
			'.github/workflows/ci.yml',
			'.github/workflows/release.yml',
			'.gitignore',
			'AGENTS.md',
			'README.md',
			'SECURITY.md',
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
			'docs/21-public-launch-readiness/baseline-gap-and-forbidden-claims.md',
			'docs/21-public-launch-readiness/final-public-launch-gate-decision.md',
			'docs/21-public-launch-readiness/public-docs-onboarding-and-claims.md',
			'docs/11-hardening/release-gates.md',
			'examples/agents/README.md',
			'scripts/check-distribution.js',
			'scripts/check-packlist.js',
			'scripts/check-public-release-foundation.js',
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

	it('validates local package install proof arguments without accepting fake artifacts', async () => {
		const { parseLocalInstallProofArgs } = await loadDistributionValidator()

		expect(parseLocalInstallProofArgs([])).toEqual({ keepTemp: false })
		expect(parseLocalInstallProofArgs(['--keep-temp'])).toEqual({ keepTemp: true })
		expect(() => parseLocalInstallProofArgs(['--from-tgz', 'old.tgz'])).toThrow(
			'Unknown argument for local install proof: --from-tgz',
		)
	})

	it('requires explicit old and new tarballs for upgrade and rollback proof', async () => {
		const { parseUpgradeRollbackProofArgs } = await loadDistributionValidator()
		const proofCwd = path.resolve('proof-root')

		expect(() => parseUpgradeRollbackProofArgs([])).toThrow(
			'Upgrade/rollback proof requires --from-tgz <path> and --to-tgz <path>; no previous artifact means rollback proof is not available.',
		)
		expect(() => parseUpgradeRollbackProofArgs(['--from-tgz'])).toThrow(
			'Missing value for --from-tgz.',
		)
		expect(() =>
			parseUpgradeRollbackProofArgs(['--from-tgz', 'same.tgz', '--to-tgz', 'same.tgz'], proofCwd),
		).toThrow('Upgrade/rollback proof requires distinct --from-tgz and --to-tgz artifacts.')
		expect(
			parseUpgradeRollbackProofArgs(
				['--from-tgz', 'old.tgz', '--to-tgz', 'new.tgz', '--keep-temp'],
				proofCwd,
			),
		).toEqual({
			fromTgz: path.join(proofCwd, 'old.tgz'),
			toTgz: path.join(proofCwd, 'new.tgz'),
			keepTemp: true,
		})
	})

	it('keeps SBOM proof local while recording provenance and signing deferrals', async () => {
		const { getSupplyChainDeferrals, parseLocalSbomProofArgs, validateSbomDocument } =
			await loadDistributionValidator()
		const proofCwd = path.resolve('proof-root')

		expect(parseLocalSbomProofArgs(['--from-tgz', 'candidate.tgz'], proofCwd)).toEqual({
			fromTgz: path.join(proofCwd, 'candidate.tgz'),
			output: undefined,
			keepTemp: false,
		})
		expect(
			parseLocalSbomProofArgs(
				['--from-tgz', 'candidate.tgz', '--output', 'candidate.spdx.json', '--keep-temp'],
				proofCwd,
			),
		).toEqual({
			fromTgz: path.join(proofCwd, 'candidate.tgz'),
			output: path.join(proofCwd, 'candidate.spdx.json'),
			keepTemp: true,
		})
		expect(
			validateSbomDocument({
				spdxVersion: 'SPDX-2.3',
				name: 'dennett-agent-orchestrator',
				packages: [{ name: 'dennett-agent-orchestrator' }],
			}),
		).toEqual([])
		expect(validateSbomDocument({ name: 'other' })).toEqual([
			'SBOM output must include an SPDX version.',
			'SBOM output must include package entries.',
		])
		expect(getSupplyChainDeferrals()).toEqual([
			'npm provenance is deferred because package publication is blocked by private: true and no npm publish command may run in this stage.',
			'Package signing is deferred because no local signing identity or publication signing infrastructure is configured in this stage.',
		])
	})

	it('rejects public launch claims while the package remains private', async () => {
		const { validatePublicClaims } = await loadPublicReleaseFoundationValidator()

		expect(
			validatePublicClaims({
				documents: [
					{
						filePath: 'docs/claim.md',
						text: 'Public launch is approved for general availability.',
					},
				],
				externalBetaNotRun: false,
				hostedOrProductionNotApproved: false,
				missingPublicArtifactProof: false,
				ossLaunchNotApproved: true,
			}),
		).toEqual([
			'docs/claim.md:1 claims public launch, GA, or production approval claim while OSS v0.1 public launch is not approved: Public launch is approved for general availability.',
		])
	})

	it('rejects completed external beta claims while Stage 16 is not-run', async () => {
		const { validatePublicClaims } = await loadPublicReleaseFoundationValidator()

		expect(
			validatePublicClaims({
				documents: [
					{
						filePath: 'docs/beta.md',
						text: 'External beta has been completed.',
					},
				],
				externalBetaNotRun: true,
				hostedOrProductionNotApproved: false,
				missingPublicArtifactProof: false,
				ossLaunchNotApproved: false,
			}),
		).toEqual([
			'docs/beta.md:1 claims external beta completion claim while external beta evidence is not-run: External beta has been completed.',
		])
	})

	it('rejects public provenance SBOM and registry claims without proof', async () => {
		const { validatePublicClaims } = await loadPublicReleaseFoundationValidator()

		expect(
			validatePublicClaims({
				documents: [
					{
						filePath: 'docs/supply.md',
						text: [
							'npm provenance is available.',
							'SBOM retention is complete.',
							'public registry install is supported.',
						].join('\n'),
					},
				],
				externalBetaNotRun: false,
				hostedOrProductionNotApproved: false,
				missingPublicArtifactProof: true,
				ossLaunchNotApproved: false,
			}),
		).toEqual([
			'docs/supply.md:1 claims public provenance claim while public provenance/SBOM/registry proof is missing: npm provenance is available.',
			'docs/supply.md:2 claims retained or public SBOM claim while public provenance/SBOM/registry proof is missing: SBOM retention is complete.',
			'docs/supply.md:3 claims public registry proof claim while public provenance/SBOM/registry proof is missing: public registry install is supported.',
		])
	})

	it('keeps public artifact claim scanning tied to proof blockers instead of metadata blockers', async () => {
		const { hasMissingPublicArtifactProof } = await loadPublicReleaseFoundationValidator()

		expect(
			hasMissingPublicArtifactProof([
				{
					label: 'Documentation and metadata blockers',
					blockers: ['package.json homepage metadata is not set.'],
				},
			]),
		).toBe(false)
		expect(
			hasMissingPublicArtifactProof([
				{
					label: 'Package/public registry evidence blockers',
					blockers: [
						'No approved npm publication or equivalent public registry proof is recorded.',
					],
				},
			]),
		).toBe(true)
		expect(
			hasMissingPublicArtifactProof([
				{
					label: 'Supply-chain evidence blockers',
					blockers: ['No retained canonical SBOM artifact path or release attachment is recorded.'],
				},
			]),
		).toBe(true)
	})

	it('keeps public artifact hash proof blocked when only release-candidate hashes are conditional', async () => {
		const { collectOssLaunchBlockers } = await loadPublicReleaseFoundationValidator()

		const blockers = collectOssLaunchBlockers({
			externalBetaDoc: 'External beta evidence has a separate accepted record.',
			finalGateDoc: 'Final decision: OSS v0.1 public launch approved',
			packageIdentityDoc: 'Registry ownership and public publication proof are recorded.',
			packageJson: {
				private: false,
				version: '0.1.0',
				bugs: { url: 'https://github.com/Andrey-Good/dennett-agent-orchestrator/issues' },
				homepage: 'https://github.com/Andrey-Good/dennett-agent-orchestrator#readme',
				keywords: ['agent-orchestration'],
			},
			supplyChainDoc: [
				'| Control | Status | Reason |',
				'| --- | --- | --- |',
				'| Artifact hash manifest | Conditional release-candidate control. | Release-candidate CI records `SHA256SUMS`; local proof without the release workflow still creates temporary tarballs and does not retain a canonical hash manifest. |',
			].join('\n'),
		})

		expect(blockers).toContainEqual({
			label: 'Supply-chain evidence blockers',
			blockers: ['No artifact hash manifest is recorded for a public release artifact.'],
		})
	})

	it('allows public artifact hash proof only after explicit public-release evidence is recorded', async () => {
		const { collectOssLaunchBlockers } = await loadPublicReleaseFoundationValidator()

		const blockers = collectOssLaunchBlockers({
			externalBetaDoc: 'External beta evidence has a separate accepted record.',
			finalGateDoc: 'Final decision: OSS v0.1 public launch approved',
			packageIdentityDoc: 'Registry ownership and public publication proof are recorded.',
			packageJson: {
				private: false,
				version: '0.1.0',
				bugs: { url: 'https://github.com/Andrey-Good/dennett-agent-orchestrator/issues' },
				homepage: 'https://github.com/Andrey-Good/dennett-agent-orchestrator#readme',
				keywords: ['agent-orchestration'],
			},
			supplyChainDoc:
				'public-release-artifact-hash-evidence: recorded\n\n| Public release artifact hash manifest | Recorded. | Retained evidence path and SHA-256 are recorded for the approved public release artifact. |',
		})

		expect(blockers.find(({ label }) => label === 'Supply-chain evidence blockers')).toBeUndefined()
	})

	it('allows explicit forbidden-claim boundary text', async () => {
		const { validatePublicClaims } = await loadPublicReleaseFoundationValidator()

		expect(
			validatePublicClaims({
				documents: [
					{
						filePath: 'docs/boundary.md',
						text: [
							'Do not claim public launch is approved.',
							'Public launch is blocked.',
							'External beta is not-run and completed beta claims are forbidden.',
							'SBOM retention is deferred.',
						].join('\n'),
					},
				],
				externalBetaNotRun: true,
				hostedOrProductionNotApproved: true,
				missingPublicArtifactProof: true,
				ossLaunchNotApproved: true,
			}),
		).toEqual([])
	})
})

describe('CLI contract freeze', () => {
	it('locks the command inventory and stability labels', () => {
		const contracts = getCliCommandContracts()
		const program = buildCliProgram()

		expect(program.commands.map((command) => command.name())).toEqual(
			contracts.map((contract) => contract.name),
		)
		expect(contracts.map(({ name, stability }) => ({ name, stability }))).toEqual([
			{ name: 'runtime-model-list', stability: 'experimental' },
			{ name: 'runtime-env-inspect', stability: 'experimental' },
			{ name: 'support-bundle', stability: 'stable_safety_protocol' },
			{ name: 'memory-provider-register', stability: 'experimental' },
			{ name: 'memory-provider-list', stability: 'experimental' },
			{ name: 'memory-provider-show', stability: 'experimental' },
			{ name: 'memory-write', stability: 'experimental' },
			{ name: 'memory-read', stability: 'experimental' },
			{ name: 'memory-search', stability: 'experimental' },
			{ name: 'memory-list', stability: 'experimental' },
			{ name: 'memory-update', stability: 'experimental' },
			{ name: 'memory-delete', stability: 'experimental' },
			{ name: 'memory-cleanup-preview', stability: 'stable_safety_protocol' },
			{ name: 'memory-cleanup-verified-delete', stability: 'stable_safety_protocol' },
			{ name: 'subagent-launch', stability: 'experimental' },
			{ name: 'subagent-list', stability: 'experimental' },
			{ name: 'subagent-show', stability: 'experimental' },
			{ name: 'subagent-status', stability: 'experimental' },
			{ name: 'subagent-wait', stability: 'experimental' },
			{ name: 'subagent-record-control', stability: 'experimental' },
			{ name: 'subagent-cancel', stability: 'experimental' },
			{ name: 'subagent-record-review', stability: 'experimental' },
			{ name: 'subagent-link-repair', stability: 'experimental' },
			{ name: 'subagent-close', stability: 'experimental' },
			{ name: 'register', stability: 'stable' },
			{ name: 'status', stability: 'stable' },
			{ name: 'deploy', stability: 'stable' },
			{ name: 'builder', stability: 'experimental' },
			{ name: 'trigger-register', stability: 'experimental' },
			{ name: 'trigger-list', stability: 'experimental' },
			{ name: 'event-dispatch', stability: 'experimental' },
			{ name: 'run-live', stability: 'stable' },
			{ name: 'run', stability: 'stable' },
			{ name: 'run-status', stability: 'stable' },
			{ name: 'comment', stability: 'experimental' },
			{ name: 'reply', stability: 'stable' },
			{ name: 'resume', stability: 'stable' },
		])

		for (const contract of contracts) {
			const command = program.commands.find((candidate) => candidate.name() === contract.name)
			const expectedLabel =
				contract.stability === 'stable_safety_protocol'
					? '[stable/safety-protocol]'
					: `[${contract.stability}]`
			expect(command?.description()).toContain(expectedLabel)
		}
	})

	it('shows stability classes in deterministic top-level help', () => {
		const help = buildCliProgram().helpInformation()

		expect(help).toContain('Bounded local CLI for portable agent runs')
		expect(help).toContain('marked experimental surfaces')
		expect(help).not.toContain('Phase 8 agent lifecycle')
		expect(help).toContain('run-live')
		expect(help).toContain('[stable] run the current live revision for a registered agent')
		expect(help).toContain('memory-cleanup-preview')
		expect(help).toContain('[stable/safety-protocol]')
		expect(help).toContain('runtime-model-list')
		expect(help).toContain('[experimental] list models through the current runtime adapter')
		expect(help).toContain('support-bundle')
		expect(help).toContain('[stable/safety-protocol] emit a local-only redacted diagnostics')
		expect(help).toContain('help [command]')
		expect(help).toContain('[stable] display help for command')
	})

	it('snapshots the stable run-status output envelope', () => {
		const snapshot: PersistedRunSnapshot = {
			run: {
				run_id: 'run-contract',
				logical_agent_id: 'agent.contract',
				resolved_revision_id: 'rev-contract',
				entry_node_id: 'start',
				started_via: 'direct',
				status: 'waiting_for_user',
				params: {},
				event: null,
				last_attempt_sequence: 1,
				last_boundary_sequence: 1,
				created_at: '2026-04-28T08:00:00.000Z',
				updated_at: '2026-04-28T08:00:01.000Z',
			},
			chat: null,
			visible_messages: [
				{
					message_id: 'msg-1',
					chat_id: 'chat-1',
					run_id: 'run-contract',
					message_sequence: 1,
					kind: 'blocking_prompt',
					payload: { text: 'Continue?' },
					created_at: '2026-04-28T08:00:01.000Z',
				},
			],
			attempts: [
				{
					attempt_id: 'attempt-1',
					run_id: 'run-contract',
					node_id: 'start',
					attempt_sequence: 1,
					output_mode: 'text',
					state: 'blocked_wait',
					outcome: null,
					blocked_on_user_prompt: true,
					runtime_handle: { thread_id: 'thread-1' },
					committed_output_id: null,
					resume_boundary_sequence: 1,
					started_at: '2026-04-28T08:00:00.000Z',
					committed_at: null,
				},
			],
			latest_committed_outputs: [],
			current_vars: {},
			resume: {
				run_id: 'run-contract',
				resolved_revision_id: 'rev-contract',
				native_resume_available: true,
				local_resume_available: true,
				last_durable_boundary_sequence: 1,
				last_durable_boundary_kind: 'blocked_prompt_wait',
				last_attempt_id: 'attempt-1',
				pending_prompt: {
					run_id: 'run-contract',
					attempt_id: 'attempt-1',
					prompt_id: 'prompt-1',
					payload: {
						kind: 'text',
						require_response: true,
						text: 'Continue?',
					},
					request_handle: { request_id: 'request-1' },
					unresolved: true,
					blocks_forward_progress: true,
					reply: {
						reply_id: 'reply-1',
						run_id: 'run-contract',
						attempt_id: 'attempt-1',
						prompt_id: 'prompt-1',
						payload: {
							kind: 'text',
							text: 'Yes',
						},
						idempotency_key: 'key-1',
						delivery_status: 'recorded',
						delivery_error_message: null,
						recorded_at: '2026-04-28T08:00:02.000Z',
						delivered_at: null,
					},
				},
				native_session_handle: { thread_id: 'thread-1' },
				local_context_snapshot: null,
				updated_at: '2026-04-28T08:00:02.000Z',
			},
		}

		expect(buildRunInteractionStatus(snapshot)).toMatchInlineSnapshot(`
			{
			  "active_attempt": {
			    "has_runtime_handle": true,
			    "node_id": "start",
			    "state": "blocked_wait",
			  },
			  "interaction": {
			    "pending_prompt": {
			      "attempt_id": "attempt-1",
			      "has_request_handle": true,
			      "kind": "text",
			      "prompt_id": "prompt-1",
			      "reply": {
			        "delivered_at": null,
			        "delivery_status": "recorded",
			        "prompt_id": "prompt-1",
			        "recorded_at": "2026-04-28T08:00:02.000Z",
			        "reply_id": "reply-1",
			      },
			      "require_response": true,
			    },
			    "visible_transcript_messages": 1,
			    "waiting_for_user": true,
			  },
			  "redaction": {
			    "prompt_payload_omitted": true,
			    "reason": "run-status omits prompt and reply payload content; use the local state database only under the project data-retention policy.",
			    "reply_payload_omitted": true,
			  },
			  "resume": {
			    "has_native_session_handle": true,
			    "last_durable_boundary_kind": "blocked_prompt_wait",
			    "last_durable_boundary_sequence": 1,
			    "local_resume_available": true,
			    "native_resume_available": true,
			  },
			  "run": {
			    "entry_node_id": "start",
			    "last_boundary_sequence": 1,
			    "resolved_revision_id": "rev-contract",
			    "run_id": "run-contract",
			    "status": "waiting_for_user",
			  },
			}
		`)
	})
})
