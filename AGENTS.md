
# Denet — Root Agent Instructions

## Mission

Implement Denet as specified: a personal agentic operating environment with direct project agents, one persistent orchestrator, evidence-grounded memory, user-controlled autonomy, replaceable providers, voice/ambient interaction and multi-device operation.

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

- Product vision: `docs/specifications/00_Denet_Functional_Concept.md`
- Ownership/contracts: `docs/specifications/01_Denet_Specification_Index_and_Shared_Contracts.md`
- Memory: `docs/specifications/10_Denet_Memory_Fabric.md`
- Agents/tasks/projects: `docs/specifications/20_Denet_Agentic_Control_Fabric.md`
- Trust/identity/permissions: `docs/specifications/30_Denet_Trust_Identity_Autonomy_and_Permissions.md`
- Voice/ambient: `docs/specifications/40_Denet_Voice_and_Ambient_Interaction_Fabric.md`
- Capabilities/providers/integrations: `docs/specifications/41_Denet_Capabilities_Providers_and_Integrations.md`
- Server/events/sync: `docs/specifications/50_Denet_Server_Runtime_Events_Sync_and_Portability.md`
- Desktop/mobile UX: `docs/specifications/60_*`, `61_*`
- Validation: `docs/specifications/70_Denet_End_to_End_Validation_and_Architecture_Handoff.md`
- Architecture: `docs/architecture/80_*` through `83_*`


## Implementation system

- Strategy: `docs/implementation/00_IMPLEMENTATION_AND_EVOLUTION_STRATEGY.md`
- Agent protocol: `docs/implementation/01_AGENT_EXECUTION_PROTOCOL.md`
- Owner playbook: `docs/implementation/02_OWNER_PLAYBOOK.md`
- Work Package model: `docs/implementation/03_WORK_PACKAGE_SYSTEM.md`
- Milestone map: `docs/implementation/04_MILESTONE_DEPENDENCY_MAP.md`
- Test catalogue: `docs/testing/TEST_CATALOGUE_AND_QUALITY_GATES.md`

Do not begin a semantic code change without a Work Package. A broad user request must first be converted into a bounded package or autonomous batch.

## Repository architecture rules

1. One mutable state has one authoritative owner.
2. UI, provider sessions and model prompts are never the only source of important state.
3. Domain and application layers depend on ports, not provider SDKs or database clients.
4. Provider-specific types stay inside adapters or wire-conversion modules.
5. External effects use Effect Claim/Receipt, idempotency and reconciliation.
6. Permissions are enforced outside models.
7. `denet-node` owns local durable behavior; Tauri/React is a client shell.
8. `denet-memory-core` defines one logical memory. PostgreSQL/SQLite are deployment adapters, not different memories.
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

- identify owner and invariant;
- identify permission/effect implications;
- identify protocol/schema/migration impact;
- identify tests required.

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
