import { readFile } from 'node:fs/promises'
import path from 'node:path'
import type { ValidateFunction } from 'ajv'
import { describe, expect, it } from 'vitest'
import { createAjv2020Validator } from '../../src/core/output-schema-validator.js'

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

function validAgentFile(): Record<string, unknown> {
	return {
		graph_contract_version: '1.0',
		meta: {
			id: 'agent.builder.schema',
			name: 'Builder Output Schema Agent',
		},
		entry_node_id: 'start',
		nodes: [
			{
				id: 'start',
				kind: 'runtime_agent',
				runtime_adapter: 'codex',
				prompt: 'Return a concise answer.',
				input: {
					parts: [
						{
							type: 'text',
							text: 'Hello.',
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

function richPublicContractAgentFile(): Record<string, unknown> {
	return {
		graph_contract_version: '1.0',
		meta: {
			id: 'agent.builder.rich-schema',
			name: 'Builder Rich Schema Agent',
			description: 'Representative Builder 2.0 draft using public portable surfaces.',
		},
		entry_node_id: 'plan',
		params: {
			task: {
				type: 'string',
				required: true,
				description: 'Task to plan and review.',
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
						summary: 'Portable project memory intent for retrieval and summaries.',
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
				description: 'Portable runtime source identity, not account metadata.',
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
				prompt: 'Plan the work using only public portable contract surfaces.',
				input: {
					parts: [
						{
							type: 'ref',
							ref: 'params.task',
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
				agent_ref: 'agent.portable-reviewer',
				input: {
					parts: [
						{
							type: 'ref',
							ref: 'node.plan.json.summary',
						},
					],
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

describe('builder output wrapper schema', () => {
	it('accepts the formal builder output wrapper containing an agent_file', async () => {
		const validate = await loadBuilderOutputValidator()

		expect(
			validate({
				agent_file: validAgentFile(),
			}),
		).toBe(true)
	})

	it('accepts representative Builder 2.0 public-contract surfaces in the formal wrapper', async () => {
		const validate = await loadBuilderOutputValidator()

		expect(
			validate({
				agent_file: richPublicContractAgentFile(),
			}),
		).toBe(true)
	})

	it.each([
		{
			name: 'missing agent_file wrapper field',
			payload: {
				file: validAgentFile(),
			},
			keyword: 'required',
		},
		{
			name: 'additional wrapper property',
			payload: {
				agent_file: validAgentFile(),
				diagnostics: [],
			},
			keyword: 'additionalProperties',
		},
		{
			name: 'invalid embedded agent file',
			payload: {
				agent_file: {
					...validAgentFile(),
					nodes: [],
				},
			},
			keyword: 'minItems',
		},
	])('rejects $name', async ({ payload, keyword }) => {
		const validate = await loadBuilderOutputValidator()

		expect(validate(payload)).toBe(false)
		expect(validate.errors).toEqual(
			expect.arrayContaining([
				expect.objectContaining({
					keyword,
				}),
			]),
		)
	})
})
