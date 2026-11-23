# Текущее состояние новой реализации

Обновлено 2026-11-23. Текущая ветка реализации D1: `d1/first-vertical-slice` (от принятой базы D0 `d0/foundation` `1a40e46`). Новый baseline не наследует завершённые milestones, тесты или approvals старой реализации.

## Текущая работа

Текущий этап: **D1** (первый настоящий путь: агент на `agy acp`, самостоятельный `memoryd` на PostgreSQL, сохраняемый разговор и новый desktop-чат). Состояние: **технически завершён; ожидает владельца (точка приёмки H1)**. До явного принятия H1 переход к D2 не начинается.

Запись приёмки **H0** (2026-11-23): владелец явно принял этап D0 (`d0/foundation` @ `1a40e46`, CI [#45179510052](https://github.com/Andrey-Good/dennett-agent-orchestrator/actions/runs/45179510052)) и разрешил запуск D1.
Запись дизайн-согласования **D1 Desktop Chat** (2026-11-23): владелец подтвердил визуальное направление `WP-M01-003` v5 (единый бесшовный Windows 11 Mica сверху и слева + отдельный скруглённый прямоугольник чата + отдельная выезжающая правая панель артефактов поверх того же Mica-фона, код пишется с нуля).

Основные коммиты D1 в ветке `d1/first-vertical-slice`:
- `26c8c4e` — фиксация приёмки H0 и старта D1.
- `dc5e184` — **Slice 1** (проверено на Carbon): самостоятельный `dennett-memoryd` на PostgreSQL (`dennett-memory-core`, миграции `0001_memory_core_v1.sql`, `event_evidence_links` по V19, `StandaloneAuthVerifier`, `axum` API, `dennett-memory-client`, `dennett-memory-cli`, `dennett-memory-mcp`; `V01`, `V06`, `V19`; 3 блокерных дефекта найдены Carbon и закрыты, 3 мутации пойманы).
- `2c3d1b4` — **Slice 2** (проверено на Carbon): `dennett-head` (`dennett_control` БД, миграции, `PolicyEngine`, Agent/ConfigVersion/Project/Session/Messages/Artifacts, V02 `memory_commit_outbox`, `HeadClient`/`dennettctl`) + продуктовый `adapters/antigravity-acp` с реальным `dennett-memory-mcp`, `PreToolUse` защитой записи в workspace, `AllowOnce`/`RejectOnce`, `Stop`/`session/cancel`, явным `session_id`/rehydrate и строгим разделением `oauth-personal` vs `gemini-api-key` (`V02`, `V04`, `V05`, `V06`, `V29`, `A01–A06`, `A09`; 5 блокерных дефектов найдены Carbon и закрыты, 3 мутации пойманы).
- `85b8f76` — **Slice 3** (проверено на Carbon): новый `apps/desktop` (Tauri 2 + React 19/TS/Vite, Windows 11 Mica двухслойная композиция v5, правая панель артефактов, честные состояния пустого старта / Head offline / Stop / V02 деградации памяти / `SIMULATED` vs `Antigravity ACP · Google AI Ultra`, `V03` независимость демонов при закрытии окна, безопасный запуск `dennett-check demo-d1`, CI на Linux + Windows + PG; 3 блокерных дефекта найдены Carbon и закрыты, 4 мутации пойманы).
- `b1432a1` — фикс кодировки `cp1252` в `ops/runner/src/dennett_runner/cli.py` для `windows-latest` CI.

## Подтверждённые проверки D1 (на `b1432a1`)

- **GitHub Actions CI (`Dennett CI`):** реальный зелёный прогон [#45229373219](https://github.com/Andrey-Good/dennett-agent-orchestrator/actions/runs/45229373219) на `b1432a1` — все 3 jobs `success`:
  - `Docs, deps, fast & self-check (ubuntu-latest)` — PASS
  - `Docs, deps, fast & self-check (windows-latest)` — PASS
  - `PostgreSQL, memoryd, Head & Desktop V03 integration (--manage-pg)` — PASS
- **Локально на Windows 11 (`85b8f76` / `b1432a1`):**
  - `uv run --project ops/runner dennett-check all --manage-pg` — PASS: `docs` (108 файлов, 4125 ссылок), `deps` (493 locked crates; подтверждены границы: `dennett-head` не зависит от `dennett-memory-core`, `dennett-desktop` не зависит от `dennett-head`/`memory-core`/PG/`antigravity-acp`, 0 `claude`/`codex`/`openai`/`anthropic`), `fast` (`tsc --noEmit`, Vitest 11/11, Vite build, `fmt`, `clippy -D warnings`, `rustdoc -D warnings`, `cargo test --doc`, 105/105 unit+proptest, 0 skipped/leaky), `self-check` (4/4), `integration --manage-pg` (27/27 реальных PostgreSQL 16 + мультипроцессных тестов, включая `V01`, `V02`, `V03`, `V05`, `V06`, `V19`, `V29`).
  - Нативные сборки Tauri 2: `cargo build -p dennett-desktop` и `cargo build -p dennett-desktop --features custom-protocol` — PASS (`target/debug/dennett-desktop.exe`).
  - `uv run --project ops/runner dennett-check runtime-probe` (живой `agy acp 1.5.15`, `Google AI Ultra`): PASS (4/4 живых E2E теста — `acp_live_probe` 2/2 + `head_live_agy_e2e` 2/2: пре-сид факта в отдельный `memoryd` при выключенном `head` → вызов `dennett-memory-mcp` живым агентом → разрешённый файл + блокировка запрещённого файла → запись обновления в память → перезапуск `head` и `memoryd` → живой `Stop`/cancel → fail-closed `gemini-api-key` без ключа; реальный `~/.gemini/antigravity-cli/conversations` не тронут: `33 -> 33`, 0 висящих контейнеров).
  - Негативный путь `dennett-check integration` без `--manage-pg` — PASS (exit 1, без тихого skip).
  - Мутационные проверки Carbon-верификаторами: **10/10 пойманы и байт-в-байт откачены** (3 в Slice 1, 3 в Slice 2, 4 в Slice 3).

## Ограничения D1 (граница этапа)

- Реализовано сквозное ядро D1 (память с источниками/версиями/историей/scope-правами, `head` + `agy acp`, базовый desktop-чат и обычный артефакт). Полнотекстовый/векторный поиск и консолидация памяти — D3; длительные фоновые поручения/расписания/таймеры — D2; гибридные веб-поверхности L/MCP Apps — D4; multi-device и резервная реплика — D5; голос и сенсорика — D6.
- Живой LLM-прогон по платному `GEMINI_API_KEY` (фоновый профиль) остаётся на D2/D3; на D1 живьём подтверждены интерактивный `oauth-personal` (`Google AI Ultra`) и жёсткий fail-closed отказ `gemini-api-key` без ключа.
- Авторизация HTTP API `dennett-head` сейчас привязана к `loopback-only` (однопользовательский локальный D1); mTLS/peer auth для Node и нескольких устройств вводится на D5.
- На Windows внешний `SIGTERM` (`TerminateProcess`) в `demo-d1` обходит Python `finally`, а штатный интерактивный `Ctrl+C` корректно останавливает и убирает все демо-процессы и контейнер PG.

## Этапы и приёмка

- D0 — основа/проверки: **принят (H0 принят владельцем 2026-11-23 на `d0/foundation` `1a40e46`)**.
- D1 — агент (`agy acp`) + `memoryd` + реальный сохраняемый чат: **технически завершён (`d1/first-vertical-slice` `b1432a1`); ожидает приёмки H1**.
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
- Docker `29.4.3` (`postgres:16-alpine`).
- Antigravity CLI `agy 1.5.15` (подписка `Google AI Ultra`, `g1-ultra-tier`).
- Архивная ветка старого кода: `archive/before-code-reset-2026-09-27`.

## Как обновлять эту запись

После связного шага кратко указать commit, результат, выполненные и оставшиеся проверки, препятствие, следующую задачу. Перед Hn дать ссылку на build, test evidence, ошибки и личную демонстрацию. Принятие фиксируется только по явному ответу владельца с областью и версией. Не публиковать приватные логи и токены.

## Открытые дефекты новой программы

Открытых блокирующих дефектов D1 нет (все 11 замечаний Carbon-аудита Slice 1–3 и CI `cp1252` баг закрыты до H1).
