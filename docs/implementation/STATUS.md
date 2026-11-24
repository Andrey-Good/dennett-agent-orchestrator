# Текущее состояние новой реализации

Обновлено 2026-11-24. Текущая ветка реализации D2: `d2/durable-runs-and-stop` (от принятой базы D1 `7496ee2`). Новый baseline не наследует завершённые milestones, тесты или approvals старой реализации.

## Текущая работа

Текущий этап: **D2 («Поручения, автономное ожидание и предсказуемая остановка»)**. Состояние: **Slice A (`34d67ac`) + Slice B (`fe387e8`) + все 8 исправлений полного аудита H2 (`H2-01`..`H2-08`, включая устранение цикла блокировок V15 в `append_control_events_batch_in_tx`, схема `control` `v5`) завершены и независимо верифицированы на Carbon (живой браузерный UI/UX-обход + код/безопасность/стресс-тесты/мутации); остановка H2 ожидает приёмки владельца**. До явного принятия H2 переход к D3 не начинается.

Запись приёмки **H0** (2026-11-23): владелец явно принял этап D0 (`d0/foundation` @ `1a40e46`, CI [#45179510052](https://github.com/Andrey-Good/dennett-agent-orchestrator/actions/runs/45179510052)) и разрешил запуск D1.
Запись приёмки **H1** (2026-11-24): этап D1 (`d1/first-vertical-slice` @ `7496ee2`, `H1-01`..`H1-08` закрыты) принят владельцем; разрешён запуск этапа D2 на ветке `d2/durable-runs-and-stop`.
Запись дизайн-согласования **D2** (2026-11-24): утверждена спецификация [D2_ARCHITECTURE_AND_UX_BLUEPRINT.md](../design/D2_ARCHITECTURE_AND_UX_BLUEPRINT.md) (`c88debe`) в визуальном направлении `WP-M01-003` v5 без `<select>` и без `backdrop-filter`.

Основные коммиты и срезы D2 в ветке `d2/durable-runs-and-stop`:
- `c88debe` — спецификация архитектуры, схемы PostgreSQL `v4`, инвариантов конкурентности (`V07`, `V15`–`V18`, `90 §30`, `A05`/`A06`/`A07`/`A10`) и UX-экранов D2.
- `34d67ac` — **Slice A** (проверено на Carbon):
  - `crates/dennett-contracts/src/d2.rs` без расширения прямых зависимостей (`serde`, `thiserror`, `uuid`);
  - forward-миграция `services/head/migrations/0004_d2_durable_tasks_and_lifecycle.sql` (`LATEST_CONTROL_SCHEMA_VERSION = 4`), append-only `control_events`, `control_stream_counters` (`V15`/`V18`), `ordered_outbox` + `ordered_outbox_heads` (`V16`/`V17`);
  - `tasks`, `HeadScheduler` (таймеры `OnceAt`/`Interval`/`DailyLocalTime` с IANA TZ и `execute_once`/`skip`/`ask`), идемпотентные сигналы, `generation` fencing (`V07`), адресный `Stop`, делегирование помощнику с ослаблением прав и текстовые сообщения `POST /v1/agents/{id}/messages` (`A07`);
  - интерактивный `ask_before_write` (`permission_requests`, `inbox_cards`, `capability_grants` с отзывом, асинхронный `PolicyEvaluator` в `adapters/antigravity-acp`, сброс pending-запросов при `Stop`, `A05`/`A06`);
  - `SimulatedExternalConnector` с персистентным ledger (`--effects-ledger`), `LostAck -> unknown_external`, запретом слепого повтора, честным `Stop` (`external_outcome_unknown = true`) и `/reconcile`;
  - строгий жизненный цикл проектов контракта C (`rebind`/`detach-workspace`/`archive`/`unarchive`/`remove`/`delete`, `90 §30`), версии/полнота/жизненный цикл артефактов контракта D, `client_message_id` идемпотентность и полный CLI-паритет `dennettctl`.
- **Slice B + исправления полного аудита H2 (`H2-01`..`H2-08`, рабочая копия поверх `34d67ac`)**:
  - forward-миграция `services/head/migrations/0005_d2_h2_signal_and_recovery.sql` (`LATEST_CONTROL_SCHEMA_VERSION = 5`): колонка `tasks.expected_signal_name` для точного сопоставления имени сигнала (`H2-04`);
  - сквозное закрытие `H2-01`..`H2-08`: очистка зависших `permission_requests`/`inbox_cards`/`runs` при таймауте, обрыве HTTP (`ActiveRunGuard::drop`) и идемпотентном `Deny` (`H2-01`), аварийное восстановление `recover_after_startup` и автоподхват `queued`-поручений в `HeadScheduler` (`H2-02`), единая очистка и скрытие данных `deleted_record`/архивных проектов в `remove_project`/`delete_project`/UI (`H2-03`), сопоставление `expected_signal_name` и сохранение `waiting_signal` при правке цели с полем имени сигнала в `TasksCard` (`H2-04`), строгая сериализация `session_exec_lock` до регистрации `active_runs` (`H2-05`), подключение изолированного фонового runtime `gemini-api-key` при `--runtime antigravity --auth-profile oauth-personal` (`H2-06`), групповое логическое удаление всех ревизий артефакта и сброс превью (`H2-07`), глобальный `Escape` со всех вкладок, активные чаты в `Радаре активности`, устранение дубля текста `UNKNOWN` и raw backticks (`H2-08`);
  - `apps/desktop/src-tauri`: все D2 методы `HeadClient` проброшены в `HeadBridge` и Tauri-команды без расширения `capabilities/default.json` и CSP и без нарушения изоляции крейта `dennett-desktop`; добавлены 4 реальных PG16 интеграционных теста (`desktop_bridge_v03_integration`: 6/6);
  - `apps/desktop/src`: живая курсорная синхронизация `/v1/state/snapshot` + `/v1/events?after_cursor=...` с авто-resync по `409`, рабочие экраны `ActivityRail` (`Чаты` с inline-карточками подтверждения/уточнения/`UNKNOWN`, `Входящие` с живым счётчиком, `Радаром активности` и отзывом `capability_grants`, `Поручения` с быстрым созданием, кастомными поповерами расписания/бюджета, inline-редактированием цели, настраиваемым именем сигнала и адресным `Stop`), интерактивный `Спрашивать запись` в `Composer` и отзыв грантов в `SettingsSheet`, сохранение черновиков и офлайн-очередь с `client_message_id`, контекстные меню `…` проектов (контракт C + секция `Архив`) и версий/жизненного цикла артефактов (`v1`/`v2` в `WorkspacePanel`), capture-phase `Escape` на всех меню; 0 `<select>`, 0 `backdrop-filter`, 0 `dangerouslySetInnerHTML`, 0 dev-прокси строк в `apps/desktop/dist`;
  - `ops/runner`: добавлен `dennett-check demo-d2` (с `--runtime`, `--auth-profile`, `--no-ui`, `--dry-run`, `--effects-ledger`, `control_schema_version = 5` и проверкой блокировки `dennett-desktop.exe`), подняты пороги `MIN_INTEGRATION_TESTS = 46` и `MIN_DESKTOP_FRONTEND_TESTS = 32`.

## Подтверждённые проверки D2 (актуальная рабочая копия `d2/durable-runs-and-stop` поверх `34d67ac`)

- **Локально на Windows 11 (2026-11-24):**
  - `uv run --project ops/runner dennett-check all --manage-pg` — PASS:
    - `docs`: 111 Markdown-файлов, 4175 локальных ссылок валидны;
    - `deps`: 504 locked crates, все границы сохранены (`dennett-contracts` direct = `['serde', 'thiserror', 'uuid']`, `dennett-head` не зависит от `dennett-memory-core`, `dennett-desktop` изолирован от `Head`/`memory-core`/PG/`antigravity-acp`, 0 `claude`/`codex`/`openai`/`anthropic`);
    - `fast`: `tsc --noEmit`, Vitest **32/32** (0 failed, 0 skipped), Vite production build (0 совпадений dev-proxy/picker в `apps/desktop/dist`), `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo doc --no-deps` (`-D warnings`), `cargo test --doc`, fast nextest **124/124** (0 skipped, 0 leaky);
    - `self-check`: 4/4 инварианта harness подтверждены;
    - `integration --manage-pg`: **46/46** реальных PostgreSQL 16 + мультипроцессных тестов (0 skipped, 0 leaky; схема `control` `v5`, включая все D1-регрессии + `head_d2_concurrency_integration` `V07`/`V15`–`V18` и `H2-01`..`H2-07`, `head_d2_durable_and_stop_integration` `A05`/`A06`/`A07`/`A10`/`LostAck`/контракты C и D/`90 §30` и `desktop_bridge_v03_integration` 6/6).
  - `uv run --project ops/runner dennett-check demo-d2 --dry-run` — PASS (изолированный план запуска PG `v5` + `memoryd` + `Head` `--effects-ledger` + `ask_before_write`).
  - Нативная сборка Tauri 2: `cargo build -p dennett-desktop --features custom-protocol` — PASS (`target/debug/dennett-desktop.exe`, без запуска окна; `rg "__dennett_dev_head|__dennett_dev_dirs" --no-ignore apps/desktop/dist` = 0 совпадений).
  - `uv run --project ops/runner dennett-check runtime-probe` (живой `agy acp 1.5.15`, `Google AI Ultra`): PASS (**4/4** живых E2E теста — `acp_live_probe` 2/2 + `head_live_agy_e2e` 2/2; реальный `~/.gemini/antigravity-cli/conversations` не тронут: `before=25 after=25`, 0 висящих контейнеров).

## Ограничения D2 (граница этапа)

- Реализован сквозной контур D2: долговечные поручения/таймеры/сигналы, упорядоченный журнал доставки `V15`–`V18`, интерактивный `ask_before_write` с отзываемыми грантами, делегирование помощнику с ослаблением прав и текстовой кооперацией (`V07`, `A07`), `SimulatedExternalConnector` с `LostAck -> unknown_external` и сверкой `/reconcile`, строгий жизненный цикл проектов/артефактов (контракты C и D, `90 §30`), курсорный Desktop UI (`Входящие`, `Поручения`, офлайн-очередь черновиков с `client_message_id`) и `dennett-check demo-d2`.
- Реальные внешние коннекторы (Telegram/email/computer-use) остаются на D7 (в D2 проверяется детерминированный `SimulatedExternalConnector` с персистентным ledger); полнотекстовый/векторный поиск памяти и консолидация — D3; гибридные веб-поверхности L/MCP Apps — D4; multi-device, mTLS/peer auth и резервная реплика — D5; голос и сенсорика — D6.
- Живой LLM-прогон по платному `GEMINI_API_KEY` (фоновый профиль) зависит от наличия ключа в окружении; без ключа профиль fail-closed с `AUTH_REQUIRED` без тихого переключения на `oauth-personal`.
- На Windows внешний `SIGTERM` (`TerminateProcess`) в `demo-d1`/`demo-d2` обходит Python `finally`, а штатный интерактивный `Ctrl+C` корректно останавливает и убирает все демо-процессы и контейнер PG.

## Этапы и приёмка

- D0 — основа/проверки: **принят (H0 принят владельцем 2026-11-23 на `d0/foundation` `1a40e46`)**.
- D1 — агент (`agy acp`) + `memoryd` + реальный сохраняемый чат: **принят (H1 принят владельцем 2026-11-24 на `d1/first-vertical-slice` `7496ee2`)**.
- D2 — длительные поручения/ожидание/отмена: **реализован и верифицирован на `d2/durable-runs-and-stop` (схема `v5`, закрыты `H2-01`..`H2-08` и живой браузерный UI/UX-аудит); ожидает приёмки H2 владельцем**.
- D3 — полная память и подключения: не начат; H3 не принят.
- D4 — графика и рабочие экраны: не начат; H4 не принят.
- D5 — устройства и полный резерв: не начат; H5 не принят.
- D6 — голос и сенсорика: не начат; H6 не принят.
- D7 — внешние действия и computer-use: не начат; H7 не принят.
- D8 — проверяемое самоулучшение: не начат; H8 не принят.
- D9 — весь продукт и выпуск: не начат; H9 не принят.

## Среда и внешние условия

Локальная Windows 11 среда (2026-11-24):
- Rust stable `1.100.0` (`rustc`, `cargo`, `rustfmt`, `clippy`, `cargo-nextest 0.9.152`), MSVC Build Tools / VS 2022.
- Python `3.13.5` + `uv 0.11.23`, Node `v22.17.1` / `pnpm 10.33.0`.
- Docker `29.4.3` (`postgres:16-alpine`).
- Antigravity CLI `agy 1.5.15` (подписка `Google AI Ultra`, `g1-ultra-tier`).
- Архивная ветка старого кода: `archive/before-code-reset-2026-09-27`.

## Как обновлять эту запись

После связного шага кратко указать commit, результат, выполненные и оставшиеся проверки, препятствие, следующую задачу. Перед Hn дать ссылку на build, test evidence, ошибки и личную демонстрацию. Принятие фиксируется только по явному ответу владельца с областью и версией. Не публиковать приватные логи и токены.

## Открытые дефекты новой программы

Открытых блокирующих дефектов D0/D1/D2 нет; все 8 дефектов аудита H1 (`H1-01`..`H1-08`) и все 8 дефектов полного аудита H2 (`H2-01`..`H2-08`) исправлены, покрыты регрессионными и мутационными проверками и подтверждены в живом браузерном UI/UX-аудите на Carbon. Ожидание приёмки H2 владельцем.
