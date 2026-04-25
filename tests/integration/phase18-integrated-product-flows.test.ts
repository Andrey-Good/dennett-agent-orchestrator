import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { AgentFile, MemoryBinding } from '../../src/core/agent-file.js'
import { AgentLifecycleService } from '../../src/core/agent-lifecycle.js'
import { BuilderAgentService } from '../../src/core/builder-service.js'
import { resumeAgentRun, runAgentFile } from '../../src/core/graph-runner.js'
import type { JsonObject, JsonValue } from '../../src/core/json.js'
import {
	MEM0_PROVIDER_FAMILY,
	MemoryProviderRegistryService,
} from '../../src/core/memory-provider-registry.js'
import {
	MemoryService,
	type RuntimeMemorySuccessWriteResult,
} from '../../src/core/memory-service.js'
import type { MemoryProviderRecord } from '../../src/core/state/index.js'
import { SQLiteLocalStateStore } from '../../src/core/state/index.js'
import { ManagedSubagentService } from '../../src/core/subagent-service.js'
import type {
	RuntimeAdapter,
	RuntimeAdapterCapabilities,
	RuntimeAdapterExecutionRequest,
	RuntimeEvent,
	RuntimeExecutionSession,
	RuntimeMemoryBindingIntent,
	RuntimeMemoryCapability,
	RuntimeSourceInspectionResult,
	RuntimeTerminalResult,
} from '../../src/ports/runtime.js'

const storesToClose: SQLiteLocalStateStore[] = []
const tempDirsToRemove: string[] = []

const TEXT_OUTPUT = { mode: 'text' } as const
const JSON_OBJECT_OUTPUT = {
	mode: 'json',
	schema: {
		type: 'object',
		additionalProperties: true,
	},
} as const
const RUNTIME_MEMORY_CAPABILITIES = new Set<RuntimeMemoryCapability>([
	'read',
	'write',
	'entity_scoped',
	'user_scoped',
	'group_scoped',
	'session_scoped',
	'graph_context',
	'temporal_index',
	'profile_synthesis',
	'rag_retrieval',
	'infer_extract',
	'versioned_write',
	'mcp_transport',
])

type StubExecutionDescriptor =
	| RuntimeTerminalResult
	| {
			runtime_handle?: JsonValue | null
			native_session_handle?: JsonValue | null
			terminal_result: RuntimeTerminalResult | Promise<RuntimeTerminalResult>
			events?: AsyncIterable<RuntimeEvent>
	  }

function emptyEventStream(): AsyncIterable<RuntimeEvent> {
	return {
		async *[Symbol.asyncIterator]() {
			// Intentionally empty.
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

async function createStore(): Promise<SQLiteLocalStateStore> {
	const tempDir = await mkdtemp(path.join(os.tmpdir(), 'dennett-phase18-integration-'))
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

function memoryBindingConfig(binding: MemoryBinding): JsonObject {
	if (binding.config && typeof binding.config === 'object' && !Array.isArray(binding.config)) {
		return binding.config
	}

	return {}
}

function isRuntimeMemoryCapability(value: string): value is RuntimeMemoryCapability {
	return RUNTIME_MEMORY_CAPABILITIES.has(value as RuntimeMemoryCapability)
}

function memoryBindingRequiredCapabilities(binding: MemoryBinding): RuntimeMemoryCapability[] {
	const rawCapabilities = memoryBindingConfig(binding).required_capabilities
	if (!Array.isArray(rawCapabilities)) {
		return []
	}

	return [
		...new Set(
			rawCapabilities
				.map((capability) => String(capability))
				.filter((capability): capability is RuntimeMemoryCapability =>
					isRuntimeMemoryCapability(capability),
				),
		),
	]
}

function memoryBindingIntent(binding: MemoryBinding): RuntimeMemoryBindingIntent {
	const rawIntent = memoryBindingConfig(binding).intent
	const intent =
		rawIntent && typeof rawIntent === 'object' && !Array.isArray(rawIntent) ? rawIntent : {}
	const summary =
		typeof intent.summary === 'string' && intent.summary.length > 0
			? intent.summary
			: `Memory binding ${binding.id}`
	const labels = Array.isArray(intent.labels) ? intent.labels.map((label) => String(label)) : []

	return labels.length > 0
		? {
				summary,
				labels,
			}
		: {
				summary,
			}
}

function registerPhase18MemoryFixture(store: SQLiteLocalStateStore): {
	provider: MemoryProviderRecord
	preparedQueries: string[]
	writtenContents: string[]
} {
	const registry = new MemoryProviderRegistryService({
		state_store: store,
	})
	const provider = registry.registerProvider({
		provider_id: 'phase18-test-memory',
		codex_ref: 'memory://phase18/project',
		provider_family: MEM0_PROVIDER_FAMILY,
		display_name: 'Phase 18 Test Memory',
		transport: 'sdk',
		supported_capabilities: ['read', 'write', 'rag_retrieval'],
		config: {
			fixture: 'phase18-offline-memory',
		},
	})
	const preparedQueries: string[] = []
	const writtenContents: string[] = []

	vi.spyOn(MemoryService.prototype, 'prepareRuntimeMemoryBindingContext').mockImplementation(
		async ({ binding, scope, read }) => {
			const requiredCapabilities = memoryBindingRequiredCapabilities(binding)
			const resolvedProvider = registry.resolveProvider({
				codex_ref: binding.codex_ref,
				required_capabilities: requiredCapabilities,
			})
			const query = read?.query ?? ''
			preparedQueries.push(query)

			return {
				context: {
					binding_id: binding.id,
					codex_ref: binding.codex_ref,
					intent: memoryBindingIntent(binding),
					required_capabilities: requiredCapabilities,
					scope,
					read: read
						? {
								query,
								records: [
									{
										id: 'phase18-memory-record',
										content: 'Registered project memory for the Phase 18 integrated flow.',
										scope,
										metadata: {
											provider_id: resolvedProvider.provider_id,
										},
										score: 1,
									},
								],
							}
						: undefined,
					write: {
						enabled: true,
						mode: 'node_success_output',
					},
				},
				provider: resolvedProvider,
				read_enabled: requiredCapabilities.includes('read'),
				write_enabled: requiredCapabilities.includes('write'),
				required_capabilities: requiredCapabilities,
			}
		},
	)

	vi.spyOn(MemoryService.prototype, 'writeRuntimeMemoryOnSuccess').mockImplementation(
		async ({ binding, content }): Promise<RuntimeMemorySuccessWriteResult> => {
			registry.resolveProvider({
				codex_ref: binding.codex_ref,
				required_capabilities: memoryBindingRequiredCapabilities(binding),
			})
			writtenContents.push(content)

			return {
				status: 'written',
				dennett_write_key: 'phase18-test-memory-write',
				metadata: {
					fixture: 'phase18-offline-memory',
				},
				result: {
					records: [
						{
							id: 'phase18-written-memory-record',
							content,
							scope: {},
						},
					],
				},
			}
		},
	)

	return {
		provider,
		preparedQueries,
		writtenContents,
	}
}

function buildPhase18CandidateAgent(agentId: string, childAgentId: string): AgentFile {
	return {
		graph_contract_version: '1.0',
		meta: {
			id: agentId,
			name: 'Phase 18 Integrated Parent Agent',
			description:
				'Offline integrated flow over builder, lifecycle, runtime, memory, interaction, and child run seams.',
		},
		entry_node_id: 'plan',
		params: {
			task: {
				type: 'string',
				required: true,
			},
		},
		memory_bindings: [
			{
				id: 'project-memory',
				kind: 'runtime_memory',
				codex_ref: 'memory://phase18/project',
				scope: 'agent',
				config: {
					intent: {
						summary: 'Project memory retrieved by the planning node.',
						labels: ['phase18', 'offline'],
					},
					required_capabilities: ['read', 'write', 'rag_retrieval'],
				},
			},
		],
		runtime_sources: [
			{
				id: 'primary-source',
				runtime_adapter: 'codex',
				source_ref: 'workspace://primary',
			},
			{
				id: 'fallback-source',
				runtime_adapter: 'codex',
				source_ref: 'workspace://fallback',
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
		},
		final_output: {
			mode: 'last_node_output',
		},
		nodes: [
			{
				id: 'plan',
				kind: 'runtime_agent',
				runtime_adapter: 'codex',
				prompt: 'Plan with memory, selected runtime source, and explicit user approval.',
				input: {
					parts: [{ type: 'ref', ref: 'params.task' }],
				},
				output: JSON_OBJECT_OUTPUT,
				memory_ids: ['project-memory'],
				runtime_options: {
					model: 'gpt-5.3-codex',
					reasoning_effort: 'high',
					speed_tier: 'fast',
					personality: 'pragmatic',
				},
				runtime_source_policy: 'restrict',
				runtime_source_ids: ['primary-source', 'fallback-source'],
			},
			{
				id: 'delegate',
				kind: 'orchestrator_agent',
				agent_ref: childAgentId,
				input: {
					parts: [
						{ type: 'text', text: 'Review approved plan: ' },
						{ type: 'ref', ref: 'node.plan.json.summary' },
					],
				},
				output: TEXT_OUTPUT,
			},
		],
		edges: [
			{
				from: 'plan',
				to: 'delegate',
			},
		],
	}
}

function buildChildAgent(agentId: string): AgentFile {
	return {
		graph_contract_version: '1.0',
		meta: {
			id: agentId,
			name: 'Phase 18 Child Reviewer',
		},
		entry_node_id: 'review',
		final_output: {
			mode: 'last_node_output',
		},
		nodes: [
			{
				id: 'review',
				kind: 'runtime_agent',
				runtime_adapter: 'codex',
				prompt: 'Return the delegated review result.',
				input: {
					parts: [{ type: 'ref', ref: 'params.input' }],
				},
				output: TEXT_OUTPUT,
			},
		],
	}
}

function buildManagedParentCoordinatorAgent(agentId: string): AgentFile {
	return {
		graph_contract_version: '1.0',
		meta: {
			id: agentId,
			name: 'Phase 18 Managed Subagent Coordinator',
		},
		entry_node_id: 'coordinate',
		final_output: {
			mode: 'none',
		},
		nodes: [
			{
				id: 'coordinate',
				kind: 'runtime_agent',
				runtime_adapter: 'codex',
				prompt: 'Coordinate managed subagents through the service boundary.',
				input: {
					parts: [{ type: 'ref', ref: 'params.task' }],
				},
				output: TEXT_OUTPUT,
			},
		],
	}
}

function buildManagedWorkerAgent(agentId: string): AgentFile {
	return {
		graph_contract_version: '1.0',
		meta: {
			id: agentId,
			name: 'Phase 18 Managed Worker',
		},
		entry_node_id: 'work',
		final_output: {
			mode: 'last_node_output',
		},
		nodes: [
			{
				id: 'work',
				kind: 'runtime_agent',
				runtime_adapter: 'codex',
				prompt: 'Return the managed worker result.',
				input: {
					parts: [{ type: 'ref', ref: 'params.input' }],
				},
				output: TEXT_OUTPUT,
			},
		],
	}
}

function buildManagedReviewerAgent(agentId: string): AgentFile {
	return {
		graph_contract_version: '1.0',
		meta: {
			id: agentId,
			name: 'Phase 18 Managed Reviewer',
		},
		entry_node_id: 'review',
		final_output: {
			mode: 'last_node_output',
		},
		nodes: [
			{
				id: 'review',
				kind: 'runtime_agent',
				runtime_adapter: 'codex',
				prompt: 'Return structured managed review findings.',
				input: {
					parts: [{ type: 'ref', ref: 'params.input' }],
				},
				output: JSON_OBJECT_OUTPUT,
			},
		],
	}
}

function buildNegativeCapabilityAgent(
	agentId: string,
	options: {
		include_unsupported_runtime_option?: boolean
	} = {},
): AgentFile {
	const includeUnsupportedRuntimeOption = options.include_unsupported_runtime_option ?? true

	return {
		graph_contract_version: '1.0',
		meta: {
			id: agentId,
			name: 'Phase 18 Negative Capability Agent',
			description:
				'Syntactically valid draft that combines unsupported runtime controls with an unregistered memory provider.',
		},
		entry_node_id: 'blocked',
		params: {
			task: {
				type: 'string',
				required: true,
			},
		},
		memory_bindings: [
			{
				id: 'missing-project-memory',
				kind: 'runtime_memory',
				codex_ref: 'memory://phase18/unregistered',
				scope: 'agent',
				config: {
					intent: {
						summary: 'Memory provider intentionally not registered for negative coverage.',
					},
					required_capabilities: ['read', 'write'],
				},
			},
		],
		final_output: {
			mode: 'last_node_output',
		},
		nodes: [
			{
				id: 'blocked',
				kind: 'runtime_agent',
				runtime_adapter: 'codex',
				prompt: 'This node must fail at capability validation before launch.',
				input: {
					parts: [{ type: 'ref', ref: 'params.task' }],
				},
				output: TEXT_OUTPUT,
				memory_ids: ['missing-project-memory'],
				runtime_options: {
					model: 'gpt-5.3-codex',
					...(includeUnsupportedRuntimeOption ? { temperature: 0.1 } : {}),
				},
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

function createIntegratedStubAdapter(
	results: StubExecutionDescriptor[],
	inspectResults: Record<string, RuntimeSourceInspectionResult>,
) {
	const requests: RuntimeAdapterExecutionRequest[] = []
	const capabilities: RuntimeAdapterCapabilities = {
		supports_native_resume: true,
		supports_live_comments: true,
		supports_builtin_user_chat_mcp: true,
		supports_memory_bindings: true,
		supports_model_discovery: true,
		supports_runtime_environment_introspection: true,
		supports_reasoning_effort: true,
		supports_speed_tiers: true,
		supports_personality: true,
		supports_explicit_runtime_source: true,
		supports_runtime_source_introspection: true,
	}

	const adapter: RuntimeAdapter = {
		describeCapabilities() {
			return capabilities
		},
		async startExecution(request): Promise<RuntimeExecutionSession> {
			requests.push(request)
			const next = results.shift()
			if (!next) {
				throw new Error('Stub adapter has no remaining terminal result.')
			}

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
		},
		async listModels() {
			return {
				models: [],
			}
		},
		async inspectRuntimeEnvironment() {
			return {
				auth: {
					authenticated: true,
					requires_openai_auth: false,
				},
				account: {
					status: 'available',
				},
				rate_limits: [],
				config: {},
			}
		},
		async inspectRuntimeSource(source) {
			return (
				inspectResults[source.id] ?? {
					source_id: source.id,
					availability: 'unknown',
					limit_status: 'unknown',
				}
			)
		},
		async deliverComment() {
			throw new Error('not used in test')
		},
		async deliverUserChatResponse() {
			throw new Error('not used in test')
		},
		async cancelExecution() {
			throw new Error('not used in test')
		},
	}

	return {
		adapter,
		requests,
	}
}

afterEach(async () => {
	vi.restoreAllMocks()

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

describe('Phase 18 offline integrated product flows', () => {
	it('builds, deploys, prompts, resumes, and delegates across local subsystem seams', async () => {
		const store = await createStore()
		const tempDir = path.dirname(store.database_path)
		const lifecycle = new AgentLifecycleService({ state_store: store })
		const memoryFixture = registerPhase18MemoryFixture(store)
		const parentAgentId = 'agent.phase18.integrated.parent'
		const childAgentId = 'agent.phase18.integrated.child'
		const candidateAgent = buildPhase18CandidateAgent(parentAgentId, childAgentId)
		const childPath = await writeAgentFile(
			tempDir,
			'phase18-child.json',
			buildChildAgent(childAgentId),
		)
		await lifecycle.registerAgentFile(childPath)
		await lifecycle.deployAgentFile(childPath)

		const { adapter, requests } = createIntegratedStubAdapter(
			[
				{
					outcome: 'success',
					output: JSON_OBJECT_OUTPUT,
					output_json: {
						agent_file: toJsonObject(candidateAgent),
					},
				},
				{
					runtime_handle: {
						runtime: 'parent-plan-initial',
					},
					native_session_handle: {
						session: 'parent-plan-initial',
					},
					terminal_result: new Promise<RuntimeTerminalResult>(() => undefined),
					events: singleEventStream({
						kind: 'user_chat_request',
						request_handle: {
							kind: 'codex_app_server_user_chat_request',
							threadId: 'phase18-thread',
							turnId: 'phase18-turn',
							itemId: 'phase18-tool',
							requestId: 18,
							prompt_id: 'phase18-approval',
						},
						payload: {
							kind: 'text',
							prompt_id: 'phase18-approval',
							text: 'Approve the memory-backed plan before delegation?',
							require_response: true,
						},
					}),
				},
				{
					runtime_handle: {
						runtime: 'parent-plan-resumed',
					},
					native_session_handle: {
						session: 'parent-plan-resumed',
					},
					terminal_result: {
						outcome: 'success',
						output: JSON_OBJECT_OUTPUT,
						output_json: {
							summary: 'approved memory-backed plan',
							count: 1,
						},
					},
				},
				{
					outcome: 'success',
					output: TEXT_OUTPUT,
					output_text: 'child accepted approved memory-backed plan',
				},
			],
			{
				'primary-source': {
					source_id: 'primary-source',
					availability: 'unavailable',
					limit_status: 'ok',
				},
				'fallback-source': {
					source_id: 'fallback-source',
					availability: 'available',
					limit_status: 'ok',
				},
			},
		)

		const builder = new BuilderAgentService({
			state_store: store,
			runtime_adapter: adapter,
		})
		const built = await builder.buildAgentDraft({
			target_agent_id: parentAgentId,
			request:
				'Create an integrated agent using runtime sources, memory, user approval, and child delegation.',
			run_id: 'run-phase18-builder',
		})

		expect(built.draft.revision.revision_kind).toBe('draft')
		expect(built.draft.status.agent.live_revision_id).toBeNull()
		expect(JSON.parse(String(requests[0]?.input_message))).toMatchObject({
			constraints: {
				public_contract_only: true,
			},
			portable_authoring_guidance: {
				allowed_public_surfaces: expect.arrayContaining([
					'memory_bindings',
					'runtime_sources',
					'interaction',
					'orchestrator_agent',
				]),
			},
		})

		const deployed = await lifecycle.deployAgentFile(built.draft.revision.file_path)
		expect(deployed.revision.revision_kind).toBe('live')
		expect(deployed.status.agent.live_revision_id).toBe(deployed.revision.revision_id)
		expect(memoryFixture.provider).toMatchObject({
			provider_id: 'phase18-test-memory',
			codex_ref: 'memory://phase18/project',
			provider_family: MEM0_PROVIDER_FAMILY,
			supported_capabilities: ['read', 'write', 'rag_retrieval'],
		})

		const blocked = await runAgentFile(
			built.candidate_agent_file,
			adapter,
			{ task: 'Ship a realistic offline integrated flow.' },
			{
				state_store: store,
				resolved_revision_id: deployed.revision.resolved_revision_id,
				logical_agent_id: parentAgentId,
				run_id: 'run-phase18-parent',
			},
		)

		expect(blocked).toEqual({
			status: 'waiting_for_user',
			run_id: 'run-phase18-parent',
			run_status: 'waiting_for_user',
			code: 'RUN_WAITING_FOR_USER',
			message: 'Run "run-phase18-parent" is blocked on user input from node "plan".',
			resume_available: true,
		})
		expect(requests[1]).toMatchObject({
			node_id: 'plan',
			input_message: 'Ship a realistic offline integrated flow.',
			effective_bindings: {
				memory_bindings: [
					{
						id: 'project-memory',
						kind: 'runtime_memory',
						codex_ref: 'memory://phase18/project',
						scope: 'agent',
					},
				],
			},
			memory_context: {
				bindings: [
					{
						binding_id: 'project-memory',
						codex_ref: 'memory://phase18/project',
						required_capabilities: ['read', 'write', 'rag_retrieval'],
						read: {
							query: 'Ship a realistic offline integrated flow.',
							records: [
								{
									id: 'phase18-memory-record',
									content: 'Registered project memory for the Phase 18 integrated flow.',
								},
							],
						},
						write: {
							enabled: true,
							mode: 'node_success_output',
						},
					},
				],
			},
			runtime_options: {
				model: 'gpt-5.3-codex',
				reasoning_effort: 'high',
				speed_tier: 'fast',
				personality: 'pragmatic',
			},
			runtime_source: {
				id: 'fallback-source',
				source_ref: 'workspace://fallback',
			},
			interaction: {
				comments_enabled: true,
				user_chat_server_name: 'orchestrator.user_chat',
			},
			resume: {
				mode: 'fresh',
			},
		})

		const blockedSnapshot = store.getPersistedRunSnapshot('run-phase18-parent')
		expect(blockedSnapshot?.run.status).toBe('waiting_for_user')
		expect(blockedSnapshot?.resume.pending_prompt).toMatchObject({
			prompt_id: 'phase18-approval',
			unresolved: true,
		})
		expect(memoryFixture.preparedQueries).toEqual(['Ship a realistic offline integrated flow.'])
		expect(memoryFixture.writtenContents).toEqual([])

		store.appendVisibleChatMessage({
			run_id: 'run-phase18-parent',
			kind: 'user_message',
			payload: {
				kind: 'text',
				prompt_id: 'phase18-approval',
				text: 'Approved for delegation.',
			},
		})

		const resumed = await resumeAgentRun(
			built.candidate_agent_file,
			adapter,
			'run-phase18-parent',
			{
				state_store: store,
				resolved_revision_id: deployed.revision.resolved_revision_id,
			},
		)

		expect(resumed).toEqual({
			status: 'success',
			run_id: 'run-phase18-parent',
			run_status: 'completed',
			final_output: 'child accepted approved memory-backed plan',
			final_output_mode: 'text',
			node_outputs: expect.any(Map),
		})
		expect(requests[2]).toMatchObject({
			node_id: 'plan',
			input_message: 'Ship a realistic offline integrated flow.',
			memory_context: {
				bindings: [
					{
						binding_id: 'project-memory',
						read: {
							query: 'Ship a realistic offline integrated flow.',
						},
						write: {
							enabled: true,
							mode: 'node_success_output',
						},
					},
				],
			},
			runtime_source: {
				id: 'fallback-source',
			},
			interaction: {
				comments_enabled: true,
				user_chat_server_name: 'orchestrator.user_chat',
				user_chat_reply: {
					kind: 'text',
					prompt_id: 'phase18-approval',
					text: 'Approved for delegation.',
				},
			},
			resume: {
				mode: 'native_resume',
				native_session_handle: {
					session: 'parent-plan-initial',
				},
			},
		})
		expect(requests[3]).toMatchObject({
			node_id: 'review',
			input_message: 'Review approved plan: approved memory-backed plan',
			interaction: {
				comments_enabled: false,
			},
		})

		const finalSnapshot = store.getPersistedRunSnapshot('run-phase18-parent')
		expect(finalSnapshot?.run.status).toBe('completed')
		expect(finalSnapshot?.attempts).toEqual([
			expect.objectContaining({
				node_id: 'plan',
				outcome: null,
			}),
			expect.objectContaining({
				node_id: 'plan',
				outcome: 'success',
			}),
			expect.objectContaining({
				node_id: 'delegate',
				outcome: 'success',
			}),
		])
		expect(finalSnapshot?.resume.pending_prompt).toBeNull()
		expect(memoryFixture.preparedQueries).toEqual([
			'Ship a realistic offline integrated flow.',
			'Ship a realistic offline integrated flow.',
		])
		expect(memoryFixture.writtenContents).toEqual([
			'{"count":1,"summary":"approved memory-backed plan"}',
		])
	})

	it('runs managed worker, reviewer, fix, and re-review through the managed subagent service boundary', async () => {
		const store = await createStore()
		const tempDir = path.dirname(store.database_path)
		const lifecycle = new AgentLifecycleService({ state_store: store })
		const parentAgentId = 'agent.phase18.managed.parent'
		const workerAgentId = 'agent.phase18.managed.worker'
		const reviewerAgentId = 'agent.phase18.managed.reviewer'
		const parentAgent = buildManagedParentCoordinatorAgent(parentAgentId)
		const parentPath = await writeAgentFile(tempDir, 'phase18-managed-parent.json', parentAgent)
		const workerPath = await writeAgentFile(
			tempDir,
			'phase18-managed-worker.json',
			buildManagedWorkerAgent(workerAgentId),
		)
		const reviewerPath = await writeAgentFile(
			tempDir,
			'phase18-managed-reviewer.json',
			buildManagedReviewerAgent(reviewerAgentId),
		)

		await lifecycle.registerAgentFile(parentPath)
		const deployedParent = await lifecycle.deployAgentFile(parentPath)
		await lifecycle.registerAgentFile(workerPath)
		await lifecycle.deployAgentFile(workerPath)
		await lifecycle.registerAgentFile(reviewerPath)
		await lifecycle.deployAgentFile(reviewerPath)
		store.createRun({
			run_id: 'run-phase18-managed-parent',
			logical_agent_id: parentAgentId,
			resolved_revision_id: deployedParent.revision.resolved_revision_id,
			entry_node_id: parentAgent.entry_node_id,
			started_via: 'direct',
			params: {
				task: 'Coordinate managed worker, review, fix, and close.',
			},
		})

		const { adapter, requests } = createIntegratedStubAdapter(
			[
				{
					outcome: 'success',
					output: TEXT_OUTPUT,
					output_text: 'worker produced initial patch',
				},
				{
					outcome: 'success',
					output: JSON_OBJECT_OUTPUT,
					output_json: {
						summary: 'Review found one correctness issue.',
						findings: [
							{
								finding_id: 'finding-phase18-review',
								severity: 'high',
								category: 'correctness',
								summary: 'Initial patch misses the managed close assertion.',
								evidence_refs: ['tests/integration/phase18-integrated-product-flows.test.ts'],
								recommended_action: 'fix',
							},
						],
					},
				},
				{
					outcome: 'success',
					output: TEXT_OUTPUT,
					output_text: 'fix worker added managed close assertion',
				},
				{
					outcome: 'success',
					output: JSON_OBJECT_OUTPUT,
					output_json: {
						summary: 'Re-review accepted the fix.',
						findings: [],
					},
				},
			],
			{},
		)
		const service = new ManagedSubagentService({
			state_store: store,
			runtime_adapter: adapter,
			lifecycle_service: lifecycle,
		})

		const initialWorker = await service.launch({
			parent_run_id: 'run-phase18-managed-parent',
			parent_task_id: 'task-phase18-managed-flow',
			child_role: 'worker',
			agent_ref: workerAgentId,
			objective: 'Implement the initial managed subagent flow coverage.',
			input_message: 'Worker package: add managed subagent integrated coverage.',
			acceptance_criteria: ['Return a concrete worker summary.'],
			prohibitions: ['Do not touch docs or CLI files.'],
			write_set: {
				mode: 'allow_list',
				items: [
					{
						resource_kind: 'file',
						resource_ref: 'tests/integration/phase18-integrated-product-flows.test.ts',
						scope: 'exact',
						access: 'create_or_modify',
					},
				],
			},
			budgets: {
				max_children: 4,
				max_review_loops: 2,
				max_spawn_depth: 1,
			},
		})
		const initialWorkerResult = await service.wait({
			subagent_id: initialWorker.subagent_id,
			wait_mode: 'terminal_only',
		})

		expect(initialWorkerResult).toMatchObject({
			state: 'terminal',
			outcome: 'accepted',
			final_payload: {
				summary: 'worker produced initial patch',
			},
		})
		expect(store.getManagedSubagentRecord(initialWorker.subagent_id)).toMatchObject({
			state: 'terminal',
			close_disposition: null,
			lineage: {
				root_run_id: 'run-phase18-managed-parent',
				parent_run_id: 'run-phase18-managed-parent',
				parent_task_id: 'task-phase18-managed-flow',
				depth: 1,
			},
		})

		const firstReview = await service.launch({
			parent_run_id: 'run-phase18-managed-parent',
			parent_task_id: 'task-phase18-managed-flow',
			child_role: 'reviewer',
			agent_ref: reviewerAgentId,
			objective: 'Review the initial worker result.',
			input_message: `Review worker result: ${initialWorkerResult.final_payload?.summary}`,
			acceptance_criteria: ['Return findings when a fix is required.'],
			prohibitions: ['Do not modify the worker output.'],
			write_set: {
				mode: 'allow_list',
				items: [
					{
						resource_kind: 'generic_resource',
						resource_ref: 'review://phase18/initial',
						scope: 'exact',
						access: 'create_or_modify',
					},
				],
			},
			budgets: {
				max_children: 4,
				max_review_loops: 2,
				max_spawn_depth: 1,
			},
		})
		const firstReviewResult = await service.wait({
			subagent_id: firstReview.subagent_id,
			wait_mode: 'terminal_only',
		})

		expect(firstReviewResult).toMatchObject({
			state: 'terminal',
			outcome: 'review_required',
			final_payload: {
				summary: 'Review found one correctness issue.',
			},
			findings: [
				{
					finding_id: 'finding-phase18-review',
					severity: 'high',
					category: 'correctness',
					recommended_action: 'fix',
				},
			],
			reason_code: 'review_findings_raised',
		})
		const closedReviewFindings = await service.close({
			subagent_id: firstReview.subagent_id,
			close_disposition: 'abandoned_by_parent',
		})
		expect(closedReviewFindings).toMatchObject({
			close_status: 'closed',
			state: 'closed',
			outcome: 'review_required',
			reason_code: 'review_findings_raised',
		})
		const closedInitialWorker = await service.close({
			subagent_id: initialWorker.subagent_id,
			close_disposition: 'accepted_by_parent',
		})
		expect(closedInitialWorker).toMatchObject({
			close_status: 'closed',
			state: 'closed',
			outcome: 'accepted',
		})

		const fixWorker = await service.launch({
			parent_run_id: 'run-phase18-managed-parent',
			parent_task_id: 'task-phase18-managed-flow',
			child_role: 'worker',
			agent_ref: workerAgentId,
			objective: 'Fix the reviewer finding.',
			input_message: 'Fix package: add the missing managed close assertion.',
			acceptance_criteria: ['Return a concrete fix summary.'],
			prohibitions: ['Only address finding-phase18-review.'],
			write_set: {
				mode: 'allow_list',
				items: [
					{
						resource_kind: 'file',
						resource_ref: 'tests/integration/phase18-integrated-product-flows.test.ts',
						scope: 'exact',
						access: 'create_or_modify',
					},
				],
			},
			budgets: {
				max_children: 4,
				max_review_loops: 2,
				max_spawn_depth: 1,
			},
		})
		const fixResult = await service.wait({
			subagent_id: fixWorker.subagent_id,
			wait_mode: 'terminal_only',
		})
		expect(fixResult).toMatchObject({
			state: 'terminal',
			outcome: 'accepted',
			final_payload: {
				summary: 'fix worker added managed close assertion',
			},
		})

		const secondReview = await service.launch({
			parent_run_id: 'run-phase18-managed-parent',
			parent_task_id: 'task-phase18-managed-flow',
			child_role: 'reviewer',
			agent_ref: reviewerAgentId,
			objective: 'Re-review the fix worker result.',
			input_message: `Re-review fix result: ${fixResult.final_payload?.summary}`,
			acceptance_criteria: ['Return no findings when the fix is acceptable.'],
			prohibitions: ['Do not request unrelated changes.'],
			write_set: {
				mode: 'allow_list',
				items: [
					{
						resource_kind: 'generic_resource',
						resource_ref: 'review://phase18/fix',
						scope: 'exact',
						access: 'create_or_modify',
					},
				],
			},
			budgets: {
				max_children: 4,
				max_review_loops: 2,
				max_spawn_depth: 1,
			},
		})
		const secondReviewResult = await service.wait({
			subagent_id: secondReview.subagent_id,
			wait_mode: 'terminal_only',
		})
		expect(secondReviewResult).toMatchObject({
			state: 'terminal',
			outcome: 'accepted',
			final_payload: {
				summary: 'Re-review accepted the fix.',
			},
			findings: null,
			reason_code: null,
		})

		const closedFixWorker = await service.close({
			subagent_id: fixWorker.subagent_id,
			close_disposition: 'accepted_by_parent',
		})
		const closedSecondReview = await service.close({
			subagent_id: secondReview.subagent_id,
			close_disposition: 'accepted_by_parent',
		})

		expect(closedFixWorker).toMatchObject({
			close_status: 'closed',
			state: 'closed',
			outcome: 'accepted',
		})
		expect(closedSecondReview).toMatchObject({
			close_status: 'closed',
			state: 'closed',
			outcome: 'accepted',
		})
		expect(requests.map((request) => request.input_message)).toEqual([
			'Worker package: add managed subagent integrated coverage.',
			'Review worker result: worker produced initial patch',
			'Fix package: add the missing managed close assertion.',
			'Re-review fix result: fix worker added managed close assertion',
		])
		expect(
			store.listManagedSubagentRecords({ parent_run_id: 'run-phase18-managed-parent' }),
		).toEqual([
			expect.objectContaining({
				subagent_id: initialWorker.subagent_id,
				child_role: 'worker',
				state: 'closed',
				close_disposition: 'accepted_by_parent',
			}),
			expect.objectContaining({
				subagent_id: firstReview.subagent_id,
				child_role: 'reviewer',
				state: 'closed',
				close_disposition: 'abandoned_by_parent',
				terminal_result: expect.objectContaining({
					outcome: 'review_required',
					reason_code: 'review_findings_raised',
				}),
			}),
			expect.objectContaining({
				subagent_id: fixWorker.subagent_id,
				child_role: 'worker',
				state: 'closed',
				close_disposition: 'accepted_by_parent',
			}),
			expect.objectContaining({
				subagent_id: secondReview.subagent_id,
				child_role: 'reviewer',
				state: 'closed',
				close_disposition: 'accepted_by_parent',
				terminal_result: expect.objectContaining({
					outcome: 'accepted',
				}),
			}),
		])

		const persistedParentAgent = await readFile(deployedParent.live_file_path, 'utf8')
		expect(persistedParentAgent).not.toContain(initialWorker.subagent_id)
		expect(persistedParentAgent).not.toContain('finding-phase18-review')
		expect(persistedParentAgent).not.toContain('accepted_by_parent')
	})

	it('fails unsupported runtime options at the runtime-owned gate before run creation without mutating lifecycle state', async () => {
		const store = await createStore()
		const tempDir = path.dirname(store.database_path)
		const lifecycle = new AgentLifecycleService({ state_store: store })
		const agentId = 'agent.phase18.negative.runtime.capability'
		const agentFile = buildNegativeCapabilityAgent(agentId)
		const agentPath = await writeAgentFile(
			tempDir,
			'phase18-negative-runtime-capability.json',
			agentFile,
		)
		const registered = await lifecycle.registerAgentFile(agentPath)
		const deployed = await lifecycle.deployAgentFile(agentPath)
		const statusBeforeFailure = await lifecycle.getAgentStatus(agentId)
		const { adapter, requests } = createIntegratedStubAdapter(
			[
				{
					outcome: 'success',
					output: TEXT_OUTPUT,
					output_text: 'should not launch',
				},
			],
			{},
		)

		await expect(
			runAgentFile(
				agentFile,
				adapter,
				{ task: 'Exercise the combined negative capability gate.' },
				{
					state_store: store,
					resolved_revision_id: deployed.revision.resolved_revision_id,
					logical_agent_id: agentId,
					run_id: 'run-phase18-negative-runtime-capability',
				},
			),
		).rejects.toMatchObject({
			code: 'UNSUPPORTED_RUNTIME_CONTEXT',
			message:
				'Node "blocked" declares runtime option "temperature", which is not implemented in the current execution slice.',
		})

		const statusAfterFailure = await lifecycle.getAgentStatus(agentId)
		expect(requests).toHaveLength(0)
		expect(store.getPersistedRunSnapshot('run-phase18-negative-runtime-capability')).toBeNull()
		expect(store.listMemoryProviderRecords()).toEqual([])
		expect(statusAfterFailure.agent.live_revision_id).toBe(
			statusBeforeFailure.agent.live_revision_id,
		)
		expect(statusAfterFailure.live_revision?.revision_id).toBe(
			statusBeforeFailure.live_revision?.revision_id,
		)
		expect(statusAfterFailure.revisions.map((revision) => revision.revision_id)).toEqual(
			statusBeforeFailure.revisions.map((revision) => revision.revision_id),
		)
		expect(statusAfterFailure.revisions.map((revision) => revision.availability_state)).toEqual(
			statusBeforeFailure.revisions.map((revision) => revision.availability_state),
		)
		expect(deployed.status.agent.live_revision_id).toBe(deployed.revision.revision_id)
		expect(registered.status.agent.live_revision_id).toBeNull()

		const persistedAgent = JSON.parse(await readFile(deployed.live_file_path, 'utf8')) as JsonObject
		expect(persistedAgent).toMatchObject({
			memory_bindings: [
				{
					id: 'missing-project-memory',
					codex_ref: 'memory://phase18/unregistered',
				},
			],
			nodes: [
				expect.objectContaining({
					id: 'blocked',
					runtime_options: {
						model: 'gpt-5.3-codex',
						temperature: 0.1,
					},
				}),
			],
		})
		expect(JSON.stringify(persistedAgent)).not.toContain('provider_id')
		expect(JSON.stringify(persistedAgent)).not.toContain('api_key')
		expect(JSON.stringify(persistedAgent)).not.toContain('runtime_capabilities')
	})

	it('fails an unregistered runtime memory provider at the memory-owned gate before runtime launch without mutating lifecycle state', async () => {
		const store = await createStore()
		const tempDir = path.dirname(store.database_path)
		const lifecycle = new AgentLifecycleService({ state_store: store })
		const agentId = 'agent.phase18.negative.memory.capability'
		const agentFile = buildNegativeCapabilityAgent(agentId, {
			include_unsupported_runtime_option: false,
		})
		const agentPath = await writeAgentFile(
			tempDir,
			'phase18-negative-memory-capability.json',
			agentFile,
		)
		const registered = await lifecycle.registerAgentFile(agentPath)
		const deployed = await lifecycle.deployAgentFile(agentPath)
		const statusBeforeFailure = await lifecycle.getAgentStatus(agentId)
		const { adapter, requests } = createIntegratedStubAdapter(
			[
				{
					outcome: 'success',
					output: TEXT_OUTPUT,
					output_text: 'should not launch',
				},
			],
			{},
		)

		await expect(
			runAgentFile(
				agentFile,
				adapter,
				{ task: 'Exercise the missing memory provider capability gate.' },
				{
					state_store: store,
					resolved_revision_id: deployed.revision.resolved_revision_id,
					logical_agent_id: agentId,
					run_id: 'run-phase18-negative-memory-capability',
				},
			),
		).resolves.toMatchObject({
			status: 'failure',
			run_id: 'run-phase18-negative-memory-capability',
			run_status: 'failed',
			code: 'MEMORY_PROVIDER_NOT_FOUND',
			message:
				'Node "blocked" failed before terminal classification. Memory provider codex_ref "memory://phase18/unregistered" is not registered locally.',
			resume_available: true,
		})

		const statusAfterFailure = await lifecycle.getAgentStatus(agentId)
		expect(requests).toHaveLength(0)
		const failedSnapshot = store.getPersistedRunSnapshot('run-phase18-negative-memory-capability')
		expect(failedSnapshot?.run.status).toBe('failed')
		expect(failedSnapshot?.attempts).toEqual([
			expect.objectContaining({
				node_id: 'blocked',
				outcome: 'runtime_error',
				runtime_handle: null,
			}),
		])
		expect(store.listMemoryProviderRecords()).toEqual([])
		expect(statusAfterFailure.agent.live_revision_id).toBe(
			statusBeforeFailure.agent.live_revision_id,
		)
		expect(statusAfterFailure.live_revision?.revision_id).toBe(
			statusBeforeFailure.live_revision?.revision_id,
		)
		expect(statusAfterFailure.revisions.map((revision) => revision.revision_id)).toEqual(
			statusBeforeFailure.revisions.map((revision) => revision.revision_id),
		)
		expect(statusAfterFailure.revisions.map((revision) => revision.availability_state)).toEqual(
			statusBeforeFailure.revisions.map((revision) => revision.availability_state),
		)
		expect(deployed.status.agent.live_revision_id).toBe(deployed.revision.revision_id)
		expect(registered.status.agent.live_revision_id).toBeNull()

		const persistedAgent = JSON.parse(await readFile(deployed.live_file_path, 'utf8')) as JsonObject
		expect(persistedAgent).toMatchObject({
			memory_bindings: [
				{
					id: 'missing-project-memory',
					codex_ref: 'memory://phase18/unregistered',
				},
			],
			nodes: [
				expect.objectContaining({
					id: 'blocked',
					runtime_options: {
						model: 'gpt-5.3-codex',
					},
				}),
			],
		})
		expect(JSON.stringify(persistedAgent)).not.toContain('temperature')
		expect(JSON.stringify(persistedAgent)).not.toContain('provider_id')
		expect(JSON.stringify(persistedAgent)).not.toContain('api_key')
		expect(JSON.stringify(persistedAgent)).not.toContain('runtime_capabilities')
	})
})
