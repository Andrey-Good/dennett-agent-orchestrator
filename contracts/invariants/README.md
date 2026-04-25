# Stage 3 Invariants

This directory is the machine-oriented index for Stage 3 contract invariants.
Each leaf document points back to the normative owner doc instead of restating the contract.

## Layout

| Surface | Owner doc | Purpose |
| --- | --- | --- |
| Portable agent root and bindings | [agent-json-top-level-and-bindings.md](./agent-json-top-level-and-bindings.md) | Root closure, metadata, parameter binding, and top-level binding rules. |
| Nodes and edges | [agent-json-nodes-and-edges.md](./agent-json-nodes-and-edges.md) | Node kind constraints, input references, output modes, and edge traversal rules. |
| Interaction and chat | [agent-json-interaction-and-chat.md](./agent-json-interaction-and-chat.md) | Comment routing, built-in user-chat enablement, and chat policy defaults. |
| Built-in MCP payload | [orchestrator-user-chat-mcp.md](./orchestrator-user-chat-mcp.md) | Request/response payload invariants for `orchestrator.user_chat`. |
| Runtime adapter boundary | [runtime-adapter.md](./runtime-adapter.md) | Normalized adapter request, capability, event, and terminal-result rules. |

## Usage

- Keep these tables aligned to the owner docs in `docs/03-contracts/`.
- Use the fixture directories under `tests/fixtures/agents/` for concrete valid and invalid file shapes.
- Use the golden cases under `tests/golden/` for behavior that future automated tests should assert.
