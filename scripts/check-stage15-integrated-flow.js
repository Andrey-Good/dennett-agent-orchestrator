import { execFile } from 'node:child_process'
import { mkdtemp, rm } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { pathToFileURL } from 'node:url'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)
const commandTimeoutMs = 120_000
const cliPath = path.resolve('dist', 'src', 'interfaces', 'cli.js')
const phase5ExamplePath = path.resolve('examples', 'agents', 'valid', 'phase5-codex-minimal.json')

function commandOptions(options = {}) {
	return {
		cwd: process.cwd(),
		windowsHide: true,
		maxBuffer: 10 * 1024 * 1024,
		timeout: commandTimeoutMs,
		...options,
	}
}

function quoteWindowsShellArg(value) {
	const stringValue = String(value)
	if (/^[-A-Za-z0-9_./:=@\\]+$/.test(stringValue)) {
		return stringValue
	}

	return `"${stringValue.replace(/"/g, '""')}"`
}

async function execNode(args, options = {}) {
	return await execFileAsync(process.execPath, args, commandOptions(options))
}

async function execTool(command, args, options = {}) {
	if (process.platform === 'win32') {
		return await execFileAsync(
			process.env.ComSpec ?? 'cmd.exe',
			['/d', '/s', '/c', [command, ...args].map(quoteWindowsShellArg).join(' ')],
			commandOptions(options),
		)
	}

	return await execFileAsync(command, args, commandOptions(options))
}

function assertCliHelp(stdout) {
	if (!stdout.includes('dennett-agent-orchestrator') || !stdout.includes('support-bundle')) {
		throw new Error('Built CLI help did not include the expected command inventory.')
	}
}

function assertSupportBundle(stdout, expectedStateExists) {
	const bundle = JSON.parse(stdout)
	if (bundle?.local_only !== true) {
		throw new Error('support-bundle output must be local_only.')
	}
	if (bundle?.state_db?.path_redacted !== true) {
		throw new Error('support-bundle output must redact the state DB path.')
	}
	if (bundle?.state_db?.exists !== expectedStateExists) {
		throw new Error(`support-bundle state DB exists must be ${expectedStateExists}.`)
	}
	if (!bundle?.support_boundary?.stable_safety_protocol?.includes?.('support-bundle')) {
		throw new Error('support-bundle must appear in the stable safety protocol boundary.')
	}
}

async function runFailedLocalFlow(stateDbPath) {
	try {
		await execNode([
			cliPath,
			'run',
			phase5ExamplePath,
			'--run-id',
			'stage15-missing-required-param',
			'--state-db',
			stateDbPath,
		])
		throw new Error('Expected the missing-param Phase 5 local flow to fail.')
	} catch (error) {
		if (error instanceof Error && error.message.includes('Expected the missing-param')) {
			throw error
		}
		const failedProcess = error
		const stderr = typeof failedProcess?.stderr === 'string' ? failedProcess.stderr : ''
		if (
			!stderr.includes('MISSING_PARAM') &&
			!stderr.includes('INVALID_INPUT') &&
			!stderr.includes('RESOLUTION_ERROR')
		) {
			throw new Error(`Failed local flow did not report the expected validation error: ${stderr}`)
		}
	}
}

export async function runStage15IntegratedFlowProof() {
	const tempRoot = await mkdtemp(path.join(os.tmpdir(), 'dennett-stage15-integrated-proof-'))

	try {
		await execTool('pnpm', ['build'])

		const help = await execNode([cliPath, '--help'])
		assertCliHelp(help.stdout)

		const supportStateDbPath = path.join(tempRoot, 'support-state.sqlite')
		const supportBundle = await execNode([
			cliPath,
			'support-bundle',
			'--state-db',
			supportStateDbPath,
		])
		assertSupportBundle(supportBundle.stdout, false)

		const failedFlowStateDbPath = path.join(tempRoot, 'failed-flow-state.sqlite')
		await runFailedLocalFlow(failedFlowStateDbPath)
		const failedFlowSupportBundle = await execNode([
			cliPath,
			'support-bundle',
			'--state-db',
			failedFlowStateDbPath,
		])
		assertSupportBundle(failedFlowSupportBundle.stdout, true)

		await execTool('pnpm', [
			'vitest',
			'run',
			'--config',
			'vitest.config.ts',
			'tests/unit/public-examples.test.ts',
			'tests/integration/stage7-cli-integrated-flow.test.ts',
		])

		console.log('Stage 15 integrated local public flow proof passed.')
	} finally {
		await rm(tempRoot, { recursive: true, force: true })
	}
}

const isMainModule =
	process.argv[1] !== undefined && pathToFileURL(process.argv[1]).href === import.meta.url

if (isMainModule) {
	try {
		await runStage15IntegratedFlowProof()
	} catch (error) {
		console.error(error instanceof Error ? error.message : String(error))
		process.exitCode = 1
	}
}
