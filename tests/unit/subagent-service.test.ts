import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import type { AgentFile } from '../../src/core/agent-file.js'
import { AgentLifecycleService } from '../../src/core/agent-lifecycle.js'
import type { AppError } from '../../src/core/errors.js'
import { SQLiteLocalStateStore } from '../../src/core/state/index.js'
import { ManagedSubagentService } from '../../src/core/subagent-service.js'
import type {
	RuntimeAdapter,
	RuntimeAdapterCapabilities,
	RuntimeAdapterExecutionRequest,
	RuntimeEvent,
	RuntimeExecutionSession,
	RuntimeModelCatalogPage,
	RuntimeTerminalResult,
} from '../../src/ports/runtime.js'
import type { ManagedSubagentFinding } from '../../src/ports/subagents.js'

const storesToClose: SQLiteLocalStateStore[] = []
const tempDirsToRemove: string[] = []

const TEXT_OUTPUT = { mode: 'text' } as const
const JSON_OUTPUT = { mode: 'json', schema: { type: 'object' } } as const
const NEVER_RESOLVING_PROMISE = new Promise<IteratorResult<RuntimeEvent>>(() => {})

function createDeferred<T>() {
	let resolve!: (value: T) => void
	let reject!: (error?: unknown) => void
	const promise = new Promise<T>((innerResolve, innerReject) => {
		resolve = innerResolve
		reject = innerReject
	})
	return { promise, resolve, reject }
}

async function createStore(): Promise<SQLiteLocalStateStore> {
	const tempDir = await mkdtemp(path.join(os.tmpdir(), 'dennett-phase16-subagents-'))
	const store = new SQLiteLocalStateStore({
		database_path: path.join(tempDir, 'local-state.sqlite'),
	})
	storesToClose.push(store)
	tempDirsToRemove.push(tempDir)
	return store
}

function createStubAdapter(
	results: Array<RuntimeTerminalResult | Promise<RuntimeTerminalResult>>,
): {
	adapter: RuntimeAdapter
	requests: RuntimeAdapterExecutionRequest[]
} {
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
		async startExecution(request): Promise<RuntimeExecutionSession> {
			requests.push(request)
			const next = results.shift()
			if (!next) {
				throw new Error('stub adapter has no remaining result')
			}
			return {
				runtime_handle: null,
				native_session_handle: null,
				terminal_result: Promise.resolve(next),
				events: {
					[Symbol.asyncIterator]() {
						return {
							next: () => NEVER_RESOLVING_PROMISE,
						}
					},
				},
			}
		},
		async listModels(): Promise<RuntimeModelCatalogPage> {
			throw new Error('not used in test')
		},
		async inspectRuntimeEnvironment() {
			throw new Error('not used in test')
		},
		async inspectRuntimeSource() {
			throw new Error('not used in test')
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

	return { adapter, requests }
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

function buildWorkerChildAgentFile(agentId: string): AgentFile {
	return {
		graph_contract_version: '1.0',
		meta: {
			id: agentId,
			name: 'Managed Child Worker',
		},
		entry_node_id: 'child-start',
		final_output: {
			mode: 'last_node_output',
		},
		nodes: [
			{
				id: 'child-start',
				kind: 'runtime_agent',
				runtime_adapter: 'codex',
				prompt: 'Return the delegated result.',
				input: {
					parts: [{ type: 'ref', ref: 'params.input' }],
				},
				output: TEXT_OUTPUT,
			},
		],
	}
}

function buildReviewerChildAgentFile(agentId: string): AgentFile {
	return {
		graph_contract_version: '1.0',
		meta: {
			id: agentId,
			name: 'Managed Child Reviewer',
		},
		entry_node_id: 'review-start',
		final_output: {
			mode: 'last_node_output',
		},
		nodes: [
			{
				id: 'review-start',
				kind: 'runtime_agent',
				runtime_adapter: 'codex',
				prompt: 'Return a structured review result.',
				input: {
					parts: [{ type: 'ref', ref: 'params.input' }],
				},
				output: JSON_OUTPUT,
			},
		],
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

describe('ManagedSubagentService', () => {
	it('launches, waits, and closes a worker-role managed subagent without touching plain orchestrator_agent behavior', async () => {
		const store = await createStore()
		store.createRun({
			run_id: 'parent-run',
			resolved_revision_id: 'rev-parent',
			entry_node_id: 'entry',
			started_via: 'direct',
		})

		const tempDir = path.dirname(store.database_path)
		const childAgentId = 'agent.phase16.child'
		const childPath = await writeAgentFile(
			tempDir,
			'phase16-child.json',
			buildWorkerChildAgentFile(childAgentId),
		)
		const lifecycle = new AgentLifecycleService({ state_store: store })
		await lifecycle.registerAgentFile(childPath)
		await lifecycle.deployAgentFile(childPath)

		const { adapter, requests } = createStubAdapter([
			{
				outcome: 'success',
				output: TEXT_OUTPUT,
				output_text: 'child-complete',
			},
		])
		const service = new ManagedSubagentService({
			state_store: store,
			runtime_adapter: adapter,
			lifecycle_service: lifecycle,
		})

		const launched = await service.launch({
			parent_run_id: 'parent-run',
			parent_task_id: 'task-1',
			child_role: 'worker',
			agent_ref: childAgentId,
			objective: 'Implement the delegated fix',
			input_message: 'Worker package input',
			acceptance_criteria: ['Return final worker output'],
			prohibitions: ['Do not edit outside write_set'],
			write_set: {
				mode: 'allow_list',
				items: [
					{
						resource_kind: 'file',
						resource_ref: 'src/core/example.ts',
						scope: 'exact',
						access: 'create_or_modify',
					},
				],
			},
			read_context: {
				mode: 'explicit_only',
				items: [
					{
						context_kind: 'file',
						context_ref: 'src/core/example.ts',
						inclusion: 'reference_only',
						required: false,
					},
				],
			},
			required_validations: [
				{
					validation_id: 'unit-tests',
					description: 'Run the delegated unit tests or report why they were not run.',
					required: true,
				},
			],
			interaction_policy: 'silent',
		})

		expect(launched).toMatchObject({
			child_role: 'worker',
			child_logical_agent_id: childAgentId,
			state: 'running',
			terminal_result: null,
			close_disposition: null,
			lineage: {
				root_run_id: 'parent-run',
				parent_run_id: 'parent-run',
				parent_task_id: 'task-1',
				depth: 1,
			},
			task_package: {
				agent_ref: childAgentId,
				objective: 'Implement the delegated fix',
				input_message: 'Worker package input',
				read_context: {
					mode: 'explicit_only',
					items: [
						{
							context_kind: 'file',
							context_ref: 'src/core/example.ts',
							inclusion: 'reference_only',
							required: false,
						},
					],
				},
				required_validations: [
					{
						validation_id: 'unit-tests',
						description: 'Run the delegated unit tests or report why they were not run.',
						required: true,
					},
				],
				interaction_policy: 'silent',
			},
		})
		expect(store.getManagedSubagentRecord(launched.subagent_id)?.task_package).toMatchObject({
			read_context: {
				mode: 'explicit_only',
				items: [
					{
						context_kind: 'file',
						context_ref: 'src/core/example.ts',
						inclusion: 'reference_only',
						required: false,
					},
				],
			},
			required_validations: [
				{
					validation_id: 'unit-tests',
					description: 'Run the delegated unit tests or report why they were not run.',
					required: true,
				},
			],
			interaction_policy: 'silent',
		})

		const waited = await service.wait({
			subagent_id: launched.subagent_id,
			wait_mode: 'terminal_only',
		})
		expect(waited).toMatchObject({
			subagent_id: launched.subagent_id,
			state: 'terminal',
			outcome: 'accepted',
			final_payload: {
				summary: 'child-complete',
			},
			findings: null,
			reason_code: null,
		})
		expect(requests).toHaveLength(1)
		expect(requests[0]?.input_message).toBe('Worker package input')

		const closed = await service.close({
			subagent_id: launched.subagent_id,
			close_disposition: 'accepted_by_parent',
		})
		expect(closed).toMatchObject({
			subagent_id: launched.subagent_id,
			close_status: 'closed',
			state: 'closed',
			outcome: 'accepted',
			reason_code: null,
		})
		expect(store.getManagedSubagentRecord(launched.subagent_id)).toMatchObject({
			state: 'closed',
			close_disposition: 'accepted_by_parent',
		})
	})

	it('rejects invalid task-package read context, required validations, and interaction policy before resolving a child', async () => {
		const store = await createStore()
		store.createRun({
			run_id: 'parent-run-task-package-invalid',
			resolved_revision_id: 'rev-parent-task-package-invalid',
			entry_node_id: 'entry',
			started_via: 'direct',
		})

		const { adapter } = createStubAdapter([])
		const service = new ManagedSubagentService({
			state_store: store,
			runtime_adapter: adapter,
		})
		const baseRequest = {
			parent_run_id: 'parent-run-task-package-invalid',
			parent_task_id: 'task-package-invalid',
			child_role: 'worker',
			agent_ref: 'agent.phase16.child.invalid-task-package',
			objective: 'Reject malformed managed task package fields',
			input_message: 'input',
			acceptance_criteria: ['Reject invalid launch payloads'],
			write_set: {
				mode: 'allow_list',
				items: [
					{
						resource_kind: 'file',
						resource_ref: 'src/core/task-package-invalid.ts',
						scope: 'exact',
						access: 'create_or_modify',
					},
				],
			},
		} as const satisfies Omit<Parameters<ManagedSubagentService['launch']>[0], 'child_role'>

		await expect(
			service.launch({
				...baseRequest,
				child_role: 'worker',
				read_context: {
					mode: 'explicit_only',
					items: [
						{
							context_kind: 'file',
							context_ref: '',
							inclusion: 'reference_only',
							required: false,
						},
					],
				} as Parameters<ManagedSubagentService['launch']>[0]['read_context'],
			}),
		).rejects.toMatchObject({
			code: 'INVALID_SUBAGENT_REQUEST',
			message:
				'Managed subagent read_context item 1 context_ref must be a non-empty string.',
		} satisfies Pick<AppError, 'code' | 'message'>)

		await expect(
			service.launch({
				...baseRequest,
				child_role: 'worker',
				required_validations: [],
			}),
		).rejects.toMatchObject({
			code: 'INVALID_SUBAGENT_REQUEST',
			message: 'Managed subagent required_validations must be a non-empty array when present.',
		} satisfies Pick<AppError, 'code' | 'message'>)

		const missingAcceptanceCriteriaRequest = {
			...baseRequest,
			child_role: 'worker',
		} as Partial<Parameters<ManagedSubagentService['launch']>[0]>
		delete missingAcceptanceCriteriaRequest.acceptance_criteria
		await expect(
			service.launch(
				missingAcceptanceCriteriaRequest as Parameters<ManagedSubagentService['launch']>[0],
			),
		).rejects.toMatchObject({
			code: 'INVALID_SUBAGENT_REQUEST',
			message: 'Managed subagent acceptance_criteria must be a non-empty array.',
		} satisfies Pick<AppError, 'code' | 'message'>)

		await expect(
			service.launch({
				...baseRequest,
				child_role: 'worker',
				acceptance_criteria:
					'Reject invalid launch payloads' as unknown as Parameters<
						ManagedSubagentService['launch']
					>[0]['acceptance_criteria'],
			}),
		).rejects.toMatchObject({
			code: 'INVALID_SUBAGENT_REQUEST',
			message: 'Managed subagent acceptance_criteria must be a non-empty array.',
		} satisfies Pick<AppError, 'code' | 'message'>)

		await expect(
			service.launch({
				...baseRequest,
				child_role: 'worker',
				acceptance_criteria: [] as unknown as Parameters<
					ManagedSubagentService['launch']
				>[0]['acceptance_criteria'],
			}),
		).rejects.toMatchObject({
			code: 'INVALID_SUBAGENT_REQUEST',
			message: 'Managed subagent acceptance_criteria must be a non-empty array.',
		} satisfies Pick<AppError, 'code' | 'message'>)

		await expect(
			service.launch({
				...baseRequest,
				child_role: 'worker',
				acceptance_criteria: ['   '] as Parameters<
					ManagedSubagentService['launch']
				>[0]['acceptance_criteria'],
			}),
		).rejects.toMatchObject({
			code: 'INVALID_SUBAGENT_REQUEST',
			message: 'Managed subagent acceptance_criteria item 1 must be a non-empty string.',
		} satisfies Pick<AppError, 'code' | 'message'>)

		await expect(
			service.launch({
				...baseRequest,
				child_role: 'worker',
				acceptance_criteria: [
					'Reject invalid launch payloads',
					42,
				] as unknown as Parameters<
					ManagedSubagentService['launch']
				>[0]['acceptance_criteria'],
			}),
		).rejects.toMatchObject({
			code: 'INVALID_SUBAGENT_REQUEST',
			message: 'Managed subagent acceptance_criteria item 2 must be a non-empty string.',
		} satisfies Pick<AppError, 'code' | 'message'>)

		await expect(
			service.launch({
				...baseRequest,
				child_role: 'worker',
				interaction_policy:
					'interactive' as Parameters<
						ManagedSubagentService['launch']
					>[0]['interaction_policy'],
			}),
		).rejects.toMatchObject({
			code: 'INVALID_SUBAGENT_REQUEST',
			message:
				'Managed subagent interaction_policy must be "silent" in the current Stage 16 slice.',
		} satisfies Pick<AppError, 'code' | 'message'>)

		expect(
			store.listManagedSubagentRecords({
				parent_run_id: 'parent-run-task-package-invalid',
			}),
		).toHaveLength(0)
	})

	it('accepts explorer and integrator roles as explicit managed subagent roles', async () => {
		const store = await createStore()
		store.createRun({
			run_id: 'parent-run-role-parity',
			resolved_revision_id: 'rev-parent-role-parity',
			entry_node_id: 'entry',
			started_via: 'direct',
		})

		const tempDir = path.dirname(store.database_path)
		const childAgentId = 'agent.phase16.child.role-parity'
		const childPath = await writeAgentFile(
			tempDir,
			'phase16-child-role-parity.json',
			buildWorkerChildAgentFile(childAgentId),
		)
		const lifecycle = new AgentLifecycleService({ state_store: store })
		await lifecycle.registerAgentFile(childPath)
		await lifecycle.deployAgentFile(childPath)

		const { adapter } = createStubAdapter([
			{
				outcome: 'success',
				output: TEXT_OUTPUT,
				output_text: 'explorer-complete',
			},
			{
				outcome: 'success',
				output: TEXT_OUTPUT,
				output_text: 'integrator-complete',
			},
		])
		const service = new ManagedSubagentService({
			state_store: store,
			runtime_adapter: adapter,
			lifecycle_service: lifecycle,
		})

		const explorer = await service.launch({
			parent_run_id: 'parent-run-role-parity',
			parent_task_id: 'task-role-explorer',
			child_role: 'explorer',
			agent_ref: childAgentId,
			objective: 'Explore the bounded implementation area',
			input_message: 'explorer input',
			acceptance_criteria: ['Return an exploration summary'],
			write_set: {
				mode: 'allow_list',
				items: [
					{
						resource_kind: 'file',
						resource_ref: 'subagent_tasks/explorer-notes.md',
						scope: 'exact',
						access: 'create_or_modify',
					},
				],
			},
		})
		expect(explorer.child_role).toBe('explorer')
		await expect(
			service.wait({
				subagent_id: explorer.subagent_id,
				wait_mode: 'terminal_only',
			}),
		).resolves.toMatchObject({
			state: 'terminal',
			outcome: 'accepted',
			final_payload: {
				summary: 'explorer-complete',
			},
		})

		const integrator = await service.launch({
			parent_run_id: 'parent-run-role-parity',
			parent_task_id: 'task-role-integrator',
			child_role: 'integrator',
			agent_ref: childAgentId,
			objective: 'Integrate accepted child outputs',
			input_message: 'integrator input',
			acceptance_criteria: ['Return an integration summary'],
			write_set: {
				mode: 'allow_list',
				items: [
					{
						resource_kind: 'file',
						resource_ref: 'subagent_tasks/integrator-notes.md',
						scope: 'exact',
						access: 'create_or_modify',
					},
				],
			},
		})
		expect(integrator.child_role).toBe('integrator')
		await expect(
			service.wait({
				subagent_id: integrator.subagent_id,
				wait_mode: 'terminal_only',
			}),
		).resolves.toMatchObject({
			state: 'terminal',
			outcome: 'accepted',
			final_payload: {
				summary: 'integrator-complete',
			},
		})
	})

	it('rejects a second sibling launch when max_children is exhausted', async () => {
		const store = await createStore()
		store.createRun({
			run_id: 'parent-run-budget',
			resolved_revision_id: 'rev-parent-budget',
			entry_node_id: 'entry',
			started_via: 'direct',
		})

		const tempDir = path.dirname(store.database_path)
		const childAgentId = 'agent.phase16.child.budget'
		const childPath = await writeAgentFile(
			tempDir,
			'phase16-child-budget.json',
			buildWorkerChildAgentFile(childAgentId),
		)
		const lifecycle = new AgentLifecycleService({ state_store: store })
		await lifecycle.registerAgentFile(childPath)
		await lifecycle.deployAgentFile(childPath)

		const deferred = createDeferred<RuntimeTerminalResult>()
		const { adapter } = createStubAdapter([deferred.promise])
		const service = new ManagedSubagentService({
			state_store: store,
			runtime_adapter: adapter,
			lifecycle_service: lifecycle,
		})

		const first = await service.launch({
			parent_run_id: 'parent-run-budget',
			parent_task_id: 'task-budget',
			child_role: 'worker',
			agent_ref: childAgentId,
			objective: 'First worker',
			input_message: 'first input',
			acceptance_criteria: ['Finish'],
			write_set: {
				mode: 'allow_list',
				items: [
					{
						resource_kind: 'file',
						resource_ref: 'src/core/budget-one.ts',
						scope: 'exact',
						access: 'create_or_modify',
					},
				],
			},
			budgets: {
				max_children: 1,
				max_spawn_depth: 1,
			},
		})

		expect(first.task_package.budgets).toMatchObject({
			max_children: 1,
			max_spawn_depth: 1,
		})

		await expect(
			service.launch({
				parent_run_id: 'parent-run-budget',
				parent_task_id: 'task-budget',
				child_role: 'worker',
				agent_ref: childAgentId,
				objective: 'Second worker',
				input_message: 'second input',
				acceptance_criteria: ['Finish'],
				write_set: {
					mode: 'allow_list',
					items: [
						{
							resource_kind: 'file',
							resource_ref: 'src/core/budget-two.ts',
							scope: 'exact',
							access: 'create_or_modify',
						},
					],
				},
				budgets: {
					max_children: 1,
					max_spawn_depth: 1,
				},
			}),
		).rejects.toMatchObject({
			code: 'SUBAGENT_BUDGET_EXHAUSTED',
		} satisfies Pick<AppError, 'code'>)

		deferred.resolve({
			outcome: 'success',
			output: TEXT_OUTPUT,
			output_text: 'first-complete',
		})
		const finished = await service.wait({
			subagent_id: first.subagent_id,
			wait_mode: 'terminal_only',
		})
		expect(finished.outcome).toBe('accepted')
	})

	it('rejects unknown keys on managed-subagent launch budget objects', async () => {
		const store = await createStore()
		store.createRun({
			run_id: 'parent-run-budget-shape',
			resolved_revision_id: 'rev-parent-budget-shape',
			entry_node_id: 'entry',
			started_via: 'direct',
		})

		const tempDir = path.dirname(store.database_path)
		const childAgentId = 'agent.phase16.child.budget.shape'
		const childPath = await writeAgentFile(
			tempDir,
			'phase16-child-budget-shape.json',
			buildWorkerChildAgentFile(childAgentId),
		)
		const lifecycle = new AgentLifecycleService({ state_store: store })
		await lifecycle.registerAgentFile(childPath)
		await lifecycle.deployAgentFile(childPath)

		const { adapter } = createStubAdapter([])
		const service = new ManagedSubagentService({
			state_store: store,
			runtime_adapter: adapter,
			lifecycle_service: lifecycle,
		})

		await expect(
			service.launch({
				parent_run_id: 'parent-run-budget-shape',
				parent_task_id: 'task-budget-shape',
				child_role: 'worker',
				agent_ref: childAgentId,
				objective: 'Worker with malformed budget',
				input_message: 'input',
				acceptance_criteria: ['Finish'],
				write_set: {
					mode: 'allow_list',
					items: [
						{
							resource_kind: 'file',
							resource_ref: 'src/core/budget-shape.ts',
							scope: 'exact',
							access: 'create_or_modify',
						},
					],
				},
				budgets: {
					max_children: 1,
					unknown_limit: 5,
				} as unknown as NonNullable<Parameters<ManagedSubagentService['launch']>[0]['budgets']>,
			}),
		).rejects.toMatchObject({
			code: 'INVALID_SUBAGENT_REQUEST',
			message: 'Managed subagent budget object does not allow unknown keys: unknown_limit.',
		} satisfies Pick<AppError, 'code' | 'message'>)
	})

	it('rejects structurally invalid managed-subagent write sets before resolving a child', async () => {
		const store = await createStore()
		store.createRun({
			run_id: 'parent-run-write-set-shape',
			resolved_revision_id: 'rev-parent-write-set-shape',
			entry_node_id: 'entry',
			started_via: 'direct',
		})

		const { adapter } = createStubAdapter([])
		const service = new ManagedSubagentService({
			state_store: store,
			runtime_adapter: adapter,
		})

		await expect(
			service.launch({
				parent_run_id: 'parent-run-write-set-shape',
				parent_task_id: 'task-write-set-shape',
				child_role: 'worker',
				agent_ref: 'agent.phase16.child.write-set-shape',
				objective: 'Worker with malformed write set',
				input_message: 'input',
				acceptance_criteria: ['Finish'],
				write_set: {
					mode: 'allow_list',
					items: [
						{
							resource_kind: 'file',
							resource_ref: 'src/core/write-set-shape.ts',
							scope: 'recursive',
							access: 'create_or_modify',
						},
					],
				} as unknown as Parameters<ManagedSubagentService['launch']>[0]['write_set'],
			}),
		).rejects.toMatchObject({
			code: 'INVALID_SUBAGENT_REQUEST',
			message: 'Managed subagent write_set item 1 scope must be one of: exact, descendants.',
		} satisfies Pick<AppError, 'code' | 'message'>)

		expect(
			store.listManagedSubagentRecords({
				parent_run_id: 'parent-run-write-set-shape',
			}),
		).toHaveLength(0)
	})

	it('returns reviewer findings and enforces the review-loop ceiling', async () => {
		const store = await createStore()
		store.createRun({
			run_id: 'parent-run-review',
			resolved_revision_id: 'rev-parent-review',
			entry_node_id: 'entry',
			started_via: 'direct',
		})
		store.createRun({
			run_id: 'parent-run-review-independent',
			resolved_revision_id: 'rev-parent-review-independent',
			entry_node_id: 'entry',
			started_via: 'direct',
		})

		const tempDir = path.dirname(store.database_path)
		const childAgentId = 'agent.phase16.child.reviewer'
		const childPath = await writeAgentFile(
			tempDir,
			'phase16-child-reviewer.json',
			buildReviewerChildAgentFile(childAgentId),
		)
		const lifecycle = new AgentLifecycleService({ state_store: store })
		await lifecycle.registerAgentFile(childPath)
		await lifecycle.deployAgentFile(childPath)

		const firstReviewResult = {
			outcome: 'success',
			output: JSON_OUTPUT,
			output_json: {
				summary: 'Review found a boundary issue.',
				findings: [
					{
						finding_id: 'finding-1',
						severity: 'high',
						category: 'boundary',
						summary: 'The delegated change touches a disallowed path.',
						evidence_refs: ['src/core/reviewer-example.ts'],
						recommended_action: 'fix',
					},
				],
			},
		} as const satisfies RuntimeTerminalResult
		const { adapter } = createStubAdapter([firstReviewResult, firstReviewResult])
		const service = new ManagedSubagentService({
			state_store: store,
			runtime_adapter: adapter,
			lifecycle_service: lifecycle,
		})

		const reviewed = await service.launch({
			parent_run_id: 'parent-run-review',
			parent_task_id: 'task-review',
			child_role: 'reviewer',
			agent_ref: childAgentId,
			objective: 'Review the delegated patch',
			input_message: 'Review input',
			acceptance_criteria: ['Return findings when needed'],
			write_set: {
				mode: 'allow_list',
				items: [
					{
						resource_kind: 'file',
						resource_ref: 'src/core/reviewer-example.ts',
						scope: 'exact',
						access: 'create_or_modify',
					},
				],
			},
			budgets: {
				max_children: 3,
				max_spawn_depth: 1,
				max_review_loops: 1,
			},
		})

		const reviewWait = await service.wait({
			subagent_id: reviewed.subagent_id,
			wait_mode: 'terminal_or_update',
		})
		expect(reviewWait).toMatchObject({
			subagent_id: reviewed.subagent_id,
			state: 'terminal',
			outcome: 'review_required',
			final_payload: {
				summary: 'Review found a boundary issue.',
			},
			findings: [
				{
					finding_id: 'finding-1',
					severity: 'high',
					category: 'boundary',
				},
			],
			reason_code: 'review_findings_raised',
		})

		const reviewClosed = await service.close({
			subagent_id: reviewed.subagent_id,
			close_disposition: 'abandoned_by_parent',
		})
		expect(reviewClosed).toMatchObject({
			close_status: 'closed',
			state: 'closed',
			outcome: 'review_required',
		})

		await expect(
			service.launch({
				parent_run_id: 'parent-run-review',
				parent_task_id: 'task-review',
				child_role: 'reviewer',
				agent_ref: childAgentId,
				objective: 'Review again',
				input_message: 'Second review input',
				acceptance_criteria: ['Return findings when needed'],
				write_set: {
					mode: 'allow_list',
					items: [
						{
							resource_kind: 'file',
							resource_ref: 'src/core/reviewer-example-two.ts',
							scope: 'exact',
							access: 'create_or_modify',
						},
					],
				},
				budgets: {
					max_children: 3,
					max_spawn_depth: 1,
					max_review_loops: 1,
				},
			}),
		).rejects.toMatchObject({
			code: 'SUBAGENT_BUDGET_EXHAUSTED',
		} satisfies Pick<AppError, 'code'>)

		const independentReview = await service.launch({
			parent_run_id: 'parent-run-review-independent',
			parent_task_id: 'task-review',
			child_role: 'reviewer',
			agent_ref: childAgentId,
			objective: 'Review same task id in an independent parent run',
			input_message: 'Independent review input',
			acceptance_criteria: ['Return findings when needed'],
			write_set: {
				mode: 'allow_list',
				items: [
					{
						resource_kind: 'file',
						resource_ref: 'src/core/reviewer-example-independent.ts',
						scope: 'exact',
						access: 'create_or_modify',
					},
				],
			},
			budgets: {
				max_children: 3,
				max_spawn_depth: 1,
				max_review_loops: 1,
			},
		})
		expect(independentReview.lineage).toMatchObject({
			parent_run_id: 'parent-run-review-independent',
			parent_task_id: 'task-review',
		})
	})

	it('records accepted, changes-requested, repair-linked, and budget-exhausted review workflow state', async () => {
		const store = await createStore()
		store.createRun({
			run_id: 'parent-run-review-workflow',
			resolved_revision_id: 'rev-parent-review-workflow',
			entry_node_id: 'entry',
			started_via: 'direct',
		})
		store.createRun({
			run_id: 'parent-run-review-workflow-independent',
			resolved_revision_id: 'rev-parent-review-workflow-independent',
			entry_node_id: 'entry',
			started_via: 'direct',
		})

		const { adapter } = createStubAdapter([])
		const service = new ManagedSubagentService({
			state_store: store,
			runtime_adapter: adapter,
		})

		const createReviewer = (args: {
			subagent_id: string
			parent_run_id?: string
			parent_task_id: string
			resource_ref: string
			max_review_loops: number
			findings?: ManagedSubagentFinding[]
		}) => {
			const parentRunId = args.parent_run_id ?? 'parent-run-review-workflow'
			const review = store.createManagedSubagentRecord({
				subagent_id: args.subagent_id,
				child_run_id: `${args.subagent_id}-run`,
				child_role: 'reviewer',
				child_logical_agent_id: 'agent.phase16.child.workflow-reviewer',
				child_resolved_revision_id: 'rev-child-workflow-reviewer',
				lineage: {
					root_run_id: parentRunId,
					parent_run_id: parentRunId,
					parent_task_id: args.parent_task_id,
					depth: 1,
				},
				task_package: {
					agent_ref: 'agent.phase16.child.workflow-reviewer',
					objective: 'Review workflow state',
					input_message: 'review input',
					acceptance_criteria: ['Record explicit review workflow state'],
					prohibitions: [],
					write_set: {
						mode: 'allow_list',
						items: [
							{
								resource_kind: 'file',
								resource_ref: args.resource_ref,
								scope: 'exact',
								access: 'create_or_modify',
							},
						],
					},
					budgets: {
						max_review_loops: args.max_review_loops,
					},
					control_messages: [],
				},
			})
			const finalOutput = {
				summary: args.findings ? 'changes requested' : 'accepted',
				...(args.findings ? { findings: args.findings } : {}),
			}
			store.markManagedSubagentTerminal({
				subagent_id: review.subagent_id,
				terminal_result: {
					outcome: args.findings ? 'review_required' : 'accepted',
					child_run_status: 'completed',
					final_output: finalOutput,
					final_output_mode: 'json',
					final_payload: {
						summary: finalOutput.summary,
					},
					findings: args.findings ?? null,
					reason_code: args.findings ? 'review_findings_raised' : null,
				},
			})
			return review
		}

		const acceptedReview = createReviewer({
			subagent_id: 'review-workflow-accepted',
			parent_task_id: 'task-workflow-accepted',
			resource_ref: 'src/core/workflow-accepted.ts',
			max_review_loops: 1,
		})
		await expect(
			service.recordReviewDecision({
				subagent_id: acceptedReview.subagent_id,
				decision: 'accepted',
			}),
		).resolves.toMatchObject({
			workflow_state: {
				decision: 'accepted',
				finding_ids: [],
				outcome: 'accepted',
				budget_exhausted: false,
				repair_subagent_id: null,
			},
		})

		const changeFindings: ManagedSubagentFinding[] = [
			{
				finding_id: 'finding-change-1',
				severity: 'high',
				category: 'correctness',
				summary: 'Repair is required.',
				evidence_refs: ['src/core/workflow-change.ts'],
				recommended_action: 'fix',
			},
		]
		const changesReview = createReviewer({
			subagent_id: 'review-workflow-changes',
			parent_task_id: 'task-workflow-repair',
			resource_ref: 'src/core/workflow-change.ts',
			max_review_loops: 2,
			findings: [...changeFindings],
		})
		const changesRequested = await service.recordReviewDecision({
			subagent_id: changesReview.subagent_id,
			decision: 'changes_requested',
		})
		expect(changesRequested).toMatchObject({
			workflow_state: {
				loop_index: 1,
				max_review_loops: 2,
				decision: 'changes_requested',
				finding_ids: ['finding-change-1'],
				outcome: 'changes_requested',
				budget_exhausted: false,
			},
		})

		const repairWorker = store.createManagedSubagentRecord({
			subagent_id: 'repair-workflow-worker',
			child_run_id: 'repair-workflow-worker-run',
			child_role: 'worker',
			child_logical_agent_id: 'agent.phase16.child.workflow-worker',
			child_resolved_revision_id: 'rev-child-workflow-worker',
			lineage: {
				root_run_id: 'parent-run-review-workflow',
				parent_run_id: 'parent-run-review-workflow',
				parent_task_id: 'task-workflow-repair',
				depth: 1,
			},
			task_package: {
				agent_ref: 'agent.phase16.child.workflow-worker',
				objective: 'Repair reviewer findings',
				input_message: 'repair input',
				acceptance_criteria: ['Address finding-change-1'],
				prohibitions: [],
				write_set: {
					mode: 'allow_list',
					items: [
						{
							resource_kind: 'file',
							resource_ref: 'src/core/workflow-repair.ts',
							scope: 'exact',
							access: 'create_or_modify',
						},
					],
				},
				control_messages: [],
			},
		})
		const linked = await service.linkReviewRepair({
			review_subagent_id: changesReview.subagent_id,
			repair_subagent_id: repairWorker.subagent_id,
		})
		expect(linked).toMatchObject({
			workflow_state: {
				repair_subagent_id: repairWorker.subagent_id,
				outcome: 'repair_linked',
			},
		})
		expect(store.getManagedSubagentRecord(repairWorker.subagent_id)?.workflow_state).toMatchObject({
			reviewer_subagent_id: changesReview.subagent_id,
			repair_subagent_id: repairWorker.subagent_id,
			outcome: 'repair_linked',
		})
		const linkedReviewState = store.getManagedSubagentRecord(
			changesReview.subagent_id,
		)?.workflow_state
		await expect(
			service.linkReviewRepair({
				review_subagent_id: changesReview.subagent_id,
				repair_subagent_id: repairWorker.subagent_id,
			}),
		).resolves.toEqual({
			review_subagent_id: changesReview.subagent_id,
			repair_subagent_id: repairWorker.subagent_id,
			workflow_state: linkedReviewState,
		})
		expect(store.getManagedSubagentRecord(changesReview.subagent_id)?.workflow_state).toEqual(
			linkedReviewState,
		)
		expect(store.getManagedSubagentRecord(repairWorker.subagent_id)?.workflow_state).toEqual(
			linkedReviewState,
		)

		const differentRepairWorker = store.createManagedSubagentRecord({
			subagent_id: 'repair-workflow-worker-different',
			child_run_id: 'repair-workflow-worker-different-run',
			child_role: 'worker',
			child_logical_agent_id: 'agent.phase16.child.workflow-worker',
			child_resolved_revision_id: 'rev-child-workflow-worker',
			lineage: {
				root_run_id: 'parent-run-review-workflow',
				parent_run_id: 'parent-run-review-workflow',
				parent_task_id: 'task-workflow-repair',
				depth: 1,
			},
			task_package: {
				agent_ref: 'agent.phase16.child.workflow-worker',
				objective: 'Attempt conflicting repair link',
				input_message: 'different repair input',
				acceptance_criteria: ['Do not replace an existing repair link'],
				prohibitions: [],
				write_set: {
					mode: 'allow_list',
					items: [
						{
							resource_kind: 'file',
							resource_ref: 'src/core/workflow-repair-different.ts',
							scope: 'exact',
							access: 'create_or_modify',
						},
					],
				},
				control_messages: [],
			},
		})
		await expect(
			service.linkReviewRepair({
				review_subagent_id: changesReview.subagent_id,
				repair_subagent_id: differentRepairWorker.subagent_id,
			}),
		).rejects.toMatchObject({
			code: 'SUBAGENT_WORKFLOW_CONFLICT',
		} satisfies Pick<AppError, 'code'>)
		expect(store.getManagedSubagentRecord(changesReview.subagent_id)?.workflow_state).toEqual(
			linkedReviewState,
		)
		expect(
			store.getManagedSubagentRecord(differentRepairWorker.subagent_id)?.workflow_state,
		).toBeNull()

		const secondChangesReview = createReviewer({
			subagent_id: 'review-workflow-changes-second',
			parent_task_id: 'task-workflow-repair',
			resource_ref: 'src/core/workflow-change-second.ts',
			max_review_loops: 3,
			findings: [
				{
					finding_id: 'finding-change-2',
					severity: 'medium',
					category: 'correctness',
					summary: 'A second review needs a different repair worker.',
					evidence_refs: ['src/core/workflow-change-second.ts'],
					recommended_action: 'fix',
				},
			],
		})
		await expect(
			service.recordReviewDecision({
				subagent_id: secondChangesReview.subagent_id,
				decision: 'changes_requested',
			}),
		).resolves.toMatchObject({
			workflow_state: {
				loop_index: 2,
				outcome: 'changes_requested',
				budget_exhausted: false,
			},
		})
		await expect(
			service.linkReviewRepair({
				review_subagent_id: secondChangesReview.subagent_id,
				repair_subagent_id: repairWorker.subagent_id,
			}),
		).rejects.toMatchObject({
			code: 'SUBAGENT_WORKFLOW_CONFLICT',
		} satisfies Pick<AppError, 'code'>)
		expect(store.getManagedSubagentRecord(repairWorker.subagent_id)?.workflow_state).toMatchObject({
			reviewer_subagent_id: changesReview.subagent_id,
			repair_subagent_id: repairWorker.subagent_id,
			outcome: 'repair_linked',
		})

		const firstSharedTaskReview = createReviewer({
			subagent_id: 'review-workflow-shared-task-first-run',
			parent_task_id: 'task-workflow-shared',
			resource_ref: 'src/core/workflow-shared-first-run.ts',
			max_review_loops: 3,
			findings: [
				{
					finding_id: 'finding-shared-1',
					severity: 'high',
					category: 'correctness',
					summary: 'First parent run needs changes.',
					evidence_refs: ['src/core/workflow-shared-first-run.ts'],
					recommended_action: 'fix',
				},
			],
		})
		await expect(
			service.recordReviewDecision({
				subagent_id: firstSharedTaskReview.subagent_id,
				decision: 'changes_requested',
			}),
		).resolves.toMatchObject({
			workflow_state: {
				loop_index: 1,
				outcome: 'changes_requested',
				budget_exhausted: false,
			},
		})
		const independentSharedTaskReview = createReviewer({
			subagent_id: 'review-workflow-shared-task-independent-run',
			parent_run_id: 'parent-run-review-workflow-independent',
			parent_task_id: 'task-workflow-shared',
			resource_ref: 'src/core/workflow-shared-independent-run.ts',
			max_review_loops: 3,
			findings: [
				{
					finding_id: 'finding-shared-2',
					severity: 'high',
					category: 'correctness',
					summary: 'Independent parent run needs changes.',
					evidence_refs: ['src/core/workflow-shared-independent-run.ts'],
					recommended_action: 'fix',
				},
			],
		})
		await expect(
			service.recordReviewDecision({
				subagent_id: independentSharedTaskReview.subagent_id,
				decision: 'changes_requested',
			}),
		).resolves.toMatchObject({
			workflow_state: {
				loop_index: 1,
				outcome: 'changes_requested',
				budget_exhausted: false,
			},
		})

		const budgetReview = createReviewer({
			subagent_id: 'review-workflow-budget',
			parent_task_id: 'task-workflow-budget',
			resource_ref: 'src/core/workflow-budget.ts',
			max_review_loops: 1,
			findings: [
				{
					finding_id: 'finding-budget-1',
					severity: 'critical',
					category: 'architecture',
					summary: 'No review loops remain.',
					evidence_refs: ['src/core/workflow-budget.ts'],
					recommended_action: 'replan',
				},
			],
		})
		await expect(
			service.recordReviewDecision({
				subagent_id: budgetReview.subagent_id,
				decision: 'changes_requested',
			}),
		).resolves.toMatchObject({
			workflow_state: {
				loop_index: 1,
				max_review_loops: 1,
				decision: 'changes_requested',
				finding_ids: ['finding-budget-1'],
				outcome: 'budget_exhausted',
				budget_exhausted: true,
			},
		})
		await expect(
			service.linkReviewRepair({
				review_subagent_id: budgetReview.subagent_id,
				repair_subagent_id: repairWorker.subagent_id,
			}),
		).rejects.toMatchObject({
			code: 'SUBAGENT_WORKFLOW_INVALID',
		} satisfies Pick<AppError, 'code'>)
	})

	it('proves the bounded worker to reviewer to repair to final-review accepted workflow path', async () => {
		const store = await createStore()
		store.createRun({
			run_id: 'parent-run-review-loop-e2e',
			resolved_revision_id: 'rev-parent-review-loop-e2e',
			entry_node_id: 'entry',
			started_via: 'direct',
		})

		const tempDir = path.dirname(store.database_path)
		const workerAgentId = 'agent.phase16.child.review-loop-worker'
		const reviewerAgentId = 'agent.phase16.child.review-loop-reviewer'
		const workerPath = await writeAgentFile(
			tempDir,
			'phase16-review-loop-worker.json',
			buildWorkerChildAgentFile(workerAgentId),
		)
		const reviewerPath = await writeAgentFile(
			tempDir,
			'phase16-review-loop-reviewer.json',
			buildReviewerChildAgentFile(reviewerAgentId),
		)
		const lifecycle = new AgentLifecycleService({ state_store: store })
		await lifecycle.registerAgentFile(workerPath)
		await lifecycle.deployAgentFile(workerPath)
		await lifecycle.registerAgentFile(reviewerPath)
		await lifecycle.deployAgentFile(reviewerPath)

		const { adapter, requests } = createStubAdapter([
			{
				outcome: 'success',
				output: TEXT_OUTPUT,
				output_text: 'initial-worker-complete',
			},
			{
				outcome: 'success',
				output: JSON_OUTPUT,
				output_json: {
					summary: 'Reviewer requested one repair.',
					findings: [
						{
							finding_id: 'finding-e2e-1',
							severity: 'high',
							category: 'correctness',
							summary: 'Repair loop must be linked before final review.',
							evidence_refs: ['src/core/review-loop-e2e.ts'],
							recommended_action: 'fix',
						},
					],
				},
			},
			{
				outcome: 'success',
				output: TEXT_OUTPUT,
				output_text: 'repair-worker-complete',
			},
			{
				outcome: 'success',
				output: JSON_OUTPUT,
				output_json: {
					summary: 'Final review accepted the repair.',
				},
			},
		] satisfies RuntimeTerminalResult[])
		const service = new ManagedSubagentService({
			state_store: store,
			runtime_adapter: adapter,
			lifecycle_service: lifecycle,
		})

		const initialWorker = await service.launch({
			parent_run_id: 'parent-run-review-loop-e2e',
			parent_task_id: 'task-review-loop-e2e',
			child_role: 'worker',
			agent_ref: workerAgentId,
			objective: 'Implement the initial bounded change',
			input_message: 'initial worker input',
			acceptance_criteria: ['Return initial worker output'],
			write_set: {
				mode: 'allow_list',
				items: [
					{
						resource_kind: 'file',
						resource_ref: 'src/core/review-loop-e2e-initial.ts',
						scope: 'exact',
						access: 'create_or_modify',
					},
				],
			},
		})
		await expect(
			service.wait({
				subagent_id: initialWorker.subagent_id,
				wait_mode: 'terminal_or_update',
			}),
		).resolves.toMatchObject({
			state: 'terminal',
			outcome: 'accepted',
			final_payload: {
				summary: 'initial-worker-complete',
			},
		})
		expect(requests).toHaveLength(1)

		const reviewer = await service.launch({
			parent_run_id: 'parent-run-review-loop-e2e',
			parent_task_id: 'task-review-loop-e2e',
			child_role: 'reviewer',
			agent_ref: reviewerAgentId,
			objective: 'Review the initial bounded change',
			input_message: 'review initial worker output',
			acceptance_criteria: ['Return findings when repair is required'],
			write_set: {
				mode: 'allow_list',
				items: [
					{
						resource_kind: 'file',
						resource_ref: 'src/core/review-loop-e2e-review.ts',
						scope: 'exact',
						access: 'create_or_modify',
					},
				],
			},
			budgets: {
				max_review_loops: 2,
			},
		})
		const reviewerWait = await service.wait({
			subagent_id: reviewer.subagent_id,
			wait_mode: 'terminal_or_update',
		})
		expect(reviewerWait).toMatchObject({
			state: 'terminal',
			outcome: 'review_required',
			findings: [
				{
					finding_id: 'finding-e2e-1',
				},
			],
		})
		expect(requests).toHaveLength(2)
		await expect(
			service.recordReviewDecision({
				subagent_id: reviewer.subagent_id,
				decision: 'changes_requested',
			}),
		).resolves.toMatchObject({
			workflow_state: {
				loop_index: 1,
				max_review_loops: 2,
				decision: 'changes_requested',
				finding_ids: ['finding-e2e-1'],
				outcome: 'changes_requested',
				budget_exhausted: false,
				repair_subagent_id: null,
			},
		})

		const repairWorker = await service.launch({
			parent_run_id: 'parent-run-review-loop-e2e',
			parent_task_id: 'task-review-loop-e2e',
			child_role: 'worker',
			agent_ref: workerAgentId,
			objective: 'Repair finding-e2e-1',
			input_message: 'repair finding-e2e-1',
			acceptance_criteria: ['Address finding-e2e-1'],
			write_set: {
				mode: 'allow_list',
				items: [
					{
						resource_kind: 'file',
						resource_ref: 'src/core/review-loop-e2e-repair.ts',
						scope: 'exact',
						access: 'create_or_modify',
					},
				],
			},
		})
		const linkedRepair = await service.linkReviewRepair({
			review_subagent_id: reviewer.subagent_id,
			repair_subagent_id: repairWorker.subagent_id,
		})
		expect(linkedRepair).toMatchObject({
			workflow_state: {
				reviewer_subagent_id: reviewer.subagent_id,
				repair_subagent_id: repairWorker.subagent_id,
				outcome: 'repair_linked',
			},
		})
		await expect(
			service.wait({
				subagent_id: repairWorker.subagent_id,
				wait_mode: 'terminal_or_update',
			}),
		).resolves.toMatchObject({
			state: 'terminal',
			outcome: 'accepted',
			final_payload: {
				summary: 'repair-worker-complete',
			},
		})

		const finalReview = await service.launch({
			parent_run_id: 'parent-run-review-loop-e2e',
			parent_task_id: 'task-review-loop-e2e',
			child_role: 'final_review',
			agent_ref: reviewerAgentId,
			objective: 'Perform final review after repair',
			input_message: 'final review repaired output',
			acceptance_criteria: ['Accept only when repair is sufficient'],
			write_set: {
				mode: 'allow_list',
				items: [
					{
						resource_kind: 'file',
						resource_ref: 'src/core/review-loop-e2e-final-review.ts',
						scope: 'exact',
						access: 'create_or_modify',
					},
				],
			},
			budgets: {
				max_review_loops: 2,
			},
		})
		await expect(
			service.wait({
				subagent_id: finalReview.subagent_id,
				wait_mode: 'terminal_or_update',
			}),
		).resolves.toMatchObject({
			state: 'terminal',
			outcome: 'accepted',
			final_payload: {
				summary: 'Final review accepted the repair.',
			},
		})
		await expect(
			service.recordReviewDecision({
				subagent_id: finalReview.subagent_id,
				decision: 'accepted',
			}),
		).resolves.toMatchObject({
			workflow_state: {
				loop_index: 2,
				max_review_loops: 2,
				decision: 'accepted',
				finding_ids: [],
				outcome: 'accepted',
				budget_exhausted: false,
				repair_subagent_id: null,
			},
		})

		expect(
			store
				.listManagedSubagentRecords({
					parent_run_id: 'parent-run-review-loop-e2e',
					parent_task_id: 'task-review-loop-e2e',
				})
				.map((record) => ({
					role: record.child_role,
					state: record.state,
					workflow_outcome: record.workflow_state?.outcome ?? null,
				})),
		).toEqual([
			{ role: 'worker', state: 'terminal', workflow_outcome: null },
			{ role: 'reviewer', state: 'terminal', workflow_outcome: 'repair_linked' },
			{ role: 'worker', state: 'terminal', workflow_outcome: 'repair_linked' },
			{ role: 'final_review', state: 'terminal', workflow_outcome: 'accepted' },
		])
	})

	it('returns explicit status and records explicit cancellation without claiming runtime cancellation', async () => {
		const store = await createStore()
		store.createRun({
			run_id: 'parent-run-explicit-cancel',
			resolved_revision_id: 'rev-parent-explicit-cancel',
			entry_node_id: 'entry',
			started_via: 'direct',
		})

		const tempDir = path.dirname(store.database_path)
		const childAgentId = 'agent.phase16.child.explicit-cancel'
		const childPath = await writeAgentFile(
			tempDir,
			'phase16-child-explicit-cancel.json',
			buildWorkerChildAgentFile(childAgentId),
		)
		const lifecycle = new AgentLifecycleService({ state_store: store })
		await lifecycle.registerAgentFile(childPath)
		await lifecycle.deployAgentFile(childPath)

		const deferred = createDeferred<RuntimeTerminalResult>()
		const { adapter } = createStubAdapter([deferred.promise])
		const service = new ManagedSubagentService({
			state_store: store,
			runtime_adapter: adapter,
			lifecycle_service: lifecycle,
		})

		const launched = await service.launch({
			parent_run_id: 'parent-run-explicit-cancel',
			parent_task_id: 'task-explicit-cancel',
			child_role: 'worker',
			agent_ref: childAgentId,
			objective: 'Cancelable worker through explicit primitive',
			input_message: 'cancel input',
			acceptance_criteria: ['Return final worker output'],
			write_set: {
				mode: 'allow_list',
				items: [
					{
						resource_kind: 'file',
						resource_ref: 'src/core/explicit-cancel.ts',
						scope: 'exact',
						access: 'create_or_modify',
					},
				],
			},
		})

		await expect(
			service.status({
				subagent_id: launched.subagent_id,
			}),
		).resolves.toMatchObject({
			subagent_id: launched.subagent_id,
			state: 'running',
			child_role: 'worker',
		})

		const cancelled = await service.cancel({
			subagent_id: launched.subagent_id,
			message_id: 'explicit-cancel-message',
			reason: 'operator requested stop',
		})
		expect(cancelled).toMatchObject({
			subagent_id: launched.subagent_id,
			cancel_status: 'cancelling',
			state: 'cancelling',
			outcome: null,
			reason_code: null,
			runtime_cancellation_delivered: false,
		})
		expect(store.getManagedSubagentRecord(launched.subagent_id)).toMatchObject({
			state: 'cancelling',
			close_disposition: 'cancelled_by_parent',
			task_package: {
				control_messages: [
					{
						message_id: 'explicit-cancel-message',
						message_kind: 'cancel',
						payload: {
							reason: 'operator requested stop',
						},
					},
				],
			},
		})

		const duplicateCancel = await service.cancel({
			subagent_id: launched.subagent_id,
			message_id: 'explicit-cancel-message',
			reason: 'operator requested stop',
		})
		expect(duplicateCancel).toMatchObject({
			cancel_status: 'already_cancelling',
			state: 'cancelling',
			runtime_cancellation_delivered: false,
		})
		expect(
			store.getManagedSubagentRecord(launched.subagent_id)?.task_package.control_messages,
		).toHaveLength(1)

		deferred.resolve({
			outcome: 'success',
			output: TEXT_OUTPUT,
			output_text: 'child-complete-after-explicit-cancel',
		})

		const waited = await service.wait({
			subagent_id: launched.subagent_id,
			wait_mode: 'terminal_or_update',
		})
		expect(waited).toMatchObject({
			state: 'terminal',
			outcome: 'cancelled',
			final_payload: null,
			reason_code: 'parent_cancelled',
		})

		const terminalCancel = await service.cancel({
			subagent_id: launched.subagent_id,
			message_id: 'explicit-cancel-after-terminal',
		})
		expect(terminalCancel).toMatchObject({
			cancel_status: 'already_terminal',
			state: 'terminal',
			outcome: 'cancelled',
			reason_code: 'parent_cancelled',
			runtime_cancellation_delivered: false,
		})
	})

	it('accepts bounded control messages and honors cancelled_by_parent close semantics', async () => {
		const store = await createStore()
		store.createRun({
			run_id: 'parent-run-cancel',
			resolved_revision_id: 'rev-parent-cancel',
			entry_node_id: 'entry',
			started_via: 'direct',
		})

		const tempDir = path.dirname(store.database_path)
		const childAgentId = 'agent.phase16.child.cancel'
		const childPath = await writeAgentFile(
			tempDir,
			'phase16-child-cancel.json',
			buildWorkerChildAgentFile(childAgentId),
		)
		const lifecycle = new AgentLifecycleService({ state_store: store })
		await lifecycle.registerAgentFile(childPath)
		await lifecycle.deployAgentFile(childPath)

		const deferred = createDeferred<RuntimeTerminalResult>()
		const { adapter } = createStubAdapter([deferred.promise])
		const service = new ManagedSubagentService({
			state_store: store,
			runtime_adapter: adapter,
			lifecycle_service: lifecycle,
		})

		const launched = await service.launch({
			parent_run_id: 'parent-run-cancel',
			parent_task_id: 'task-cancel',
			child_role: 'worker',
			agent_ref: childAgentId,
			objective: 'Cancelable worker',
			input_message: 'cancel input',
			acceptance_criteria: ['Return final worker output'],
			write_set: {
				mode: 'allow_list',
				items: [
					{
						resource_kind: 'file',
						resource_ref: 'src/core/cancel-one.ts',
						scope: 'exact',
						access: 'create_or_modify',
					},
				],
			},
			budgets: {
				max_children: 2,
				max_spawn_depth: 1,
				max_review_loops: 1,
			},
		})

		const statusMessage = await service.send({
			subagent_id: launched.subagent_id,
			message_id: 'msg-status',
			message_kind: 'request_status',
			payload: {},
		})
		expect(statusMessage).toMatchObject({
			delivery_state: 'accepted',
			state: 'running',
			reason_code: null,
		})
		expect(
			store.getManagedSubagentRecord(launched.subagent_id)?.task_package.control_messages,
		).toHaveLength(1)

		const duplicateStatusMessage = await service.send({
			subagent_id: launched.subagent_id,
			message_id: 'msg-status',
			message_kind: 'request_status',
			payload: {},
		})
		expect(duplicateStatusMessage).toMatchObject({
			delivery_state: 'accepted',
			state: 'running',
			reason_code: null,
		})
		expect(
			store.getManagedSubagentRecord(launched.subagent_id)?.task_package.control_messages,
		).toHaveLength(1)

		const conflictingDuplicateStatusMessage = await service.send({
			subagent_id: launched.subagent_id,
			message_id: 'msg-status',
			message_kind: 'clarify_scope',
			payload: {
				summary: 'Different control intent with a reused id',
			},
		})
		expect(conflictingDuplicateStatusMessage).toMatchObject({
			delivery_state: 'rejected',
			state: 'running',
			reason_code: 'invalid_control_message',
		})
		expect(
			store.getManagedSubagentRecord(launched.subagent_id)?.task_package.control_messages,
		).toHaveLength(1)

		const rejectedCancel = await service.cancel({
			subagent_id: launched.subagent_id,
			message_id: 'msg-status',
			reason: 'operator tried to stop with a reused status id',
		})
		expect(rejectedCancel).toMatchObject({
			cancel_status: 'rejected',
			state: 'running',
			outcome: null,
			reason_code: 'invalid_control_message',
			runtime_cancellation_delivered: false,
		})
		expect(store.getManagedSubagentRecord(launched.subagent_id)).toMatchObject({
			state: 'running',
			close_disposition: null,
		})
		expect(
			store.getManagedSubagentRecord(launched.subagent_id)?.task_package.control_messages,
		).toHaveLength(1)

		const budgetMessage = await service.send({
			subagent_id: launched.subagent_id,
			message_id: 'msg-budget',
			message_kind: 'update_budget',
			payload: {
				budgets: {
					max_children: 1,
				},
			},
		})
		expect(budgetMessage).toMatchObject({
			delivery_state: 'accepted',
			state: 'running',
			reason_code: null,
		})
		expect(
			store.getManagedSubagentRecord(launched.subagent_id)?.task_package.budgets,
		).toMatchObject({
			max_children: 1,
			max_spawn_depth: 1,
			max_review_loops: 1,
		})

		const closing = await service.close({
			subagent_id: launched.subagent_id,
			close_disposition: 'cancelled_by_parent',
		})
		expect(closing).toMatchObject({
			close_status: 'closing',
			state: 'cancelling',
		})

		deferred.resolve({
			outcome: 'success',
			output: TEXT_OUTPUT,
			output_text: 'child-complete-after-cancel',
		})

		const waited = await service.wait({
			subagent_id: launched.subagent_id,
			wait_mode: 'terminal_or_update',
		})
		expect(waited).toMatchObject({
			state: 'terminal',
			outcome: 'cancelled',
			final_payload: null,
			reason_code: 'parent_cancelled',
		})

		const closed = await service.close({
			subagent_id: launched.subagent_id,
			close_disposition: 'cancelled_by_parent',
		})
		expect(closed).toMatchObject({
			close_status: 'closed',
			state: 'closed',
			outcome: 'cancelled',
			reason_code: 'parent_cancelled',
		})
	})

	it('rejects unknown keys on bounded control-message payloads and nested budget objects', async () => {
		const store = await createStore()
		store.createRun({
			run_id: 'parent-run-control-shape',
			resolved_revision_id: 'rev-parent-control-shape',
			entry_node_id: 'entry',
			started_via: 'direct',
		})

		const tempDir = path.dirname(store.database_path)
		const childAgentId = 'agent.phase16.child.control.shape'
		const childPath = await writeAgentFile(
			tempDir,
			'phase16-child-control-shape.json',
			buildWorkerChildAgentFile(childAgentId),
		)
		const lifecycle = new AgentLifecycleService({ state_store: store })
		await lifecycle.registerAgentFile(childPath)
		await lifecycle.deployAgentFile(childPath)

		const deferred = createDeferred<RuntimeTerminalResult>()
		const { adapter } = createStubAdapter([deferred.promise])
		const service = new ManagedSubagentService({
			state_store: store,
			runtime_adapter: adapter,
			lifecycle_service: lifecycle,
		})

		const launched = await service.launch({
			parent_run_id: 'parent-run-control-shape',
			parent_task_id: 'task-control-shape',
			child_role: 'worker',
			agent_ref: childAgentId,
			objective: 'Worker with strict control payloads',
			input_message: 'input',
			acceptance_criteria: ['Finish'],
			write_set: {
				mode: 'allow_list',
				items: [
					{
						resource_kind: 'file',
						resource_ref: 'src/core/control-shape.ts',
						scope: 'exact',
						access: 'create_or_modify',
					},
				],
			},
			budgets: {
				max_children: 2,
			},
		})

		const invalidClarify = await service.send({
			subagent_id: launched.subagent_id,
			message_id: 'msg-invalid-clarify',
			message_kind: 'clarify_scope',
			payload: {
				summary: 'Need one clarification',
				extra_field: true,
			},
		})
		expect(invalidClarify).toMatchObject({
			delivery_state: 'rejected',
			state: 'running',
			reason_code: 'invalid_control_message',
		})

		const invalidBudgetUpdate = await service.send({
			subagent_id: launched.subagent_id,
			message_id: 'msg-invalid-budget',
			message_kind: 'update_budget',
			payload: {
				budgets: {
					max_children: 1,
					unknown_limit: 3,
				},
			},
		})
		expect(invalidBudgetUpdate).toMatchObject({
			delivery_state: 'rejected',
			state: 'running',
			reason_code: 'invalid_control_message',
		})

		deferred.resolve({
			outcome: 'success',
			output: TEXT_OUTPUT,
			output_text: 'done',
		})
		await service.wait({
			subagent_id: launched.subagent_id,
			wait_mode: 'terminal_only',
		})
	})

	it('rejects a sibling managed subagent with an overlapping write_set before child start', async () => {
		const store = await createStore()
		store.createRun({
			run_id: 'parent-run-conflict',
			resolved_revision_id: 'rev-parent-conflict',
			entry_node_id: 'entry',
			started_via: 'direct',
		})

		const tempDir = path.dirname(store.database_path)
		const childAgentId = 'agent.phase16.child.conflict'
		const childPath = await writeAgentFile(
			tempDir,
			'phase16-child-conflict.json',
			buildWorkerChildAgentFile(childAgentId),
		)
		const lifecycle = new AgentLifecycleService({ state_store: store })
		await lifecycle.registerAgentFile(childPath)
		await lifecycle.deployAgentFile(childPath)

		const deferred = createDeferred<RuntimeTerminalResult>()
		const { adapter, requests } = createStubAdapter([deferred.promise])
		const service = new ManagedSubagentService({
			state_store: store,
			runtime_adapter: adapter,
			lifecycle_service: lifecycle,
		})

		const first = await service.launch({
			parent_run_id: 'parent-run-conflict',
			parent_task_id: 'task-conflict',
			child_role: 'worker',
			agent_ref: childAgentId,
			objective: 'First worker',
			input_message: 'first input',
			acceptance_criteria: ['Finish'],
			write_set: {
				mode: 'allow_list',
				items: [
					{
						resource_kind: 'directory',
						resource_ref: 'src/core/subagents',
						scope: 'descendants',
						access: 'create_or_modify',
					},
				],
			},
		})

		await expect(
			service.launch({
				parent_run_id: 'parent-run-conflict',
				parent_task_id: 'task-conflict',
				child_role: 'worker',
				agent_ref: childAgentId,
				objective: 'Second worker',
				input_message: 'second input',
				acceptance_criteria: ['Finish'],
				write_set: {
					mode: 'allow_list',
					items: [
						{
							resource_kind: 'file',
							resource_ref: 'src/core/subagents/example.ts',
							scope: 'exact',
							access: 'modify_existing',
						},
					],
				},
			}),
		).rejects.toMatchObject({
			code: 'SUBAGENT_WRITE_SET_CONFLICT',
		} satisfies Pick<AppError, 'code'>)

		expect(store.listManagedSubagentRecords({ parent_run_id: 'parent-run-conflict' })).toHaveLength(
			1,
		)

		deferred.resolve({
			outcome: 'success',
			output: TEXT_OUTPUT,
			output_text: 'first-complete',
		})
		const finished = await service.wait({
			subagent_id: first.subagent_id,
			wait_mode: 'terminal_only',
		})
		expect(finished.state).toBe('terminal')
		expect(requests).toHaveLength(1)
	})
})
