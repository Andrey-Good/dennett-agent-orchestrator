# Stage 17 Builder Contract Proof And Failure Modes

Status: accepted for bounded deterministic local proof and user-facing failure-mode reference.

## What Is Proven

The Stage 17 proof is deterministic and local. It does not call a live model, memory provider, App Server, or managed-subagent runtime.

Stage 17 is accepted for this bounded deterministic Builder 2.0 scope only. Acceptance covers the public-contract rich draft proof, builder repair pass, failure docs, schema/audit gates, and draft-only persistence.

The proof covers representative builder-authored draft candidates that use public Agent JSON contract surfaces:

- runtime controls through `runtime_options` and runtime-source references;
- memory intent through portable `memory_bindings`, including a Mem0 `provider_extension` limited to the portable `mem0_config` subtree;
- user interaction through `interaction.comments`, `interaction.user_mcp`, and `chat` settings;
- portable nested work through `orchestrator_agent` nodes, not managed-subagent task packages.

The formal wrapper proof lives in `tests/unit/builder-output-schema.test.ts`. The service-level persistence and audit proof lives in `tests/unit/builder-service.test.ts`.

## Deterministic Proof Cases

The schema test accepts a representative Builder 2.0 wrapper with the exact shape:

```json
{
  "agent_file": {
    "graph_contract_version": "1.0"
  }
}
```

The embedded agent file includes runtime options, runtime sources, memory bindings, interaction/chat settings, and a portable `orchestrator_agent` reviewer node. Acceptance means only that the wrapper and embedded portable Agent JSON are contract-shaped.

The builder service tests prove the stronger draft flow:

- the builder context instructs authoring through public contract surfaces only;
- an accepted richer candidate is persisted as a draft revision only;
- the live revision is not changed by builder creation;
- hidden managed-subagent task packages, local provider data, runtime account/rate-limit data, provider secrets, invalid runtime options, unsupported runtime surfaces, and invalid JSON output schemas are rejected before persistence;
- one repair attempt receives structured previous-failure diagnostics;
- if the repair attempt is still invalid, no candidate is persisted.

## User-Facing Failure Modes

Builder failures are reported as host diagnostics or errors. They are not inserted into portable Agent JSON.

### Wrapper Failure

Cause: the builder did not return the exact wrapper shape, returned no `agent_file`, returned a non-object `agent_file`, or added extra top-level wrapper keys such as `diagnostics`.

User action: ask the builder to return only `{"agent_file": <agent-json>}`. Do not copy diagnostics into the candidate file.

Persistence result: no draft is saved unless the bounded repair attempt returns a valid wrapper.

### Schema Failure

Cause: the embedded agent file is not valid Agent JSON, such as missing `graph_contract_version`, no nodes, invalid node output shape, or invalid top-level fields.

User action: fix the candidate to match the public Agent JSON schema before trying again.

Persistence result: no draft is saved unless the bounded repair attempt returns a schema-valid candidate.

### Identity Failure

Cause: the candidate `meta.id` does not match the requested target agent id, or an update/create request violates lifecycle identity rules.

User action: keep `meta.id` exactly equal to the target agent id and use the explicit revise/update flow when changing an existing agent.

Persistence result: no draft is saved.

### Audit Failure

Cause: the candidate is schema-valid but violates Builder 2.0 public-contract boundaries. Examples include unsupported runtime options, unsupported runtime capability usage, local provider registration data, provider secrets, runtime account details, rate limits, hidden managed-subagent task packages, or invalid JSON output schemas.

User action: remove non-portable data and express only portable intent. Configure providers, runtime accounts, rate limits, and managed-subagent task packages through their owner surfaces instead of inside Agent JSON.

Persistence result: no draft is saved.

### Repair Failure

Cause: the first builder response failed validation or wrapper extraction, then the single repair attempt also failed.

User action: inspect the previous-failure gate and code, then submit a corrected request or manually edit the candidate against the owner contract.

Persistence result: no draft is saved from either attempt.

## Deferred Claims

This proof does not claim:

- live builder generation quality;
- live execution of builder-authored drafts;
- real provider read/write/search;
- native App Server memory;
- broad runtime compatibility;
- full user-interaction product flow;
- managed-subagent MCP authoring, review loops, budgets, or write-scope enforcement inside portable Agent JSON;
- public Builder 2.0 readiness.

Those claims remain owned by later integrated product-flow and real-world proof stages.
