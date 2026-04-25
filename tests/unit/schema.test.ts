import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { describe, expect, it } from 'vitest'
import { loadAndValidateAgentFile } from '../../src/core/schema.js'

const TEXT_OUTPUT = { mode: 'text' } as const
const JSON_OBJECT_OUTPUT = {
	mode: 'json',
	schema: {
		type: 'object',
		additionalProperties: true,
	},
} as const

async function withTempJsonFile<T>(
	contents: unknown,
	callback: (filePath: string) => Promise<T>,
): Promise<T> {
	const tempDir = await mkdtemp(path.join(os.tmpdir(), 'dennett-schema-loader-'))
	const filePath = path.join(tempDir, 'agent.json')

	try {
		await writeFile(filePath, JSON.stringify(contents, null, 2), 'utf8')
		return await callback(filePath)
	} finally {
		await rm(tempDir, { recursive: true, force: true })
	}
}

describe('loadAndValidateAgentFile', () => {
	it('loads the complete portable contract fixture without applying slice gating', async () => {
		const fixturePath = path.resolve(
			process.cwd(),
			'tests',
			'fixtures',
			'agents',
			'valid',
			'complete-agent.json',
		)

		const agentFile = await loadAndValidateAgentFile(fixturePath)

		expect(agentFile.meta.id).toBe('stage3-complete-agent')
		expect(agentFile.permissions).toMatchObject({
			profile: 'workspace-write',
		})
		expect(agentFile.skills).toEqual(
			expect.arrayContaining([
				expect.objectContaining({
					id: 'triage_skill',
					codex_ref: 'skills/triage.md',
				}),
				expect.objectContaining({
					id: 'handoff_style',
					frozen: true,
				}),
			]),
		)
		expect(agentFile.runtime_sources).toEqual(
			expect.arrayContaining([
				expect.objectContaining({
					id: 'primary_codex',
					runtime_adapter: 'codex',
				}),
			]),
		)
		expect(agentFile.nodes).toEqual(
			expect.arrayContaining([
				expect.objectContaining({
					id: 'triage',
					kind: 'runtime_agent',
					runtime_options: {
						model: 'gpt-5.3-codex',
						temperature: 0.1,
					},
				}),
				expect.objectContaining({
					id: 'handoff',
					kind: 'orchestrator_agent',
				}),
			]),
		)
	})

	it('accepts skill bindings with only codex_ref when frozen is absent', async () => {
		const agentFile = {
			graph_contract_version: '1.0',
			meta: {
				id: 'codex-ref-only-skill-agent',
				name: 'Codex Ref Only Skill Agent',
			},
			entry_node_id: 'start',
			skills: [
				{
					id: 'skill-a',
					codex_ref: 'skills/foo.md',
				},
			],
			nodes: [
				{
					id: 'start',
					kind: 'runtime_agent',
					runtime_adapter: 'codex',
					prompt: 'Respond briefly.',
					input: {
						parts: [{ type: 'text', text: 'Hello' }],
					},
					output: TEXT_OUTPUT,
				},
			],
		}

		await withTempJsonFile(agentFile, async (filePath) => {
			await expect(loadAndValidateAgentFile(filePath)).resolves.toMatchObject({
				skills: [
					{
						id: 'skill-a',
						codex_ref: 'skills/foo.md',
					},
				],
			})
		})
	})

	it('still rejects frozen skill bindings that omit inline_text', async () => {
		const fixturePath = path.resolve(
			process.cwd(),
			'tests',
			'fixtures',
			'agents',
			'invalid',
			'skill-frozen-without-inline-text.json',
		)

		await expect(loadAndValidateAgentFile(fixturePath)).rejects.toThrow(
			/Agent file schema validation failed:/,
		)
	})

	it('accepts constrained params with allowed values', async () => {
		const fixturePath = path.resolve(
			process.cwd(),
			'tests',
			'fixtures',
			'agents',
			'valid',
			'params-constrained-values.json',
		)

		const agentFile = await loadAndValidateAgentFile(fixturePath)

		expect(agentFile.params?.model_profile).toMatchObject({
			type: 'string',
			default: 'gpt-5.3-codex',
			allowed_values: ['gpt-5.3-codex', 'gpt-5.4-mini-codex'],
			constraints: {
				min_length: 3,
				max_length: 32,
				pattern: '^[a-z0-9.-]+$',
			},
		})
		expect(agentFile.params?.review_count).toMatchObject({
			type: 'number',
			default: 2,
			allowed_values: [1, 2, 3],
			constraints: {
				minimum: 1,
				maximum: 3,
			},
		})
	})

	it.each([
		'params-default-not-in-allowed-values.json',
		'params-constraints-invalid-range.json',
		'params-number-default-outside-constraints.json',
		'params-allowed-values-duplicate.json',
		'params-allowed-values-outside-constraints.json',
	])('rejects invalid constrained param fixture %s', async (fixtureName) => {
		const fixturePath = path.resolve(
			process.cwd(),
			'tests',
			'fixtures',
			'agents',
			'invalid',
			fixtureName,
		)

		await expect(loadAndValidateAgentFile(fixturePath)).rejects.toThrow()
	})

	it('rejects runtime nodes that reference unknown memory bindings', async () => {
		const agentFile = {
			graph_contract_version: '1.0',
			meta: {
				id: 'unknown-memory-binding',
				name: 'Unknown Memory Binding',
			},
			entry_node_id: 'start',
			memory_bindings: [
				{
					id: 'known-memory',
					kind: 'runtime_memory',
					codex_ref: 'memory://known',
					config: {
						intent: {
							summary: 'Known memory binding.',
						},
						required_capabilities: ['read'],
					},
					scope: 'agent',
				},
			],
			nodes: [
				{
					id: 'start',
					kind: 'runtime_agent',
					runtime_adapter: 'codex',
					prompt: 'Respond briefly.',
					input: {
						parts: [{ type: 'text', text: 'Hello' }],
					},
					memory_ids: ['missing-memory'],
					output: TEXT_OUTPUT,
				},
			],
		}

		await withTempJsonFile(agentFile, async (filePath) => {
			await expect(loadAndValidateAgentFile(filePath)).rejects.toThrow(
				'Node "start" references unknown memory_binding "missing-memory".',
			)
		})
	})

	it('rejects runtime memory bindings that omit config', async () => {
		const agentFile = {
			graph_contract_version: '1.0',
			meta: {
				id: 'memory-binding-missing-config',
				name: 'Memory Binding Missing Config',
			},
			entry_node_id: 'start',
			memory_bindings: [
				{
					id: 'memory-a',
					kind: 'runtime_memory',
					codex_ref: 'memory://missing-config',
					scope: 'agent',
				},
			],
			nodes: [
				{
					id: 'start',
					kind: 'runtime_agent',
					runtime_adapter: 'codex',
					prompt: 'Respond briefly.',
					input: {
						parts: [{ type: 'text', text: 'Hello' }],
					},
					output: TEXT_OUTPUT,
				},
			],
		}

		await withTempJsonFile(agentFile, async (filePath) => {
			await expect(loadAndValidateAgentFile(filePath)).rejects.toThrow(
				/Agent file schema validation failed:/,
			)
		})
	})

	it('rejects runtime memory bindings whose provider_extension.config is not an object', async () => {
		const agentFile = {
			graph_contract_version: '1.0',
			meta: {
				id: 'memory-binding-invalid-provider-extension-config',
				name: 'Memory Binding Invalid Provider Extension Config',
			},
			entry_node_id: 'start',
			memory_bindings: [
				{
					id: 'memory-a',
					kind: 'runtime_memory',
					codex_ref: 'memory://invalid-provider-extension-config',
					scope: 'agent',
					config: {
						intent: {
							summary: 'Invalid provider extension config binding.',
						},
						required_capabilities: ['read'],
						provider_extension: {
							provider: 'mem0',
							config: 'not-an-object',
						},
					},
				},
			],
			nodes: [
				{
					id: 'start',
					kind: 'runtime_agent',
					runtime_adapter: 'codex',
					prompt: 'Respond briefly.',
					input: {
						parts: [{ type: 'text', text: 'Hello' }],
					},
					output: TEXT_OUTPUT,
				},
			],
		}

		await withTempJsonFile(agentFile, async (filePath) => {
			await expect(loadAndValidateAgentFile(filePath)).rejects.toThrow(
				/Agent file schema validation failed:/,
			)
		})
	})

	it('rejects Mem0 provider_extension.config values that try to override local registration fields', async () => {
		const agentFile = {
			graph_contract_version: '1.0',
			meta: {
				id: 'memory-binding-forbidden-mem0-provider-override',
				name: 'Memory Binding Forbidden Mem0 Provider Override',
			},
			entry_node_id: 'start',
			memory_bindings: [
				{
					id: 'memory-a',
					kind: 'runtime_memory',
					codex_ref: 'memory://forbidden-provider-override',
					scope: 'agent',
					config: {
						intent: {
							summary: 'Binding that incorrectly tries to override local Mem0 registration fields.',
						},
						required_capabilities: ['read'],
						provider_extension: {
							provider: 'mem0',
							config: {
								python_executable: 'C:/forbidden/python.exe',
							},
						},
					},
				},
			],
			nodes: [
				{
					id: 'start',
					kind: 'runtime_agent',
					runtime_adapter: 'codex',
					prompt: 'Respond briefly.',
					input: {
						parts: [{ type: 'text', text: 'Hello' }],
					},
					output: TEXT_OUTPUT,
				},
			],
		}

		await withTempJsonFile(agentFile, async (filePath) => {
			await expect(loadAndValidateAgentFile(filePath)).rejects.toThrow(
				/Agent file schema validation failed:/,
			)
		})
	})

	it('accepts Mem0 provider_extension.config when it stays inside the documented mem0_config subtree', async () => {
		const agentFile = {
			graph_contract_version: '1.0',
			meta: {
				id: 'memory-binding-valid-mem0-provider-extension',
				name: 'Memory Binding Valid Mem0 Provider Extension',
			},
			entry_node_id: 'start',
			memory_bindings: [
				{
					id: 'memory-a',
					kind: 'runtime_memory',
					codex_ref: 'memory://valid-provider-extension',
					scope: 'agent',
					config: {
						intent: {
							summary: 'Valid Mem0 provider extension config binding.',
						},
						required_capabilities: ['read'],
						provider_extension: {
							provider: 'mem0',
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
			nodes: [
				{
					id: 'start',
					kind: 'runtime_agent',
					runtime_adapter: 'codex',
					prompt: 'Respond briefly.',
					input: {
						parts: [{ type: 'text', text: 'Hello' }],
					},
					output: TEXT_OUTPUT,
				},
			],
		}

		await withTempJsonFile(agentFile, async (filePath) => {
			await expect(loadAndValidateAgentFile(filePath)).resolves.toMatchObject({
				memory_bindings: [
					{
						id: 'memory-a',
						config: {
							provider_extension: {
								provider: 'mem0',
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
			})
		})
	})

	it('rejects Mem0 provider_extension.config when graph_store omits provider', async () => {
		const agentFile = {
			graph_contract_version: '1.0',
			meta: {
				id: 'memory-binding-invalid-mem0-graph-store-missing-provider',
				name: 'Memory Binding Invalid Mem0 Graph Store Missing Provider',
			},
			entry_node_id: 'start',
			memory_bindings: [
				{
					id: 'memory-a',
					kind: 'runtime_memory',
					codex_ref: 'memory://invalid-graph-store-missing-provider',
					scope: 'agent',
					config: {
						intent: {
							summary: 'Binding that incorrectly omits graph_store.provider.',
						},
						required_capabilities: ['read', 'graph_context'],
						provider_extension: {
							provider: 'mem0',
							config: {
								mem0_config: {
									graph_store: {},
								},
							},
						},
					},
				},
			],
			nodes: [
				{
					id: 'start',
					kind: 'runtime_agent',
					runtime_adapter: 'codex',
					prompt: 'Respond briefly.',
					input: {
						parts: [{ type: 'text', text: 'Hello' }],
					},
					output: TEXT_OUTPUT,
				},
			],
		}

		await withTempJsonFile(agentFile, async (filePath) => {
			await expect(loadAndValidateAgentFile(filePath)).rejects.toThrow(
				/Agent file schema validation failed:/,
			)
		})
	})

	it('rejects Mem0 provider_extension.config when it tries to override nested llm credentials', async () => {
		const agentFile = {
			graph_contract_version: '1.0',
			meta: {
				id: 'memory-binding-forbidden-mem0-llm-override',
				name: 'Memory Binding Forbidden Mem0 LLM Override',
			},
			entry_node_id: 'start',
			memory_bindings: [
				{
					id: 'memory-a',
					kind: 'runtime_memory',
					codex_ref: 'memory://forbidden-llm-override',
					scope: 'agent',
					config: {
						intent: {
							summary: 'Binding that incorrectly tries to override Mem0 llm credentials.',
						},
						required_capabilities: ['read'],
						provider_extension: {
							provider: 'mem0',
							config: {
								mem0_config: {
									llm: {
										config: {
											api_key: 'forbidden-key',
										},
									},
								},
							},
						},
					},
				},
			],
			nodes: [
				{
					id: 'start',
					kind: 'runtime_agent',
					runtime_adapter: 'codex',
					prompt: 'Respond briefly.',
					input: {
						parts: [{ type: 'text', text: 'Hello' }],
					},
					output: TEXT_OUTPUT,
				},
			],
		}

		await withTempJsonFile(agentFile, async (filePath) => {
			await expect(loadAndValidateAgentFile(filePath)).rejects.toThrow(
				/Agent file schema validation failed:/,
			)
		})
	})

	it('rejects Mem0 provider_extension.config when it tries to override nested embedder credentials', async () => {
		const agentFile = {
			graph_contract_version: '1.0',
			meta: {
				id: 'memory-binding-forbidden-mem0-embedder-override',
				name: 'Memory Binding Forbidden Mem0 Embedder Override',
			},
			entry_node_id: 'start',
			memory_bindings: [
				{
					id: 'memory-a',
					kind: 'runtime_memory',
					codex_ref: 'memory://forbidden-embedder-override',
					scope: 'agent',
					config: {
						intent: {
							summary: 'Binding that incorrectly tries to override Mem0 embedder credentials.',
						},
						required_capabilities: ['read'],
						provider_extension: {
							provider: 'mem0',
							config: {
								mem0_config: {
									embedder: {
										config: {
											api_key: 'forbidden-key',
										},
									},
								},
							},
						},
					},
				},
			],
			nodes: [
				{
					id: 'start',
					kind: 'runtime_agent',
					runtime_adapter: 'codex',
					prompt: 'Respond briefly.',
					input: {
						parts: [{ type: 'text', text: 'Hello' }],
					},
					output: TEXT_OUTPUT,
				},
			],
		}

		await withTempJsonFile(agentFile, async (filePath) => {
			await expect(loadAndValidateAgentFile(filePath)).rejects.toThrow(
				/Agent file schema validation failed:/,
			)
		})
	})

	it('rejects Mem0 provider_extension.config when it tries to override nested vector-store credentials', async () => {
		const agentFile = {
			graph_contract_version: '1.0',
			meta: {
				id: 'memory-binding-forbidden-mem0-vector-store-override',
				name: 'Memory Binding Forbidden Mem0 Vector Store Override',
			},
			entry_node_id: 'start',
			memory_bindings: [
				{
					id: 'memory-a',
					kind: 'runtime_memory',
					codex_ref: 'memory://forbidden-vector-store-override',
					scope: 'agent',
					config: {
						intent: {
							summary: 'Binding that incorrectly tries to override Mem0 vector-store credentials.',
						},
						required_capabilities: ['read'],
						provider_extension: {
							provider: 'mem0',
							config: {
								mem0_config: {
									vector_store: {
										config: {
											api_key: 'forbidden-key',
										},
									},
								},
							},
						},
					},
				},
			],
			nodes: [
				{
					id: 'start',
					kind: 'runtime_agent',
					runtime_adapter: 'codex',
					prompt: 'Respond briefly.',
					input: {
						parts: [{ type: 'text', text: 'Hello' }],
					},
					output: TEXT_OUTPUT,
				},
			],
		}

		await withTempJsonFile(agentFile, async (filePath) => {
			await expect(loadAndValidateAgentFile(filePath)).rejects.toThrow(
				/Agent file schema validation failed:/,
			)
		})
	})

	it('rejects Mem0 provider_extension.config when graph_store.config contains nested keys such as api_key', async () => {
		const agentFile = {
			graph_contract_version: '1.0',
			meta: {
				id: 'memory-binding-forbidden-mem0-graph-store-config-override',
				name: 'Memory Binding Forbidden Mem0 Graph Store Config Override',
			},
			entry_node_id: 'start',
			memory_bindings: [
				{
					id: 'memory-a',
					kind: 'runtime_memory',
					codex_ref: 'memory://forbidden-graph-store-config-override',
					scope: 'agent',
					config: {
						intent: {
							summary: 'Binding that incorrectly tries to override graph_store nested config.',
						},
						required_capabilities: ['read'],
						provider_extension: {
							provider: 'mem0',
							config: {
								mem0_config: {
									graph_store: {
										provider: 'networkx',
										config: {
											api_key: 'forbidden-key',
										},
									},
								},
							},
						},
					},
				},
			],
			nodes: [
				{
					id: 'start',
					kind: 'runtime_agent',
					runtime_adapter: 'codex',
					prompt: 'Respond briefly.',
					input: {
						parts: [{ type: 'text', text: 'Hello' }],
					},
					output: TEXT_OUTPUT,
				},
			],
		}

		await withTempJsonFile(agentFile, async (filePath) => {
			await expect(loadAndValidateAgentFile(filePath)).rejects.toThrow(
				/Agent file schema validation failed:/,
			)
		})
	})

	it('rejects json outputs that omit a schema', async () => {
		const agentFile = {
			graph_contract_version: '1.0',
			meta: {
				id: 'json-output-missing-schema',
				name: 'JSON Output Missing Schema',
			},
			entry_node_id: 'start',
			nodes: [
				{
					id: 'start',
					kind: 'runtime_agent',
					runtime_adapter: 'codex',
					prompt: 'Respond with JSON.',
					input: {
						parts: [{ type: 'text', text: 'Hello' }],
					},
					output: {
						mode: 'json',
					},
				},
			],
		}

		await withTempJsonFile(agentFile, async (filePath) => {
			await expect(loadAndValidateAgentFile(filePath)).rejects.toThrow(
				/Agent file schema validation failed:/,
			)
		})
	})

	it('rejects text outputs that declare a schema', async () => {
		const agentFile = {
			graph_contract_version: '1.0',
			meta: {
				id: 'text-output-with-schema',
				name: 'Text Output With Schema',
			},
			entry_node_id: 'start',
			nodes: [
				{
					id: 'start',
					kind: 'runtime_agent',
					runtime_adapter: 'codex',
					prompt: 'Respond with text.',
					input: {
						parts: [{ type: 'text', text: 'Hello' }],
					},
					output: {
						...TEXT_OUTPUT,
						schema: JSON_OBJECT_OUTPUT.schema,
					},
				},
			],
		}

		await withTempJsonFile(agentFile, async (filePath) => {
			await expect(loadAndValidateAgentFile(filePath)).rejects.toThrow(
				/Agent file schema validation failed:/,
			)
		})
	})
})
