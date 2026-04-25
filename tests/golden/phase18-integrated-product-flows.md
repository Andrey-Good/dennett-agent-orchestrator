# Phase 18 Integrated Product Flows Golden Acceptance

This artifact defines the golden acceptance expectations for Phase 18.
Phase 18 proves that already-defined product surfaces work together in local,
offline, deterministic flows. It does not claim external provider readiness,
live-runtime behavior, hosted reliability, or real-world proof; those belong to
Phase 19 and must be captured in the Phase 19 [evidence log](../../docs/20-real-world-proof-and-release/evidence-log.md) before a [release decision record](../../docs/20-real-world-proof-and-release/release-decision-record.md) can support release-readiness claims.

## Acceptance Boundary

| Boundary | Phase 18 Local/Offline Evidence | Phase 19 Real-World Proof |
| --- | --- | --- |
| Runtime access | Deterministic fake or local adapter evidence only. | Authenticated real runtime calls with captured operational evidence. |
| Memory access | Local fake or in-process provider evidence only. | Real provider registration, read, write, and search against user-owned external memory. |
| User interaction | Durable state transitions and CLI-visible behavior without live external chat. | Live user interaction against runtimes and interfaces that support it. |
| Subagents | Portable child-run and managed orchestration semantics proven with local state. | Real multi-agent work under provider, budget, cancellation, and lineage constraints. |
| Builder output | Generated artifacts validated against public contracts and local fixtures. | Builder-created agents exercised against real runtimes and providers. |

## Golden Scenarios

### Scenario 1: Builder Output Enters Lifecycle And Runs Locally

- Product surfaces: builder, registry, draft/live/deploy lifecycle, graph execution, runtime adapter.
- Local/offline evidence:
  - Builder output is represented as a portable agent artifact that validates against the public contract.
  - The artifact can be registered as a draft, promoted through the local lifecycle, and selected by immutable version identity.
  - A deterministic local runtime adapter can execute the deployed graph and produce the expected final output.
- Executable Stage 7 mapping:
  - `tests/integration/phase18-integrated-product-flows.test.ts` -> `builds, deploys, prompts, resumes, and delegates across local subsystem seams`
  - `tests/integration/stage7-cli-integrated-flow.test.ts` -> `builds, registers, deploys, waits for user input, replies, and resumes offline`
- Phase 19 deferral:
  - No claim is made that the same builder output has run against an authenticated external runtime.
  - No claim is made about marketplace, multi-account, or hosted deployment behavior.

### Scenario 2: Interaction And Resume Preserve Durable State

- Product surfaces: graph execution, user interaction, storage/chat/resume, CLI.
- Local/offline evidence:
  - A run can enter a blocked prompt or wait state through the documented interaction surface.
  - The prompt, reply, wait state, and resume decision are persisted in local state.
  - Resuming after a local reply continues from the expected node without losing prior outputs or chat state.
- Executable Stage 7 mapping:
  - `tests/integration/phase18-integrated-product-flows.test.ts` -> `builds, deploys, prompts, resumes, and delegates across local subsystem seams`
  - `tests/integration/stage7-cli-integrated-flow.test.ts` -> `builds, registers, deploys, waits for user input, replies, and resumes offline`
  - `tests/integration/stage7-interaction-edge-cases.test.ts` -> `rejects late replies after the prompt run has completed`
  - `tests/integration/stage7-interaction-edge-cases.test.ts` -> `records duplicate prompt replies append-only until resume consumes the latest match`
  - `tests/integration/stage7-interaction-edge-cases.test.ts` -> `uses the newest matching reply when a pending prompt is superseded before resume`
- Phase 19 deferral:
  - No claim is made about live chat inside an external runtime session.
  - No claim is made about risky mid-run model changes in a real provider conversation.

### Scenario 3: Memory Bindings Affect Execution Without Polluting Agent Files

- Product surfaces: memory bindings, graph execution, runtime adapter, storage.
- Local/offline evidence:
  - An agent declares portable memory requirements without embedding provider credentials or provider-specific records.
  - A local memory provider is registered outside the agent file and negotiated through the internal memory layer.
  - Read, write, and search results can affect a deterministic local run in a traceable way.
- Executable Stage 7 mapping:
  - `tests/integration/phase18-integrated-product-flows.test.ts` -> `builds, deploys, prompts, resumes, and delegates across local subsystem seams`
  - `tests/integration/phase18-integrated-product-flows.test.ts` -> `fails an unregistered runtime memory provider at the memory-owned gate before runtime launch without mutating lifecycle state`
- Phase 19 deferral:
  - No claim is made that Mem0 or any other external memory provider is operationally ready.
  - No claim is made about latency, provider limits, account configuration, or cross-device persistence.

### Scenario 4: Managed Subagent Flow Enforces Review And Fix Semantics

- Product surfaces: managed subagent orchestration, roles, child runs, lineage, write-scope ownership.
- Local/offline evidence:
  - A parent run can create worker and reviewer child runs with distinct roles and non-overlapping write scopes.
  - Worker completion alone does not close the parent task; reviewer approval or explicit rejection of feedback is required.
  - A valid reviewer finding can trigger a follow-up worker run, and lineage records connect the original work, review, fix, and re-review.
- Executable Stage 7 mapping:
  - `tests/integration/phase18-integrated-product-flows.test.ts` -> `runs managed worker, reviewer, fix, and re-review through the managed subagent service boundary`
  - `tests/unit/subagent-service.test.ts` -> `rejects a sibling managed subagent with an overlapping write_set before child start`
  - `tests/unit/subagent-service.test.ts` -> `rejects a second sibling launch when max_children is exhausted`
  - `tests/unit/subagent-service.test.ts` -> `returns reviewer findings and enforces the review-loop ceiling`
  - `tests/unit/subagent-service.test.ts` -> `accepts bounded control messages and honors cancelled_by_parent close semantics`
  - `tests/unit/subagent-service.test.ts` -> `launches, waits, and closes a worker-role managed subagent without touching plain orchestrator_agent behavior`
- Phase 19 deferral:
  - No claim is made about real isolated workspaces, remote execution, provider-side cancellation, or budget enforcement under live load.

### Scenario 5: Runtime Capabilities Gate Integrated Behavior

- Product surfaces: runtime sources, capability metadata, runtime controls, graph execution, interaction.
- Local/offline evidence:
  - Runtime capability metadata determines whether optional controls such as reasoning effort, speed tier, or user-chat behavior are enabled.
  - Unsupported controls fail closed or are omitted according to the portable runtime contract.
  - The graph produces deterministic local evidence for both supported and unsupported capability paths.
- Executable Stage 7 mapping:
  - `tests/integration/phase18-integrated-product-flows.test.ts` -> `builds, deploys, prompts, resumes, and delegates across local subsystem seams`
  - `tests/integration/phase18-integrated-product-flows.test.ts` -> `fails unsupported runtime options at the runtime-owned gate before run creation without mutating lifecycle state`
  - `tests/integration/stage7-interaction-edge-cases.test.ts` -> `defers risky mid-run model changes by rejecting changed revision resumes`
- Phase 19 deferral:
  - No claim is made about real provider model discovery, account limits, live rate limits, or native event streams.

### Scenario 6: Combined Negative Capability Gates Stay At The Owning Boundary

- Product surfaces: lifecycle, runtime capability gating, memory provider negotiation, portable agent JSON.
- Local/offline evidence:
  - A syntactically valid draft requests unsupported runtime options and an unregistered memory provider.
  - Lifecycle state already accepted by completed steps is preserved after the failed deploy or execution attempt.
  - Unsupported runtime options and the unregistered memory provider are reported as owner-specific capability failures, not collapsed into one generic execution error.
  - The earliest violated owner gate is reported deterministically according to the execution path; if runtime validation runs first, the runtime-owned unsupported option is primary, and if memory negotiation runs first, the memory-owned unregistered provider is primary.
  - Portable Agent JSON remains portable: runtime discovery metadata, provider registration metadata, credentials, and provider-specific records are not persisted into the agent file as a side effect of the failure.
- Executable Stage 7 mapping:
  - `tests/integration/phase18-integrated-product-flows.test.ts` -> `fails unsupported runtime options at the runtime-owned gate before run creation without mutating lifecycle state`
  - `tests/integration/phase18-integrated-product-flows.test.ts` -> `fails an unregistered runtime memory provider at the memory-owned gate before runtime launch without mutating lifecycle state`
- Phase 19 deferral:
  - No claim is made that the same combined negative case has been observed against a live runtime or an external memory provider.
  - No claim is made about real provider account configuration, live model capability discovery, or operational error taxonomy.

### Scenario 7: Multi-Feature Conflict Rules Are Explicit

- Product surfaces: builder, lifecycle, runtime, memory, interaction, subagents.
- Local/offline evidence:
  - The Stage 7 evidence set combines generated agent artifacts, lifecycle selection, memory access, user interaction, runtime capability gating, and subagent review across exact executable tests.
  - Conflict rules are visible in local evidence, for example lifecycle immutability winning over draft mutation, capability checks gating runtime controls, and parent/child interaction remaining isolated.
  - The evidence is reproducible without external credentials, paid services, or network availability.
- Executable Stage 7 mapping:
  - `tests/integration/phase18-integrated-product-flows.test.ts` -> `builds, deploys, prompts, resumes, and delegates across local subsystem seams`
  - `tests/integration/phase18-integrated-product-flows.test.ts` -> `runs managed worker, reviewer, fix, and re-review through the managed subagent service boundary`
  - `tests/integration/phase18-integrated-product-flows.test.ts` -> `fails unsupported runtime options at the runtime-owned gate before run creation without mutating lifecycle state`
  - `tests/integration/phase18-integrated-product-flows.test.ts` -> `fails an unregistered runtime memory provider at the memory-owned gate before runtime launch without mutating lifecycle state`
  - No single Stage 7 executable combines every listed surface with live stress, concurrency, cancellation, and provider behavior; that complete integrated proof is deferred to Phase 19.
- Phase 19 deferral:
  - No claim is made that the same combined flow has passed live provider stress, concurrency, cancellation, or operational readiness checks.

## Evidence Rules

- Phase 18 evidence must be reproducible locally and offline.
- Phase 18 evidence may use deterministic fakes, fixtures, golden traces, or local adapters when those match public contracts.
- Phase 18 evidence must not require secrets, hosted services, authenticated provider calls, or network availability.
- Phase 18 acceptance is not satisfied by documentation alone; every scenario should eventually map to an executable local test, fixture, trace, or CLI transcript.
- Phase 19 proof must remain separate and should record real provider identity, account/runtime prerequisites, live execution timestamps, and observed operational limits when implemented.
- Phase 19 release readiness must not be inferred from this golden file; it requires the Phase 19 evidence log and release decision record.
