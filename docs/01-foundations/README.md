[English](#english) | [Русский](#russian)

## English

# Foundations

Status: approved foundations section map.  
Owns: section scope, document routing for foundational topics, and the rule of what must be decided here before lower-level design.  
Does not own: detailed architecture, machine-checkable contracts, or runtime-specific operational protocols.  
Primary sources: [documentation root](../README.md), [canonical specification](../../agent_orchestrator_final_spec_v2.md), [architecture section](../02-architecture/README.md).

This section is the floor under the rest of the specification set. Before architecture, contracts, execution flow, or lifecycle details can be stable, the project needs a fixed answer to a smaller set of questions:

- What the product is and is not.
- Where the orchestrator boundary ends and the runtime boundary begins.
- Which terms mean exactly what.
- Which artifact is authoritative for which class of information.
- Which laws are non-negotiable and which behaviors are defaults.
- Which implementation stack is locked for the repository.

If a future document changes the meaning of any of those answers, that change must be deliberate and visible. It is not a local clarification; it is a foundations change.

## Documents In This Section

| Document | Owns | Use it when you need to answer |
| --- | --- | --- |
| [`project-scope-and-non-goals.md`](./project-scope-and-non-goals.md) | Product scope, MVP boundary, explicit non-goals | "Should this capability exist in the base product at all?" |
| [`system-boundaries.md`](./system-boundaries.md) | Responsibility boundaries between orchestrator, runtime, interfaces, and storage | "Which layer should implement this behavior?" |
| [`glossary.md`](./glossary.md) | Canonical terms and naming discipline | "What do we call this concept in docs, code, and tests?" |
| [`source-of-truth-model.md`](./source-of-truth-model.md) | Authoritative artifact per information class | "Which artifact wins if two representations disagree?" |
| [`invariants-and-defaults.md`](./invariants-and-defaults.md) | Project laws and default behavior | "Is this behavior mandatory, forbidden, or merely default?" |
| [`technology-stack.md`](./technology-stack.md) | Locked implementation stack and dependency guardrails | "Which technologies are mandatory or forbidden for MVP implementation?" |

## What Must Be Decided Here

The foundations layer should answer questions that otherwise cause architecture drift everywhere else:

- Whether the product is an orchestrator or a new runtime.
- Whether an idea belongs in the minimal stable core, a later extension, or outside the product entirely.
- Whether a data store is canonical or merely derived.
- Whether a dependency choice is free, constrained, or already locked.
- Whether a term names a domain concept, an implementation detail, or an external ecosystem artifact.

## What Must Not Be Decided Only Here

The foundations section is intentionally upstream, but it is not a dumping ground for every rule:

- Detailed adapter protocols belong in [`02-architecture`](../02-architecture/README.md) and later focused docs.
- Formal schemas and exact field constraints belong in [`03-contracts`](../03-contracts/README.md); worked examples and anti-pattern illustrations belong in [`10-examples`](../10-examples/README.md).
- Execution-step rules belong in [`04-execution`](../04-execution/README.md).
- State persistence details belong in [`05-state`](../05-state/README.md).
- Lifecycle workflow details belong in [`07-lifecycle`](../07-lifecycle/README.md).
- Extension-specific contracts belong in [`08-extensions`](../08-extensions/README.md).

## Implementation Use

Before adding a module, dependency, or behavior, answer these questions in this order:

1. Is the idea in scope according to [`project-scope-and-non-goals.md`](./project-scope-and-non-goals.md)?
2. Which side of the system boundary should own it according to [`system-boundaries.md`](./system-boundaries.md)?
3. Which term should the code and tests use according to [`glossary.md`](./glossary.md)?
4. Which artifact becomes authoritative according to [`source-of-truth-model.md`](./source-of-truth-model.md)?
5. Is the behavior mandated or defaulted by [`invariants-and-defaults.md`](./invariants-and-defaults.md)?
6. Does the implementation fit the locked choices in [`technology-stack.md`](./technology-stack.md)?

If any answer is unclear, the correct move is to tighten the foundations docs first rather than letting code invent its own interpretation.

## Russian

# Foundations

Статус: утвержденная карта раздела foundations.  
Владеет: scope самого раздела, маршрутизацией по документам foundations и правилом, что должно быть решено здесь до нижележащего дизайна.  
Не владеет: детальной архитектурой, машинно-проверяемыми контрактами и runtime-специфичными операционными протоколами.  
Основные источники: [корень документации](../README.md), [каноническая спецификация](../../agent_orchestrator_final_spec_v2.md), [раздел architecture](../02-architecture/README.md).

Этот раздел — опорный слой для остального набора спецификаций. До того как архитектура, контракты, flow исполнения или lifecycle-детали станут устойчивыми, проекту нужен фиксированный ответ на меньший набор вопросов:

- Чем продукт является и чем не является.
- Где заканчивается граница оркестратора и начинается граница runtime.
- Какие термины что именно означают.
- Какой артефакт является авторитетным для какого класса данных.
- Какие законы проекта нерушимы, а какие поведения являются дефолтами.
- Какой implementation stack зафиксирован для репозитория.

Если будущий документ меняет смысл любого из этих ответов, это должно быть намеренным и заметным изменением. Это не локальное уточнение, а изменение foundations.

## Документы Раздела

| Документ | Чем владеет | Когда его читать |
| --- | --- | --- |
| [`project-scope-and-non-goals.md`](./project-scope-and-non-goals.md) | Scope продукта, граница MVP, явные non-goals | Когда нужно ответить: "Эта возможность вообще должна существовать в базовом продукте?" |
| [`system-boundaries.md`](./system-boundaries.md) | Границы ответственности между orchestrator, runtime, interfaces и storage | Когда нужно ответить: "Какой слой должен реализовать это поведение?" |
| [`glossary.md`](./glossary.md) | Канонические термины и дисциплина именования | Когда нужно ответить: "Как мы называем эту сущность в доках, коде и тестах?" |
| [`source-of-truth-model.md`](./source-of-truth-model.md) | Авторитетный артефакт для каждого класса информации | Когда нужно ответить: "Что считается истинным, если два представления расходятся?" |
| [`invariants-and-defaults.md`](./invariants-and-defaults.md) | Законы проекта и поведение по умолчанию | Когда нужно ответить: "Это обязательно, запрещено или просто дефолт?" |
| [`technology-stack.md`](./technology-stack.md) | Зафиксированный implementation stack и guardrails по зависимостям | Когда нужно ответить: "Какие технологии обязательны или запрещены для MVP?" |

## Что Должно Решаться Здесь

Слой foundations должен отвечать на вопросы, которые иначе вызывают drift архитектуры во всех остальных разделах:

- Является ли продукт оркестратором или новой runtime-системой.
- Принадлежит ли идея минимальному стабильному ядру, более позднему расширению или вообще вне продукта.
- Является ли хранилище каноническим или только производным.
- Свободен ли выбор зависимости, ограничен или уже зафиксирован.
- Обозначает ли термин доменную сущность, деталь реализации или внешний артефакт экосистемы.

## Что Не Должно Жить Только Здесь

Раздел foundations намеренно расположен выше других, но не является свалкой для любых правил:

- Детальные adapter protocols относятся к [`02-architecture`](../02-architecture/README.md) и более узким документам.
- Формальные схемы и точные ограничения полей относятся к [`03-contracts`](../03-contracts/README.md); рабочие примеры и иллюстрации анти-паттернов относятся к [`10-examples`](../10-examples/README.md).
- Пошаговые правила исполнения относятся к [`04-execution`](../04-execution/README.md).
- Детали persistence state относятся к [`05-state`](../05-state/README.md).
- Детали lifecycle workflow относятся к [`07-lifecycle`](../07-lifecycle/README.md).
- Extension-specific контракты относятся к [`08-extensions`](../08-extensions/README.md).

## Как Использовать Этот Раздел При Реализации

Перед добавлением модуля, зависимости или поведения отвечайте на вопросы в таком порядке:

1. Находится ли идея в scope по [`project-scope-and-non-goals.md`](./project-scope-and-non-goals.md)?
2. Какая сторона системной границы должна ей владеть по [`system-boundaries.md`](./system-boundaries.md)?
3. Какие термины должны использовать код и тесты по [`glossary.md`](./glossary.md)?
4. Какой артефакт становится авторитетным по [`source-of-truth-model.md`](./source-of-truth-model.md)?
5. Является ли поведение обязательным или дефолтным по [`invariants-and-defaults.md`](./invariants-and-defaults.md)?
6. Укладывается ли реализация в lock-решения из [`technology-stack.md`](./technology-stack.md)?

Если любой ответ неясен, правильный ход — сначала ужесточить foundations docs, а не позволять коду изобретать собственную интерпретацию.
