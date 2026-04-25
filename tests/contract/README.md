# Stage 3 Contract Test Support

This directory is the machine-oriented entrypoint for Stage 3 contract tests.
The actual test runner does not exist yet; these files define the intended coverage shape.

## Coverage Map

| Area | Source of truth | Support artifacts |
| --- | --- | --- |
| Portable agent root | [contracts/invariants/agent-json-top-level-and-bindings.md](../../contracts/invariants/agent-json-top-level-and-bindings.md) | `tests/fixtures/agents/valid/*.json`, `tests/fixtures/agents/invalid/*.json` |
| Nodes and edges | [contracts/invariants/agent-json-nodes-and-edges.md](../../contracts/invariants/agent-json-nodes-and-edges.md) | `tests/fixtures/agents/valid/*.json`, `tests/fixtures/agents/invalid/*.json` |
| Interaction and chat | [contracts/invariants/agent-json-interaction-and-chat.md](../../contracts/invariants/agent-json-interaction-and-chat.md) | `tests/fixtures/agents/valid/*.json`, `tests/golden/stage3-interaction-acceptance.md` |
| Built-in MCP payload | [contracts/invariants/orchestrator-user-chat-mcp.md](../../contracts/invariants/orchestrator-user-chat-mcp.md) | `tests/golden/stage3-interaction-acceptance.md` |
| Runtime adapter boundary | [contracts/invariants/runtime-adapter.md](../../contracts/invariants/runtime-adapter.md) | `tests/golden/stage3-contract-acceptance.md` |

## Fixture Intent

- Valid fixtures are contract-shaped agent files that should pass structural validation.
- Invalid fixtures are contract-shaped agent files with one targeted violation each.
- Golden cases describe the observable assertions future automated tests should enforce.

## Fixture Index

### Valid

- `tests/fixtures/agents/valid/minimal-runtime-agent.json`
- `tests/fixtures/agents/valid/complete-agent.json`
- `tests/fixtures/agents/valid/params-constrained-values.json`

### Invalid

- `tests/fixtures/agents/invalid/root-extra-field.json`
- `tests/fixtures/agents/invalid/duplicate-node-ids.json`
- `tests/fixtures/agents/invalid/entry-node-missing.json`
- `tests/fixtures/agents/invalid/runtime-source-policy-inherit-with-list.json`
- `tests/fixtures/agents/invalid/comments-targets-orchestrator-node.json`
- `tests/fixtures/agents/invalid/user-mcp-wrong-server-name.json`
- `tests/fixtures/agents/invalid/orchestrator-agent-missing-agent-ref.json`
- `tests/fixtures/agents/invalid/illegal-input-ref-namespace.json`
- `tests/fixtures/agents/invalid/runtime-agent-missing-prompt.json`
- `tests/fixtures/agents/invalid/orchestrator-agent-forbidden-runtime-adapter.json`
- `tests/fixtures/agents/invalid/edges-point-to-unknown-node.json`
- `tests/fixtures/agents/invalid/secret-markers-equal.json`
- `tests/fixtures/agents/invalid/memory-binding-invalid-scope.json`
- `tests/fixtures/agents/invalid/params-default-type-mismatch.json`
- `tests/fixtures/agents/invalid/params-default-not-in-allowed-values.json`
- `tests/fixtures/agents/invalid/params-constraints-invalid-range.json`
- `tests/fixtures/agents/invalid/params-number-default-outside-constraints.json`
- `tests/fixtures/agents/invalid/params-allowed-values-duplicate.json`
- `tests/fixtures/agents/invalid/params-allowed-values-outside-constraints.json`
- `tests/fixtures/agents/invalid/skill-frozen-without-inline-text.json`
- `tests/fixtures/agents/invalid/runtime-source-adapter-mismatch.json`
