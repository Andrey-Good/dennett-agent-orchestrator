# Valid Agent Fixtures

These files are expected to satisfy the current Stage 3 portable agent-file contract.

| File | Coverage |
| --- | --- |
| `minimal-runtime-agent.json` | Smallest useful portable file: required root fields plus one `runtime_agent` node. |
| `complete-agent.json` | Wider coverage of bindings, interaction policy, memory, runtime sources, edges, and mixed node kinds. |
| `params-constrained-values.json` | Constrained params with portable `allowed_values`, `constraints`, and a model-like declaration-only warning. |
| `phase5-codex-minimal.json` | Phase 5 smoke fixture: a tiny single-node runtime agent pinned to `gpt-5.3-codex` for the App Server-native Codex path. |
