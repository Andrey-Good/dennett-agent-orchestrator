# Nodes And Edges Invariants

Owner doc: [docs/03-contracts/agent-json/nodes-and-edges-contract.md](../../docs/03-contracts/agent-json/nodes-and-edges-contract.md)

| ID | Invariant | Notes |
| --- | --- | --- |
| NE-01 | `nodes` is required and must contain at least one node. | The portable file has no meaningful graph without a node list. |
| NE-02 | Every node is closed except for `runtime_options`. | Unknown fields are not permitted on node objects. |
| NE-03 | `id` is unique within `nodes`. | `entry_node_id` must resolve to one of the node IDs. |
| NE-04 | Node `kind` is either `runtime_agent` or `orchestrator_agent`. | Each kind has a distinct allowed-field set. |
| NE-05 | `runtime_agent` requires `runtime_adapter`, `prompt`, `input`, and `output`. | `agent_ref` is forbidden on this kind. |
| NE-06 | `orchestrator_agent` requires `agent_ref`, `input`, and `output`. | Runtime-adapter fields are forbidden on this kind. |
| NE-07 | Input parts preserve order and use only `text` or `ref`. | Reference namespaces are limited to the documented set. |
| NE-08 | `output.mode = json` requires a JSON object result and an object-shaped JSON Schema. | Arrays, scalars, `null`, and non-object schemas are invalid for that mode. |
| NE-09 | `edges` are control-flow only. | They never transport payload data. |
| NE-10 | Edge conditions fail the run if evaluation fails. | Failure is not silently treated as `false`. |
| NE-11 | Node-level binding lists must not contain duplicates. | Unknown IDs are invalid at resolution time. |
| NE-12 | `runtime_source_policy` governs `runtime_source_ids`. | `inherit` forbids an explicit list; the other policies require one. |
