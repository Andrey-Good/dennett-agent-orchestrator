import { existsSync } from 'node:fs'
import { readdir, readFile } from 'node:fs/promises'
import path from 'node:path'
import { pathToFileURL } from 'node:url'

const SOURCE_ROOT = 'src'
const SOURCE_EXTENSIONS = ['.ts', '.tsx']
const IMPORT_SPECIFIER_PATTERNS = [
	/(?:^|\n)\s*import\s+(?:type\s+)?(?:[\s\S]{0,2000}?\s+from\s*)?['"]([^'"]+)['"]/g,
	/(?:^|\n)\s*export\s+(?:type\s+)?[\s\S]{0,2000}?\s+from\s*['"]([^'"]+)['"]/g,
	/\bimport\s*\(\s*['"]([^'"]+)['"]\s*\)/g,
]

const CONCRETE_TECHNOLOGY_IMPORTS = [
	{
		specifier: 'node:sqlite',
		reason: 'SQLite storage driver imports must stay behind a persistence adapter boundary.',
	},
	{
		specifier: 'openai',
		reason: 'OpenAI runtime SDK imports must stay behind a runtime adapter boundary.',
	},
	{
		specifier: '@openai/codex',
		reason: 'Codex runtime SDK imports must stay behind the Codex runtime adapter boundary.',
	},
	{
		specifier: '@openai/codex-sdk',
		reason: 'Codex runtime SDK imports must stay behind the Codex runtime adapter boundary.',
	},
	{
		specifier: 'mem0',
		reason: 'Mem0 provider imports must stay behind the memory adapter boundary.',
	},
	{
		specifier: 'mem0ai',
		reason: 'Mem0 provider imports must stay behind the memory adapter boundary.',
	},
]

export const EXISTING_ARCHITECTURE_BOUNDARY_EXCEPTIONS = [
	{
		ruleId: 'core-must-not-import-adapters',
		importer: 'src/core/builder-service.ts',
		imported: 'src/adapters/codex/codex-app-server-runtime-adapter.ts',
		reason:
			'Existing builder service wires the Codex adapter directly; moving Codex startup wiring is outside this task.',
	},
	{
		ruleId: 'core-must-not-import-adapters',
		importer: 'src/core/memory-service.ts',
		imported: 'src/adapters/memory/mem0-memory-adapter.ts',
		reason:
			'Existing memory service wires the Mem0 adapter directly; moving Mem0 startup wiring is outside this task.',
	},
	{
		ruleId: 'concrete-technology-imports-stay-in-adapters-or-interface-startup',
		importer: 'src/core/state/sqlite-state-store.ts',
		imported: 'node:sqlite',
		reason:
			'Existing SQLite store currently lives under core/state; moving SQLite persistence code is outside this task.',
	},
]

function toPosixPath(filePath) {
	return filePath.replace(/\\/g, '/')
}

function normalizeRelativePath(filePath) {
	return toPosixPath(filePath).replace(/^\.\//, '')
}

function isSourceFile(filePath) {
	return SOURCE_EXTENSIONS.some((extension) => filePath.endsWith(extension)) && !filePath.endsWith('.d.ts')
}

async function collectSourceFiles(directory) {
	const entries = await readdir(directory, { withFileTypes: true })
	const files = []

	for (const entry of entries) {
		const entryPath = path.join(directory, entry.name)
		if (entry.isDirectory()) {
			files.push(...(await collectSourceFiles(entryPath)))
		} else if (entry.isFile() && isSourceFile(entry.name)) {
			files.push(entryPath)
		}
	}

	return files.sort((left, right) => toPosixPath(left).localeCompare(toPosixPath(right)))
}

function lineNumberAt(sourceText, index) {
	let line = 1
	for (let offset = 0; offset < index; offset += 1) {
		if (sourceText[offset] === '\n') {
			line += 1
		}
	}

	return line
}

function findImportSpecifiers(sourceText) {
	const imports = []

	for (const pattern of IMPORT_SPECIFIER_PATTERNS) {
		pattern.lastIndex = 0
		for (const match of sourceText.matchAll(pattern)) {
			imports.push({
				specifier: match[1],
				line: lineNumberAt(sourceText, match.index ?? 0),
			})
		}
	}

	return imports.sort((left, right) => left.line - right.line || left.specifier.localeCompare(right.specifier))
}

function candidateTypeScriptTargets(targetPath) {
	const extension = path.extname(targetPath)
	if (extension === '.js' || extension === '.jsx' || extension === '.mjs' || extension === '.cjs') {
		const withoutExtension = targetPath.slice(0, -extension.length)
		return [`${withoutExtension}.ts`, `${withoutExtension}.tsx`, path.join(withoutExtension, 'index.ts')]
	}
	if (extension === '') {
		return [
			`${targetPath}.ts`,
			`${targetPath}.tsx`,
			path.join(targetPath, 'index.ts'),
			path.join(targetPath, 'index.tsx'),
		]
	}

	return [targetPath]
}

function resolveInternalImport({ importer, rootDir, specifier }) {
	if (!specifier.startsWith('.')) {
		return undefined
	}

	const importerPath = path.join(rootDir, importer)
	const targetPath = path.resolve(path.dirname(importerPath), specifier)
	const candidates = candidateTypeScriptTargets(targetPath)
	const resolvedTarget = candidates.find((candidate) => existsSync(candidate)) ?? candidates[0]
	const relativeTarget = normalizeRelativePath(path.relative(rootDir, resolvedTarget))

	if (relativeTarget.startsWith('..') || path.isAbsolute(relativeTarget)) {
		return undefined
	}

	return relativeTarget
}

function matchesPackageSpecifier(specifier, packageName) {
	return specifier === packageName || specifier.startsWith(`${packageName}/`)
}

function concreteTechnologyImportFor(specifier) {
	return CONCRETE_TECHNOLOGY_IMPORTS.find((entry) =>
		matchesPackageSpecifier(specifier, entry.specifier),
	)
}

function isAdapterOrInterfaceStartupContext(importer) {
	return importer.startsWith('src/adapters/') || importer.startsWith('src/interfaces/')
}

function createViolation({ edge, message, ruleId }) {
	return {
		ruleId,
		importer: edge.importer,
		imported: edge.resolvedTarget ?? edge.specifier,
		specifier: edge.specifier,
		line: edge.line,
		message,
	}
}

function evaluateImportEdge(edge) {
	const violations = []
	const importer = edge.importer
	const imported = edge.resolvedTarget

	if (importer.startsWith('src/core/')) {
		if (imported?.startsWith('src/interfaces/')) {
			violations.push(
				createViolation({
					edge,
					ruleId: 'core-must-not-import-interfaces',
					message: 'Core modules must not import user-facing interface modules.',
				}),
			)
		}
		if (matchesPackageSpecifier(edge.specifier, 'commander')) {
			violations.push(
				createViolation({
					edge,
					ruleId: 'core-must-not-import-commander',
					message: 'Core modules must not depend on commander or CLI parsing concerns.',
				}),
			)
		}
		if (imported?.startsWith('src/adapters/')) {
			violations.push(
				createViolation({
					edge,
					ruleId: 'core-must-not-import-adapters',
					message: 'Core modules must depend on ports instead of concrete adapters.',
				}),
			)
		}
	}

	if (importer.startsWith('src/ports/')) {
		if (imported?.startsWith('src/adapters/')) {
			violations.push(
				createViolation({
					edge,
					ruleId: 'ports-must-not-import-adapters',
					message: 'Port contracts must not import concrete adapter implementations.',
				}),
			)
		}
		if (imported?.startsWith('src/interfaces/')) {
			violations.push(
				createViolation({
					edge,
					ruleId: 'ports-must-not-import-interfaces',
					message: 'Port contracts must not import user-facing interface modules.',
				}),
			)
		}
	}

	const concreteTechnologyImport = concreteTechnologyImportFor(edge.specifier)
	if (concreteTechnologyImport !== undefined && !isAdapterOrInterfaceStartupContext(importer)) {
		violations.push(
			createViolation({
				edge,
				ruleId: 'concrete-technology-imports-stay-in-adapters-or-interface-startup',
				message: concreteTechnologyImport.reason,
			}),
		)
	}

	return violations
}

function allowlistKey(entry) {
	return `${entry.ruleId}\0${entry.importer}\0${entry.imported}`
}

export async function collectImportEdges({ rootDir = process.cwd(), sourceRoot = SOURCE_ROOT } = {}) {
	const absoluteRootDir = path.resolve(rootDir)
	const absoluteSourceRoot = path.join(absoluteRootDir, sourceRoot)
	const sourceFiles = await collectSourceFiles(absoluteSourceRoot)
	const edges = []

	for (const sourceFile of sourceFiles) {
		const importer = normalizeRelativePath(path.relative(absoluteRootDir, sourceFile))
		const sourceText = await readFile(sourceFile, 'utf8')
		for (const importEntry of findImportSpecifiers(sourceText)) {
			edges.push({
				importer,
				line: importEntry.line,
				resolvedTarget: resolveInternalImport({
					importer,
					rootDir: absoluteRootDir,
					specifier: importEntry.specifier,
				}),
				specifier: importEntry.specifier,
			})
		}
	}

	return {
		edges,
		scannedFiles: sourceFiles.map((sourceFile) =>
			normalizeRelativePath(path.relative(absoluteRootDir, sourceFile)),
		),
	}
}

export async function checkArchitectureBoundaries({
	allowlist = EXISTING_ARCHITECTURE_BOUNDARY_EXCEPTIONS,
	rootDir = process.cwd(),
	sourceRoot = SOURCE_ROOT,
} = {}) {
	const { edges, scannedFiles } = await collectImportEdges({ rootDir, sourceRoot })
	const allowlistedKeys = new Set(allowlist.map(allowlistKey))
	const seenAllowlistedKeys = new Set()
	const violations = []
	const allowlistedViolations = []

	for (const edge of edges) {
		for (const violation of evaluateImportEdge(edge)) {
			const key = allowlistKey(violation)
			if (allowlistedKeys.has(key)) {
				allowlistedViolations.push(violation)
				seenAllowlistedKeys.add(key)
			} else {
				violations.push(violation)
			}
		}
	}

	const staleAllowlistEntries = allowlist.filter((entry) => !seenAllowlistedKeys.has(allowlistKey(entry)))

	return {
		allowlistedViolations,
		scannedFiles,
		staleAllowlistEntries,
		violations,
	}
}

function formatViolation(violation) {
	return `${violation.ruleId}: ${violation.importer}:${violation.line} imports ${violation.imported}. ${violation.message}`
}

function formatAllowlistEntry(entry) {
	return `${entry.ruleId}: ${entry.importer} imports ${entry.imported}. Reason: ${entry.reason}`
}

const isMainModule =
	process.argv[1] !== undefined && pathToFileURL(process.argv[1]).href === import.meta.url

if (isMainModule) {
	const result = await checkArchitectureBoundaries()

	if (result.violations.length > 0 || result.staleAllowlistEntries.length > 0) {
		for (const violation of result.violations) {
			console.error(formatViolation(violation))
		}
		for (const entry of result.staleAllowlistEntries) {
			console.error(`Stale architecture boundary allowlist entry: ${formatAllowlistEntry(entry)}`)
		}
		process.exitCode = 1
	} else {
		console.log(
			`Architecture boundary check passed (${result.scannedFiles.length} files, ${result.allowlistedViolations.length} allowlisted existing exceptions).`,
		)
		if (result.allowlistedViolations.length > 0) {
			console.log('Allowlisted existing exceptions:')
			for (const violation of result.allowlistedViolations) {
				const entry = EXISTING_ARCHITECTURE_BOUNDARY_EXCEPTIONS.find(
					(candidate) => allowlistKey(candidate) === allowlistKey(violation),
				)
				console.log(`- ${formatAllowlistEntry(entry ?? violation)}`)
			}
		}
	}
}
