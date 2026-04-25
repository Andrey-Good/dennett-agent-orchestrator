[English](#english) | [Русский](#russian)

<a id="english"></a>
# English

# Subagent Context And Memory

Status: normative owner for managed child-run lineage and persistence boundaries.

Owns: child-run lineage records; explicit context-inheritance records; attempt, review, and acceptance state categories; and persistence prohibitions for managed subagent work.

Does not own: execution sequencing, MCP payload shape, or the semantics of memory bindings and runtime sources themselves.

Related documents:

- [Subagent Orchestration Model](../02-architecture/subagent-orchestration-model.md)
- [Subagent MCP Contract](../03-contracts/subagent-mcp-contract.md)
- [Subagent Task Lifecycle](../04-execution/subagent-task-lifecycle.md)
- [State README](./README.md)
- [Chat and Resume](./chat-and-resume.md)
- [Local Storage Model](./local-storage-model.md)
- [Memory Bindings](../08-extensions/memory-bindings.md)
- [Runtime Sources](../08-extensions/runtime-sources.md)

This document records the orchestration facts needed to resume, inspect, or audit a managed child run.
It does not redefine the child run itself, and it does not turn local state into the source of truth for portable artifacts that are already owned elsewhere.

## 1. State Categories

Managed child-run state is grouped into explicit categories.

The state store may contain:

- lineage records;
- context-inheritance records;
- attempt records;
- review records;
- acceptance records;
- selection metadata for explicitly chosen memory bindings or runtime sources, when such selections are part of the run;
- budget accounting counters for nesting depth, total descendants per root task, outstanding sibling children, repair/review loops per subtask, and cumulative task-tree budget consumption.

These counters are the persisted enforcement surface for nested-spawn caps and are required to reject launches or resume runs without losing limit state.

The state store must not invent new implicit categories that let one child inherit the parent's whole working set by default.

## 2. Lineage Records

Lineage records identify how a child run relates back to the parent task tree.

At minimum, lineage records capture:

- the parent run identity;
- the parent task identity;
- the child run identity;
- the child role;
- the root task identity when the child belongs to a deeper tree;
- the sibling group or branch identity when parallel children exist;
- the launch order or ancestry position needed to reconstruct the tree.

Lineage records are about relationship, not hidden reasoning. They do not store chain-of-thought or internal tool traces.

## 3. Context-Inheritance Records

Context inheritance must be explicit and selective.

For a managed child, the persisted context record may include:

- the objective and acceptance criteria passed to the child;
- the explicit write set;
- the explicit read-context package;
- prohibitions and budget limits;
- any narrowed memory-binding or runtime-source selection metadata that was explicitly applied for the run.

The record must not imply that the child inherited the parent's whole thread, the parent's full workspace, or any undeclared memory state.

If the parent passed only summaries or references, the persisted record must preserve that fact rather than inflating the child context into a full copy.

## 4. Attempt, Review, And Acceptance Records

Managed child execution needs a small set of durable outcome records.

Attempt records capture:

- launch timestamp;
- terminal timestamp;
- normalized terminal outcome;
- close or cancellation reason;
- retry ordinal when a child is relaunched.

Review records capture:

- reviewer child identity;
- the finding summary or structured finding list;
- whether the parent treated the finding as valid;
- the repair-child link when a valid finding triggered repair;
- the final decision for that review cycle.

Acceptance records capture:

- the final declared payload that the parent accepted;
- the portable artifact reference, when the child produced one;
- the parent decision that closed the child boundary.

These records let the parent reconstruct what happened without storing hidden traces.

## 5. Memory And Runtime Selection Metadata

This document may store opaque selection metadata for a run when the parent explicitly chooses a memory binding or runtime source.

That metadata is only a reference record. The meaning of `memory_bindings` and `runtime_sources` stays owned by their extension docs.

This document does not copy the external memory content into local state by default, and it does not define how a runtime adapter resolves the selected source.

## 6. Persistence Prohibitions

Managed child persistence must not store:

- chain-of-thought;
- internal tool traces;
- hidden intermediate reasoning;
- full parent working sets by default;
- undeclared memory state;
- raw runtime-vendor transcripts or secret session logs;
- automatic user chat routed through the child boundary;
- any data whose only purpose would be to reconstruct the child's private prompt history.

If a future extension needs more history, it must define that separately instead of widening this document by implication.

## 7. Resume Boundary

This document works with [Chat and Resume](./chat-and-resume.md), but it does not redefine resume policy.

If a managed child can be resumed, the resume path must restore the same bounded context package and lineage record, not a widened parent workspace.

The portable artifact remains canonical where another owner document already defines it. This document stores references and audit facts needed for continuity, not a second source of truth for the artifact itself.

## 8. Cross-Links

- The orchestration model lives in [Subagent Orchestration Model](../02-architecture/subagent-orchestration-model.md).
- The launch contract lives in [Subagent MCP Contract](../03-contracts/subagent-mcp-contract.md).
- The execution sequence lives in [Subagent Task Lifecycle](../04-execution/subagent-task-lifecycle.md).
- Local chat and resume rules live in [Chat and Resume](./chat-and-resume.md).

<a id="russian"></a>
# Русский

# Контекст И Memory Subagent

Статус: нормативный владелец lineage и persistence boundaries для managed child-run.

Владеет: записями child-run lineage; явными записями context inheritance; категориями attempt, review и acceptance state; и запретами на хранение для managed subagent work.

Не владеет: sequencing исполнения, формой MCP payload или семантикой memory bindings и runtime sources как таковых.

Связанные документы:

- [Модель оркестрации subagent](../02-architecture/subagent-orchestration-model.md)
- [Контракт MCP для Subagent](../03-contracts/subagent-mcp-contract.md)
- [Жизненный цикл subagent-задач](../04-execution/subagent-task-lifecycle.md)
- [State README](./README.md)
- [Chat and Resume](./chat-and-resume.md)
- [Local Storage Model](./local-storage-model.md)
- [Memory Bindings](../08-extensions/memory-bindings.md)
- [Runtime Sources](../08-extensions/runtime-sources.md)

Этот документ фиксирует orchestration facts, которые нужны, чтобы resume, inspect или audit managed child run.
Он не переопределяет сам child run и не делает local state источником истины для portable artifacts, которые уже принадлежат другому owner document.

## 1. Категории State

Managed child-run state группируется в явные категории.

State store может содержать:

- lineage records;
- context-inheritance records;
- attempt records;
- review records;
- acceptance records;
- selection metadata для явно выбранных memory bindings или runtime sources, когда такие selections входят в run;
- budget accounting counters для nesting depth, total descendants per root task, outstanding sibling children, repair/review loops для подзадачи и cumulative task-tree budget consumption.

Эти counters являются persisted enforcement surface для nested-spawn caps и нужны, чтобы отклонять launches или возобновлять runs без потери limit state.

State store не должен изобретать новые неявные категории, которые позволят одному child по умолчанию наследовать весь working set parent.

## 2. Записи Lineage

Lineage records показывают, как child run связан с parent task tree.

Как минимум lineage records фиксируют:

- identity parent run;
- identity parent task;
- identity child run;
- child role;
- root task identity, если child находится внутри более глубокого tree;
- sibling group или branch identity, когда существуют parallel children;
- launch order или ancestry position, необходимые для восстановления tree.

Lineage records описывают relation, а не hidden reasoning. Они не хранят chain-of-thought или internal tool traces.

## 3. Записи Context-Inheritance

Context inheritance должна быть явной и selective.

Для managed child persisted context record может включать:

- objective и acceptance criteria, переданные child;
- явный write set;
- явный read-context package;
- prohibitions и budget limits;
- любые narrowed memory-binding или runtime-source selection metadata, которые были явно применены для run.

Запись не должна подразумевать, что child унаследовал весь thread parent, полный workspace parent или любую необъявленную memory state.

Если parent передал только summaries или references, persisted record должен сохранять этот факт, а не превращать child context в полную копию.

## 4. Записи Attempt, Review и Acceptance

Managed child execution нуждается в небольшом наборе durable outcome records.

Attempt records фиксируют:

- launch timestamp;
- terminal timestamp;
- normalized terminal outcome;
- close или cancellation reason;
- retry ordinal, если child запускается повторно.

Review records фиксируют:

- reviewer child identity;
- summary findings или structured finding list;
- считал ли parent finding valid;
- repair-child link, если valid finding запустил repair;
- final decision для этого review cycle.

Acceptance records фиксируют:

- final declared payload, который принял parent;
- reference на portable artifact, если child его произвел;
- decision parent, закрывшее child boundary.

Эти записи позволяют parent восстановить, что произошло, без хранения hidden traces.

## 5. Metadata Memory И Runtime Selection

Этот документ может хранить opaque selection metadata для run, если parent явно выбрал memory binding или runtime source.

Такие metadata являются только reference record. Смысл `memory_bindings` и `runtime_sources` остается во владении их extension docs.

Этот документ не копирует внешнее содержимое memory в local state по умолчанию и не определяет, как runtime adapter разрешает выбранный source.

## 6. Запреты На Хранение

Managed child persistence не должна хранить:

- chain-of-thought;
- internal tool traces;
- hidden intermediate reasoning;
- full parent working sets по умолчанию;
- необъявленное memory state;
- raw runtime-vendor transcripts или secret session logs;
- automatic user chat, routed через child boundary;
- любые данные, единственная цель которых - восстановить private prompt history child.

Если будущему расширению понадобится больше history, оно должно определить это отдельно, а не расширять этот документ по умолчанию.

## 7. Граница Resume

Этот документ работает вместе с [Chat and Resume](./chat-and-resume.md), но не переопределяет policy resume.

Если managed child можно resume-ить, путь resume должен восстановить тот же bounded context package и lineage record, а не расширенный parent workspace.

Portable artifact остается canonical там, где другой owner document уже определил его. Этот документ хранит ссылки и audit facts, нужные для continuity, а не вторую source of truth для самого artifact.

## 8. Перекрестные Ссылки

- Orchestration model находится в [Модели оркестрации subagent](../02-architecture/subagent-orchestration-model.md).
- Launch contract находится в [Контракте MCP для Subagent](../03-contracts/subagent-mcp-contract.md).
- Execution sequence находится в [Жизненном цикле subagent-задач](../04-execution/subagent-task-lifecycle.md).
- Local chat и resume rules находятся в [Chat and Resume](./chat-and-resume.md).
