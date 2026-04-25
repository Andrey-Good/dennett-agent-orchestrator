# Runtime Adapter Invariants

Owner doc: [docs/03-contracts/runtime-adapter-contract.md](../../docs/03-contracts/runtime-adapter-contract.md)

| ID | Invariant | Notes |
| --- | --- | --- |
| RA-01 | The adapter boundary applies only to `runtime_agent` nodes. | `orchestrator_agent` execution stays in orchestrator core. |
| RA-02 | Capability flags are explicit booleans and govern rejection. | Unsupported behavior must fail instead of degrading silently. |
| RA-03 | The normalized request is already resolved. | The adapter must not re-read the agent file to discover missing data. |
| RA-04 | `input_message` contains no unresolved `ref` tokens. | Reference lookup happens before adapter invocation. |
| RA-05 | `runtime_source`, when present, is authoritative. | The adapter must not reopen source selection or fall back silently. |
| RA-06 | `inspectRuntimeSource` is only valid when introspection is supported. | The result is a closed normalized status object. |
| RA-07 | Event payloads are normalized to `comment` and `user_chat_request`. | Vendor telemetry must not cross the boundary unnormalized. |
| RA-08 | Exactly one terminal result closes each execution. | `error.details` is the only opaque exception in the result object. |
| RA-09 | `invalid_output` must stay distinct from `runtime_error`. | Boundary validation does not rewrite outcome semantics. |
| RA-10 | Live comments and built-in user-chat responses are separate ingress paths. | Each is rejected explicitly when its preconditions are not met. |
| RA-11 | The Codex path in this repository stays behind an App Server-backed adapter boundary. | A product interface must not satisfy the adapter contract through one-shot vendor CLI orchestration; launching a long-lived App Server process behind the adapter boundary is allowed. |
