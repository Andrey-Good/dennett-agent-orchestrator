# Interaction And Chat Invariants

Owner doc: [docs/03-contracts/agent-json/interaction-and-chat-contract.md](../../docs/03-contracts/agent-json/interaction-and-chat-contract.md)

| ID | Invariant | Notes |
| --- | --- | --- |
| IC-01 | `interaction` is closed and optional. | If absent, both interaction channels default to disabled. |
| IC-02 | `comments.enabled = true` requires non-empty `target_node_ids`. | Targets must resolve to existing `runtime_agent` nodes. |
| IC-03 | Comment delivery is only for the active node and only when targeted. | Unsupported adapters must reject delivery explicitly. |
| IC-04 | `user_mcp.server_name` defaults to `orchestrator.user_chat`. | Any explicit value must match that server name. |
| IC-05 | `user_mcp.enabled = true` requires adapter support. | A run is invalid if the adapter cannot expose the built-in MCP. |
| IC-06 | `chat` is closed and optional. | Defaults favor native resume, visible-message storage, and fresh-start allowance. |
| IC-07 | `secret_markers` are optional and closed. | Open and close markers must be non-empty and distinct. |
| IC-08 | Comments and built-in MCP are distinct channels. | Free-form user text must not be auto-bound to MCP responses. |
| IC-09 | Nested `orchestrator_agent` runs do not surface child live interaction upward. | Parent and child routing remain separated. |
