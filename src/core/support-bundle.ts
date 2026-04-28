import { execFileSync } from 'node:child_process'
import { readFile, stat } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { hashDiagnosticIdentifier, redactDiagnosticsValue } from './diagnostics-redaction.js'
import type { JsonObject, JsonValue } from './json.js'

export type SupportBundleCommandContract = {
	name: string
	stability: string
	summary: string
}

export type SupportBundleOptions = {
	cwd?: string
	packageRoot?: string
	stateDbPath: string
	commandContracts: SupportBundleCommandContract[]
}

type PackageMetadata = {
	name: string | null
	version: string | null
	private: boolean | null
	license: string | null
	package_manager: string | null
	engines: JsonObject | null
	bin_names: string[]
	repository: JsonValue | null
	support: JsonValue | null
}

function asRecord(value: unknown): Record<string, unknown> | null {
	if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
		return value as Record<string, unknown>
	}
	return null
}

function toJsonValueOrNull(value: unknown): JsonValue | null {
	if (
		value === null ||
		typeof value === 'string' ||
		typeof value === 'number' ||
		typeof value === 'boolean'
	) {
		return value
	}
	if (Array.isArray(value)) {
		return value.map((entry) => toJsonValueOrNull(entry)).filter((entry) => entry !== null)
	}
	const record = asRecord(value)
	if (record) {
		const jsonObject: JsonObject = {}
		for (const [key, entry] of Object.entries(record)) {
			const jsonValue = toJsonValueOrNull(entry)
			if (jsonValue !== null) {
				jsonObject[key] = jsonValue
			}
		}
		return jsonObject
	}
	return null
}

async function readPackageJson(packageRoot: string): Promise<Record<string, unknown> | null> {
	try {
		return JSON.parse(await readFile(path.join(packageRoot, 'package.json'), 'utf8')) as Record<
			string,
			unknown
		>
	} catch {
		return null
	}
}

function getBinNames(bin: unknown): string[] {
	if (typeof bin === 'string') {
		return ['default']
	}
	const record = asRecord(bin)
	if (!record) {
		return []
	}
	return Object.keys(record).sort()
}

function getPackageMetadata(packageJson: Record<string, unknown> | null): PackageMetadata {
	const engines = asRecord(packageJson?.engines)
	return {
		name: typeof packageJson?.name === 'string' ? packageJson.name : null,
		version: typeof packageJson?.version === 'string' ? packageJson.version : null,
		private: typeof packageJson?.private === 'boolean' ? packageJson.private : null,
		license: typeof packageJson?.license === 'string' ? packageJson.license : null,
		package_manager:
			typeof packageJson?.packageManager === 'string' ? packageJson.packageManager : null,
		engines: engines ? (toJsonValueOrNull(engines) as JsonObject) : null,
		bin_names: getBinNames(packageJson?.bin),
		repository: toJsonValueOrNull(packageJson?.repository),
		support: toJsonValueOrNull(packageJson?.support),
	}
}

function runVersionCommand(command: string, args: string[]): string | null {
	const executable = process.platform === 'win32' ? `${command}.cmd` : command
	try {
		return execFileSync(executable, args, {
			encoding: 'utf8',
			stdio: ['ignore', 'pipe', 'ignore'],
			timeout: 3000,
			windowsHide: true,
		}).trim()
	} catch {
		return null
	}
}

function runGitCommand(cwd: string, args: string[]): string | null {
	try {
		return execFileSync('git', args, {
			cwd,
			encoding: 'utf8',
			stdio: ['ignore', 'pipe', 'ignore'],
			timeout: 3000,
			windowsHide: true,
		}).trim()
	} catch {
		return null
	}
}

function summarizeGitStatus(statusOutput: string | null): JsonObject {
	if (statusOutput === null) {
		return {
			available: false,
		}
	}
	const lines = statusOutput.split(/\r?\n/).filter((line) => line.trim() !== '')
	const byStatus: Record<string, number> = {}
	for (const line of lines) {
		const statusCode = line.slice(0, 2).trim() || 'unknown'
		byStatus[statusCode] = (byStatus[statusCode] ?? 0) + 1
	}
	return {
		available: true,
		clean: lines.length === 0,
		changed_count: lines.length,
		by_status: byStatus,
	}
}

async function getStateDbSummary(cwd: string, stateDbPath: string): Promise<JsonObject> {
	const resolvedPath = path.resolve(cwd, stateDbPath)
	try {
		const stateDbStat = await stat(resolvedPath)
		return {
			exists: true,
			size_bytes: stateDbStat.size,
			path_redacted: true,
			path_hash: hashDiagnosticIdentifier(resolvedPath),
		}
	} catch {
		return {
			exists: false,
			size_bytes: null,
			path_redacted: true,
			path_hash: hashDiagnosticIdentifier(resolvedPath),
		}
	}
}

function groupCommandsByStability(commandContracts: SupportBundleCommandContract[]): JsonObject {
	const grouped: Record<string, string[]> = {}
	for (const contract of commandContracts) {
		grouped[contract.stability] = [...(grouped[contract.stability] ?? []), contract.name]
	}
	return grouped
}

function defaultPackageRoot(cwd: string): string {
	const sourceRoot = path.resolve(fileURLToPath(new URL('../../', import.meta.url)))
	if (sourceRoot.endsWith(`${path.sep}dist`)) {
		return path.resolve(sourceRoot, '..')
	}
	return cwd
}

export async function buildSupportBundle(options: SupportBundleOptions): Promise<JsonObject> {
	const cwd = options.cwd ?? process.cwd()
	const packageJson = await readPackageJson(options.packageRoot ?? defaultPackageRoot(cwd))
	const gitInsideWorkTree = runGitCommand(cwd, ['rev-parse', '--is-inside-work-tree']) === 'true'
	const gitCommit = gitInsideWorkTree
		? runGitCommand(cwd, ['rev-parse', '--short=12', 'HEAD'])
		: null
	const gitStatus = gitInsideWorkTree ? runGitCommand(cwd, ['status', '--short']) : null

	const bundle: JsonObject = {
		generated_at: new Date().toISOString(),
		local_only: true,
		package: getPackageMetadata(packageJson) as unknown as JsonObject,
		environment: {
			node: process.version,
			npm: runVersionCommand('npm', ['--version']),
			pnpm: runVersionCommand('pnpm', ['--version']),
			platform: os.platform(),
			arch: os.arch(),
			os_release: os.release(),
		},
		commands: {
			inventory: options.commandContracts.map((contract) => ({
				name: contract.name,
				stability: contract.stability,
				summary: contract.summary,
			})),
			by_stability: groupCommandsByStability(options.commandContracts),
		},
		git: {
			available: gitInsideWorkTree,
			commit: gitCommit,
			status: summarizeGitStatus(gitStatus),
			paths_omitted: true,
		},
		state_db: await getStateDbSummary(cwd, options.stateDbPath),
		support_boundary: {
			stable: options.commandContracts
				.filter((contract) => contract.stability === 'stable')
				.map((contract) => contract.name),
			stable_safety_protocol: options.commandContracts
				.filter((contract) => contract.stability === 'stable_safety_protocol')
				.map((contract) => contract.name),
			experimental: options.commandContracts
				.filter((contract) => contract.stability === 'experimental')
				.map((contract) => contract.name),
			note: 'Experimental command behavior may change; support bundle output remains redacted and local-only.',
		},
		redaction: {
			mode: 'default_redacted',
			paths: 'hashed_only',
			omitted_payloads: [
				'prompt_payloads',
				'reply_payloads',
				'memory_contents',
				'provider_config',
				'runtime_handles',
				'credentialed_urls',
				'account_email',
			],
		},
	}

	return redactDiagnosticsValue(bundle) as JsonObject
}
