import { spawn, spawnSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import path from 'node:path'
import { pathToFileURL } from 'node:url'

const mem0PythonPath = path.resolve(process.cwd(), '.local', 'mem0-venv', 'Scripts', 'python.exe')
const defaultTimeoutMs = 180_000

function getTimeoutMs() {
	const rawValue = process.env.DENNETT_MEM0_TEST_TIMEOUT_MS
	if (rawValue === undefined || rawValue.trim() === '') {
		return defaultTimeoutMs
	}

	const timeoutMs = Number.parseInt(rawValue, 10)
	if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) {
		throw new Error(`DENNETT_MEM0_TEST_TIMEOUT_MS must be a positive integer, got ${rawValue}.`)
	}

	return timeoutMs
}

function terminateProcessTree(child) {
	if (child.pid === undefined) {
		return
	}

	if (process.platform === 'win32') {
		spawnSync('taskkill.exe', ['/pid', String(child.pid), '/t', '/f'], {
			stdio: 'ignore',
			windowsHide: true,
		})
		return
	}

	child.kill('SIGTERM')
}

export async function runMem0Tests() {
	if (!existsSync(mem0PythonPath)) {
		console.warn(
			`Skipping Mem0 local tests: missing ${mem0PythonPath}. Set it up locally before running test:mem0.`,
		)
		return
	}

	const timeoutMs = getTimeoutMs()
	const pnpmArgs = [
		'vitest',
		'run',
		'--config',
		'vitest.config.ts',
		'tests/unit/mem0-memory-adapter.test.ts',
		'tests/unit/memory-cli.test.ts',
		'tests/unit/memory-service.test.ts',
	]
	const command = process.platform === 'win32' ? 'cmd.exe' : 'pnpm'
	const args = process.platform === 'win32' ? ['/d', '/s', '/c', 'pnpm', ...pnpmArgs] : pnpmArgs

	console.error(`Running Mem0 local tests with timeout ${timeoutMs}ms.`)

	const child = spawn(command, args, {
		cwd: process.cwd(),
		env: {
			...process.env,
			DENNETT_RUN_MEM0_TESTS: '1',
		},
		stdio: 'inherit',
		windowsHide: true,
	})

	const exitCode = await new Promise((resolve, reject) => {
		let timedOut = false
		let settled = false
		let timeoutExit
		const settle = (exitCode) => {
			if (settled) {
				return
			}

			settled = true
			clearTimeout(timeout)
			if (timeoutExit !== undefined) {
				clearTimeout(timeoutExit)
			}
			resolve(exitCode)
		}
		const timeout = setTimeout(() => {
			timedOut = true
			console.error(`Mem0 local tests timed out after ${timeoutMs}ms; terminating test process.`)
			terminateProcessTree(child)
			timeoutExit = setTimeout(() => settle(124), 5_000)
		}, timeoutMs)

		child.once('error', (error) => {
			clearTimeout(timeout)
			if (timeoutExit !== undefined) {
				clearTimeout(timeoutExit)
			}
			reject(error)
		})
		child.once('exit', (code, signal) => {
			if (timedOut) {
				settle(124)
				return
			}

			if (signal !== null) {
				console.error(`Mem0 local tests exited after signal ${signal}.`)
				settle(1)
				return
			}

			settle(code ?? 1)
		})
	})

	if (exitCode !== 0) {
		process.exitCode = exitCode
	}
}

const isMainModule =
	process.argv[1] !== undefined && pathToFileURL(process.argv[1]).href === import.meta.url

if (isMainModule) {
	try {
		await runMem0Tests()
	} catch (error) {
		console.error(error instanceof Error ? error.message : String(error))
		process.exitCode = 1
	}
}
