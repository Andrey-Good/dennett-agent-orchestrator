# Текущее состояние новой реализации

Обновлено 2026-11-23. Текущая ветка реализации D1: `d1/first-vertical-slice` (от принятой базы D0 `d0/foundation` `1a40e46`). Новый baseline не наследует завершённые milestones, тесты или approvals старой реализации.

## Текущая работа

Текущий этап: **D1** (первый настоящий путь: агент на `agy acp`, самостоятельный `memoryd` на PostgreSQL, сохраняемый разговор и новый desktop-чат). Состояние: **в работе** (остановка этапа — **H1**).

Запись приёмки **H0** (2026-11-23): владелец явно принял этап D0 (`d0/foundation` @ `1a40e46`, CI [#45179510052](https://github.com/Andrey-Good/dennett-agent-orchestrator/actions/runs/45179510052)) и разрешил запуск D1 («Меня в принципе всё устраивает. Итак, официально разрешаю выполнение D1.»).

Порядок срез-за-срезом внутри D1 (с независимой проверкой Carbon-верификатором после каждого куска):
1. **Slice 1 (готово, проверено на Carbon, коммит `dc5e184`):** самостоятельный `dennett-memoryd` на PostgreSQL (`dennett-memory-core`, `dennett-memory-client`, миграции, источники/заметки/текущее состояние/история/исправления, scope-авторизация, API + CLI + MCP на одном доменном обработчике; `V01`, `V19`, `V06`; 3 блокерных дефекта найдены Carbon-верификатором и закрыты, 3 мутации пойманы).
2. **Slice 2 (готово, проверено на Carbon, коммит `2c3d1b4`):** `dennett-head` (`dennett_control` БД, Agent/Session/ConfigVersion/Policy/Artifacts/outbox при падении памяти, `HeadClient`/`dennettctl`) + продуктовый адаптер `agy acp` с MCP-инструментами `memoryd`, явным `session_id`/rehydrate, внешним `PolicyEngine` + `PreToolUse` защитой записи, `Allow`/`Deny` и `Stop` (`V02`, `V04`, `V05`, `V06`, `V29`, `A01–A06`, `A09`; 5 блокерных дефектов найдены Carbon-верификатором и закрыты, 3 мутации пойманы; 101/101 fast, 25/25 real PG integration, 4/4 live `agy 1.5.15` E2E без следов в `~/.gemini`).
3. **Slice 3 (ожидает дизайн-согласования владельца):** согласование экрана desktop-чата (на основе принятого направления `WP-M01-003` v5) → реализация desktop-клиента на Tauri 2 + React/TS (`V03`) и финальная сквозная проверка D1 до остановки H1.

Основные коммиты D0 в ветке `d0/foundation`:
- `fc68cec` — закрепление решения владельца о первом runtime **Google Antigravity (`agy acp`)** в `AGENTS.md`, контракте M (ред. 2.0), `ANTIGRAVITY_RUNTIME_ACCEPTANCE.md` (`A01–A12`), переводе `CLAUDE_RUNTIME_ACCEPTANCE.md` в отложенный `R-CLAUDE`, и привязка разделов бизнес-спецификаций в `docs/testing/REQUIREMENTS_COVERAGE.md`.
- `fbbd61b` — воспроизводимый Rust workspace (`dennett-contracts`, `dennett-testkit`, `services/memoryd` D0-bootstrap), изолированный Docker PostgreSQL 16 профиль (`ops/test-env/`), test runner на `uv` (`ops/runner/`), минимальный CI (`.github/workflows/ci.yml`), живые D0-пробы `agy acp 1.5.15`.
- `012b0a0` — закрытие всех 11 найденных дефектов ревью H0 (`H0-01`..`H0-11`): единый UTF-8-безопасный и строго идемпотентный модуль редакции секретов в `dennett-contracts`, парсинг PG DSN через `tokio_postgres::Config` без fallback на боевой env, полная изоляция живой `agy acp`-пробы от `~/.gemini`, проверка NTFS junctions/symlinks и `TEMP` через junction, эфемерные порты и уникальные имена Compose-проектов для параллельных worktrees, `rustdoc` + doctests + nextest filtersets в `dennett-check`, запуск CI на пуше с `--manage-pg`, фиксы JSON-RPC `id: null` и Winsock `TCP RST`.

Решение владельца (2026-11-23):
- Первый продуктовый runtime: родной движок **Google Antigravity (`agy acp`, ACP v1 over stdio)** с двумя явно разведёнными профилями входа:
  - интерактивный профиль через подписку **Google AI Ultra** (`oauth-personal`);
  - отдельный профиль **Gemini API key** (`gemini-api-key`) для будущих безлюдных фоновых задач.
- **Claude Code (`R-CLAUDE`)** и **Codex (`R-CODEX`)** переведены в отложенные будущие адаптеры вне обязательной приёмки D0–D9.

## Подтверждённые проверки D0 (на `012b0a0`)

- **GitHub Actions CI (`Dennett D0 CI`):** реальный зелёный прогон [#45179510052](https://github.com/Andrey-Good/dennett-agent-orchestrator/actions/runs/45179510052) на `012b0a0` — все 3 jobs `success`:
  - `Docs, deps, fast & self-check (ubuntu-latest)` — PASS
  - `Docs, deps, fast & self-check (windows-latest)` — PASS
  - `PostgreSQL isolation & standalone memoryd (--manage-pg)` — PASS (через единый `docker-compose.test.yml`)
- **Локально на Windows 11 (`012b0a0`):**
  - `uv run --project ops/runner dennett-check all --manage-pg` — PASS: `docs` (108 файлов, 4125 ссылок), `deps` (123 locked crates, без `claude`/`codex`/`openai`/`anthropic`), `fast` (`fmt`, `clippy -D warnings`, `rustdoc -D warnings`, `cargo test --doc`, 47/47 unit+proptest, 0 skipped/leaky), `self-check` (4/4), `integration --manage-pg` (11/11 на Docker `postgres:16-alpine` + отдельный `dennett-memoryd` с уникальным Compose-проектом и эфемерным портом).
  - Негативный путь `uv run --project ops/runner dennett-check integration` (без `--manage-pg` и без env-переменных БД) — PASS (exit 1, без тихого skip).
  - `uv run --project ops/runner dennett-check runtime-probe` (`agy 1.5.15`) — PASS (2/2): изолированный `gemini-api-key` без ключа → `-32602`/`-32000`; живой интерактивный прогон с `Google AI Ultra`, Rust MCP-инструментом памяти (`AllowOnce`), отказом на `run_command` (`RejectOnce`) и `session/cancel` (`cancelled`); реальный `~/.gemini/antigravity-cli/conversations` остался нетронутым (`before=33 after=33`).
- **Мутационные проверки (3/3 пойманы и откачены)** и **два независимых аудита + полный разбор H0-01..H0-11** закрыты до текущего коммита.

## Ограничения и находки для D1

- **Граница D0:** продуктовые домены (Memory Fabric, Head, Node, Desktop UI) ещё не реализованы; `dennett-memoryd` на D0 является bootstrap-сервисом проверки здоровья/PG (`memory_fabric_implemented: false`).
- **Находка безопасности `agy acp 1.5.15` для D1:** в режиме `default` нативный `write_file` внутри рабочей папки `cwd` проходит без `session/request_permission` (в отличие от `run_command` и MCP-инструментов). На D1/D2 защиту записи в workspace нужно обеспечивать явными правилами `permissions.deny`/`ask` и изоляцией worktree/песочницы.
- **Фоновый профиль `gemini-api-key`:** изоляция и отказ без ключа подтверждены; живой прогон с реальным `GEMINI_API_KEY` оставлен на этап появления фоновых задач (D2/D3).
- **Защита `main` на GitHub:** на прежнем `main` (`facebf8`) использовались два старых workflow — `Fast Gate` и `Protocol Compatibility`. Если правила branch protection на GitHub всё ещё требуют эти удалённые проверки, при открытии PR из `d0/foundation` владельцу нужно переключить их на новые jobs из `.github/workflows/ci.yml`: `Docs, deps, fast & self-check (ubuntu-latest)`, `Docs, deps, fast & self-check (windows-latest)` и `PostgreSQL isolation & standalone memoryd (--manage-pg)`.

## Этапы и приёмка

- D0 — основа/проверки: **принят (H0 принят владельцем 2026-11-23 на `d0/foundation` `1a40e46`)**.
- D1 — агент (`agy acp`) + `memoryd` + реальный сохраняемый чат: **в работе** (`d1/first-vertical-slice`); H1 не принят.
- D2 — длительные поручения/ожидание/отмена: не начат; H2 не принят.
- D3 — полная память и подключения: не начат; H3 не принят.
- D4 — графика и рабочие экраны: не начат; H4 не принят.
- D5 — устройства и полный резерв: не начат; H5 не принят.
- D6 — голос и сенсорика: не начат; H6 не принят.
- D7 — внешние действия и computer-use: не начат; H7 не принят.
- D8 — проверяемое самоулучшение: не начат; H8 не принят.
- D9 — весь продукт и выпуск: не начат; H9 не принят.

## Среда и внешние условия

Локальная Windows 11 среда (2026-11-23):
- Rust stable `1.100.0` (`rustc`, `cargo`, `rustfmt`, `clippy`, `cargo-nextest 0.9.152`), MSVC Build Tools / VS 2022.
- Python `3.13.5` + `uv 0.11.23`, Node `v22.17.1` / `pnpm 10.33.0`.
- Docker `29.4.3` с образом `postgres:16-alpine` (при Resource Saver может засыпать в простое — перед `--manage-pg` нужен активный Docker Desktop).
- Antigravity CLI `agy 1.5.15` (подписка `Google AI Ultra`, `g1-ultra-tier`).
- Архивная ветка старого кода: `archive/before-code-reset-2026-09-27`.

## Как обновлять эту запись

После связного шага кратко указать commit, результат, выполненные и оставшиеся проверки, препятствие, следующую задачу. Перед Hn дать ссылку на build, test evidence, ошибки и личную демонстрацию. Принятие фиксируется только по явному ответу владельца с областью и версией. Не публиковать приватные логи и токены.

## Открытые дефекты новой программы

Открытых дефектов D0 нет (`H0-01`..`H0-11` закрыты в `012b0a0`). Продуктовые модули D1–D9 ещё не реализованы.
