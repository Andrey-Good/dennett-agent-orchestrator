[English](#english) | [Russian](#russian)

<a id="english"></a>
# Phase 16 Managed Subagent Orchestration

Status: accepted for the bounded local managed-subagent layer. Broader daemon, live runtime, cross-process, hosted, and operator-platform claims remain deferred.

Related documents:

- [Subagent Orchestration Model](../02-architecture/subagent-orchestration-model.md)
- [Subagent MCP Contract](../03-contracts/subagent-mcp-contract.md)
- [Subagent Context and Memory](../05-state/subagent-context-and-memory.md)
- [Phase 12 Capability Gap Lock](../13-capability-gap-lock/phase-12-capability-gap-lock.md)
- [Managed Subagent Productization](../21-public-launch-readiness/managed-subagent-productization.md)

## Bounded Acceptance Decision

Stage 16 is accepted for a local, state-backed managed-subagent layer that remains inside the current Core/CLI process boundary.

Accepted capabilities:

- managed subagent service and port separate from plain `orchestrator_agent` recursion;
- task-package snapshots with `read_context`, `required_validations`, and the bounded `interaction_policy: "silent"` value;
- roles `worker`, `reviewer`, `explorer`, `integrator`, and `final_review`;
- primitives `launch`, `status`, `wait`, `send`, `cancel`, and `close`;
- durable lineage, child status projection, terminal result, findings, close disposition, budgets, bounded control messages, and review-loop state;
- sibling `write_set` conflict rejection as coordination metadata before the second child starts;
- state-level cancellation intent without live runtime cancellation delivery;
- operator-driven review/fix workflow state for worker, reviewer, repair worker, final review, accepted outcome, and budget exhaustion;
- local CLI visibility for launch-and-wait, list, show, status, wait/reconcile, record-control, cancel, and close.

Deferred claims:

- durable daemon or background subagent runners;
- live runtime status probes, live control delivery, or live cancellation delivery;
- cross-process attachment to a running managed child;
- hosted/UI orchestration, fleet management, runtime-host behavior, or service-level operator platform readiness;
- automatic repair orchestration, review quality enforcement, or filesystem sandboxing;
- broad external beta, live provider, production-scale, or public release proof.

Those deferred claims belong to future core-process/runtime-host work, Stage 18 integrated product flows, and Stage 19 real-world proof rather than to this bounded Stage 16 acceptance.

## Implemented Slice

Phase 16 has a first real managed subagent layer that is separate from plain `orchestrator_agent` recursion.

The implemented executable boundary is intentionally narrow:

- a managed subagent port exists;
- Core exposes a dedicated `ManagedSubagentService`;
- the service supports `worker`, `reviewer`, `explorer`, `integrator`, and `final_review` roles;
- the service supports `launch`, `status`, `wait`, `send`, `cancel`, and `close`;
- `wait` supports `terminal_only` and `terminal_or_update`;
- durable state records lineage, task-package snapshot, reviewer-like findings, child status, terminal result, close disposition, budgets, and bounded control messages;
- sibling `write_set` conflicts are rejected before the second child starts;
- persisted budgets are stored with the task package and the central slice enforces sibling caps, review-loop caps, and cancel/close state transitions;
- the CLI exposes a bounded operator surface through `subagent-launch`, `subagent-list`, `subagent-show`, `subagent-status`, `subagent-wait`, `subagent-record-control`, `subagent-cancel`, and `subagent-close`;
- plain `orchestrator_agent` graph recursion remains unchanged.

## Stage 8 Operator Surface

TASK-548 made the first managed-subagent operator surface visible through local CLI commands.

The command boundary is:

- `subagent-launch` is launch-and-wait only: it starts the child and waits for terminal completion in the same CLI process;
- `subagent-list` and `subagent-show` inspect persisted local state;
- `subagent-status` returns the durable managed state projection and does not probe a live runtime;
- `subagent-wait` reconciles or inspects persisted terminal/update state and does not attach to a live child launched by another process;
- `subagent-record-control` records bounded control messages in state and does not live-deliver them to a running child;
- `subagent-cancel` records cancellation intent in durable managed state and reports `runtime_cancellation_delivered: false`;
- `subagent-close` records parent disposition and does not claim runtime cancellation delivery.

This is enough to document a limited local CLI managed-subagent operator surface. It is not enough to document a complete public orchestration platform.

## What This Slice Does Not Claim

This slice does not yet implement the full managed subagent vision.

It does not include:

- durable background subagent runners;
- live runtime status probes;
- live runtime cancellation delivery;
- live delivery of control messages to running children;
- cross-process attachment to a running managed child;
- a broader delegated context package such as `read_context`-driven launch semantics;
- end-to-end repair orchestration or review/fix loops enforced as complete product semantics;
- surfaced child interaction through the parent boundary;
- hosted/UI orchestration, fleet management, or service-level behavior;
- broad external live proof.

## Current Execution Boundary

The current worker/reviewer/explorer/integrator/final-review slice launches a managed child by:

1. resolving a live child agent through lifecycle;
2. recording a durable managed subagent object before child execution;
3. launching the child through the existing child-run machinery;
4. updating durable managed state when the child becomes terminal;
5. allowing the parent to call `status`, `wait` for terminal state or a non-terminal update, and then `close` the boundary explicitly;
6. accepting bounded parent control messages through `send`, including request-status, cancel, and budget-tightening state transitions;
7. exposing `cancel` as an explicit state-level primitive that records cancellation intent without claiming live runtime cancellation delivery.

`wait` is honest for the current slice:

- same-process waits are backed by a live in-memory promise when launched inside that process;
- CLI waits reconcile persisted state and do not attach to a live child launched by another process;
- durable state is the source of truth for lineage, task-package snapshot, findings, terminal result, close disposition, budgets, and recorded control messages;
- no cross-process orchestration guarantee is claimed yet.

## Conflict Rule In This Slice

The first executable conflict rule is intentionally conservative.

Two sibling managed subagents conflict when their recorded `write_set` targets overlap by:

- exact same resource reference; or
- ancestor/descendant resource chain after path-style normalization.

This keeps the first slice safe even if it sometimes rejects work that a later, richer policy might allow. This rule is coordination metadata, not a filesystem sandbox or security boundary.

## Why This Counts As A Real Managed Layer

Before this phase, the executable system had only:

- plain `orchestrator_agent` recursion; and
- documentation for richer managed orchestration.

After this phase, the executable system now also has:

- a distinct managed subagent Core service;
- a distinct managed state surface;
- worker/reviewer/explorer/integrator/final-review task-package snapshots;
- durable parent/child lineage for managed launches;
- bounded operator commands for launch-and-wait, list, show, durable status, wait/reconcile, record-control, explicit cancel, and close;
- explicit close semantics separate from plain child-run completion.

That is enough to claim a real managed-layer contract slice and a limited local CLI operator surface.

It is not enough to claim the broader documented model in full.

<a id="russian"></a>
# Phase 16 Managed Subagent Orchestration

Статус: owner-note для текущего managed-subagent contract-completion slice и Stage 8 operator surface.

Связанные документы:

- [Subagent Orchestration Model](../02-architecture/subagent-orchestration-model.md)
- [Subagent MCP Contract](../03-contracts/subagent-mcp-contract.md)
- [Subagent Context and Memory](../05-state/subagent-context-and-memory.md)
- [Phase 12 Capability Gap Lock](../13-capability-gap-lock/phase-12-capability-gap-lock.md)
- [Managed Subagent Productization](../21-public-launch-readiness/managed-subagent-productization.md)

## Что реально реализовано

В системе есть первый настоящий managed-subagent слой, отделенный от обычной рекурсии `orchestrator_agent`.

Реализованная граница намеренно узкая:

- есть managed subagent port;
- в Core есть отдельный `ManagedSubagentService`;
- сервис поддерживает роли `worker`, `reviewer`, `explorer`, `integrator` и `final_review`;
- сервис поддерживает `launch`, `status`, `wait`, `send`, `cancel` и `close`;
- `wait` поддерживает `terminal_only` и `terminal_or_update`;
- durable state хранит lineage, snapshot task package, reviewer-like findings, child status, terminal result, close disposition, budgets и bounded control messages;
- конфликты sibling `write_set` отклоняются до старта второго child;
- persisted budgets хранятся вместе с task package, а central slice enforces sibling caps, review-loop caps и cancel/close state transitions;
- CLI exposes bounded operator surface через `subagent-launch`, `subagent-list`, `subagent-show`, `subagent-status`, `subagent-wait`, `subagent-record-control`, `subagent-cancel` и `subagent-close`;
- обычное поведение `orchestrator_agent` graph recursion не меняется.

## Stage 8 operator surface

TASK-548 сделал первую operator-facing поверхность managed subagents видимой через local CLI commands.

Граница commands:

- `subagent-launch` является только launch-and-wait: он запускает child и ждет terminal completion в том же CLI process;
- `subagent-list` и `subagent-show` inspect persisted local state;
- `subagent-status` возвращает durable managed state projection и не выполняет live runtime probe;
- `subagent-wait` reconcile/inspect persisted terminal/update state и не attach-ится к live child, запущенному другим process;
- `subagent-record-control` records bounded control messages в state и не live-deliver-ит их running child;
- `subagent-cancel` записывает cancellation intent в durable managed state и сообщает `runtime_cancellation_delivered: false`;
- `subagent-close` records parent disposition и не заявляет runtime cancellation delivery.

Этого достаточно, чтобы документировать limited local CLI managed-subagent operator surface. Этого недостаточно, чтобы документировать complete public orchestration platform.

## Чего этот slice не заявляет

Этот slice еще не реализует полное видение managed subagents.

Здесь пока нет:

- durable background subagent runners;
- live runtime status probes;
- live runtime cancellation delivery;
- live delivery control messages в running children;
- cross-process attachment к running managed child;
- более широкого delegated context package, например `read_context`-driven launch semantics;
- end-to-end repair orchestration или review/fix loops, enforced как complete product semantics;
- surfaced child interaction через parent boundary;
- hosted/UI orchestration, fleet management или service-level behavior;
- широкого external live proof.

## Текущая исполнимая граница

В текущем worker/reviewer/explorer/integrator/final-review slice managed child запускается так:

1. через lifecycle резолвится live child agent;
2. до старта child execution создается durable managed subagent object;
3. child запускается через уже существующую child-run machinery;
4. когда child доходит до terminal state, durable managed state обновляется;
5. parent может вызвать `status`, `wait` для terminal state или non-terminal update, затем явно `close` boundary;
6. bounded parent control messages принимаются через `send`, включая request-status, cancel и budget-tightening state transitions;
7. `cancel` доступен как явный state-level primitive, который записывает cancellation intent без заявления live runtime cancellation delivery.

`wait` в текущем slice честный:

- same-process waits backed by live in-memory promise, когда launch произошел внутри этого process;
- CLI waits reconcile persisted state и не attach-ятся к live child, запущенному другим process;
- durable state является source of truth для lineage, task-package snapshot, findings, terminal result, close disposition, budgets и recorded control messages;
- cross-process orchestration guarantee пока не заявляется.

## Правило конфликтов в этом slice

Первое исполнимое правило конфликтов намеренно консервативное.

Два sibling managed subagents конфликтуют, если их `write_set` targets пересекаются по:

- точному совпадению resource reference; или
- цепочке ancestor/descendant после path-style normalization.

Это делает первый slice безопасным, даже если позже более богатая policy сможет разрешать часть таких случаев. Это правило является coordination metadata, а не filesystem sandbox или security boundary.

## Почему это уже считается настоящим managed layer

До этой фазы в executable системе были только:

- обычная рекурсия `orchestrator_agent`; и
- docs для richer managed orchestration.

После этой фазы в executable системе также есть:

- отдельный managed-subagent service в Core;
- отдельная managed state surface;
- worker/reviewer/explorer/integrator/final-review task-package snapshots;
- durable parent/child lineage для managed launches;
- bounded operator commands для launch-and-wait, list, show, durable status, wait/reconcile, record-control, explicit cancel и close;
- явная close semantics, отделенная от plain child-run completion.

Этого достаточно, чтобы честно заявлять real managed-layer contract slice и limited local CLI operator surface.

Этого все еще недостаточно, чтобы заявлять полное завершение всей широкой модели Phase 16.
