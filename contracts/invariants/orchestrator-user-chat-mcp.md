# Built-In User Chat MCP Invariants

Owner doc: [docs/03-contracts/orchestrator-user-chat-mcp-contract.md](../../docs/03-contracts/orchestrator-user-chat-mcp-contract.md)

| ID | Invariant | Notes |
| --- | --- | --- |
| MCP-01 | The canonical server name is `orchestrator.user_chat`. | The agent-file enablement must reference the same server name. |
| MCP-02 | Request payloads are closed objects. | `kind`, `text`, and `require_response` are required. |
| MCP-03 | `kind = text` and `kind = options` have disjoint payload requirements. | `options` is required only for `kind = options`. |
| MCP-04 | Option IDs are unique within one request. | `value` is the payload returned on selection. |
| MCP-05 | Responses must match the originating prompt. | Ambiguous or mismatched responses are invalid. |
| MCP-06 | `kind = option` is only valid for option-based prompts. | `option_id` must come from the original request. |
| MCP-07 | Free-form user text is not an implicit MCP response. | Explicit prompt binding is required. |
| MCP-08 | Unknown fields are invalid in request or response payloads. | The contract stays closed. |
