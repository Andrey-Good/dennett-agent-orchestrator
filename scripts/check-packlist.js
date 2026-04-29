import { execFile } from 'node:child_process'
import { readFile } from 'node:fs/promises'
import { pathToFileURL } from 'node:url'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)

const REQUIRED_PACKAGE_FILES = [
	'package.json',
	'README.md',
	'LICENSE',
	'dist/src/interfaces/cli.js',
	'contracts/json-schema/agent-file.schema.json',
	'contracts/json-schema/agent-json.defs.schema.json',
]

const REQUIRED_PACKAGE_SCRIPTS = [
	'build',
	'dist:clean',
	'dist:check',
	'packlist:check',
	'public-release-foundation:check',
	'package:local-install:proof',
	'package:upgrade-rollback:proof',
	'package:check',
	'supply-chain:local:proof',
	'typecheck',
]

const EXPECTED_FILES_ALLOWLIST = ['dist/src/**', 'contracts/json-schema/*.schema.json']
const EXPECTED_PACKAGE_EXPORTS = {
	'./package.json': './package.json',
	'./contracts/json-schema/*.schema.json': './contracts/json-schema/*.schema.json',
}
const EXPECTED_PACKAGE_BUGS = {
	url: 'https://github.com/Andrey-Good/dennett-agent-orchestrator/issues',
}
const EXPECTED_PACKAGE_HOMEPAGE =
	'https://github.com/Andrey-Good/dennett-agent-orchestrator#readme'
const EXPECTED_PACKAGE_KEYWORDS = [
	'agent-orchestration',
	'agent-runtime',
	'codex',
	'cli',
	'workflow',
]

function isAllowedPackageFile(filePath) {
	return (
		filePath === 'package.json' ||
		filePath === 'README.md' ||
		filePath === 'LICENSE' ||
		filePath.startsWith('dist/src/') ||
		(/^contracts\/json-schema\/[^/]+\.schema\.json$/.test(filePath) &&
			!filePath.includes('..'))
	)
}

function isForbiddenPackageFile(filePath) {
	return (
		filePath.startsWith('.local/') ||
		filePath.startsWith('.github/') ||
		filePath.startsWith('subagent_tasks/') ||
		filePath.startsWith('tests/') ||
		filePath.startsWith('src/') ||
		filePath.endsWith('.sqlite') ||
		filePath.endsWith('.db') ||
		filePath.endsWith('.tgz') ||
		filePath.endsWith('.env') ||
		/^dist\/vitest\.config\./.test(filePath)
	)
}

function parseNodeFloor(range) {
	const match = /^>=(\d+)\.(\d+)\.(\d+)$/.exec(range)
	if (!match) {
		return null
	}

	return {
		major: Number(match[1]),
		minor: Number(match[2]),
		patch: Number(match[3]),
	}
}

function isNodeFloorSupported(range) {
	const floor = parseNodeFloor(range)
	if (!floor) {
		return false
	}

	if (floor.major !== 22) {
		return floor.major > 22
	}
	if (floor.minor !== 13) {
		return floor.minor > 13
	}
	return floor.patch >= 0
}

export function validatePackageMetadata(packageJson) {
	const errors = []

	if (packageJson.private !== true) {
		errors.push('package.json must remain private.')
	}

	if (!isNodeFloorSupported(packageJson.engines?.node)) {
		errors.push('package.json engines.node must be >=22.13.0 or stricter.')
	}

	if (typeof packageJson.description !== 'string' || packageJson.description.length === 0) {
		errors.push('package.json description must describe the local package artifact.')
	}

	if (
		packageJson.repository?.type !== 'git' ||
		packageJson.repository?.url !== 'https://github.com/Andrey-Good/dennett-agent-orchestrator'
	) {
		errors.push('package.json repository metadata must match the verified origin remote.')
	}

	if (JSON.stringify(packageJson.bugs) !== JSON.stringify(EXPECTED_PACKAGE_BUGS)) {
		errors.push(
			`package.json bugs must be ${JSON.stringify(EXPECTED_PACKAGE_BUGS)} for public issue routing metadata.`,
		)
	}

	if (packageJson.homepage !== EXPECTED_PACKAGE_HOMEPAGE) {
		errors.push(`package.json homepage must be ${EXPECTED_PACKAGE_HOMEPAGE}.`)
	}

	if (JSON.stringify(packageJson.keywords) !== JSON.stringify(EXPECTED_PACKAGE_KEYWORDS)) {
		errors.push(
			`package.json keywords must be ${JSON.stringify(EXPECTED_PACKAGE_KEYWORDS)} for public package discovery metadata.`,
		)
	}

	if (JSON.stringify(packageJson.files) !== JSON.stringify(EXPECTED_FILES_ALLOWLIST)) {
		errors.push(
			`package.json files must be ${JSON.stringify(EXPECTED_FILES_ALLOWLIST)} to keep package inventory allowlisted.`,
		)
	}

	if (JSON.stringify(packageJson.exports) !== JSON.stringify(EXPECTED_PACKAGE_EXPORTS)) {
		errors.push(
			`package.json exports must be ${JSON.stringify(EXPECTED_PACKAGE_EXPORTS)} to keep JavaScript internals out of the stable public API.`,
		)
	}

	for (const scriptName of REQUIRED_PACKAGE_SCRIPTS) {
		if (typeof packageJson.scripts?.[scriptName] !== 'string') {
			errors.push(`package.json is missing required script "${scriptName}".`)
		}
	}

	return errors
}

export function validatePacklistFiles(files) {
	const errors = []
	const fileSet = new Set(files)

	for (const requiredFile of REQUIRED_PACKAGE_FILES) {
		if (!fileSet.has(requiredFile)) {
			errors.push(`Package inventory is missing required file: ${requiredFile}`)
		}
	}

	for (const filePath of files) {
		if (!isAllowedPackageFile(filePath)) {
			errors.push(`Package inventory contains non-allowlisted file: ${filePath}`)
		}
		if (isForbiddenPackageFile(filePath)) {
			errors.push(`Package inventory contains forbidden file: ${filePath}`)
		}
	}

	return errors
}

async function readPackageJson() {
	return JSON.parse(await readFile('package.json', 'utf8'))
}

async function readPacklist() {
	const packCommand = ['pack', '--dry-run', '--json', '--ignore-scripts']
	const { stdout } =
		process.platform === 'win32'
			? await execFileAsync(
					process.env.ComSpec ?? 'cmd.exe',
					['/d', '/s', '/c', `npm ${packCommand.join(' ')}`],
					{
						windowsHide: true,
						maxBuffer: 10 * 1024 * 1024,
					},
				)
			: await execFileAsync('npm', packCommand, {
					windowsHide: true,
					maxBuffer: 10 * 1024 * 1024,
				})
	const parsed = JSON.parse(stdout)
	const files = parsed[0]?.files
	if (!Array.isArray(files)) {
		throw new Error('Unable to read npm pack dry-run file inventory.')
	}

	return files.map((entry) => entry.path.replace(/\\/g, '/')).sort((left, right) => left.localeCompare(right))
}

const isMainModule =
	process.argv[1] !== undefined && pathToFileURL(process.argv[1]).href === import.meta.url

if (isMainModule) {
	const packageJson = await readPackageJson()
	const files = await readPacklist()
	const errors = [...validatePackageMetadata(packageJson), ...validatePacklistFiles(files)]

	if (errors.length > 0) {
		for (const error of errors) {
			console.error(error)
		}
		process.exitCode = 1
	} else {
		console.log(`Validated package inventory with ${files.length} files.`)
	}
}
