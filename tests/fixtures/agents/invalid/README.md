# Invalid Agent Fixtures

Each file here is intentionally invalid for one named contract reason.

| File | Targeted violation |
| --- | --- |
| `root-extra-field.json` | Root closure violation. |
| `duplicate-node-ids.json` | Duplicate node IDs. |
| `entry-node-missing.json` | `entry_node_id` does not resolve. |
| `runtime-source-policy-inherit-with-list.json` | `inherit` combined with `runtime_source_ids`. |
| `comments-targets-orchestrator-node.json` | Comment target is not a `runtime_agent` node. |
| `user-mcp-wrong-server-name.json` | Non-canonical built-in MCP server name. |
| `orchestrator-agent-missing-agent-ref.json` | Missing required `agent_ref` on `orchestrator_agent`. |
| `illegal-input-ref-namespace.json` | Illegal input reference namespace. |
| `runtime-agent-missing-prompt.json` | Missing required `prompt` on `runtime_agent`. |
| `orchestrator-agent-forbidden-runtime-adapter.json` | Forbidden runtime-adapter field on `orchestrator_agent`. |
| `edges-point-to-unknown-node.json` | Edge target does not resolve. |
| `secret-markers-equal.json` | Secret markers are equal. |
| `memory-binding-invalid-scope.json` | Invalid memory binding scope. |
| `params-default-type-mismatch.json` | Parameter default does not match its declared type. |
| `params-default-not-in-allowed-values.json` | Parameter default is missing from declared `allowed_values`. |
| `params-constraints-invalid-range.json` | Parameter constraint lower bound exceeds upper bound. |
| `params-number-default-outside-constraints.json` | Parameter default violates declared constraints. |
| `params-allowed-values-duplicate.json` | Parameter `allowed_values` repeats the same value. |
| `params-allowed-values-outside-constraints.json` | Parameter `allowed_values` violates declared constraints. |
| `skill-frozen-without-inline-text.json` | Frozen skill is missing `inline_text`. |
| `runtime-source-adapter-mismatch.json` | Runtime source adapter does not match the node adapter. |
