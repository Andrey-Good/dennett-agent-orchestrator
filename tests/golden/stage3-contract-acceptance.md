# Stage 3 Contract Acceptance Cases

These cases define the assertions future automated tests should implement directly.

## Case 1: Minimal Portable Agent File Is Accepted

- Fixture: `tests/fixtures/agents/valid/minimal-runtime-agent.json`
- Assert that the file passes closed-root validation.
- Assert that the required root fields are present.
- Assert that the single `runtime_agent` node is structurally valid.

## Case 2: Comprehensive Portable Agent File Is Accepted

- Fixture: `tests/fixtures/agents/valid/complete-agent.json`
- Assert that all documented top-level containers can coexist in one file.
- Assert that bindings remain unique within their own collections.
- Assert that `runtime_source_policy = prefer_first` accepts a non-empty ordered source list.
- Assert that node-level permissions and memory allowlists can narrow the top-level declarations.

## Case 2A: Constrained Parameters Are Accepted

- Fixture: `tests/fixtures/agents/valid/params-constrained-values.json`
- Assert that `allowed_values` accepts a non-empty declared set of portable variants.
- Assert that simple `constraints` are accepted only in the supported small subset.
- Assert that the fixture documents a model-like parameter as declaration-only, without implying automatic runtime switching behavior.

## Case 3: Root Closure Rejects Unknown Top-Level Fields

- Fixture: `tests/fixtures/agents/invalid/root-extra-field.json`
- Assert that the validator rejects the file because the root object is closed.

## Case 4: Node Identity And Entry Resolution Are Enforced

- Fixtures: `tests/fixtures/agents/invalid/duplicate-node-ids.json`, `tests/fixtures/agents/invalid/entry-node-missing.json`
- Assert that duplicate node IDs are rejected.
- Assert that `entry_node_id` must resolve to one node in `nodes`.

## Case 5: Node-Kind Rules Are Enforced

- Fixture: `tests/fixtures/agents/invalid/orchestrator-agent-missing-agent-ref.json`
- Assert that `orchestrator_agent` cannot omit `agent_ref`.

## Case 6: Source Narrowing Rules Are Enforced

- Fixture: `tests/fixtures/agents/invalid/runtime-source-policy-inherit-with-list.json`
- Assert that `inherit` cannot be combined with `runtime_source_ids`.

## Case 7: Illegal Reference Namespaces Are Rejected

- Fixture: `tests/fixtures/agents/invalid/illegal-input-ref-namespace.json`
- Assert that only the documented reference namespaces are accepted.

## Case 8: Missing Runtime-Node Requirements Are Rejected

- Fixture: `tests/fixtures/agents/invalid/runtime-agent-missing-prompt.json`
- Assert that `runtime_agent` requires `prompt`.

## Case 9: Wrong-Kind Fields Are Rejected

- Fixture: `tests/fixtures/agents/invalid/orchestrator-agent-forbidden-runtime-adapter.json`
- Assert that `orchestrator_agent` cannot carry `runtime_adapter`.

## Case 10: Edge Targets Must Resolve

- Fixture: `tests/fixtures/agents/invalid/edges-point-to-unknown-node.json`
- Assert that unknown edge targets are rejected.

## Case 11: Secret Marker Equality Is Rejected

- Fixture: `tests/fixtures/agents/invalid/secret-markers-equal.json`
- Assert that secret markers must be distinct.

## Case 12: Memory Scope Must Be Valid

- Fixture: `tests/fixtures/agents/invalid/memory-binding-invalid-scope.json`
- Assert that memory binding scope is limited to `agent` or `node`.

## Case 13: Parameter Defaults Must Match Declared Types

- Fixture: `tests/fixtures/agents/invalid/params-default-type-mismatch.json`
- Assert that a parameter default must satisfy the declared `type`.

## Case 13A: Parameter Defaults Must Respect Allowed Values

- Fixture: `tests/fixtures/agents/invalid/params-default-not-in-allowed-values.json`
- Assert that `default` must appear in `allowed_values` when `allowed_values` is present.

## Case 13B: Parameter Constraint Ranges Must Be Coherent

- Fixture: `tests/fixtures/agents/invalid/params-constraints-invalid-range.json`
- Assert that a lower bound in `constraints` cannot exceed the corresponding upper bound.

## Case 13C: Parameter Defaults Must Respect Constraints

- Fixture: `tests/fixtures/agents/invalid/params-number-default-outside-constraints.json`
- Assert that `default` must satisfy declared portable constraints.

## Case 13D: Parameter Allowed Values Must Be Unique

- Fixture: `tests/fixtures/agents/invalid/params-allowed-values-duplicate.json`
- Assert that `allowed_values` rejects duplicate values even when the schema accepts the array shape.

## Case 13E: Parameter Allowed Values Must Respect Constraints

- Fixture: `tests/fixtures/agents/invalid/params-allowed-values-outside-constraints.json`
- Assert that every declared `allowed_values` item must satisfy declared portable constraints.

## Case 14: Frozen Skills Need Inline Text

- Fixture: `tests/fixtures/agents/invalid/skill-frozen-without-inline-text.json`
- Assert that `frozen = true` requires `inline_text`.

## Case 15: Runtime Source Adapter Must Match

- Fixture: `tests/fixtures/agents/invalid/runtime-source-adapter-mismatch.json`
- Assert that every declared runtime source must match the node adapter that references it.
