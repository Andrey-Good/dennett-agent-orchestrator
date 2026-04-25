[English](#english) | [Русский](#russian)

## English

# System Boundaries

Status: approved foundational specification.  
Owns: responsibility boundaries between orchestrator core, runtime adapters, external runtimes, interfaces, files, and operational storage.  
Does not own: concrete adapter method signatures, storage schema details, or CLI command design.  
Primary sources: [canonical specification](../../agent_orchestrator_final_spec_v2.md), [project scope](./project-scope-and-non-goals.md), [source-of-truth model](./source-of-truth-model.md), [runtime integration model](../02-architecture/runtime-integration-model.md).

## Boundary Principle

The orchestrator is responsible for the boundary of an agent call. It decides what to run, how execution moves through the graph, what input message is assembled, which permissions and integrations are exposed, and how the final output is interpreted.

The orchestrator is not responsible for the agent's internal reasoning once a node has been handed to a runtime. From the orchestrator point of view, a node execution is a black box with a controlled input and a controlled output.

## Responsibility Matrix

| Boundary | The orchestrator side owns | The other side owns | Implementation consequence |
| --- | --- | --- | --- |
| Core vs external runtime | Node selection, graph order, assembled input, permission envelope, exposed skills/MCPs/plugins, final-output interpretation, run outcomes | Internal reasoning, native tool execution, vendor session details, runtime-specific side effects | Core must not assume visibility into runtime internals. |
| Core vs runtime adapter | Orchestrator semantics and normalized runtime port | Translation between normalized semantics and vendor APIs | Vendor SDK imports belong in adapters, not in core. |
| Core vs interfaces | Commands, domain operations, state transitions, error semantics | Parsing of user input, presentation, rendering, transport | CLI or UI must not become a second domain layer. |
| Agent file vs operational storage | Portable agent definition | Derived indexes, drafts, chat state, resume metadata, working copy associations | Storage may support execution, but must not redefine the agent. |
| Agent file vs events/triggers | Graph and agent-level configuration | External launch conditions and trigger payload wiring | Triggers live outside the agent file. |
| Chat/resume vs memory | Immediate conversational continuity | Long-lived external context when memory is enabled later | Do not treat chat state as generic memory. |

## What The Orchestrator Must Own

The orchestrator core must own at least the following classes of behavior:

- Validation that a requested agent file is compatible with the supported graph contract version.
- Sequential graph traversal in the base model.
- Construction of node input from explicit text and allowed references.
- Passing permissions, skills, MCPs, plugins, and runtime options through a normalized boundary.
- Collecting final node output and mapping it to run-level outcome and final response behavior.
- Persistence of the local state required for chats, resume, registry, drafts, and related operational metadata.

## What The Runtime Must Own

The runtime side must own at least the following:

- The actual execution of an agent call after the orchestrator hands off a node.
- Runtime-native reasoning behavior and tool invocation strategy.
- Vendor-specific session identifiers, event streams, and low-level call lifecycle.
- Native live-comment or native resume mechanics when the chosen runtime supports them.

The orchestrator may expose or prefer native runtime capabilities through an adapter, but it must still treat them as runtime capabilities rather than rewrite the product model around one vendor.

## Forbidden Boundary Violations

The following are architectural defects:

- Importing a vendor SDK directly into core, interfaces, contracts, or foundational domain types.
- Letting the runtime adapter redefine the meaning of graph concepts such as nodes, edges, params, vars, or outputs.
- Letting interfaces call vendor-specific code paths directly while bypassing core orchestration.
- Letting local storage become the canonical source of the agent definition.
- Letting events, triggers, or runtime session artifacts silently modify the portable agent file contract.

## Boundary Checks For New Work

When implementing a new feature, answer these questions before choosing a module:

1. Is the feature about orchestrator semantics or runtime internals?
2. Is it defining the portable agent or only storing derived local metadata?
3. Is it domain logic, adapter translation, or interface presentation?
4. Does it require a new explicit port, or is it smuggling vendor concepts into the core?

If the answer suggests mixed ownership, the design is probably wrong or still under-specified.

## Russian

# Системные Границы

Статус: утвержденная foundational-спецификация.  
Владеет: границами ответственности между orchestrator core, runtime adapters, внешними runtime, интерфейсами, файлами и операционным storage.  
Не владеет: конкретными сигнатурами adapter methods, деталями storage schemas и дизайном CLI-команд.  
Основные источники: [каноническая спецификация](../../agent_orchestrator_final_spec_v2.md), [scope проекта](./project-scope-and-non-goals.md), [модель source of truth](./source-of-truth-model.md), [runtime integration model](../02-architecture/runtime-integration-model.md).

## Главный Принцип Границы

Оркестратор отвечает за границу вызова агента. Он решает, что запускать, как исполнение движется по графу, какое входное сообщение собирается, какие права и интеграции открываются и как интерпретируется финальный output.

Оркестратор не отвечает за внутренний reasoning агента после того, как нода передана runtime. С точки зрения оркестратора исполнение ноды — это black box с контролируемым входом и контролируемым выходом.

## Матрица Ответственности

| Граница | Чем владеет сторона оркестратора | Чем владеет другая сторона | Импликация для реализации |
| --- | --- | --- | --- |
| Core vs external runtime | Выбор ноды, порядок графа, собранный input, permission envelope, доступные skills/MCPs/plugins, интерпретация final output, run outcomes | Внутренний reasoning, native tool execution, vendor session details, runtime-specific side effects | Core не должен рассчитывать на видимость runtime internals. |
| Core vs runtime adapter | Семантика оркестратора и нормализованный runtime port | Перевод нормализованной семантики в vendor APIs | Импорты vendor SDK принадлежат adapters, а не core. |
| Core vs interfaces | Команды, доменные операции, переходы состояния, семантика ошибок | Парсинг пользовательского ввода, presentation, rendering, transport | CLI или UI не должны становиться вторым доменным слоем. |
| Agent file vs operational storage | Переносимое определение агента | Производные индексы, drafts, chat state, resume metadata, связи рабочих копий | Storage может поддерживать исполнение, но не должен переопределять агента. |
| Agent file vs events/triggers | Конфигурация графа и агента | Внешние условия запуска и wiring trigger payload | Triggers живут вне agent file. |
| Chat/resume vs memory | Непосредственная conversational continuity | Долговременный внешний контекст, если позже включена memory | Нельзя считать chat state универсальной памятью. |

## Чем Обязан Владеть Оркестратор

Core оркестратора обязан владеть как минимум следующими классами поведения:

- Валидацией того, что запрошенный agent file совместим с поддерживаемой версией graph contract.
- Последовательным обходом графа в базовой модели.
- Сборкой node input из явного текста и разрешенных ссылок.
- Передачей permissions, skills, MCPs, plugins и runtime options через нормализованную границу.
- Сбором final node output и преобразованием его в run-level outcome и поведение финального ответа.
- Persistence локального состояния, нужного для chats, resume, registry, drafts и связанной операционной metadata.

## Чем Обязан Владеть Runtime

Сторона runtime обязана владеть как минимум следующим:

- Фактическим исполнением вызова агента после того, как оркестратор передал ноду.
- Runtime-native поведением reasoning и стратегией вызова инструментов.
- Vendor-specific session identifiers, event streams и low-level lifecycle вызова.
- Native live-comment или native resume механизмами, если выбранный runtime их поддерживает.

Оркестратор может пробрасывать или предпочитать native runtime capabilities через adapter, но все равно обязан считать их возможностями runtime, а не перестраивать модель продукта вокруг одного vendor.

## Запрещенные Нарушения Границы

Следующее является архитектурным дефектом:

- Прямой импорт vendor SDK в core, interfaces, contracts или foundational domain types.
- Ситуация, где runtime adapter переопределяет смысл graph concepts вроде nodes, edges, params, vars или outputs.
- Ситуация, где interfaces напрямую вызывают vendor-specific code paths в обход core orchestration.
- Ситуация, где local storage становится каноническим source of truth для определения агента.
- Ситуация, где events, triggers или runtime session artifacts молча меняют контракт переносимого agent file.

## Проверка Границы Для Новой Работы

Перед выбором модуля для новой возможности ответьте на вопросы:

1. Речь идет о семантике оркестратора или о runtime internals?
2. Определяется переносимый агент или только хранится производная local metadata?
3. Это доменная логика, adapter translation или interface presentation?
4. Нужен новый явный port или дизайн протаскивает vendor concepts в core?

Если ответы указывают на смешанное владение, дизайн, скорее всего, неверен или еще не доопределен.
