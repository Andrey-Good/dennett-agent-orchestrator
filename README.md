[English](#english) | [Русский](#russian)

## English

# dennett-agent-orchestrator

Status: Repository entrypoint and implementation map.  
Owns: repository purpose, reading order, directory intent, and top-level contributor guardrails.  
Does not own: detailed architecture, formal contracts, runtime adapter protocol, or lifecycle rules.  
Primary sources: [canonical specification](./agent_orchestrator_final_spec_v2.md), [documentation root](./docs/README.md), [foundations](./docs/01-foundations/README.md), and the official [OpenAI App Server material](https://developers.openai.com/codex/app-server) for Codex-specific behavior.

`dennett-agent-orchestrator` is a codex-first orchestrator for agent runs. It stores agents as portable JSON files, executes their graphs through runtime adapters, preserves only the state needed for chats and resume, and keeps the agent definition separate from chats, events, and interfaces. In this repository the Codex runtime path is App Server-native inside the Codex adapter; the adapter should prefer native App Server primitives over custom reimplementation whenever the official protocol already exposes them. The product CLI is only a user-facing interface over Core, not the mechanism that executes Codex.

The current executable slice now also includes a real local memory-provider subsystem: user-owned provider registration, capability negotiation, a Mem0-first adapter path, and direct provider-backed memory CRUD, search, and bounded namespace-scoped cleanup through Core and CLI. It also includes the narrow Stage 2 runtime-memory path for Codex execution: Core resolves registered memory providers, passes provider-neutral `memory_context` to the runtime adapter, renders retrieved records into Codex developer instructions, and writes successful node output back through the provider. This is prompt-rendered memory context, not native App Server memory; broader runtime-memory behavior, additional providers, true restore, graph-store cleanup, provider-wide cleanup, and provider reliability remain deferred.

The repository is not a new general-purpose agent runtime. It is an orchestration system around agent execution boundaries. Core code decides what to run, with which inputs, permissions, skills, MCPs, plugins, and runtime configuration. The runtime remains responsible for the agent's internal reasoning and tool behavior.

## Local Distribution Artifact

Generated `dist` output is build-local, not tracked source. From a clean checkout, local users should run `pnpm build` before any CLI artifact smoke such as `node dist/src/interfaces/cli.js --help`.

The bounded stable CLI/API contract is documented in [`docs/21-public-launch-readiness/stable-cli-api-contract-freeze.md`](./docs/21-public-launch-readiness/stable-cli-api-contract-freeze.md). Only CLI commands labeled `[stable]`, the `[stable/safety-protocol]` memory cleanup flow, exported JSON schema artifacts, and the no-stable-JS-API package boundary are frozen. Commands labeled `[experimental]` and deep imports from `dist` or `src` are not stable public API.

The bounded release decision is `release` only for `local-cli-repository-readiness` on candidate commit `c3ad3eafca28f4a602a6e44d1861054aabc96a03`. The authoritative final gates passed: `pnpm install --frozen-lockfile`, `pnpm typecheck`, `pnpm lint`, `pnpm test` under an explicit `1200000ms` wrapper in `271880ms`, `pnpm build`, `node --no-warnings -e "await import('node:sqlite')"`, `node dist/src/interfaces/cli.js --help`, `pnpm dist:check`, `pnpm packlist:check`, `pnpm package:check`, and `pnpm release-candidate:check`.

The current Stage 11 package proof is local and controlled: `pnpm package:local-install:proof` builds and packs a local `.tgz`, installs it into a temporary npm consumer project, smokes the installed bin, uninstalls it, and verifies package/bin removal from that consumer project. `pnpm package:upgrade-rollback:proof -- --from-tgz <old.tgz> --to-tgz <new.tgz>` proves only explicit two-tarball upgrade/rollback smoke, and `pnpm supply-chain:local:proof` validates local SPDX SBOM generation without retaining a canonical SBOM file. The package remains `private: true`; this does not claim npm/public package publication, public registry install, namespace ownership, signing, provenance, retained SBOMs, an installer, Docker image, hosted or managed deployment, production SaaS/readiness/load, hosted telemetry/audit readiness, cloud deployment, live provider stress/reliability, broad runtime-memory/provider support, native App Server memory, full App Server certification, full user interaction readiness, or operator-facing managed subagent readiness. Details live in [`docs/21-public-launch-readiness/distribution-proof.md`](./docs/21-public-launch-readiness/distribution-proof.md), with hosted/managed deferral owned by [`docs/21-public-launch-readiness/hosted-managed-deployment-scope.md`](./docs/21-public-launch-readiness/hosted-managed-deployment-scope.md).

## What Exists In The Repository

At the current stage the repository is mainly a specification and implementation skeleton. The directory structure is already locked so future code lands in consistent places instead of inventing new top-level layouts.

| Path | Purpose | Implementation note |
| --- | --- | --- |
| [`contracts/`](./contracts/) | Formal contracts and schemas | Machine-checkable definitions belong here, not in prose docs. |
| [`docs/`](./docs/README.md) | Human-readable specification tree | Read this before implementing behavior that is not obvious. |
| [`examples/`](./examples/) | Human-oriented sample agents and scenarios | Examples illustrate contracts; they do not define them. |
| [`src/core/`](./src/core/) | Orchestration domain logic | Must stay free of vendor SDK imports. |
| [`src/ports/`](./src/ports/) | Internal boundaries used by core | Ports express orchestrator semantics, not vendor naming. |
| [`src/adapters/`](./src/adapters/) | Integrations with runtimes and storage technologies | Codex-specific code belongs here, behind ports. |
| [`src/interfaces/`](./src/interfaces/) | CLI and future UI-facing entrypoints | Interfaces talk to core; they do not host domain rules. |
| [`src/resources/`](./src/resources/) | Non-code resources reserved for runtime support | Keep generated or bundled resources separate from logic. |
| [`tests/`](./tests/) | Contract, unit, integration, fixture, and golden tests | Test categories are intentionally separated to reduce drift. |
| [`subagent_tasks/`](./subagent_tasks/) | Task ownership documents for delegated work | Large changes should be decomposed here before implementation. |

## Read This Before Coding

Implementation work should follow this order:

1. Start with the [canonical specification](./agent_orchestrator_final_spec_v2.md) for the product identity and project laws.
2. Read the [foundations section](./docs/01-foundations/README.md) to lock scope, terminology, system boundaries, truth sources, defaults, and stack decisions.
3. Move to the relevant detailed section under [`docs/`](./docs/README.md) before changing architecture, contracts, execution logic, state, interaction, lifecycle, or extensions.
4. If the needed rule does not exist and the change is significant or contested, record it through an ADR in [`docs/09-adrs`](./docs/09-adrs/README.md) instead of inventing behavior silently in code.
5. For work beyond the completed 1-11 foundation, check [`docs/13-capability-gap-lock`](./docs/13-capability-gap-lock/README.md) before claiming a capability is already implemented. That section is the canonical freeze for `implemented`, `partial`, `documented_only`, `runtime_blocked`, and `not_started`.
6. For any real-world proof or release-readiness claim, check [`docs/20-real-world-proof-and-release`](./docs/20-real-world-proof-and-release/README.md), the canonical [`release scope lock`](./docs/20-real-world-proof-and-release/release-scope-lock.md), and the completed [`release decision record`](./docs/20-real-world-proof-and-release/release-decision-record.md). The locked target `local-cli-repository-readiness` is released only in that bounded sense on candidate commit `c3ad3eafca28f4a602a6e44d1861054aabc96a03`; local/offline Phase 18 evidence is not enough for any broader Phase 19 release claim.

## Repository-Level Guardrails

- The product is an orchestrator of agent runs, not a replacement agent platform.
- The portable `agent JSON` file is the source of truth for the agent definition.
- Local metadata storage may index or support execution, but it must not redefine the agent.
- `skills`, `MCPs`, and `plugins` follow the compatible runtime ecosystem; this repository only references and routes them.
- The runtime adapter boundary is mandatory. Vendor client imports do not belong in core, contracts, or interfaces.
- The repository CLI is a product interface layer, not a Codex execution path. Codex integration belongs behind the App Server-native runtime adapter boundary.
- Chats and resume state are operational state, not the agent definition and not long-term memory by default.

## How To Extend The Repository Safely

- Add new product rules in the narrowest document that should own them.
- Add new directories only when the existing layout cannot express the responsibility clearly.
- Prefer changing one owner document over duplicating the same rule in several places.
- Treat README files as entrypoints and maps; detailed normative behavior should live in focused documents linked from them.

## Russian

Phase 19 routing note: release-readiness claims require [`docs/20-real-world-proof-and-release`](./docs/20-real-world-proof-and-release/README.md) evidence and a completed [`release decision record`](./docs/20-real-world-proof-and-release/release-decision-record.md). `local-cli-repository-readiness` is released only as a bounded local CLI/repository target on candidate commit `c3ad3eafca28f4a602a6e44d1861054aabc96a03`; local/offline Phase 18 evidence is not enough for broader Phase 19 release claims.

Stage 11 distribution note: current package proof is local `.tgz` proof only. It covers controlled install/uninstall in a temporary npm consumer project, explicit two-tarball upgrade/rollback smoke when both tarballs are supplied, and local SPDX SBOM validation. It does not prove public npm publication, public registry install, signing, provenance, retained SBOMs, or rollback without a prior artifact.

Авторитетное финальное gate evidence для bounded local CLI/repository scope: commit `c3ad3eafca28f4a602a6e44d1861054aabc96a03`; `pnpm test` прошел под explicit `1200000ms` wrapper за `271880ms`; полный gate set также прошел `pnpm install --frozen-lockfile`, `pnpm typecheck`, `pnpm lint`, `pnpm build`, `node --no-warnings -e "await import('node:sqlite')"`, `node dist/src/interfaces/cli.js --help`, `pnpm dist:check`, `pnpm packlist:check`, `pnpm package:check` и `pnpm release-candidate:check`. Это доказывает только bounded local CLI/repository readiness и не расширяет release claim на hosted/managed deployment, npm/public package publication, installers/containers, production SaaS/readiness/load, live provider stress/reliability, broad runtime-memory/provider support, native App Server memory, full App Server certification, full user interaction readiness, operator-facing managed subagent readiness, public package install, signing/provenance, retained SBOMs или rollback beyond explicit local two-tarball smoke.

# dennett-agent-orchestrator

Статус: корневая точка входа в репозиторий и карта реализации.  
Владеет: назначением репозитория, порядком чтения, смыслом каталогов и верхнеуровневыми guardrails для участников.  
Не владеет: детальной архитектурой, формальными контрактами, протоколом runtime adapter и правилами жизненного цикла.  
Основные источники: [каноническая спецификация](./agent_orchestrator_final_spec_v2.md), [корень документации](./docs/README.md), [раздел foundations](./docs/01-foundations/README.md).

`dennett-agent-orchestrator` — это codex-first оркестратор запусков агентов. Он хранит агентов как переносимые JSON-файлы, исполняет их графы через runtime adapters, сохраняет только то состояние, которое нужно для чатов и resume, и держит определение агента отдельно от чатов, событий и интерфейсов. В этом репозитории путь выполнения через Codex реализуется как App Server-native путь внутри Codex adapter; adapter должен по возможности использовать нативные примитивы App Server вместо собственной реализации, когда официальный протокол уже их предоставляет. Продуктовый CLI является только пользовательским интерфейсом над Core, а не механизмом исполнения Codex.

Текущий исполняемый срез также включает реальную локальную подсистему memory providers: пользовательскую регистрацию providers, согласование capabilities, Mem0-first adapter path и прямые provider-backed memory CRUD, search и bounded namespace-scoped cleanup через Core и CLI. Он также включает узкий Stage 2 runtime-memory path для Codex execution: Core разрешает registered memory providers, передает provider-neutral `memory_context` в runtime adapter, встраивает retrieved records в Codex developer instructions и пишет successful node output обратно через provider. Это prompt-rendered memory context, а не native App Server memory; broader runtime-memory behavior, additional providers, true restore, graph-store cleanup, provider-wide cleanup и provider reliability остаются deferred.

Репозиторий не является новой универсальной agent runtime-системой. Это система оркестрации вокруг границ исполнения агента. Core-код решает, что запускать, с какими входами, правами, skills, MCPs, plugins и runtime-конфигурацией. Сам runtime по-прежнему отвечает за внутренний reasoning агента и поведение его инструментов.

## Что Уже Есть В Репозитории

На текущем этапе репозиторий в основном состоит из спецификации и каркаса реализации. Структура каталогов уже зафиксирована, чтобы будущий код ложился в понятные места, а не изобретал новые верхнеуровневые раскладки.

| Путь | Назначение | Импликация для реализации |
| --- | --- | --- |
| [`contracts/`](./contracts/) | Формальные контракты и схемы | Машинно-проверяемые определения живут здесь, а не в prose-доках. |
| [`docs/`](./docs/README.md) | Дерево человекочитаемых спецификаций | Читайте его до реализации неочевидного поведения. |
| [`examples/`](./examples/) | Человекочитаемые примеры агентов и сценариев | Примеры иллюстрируют контракты, но не определяют их. |
| [`src/core/`](./src/core/) | Доменная логика оркестрации | Здесь не должно быть импортов vendor SDK. |
| [`src/ports/`](./src/ports/) | Внутренние границы, которыми пользуется core | Ports выражают семантику оркестратора, а не имена vendor API. |
| [`src/adapters/`](./src/adapters/) | Интеграции с runtime и storage-технологиями | Codex-специфичный код должен жить здесь, за портами. |
| [`src/interfaces/`](./src/interfaces/) | CLI и будущие пользовательские точки входа | Интерфейсы работают с core и не хранят доменные правила. |
| [`src/resources/`](./src/resources/) | Резерв для некодовых ресурсов runtime-поддержки | Держите bundled или generated ресурсы отдельно от логики. |
| [`tests/`](./tests/) | Contract, unit, integration, fixture и golden тесты | Категории тестов разделены намеренно, чтобы не размывать ответственность. |
| [`subagent_tasks/`](./subagent_tasks/) | Документы владения задачами для делегированной работы | Крупные изменения нужно декомпозировать здесь до реализации. |

## Что Читать Перед Кодом

Работа по реализации должна идти в таком порядке:

1. Начать с [канонической спецификации](./agent_orchestrator_final_spec_v2.md), чтобы зафиксировать идентичность продукта и законы проекта.
2. Прочитать [раздел foundations](./docs/01-foundations/README.md), чтобы закрепить scope, терминологию, системные границы, источники истины, дефолты и стек.
3. Затем перейти в нужный подробный раздел внутри [`docs/`](./docs/README.md) до изменения архитектуры, контрактов, логики исполнения, состояния, interaction, lifecycle или extensions.
4. Если нужного правила нет, а изменение значимое или спорное, фиксировать его через ADR в [`docs/09-adrs`](./docs/09-adrs/README.md), а не изобретать поведение молча в коде.
5. Для работы поверх завершенного фундамента 1-11 сначала смотрите [`docs/13-capability-gap-lock`](./docs/13-capability-gap-lock/README.md) и только потом заявляйте, что какая-либо возможность уже реализована. Этот раздел канонически фиксирует состояния `implemented`, `partial`, `documented_only`, `runtime_blocked` и `not_started`.
6. Для любого real-world proof или release-readiness claim проверяйте [`docs/20-real-world-proof-and-release`](./docs/20-real-world-proof-and-release/README.md), каноническую [`release scope lock`](./docs/20-real-world-proof-and-release/release-scope-lock.md) и завершенную [`release decision record`](./docs/20-real-world-proof-and-release/release-decision-record.md). Зафиксированная цель `local-cli-repository-readiness` выпущена только в bounded local CLI/repository смысле на candidate commit `c3ad3eafca28f4a602a6e44d1861054aabc96a03`; local/offline Phase 18 evidence недостаточно для broader Phase 19 release claims.

## Guardrails На Уровне Репозитория

- Продукт — это оркестратор запусков агентов, а не замена agent platform.
- Переносимый `agent JSON`-файл является источником истины для определения агента.
- Локальное metadata-хранилище может индексировать или поддерживать исполнение, но не должно переопределять агента.
- `skills`, `MCPs` и `plugins` следуют экосистеме совместимого runtime; этот репозиторий только ссылается на них и маршрутизирует их.
- Граница runtime adapter обязательна. Импортов vendor client не должно быть в core, контрактах и интерфейсах.
- CLI репозитория — это слой пользовательского интерфейса, а не путь исполнения Codex. Интеграция с Codex должна оставаться за границей App Server-native runtime adapter.
- Чаты и resume state — это операционное состояние, а не определение агента и не долговременная память по умолчанию.

## Как Безопасно Расширять Репозиторий

- Добавляйте новые правила продукта в самый узкий документ, который должен ими владеть.
- Создавайте новые каталоги только тогда, когда существующая раскладка не может ясно выразить ответственность.
- Предпочитайте изменение одного документа-владельца дублированию одного и того же правила в нескольких местах.
- Рассматривайте README-файлы как точки входа и карты; детальное нормативное поведение должно жить в профильных документах, на которые README ссылаются.
