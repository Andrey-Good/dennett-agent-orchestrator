import { readFile } from 'node:fs/promises'
import path from 'node:path'
import type { ValidateFunction } from 'ajv'
import { describe, expect, it } from 'vitest'
import { createAjv2020Validator } from '../../src/core/output-schema-validator.js'

async function loadRuntimeAdapterRequestValidator(): Promise<ValidateFunction<unknown>> {
	const schemaDir = path.resolve(process.cwd(), 'contracts', 'json-schema')
	const requestSchemaPath = path.join(schemaDir, 'runtime-adapter-request.schema.json')
	const defsSchemaPath = path.join(schemaDir, 'agent-json.defs.schema.json')

	const [requestSchema, defsSchema] = await Promise.all([
		readFile(requestSchemaPath, 'utf8').then((contents) => JSON.parse(contents)),
		readFile(defsSchemaPath, 'utf8').then((contents) => JSON.parse(contents)),
	])

	const ajv = createAjv2020Validator()
	ajv.addSchema(defsSchema)
	return ajv.compile(requestSchema)
}

function baseRuntimeAdapterRequest(): Record<string, unknown> {
	return {
		node_id: 'node-a',
		runtime_adapter: 'codex',
		prompt: 'Answer briefly.',
		input_message: 'What should I remember?',
		output: {
			mode: 'text',
		},
		effective_bindings: {
			skills: [],
			mcps: [],
			plugins: [],
		},
		permissions: {},
		runtime_options: {},
		interaction: {
			comments_enabled: false,
		},
		resume: {
			mode: 'fresh',
		},
	}
}

describe('runtime-adapter-request schema memory_context', () => {
	it('accepts existing runtime adapter requests without memory_context', async () => {
		const validate = await loadRuntimeAdapterRequestValidator()

		expect(validate(baseRuntimeAdapterRequest())).toBe(true)
	})

	it('accepts provider-neutral memory_context with read query and normalized records', async () => {
		const validate = await loadRuntimeAdapterRequestValidator()
		const request = {
			...baseRuntimeAdapterRequest(),
			memory_context: {
				bindings: [
					{
						binding_id: 'project-memory',
						codex_ref: 'memory://project',
						intent: {
							summary: 'Project-local memory for user preferences.',
							labels: ['project'],
						},
						required_capabilities: ['read', 'write'],
						scope: {
							agent_id: 'agent-a',
							run_id: 'run-a',
							user_id: 'user-a',
						},
						read: {
							query: 'What should I remember?',
							records: [
								{
									id: 'memory-record-a',
									content: 'Use concise answers.',
									scope: {
										agent_id: 'agent-a',
										user_id: 'user-a',
									},
									metadata: {
										source: 'memory-port',
									},
									score: 0.92,
									provider_data: {
										safe_rank: 1,
									},
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
		}

		expect(validate(request)).toBe(true)
	})

	it('rejects memory_context with provider configuration or other undeclared secret fields', async () => {
		const validate = await loadRuntimeAdapterRequestValidator()
		const request = {
			...baseRuntimeAdapterRequest(),
			memory_context: {
				bindings: [
					{
						binding_id: 'project-memory',
						codex_ref: 'memory://project',
						intent: {
							summary: 'Project-local memory.',
						},
						required_capabilities: ['read'],
						scope: {
							agent_id: 'agent-a',
							run_id: 'run-a',
						},
						provider_config: {
							api_key: 'forbidden',
						},
						read: {
							query: 'What should I remember?',
							records: [],
						},
						write: {
							enabled: false,
							disabled_reason: 'Binding is read-only.',
						},
					},
				],
			},
		}

		expect(validate(request)).toBe(false)
		expect(validate.errors).toEqual(
			expect.arrayContaining([
				expect.objectContaining({
					keyword: 'additionalProperties',
					instancePath: '/memory_context/bindings/0',
				}),
			]),
		)
	})

	it('rejects read contexts that omit the retrieval query', async () => {
		const validate = await loadRuntimeAdapterRequestValidator()
		const request = {
			...baseRuntimeAdapterRequest(),
			memory_context: {
				bindings: [
					{
						binding_id: 'project-memory',
						codex_ref: 'memory://project',
						intent: {
							summary: 'Project-local memory.',
						},
						required_capabilities: ['read'],
						scope: {
							agent_id: 'agent-a',
							run_id: 'run-a',
						},
						read: {
							records: [],
						},
						write: {
							enabled: false,
							disabled_reason: 'Binding is read-only.',
						},
					},
				],
			},
		}

		expect(validate(request)).toBe(false)
		expect(validate.errors).toEqual(
			expect.arrayContaining([
				expect.objectContaining({
					keyword: 'required',
					instancePath: '/memory_context/bindings/0/read',
				}),
			]),
		)
	})
})
