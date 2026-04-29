import { access, readFile, readdir } from 'node:fs/promises'
import { pathToFileURL } from 'node:url'

const REQUIRED_DOCS = [
	'docs/21-public-launch-readiness/baseline-gap-and-forbidden-claims.md',
	'docs/21-public-launch-readiness/public-launch-scope.md',
	'docs/21-public-launch-readiness/security-privacy-legal-foundation.md',
	'docs/21-public-launch-readiness/release-engineering-and-supply-chain.md',
	'docs/21-public-launch-readiness/distribution-proof.md',
	'docs/21-public-launch-readiness/install-upgrade-uninstall-rollback.md',
	'docs/21-public-launch-readiness/package-identity-and-registry.md',
	'docs/21-public-launch-readiness/supply-chain-attestation.md',
	'docs/21-public-launch-readiness/hosted-managed-deployment-scope.md',
	'docs/21-public-launch-readiness/public-docs-onboarding-and-claims.md',
	'docs/21-public-launch-readiness/external-beta-readiness.md',
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
	'scripts/check-distribution.js',
	'scripts/check-packlist.js',
	'scripts/check-release-candidate.js',
	'scripts/check-public-release-foundation.js',
]

const CLAIM_SCAN_ROOTS = ['README.md', 'docs']

const BLOCKED_FINAL_GATE_STATUS = 'Final decision: OSS v0.1 public launch blocked / local-package-readiness-only'
const APPROVED_FINAL_GATE_STATUS = 'Final decision: OSS v0.1 public launch approved'
const PACKAGE_EVIDENCE_BLOCKER_LABEL = 'Package/public registry evidence blockers'
const SUPPLY_CHAIN_EVIDENCE_BLOCKER_LABEL = 'Supply-chain evidence blockers'
const PUBLIC_RELEASE_ARTIFACT_HASH_EVIDENCE_MARKER =
	'public-release-artifact-hash-evidence: recorded'
const PUBLIC_RELEASE_ARTIFACT_HASH_RECORDED_ROW_PATTERN =
	/\|\s*Public release artifact hash manifest\s*\|\s*Recorded\.\s*\|/i

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

const HOSTED_OR_PRODUCTION_PROOF_PATTERNS = [
	{
		label: 'hosted, SaaS, managed, or production operations claim',
		regex:
			/\b(?:hosted|managed|saas|cloud deployment|uptime|sla|status-page|status page|production-load|production load|managed incident-response|managed incident response)\s+(?:is\s+|has\s+been\s+)?(?:available|complete|completed|ready|proven|supported|implemented|approved|greenlit)\b/i,
	},
	{
		label: 'hosted, SaaS, managed, or production operations claim',
		regex:
			/\b(?:available|complete|completed|ready|proven|supported|implemented|approved|greenlit)\s+(?:for\s+)?(?:hosted|managed|saas|cloud deployment|uptime|sla|status-page|status page|production-load|production load|managed incident-response|managed incident response)\b/i,
	},
]

const CLAIM_BOUNDARY_PATTERN =
	/\b(?:do not|must not|does not|is not|are not|not\s+(?:a|an|the|approved|complete|completed|ready|run|proven|recorded|available)|no\s+|blocked|blocks|deferred|defer|forbidden|unsupported|missing|absent|incomplete|before|until|cannot|can't|future|requires|required before|remains|outside|excluded|not-run|not run|non-goal|non-goals)\b|(?:\u043d\u0435\s+(?:\u0434\u043e\u043a\u0430\u0437\u044b\u0432\u0430\u0435\u0442|\u043e\u0437\u043d\u0430\u0447\u0430\u0435\u0442|\u0433\u043e\u0442\u043e\u0432\u0430|\u0433\u043e\u0442\u043e\u0432\u043e|\u0433\u043e\u0442\u043e\u0432\u044b|\u044f\u0432\u043b\u044f\u0435\u0442\u0441\u044f))/i

const CLAIM_BOUNDARY_SECTION_PATTERN =
	/\b(?:forbidden|non-goals?|blocked gates?|block criteria|deferred|out of scope|still forbidden|conditions before|future approval requirements|exit criteria|what remains|must not)\b|[\u0417\u0437]\u0430\u043f\u0440/i

const CLAIM_BOUNDARY_LIST_START_PATTERN =
	/\b(?:avoid language like|do not claim|must not claim|forbidden high-level claims|forbidden public claims|forbidden claims)\b|\u0418\u0437\u0431\u0435\u0433|\u041d\u0435 \u0437\u0430\u044f\u0432|\u043d\u0435\u043b\u044c\u0437\u044f/i

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
	hostedOrProductionNotApproved,
	missingPublicArtifactProof,
	ossLaunchNotApproved,
	packagePrivate,
}) {
	const errors = []
	const launchNotApproved = ossLaunchNotApproved ?? packagePrivate === true

	if (launchNotApproved) {
		for (const finding of findUnguardedClaimLines({
			documents,
			patterns: PUBLIC_APPROVAL_PATTERNS,
		})) {
			errors.push(
				`${finding.filePath}:${finding.line} claims ${finding.label} while OSS v0.1 public launch is not approved: ${finding.text}`,
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

	if (hostedOrProductionNotApproved) {
		for (const finding of findUnguardedClaimLines({
			documents,
			patterns: HOSTED_OR_PRODUCTION_PROOF_PATTERNS,
		})) {
			errors.push(
				`${finding.filePath}:${finding.line} claims ${finding.label} while hosted/SaaS/production readiness is deferred: ${finding.text}`,
			)
		}
	}

	return errors
}

function hasOwnMetadata(packageJson, fieldName) {
	return Object.hasOwn(packageJson, fieldName) && packageJson[fieldName] !== undefined
}

function hasRecordedPublicReleaseArtifactHashEvidence(supplyChainDoc) {
	return (
		supplyChainDoc.includes(PUBLIC_RELEASE_ARTIFACT_HASH_EVIDENCE_MARKER) ||
		PUBLIC_RELEASE_ARTIFACT_HASH_RECORDED_ROW_PATTERN.test(supplyChainDoc)
	)
}

export function collectOssLaunchBlockers({
	externalBetaDoc,
	finalGateDoc,
	packageIdentityDoc,
	packageJson,
	supplyChainDoc,
}) {
	const blockerGroups = [
		{
			label: PACKAGE_EVIDENCE_BLOCKER_LABEL,
			blockers: [],
		},
		{
			label: 'External beta blockers',
			blockers: [],
		},
		{
			label: SUPPLY_CHAIN_EVIDENCE_BLOCKER_LABEL,
			blockers: [],
		},
		{
			label: 'Documentation and metadata blockers',
			blockers: [],
		},
	]
	const [packageBlockers, betaBlockers, supplyChainBlockers, metadataBlockers] = blockerGroups.map(
		(group) => group.blockers,
	)

	if (packageJson.private === true) {
		packageBlockers.push('package.json still has "private": true; public npm publication remains blocked.')
	}
	if (packageJson.version === '0.0.0') {
		packageBlockers.push('package.json version is still the pre-publication placeholder 0.0.0.')
	}
	if (packageIdentityDoc.includes('No public registry ownership proof is recorded')) {
		packageBlockers.push('No public registry namespace or package ownership proof is recorded.')
	}
	if (packageIdentityDoc.includes('no `npm publish` has been run or approved')) {
		packageBlockers.push('No approved npm publication or equivalent public registry proof is recorded.')
	}
	if (packageIdentityDoc.includes('no public registry install path is claimed')) {
		packageBlockers.push('No public registry install path is proven or claimed.')
	}

	if (externalBetaDoc.includes('external-beta-not-run') || externalBetaDoc.includes('`not-run`')) {
		betaBlockers.push('Stage 16 external beta remains not-run; no accepted external participant evidence exists.')
	}

	if (supplyChainDoc.includes('There is no canonical SBOM file path')) {
		supplyChainBlockers.push('No retained canonical SBOM artifact path or release attachment is recorded.')
	}
	if (supplyChainDoc.includes('| npm provenance | Deferred.')) {
		supplyChainBlockers.push('npm provenance remains deferred.')
	}
	if (supplyChainDoc.includes('| Package signing | Deferred.')) {
		supplyChainBlockers.push('Package signing remains deferred.')
	}
	if (!hasRecordedPublicReleaseArtifactHashEvidence(supplyChainDoc)) {
		supplyChainBlockers.push('No artifact hash manifest is recorded for a public release artifact.')
	}

	for (const fieldName of ['bugs', 'homepage', 'keywords']) {
		if (!hasOwnMetadata(packageJson, fieldName)) {
			metadataBlockers.push(`package.json ${fieldName} metadata is not set.`)
		}
	}
	if (!finalGateDoc.includes(BLOCKED_FINAL_GATE_STATUS) && !finalGateDoc.includes(APPROVED_FINAL_GATE_STATUS)) {
		metadataBlockers.push(
			`Final gate doc must state either "${BLOCKED_FINAL_GATE_STATUS}" or "${APPROVED_FINAL_GATE_STATUS}".`,
		)
	}

	return blockerGroups.filter((group) => group.blockers.length > 0)
}

export function hasMissingPublicArtifactProof(launchBlockers) {
	return launchBlockers.some(
		(group) =>
			group.label === PACKAGE_EVIDENCE_BLOCKER_LABEL ||
			group.label === SUPPLY_CHAIN_EVIDENCE_BLOCKER_LABEL,
	)
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
	const packageIdentityDoc = await readFile(
		'docs/21-public-launch-readiness/package-identity-and-registry.md',
		'utf8',
	)
	const supplyChainDoc = await readFile(
		'docs/21-public-launch-readiness/supply-chain-attestation.md',
		'utf8',
	)

	if (packageJson.license !== 'Apache-2.0') {
		errors.push('package.json license must be Apache-2.0 to match the root LICENSE.')
	}

	if (!licenseText.includes('Apache License') || !licenseText.includes('Version 2.0')) {
		errors.push('Root LICENSE must contain Apache License Version 2.0 text.')
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
	if (!readinessReadme.includes('./public-docs-onboarding-and-claims.md')) {
		errors.push(
			'docs/21-public-launch-readiness/README.md must link the Stage 14 public docs/onboarding/claims document.',
		)
	}

	const launchBlockers = collectOssLaunchBlockers({
		externalBetaDoc,
		finalGateDoc,
		packageIdentityDoc,
		packageJson,
		supplyChainDoc,
	})
	const finalGateApprovesOssLaunch = finalGateDoc.includes(APPROVED_FINAL_GATE_STATUS)
	const finalGateBlocksOssLaunch = finalGateDoc.includes(BLOCKED_FINAL_GATE_STATUS)

	if (launchBlockers.length > 0 && !finalGateBlocksOssLaunch) {
		errors.push(
			`Stage 17 final gate doc must state "${BLOCKED_FINAL_GATE_STATUS}" while OSS v0.1 blockers remain.`,
		)
	}
	if (launchBlockers.length > 0 && finalGateApprovesOssLaunch) {
		errors.push('Stage 17 final gate doc approves OSS v0.1 launch while blockers remain.')
	}
	if (launchBlockers.length === 0 && !finalGateApprovesOssLaunch && !finalGateBlocksOssLaunch) {
		errors.push(
			`Stage 17 final gate doc must record an explicit OSS v0.1 launch decision using "${APPROVED_FINAL_GATE_STATUS}" or "${BLOCKED_FINAL_GATE_STATUS}".`,
		)
	}

	const externalBetaNotRun =
		externalBetaDoc.includes('external-beta-not-run') && externalBetaDoc.includes('`not-run`')
	const missingPublicArtifactProof = hasMissingPublicArtifactProof(launchBlockers)

	errors.push(
		...validatePublicClaims({
			documents: await readClaimScanDocuments(),
			externalBetaNotRun,
			hostedOrProductionNotApproved: true,
			missingPublicArtifactProof,
			ossLaunchNotApproved: !finalGateApprovesOssLaunch,
		}),
	)

	for (const group of launchBlockers) {
		for (const blocker of group.blockers) {
			futureBlockers.push(`${group.label}: ${blocker}`)
		}
	}

	return { errors, futureBlockers, launchBlockers }
}

const isMainModule =
	process.argv[1] !== undefined && pathToFileURL(process.argv[1]).href === import.meta.url

if (isMainModule) {
	const { errors, launchBlockers } = await validatePublicReleaseFoundation()

	if (errors.length > 0) {
		for (const error of errors) {
			console.error(error)
		}
		process.exitCode = 1
	} else {
		console.log('Public release foundation check passed.')
	}

	if (launchBlockers.length > 0) {
		console.log('OSS v0.1 public launch gate: BLOCKED (not approved).')
		console.log('Remaining blockers:')
		for (const group of launchBlockers) {
			console.log(`- ${group.label}:`)
			for (const blocker of group.blockers) {
				console.log(`  - ${blocker}`)
			}
		}
	} else if (errors.length === 0) {
		console.log('OSS v0.1 public launch gate blockers are clear; rely on the final gate doc for approval state.')
	}
}
