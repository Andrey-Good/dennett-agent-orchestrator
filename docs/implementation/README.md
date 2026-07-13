# Implementation Documentation

This section turns the product and architecture into a sustainable execution system.

## Reading order

1. [`00_IMPLEMENTATION_AND_EVOLUTION_STRATEGY.md`](00_IMPLEMENTATION_AND_EVOLUTION_STRATEGY.md) — how Denet is built and kept maintainable for years.
2. [`01_AGENT_EXECUTION_PROTOCOL.md`](01_AGENT_EXECUTION_PROTOCOL.md) — exact operating protocol for GPT-5.6 Sol or another coding agent.
3. [`02_OWNER_PLAYBOOK.md`](02_OWNER_PLAYBOOK.md) — what the repository owner decides and how to supervise without micromanaging code.
4. [`03_WORK_PACKAGE_SYSTEM.md`](03_WORK_PACKAGE_SYSTEM.md) — machine-readable milestones, work packages and autonomous batches.
5. [`../testing/TEST_CATALOGUE_AND_QUALITY_GATES.md`](../testing/TEST_CATALOGUE_AND_QUALITY_GATES.md) — structured test requirements and release gates.

## Core rule

A coding agent may reason freely inside a bounded Work Package. It may not silently redefine product semantics, state ownership, permissions, external effects or architecture.

## Current planning

See [`../../planning/README.md`](../../planning/README.md) and the nearest milestone file.

- [`04_MILESTONE_DEPENDENCY_MAP.md`](04_MILESTONE_DEPENDENCY_MAP.md) — зависимость этапов, owner gates и автономные горизонты.
