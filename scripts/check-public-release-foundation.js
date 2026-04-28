import { access, readFile } from 'node:fs/promises'
import { pathToFileURL } from 'node:url'

const REQUIRED_DOCS = [
	'docs/21-public-launch-readiness/baseline-gap-and-forbidden-claims.md',
	'docs/21-public-launch-readiness/public-launch-scope.md',
	'docs/21-public-launch-readiness/security-privacy-legal-foundation.md',
	'docs/21-public-launch-readiness/release-engineering-and-supply-chain.md',
]

const REQUIRED_SCRIPTS = [
	'public-release-foundation:check',
	'packlist:check',
	'release-candidate:check',
	'package:check',
]

const REQUIRED_GUARD_FILES = [
	'scripts/check-packlist.js',
	'scripts/check-release-candidate.js',
	'scripts/check-public-release-foundation.js',
]

async function pathExists(filePath) {
	try {
		await access(filePath)
		return true
	} catch {
		return false
	}
}

async function readJson(filePath) {
	return JSON.parse(await readFile(filePath, 'utf8'))
}

async function validatePublicReleaseFoundation() {
	const errors = []
	const futureBlockers = []
	const packageJson = await readJson('package.json')
	const licenseText = await readFile('LICENSE', 'utf8')
	const readinessReadme = await readFile('docs/21-public-launch-readiness/README.md', 'utf8')

	if (packageJson.license !== 'Apache-2.0') {
		errors.push('package.json license must be Apache-2.0 to match the root LICENSE.')
	}

	if (!licenseText.includes('Apache License') || !licenseText.includes('Version 2.0')) {
		errors.push('Root LICENSE must contain Apache License Version 2.0 text.')
	}

	if (packageJson.private !== true) {
		errors.push(
			'package.json must remain private until later public distribution proof approves publication.',
		)
	}

	if (!Array.isArray(packageJson.files) || packageJson.files.length === 0) {
		errors.push('package.json must keep a non-empty files allowlist for package inventory control.')
	}

	for (const scriptName of REQUIRED_SCRIPTS) {
		if (typeof packageJson.scripts?.[scriptName] !== 'string') {
			errors.push(`package.json is missing required script "${scriptName}".`)
		}
	}

	for (const filePath of [...REQUIRED_GUARD_FILES, 'SECURITY.md', ...REQUIRED_DOCS]) {
		if (!(await pathExists(filePath))) {
			errors.push(`Required release-foundation file is missing: ${filePath}`)
		}
	}

	if (!readinessReadme.includes('./release-engineering-and-supply-chain.md')) {
		errors.push(
			'docs/21-public-launch-readiness/README.md must link the Stage 4 release-engineering document.',
		)
	}

	if (packageJson.repository === undefined) {
		futureBlockers.push('package.json repository metadata is not set.')
	}
	if (packageJson.bugs === undefined) {
		futureBlockers.push('package.json bugs metadata is not set.')
	}
	if (packageJson.homepage === undefined) {
		futureBlockers.push('package.json homepage metadata is not set.')
	}
	if (packageJson.keywords === undefined) {
		futureBlockers.push('package.json keywords are not set.')
	}

	futureBlockers.push('SBOM generation and retention are not implemented.')
	futureBlockers.push('Package provenance/signing proof is not implemented.')
	futureBlockers.push('Public install, upgrade, uninstall, and rollback proof is not recorded.')

	return { errors, futureBlockers }
}

const isMainModule =
	process.argv[1] !== undefined && pathToFileURL(process.argv[1]).href === import.meta.url

if (isMainModule) {
	const { errors, futureBlockers } = await validatePublicReleaseFoundation()

	if (errors.length > 0) {
		for (const error of errors) {
			console.error(error)
		}
		process.exitCode = 1
	} else {
		console.log('Public release foundation check passed.')
	}

	if (futureBlockers.length > 0) {
		console.log('Future publication blockers not enforced by the private Stage 4 foundation:')
		for (const blocker of futureBlockers) {
			console.log(`- ${blocker}`)
		}
	}
}
