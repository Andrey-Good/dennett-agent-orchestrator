import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { buildSupportBundle } from '../../src/core/support-bundle.js'
import { buildCliProgram } from '../../src/interfaces/cli.js'

const tempDirsToRemove: string[] = []

async function createSupportBundleFixture(): Promise<{
	tempDir: string
	stateDbPath: string
}> {
	const tempDir = await mkdtemp(path.join(os.tmpdir(), 'dennett-support-bundle-'))
	tempDirsToRemove.push(tempDir)
	await writeFile(
		path.join(tempDir, 'package.json'),
		JSON.stringify(
			{
				name: 'dennett-test-package',
				version: '9.9.9',
				private: true,
				license: 'Apache-2.0',
				packageManager: 'pnpm@10.33.0',
				engines: {
					node: '>=22.13.0',
				},
				bin: {
					'dennett-agent-orchestrator': './dist/src/interfaces/cli.js',
				},
				repository: {
					type: 'git',
					url: 'https://alice:secret-token@example.com/private/repo.git',
				},
				support: {
					email: 'alice@example.com',
					url: 'https://support.example.test/form?api_key=sk-support-secret123',
				},
			},
			null,
			2,
		),
		'utf8',
	)
	const stateDir = path.join(tempDir, 'Users', 'Alice', 'private')
	await mkdir(stateDir, { recursive: true })
	const stateDbPath = path.join(stateDir, 'local-state.sqlite')
	await writeFile(stateDbPath, 'sqlite placeholder', 'utf8')
	return { tempDir, stateDbPath }
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

describe('support bundle diagnostics', () => {
	it('builds a local-only redacted bundle without raw account emails, secrets, or paths', async () => {
		const fixture = await createSupportBundleFixture()

		const bundle = await buildSupportBundle({
			cwd: fixture.tempDir,
			packageRoot: fixture.tempDir,
			stateDbPath: fixture.stateDbPath,
			commandContracts: [
				{
					name: 'support-bundle',
					stability: 'stable_safety_protocol',
					summary: 'emit a local-only redacted diagnostics support bundle',
				},
			],
		})
		const serialized = JSON.stringify(bundle)

		expect(bundle.local_only).toBe(true)
		expect(bundle.state_db).toMatchObject({
			exists: true,
			size_bytes: 18,
			path_redacted: true,
		})
		expect(serialized).not.toContain('alice@example.com')
		expect(serialized).not.toContain('secret-token')
		expect(serialized).not.toContain('sk-support-secret123')
		expect(serialized).not.toContain(fixture.tempDir)
		expect(serialized).not.toContain(fixture.stateDbPath)
		expect(serialized).not.toContain('Users')
		expect(serialized).toContain('[REDACTED_EMAIL]')
		expect(serialized).toContain('[REDACTED_SECRET]')
	})

	it('exposes the support-bundle CLI command with redacted stdout', async () => {
		const fixture = await createSupportBundleFixture()
		const originalCwd = process.cwd()
		let stdout = ''
		const stdoutSpy = vi.spyOn(process.stdout, 'write').mockImplementation((chunk) => {
			stdout += String(chunk)
			return true
		})

		try {
			process.chdir(fixture.tempDir)
			const program = buildCliProgram()
			program.exitOverride()

			await program.parseAsync(['support-bundle', '--state-db', fixture.stateDbPath], {
				from: 'user',
			})
		} finally {
			process.chdir(originalCwd)
			stdoutSpy.mockRestore()
		}

		const parsed = JSON.parse(stdout) as {
			local_only: boolean
			state_db: { exists: boolean; path_redacted: boolean }
		}
		expect(parsed.local_only).toBe(true)
		expect(parsed.state_db).toMatchObject({
			exists: true,
			path_redacted: true,
		})
		expect(stdout).not.toContain('alice@example.com')
		expect(stdout).not.toContain('secret-token')
		expect(stdout).not.toContain(fixture.tempDir)
		expect(stdout).not.toContain(fixture.stateDbPath)
	})
})
