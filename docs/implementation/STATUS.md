# Текущее состояние новой реализации

Обновлено 2026-11-24. Текущая ветка реализации D2: `d2/durable-runs-and-stop` (от принятой базы D1 `7496ee2`). Новый baseline не наследует завершённые milestones, тесты или approvals старой реализации.

## Текущая работа

Текущий этап: **D2 («Поручения, автономное ожидание и предсказуемая остановка»)**. Состояние: **завершён и верифицирован полностью (`d2/durable-runs-and-stop`, схема `control` `v7`)** — включая Slice A/B, закрытие всех 21 замечаний внешнего аудита, `dennett-control-mcp`, все UX-директивы владельца, а также одиночные чаты вне проектов («Чаты» под «Проекты» с пропорцией `≤ 2/3 : ≥ 1/3` и независимыми скроллами), закрепление проектов и чатов и кнопку копирования сообщений в ленте. Пройдены живой браузерный UI/UX-обход на Carbon (с прямой сверкой с эталоном) и независимое ревью на Claude Opus 4.6 (`[]`). Ожидается финальная отмашка на старт **D3**.

Запись приёмки **H0** (2026-11-23): владелец явно принял этап D0 (`d0/foundation` @ `1a40e46`, CI [#45179510052](https://github.com/Andrey-Good/dennett-agent-orchestrator/actions/runs/45179510052)) и разрешил запуск D1.
Запись приёмки **H1** (2026-11-24): этап D1 (`d1/first-vertical-slice` @ `7496ee2`, `H1-01`..`H1-08` закрыты) принят владельцем; разрешён запуск этапа D2 на ветке `d2/durable-runs-and-stop`.
Запись дизайн-согласования **D2** (2026-11-24): утверждена спецификация [D2_ARCHITECTURE_AND_UX_BLUEPRINT.md](../design/D2_ARCHITECTURE_AND_UX_BLUEPRINT.md) (`c88debe`) и дополнение укрепления/UX [D2_HARDENING_AND_UX_REFINEMENT.md](../design/D2_HARDENING_AND_UX_REFINEMENT.md) (`ef37f19`) в визуальном направлении `WP-M01-003` v5 без `<select>` и без `backdrop-filter`.

Основные коммиты и срезы D2 в ветке `d2/durable-runs-and-stop`:
- `c88debe` — спецификация архитектуры, схемы PostgreSQL `v4`, инвариантов конкурентности (`V07`, `V15`–`V18`, `90 §30`, `A05`/`A06`/`A07`/`A10`) и UX-экранов D2.
- `34d67ac` — **Slice A** (проверено на Carbon): `dennett-contracts/src/d2.rs`, миграция `0004` (`v4`), append-only `control_events`, `ordered_outbox`, `HeadScheduler`, `ask_before_write`, `SimulatedExternalConnector`, контракты C и D, CLI-паритет `dennettctl`.
- `3c2368e` — **Slice B + `H2-01`..`H2-08`**: миграция `0005` (`v5`, `tasks.expected_signal_name`), аварийное восстановление `recover_after_startup`, очистка pending-разрешений при обрыве/таймауте, устранение цикла блокировок V15, Desktop D2 поверхности и `dennett-check demo-d2`.
- `ef37f19` + `d1c2acf` — **Slice R1 (Backend, Security & Control MCP Hardening, верифицировано на Carbon)**:
  - forward-миграция `services/head/migrations/0006_d2_hardening_r1.sql` (`LATEST_CONTROL_SCHEMA_VERSION = 6`): `sessions.session_kind` (`user_chat` / `task_run`), `task_timers.pause_reason` (`project_archived` / `user_paused`), `permission_requests.preview_snippet`;
  - закрытие `D2-SEC-01`/`02` (укреплённый `PolicyEngine`, канонизация целевых путей, структурное редактирование `preview_snippet`), `D2-ARCH-01..03` (`StateSnapshotResponse.running_session_ids`, унификация outbox/turn core, изоляция memoryd), `D2-BUG-01..05` (надёжные human-wait переходы, `rebind`, `WeeklyWallTime` расписание, сохранение `unknown_external` при архивации/удалении, `resolution_summary` в `ReconcileEffectResponse`) и `D2-OVER-01/02`;
  - новый MCP-сервер `dennett-control-mcp` (`dennett_event_create`, `dennett_event_list`, `dennett_event_update`, `dennett_event_cancel`) для агентного создания событий и расписаний из чата.
- **Slice R2 (Desktop UI, Tauri Mica & Vite Hardening, рабочая копия поверх `d1c2acf`)**:
  - **3 прямые UX-директивы владельца**: удалена ручная форма создания задач из `TasksCard.tsx` и заменена кнопкой `+` («Создать событие», 28×28) с автосозданием чистого чата и предзаполнением `"Создай событие: мне необходимо "` (курсор в конце); боковая панель проектов/чатов автоматически скрывается на экранах `Входящие` и `Поручения` (с полным `ProjectScopeFilter` по всем активным проектам и сохранением нижнего `UserSettingsBar`); из `ActivityRail.tsx` удалены дублирующая кнопка `Workspace` и нерабочая кнопка `Plug`;
  - **`D2-SEC-03` (`apps/desktop/devPickerGuard.ts`, `vite.config.ts`, `DevFolderPickerModal.tsx`)**: отказ на non-loopback `Host`/`Origin`, обязательные `Sec-Fetch-Site: same-origin` и заголовок `X-Dennett-Dev-Picker: 1` для `/__dennett_dev_dirs`;
  - **`D2-ARCH-01` + `D2-OVER-03` + `D2-BUG-06` (`WorkspaceProvider.tsx`, `MessageTimeline.tsx`)**: серверный `running_session_ids` без локальной блокировки `cancelSessionRun`; single-flight coalescing и монотонный `snapshotSeqRef`, спокойный ритм опроса (`2500ms` idle / `1000ms` active / `5000ms` hidden), сдвиг `streamCursorRef` только после успешного применения снимка, исправленный холодный реконнект при `sessions.length === 0`, явное подтверждение владельцем отправки офлайн-очереди без слепого авто-flush с пропуском терминальных `404`/`409` без клина очереди и строгая дедупликация оптимистических пузырей по `client_message_id`;
  - **`D2-UX-01`..`06` (`src-tauri/src/lib.rs`, `commands.rs`, `PortalPopover.tsx`, `useModalFocusTrap.ts`, `ProjectScopeFilter.tsx`, `TitleBar.tsx`, `Sidebar.tsx`, `WorkspacePanel.tsx`, `Composer.tsx`, `TasksCard.tsx`, `InboxRadarCard.tsx`, `StatusBanners.tsx`, `SettingsSheet.tsx`, `CommandPalette.tsx`, `tokens.css`, `desktop.css`, `ops/runner/src/dennett_runner/cli.py`)**: проба Mica с `is_mica_available` и solid dark fallback (`native-mica-unavailable` + `prefers-reduced-transparency: reduce`), плотность `.workspace-panel` `rgba(38, 38, 38, 0.50)`, двойной клик `TitleBar` → `toggleMaximize()` и защита `.command-search` от drag, единая плашка `SIMULATED`, адаптивный `960x620` (<1100px авто-свёртывание соседней панели), единый `PortalPopover` в `document.body` со взаимным исключением и capture-phase `Escape`, тёмный `:hover` на `.send-button.stop-button`, единые тёмные скроллбары без белых кнопок, скрытие `task_run` в `Sidebar` с сохранением «Открыть чат», инспекция архивных проектов в режиме чтения, подтверждение перед `remove`/`delete` проекта и `delete` артефакта, кликабельное переименование чата в шапке, сворачиваемое дерево файлов и архив артефактов без пустой кнопки `⋯` на tombstone, `SHA-256` + Копировать + безопасный Markdown-просмотр (`skipHtml`), синхронизация стартовой модели агента с `default_model_id`, честный статус «Ожидает подтверждения» с redacted `preview_snippet`, прямая сверка `UNKNOWN` в `TasksCard` с баннером вердикта, inline-карточки `budget_exceeded`/`missed_schedule`, единственное поле ввода при `waiting_clarification`, человеческий обратный отсчёт `через Xч Yм`, сохранение позиции скролла и `useModalFocusTrap` с возвратом фокуса.
  - `ops/runner`: пороги подняты до `MIN_DESKTOP_FRONTEND_TESTS = 45` и `MIN_INTEGRATION_TESTS = 55`, стартовые агенты `demo-d1`/`demo-d2` синхронизируются с `default_model_id` живого каталога.
- `c76c395` + `b08e56a` + `0b3c22c` — **Одиночные чаты, пины и `Copy` (схема `v7`, параллельные `share`-ворктри + Carbon Chromium + Claude Opus 4.6 `[]`)**:
  - forward-миграция `services/head/migrations/0007_standalone_chats_and_pins.sql` (`LATEST_CONTROL_SCHEMA_VERSION = 7`): `sessions.project_id` и `artifacts.project_id` стали `NULLABLE`, добавлены `pinned`/`pinned_at` и индексы сортировки на `projects` и `sessions`;
  - одиночные чаты работают изолированно (`DenyAll`, отдельный scratch-каталог, скоуп памяти `chat:standalone`, без `dennett-control-mcp`), поддерживают inline-артефакты, автозаголовок и выживают при архивации/удалении проектов;
  - в `Sidebar` реализованы две независимые скроллируемые секции («Проекты» `max-height: 66.666%` и «Чаты» `min-height: 33.334%` со своим `+`), закрепление проектов и чатов (`PushPin`), крошки `Чаты / <название>`, и кнопка копирования (`Copy`) на сообщениях пользователя и ассистента в `MessageTimeline`.

## Подтверждённые проверки D2 (`d2/durable-runs-and-stop`, схема `v7`)

- **Локально на Windows 11 (2026-11-24):**
  - `docs`: **112** Markdown-файлов, **4287** локальных ссылок валидны; `deps`: **505** locked crates, все границы сохранены;
  - `fast`: `tsc --noEmit`, Vitest **45/45** (`0 failed, 0 skipped`), Vite production build (`0` dev-утечек в `dist`), `rustfmt`, `clippy -D warnings`, `rustdoc -D warnings`, doctests, fast nextest **142/142** (`0 skipped, 0 leaky`);
  - `integration --manage-pg`: **55/55** реальных PostgreSQL 16 + мультипроцессных тестов (`0 skipped, 0 leaky`, схема `control` `v7`);
  - Нативная сборка Tauri 2: `cargo build -p dennett-desktop --features custom-protocol` — PASS;
  - Живой браузерный аудит на Carbon (`1280x800` и `960x620` поверх реального `agy acp 1.5.15`, `0` ошибок) и независимое ревью диффа на Claude Opus 4.6 (`[]`).

## Ограничения D2 (граница этапа)

- Реализован сквозной контур D2: долговечные поручения/таймеры/сигналы (`OnceAt`/`Interval`/`DailyLocalTime`/`WeeklyWallTime`), агентное создание событий через `dennett-control-mcp`, упорядоченный журнал доставки `V15`–`V18`, интерактивный `ask_before_write` с отзываемыми грантами и `preview_snippet`, делегирование помощнику с ослаблением прав и текстовой кооперацией (`V07`, `A07`), `SimulatedExternalConnector` с `LostAck -> unknown_external` и сверкой `/reconcile`, строгий жизненный цикл проектов/артефактов (контракты C и D, `90 §30`), курсорный Desktop UI (`Чаты`, `Входящие`, `Поручения`, подтверждаемая офлайн-очередь черновиков с `client_message_id`) и `dennett-check demo-d2`.
- Реальные внешние коннекторы (Telegram/email/computer-use) остаются на D7 (в D2 проверяется детерминированный `SimulatedExternalConnector` с персистентным ledger); полнотекстовый/векторный поиск памяти и консолидация — D3; гибридные веб-поверхности L/MCP Apps — D4; multi-device, mTLS/peer auth и резервная реплика — D5; голос и сенсорика — D6.
- Живой LLM-прогон по платному `GEMINI_API_KEY` (фоновый профиль) зависит от наличия ключа в окружении; без ключа профиль fail-closed с `AUTH_REQUIRED` без тихого переключения на `oauth-personal`.
- На Windows внешний `SIGTERM` (`TerminateProcess`) в `demo-d1`/`demo-d2` обходит Python `finally`, а штатный интерактивный `Ctrl+C` корректно останавливает и убирает все демо-процессы и контейнер PG.

## Этапы и приёмка

- D0 — основа/проверки: **принят (H0 принят владельцем 2026-11-23 на `d0/foundation` `1a40e46`)**.
- D1 — агент (`agy acp`) + `memoryd` + реальный сохраняемый чат: **принят (H1 принят владельцем 2026-11-24 на `d1/first-vertical-slice` `7496ee2`)**.
- D2 — длительные поручения/ожидание/отмена: **завершён и верифицирован на `d2/durable-runs-and-stop` (схема `v7`: все 21 аудит-фиксы, `dennett-control-mcp`, одиночные чаты `2/3:1/3`, пины, `Copy`, Carbon live browser + Claude Opus 4.6 `[]`); готов к финальному закрытию H2 и переходу к D3**.
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

Открытых блокирующих дефектов D0/D1/D2 нет; все дефекты H1, H2, внешнего аудита (21 пункт) и живого браузерного ревью одиночных чатов/пинов (`CHATS-UI-01`/`02`) закрыты и подтверждены тестами, живым браузером и ревью на Claude Opus 4.6 (`[]`).
