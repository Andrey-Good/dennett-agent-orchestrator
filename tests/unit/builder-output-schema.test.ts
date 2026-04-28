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

describe('builder output wrapper schema', () => {
	it('accepts the formal builder output wrapper containing an agent_file', async () => {
		const validate = await loadBuilderOutputValidator()

		expect(
			validate({
				agent_file: validAgentFile(),
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
