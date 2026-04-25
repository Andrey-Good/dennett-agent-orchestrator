import { execFile } from 'node:child_process'
import { pathToFileURL } from 'node:url'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)

const REQUIRED_CANDIDATE_PATHS = [
	{ label: '.github/workflows/ci.yml', matches: (filePath) => filePath === '.github/workflows/ci.yml' },
	{ label: '.gitignore', matches: (filePath) => filePath === '.gitignore' },
	{ label: 'AGENTS.md', matches: (filePath) => filePath === 'AGENTS.md' },
	{ label: 'README.md', matches: (filePath) => filePath === 'README.md' },
	{
		label: 'agent_orchestrator_final_spec_v2.md',
		matches: (filePath) => filePath === 'agent_orchestrator_final_spec_v2.md',
	},
	{ label: 'biome.json', matches: (filePath) => filePath === 'biome.json' },
	{ label: 'package.json', matches: (filePath) => filePath === 'package.json' },
	{ label: 'pnpm-lock.yaml', matches: (filePath) => filePath === 'pnpm-lock.yaml' },
	{ label: 'tsconfig.build.json', matches: (filePath) => filePath === 'tsconfig.build.json' },
	{ label: 'tsconfig.json', matches: (filePath) => filePath === 'tsconfig.json' },
	{ label: 'vitest.config.ts', matches: (filePath) => filePath === 'vitest.config.ts' },
	{ label: 'contracts/invariants/**', matches: (filePath) => filePath.startsWith('contracts/invariants/') },
	{
		label: 'contracts/json-schema/*.schema.json',
		matches: (filePath) => /^contracts\/json-schema\/[^/]+\.schema\.json$/.test(filePath),
	},
	{
		label: 'contracts/typescript/*.ts',
		matches: (filePath) => /^contracts\/typescript\/[^/]+\.ts$/.test(filePath),
	},
	{ label: 'docs/**', matches: (filePath) => filePath.startsWith('docs/') },
	{ label: 'examples/**', matches: (filePath) => filePath.startsWith('examples/') },
	{ label: 'scripts/check-distribution.js', matches: (filePath) => filePath === 'scripts/check-distribution.js' },
	{ label: 'scripts/check-packlist.js', matches: (filePath) => filePath === 'scripts/check-packlist.js' },
	{
		label: 'scripts/check-release-candidate.js',
		matches: (filePath) => filePath === 'scripts/check-release-candidate.js',
	},
	{ label: 'scripts/clean-dist.js', matches: (filePath) => filePath === 'scripts/clean-dist.js' },
	{ label: 'src/**', matches: (filePath) => filePath.startsWith('src/') },
	{ label: 'tests/**', matches: (filePath) => filePath.startsWith('tests/') },
]

const FORBIDDEN_PATH_RULES = [
	{ label: 'local state', matches: (filePath) => filePath === '.local' || filePath.startsWith('.local/') },
	{
		label: 'subagent_tasks orchestration scratch documents',
		matches: (filePath) => filePath === 'subagent_tasks' || filePath.startsWith('subagent_tasks/'),
	},
	{ label: 'dist build output', matches: (filePath) => filePath === 'dist' || filePath.startsWith('dist/') },
	{
		label: 'coverage output',
		matches: (filePath) => filePath === 'coverage' || filePath.startsWith('coverage/'),
	},
	{
		label: 'node_modules dependency tree',
		matches: (filePath) => filePath === 'node_modules' || filePath.startsWith('node_modules/'),
	},
	{
		label: 'pnpm store cache',
		matches: (filePath) => filePath === '.pnpm-store' || filePath.startsWith('.pnpm-store/'),
	},
	{
		label: 'stale generated TypeScript contract JavaScript',
		matches: (filePath) => /^contracts\/typescript\/[^/]+\.js$/.test(filePath),
	},
	{ label: 'package archive', matches: (filePath) => /\.(?:tgz|tar\.gz)$/.test(filePath) },
	{ label: 'local database artifact', matches: (filePath) => /\.(?:db|sqlite|sqlite3)$/.test(filePath) },
	{ label: 'local log artifact', matches: (filePath) => /\.log$/.test(filePath) },
	{ label: 'local temp artifact', matches: (filePath) => /\.(?:tmp|temp|pid)$/.test(filePath) },
	{
		label: 'local temp directory',
		matches: (filePath) =>
			filePath === 'tmp' || filePath === 'temp' || filePath.startsWith('tmp/') || filePath.startsWith('temp/'),
	},
	{ label: 'local environment file', matches: (filePath) => /^\.env(?:\.|$)/.test(filePath) },
]

const PRODUCT_PATH_RULES = [
	{ label: '.github/**', matches: (filePath) => filePath.startsWith('.github/') },
	{ label: '.gitignore', matches: (filePath) => filePath === '.gitignore' },
	{ label: 'AGENTS.md', matches: (filePath) => filePath === 'AGENTS.md' },
	{ label: 'README.md', matches: (filePath) => filePath === 'README.md' },
	{
		label: 'agent_orchestrator_final_spec_v2.md',
		matches: (filePath) => filePath === 'agent_orchestrator_final_spec_v2.md',
	},
	{ label: 'biome.json', matches: (filePath) => filePath === 'biome.json' },
	{ label: 'contracts/**', matches: (filePath) => filePath.startsWith('contracts/') },
	{ label: 'docs/**', matches: (filePath) => filePath.startsWith('docs/') },
	{ label: 'examples/**', matches: (filePath) => filePath.startsWith('examples/') },
	{ label: 'package.json', matches: (filePath) => filePath === 'package.json' },
	{ label: 'pnpm-lock.yaml', matches: (filePath) => filePath === 'pnpm-lock.yaml' },
	{ label: 'scripts/**', matches: (filePath) => filePath.startsWith('scripts/') },
	{ label: 'src/**', matches: (filePath) => filePath.startsWith('src/') },
	{ label: 'tests/**', matches: (filePath) => filePath.startsWith('tests/') },
	{ label: 'tsconfig.build.json', matches: (filePath) => filePath === 'tsconfig.build.json' },
	{ label: 'tsconfig.json', matches: (filePath) => filePath === 'tsconfig.json' },
	{ label: 'vitest.config.ts', matches: (filePath) => filePath === 'vitest.config.ts' },
]

export function normalizeGitPath(filePath) {
	return filePath.replace(/\\/g, '/').replace(/^\.\//, '')
}

export function getForbiddenPathReason(filePath) {
	const normalizedPath = normalizeGitPath(filePath)
	const rule = FORBIDDEN_PATH_RULES.find((candidateRule) => candidateRule.matches(normalizedPath))

	return rule?.label
}

export function getProductPathReason(filePath) {
	const normalizedPath = normalizeGitPath(filePath)
	const rule = PRODUCT_PATH_RULES.find((candidateRule) => candidateRule.matches(normalizedPath))

	return rule?.label
}

export function validateReleaseCandidate({ candidateFiles, untrackedFiles }) {
	const normalizedCandidateFiles = [...new Set(candidateFiles.map(normalizeGitPath))].sort((left, right) =>
		left.localeCompare(right),
	)
	const normalizedUntrackedFiles = [...new Set(untrackedFiles.map(normalizeGitPath))].sort((left, right) =>
		left.localeCompare(right),
	)
	const errors = []
	const untrackedProductPathsByReason = new Map()

	for (const requiredPath of REQUIRED_CANDIDATE_PATHS) {
		if (!normalizedCandidateFiles.some(requiredPath.matches)) {
			errors.push(`Release candidate is missing tracked or staged path: ${requiredPath.label}`)
		}
	}

	for (const filePath of normalizedCandidateFiles) {
		const reason = getForbiddenPathReason(filePath)
		if (reason) {
			errors.push(`Release candidate includes forbidden tracked/staged path (${reason}): ${filePath}`)
		}
	}

	for (const filePath of normalizedUntrackedFiles) {
		const reason = getForbiddenPathReason(filePath)
		if (reason) {
			errors.push(`Forbidden untracked artifact is still visible to git status (${reason}): ${filePath}`)
			continue
		}

		const productReason = getProductPathReason(filePath)
		if (productReason) {
			const existingPaths = untrackedProductPathsByReason.get(productReason) ?? []
			existingPaths.push(filePath)
			untrackedProductPathsByReason.set(productReason, existingPaths)
		}
	}

	for (const [reason, filePaths] of untrackedProductPathsByReason) {
		if (filePaths.length === 1) {
			errors.push(`Product path is visible but not tracked or staged (${reason}): ${filePaths[0]}`)
			continue
		}

		const samplePaths = filePaths.slice(0, 3).join(', ')
		errors.push(
			`Product paths are visible but not tracked or staged (${reason}): ${samplePaths}, ... (${filePaths.length} total)`,
		)
	}

	return errors
}

async function gitList(args) {
	const { stdout } = await execFileAsync('git', args, {
		windowsHide: true,
		maxBuffer: 10 * 1024 * 1024,
	})

	return stdout
		.split('\0')
		.filter(Boolean)
		.map(normalizeGitPath)
}

async function readGitCandidateState() {
	return {
		candidateFiles: await gitList(['ls-files', '-z']),
		untrackedFiles: await gitList(['ls-files', '--others', '--exclude-standard', '-z']),
	}
}

const isMainModule =
	process.argv[1] !== undefined && pathToFileURL(process.argv[1]).href === import.meta.url

if (isMainModule) {
	const errors = validateReleaseCandidate(await readGitCandidateState())

	if (errors.length > 0) {
		for (const error of errors) {
			console.error(error)
		}
		process.exitCode = 1
	} else {
		console.log('Release candidate git hygiene check passed.')
	}
}
