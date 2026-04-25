[English](#english) | [Русский](#russian)

<a id="english"></a>
# Core And Interfaces

Status: approved.

Related documents:

- [`README.md`](./README.md)
- [`runtime-integration-model.md`](./runtime-integration-model.md)
- [`../01-foundations/technology-stack.md`](../01-foundations/technology-stack.md)
- [`../03-contracts/README.md`](../03-contracts/README.md)
- [`../04-execution/README.md`](../04-execution/README.md)
- [`../05-state/README.md`](../05-state/README.md)
- [`../07-lifecycle/README.md`](../07-lifecycle/README.md)
- [`../09-adrs/ADR-0001-codex-first-not-codex-only.md`](../09-adrs/ADR-0001-codex-first-not-codex-only.md)

This document defines the operational split between `Core` and `Interfaces` and the internal layout that future code should follow. Externally the product has two levels, exactly as defined by the canon. Internally the Core side still needs disciplined structure so orchestration logic, persistence, and runtime integration do not collapse into one layer.

`adapters/*` in the examples below is not a third product level. It is part of the Core side of the system, organized as outer implementations of Core-owned ports. In repository terms, the Core side spans `src/core`, `src/ports`, and `src/adapters`.

## 1. Normative System Shape

The system has two product levels:

- `Core` is the single home of graph orchestration, agent execution coordination, event handling, chat and resume storage boundaries, agent file operations, and runtime adapter coordination.
- `Interfaces` are user-facing shells over Core: CLI now, UI later, and other entrypoints only if they remain thin. They are not runtime implementations.

The architectural rule is simple: interfaces talk to Core, but project logic does not migrate into interfaces.

## 2. Locked Source Layout

The top-level `src/` shape is already locked. Future code must fit inside it instead of inventing competing trees. A compliant implementation should look close to this:

```text
src/
  core/
    domain/
    application/
  ports/
    runtime/
    agents/
    state/
    registry/
    events/
  adapters/
    runtime/
      codex/
    persistence/
      filesystem/
      sqlite/
    events/
  interfaces/
    cli/
    ui/                  # future
  resources/
```

The intent of this layout is:

- `src/core/domain` contains pure orchestration concepts and invariants.
- `src/core/application` contains use cases and coordination logic.
- `src/ports/*` contains technology-neutral interfaces owned by Core.
- `src/adapters/*` contains concrete implementations for runtime, files, SQLite, and external event sources.
- `src/interfaces/*` contains presentation, user interaction, and thin startup modules for each interface process.
- `src/resources/*` contains non-code assets required by the product without becoming a second logic layer.
- Bootstrapping and object-graph wiring live in thin startup modules under the relevant locked subtree, usually the active `src/interfaces/*` entrypoint, rather than in a separate top-level assembly directory.

## 3. Responsibilities By Repository Area

| Repository area | Owns | May depend on | Must not know |
| --- | --- | --- | --- |
| `src/core/domain` | graph concepts, node-kind meaning, normalized run concepts, permission and capability references, invariant checks | language/runtime standard library and other domain modules | CLI commands, UI state, SQLite schema, file formats beyond the domain view, vendor SDKs |
| `src/core/application` | start run, resume run, comment on run, invoke child agent, coordinate registry and draft/live actions, orchestrate state transitions | `src/core/domain` and `src/ports/*` | vendor types, direct SQL calls, direct SDK calls, terminal rendering |
| `src/ports/*` | runtime gateway, agent file repository boundary, state store boundary, registry boundary, event ingress/egress boundaries | `src/core/domain` types only | Codex client types, SQLite driver types, CLI/UI types |
| `src/adapters/*` | translation to concrete technologies such as Codex App Server client/runtime code, filesystem, SQLite, external triggers | `src/ports/*`, adapter-local helpers, external libraries | graph control flow, canonical agent definition, interface policy |
| `src/interfaces/*` | argument parsing, screen rendering, input collection, progress display, command/session UX, and thin per-interface startup modules | `src/core/application`, interface-local mappers, and concrete adapter factories only during process assembly | next-node decisions, storage schema, vendor SDK APIs beyond startup wiring, graph mutation rules |
| `src/resources/*` | non-code prompts, templates, fixtures, or bundled assets needed by runtime support or interfaces | no application code; referenced by `src/core`, `src/adapters`, or `src/interfaces` | domain rules, SDK integration logic, or business decisions |

## 4. Dependency Direction

The stable dependency graph is:

```text
src/interfaces/*  -->  src/core/application  -->  src/core/domain
                                       |
                                       v
                                    src/ports/*
                                       ^
                                       |
                                  src/adapters/*
```

Thin startup modules under `src/interfaces/*` may instantiate concrete adapters and hand them to Core. That assembly role does not justify a separate top-level assembly tree.

The following rules are mandatory:

1. `src/core/domain` imports only domain code and language-level utilities.
2. `src/core/application` may depend on domain types and Core-owned ports, but never on concrete adapters.
3. `src/ports/*` defines contracts that adapters implement; ports never import adapter code.
4. User-facing modules under `src/interfaces/*` may call application services or query facades only. Startup modules in that same subtree may assemble the object graph, but they may not bypass Core for runtime or storage behavior.
5. `src/adapters/*` may depend on external libraries, but vendor or storage-specific types must stop at the adapter boundary.
6. Vendor types are forbidden outside adapter packages even as exported TypeScript types.
7. If one adapter needs data from another adapter, the coordination belongs in `src/core/application` or in the thin startup module that assembles the process, not in an adapter-to-adapter import chain.

## 5. What Core Must Know, And What It Must Not Know

Core must know:

- how to load and interpret the orchestrator's own agent model;
- how to execute graph control flow;
- how to decide whether a node is `runtime_agent` or `orchestrator_agent`;
- how to route final node outputs into graph-visible state;
- how to coordinate registry, drafts, live versions, events, chats, and resume state as Core responsibilities.

Core must not know:

- `commander`, terminal formatting rules, browser routing, or UI framework state models;
- Codex client classes, event names, error shapes, or session objects;
- hidden runtime internals such as tool-call traces or chain-of-thought;
- vendor-specific schemas for skills, MCPs, plugins, memory backends, or execution sources beyond the minimum references Core is responsible for selecting.

## 6. What Interfaces May Do, And What They Must Not Do

Interfaces may:

- parse user input and turn it into Core commands;
- render run progress and final outputs;
- expose controls for start, resume, comment, inspect, deploy, and similar actions;
- subscribe to normalized Core events.

Interfaces must not:

- choose graph transitions or decide the next node;
- implement native resume or fallback resume logic on their own;
- read from or write to SQLite, agent files, or runtime client code directly;
- shell out to a vendor CLI as the product's runtime execution path;
- interpret `runtime_options` or vendor-specific capabilities beyond presentation metadata;
- create a second lifecycle policy for closing the interface.

The thin startup module for an interface process, not the view/controller module, owns the `keep_core_running` versus `stop_core` policy. The default remains `keep_core_running`, and the architecture must preserve the possibility that Core lifetime is longer than interface lifetime.

## 7. Special Architectural Rules

The following rules are specific to this project and are not generic clean-architecture advice:

- `runtime_agent` nodes always cross the runtime boundary through the runtime port.
- `orchestrator_agent` nodes never call a runtime adapter for the parent hop; Core loads another agent file and recursively invokes orchestration.
- The builder agent is still an orchestrated agent handled by Core, not a hidden UI subsystem.
- Registry, drafts, live revisions, and deploy actions are Core capabilities exposed by interfaces, not implemented inside interfaces.
- Skills, MCPs, plugins, permissions, memory bindings, and runtime source ids are selected by Core and translated by adapters; interfaces do not reinterpret their vendor meaning.
- When Codex is the selected runtime family, execution still goes through the App Server-native Codex adapter; the product CLI is not a second Codex adapter.

## 8. Testing And Review Implications

The architecture should be visible in the test strategy:

- domain and application tests run without loading Codex client code or a SQLite driver;
- adapter tests prove translation between Core ports and concrete technologies;
- interface tests focus on command wiring and rendering, not orchestration rules;
- static review should be able to verify that imports of Codex client code appear only under `src/adapters/runtime/codex` or another runtime-adapter subtree.

## 9. Compliance Checklist

An implementation respects this document only if all of the following stay true:

- removing the CLI package does not remove orchestration logic;
- adding a future UI does not require moving business logic out of Core;
- Core services can be instantiated with test doubles for runtime and storage ports;
- no direct vendor SDK import appears in domain, application, ports, or interfaces;
- SQLite remains an implementation detail of persistence adapters, not the canonical model of the agent graph.

<a id="russian"></a>
# Core и интерфейсы

Статус: утверждено.

Связанные документы:

- [`README.md`](./README.md)
- [`runtime-integration-model.md`](./runtime-integration-model.md)
- [`../01-foundations/technology-stack.md`](../01-foundations/technology-stack.md)
- [`../03-contracts/README.md`](../03-contracts/README.md)
- [`../04-execution/README.md`](../04-execution/README.md)
- [`../05-state/README.md`](../05-state/README.md)
- [`../07-lifecycle/README.md`](../07-lifecycle/README.md)
- [`../09-adrs/ADR-0001-codex-first-not-codex-only.md`](../09-adrs/ADR-0001-codex-first-not-codex-only.md)

Этот документ задает операционное разделение между `Core` и `Interfaces` и внутреннюю раскладку, которой должна следовать будущая реализация. Снаружи у продукта ровно два уровня, как и зафиксировано каноном. Внутри стороны Core все равно нужна дисциплина, чтобы логика оркестрации, persistence и runtime integration не схлопнулись в один слой.

`adapters/*` в примерах ниже не образует третий продуктовый уровень. Это часть стороны Core, оформленная как внешние реализации Core-owned ports. На уровне репозитория сторона Core разложена по `src/core`, `src/ports` и `src/adapters`.

## 1. Нормативная форма системы

У системы есть два продуктовых уровня:

- `Core` - единственный дом графовой оркестрации, координации исполнения агентов, обработки событий, границ хранения chats и resume, операций с agent files и координации runtime adapters.
- `Interfaces` - пользовательские оболочки поверх Core: сейчас CLI, позже UI и другие точки входа только если они остаются тонкими. Они не являются runtime-реализациями.

Архитектурное правило простое: интерфейсы обращаются к Core, но логика проекта не мигрирует в интерфейсы.

## 2. Зафиксированная раскладка исходников

Верхнеуровневая форма `src/` уже зафиксирована. Будущий код должен укладываться в нее, а не изобретать конкурирующие деревья. Корректная реализация должна быть близка к такой:

```text
src/
  core/
    domain/
    application/
  ports/
    runtime/
    agents/
    state/
    registry/
    events/
  adapters/
    runtime/
      codex/
    persistence/
      filesystem/
      sqlite/
    events/
  interfaces/
    cli/
    ui/                  # future
  resources/
```

Смысл этой раскладки такой:

- `src/core/domain` содержит чистые концепты оркестрации и инварианты.
- `src/core/application` содержит use cases и координационную логику.
- `src/ports/*` содержит технологически-нейтральные интерфейсы, которыми владеет Core.
- `src/adapters/*` содержит конкретные реализации для runtime, файлов, SQLite и внешних источников событий.
- `src/interfaces/*` содержит presentation, пользовательское взаимодействие и тонкие startup-модули для каждого интерфейсного процесса.
- `src/resources/*` содержит некодовые ресурсы, нужные продукту, но не превращается во второй слой логики.
- Bootstrapping и object-graph wiring живут в тонких startup-модулях внутри подходящего зафиксированного поддерева, обычно во входной точке `src/interfaces/*`, а не в отдельном верхнеуровневом сборочном каталоге.

## 3. Ответственность по областям репозитория

| Область репозитория | Чем владеет | От чего может зависеть | Чего знать не должен |
| --- | --- | --- | --- |
| `src/core/domain` | концептами графа, смыслом видов нод, нормализованными понятиями run, ссылками на permissions и capabilities, инвариантами | стандартной библиотекой языка/среды и другими domain-модулями | CLI-командами, UI-state, схемой SQLite, файловыми форматами за пределами доменного взгляда, vendor client |
| `src/core/application` | запуском run, resume, live comments, вызовом дочернего агента, координацией registry и draft/live действий, переходами состояния | `src/core/domain` и `src/ports/*` | vendor types, прямыми SQL-вызовами, прямыми client-вызовами, terminal rendering |
| `src/ports/*` | runtime gateway, границей repository для agent files, границей state store, границей registry, границами event ingress/egress | только типами из `src/core/domain` | типами Codex client, типами SQLite driver, типами CLI/UI |
| `src/adapters/*` | переводом в конкретные технологии, такие как Codex App Server client/runtime code, filesystem, SQLite и внешние triggers | `src/ports/*`, локальными helper-модулями adapter-а и внешними библиотеками | control flow графа, каноническим описанием агента, политикой интерфейсов |
| `src/interfaces/*` | разбором аргументов, рендерингом экранов, сбором ввода, показом прогресса, UX команд и сессий, а также тонкими startup-модулями интерфейса | `src/core/application`, локальными mapper-ами интерфейса и фабриками конкретных adapter-ов только во время сборки процесса | решением о следующей ноде, схемой хранения, API vendor SDK за пределами startup wiring, правилами мутации графа |
| `src/resources/*` | некодовыми prompts, templates, fixtures и bundled assets, нужными runtime support или интерфейсам | не содержит application code; на него могут ссылаться `src/core`, `src/adapters` или `src/interfaces` | доменными правилами, логикой SDK-интеграции или business decisions |

## 4. Направление зависимостей

Устойчивый граф зависимостей такой:

```text
src/interfaces/*  -->  src/core/application  -->  src/core/domain
                                       |
                                       v
                                    src/ports/*
                                       ^
                                       |
                                  src/adapters/*
```

Тонкие startup-модули внутри `src/interfaces/*` могут создавать конкретные adapters и передавать их в Core. Эта сборочная роль не оправдывает отдельное верхнеуровневое сборочное дерево.

Обязательные правила:

1. `src/core/domain` импортирует только domain-код и utilities уровня языка.
2. `src/core/application` может зависеть от доменных типов и принадлежащих Core портов, но никогда не зависит от конкретных adapter-ов.
3. `src/ports/*` определяет контракты, которые реализуют adapter-ы; ports никогда не импортируют код adapter-ов.
4. Пользовательские модули внутри `src/interfaces/*` могут вызывать только application services или query facades. Startup-модули в том же поддереве могут собирать object graph, но не имеют права обходить Core для runtime- или storage-поведения.
5. `src/adapters/*` могут зависеть от внешних библиотек, но vendor-specific и storage-specific типы должны останавливаться на границе adapter-а.
6. Vendor types запрещены вне пакетов adapter-ов даже как экспортируемые TypeScript types.
7. Если одному adapter-у нужны данные от другого, координация должна жить в `src/core/application` или в тонком startup-модуле, который собирает процесс, а не в цепочке импорта adapter -> adapter.

## 5. Что Core должен знать, а чего нет

Core обязан знать:

- как загрузить и интерпретировать собственную agent model оркестратора;
- как исполнять control flow графа;
- как определить, является ли нода `runtime_agent` или `orchestrator_agent`;
- как маршрутизировать финальные output ноды в видимое для графа состояние;
- как координировать registry, drafts, live versions, events, chats и resume state как обязанности Core.

Core не должен знать:

- `commander`, правила terminal formatting, browser routing или state-модели UI-фреймворков;
- классы, event names, error shapes и session objects из Codex client code;
- скрытые внутренности runtime, такие как tool-call traces или chain-of-thought;
- vendor-specific схемы для skills, MCPs, plugins, memory backends или execution sources сверх минимальных ссылок, которые Core обязан выбирать.

## 6. Что интерфейсы могут делать, а что нет

Интерфейсы могут:

- разбирать пользовательский ввод и превращать его в команды Core;
- рендерить прогресс run и финальные outputs;
- давать пользователю управление запуском, resume, комментариями, просмотром, deploy и подобными действиями;
- подписываться на нормализованные события Core.

Интерфейсы не должны:

- выбирать переходы графа или решать, какая нода будет следующей;
- самостоятельно реализовывать native resume или fallback resume;
- напрямую читать или писать в SQLite, agent files или runtime client code;
- делать shell-out в vendor CLI как в продуктовый путь исполнения runtime;
- интерпретировать `runtime_options` или vendor-specific capabilities глубже presentation metadata;
- создавать собственную lifecycle-policy для закрытия интерфейса.

Тонкий startup-модуль интерфейсного процесса, а не view/controller, владеет политикой `keep_core_running` versus `stop_core`. Политика по умолчанию остается `keep_core_running`, и архитектура должна сохранять возможность того, что жизненный цикл Core дольше жизненного цикла интерфейса.

## 7. Специальные архитектурные правила

Следующие правила специфичны именно для этого проекта и не являются общими советами из абстрактной clean architecture:

- ноды `runtime_agent` всегда пересекают runtime-boundary через runtime port;
- ноды `orchestrator_agent` никогда не вызывают runtime adapter для родительского шага; Core загружает другой agent file и рекурсивно запускает оркестрацию;
- builder agent остается обычным orchestrated agent, которым управляет Core, а не скрытой UI-подсистемой;
- registry, drafts, live revisions и deploy actions - это возможности Core, которые интерфейсы только открывают пользователю;
- skills, MCPs, plugins, permissions, memory bindings и runtime source ids выбираются Core и транслируются adapters; интерфейсы не переосмысляют их vendor-значение.
- Когда выбран runtime-family Codex, исполнение все равно проходит через App Server-native Codex adapter; продуктовый CLI не является вторым Codex adapter-ом.

## 8. Последствия для тестов и ревью

Архитектура должна быть видна в стратегии тестирования:

- тесты domain и application запускаются без загрузки Codex client code или SQLite driver;
- тесты adapter-ов доказывают трансляцию между Core ports и конкретными технологиями;
- тесты интерфейсов проверяют wiring команд и rendering, а не правила оркестрации;
- статическое ревью должно позволять убедиться, что импорты Codex client code встречаются только внутри `src/adapters/runtime/codex` или другого поддерева runtime-adapter-а.

## 9. Чеклист соответствия

Реализация уважает этот документ только если одновременно выполняется следующее:

- удаление CLI-пакета не удаляет логику оркестрации;
- добавление будущего UI не требует переносить business logic из Core;
- сервисы Core можно создать с test doubles для runtime и storage ports;
- в domain, application, ports и interfaces нет прямых импортов vendor SDK;
- SQLite остается деталью реализации persistence adapters, а не канонической моделью графа агента.
