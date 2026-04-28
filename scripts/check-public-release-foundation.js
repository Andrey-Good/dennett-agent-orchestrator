import { access, readFile, readdir } from 'node:fs/promises'
import { pathToFileURL } from 'node:url'

const REQUIRED_DOCS = [
	'docs/21-public-launch-readiness/baseline-gap-and-forbidden-claims.md',
	'docs/21-public-launch-readiness/public-launch-scope.md',
	'docs/21-public-launch-readiness/security-privacy-legal-foundation.md',
	'docs/21-public-launch-readiness/release-engineering-and-supply-chain.md',
	'docs/21-public-launch-readiness/final-public-launch-gate-decision.md',
]

const REQUIRED_SCRIPTS = [
	'public-release-foundation:check',
	'packlist:check',
	'release-candidate:check',
	'package:local-install:proof',
	'package:upgrade-rollback:proof',
	'package:check',
	'supply-chain:local:proof',
]

const REQUIRED_GUARD_FILES = [
	'scripts/check-packlist.js',
	'scripts/check-release-candidate.js',
	'scripts/check-public-release-foundation.js',
]

const CLAIM_SCAN_ROOTS = ['README.md', 'docs']

const PUBLIC_APPROVAL_PATTERNS = [
	{
		label: 'public launch, GA, or production approval claim',
		regex:
			/\b(?:public launch|public-readiness|public readiness|general availability|ga|production(?: readiness| launch)?|production-ready)\s+(?:is\s+|has\s+been\s+)?(?:approved|complete|completed|ready|proven|certified|passed|greenlit)\b/i,
	},
	{
		label: 'public launch, GA, or production approval claim',
		regex:
			/\b(?:approved|complete|completed|ready|proven|certified|passed|greenlit)\s+(?:for\s+)?(?:public launch|public-readiness|public readiness|general availability|ga|production(?: readiness| launch)?|production-ready)\b/i,
	},
]

const EXTERNAL_BETA_COMPLETION_PATTERNS = [
	{
		label: 'external beta completion claim',
		regex:
			/\b(?:external beta|beta-user validation|beta user validation|beta exit|completed beta)\s+(?:is\s+|has\s+been\s+)?(?:complete|completed|passed|approved|accepted|proven|done|greenlit)\b/i,
	},
	{
		label: 'external beta completion claim',
		regex:
			/\b(?:complete|completed|passed|approved|accepted|proven|done|greenlit)\s+(?:external beta|beta-user validation|beta user validation|beta exit)\b/i,
	},
]

const PUBLIC_ARTIFACT_PROOF_PATTERNS = [
	{
		label: 'public provenance claim',
		regex:
			/\b(?:npm provenance|package provenance|public provenance|provenance)\s+(?:is\s+|has\s+been\s+)?(?:available|complete|completed|recorded|published|attached|proven|signed|enabled)\b/i,
	},
	{
		label: 'retained or public SBOM claim',
		regex:
			/\b(?:retained sbom|published sbom|public sbom|sbom retention|sbom)\s+(?:is\s+|has\s+been\s+)?(?:available|complete|completed|recorded|published|attached|retained|proven)\b/i,
	},
	{
		label: 'public registry proof claim',
		regex:
			/\b(?:public registry install|public registry|npm publication|npm package page|npm install dennett-agent-orchestrator|public package)\s+(?:is\s+|has\s+been\s+)?(?:available|complete|completed|published|proven|supported|recorded|approved)\b/i,
	},
	{
		label: 'public registry proof claim',
		regex:
			/\b(?:available|complete|completed|published|proven|supported|recorded|approved)\s+(?:on\s+)?(?:public registry|npm registry|npm package page)\b/i,
	},
]

const CLAIM_BOUNDARY_PATTERN =
	/\b(?:do not|must not|does not|is not|are not|not\s+(?:a|an|the|approved|complete|completed|ready|run|proven|recorded|available)|no\s+|blocked|blocks|deferred|defer|forbidden|unsupported|missing|absent|incomplete|before|until|cannot|can't|future|requires|required before|remains|outside|excluded|not-run|not run|non-goal|non-goals)\b/i

const CLAIM_BOUNDARY_SECTION_PATTERN =
	/\b(?:forbidden|non-goals?|blocked gates?|block criteria|deferred|out of scope|still forbidden|conditions before|future approval requirements|exit criteria|what remains|must not)\b|[\u0417\u0437]\u0430\u043f\u0440/i

const CLAIM_BOUNDARY_LIST_START_PATTERN =
	/\b(?:avoid language like|do not claim|must not claim)\b|\u0418\u0437\u0431\u0435\u0433|\u041d\u0435 \u0437\u0430\u044f\u0432|\u043d\u0435\u043b\u044c\u0437\u044f/i

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

async function collectMarkdownFiles(filePath) {
	if (!(await pathExists(filePath))) {
		return []
	}

	if (filePath.endsWith('.md')) {
		return [filePath]
	}

	const entries = await readdir(filePath, { withFileTypes: true })
	const nestedFiles = await Promise.all(
		entries.map((entry) => {
			const entryPath = `${filePath}/${entry.name}`
			if (entry.isDirectory()) {
				return collectMarkdownFiles(entryPath)
			}
			if (entry.isFile() && entryPath.endsWith('.md')) {
				return [entryPath]
			}
			return []
		}),
	)

	return nestedFiles.flat()
}

function isBoundaryLine(line) {
	return CLAIM_BOUNDARY_PATTERN.test(line)
}

function findUnguardedClaimLines({ documents, patterns }) {
	const findings = []

	for (const document of documents) {
		let inCodeFence = false
		let inBoundarySection = false
		let inBoundaryList = false
		const lines = document.text.split(/\r?\n/)

		for (const [index, line] of lines.entries()) {
			if (line.trimStart().startsWith('```')) {
				inCodeFence = !inCodeFence
				continue
			}
			const heading = /^(#{1,6})\s+(.+?)\s*$/.exec(line)
			if (heading) {
				inBoundarySection = CLAIM_BOUNDARY_SECTION_PATTERN.test(heading[2])
				inBoundaryList = false
			}
			if (CLAIM_BOUNDARY_LIST_START_PATTERN.test(line)) {
				inBoundaryList = true
			}
			if (inBoundaryList && line.trim() !== '' && !line.trimStart().startsWith('-')) {
				inBoundaryList = CLAIM_BOUNDARY_LIST_START_PATTERN.test(line)
			}
			if (inCodeFence || inBoundarySection || inBoundaryList || isBoundaryLine(line)) {
				continue
			}

			for (const pattern of patterns) {
				if (pattern.regex.test(line)) {
					findings.push({
						filePath: document.filePath,
						line: index + 1,
						label: pattern.label,
						text: line.trim(),
					})
					break
				}
			}
		}
	}

	return findings
}

async function readClaimScanDocuments() {
	const filePaths = [
		...new Set((await Promise.all(CLAIM_SCAN_ROOTS.map(collectMarkdownFiles))).flat()),
	].sort((left, right) => left.localeCompare(right))

	return Promise.all(
		filePaths.map(async (filePath) => ({
			filePath,
			text: await readFile(filePath, 'utf8'),
		})),
	)
}

export function validatePublicClaims({
	documents,
	externalBetaNotRun,
	missingPublicArtifactProof,
	packagePrivate,
}) {
	const errors = []

	if (packagePrivate) {
		for (const finding of findUnguardedClaimLines({
			documents,
			patterns: PUBLIC_APPROVAL_PATTERNS,
		})) {
			errors.push(
				`${finding.filePath}:${finding.line} claims ${finding.label} while package.json private is true: ${finding.text}`,
			)
		}
	}

	if (externalBetaNotRun) {
		for (const finding of findUnguardedClaimLines({
			documents,
			patterns: EXTERNAL_BETA_COMPLETION_PATTERNS,
		})) {
			errors.push(
				`${finding.filePath}:${finding.line} claims ${finding.label} while external beta evidence is not-run: ${finding.text}`,
			)
		}
	}

	if (missingPublicArtifactProof) {
		for (const finding of findUnguardedClaimLines({
			documents,
			patterns: PUBLIC_ARTIFACT_PROOF_PATTERNS,
		})) {
			errors.push(
				`${finding.filePath}:${finding.line} claims ${finding.label} while public provenance/SBOM/registry proof is missing: ${finding.text}`,
			)
		}
	}

	return errors
}

export async function validatePublicReleaseFoundation() {
	const errors = []
	const futureBlockers = []
	const packageJson = await readJson('package.json')
	const licenseText = await readFile('LICENSE', 'utf8')
	const readinessReadme = await readFile('docs/21-public-launch-readiness/README.md', 'utf8')
	const finalGateDoc = await readFile(
		'docs/21-public-launch-readiness/final-public-launch-gate-decision.md',
		'utf8',
	)
	const externalBetaDoc = await readFile(
		'docs/21-public-launch-readiness/external-beta-readiness.md',
		'utf8',
	)

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
	if (!readinessReadme.includes('./final-public-launch-gate-decision.md')) {
		errors.push(
			'docs/21-public-launch-readiness/README.md must link the Stage 17 final public launch gate decision.',
		)
	}
	if (!finalGateDoc.includes('public launch blocked / local-package-readiness-only')) {
		errors.push(
			'Stage 17 final gate doc must state: public launch blocked / local-package-readiness-only.',
		)
	}
	if (!finalGateDoc.includes('895836d7ecae6b7dc17641ecfebd28602efd3eda')) {
		errors.push('Stage 17 final gate doc must record the current TASK-623 commit.')
	}

	const externalBetaNotRun =
		externalBetaDoc.includes('external-beta-not-run') && externalBetaDoc.includes('`not-run`')
	const missingPublicArtifactProof =
		packageJson.private === true ||
		packageJson.bugs === undefined ||
		packageJson.homepage === undefined ||
		packageJson.keywords === undefined

	errors.push(
		...validatePublicClaims({
			documents: await readClaimScanDocuments(),
			externalBetaNotRun,
			missingPublicArtifactProof,
			packagePrivate: packageJson.private === true,
		}),
	)

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

	futureBlockers.push('SBOM retention and publication attachment are not implemented.')
	futureBlockers.push('Package provenance/signing proof remains deferred to publication infrastructure.')
	futureBlockers.push('Public registry install, upgrade, uninstall, and rollback proof is not recorded.')

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
