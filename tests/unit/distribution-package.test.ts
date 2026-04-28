import { readFile } from 'node:fs/promises'
import { describe, expect, it } from 'vitest'
import type { PersistedRunSnapshot } from '../../src/core/state/types.js'
import {
	buildCliProgram,
	buildRunInteractionStatus,
	getCliCommandContracts,
} from '../../src/interfaces/cli.js'

type PackageJson = {
	private?: boolean
	engines?: {
		node?: string
	}
	exports?: Record<string, string>
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
			'dist:clean': 'node scripts/clean-dist.js',
			'dist:check': 'node scripts/check-distribution.js',
			'packlist:check': 'node scripts/check-packlist.js',
			'release-candidate:check': 'node scripts/check-release-candidate.js',
			'public-release-foundation:check': 'node scripts/check-public-release-foundation.js',
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
			{ name: 'subagent-wait', stability: 'experimental' },
			{ name: 'subagent-record-control', stability: 'experimental' },
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
