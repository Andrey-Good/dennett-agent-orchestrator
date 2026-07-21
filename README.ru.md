# Dennett

Dennett — персональная агентная операционная среда: постоянный оркестратор, прямые проектные агенты в реальных папках и репозиториях, доказательная долговременная память, голосовой и фоновый сенсорный режим, сменные модели и инструменты, несколько устройств и регулируемая автономность.

> **Статус репозитория:** milestones M00 и M01 приняты. Native Windows Project Chat уже проводит локальный разговор через persistent Node, embedded Head и Codex из ChatGPT subscription, сохраняет authoritative session и восстанавливает её после перезапуска UI. Это первый ограниченный vertical slice, а не production release.

## С чего начать

- [Карта документации](docs/README.md)
- [Функциональная концепция](docs/specifications/00_Dennett_Functional_Concept.md)
- [Системная архитектура](docs/architecture/80_Dennett_System_Architecture_and_Runtime_Topology.md)
- [План реализации и тестирования](docs/architecture/83_Dennett_Client_Operations_Testing_and_Implementation_Blueprint.md)
- [Инструкции coding-агентам](AGENTS.md)
- [Инженерная хроника разработки](blog/INDEX.md)

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
- `blog/` — неканоническая evidence-linked хроника решений, ошибок и owner feedback.

## Разработка

После установки [mise](https://mise.jdx.dev/) выполните:

```bash
mise trust
mise install
just bootstrap
just check
just generate
just test-contracts
```

Версии инструментов и все зависимости закреплены lock-файлами. Python и его пакеты
устанавливаются только через `uv`; bootstrap работает без cloud credentials. На Windows для
Rust нужен Visual Studio Build Tools с workload **Desktop development with C++**.

`just generate` воспроизводит зафиксированные Rust- и TypeScript-клиенты из
`protocols/proto`, а `just test-contracts` проверяет их актуальность и совместимость с `main`.

Лицензия пока не выбрана; файл `LICENSE` намеренно отсутствует до отдельного решения владельца.
