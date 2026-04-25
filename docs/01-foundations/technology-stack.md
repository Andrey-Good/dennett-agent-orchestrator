[English](#english) | [Русский](#russian)

## English

# Technology Stack

Status: approved foundational specification.  
Owns: locked implementation stack for the repository, mandatory dependency guardrails, and the change policy for stack-level decisions.  
Does not own: exact package versions, project scripts, or lower-level module design.  
Primary sources: [canonical specification](../../agent_orchestrator_final_spec_v2.md), [runtime integration model](../02-architecture/runtime-integration-model.md), [ADR-0001](../09-adrs/ADR-0001-codex-first-not-codex-only.md).

## Locked Stack Decisions

The repository is expected to implement the product on the following stack:

| Area | Locked choice | Why it is locked | Guardrail |
| --- | --- | --- | --- |
| Production language | TypeScript | One language across CLI, orchestration logic, adapters, and contracts reduces boundary friction | Do not introduce a second production language for core behavior without an ADR |
| Execution environment | Node.js | The repository is shaped around a TypeScript/Node execution model | Do not design core behavior around a different host runtime |
| Package manager and workspace tool | `pnpm` | Dependency and workspace behavior should stay consistent across contributors and CI | `npm` or `yarn` may exist for compatibility tooling, but not as the canonical workflow |
| External contract validation | `JSON Schema 2020-12` with `Ajv` | Public and file-based contracts need a standard, portable validation model | Do not replace schema validation with ad hoc code for canonical file contracts |
| CLI framework | `commander` | CLI structure should use a stable library instead of a homegrown command parser | Do not build a custom command framework as the default interface layer |
| Local metadata storage | SQLite | The project needs a local transactional metadata store without turning storage into a distributed system requirement | SQLite stays local and derivative, never the canonical agent-definition store |
| Test runner | `vitest` | Fast TypeScript-native unit, contract, and integration testing is part of the expected workflow | Do not fragment basic test execution across multiple primary runners without a decision record |
| Formatting and linting | `Biome` | Formatting and linting should have one standard toolchain in the repo | Do not create competing "official" formatting paths |
| Codex integration | App Server-native Codex adapter inside the Codex adapter only | The project is codex-first, but Codex-specific behavior should follow the official App Server material and the generated protocol surface; vendor client knowledge must remain behind the adapter boundary and must not be replaced with a Codex CLI subprocess path | Do not import vendor client types into core, contracts, storage boundaries, or interfaces, and do not use a product-level CLI as the Codex runtime path |

## What This Stack Decision Means In Practice

- Production implementation belongs in TypeScript running on Node.js.
- The standard repository workflow should assume `pnpm`.
- Contract validation for portable files and other external inputs should be expressed in JSON Schema and executed with Ajv.
- CLI behavior should be built on top of `commander`, not a bespoke parser.
- SQLite is available for local metadata such as registry state, resume data, and other derived operational storage.
- Codex-specific runtime access goes through the adapter layer using App Server-native protocol semantics, not by shelling out to a Codex CLI path from product code.

## What Is Intentionally Not Locked Here

This document does not lock every dependency choice. The following remain implementation decisions until another spec or ADR says otherwise:

- The exact Node.js version range.
- The exact SQLite driver library.
- The logging library, if any.
- Build-output packaging details.
- Internal helper libraries that do not redefine a locked repository standard.

Leaving these choices open does not permit replacing the locked stack categories above. It only means the lower-level implementation may choose the concrete tool inside the allowed boundary.

## When An ADR Is Required

Create or update an ADR before doing any of the following:

- Replacing a locked stack choice with a different standard.
- Moving Codex client/runtime code outside the adapter boundary.
- Replacing the App Server-native Codex adapter path with a CLI- or shell-based runtime path.
- Promoting a new library into a repository-wide standard for an already locked area.
- Turning SQLite into the canonical home of agent definitions rather than local metadata.

## Implementation Consequences

- Dependency review should distinguish between local helper libraries and stack-level standards.
- Tests should be runnable without loading vendor SDK code into the core test path.
- Storage code should be written with the assumption that the file remains canonical and SQLite remains derivative.
- New adapters must conform to the same boundary policy even if they use different vendor libraries internally.

## Russian

# Технологический Стек

Статус: утвержденная foundational-спецификация.  
Владеет: зафиксированным implementation stack для репозитория, обязательными guardrails по зависимостям и политикой изменения stack-level решений.  
Не владеет: точными версиями пакетов, project scripts и нижележащим дизайном модулей.  
Основные источники: [каноническая спецификация](../../agent_orchestrator_final_spec_v2.md), [runtime integration model](../02-architecture/runtime-integration-model.md), [ADR-0001](../09-adrs/ADR-0001-codex-first-not-codex-only.md).

## Зафиксированные Решения По Стеку

Ожидается, что продукт в этом репозитории реализуется на следующем стеке:

| Область | Зафиксированный выбор | Почему он зафиксирован | Guardrail |
| --- | --- | --- | --- |
| Production language | TypeScript | Один язык для CLI, orchestration logic, adapters и contracts уменьшает трение на границах | Не вводите второй production language для core behavior без ADR |
| Execution environment | Node.js | Репозиторий построен вокруг модели исполнения TypeScript/Node | Не проектируйте core behavior под другой host runtime |
| Package manager и workspace tool | `pnpm` | Поведение зависимостей и workspaces должно быть единым у разработчиков и CI | `npm` или `yarn` могут существовать для совместимости, но не как канонический workflow |
| Валидация внешних контрактов | `JSON Schema 2020-12` вместе с `Ajv` | Публичным и файловым контрактам нужна стандартная переносимая модель проверки | Не заменяйте schema validation на ad hoc код для канонических файловых контрактов |
| CLI framework | `commander` | Структура CLI должна строиться на стабильной библиотеке, а не на самодельном parser | Не создавайте custom command framework как базовый interface layer |
| Local metadata storage | SQLite | Проекту нужно локальное транзакционное metadata-хранилище без превращения storage в требование к распределенной инфраструктуре | SQLite остается локальной производной системой, а не каноническим store определения агента |
| Test runner | `vitest` | Быстрые TypeScript-native unit, contract и integration tests входят в ожидаемый workflow | Не дробите базовый запуск тестов между несколькими primary runners без decision record |
| Formatting и linting | `Biome` | Форматирование и linting должны иметь один стандартный toolchain в репозитории | Не создавайте конкурирующие "официальные" пути форматирования |
| Интеграция с Codex | App Server-native Codex adapter только внутри Codex adapter | Проект codex-first, но Codex-specific поведение должно следовать официальным материалам App Server и сгенерированной поверхности протокола; знание о vendor client обязано оставаться за adapter boundary и не должно заменяться Codex CLI subprocess path | Не импортируйте vendor client types в core, contracts, storage boundaries или interfaces и не используйте product-level CLI как путь runtime для Codex |

## Что Это Означает На Практике

- Production-реализация должна быть на TypeScript поверх Node.js.
- Стандартный workflow репозитория должен исходить из использования `pnpm`.
- Валидация переносимых файлов и других внешних входов должна описываться через JSON Schema и исполняться с Ajv.
- CLI-поведение должно строиться на `commander`, а не на собственном parser.
- SQLite допустим для локальной metadata, такой как registry state, resume data и другое производное операционное хранение.
- Codex-specific runtime access должен идти через слой adapter с использованием App Server-native protocol semantics, а не через shell-out в Codex CLI из продуктового кода.

## Что Здесь Намеренно Не Зафиксировано

Этот документ не фиксирует каждый выбор зависимости. Следующие вещи остаются решениями реализации, пока другой spec или ADR не скажет иное:

- Точный диапазон версий Node.js.
- Точная библиотека-драйвер для SQLite.
- Библиотека логирования, если она понадобится.
- Детали packaging build outputs.
- Внутренние helper-библиотеки, которые не переопределяют зафиксированный стандарт репозитория.

Открытость этих решений не дает права заменять зафиксированные категории стека выше. Она лишь означает, что нижележащая реализация может выбрать конкретный инструмент внутри разрешенной границы.

## Когда Нужен ADR

Создавайте или обновляйте ADR перед любым из следующих шагов:

- Замена зафиксированного stack choice на другой стандарт.
- Вынос Codex client/runtime code за пределы adapter boundary.
- Замена App Server-native пути Codex adapter-а на CLI- или shell-based runtime path.
- Повышение новой библиотеки до статуса repository-wide стандарта в уже зафиксированной области.
- Превращение SQLite в канонический дом определения агента вместо локальной metadata.

## Последствия Для Реализации

- Review зависимостей должен различать локальные helper-библиотеки и stack-level стандарты.
- Tests должны запускаться без загрузки vendor SDK в core test path.
- Storage-код должен писаться с предпосылкой, что файл остается каноническим, а SQLite остается производным.
- Новые adapters обязаны соблюдать ту же boundary policy, даже если внутри используют другие vendor libraries.
