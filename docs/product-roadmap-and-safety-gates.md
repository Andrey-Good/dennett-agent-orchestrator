# Product Roadmap And Safety Gates

Status: durable roadmap index and stage-gating policy.
Derives from: [Project Instructions](../AGENTS.md), [Documentation Map](./README.md), [Capability Gap Lock](./13-capability-gap-lock/phase-12-capability-gap-lock.md), [Release Gates](./11-hardening/release-gates.md), and [Phase 19 Real-World Proof And Release](./20-real-world-proof-and-release/README.md).
Owns: logical stage grouping, operational sequencing guidance, and cross-cutting architecture/code safety gates.
Does not own: canonical roadmap authority, detailed subsystem behavior, implementation status claims, or release decisions that belong to narrower owner documents.

The canonical 19-stage product plan is owned by `AGENTS.md`. Phase 12 capability status, gap labels, and readiness rules are owned by the Phase 12 capability gap lock.

This document preserves the product roadmap as a durable planning artifact. It is a sequencing and quality-control document, not proof that every stage is complete. Completion claims must continue to come from the relevant owner docs, tests, evidence logs, and release decision records.

## Stage Groups

The roadmap is divided into three logical groups:

| Group | Stages | Purpose |
| --- | --- | --- |
| Foundation and stable local product | 1-11 | Lock the canon, define contracts, build the minimal vertical slice, stabilize core behavior, and harden the local product surface. |
| Post-foundation capability expansion | 12-17 | Freeze capability truth, add native memory/runtime/interaction/subagent/builder capabilities, and keep each extension behind public contracts. |
| Integrated proof and release evidence | 18-19 | Prove that the subsystems work together and make release decisions from evidence instead of intent. |

The documentation tree has a `12-reference-targets` section for non-normative references, so roadmap Stage 12 is recorded under `13-capability-gap-lock`; later roadmap stages are similarly offset by one docs section number.

## Non-Negotiable Sequencing Rule

Future work must not advance to the next roadmap stage until the current stage passes its exit gates. A later-stage discovery may create a backport task for an earlier owner area, but it must not silently redefine the current stage as complete.

If a stage cannot pass because of a blocker, the blocker must be recorded in the owning document, capability matrix, ADR, evidence log, or task document before another stage builds on top of it.

## Continuous Safety Gates

These gates apply before, during, and after every stage.

### Before A Stage Starts

- Confirm the stage owner document exists or create it before implementation begins.
- Define in-scope and out-of-scope behavior, including explicit non-goals.
- Identify affected contracts, schemas, domain types, examples, and storage/runtime boundaries.
- Decide whether the work needs an ADR before code is written.
- Split large work into subagent tasks with non-overlapping write scopes when orchestration is justified.
- Reuse platform, runtime, framework, and provider primitives where they satisfy the needed contract instead of rebuilding them unnecessarily.

### During A Stage

- Keep core logic, storage, runtime adapters, interfaces, and documentation boundaries separate.
- Prefer contract-first changes: schemas, types, invariants, examples, and negative cases should lead implementation.
- Keep runtime-specific behavior behind stable internal contracts.
- Document invented business logic in an owner doc, ADR, task document, or acceptance test.
- Use worker/reviewer subagent loops for substantial changes, and do not assign overlapping write scopes.
- Treat failing tests, contract mismatches, or documentation contradictions as blockers, not cleanup items.

### Before A Stage Exits

- Verify the owner docs, code, tests, examples, and CLI/user-facing claims agree.
- Run the stage-appropriate automated checks and record any manual validation that cannot be automated.
- Confirm no public or internal doc claims a capability that is only stubbed, mocked, or partially proven.
- Update the capability matrix when implementation status changes.
- Run a final system-level review for cross-stage conflicts.
- Record remaining gaps as deferred, blocked, or intentionally out of scope.

## Stage Plan

### 1. Canon And MVP Boundary Lock

Goal:

- lock what is considered the canonical high-level specification;
- define MVP boundaries;
- separate first-version requirements from deferred work;
- document invariants and non-goals.

Exit gates:

- one canonical high-level source is identified;
- MVP and non-MVP behavior are separated;
- non-goals and invariants are discoverable in the docs tree.

### 2. Detailed Specifications

Goal:

- break the main high-level document into narrower detailed specifications;
- remove ambiguity;
- describe non-obvious business logic before coding begins.

Minimum document set:

- scope and non-goals;
- glossary and domain model;
- agent JSON contract;
- graph execution;
- runtime adapter contract;
- storage, chat, and resume;
- interaction and user chat MCP;
- registry, draft, live, and deploy;
- testing strategy;
- ADRs for contested architectural decisions.

Exit gates:

- each major subsystem has a focused owner document;
- behavioral rules are not hidden only in implementation code;
- contested decisions are routed to ADRs.

### 3. Executable Contracts

Goal:

- turn important rules into formal, testable contracts;
- prepare schemas, types, invariants, golden examples, and negative cases.

Expected results:

- JSON Schema;
- domain types;
- invariant table;
- acceptance cases;
- valid and invalid configuration examples.

Exit gates:

- valid and invalid cases are represented by tests or fixtures;
- schemas and types match documented behavior;
- future code can validate inputs before execution.

### 4. Code Architecture

Goal:

- define module boundaries before implementation expands;
- define the responsibility of each layer;
- prevent mixing core logic, storage, runtime adapters, and interfaces.

Exit gates:

- dependencies flow through documented boundaries;
- runtime-specific code is isolated behind adapters;
- storage and interface code do not become implicit core logic owners.

### 5. Minimal Vertical Slice

Goal:

- assemble the first end-to-end working version;
- run a simple graph through one runtime adapter;
- obtain a valid final output through CLI.

Exit gates:

- a minimal graph run works end to end;
- the CLI path exercises real core behavior;
- the output shape satisfies the accepted contract.

### 6. Stable Core

Goal:

- implement reliable graph execution;
- implement outcomes, vars, outputs, resume, chat state, and interruptions;
- implement crash-safe and atomic behavior where it matters.

Exit gates:

- core graph behavior is covered by meaningful regression tests;
- crash/recovery-sensitive paths use the documented storage policy;
- interruptions and resume behavior do not rely on transient process memory.

### 7. Live Interaction During Run

Goal:

- implement live comments;
- implement the built-in MCP for agent-user communication;
- implement routing rules for user messages during execution.

Exit gates:

- live interaction has a durable owner contract;
- user-message routing is deterministic and testable;
- unsupported runtime paths fail honestly instead of pretending to support interaction.

### 8. Agent Lifecycle

Goal:

- implement registry behavior;
- implement drafts, live revisions, and deploy;
- preserve safe publishing of changes;
- support independent versioning axes.

Exit gates:

- draft/live/deploy state transitions are explicit and tested;
- safe publishing does not overwrite unrelated live state accidentally;
- versioning rules are documented and reflected in CLI behavior.

### 9. Extensions On Top Of Stable Core

Goal:

- add memory bindings;
- add runtime sources, accounts, and limits;
- add events/triggers;
- add orchestrator_agent and nested graphs.

Sequencing constraint:

- these parts must not be implemented before the core is stable.

Exit gates:

- extensions consume stable core contracts instead of bypassing them;
- optional capabilities are capability-gated;
- unsupported bindings fail with clear diagnostics.

### 10. Builder-Agent

Goal:

- implement the built-in builder agent only after the base contracts are stable;
- make the builder rely on public system contracts, not hidden internal magic.

Exit gates:

- builder output validates against public contracts;
- builder behavior is documented as product behavior, not an internal shortcut;
- generated drafts have acceptance tests or golden examples.

### 11. Hardening And Release Readiness

Goal:

- run load and integration validation;
- add crash/recovery tests;
- add backward compatibility checks;
- add CI, linters, type checks, and coverage on critical areas;
- write operational documentation.

Exit gates:

- mandatory release gates are documented and runnable;
- operational and recovery risks are recorded;
- release readiness is not claimed without the required evidence.

### 12. Capability Gap Lock

Goal:

- freeze a truthful matrix of what is implemented, partially implemented, documented only, runtime-blocked, or still missing;
- map each meaningful capability to owner docs, code status, test status, and live-proof status;
- stop further roadmap execution from inventing scope on the fly;
- record the first external memory target and its real readiness constraints.

Expected results:

- a canonical docs-to-code-to-tests gap matrix;
- explicit status labels and acceptance rules for future roadmap steps;
- a frozen Mem0-first readiness note that distinguishes package availability from real provider readiness;
- a canonical post-11 roadmap owned by `AGENTS.md`, governed by the Phase 12 gap lock where status and readiness are concerned, and linked from the docs tree.

Exit gates:

- status labels are stable and consistently applied;
- later-stage work can start from the matrix without rediscovering the whole product;
- no capability is marked done without docs, code, tests, and live proof where live proof is required.

### 13. Native Memory Integration (Mem0 First)

Goal:

- implement the first real external memory provider path behind the internal memory layer;
- keep the provider user-owned and locally registered;
- prove that portable memory bindings can drive a real provider adapter without making memory part of the agent file itself.

Expected results:

- a working internal memory port in code;
- local provider registration and capability negotiation;
- the first provider adapter for Mem0;
- real read/write/search behavior exercised by tests and at least one live proof path.

Exit gates:

- provider behavior is behind the internal memory contract;
- local registration does not leak provider secrets into portable agent files;
- tests and live proof distinguish real provider behavior from stubs.

### 14. Native Runtime Surface Completion

Goal:

- complete App Server-native runtime features that are useful for Dennett and should not be reimplemented manually;
- expose richer runtime capability metadata while preserving the vendor-neutral core boundary.

Expected results:

- model discovery and model metadata;
- reasoning-effort, speed-tier, and related runtime controls when the source runtime supports them;
- account, auth, config, and rate-limit introspection;
- capability-gated behavior for runtime sources, limits, and richer native events.

Exit gates:

- native runtime features reuse real runtime surfaces where available;
- vendor-neutral core contracts remain stable;
- unsupported controls are reported as unsupported instead of silently ignored.

### 15. Full User Interaction Layer

Goal:

- complete the mid-run interaction model, not only comments;
- make user chat, blocked prompts, replies, resume-after-reply, and risky-parameter-change handling behave as one coherent product surface.

Expected results:

- a real built-in user-chat flow where the active runtime supports it;
- durable prompt/reply and wait-state handling;
- explicit policies for risky mid-run changes such as model changes inside an existing live chat;
- tested user-visible interaction semantics across CLI and core state.

Exit gates:

- wait states are durable and resumable;
- risky mid-run changes have documented policy outcomes;
- CLI and core state present the same user-visible semantics.

### 16. Managed Subagent Orchestration

Goal:

- implement the richer managed subagent system on top of the stable portable child-run primitive;
- move from bare `orchestrator_agent` child launches to governed multi-agent orchestration.

Expected results:

- create, send, wait, status, close, and cancel primitives;
- roles such as worker, reviewer, explorer, and integrator;
- write-scope ownership, lineage, budgets, and nested-spawn policy;
- review and fix loops enforced by product semantics, not only prompt convention.

Exit gates:

- managed subagent ownership boundaries are represented and enforced at the accepted coordination layer; no filesystem sandbox or hard write-prevention claim is made unless a later implementation proves it;
- review/fix loops are represented by durable product state;
- nested spawning, cancellation, and budget behavior are explicit, with unsupported daemon/live/cross-process behavior recorded as deferred instead of implied.

### 17. Builder 2.0

Goal:

- upgrade the builder from a draft-producing first slice to a full authoring system that can target richer runtime, memory, and subagent surfaces;
- keep builder behavior inside public contracts.

Expected results:

- builder support for the richer portable contract;
- builder awareness of memory bindings, provider capability requirements, and managed subagent patterns;
- stronger self-review and revision workflows without hidden builder-only shortcuts.

Exit gates:

- builder outputs remain portable and contract-valid;
- richer surfaces are used only through public contracts;
- self-review and revision behavior has acceptance coverage.

### 18. Integrated Product Flows

Goal:

- prove that the major subsystems work together as one product rather than as isolated feature slices.

Expected results:

- end-to-end flows that combine user interaction, memory, runtime features, builder output, lifecycle, and subagents;
- clear conflict rules where multiple subsystems interact;
- acceptance coverage for realistic multi-feature scenarios.

Exit gates:

- integrated scenarios cover cross-subsystem conflicts;
- local/offline proof is clearly distinguished from live external proof;
- unresolved integration gaps are assigned to the correct owner stage.

### 19. Real-World Proof And Release

Goal:

- move from internally coherent implementation to externally credible product readiness.

Expected results:

- live end-to-end proofs against real runtimes and providers;
- regression and stress coverage for the integrated system;
- operational runbooks and final release criteria;
- a release decision based on evidence rather than architectural intent alone.

Exit gates:

- live proof, stress proof, and operational evidence are recorded;
- release scope is locked and honest about deferred capabilities;
- a release decision record exists before any release-readiness claim is made.

## Status And Evidence Rule

This roadmap records desired sequencing. It must not be used by itself to claim a stage is implemented, released, or production-ready. Use the stage owner documents, the capability gap matrix, validation logs, and release decision records for current status.

When status changes, update the narrowest responsible owner document first, then update navigation or this roadmap only if the change affects sequencing or safety gates.
