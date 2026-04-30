import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { CodexAppServerRuntimeAdapter } from '../../src/adapters/codex/codex-app-server-runtime-adapter.js'
import { AgentLifecycleService } from '../../src/core/agent-lifecycle.js'
import { SQLiteLocalStateStore } from '../../src/core/state/index.js'
import { buildCliProgram } from '../../src/interfaces/cli.js'

const storesToClose: SQLiteLocalStateStore[] = []
const tempDirsToRemove: string[] = []

async function createStore(prefix: string): Promise<SQLiteLocalStateStore> {
	const tempDir = await mkdtemp(path.join(os.tmpdir(), prefix))
	tempDirsToRemove.push(tempDir)
	const store = new SQLiteLocalStateStore({
		database_path: path.join(tempDir, 'local-state.sqlite'),
	})
	storesToClose.push(store)
	return store
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

function parseStdout(stdout: string): unknown {
	return JSON.parse(stdout)
}

function seedManagedSubagents(store: SQLiteLocalStateStore): void {
	store.createRun({
		run_id: 'parent-run-cli',
		resolved_revision_id: 'rev-parent-cli',
		entry_node_id: 'entry',
		started_via: 'direct',
		created_at: '2026-04-28T08:00:00.000Z',
	})

	store.createManagedSubagentRecord({
		subagent_id: 'subagent-running-cli',
		child_run_id: 'child-run-running-cli',
		child_role: 'worker',
		child_logical_agent_id: 'agent.child.worker',
		child_resolved_revision_id: 'rev-child-worker',
		lineage: {
			root_run_id: 'parent-run-cli',
			parent_run_id: 'parent-run-cli',
			parent_task_id: 'task-cli',
			depth: 1,
		},
		task_package: {
			agent_ref: 'agent.child.worker',
			objective: 'Implement a bounded CLI-owned change',
			input_message: 'Worker input',
			acceptance_criteria: ['Return a summary'],
			prohibitions: ['Do not edit docs'],
			write_set: {
				mode: 'allow_list',
				items: [
					{
						resource_kind: 'file',
						resource_ref: 'src/core/subagent-service.ts',
						scope: 'exact',
						access: 'create_or_modify',
					},
				],
			},
			budgets: {
				max_children: 2,
				max_spawn_depth: 1,
			},
		},
		created_at: '2026-04-28T08:00:01.000Z',
		updated_at: '2026-04-28T08:00:01.000Z',
	})

	const terminal = store.createManagedSubagentRecord({
		subagent_id: 'subagent-terminal-cli',
		child_run_id: 'child-run-terminal-cli',
		child_role: 'reviewer',
		child_logical_agent_id: 'agent.child.reviewer',
		child_resolved_revision_id: 'rev-child-reviewer',
		lineage: {
			root_run_id: 'parent-run-cli',
			parent_run_id: 'parent-run-cli',
			parent_task_id: 'task-review-cli',
			depth: 1,
		},
		task_package: {
			agent_ref: 'agent.child.reviewer',
			objective: 'Review the bounded CLI-owned change',
			input_message: 'Reviewer input',
			acceptance_criteria: ['Return findings when needed'],
			prohibitions: [],
			write_set: {
				mode: 'allow_list',
				items: [
					{
						resource_kind: 'file',
						resource_ref: 'tests/unit/subagent-cli.test.ts',
						scope: 'exact',
						access: 'create_or_modify',
					},
				],
			},
		},
		created_at: '2026-04-28T08:00:02.000Z',
		updated_at: '2026-04-28T08:00:02.000Z',
	})

	store.markManagedSubagentTerminal({
		subagent_id: terminal.subagent_id,
		terminal_at: '2026-04-28T08:00:03.000Z',
		terminal_result: {
			outcome: 'accepted',
			child_run_status: 'completed',
			final_output: {
				summary: 'review accepted',
			},
			final_output_mode: 'json',
			final_payload: {
				summary: 'review accepted',
			},
			findings: null,
			reason_code: null,
		},
	})
}

async function writeLaunchChildAgent(tempDir: string): Promise<string> {
	const agentPath = path.join(tempDir, 'managed-child-agent.json')
	await writeFile(
		agentPath,
		`${JSON.stringify(
			{
				graph_contract_version: '1.0',
				meta: {
					id: 'agent.child.launch',
					name: 'Managed CLI Launch Child',
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
						prompt: 'Return the delegated result.',
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
			},
			null,
			2,
		)}\n`,
		'utf8',
	)
	return agentPath
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

describe('managed subagent CLI operator surface', () => {
	it('launches a managed subagent only as an in-process launch-and-wait operation', async () => {
		const store = await createStore('dennett-stage8-subagent-cli-launch-')
		store.createRun({
			run_id: 'parent-run-launch-cli',
			resolved_revision_id: 'rev-parent-launch-cli',
			entry_node_id: 'entry',
			started_via: 'direct',
		})
		const tempDir = path.dirname(store.database_path)
		const childAgentPath = await writeLaunchChildAgent(tempDir)
		const lifecycle = new AgentLifecycleService({ state_store: store })
		await lifecycle.registerAgentFile(childAgentPath)
		await lifecycle.deployAgentFile(childAgentPath)

		const startExecution = vi
			.spyOn(CodexAppServerRuntimeAdapter.prototype, 'startExecution')
			.mockResolvedValue({
				runtime_handle: null,
				native_session_handle: null,
				terminal_result: Promise.resolve({
					outcome: 'success',
					output: {
						mode: 'text',
					},
					output_text: 'launch-complete',
				}),
				events: {
					async *[Symbol.asyncIterator]() {},
				},
			})

		const launched = await runCli([
			'subagent-launch',
			'agent.child.launch',
			'--parent-run-id',
			'parent-run-launch-cli',
			'--parent-task-id',
			'task-launch-cli',
			'--role',
			'explorer',
			'--objective',
			'Run the CLI-managed child',
			'--input-message',
			'Child input from CLI',
			'--acceptance-criterion',
			'Return a summary',
			'--write-set',
			JSON.stringify({
				mode: 'allow_list',
				items: [
					{
						resource_kind: 'file',
						resource_ref: 'src/interfaces/cli.ts',
						scope: 'exact',
						access: 'create_or_modify',
					},
				],
			}),
			'--read-context',
			JSON.stringify({
				mode: 'explicit_only',
				items: [
					{
						context_kind: 'file',
						context_ref: 'src/interfaces/cli.ts',
						inclusion: 'reference_only',
						required: false,
					},
				],
			}),
			'--required-validations',
			JSON.stringify([
				{
					validation_id: 'cli-launch-unit',
					description: 'Run the CLI launch unit test.',
					required: true,
				},
			]),
			'--interaction-policy',
			'silent',
			'--state-db',
			store.database_path,
		])

		expect(startExecution).toHaveBeenCalledTimes(1)
		const output = parseStdout(launched.stdout) as {
			wait: { state: string; outcome: string; final_payload: { summary: string } }
			record: {
				child_role: string
				state: string
				terminal_result: { outcome: string }
				task: {
					write_set: Record<string, unknown>
					read_context: Record<string, unknown>
					required_validations: Array<Record<string, unknown>>
					interaction_policy: string
				}
			}
			launch_semantics: { background_execution: boolean; waited_in_process: boolean; note: string }
		}
		expect(output).toMatchObject({
			wait: {
				state: 'terminal',
				outcome: 'accepted',
				final_payload: {
					summary: 'launch-complete',
				},
			},
			record: {
				child_role: 'explorer',
				state: 'terminal',
				terminal_result: {
					outcome: 'accepted',
				},
			},
			launch_semantics: {
				background_execution: false,
				waited_in_process: true,
			},
		})
		expect(output.record.task.write_set).toMatchObject({
			mode: 'allow_list',
			items: [
				{
					resource_kind: 'file',
					resource_ref: 'src/interfaces/cli.ts',
					scope: 'exact',
					access: 'create_or_modify',
				},
			],
		})
		expect(output.record.task.read_context).toMatchObject({
			mode: 'explicit_only',
			items: [
				{
					context_kind: 'file',
					context_ref: 'src/interfaces/cli.ts',
					inclusion: 'reference_only',
					required: false,
				},
			],
		})
		expect(output.record.task.required_validations).toEqual([
			{
				validation_id: 'cli-launch-unit',
				description: 'Run the CLI launch unit test.',
				required: true,
			},
		])
		expect(output.record.task.interaction_policy).toBe('silent')
		expect(output.launch_semantics.note).toContain('does not create a durable background worker')
	})

	it('rejects structurally invalid launch write-set JSON before child execution', async () => {
		const store = await createStore('dennett-stage8-subagent-cli-invalid-write-set-')
		store.createRun({
			run_id: 'parent-run-invalid-write-set-cli',
			resolved_revision_id: 'rev-parent-invalid-write-set-cli',
			entry_node_id: 'entry',
			started_via: 'direct',
		})

		await expect(
			runCli([
				'subagent-launch',
				'agent.child.launch',
				'--parent-run-id',
				'parent-run-invalid-write-set-cli',
				'--parent-task-id',
				'task-invalid-write-set-cli',
				'--role',
				'worker',
				'--objective',
				'Run the CLI-managed child',
				'--input-message',
				'Child input from CLI',
				'--acceptance-criterion',
				'Return a summary',
				'--write-set',
				JSON.stringify({
					mode: 'allow_list',
					items: [
						{
							resource_kind: 'file',
							resource_ref: 'src/interfaces/cli.ts',
							scope: 'recursive',
							access: 'create_or_modify',
						},
					],
				}),
				'--state-db',
				store.database_path,
			]),
		).rejects.toMatchObject({
			code: 'INVALID_SUBAGENT_REQUEST',
			message: 'Managed subagent write_set item 1 scope must be one of: exact, descendants.',
		})

		expect(
			store.listManagedSubagentRecords({
				parent_run_id: 'parent-run-invalid-write-set-cli',
			}),
		).toHaveLength(0)
	})

	it('rejects unsupported launch interaction policy before child execution', async () => {
		const store = await createStore('dennett-stage16-subagent-cli-invalid-interaction-')
		store.createRun({
			run_id: 'parent-run-invalid-interaction-cli',
			resolved_revision_id: 'rev-parent-invalid-interaction-cli',
			entry_node_id: 'entry',
			started_via: 'direct',
		})

		await expect(
			runCli([
				'subagent-launch',
				'agent.child.launch',
				'--parent-run-id',
				'parent-run-invalid-interaction-cli',
				'--parent-task-id',
				'task-invalid-interaction-cli',
				'--role',
				'worker',
				'--objective',
				'Run the CLI-managed child',
				'--input-message',
				'Child input from CLI',
				'--acceptance-criterion',
				'Return a summary',
				'--write-set',
				JSON.stringify({
					mode: 'allow_list',
					items: [
						{
							resource_kind: 'file',
							resource_ref: 'src/interfaces/cli.ts',
							scope: 'exact',
							access: 'create_or_modify',
						},
					],
				}),
				'--interaction-policy',
				'interactive',
				'--state-db',
				store.database_path,
			]),
		).rejects.toMatchObject({
			code: 'INVALID_SUBAGENT_REQUEST',
			message:
				'Managed subagent interaction_policy must be "silent" in the current Stage 16 slice.',
		})

		expect(
			store.listManagedSubagentRecords({
				parent_run_id: 'parent-run-invalid-interaction-cli',
			}),
		).toHaveLength(0)
	})

	it('lists and shows managed subagents with lineage, ownership, budgets, and honest semantics', async () => {
		const store = await createStore('dennett-stage8-subagent-cli-list-')
		seedManagedSubagents(store)
		const stateDbPath = store.database_path

		const listed = await runCli([
			'subagent-list',
			'--parent-run-id',
			'parent-run-cli',
			'--state-db',
			stateDbPath,
		])
		expect(listed).toMatchObject({
			stderr: '',
			exitCode: undefined,
		})

		const listOutput = parseStdout(listed.stdout) as Array<{
			subagent_id: string
			child_role: string
			lineage: { parent_run_id: string; parent_task_id: string }
			task: {
				budgets: Record<string, number>
				control_message_count: number
				read_context: { mode: string; items: unknown[] }
				required_validations: unknown[]
				interaction_policy: string
			}
			operator_semantics: {
				write_scope_enforcement: string
				control_messages: string
				interaction_policy: string
				cancellation: string
			}
		}>
		expect(listOutput.map((entry) => entry.subagent_id)).toEqual([
			'subagent-running-cli',
			'subagent-terminal-cli',
		])
		expect(listOutput[0]).toMatchObject({
			child_role: 'worker',
			lineage: {
				parent_run_id: 'parent-run-cli',
				parent_task_id: 'task-cli',
			},
			task: {
				budgets: {
					max_children: 2,
					max_spawn_depth: 1,
				},
				control_message_count: 0,
				read_context: {
					mode: 'explicit_only',
					items: [],
				},
				required_validations: [],
				interaction_policy: 'silent',
			},
		})
		expect(listOutput[0]?.operator_semantics.write_scope_enforcement).toContain(
			'not_filesystem_sandbox',
		)
		expect(listOutput[0]?.operator_semantics.control_messages).toContain('does not live-deliver')
		expect(listOutput[0]?.operator_semantics.interaction_policy).toContain(
			'silent_no_parent_user_boundary',
		)
		expect(listOutput[0]?.operator_semantics.cancellation).toBe('not_requested')

		const shown = await runCli([
			'subagent-show',
			'subagent-terminal-cli',
			'--state-db',
			stateDbPath,
		])
		const showOutput = parseStdout(shown.stdout) as {
			subagent_id: string
			state: string
			child_agent: { logical_agent_id: string }
			terminal_result: { outcome: string }
		}
		expect(showOutput).toMatchObject({
			subagent_id: 'subagent-terminal-cli',
			state: 'terminal',
			child_agent: {
				logical_agent_id: 'agent.child.reviewer',
			},
			terminal_result: {
				outcome: 'accepted',
			},
		})
	})

	it('returns explicit status and records explicit cancel intent without runtime cancellation claims', async () => {
		const store = await createStore('dennett-stage16-subagent-cli-status-cancel-')
		seedManagedSubagents(store)
		const stateDbPath = store.database_path

		const status = await runCli([
			'subagent-status',
			'subagent-running-cli',
			'--state-db',
			stateDbPath,
		])
		const statusOutput = parseStdout(status.stdout) as {
			record: {
				subagent_id: string
				state: string
				child_role: string
			}
			status_semantics: {
				durable_state_projection: boolean
				live_runtime_probe: boolean
				note: string
			}
		}
		expect(statusOutput).toMatchObject({
			record: {
				subagent_id: 'subagent-running-cli',
				state: 'running',
				child_role: 'worker',
			},
			status_semantics: {
				durable_state_projection: true,
				live_runtime_probe: false,
			},
		})
		expect(statusOutput.status_semantics.note).toContain('does not probe')

		await runCli([
			'subagent-record-control',
			'subagent-running-cli',
			'--kind',
			'request_status',
			'--message-id',
			'cancel-conflict-cli',
			'--payload',
			'{}',
			'--state-db',
			stateDbPath,
		])
		const rejectedCancel = await runCli([
			'subagent-cancel',
			'subagent-running-cli',
			'--message-id',
			'cancel-conflict-cli',
			'--reason',
			'operator tried to cancel with a reused status id',
			'--state-db',
			stateDbPath,
		])
		const rejectedCancelOutput = parseStdout(rejectedCancel.stdout) as {
			cancel_status: string
			state: string
			reason_code: string
			runtime_cancellation_delivered: boolean
			record: { task: { control_message_count: number } }
			cancel_semantics: {
				recorded_in_state: boolean
				duplicate_id_conflict: boolean
				runtime_cancellation_delivered: boolean
				note: string
			}
		}
		expect(rejectedCancelOutput).toMatchObject({
			cancel_status: 'rejected',
			state: 'running',
			reason_code: 'invalid_control_message',
			runtime_cancellation_delivered: false,
			record: {
				task: {
					control_message_count: 1,
				},
			},
			cancel_semantics: {
				recorded_in_state: false,
				duplicate_id_conflict: true,
				runtime_cancellation_delivered: false,
			},
		})
		expect(rejectedCancelOutput.cancel_semantics.note).toContain(
			'no cancellation intent was recorded',
		)

		const cancel = await runCli([
			'subagent-cancel',
			'subagent-running-cli',
			'--message-id',
			'explicit-cli-cancel',
			'--reason',
			'operator stopped the task',
			'--state-db',
			stateDbPath,
		])
		const cancelOutput = parseStdout(cancel.stdout) as {
			cancel_status: string
			state: string
			runtime_cancellation_delivered: boolean
			record: {
				operator_semantics: { cancellation: string }
				task: { control_message_count: number }
			}
			cancel_semantics: {
				recorded_in_state: boolean
				runtime_cancellation_delivered: boolean
				note: string
			}
		}
		expect(cancelOutput).toMatchObject({
			cancel_status: 'cancelling',
			state: 'cancelling',
			runtime_cancellation_delivered: false,
			record: {
				task: {
					control_message_count: 2,
				},
			},
			cancel_semantics: {
				recorded_in_state: true,
				runtime_cancellation_delivered: false,
			},
		})
		expect(cancelOutput.cancel_semantics.note).toContain('no runtime cancellation signal')
		expect(cancelOutput.record.operator_semantics.cancellation).toContain('not_runtime_cancel')

		const duplicateCancel = await runCli([
			'subagent-cancel',
			'subagent-running-cli',
			'--message-id',
			'explicit-cli-cancel',
			'--reason',
			'operator stopped the task',
			'--state-db',
			stateDbPath,
		])
		const duplicateCancelOutput = parseStdout(duplicateCancel.stdout) as {
			cancel_status: string
			state: string
			record: { task: { control_message_count: number } }
		}
		expect(duplicateCancelOutput).toMatchObject({
			cancel_status: 'already_cancelling',
			state: 'cancelling',
			record: {
				task: {
					control_message_count: 2,
				},
			},
		})
	})

	it('records and inspects review/fix workflow state without spawning repair work', async () => {
		const store = await createStore('dennett-stage16-subagent-cli-review-workflow-')
		seedManagedSubagents(store)
		const stateDbPath = store.database_path

		const accepted = await runCli([
			'subagent-record-review',
			'subagent-terminal-cli',
			'--decision',
			'accepted',
			'--state-db',
			stateDbPath,
		])
		const acceptedOutput = parseStdout(accepted.stdout) as {
			workflow_state: { decision: string; outcome: string; budget_exhausted: boolean }
			review_workflow_semantics: {
				automatic_repair_spawned: boolean
				review_quality_enforced: boolean
			}
		}
		expect(acceptedOutput).toMatchObject({
			workflow_state: {
				decision: 'accepted',
				outcome: 'accepted',
				budget_exhausted: false,
			},
			review_workflow_semantics: {
				automatic_repair_spawned: false,
				review_quality_enforced: false,
			},
		})

		const review = store.createManagedSubagentRecord({
			subagent_id: 'subagent-review-change-cli',
			child_run_id: 'child-run-review-change-cli',
			child_role: 'reviewer',
			child_logical_agent_id: 'agent.child.reviewer',
			child_resolved_revision_id: 'rev-child-reviewer',
			lineage: {
				root_run_id: 'parent-run-cli',
				parent_run_id: 'parent-run-cli',
				parent_task_id: 'task-review-fix-cli',
				depth: 1,
			},
			task_package: {
				agent_ref: 'agent.child.reviewer',
				objective: 'Review a repairable CLI workflow change',
				input_message: 'Reviewer input',
				acceptance_criteria: ['Return findings when needed'],
				prohibitions: [],
				write_set: {
					mode: 'allow_list',
					items: [
						{
							resource_kind: 'file',
							resource_ref: 'tests/unit/subagent-cli-review.ts',
							scope: 'exact',
							access: 'create_or_modify',
						},
					],
				},
				budgets: {
					max_review_loops: 2,
				},
			},
			created_at: '2026-04-28T08:00:04.000Z',
			updated_at: '2026-04-28T08:00:04.000Z',
		})
		store.markManagedSubagentTerminal({
			subagent_id: review.subagent_id,
			terminal_at: '2026-04-28T08:00:05.000Z',
			terminal_result: {
				outcome: 'review_required',
				child_run_status: 'completed',
				final_output: {
					summary: 'changes requested',
					findings: [
						{
							finding_id: 'finding-cli-1',
							severity: 'high',
							category: 'correctness',
							summary: 'Repair is required.',
							evidence_refs: ['tests/unit/subagent-cli-review.ts'],
							recommended_action: 'fix',
						},
					],
				},
				final_output_mode: 'json',
				final_payload: {
					summary: 'changes requested',
				},
				findings: [
					{
						finding_id: 'finding-cli-1',
						severity: 'high',
						category: 'correctness',
						summary: 'Repair is required.',
						evidence_refs: ['tests/unit/subagent-cli-review.ts'],
						recommended_action: 'fix',
					},
				],
				reason_code: 'review_findings_raised',
			},
		})
		const repair = store.createManagedSubagentRecord({
			subagent_id: 'subagent-repair-cli',
			child_run_id: 'child-run-repair-cli',
			child_role: 'worker',
			child_logical_agent_id: 'agent.child.worker',
			child_resolved_revision_id: 'rev-child-worker',
			lineage: {
				root_run_id: 'parent-run-cli',
				parent_run_id: 'parent-run-cli',
				parent_task_id: 'task-review-fix-cli',
				depth: 1,
			},
			task_package: {
				agent_ref: 'agent.child.worker',
				objective: 'Repair the review finding',
				input_message: 'Repair input',
				acceptance_criteria: ['Address finding-cli-1'],
				prohibitions: [],
				write_set: {
					mode: 'allow_list',
					items: [
						{
							resource_kind: 'file',
							resource_ref: 'tests/unit/subagent-cli-repair.ts',
							scope: 'exact',
							access: 'create_or_modify',
						},
					],
				},
			},
			created_at: '2026-04-28T08:00:06.000Z',
			updated_at: '2026-04-28T08:00:06.000Z',
		})

		const changes = await runCli([
			'subagent-record-review',
			review.subagent_id,
			'--decision',
			'changes_requested',
			'--state-db',
			stateDbPath,
		])
		const changesOutput = parseStdout(changes.stdout) as {
			workflow_state: {
				decision: string
				finding_ids: string[]
				outcome: string
				budget_exhausted: boolean
			}
		}
		expect(changesOutput).toMatchObject({
			workflow_state: {
				decision: 'changes_requested',
				finding_ids: ['finding-cli-1'],
				outcome: 'changes_requested',
				budget_exhausted: false,
			},
		})

		const linked = await runCli([
			'subagent-link-repair',
			review.subagent_id,
			'--repair-subagent-id',
			repair.subagent_id,
			'--state-db',
			stateDbPath,
		])
		const linkedOutput = parseStdout(linked.stdout) as {
			workflow_state: { repair_subagent_id: string; outcome: string }
			repair_workflow_semantics: {
				automatic_repair_spawned: boolean
				automatic_rereview_spawned: boolean
			}
			repair_record: { workflow_state: { reviewer_subagent_id: string } }
		}
		expect(linkedOutput).toMatchObject({
			workflow_state: {
				repair_subagent_id: repair.subagent_id,
				outcome: 'repair_linked',
			},
			repair_workflow_semantics: {
				automatic_repair_spawned: false,
				automatic_rereview_spawned: false,
			},
			repair_record: {
				workflow_state: {
					reviewer_subagent_id: review.subagent_id,
				},
			},
		})

		const shown = await runCli([
			'subagent-show',
			review.subagent_id,
			'--state-db',
			stateDbPath,
		])
		const shownOutput = parseStdout(shown.stdout) as {
			workflow_state: { outcome: string; repair_subagent_id: string }
			operator_semantics: { review_fix_workflow: string }
		}
		expect(shownOutput.workflow_state).toMatchObject({
			outcome: 'repair_linked',
			repair_subagent_id: repair.subagent_id,
		})
		expect(shownOutput.operator_semantics.review_fix_workflow).toContain(
			'durable_projection_only',
		)

		const finalReview = store.createManagedSubagentRecord({
			subagent_id: 'subagent-final-review-cli',
			child_run_id: 'child-run-final-review-cli',
			child_role: 'final_review',
			child_logical_agent_id: 'agent.child.reviewer',
			child_resolved_revision_id: 'rev-child-reviewer',
			lineage: {
				root_run_id: 'parent-run-cli',
				parent_run_id: 'parent-run-cli',
				parent_task_id: 'task-review-fix-cli',
				depth: 1,
			},
			task_package: {
				agent_ref: 'agent.child.reviewer',
				objective: 'Final review after repair',
				input_message: 'Final review input',
				acceptance_criteria: ['Accept after repair is sufficient'],
				prohibitions: [],
				write_set: {
					mode: 'allow_list',
					items: [
						{
							resource_kind: 'file',
							resource_ref: 'tests/unit/subagent-cli-final-review.ts',
							scope: 'exact',
							access: 'create_or_modify',
						},
					],
				},
				budgets: {
					max_review_loops: 2,
				},
			},
			created_at: '2026-04-28T08:00:07.000Z',
			updated_at: '2026-04-28T08:00:07.000Z',
		})
		store.markManagedSubagentTerminal({
			subagent_id: finalReview.subagent_id,
			terminal_at: '2026-04-28T08:00:08.000Z',
			terminal_result: {
				outcome: 'accepted',
				child_run_status: 'completed',
				final_output: {
					summary: 'final review accepted',
				},
				final_output_mode: 'json',
				final_payload: {
					summary: 'final review accepted',
				},
				findings: null,
				reason_code: null,
			},
		})

		const finalAccepted = await runCli([
			'subagent-record-review',
			finalReview.subagent_id,
			'--decision',
			'accepted',
			'--state-db',
			stateDbPath,
		])
		const finalAcceptedOutput = parseStdout(finalAccepted.stdout) as {
			workflow_state: {
				loop_index: number
				max_review_loops: number
				decision: string
				outcome: string
				budget_exhausted: boolean
			}
			record: {
				child_role: string
				workflow_state: {
					outcome: string
				}
			}
		}
		expect(finalAcceptedOutput).toMatchObject({
			workflow_state: {
				loop_index: 2,
				max_review_loops: 2,
				decision: 'accepted',
				outcome: 'accepted',
				budget_exhausted: false,
			},
			record: {
				child_role: 'final_review',
				workflow_state: {
					outcome: 'accepted',
				},
			},
		})
	})

	it('waits on terminal state and records control/cancel requests without claiming live delivery', async () => {
		const store = await createStore('dennett-stage8-subagent-cli-control-')
		seedManagedSubagents(store)
		const stateDbPath = store.database_path

		const waited = await runCli([
			'subagent-wait',
			'subagent-terminal-cli',
			'--wait-mode',
			'terminal_only',
			'--timeout-ms',
			'5',
			'--state-db',
			stateDbPath,
		])
		const waitOutput = parseStdout(waited.stdout) as {
			state: string
			outcome: string
			wait_semantics: {
				durable_reconciliation: boolean
				live_execution_wait: boolean
				timeout_ms_requested: number | null
				timeout_ms_applied: boolean
				note: string
			}
		}
		expect(waitOutput).toMatchObject({
			state: 'terminal',
			outcome: 'accepted',
			wait_semantics: {
				durable_reconciliation: true,
				live_execution_wait: false,
				timeout_ms_requested: 5,
				timeout_ms_applied: false,
			},
		})
		expect(waitOutput.wait_semantics.note).toContain(
			'does not attach to a live in-process subagent',
		)

		const statusControl = await runCli([
			'subagent-record-control',
			'subagent-running-cli',
			'--kind',
			'request_status',
			'--message-id',
			'control-status-cli',
			'--payload',
			'{}',
			'--state-db',
			stateDbPath,
		])
		const statusOutput = parseStdout(statusControl.stdout) as {
			delivery_state: string
			delivery_semantics: { recorded_in_state: boolean; live_delivery: boolean }
			record: { task: { control_message_count: number } }
		}
		expect(statusOutput).toMatchObject({
			delivery_state: 'accepted',
			delivery_semantics: {
				recorded_in_state: true,
				live_delivery: false,
			},
			record: {
				task: {
					control_message_count: 1,
				},
			},
		})

		const duplicateStatusControl = await runCli([
			'subagent-record-control',
			'subagent-running-cli',
			'--kind',
			'request_status',
			'--message-id',
			'control-status-cli',
			'--payload',
			'{}',
			'--state-db',
			stateDbPath,
		])
		const duplicateStatusOutput = parseStdout(duplicateStatusControl.stdout) as {
			delivery_state: string
			delivery_semantics: { recorded_in_state: boolean; idempotent_replay: boolean }
			record: { task: { control_message_count: number } }
		}
		expect(duplicateStatusOutput).toMatchObject({
			delivery_state: 'accepted',
			delivery_semantics: {
				recorded_in_state: false,
				idempotent_replay: true,
			},
			record: {
				task: {
					control_message_count: 1,
				},
			},
		})

		const conflictingDuplicateStatusControl = await runCli([
			'subagent-record-control',
			'subagent-running-cli',
			'--kind',
			'clarify_scope',
			'--message-id',
			'control-status-cli',
			'--payload',
			'{"summary":"Different control intent with a reused id"}',
			'--state-db',
			stateDbPath,
		])
		const conflictingDuplicateStatusOutput = parseStdout(
			conflictingDuplicateStatusControl.stdout,
		) as {
			delivery_state: string
			reason_code: string
			delivery_semantics: {
				recorded_in_state: boolean
				duplicate_id_conflict: boolean
				note: string
			}
			record: { task: { control_message_count: number } }
		}
		expect(conflictingDuplicateStatusOutput).toMatchObject({
			delivery_state: 'rejected',
			reason_code: 'invalid_control_message',
			delivery_semantics: {
				recorded_in_state: false,
				duplicate_id_conflict: true,
			},
			record: {
				task: {
					control_message_count: 1,
				},
			},
		})
		expect(conflictingDuplicateStatusOutput.delivery_semantics.note).toContain(
			'already exists with different kind or payload',
		)

		const cancelControl = await runCli([
			'subagent-record-control',
			'subagent-running-cli',
			'--kind',
			'cancel',
			'--message-id',
			'control-cancel-cli',
			'--payload',
			'{"reason":"operator stopped the task"}',
			'--state-db',
			stateDbPath,
		])
		const cancelOutput = parseStdout(cancelControl.stdout) as {
			state: string
			delivery_semantics: { runtime_cancellation_delivered: boolean; note: string }
			record: { operator_semantics: { cancellation: string } }
		}
		expect(cancelOutput).toMatchObject({
			state: 'cancelling',
			delivery_semantics: {
				runtime_cancellation_delivered: false,
			},
		})
		expect(cancelOutput.delivery_semantics.note).toContain('no runtime cancellation signal')
		expect(cancelOutput.record.operator_semantics.cancellation).toContain('not_runtime_cancel')

		const close = await runCli([
			'subagent-close',
			'subagent-running-cli',
			'--disposition',
			'cancelled_by_parent',
			'--state-db',
			stateDbPath,
		])
		const closeOutput = parseStdout(close.stdout) as {
			close_status: string
			state: string
			close_semantics: { runtime_cancellation_delivered: boolean; note: string }
		}
		expect(closeOutput).toMatchObject({
			close_status: 'closing',
			state: 'cancelling',
			close_semantics: {
				runtime_cancellation_delivered: false,
			},
		})
		expect(closeOutput.close_semantics.note).toContain('does not claim runtime cancellation')
	})
})
