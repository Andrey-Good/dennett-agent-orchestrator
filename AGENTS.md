
# Dennett — Root Agent Instructions

## Mission

Implement Dennett as specified: a personal agentic operating environment with direct project agents, one persistent orchestrator, evidence-grounded memory, user-controlled autonomy, replaceable providers, voice/ambient interaction and multi-device operation.

The repository is architecture-first. Do not invent product semantics inside code when a canonical document owns them.

## Reading order

1. `README.md` and `docs/README.md`.
2. The assigned Work Package or Autonomous Batch under `planning/`.
3. `docs/implementation/01_AGENT_EXECUTION_PROTOCOL.md`.
4. Nearest nested `AGENTS.md`.
5. The linked business specification for the subsystem.
6. The linked architecture volume and relevant ADR.
7. Public traits/schemas and existing tests.
8. A similar existing module or adapter.

## Canonical documents

- Product vision: `docs/specifications/00_Dennett_Functional_Concept.md`
- Ownership/contracts: `docs/specifications/01_Dennett_Specification_Index_and_Shared_Contracts.md`
- Memory: `docs/specifications/10_Dennett_Memory_Fabric.md`
- Agents/tasks/projects: `docs/specifications/20_Dennett_Agentic_Control_Fabric.md`
- Trust/identity/permissions: `docs/specifications/30_Dennett_Trust_Identity_Autonomy_and_Permissions.md`
- Voice/ambient: `docs/specifications/40_Dennett_Voice_and_Ambient_Interaction_Fabric.md`
- Capabilities/providers/integrations: `docs/specifications/41_Dennett_Capabilities_Providers_and_Integrations.md`
- Server/events/sync: `docs/specifications/50_Dennett_Server_Runtime_Events_Sync_and_Portability.md`
- Desktop/mobile UX: `docs/specifications/60_*`, `61_*`
- Validation: `docs/specifications/70_Dennett_End_to_End_Validation_and_Architecture_Handoff.md`
- Architecture: `docs/architecture/80_*` through `83_*`


## Implementation system

- Strategy: `docs/implementation/00_IMPLEMENTATION_AND_EVOLUTION_STRATEGY.md`
- Agent protocol: `docs/implementation/01_AGENT_EXECUTION_PROTOCOL.md`
- Owner playbook: `docs/implementation/02_OWNER_PLAYBOOK.md`
- Work Package model: `docs/implementation/03_WORK_PACKAGE_SYSTEM.md`
- Milestone map: `docs/implementation/04_MILESTONE_DEPENDENCY_MAP.md`
- Test catalogue: `docs/testing/TEST_CATALOGUE_AND_QUALITY_GATES.md`

Do not begin a semantic code change without a Work Package. A broad user request must first be converted into a bounded package or autonomous batch.

## Engineering priorities

Use this priority order when trade-offs are unavoidable:

1. correctness;
2. reliability and recoverability;
3. architectural clarity;
4. testability;
5. extensibility and replaceability;
6. maintainability;
7. implementation speed.

Code, tests and project artifacts must remain understandable without access to the model transcript or hidden reasoning.

Apply DRY, KISS and YAGNI as review constraints, not as excuses to weaken behavior or create a dead end:

- design public contracts, ownership boundaries and ports for the planned product evolution (including replaceable providers, additional capabilities and multi-device operation), then choose the simplest internal implementation that satisfies those contracts and their failure modes;
- remove duplication when it repeats authoritative knowledge or is already causing divergent behavior; do not introduce an abstraction merely because two snippets look similar;
- preserve documented replacement seams and typed capability discovery now, but do not pre-implement speculative provider behavior, generic frameworks or fallback machinery without a current contract or demonstrated need;
- every added layer, state machine or abstraction must protect a named invariant, recovery path, test seam or confirmed provider boundary. If it does not, simplify or remove it.

If required product or architecture semantics are missing, ambiguous or contradictory, do not silently invent a permanent rule in code. Capture the assumption or decision in the Work Package, a decision request, specification, ADR or acceptance test. Stop for owner input when the choice changes product behavior, privacy, authority, external cost, recoverability or another difficult-to-reverse boundary.

Prefer mature capabilities already provided by the selected runtime, SDK, framework or platform. Add translation, normalization, safety enforcement and stable project-level ports around them; implement custom lifecycle, session, tool, approval or discovery machinery only when the existing capability cannot satisfy the documented contract, reliability, portability or replacement requirements.

## Current runtime constraint

Until the owner explicitly changes this constraint:

- the official Codex runtime bundled with the pinned Codex SDK dependency is the only permitted real agent runtime integration; use the high-level TypeScript SDK for simple sequential runs and the bundled Codex App Server when a documented product contract needs richer native behavior such as in-flight steer;
- deterministic fake/in-memory runtimes remain required for tests and credential-free development;
- Codex SDK and provider-specific types stay inside adapter or adapter-host roots; domain and application code depend on `AgentRuntimePort` and other provider-neutral ports;
- implement Codex-first, not Codex-only: preserve descriptors, capability probes, session mapping, typed `native_extensions` and conformance boundaries so later providers and local models can be added without rewriting core behavior;
- do not rebuild Codex-native sessions, workspace handling, tools, approvals, checkpoints or event streams unless a documented Dennett contract requires behavior the SDK does not provide.

## Repository stewardship

Within this repository and its local development environment, routine engineering operations do not require per-step owner approval:

- install required development tools and dependencies;
- create short-lived branches/worktrees;
- commit coherent changes;
- push branches or verified `main` updates;
- maintain CI, repository metadata and contributor-facing documentation.

Keep `main` buildable, testable, public-safe and suitable as the current usable state of the project. Incomplete behavior must stay on a branch or behind a disabled feature/capability gate. Never commit secrets, private user content, local credentials or machine-specific state.

Ask before actions that materially extend beyond the repository scope, introduce paid or hosted commitments, change external accounts/security settings, publish private data, or perform destructive operations outside documented development workflows.

## Repository architecture rules

1. One mutable state has one authoritative owner.
2. UI, provider sessions and model prompts are never the only source of important state.
3. Domain and application layers depend on ports, not provider SDKs or database clients.
4. Provider-specific types stay inside adapters or wire-conversion modules.
5. External effects use Effect Claim/Receipt, idempotency and reconciliation.
6. Permissions are enforced outside models.
7. `dennett-node` owns local durable behavior; Tauri/React is a client shell.
8. `dennett-memory-core` defines one logical memory. PostgreSQL/SQLite are deployment adapters, not different memories.
9. A normal client cache cannot become canonical memory merely because the device is online.
10. Device Head promotion is allowed only when `head_eligibility` was explicitly granted and eligibility checks pass.
11. Start with one strong agent. Add subagents/workflows only after a documented marginal-utility reason.
12. Derived indexes and projections must be rebuildable from canonical data.
13. No silent last-write-wins for meaningful conflicts.
14. Local-only development must work without cloud credentials.
15. New adapters must pass the relevant conformance suite.

## Dependency direction

```text
UI / CLI / protocol ingress
        ↓
application services
        ↓
domain core
        ↓
ports
        ↑
adapters / persistence / providers / OS
```

Forbidden examples:

- frontend → SQL/SQLite/PostgreSQL directly;
- memory-core → provider SDK;
- agent-core → OpenAI/Claude-specific types;
- adapter → private repository/table of another module;
- connector → bypass Effect Bridge;
- project file or prompt → modify Trust policy directly.

## Head eligibility

Device role is configuration, not device type.

- `none` — default; never becomes Head.
- `emergency` — can provide restricted local continuity after explicit user action or a preauthorized emergency policy.
- `full` — may become Head only if the user explicitly granted the role, strong authentication succeeds, required keys and canonical data are available, and fencing/epoch rules pass.

A PC configured as a full Head uses the same canonical Memory Fabric and server-grade storage role as a dedicated server. SQLite on ordinary clients remains cache/offline state.

## Change protocol

Before editing:

- write a compact behavior contract for every non-trivial change: user-visible outcome, explicit non-goals, authoritative state owner, lifecycle/state transitions, failure and recovery behavior, and acceptance scenarios; keep it in the Work Package, test names or a nearby design note rather than creating a document when those existing artifacts are sufficient;
- for an external SDK, runtime, OS or framework capability, verify the installed version and exercise the smallest disposable technical spike before designing a fallback or changing production architecture; do not infer that a lower-level official capability is absent only because a high-level wrapper does not expose it;
- identify owner and invariant;
- identify permission/effect implications;
- identify protocol/schema/migration impact;
- identify tests required.

Implement the smallest end-to-end slice that proves the contract, run its focused tests, and only then widen the change. If the spike contradicts the intended design, revise the contract before expanding code. This is a quality gate, not a requirement for duplicate prose, approvals or ceremony.

After editing:

- run format/lint/unit/contract tests;
- run scenario tests for stateful changes;
- update docs/ADR if contract or architecture changed;
- report observed behavior, not only model claims.

## Common commands

```bash
python tools/verify_repo.py
python tools/verify_docs.py
python tools/generate_doc_index.py --check
python tools/verify_planning.py
python -m unittest discover -s services/adapter-host-python/tests
pnpm typecheck
cargo test --workspace
```

## Generated files

Do not edit generated protocol clients, schema indexes or documentation manifests directly. Change the source schema and regenerate.

## Definition of Done

A change is done only when:

- behavior matches canonical specifications;
- authoritative state and failure path are clear;
- cancellation and recovery are handled where relevant;
- tests cover normal, partial, stale/offline and failure behavior;
- no secret/private content is added to logs or fixtures;
- new observability is privacy-aware;
- documentation and ADRs remain consistent;
- repository verification passes.
