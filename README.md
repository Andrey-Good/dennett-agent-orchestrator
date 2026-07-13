# Denet

[Русская версия](README.ru.md)

**Denet — персональная агентная операционная среда:** постоянный главный оркестратор, прямые проектные агенты в реальных папках и репозиториях, доказательная долговременная память, голосовое и фоновое восприятие, заменяемые модели и инструменты, работа с нескольких устройств и регулируемая пользователем автономность.

> **Статус репозитория:** архитектурно завершённая заготовка реализации. Сам продукт ещё не реализован. Репозиторий содержит канонические продуктовые спецификации, четыре архитектурных тома, исполняемые формы ключевых контрактов, границы модулей, тестовые сценарии и намеренно тонкий кодовый каркас для дальнейшей реализации людьми и coding-agent-ами.

## С чего начать

| Цель | Что читать |
|---|---|
| Понять продукт за 15 минут | [`docs/README.md`](docs/README.md) → [`00_Denet_Functional_Concept.md`](docs/specifications/00_Denet_Functional_Concept.md) |
| Понять архитектуру | [`docs/architecture/README.md`](docs/architecture/README.md) → тома 80–83 |
| Реализовать конкретную часть | [`docs/implementation/README.md`](docs/implementation/README.md) → корневой [`AGENTS.md`](AGENTS.md) → ближайший вложенный `AGENTS.md` |
| Понять порядок реализации всего проекта | [`04_MILESTONE_DEPENDENCY_MAP.md`](docs/implementation/04_MILESTONE_DEPENDENCY_MAP.md) → [`ROADMAP.md`](ROADMAP.md) |
| Добавить провайдера, MCP, skill, connector, voice- или computer-use-backend | [Том 82](docs/architecture/82_Denet_Agent_Voice_Capability_and_Integration_Architecture.md) и [`adapters/AGENTS.md`](adapters/AGENTS.md) |
| Работать с памятью или синхронизацией | [Спецификация 10](docs/specifications/10_Denet_Memory_Fabric.md), [том 81](docs/architecture/81_Denet_Data_Memory_Storage_Sync_and_Protocol_Architecture.md), затем `crates/denet-memory-core/AGENTS.md` |
| Посмотреть принятые решения и компромиссы | [`docs/decisions/README.md`](docs/decisions/README.md) |

## Ключевые принципы

- **Прямая проектная работа остаётся простой.** Проект — реальная папка или репозиторий с прямым чатом агента, в основе похожим на Codex App и Claude Code.
- **Один сильный агент — baseline.** Команды, ревьюеры и долговечные workflow появляются только тогда, когда их польза выше расходов на контекст, задержку и координацию.
- **Одна логическая память при разных ролях устройств.** ПК, назначенный Head, использует ту же каноническую Memory Fabric, что и выделенный сервер. SQLite обычного клиента — кэш и offline-журнал, а не вторая конкурирующая память.
- **Переход устройства в Head — только по предварительному разрешению.** Новое устройство всегда получает `head_eligibility = none`; только владелец может выдать `emergency` или `full`.
- **Стабильное ядро, заменяемые края.** Провайдеры, agent runtimes, speech, computer-use, screen capture, MCP и connectors подключаются адаптерами через типизированные порты.
- **Каноническое состояние живёт вне model context.** Задачи, разрешения, эффекты, память, artifacts и sync имеют явных владельцев и пути восстановления.
- **Закрытие окна не останавливает Denet.** Tauri — оболочка desktop; `denet-node` — постоянный локальный демон.
- **Тестируемость является частью архитектуры.** Для границ предусмотрены fake-реализации, conformance suites, детерминированные сценарии отказов и наблюдаемое состояние.

## Карта репозитория

```text
docs/          Спецификации, shared contracts, архитектура, ADR и runbooks
apps/          Desktop- и mobile-клиенты
services/      Head, Node, memory service, adapter hosts и sensor worker
crates/        Стабильные Rust-модули домена и application layer
adapters/      Заменяемые интеграции провайдеров, инструментов, транспорта и storage
protocols/     Protobuf-контракты и описание протоколов
schemas/       JSON Schema переносимых пакетов и component descriptors
tests/         Structured test catalogue, contract, integration, E2E и deterministic scenarios
planning/      Milestones, Work Packages, autonomous batches, decisions and debt
tools/         Проверки репозитория, документации и инструменты разработчика
```

## Команды разработки

Каркас намеренно реализует только тонкий vertical slice и стабильные интерфейсы. В нём нет фиктивной «полной реализации», спрятанной за сотнями `TODO`.

```bash
# Один раз: зависимости инструментов репозитория
python -m pip install -r requirements-dev.txt

# Проверка репозитория и документации
python tools/verify_repo.py
python tools/verify_docs.py
python tools/verify_planning.py

# Тесты Python adapter host
python -m unittest discover -s services/adapter-host-python/tests

# После установки зависимостей TypeScript
pnpm install
pnpm typecheck

# После установки stable Rust toolchain
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Перед изменениями прочитайте [`CONTRIBUTING.md`](CONTRIBUTING.md).

## Безопасность

Не публикуйте уязвимости в открытых issues. Следуйте [`SECURITY.md`](SECURITY.md). Ни prompt модели, ни запись памяти, ни файл проекта, ни plugin не могут самостоятельно выдать себе права; реальные эффекты всегда проходят через Trust и Effect boundaries.

## Лицензия

Лицензия проекта пока не выбрана. До появления `LICENSE` репозиторий следует считать **all rights reserved** в отношении распространения и производных работ.
