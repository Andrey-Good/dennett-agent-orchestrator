[English](#english) | [Русский](#russian)

# English

## Runtime Sources

Status: normative owner for the runtime-source extension.

Related documents:

- [Canonical spec](../../agent_orchestrator_final_spec_v2.md)
- [Runtime Integration Model](../02-architecture/runtime-integration-model.md)
- [Agent JSON contract](../03-contracts/agent-json/README.md)
- [Draft, Live, and Deploy](../07-lifecycle/draft-live-deploy.md)
- [Versioning Axes](../07-lifecycle/versioning-axes.md)
- [ADR-0001: Codex-First, Not Codex-Only](../09-adrs/ADR-0001-codex-first-not-codex-only.md)

## Purpose

Runtime sources let an agent definition constrain which concrete execution sources may be used inside one runtime family.

First Core chooses the runtime service or adapter family for the node. Then it resolves any concrete runtime source inside that family. This extension governs only the second step and does not replace the adapter implementation itself.

Examples include:

- multiple connected accounts;
- multiple API keys or subscriptions;
- multiple sessions or workspaces;
- multiple execution backends exposed by one adapter.

This extension exists so the agent file can express allowed launch surfaces without embedding raw credentials or taking over runtime internals.

## Agent-Level Declaration

The top-level `runtime_sources` field declares the named execution sources an agent may reference.

Each binding has these stable semantics:

| Field | Meaning |
| --- | --- |
| `id` | Stable identifier inside the agent file. |
| `runtime_adapter` | Which adapter family this source belongs to, such as `codex`. |
| `source_ref` | Opaque reference to a locally configured source. It is a handle, not a raw secret. |
| `description` | Optional human-readable description. |

If `runtime_sources` is absent, source selection is delegated entirely to local runtime configuration and interface choices outside the file.

If those outside-the-file choices later narrow execution to a specific configured source, Core still must pass that choice explicitly across the runtime boundary. The local source catalog itself remains out of scope for this extension.

## Matching Rule

A runtime source is eligible for a node only when its `runtime_adapter` matches the adapter used by that node.

Core must not route a node through a source from a different adapter family just because the local source exists.

## Boundary Rule

Source selection and source introspection are separate concerns.

- Core owns the effective eligible source set after combining adapter matching, node policy, and any user-side narrowing.
- The adapter family remains fixed before this step. Runtime-source rules do not authorize interfaces or adapters to swap out the runtime implementation path itself.
- If that effective set has been narrowed by declared `runtime_sources`, `runtime_source_policy`, or user choice, Core must choose one eligible source before the adapter call and pass it explicitly as `runtime_source`.
- `runtime_source` may be absent from the normalized execution request only when Core intentionally delegates source choice because no file-level, node-level, or user-level narrowing applies for that invocation.
- That delegated path allows the adapter to use its local default source, but it must never be used to bypass a narrowed eligible set.

When Core passes `runtime_source`, the boundary object is closed and MUST contain these fields:

| Field | Meaning |
| --- | --- |
| `id` | Selected source identifier. For file-declared sources, this equals the matching `runtime_sources[].id`. For sources surfaced only by local configuration or UI choice, this is the local configured-source identifier that Core resolved for the current run. |
| `runtime_adapter` | Adapter family for the selected source. It must match the node `runtime_adapter`. |
| `source_ref` | Opaque adapter-specific handle for the concrete source the adapter must resolve and execute against. |
| `description` | Optional human-readable label preserved when Core knows one. |

Outside-the-file `id` values are local configured-source identifiers, not portable agent-file ids. The adapter must treat `id` as an identity label for the already selected source and must use `source_ref` to resolve execution; it must not reopen source selection by reinterpreting `id`.

## Node-Level Selection

Nodes may declare:

- `runtime_source_ids`;
- `runtime_source_policy`.

This document fixes their meaning.

### `inherit`

`inherit` means the node does not narrow source selection beyond the agent-level declaration.

If the agent declares matching `runtime_sources`, all matching sources are eligible and Core owns any source selection or tie-breaking within that eligible set before the adapter call. Core must resolve one eligible source and pass it explicitly as `runtime_source`.

If the agent declares no matching `runtime_sources` for the node and the user has not narrowed source choice, Core may delegate source selection and omit `runtime_source`, allowing the adapter's local default source selection to apply.

Using `runtime_source_ids` together with `inherit` is invalid because it expresses both narrowing and non-narrowing at the same time.

### `restrict`

`restrict` requires a non-empty `runtime_source_ids` list.

Only the listed ids are eligible. Their order has no preference semantics.

Core may remove a listed source from consideration before launch only when supported runtime-source introspection for that exact resolved source reports `availability = unavailable` or `limit_status = exhausted`. `unknown` does not justify removal.

If one or more eligible sources remain after compatibility checks, user narrowing, and any supported removal above, Core must deterministically choose one source before the adapter call and pass it explicitly. The portable tie-break rule is the lexicographically smallest resolved `runtime_source.id`.

The node may fail before launch for source availability only when supported runtime-source introspection has established that every eligible listed source is unusable under the rule above.

### `prefer_first`

`prefer_first` also requires a non-empty `runtime_source_ids` list.

The listed ids form an ordered preference list and also the full eligible set for the node. Core must examine them in that order and choose the first source that is not already known unusable through supported runtime-source introspection for that exact source.

Core may skip a listed source before launch only when supported runtime-source introspection reports `availability = unavailable` or `limit_status = exhausted`. If that capability is unavailable, or if inspection returns only `unknown`, Core must not synthesize availability filtering and must instead choose the first eligible listed source in order.

If every listed source is explicitly known unusable under that rule, the node fails before launch. After Core has passed the resolved source, neither Core nor the adapter may auto-probe later listed sources unless another document explicitly defines a retry policy.

## User Choice vs Agent Allowlist

An interface may let the user further narrow the source choice for a run.

That user choice can only reduce the eligible set. It must never expand beyond what the node policy and agent-level declaration already allow.

If user narrowing leaves one or more eligible sources, Core still owns the final choice inside that reduced set and must pass the resolved `runtime_source` explicitly. If user narrowing empties the eligible set, launch fails before the adapter call.

## Limits and Availability

Source introspection is optional and separate from source selection.

If the adapter contract reports `supports_runtime_source_introspection`, Core may inspect a resolved `runtime_source` object that it can lawfully pass across the runtime boundary, whether that source came from declared `runtime_sources` or from a local source catalog surfaced outside the file. That metadata may be shown in local interfaces and cached as local metadata.

If the adapter contract does not report that capability, Core must assume this extension provides no portable availability or limit view for that source. Core should not fabricate synthetic runtime status or synthetic limits to look more complete than the underlying runtime really is.

Portable pre-launch source-availability knowledge exists only when this contract actually provides normalized inspection metadata for the exact resolved `runtime_source` object. Without that knowledge, Core may narrow by allowlists and user choice, but it must not claim that a source is currently available or unavailable.

This means availability-based pre-launch filtering or failure is allowed only for sources explicitly reported as unusable through `availability = unavailable` or `limit_status = exhausted`. `unknown` never authorizes synthetic failure or synthetic fallback.

This extension does not define portable inspection of an unnamed adapter default used only in delegated-selection mode.

## Local Capability Discovery Around A Runtime Source

App Server exposes useful local discovery surfaces around configured sources that are not portable agent-file fields.

Verified Codex-specific examples include:

- model discovery through `model/list`, including display metadata, hidden and default flags, supported reasoning efforts, default reasoning effort, input modalities, personality support, and additional speed tiers;
- local auth, account, config, and rate-limit introspection through `getAccount`, `getAccountRateLimits`, `config/read`, and `config/requirements/read`.

These capabilities may inform local source pickers, diagnostics, or later staging of model, effort, service-tier, or personality controls around a configured source.

They must not:

- be copied into portable `runtime_sources` entries as if they were stable file truth;
- invent portable agent-file fields for rate limits, account identity, speed tiers, or model catalogs;
- widen the eligible set beyond what the file, node, and user rules already allow.

Current-stage versus later-stage use:

- current-stage local UI or CLI may read and display this metadata, and the adapter may use it to explain why a configured source is selectable, limited, or exhausted. Phase 14 exposes the first normalized local surface for model discovery and runtime-environment inspection, but that still does not make those fields portable agent-file truth;
- later-stage launch policy may consume this metadata only after a dedicated contract or extension document owns that policy. Until then, allowlist narrowing, `runtime_source_policy`, and explicit runtime-source inspection results remain the only portable launch rules defined here.

## Cross-Agent Boundaries

Runtime source choices belong to the currently executing agent definition.

When an `orchestrator_agent` node launches another agent, the callee resolves its own runtime sources from its own definition. The caller's source allowlist does not automatically flow across that boundary.

## Failure Semantics

The following conditions are configuration, launch, or boundary-contract errors:

- a node references an unknown `runtime_source_id`;
- a referenced source belongs to a different `runtime_adapter` than the node;
- `restrict` or `prefer_first` is used with an empty source list;
- `inherit` is combined with an explicit source list;
- source choice has been narrowed by file-level, node-level, or user-level rules, but Core does not resolve and pass an explicit `runtime_source` before the adapter call;
- source choice has been narrowed, but the selected adapter does not support explicit runtime-source execution;
- supported runtime-source introspection has established that every eligible source is unusable when launch begins;
- the adapter executes against a different source than the one Core selected.

These failures should be explicit. Core must not silently fall back to an unrelated account, session, adapter, or widened source set.

## What This Extension Must Not Do

Runtime sources must not:

- store raw secrets in the portable agent file;
- replace the runtime adapter boundary with vendor-specific contract leakage;
- turn the agent file into a copy of local account configuration;
- treat adapter-default source selection as a back door around a narrowed allowlist;
- imply that every runtime family exposes the same source model.

# Russian

## Runtime Sources

Статус: нормативный владелец расширения runtime-source.

Связанные документы:

- [Каноническая спецификация](../../agent_orchestrator_final_spec_v2.md)
- [Модель интеграции runtime](../02-architecture/runtime-integration-model.md)
- [Контракт Agent JSON](../03-contracts/agent-json/README.md)
- [Draft, Live и Deploy](../07-lifecycle/draft-live-deploy.md)
- [Оси версионирования](../07-lifecycle/versioning-axes.md)
- [ADR-0001: Codex-First, Not Codex-Only](../09-adrs/ADR-0001-codex-first-not-codex-only.md)

## Назначение

Runtime sources позволяют определению агента ограничивать, какие именно concrete execution sources могут использоваться внутри одного runtime-family.

Сначала Core выбирает для ноды runtime service или family adapter-а. Затем он разрешает конкретный runtime source внутри этого family. Это расширение управляет только вторым шагом и не подменяет собой реализацию самого adapter-а.

Примеры:

- несколько подключенных аккаунтов;
- несколько API-ключей или подписок;
- несколько сессий или workspace;
- несколько execution backend, которые открывает один adapter.

Это расширение нужно затем, чтобы agent file мог выражать допустимые launch surfaces, не встраивая сырые credentials и не забирая на себя внутренности runtime.

## Объявление на уровне агента

Top-level поле `runtime_sources` объявляет именованные execution sources, на которые агент может ссылаться.

У каждого binding есть следующая стабильная семантика:

| Поле | Смысл |
| --- | --- |
| `id` | Стабильный идентификатор внутри agent file. |
| `runtime_adapter` | Семейство adapter, к которому относится источник, например `codex`. |
| `source_ref` | Opaque reference на локально настроенный source. Это handle, а не сырой secret. |
| `description` | Optional человекочитаемое описание. |

Если `runtime_sources` отсутствует, выбор source полностью делегируется локальной runtime-конфигурации и решениям интерфейса вне файла.

Если такие внефайловые решения позже сужают исполнение до конкретного настроенного source, Core все равно обязан явно передать этот выбор через runtime boundary. Сам локальный каталог sources остается вне области действия этого расширения.

## Правило соответствия

Runtime source считается допустимым для ноды только тогда, когда его `runtime_adapter` совпадает с adapter, используемым этой нодой.

Core не должен направлять ноду через источник из другого adapter-family только потому, что локально такой source существует.

## Правило Границы

Source selection и source introspection - это разные сущности.

- Core владеет эффективным допустимым множеством sources после объединения adapter matching, node policy и любого user-side narrowing.
- Family adapter-а к этому моменту уже зафиксировано. Правила runtime sources не дают интерфейсам или adapter-ам права менять сам путь реализации runtime.
- Если это эффективное множество уже сужено через объявленные `runtime_sources`, `runtime_source_policy` или user choice, Core обязан выбрать один допустимый source до вызова adapter и передать его явно как `runtime_source`.
- `runtime_source` может отсутствовать в нормализованном execution request только тогда, когда Core сознательно делегирует выбор source, потому что для этого запуска не действует ни file-level, ни node-level, ни user-level сужение.
- Такой делегированный путь позволяет adapter использовать свой локальный default source, но он никогда не должен служить обходом уже суженного допустимого множества.

Когда Core передает `runtime_source`, boundary object является закрытым и обязан содержать следующие поля:

| Поле | Смысл |
| --- | --- |
| `id` | Идентификатор выбранного source. Для объявленных в файле sources он совпадает с соответствующим `runtime_sources[].id`. Для sources, surfaced only by local configuration or UI choice, это идентификатор локально настроенного source, который Core разрешил для текущего run-а. |
| `runtime_adapter` | Семейство adapter-а выбранного source. Оно обязано совпадать с `runtime_adapter` ноды. |
| `source_ref` | Opaque adapter-specific handle конкретного source, который adapter обязан разрешить и использовать для исполнения. |
| `description` | Необязательная человеко-читаемая метка, если она известна Core. |

`id`, пришедшие не из файла, являются идентификаторами локально настроенных sources, а не переносимыми agent-file ids. Adapter обязан трактовать `id` только как identity label уже выбранного source и использовать для исполнения `source_ref`; он не должен заново открывать выбор source, переосмысляя `id`.

## Выбор на уровне ноды

Ноды могут объявлять:

- `runtime_source_ids`;
- `runtime_source_policy`.

Этот документ фиксирует их смысл.

### `inherit`

`inherit` означает, что нода не сужает выбор source сверх agent-level declaration.

Если агент объявляет подходящие `runtime_sources`, все matching sources считаются допустимыми, и Core владеет любым выбором источника и tie-break внутри этого допустимого множества до вызова adapter. Core обязан разрешить один допустимый source и явно передать его как `runtime_source`.

Если агент не объявляет ни одного matching `runtime_sources` для этой ноды и пользователь не сужал выбор source, Core может делегировать выбор source и не передавать `runtime_source`, позволяя применить локальный default source selection конкретного adapter.

Использование `runtime_source_ids` вместе с `inherit` является невалидным, потому что одновременно выражает сужение и отсутствие сужения.

### `restrict`

`restrict` требует непустого списка `runtime_source_ids`.

Допустимыми считаются только перечисленные ids. Их порядок не выражает семантику предпочтения.

Core может исключать перечисленный source из рассмотрения до запуска только тогда, когда поддерживаемая runtime-source introspection для этого уже разрешенного source сообщает `availability = unavailable` или `limit_status = exhausted`. Значение `unknown` не оправдывает исключение.

Если после compatibility checks, user narrowing и такого поддерживаемого исключения остается один или больше допустимых sources, Core обязан детерминированно выбрать один source до вызова adapter и явно передать его. Переносимое tie-break правило: лексикографически минимальный `runtime_source.id`.

Нода может завершиться ошибкой до запуска по причине доступности source только тогда, когда поддерживаемая runtime-source introspection установила, что каждый допустимый перечисленный source неиспользуем по правилу выше.

### `prefer_first`

`prefer_first` тоже требует непустого списка `runtime_source_ids`.

Перечисленные ids образуют упорядоченный список предпочтений и одновременно полный допустимый набор для ноды. Core обязан рассматривать их в указанном порядке и выбирать первый source, который еще не известен как неиспользуемый по поддерживаемой runtime-source introspection для этого source.

Core может пропустить перечисленный source до запуска только тогда, когда поддерживаемая runtime-source introspection сообщает `availability = unavailable` или `limit_status = exhausted`. Если эта capability недоступна или inspection возвращает только `unknown`, Core не должен синтезировать availability filtering и обязан выбрать первый допустимый listed source по порядку.

Если каждый listed source явно известен как неиспользуемый по этому правилу, нода завершается ошибкой до запуска. После того как Core передал разрешенный source, ни Core, ни adapter не должны автоматически пробовать следующие listed sources, если другой документ явно не определяет retry policy.

## Выбор пользователя и allowlist агента

Интерфейс может позволять пользователю дополнительно сужать выбор source для run-а.

Такой пользовательский выбор может только уменьшать множество допустимых источников. Он никогда не должен расширять его за пределы того, что уже разрешили node policy и agent-level declaration.

Если пользовательское сужение оставляет один или больше допустимых sources, Core все равно владеет финальным выбором внутри этого уменьшенного множества и обязан явно передать разрешенный `runtime_source`. Если пользовательское сужение делает допустимое множество пустым, запуск завершается ошибкой до вызова adapter.

## Лимиты и доступность

Source introspection является опциональной и отделена от source selection.

Если контракт adapter-а сообщает `supports_runtime_source_introspection`, Core может инспектировать разрешенный объект `runtime_source`, который он вправе передавать через runtime boundary, независимо от того, пришел этот source из объявленных `runtime_sources` или из локального каталога sources вне файла. Эти metadata могут показываться в локальных интерфейсах и кэшироваться как локальные метаданные.

Если контракт adapter-а не сообщает эту возможность, Core обязан считать, что это расширение не дает переносимого представления о доступности или лимитах этого source. Core не должен придумывать синтетический runtime status или синтетические лимиты, чтобы выглядеть полнее реального runtime.

Переносимое pre-launch знание о доступности source существует только тогда, когда этот контракт реально предоставляет нормализованные metadata инспекции для конкретного разрешенного объекта `runtime_source`. Без этой информации Core может сужать множество по allowlist-ам и user choice, но не должен утверждать, что source сейчас доступен или недоступен.

Это означает, что availability-based pre-launch filtering или failure разрешены только для sources, которые inspection явно пометила как неиспользуемые через `availability = unavailable` или `limit_status = exhausted`. Значение `unknown` никогда не разрешает синтетический failure или synthetic fallback.

Это расширение не определяет переносимую инспекцию безымянного adapter default, который используется только в делегированном режиме выбора.

## Локальный capability-discovery вокруг runtime source

App Server открывает полезные локальные поверхности discovery вокруг настроенных sources, которые не являются полями portable agent file.

Проверенные Codex-specific примеры включают:

- discovery моделей через `model/list`, включая display metadata, hidden/default flags, supported reasoning efforts, default reasoning effort, input modalities, personality support и additional speed tiers;
- локальную introspection auth/account/config/rate limits через `getAccount`, `getAccountRateLimits`, `config/read` и `config/requirements/read`.

Эти возможности могут помогать local source pickers, diagnostics или более позднему введению controls для model, effort, service tier или personality вокруг настроенного source.

Они не должны:

- копироваться внутрь portable `runtime_sources` entries так, будто это стабильная файловая истина;
- изобретать portable agent-file fields для rate limits, account identity, speed tiers или model catalogs;
- расширять допустимое множество сверх того, что уже разрешили file-, node- и user-правила.

Current-stage versus later-stage использование:

- на current stage local UI или CLI могут читать и показывать эту metadata, а adapter может использовать ее, чтобы объяснять, почему настроенный source selectable, limited или exhausted;
- later-stage launch policy может опираться на эту metadata только после того, как профильный contract- или extension-документ-владелец задаст такую policy. До этого allowlist narrowing, `runtime_source_policy` и явные результаты runtime-source inspection остаются единственными portable launch rules, определенными здесь.

## Межагентные границы

Выбор runtime source принадлежит определению агента, которое исполняется в данный момент.

Когда нода `orchestrator_agent` запускает другого агента, вызываемый агент разрешает свои runtime sources из собственного определения. Allowlist источников вызывающего агента не переносится через эту границу автоматически.

## Семантика отказов

Следующие условия являются configuration-, launch- или boundary-contract-ошибками:

- нода ссылается на неизвестный `runtime_source_id`;
- указанный источник относится к другому `runtime_adapter`, чем сама нода;
- `restrict` или `prefer_first` используются с пустым списком sources;
- `inherit` комбинируется с явным списком sources;
- выбор source уже сужен file-level, node-level или user-level правилами, но Core не разрешает и не передает явный `runtime_source` до вызова adapter;
- выбор source уже сужен, но выбранный adapter не поддерживает explicit runtime-source execution;
- поддерживаемая runtime-source introspection установила, что в момент запуска каждый допустимый source неиспользуем;
- adapter выполняет запрос против source, отличного от того, который выбрал Core.

Эти отказы должны быть явными. Core не должен молча делать fallback на несвязанный аккаунт, сессию, adapter или повторно расширенное source-множество.

## Чего это расширение делать не должно

Runtime sources не должны:

- хранить сырые secrets в переносимом agent file;
- подменять границу runtime adapter утечкой vendor-specific контракта;
- превращать agent file в копию локальной конфигурации аккаунтов;
- превращать adapter-default source selection в обход уже суженного allowlist;
- подразумевать, что каждое runtime-family открывает одну и ту же модель sources.
