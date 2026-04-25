# Stage 3 Interaction Acceptance Cases

These cases define the acceptance behavior for comment routing and the built-in user-chat MCP.

## Case 1: Comments And Built-In MCP Can Coexist

- Fixture: `tests/fixtures/agents/valid/complete-agent.json`
- Assert that `interaction.comments.enabled = true` can target a `runtime_agent` node.
- Assert that `interaction.user_mcp.enabled = true` can name `orchestrator.user_chat`.
- Assert that both channels can be enabled in one portable file without conflict.

## Case 2: Comment Targets Must Be Runtime Agents

- Fixture: `tests/fixtures/agents/invalid/comments-targets-orchestrator-node.json`
- Assert that a non-`runtime_agent` target is rejected.

## Case 3: Built-In MCP Server Name Is Canonical

- Fixture: `tests/fixtures/agents/invalid/user-mcp-wrong-server-name.json`
- Assert that any explicit `server_name` must equal `orchestrator.user_chat`.

## Case 4: Free-Form Text Is Not Auto-Bound To MCP

- This is a routing assertion, not a file-shape assertion.
- When the built-in MCP has a pending prompt, free-form user text remains a comment unless the interface explicitly binds it to that prompt.

## Case 5: Nested Runs Do Not Surface Child Live Interaction Upward

- This is a runtime routing assertion.
- A child `orchestrator_agent` run must not leak its comments or built-in MCP traffic into the parent run.
