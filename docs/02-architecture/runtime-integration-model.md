[English](#english) | [Русский](#russian)

<a id="english"></a>
# Runtime Integration Model

Status: approved.

Related documents:

- [`core-and-interfaces.md`](./core-and-interfaces.md)
- [`../01-foundations/technology-stack.md`](../01-foundations/technology-stack.md)
- [`../01-foundations/glossary.md`](../01-foundations/glossary.md)
- [`../04-execution/README.md`](../04-execution/README.md)
- [`../05-state/README.md`](../05-state/README.md)
- [`../08-extensions/runtime-sources.md`](../08-extensions/runtime-sources.md)
- [`../09-adrs/ADR-0001-codex-first-not-codex-only.md`](../09-adrs/ADR-0001-codex-first-not-codex-only.md)

This document defines the architectural boundary between the orchestrator and any external runtime. It is intentionally code-guiding, not a field-level adapter contract. Exact request and event shapes belong in contracts and execution documents. In repository terms, Core-owned runtime decisions live under `src/core`, runtime ports live under `src/ports`, and vendor code lives under `src/adapters`.

## 1. Normative Boundary

Any external runtime must be reached only through an orchestrator-owned runtime port and a runtime adapter that implements it.

For Codex, the official OpenAI App Server material and this repository's generated protocol surface are the Codex-specific source of truth. The adapter should prefer native App Server primitives, including thread, turn, item, review, command, and MCP operations, whenever the protocol already exposes them. That does not grant permission for Core, storage, or interfaces to import vendor client code directly.

For this repository the Codex adapter path is App Server-native. The adapter translates normalized requests into the App Server protocol surface and may launch a long-lived App Server process or binary as that server endpoint. Codex execution is not modeled as product-level CLI orchestration or one-shot shell subprocess responsibility.

This document applies to `runtime_agent` nodes. `orchestrator_agent` nodes stay inside Core and recurse into the orchestrator itself by loading another agent file.

## 2. Runtime Flow

```text
Interface action
    |
    v
Core application service
    |
    +--> agent file / registry / state lookups
    |
    +--> runtime service selection + runtime source selection
    |
    v
Runtime port
    |
    v
Runtime adapter (Codex today, more later)
    |
    v
External runtime

External runtime
    |
    v
Adapter-specific events / result / errors
    |
    v
Normalized runtime events / final output
    |
    v
Core execution service
    |
    +--> state persistence
    +--> final output routing
    +--> interface observers
```

Core sees normalized runtime semantics. The adapter alone sees vendor semantics.

## 3. What Core Owns Before The Boundary

Before calling a runtime adapter, Core is responsible for:

- resolving that the current node is a `runtime_agent` and selecting the requested runtime adapter id, meaning the runtime service or family to invoke;
- building the canonical prompt and input message from graph data;
- resolving which skills, MCPs, plugins, permissions, memory bindings, and runtime sources are available to the node;
- narrowing the eligible runtime-source set from file-level, node-level, and user-level constraints, and choosing the final source whenever such narrowing exists;
- deciding whether the action is a fresh start, a native resume attempt, or a local resume fallback path;
- owning graph transitions, node output routing, user-visible run state, and persistence decisions;
- rejecting invalid node or runtime combinations as early as possible.

The adapter does not decide graph semantics. It receives an already-resolved execution intent from Core. That includes final runtime-source choice whenever any source constraint is in play.

## 4. What The Runtime Port Must Abstract

The runtime port must cover these categories of work without leaking vendor-specific types:

- start a `runtime_agent` node from a canonical Core request, including execution against a Core-selected runtime source when one is present;
- list available runtime models with normalized metadata when the adapter supports model discovery;
- inspect local runtime auth, account, rate-limit, and config state when the adapter supports runtime-environment introspection;
- inspect a configured runtime source for normalized availability and limit status when the adapter supports that capability;
- resume a previously started runtime session when the runtime supports native resume;
- accept live user comments when the runtime supports a native path for them;
- expose whether native resume, live comments, MCP integration, model discovery, runtime-environment introspection, reasoning-effort overrides, speed tiers, personality, `supports_explicit_runtime_source`, and `supports_runtime_source_introspection` are supported;
- emit normalized progress events, final output, and normalized failures;
- return or refresh any native session handle metadata needed for later resume;
- surface runtime-reported execution-source status, quotas, or availability only when the runtime can provide them.

This required abstraction is intentionally narrower than the full verified App Server surface. The Codex adapter may read or use additional App Server-native families behind the boundary, but Core must treat them as staged capability families until a contract or extension document gives them normalized meaning.

This document owns the categories. Exact method signatures and payload shapes belong in later contract documents.

## 5. What A Runtime Adapter Must Do

Every runtime adapter must:

- translate the canonical Core request into runtime-native API calls;
- for the Codex adapter in this repository, satisfy that translation through the App Server protocol surface, including a long-lived App Server process or binary when needed, rather than through one-shot vendor CLI orchestration semantics;
- use runtime-native features for resume, live comments, MCP wiring, thread and turn control, item/event streaming, and similar capabilities when they already exist;
- map vendor-specific identifiers, events, and failures into normalized Core categories;
- keep `runtime_options` opaque and local to adapter translation rather than promoting them into Core semantics;
- resolve concrete execution sources such as sessions, accounts, API keys, or profiles from Core-selected source references;
- honor a passed `runtime_source` exactly, and use adapter-default source selection only when Core omitted `runtime_source` to delegate that choice;
- return normalized runtime-source inspection data only when the runtime exposes it;
- return only the final node output under the declared `output` contract rather than the full internal action history;
- surface capability gaps explicitly instead of hiding them with silent fallbacks.

## 6. What A Runtime Adapter Must Not Do

A runtime adapter must not:

- choose the next node or otherwise take ownership of graph control flow;
- redefine the agent model or create a second source of truth for agent definitions;
- export vendor SDK types, errors, or identifiers through Core ports or interface-facing APIs;
- require Core to understand vendor-specific session structures;
- store chain-of-thought, internal tool traces, or full runtime internals as canonical orchestrator state;
- invent a fake live-comment channel when the runtime has no native support;
- invent its own memory data model when memory is provided through runtime features or provider-specific transports such as MCP;
- for the Codex path in this repository, satisfy the runtime boundary through one-shot vendor CLI orchestration or require a product interface to do so on the adapter's behalf; launching a long-lived App Server process or binary behind the adapter boundary is allowed;
- reopen or widen runtime-source selection after Core has already narrowed the eligible set or passed a resolved source;
- invent source availability or limit status that the runtime did not expose;
- bypass Core by rendering interface output or reading interface state directly.

## 7. Source-Of-Truth Split Across The Boundary

| Concern | Owner | Architectural consequence |
| --- | --- | --- |
| Agent graph and node configuration | `agent JSON` loaded by Core | The adapter receives resolved configuration and never becomes the canonical agent model. |
| Chats and resume metadata | Core state layer | The adapter may return native handles, but it does not own resume policy. |
| Runtime-internal session state | External runtime | Core stores only the metadata required for native resume or fallback resume. |
| `runtime_sources` catalog and policies | Core lifecycle/configuration layer | Core narrows eligible sets and selects a final source whenever constraints apply. The adapter resolves that selected `source_ref` into vendor-native clients, sessions, or accounts, or uses a local default only when Core delegated the choice. |
| `memory_bindings` availability | Core configuration | The adapter only wires availability through provider-native API/SDK paths or MCP as a transport mode under the provider adapter. MCP is not a separate memory family. |

If any adapter-local cache or SQLite data disagrees with the canonical file model, Core must treat the file model as authoritative for agent definition and use local state only as derived metadata.

## 8. Capability Mapping Rules

| Capability | Core responsibility | Adapter responsibility | If unsupported |
| --- | --- | --- | --- |
| Native resume | Prefer native resume when a valid handle exists. | Call the runtime's native resume path and return normalized status. | Core may use local resume rules defined elsewhere. |
| Live comments | Expose the feature only when the adapter reports it. | Use the runtime's native comment mechanism. | The feature is unavailable for that node. |
| Built-in MCP channel | Decide whether the system MCP should be available to the node. | Attach the MCP channel using runtime-native MCP support. | The feature is unavailable; do not invent a replacement transport here. |
| Skills, MCPs, plugins, permissions | Select which references the node may use. | Bind them through runtime-native configuration mechanisms. | Validation or execution must fail explicitly rather than degrade silently. |
| Memory bindings | Select which memory sources are visible to the node. | Bind them through the provider's natural API/SDK mechanism or, when the provider adapter explicitly supports it, MCP as a connection mode. | The node runs without that capability unless another allowed mechanism exists. |
| Explicit runtime-source execution | Narrow the eligible set from file, node, and user constraints, and choose the final source whenever those constraints apply. Omit `runtime_source` only when intentionally delegating source choice. | Execute against the passed `runtime_source` exactly. Use a local default only when Core passed no `runtime_source`. | Launches that require source narrowing must fail explicitly; adapter default selection must not bypass the narrowed set. |
| Runtime source introspection | Request inspection only for configured sources and show or store only the normalized metadata that comes back. | Inspect the configured source when the runtime exposes that capability and return normalized `availability` / `limit_status`. | Core must assume no portable availability or limit metadata exists and must not invent it. |

### Staged App Server-Native Families

The verified App Server surface is broader than the normalized runtime port defined above. The following split is architectural, not accidental.

Current-stage Codex-specific implementation may already rely on these native families behind the adapter boundary:

- per-turn launch overrides such as `model`, `effort`, `serviceTier`, `personality`, and collaboration presets when they are mapped from local adapter configuration or opaque `runtime_options`;
- model discovery through `model/list` and adjacent model metadata, when exposed through the normalized runtime port as local runtime metadata rather than portable agent-file truth;
- local auth, account, config, and rate-limit inspection through `getAuthStatus`, `account/read`, `account/rateLimits/read`, `config/read`, and `configRequirements/read`, when exposed through the normalized runtime port as diagnostics and local UX metadata rather than portable agent-file truth;
- adapter-internal consumption of thread, turn, item, plan, command, and MCP notifications needed to realize normalized resume, cancellation, live comments, final-output delivery, and any later interaction paths only when the adapter explicitly exposes them.

The following App Server-native families are useful and documented now, but remain later-stage capabilities for normalized product exposure:

- thread-oriented primitives such as `thread/fork`, `thread/rollback`, and `thread/injectItems`;
- review-oriented primitives such as `review/start` and related review notifications;
- richer notification families beyond the normalized contract, such as token-usage, model-reroute, reasoning-summary, command/file-change, MCP-progress, config-warning, and account/app/filesystem-status signals.

These families are real App Server-native capabilities. In this repository they remain Codex-specific until a contract, execution, interaction, or extension owner document gives them stable orchestrator meaning. They must not be back-ported into portable agent-file truth by implication.

## 9. Placement In The Locked Repository Layout

The top-level `src/` directories are already locked. One compliant runtime-oriented placement looks like this:

```text
src/
  core/
    application/
      runtime-selection.ts
      runtime-capabilities.ts
    domain/
  ports/
    runtime/
      runtime-port.ts
      runtime-events.ts
      runtime-sources.ts
  adapters/
    runtime/
      codex/
        codex-runtime-adapter.ts
        codex-source-resolver.ts
        codex-event-mapper.ts
        codex-result-mapper.ts
  resources/
    runtimes/            # optional non-code assets only
```

Leaf file names may vary, but the top-level placement may not: Core orchestration rules stay under `src/core`, ports live under `src/ports`, and vendor dependencies stay under `src/adapters`. If a runtime needs prompts, templates, or other bundled assets, keep them under `src/resources/` rather than mixing them into Core logic.

That adapter subtree is still part of the Core side of the product boundary. It is not a user-facing interface layer.

## 10. Adding Another Runtime Without Rewriting Core

Adding a second runtime is acceptable only if the change can be done by:

1. implementing a new adapter package for the runtime;
2. mapping its capabilities into the existing normalized runtime categories;
3. registering the adapter in the runtime catalog and in the thin startup module that assembles the active interface process;
4. adding adapter-specific translation tests;
5. avoiding changes to Core domain semantics unless the canon itself expands.

If Core must learn vendor-specific types or change graph rules just to support the new runtime, the architecture is wrong.

## 11. Compliance Criteria

An implementation complies with this model only if all of the following remain true:

- domain and application tests can run without loading Codex client code;
- replacing one runtime adapter with another does not rewrite graph orchestration rules;
- Core-owned source narrowing is not bypassed by adapter fallback, and runtime-specific source resolution stays behind the adapter boundary;
- Core sees normalized results, not vendor-native event structures;
- unsupported runtime capabilities are reported explicitly instead of being simulated inside Core;
- source availability and limit metadata are shown only when the adapter exposes them through the normalized inspection path.

<a id="russian"></a>
# Модель интеграции runtime

Статус: утверждено.

Связанные документы:

- [`core-and-interfaces.md`](./core-and-interfaces.md)
- [`../01-foundations/technology-stack.md`](../01-foundations/technology-stack.md)
- [`../01-foundations/glossary.md`](../01-foundations/glossary.md)
- [`../04-execution/README.md`](../04-execution/README.md)
- [`../05-state/README.md`](../05-state/README.md)
- [`../08-extensions/runtime-sources.md`](../08-extensions/runtime-sources.md)
- [`../09-adrs/ADR-0001-codex-first-not-codex-only.md`](../09-adrs/ADR-0001-codex-first-not-codex-only.md)

Этот документ задает архитектурную границу между оркестратором и любым внешним runtime. Он намеренно ориентирован на кодовую структуру, а не на field-level contract адаптера. Точные формы запросов и событий должны жить в документах про contracts и execution. На уровне репозитория это означает, что Core-owned runtime-решения живут в `src/core`, runtime ports - в `src/ports`, а vendor code - в `src/adapters`.

## 1. Нормативная граница

Любой внешний runtime должен вызываться только через принадлежащие оркестратору runtime port и реализующий его runtime adapter.

Для Codex официальные материалы OpenAI App Server и сгенерированная в этом репозитории поверхность протокола являются Codex-specific source of truth. Adapter должен по возможности использовать нативные примитивы App Server, включая thread, turn, item, review, command и MCP operations, когда протокол уже их предоставляет. Это не дает права Core, storage или интерфейсам импортировать vendor client code напрямую.

Для этого репозитория путь Codex adapter-а является App Server-native. Adapter переводит нормализованные запросы в поверхность протокола App Server; исполнение Codex не относится к обязанностям продуктового CLI или shell subprocess path.

Этот документ относится к нодам `runtime_agent`. Ноды `orchestrator_agent` остаются внутри Core и рекурсивно вызывают сам оркестратор через загрузку другого agent file.

## 2. Поток работы с runtime

```text
Interface action
    |
    v
Core application service
    |
    +--> agent file / registry / state lookups
    |
    +--> runtime service selection + runtime source selection
    |
    v
Runtime port
    |
    v
Runtime adapter (Codex today, more later)
    |
    v
External runtime

External runtime
    |
    v
Adapter-specific events / result / errors
    |
    v
Normalized runtime events / final output
    |
    v
Core execution service
    |
    +--> state persistence
    +--> final output routing
    +--> interface observers
```

Core видит нормализованную семантику runtime. Только adapter видит vendor-семантику.

## 3. Чем владеет Core до границы

До вызова runtime adapter Core отвечает за следующее:

- определить, что текущая нода является `runtime_agent`, и выбрать нужный идентификатор runtime adapter, то есть runtime service или family, который должен быть вызван;
- собрать канонический prompt и input message из данных графа;
- определить, какие skills, MCPs, plugins, permissions, memory bindings и runtime sources доступны ноде;
- сузить допустимое множество runtime sources на основе file-level, node-level и user-level ограничений и выбрать финальный source всякий раз, когда такое сужение существует;
- решить, является ли действие новым запуском, попыткой native resume или путем local resume fallback;
- владеть переходами графа, маршрутизацией output ноды, пользовательским run state и решениями о persistence;
- как можно раньше отклонять некорректные сочетания ноды и runtime.

Adapter не решает семантику графа. Он получает уже разрешенный execution intent от Core. Это включает и финальный выбор runtime source всякий раз, когда действует хотя бы одно source-ограничение.

## 4. Что должен абстрагировать runtime port

Runtime port должен покрывать следующие категории работы без утечки vendor-specific types:

- запуск ноды `runtime_agent` из канонического запроса Core, включая исполнение против выбранного Core runtime source, если он передан;
- инспекцию настроенного runtime source для нормализованного статуса доступности и лимитов, если адаптер поддерживает такую возможность;
- продолжение ранее начатой runtime-сессии, если runtime поддерживает native resume;
- прием live-комментариев пользователя, если runtime дает для этого нативный путь;
- сообщение о поддержке native resume, live comments, MCP integration, `supports_explicit_runtime_source` и `supports_runtime_source_introspection`;
- выдачу нормализованных progress events, финального output и нормализованных failures;
- возврат или обновление метаданных native session handle, нужных для последующего resume;
- выдачу статуса execution source, квот и доступности только тогда, когда runtime действительно умеет это предоставлять.

Эта обязательная абстракция намеренно уже, чем полная проверенная поверхность App Server. Codex adapter может читать или использовать дополнительные App Server-native семейства за границей adapter-а, но Core обязан считать их staged-возможностями, пока contract- или extension-документ не задаст им нормализованный смысл.

Этот документ владеет именно категориями. Точные сигнатуры методов и формы payload относятся к более поздним документам по контрактам.

## 5. Что обязан делать runtime adapter

Любой runtime adapter обязан:

- переводить канонический запрос Core в runtime-native API calls;
- для Codex adapter-а в этом репозитории выполнять этот перевод через поверхность протокола App Server, а не через vendor CLI subprocess path;
- использовать runtime-native возможности для resume, live comments, MCP wiring, управления thread и turn, item/event streaming и подобных механизмов, если они уже существуют;
- отображать vendor-specific identifiers, events и failures в нормализованные категории Core;
- оставлять `runtime_options` непрозрачными для Core и локальными для трансляции внутри adapter;
- разрешать конкретные execution sources, такие как sessions, accounts, API keys или profiles, из выбранных Core ссылок на источники;
- точно уважать переданный `runtime_source` и использовать adapter default source selection только тогда, когда Core не передал `runtime_source`, сознательно делегировав выбор;
- возвращать нормализованные данные inspection runtime source только тогда, когда runtime действительно их открывает;
- возвращать только финальный output ноды по объявленному контракту `output`, а не полную внутреннюю историю действий;
- явно сообщать о нехватке runtime-возможностей вместо скрытых fallback.

## 6. Что runtime adapter делать не должен

Runtime adapter не должен:

- выбирать следующую ноду или иным образом брать на себя управление control flow графа;
- переопределять модель агента или создавать второй источник истины для agent definitions;
- экспортировать vendor SDK types, errors или identifiers через Core ports или API для интерфейсов;
- требовать от Core понимания vendor-specific session structures;
- сохранять chain-of-thought, internal tool traces или полные runtime internals как каноническое состояние оркестратора;
- изобретать фальшивый канал live comments, если runtime не поддерживает его нативно;
- вводить собственную memory data model, когда память приходит через runtime features или MCP-compatible mechanisms;
- для Codex path в этом репозитории делать shell-out в vendor CLI или требовать, чтобы это делал product interface от имени adapter-а;
- заново открывать или расширять выбор runtime source после того, как Core уже сузил допустимое множество или передал разрешенный source;
- придумывать статус доступности source или лимитов, который runtime не открыл;
- обходить Core, напрямую рендеря вывод интерфейса или читая состояние интерфейса.

## 7. Разделение источников истины через границу

| Сущность | Владелец | Архитектурное следствие |
| --- | --- | --- |
| Граф агента и конфигурация нод | `agent JSON`, загружаемый Core | Adapter получает уже разрешенную конфигурацию и никогда не становится канонической моделью агента. |
| Chats и resume metadata | Слой state внутри Core | Adapter может вернуть native handles, но политикой resume он не владеет. |
| Внутреннее состояние runtime-сессии | Внешний runtime | Core хранит только метаданные, нужные для native resume или fallback resume. |
| Каталог и политики `runtime_sources` | Слой lifecycle/configuration внутри Core | Core сужает допустимые множества и выбирает финальный source всякий раз, когда действуют ограничения. Adapter переводит выбранный `source_ref` в vendor-native clients, sessions или accounts либо использует локальный default только когда Core делегировал выбор. |
| Доступность `memory_bindings` | Конфигурация Core | Adapter лишь подключает доступность через runtime-native или MCP-compatible mechanisms. |

Если какой-либо локальный кэш adapter-а или данные SQLite расходятся с канонической файловой моделью, Core обязан считать файловую модель определяющей для agent definition, а локальное state - только производными метаданными.

## 8. Правила отображения возможностей

| Возможность | Ответственность Core | Ответственность adapter | Если не поддерживается |
| --- | --- | --- | --- |
| Native resume | Предпочитать native resume, когда есть валидный handle. | Вызывать нативный путь resume в runtime и возвращать нормализованный статус. | Core может использовать local resume rules, определенные в другом разделе. |
| Live comments | Показывать функцию только если adapter о ней сообщил. | Использовать нативный механизм комментариев runtime. | Возможность недоступна для этой ноды. |
| Встроенный MCP-канал | Решать, должен ли system MCP быть доступен ноде. | Подключать MCP-канал через нативную поддержку MCP в runtime. | Возможность недоступна; здесь нельзя изобретать заменяющий транспорт. |
| Skills, MCPs, plugins, permissions | Определять, какие ссылки разрешены ноде. | Привязывать их через нативные механизмы конфигурации runtime. | Валидация или исполнение должны завершаться явной ошибкой, а не молчаливой деградацией. |
| Memory bindings | Определять, какие memory sources видимы ноде. | Подключать их естественным для runtime способом, если он есть. | Нода исполняется без этой возможности, если нет другого разрешенного механизма. |
| Явное исполнение с runtime source | Сужать допустимое множество из file-, node- и user-ограничений и выбирать финальный source всякий раз, когда такие ограничения действуют. Не передавать `runtime_source` только когда Core сознательно делегирует выбор. | Выполнять запрос ровно против переданного `runtime_source`. Использовать локальный default только когда Core не передал `runtime_source`. | Запуски, требующие source-сужения, должны явно завершаться ошибкой; adapter default selection не должен обходить суженное множество. |
| Инспекция runtime source | Запрашивать inspection только для настроенных sources и показывать или сохранять только ту нормализованную metadata, которая реально вернулась. | Инспектировать настроенный source, если runtime открывает такую возможность, и возвращать нормализованные `availability` / `limit_status`. | Core обязан считать, что переносимой metadata доступности или лимитов нет, и не должен придумывать ее. |

### Зафиксированные App Server-native семейства и этапность

Проверенная поверхность App Server шире нормализованного runtime port-а, определенного выше. Это архитектурное разделение, а не случайный пробел.

Current-stage Codex-specific реализация уже может опираться на следующие native-семейства за границей adapter-а:

- per-turn overrides запуска вроде `model`, `effort`, `serviceTier`, `personality` и collaboration presets, когда они отображаются из локальной конфигурации adapter-а или opaque `runtime_options`;
- внутреннее для adapter-а потребление thread-, turn-, item-, plan-, command- и MCP-notifications, нужных для реализации нормализованных resume, cancellation, live comments, доставки final output и только тех более поздних interaction-path, которые adapter явно открывает.

Следующие App Server-native семейства полезны и документируются уже сейчас, но для нормализованной продуктовой поверхности остаются later-stage возможностями:

- discovery моделей через `model/list` и соседние surfaces с model-metadata;
- introspection auth/account/config/rate limits через `getAccount`, `getAccountRateLimits`, `config/read` и `config/requirements/read`;
- thread-ориентированные primitives вроде `thread/fork`, `thread/rollback` и `thread/injectItems`;
- review-ориентированные primitives вроде `review/start` и связанных review-notifications;
- более богатые notification-families вне нормализованного контракта, вроде token-usage-, model-reroute-, reasoning-summary-, command/file-change-, MCP-progress-, config-warning- и account/app/filesystem-status-сигналов.

Эти семейства являются реальными App Server-native возможностями. В этом репозитории они остаются Codex-specific, пока contract-, execution-, interaction- или extension-документ-владелец не задаст им стабильный orchestrator-смысл. Их нельзя по умолчанию переносить в portable agent-file truth.

## 9. Размещение в зафиксированной раскладке репозитория

Верхнеуровневые каталоги `src/` уже зафиксированы. Один корректный вариант размещения runtime-кода выглядит так:

```text
src/
  core/
    application/
      runtime-selection.ts
      runtime-capabilities.ts
    domain/
  ports/
    runtime/
      runtime-port.ts
      runtime-events.ts
      runtime-sources.ts
  adapters/
    runtime/
      codex/
        codex-runtime-adapter.ts
        codex-source-resolver.ts
        codex-event-mapper.ts
        codex-result-mapper.ts
  resources/
    runtimes/            # optional non-code assets only
```

Точные имена leaf-файлов могут отличаться, но верхнеуровневое размещение - нет: правила orchestration Core остаются в `src/core`, ports живут в `src/ports`, а vendor dependencies остаются в `src/adapters`. Если runtime нужны prompts, templates или другие bundled assets, их следует держать в `src/resources/`, а не смешивать с логикой Core.

Это поддерево adapter-ов все равно относится к стороне Core в продуктовой границе. Оно не является пользовательским слоем интерфейсов.

## 10. Как добавлять новый runtime без переписывания Core

Добавление второго runtime допустимо только если изменение можно выполнить за счет следующего:

1. реализовать новый пакет adapter-а для runtime;
2. отобразить его возможности в уже существующие нормализованные runtime categories;
3. зарегистрировать adapter в runtime catalog и в тонком startup-модуле, который собирает активный интерфейсный процесс;
4. добавить adapter-specific translation tests;
5. не менять семантику домена Core, если сам канон проекта не расширился.

Если для поддержки нового runtime Core должен узнать vendor-specific types или изменить правила графа, значит архитектура выбрана неверно.

## 11. Критерии соответствия

Реализация соответствует этой модели только если одновременно сохраняется следующее:

- тесты domain и application запускаются без загрузки Codex client code;
- замена одного runtime adapter на другой не переписывает правила graph orchestration;
- Core-owned source narrowing не обходится через adapter fallback, а runtime-specific разрешение sources остается за границей adapter-а;
- Core видит нормализованные результаты, а не vendor-native event structures;
- неподдерживаемые возможности runtime сообщаются явно, а не симулируются внутри Core;
- metadata доступности и лимитов source показываются только тогда, когда адаптер открывает их через нормализованный путь inspection.
