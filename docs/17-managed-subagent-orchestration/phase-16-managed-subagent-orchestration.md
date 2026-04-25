[English](#english) | [Russian](#russian)

<a id="english"></a>
# Phase 16 Managed Subagent Orchestration

Status: owner note for the current managed-subagent contract-completion slice.

Related documents:

- [Subagent Orchestration Model](../02-architecture/subagent-orchestration-model.md)
- [Subagent MCP Contract](../03-contracts/subagent-mcp-contract.md)
- [Subagent Context and Memory](../05-state/subagent-context-and-memory.md)
- [Phase 12 Capability Gap Lock](../13-capability-gap-lock/phase-12-capability-gap-lock.md)

## Implemented Slice

Phase 16 now has a first real managed subagent layer that is separate from plain `orchestrator_agent` recursion.

The implemented executable boundary is intentionally narrow:

- a new managed subagent port exists;
- Core exposes a dedicated `ManagedSubagentService`;
- the service supports `worker`, `reviewer`, and `final_review` roles;
- the service supports `launch`, `wait`, `send`, and `close`;
- `wait` supports `terminal_only` and `terminal_or_update`;
- durable state now records:
  - lineage,
  - task-package snapshot,
  - reviewer-like findings,
  - child status,
  - terminal result,
  - close disposition;
- sibling `write_set` conflicts are rejected before the second child starts;
- persisted budgets are stored with the task package and the central slice enforces sibling caps, review-loop caps, and cancel/close state transitions;
- plain `orchestrator_agent` graph recursion remains unchanged.

## What This Slice Does Not Claim

This slice does not yet implement the full managed subagent vision.

It does not include:

- a broader delegated context package such as `read_context`-driven launch semantics;
- end-to-end repair orchestration beyond the contract-completion primitives implemented here;
- surfaced child interaction through the parent boundary;
- a CLI surface for managed subagents;
- broad external live proof.

## Current Execution Boundary

The current worker slice launches a managed child by:

1. resolving a live child agent through lifecycle;
2. recording a durable managed subagent object before child execution;
3. launching the child through the existing child-run machinery;
4. updating durable managed state when the child becomes terminal;
5. allowing the parent to `wait` for terminal state or a non-terminal update and then `close` the boundary explicitly;
6. accepting bounded parent control messages through `subagent.send`, including cancel and budget-tightening paths.

`wait` is honest for the current slice:

- same-process waits are backed by a live in-memory promise;
- durable state is still the source of truth for lineage, task-package snapshot, findings, terminal result, close disposition, and budgets;
- no cross-process orchestration guarantee is claimed yet.

## Conflict Rule In This Slice

The first executable conflict rule is intentionally conservative.

Two sibling managed subagents conflict when their recorded `write_set` targets overlap by:

- exact same resource reference; or
- ancestor/descendant resource chain after path-style normalization.

This keeps the first slice safe even if it sometimes rejects work that a later, richer policy might allow.

## Why This Counts As A Real Managed Layer

Before this phase, the executable system had only:

- plain `orchestrator_agent` recursion; and
- documentation for richer managed orchestration.

After this phase, the executable system now also has:

- a distinct managed subagent Core service;
- a distinct managed state surface;
- an explicit worker-role task-package snapshot;
- durable parent/child lineage for managed launches;
- explicit close semantics separate from plain child-run completion.

That is enough to claim a real managed-layer contract slice.

It is not enough to claim the broader documented model in full.

<a id="russian"></a>
# Phase 16 Managed Subagent Orchestration

Статус: owner-note для первого исполнимого slice фазы 16.

## Что реально реализовано

Теперь в системе есть первый настоящий managed-subagent слой, отделенный от обычной рекурсии `orchestrator_agent`.

Реализованная граница намеренно узкая:

- есть новый managed subagent port;
- в Core есть отдельный `ManagedSubagentService`;
- поддерживается только роль `worker`;
- поддерживаются только операции `launch`, `wait` и `close`;
- durable state теперь хранит:
  - lineage,
  - snapshot task package,
  - child status,
  - terminal result,
  - close disposition;
- конфликты sibling `write_set` отклоняются до старта второго child;
- обычное поведение `orchestrator_agent` не меняется.

## Чего этот slice не заявляет

Этот slice еще не реализует полное видение managed subagents.

Здесь пока нет:

- ролей reviewer и final-review;
- repair/review loops;
- `subagent.send`;
- полноценного budget enforcement;
- surfaced child interaction через parent boundary;
- CLI surface;
- широкого внешнего live proof.

## Текущая исполнимая граница

В текущем worker-slice managed child запускается так:

1. через lifecycle резолвится live child-agent;
2. до старта child execution создается durable managed-subagent record;
3. child запускается через уже существующую child-run механику;
4. когда child доходит до terminal состояния, durable managed state обновляется;
5. parent может вызвать `wait`, затем явно `close`.

`wait` в текущем slice честный:

- в рамках одного процесса он опирается на live in-memory promise;
- durable state при этом остается source of truth для lineage, task package, terminal result и close disposition;
- никаких cross-process гарантий оркестрации пока не заявляется.

## Правило конфликтов в этом slice

Первое исполнимое правило конфликтов намеренно консервативное.

Два sibling managed subagent конфликтуют, если их `write_set` пересекаются по:

- точному совпадению resource reference; или
- цепочке ancestor/descendant после path-style normalization.

Это делает первый slice безопасным, даже если позже более богатая policy сможет разрешать часть таких случаев.

## Почему это уже считается настоящим managed layer

До этой фазы в executable системе были только:

- обычная рекурсия `orchestrator_agent`; и
- docs для richer managed orchestration.

После этой фазы в executable системе дополнительно есть:

- отдельный managed-subagent service в Core;
- отдельная managed state surface;
- явный worker-role task-package snapshot;
- durable parent/child lineage для managed launches;
- явная close-semantics, отделенная от plain child-run completion.

Этого уже достаточно, чтобы честно говорить о первом реальном managed-layer.

Этого все еще недостаточно, чтобы заявлять полное завершение всей широкой модели Phase 16.
