# Текущее состояние новой реализации

Обновлено 2026-11-23. Ветка реализации D0: `d0/foundation` (от актуального `main` `2f4c2fb`). Новый baseline не наследует завершённые milestones, тесты или approvals старой реализации.

## Текущая работа

Текущий этап: **D0** (чистая воспроизводимая основа и исполнимые проверки). Состояние: **ожидает владельца (точка приёмки H0)**. До явного принятия H0 переход к D1 не начинается.

Основные коммиты D0 в ветке `d0/foundation`:
- `fc68cec` — закрепление решения владельца о первом runtime **Google Antigravity (`agy acp`)** в `AGENTS.md`, контракте M (ред. 2.0), `ANTIGRAVITY_RUNTIME_ACCEPTANCE.md` (`A01–A12`), переводе `CLAUDE_RUNTIME_ACCEPTANCE.md` в отложенный `R-CLAUDE`, и полная привязка разделов бизнес-спецификаций в `docs/testing/REQUIREMENTS_COVERAGE.md`.
- `fbbd61b` — воспроизводимый Rust workspace (`dennett-contracts`, `dennett-testkit`, `services/memoryd` D0-bootstrap), изолированный Docker PostgreSQL 16 тестовый профиль (`ops/test-env/`), test runner на `uv` (`ops/runner/`), минимальный GitHub Actions CI (`.github/workflows/ci.yml`), живые D0 risk-probes для `agy acp 1.5.15`.

Решение владельца (2026-11-23):
- Первый продуктовый runtime: родной движок **Google Antigravity (`agy acp`, ACP v1 over stdio)** с двумя явно разведёнными профилями входа:
  - интерактивный профиль через подписку **Google AI Ultra** (`oauth-personal`);
  - отдельный профиль **Gemini API key** (`gemini-api-key`) для будущих безлюдных фоновых задач.
- **Claude Code (`R-CLAUDE`)** и **Codex (`R-CODEX`)** переведены в отложенные будущие адаптеры вне обязательной приёмки D0–D9.

## Подтверждённые проверки D0 (на `fbbd61b`, Windows 11)

- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` — PASS (0 warnings).
- `uv run --project ops/runner dennett-check docs` — PASS (108 markdown-файлов, 4125 локальных ссылок).
- `uv run --project ops/runner dennett-check deps` — PASS (123 locked crates, `dennett-contracts` чист от SQL/UI/SDK, `dennett-memoryd` без Head/UI, в `Cargo.lock` отсутствуют `claude`/`codex`/`openai`/`anthropic`).
- `uv run --project ops/runner dennett-check fast` — PASS (43 passed, 0 failed, 0 skipped, 0 leaky; включает `proptest`, offline ACP-тесты, тесты `FaultInjectionGuard` и редакции секретов).
- `uv run --project ops/runner dennett-check self-check` — PASS (3/3 инварианта: 0-test прогон отклонён, intentional failure canary exit=100 пойман, `FaultInjectionGuard` без `DENNETT_TEST_PROFILE=1` отклонён).
- `uv run --project ops/runner dennett-check integration --manage-pg` (настоящий Docker `postgres:16-alpine` + отдельный процесс `dennett-memoryd` без Head):
  - `pg_isolation_integration` — PASS (3/3, включая отказ доступа к чужой БД и системной БД `postgres`).
  - `memoryd_process_integration` — PASS (5/5, включая `403` на `/shutdown` без `X-Dennett-Shutdown: 1`, `404` в `Production`, `503` + редакция пароля при недоступном PG).
- Негативный путь `uv run --project ops/runner dennett-check integration` (без `--manage-pg` и `DENNETT_TEST_*_PG_URL`) — PASS (exit 1, отказ без тихого skip).
- `uv run --project ops/runner dennett-check runtime-probe` (живой `agy acp 1.5.15`):
  - `acp_handshake_and_isolated_api_key_missing_fails_closed` — PASS (`initialize`, изолированный `--gemini-dir`/`--app-data-dir` без `GEMINI_API_KEY` → `-32602` / `-32000` без fallback на OAuth).
  - `acp_live_mcp_memory_tool_permission_and_cancel` — PASS (подтверждён tier `Google AI Ultra`, реальный Rust MCP-сервер `dennett-d0-mcp-fixture`, `session/request_permission` AllowOnce + возврат токена памяти, RejectOnce на `run_command` без создания файла, `session/cancel` → `stopReason: "cancelled"`).
- Мутационные проверки (3/3 пойманы и откачены до коммита): ослабление `FaultInjectionGuard`, ослабление редакции URI userinfo, ослабление проверки 0-test в runner.
- Независимый аудит Фазы 3 (два параллельных прохода: спецификация/идиомы Rust и `sharp-edges`/GHA security): все 6 BLOCKING и ключевые NON-BLOCKING замечания исправлены и перепроверены.

## Ограничения и находки для D1

- **Граница D0:** продуктовая логика (Memory Fabric, Head, Node, Desktop UI) ещё не реализована; `dennett-memoryd` на D0 является только bootstrap-процессом проверки здоровья/PG (`memory_fabric_implemented: false`).
- **Находка безопасности `agy acp 1.5.15` для D1:** в режиме `default` нативный `write_file` внутри рабочей папки `cwd` выполняется без `session/request_permission` (в отличие от `run_command` и MCP-инструментов). На D1/D2 для контроля записи в workspace нужно явно использовать правила `permissions.deny`/`ask` и изоляцию worktree/песочницы, не рассчитывая только на callback `default` mode.
- **Фоновый профиль `gemini-api-key`:** проверена строгая изоляция и отказ без ключа (`-32602`/`-32000`); живой LLM-ход по реальному `GEMINI_API_KEY` ещё не запускался (ключ не запрашивался на D0, потребуется на D2/D3 при включении безлюдных фоновых задач).
- **GitHub Actions CI и защита `main`:** `.github/workflows/ci.yml` создан с jobs `docs-deps-fast` (Ubuntu + Windows) и `postgres-integration` (Ubuntu + PG 16). Если настройки защиты `main` на GitHub всё ещё требуют удалённый старый check `Fast Gate`, владельцу нужно переключить правило защиты на новые jobs после пуша/PR.

## Этапы и приёмка

- D0 — основа/проверки: технически завершён (`d0/foundation`); **ожидает приёмки H0**.
- D1 — агент (`agy acp`) + `memoryd` + реальный сохраняемый чат: не начат; H1 не принят.
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

Открытых блокирующих дефектов D0 нет. Продуктовые модули D1–D9 ещё не реализованы.
