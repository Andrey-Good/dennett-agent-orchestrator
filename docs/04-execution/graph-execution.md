[English](#english) | [Русский](#russian)

<a id="english"></a>
# English

# Graph Execution

Status: normative.

Related documents:

- [`README.md`](./README.md)
- [`dataflow-and-input-resolution.md`](./dataflow-and-input-resolution.md)
- [`outputs-outcomes-and-final-response.md`](./outputs-outcomes-and-final-response.md)
- [`../05-state/chat-and-resume.md`](../05-state/chat-and-resume.md)
- [`../02-architecture/runtime-integration-model.md`](../02-architecture/runtime-integration-model.md)
- [`../../agent_orchestrator_final_spec_v2.md`](../../agent_orchestrator_final_spec_v2.md)

## 1. Scope

This document defines how one run traverses one agent graph.

It owns:

- run start from `entry_node_id`;
- sequential node dispatch;
- when edge conditions are evaluated;
- when graph execution stops;
- how cancellation and interruption terminate a run;
- what execution behavior is forbidden in the base model.

It does not own:

- field-by-field JSON contract rules from [`../03-contracts`](../03-contracts/README.md);
- persistence layout and resume storage from [`../05-state`](../05-state/README.md);
- live user interaction routing from [`../06-interaction`](../06-interaction/README.md).

## 2. Base Execution Model

The base graph model is strictly sequential.

Mandatory rules:

- A run has exactly one active node at a time.
- A run always starts at `entry_node_id`.
- The orchestrator advances from the current node to at most one next node.
- Parallel execution of multiple nodes is not part of the base model.
- Automatic retries are not part of the base model.
- Scheduler behavior, worker pools, and queue semantics are not part of the base model.

The graph definition comes from the resolved agent JSON revision chosen for that run. Local metadata may index or cache that graph, but it must not redefine execution semantics.

## 3. Preconditions Before Dispatch

Before the first node is invoked, core must reject the run if any mandatory precondition is false.

At minimum, the run must not start when:

- `graph_contract_version` is unsupported;
- `entry_node_id` does not resolve to an existing node;
- an edge references a missing `from` or `to` node;
- a node violates the canonical kind-specific contract;
- a node omits `output`;
- a direct contract violation would force the orchestrator to guess missing behavior.

Pre-run validation is allowed to be stricter than runtime dispatch. It is not allowed to be looser than the canonical contract.

## 4. Dispatch Boundary of a Node

A node is only a call boundary. It is not an invitation for the orchestrator to inspect agent internals.

For each node invocation, core must determine:

- which node is active;
- whether it is `runtime_agent` or `orchestrator_agent`;
- which runtime adapter or resolved child agent revision is targeted;
- the resolved node input defined in [`dataflow-and-input-resolution.md`](./dataflow-and-input-resolution.md);
- the effective skills, MCPs, plugins, permissions, and runtime options allowed for that invocation;
- the declared `output` contract.

For `orchestrator_agent`, this includes resolving `agent_ref` from logical agent identity to the child revision selected by the lifecycle live-resolution rules before the child run is launched.

For `orchestrator_agent`, graph execution owns only the portable child-run boundary: resolve the child revision, pass resolved node input, wait for the child's terminal return, and classify the parent node from that return. In the base contract, the resolved node input is delivered to the child through the child run's standard invocation params as `params.input`, not through `event.launch_note`. It does not, by itself, create managed subagent task-package semantics such as `write_set`, `read_context`, reviewer roles, or `subagent.send` / `subagent.close` behavior.

In the base model, a child run launched through `orchestrator_agent` is interaction-silent from the parent/user point of view: Core must not surface the child run's live comments or the child run's `orchestrator.user_chat` channel through the parent run. If the selected child revision would require nested surfaced live interaction, Core must reject the node before child launch as incompatible with the base model.

Core must not:

- inspect chain-of-thought;
- require tool traces as graph data;
- infer hidden inputs not declared by the node;
- mutate the graph definition in response to runtime behavior.

## 5. Run Loop

The normative run loop is:

1. Initialize immutable run facts from the resolved agent revision selected for this run, including the resolved revision identity, invocation parameters, and optional normalized event envelope.
2. Set `current_node_id = entry_node_id`.
3. Resolve the current node input against the current committed data snapshot.
4. Invoke the node through the appropriate runtime or child-run boundary.
5. Classify the node attempt into exactly one node outcome.
6. If the outcome is `success`, commit the node result and any allowed derived state updates.
7. Evaluate outgoing edges of the current node in stored order.
8. If the first matching edge exists, move to its target node and continue.
9. If no matching edge exists, terminate the graph successfully.
10. If the outcome is not `success`, terminate the run with that terminal condition.

There is no branch fan-out, no speculative execution, and no implicit retry inside this loop.

For an `orchestrator_agent` node, step 4 launches one child run from the resolved child revision, and step 5 classifies the parent node only from the child run's terminal outcome and run-level final-response payload as defined in [`outputs-outcomes-and-final-response.md`](./outputs-outcomes-and-final-response.md).
A graph run may therefore recurse through `orchestrator_agent` without entering managed subagent orchestration. If another owner surface internally realizes a managed subagent through graph recursion, that mapping is below this document and must not change node semantics here.

## 6. Edge Evaluation

Edges own only control flow.

Edges are responsible for:

- execution order;
- next-node selection;
- transition conditions.

Edges are not responsible for:

- passing payloads;
- copying outputs into inputs;
- mutating `vars`;
- hiding side channels between nodes.

Evaluation rules:

- Outgoing edges of one node are checked in the order they appear in `edges`.
- Each condition is evaluated only after the current node has already produced a committed `success`.
- The condition context is limited to `params`, `vars`, `node`, and `event`.
- The first condition that evaluates to `true` wins.
- An edge without `condition` is unconditional.
- If no edge matches, graph execution stops after the current successful node.

An edge condition must never be treated as data transport.

## 7. Terminal Behavior

The graph stops in exactly one of the following ways:

- successful completion because the last successful node has no matching next edge;
- failure after `invalid_output`;
- failure after `runtime_error`;
- explicit cancellation resulting in `cancelled`;
- external shutdown resulting in `interrupted`.

Base-model consequences:

- `success` may advance to another node or finish the graph.
- `invalid_output` terminates the run immediately.
- `runtime_error` terminates the run immediately.
- `cancelled` terminates the run immediately.
- `interrupted` terminates the run immediately.

There is no implicit fallback edge for non-success outcomes.

## 8. Cancellation and Interruption

`cancelled` and `interrupted` are different outcomes and must stay different in storage and UI.

Mandatory interpretation:

- `cancelled` means explicit user intent to stop the run.
- `interrupted` means the run stopped because core or the interface terminated it from the outside.

If the interface policy is `stop_core`, active runs become `interrupted` when shutdown reaches core. The next application start must not auto-resume them. Any continuation later is still an explicit resume action governed by [`../05-state/chat-and-resume.md`](../05-state/chat-and-resume.md).

## 9. Resume Boundary for Execution

Execution and state have separate responsibilities.

Execution owns these invariants:

- resume must not reinterpret the graph contract;
- resume must continue against the same resolved revision identity captured when the run began;
- resume must not invent a new next node;
- resume must not skip a durably recorded blocking built-in user-chat prompt;
- resume must not fabricate a successful node outcome that was never durably committed;
- resume must not silently assume that a sensitive parameter change, such as a later-mapped model selection, is compatible with the existing chat or native-resume context;
- resume must not auto-start after restart.

State owns the persistence mechanics that make these invariants possible. See [`../05-state/chat-and-resume.md`](../05-state/chat-and-resume.md) and [`../05-state/local-storage-model.md`](../05-state/local-storage-model.md).

## 10. Forbidden Behaviors

The following behaviors are forbidden in the base execution model:

- parallel node execution inside one run;
- automatic retry of a failed node;
- automatic retry of a failed trigger-run;
- implicit data transfer through edges;
- hidden prompt augmentation to force JSON output;
- treating local storage as the source of truth for graph structure;
- inventing managed subagent task-package semantics for a plain `orchestrator_agent` node;
- inferring unconfigured tools, permissions, or runtime capabilities.

## 11. Acceptance Criteria for an Implementation

An implementation conforms to this document only if:

- one run dispatches at most one node at a time;
- node transition order is deterministic from the stored `edges` order and committed data snapshot;
- every terminal run state is explained by the last committed node outcome or by successful graph exhaustion;
- non-success node outcomes do not silently continue execution;
- shutdown never turns into auto-resume on the next application start.

<a id="russian"></a>
# Русский

# Исполнение графа

Статус: нормативный.

Связанные документы:

- [`README.md`](./README.md)
- [`dataflow-and-input-resolution.md`](./dataflow-and-input-resolution.md)
- [`outputs-outcomes-and-final-response.md`](./outputs-outcomes-and-final-response.md)
- [`../05-state/chat-and-resume.md`](../05-state/chat-and-resume.md)
- [`../02-architecture/runtime-integration-model.md`](../02-architecture/runtime-integration-model.md)
- [`../../agent_orchestrator_final_spec_v2.md`](../../agent_orchestrator_final_spec_v2.md)

## 1. Область действия

Этот документ определяет, как один run проходит один граф агента.

Он владеет:

- стартом run из `entry_node_id`;
- последовательным запуском нод;
- моментом вычисления условий edges;
- моментом остановки графа;
- тем, как отмена и прерывание завершают run;
- перечнем запрещенного поведения базовой модели.

Он не владеет:

- правилами JSON-контракта по полям из [`../03-contracts`](../03-contracts/README.md);
- layout персистентности и resume-хранилища из [`../05-state`](../05-state/README.md);
- маршрутизацией live-общения с пользователем из [`../06-interaction`](../06-interaction/README.md).

## 2. Базовая модель исполнения

Базовая модель графа строго последовательная.

Обязательные правила:

- В один момент времени у run есть ровно одна активная нода.
- Run всегда начинается с `entry_node_id`.
- Оркестратор переходит от текущей ноды не более чем к одной следующей.
- Параллельное исполнение нескольких нод не входит в базовую модель.
- Автоматические ретраи не входят в базовую модель.
- Scheduler-поведение, worker pools и семантика очередей не входят в базовую модель.

Определение графа приходит из той resolved agent JSON revision, которая была выбрана для данного run. Локальные метаданные могут индексировать или кэшировать этот граф, но не могут переопределять семантику исполнения.

## 3. Предусловия перед dispatch

До вызова первой ноды core обязан отклонить run, если хотя бы одно обязательное предусловие ложно.

Как минимум run нельзя стартовать, когда:

- `graph_contract_version` не поддерживается;
- `entry_node_id` не разрешается в существующую ноду;
- edge ссылается на отсутствующую `from` или `to` ноду;
- нода нарушает канонический контракт для своего kind;
- у ноды отсутствует `output`;
- прямое нарушение контракта заставило бы оркестратор угадывать недостающее поведение.

Pre-run validation может быть строже runtime dispatch, но не может быть слабее канонического контракта.

## 4. Граница вызова ноды

Нода является только границей вызова. Это не разрешение оркестратору смотреть внутрь агента.

Для каждого вызова ноды core обязан определить:

- какая нода активна;
- является ли она `runtime_agent` или `orchestrator_agent`;
- на какой runtime adapter или в какую разрешенную child revision направлен вызов;
- разрешенный вход ноды по [`dataflow-and-input-resolution.md`](./dataflow-and-input-resolution.md);
- effective skills, MCPs, plugins, permissions и runtime options для этого вызова;
- объявленный контракт `output`.

Для `orchestrator_agent` сюда входит разрешение `agent_ref` из логической идентичности агента в child revision, выбранную по lifecycle-правилам live-resolution, до запуска child run-а.

Для `orchestrator_agent` graph execution владеет только переносимой child-run boundary: разрешить child revision, передать resolved node input, дождаться terminal return child-а и классифицировать родительскую ноду по этому return. В базовом контракте resolved node input передается child-агенту через стандартные invocation params child run-а как `params.input`, а не через `event.launch_note`. Сам по себе этот execution layer не создает managed subagent task-package semantics вроде `write_set`, `read_context`, reviewer roles или поведения `subagent.send` / `subagent.close`.

В базовой модели child run, запущенный через `orchestrator_agent`, является interaction-silent с точки зрения пользователя parent run-а: Core не должен surface-ить live comments child run-а или built-in канал `orchestrator.user_chat` child run-а через parent run. Если выбранная child revision требует surfaced nested live interaction, Core обязан отклонить ноду до запуска child-а как несовместимую с базовой моделью.

Core не должен:

- смотреть chain-of-thought;
- требовать tool traces как данные графа;
- выводить скрытые входы, не объявленные нодой;
- менять определение графа в ответ на поведение runtime.

## 5. Цикл run

Нормативный цикл run выглядит так:

1. Инициализировать неизменяемые факты run из resolved agent revision, выбранной для этого run, включая resolved revision identity, параметры запуска и optional нормализованный event envelope.
2. Установить `current_node_id = entry_node_id`.
3. Разрешить вход текущей ноды относительно текущего зафиксированного snapshot данных.
4. Вызвать ноду через соответствующую runtime- или child-run boundary.
5. Классифицировать попытку ноды ровно в один node outcome.
6. Если outcome равен `success`, зафиксировать результат ноды и все разрешенные производные обновления состояния.
7. Вычислить outgoing edges текущей ноды в порядке хранения.
8. Если первый подходящий edge существует, перейти в его target node и продолжить.
9. Если подходящего edge нет, успешно завершить граф.
10. Если outcome не равен `success`, завершить run с этим терминальным состоянием.

Внутри этого цикла нет branch fan-out, speculative execution и implicit retry.

Для ноды `orchestrator_agent` шаг 4 запускает ровно один child run из разрешенной child revision, а шаг 5 классифицирует родительскую ноду только по terminal outcome child run-а и его run-level payload финального ответа согласно [`outputs-outcomes-and-final-response.md`](./outputs-outcomes-and-final-response.md).
Следовательно, graph run может рекурсивно проходить через `orchestrator_agent`, не входя в managed subagent orchestration. Если другой owner surface внутренне реализует managed subagent через graph recursion, это отображение находится ниже уровня этого документа и не должно менять семантику ноды здесь.

## 6. Вычисление edges

Edges владеют только control flow.

Edges отвечают за:

- порядок исполнения;
- выбор следующей ноды;
- условия перехода.

Edges не отвечают за:

- передачу payload;
- копирование outputs во inputs;
- мутацию `vars`;
- скрытые side channels между нодами.

Правила вычисления:

- Outgoing edges одной ноды проверяются в том порядке, в котором записаны в `edges`.
- Каждое condition вычисляется только после того, как текущая нода уже дала зафиксированный `success`.
- Контекст condition ограничен `params`, `vars`, `node` и `event`.
- Побеждает первое condition, вернувшее `true`.
- Edge без `condition` считается безусловным.
- Если ни один edge не подошел, исполнение графа останавливается после текущей успешной ноды.

Condition edge никогда не должно трактоваться как канал передачи данных.

## 7. Терминальное поведение

Граф останавливается ровно одним из следующих способов:

- успешное завершение, потому что у последней успешной ноды нет подходящего следующего edge;
- ошибка после `invalid_output`;
- ошибка после `runtime_error`;
- явная отмена с результатом `cancelled`;
- внешнее завершение с результатом `interrupted`.

Последствия в базовой модели:

- `success` может либо перевести граф к следующей ноде, либо завершить граф.
- `invalid_output` немедленно завершает run.
- `runtime_error` немедленно завершает run.
- `cancelled` немедленно завершает run.
- `interrupted` немедленно завершает run.

Неявного fallback edge для non-success outcomes не существует.

## 8. Отмена и прерывание

`cancelled` и `interrupted` являются разными outcome и должны оставаться разными в storage и UI.

Обязательная интерпретация:

- `cancelled` означает явное намерение пользователя остановить run.
- `interrupted` означает, что run был остановлен внешним завершением core или интерфейса.

Если policy интерфейса равна `stop_core`, активные runs становятся `interrupted`, когда shutdown доходит до core. Следующий старт приложения не должен автоматически их resume-ить. Любое продолжение позже все равно является явным resume-действием по правилам [`../05-state/chat-and-resume.md`](../05-state/chat-and-resume.md).

## 9. Граница resume для исполнения

Execution и state имеют разные ответственности.

Execution владеет следующими инвариантами:

- resume не может переинтерпретировать контракт графа;
- resume обязан продолжать работу на той же resolved revision identity, которая была зафиксирована при старте run;
- resume не может придумывать новую следующую ноду;
- resume не может перескакивать через durably recorded blocking built-in user-chat prompt;
- resume не может сфабриковать успешный исход ноды, который не был durably committed;
- resume не может стартовать автоматически после рестарта.

State владеет механикой персистентности, которая делает эти инварианты возможными. См. [`../05-state/chat-and-resume.md`](../05-state/chat-and-resume.md) и [`../05-state/local-storage-model.md`](../05-state/local-storage-model.md).

## 10. Запрещенное поведение

В базовой модели исполнения запрещено:

- параллельное исполнение нод внутри одного run;
- автоматический retry неуспешной ноды;
- автоматический retry неуспешного trigger-run;
- неявная передача данных через edges;
- скрытая модификация prompt ради принудительного JSON output;
- использование local storage как источника истины для структуры графа;
- изобретение managed subagent task-package semantics для обычной ноды `orchestrator_agent`;
- вывод неописанных tools, permissions или runtime capabilities.

## 11. Критерии приемки реализации

Реализация соответствует этому документу только если:

- один run запускает не более одной ноды одновременно;
- порядок переходов между нодами детерминируется stored order в `edges` и зафиксированным snapshot данных;
- каждое терминальное состояние run объясняется последним committed node outcome или успешным исчерпанием графа;
- non-success node outcomes не продолжают исполнение молча;
- shutdown никогда не превращается в auto-resume при следующем старте приложения.
