[English](#english) | [Русский](#russian)

<a id="english"></a>
# Architecture

This section turns the project canon into code boundaries. It explains where orchestration logic lives, where runtime-specific code is allowed to exist, and how those rules fit the locked `src/` layout: `src/core`, `src/ports`, `src/adapters`, `src/interfaces`, and `src/resources`.

This `README.md` is navigational. Normative rules live in the leaf documents listed below.

## Owner Documents In This Section

- [`core-and-interfaces.md`](./core-and-interfaces.md) defines the split between Core and user-facing interfaces, the allowed dependency direction, and how Core-owned modules fit inside the locked repository shape.
- [`runtime-integration-model.md`](./runtime-integration-model.md) defines the architectural boundary between Core and external runtimes such as Codex App Server, including what the runtime port must isolate, which App Server-native capability families are current-stage versus later-stage, and what the adapter layer is forbidden to know.
- [`subagent-orchestration-model.md`](./subagent-orchestration-model.md) defines the subagent system as a Core-owned orchestration primitive, including delegation roles, child-task boundaries, nested spawning rules, budgets, and review-loop structure.

Memory is handled in this section as a provider-neutral internal capability behind the runtime adapter layer. The portability rules for memory bindings live in [../03-contracts/agent-json/memory-binding-model-contract.md](../03-contracts/agent-json/memory-binding-model-contract.md) and [../08-extensions/memory-bindings.md](../08-extensions/memory-bindings.md).

## What This Section Owns

- the operational split between `Core` and `Interfaces`;
- the placement of Core-owned logic across `src/core`, `src/ports`, and `src/adapters` without inventing a third product layer;
- the dependency direction between domain code, ports, adapters, persistence, resources, and interfaces;
- the architectural boundary around runtime adapters, execution sources, and staged App Server-native capability families.
- the provider-neutral memory layer, provider-adapter split, and capability negotiation boundary for memory bindings.

## What This Section Does Not Own

- field-level `agent JSON` rules or JSON Schema details; those belong to [`03-contracts`](../03-contracts/README.md);
- graph execution semantics, node outcomes, and detailed resume behavior; those belong to [`04-execution`](../04-execution/README.md) and [`05-state`](../05-state/README.md);
- lifecycle workflows such as registry, `draft`, `live`, and `deploy`; those belong to [`07-lifecycle`](../07-lifecycle/README.md);
- disputed or stack-changing decisions; those belong to [`09-adrs`](../09-adrs/README.md).

## How To Use This Section During Implementation

| If the question is... | Read... |
| --- | --- |
| Where should graph orchestration logic live in the locked repository shape? | [`core-and-interfaces.md`](./core-and-interfaces.md) |
| May CLI or future UI talk to SQLite or runtime adapters directly? | [`core-and-interfaces.md`](./core-and-interfaces.md) |
| Where may Codex App Server client/runtime code be imported? | [`runtime-integration-model.md`](./runtime-integration-model.md) and [`../01-foundations/technology-stack.md`](../01-foundations/technology-stack.md) |
| How are runtime features such as native resume or live comments integrated without leaking vendor types? | [`runtime-integration-model.md`](./runtime-integration-model.md) |
| Where do model discovery, reasoning and speed-tier metadata, review threads, or richer App Server notifications belong? | [`runtime-integration-model.md`](./runtime-integration-model.md) |
| What is a subagent in product terms, and how should delegation/review loops be modeled? | [`subagent-orchestration-model.md`](./subagent-orchestration-model.md) |
| What exact fields exist in agent files? | [`../03-contracts/README.md`](../03-contracts/README.md) |
| What exactly gets persisted for chats, resume, registry, or drafts? | [`../05-state/README.md`](../05-state/README.md) and [`../07-lifecycle/README.md`](../07-lifecycle/README.md) |

## Adjacent Source-Of-Truth Map

- [`../01-foundations/README.md`](../01-foundations/README.md) and [`../01-foundations/technology-stack.md`](../01-foundations/technology-stack.md) provide the ideology, stack lock, and vendor-isolation guardrails this section must preserve.
- [`../03-contracts/README.md`](../03-contracts/README.md) provides machine-checkable and file-level contracts that architecture must not duplicate.
- [`../04-execution/README.md`](../04-execution/README.md) owns time-based execution behavior after architecture chooses the responsible layer.
- [`../05-state/README.md`](../05-state/README.md) owns state, resume, and persistence details once architecture assigns the storage boundary.
- [`../07-lifecycle/README.md`](../07-lifecycle/README.md) owns registry and `draft/live/deploy` rules once architecture assigns them to Core.
- [`../08-extensions/memory-bindings.md`](../08-extensions/memory-bindings.md) owns the memory-extension architecture, including provider adapters, MCP transport placement, and failure semantics.
- [`../09-adrs/ADR-0001-codex-first-not-codex-only.md`](../09-adrs/ADR-0001-codex-first-not-codex-only.md) records the non-negotiable `codex-first, not codex-only` decision behind this section.

## Review Standard For This Section

An architecture change is acceptable only if it keeps all of the following true:

- Core remains the single home of orchestration logic.
- Interfaces stay thin and replaceable.
- Vendor client knowledge stays inside runtime adapters.
- `agent JSON` remains the source of truth for agent definitions.
- SQLite stays a local metadata store rather than becoming the canonical agent model.
- The docs do not invent a competing top-level `src/` tree.

<a id="russian"></a>
# Архитектура

Этот раздел переводит канон проекта в границы кода. Здесь фиксируется, где живет логика оркестрации, где разрешен runtime-specific код и как эти правила укладываются в зафиксированную раскладку `src/`: `src/core`, `src/ports`, `src/adapters`, `src/interfaces` и `src/resources`.

Этот `README.md` навигационный. Нормативные правила находятся в профильных документах ниже.

## Документы-владельцы в разделе

- [`core-and-interfaces.md`](./core-and-interfaces.md) фиксирует разделение между Core и пользовательскими интерфейсами, разрешенное направление зависимостей и то, как Core-owned модули укладываются в зафиксированную форму репозитория.
- [`runtime-integration-model.md`](./runtime-integration-model.md) фиксирует архитектурную границу между Core и внешними runtime, такими как Codex App Server, включая то, что обязан изолировать runtime port, какие App Server-native семейства возможностей относятся к текущему или более позднему этапу, и что запрещено знать слою adapters.
- [`subagent-orchestration-model.md`](./subagent-orchestration-model.md) фиксирует систему субагентов как Core-owned orchestration primitive, включая роли делегирования, границы child-task, правила nested spawning, бюджеты и структуру review-loop.

## Чем владеет этот раздел

- операционным разделением между `Core` и `Interfaces`;
- размещением Core-owned логики по `src/core`, `src/ports` и `src/adapters` без изобретения третьего продуктового уровня;
- направлением зависимостей между доменным кодом, портами, адаптерами, persistence, resources и интерфейсами;
- архитектурной границей вокруг runtime adapters, execution sources и этапно вводимых App Server-native семейств возможностей.

## Чем этот раздел не владеет

- правилами полей `agent JSON` и деталями JSON Schema; это относится к [`03-contracts`](../03-contracts/README.md);
- семантикой исполнения графа, исходами нод и подробным поведением resume; это относится к [`04-execution`](../04-execution/README.md) и [`05-state`](../05-state/README.md);
- lifecycle-процессами вроде registry, `draft`, `live` и `deploy`; это относится к [`07-lifecycle`](../07-lifecycle/README.md);
- спорными решениями или изменениями базового стека; это относится к [`09-adrs`](../09-adrs/README.md).

## Как пользоваться этим разделом при реализации

| Если вопрос такой... | Читать... |
| --- | --- |
| Где в зафиксированной форме репозитория должна жить логика оркестрации графа? | [`core-and-interfaces.md`](./core-and-interfaces.md) |
| Может ли CLI или будущий UI обращаться к SQLite или runtime adapters напрямую? | [`core-and-interfaces.md`](./core-and-interfaces.md) |
| Где разрешено импортировать Codex App Server client/runtime code? | [`runtime-integration-model.md`](./runtime-integration-model.md) и [`../01-foundations/technology-stack.md`](../01-foundations/technology-stack.md) |
| Как интегрировать runtime-возможности вроде native resume и live comments без утечки vendor types? | [`runtime-integration-model.md`](./runtime-integration-model.md) |
| Куда относятся discovery моделей, метаданные reasoning и speed tiers, review threads или более богатые App Server notifications? | [`runtime-integration-model.md`](./runtime-integration-model.md) |
| Что такое субагент в терминах продукта, и как моделировать делегирование и review-loop? | [`subagent-orchestration-model.md`](./subagent-orchestration-model.md) |
| Какие точные поля есть в agent files? | [`../03-contracts/README.md`](../03-contracts/README.md) |
| Что именно сохраняется для chats, resume, registry или drafts? | [`../05-state/README.md`](../05-state/README.md) и [`../07-lifecycle/README.md`](../07-lifecycle/README.md) |

## Карта смежных источников истины

- [`../01-foundations/README.md`](../01-foundations/README.md) и [`../01-foundations/technology-stack.md`](../01-foundations/technology-stack.md) задают идеологию, lock стека и guardrails по vendor isolation, которые этот раздел обязан сохранять.
- [`../03-contracts/README.md`](../03-contracts/README.md) задает машинно-проверяемые и файловые контракты, которые архитектура не должна дублировать.
- [`../04-execution/README.md`](../04-execution/README.md) владеет поведением исполнения во времени после того, как архитектура определила ответственный слой.
- [`../05-state/README.md`](../05-state/README.md) владеет state, resume и persistence-деталями после того, как архитектура определила границу хранения.
- [`../07-lifecycle/README.md`](../07-lifecycle/README.md) владеет правилами registry и `draft/live/deploy` после того, как архитектура относит их к Core.
- [`../09-adrs/ADR-0001-codex-first-not-codex-only.md`](../09-adrs/ADR-0001-codex-first-not-codex-only.md) фиксирует неоспоримое решение `codex-first, not codex-only`, на котором построен этот раздел.

## Стандарт ревью для этого раздела

Архитектурное изменение допустимо только если одновременно сохраняется следующее:

- Core остается единственным домом логики оркестрации.
- Интерфейсы остаются тонкими и заменяемыми.
- Знание о vendor client остается внутри runtime adapters.
- `agent JSON` остается источником истины для определения агентов.
- SQLite остается локальным хранилищем метаданных, а не превращается в каноническую модель агента.
- Документы не изобретают конкурирующее верхнеуровневое дерево `src/`.
