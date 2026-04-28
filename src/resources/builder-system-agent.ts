import type { AgentFile } from '../core/agent-file.js'
import { validateAgentFileValue } from '../core/schema.js'

export const BUILDER_SYSTEM_AGENT_ID = 'system.builder.phase17'

export const BUILDER_SYSTEM_AGENT_RESOURCE: AgentFile = {
	graph_contract_version: '1.0',
	meta: {
		id: BUILDER_SYSTEM_AGENT_ID,
		name: 'Phase 17 Builder System Agent',
		description: 'Generates richer portable agent JSON candidates for draft-first persistence.',
		agent_version: '1.0.0',
	},
	entry_node_id: 'builder',
	params: {
		context: {
			type: 'object',
			required: true,
			description: 'Structured builder request context supplied by the orchestrator.',
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
			prompt: [
				'You are the built-in builder system agent for Dennett Agent Orchestrator.',
				'Produce a complete portable agent definition that can be saved as a draft revision.',
				'Return JSON only, following the formal builder output wrapper contract with the exact top-level shape {"agent_file": <portable-agent-json>}.',
				'The embedded agent_file must be a complete portable contract document with graph_contract_version "1.0".',
				'meta.id must exactly match the requested target agent id.',
				'Preserve or improve the provided agent when operation is update, and keep the result self-consistent.',
				'Use only public portable contract fields. Do not invent hidden builder-only fields or private runtime data.',
				'You may author richer public surfaces when requested: params, initial_vars, skills, mcps, plugins, permissions, memory_bindings, runtime_sources, runtime_options, interaction, chat, and orchestrator_agent nodes.',
				'Memory bindings must describe intent, required_capabilities, transport preferences, and optional provider_extension only through portable contract fields.',
				'Do not copy local provider registrations, credentials, python paths, account details, rate limits, runtime catalogs, config requirements, or managed-subagent task-package internals into the agent file.',
				'For Mem0 provider_extension.config, only use the documented portable mem0_config subtree and never local provider registration fields.',
				'Runtime options may request public controls such as model, reasoning_effort, speed_tier, or personality only when the request calls for them; speed_tier must be fast or flex; keep options as portable hints, not account metadata.',
				'Use runtime_sources only as portable source identity references and node selection policies, not as embedded local auth or account configuration.',
				'Interaction must use interaction.comments and interaction.user_mcp with server_name "orchestrator.user_chat" when user chat is needed.',
				'Managed subagent patterns must be represented with public orchestrator_agent nodes and clear prompts, not hidden create/send/wait/status/close task packages inside the portable file.',
				'Prefer the simplest graph that satisfies the request; a single runtime_agent node is acceptable when enough.',
				'Use runtime_adapter "codex" for runtime_agent nodes.',
				'Do not emit explanations, markdown, or any keys outside the required JSON wrapper.',
				'The input message is a JSON object describing the target, request, and any existing agent file context.',
			].join('\n'),
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
					properties: {
						agent_file: {
							type: 'object',
							additionalProperties: true,
						},
					},
					required: ['agent_file'],
					additionalProperties: false,
				},
			},
		},
	],
}

export async function loadBuilderSystemAgentResource(): Promise<AgentFile> {
	return await validateAgentFileValue(BUILDER_SYSTEM_AGENT_RESOURCE)
}
