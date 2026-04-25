[English](#english) | [Русский](#russian)

## English

# Project Scope And Non-Goals

Status: approved foundational specification.  
Owns: product identity, scope boundary, MVP-oriented sequencing constraints, explicit non-goals, and escalation rules for out-of-scope ideas.  
Does not own: exact field-level contracts, runtime adapter protocol details, or storage schemas.  
Primary sources: [canonical specification](../../agent_orchestrator_final_spec_v2.md), [foundations index](./README.md), [system boundaries](./system-boundaries.md), [extensions section](../08-extensions/README.md).

## Scope Summary

The project is an orchestrator of agent runs. Its job is to describe agents as portable graph-based JSON artifacts, execute those graphs through runtime adapters, preserve the operational state needed for chat and resume, and expose that behavior through user-facing interfaces such as CLI and later UI.

Those interfaces are product shells over Core. They are not the runtime mechanism for Codex execution, which in this repository is App Server-native inside the Codex adapter path.

The project scope is intentionally wider than a single runtime integration, but intentionally narrower than a full agent platform. The orchestrator owns the execution boundary, not the agent's internal reasoning engine.

## What Is In Scope

The product scope includes the following capabilities and responsibilities:

- A portable `agent JSON` artifact as the canonical description of an agent graph.
- Orchestration of graph execution, including node selection, input assembly, permissions, skills, MCPs, plugins, and final-output handling.
- A runtime adapter boundary that lets the same orchestrator semantics work with Codex first and other compatible runtimes later, while keeping Codex execution App Server-native behind the adapter boundary.
- Local chat and resume support with only the state required for visible history and continuation.
- User-facing interfaces over the same core, starting with CLI.
- A local registry and lifecycle surface for known agents, drafts, live versions, and deploy operations.
- Separate handling of events and triggers outside the portable agent file.

## Minimum Stable Core Before Extensions

The first implementation slice should prove the stable core rather than chase breadth. Before extension work becomes primary, the repository should be able to do all of the following reliably:

- Load and validate an agent file against the supported graph contract version.
- Execute a sequential graph through at least one runtime adapter.
- Produce a valid final output for the run.
- Persist the minimum chat and resume state required by the base model.
- Expose the behavior through the CLI layer without turning the CLI into the runtime integration path.

This sequencing matters because later features depend on a stable orchestration contract. Builder workflows, richer lifecycle tooling, memory bindings, or multiple execution sources become expensive if the graph semantics are still moving.

## In Scope But Not Required For The First Vertical Slice

The canonical model already reserves several areas as real parts of the product, but they should not redefine the minimal stable core:

- A built-in builder agent that creates or improves agent files through the same public system contracts.
- Memory bindings as a separate axis from chat and resume.
- Multiple runtime sources, accounts, and limit-awareness within the same runtime family.
- Richer lifecycle management around drafts, live revisions, and deploy.

These are not out of scope. They are later-scope capabilities that must build on the same foundations instead of bypassing them.

## Explicit Non-Goals

The following ideas are outside the base product model unless a later extension document and ADR say otherwise:

- Turning the project into a new general-purpose agent runtime or vendor-specific agent platform.
- Owning a custom internal contract for `skills`, `MCPs`, or `plugins` instead of using the compatible runtime ecosystem.
- Making a global task queue, scheduler, worker fabric, or priority system part of the mandatory base architecture.
- Adding automatic retry semantics for runs or triggers as a hidden default behavior.
- Storing the full internal history of runtime execution as a mandatory product responsibility.
- Treating local metadata storage as the canonical representation of the agent instead of the portable file.
- Splitting the system into a speculative "brains vs muscles" architecture that is not part of the canon.
- Requiring a complex artifact store as part of the minimal core.

## Decision Filter For New Features

When a new idea appears, classify it before implementing anything:

1. If it strengthens the existing orchestration boundary without changing the product identity, it is likely in scope.
2. If it belongs to memory, runtime-source management, or another explicitly reserved axis, place it in a later section instead of forcing it into the core.
3. If it introduces queues, hidden retries, a new plugin ecosystem, or a second canonical agent representation, it is out of scope for the base model.
4. If the idea is valuable but changes one of the above boundaries, treat it as an architectural decision, not an opportunistic implementation detail.

## Implementation Consequences

- Do not design core modules as if they were runtime-internal reasoning engines.
- Do not make base execution depend on optional extension subsystems.
- Do not add convenience persistence that silently becomes a second agent-definition store.
- Do not describe deferred capabilities as open-ended future decisions when the canon already places them either inside a later section or outside the product.

## Russian

# Область Проекта И Non-Goals

Статус: утвержденная foundational-спецификация.  
Владеет: идентичностью продукта, границей scope, ограничениями на MVP-последовательность, явными non-goals и правилами эскалации для идей вне scope.  
Не владеет: точными field-level контрактами, деталями runtime adapter protocol и схемами хранения.  
Основные источники: [каноническая спецификация](../../agent_orchestrator_final_spec_v2.md), [индекс foundations](./README.md), [system boundaries](./system-boundaries.md), [раздел extensions](../08-extensions/README.md).

Проект — это оркестратор запусков агентов. Его задача — описывать агентов как переносимые JSON-артефакты с графовой структурой, исполнять эти графы через runtime adapters, сохранять операционное состояние, нужное для chat и resume, и выдавать это поведение через пользовательские интерфейсы, начиная с CLI и позже UI.

Эти интерфейсы являются продуктовыми оболочками над Core. Они не являются runtime-механизмом исполнения Codex, который в этом репозитории должен существовать только как App Server-native путь внутри Codex adapter.

Scope проекта специально шире одной runtime-интеграции, но специально уже полноценной agent platform. Оркестратор владеет границей исполнения, а не внутренним reasoning engine агента.

## Что Входит В Scope

В scope продукта входят следующие возможности и ответственности:

- Переносимый артефакт `agent JSON` как каноническое описание графа агента.
- Оркестрация исполнения графа, включая выбор ноды, сборку входа, права, skills, MCPs, plugins и обработку финального output.
- Граница runtime adapter, позволяющая одной и той же семантике оркестратора работать сначала с Codex, а затем и с другими совместимыми runtime, при этом само исполнение Codex остается App Server-native внутри границы adapter-а.
- Локальная поддержка chat и resume с сохранением только того состояния, которое нужно для видимой истории и продолжения.
- Пользовательские интерфейсы поверх одного и того же core, начиная с CLI.
- Локальный registry и lifecycle-поверхность для известных агентов, drafts, live-версий и deploy-операций.
- Отдельное обращение с events и triggers вне переносимого agent file.

## Минимальное Стабильное Ядро До Extensions

Первый срез реализации должен доказывать устойчивость ядра, а не гоняться за широтой. До того как extension-работа станет приоритетной, репозиторий должен уметь надежно делать все из списка ниже:

- Загружать и валидировать agent file относительно поддерживаемой версии graph contract.
- Исполнять последовательный граф хотя бы через один runtime adapter.
- Получать валидный финальный output run.
- Сохранять минимальный chat и resume state, который требует базовая модель.
- Выдавать это поведение через слой CLI, не превращая CLI в путь runtime-интеграции.

Эта последовательность важна, потому что более поздние возможности опираются на устойчивый orchestration contract. Builder workflows, richer lifecycle tooling, memory bindings или несколько execution sources становятся дорогими, если сама семантика графа еще плавает.

## В Scope, Но Не Обязательно Для Первого Vertical Slice

Каноническая модель уже резервирует несколько зон как реальные части продукта, но они не должны переопределять минимальное стабильное ядро:

- Встроенный builder agent, который создает или улучшает agent files через те же публичные системные контракты.
- Memory bindings как отдельная ось, не равная chat и resume.
- Несколько runtime sources, accounts и awareness о limits внутри одного runtime family.
- Более развитое lifecycle-управление вокруг drafts, live revisions и deploy.

Это не out-of-scope. Это later-scope возможности, которые должны строиться поверх тех же foundations, а не обходить их.

## Явные Non-Goals

Следующие идеи находятся вне базовой модели продукта, если только более поздний extension-документ и ADR явно не скажут обратное:

- Превращение проекта в новую универсальную agent runtime-систему или vendor-specific agent platform.
- Владение собственным внутренним контрактом для `skills`, `MCPs` или `plugins` вместо использования экосистемы совместимого runtime.
- Делание глобальной очереди задач, scheduler, worker fabric или priority system обязательной частью базовой архитектуры.
- Добавление automatic retry semantics для runs или triggers как скрытого поведения по умолчанию.
- Хранение полной внутренней истории runtime execution как обязательной ответственности продукта.
- Рассмотрение локального metadata storage как канонического представления агента вместо переносимого файла.
- Разделение системы на спекулятивную архитектуру "brains vs muscles", которой нет в каноне.
- Обязательное наличие сложного artifact store в минимальном ядре.

## Фильтр Для Новых Возможностей

Когда появляется новая идея, сначала классифицируйте ее, а уже потом реализуйте:

1. Если она усиливает существующую orchestration boundary и не меняет идентичность продукта, скорее всего, она в scope.
2. Если она относится к memory, управлению runtime sources или другой явно зарезервированной оси, ее нужно относить в более поздний раздел, а не проталкивать в core.
3. Если она вводит очереди, скрытые retries, новую plugin-экосистему или второе каноническое представление агента, она вне scope базовой модели.
4. Если идея ценна, но меняет одну из этих границ, это архитектурное решение, а не удобная мелочь реализации.

## Последствия Для Реализации

- Не проектируйте core-модули так, будто они являются runtime-внутренними reasoning engines.
- Не делайте базовое исполнение зависимым от необязательных extension-подсистем.
- Не добавляйте "удобное" persistence-хранилище, которое молча станет вторым source of truth для определения агента.
- Не описывайте отложенные возможности как неопределенные будущие решения, когда канон уже поместил их либо в более поздний раздел, либо вне продукта.
