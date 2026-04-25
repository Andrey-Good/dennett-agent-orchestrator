[English](#english) | [Russian](#russian)

<a id="english"></a>
# Phase 18 Integrated Product Flows

Status: owner note for the Phase 18 integration slice.

## Goal

Phase 18 turns separately completed subsystem slices into coherent product-flow evidence. The goal is to show that builder output, lifecycle publishing, runtime capability selection, user interaction, memory bindings, and managed subagent orchestration can be used together without hidden ownership conflicts.

The phase is successful when the product can describe and validate realistic multi-feature flows that cross subsystem boundaries while still routing detailed behavior back to each subsystem owner.

## Product-Flow Definition

An integrated product flow is a documented and validated path that:

- starts from a user-visible intent, such as authoring, revising, running, or coordinating an agent;
- crosses at least three major subsystem surfaces;
- records which subsystem owns each state transition and decision;
- defines what evidence proves the flow locally;
- explicitly lists what remains unproven until Phase 19.

The flow may use local runtime adapters, fake providers, test doubles, or dry-run execution where the subsystem contract allows them. Such evidence proves integration behavior only; it does not prove external provider reliability, live service capacity, or release readiness.

## Owned Integration Questions

Phase 18 owns questions such as:

- whether a builder-authored draft can move through validation, lifecycle persistence, deploy, and execution without bypassing owner gates;
- whether runtime capability metadata and memory capability metadata remain separate and are checked at the right boundaries;
- whether user prompts, replies, comments, and wait states remain durable when a flow also uses managed subagents;
- whether managed subagent task packages, findings, budgets, close states, and write scopes remain separate from portable agent JSON;
- whether lifecycle revision identity stays stable when builder revisions, runtime options, and memory bindings interact;
- whether a flow can fail safely with an actionable owner-specific error when a capability is missing or a conflict is detected.

## Non-Goals

Phase 18 does not:

- create new subsystem semantics;
- add new portable agent fields;
- treat builder confidence as validation proof;
- treat a memory binding as proof that an external provider is ready;
- treat local runtime metadata as portable file truth;
- claim broad managed-subagent live proof;
- define production release criteria or operational runbooks.

## Phase 18 Versus Phase 19

Phase 18 proves internal product coherence. It should use executable local evidence wherever possible and may use documented test doubles or dry runs for unavailable external dependencies.

Phase 19 proves real-world readiness. It owns live end-to-end proof against real runtimes and providers, stress and regression evidence, operational runbooks, final release criteria, and the release decision.

If a scenario passes only with local adapters, fake providers, or dry-run steps, it may satisfy Phase 18 but must be labeled as not satisfying Phase 19.

## Completion Criteria

Phase 18 is complete only when:

- each selected integrated flow has a named owner path and acceptance evidence;
- cross-subsystem conflicts are handled by explicit rules rather than implicit ordering;
- failures route to the subsystem that owns the violated contract;
- acceptance scenarios cover successful and negative cases;
- documentation avoids release-readiness claims unless Phase 19 evidence exists.

<a id="russian"></a>
# Phase 18 Integrated Product Flows

Статус: заметка-владелец для интеграционного среза Phase 18.

## Цель

Phase 18 превращает отдельно завершенные срезы подсистем в согласованные доказательства продуктовых потоков. Цель состоит в том, чтобы показать, что builder output, lifecycle publishing, runtime capability selection, user interaction, memory bindings и managed subagent orchestration могут использоваться вместе без скрытых конфликтов владения.

Фаза успешна, когда продукт может описывать и проверять реалистичные многофункциональные потоки, пересекающие границы подсистем, и при этом по-прежнему направлять детальное поведение к владельцу каждой подсистемы.

## Определение продуктового потока

Интегрированный продуктовый поток - это документированный и проверенный путь, который:

- начинается с видимого пользователю намерения, например authoring, revising, running или coordinating an agent;
- пересекает как минимум три основные поверхности подсистем;
- фиксирует, какая подсистема владеет каждым переходом состояния и решением;
- определяет, какие доказательства локально подтверждают поток;
- явно перечисляет, что остается недоказанным до Phase 19.

Поток может использовать локальные runtime adapters, fake providers, test doubles или dry-run execution там, где контракт подсистемы это допускает. Такие доказательства подтверждают только интеграционное поведение; они не доказывают надежность внешнего provider, емкость live service или release readiness.

## Интеграционные вопросы во владении

Phase 18 владеет такими вопросами:

- может ли builder-authored draft пройти validation, lifecycle persistence, deploy и execution без обхода owner gates;
- остаются ли runtime capability metadata и memory capability metadata разделенными и проверяются ли они на правильных границах;
- остаются ли user prompts, replies, comments и wait states durable, когда поток также использует managed subagents;
- остаются ли task packages, findings, budgets, close states и write scopes managed subagent отдельно от portable Agent JSON;
- остается ли lifecycle revision identity стабильной при взаимодействии builder revisions, runtime options и memory bindings;
- может ли поток безопасно завершиться с actionable owner-specific error, когда capability отсутствует или обнаружен conflict.

## Не-цели

Phase 18 не:

- создает новую семантику подсистем;
- добавляет новые переносимые поля agent;
- считает уверенность builder доказательством validation;
- считает memory binding доказательством готовности внешнего provider;
- считает локальные runtime metadata переносимой file truth;
- заявляет broad managed-subagent live proof;
- определяет production release criteria или operational runbooks.

## Phase 18 и Phase 19

Phase 18 доказывает внутреннюю продуктовую согласованность. Она должна использовать исполнимые локальные доказательства везде, где это возможно, и может использовать документированные test doubles или dry runs для недоступных внешних зависимостей.

Phase 19 доказывает готовность к реальному миру. Она владеет live end-to-end proof against real runtimes and providers, stress and regression evidence, operational runbooks, final release criteria и release decision.

Если сценарий проходит только с local adapters, fake providers или dry-run steps, он может удовлетворять Phase 18, но должен быть помечен как не удовлетворяющий Phase 19.

## Критерии завершения

Phase 18 завершена только когда:

- каждый выбранный integrated flow имеет named owner path и acceptance evidence;
- cross-subsystem conflicts обрабатываются явными правилами, а не неявным порядком;
- failures маршрутизируются к подсистеме, которая владеет нарушенным contract;
- acceptance scenarios покрывают успешные и негативные случаи;
- документация избегает заявлений о release readiness, если нет evidence Phase 19.
