import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import type { AgentFile } from '../../src/core/agent-file.js'
import { AgentLifecycleService } from '../../src/core/agent-lifecycle.js'
import { BuilderAgentService } from '../../src/core/builder-service.js'
import type { AppError } from '../../src/core/errors.js'
import type { JsonObject } from '../../src/core/json.js'
import { loadAndValidateAgentFile } from '../../src/core/schema.js'
import { SQLiteLocalStateStore } from '../../src/core/state/index.js'
import { buildCliProgram } from '../../src/interfaces/cli.js'
import type {
	RuntimeAdapter,
	RuntimeAdapterCapabilities,
	RuntimeAdapterExecutionRequest,
	RuntimeEvent,
	RuntimeExecutionSession,
	RuntimeTerminalResult,
} from '../../src/ports/runtime.js'

const storesToClose: SQLiteLocalStateStore[] = []
const tempDirsToRemove: string[] = []

function emptyEventStream(): AsyncIterable<RuntimeEvent> {
	return {
		async *[Symbol.asyncIterator]() {
			// Intentionally empty.
		},
	}
}

type BuilderResponseDescriptor =
	| {
			kind: 'success-json'
			value: JsonObject
	  }
	| {
			kind: 'failure'
			outcome: Extract<
				RuntimeTerminalResult['outcome'],
				'invalid_output' | 'runtime_error' | 'interrupted' | 'cancelled'
			>
			code: string
			message: string
	  }

async function createStore(): Promise<SQLiteLocalStateStore> {
	const tempDir = await mkdtemp(path.join(os.tmpdir(), 'dennett-phase10-builder-'))
	const store = new SQLiteLocalStateStore({
		database_path: path.join(tempDir, 'local-state.sqlite'),
	})
	storesToClose.push(store)
	tempDirsToRemove.push(tempDir)
	return store
}

function toJsonObject(value: unknown): JsonObject {
	return JSON.parse(JSON.stringify(value)) as JsonObject
}

function buildCandidateAgent(args: {
	id: string
	name: string
	prompt: string
	description?: string
}): AgentFile {
	return {
		graph_contract_version: '1.0',
		meta: {
			id: args.id,
			name: args.name,
			...(args.description ? { description: args.description } : {}),
		},
		entry_node_id: 'start',
		final_output: {
			mode: 'last_node_output',
		},
		nodes: [
			{
				id: 'start',
				kind: 'runtime_agent',
				runtime_adapter: 'codex',
				prompt: args.prompt,
				input: {
					parts: [
						{
							type: 'ref',
							ref: 'params.input',
						},
					],
				},
				output: {
					mode: 'text',
				},
			},
		],
	}
}

function buildRicherCandidateAgent(id: string): AgentFile {
	return {
		graph_contract_version: '1.0',
		meta: {
			id,
			name: 'Builder 2.0 Rich Agent',
			description: 'Exercises richer portable public contract surfaces.',
		},
		entry_node_id: 'plan',
		params: {
			task: {
				type: 'string',
				required: true,
				description: 'User task to plan and review.',
			},
		},
		memory_bindings: [
			{
				id: 'project_memory',
				kind: 'runtime_memory',
				codex_ref: 'memory://project',
				scope: 'agent',
				config: {
					intent: {
						summary: 'Project-level memory used for retrieval and durable summaries.',
						labels: ['project', 'builder'],
					},
					required_capabilities: ['read', 'write', 'rag_retrieval'],
					transport_preferences: {
						preferred: ['api'],
					},
					provider_extension: {
						provider: 'mem0',
						transport: 'api',
						config: {
							mem0_config: {
								graph_store: {
									provider: 'networkx',
									config: {},
								},
							},
						},
					},
				},
			},
		],
		runtime_sources: [
			{
				id: 'primary_codex',
				runtime_adapter: 'codex',
				source_ref: 'workspace://primary',
				description: 'Portable primary runtime source reference.',
			},
		],
		interaction: {
			comments: {
				enabled: true,
				target_node_ids: ['plan'],
			},
			user_mcp: {
				enabled: true,
				server_name: 'orchestrator.user_chat',
			},
		},
		chat: {
			prefer_native_resume: true,
			store_visible_messages: true,
			store_context_window: true,
			allow_fresh_start: true,
			secret_markers: {
				enabled: true,
				open_marker: '[[SECRET]]',
				close_marker: '[[/SECRET]]',
			},
		},
		final_output: {
			mode: 'last_node_output',
		},
		nodes: [
			{
				id: 'plan',
				kind: 'runtime_agent',
				runtime_adapter: 'codex',
				prompt: 'Plan the work using public portable runtime, memory, and interaction surfaces.',
				input: {
					parts: [{ type: 'ref', ref: 'params.task' }],
				},
				output: {
					mode: 'json',
					schema: {
						type: 'object',
						additionalProperties: true,
					},
				},
				memory_ids: ['project_memory'],
				runtime_options: {
					model: 'gpt-5.3-codex',
					reasoning_effort: 'high',
					speed_tier: 'standard',
					personality: 'concise',
				},
				runtime_source_policy: 'prefer_first',
				runtime_source_ids: ['primary_codex'],
			},
			{
				id: 'review',
				kind: 'orchestrator_agent',
				agent_ref: 'agent.managed-reviewer',
				input: {
					parts: [{ type: 'ref', ref: 'node.plan.json.summary' }],
				},
				output: {
					mode: 'text',
				},
			},
		],
		edges: [
			{
				from: 'plan',
				to: 'review',
			},
		],
	}
}

async function writeAgentFile(
	tempDir: string,
	fileName: string,
	agentFile: AgentFile,
): Promise<string> {
	const filePath = path.join(tempDir, fileName)
	await writeFile(filePath, `${JSON.stringify(agentFile, null, 2)}\n`, 'utf8')
	return filePath
}

function createBuilderStubAdapter(responses: BuilderResponseDescriptor[]) {
	const requests: RuntimeAdapterExecutionRequest[] = []
	const capabilities: RuntimeAdapterCapabilities = {
		supports_native_resume: false,
		supports_live_comments: false,
		supports_builtin_user_chat_mcp: false,
		supports_memory_bindings: false,
		supports_model_discovery: false,
		supports_runtime_environment_introspection: false,
		supports_reasoning_effort: false,
		supports_speed_tiers: false,
		supports_personality: false,
		supports_explicit_runtime_source: false,
		supports_runtime_source_introspection: false,
	}

	const adapter: RuntimeAdapter = {
		describeCapabilities() {
			return capabilities
		},
		async startExecution(request) {
			requests.push(request)
			const next = responses.shift()
			if (!next) {
				throw new Error('No builder response configured for test adapter.')
			}

			const terminal_result: RuntimeTerminalResult =
				next.kind === 'success-json'
					? {
							outcome: 'success',
							output:
								request.output.mode === 'json'
									? request.output
									: (() => {
											throw new Error('Builder test adapter expected json output.')
										})(),
							output_json: next.value,
						}
					: {
							outcome: next.outcome,
							error: {
								code: next.code,
								message: next.message,
							},
						}

			const execution: RuntimeExecutionSession = {
				runtime_handle: null,
				native_session_handle: null,
				terminal_result: Promise.resolve(terminal_result),
				events: emptyEventStream(),
			}
			return execution
		},
		async listModels() {
			throw new Error('not used in tests')
		},
		async inspectRuntimeEnvironment() {
			throw new Error('not used in tests')
		},
		async inspectRuntimeSource() {
			throw new Error('not used in tests')
		},
		async deliverComment() {
			throw new Error('not used in tests')
		},
		async deliverUserChatResponse() {
			throw new Error('not used in tests')
		},
		async cancelExecution() {
			throw new Error('not used in tests')
		},
	}

	return {
		adapter,
		requests,
	}
}

afterEach(async () => {
	while (storesToClose.length > 0) {
		storesToClose.pop()?.close()
	}

	while (tempDirsToRemove.length > 0) {
		const tempDir = tempDirsToRemove.pop()
		if (tempDir) {
			await rm(tempDir, { recursive: true, force: true })
		}
	}
})

describe('BuilderAgentService', () => {
	it('creates a validated draft by default through the builder system agent resource', async () => {
		const store = await createStore()
		const { adapter, requests } = createBuilderStubAdapter([
			{
				kind: 'success-json',
				value: {
					agent_file: toJsonObject(
						buildCandidateAgent({
							id: 'agent.builder.created',
							name: 'Builder Created Agent',
							prompt: 'Return the built draft response.',
						}),
					),
				},
			},
		])
		const service = new BuilderAgentService({
			state_store: store,
			runtime_adapter: adapter,
		})

		const result = await service.buildAgentDraft({
			target_agent_id: 'agent.builder.created',
			request: 'Create a simple drafting agent.',
		})

		expect(result.operation).toBe('create')
		expect(result.builder_run_id).toBeTruthy()
		expect(result.draft.revision).toMatchObject({
			logical_agent_id: 'agent.builder.created',
			revision_kind: 'draft',
			availability_state: 'available',
		})
		expect(result.draft.status.agent.live_revision_id).toBeNull()
		expect(requests).toHaveLength(1)
		expect(requests[0]?.prompt).toContain('built-in builder system agent')

		const builderContext = JSON.parse(String(requests[0]?.input_message)) as {
			operation: string
			existing_agent_file: unknown
			target_agent: { id: string }
		}
		expect(builderContext).toMatchObject({
			operation: 'create',
			existing_agent_file: null,
			target_agent: {
				id: 'agent.builder.created',
			},
		})

		await expect(loadAndValidateAgentFile(result.draft.revision.file_path)).resolves.toMatchObject({
			meta: {
				id: 'agent.builder.created',
				name: 'Builder Created Agent',
			},
		})
	})

	it('accepts richer portable candidates and keeps them draft-only through lifecycle persistence', async () => {
		const store = await createStore()
		const { adapter, requests } = createBuilderStubAdapter([
			{
				kind: 'success-json',
				value: {
					agent_file: toJsonObject(buildRicherCandidateAgent('agent.builder.rich')),
				},
			},
		])
		const service = new BuilderAgentService({
			state_store: store,
			runtime_adapter: adapter,
		})

		const result = await service.buildAgentDraft({
			target_agent_id: 'agent.builder.rich',
			request:
				'Create an agent using portable memory bindings, runtime source selection, user interaction, and managed reviewer handoff patterns.',
		})

		expect(result.operation).toBe('create')
		expect(result.draft.revision.revision_kind).toBe('draft')
		expect(result.draft.status.agent.live_revision_id).toBeNull()
		expect(result.candidate_agent_file).toMatchObject({
			meta: {
				id: 'agent.builder.rich',
			},
			interaction: {
				user_mcp: {
					server_name: 'orchestrator.user_chat',
				},
			},
			memory_bindings: [
				{
					id: 'project_memory',
					config: {
						provider_extension: {
							provider: 'mem0',
						},
					},
				},
			],
			runtime_sources: [
				{
					id: 'primary_codex',
					runtime_adapter: 'codex',
				},
			],
			nodes: expect.arrayContaining([
				expect.objectContaining({
					id: 'plan',
					kind: 'runtime_agent',
					memory_ids: ['project_memory'],
					runtime_source_policy: 'prefer_first',
					runtime_source_ids: ['primary_codex'],
					runtime_options: expect.objectContaining({
						reasoning_effort: 'high',
						speed_tier: 'standard',
					}),
				}),
				expect.objectContaining({
					id: 'review',
					kind: 'orchestrator_agent',
					agent_ref: 'agent.managed-reviewer',
				}),
			]),
		})

		const builderContext = JSON.parse(String(requests[0]?.input_message)) as {
			constraints: { public_contract_only: boolean }
			portable_authoring_guidance: {
				allowed_public_surfaces: string[]
				memory_bindings: { forbidden_local_data: string[] }
				managed_subagents: { forbidden_hidden_data: string[] }
			}
		}
		expect(builderContext.constraints.public_contract_only).toBe(true)
		expect(builderContext.portable_authoring_guidance.allowed_public_surfaces).toEqual(
			expect.arrayContaining([
				'memory_bindings',
				'runtime_sources',
				'runtime_options',
				'interaction',
				'orchestrator_agent',
			]),
		)
		expect(builderContext.portable_authoring_guidance.memory_bindings.forbidden_local_data).toEqual(
			expect.arrayContaining(['credentials', 'python executables', 'rate limits']),
		)
		expect(
			builderContext.portable_authoring_guidance.managed_subagents.forbidden_hidden_data,
		).toEqual(expect.arrayContaining(['managed task-package snapshots']))

		await expect(loadAndValidateAgentFile(result.draft.revision.file_path)).resolves.toMatchObject({
			meta: {
				id: 'agent.builder.rich',
				name: 'Builder 2.0 Rich Agent',
			},
		})
	})

	it('updates an existing agent using draft context and persists a new draft revision', async () => {
		const store = await createStore()
		const tempDir = path.dirname(store.database_path)
		const existingSourcePath = await writeAgentFile(
			tempDir,
			'existing-agent.json',
			buildCandidateAgent({
				id: 'agent.builder.updated',
				name: 'Existing Builder Agent',
				prompt: 'Original prompt.',
			}),
		)
		const lifecycle = new AgentLifecycleService({
			state_store: store,
		})
		const existingDraft = await lifecycle.registerAgentFile(existingSourcePath)
		const { adapter, requests } = createBuilderStubAdapter([
			{
				kind: 'success-json',
				value: {
					agent_file: toJsonObject(
						buildCandidateAgent({
							id: 'agent.builder.updated',
							name: 'Existing Builder Agent',
							prompt: 'Updated prompt.',
							description: 'Updated by builder.',
						}),
					),
				},
			},
		])
		const service = new BuilderAgentService({
			state_store: store,
			runtime_adapter: adapter,
		})

		const result = await service.buildAgentDraft({
			target_agent_id: 'agent.builder.updated',
			request: 'Revise the existing agent prompt.',
			revise: true,
		})

		expect(result.operation).toBe('update')
		expect(result.base_revision?.revision_id).toBe(existingDraft.revision.revision_id)
		expect(result.draft.status.draft_revisions).toHaveLength(2)

		const builderContext = JSON.parse(String(requests[0]?.input_message)) as {
			operation: string
			existing_agent_file: {
				meta: {
					id: string
					name: string
				}
			} | null
		}
		expect(builderContext).toMatchObject({
			operation: 'update',
			existing_agent_file: {
				meta: {
					id: 'agent.builder.updated',
					name: 'Existing Builder Agent',
				},
			},
		})
	})

	it('fails create when the logical agent id is already known in the registry', async () => {
		const store = await createStore()
		const lifecycle = new AgentLifecycleService({
			state_store: store,
		})
		lifecycle.registerTrigger({
			trigger_id: 'trigger.builder.registry-only',
			logical_agent_id: 'agent.builder.registry-only',
			trigger_ref: 'mailbox://builder-registry-only',
		})

		const { adapter, requests } = createBuilderStubAdapter([
			{
				kind: 'success-json',
				value: {
					agent_file: toJsonObject(
						buildCandidateAgent({
							id: 'agent.builder.registry-only',
							name: 'Registry Only Agent',
							prompt: 'Created from registry-only context.',
						}),
					),
				},
			},
		])
		const service = new BuilderAgentService({
			state_store: store,
			runtime_adapter: adapter,
		})

		await expect(
			service.buildAgentDraft({
				target_agent_id: 'agent.builder.registry-only',
				request: 'Create from a trigger-only registry row.',
			}),
		).rejects.toMatchObject({
			code: 'BUILDER_AGENT_ALREADY_EXISTS',
		} satisfies Partial<AppError>)
		expect(requests).toHaveLength(0)
	})

	it('fails explicit revise when no usable revision base exists', async () => {
		const store = await createStore()
		const lifecycle = new AgentLifecycleService({
			state_store: store,
		})
		lifecycle.registerTrigger({
			trigger_id: 'trigger.builder.revise-missing-base',
			logical_agent_id: 'agent.builder.revise-missing-base',
			trigger_ref: 'mailbox://builder-revise-missing-base',
		})

		const { adapter, requests } = createBuilderStubAdapter([
			{
				kind: 'success-json',
				value: {
					agent_file: toJsonObject(
						buildCandidateAgent({
							id: 'agent.builder.revise-missing-base',
							name: 'Missing Base Agent',
							prompt: 'Should never run.',
						}),
					),
				},
			},
		])
		const service = new BuilderAgentService({
			state_store: store,
			runtime_adapter: adapter,
		})

		await expect(
			service.buildAgentDraft({
				target_agent_id: 'agent.builder.revise-missing-base',
				request: 'Revise without a usable base.',
				revise: true,
			}),
		).rejects.toMatchObject({
			code: 'BUILDER_REVISION_BASE_NOT_FOUND',
		} satisfies Partial<AppError>)
		expect(requests).toHaveLength(0)
	})

	it('rejects invalid builder candidates before any draft revision is persisted', async () => {
		const store = await createStore()
		const lifecycle = new AgentLifecycleService({
			state_store: store,
		})
		const { adapter } = createBuilderStubAdapter([
			{
				kind: 'success-json',
				value: {
					agent_file: {
						meta: {
							id: 'agent.builder.invalid',
							name: 'Invalid Builder Agent',
						},
						entry_node_id: 'start',
						nodes: [],
					},
				},
			},
		])
		const service = new BuilderAgentService({
			state_store: store,
			runtime_adapter: adapter,
		})

		await expect(
			service.buildAgentDraft({
				target_agent_id: 'agent.builder.invalid',
				request: 'Create something invalid.',
			}),
		).rejects.toMatchObject({
			code: 'BUILDER_CANDIDATE_INVALID',
		} satisfies Partial<AppError>)

		await expect(lifecycle.getAgentStatus('agent.builder.invalid')).rejects.toMatchObject({
			code: 'AGENT_NOT_FOUND',
		} satisfies Partial<AppError>)
	})

	it.each([
		{
			name: 'hidden builder-only managed-subagent internals',
			id: 'agent.builder.hidden-managed-data',
			candidate: {
				...toJsonObject(
					buildCandidateAgent({
						id: 'agent.builder.hidden-managed-data',
						name: 'Hidden Managed Data Agent',
						prompt: 'Should not persist.',
					}),
				),
				managed_subagent_task_package: {
					write_set: ['src/core/example.ts'],
				},
			},
		},
		{
			name: 'local Mem0 provider registration data',
			id: 'agent.builder.local-provider-data',
			candidate: {
				...toJsonObject(buildRicherCandidateAgent('agent.builder.local-provider-data')),
				memory_bindings: [
					{
						id: 'project_memory',
						kind: 'runtime_memory',
						codex_ref: 'memory://project',
						scope: 'agent',
						config: {
							intent: {
								summary: 'Invalid local provider override.',
							},
							required_capabilities: ['read'],
							provider_extension: {
								provider: 'mem0',
								config: {
									python_executable: 'C:/local/python.exe',
								},
							},
						},
					},
				],
			},
		},
		{
			name: 'runtime account and rate-limit metadata',
			id: 'agent.builder.runtime-account-data',
			candidate: {
				...toJsonObject(
					buildCandidateAgent({
						id: 'agent.builder.runtime-account-data',
						name: 'Runtime Account Data Agent',
						prompt: 'Should not persist.',
					}),
				),
				runtime_sources: [
					{
						id: 'primary_codex',
						runtime_adapter: 'codex',
						source_ref: 'workspace://primary',
						account: {
							email: 'user@example.com',
						},
						rate_limits: [],
					},
				],
			},
		},
	])('rejects builder candidates containing $name before persistence', async ({
		id,
		candidate,
	}) => {
		const store = await createStore()
		const lifecycle = new AgentLifecycleService({
			state_store: store,
		})
		const { adapter } = createBuilderStubAdapter([
			{
				kind: 'success-json',
				value: {
					agent_file: candidate,
				},
			},
		])
		const service = new BuilderAgentService({
			state_store: store,
			runtime_adapter: adapter,
		})

		await expect(
			service.buildAgentDraft({
				target_agent_id: id,
				request: 'Create an agent but incorrectly include local or hidden data.',
			}),
		).rejects.toMatchObject({
			code: 'BUILDER_CANDIDATE_INVALID',
		} satisfies Partial<AppError>)

		await expect(lifecycle.getAgentStatus(id)).rejects.toMatchObject({
			code: 'AGENT_NOT_FOUND',
		} satisfies Partial<AppError>)
	})

	it('stores accepted builder results as drafts only', async () => {
		const store = await createStore()
		const { adapter } = createBuilderStubAdapter([
			{
				kind: 'success-json',
				value: {
					agent_file: toJsonObject(
						buildCandidateAgent({
							id: 'agent.builder.deploy',
							name: 'Deployable Builder Agent',
							prompt: 'Deploy prompt.',
						}),
					),
				},
			},
		])
		const service = new BuilderAgentService({
			state_store: store,
			runtime_adapter: adapter,
		})

		const result = await service.buildAgentDraft({
			target_agent_id: 'agent.builder.deploy',
			request: 'Create this agent as a draft.',
		})

		expect(result.draft.revision.revision_kind).toBe('draft')
		expect(result.draft.status.agent.live_revision_id).toBeNull()
	})
})

describe('builder CLI', () => {
	it('exposes a builder command with draft-by-default flags', async () => {
		let capturedOptions:
			| {
					request: string
					name?: string
					description?: string
					revise?: boolean
					runId?: string
					stateDb: string
			  }
			| undefined
		const program = buildCliProgram()
		program.exitOverride()
		const builderCommand = program.commands.find((entry) => entry.name() === 'builder')
		if (!builderCommand) {
			throw new Error('expected builder CLI command')
		}

		builderCommand.action(async (...args: unknown[]) => {
			capturedOptions = args.at(-2) as typeof capturedOptions
		})

		await program.parseAsync(
			[
				'builder',
				'agent.builder.cli',
				'--request',
				'Create from CLI.',
				'--name',
				'CLI Agent',
				'--description',
				'CLI description',
				'--revise',
				'--run-id',
				'builder-run-1',
			],
			{ from: 'user' },
		)

		expect(capturedOptions).toEqual({
			request: 'Create from CLI.',
			name: 'CLI Agent',
			description: 'CLI description',
			revise: true,
			runId: 'builder-run-1',
			stateDb: expect.any(String),
		})
	})

	it('does not advertise the unsupported builder runtime-source narrowing option', async () => {
		const program = buildCliProgram()
		program.exitOverride()
		const builderCommand = program.commands.find((entry) => entry.name() === 'builder')
		if (!builderCommand) {
			throw new Error('expected builder CLI command')
		}

		expect(builderCommand.options.some((option) => option.long === '--runtime-source-id')).toBe(
			false,
		)

		await expect(
			program.parseAsync(
				[
					'builder',
					'agent.builder.cli',
					'--request',
					'Create from CLI.',
					'--runtime-source-id',
					'source-a',
				],
				{ from: 'user' },
			),
		).rejects.toBeDefined()
	})

	it('does not advertise inline deploy on the builder command', async () => {
		const program = buildCliProgram()
		program.exitOverride()
		const builderCommand = program.commands.find((entry) => entry.name() === 'builder')
		if (!builderCommand) {
			throw new Error('expected builder CLI command')
		}

		expect(builderCommand.options.some((option) => option.long === '--deploy')).toBe(false)

		await expect(
			program.parseAsync(
				['builder', 'agent.builder.cli', '--request', 'Create from CLI.', '--deploy'],
				{ from: 'user' },
			),
		).rejects.toBeDefined()
	})
})
