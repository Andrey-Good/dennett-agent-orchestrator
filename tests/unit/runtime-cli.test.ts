import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { describe, expect, it, vi } from 'vitest'
import { CodexAppServerRuntimeAdapter } from '../../src/adapters/codex/codex-app-server-runtime-adapter.js'
import type { AppError } from '../../src/core/errors.js'
import { computeResolvedRevisionId } from '../../src/core/resolved-revision.js'
import { SQLiteLocalStateStore } from '../../src/core/state/index.js'
import {
	buildCliProgram,
	inspectRuntimeEnvironment,
	listRuntimeModels,
} from '../../src/interfaces/cli.js'
import type {
	RuntimeAdapter,
	RuntimeEnvironmentInspectionResult,
	RuntimeModelCatalogPage,
	RuntimeModelCatalogRequest,
} from '../../src/ports/runtime.js'

function createRuntimeSurfaceAdapter(overrides?: {
	models?: RuntimeModelCatalogPage
	environment?: RuntimeEnvironmentInspectionResult
	supportsModelDiscovery?: boolean
	supportsEnvironmentInspection?: boolean
}) {
	const listModels = vi.fn<
		(request?: RuntimeModelCatalogRequest) => Promise<RuntimeModelCatalogPage>
	>(async (request) => ({
		models: [],
		...(request?.cursor ? { next_cursor: request.cursor } : {}),
		...(overrides?.models ?? {}),
	}))
	const inspectEnvironment = vi.fn<() => Promise<RuntimeEnvironmentInspectionResult>>(async () => ({
		auth: {
			authenticated: false,
			requires_openai_auth: false,
		},
		account: {
			status: 'unknown',
		},
		rate_limits: [],
		config: {},
		...(overrides?.environment ?? {}),
	}))

	const adapter: RuntimeAdapter = {
		describeCapabilities() {
			return {
				supports_native_resume: false,
				supports_live_comments: false,
				supports_builtin_user_chat_mcp: false,
				supports_memory_bindings: false,
				supports_model_discovery: overrides?.supportsModelDiscovery ?? true,
				supports_runtime_environment_introspection:
					overrides?.supportsEnvironmentInspection ?? true,
				supports_reasoning_effort: true,
				supports_speed_tiers: true,
				supports_personality: true,
				supports_explicit_runtime_source: false,
				supports_runtime_source_introspection: false,
			}
		},
		async startExecution() {
			throw new Error('not used in runtime CLI tests')
		},
		listModels,
		inspectRuntimeEnvironment: inspectEnvironment,
		async inspectRuntimeSource() {
			throw new Error('not used in runtime CLI tests')
		},
		async deliverComment() {
			throw new Error('not used in runtime CLI tests')
		},
		async deliverUserChatResponse() {
			throw new Error('not used in runtime CLI tests')
		},
		async cancelExecution() {
			throw new Error('not used in runtime CLI tests')
		},
	}

	return {
		adapter,
		listModels,
		inspectEnvironment,
	}
}

describe('runtime CLI helpers', () => {
	it('records a prompt reply for resume without failing the run when live delivery times out', async () => {
		const tempDir = await mkdtemp(path.join(os.tmpdir(), 'dennett-runtime-cli-reply-timeout-'))
		const agentFilePath = path.join(tempDir, 'agent.json')
		const stateDbPath = path.join(tempDir, 'state.sqlite')
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
		const deliverySpy = vi
			.spyOn(CodexAppServerRuntimeAdapter.prototype, 'deliverUserChatResponse')
			.mockRejectedValue(new Error('CODEX_APP_SERVER_REPLY_TIMEOUT: simulated reply timeout'))

		try {
			process.exitCode = undefined
			await writeFile(
				agentFilePath,
				JSON.stringify(
					{
						graph_contract_version: '1.0',
						meta: {
							id: 'reply-timeout-agent',
							name: 'Reply Timeout Agent',
						},
						entry_node_id: 'start',
						interaction: {
							user_mcp: {
								enabled: true,
								server_name: 'orchestrator.user_chat',
							},
						},
						nodes: [
							{
								id: 'start',
								kind: 'runtime_agent',
								runtime_adapter: 'codex',
								prompt: 'Ask the user before continuing.',
								input: {
									parts: [
										{
											type: 'text',
											text: 'Hello',
										},
									],
								},
								output: {
									mode: 'text',
								},
							},
						],
					},
					null,
					2,
				),
				'utf8',
			)
			const resolvedRevisionId = await computeResolvedRevisionId(agentFilePath)
			const pendingHandle = {
				kind: 'codex_app_server_user_chat_request',
				threadId: 'thread-1',
				turnId: 'turn-1',
				itemId: 'tool-1',
				requestId: 501,
				prompt_id: 'prompt-1',
			}
			const store = new SQLiteLocalStateStore({ database_path: stateDbPath })
			const run = store.createRun({
				run_id: 'run-reply-timeout',
				resolved_revision_id: resolvedRevisionId,
				entry_node_id: 'start',
				started_via: 'direct',
				resume: {
					native_resume_available: true,
					local_resume_available: true,
					native_session_handle: {
						threadId: 'thread-1',
					},
				},
			})
			const attempt = store.startNodeAttempt({
				run_id: run.run_id,
				node_id: 'start',
				output_mode: 'text',
				runtime_handle: {
					threadId: 'thread-1',
					turnId: 'turn-1',
				},
			})
			store.commitBlockedAttempt({
				attempt_id: attempt.attempt_id,
				pending_prompt: {
					prompt_id: 'prompt-1',
					payload: {
						kind: 'text',
						prompt_id: 'prompt-1',
						text: 'Continue?',
						require_response: true,
					},
					request_handle: pendingHandle,
				},
				resume: {
					native_resume_available: true,
					local_resume_available: true,
					native_session_handle: {
						threadId: 'thread-1',
					},
				},
			})
			store.close()

			const program = buildCliProgram()
			program.exitOverride()
			await program.parseAsync(
				[
					'reply',
					agentFilePath,
					'--run-id',
					'run-reply-timeout',
					'--text',
					'Yes',
					'--state-db',
					stateDbPath,
					'--codex-app-server-reply-timeout-ms',
					'1',
				],
				{ from: 'user' },
			)

			expect(process.exitCode).toBeUndefined()
			expect(stdout).toBe('Prompt reply recorded for resume.\n')
			expect(stderr).toContain('CODEX_APP_SERVER_REPLY_TIMEOUT')
			expect(deliverySpy).toHaveBeenCalledWith(pendingHandle, {
				kind: 'text',
				prompt_id: 'prompt-1',
				text: 'Yes',
			})

			const verificationStore = new SQLiteLocalStateStore({ database_path: stateDbPath })
			const snapshot = verificationStore.getPersistedRunSnapshot('run-reply-timeout')
			verificationStore.close()

			expect(snapshot?.run.status).toBe('waiting_for_user')
			expect(snapshot?.resume.pending_prompt).toMatchObject({
				prompt_id: 'prompt-1',
				request_handle: pendingHandle,
			})
			expect(snapshot?.attempts).toEqual([
				expect.objectContaining({
					state: 'blocked_wait',
					outcome: null,
				}),
			])
			expect(snapshot?.visible_messages).toEqual([
				expect.objectContaining({
					kind: 'user_message',
					payload: {
						kind: 'text',
						prompt_id: 'prompt-1',
						text: 'Yes',
					},
				}),
			])
		} finally {
			deliverySpy.mockRestore()
			stdoutSpy.mockRestore()
			stderrSpy.mockRestore()
			process.exitCode = originalExitCode
			await rm(tempDir, { recursive: true, force: true })
		}
	})

	it('lists models through the normalized runtime surface', async () => {
		const harness = createRuntimeSurfaceAdapter({
			models: {
				models: [
					{
						id: 'gpt-5.3-codex',
						hidden: false,
						is_default: true,
						input_modalities: ['text'],
						supports_personality: true,
						supported_reasoning_efforts: ['minimal', 'medium'],
						additional_speed_tiers: ['fast'],
					},
				],
				next_cursor: 'cursor-2',
			},
		})

		const result = await listRuntimeModels(
			{
				cursor: 'cursor-1',
				limit: 5,
				includeHidden: true,
			},
			harness.adapter,
		)

		expect(harness.listModels).toHaveBeenCalledWith({
			cursor: 'cursor-1',
			limit: 5,
			include_hidden: true,
		})
		expect(result).toEqual({
			models: [
				{
					id: 'gpt-5.3-codex',
					hidden: false,
					is_default: true,
					input_modalities: ['text'],
					supports_personality: true,
					supported_reasoning_efforts: ['minimal', 'medium'],
					additional_speed_tiers: ['fast'],
				},
			],
			next_cursor: 'cursor-2',
		})
	})

	it('inspects the runtime environment through the normalized runtime surface', async () => {
		const harness = createRuntimeSurfaceAdapter({
			environment: {
				auth: {
					authenticated: true,
					auth_method: 'chatgpt',
					requires_openai_auth: false,
				},
				account: {
					status: 'available',
					account_type: 'chatgpt',
					plan_type: 'pro',
				},
				rate_limits: [],
				config: {
					model: 'gpt-5.3-codex',
					service_tier: 'fast',
				},
			},
		})

		const result = await inspectRuntimeEnvironment(harness.adapter)

		expect(harness.inspectEnvironment).toHaveBeenCalledTimes(1)
		expect(result).toEqual({
			auth: {
				authenticated: true,
				auth_method: 'chatgpt',
				requires_openai_auth: false,
			},
			account: {
				status: 'available',
				account_type: 'chatgpt',
				plan_type: 'pro',
			},
			rate_limits: [],
			config: {
				model: 'gpt-5.3-codex',
				service_tier: 'fast',
			},
		})
	})

	it('supports redacted runtime environment inspection output', async () => {
		const harness = createRuntimeSurfaceAdapter({
			environment: {
				auth: {
					authenticated: true,
					auth_method: 'chatgpt',
					requires_openai_auth: false,
				},
				account: {
					status: 'available',
					account_type: 'chatgpt',
					email: 'alice@example.com',
				},
				rate_limits: [],
				config: {
					model: 'gpt-5.3-codex',
					profile: 'C:\\Users\\Alice\\private\\profile.toml',
				},
			},
		})

		const result = await inspectRuntimeEnvironment(harness.adapter, { redacted: true })
		const serialized = JSON.stringify(result)

		expect(serialized).not.toContain('alice@example.com')
		expect(serialized).not.toContain('C:\\Users\\Alice')
		expect(serialized).toContain('[REDACTED_EMAIL]')
		expect(serialized).toContain('[REDACTED_PATH]')
		expect(serialized).toContain('gpt-5.3-codex')
	})

	it('fails fast when the adapter does not support model discovery', async () => {
		const harness = createRuntimeSurfaceAdapter({
			supportsModelDiscovery: false,
		})

		await expect(
			listRuntimeModels(
				{
					limit: 5,
				},
				harness.adapter,
			),
		).rejects.toMatchObject({
			code: 'UNSUPPORTED_RUNTIME_SURFACE',
			message: 'The current runtime adapter does not support model discovery.',
		} satisfies Pick<AppError, 'code' | 'message'>)
	})

	it('parses runtime-model-list CLI options correctly', async () => {
		let capturedOptions:
			| {
					cursor?: string
					limit?: string
					includeHidden?: boolean
			  }
			| undefined
		const program = buildCliProgram()
		program.exitOverride()
		const command = program.commands.find((entry) => entry.name() === 'runtime-model-list')
		if (!command) {
			throw new Error('expected runtime-model-list CLI command')
		}

		command.action(async (...args: unknown[]) => {
			capturedOptions = args.at(-2) as typeof capturedOptions
		})

		await program.parseAsync(
			[
				'runtime-model-list',
				'--cursor',
				'cursor-1',
				'--limit',
				'5',
				'--include-hidden',
				'--codex-app-server-model-catalog-timeout-ms',
				'25',
			],
			{ from: 'user' },
		)

		expect(capturedOptions).toEqual({
			cursor: 'cursor-1',
			limit: '5',
			includeHidden: true,
			codexAppServerModelCatalogTimeoutMs: '25',
		})
	})

	it('parses runtime timeout CLI options without inventing new commands', async () => {
		const program = buildCliProgram()
		program.exitOverride()
		const commandNames = program.commands.map((entry) => entry.name())
		expect(commandNames).toContain('runtime-env-inspect')
		expect(commandNames).toContain('run')
		expect(commandNames).toContain('comment')
		expect(commandNames).toContain('reply')

		const runtimeEnv = program.commands.find((entry) => entry.name() === 'runtime-env-inspect')
		const run = program.commands.find((entry) => entry.name() === 'run')
		const comment = program.commands.find((entry) => entry.name() === 'comment')
		const reply = program.commands.find((entry) => entry.name() === 'reply')

		expect(runtimeEnv?.options.map((option) => option.long)).toContain(
			'--codex-app-server-environment-timeout-ms',
		)
		expect(runtimeEnv?.options.map((option) => option.long)).toContain('--redacted')
		expect(run?.options.map((option) => option.long)).toContain(
			'--codex-app-server-execution-timeout-ms',
		)
		expect(comment?.options.map((option) => option.long)).toContain(
			'--codex-app-server-comment-timeout-ms',
		)
		expect(reply?.options.map((option) => option.long)).toContain(
			'--codex-app-server-reply-timeout-ms',
		)
	})
})
