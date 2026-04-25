[English](#english) | [Русский](#russian)

<a id="english"></a>
# English

# Execution

This section owns the runtime behavior of graph execution. It explains how a run starts, how nodes are invoked, how edges are evaluated, how outputs become execution data, and how terminal outcomes are produced.

## Document Owners

- [`graph-execution.md`](./graph-execution.md) owns the execution order, node dispatch loop, edge selection, and terminal run behavior.
- [`subagent-task-lifecycle.md`](./subagent-task-lifecycle.md) owns delegated child-run sequencing, worker/reviewer/final-review transitions, retry and escalation rules, parallel-vs-sequential semantics, and how later Codex-native review or thread helpers fit without taking ownership away from Core.
- [`dataflow-and-input-resolution.md`](./dataflow-and-input-resolution.md) owns `params`, `vars`, node outputs, event payload access, and node input resolution.
- [`outputs-outcomes-and-final-response.md`](./outputs-outcomes-and-final-response.md) owns the explicit node `output` contract at execution time, node outcomes, and the final agent response policy.

## Section Boundaries

- Portable file contracts belong to [`03-contracts`](../03-contracts/README.md), especially [`agent-json`](../03-contracts/agent-json/README.md).
- Persistence, chat state, resume state, local storage, and crash recovery belong to [`05-state`](../05-state/README.md).
- User-visible live interaction during a run belongs to [`06-interaction`](../06-interaction/README.md).
- Runtime adapter architecture belongs to [`02-architecture/runtime-integration-model.md`](../02-architecture/runtime-integration-model.md).
- Managed child-run sequencing belongs to [`subagent-task-lifecycle.md`](./subagent-task-lifecycle.md); that document refines the architecture owner without replacing it.
- Draft/live/deploy and revision lifecycle belong to [`07-lifecycle`](../07-lifecycle/README.md).
- The canonical source above this section remains [`agent_orchestrator_final_spec_v2.md`](../../agent_orchestrator_final_spec_v2.md).

## Reading Order

1. Read [`graph-execution.md`](./graph-execution.md) for the control-flow model.
2. Read [`subagent-task-lifecycle.md`](./subagent-task-lifecycle.md) for delegated child-run sequencing, review loops, and the staged role of later Codex-native review or thread primitives.
3. Read [`dataflow-and-input-resolution.md`](./dataflow-and-input-resolution.md) for what data a node can actually see.
4. Read [`outputs-outcomes-and-final-response.md`](./outputs-outcomes-and-final-response.md) for how execution results are validated and surfaced.
5. Read [`../05-state/chat-and-resume.md`](../05-state/chat-and-resume.md) immediately after this section if resume behavior matters.

This README is navigation only. It does not define rules by itself.

<a id="russian"></a>
# Русский

# Исполнение

Этот раздел владеет runtime-поведением исполнения графа. Здесь описывается, как стартует запуск, как вызываются ноды, как выбираются переходы, как outputs становятся данными исполнения и как формируются терминальные исходы.

## Документы-владельцы

- [`graph-execution.md`](./graph-execution.md) владеет порядком исполнения, циклом запуска нод, выбором edges и терминальным поведением run.
- [`subagent-task-lifecycle.md`](./subagent-task-lifecycle.md) владеет sequencing делегированных child-run, переходами worker/reviewer/final-review, правилами retry и escalation, parallel-vs-sequential семантикой и этапной ролью более поздних Codex-native review или thread-примитивов.
- [`dataflow-and-input-resolution.md`](./dataflow-and-input-resolution.md) владеет `params`, `vars`, outputs нод, доступом к payload события и разрешением входа ноды.
- [`outputs-outcomes-and-final-response.md`](./outputs-outcomes-and-final-response.md) владеет режимами output ноды, исходами ноды и политикой финального ответа агента.

## Границы раздела

- Переносимые файловые контракты относятся к [`03-contracts`](../03-contracts/README.md), прежде всего к [`agent-json`](../03-contracts/agent-json/README.md).
- Персистентность, состояние чатов, resume state, локальное хранилище и восстановление после сбоев относятся к [`05-state`](../05-state/README.md).
- Пользовательское live-взаимодействие во время run относится к [`06-interaction`](../06-interaction/README.md).
- Архитектурная граница runtime adapters относится к [`02-architecture/runtime-integration-model.md`](../02-architecture/runtime-integration-model.md).
- Managed child-run sequencing относится к [`subagent-task-lifecycle.md`](./subagent-task-lifecycle.md); этот документ уточняет architecture owner, а не заменяет его.
- Draft/live/deploy и жизненный цикл ревизий относятся к [`07-lifecycle`](../07-lifecycle/README.md).
- Канонический источник над этим разделом остается [`agent_orchestrator_final_spec_v2.md`](../../agent_orchestrator_final_spec_v2.md).

## Порядок чтения

1. Сначала читайте [`graph-execution.md`](./graph-execution.md) для модели control flow.
2. Затем читайте [`subagent-task-lifecycle.md`](./subagent-task-lifecycle.md) для sequencing делегированных child-run, review loops и этапной роли более поздних Codex-native review или thread-примитивов.
3. Затем читайте [`dataflow-and-input-resolution.md`](./dataflow-and-input-resolution.md) для понимания того, какие данные вообще видит нода.
4. Затем читайте [`outputs-outcomes-and-final-response.md`](./outputs-outcomes-and-final-response.md) для правил валидации и публикации результатов исполнения.
5. Если важен resume, сразу переходите к [`../05-state/chat-and-resume.md`](../05-state/chat-and-resume.md).

Этот README выполняет только навигационную роль и сам по себе норм не задает.
