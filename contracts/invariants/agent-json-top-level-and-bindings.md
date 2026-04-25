# Top-Level And Bindings Invariants

Owner doc: [docs/03-contracts/agent-json/top-level-and-bindings-contract.md](../../docs/03-contracts/agent-json/top-level-and-bindings-contract.md)

| ID | Invariant | Notes |
| --- | --- | --- |
| TL-01 | The root object is closed. | Only the documented top-level fields are legal in the current contract version. |
| TL-02 | `graph_contract_version` is required and explicit. | It is independent from `meta.agent_version`. |
| TL-03 | `meta` is closed and must include `id` and `name`. | `description` and `agent_version` remain optional. |
| TL-04 | `params` is an open map of closed descriptors. | `default` and every `allowed_values` item must match the declared `type` when present. |
| TL-04A | `params.<name>.allowed_values` is a non-empty, value-unique declared set. | Deep-value uniqueness is enforced in the invariant layer rather than portable JSON Schema. |
| TL-04B | `params.<name>.constraints` is limited to the documented portable subset for the declared type. | Lower bounds must not exceed upper bounds. |
| TL-04C | Declared constrained parameter values must satisfy their own constraints. | Applies to both `default` and every `allowed_values` item. |
| TL-05 | `initial_vars` is an open map of JSON values. | Values are not schema-restricted beyond JSON validity. |
| TL-06 | Binding collections are unique by `id` within each collection. | Uniqueness is collection-local, not global. |
| TL-07 | `skills` require either `codex_ref` or `inline_text`. | `frozen = true` requires `inline_text`. |
| TL-08 | `mcps`, `plugins`, `memory_bindings`, and `runtime_sources` are identity wrappers. | Their `config` fields remain pass-through. |
| TL-09 | `final_output.mode` is optional and defaults to `last_node_output`. | `none` is valid and disables automatic final-user output. |
| TL-10 | `memory_bindings.scope` is limited to `agent` or `node`. | Node-level allowlists refine the scope. |
