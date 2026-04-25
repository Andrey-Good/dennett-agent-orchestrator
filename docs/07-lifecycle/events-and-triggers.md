[English](#english) | [Русский](#russian)

# English

## Events and Triggers

Status: normative owner for external event launch semantics.

Related documents:

- [Canonical spec](../../agent_orchestrator_final_spec_v2.md)
- [Draft, Live, and Deploy](./draft-live-deploy.md)
- [Agent Registry](./agent-registry.md)
- [Execution](../04-execution/README.md)
- [Interaction](../06-interaction/README.md)

## Trigger vs Event

A trigger is the source that can emit launches. An event is the concrete launch request materialized from one trigger firing.

In other words:

- a trigger is durable configuration or a durable external source;
- an event is one immutable occurrence handled by Core.

This distinction matters because triggers may survive for a long time, while each event creates at most one run attempt in the base model.

## Ownership Boundary

Triggers and events live outside the portable agent file.

The agent file may define how a run behaves once it starts, but it does not own:

- which external source can start that run;
- when a trigger fires;
- what event history already happened;
- whether a particular event succeeded or failed.

Those facts belong to the local lifecycle surface.

## Minimum Event Shape

An event should carry at least these logical parts:

- a trigger reference that explains where it came from;
- a logical agent reference that identifies which agent should run;
- an optional payload that, when present, becomes `event.payload` in the graph-visible run data model;
- an optional start prompt or launch note that, when present, becomes `event.launch_note` in the graph-visible run data model;
- immutable local metadata such as creation time and dispatch outcome.

The exact storage schema is implementation-specific, but these meanings are not.

## Dispatch Rules

The base dispatch rule is simple:

- one event creates one run attempt;
- if that run fails, Core does not automatically retry it.

There is no built-in auto-retry policy, scheduler, worker queue, or priority system in the base architecture.

## Agent Resolution at Dispatch Time

Events bind to the logical agent identity, not to drafts.

When an event is dispatched, Core should:

1. resolve the logical agent to the current live revision;
2. verify that the live revision is available and supported;
3. launch one run from that resolved revision.

This means a later deploy affects future events, not the run that already started from an earlier event.

## Event Payload Semantics

The event payload is run input, not hidden mutable state.

It may be referenced by the graph through `event.payload` and `event.payload.<path>`, but it must not:

- rewrite the agent file;
- mutate stored drafts or live revisions;
- silently become long-term memory;
- bypass contract validation by introducing undeclared lifecycle fields.

Once the event is materialized, its payload should be treated as immutable historical input for that run.

## Graph-Visible Event Surface

The local event record and the graph-visible `event` data space are related but not identical.

For graph execution, Core exposes one immutable normalized envelope with these reserved top-level keys:

- `event.payload`: the trigger payload exactly as accepted for that event, when present;
- `event.launch_note`: the optional start prompt or launch note string, when present.

Consequences:

- payload keys are never flattened directly under `event`;
- the launch note is never hidden-injected into a node prompt outside `event.launch_note`;
- trigger references, logical agent references, timestamps, dispatch outcome, storage ids, and other local lifecycle metadata do not automatically appear inside the graph-visible `event` namespace unless a future specification adds them explicitly.

## Start Prompt Semantics

An event may carry an optional start prompt or launch note when the trigger source needs to add operator intent at launch time.

This field is still launch input, not a patch to the agent definition. In the graph-visible run data model it appears only as `event.launch_note`. Core must not inject it through a hidden prompt channel or merge it into `event.payload`.

## Failure Handling

Core should fail fast and visibly in these cases:

- the logical agent has no current live revision;
- the current live revision is missing or invalid;
- the resolved agent uses an unsupported `graph_contract_version`;
- the runtime launch fails.

In all of these cases, the event is recorded as failed or rejected. Core must not silently substitute another draft or auto-replay the event.

## Idempotency and Deduplication

The base architecture does not define a global idempotency or deduplication framework for triggers.

If a specific trigger implementation needs deduplication, it should provide it as trigger-specific behavior or a future extension. The lifecycle model here only requires that each accepted event instance has one explicit dispatch outcome.

## Relationship to Comments and Live Interaction

Once a run starts from an event, live comments and the built-in MCP user channel follow the same runtime interaction rules as any other run.

This document governs only how the run starts, not how human interaction is routed after dispatch.

## What Is Explicitly Out of Scope

This document does not define:

- a mandatory scheduler;
- background workers as a required deployment model;
- automatic retries;
- priority queues;
- multi-event batching;
- guaranteed exactly-once delivery across all possible trigger implementations.

Those capabilities require separate extensions or later architecture decisions.

# Russian

## События и триггеры

Статус: нормативный владелец семантики внешнего запуска через события.

Связанные документы:

- [Каноническая спецификация](../../agent_orchestrator_final_spec_v2.md)
- [Draft, Live и Deploy](./draft-live-deploy.md)
- [Реестр агентов](./agent-registry.md)
- [Исполнение](../04-execution/README.md)
- [Взаимодействие](../06-interaction/README.md)

## Trigger и Event

Триггер — это источник, который может испускать запуски. Событие — это конкретный запрос на запуск, материализованный из одного срабатывания триггера.

Иными словами:

- trigger — это долговременная конфигурация или долговременный внешний источник;
- event — это одно неизменяемое occurrence, которое обрабатывает Core.

Это различие важно, потому что триггеры могут жить долго, тогда как каждое событие в базовой модели создает не более одной попытки run-а.

## Граница владения

Триггеры и события живут вне переносимого agent file.

Agent file может определять, как ведет себя run после старта, но не владеет:

- тем, какой внешний источник может запустить этот run;
- моментом, когда срабатывает trigger;
- историей уже произошедших событий;
- фактом успеха или неуспеха конкретного события.

Эти факты относятся к локальной lifecycle-поверхности.

## Минимальная форма события

Событие должно нести как минимум следующие логические части:

- ссылку на trigger, объясняющую его происхождение;
- ссылку на логического агента, определяющую, какой агент нужно запустить;
- optional payload, который при наличии становится `event.payload` в graph-visible модели данных run-а;
- optional start prompt или launch note, который при наличии становится `event.launch_note` в graph-visible модели данных run-а;
- неизменяемые локальные метаданные, такие как время создания и исход dispatch.

Точная схема хранения зависит от реализации, но эти смыслы не меняются.

## Правила dispatch

Базовое правило dispatch предельно простое:

- одно событие создает одну попытку run-а;
- если этот run завершился неудачно, Core не выполняет автоматический retry.

Во встроенной архитектуре нет auto-retry policy, scheduler, worker queue или priority system.

## Разрешение агента в момент dispatch

События привязываются к логической идентичности агента, а не к drafts.

Когда событие dispatch-ится, Core должен:

1. разрешить логического агента в текущую live-ревизию;
2. убедиться, что live-ревизия доступна и поддерживается;
3. запустить один run из этой разрешенной ревизии.

Это означает, что более поздний deploy влияет на будущие события, а не на run, уже запущенный прежним событием.

## Семантика payload события

Payload события является входом run-а, а не скрытым изменяемым состоянием.

На него можно ссылаться через `event.payload` и `event.payload.<path>`, но он не должен:

- переписывать agent file;
- мутировать сохраненные drafts или live-ревизии;
- молча становиться долговременной memory;
- обходить валидацию контракта путем добавления необъявленных lifecycle-полей.

После материализации события его payload должен трактоваться как неизменяемый исторический input этого run-а.

## Graph-visible поверхность события

Локальная запись события и graph-visible пространство данных `event` связаны, но не тождественны.

Для исполнения графа Core открывает один нормализованный неизменяемый envelope со следующими зарезервированными top-level ключами:

- `event.payload`: payload trigger-а в точности в том виде, в каком он был принят для этого события, если он присутствует;
- `event.launch_note`: optional строка start prompt или launch note, если она присутствует.

Следствия:

- ключи payload никогда не схлопываются напрямую под `event`;
- launch note никогда не внедряется скрыто в prompt ноды вне `event.launch_note`;
- ссылки на trigger, ссылки на логического агента, timestamps, исход dispatch, storage ids и другие локальные lifecycle-метаданные не появляются автоматически внутри graph-visible namespace `event`, если только будущая спецификация не добавит их явно.

## Семантика start prompt

Событие может нести optional start prompt или launch note, когда trigger-источнику нужно добавить намерение оператора в момент запуска.

Это поле тоже является входом запуска, а не патчем определения агента. В graph-visible модели данных run-а оно появляется только как `event.launch_note`. Core не должен внедрять его через скрытый prompt-channel или смешивать его с `event.payload`.

## Обработка отказов

Core должен быстро и явно отказывать в следующих случаях:

- у логического агента нет текущей live-ревизии;
- текущая live-ревизия отсутствует или невалидна;
- разрешенный агент использует неподдерживаемый `graph_contract_version`;
- запуск runtime завершается ошибкой.

Во всех этих случаях событие фиксируется как failed или rejected. Core не должен молча подставлять другой draft или автоматически переигрывать событие.

## Идемпотентность и дедупликация

Базовая архитектура не определяет глобальный фреймворк идемпотентности или дедупликации для triggers.

Если конкретной реализации trigger нужна дедупликация, она должна предоставляться как trigger-specific behavior или будущим расширением. Lifecycle-модель здесь требует только того, чтобы у каждого принятого экземпляра события был один явный dispatch outcome.

## Связь с комментариями и live-взаимодействием

После старта run-а из события live-comments и встроенный MCP-канал пользователя подчиняются тем же runtime-правилам взаимодействия, что и любой другой run.

Этот документ управляет только тем, как run начинается, а не тем, как маршрутизируется человеческое взаимодействие после dispatch.

## Что явно вне scope

Этот документ не определяет:

- обязательный scheduler;
- фоновых workers как обязательную модель деплоя;
- автоматические retry;
- priority queues;
- пакетную обработку нескольких событий;
- гарантию exactly-once delivery для всех возможных реализаций trigger.

Такие возможности требуют отдельных расширений или последующих архитектурных решений.
