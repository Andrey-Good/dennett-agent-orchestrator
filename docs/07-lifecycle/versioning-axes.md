[English](#english) | [Русский](#russian)

# English

## Versioning Axes

Status: normative owner for how lifecycle and contract version axes stay independent.

Related documents:

- [Canonical spec](../../agent_orchestrator_final_spec_v2.md)
- [Agent JSON contract](../03-contracts/agent-json/README.md)
- [Draft, Live, and Deploy](./draft-live-deploy.md)
- [Agent Registry](./agent-registry.md)
- [ADR-0001: Codex-First, Not Codex-Only](../09-adrs/ADR-0001-codex-first-not-codex-only.md)

## Why This Separation Exists

The project uses several different version notions because they answer different questions:

- Can this file be interpreted correctly?
- Which logical release of the agent is this?
- Which local revision is the current default?
- Which capabilities does this build of the product support?

Collapsing those questions into one number creates ambiguity in deploy flows, compatibility checks, and support decisions.

## The Four Required Axes

### `graph_contract_version`

This is the compatibility version for the graph format and semantics.

It is owned by the portable agent file and means only one thing: which graph-processing logic Core must understand in order to run the file correctly.

Core must reject a run when this version is unsupported. It must not guess, partially parse, or silently downgrade behavior.

### `meta.agent_version`

This is the logical version of the agent artifact and idea.

It is also owned by the portable agent file. It communicates author intent about the evolution of the agent itself, not the local deployment state and not the product build.

Core may display it, compare it, or use it in review workflows, but it must not treat it as the compatibility gate.

### `live revision`

This is the local working revision selected as the current live default.

It is owned by the local lifecycle surface and registry, not by the portable file schema. It may be an opaque id, monotonic counter, content hash, or another local revision marker, but it must remain a local working-surface concept.

Deploy changes the live revision even when `graph_contract_version` and `meta.agent_version` stay the same.

### Product version

This is the version of the orchestrator program itself.

It is owned by the software release process. It describes available product capabilities and the range of supported `graph_contract_version` values. It does not describe the logical maturity of one agent definition.

## Canonical Owners

Each axis has one owner:

- the agent file owns `graph_contract_version` and `meta.agent_version`;
- the lifecycle and registry surface owns the live revision;
- the product release process owns the tool version.

No axis may be copied into another owner and treated as equivalent truth.

## Required Behavioral Rules

The following behaviors are mandatory:

- unsupported `graph_contract_version` blocks execution before runtime launch;
- changing `meta.agent_version` does not publish a revision by itself;
- deploying a draft creates or selects a new live revision even if the logical agent version string is unchanged;
- upgrading the product may expand compatibility, but it does not rewrite stored agent versions or local live history.

## Practical Examples

These examples illustrate the separation:

- A prompt tweak with no contract change: same `graph_contract_version`, maybe same or new `meta.agent_version`, new live revision after deploy.
- A new graph capability requiring new parsing logic: new `graph_contract_version`, probably new `meta.agent_version`, new live revision after deploy, and possibly a newer product build requirement.
- A product update that adds support for an existing graph contract: new product version, unchanged agent file, unchanged live revision until the user deploys or selects another revision.

## What Must Not Happen

The system must not:

- use `meta.agent_version` as a substitute for contract compatibility;
- store the live revision as a portable field inside the agent JSON;
- infer deploy order from product version numbers;
- assume that the highest version-looking string is automatically the correct live choice.

## Relationship to Unsupported Drafts

A draft may exist locally even when its `graph_contract_version` is not yet runnable by the current Core.

That draft may be edited, inspected, or kept for future work, but it must not become the active live revision until the running product actually supports that contract version.

# Russian

## Оси версионирования

Статус: нормативный владелец того, как lifecycle- и contract-оси версий остаются независимыми.

Связанные документы:

- [Каноническая спецификация](../../agent_orchestrator_final_spec_v2.md)
- [Контракт Agent JSON](../03-contracts/agent-json/README.md)
- [Draft, Live и Deploy](./draft-live-deploy.md)
- [Реестр агентов](./agent-registry.md)
- [ADR-0001: Codex-First, Not Codex-Only](../09-adrs/ADR-0001-codex-first-not-codex-only.md)

## Зачем нужно это разделение

Проект использует несколько разных представлений версии, потому что они отвечают на разные вопросы:

- Можно ли корректно интерпретировать этот файл?
- Какой это логический релиз самого агента?
- Какая локальная ревизия сейчас считается версией по умолчанию?
- Какие возможности поддерживает данная сборка продукта?

Сведение этих вопросов к одному числу создает двусмысленность в deploy-flow, compatibility-проверках и support-решениях.

## Четыре обязательные оси

### `graph_contract_version`

Это версия совместимости формата графа и его семантики.

Ею владеет переносимый agent file, и она означает только одно: какую логику обработки графа Core должен понимать, чтобы корректно запустить этот файл.

Core обязан отклонять run, если эта версия не поддерживается. Он не должен угадывать, выполнять частичный parse или молча понижать поведение.

### `meta.agent_version`

Это логическая версия артефакта и идеи агента.

Ею тоже владеет переносимый agent file. Она выражает намерение автора относительно эволюции самого агента, а не локального deploy-state и не сборки продукта.

Core может показывать ее, сравнивать или использовать в review-workflow, но не должен воспринимать ее как gate совместимости.

### `live revision`

Это локальная рабочая ревизия, выбранная как текущий live-default.

Ей владеет локальная lifecycle-поверхность и реестр, а не переносимая схема файла. Она может быть opaque id, monotonic counter, content hash или другой локальной revision-меткой, но обязана оставаться понятием локальной рабочей поверхности.

Deploy меняет live revision даже тогда, когда `graph_contract_version` и `meta.agent_version` остаются прежними.

### Версия продукта

Это версия самой программы-оркестратора.

Ею владеет процесс релиза ПО. Она описывает доступные возможности продукта и диапазон поддерживаемых `graph_contract_version`. Она не описывает логическую зрелость конкретного определения агента.

## Канонические владельцы

У каждой оси есть один владелец:

- agent file владеет `graph_contract_version` и `meta.agent_version`;
- lifecycle- и registry-поверхность владеет live revision;
- процесс релиза продукта владеет версией утилиты.

Ни одна ось не может быть скопирована в другого владельца и трактоваться как эквивалентная истина.

## Обязательные поведенческие правила

Ниже перечислены обязательные правила поведения:

- неподдерживаемый `graph_contract_version` блокирует исполнение до запуска runtime;
- изменение `meta.agent_version` само по себе не публикует ревизию;
- deploy draft создает или выбирает новую live revision даже если строка логической версии агента не изменилась;
- обновление продукта может расширить совместимость, но не переписывает сохраненные версии агента и локальную историю live.

## Практические примеры

Эти примеры показывают разделение:

- Небольшая правка prompt без изменения контракта: тот же `graph_contract_version`, возможно тот же или новый `meta.agent_version`, новая live revision после deploy.
- Новая возможность графа, требующая новой логики разбора: новый `graph_contract_version`, вероятно новый `meta.agent_version`, новая live revision после deploy и, возможно, требование более новой сборки продукта.
- Обновление продукта, которое добавляет поддержку уже существующего graph contract: новая версия продукта, неизменный agent file, неизменная live revision до тех пор, пока пользователь не выполнит deploy или не выберет другую ревизию.

## Чего происходить не должно

Система не должна:

- использовать `meta.agent_version` как замену совместимости контракта;
- хранить live revision как переносимое поле внутри agent JSON;
- выводить порядок deploy из номеров версии продукта;
- считать, что строка, похожая на самую высокую версию, автоматически является правильным live-выбором.

## Связь с неподдерживаемыми drafts

Draft может существовать локально даже тогда, когда его `graph_contract_version` еще нельзя запускать текущим Core.

Такой draft можно редактировать, просматривать или хранить для будущей работы, но он не должен становиться активной live-ревизией, пока текущий продукт реально не поддерживает эту версию контракта.
