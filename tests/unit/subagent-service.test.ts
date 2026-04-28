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
			},
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
		const { adapter } = createStubAdapter([firstReviewResult])
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
