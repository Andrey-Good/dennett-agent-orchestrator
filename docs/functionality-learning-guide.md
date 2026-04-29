# Functionality Learning Guide

Use this guide when you want to learn Dennett functionality before reading the internal owner-document map. This page is navigation only; it does not add behavior, stability, package, hosted, or public-readiness claims.

## First Pass

1. Read [project scope and non-goals](./01-foundations/project-scope-and-non-goals.md) and the [glossary](./01-foundations/glossary.md) to learn the product boundary and terms.
2. Read the [canonical agent JSON example](./10-examples/canonical-agent-json-example.md) to see one complete portable agent definition.
3. Read [valid patterns](./10-examples/valid-patterns.md) and [invalid patterns](./10-examples/invalid-patterns.md) to understand what the system accepts and rejects.
4. Read [interaction sequences](./10-examples/interaction-sequences.md) when you need behavior over time, such as comments, user chat, explicit resume, and revision binding.

## Core Concepts

- Agent JSON: start at the [agent JSON contract](./03-contracts/agent-json/README.md), then read top-level bindings, nodes and edges, interaction/chat, and memory binding only as needed.
- Graph execution: read [graph execution](./04-execution/graph-execution.md), then [dataflow and input resolution](./04-execution/dataflow-and-input-resolution.md), then [outputs, outcomes, and final response](./04-execution/outputs-outcomes-and-final-response.md).
- State and resume: read [chat and resume](./05-state/chat-and-resume.md), [local storage](./05-state/local-storage-model.md), and [atomic write policy](./05-state/atomic-write-policy.md).
- Lifecycle: read [agent registry](./07-lifecycle/agent-registry.md), [draft/live/deploy](./07-lifecycle/draft-live-deploy.md), and [versioning axes](./07-lifecycle/versioning-axes.md).

## CLI And Operator Path

Start with the stable command inventory in [Stable CLI/API Contract Freeze](./21-public-launch-readiness/stable-cli-api-contract-freeze.md). Use it to distinguish stable local commands such as `register`, `deploy`, `run`, `run-live`, `run-status`, `reply`, `resume`, and `support-bundle` from commands that are still explicitly experimental.

From a source checkout, build first and then use the local package script alias:

```powershell
pnpm build
pnpm dennett --help
pnpm dennett run <agent-file>
```

After the command inventory, read [draft/live/deploy](./07-lifecycle/draft-live-deploy.md) for registration and publishing semantics, [interaction sequences](./10-examples/interaction-sequences.md) for reply and resume flows, and [operational readiness](./11-hardening/operational-readiness.md) for local diagnostics boundaries.

## Experimental Surfaces

Read these after the core model, and check the CLI stability labels before relying on them as stable user workflows:

- Memory: [memory bindings](./08-extensions/memory-bindings.md) and the [memory binding contract](./03-contracts/agent-json/memory-binding-model-contract.md).
- Builder: [builder agent](./08-extensions/builder-agent.md).
- Subagents: [subagent orchestration model](./02-architecture/subagent-orchestration-model.md), [subagent MCP contract](./03-contracts/subagent-mcp-contract.md), and [subagent task lifecycle](./04-execution/subagent-task-lifecycle.md).
- Runtime inspection: [runtime sources](./08-extensions/runtime-sources.md), [runtime integration model](./02-architecture/runtime-integration-model.md), and [runtime adapter contract](./03-contracts/runtime-adapter-contract.md).
- Triggers and events: [events and triggers](./07-lifecycle/events-and-triggers.md).

## Contributor And Architecture Path

For design or implementation work, switch from learning flow to owner documents:

- [Architecture](./02-architecture/README.md) for layers, module boundaries, runtime integration, and subagent architecture.
- [Contracts](./03-contracts/README.md) for schemas, adapter contracts, MCP contracts, and invariant ownership.
- [ADRs](./09-adrs/README.md) for rationale behind contested decisions.
- [Hardening](./11-hardening/README.md) and [public launch readiness](./21-public-launch-readiness/README.md) for validation, operational boundaries, and claim limits.
- [Capability gap lock](./13-capability-gap-lock/README.md) when you need current documented status instead of intended architecture.
