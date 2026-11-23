# Dennett

**Агентная операционная среда: разговор, самостоятельные агенты, независимая память, длительные поручения и работа между устройствами.**

## Текущая отправная точка

27 сентября 2026 года владелец выбрал **реализацию с нуля** без переноса старого кода. Архитектура и бизнес-логика сохранены. На текущем этапе **D0** созданы минимальная воспроизводимая сборка (Rust workspace `dennett-contracts`, `dennett-testkit`, диагностический каркас `dennett-memoryd`), изолированный тестовый профиль PostgreSQL, единый runner проверок (`dennett-check`) и минимальный CI. Продуктовые модули этапов D1–D9 (Memory Fabric, Head/Node, UI, продуктовый runtime-адаптер) ещё не реализованы; готовый продукт на этом этапе не заявляется.

Старая реализация сохранена в истории и на ветке `archive/before-code-reset-2026-09-27`. Архитектурный исходник этой подготовки — `b1bf960418da588a71f47a981e18b2fa93a0022f`. Ничего не удаляется из Git-истории или пользовательских данных вне репозитория.

## Первый агентный движок

Решением владельца (2026-11-23) сначала реализуется родной движок **Google Antigravity (`agy acp`, Agent Client Protocol v1 over stdio)** с двумя явно разведёнными профилями входа: подписка **Google AI Ultra** (`oauth-personal`) для интерактивной работы и отдельный профиль **Gemini API key** (`gemini-api-key`) для будущих безлюдных фоновых задач. **Claude Code (`R-CLAUDE`)** и **Codex (`R-CODEX`)** сохраняются как отложенные будущие адаптеры, не входят в D0–D9 и не нужны для первого запуска. Модель кодирующего агента и конкретная модель продукта не закрепляются. Подробности — [контракт M (ред. 2.0)](docs/specifications/contracts/M_Runtime_Rollout_Contract.md) и [A01–A12](docs/testing/ANTIGRAVITY_RUNTIME_ACCEPTANCE.md).

## Как начать работу агентом

Открой актуальный checkout и передай агенту:

> Прочитай AGENTS.md и docs/implementation/STATUS.md. Начни новую реализацию Dennett с этапа D0 по утверждённому плану. Пиши тесты вместе с кодом, проверяй крайние случаи, намеренные отказы и регрессии. Не восстанавливай старую реализацию из истории. Работай самостоятельно внутри этапа; остановись на H0, покажи реальные результаты и дождись моей приёмки. На дальнейшем движении соблюдай все остановки и дизайн-согласования.

После принятия Hn агент продолжает следующий этап до его остановки. Владелец проверяет понятный результат и дизайн, а не управляет каждой функцией. Модель разработки не закреплена. Для Claude Code файл CLAUDE.md импортирует общие правила, не создаёт второй регламент.

## Документы

- [План D0–D9](docs/implementation/04_MILESTONE_DEPENDENCY_MAP.md) — порядок, результат этапа, приёмка и остановки.
- [Процесс агента](docs/implementation/01_AGENT_EXECUTION_PROTOCOL.md) — написание, проверка, интеграция и диагностика.
- [Стратегия тестов](docs/testing/TEST_CATALOGUE_AND_QUALITY_GATES.md) — unit, contract, integration, end-to-end, генеративные и отказные проверки, регрессии.
- [Личная приёмка](docs/implementation/02_OWNER_PLAYBOOK.md) — что смотреть самому и когда согласовывать макет.
- [Текущий статус](docs/implementation/STATUS.md) и [покрытие требований](docs/testing/REQUIREMENTS_COVERAGE.md) — без наследования старых отметок готовности.
- [Архитектура](docs/architecture/README.md) и [карта всей документации](docs/README.md).

Целевой результат — весь обязательный согласованный продукт, не только демонстрационный чат. Завершение требует работающей устанавливаемой сборки, выполненных проверок, сохранения принятых возможностей и владельческой приёмки. Необязательный backend можно отложить; обязательную функцию нельзя молча вычеркнуть.

## Сохранённые границы

Память работает отдельным memoryd и доступна без Head. Каноническое серверное хранение — PostgreSQL, полный резервный ПК имеет серверный комплект. Готовые агентные движки используются без дублирования их исполнения. Смысловые связи агентов описываются текстом. Python-графики, файлы и произвольная веб-графика не ограничены MCP Apps/A2UI.

## Среда разработки и команды проверки (D0)

Минимальная воспроизводимая основа D0 включает Rust workspace (`crates/dennett-contracts`, `crates/dennett-testkit`, `services/memoryd`), изолированный тестовый профиль PostgreSQL ([`ops/test-env/README.md`](ops/test-env/README.md)) и единый runner проверок [`ops/runner`](ops/runner/pyproject.toml). На этапе D0 `dennett-memoryd` является диагностическим каркасом подключения к PostgreSQL (`memory_fabric_implemented: false`), а не готовой Memory Fabric.

### Требования к локальной среде

- Rust toolchain по [`rust-toolchain.toml`](rust-toolchain.toml) (`1.100.0`, компоненты `rustfmt`, `clippy`) и `cargo-nextest`.
- Python `>= 3.12` и `uv` для запуска `dennett-check`.
- Docker (образ `postgres:16-alpine`) — для локальных интеграционных проверок PostgreSQL (`--manage-pg`).
- Google Antigravity CLI `agy >= 1.5.0` (с подкомандой `agy acp`) — только для живой пробы `runtime-probe`.

### Подготовка runner

```bash
uv sync --project ops/runner
```

### Команды проверки (`dennett-check`)

- `uv run --project ops/runner dennett-check docs` — проверяет обязательные документы, валидирует все локальные Markdown-ссылки в корне и `docs/` (кроме `docs/archive/`) и сверяет ссылки на `agy acp` / `ANTIGRAVITY_RUNTIME_ACCEPTANCE.md`.
- `uv run --project ops/runner dennett-check deps` — проверяет `rust-toolchain.toml`, синхронизацию `Cargo.lock` (`cargo metadata --locked`) и архитектурные границы (`dennett-contracts` без SQL/HTTP/UI/SDK; `dennett-memoryd` без Head/UI; отсутствие отложенных адаптеров `claude`/`codex`/`openai`/`anthropic`).
- `uv run --project ops/runner dennett-check fast` — прогоняет `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` и быстрые unit/safety тесты (`unit_safety` + lib/bin tests) с запретом 0-test и skipped прогонов.
- `uv run --project ops/runner dennett-check self-check` — доказывает работоспособность самого тестового контура: (1) отклоняет пустой 0-test прогон, (2) ловит намеренно падающий тест (`DENNETT_FORCE_INTENTIONAL_FAILURE=1`), (3) подтверждает отказ `FaultInjectionGuard` без `DENNETT_TEST_PROFILE=1`.
- `uv run --project ops/runner dennett-check integration --manage-pg` — поднимает одноразовый `postgres:16-alpine` на `127.0.0.1:55432` (`tmpfs`), прогоняет реальные тесты изоляции ролей/БД (`pg_isolation_integration`) и отдельного процесса `dennett-memoryd` без Head (`memoryd_process_integration`), затем удаляет контейнер (`down -v`). Без `--manage-pg` требует заранее заданные `DENNETT_TEST_MEMORY_PG_URL` и `DENNETT_TEST_CONTROL_PG_URL` (иначе явно падает, без тихого пропуска).
- `uv run --project ops/runner dennett-check runtime-probe` — живая D0-проба совместимости `agy acp` (A01 handshake/версия/изоляция отсутствующего `GEMINI_API_KEY` без fallback на OAuth; A03/A05/A06 вызов тестового MCP-инструмента памяти через `AllowOnce`, отказ `run_command` через `RejectOnce` и отмена `session/cancel`).
- `uv run --project ops/runner dennett-check all --manage-pg` — последовательно выполняет `docs`, `deps`, `fast`, `self-check` и `integration --manage-pg`.

Источники, сила свидетельств и реальные проверки различаются. Наличие плана и зелёного CI не доказывает отсутствия всех ошибок. [Статус документов](docs/restart/README.md) определяет приоритет; [SECURITY.md](SECURITY.md) — границы безопасной разработки. Лицензия публичного продукта ещё требует явного решения владельца.
