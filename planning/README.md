# Denet Planning

This directory stores the machine-readable implementation plan.

- `milestones/` — milestone definitions and current rolling horizon.
- `work-packages/` — one file per executable Work Package once the catalogue is expanded.
- `batches/` — bounded autonomous batches.
- `decisions/` — pending and resolved product/architecture decisions.
- `debt/` — explicit technical debt with owner and repayment trigger.
- `templates/` — authoring templates.

Canonical process:

1. Read [`docs/implementation/00_IMPLEMENTATION_AND_EVOLUTION_STRATEGY.md`](../docs/implementation/00_IMPLEMENTATION_AND_EVOLUTION_STRATEGY.md).
2. Execute packages through [`01_AGENT_EXECUTION_PROTOCOL.md`](../docs/implementation/01_AGENT_EXECUTION_PROTOCOL.md).
3. Follow [`03_WORK_PACKAGE_SYSTEM.md`](../docs/implementation/03_WORK_PACKAGE_SYSTEM.md).
4. Never mark a package `READY` without valid acceptance test IDs.
5. Keep the nearest milestone detailed and future milestones progressively coarser.

Planning files are not a replacement for canonical specifications or architecture. A package may reference them but cannot override them.
