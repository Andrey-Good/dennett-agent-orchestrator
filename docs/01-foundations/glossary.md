[English](#english) | [Русский](#russian)

## English

# Glossary

Status: approved foundational terminology reference.  
Owns: canonical meanings of core terms used by docs, code, tests, and reviews.  
Does not own: exact field schemas or module names, except where terminology constrains naming.  
Primary sources: [canonical specification](../../agent_orchestrator_final_spec_v2.md), [project scope](./project-scope-and-non-goals.md), [system boundaries](./system-boundaries.md).

The purpose of this glossary is not literary consistency. It is operational precision. If code, docs, tests, and reviews use different words for the same concept, the project will slowly invent multiple models. The terms below should be used consistently unless a narrower document explicitly introduces a more specific subtype.

## Core Execution Terms

| Term | Meaning | Implementation note |
| --- | --- | --- |
| Agent file | A portable JSON file that defines an agent graph | This is the canonical agent artifact. Do not equate it with a database row. |
| Agent | The conceptual graph-based automation described by an agent file | Use this when talking about the defined artifact, not a live runtime session. |
| Run | One execution of an agent graph | A run is operational state, not the agent definition itself. |
| Graph | The ordered execution structure of nodes and edges | In the base model it executes sequentially. |
| Node | One execution unit inside the graph | A node describes the boundary of a call, not the runtime internals. |
| Edge | A transition rule between nodes | Edges control execution flow, not data transport. |
| Param | User-configurable input for an agent without cloning the file | Params are distinct from vars and runtime-local state. |
| Var | Graph-level mutable runtime value | When names collide, the latest written value wins. |
| Node output | The final output returned by one node | It is not the node's full hidden reasoning history. |
| Final output | The run-level result shown as the agent's final answer when configured | By default it is the last successful node output. |

## Integration Terms

| Term | Meaning | Implementation note |
| --- | --- | --- |
| Runtime service selection | The Core-side choice of which runtime adapter family or service a node call targets | This happens before any runtime source selection inside that family. |
| Runtime adapter | The orchestrator-owned adapter for one external runtime | It translates normalized orchestrator semantics into vendor APIs. |
| Runtime source | A configured execution source inside a runtime family | This is different from the adapter itself. |
| Skill | Textual instruction or instruction bundle available to the agent | The orchestrator references skills; it does not define a new universal skill format. |
| MCP | An interface through which the agent interacts with an external capability | Use `MCP` as the model term; do not rename this concept to `SDK` inside the agent model. |
| Plugin | A packaged bundle of skills and MCPs | Again, the detailed plugin contract belongs to the compatible runtime ecosystem. |
| Permission set | The permission envelope passed to a node execution | Permissions may be defined globally or per node. |

## State And Lifecycle Terms

| Term | Meaning | Implementation note |
| --- | --- | --- |
| Chat | Locally stored visible conversation state primarily needed for resume and user continuity | Chat is not a synonym for memory. |
| Resume state | The minimum persisted data required to continue a run or conversation | Resume is explicit, not magic auto-reconstruction. |
| Event | A separate entity that triggers an agent run | Events do not live inside the agent file. |
| Trigger | The external condition or source that emits an event | Triggers are outside the portable agent contract. |
| Registry | The local working surface that indexes known agents | It is derived and operational, not the source of truth for the agent definition. |
| Draft | A working copy or editable state before publication as the active version | A draft is not automatically the portable canonical artifact. |
| Live revision | The currently active working version recognized by the local system | This is separate from `meta.agent_version`. |
| Deploy | The explicit action that promotes a chosen draft or candidate to live | Deploy is a lifecycle operation, not a graph feature. |
| Memory binding | A configured long-lived context source made available to an agent or node | Memory is a later extension and must not be conflated with chat state. |

## Naming Discipline

Use the following distinctions consistently:

- Say `agent file` when the file artifact matters; say `agent` when the conceptual automation matters.
- Say `registry entry` or `registry metadata` for local indexed information, not `agent record`.
- Say `runtime service selection` for choosing the runtime family and `runtime source` for choosing an execution source inside that family.
- Say `runtime adapter` for the orchestrator-side integration code and `runtime source` for a selectable execution source.
- Say `MCP`, not `SDK`, when referring to the modeled external capability concept.
- Say `product CLI` or `interface CLI` for the user-facing shell, not for the Codex runtime path.
- Say `chat` and `resume state` for continuation data, and `memory` only for the separate long-lived context axis.

## Terms To Avoid Or Use Carefully

- `SDK` as a domain term for modeled integrations. In this project the modeled concept is `MCP`; a vendor SDK is only an implementation dependency inside an adapter.
- `Database agent` or `stored agent` when referring to the source of truth. The portable file owns that role.
- `Parallel graph` unless a later document explicitly adds that capability. The base model is sequential.
- `Retry` as an assumed background behavior. Retries are not part of the base trigger/run model.

## Russian

# Глоссарий

Статус: утвержденный foundational-справочник терминологии.  
Владеет: каноническими значениями ключевых терминов для docs, кода, тестов и review.  
Не владеет: точными field schemas или именами модулей, кроме случаев, когда терминология ограничивает именование.  
Основные источники: [каноническая спецификация](../../agent_orchestrator_final_spec_v2.md), [scope проекта](./project-scope-and-non-goals.md), [system boundaries](./system-boundaries.md).

Цель этого глоссария — не литературная одинаковость, а операционная точность. Если код, docs, tests и review используют разные слова для одной сущности, проект постепенно изобретет несколько моделей сразу. Термины ниже должны использоваться последовательно, если только более узкий документ явно не вводит более конкретный подтип.

## Термины Базового Исполнения

| Термин | Смысл | Импликация для реализации |
| --- | --- | --- |
| Agent file | Переносимый JSON-файл, который определяет граф агента | Это канонический артефакт агента. Его нельзя приравнивать к строке в базе. |
| Agent | Концептуальная графовая автоматизация, описанная agent file | Используйте термин для описанного артефакта, а не живой runtime session. |
| Run | Одно исполнение графа агента | Run — это операционное состояние, а не само определение агента. |
| Graph | Упорядоченная структура исполнения из nodes и edges | В базовой модели граф исполняется последовательно. |
| Node | Одна единица исполнения внутри графа | Нода описывает границу вызова, а не runtime internals. |
| Edge | Правило перехода между нодами | Edges управляют execution flow, а не переносом данных. |
| Param | Пользовательски настраиваемый вход агента без клонирования файла | Params отличны от vars и runtime-local state. |
| Var | Изменяемое runtime-значение на уровне графа | При совпадении имен побеждает последнее записанное значение. |
| Node output | Финальный output, возвращенный одной нодой | Это не полная скрытая история reasoning внутри ноды. |
| Final output | Run-level результат, показываемый как финальный ответ агента, если это настроено | По умолчанию это последний успешный node output. |

## Термины Интеграции

| Термин | Смысл | Импликация для реализации |
| --- | --- | --- |
| Runtime service selection | Выбор на стороне Core того, какое runtime-family или service должна использовать нода | Этот выбор происходит раньше выбора runtime source внутри выбранного family. |
| Runtime adapter | Адаптер оркестратора для одного внешнего runtime | Он переводит нормализованную семантику оркестратора в vendor APIs. |
| Runtime source | Настроенный источник исполнения внутри одного runtime family | Это не то же самое, что сам adapter. |
| Skill | Текстовая инструкция или набор инструкций, доступный агенту | Оркестратор ссылается на skills, но не определяет новый универсальный формат skill. |
| MCP | Интерфейс, через который агент взаимодействует с внешней возможностью | Используйте термин `MCP`; не переименовывайте эту сущность в `SDK` внутри модели агента. |
| Plugin | Пакетный набор skills и MCPs | Детальный plugin contract принадлежит экосистеме совместимого runtime. |
| Permission set | Набор прав, передаваемый в исполнение ноды | Права могут задаваться глобально или на уровне ноды. |

## Термины Состояния И Lifecycle

| Термин | Смысл | Импликация для реализации |
| --- | --- | --- |
| Chat | Локально сохраненное видимое состояние разговора, нужное прежде всего для resume и непрерывности общения | Chat не является синонимом memory. |
| Resume state | Минимальный сохраненный набор данных, достаточный для продолжения run или диалога | Resume является явным, а не магически восстановленным состоянием. |
| Event | Отдельная сущность, которая инициирует run агента | Events не живут внутри agent file. |
| Trigger | Внешнее условие или источник, который выпускает event | Triggers находятся вне переносимого agent contract. |
| Registry | Локальная рабочая поверхность, индексирующая известных агентов | Это производный операционный слой, а не source of truth для определения агента. |
| Draft | Рабочая копия или редактируемое состояние до публикации активной версии | Draft не становится автоматически переносимым каноническим артефактом. |
| Live revision | Текущая активная рабочая версия, которую локальная система считает основной | Это отдельная ось, не равная `meta.agent_version`. |
| Deploy | Явное действие, переводящее выбранный draft или candidate в live | Deploy — операция lifecycle, а не особенность графа. |
| Memory binding | Настроенный долговременный источник контекста, доступный агенту или ноде | Memory — более позднее расширение и не должна смешиваться с chat state. |

## Дисциплина Именования

Последовательно используйте следующие различия:

- Говорите `agent file`, когда важен файловый артефакт; говорите `agent`, когда важна концептуальная автоматизация.
- Говорите `registry entry` или `registry metadata` для локальной индексной информации, а не `agent record`.
- Говорите `runtime service selection`, когда выбирается runtime-family, и `runtime source`, когда выбирается источник исполнения внутри этого family.
- Говорите `runtime adapter` для интеграционного кода на стороне оркестратора и `runtime source` для выбираемого источника исполнения.
- Говорите `MCP`, а не `SDK`, когда речь идет о моделируемой внешней возможности.
- Говорите `product CLI` или `interface CLI` для пользовательской оболочки, а не для пути исполнения Codex.
- Говорите `chat` и `resume state` для данных продолжения, а `memory` — только для отдельной оси долговременного контекста.

## Термины, Которых Нужно Избегать Или Использовать Осторожно

- `SDK` как доменный термин для моделируемых интеграций. В проекте доменная сущность — это `MCP`; vendor SDK — лишь зависимость реализации внутри adapter.
- `Database agent` или `stored agent`, если речь идет об источнике истины. Этой ролью владеет переносимый файл.
- `Parallel graph`, если только более поздний документ явно не добавит такую возможность. Базовая модель последовательна.
- `Retry` как предполагаемое фоновое поведение. Retries не входят в базовую trigger/run-модель.
