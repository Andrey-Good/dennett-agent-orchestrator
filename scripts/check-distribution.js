import { execFile } from 'node:child_process'
import {
	access,
	mkdir,
	mkdtemp,
	readdir,
	rm,
	writeFile,
} from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { pathToFileURL } from 'node:url'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)
const distPath = path.resolve('dist')
const cliPath = path.join(distPath, 'src', 'interfaces', 'cli.js')
const packageName = 'dennett-agent-orchestrator'
const commandTimeoutMs = 120_000

async function collectFiles(directory) {
	const entries = await readdir(directory, { withFileTypes: true })
	const files = []

	for (const entry of entries) {
		const entryPath = path.join(directory, entry.name)
		if (entry.isDirectory()) {
			files.push(...(await collectFiles(entryPath)))
		} else if (entry.isFile()) {
			files.push(entryPath)
		}
	}

	return files
}

async function pathExists(filePath) {
	try {
		await access(filePath)
		return true
	} catch {
		return false
	}
}

function quoteWindowsShellArg(value) {
	const stringValue = String(value)
	if (/^[A-Za-z0-9_./:=@\\-]+$/.test(stringValue)) {
		return stringValue
	}

	return `"${stringValue.replace(/"/g, '""')}"`
}

function commandOptions(options = {}) {
	return {
		cwd: process.cwd(),
		windowsHide: true,
		maxBuffer: 10 * 1024 * 1024,
		timeout: commandTimeoutMs,
		...options,
	}
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

async function execPath(filePath, args, options = {}) {
	if (process.platform === 'win32') {
		return await execFileAsync(
			process.env.ComSpec ?? 'cmd.exe',
			['/d', '/s', '/c', [filePath, ...args].map(quoteWindowsShellArg).join(' ')],
			commandOptions(options),
		)
	}

	return await execFileAsync(filePath, args, commandOptions(options))
}

function assertHelpOutput(stdout) {
	if (!stdout.includes('dennett-agent-orchestrator') || !stdout.includes('Usage:')) {
		throw new Error('CLI help output did not contain the expected command identity.')
	}
}

function installedBinPath(consumerDirectory) {
	return path.join(
		consumerDirectory,
		'node_modules',
		'.bin',
		process.platform === 'win32' ? `${packageName}.cmd` : packageName,
	)
}

function installedPackagePath(consumerDirectory) {
	return path.join(consumerDirectory, 'node_modules', packageName)
}

async function writeConsumerPackageJson(consumerDirectory) {
	await writeFile(
		path.join(consumerDirectory, 'package.json'),
		`${JSON.stringify(
			{
				name: 'dennett-local-package-proof-consumer',
				version: '0.0.0',
				private: true,
				type: 'module',
			},
			null,
			'\t',
		)}\n`,
	)
}

async function createConsumerProject(tempRoot) {
	const consumerDirectory = path.join(tempRoot, 'consumer')
	await mkdir(consumerDirectory, { recursive: true })
	await writeConsumerPackageJson(consumerDirectory)

	return consumerDirectory
}

async function packCurrentArtifact(tempRoot) {
	const artifactDirectory = path.join(tempRoot, 'artifact')
	await mkdir(artifactDirectory, { recursive: true })
	await execTool('pnpm', ['build'])

	const { stdout } = await execTool('npm', [
		'pack',
		'--json',
		'--ignore-scripts',
		'--pack-destination',
		artifactDirectory,
	])
	const parsedPack = JSON.parse(stdout)
	const packEntry = parsedPack[0]
	if (typeof packEntry?.filename !== 'string') {
		throw new Error('npm pack did not return a package artifact filename.')
	}

	const tgzPath = path.join(artifactDirectory, path.basename(packEntry.filename))
	if (!(await pathExists(tgzPath))) {
		throw new Error(`npm pack reported an artifact that does not exist: ${tgzPath}`)
	}

	return tgzPath
}

async function installTgz(consumerDirectory, tgzPath) {
	await execTool(
		'npm',
		['install', '--ignore-scripts', '--no-audit', '--fund=false', tgzPath],
		{ cwd: consumerDirectory },
	)
}

async function uninstallPackage(consumerDirectory) {
	await execTool(
		'npm',
		['uninstall', '--ignore-scripts', '--no-audit', '--fund=false', packageName],
		{ cwd: consumerDirectory },
	)
}

async function smokeInstalledBin(consumerDirectory) {
	const binPath = installedBinPath(consumerDirectory)
	if (!(await pathExists(binPath))) {
		throw new Error(`Installed package bin is missing: ${binPath}`)
	}

	const { stdout } = await execPath(binPath, ['--help'], {
		cwd: consumerDirectory,
	})
	assertHelpOutput(stdout)

	return stdout
}

async function assertPackageUnavailable(consumerDirectory) {
	const errors = []

	if (await pathExists(installedPackagePath(consumerDirectory))) {
		errors.push(`Package directory still exists after uninstall: ${installedPackagePath(consumerDirectory)}`)
	}
	if (await pathExists(installedBinPath(consumerDirectory))) {
		errors.push(`Package bin still exists after uninstall: ${installedBinPath(consumerDirectory)}`)
	}

	if (errors.length > 0) {
		throw new Error(errors.join('\n'))
	}
}

export function parseLocalInstallProofArgs(argv) {
	const options = { keepTemp: false }

	for (const arg of argv) {
		if (arg === '--keep-temp') {
			options.keepTemp = true
			continue
		}

		throw new Error(`Unknown argument for local install proof: ${arg}`)
	}

	return options
}

function requireFlagValue(argv, index, flagName) {
	const value = argv[index + 1]
	if (value === undefined || value.startsWith('--')) {
		throw new Error(`Missing value for ${flagName}.`)
	}

	return value
}

export function parseUpgradeRollbackProofArgs(argv, cwd = process.cwd()) {
	const options = {
		fromTgz: undefined,
		toTgz: undefined,
		keepTemp: false,
	}

	for (let index = 0; index < argv.length; index += 1) {
		const arg = argv[index]
		if (arg === '--keep-temp') {
			options.keepTemp = true
			continue
		}
		if (arg === '--from-tgz') {
			options.fromTgz = path.resolve(cwd, requireFlagValue(argv, index, arg))
			index += 1
			continue
		}
		if (arg === '--to-tgz') {
			options.toTgz = path.resolve(cwd, requireFlagValue(argv, index, arg))
			index += 1
			continue
		}

		throw new Error(`Unknown argument for upgrade/rollback proof: ${arg}`)
	}

	if (options.fromTgz === undefined || options.toTgz === undefined) {
		throw new Error(
			'Upgrade/rollback proof requires --from-tgz <path> and --to-tgz <path>; no previous artifact means rollback proof is not available.',
		)
	}
	if (options.fromTgz === options.toTgz) {
		throw new Error('Upgrade/rollback proof requires distinct --from-tgz and --to-tgz artifacts.')
	}

	return options
}

export function parseLocalSbomProofArgs(argv, cwd = process.cwd()) {
	const options = {
		fromTgz: undefined,
		keepTemp: false,
	}

	for (let index = 0; index < argv.length; index += 1) {
		const arg = argv[index]
		if (arg === '--keep-temp') {
			options.keepTemp = true
			continue
		}
		if (arg === '--from-tgz') {
			options.fromTgz = path.resolve(cwd, requireFlagValue(argv, index, arg))
			index += 1
			continue
		}

		throw new Error(`Unknown argument for local SBOM proof: ${arg}`)
	}

	return options
}

async function assertTgzExists(tgzPath, label) {
	if (!tgzPath.endsWith('.tgz')) {
		throw new Error(`${label} must point to a .tgz package artifact: ${tgzPath}`)
	}
	if (!(await pathExists(tgzPath))) {
		throw new Error(`${label} package artifact does not exist: ${tgzPath}`)
	}
}

export function validateSbomDocument(sbomDocument) {
	const errors = []

	if (typeof sbomDocument !== 'object' || sbomDocument === null) {
		return ['SBOM output must be a JSON object.']
	}
	if (typeof sbomDocument.spdxVersion !== 'string') {
		errors.push('SBOM output must include an SPDX version.')
	}
	if (!Array.isArray(sbomDocument.packages)) {
		errors.push('SBOM output must include package entries.')
	} else if (!sbomDocument.packages.some((packageEntry) => packageEntry?.name === packageName)) {
		errors.push(`SBOM output must identify ${packageName}.`)
	}

	return errors
}

export function getSupplyChainDeferrals() {
	return [
		'npm provenance is deferred because package publication is blocked by private: true and no npm publish command may run in this stage.',
		'Package signing is deferred because no local signing identity or publication signing infrastructure is configured in this stage.',
	]
}

export async function runDistributionCheck() {
	const files = await collectFiles(distPath)
	const normalizedFiles = files.map((filePath) =>
		path.relative(process.cwd(), filePath).replace(/\\/g, '/'),
	)

	if (!normalizedFiles.includes('dist/src/interfaces/cli.js')) {
		throw new Error('Missing generated CLI artifact: dist/src/interfaces/cli.js')
	}

	const vitestConfigArtifacts = normalizedFiles.filter((filePath) =>
		/^dist\/vitest\.config\.(js|d\.ts|d\.ts\.map)$/.test(filePath),
	)

	if (vitestConfigArtifacts.length > 0) {
		throw new Error(`Build emitted forbidden vitest config artifacts: ${vitestConfigArtifacts.join(', ')}`)
	}

	const { stdout } = await execFileAsync(process.execPath, [cliPath, '--help'], {
		cwd: process.cwd(),
		windowsHide: true,
	})
	assertHelpOutput(stdout)
}

export async function runLocalPackageInstallProof(options = { keepTemp: false }) {
	const tempRoot = await mkdtemp(path.join(os.tmpdir(), 'dennett-local-install-proof-'))

	try {
		const tgzPath = await packCurrentArtifact(tempRoot)
		const consumerDirectory = await createConsumerProject(tempRoot)
		await installTgz(consumerDirectory, tgzPath)
		await smokeInstalledBin(consumerDirectory)
		await uninstallPackage(consumerDirectory)
		await assertPackageUnavailable(consumerDirectory)

		console.log(`Local package install/uninstall proof passed for ${tgzPath}.`)
		if (options.keepTemp) {
			console.log(`Kept proof workspace: ${tempRoot}`)
		}

		return { tempRoot, tgzPath, consumerDirectory }
	} finally {
		if (!options.keepTemp) {
			await rm(tempRoot, { recursive: true, force: true })
		}
	}
}

export async function runUpgradeRollbackProof(options) {
	await assertTgzExists(options.fromTgz, '--from-tgz')
	await assertTgzExists(options.toTgz, '--to-tgz')

	const tempRoot = await mkdtemp(path.join(os.tmpdir(), 'dennett-upgrade-rollback-proof-'))

	try {
		const consumerDirectory = await createConsumerProject(tempRoot)
		await installTgz(consumerDirectory, options.fromTgz)
		await smokeInstalledBin(consumerDirectory)
		await installTgz(consumerDirectory, options.toTgz)
		await smokeInstalledBin(consumerDirectory)
		await installTgz(consumerDirectory, options.fromTgz)
		await smokeInstalledBin(consumerDirectory)

		console.log(
			`Upgrade/rollback proof passed: ${options.fromTgz} -> ${options.toTgz} -> ${options.fromTgz}.`,
		)
		if (options.keepTemp) {
			console.log(`Kept proof workspace: ${tempRoot}`)
		}

		return { tempRoot, consumerDirectory }
	} finally {
		if (!options.keepTemp) {
			await rm(tempRoot, { recursive: true, force: true })
		}
	}
}

export async function runLocalSbomProof(options = { fromTgz: undefined, keepTemp: false }) {
	const tempRoot = await mkdtemp(path.join(os.tmpdir(), 'dennett-local-sbom-proof-'))

	try {
		const tgzPath = options.fromTgz ?? (await packCurrentArtifact(tempRoot))
		await assertTgzExists(tgzPath, '--from-tgz')

		const consumerDirectory = await createConsumerProject(tempRoot)
		await installTgz(consumerDirectory, tgzPath)

		const { stdout } = await execTool(
			'npm',
			['sbom', '--sbom-format=spdx', '--sbom-type=application'],
			{ cwd: consumerDirectory },
		)
		const sbomDocument = JSON.parse(stdout)
		const sbomErrors = validateSbomDocument(sbomDocument)
		if (sbomErrors.length > 0) {
			throw new Error(sbomErrors.join('\n'))
		}

		console.log(`Local SPDX SBOM generation proof passed for ${tgzPath}.`)
		console.log('Deferred supply-chain publication controls:')
		for (const deferral of getSupplyChainDeferrals()) {
			console.log(`- ${deferral}`)
		}
		if (options.keepTemp) {
			console.log(`Kept proof workspace: ${tempRoot}`)
		}

		return { tempRoot, tgzPath, consumerDirectory, sbomDocument }
	} finally {
		if (!options.keepTemp) {
			await rm(tempRoot, { recursive: true, force: true })
		}
	}
}

const isMainModule =
	process.argv[1] !== undefined && pathToFileURL(process.argv[1]).href === import.meta.url

if (isMainModule) {
	const [command = 'dist-check', ...args] = process.argv.slice(2)

	try {
		if (command === 'dist-check') {
			await runDistributionCheck()
		} else if (command === 'local-install-proof') {
			await runLocalPackageInstallProof(parseLocalInstallProofArgs(args))
		} else if (command === 'upgrade-rollback-proof') {
			await runUpgradeRollbackProof(parseUpgradeRollbackProofArgs(args))
		} else if (command === 'local-sbom-proof') {
			await runLocalSbomProof(parseLocalSbomProofArgs(args))
		} else {
			throw new Error(`Unknown distribution check command: ${command}`)
		}
	} catch (error) {
		console.error(error instanceof Error ? error.message : String(error))
		process.exitCode = 1
	}
}
