import { mkdtemp, readdir, readFile, rm } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import type { ValidateFunction } from 'ajv'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { CodexAppServerRuntimeAdapter } from '../../src/adapters/codex/codex-app-server-runtime-adapter.js'
import type { AgentFile, InputPart } from '../../src/core/agent-file.js'
import { createAjv2020Validator } from '../../src/core/output-schema-validator.js'
import { loadAndValidateAgentFile } from '../../src/core/schema.js'
import { buildCliProgram } from '../../src/interfaces/cli.js'
import type {
	RuntimeAdapterExecutionRequest,
	RuntimeEvent,
	RuntimeExecutionSession,
} from '../../src/ports/runtime.js'

const tempDirsToRemove: string[] = []

async function listJsonFiles(directory: string): Promise<string[]> {
	const entries = await readdir(directory, { withFileTypes: true })
	const nestedFiles = await Promise.all(
		entries.map(async (entry) => {
			const entryPath = path.join(directory, entry.name)
			if (entry.isDirectory()) {
				return listJsonFiles(entryPath)
			}
			return entry.isFile() && entry.name.endsWith('.json') ? [entryPath] : []
		}),
	)
	return nestedFiles.flat().sort((left, right) => left.localeCompare(right))
}

async function loadJsonFile(filePath: string): Promise<unknown> {
	return JSON.parse(await readFile(filePath, 'utf8')) as unknown
}

async function loadBuilderOutputValidator(): Promise<ValidateFunction<unknown>> {
	const schemaDir = path.resolve(process.cwd(), 'contracts', 'json-schema')
	const [builderOutputSchema, agentFileSchema, defsSchema] = await Promise.all(
		['builder-output.schema.json', 'agent-file.schema.json', 'agent-json.defs.schema.json'].map(
			(fileName) =>
				readFile(path.join(schemaDir, fileName), 'utf8').then((contents) => JSON.parse(contents)),
		),
	)

	const ajv = createAjv2020Validator()
	ajv.addSchema(defsSchema)
	ajv.addSchema(agentFileSchema)
	return ajv.compile(builderOutputSchema)
}

function collectInputParts(agentFile: AgentFile): InputPart[] {
	return agentFile.nodes.flatMap((node) => node.input.parts)
}

function collectTopLevelParamReferences(agentFile: AgentFile): string[] {
	const params = new Set<string>()
	for (const part of collectInputParts(agentFile)) {
		if (part.type === 'ref' && part.ref.startsWith('params.')) {
			params.add(part.ref.slice('params.'.length).split('.')[0] ?? '')
		}
	}
	return [...params].filter(Boolean).sort((left, right) => left.localeCompare(right))
}

function emptyEventStream(): AsyncIterable<RuntimeEvent> {
	return {
		async *[Symbol.asyncIterator]() {
			// No runtime events are needed for the offline public example smoke.
		},
	}
}

async function runCli(args: string[]): Promise<{
	stdout: string
	stderr: string
	exitCode: string | number | undefined
}> {
	const originalExitCode = process.exitCode
	let stdout = ''
	let stderr = ''
	const stdoutSpy = vi.spyOn(process.stdout, 'write').mockImplementation((chunk) => {
		stdout += String(chunk)
		return true
	})
	const stderrSpy = vi.spyOn(process.stderr, 'write').mockImplementation((chunk) => {
		stderr += String(chunk)
		return true
	})

	try {
		process.exitCode = undefined
		const program = buildCliProgram()
		program.exitOverride()
		await program.parseAsync(args, { from: 'user' })
		return {
			stdout,
			stderr,
			exitCode: process.exitCode,
		}
	} finally {
		stdoutSpy.mockRestore()
		stderrSpy.mockRestore()
		process.exitCode = originalExitCode
	}
}

afterEach(async () => {
	vi.restoreAllMocks()
	while (tempDirsToRemove.length > 0) {
		const tempDir = tempDirsToRemove.pop()
		if (tempDir) {
			await rm(tempDir, { recursive: true, force: true })
		}
	}
})

describe('public agent examples', () => {
	it('keeps every examples/agents/valid JSON file loadable as Agent JSON with declared param refs', async () => {
		const validExampleDir = path.resolve(process.cwd(), 'examples', 'agents', 'valid')
		const exampleFiles = await listJsonFiles(validExampleDir)

		expect(exampleFiles.length).toBeGreaterThan(0)

		for (const exampleFile of exampleFiles) {
			const agentFile = await loadAndValidateAgentFile(exampleFile)
			const declaredParamNames = new Set(Object.keys(agentFile.params ?? {}))

			for (const paramName of collectTopLevelParamReferences(agentFile)) {
				expect(
					declaredParamNames,
					`${path.relative(process.cwd(), exampleFile)} declares ${paramName}`,
				).toContain(paramName)
			}
		}
	})

	it('validates builder wrapper examples against the formal builder-output schema', async () => {
		const builderDraftDir = path.resolve(process.cwd(), 'examples', 'agents', 'builder-drafts')
		const validate = await loadBuilderOutputValidator()
		const validWrapperFiles = (await listJsonFiles(builderDraftDir)).filter((filePath) =>
			path.basename(filePath).startsWith('valid-'),
		)

		expect(validWrapperFiles.length).toBeGreaterThan(0)

		for (const wrapperFile of validWrapperFiles) {
			expect(
				validate(await loadJsonFile(wrapperFile)),
				`${path.relative(process.cwd(), wrapperFile)} should pass builder-output.schema.json`,
			).toBe(true)
		}
	})

	it('rejects invalid builder wrapper examples for their documented wrapper violation', async () => {
		const expectedInvalidReasons = new Map([
			[
				'invalid-output-wrapper-extra-diagnostics.json',
				{
					keyword: 'additionalProperties',
					additionalProperty: 'diagnostics',
				},
			],
		])
		const builderDraftDir = path.resolve(process.cwd(), 'examples', 'agents', 'builder-drafts')
		const validate = await loadBuilderOutputValidator()
		const invalidWrapperFiles = (await listJsonFiles(builderDraftDir)).filter((filePath) =>
			path.basename(filePath).startsWith('invalid-'),
		)

		expect(invalidWrapperFiles.length).toBeGreaterThan(0)

		for (const wrapperFile of invalidWrapperFiles) {
			const expectedReason = expectedInvalidReasons.get(path.basename(wrapperFile))
			expect(
				expectedReason,
				`${path.relative(process.cwd(), wrapperFile)} has an expected invalid reason`,
			).toBeDefined()

			expect(validate(await loadJsonFile(wrapperFile))).toBe(false)
			expect(validate.errors).toEqual(
				expect.arrayContaining([
					expect.objectContaining({
						keyword: expectedReason?.keyword,
						params: expect.objectContaining({
							additionalProperty: expectedReason?.additionalProperty,
						}),
					}),
				]),
			)
		}
	})

	it('runs the Phase 5 example through the CLI offline with --param topic resolved', async () => {
		const tempDir = await mkdtemp(path.join(os.tmpdir(), 'dennett-public-example-smoke-'))
		tempDirsToRemove.push(tempDir)
		const stateDbPath = path.join(tempDir, 'local-state.sqlite')
		const examplePath = path.resolve(
			process.cwd(),
			'examples',
			'agents',
			'valid',
			'phase5-codex-minimal.json',
		)
		const requests: RuntimeAdapterExecutionRequest[] = []
		vi.spyOn(CodexAppServerRuntimeAdapter.prototype, 'startExecution').mockImplementation(
			async (request): Promise<RuntimeExecutionSession> => {
				requests.push(request)
				return {
					runtime_handle: null,
					native_session_handle: null,
					terminal_result: Promise.resolve({
						outcome: 'success',
						output: {
							mode: 'text',
						},
						output_text: 'offline example result',
					}),
					events: emptyEventStream(),
				}
			},
		)

		const result = await runCli([
			'run',
			examplePath,
			'--param',
			'topic=offline public example',
			'--run-id',
			'run-public-phase5-example',
			'--state-db',
			stateDbPath,
		])

		expect(result).toEqual({
			stdout: 'offline example result\n',
			stderr: 'Run ID: run-public-phase5-example\n',
			exitCode: undefined,
		})
		expect(requests).toHaveLength(1)
		expect(requests[0]).toMatchObject({
			node_id: 'start',
			runtime_adapter: 'codex',
			input_message: 'Topic: offline public example',
			runtime_options: {
				model: 'gpt-5.3-codex',
			},
			resume: {
				mode: 'fresh',
			},
		})
	})
})
