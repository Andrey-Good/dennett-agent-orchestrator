import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { CodexAppServerRuntimeAdapter } from '../../src/adapters/codex/codex-app-server-runtime-adapter.js'
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

function repeatBuilderResponse(response: BuilderResponseDescriptor): BuilderResponseDescriptor[] {
	return [response, response]
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

function buildPermissiveBuilderAgentResource(): AgentFile {
	return {
		graph_contract_version: '1.0',
		meta: {
			id: 'system.builder.test-permissive',
			name: 'Permissive Test Builder Agent',
		},
		entry_node_id: 'builder',
		params: {
			context: {
				type: 'object',
				required: true,
			},
		},
		final_output: {
			mode: 'last_node_output',
		},
		nodes: [
			{
				id: 'builder',
				kind: 'runtime_agent',
				runtime_adapter: 'codex',
				prompt: 'Return the raw builder output for service-level wrapper validation.',
				input: {
					parts: [
						{
							type: 'ref',
							ref: 'params.context',
						},
					],
				},
				output: {
					mode: 'json',
					schema: {
						type: 'object',
						additionalProperties: true,
					},
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
					speed_tier: 'fast',
					personality: 'pragmatic',
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

function createBuilderStubAdapter(
	responses: BuilderResponseDescriptor[],
	capabilityOverrides: Partial<RuntimeAdapterCapabilities> = {},
) {
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
		...capabilityOverrides,
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
		expect(result.candidate_diagnostics).toMatchObject({
			status: 'accepted',
			issues: [],
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
		const { adapter, requests } = createBuilderStubAdapter(
			[
				{
					kind: 'success-json',
					value: {
						agent_file: toJsonObject(buildRicherCandidateAgent('agent.builder.rich')),
					},
				},
			],
			{
				supports_builtin_user_chat_mcp: true,
				supports_live_comments: true,
				supports_memory_bindings: true,
				supports_reasoning_effort: true,
				supports_speed_tiers: true,
				supports_personality: true,
				supports_explicit_runtime_source: true,
			},
		)
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
		expect(result.candidate_diagnostics.status).toBe('accepted')
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
						speed_tier: 'fast',
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
				'initial_vars',
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

	it('repairs a first invalid builder wrapper once and persists only the repaired candidate', async () => {
		const store = await createStore()
		const lifecycle = new AgentLifecycleService({
			state_store: store,
		})
		const repairedCandidate = buildCandidateAgent({
			id: 'agent.builder.repaired',
			name: 'Repaired Builder Agent',
			prompt: 'Use the repaired valid draft.',
		})
		const { adapter, requests } = createBuilderStubAdapter([
			{
				kind: 'success-json',
				value: {
					agent_file: toJsonObject(
						buildCandidateAgent({
							id: 'agent.builder.repaired',
							name: 'Invalid Wrapper Candidate',
							prompt: 'This candidate must not persist.',
						}),
					),
					diagnostics: [],
				},
			},
			{
				kind: 'success-json',
				value: {
					agent_file: toJsonObject(repairedCandidate),
				},
			},
		])
		const service = new BuilderAgentService({
			state_store: store,
			runtime_adapter: adapter,
			builder_agent_resource: buildPermissiveBuilderAgentResource(),
		})

		const result = await service.buildAgentDraft({
			target_agent_id: 'agent.builder.repaired',
			request: 'Create an agent and repair validation failures if needed.',
		})

		expect(requests).toHaveLength(2)
		expect(result.candidate_agent_file.meta.name).toBe('Repaired Builder Agent')
		expect(result.candidate_diagnostics.status).toBe('accepted')
		const repairContext = JSON.parse(String(requests[1]?.input_message)) as {
			repair_attempt?: {
				attempt_number: number
				max_attempts: number
				previous_failure: {
					code: string
					gate: string
					extra_properties: string[]
				}
			}
		}
		expect(repairContext.repair_attempt).toMatchObject({
			attempt_number: 2,
			max_attempts: 2,
			previous_failure: {
				code: 'BUILDER_INVALID_OUTPUT',
				gate: 'wrapper_extraction',
				extra_properties: ['diagnostics'],
			},
		})

		const status = await lifecycle.getAgentStatus('agent.builder.repaired')
		expect(status.draft_revisions).toHaveLength(1)
		await expect(loadAndValidateAgentFile(result.draft.revision.file_path)).resolves.toMatchObject({
			meta: {
				id: 'agent.builder.repaired',
				name: 'Repaired Builder Agent',
			},
		})
	})

	it('stops after one repair attempt when the repaired candidate is still invalid', async () => {
		const store = await createStore()
		const lifecycle = new AgentLifecycleService({
			state_store: store,
		})
		const invalidResponse: BuilderResponseDescriptor = {
			kind: 'success-json',
			value: {
				agent_file: {
					meta: {
						id: 'agent.builder.unrepaired',
						name: 'Still Invalid Builder Agent',
					},
					entry_node_id: 'start',
					nodes: [],
				},
			},
		}
		const { adapter, requests } = createBuilderStubAdapter(repeatBuilderResponse(invalidResponse))
		const service = new BuilderAgentService({
			state_store: store,
			runtime_adapter: adapter,
		})

		await expect(
			service.buildAgentDraft({
				target_agent_id: 'agent.builder.unrepaired',
				request: 'Create an agent that remains invalid after repair.',
			}),
		).rejects.toMatchObject({
			code: 'BUILDER_CANDIDATE_INVALID',
			message: expect.stringContaining('Builder repair attempt failed'),
			details: {
				builder_repair: {
					max_attempts: 2,
					persisted: false,
					attempts: [
						expect.objectContaining({
							attempt_number: 1,
							gate: 'schema_validation',
						}),
						expect.objectContaining({
							attempt_number: 2,
							gate: 'schema_validation',
						}),
					],
				},
			},
		} satisfies Partial<AppError>)
		expect(requests).toHaveLength(2)
		await expect(lifecycle.getAgentStatus('agent.builder.unrepaired')).rejects.toMatchObject({
			code: 'AGENT_NOT_FOUND',
		} satisfies Partial<AppError>)
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
		const { adapter } = createBuilderStubAdapter(
			repeatBuilderResponse({
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
			}),
		)
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

	it('rejects builder output wrappers with extra sibling fields before any draft revision is persisted', async () => {
		const store = await createStore()
		const lifecycle = new AgentLifecycleService({
			state_store: store,
		})
		const { adapter } = createBuilderStubAdapter(
			repeatBuilderResponse({
				kind: 'success-json',
				value: {
					agent_file: toJsonObject(
						buildCandidateAgent({
							id: 'agent.builder.extra-wrapper-field',
							name: 'Extra Wrapper Field Agent',
							prompt: 'Should not persist.',
						}),
					),
					diagnostics: [],
				},
			}),
		)
		const service = new BuilderAgentService({
			state_store: store,
			runtime_adapter: adapter,
			builder_agent_resource: buildPermissiveBuilderAgentResource(),
		})

		await expect(
			service.buildAgentDraft({
				target_agent_id: 'agent.builder.extra-wrapper-field',
				request: 'Create an agent but include wrapper diagnostics.',
			}),
		).rejects.toMatchObject({
			code: 'BUILDER_INVALID_OUTPUT',
			message: expect.stringContaining('"diagnostics"'),
			details: {
				extra_properties: ['diagnostics'],
			},
		} satisfies Partial<AppError>)

		await expect(
			lifecycle.getAgentStatus('agent.builder.extra-wrapper-field'),
		).rejects.toMatchObject({
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
		const { adapter } = createBuilderStubAdapter(
			repeatBuilderResponse({
				kind: 'success-json',
				value: {
					agent_file: candidate,
				},
			}),
		)
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

	it('rejects provider secrets smuggled through otherwise schema-valid memory fields', async () => {
		const store = await createStore()
		const lifecycle = new AgentLifecycleService({
			state_store: store,
		})
		const candidate = toJsonObject(
			buildCandidateAgent({
				id: 'agent.builder.secret-memory-config',
				name: 'Secret Memory Config Agent',
				prompt: 'Should not persist.',
			}),
		)
		candidate.memory_bindings = [
			{
				id: 'project_memory',
				kind: 'runtime_memory',
				codex_ref: 'memory://project',
				scope: 'agent',
				config: {
					intent: {
						summary: 'Portable memory intent with forbidden custom provider secret.',
					},
					required_capabilities: ['read'],
					provider_extension: {
						provider: 'custom-memory',
						config: {
							api_key: 'do-not-save',
						},
					},
				},
			},
		]
		const firstNode = (candidate.nodes as unknown as JsonObject[])[0]
		if (!firstNode) {
			throw new Error('expected first node')
		}
		firstNode.memory_ids = ['project_memory']
		const { adapter } = createBuilderStubAdapter(
			repeatBuilderResponse({
				kind: 'success-json',
				value: {
					agent_file: candidate,
				},
			}),
			{
				supports_memory_bindings: true,
			},
		)
		const service = new BuilderAgentService({
			state_store: store,
			runtime_adapter: adapter,
		})

		await expect(
			service.buildAgentDraft({
				target_agent_id: 'agent.builder.secret-memory-config',
				request: 'Create an agent with memory provider secrets.',
			}),
		).rejects.toMatchObject({
			code: 'BUILDER_CANDIDATE_AUDIT_REJECTED',
			details: {
				candidate_diagnostics: {
					issues: expect.arrayContaining([
						expect.objectContaining({
							code: 'LOCAL_PROVIDER_DATA_FORBIDDEN',
							path: '/memory_bindings/0/config/provider_extension/config/api_key',
						}),
					]),
				},
			},
		} satisfies Partial<AppError>)
		await expect(
			lifecycle.getAgentStatus('agent.builder.secret-memory-config'),
		).rejects.toMatchObject({
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

	it('rejects runtime_options.speed_tier standard during candidate audit before persistence', async () => {
		const store = await createStore()
		const lifecycle = new AgentLifecycleService({
			state_store: store,
		})
		const candidate = toJsonObject(
			buildCandidateAgent({
				id: 'agent.builder.invalid-speed-tier',
				name: 'Invalid Speed Tier Agent',
				prompt: 'Should not persist.',
			}),
		)
		const firstNode = (candidate.nodes as unknown as JsonObject[])[0]
		if (!firstNode) {
			throw new Error('expected first node')
		}
		firstNode.runtime_options = {
			speed_tier: 'standard',
		}
		const { adapter } = createBuilderStubAdapter(
			repeatBuilderResponse({
				kind: 'success-json',
				value: {
					agent_file: candidate,
				},
			}),
			{
				supports_speed_tiers: true,
			},
		)
		const service = new BuilderAgentService({
			state_store: store,
			runtime_adapter: adapter,
		})

		let caught: unknown
		try {
			await service.buildAgentDraft({
				target_agent_id: 'agent.builder.invalid-speed-tier',
				request: 'Create an agent with an invalid speed tier.',
			})
		} catch (error) {
			caught = error
		}

		expect(caught).toMatchObject({
			code: 'BUILDER_CANDIDATE_AUDIT_REJECTED',
			details: {
				candidate_diagnostics: {
					status: 'rejected',
					issues: expect.arrayContaining([
						expect.objectContaining({
							code: 'RUNTIME_OPTION_INVALID_VALUE',
							path: '/nodes/0/runtime_options/speed_tier',
						}),
					]),
				},
			},
		} satisfies Partial<AppError>)
		await expect(
			lifecycle.getAgentStatus('agent.builder.invalid-speed-tier'),
		).rejects.toMatchObject({
			code: 'AGENT_NOT_FOUND',
		} satisfies Partial<AppError>)
	})

	it.each([
		'fast',
		'flex',
	] as const)('accepts runtime_options.speed_tier %s when the selected runtime supports speed tiers', async (speedTier) => {
		const store = await createStore()
		const candidate = toJsonObject(
			buildCandidateAgent({
				id: `agent.builder.${speedTier}`,
				name: `Speed ${speedTier} Agent`,
				prompt: 'Persist valid speed tier.',
			}),
		)
		const firstNode = (candidate.nodes as unknown as JsonObject[])[0]
		if (!firstNode) {
			throw new Error('expected first node')
		}
		firstNode.runtime_options = {
			speed_tier: speedTier,
		}
		const { adapter } = createBuilderStubAdapter(
			[
				{
					kind: 'success-json',
					value: {
						agent_file: candidate,
					},
				},
			],
			{
				supports_speed_tiers: true,
			},
		)
		const service = new BuilderAgentService({
			state_store: store,
			runtime_adapter: adapter,
		})

		const result = await service.buildAgentDraft({
			target_agent_id: `agent.builder.${speedTier}`,
			request: `Create an agent with ${speedTier} speed tier.`,
		})

		expect(result.candidate_diagnostics.status).toBe('accepted')
		expect(result.candidate_agent_file.nodes[0]).toMatchObject({
			runtime_options: {
				speed_tier: speedTier,
			},
		})
	})

	it.each([
		{
			name: 'unknown runtime option key',
			runtime_options: { temperature: 0.2 } as JsonObject,
			code: 'RUNTIME_OPTION_UNKNOWN',
			path: '/nodes/0/runtime_options/temperature',
		},
		{
			name: 'invalid runtime option type',
			runtime_options: { model: 7 } as JsonObject,
			code: 'RUNTIME_OPTION_INVALID_TYPE',
			path: '/nodes/0/runtime_options/model',
		},
	])('rejects $name during candidate audit before persistence', async ({
		runtime_options,
		code,
		path,
	}) => {
		const store = await createStore()
		const lifecycle = new AgentLifecycleService({
			state_store: store,
		})
		const candidate = toJsonObject(
			buildCandidateAgent({
				id: 'agent.builder.invalid-runtime-option',
				name: 'Invalid Runtime Option Agent',
				prompt: 'Should not persist.',
			}),
		)
		const firstNode = (candidate.nodes as unknown as JsonObject[])[0]
		if (!firstNode) {
			throw new Error('expected first node')
		}
		firstNode.runtime_options = runtime_options
		const { adapter } = createBuilderStubAdapter(
			repeatBuilderResponse({
				kind: 'success-json',
				value: {
					agent_file: candidate,
				},
			}),
		)
		const service = new BuilderAgentService({
			state_store: store,
			runtime_adapter: adapter,
		})

		await expect(
			service.buildAgentDraft({
				target_agent_id: 'agent.builder.invalid-runtime-option',
				request: 'Create an agent with invalid runtime options.',
			}),
		).rejects.toMatchObject({
			code: 'BUILDER_CANDIDATE_AUDIT_REJECTED',
			details: {
				candidate_diagnostics: {
					issues: expect.arrayContaining([
						expect.objectContaining({
							code,
							path,
						}),
					]),
				},
			},
		} satisfies Partial<AppError>)
		await expect(
			lifecycle.getAgentStatus('agent.builder.invalid-runtime-option'),
		).rejects.toMatchObject({
			code: 'AGENT_NOT_FOUND',
		} satisfies Partial<AppError>)
	})

	it('rejects explicit runtime_sources when the selected runtime does not support them', async () => {
		const store = await createStore()
		const lifecycle = new AgentLifecycleService({
			state_store: store,
		})
		const candidate = toJsonObject(
			buildCandidateAgent({
				id: 'agent.builder.unsupported-runtime-source',
				name: 'Unsupported Runtime Source Agent',
				prompt: 'Should not persist.',
			}),
		)
		candidate.runtime_sources = [
			{
				id: 'primary_codex',
				runtime_adapter: 'codex',
				source_ref: 'workspace://primary',
			},
		]
		const firstNode = (candidate.nodes as unknown as JsonObject[])[0]
		if (!firstNode) {
			throw new Error('expected first node')
		}
		firstNode.runtime_source_policy = 'prefer_first'
		firstNode.runtime_source_ids = ['primary_codex']
		const { adapter } = createBuilderStubAdapter(
			repeatBuilderResponse({
				kind: 'success-json',
				value: {
					agent_file: candidate,
				},
			}),
		)
		const service = new BuilderAgentService({
			state_store: store,
			runtime_adapter: adapter,
		})

		await expect(
			service.buildAgentDraft({
				target_agent_id: 'agent.builder.unsupported-runtime-source',
				request: 'Create an agent with explicit runtime sources.',
			}),
		).rejects.toMatchObject({
			code: 'BUILDER_CANDIDATE_AUDIT_REJECTED',
			details: {
				candidate_diagnostics: {
					issues: expect.arrayContaining([
						expect.objectContaining({
							code: 'RUNTIME_CAPABILITY_UNSUPPORTED',
							path: '/runtime_sources',
						}),
					]),
				},
			},
		} satisfies Partial<AppError>)
		await expect(
			lifecycle.getAgentStatus('agent.builder.unsupported-runtime-source'),
		).rejects.toMatchObject({
			code: 'AGENT_NOT_FOUND',
		} satisfies Partial<AppError>)
	})

	it('rejects invalid JSON output schemas during candidate audit before persistence', async () => {
		const store = await createStore()
		const lifecycle = new AgentLifecycleService({
			state_store: store,
		})
		const candidate = toJsonObject(
			buildCandidateAgent({
				id: 'agent.builder.invalid-output-schema',
				name: 'Invalid Output Schema Agent',
				prompt: 'Should not persist.',
			}),
		)
		const firstNode = (candidate.nodes as unknown as JsonObject[])[0]
		if (!firstNode) {
			throw new Error('expected first node')
		}
		firstNode.output = {
			mode: 'json',
			schema: {
				type: 'object',
				properties: {
					result: {
						type: 'not-a-json-schema-type',
					},
				},
			},
		}
		const { adapter } = createBuilderStubAdapter(
			repeatBuilderResponse({
				kind: 'success-json',
				value: {
					agent_file: candidate,
				},
			}),
		)
		const service = new BuilderAgentService({
			state_store: store,
			runtime_adapter: adapter,
		})

		await expect(
			service.buildAgentDraft({
				target_agent_id: 'agent.builder.invalid-output-schema',
				request: 'Create an agent with invalid JSON schema.',
			}),
		).rejects.toMatchObject({
			code: 'BUILDER_CANDIDATE_AUDIT_REJECTED',
			details: {
				candidate_diagnostics: {
					issues: expect.arrayContaining([
						expect.objectContaining({
							code: 'JSON_OUTPUT_SCHEMA_INVALID',
							path: '/nodes/0/output/schema',
						}),
					]),
				},
			},
		} satisfies Partial<AppError>)
		await expect(
			lifecycle.getAgentStatus('agent.builder.invalid-output-schema'),
		).rejects.toMatchObject({
			code: 'AGENT_NOT_FOUND',
		} satisfies Partial<AppError>)
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

	it('prints candidate diagnostics in successful builder CLI output', async () => {
		const tempDir = await mkdtemp(path.join(os.tmpdir(), 'dennett-builder-cli-diagnostics-'))
		tempDirsToRemove.push(tempDir)
		const stateDbPath = path.join(tempDir, 'local-state.sqlite')
		let stdout = ''
		const stdoutSpy = vi.spyOn(process.stdout, 'write').mockImplementation((chunk) => {
			stdout += String(chunk)
			return true
		})
		const describeCapabilitiesSpy = vi
			.spyOn(CodexAppServerRuntimeAdapter.prototype, 'describeCapabilities')
			.mockReturnValue({
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
			})
		const startExecutionSpy = vi
			.spyOn(CodexAppServerRuntimeAdapter.prototype, 'startExecution')
			.mockImplementation(async (request) => ({
				runtime_handle: null,
				native_session_handle: null,
				terminal_result: Promise.resolve({
					outcome: 'success',
					output:
						request.output.mode === 'json'
							? request.output
							: (() => {
									throw new Error('expected builder JSON output')
								})(),
					output_json: {
						agent_file: toJsonObject(
							buildCandidateAgent({
								id: 'agent.builder.cli.diagnostics',
								name: 'CLI Diagnostics Agent',
								prompt: 'Return a simple result.',
							}),
						),
					},
				}),
				events: emptyEventStream(),
			}))

		try {
			const program = buildCliProgram()
			program.exitOverride()
			await program.parseAsync(
				[
					'builder',
					'agent.builder.cli.diagnostics',
					'--request',
					'Create from CLI.',
					'--state-db',
					stateDbPath,
				],
				{ from: 'user' },
			)
		} finally {
			stdoutSpy.mockRestore()
			describeCapabilitiesSpy.mockRestore()
			startExecutionSpy.mockRestore()
		}

		const parsed = JSON.parse(stdout) as {
			candidate_diagnostics?: {
				status?: string
				issues?: unknown[]
			}
		}
		expect(parsed.candidate_diagnostics).toEqual({
			status: 'accepted',
			issues: [],
			capabilities: expect.any(Object),
		})
	})
})
