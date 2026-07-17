# Dennett

Dennett — персональная агентная операционная среда: постоянный оркестратор, прямые проектные агенты в реальных папках и репозиториях, доказательная долговременная память, голосовой и фоновый сенсорный режим, сменные модели и инструменты, несколько устройств и регулируемая автономность.

> **Статус репозитория:** архитектура и бизнес-логика описаны; создан исполняемый каркас контрактов и модулей. Сам продукт ещё не реализован полностью.

## С чего начать

- [Карта документации](docs/README.md)
- [Функциональная концепция](docs/specifications/00_Dennett_Functional_Concept.md)
- [Системная архитектура](docs/architecture/80_Dennett_System_Architecture_and_Runtime_Topology.md)
- [План реализации и тестирования](docs/architecture/83_Dennett_Client_Operations_Testing_and_Implementation_Blueprint.md)
- [Инструкции coding-агентам](AGENTS.md)

## Ключевые решения

- Обычная работа в проекте остаётся прямым диалогом с одним сильным агентом.
- Память логически едина; SQLite обычного устройства — кэш/offline-журнал, а не вторая конкурирующая память.
- ПК может стать Head/сервером только после явного разрешения пользователя и подготовки полной Authority Replica.
- Tauri — desktop-оболочка, а `dennett-node` — постоянный локальный демон.
- Providers, voice, MCP, computer-use, screen capture и connectors подключаются как заменяемые adapters.
- Внешние действия проходят Trust, Effect Claim, idempotency и reconciliation.
- Тестирование, восстановление и наблюдаемость являются частью архитектуры.

## Репозиторий

- `docs/` — канонические спецификации, архитектура, ADR и runbooks;
- `apps/` — desktop/mobile shells;
- `services/` — Head, Node, memoryd, adapter hosts, sensor worker;
- `crates/` — стабильные Rust-модули;
- `adapters/` — сменные integrations;
- `protocols/` и `schemas/` — wire/package contracts;
- `tests/` — contract, integration, E2E и failure scenarios.

## Разработка

После установки [mise](https://mise.jdx.dev/) выполните:

```bash
mise trust
mise install
just bootstrap
just check
```

Версии инструментов и все зависимости закреплены lock-файлами. Python и его пакеты
устанавливаются только через `uv`; bootstrap работает без cloud credentials. На Windows для
Rust нужен Visual Studio Build Tools с workload **Desktop development with C++**.

Лицензия пока не выбрана; файл `LICENSE` намеренно отсутствует до отдельного решения владельца.
