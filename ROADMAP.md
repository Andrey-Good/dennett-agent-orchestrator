# Implementation Roadmap

Denet is implemented as tested vertical slices, not by completing every subsystem in isolation.

1. **Repository and contracts** — compile core contracts, generated protocol clients and checks.
2. **Local desktop conversation** — Tauri → Node → embedded Head → fake/one real runtime → memory event.
3. **Project workspace and review** — real folder, diff, tests, artifacts and checkpoints.
4. **Managed Runs and control surfaces** — cancellation, recovery, Inbox and Radar.
5. **Memory production baseline** — PostgreSQL/SQLite/object store, retrieval and deletion.
6. **Personal server and devices** — pairing, sync, offline logs and opt-in Head handoff.
7. **Mobile trusted remote** — approvals, capture, voice, interruption-safe resumption.
8. **Capability ecosystem** — providers, local models, skills, MCP and plugins.
9. **Voice and ambient** — realtime/chained voice, always-on local microphone path and event-driven screen capture.
10. **External communication and computer-use** — connectors, effects, reconciliation and bounded GUI control.
11. **Production hardening** — signed updates, restore drills, simulation, soak and packaging.

Exit criteria are defined in `docs/architecture/83_Denet_Client_Operations_Testing_and_Implementation_Blueprint.md`.


## How the roadmap is executed

The roadmap is not handed to one agent as a single task. Before a milestone starts, it is refined into vertical slices and Work Packages under `planning/`. Agents execute those packages through `docs/implementation/01_AGENT_EXECUTION_PROTOCOL.md`; autonomous work is limited by an explicit batch envelope and stops at product, security, migration or architecture decisions.

- Strategy: `docs/implementation/00_IMPLEMENTATION_AND_EVOLUTION_STRATEGY.md`
- Work Package model: `docs/implementation/03_WORK_PACKAGE_SYSTEM.md`
- Test catalogue: `docs/testing/TEST_CATALOGUE_AND_QUALITY_GATES.md`
- Owner role: `docs/implementation/02_OWNER_PLAYBOOK.md`

Only the nearest milestone is planned in implementation detail. Later milestones remain progressively coarser and are refined using evidence from completed vertical slices and risk spikes.
