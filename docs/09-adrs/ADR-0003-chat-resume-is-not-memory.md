[English](#english) | [Русский](#russian)

# English

## ADR-0003: Chat Resume Is Not Memory

Status: Accepted
Date: 2026-04-21

Related normative documents:

- [State](../05-state/README.md)
- [Interaction](../06-interaction/README.md)
- [Memory Bindings](../08-extensions/memory-bindings.md)
- [Draft, Live, and Deploy](../07-lifecycle/draft-live-deploy.md)

## Context

The project needed two different capabilities that are easy to blur together:

- continuing an existing conversation or run;
- giving agents access to durable external context beyond the current chat.

Many systems collapse both into one storage concept, usually because it is convenient to say that "everything is memory." That convenience creates architectural ambiguity quickly: the same store starts trying to serve resumability, retrieval, personalization, and hidden state transfer all at once.

## Alternatives Considered

### Alternative 1: One unified memory system for chat, resume, and long-term context

This option looked simple conceptually, but it would have made it hard to tell whether a piece of context existed because of the current conversation, an explicit memory binding, or a hidden system heuristic. It also risked turning resume state into an implicit long-term knowledge base.

### Alternative 2: Separate chat/resume state from optional memory bindings

This option treated resumability as one concern and durable external context as another, even when a runtime might expose both through related mechanisms.

## Decision Rationale

The project chose the second option.

The separation keeps the architecture legible:

- chat and resume stay focused on continuity of an existing run or conversation;
- memory stays optional and declarative;
- extension-specific memory behavior can evolve without rewriting the state model.

This choice also fits the black-box runtime principle better, because the orchestrator does not need to pretend that every remembered thing came from the same subsystem.

## Consequences

Positive consequences:

- hidden state drift is reduced;
- memory access becomes explicit in the agent definition;
- chat storage can stay minimal and resume-oriented.

Accepted costs:

- some integrations expose related concepts through one vendor surface and still need clean separation in Core;
- users and builders need to think about whether a need is conversational continuity or durable external context;
- there is no shortcut where saved chat automatically becomes long-term memory.

## What This ADR Owns

This ADR owns the rationale for keeping chat/resume separate from memory bindings. It does not own the current storage rules or the current memory-binding semantics. Those rules live in the linked normative documents above.

# Russian

## ADR-0003: Chat Resume Is Not Memory

Статус: Принято
Дата: 2026-04-21

Связанные нормативные документы:

- [Состояние](../05-state/README.md)
- [Взаимодействие](../06-interaction/README.md)
- [Memory Bindings](../08-extensions/memory-bindings.md)
- [Draft, Live и Deploy](../07-lifecycle/draft-live-deploy.md)

## Контекст

Проекту нужны были две разные возможности, которые легко размыть между собой:

- продолжение существующего разговора или run-а;
- предоставление агентам долговременного внешнего контекста за пределами текущего чата.

Многие системы схлопывают обе возможности в одно понятие хранения, обычно потому, что удобно сказать: "все это и есть memory". Такая простота быстро создает архитектурную двусмысленность: одно и то же хранилище начинает одновременно обслуживать resumability, retrieval, personalization и скрытую передачу состояния.

## Рассмотренные альтернативы

### Альтернатива 1: Одна единая memory-system для chat, resume и long-term context

Этот вариант выглядел концептуально простым, но в нем было бы трудно понять, существует ли конкретный кусок контекста из-за текущего разговора, явного memory binding или скрытой системной эвристики. Он также рисковал превратить resume state в неявную долгосрочную knowledge base.

### Альтернатива 2: Разделить chat/resume state и optional memory bindings

Этот вариант рассматривал resumability как одну задачу, а долговременный внешний контекст как другую, даже если конкретный runtime может открывать обе через связанные механизмы.

## Мотивация решения

Проект выбрал второй вариант.

Такое разделение делает архитектуру читаемой:

- chat и resume остаются сфокусированными на непрерывности существующего run-а или разговора;
- memory остается optional и декларативной;
- extension-specific поведение памяти может развиваться, не переписывая state-модель.

Этот выбор также лучше согласуется с принципом black-box runtime, потому что оркестратору не приходится делать вид, будто вся запомненная информация пришла из одной и той же подсистемы.

## Последствия

Положительные последствия:

- уменьшается скрытый дрейф состояния;
- доступ к памяти становится явным в определении агента;
- хранение чата может оставаться минимальным и ориентированным на resume.

Осознанно принятые издержки:

- некоторые интеграции открывают связанные понятия через одну vendor-surface, и в Core их все равно нужно чисто разделять;
- пользователям и builder приходится различать, нужна ли им continuity разговора или durable external context;
- не существует shortcut, при котором сохраненный чат автоматически становится long-term memory.

## Чем владеет этот ADR

Этот ADR владеет мотивацией разделения chat/resume и memory bindings. Он не владеет текущими правилами хранения или текущей семантикой memory bindings. Эти правила живут в указанных выше нормативных документах.
