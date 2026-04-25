import { readFile } from 'node:fs/promises'
import path from 'node:path'
import type { ValidateFunction } from 'ajv'
import { describe, expect, it } from 'vitest'
import { CodexAppServerRuntimeAdapter } from '../../src/adapters/codex/codex-app-server-runtime-adapter.js'
import { createAjv2020Validator } from '../../src/core/output-schema-validator.js'

async function loadRuntimeAdapterCapabilitiesValidator(): Promise<ValidateFunction<unknown>> {
	const schemaPath = path.resolve(
		process.cwd(),
		'contracts',
		'json-schema',
		'runtime-adapter-capabilities.schema.json',
	)
	const schema = await readFile(schemaPath, 'utf8').then((contents) => JSON.parse(contents))

	return createAjv2020Validator().compile(schema)
}

function representativeCapabilities(): Record<string, boolean> {
	return {
		supports_native_resume: false,
		supports_live_comments: false,
		supports_builtin_user_chat_mcp: false,
		supports_memory_bindings: true,
		supports_model_discovery: true,
		supports_runtime_environment_introspection: true,
		supports_reasoning_effort: true,
		supports_speed_tiers: true,
		supports_personality: true,
		supports_explicit_runtime_source: false,
		supports_runtime_source_introspection: false,
	}
}

describe('runtime-adapter-capabilities schema', () => {
	it('accepts the actual Codex adapter capabilities', async () => {
		const validate = await loadRuntimeAdapterCapabilitiesValidator()
		const adapter = new CodexAppServerRuntimeAdapter('C:/Dev/dennett-agent-orchestrator')

		expect(validate(adapter.describeCapabilities())).toBe(true)
	})

	it('accepts representative current capabilities including memory bindings', async () => {
		const validate = await loadRuntimeAdapterCapabilitiesValidator()

		expect(validate(representativeCapabilities())).toBe(true)
	})

	it('rejects unknown capability fields while remaining closed', async () => {
		const validate = await loadRuntimeAdapterCapabilitiesValidator()
		const capabilities = {
			...representativeCapabilities(),
			supports_unlisted_native_feature: true,
		}

		expect(validate(capabilities)).toBe(false)
		expect(validate.errors).toEqual(
			expect.arrayContaining([
				expect.objectContaining({
					keyword: 'additionalProperties',
					instancePath: '',
				}),
			]),
		)
	})
})
