import { mkdtemp, readFile, rm } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { CodexAppServerRuntimeAdapter } from '../../src/adapters/codex/codex-app-server-runtime-adapter.js'
import type { AgentFile } from '../../src/core/agent-file.js'
import type { JsonObject, JsonValue } from '../../src/core/json.js'
import { SQLiteLocalStateStore } from '../../src/core/state/index.js'
import { buildCliProgram } from '../../src/interfaces/cli.js'
import type {
	RuntimeAdapterExecutionRequest,
	RuntimeEvent,
	RuntimeExecutionSession,
	RuntimeTerminalResult,
	UserChatResponsePayload,
} from '../../src/ports/runtime.js'

const tempDirsToRemove: string[] = []
const TEXT_OUTPUT = { mode: 'text' } as const
const JSON_OBJECT_OUTPUT = {
	mode: 'json',
	schema: {
		type: 'object',
		additionalProperties: true,
	},
} as const
const TARGET_AGENT_ID = 'agent.stage7.cli.integrated'
const RUN_ID = 'run-stage7-cli'
const BUILDER_RUN_ID = 'run-stage7-builder'
const PROMPT_ID = 'stage7-approval'
const TOPIC = 'Stage 7 CLI proof'
const REPLY_TEXT = 'Approved through the offline CLI fixture.'
const TRANSCRIPT_FIXTURE_PATH = path.resolve(
	process.cwd(),
	'tests',
	'fixtures',
	'stage7-cli-integrated-flow-transcript.md',
)

type CliResult = {
	stdout: string
	stderr: string
	exitCode: string | number | undefined
}

type StubExecutionDescriptor =
	| RuntimeTerminalResult
	| {
			runtime_handle?: JsonValue | null
			native_session_handle?: JsonValue | null
			terminal_result: RuntimeTerminalResult | Promise<RuntimeTerminalResult>
			events?: AsyncIterable<RuntimeEvent>
	  }

function buildCandidateAgent(): AgentFile {
	return {
		graph_contract_version: '1.0',
		meta: {
			id: TARGET_AGENT_ID,
			name: 'Stage 7 CLI Integrated Flow Fixture',
			description:
				'Offline fixture agent for deterministic CLI builder, lifecycle, user reply, and resume coverage.',
		},
		entry_node_id: 'ask',
		params: {
			topic: {
				type: 'string',
				required: true,
			},
		},
		interaction: {
			user_mcp: {
				enabled: true,
				server_name: 'orchestrator.user_chat',
			},
		},
		chat: {
			prefer_native_resume: true,
			store_visible_messages: true,
			allow_fresh_start: true,
		},
		final_output: {
			mode: 'last_node_output',
		},
		nodes: [
			{
				id: 'ask',
				kind: 'runtime_agent',
				runtime_adapter: 'codex',
				prompt: 'Ask for user approval, then return the approved CLI proof summary.',
				input: {
					parts: [
						{
							type: 'ref',
							ref: 'params.topic',
						},
					],
				},
				output: TEXT_OUTPUT,
			},
		],
	}
}

function emptyEventStream(): AsyncIterable<RuntimeEvent> {
	return {
		async *[Symbol.asyncIterator]() {
			// No live runtime events are emitted by this deterministic fixture.
		},
	}
}

function singleEventStream(event: RuntimeEvent): AsyncIterable<RuntimeEvent> {
	return {
		async *[Symbol.asyncIterator]() {
			yield event
		},
	}
}

function createStubSession(next: StubExecutionDescriptor): RuntimeExecutionSession {
	if ('terminal_result' in next) {
		return {
			runtime_handle: next.runtime_handle ?? null,
			native_session_handle: next.native_session_handle ?? null,
			terminal_result: Promise.resolve(next.terminal_result),
			events: next.events ?? emptyEventStream(),
		}
	}

	return {
		runtime_handle: null,
		native_session_handle: null,
		terminal_result: Promise.resolve(next),
		events: emptyEventStream(),
	}
}

function parseJsonDocuments(output: string): unknown[] {
	const documents: unknown[] = []
	let start = -1
	let depth = 0
	let inString = false
	let escaped = false

	for (let index = 0; index < output.length; index += 1) {
		const char = output[index]

		if (start === -1) {
			if (char === '{') {
				start = index
				depth = 1
			}
			continue
		}

		if (inString) {
			if (escaped) {
				escaped = false
			} else if (char === '\\') {
				escaped = true
			} else if (char === '"') {
				inString = false
			}
			continue
		}

		if (char === '"') {
			inString = true
		} else if (char === '{') {
			depth += 1
		} else if (char === '}') {
			depth -= 1
			if (depth === 0) {
				documents.push(JSON.parse(output.slice(start, index + 1)) as unknown)
				start = -1
			}
		}
	}

	return documents
}

function asJsonObject(value: unknown, label: string): JsonObject {
	if (value === null || Array.isArray(value) || typeof value !== 'object') {
		throw new Error(`Expected ${label} to be a JSON object.`)
	}
	return value as JsonObject
}

function readObjectPath(value: JsonObject, pathSegments: string[]): unknown {
	let current: unknown = value
	for (const segment of pathSegments) {
		const currentObject = asJsonObject(current, pathSegments.join('.'))
		current = currentObject[segment]
	}
	return current
}

async function runCli(args: string[]): Promise<CliResult> {
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

function buildExpectedTranscript(): string {
	return `# Stage 7 CLI Integrated Flow Transcript

This transcript is generated from normalized CLI assertions in \`tests/integration/stage7-cli-integrated-flow.test.ts\`.

## Offline Boundary

- Runtime adapter: Codex App Server methods are mocked in-process.
- State: temporary SQLite database.
- Live providers/network: not used.
- Proof limit: this proves CLI wiring and durable local resume semantics, not live Codex runtime behavior.

## Flow

1. \`$ dennett-agent-orchestrator builder ${TARGET_AGENT_ID} --request <offline-builder-request> --run-id ${BUILDER_RUN_ID} --state-db <temp-state-db>\`
   - exit: 0
   - stdout: operation=create; builder_run_id=${BUILDER_RUN_ID}; draft_revision.kind=draft; live_revision=null
   - stderr: <empty>
2. \`$ dennett-agent-orchestrator register <builder-draft-agent-file> --state-db <temp-state-db>\`
   - exit: 0
   - stdout: logical_agent_id=${TARGET_AGENT_ID}; revision.kind=draft
3. \`$ dennett-agent-orchestrator status ${TARGET_AGENT_ID} --state-db <temp-state-db>\`
   - exit: 0
   - stdout: live_revision=null; draft_revisions=1
4. \`$ dennett-agent-orchestrator deploy <builder-draft-agent-file> --state-db <temp-state-db>\`
   - exit: 0
   - stdout: logical_agent_id=${TARGET_AGENT_ID}; revision.kind=live; live_file_path=<live-agent-file>
5. \`$ dennett-agent-orchestrator run-live ${TARGET_AGENT_ID} --param topic="${TOPIC}" --run-id ${RUN_ID} --state-db <temp-state-db>\`
   - exit: 1
   - stderr: Run ID: ${RUN_ID}; Local resume remains available.; RUN_WAITING_FOR_USER
6. \`$ dennett-agent-orchestrator reply <live-agent-file> --run-id ${RUN_ID} --prompt-id ${PROMPT_ID} --text "${REPLY_TEXT}" --state-db <temp-state-db>\`
   - exit: 0
   - stdout: Prompt reply delivered.
7. \`$ dennett-agent-orchestrator run-status --run-id ${RUN_ID} --state-db <temp-state-db>\`
   - exit: 0
   - stdout: run.status=waiting_for_user; pending_prompt.prompt_id=${PROMPT_ID}; pending_prompt.reply.delivery_status=delivered_live
8. \`$ dennett-agent-orchestrator resume <live-agent-file> --run-id ${RUN_ID} --state-db <temp-state-db>\`
   - exit: 0
   - stderr: Run ID: ${RUN_ID}
   - stdout: final output="Approved Stage 7 CLI proof after offline prompt reply."
`
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

describe('Stage 7 CLI integrated flow', () => {
	it('builds, registers, deploys, waits for user input, replies, and resumes offline', async () => {
		const tempDir = await mkdtemp(path.join(os.tmpdir(), 'dennett-stage7-cli-flow-'))
		tempDirsToRemove.push(tempDir)
		const stateDbPath = path.join(tempDir, 'local-state.sqlite')
		const candidateAgent = buildCandidateAgent()
		const requests: RuntimeAdapterExecutionRequest[] = []
		const replies: Array<{ execution: unknown; response: UserChatResponsePayload }> = []
		const sessions: StubExecutionDescriptor[] = [
			{
				outcome: 'success',
				output: JSON_OBJECT_OUTPUT,
				output_json: {
					agent_file: candidateAgent as unknown as JsonObject,
				},
			},
			{
				runtime_handle: {
					threadId: 'stage7-thread',
					turnId: 'stage7-turn',
				},
				native_session_handle: {
					threadId: 'stage7-thread',
				},
				terminal_result: new Promise<RuntimeTerminalResult>(() => undefined),
				events: singleEventStream({
					kind: 'user_chat_request',
					request_handle: {
						kind: 'codex_app_server_user_chat_request',
						threadId: 'stage7-thread',
						turnId: 'stage7-turn',
						itemId: 'stage7-tool',
						requestId: 424,
						prompt_id: PROMPT_ID,
					},
					payload: {
						kind: 'text',
						prompt_id: PROMPT_ID,
						text: 'Approve the deterministic Stage 7 CLI flow?',
						require_response: true,
					},
				}),
			},
			{
				runtime_handle: {
					threadId: 'stage7-thread',
					turnId: 'stage7-resume-turn',
				},
				native_session_handle: {
					threadId: 'stage7-thread',
				},
				terminal_result: {
					outcome: 'success',
					output: TEXT_OUTPUT,
					output_text: 'Approved Stage 7 CLI proof after offline prompt reply.',
				},
			},
		]
		const startExecution = vi
			.spyOn(CodexAppServerRuntimeAdapter.prototype, 'startExecution')
			.mockImplementation(async (request) => {
				requests.push(request)
				const next = sessions.shift()
				if (!next) {
					throw new Error('Unexpected CLI runtime launch.')
				}
				return createStubSession(next)
			})
		const deliverUserChatResponse = vi
			.spyOn(CodexAppServerRuntimeAdapter.prototype, 'deliverUserChatResponse')
			.mockImplementation(async (execution, response) => {
				replies.push({ execution, response })
			})

		const builder = await runCli([
			'builder',
			TARGET_AGENT_ID,
			'--request',
			'Create an offline CLI-integrated Stage 7 fixture agent.',
			'--run-id',
			BUILDER_RUN_ID,
			'--state-db',
			stateDbPath,
		])
		expect(builder.exitCode).toBeUndefined()
		expect(builder.stderr).toBe('')
		const builderOutput = asJsonObject(parseJsonDocuments(builder.stdout)[0], 'builder output')
		expect(builderOutput).toMatchObject({
			operation: 'create',
			builder_run_id: BUILDER_RUN_ID,
		})
		expect(readObjectPath(builderOutput, ['draft_revision', 'revision_kind'])).toBe('draft')
		expect(readObjectPath(builderOutput, ['draft_status', 'agent', 'live_revision_id'])).toBeNull()
		const draftFilePath = readObjectPath(builderOutput, ['draft_revision', 'file_path'])
		expect(typeof draftFilePath).toBe('string')

		const register = await runCli(['register', String(draftFilePath), '--state-db', stateDbPath])
		expect(register.exitCode).toBeUndefined()
		expect(register.stderr).toBe('')
		const registerDocuments = parseJsonDocuments(register.stdout).map((document, index) =>
			asJsonObject(document, `register output ${index}`),
		)
		expect(registerDocuments[1]).toMatchObject({
			logical_agent_id: TARGET_AGENT_ID,
		})
		expect(readObjectPath(registerDocuments[1] ?? {}, ['revision', 'revision_kind'])).toBe('draft')

		const status = await runCli(['status', TARGET_AGENT_ID, '--state-db', stateDbPath])
		expect(status.exitCode).toBeUndefined()
		expect(status.stderr).toBe('')
		const statusOutput = asJsonObject(parseJsonDocuments(status.stdout)[0], 'status output')
		expect(statusOutput.live_revision).toBeNull()
		expect(statusOutput.draft_revisions).toHaveLength(1)

		const deploy = await runCli(['deploy', String(draftFilePath), '--state-db', stateDbPath])
		expect(deploy.exitCode).toBeUndefined()
		expect(deploy.stderr).toBe('')
		const deployDocuments = parseJsonDocuments(deploy.stdout).map((document, index) =>
			asJsonObject(document, `deploy output ${index}`),
		)
		expect(deployDocuments[1]).toMatchObject({
			logical_agent_id: TARGET_AGENT_ID,
		})
		expect(readObjectPath(deployDocuments[1] ?? {}, ['revision', 'revision_kind'])).toBe('live')
		const liveFilePath = readObjectPath(deployDocuments[1] ?? {}, ['live_file_path'])
		expect(typeof liveFilePath).toBe('string')

		const runLive = await runCli([
			'run-live',
			TARGET_AGENT_ID,
			'--param',
			`topic=${TOPIC}`,
			'--run-id',
			RUN_ID,
			'--state-db',
			stateDbPath,
		])
		expect(runLive.exitCode).toBe(1)
		expect(runLive.stdout).toBe('')
		expect(runLive.stderr).toContain(`Run ID: ${RUN_ID}\n`)
		expect(runLive.stderr).toContain('Local resume remains available.\n')
		expect(runLive.stderr).toContain('RUN_WAITING_FOR_USER')

		const reply = await runCli([
			'reply',
			String(liveFilePath),
			'--run-id',
			RUN_ID,
			'--prompt-id',
			PROMPT_ID,
			'--text',
			REPLY_TEXT,
			'--state-db',
			stateDbPath,
		])
		expect(reply.exitCode).toBeUndefined()
		expect(reply.stdout).toBe('Prompt reply delivered.\n')
		expect(reply.stderr).toBe('')

		const runStatus = await runCli(['run-status', '--run-id', RUN_ID, '--state-db', stateDbPath])
		expect(runStatus.exitCode).toBeUndefined()
		expect(runStatus.stderr).toBe('')
		const runStatusOutput = asJsonObject(
			parseJsonDocuments(runStatus.stdout)[0],
			'run status output',
		)
		expect(readObjectPath(runStatusOutput, ['run', 'status'])).toBe('waiting_for_user')
		expect(readObjectPath(runStatusOutput, ['interaction', 'pending_prompt', 'prompt_id'])).toBe(
			PROMPT_ID,
		)
		expect(
			readObjectPath(runStatusOutput, [
				'interaction',
				'pending_prompt',
				'reply',
				'delivery_status',
			]),
		).toBe('delivered_live')
		expect(readObjectPath(runStatusOutput, ['redaction', 'prompt_payload_omitted'])).toBe(true)
		expect(readObjectPath(runStatusOutput, ['redaction', 'reply_payload_omitted'])).toBe(true)

		const resume = await runCli([
			'resume',
			String(liveFilePath),
			'--run-id',
			RUN_ID,
			'--state-db',
			stateDbPath,
		])
		expect(resume.exitCode).toBeUndefined()
		expect(resume.stderr).toBe(`Run ID: ${RUN_ID}\n`)
		expect(resume.stdout).toBe('Approved Stage 7 CLI proof after offline prompt reply.\n')

		expect(startExecution).toHaveBeenCalledTimes(3)
		expect(requests[0]).toMatchObject({
			node_id: 'builder',
			runtime_adapter: 'codex',
		})
		expect(JSON.parse(String(requests[0]?.input_message))).toMatchObject({
			target_agent: {
				id: TARGET_AGENT_ID,
			},
			operation: 'create',
			request: 'Create an offline CLI-integrated Stage 7 fixture agent.',
		})
		expect(requests[1]).toMatchObject({
			node_id: 'ask',
			input_message: TOPIC,
			interaction: {
				comments_enabled: false,
				user_chat_server_name: 'orchestrator.user_chat',
			},
			resume: {
				mode: 'fresh',
			},
		})
		expect(deliverUserChatResponse).toHaveBeenCalledTimes(1)
		expect(replies).toEqual([
			{
				execution: expect.objectContaining({
					prompt_id: PROMPT_ID,
				}),
				response: {
					kind: 'text',
					prompt_id: PROMPT_ID,
					text: REPLY_TEXT,
				},
			},
		])
		expect(requests[2]).toMatchObject({
			node_id: 'ask',
			input_message: TOPIC,
			interaction: {
				comments_enabled: false,
				user_chat_server_name: 'orchestrator.user_chat',
				user_chat_reply: {
					kind: 'text',
					prompt_id: PROMPT_ID,
					text: REPLY_TEXT,
				},
			},
			resume: {
				mode: 'native_resume',
				native_session_handle: {
					threadId: 'stage7-thread',
				},
			},
		})
		expect(sessions).toEqual([])

		const verificationStore = new SQLiteLocalStateStore({ database_path: stateDbPath })
		const snapshot = verificationStore.getPersistedRunSnapshot(RUN_ID)
		verificationStore.close()
		expect(snapshot?.run.status).toBe('completed')
		expect(snapshot?.resume.pending_prompt).toBeNull()
		expect(snapshot?.visible_messages).toEqual([
			expect.objectContaining({
				kind: 'blocking_prompt',
				payload: expect.objectContaining({
					prompt_id: PROMPT_ID,
				}),
			}),
			expect.objectContaining({
				kind: 'user_message',
				payload: {
					kind: 'text',
					prompt_id: PROMPT_ID,
					text: REPLY_TEXT,
				},
			}),
		])

		const transcript = buildExpectedTranscript()
		await expect(readFile(TRANSCRIPT_FIXTURE_PATH, 'utf8')).resolves.toBe(transcript)
	})
})
