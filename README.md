# dennett-agent-orchestrator

`dennett-agent-orchestrator` is a local-first orchestrator for portable agent runs. Agents are described as JSON files, executed as graphs through runtime adapters, and backed by local operational state for runs, replies, resume, and diagnostics.

## Status

This project is pre-release. Use it from a source checkout.

- The package is not published to the npm registry yet; use the source-checkout workflow below.
- The CLI is the main supported entrypoint.
- JavaScript internals are not a stable public API.
- Some CLI commands are marked experimental in `--help`; treat those surfaces as subject to change.

## Quick Start

Requirements:

- Node.js `>=22.13.0`
- pnpm `10.33.0` or compatible

```powershell
git clone https://github.com/Andrey-Good/dennett-agent-orchestrator.git
cd dennett-agent-orchestrator
corepack enable
pnpm install --frozen-lockfile
pnpm build
pnpm dennett --help
```

The default local state database is `.dennett/local-state.sqlite` inside the checkout. Stateful commands can use `--state-db <path>` when you want an isolated database.

## Main Commands

Run commands from a built source checkout:

```powershell
pnpm dennett <command> ...
```

Core workflow commands:

- `run <agent-file>` runs a portable Agent JSON file directly.
- `register <agent-file>` registers an agent file as a draft revision.
- `deploy <agent-file>` publishes an agent file as the live revision.
- `status <agent-id>` shows registered agent lifecycle state.
- `run-live <agent-id>` runs the current live revision of a registered agent.
- `run-status` inspects durable run and interaction state.
- `reply <agent-file>` records or delivers a reply to a waiting user prompt.
- `resume <agent-file>` resumes a durable local run.
- `support-bundle` emits a local redacted diagnostics bundle.

Examples:

```powershell
pnpm dennett run <agent-file>
pnpm dennett register <agent-file>
pnpm dennett status <agent-id>
pnpm dennett deploy <agent-file>
pnpm dennett run-live <agent-id>
pnpm dennett run-status
pnpm dennett support-bundle
```

CLI help labels commands inline as `[stable]`, `[stable/safety-protocol]`, or `[experimental]`. Experimental surfaces currently include runtime inspection and model listing, memory provider commands, the builder command, triggers and events, and managed subagent commands.

## Smoke Checks

Run the focused public example suite:

```powershell
pnpm test -- tests/unit/public-examples.test.ts
```

Build and inspect the CLI:

```powershell
pnpm build
pnpm dennett --help
pnpm dennett support-bundle
```

Live example agents are listed in [examples/agents](./examples/agents/README.md). Live runs require local Codex/App Server authentication and access to the model named in the selected example agent file. Offline schema and fixture tests can pass even when a local live runtime is not available.

## What Works Now

- Build the TypeScript project from source.
- Run the local CLI through the `pnpm dennett` source-checkout alias.
- Validate and run portable Agent JSON examples.
- Use local SQLite-backed operational state for runs and interaction records.
- Register, inspect, deploy, and run local agent revisions through CLI commands.
- Generate a redacted local support bundle.
- Run contract, unit, and focused public-example tests.
- Experiment with runtime inspection, memory provider bindings, builder output, triggers, and managed subagent commands where the CLI marks them experimental.

## Not Supported Yet

- Registry installation from npm.
- Managed cloud service, uptime guarantees, or managed deployment.
- Production certification claims.
- Stable public JavaScript SDK/API beyond documented contracts and CLI behavior.
- Broad runtime-provider or memory-provider compatibility guarantees.
- Cloud-managed memory, fully managed user interaction, or fully governed multi-agent orchestration as stable product surfaces.

## Documentation

- [Documentation map](./docs/README.md)
- [Agent JSON contract docs](./docs/03-contracts/agent-json/README.md)
- [JSON schemas](./contracts/json-schema/)
- [Example agents](./examples/agents/README.md)
- [Contributing guide](./CONTRIBUTING.md)
- [Security policy](./SECURITY.md)
- [Changelog](./CHANGELOG.md)
- [License](./LICENSE)

## Contributing

Before opening a change, read [CONTRIBUTING.md](./CONTRIBUTING.md). For security reports, use [SECURITY.md](./SECURITY.md) instead of public issues.

The project is licensed under [Apache-2.0](./LICENSE).

---

# dennett-agent-orchestrator на русском

`dennett-agent-orchestrator` - локальный оркестратор переносимых агентных запусков. Агент описывается JSON-файлом, выполняется как граф через runtime-адаптеры, а состояние запусков, ответов, возобновления и диагностики хранится локально.

## Текущий статус

Проект находится в pre-release состоянии. Используйте его из исходного checkout.

- Пакет пока не опубликован в npm registry; используйте source-checkout workflow ниже.
- Основная поддерживаемая точка входа - CLI.
- Внутренние JavaScript-модули не являются стабильным публичным API.
- Команды, помеченные в `--help` как experimental, могут меняться.

## Быстрый старт

Требования:

- Node.js `>=22.13.0`
- pnpm `10.33.0` или совместимая версия

```powershell
git clone https://github.com/Andrey-Good/dennett-agent-orchestrator.git
cd dennett-agent-orchestrator
corepack enable
pnpm install --frozen-lockfile
pnpm build
pnpm dennett --help
```

По умолчанию локальная база состояния создается в `.dennett/local-state.sqlite` внутри checkout. Для изолированной базы используйте `--state-db <path>` в командах, которые работают с состоянием.

## Основные команды

Запускайте команды из собранного исходного checkout:

```powershell
pnpm dennett <command> ...
```

Основной рабочий процесс:

- `run <agent-file>` запускает portable Agent JSON файл напрямую.
- `register <agent-file>` регистрирует файл агента как draft-ревизию.
- `deploy <agent-file>` публикует файл агента как live-ревизию.
- `status <agent-id>` показывает состояние зарегистрированного агента.
- `run-live <agent-id>` запускает текущую live-ревизию зарегистрированного агента.
- `run-status` показывает durable run и interaction state.
- `reply <agent-file>` записывает или доставляет ответ на ожидающий user prompt.
- `resume <agent-file>` возобновляет durable local run.
- `support-bundle` создает локальный redacted diagnostics bundle.

Примеры:

```powershell
pnpm dennett run <agent-file>
pnpm dennett register <agent-file>
pnpm dennett status <agent-id>
pnpm dennett deploy <agent-file>
pnpm dennett run-live <agent-id>
pnpm dennett run-status
pnpm dennett support-bundle
```

CLI help помечает команды встроенными метками `[stable]`, `[stable/safety-protocol]` или `[experimental]`. Сейчас к экспериментальным поверхностям относятся runtime inspection и model list, команды memory provider, builder, triggers и events, а также managed subagent commands.

## Проверка

Запустите тесты публичных примеров:

```powershell
pnpm test -- tests/unit/public-examples.test.ts
```

Проверьте сборку и CLI:

```powershell
pnpm build
pnpm dennett --help
pnpm dennett support-bundle
```

Live-примеры перечислены в [examples/agents](./examples/agents/README.md). Live-запуск требует локальную аутентификацию Codex/App Server и доступ к модели, указанной в выбранном файле примера. Offline-тесты схем и примеров могут проходить даже без доступного live runtime.

## Что уже работает

- Сборка TypeScript-проекта из исходников.
- Запуск локального CLI через source-checkout alias `pnpm dennett`.
- Валидация и запуск примеров Agent JSON.
- Локальное SQLite-состояние для запусков и interaction records.
- Регистрация, просмотр, deploy и запуск локальных ревизий агента через CLI.
- Создание локального redacted support bundle.
- Запуск contract, unit и focused public-example тестов.
- Эксперименты с runtime inspection, memory provider bindings, builder output, triggers и managed subagent commands там, где CLI помечает их как experimental.

## Что пока не поддерживается

- Установка из npm registry.
- Managed cloud service, uptime guarantees или managed deployment.
- Заявления о production certification.
- Стабильный публичный JavaScript SDK/API за пределами документированных контрактов и CLI.
- Гарантии широкой совместимости runtime-provider или memory-provider.
- Стабильные product surfaces для cloud-managed memory, fully managed user interaction или fully governed multi-agent orchestration.

## Документация

- [Карта документации](./docs/README.md)
- [Agent JSON contract docs](./docs/03-contracts/agent-json/README.md)
- [JSON schemas](./contracts/json-schema/)
- [Примеры агентов](./examples/agents/README.md)
- [Contributing guide](./CONTRIBUTING.md)
- [Security policy](./SECURITY.md)
- [Changelog](./CHANGELOG.md)
- [License](./LICENSE)

## Участие

Перед изменениями прочитайте [CONTRIBUTING.md](./CONTRIBUTING.md). Для сообщений об уязвимостях используйте [SECURITY.md](./SECURITY.md), а не публичные issues.

Лицензия проекта: [Apache-2.0](./LICENSE).
