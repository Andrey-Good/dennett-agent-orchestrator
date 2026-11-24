# D2 Hardening, Agent-Native Event MCP & UX Refinement Blueprint

- **Статус:** Утверждённый инженерный и продуктовый план доработки этапа `D2` перед финальной сдачей ворот `H2`
- **Ветка базового среза:** `d2/durable-runs-and-stop` (`3c2368e`)
- **Связанные документы:** [AGENTS.md](../../AGENTS.md), [STATUS.md](../implementation/STATUS.md), [04_MILESTONE_DEPENDENCY_MAP.md](../implementation/04_MILESTONE_DEPENDENCY_MAP.md), [D2_ARCHITECTURE_AND_UX_BLUEPRINT.md](D2_ARCHITECTURE_AND_UX_BLUEPRINT.md), [UX_AND_PRODUCT_BACKLOG.md](UX_AND_PRODUCT_BACKLOG.md), [TEST_CATALOGUE_AND_QUALITY_GATES.md](../testing/TEST_CATALOGUE_AND_QUALITY_GATES.md), [ANTIGRAVITY_RUNTIME_ACCEPTANCE.md](../testing/ANTIGRAVITY_RUNTIME_ACCEPTANCE.md)

---

## 1. Назначение, границы и критический отбор продуктовых предложений

Настоящий документ закрывает три связанных блока поверх текущей реализации `D2`:

1. **15 архитектурных, защитных, конкурентных и инфраструктурных дефектов внешнего аудита** (`D2-ARCH-01`..`03`, `D2-SEC-01`..`03`, `D2-BUG-01`..`06`, `D2-OPS-01`, `D2-OVER-01`..`03`) с привязкой к точным строкам текущего репозитория.
2. **Агентно-нативное управление расписаниями и ожиданием сигналов через минимальный MCP-адаптер (`dennett-control-mcp`)** и прямую миграцию схемы `v6` (`0006_d2_hardening_and_events.sql`), чтобы владелец создавал отложенные/регулярные/сигнальные события естественным диалогом с агентом в чате без бюрократических форм и без изобретения собственного workflow-DSL/DAG.
3. **Единую доработку десктопного интерфейса (`D2-UX-01`..`D2-UX-06` + 3 продуктовые директивы владельца + выверенные UX-решения)**.

### 1.1. Неприкосновенные архитектурные границы (`AGENTS.md` и `dennett-check arch`)

- **`dennett-contracts`:** прямые зависимости в `Cargo.toml` остаются строго `{"serde", "thiserror", "uuid"}` (проверяется в [cli.py](../../ops/runner/src/dennett_runner/cli.py)). Никаких `chrono`, `url`, `percent-encoding` или `tokio` внутри `dennett-contracts` — все валидаторы расписаний и подмножеств политик пишутся на чистом `std`.
- **`dennett-memoryd` и `dennett-memory-core`:** полностью независимы от `dennett-head`. Защита loopback `Host`/`Origin`/`Sec-Fetch-Site` в `dennett-memoryd` (`D2-SEC-03`) реализуется автономно (либо через чистую функцию без внешних крейтов), без импорта `dennett-head`.
- **`CLI / MCP / UI -> HeadClient -> dennett-head`:** новый stdio MCP-сервер `dennett-control-mcp` строится по тому же тонкому шаблону, что и [adapters/memory-mcp/src/main.rs](../../adapters/memory-mcp/src/main.rs), и обращается исключительно к `HeadClient` (`crates/dennett-head-client`), не подключаясь к PostgreSQL напрямую.
- **Тонкий десктоп (`dennett-desktop`):** не зависит от `dennett-head`, `dennett-memory-*`, `dennett-runtime-antigravity`, `dennett-control-mcp`, `dennett-testkit` и драйверов БД.
- **Родной движок `agy acp`:** остаётся единственным продуктовым runtime `D0`–`D9` с явным разведением профилей `oauth-personal` (интерактивный) и `gemini-api-key` (headless-фон) без тихого переключения между ними и без добавления зависимостей Claude Code / Codex.
- **Честность `UNKNOWN` и сохранность файлов пользователя:** ни одна операция `Stop`, `archive`, `remove`, `delete` или `finalize_turn` не имеет права перетереть `unknown_external` / `unknown_effect` в «успешно» или «тихо отменено» до явной сверки (`reconcile_external_effect`), и ни одна операция жизненного цикла не удаляет файлы в рабочей папке пользователя (`user_workspace_files_deleted == false`).

### 1.2. Критическая оценка предложений UX-консультанта (`@opus-ux-advisor`)

Все предложения из дизайн-меморандума проверены на соответствие `AGENTS.md`, `WP-M01-003` и анти-оверинжинирингу `D2`:

- **ПРИНЯТО в реализацию `D2`:**
  1. Полное удаление ручной 6-польной формы `.d2-task-composer-shell` из [TasksCard.tsx](../../apps/desktop/src/components/TasksCard.tsx); замена на лаконичную кнопку `+` (`title="Создать событие"`, `data-testid="task-create-plus-btn"`), которая атомарно создаёт чистый чат `user_chat` в целевом проекте, переключает `activeView = "chat"` и подставляет черновик `"Создай событие: мне необходимо "` с курсором в конце строки.
  2. Автоматическое схлопывание левой колонки проектов/чатов на экранах `Входящие` и `Поручения` (`effectiveSidebarOpen = state.isSidebarOpen && state.activeView === "chat"`) с сохранением пользовательского флага `isSidebarOpen` при возврате в `Чаты` и аккуратным отображением нижней панели профиля в `52px` рельсе.
  3. Полноценный кастомный фильтр по всем активным/отвязанным проектам (`Все проекты` + список всех проектов на базе `.floating-popover`, `0 <select>`) в шапках `Входящие` и `Поручения`.
  4. Удаление дублирующей кнопки `Workspace` (`Browsers`) и мёртвой отключённой заглушки `Plug` из [ActivityRail.tsx](../../apps/desktop/src/components/ActivityRail.tsx), оставляющее в рельсе ровно 3 рабочих режима (`Чаты`, `Входящие`, `Поручения`) и индикаторы квот `QuotaRings` внизу.
  5. Отделение служебных сессий фоновых запусков (`session_kind = 'task_run'`) от пользовательских диалогов (`session_kind = 'user_chat'`): `Sidebar` и `CommandPalette` по умолчанию показывают чистый список `user_chat`, а переход по кнопке «Открыть чат» из `Поручений` или `Входящих` открывает `task_run`-сессию с понятной плашкой контекста и разрешает отправку из нижнего поля ввода только тогда, когда поручение ждёт ответа владельца (`waiting_clarification` -> маршрутизация в `replyToTask`), исключая дублирование двух полей ввода.
  6. Живой обратный отсчёт `через Xч Yм` (`tabular-nums`), однокликовая отправка ожидаемого сигнала (`Отправить сигнал «{name}»` с опциональным раскрытием текста полезной нагрузки) и прямая кнопка «Проверить у сервиса» на карточках `unknown_external` в `TasksCard`.
  7. Честная индикация статуса «Ожидает подтверждения» вместо ложного спиннера «Выполняется» при открытом `permission_request`, компактный редактированный предпросмотр записываемого фрагмента (`preview_snippet`) в карточках подтверждения, явная сводка вердикта сверки `UNKNOWN` и сохранение позиции прокрутки списков.
  8. Доработка правой панели `WorkspacePanel`: сворачиваемые секции и вложенное дерево папок, отдельный свёрнутый блок `Архив (N)` для архивных артефактов, явное подтверждение перед необратимым `delete`/`remove`, скрытие пустой кнопки `⋯` у `deleted`-заглушек, показ серверного `SHA-256` с кнопкой копирования и переключатель `Просмотр / Исходник` для Markdown-артефактов через уже установленный `<ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml>` (без `dangerouslySetInnerHTML`).
  9. Порталирование выпадающих меню (`createPortal(..., document.body)` + `position: fixed` с расчётом границ окна), исправление клика по `.command-search` и двойного клика по `TitleBar`, корректный красный `:hover` для `.stop-button`, авто-схлопывание боковой панели при ширине `< 1100px` (`960x620`) и ловушка фокуса `useModalFocusTrap` в модальных окнах.

- **СКОРРЕКТИРОВАНО / ОТКЛОНЕНО (защита от лишнего кода и визуальных компромиссов):**
  1. **Стрелки полос прокрутки в Windows WebView2 — исправить полностью, а не списывать в ограничения:** в Chromium/WebView2 свойство `scrollbar-width: thin` конфликтует с `::-webkit-scrollbar` либо оставляет системные кнопки-стрелки. В `desktop.css` задаются явные правила `::-webkit-scrollbar`, `::-webkit-scrollbar-thumb`, `::-webkit-scrollbar-track` и `::-webkit-scrollbar-button { display: none; width: 0; height: 0; }` (при снятии `scrollbar-width` на WebKit/Blink-контейнерах), что полностью убирает светлые стрелки Windows.
  2. **`SHA-256` артефакта — не пересчитывать в браузере через `crypto.subtle`:** поле `sha256: String` уже канонически хранится в PostgreSQL (`0001_control_core_v1.sql`) и приходит в каждом `ArtifactRecord`. Панель `WorkspacePanel` отображает готовый `selectedArt.sha256` напрямую.
  3. **Еженедельное расписание (`weekly_days`) — включить сразу в `v6` без внешних библиотек:** поддержка запросов вида *«каждый вторник и четверг в 09:00»* реализуется добавлением `ScheduleKind::WeeklyWallTime { weekdays_mask: u8, hour: u8, minute: u8 }` и 12 строками чистого SQL в PostgreSQL 16 (`AT TIME ZONE` + `EXTRACT(ISODOW FROM ...)` по горизонту 1..8 суток), без cron-парсеров и сторонних зависимостей.

---

## 2. Разделение работ на непересекающиеся срезы (`Slice R1` и `Slice R2`)

Чтобы исключить конфликты правок между исполнителями на **Neon**, работа делится на два строго изолированных по файлам среза (с последовательной либо изолированной в отдельных worktree сборкой и финальной совместной интеграцией):

### Slice R1 — Backend, Security, `dennett-control-mcp`, Schema `v6`, Runner & CI
- **Владеемые пути:**
  - `Cargo.toml` (добавление `adapters/control-mcp` в `[workspace.members]` и `[workspace.dependencies]`)
  - `crates/dennett-contracts/src/{lib.rs, control.rs, d2.rs}` (обратно-совместимые `#[serde(default)]` поля для `SessionKind`, `TimerPauseReason`, `WeeklyWallTime`, `PermissionRequestRecord.preview_snippet`, исправление `write_policy_is_subset_of` и `OnceAt` валидации)
  - `services/head/migrations/0006_d2_hardening_and_events.sql` и `services/head/src/migrations.rs` (`LATEST_CONTROL_SCHEMA_VERSION = 6`)
  - `services/head/src/{main.rs, lib.rs, http.rs, store.rs, service.rs, tasks.rs, lifecycle.rs, effects.rs, approvals.rs, events.rs, policy.rs, memory_auth.rs}` + модульные и интеграционные тесты в `services/head/tests/*`
  - `services/memoryd/src/main.rs` (`D2-SEC-03` loopback middleware без зависимости от `dennett-head`)
  - `adapters/antigravity-acp/src/{lib.rs, antigravity.rs, transport.rs, simulated.rs}` (`D2-SEC-01` slot reuse digest & TTL, `D2-BUG-03` пауза дедлайна на время ожидания человека, `D2-BUG-05` non-blocking `Drop`, `[SIM:EVENT:...]` в `SimulatedRuntime`)
  - `adapters/control-mcp/{Cargo.toml, src/main.rs}` (новый минимальный stdio MCP-сервер поверх `HeadClient`)
  - `crates/dennett-head-client/src/{lib.rs, bin/dennettctl.rs}` (передача опционального `X-Dennett-Control-Token`, флаги бюджетов в `task update`, синхронизация `--help`, фильтр `session_kind`)
  - `crates/dennett-testkit/src/lib.rs` (при необходимости обновления тестовых хелперов)
  - `ops/runner/src/dennett_runner/cli.py` и `.github/workflows/ci.yml` (`D2-OPS-01` + сборка `dennett-control-mcp` + архитектурные проверки границ для `dennett-control-mcp`)
- **Строгий запрет для R1:** не редактировать файлы внутри `apps/desktop/**` (все новые поля в Rust-структурах запросов контрактов снабжаются `#[serde(default)]`, чтобы `apps/desktop/src-tauri` продолжал собираться без правок до вливания `R2`).

### Slice R2 — Desktop UI, Tauri Shell, Vite Dev Security & Frontend State
- **Владеемые пути:**
  - `apps/desktop/vite.config.ts` (`D2-SEC-03` защита `/__dennett_dev_dirs`)
  - `apps/desktop/src-tauri/{tauri.conf.json, src/lib.rs, src/commands.rs, src/bridge.rs}` (проверка поддержки Mica / fallback фона для Windows 10 и проброс новых полей контрактов)
  - `apps/desktop/src/api/{contracts.ts, bridge.ts, devHttpBridge.ts, mockBridge.ts}`
  - `apps/desktop/src/state/WorkspaceProvider.tsx` (`D2-OVER-03`, `D2-BUG-06`, канонический статус активных запусков из серверного снимка, `createBlankChatWithDraft`, авто-режим сайдбара, responsive `< 1100px`)
  - `apps/desktop/src/components/{PortalPopover.tsx, useModalFocusTrap.ts, ActivityRail.tsx, TitleBar.tsx, Sidebar.tsx, TasksCard.tsx, InboxRadarCard.tsx, WorkspacePanel.tsx, Composer.tsx, MessageTimeline.tsx, StatusBanners.tsx, SettingsSheet.tsx, CommandPalette.tsx, DevFolderPickerModal.tsx, StatsPopover.tsx}`
  - `apps/desktop/src/styles/{tokens.css, desktop.css}`
  - `apps/desktop/src/**/*.test.{ts,tsx}`
- **Финальная документация состояния (после зелёного прогона R1 + R2):** `docs/implementation/STATUS.md`, `docs/design/UX_AND_PRODUCT_BACKLOG.md`.

---

## 3. Прямая миграция БД `0006_d2_hardening_and_events.sql` (`v6`) и дополнения `dennett-contracts`

Обновить `LATEST_CONTROL_SCHEMA_VERSION` в [migrations.rs](../../services/head/src/migrations.rs) с `5` до `6` и зарегистрировать `0006_d2_hardening_and_events.sql` (строго forward-only миграция без правок `0001`–`0005`):

### 3.1. Изменения схемы PostgreSQL (`0006_d2_hardening_and_events.sql`)

1. **Тип сессии (`sessions.session_kind`):**
   ```sql
   ALTER TABLE sessions
       ADD COLUMN IF NOT EXISTS session_kind TEXT NOT NULL DEFAULT 'user_chat'
       CHECK (session_kind IN ('user_chat', 'task_run'));

   -- Ретроспективно помечаем существующие сессии, привязанные к фоновым/отложенным задачам,
   -- если они были созданы специально под задачу (title начинается с task title или нет ручных ходов до задачи):
   UPDATE sessions s
   SET session_kind = 'task_run'
   WHERE EXISTS (SELECT 1 FROM tasks t WHERE t.session_id = s.session_id)
     AND NOT EXISTS (
         SELECT 1 FROM messages m
         WHERE m.session_id = s.session_id
           AND m.role = 'user'
           AND m.created_at < (SELECT MIN(t2.created_at) FROM tasks t2 WHERE t2.session_id = s.session_id)
     );

   CREATE INDEX IF NOT EXISTS idx_sessions_project_kind_updated
       ON sessions (project_id, session_kind, updated_at DESC);
   ```
2. **Причина паузы таймера и еженедельное расписание (`task_timers`):**
   ```sql
   ALTER TABLE task_timers
       ADD COLUMN IF NOT EXISTS pause_reason TEXT
       CHECK (pause_reason IS NULL OR pause_reason IN ('archived_project', 'misfire_ask')),
       ADD COLUMN IF NOT EXISTS weekdays_mask SMALLINT
       CHECK (weekdays_mask IS NULL OR (weekdays_mask BETWEEN 1 AND 127));

   ALTER TABLE task_timers DROP CONSTRAINT IF EXISTS task_timers_recurrence_kind_check;
   ALTER TABLE task_timers ADD CONSTRAINT task_timers_recurrence_kind_check
       CHECK (recurrence_kind IS NULL OR recurrence_kind IN ('interval', 'daily_wall', 'weekly_wall'));

   -- Существующие paused-таймеры без причины получают консервативную метку:
   UPDATE task_timers t
   SET pause_reason = CASE
       WHEN EXISTS (
           SELECT 1 FROM projects p
           WHERE p.project_id = (SELECT tk.project_id FROM tasks tk WHERE tk.task_id = t.task_id)
             AND p.lifecycle_state = 'archived'
       ) THEN 'archived_project'
       ELSE 'misfire_ask'
   END
   WHERE t.status = 'paused' AND t.pause_reason IS NULL;
   ```
3. **Персистентный вход продолжения задачи (`tasks.pending_continuation_input` для `D2-BUG-01`):**
   ```sql
   ALTER TABLE tasks
       ADD COLUMN IF NOT EXISTS pending_continuation_input TEXT;
   ```
   Ответ владельца в `reply_to_task` / `resolve_clarification_card` и payload сигнала при пробуждении из `waiting_signal` записываются прямо в `tasks.pending_continuation_input` внутри той же транзакции, которая переводит задачу в `status = 'queued'`. В `execute_task_generation` это поле атомарно считывается и очищается под `FOR UPDATE` вместе с необработанными строками `task_signals (consumed_in_generation IS NULL)`, поэтому ни гонка с `HeadScheduler`, ни рестарт `dennett-head` никогда не теряют ответ пользователя или сигнал.
4. **Редактированный сниппет предпросмотра записи (`permission_requests.preview_snippet` для `D2-UX-05`):**
   ```sql
   ALTER TABLE permission_requests
       ADD COLUMN IF NOT EXISTS preview_snippet TEXT;
   ```
5. **Консолидация `memory_commit_outbox` в `ordered_outbox` (`D2-OVER-01`):**
   - В миграции `0006` перенести все существующие записи из `memory_commit_outbox` в `ordered_outbox` (`destination = 'memoryd'`, `ordering_key = scope`, `idempotency_key = 'mem-commit:' || idempotency_key`, `payload_json = jsonb_build_object('outbox_id', outbox_id, 'scope', scope, 'note_id', note_id, 'idempotency_key', idempotency_key, 'request_json', payload_json::jsonb, 'legacy_created_at_ms', created_at)`, `status = CASE status WHEN 'delivered' THEN 'delivered' ELSE 'pending' END`) и синхронизировать `ordered_outbox_heads (destination, ordering_key, next_seq)` до `MAX(seq_in_key) + 1`.
   - Оставить таблицу `memory_commit_outbox` пустой/архивной без записи в неё из кода (либо `DROP TABLE memory_commit_outbox`), переведя весь runtime-код `HeadService` на единый `ordered_outbox`.

### 3.2. Изменения в `crates/dennett-contracts` (0 новых зависимостей)

- **`control.rs`:**
  - Добавить `#[non_exhaustive] pub enum SessionKind { UserChat, TaskRun }` (`serde(rename_all = "snake_case")`, `Default = UserChat`).
  - В `SessionRecord` и `CreateSessionRequest` добавить `#[serde(default)] pub session_kind: SessionKind`.
- **`d2.rs`:**
  - В `ScheduleKind` добавить вариант:
    ```rust
    WeeklyWallTime {
        /// Bitmask of ISO weekdays: bit 0 (1) = Monday .. bit 6 (64) = Sunday; must be 1..=127.
        weekdays_mask: u8,
        hour: u8,
        minute: u8,
    }
    ```
    В `ScheduleSpec::validate()` проверять `1 <= weekdays_mask && weekdays_mask <= 127`, `hour <= 23`, `minute <= 59`, а также вызвать `fire_at.validate()?` в ветке `ScheduleKind::OnceAt { fire_at }` (`D2-BUG-05`).
  - Добавить `pub enum TimerPauseReason { ArchivedProject, MisfireAsk }` и `#[serde(default)] pub pause_reason: Option<TimerPauseReason>` в `TaskTimerRecord`.
  - В `PermissionRequestRecord` добавить `#[serde(default)] pub preview_snippet: Option<String>`.
  - В `ReconcileEffectResponse` (или в `InboxCardRecord.detail` при резолве `unknown_effect`) убедиться, что возвращается человекочитаемая сводка результата сверки (`resolution_summary: String` с `#[serde(default)]`).
  - В `write_policy_is_subset_of` ([d2.rs:2720-2745](../../crates/dennett-contracts/src/d2.rs)) **удалить** ветку `(WorkspaceWritePolicy::AskBeforeWrite, WorkspaceWritePolicy::AllowPaths { .. }) => true` (`D2-SEC-02`).

---

## 4. Детальная инженерная спецификация по всем 15 пунктам аудита

> [!IMPORTANT]
> Во всех транзакциях `services/head` строго сохраняется порядок блокировок `V15`:
> **доменные строки (`projects` -> `tasks` -> `task_timers` / `runs` / `inbox_cards` / `permission_requests` / `effect_claims` / `ordered_outbox`) -> `audit_events` -> отсортированные `pg_advisory_xact_lock('entity:<key>')` -> последним `pg_advisory_xact_lock('control:journal')` при вставке в `control_events`.**

### 4.1. `D2-ARCH-01` (CRITICAL) — Отвязанное от HTTP-запроса выполнение хода чата (`V03`) и join по `client_message_id`
- **Точные места дефекта:**
  - [service.rs:223-253](../../services/head/src/service.rs) (`ActiveRunGuard::drop` вызывает `abort_handle.abort()` и `finalize_abandoned_run` при обрыве HTTP-соединения или закрытии окна UI);
  - [service.rs:1838-2372](../../services/head/src/service.rs) (`send_message` выполняет ход прямо внутри будущего HTTP-хендлера под `HTTP_REQUEST_TIMEOUT = 180s` из [http.rs:44](../../services/head/src/http.rs));
  - [service.rs:2421-2430](../../services/head/src/service.rs) (`try_replay_client_message` возвращает `409 Conflict`, если `run.status == Running`);
  - [WorkspaceProvider.tsx:134](../../apps/desktop/src/state/WorkspaceProvider.tsx), [WorkspaceProvider.tsx:1546-1551](../../apps/desktop/src/state/WorkspaceProvider.tsx) (`cancelSessionRun` блокирует отмену, если `session_id` нет в локальном `runningSessionIdsRef`, который сбрасывается при перезапуске окна UI);
  - [head_d2_concurrency_integration.rs:1156-1217](../../services/head/tests/head_d2_concurrency_integration.rs) (`H2-01` Part C проверяет старое поведение отмены при `in_flight.abort()`).
- **Точное решение:**
  1. Заменить реестр `active_runs: Mutex<HashMap<SessionId, ActiveRunEntry>>` в `HeadService`: каждая запись `ActiveRunEntry` хранит `run_id: RunId`, `client_message_id: Option<String>`, `cancel_flag: Arc<AtomicBool>`, `abort_handle: AbortHandle` и `completion_rx: tokio::sync::watch::Receiver<Option<Result<SendMessageResponse, DennettError>>>`.
  2. После валидации входных параметров `send_message` запускает сам ход сессии внутри **отвязанного `tokio::spawn`**, принадлежащего `HeadService` (а не HTTP-запросу!). Внутри `tokio::spawn` живёт внутренний guard, который финализирует run только при панике воркера или остановке демона `HeadService`, но **никогда** при закрытии клиентского HTTP-сокета!
  3. HTTP-обработчик `POST /v1/sessions/{id}/messages` подписывается на `completion_rx.changed().await`. Если клиент закрыл окно Tauri или оборвал HTTP-запрос (или сработал клиентский таймаут), HTTP-обработчик просто выходит, а фоновый `tokio::spawn` в `dennett-head` штатно доводит ход до конца и коммитит ответ ассистента и артефакты в PostgreSQL (`V03`).
  4. **Идемпотентный join для `client_message_id`:** если в `send_message` приходит повторный `client_message_id`, и соответствующий `run_id` сейчас активен в `active_runs` (`status == Running`), вызов НЕ возвращает `409 Conflict`, а клонирует `completion_rx` этого запуска и дожидается его результата (а если запуск уже завершился в БД — возвращает сохранённый `SendMessageResponse` как сейчас). `409 Conflict` остаётся только для попытки отправить *другое* новое сообщение с иным `client_message_id` в занятую сессию.
  5. **Таймаут маршрута сообщений в `http.rs`:** вынести `/v1/sessions/{session_id}/messages` из общего `TimeoutLayer::new(Duration::from_secs(180))` либо поднять HTTP-таймаут этого маршрута выше `turn_timeout` с учётом пауз на подтверждение человека (`D2-BUG-03`), чтобы HTTP-слой не рвал живое ожидание интерактивного `AskBeforeWrite`.
  6. **Каноническое состояние запуска в UI (`Slice R2`):** `WorkspaceProvider` вычисляет `runningSessionIds` объединением локальных in-flight вызовов И серверного состояния (`runs` со статусом `running` в `activeDetail` / активных событий `run_started`..`run_completed` / `StateSnapshotResponse` — добавить `running_session_ids: Vec<SessionId>` с `#[serde(default)]` в `StateSnapshotResponse` из `SELECT DISTINCT session_id FROM runs WHERE status = 'running'`, чтобы UI мгновенно видел все активные сессии сразу после рестарта окна!). В `cancelSessionRun` убрать ранний выход `if (!runningSessionIdsRef.current.has(targetSessionId)) return;` — всегда отправлять `bridge.cancelRun(targetSessionId)` на сервер.
  7. **Адаптация теста `H2-01` Part C:** в [head_d2_concurrency_integration.rs:1156-1217](../../services/head/tests/head_d2_concurrency_integration.rs) проверить два свойства: (a) обрыв клиентского `JoinHandle` (`in_flight.abort()`) **не** убивает серверный запуск (`V03`), повторный вызов с тем же `client_message_id` присоединяется к нему, а явный `cancel_session_run` (или серверный `turn_timeout`) чисто освобождает слот сессии без зависших `running`-строк.

### 4.2. `D2-ARCH-02` (CRITICAL) — Полная семантика `archive_project` / `unarchive_project` (`90 §30` / Contract C) и `pause_reason`
- **Точные места дефекта:**
  - [lifecycle.rs:435-530](../../services/head/src/lifecycle.rs) (`archive_project` ставит на паузу только `pending`-таймеры, не отменяя активные запуски, не закрывая сессии во всех runtime-профилях и не паркуя активные задачи);
  - [lifecycle.rs:532-621](../../services/head/src/lifecycle.rs) (`unarchive_project` будит все `paused`-таймеры подряд, включая остановленные из-за `misfire_policy = 'ask'`);
  - [tasks.rs:3054-3070](../../services/head/src/tasks.rs) (`complete_task_run` ищет только `status = 'pending'`, и если проект заархивировали во время хода и таймер стал `paused`, периодическая задача ошибочно помечается `succeeded`).
- **Точное решение:**
  1. В `archive_project`:
     - Взять `SELECT ... FROM projects WHERE project_id = $1 FOR UPDATE`.
     - Пометить активные задачи проекта в `stopped_tasks`, отменить активные ходы сессий и закрыть все runtime-сессии проекта **в обоих профилях** (`self.runtime_router().cancel_turn_all(sid)` и `close_session_all(sid)`).
     - В транзакции: перевести `projects.lifecycle_state = 'archived'`; перевести `task_timers` со статусом `'pending'` в `status = 'paused', pause_reason = 'archived_project'`; перевести задачи в статусах `('queued', 'running', 'waiting_approval', 'waiting_clarification', 'waiting_signal')` (кроме `unknown_external`!) в терминальный/приостановленный статус `'cancelled'` с `error_code = 'project_archived'` (а периодические задачи с `paused`-таймером перевести в `'waiting_timer'` с `wait_detail = 'Проект в архиве — расписание приостановлено'`); отозвать открытые `permission_requests` (`expired`) и закрыть открытые `inbox_cards` (`withdrawn`), **строго исключая** `kind = 'unknown_effect'`.
  2. В `unarchive_project`:
     - Возобновлять (`status = 'pending', pause_reason = NULL`, с пересчётом `next_fire_at > now` для пропущенных окон или по `misfire_policy`) **только** те таймеры, у которых `status = 'paused' AND pause_reason = 'archived_project'`. Таймеры с `pause_reason = 'misfire_ask'` остаются на паузе до решения карточки `missed_schedule`.
  3. В `complete_task_run` ([tasks.rs:3054-3070](../../services/head/src/tasks.rs)):
     - Проверять таймер по `status IN ('pending', 'paused')`. Если у задачи есть повторяющийся таймер в статусе `'paused'`, переводить задачу в `TaskStatus::WaitingTimer` (с `wait_detail` по `pause_reason`), **никогда** не закрывая периодическую задачу в `TaskStatus::Succeeded`.

### 4.3. `D2-ARCH-03` (CRITICAL) — Честность `UNKNOWN` и обязательный `PolicyEngine` для внешних эффектов
- **Точные места дефекта:**
  - [effects.rs:397-550](../../services/head/src/effects.rs) (`dispatch_external_effect` не проверяет `PolicyEngine`, `project.lifecycle_state == Active` и принадлежность `session_id`/`task_id` к `project_id`);
  - [approvals.rs:1742-1774](../../services/head/src/approvals.rs) (`InboxResolutionAction::StopTask` разрешает закрыть карточку `InboxCardKind::UnknownEffect` в `resolved` без сверки!);
  - [lifecycle.rs:64-133](../../services/head/src/lifecycle.rs) (`terminate_project_work_in_tx` при `remove`/`delete` затирает `tasks.status = 'unknown_external'` в `'cancelled'`, отзывает карточки `unknown_effect` и бросает `effect_claims` в статусе `'dispatching'`);
  - [service.rs:3379-3401](../../services/head/src/service.rs) и [service.rs:2594](../../services/head/src/service.rs) (`finalize_turn` затирает `runs.status = 'unknown_external'` обратно в `outcome.status`, а `cancel_session_run` возвращает `RunStatus::Cancelled` вместо `RunStatus::UnknownExternal`);
  - [effects.rs:865-997](../../services/head/src/effects.rs) (`reconcile_external_effect` читает `effect_claims` без блокировки и обновляет без `AND status = 'unknown'`).
- **Точное решение:**
  1. В `dispatch_external_effect`:
     - Проверить `project.lifecycle_state == ProjectLifecycleState::Active` (отклонять `archived`, `deleted_record`, а для `detached` — если требуется активный проект).
     - Проверить, что `session.project_id == req.project_id`, а при наличии `req.task_id` — что `task.project_id == req.project_id && task.session_id == req.session_id`.
     - Прогнать операцию через `PolicyEngine` (`tool_name = format!("external_effect:{}", req.operation)`, `tool_kind = "external_effect"`, `target = Some(req.target_recipient.clone())`), записать `permission_decisions` в БД и при `Deny` (по умолчанию без явного `Allow`-правила или активного гранта!) отклонить вызов до создания/отправки `effect_claim` (`V05`).
  2. В `resolve_inbox_card` ([approvals.rs:1742](../../services/head/src/approvals.rs)): если `card.kind == InboxCardKind::UnknownEffect` и запрошен `InboxResolutionAction::StopTask`, возвращать `DennettError::Conflict` (*«Карточку неизвестного внешнего эффекта нельзя закрыть остановкой — выполните сверку ReconcileEffect»*).
  3. В `terminate_project_work_in_tx` ([lifecycle.rs:64-133](../../services/head/src/lifecycle.rs)):
     - Исключить `unknown_external` из `UPDATE tasks SET status = 'cancelled' ...` (`AND status <> 'unknown_external'`).
     - Исключить `unknown_effect` из `UPDATE inbox_cards SET status = 'withdrawn' ...` (`AND kind <> 'unknown_effect'`).
     - Перевести зависшие `effect_claims` со статусом `'dispatching'` проекта в `'unknown'` и гарантировать наличие открытой `inbox_cards (kind = 'unknown_effect')`.
  4. В `finalize_turn` ([service.rs:3379-3401](../../services/head/src/service.rs)): если в БД для `run_id` уже установлен `runs.status = 'unknown_external'` или существует `effect_claims` с `run_id = $1 AND status IN ('dispatching', 'unknown')`, сохранять итоговый статус запуска строго `RunStatus::UnknownExternal` (`WHERE run_id = $1`, `CASE WHEN status = 'unknown_external' THEN 'unknown_external' ELSE $2 END`), и в `cancel_session_run` возвращать `RunStatus::UnknownExternal`, когда `external_outcome_unknown == true`.
  5. В `reconcile_external_effect` ([effects.rs:865-997](../../services/head/src/effects.rs)): выполнять `SELECT ... FROM effect_claims WHERE effect_id = $1 FOR UPDATE` внутри транзакции сверки (либо проверять до внешнего HTTP probe, а при записи результата повторно брать `FOR UPDATE` и выполнять `UPDATE effect_claims SET ... WHERE effect_id = $1 AND status = 'unknown'`, возвращая `409 Conflict`, если параллельный запрос уже завершил сверку).

### 4.4. `D2-SEC-01` (CRITICAL) — Изоляция `allowed_memory_scopes`, fail-closed токены `memoryd` и ротация слотов `agy acp`
- **Точные места дефекта:**
  - [service.rs:3748-3794](../../services/head/src/service.rs) (`build_memory_mcp_specs`: при пустом `allowed_memory_scopes` или ошибке `mint_scoped_token` откатывается на статический мастер-токен `mcp.memory_token`!);
  - [tasks.rs:3435-3451](../../services/head/src/tasks.rs) (`send_agent_message` пропускает проверку пересечения скоупов, если `sender.current_config.allowed_memory_scopes.is_empty()`);
  - [antigravity.rs:129-154](../../adapters/antigravity-acp/src/antigravity.rs), [antigravity.rs:463-473](../../adapters/antigravity-acp/src/antigravity.rs) (`decide_slot_reuse` не учитывает изменение `mcp_servers` / скоупов памяти и истечение TTL токена `dnt1.`);
  - [service.rs:2655](../../services/head/src/service.rs), [service.rs:2899](../../services/head/src/service.rs) (`commit_memory_note` и `search_memory`).
- **Точное решение:**
  1. В `build_memory_mcp_specs`:
     - Если `agent.current_config.allowed_memory_scopes.is_empty()` -> возвращать `Ok(Vec::new())` (`dennett-memory` MCP вообще не подключается агенту без разрешённых скоупов!).
     - Если `mint_scoped_token(...)` вернул `Err` или `auth_secret` не настроен при непустых скоупах, только в dev-режиме без `memory_auth_secret` допустим явный dev-токен; если `memory_auth_secret.is_some()` и `mint_scoped_token` вернул ошибку — **fail-closed (`Err(DennettError::PermissionDenied / Internal)`)**, **никогда** не подставлять статический `mcp.memory_token`!
  2. В `send_agent_message` ([tasks.rs:3435-3451](../../services/head/src/tasks.rs)): если `sender_project != recipient_project` (или всегда для межагентной пересылки), требовать непустого пересечения `sender.allowed_memory_scopes` и `recipient.allowed_memory_scopes`; пустой `allowed_memory_scopes` у отправителя = **запрет (`PermissionDenied`)**, а не полный доступ!
  3. В `decide_slot_reuse` ([antigravity.rs:129-154](../../adapters/antigravity-acp/src/antigravity.rs)):
     - Хранить в `WarmSessionSlot` отпечаток `mcp_config_digest: u64` (детерминированный хэш имён серверов, бинарников, аргументов и списков скоупов, исключая случайный nonce токена, плюс `mcp_token_expires_at_ms: Option<i64>`).
     - Если изменился набор `allowed_memory_scopes` / список `mcp_servers` ИЛИ до истечения `mcp_token_expires_at_ms` осталось `< 10 минут` (`600_000 ms`), `decide_slot_reuse` возвращает `SlotReuseDecision::RespawnAndResume`, бесшовно перезапуская дочерний `agy acp` со свежим токеном и переоткрывая контекст!
  4. В `HeadService::commit_memory_note` и `search_memory`: если запрос привязан к `session_id` / `agent_id`, проверять через `PolicyEngine` и `agent.current_config.allowed_memory_scopes`, что целевой `scope` разрешён этому агенту и проект активен.

### 4.5. `D2-SEC-02` (CRITICAL) — Ужесточение `PolicyEngine` (`Allow`-правила, `%`-кодирование в `file://`, делегирование `A07`)
- **Точные места дефекта:**
  - [policy.rs:266-269](../../services/head/src/policy.rs), [policy.rs:398-400](../../services/head/src/policy.rs), [policy.rs:716-723](../../services/head/src/policy.rs) (`Allow`-правила используют `rule_matches_any` вместо `all`, позволяя обойти проверку второго пути при мульти-таргетной операции или пустом списке целей);
  - [policy.rs:858-875](../../services/head/src/policy.rs) (`normalize_candidate_path` снимает `file://` без декодирования/отбраковки `%2e`, `%2f`, `%5c`, `%00`);
  - [d2.rs:2732-2735](../../crates/dennett-contracts/src/d2.rs) и [tasks.rs:198-233](../../services/head/src/tasks.rs) (`write_policy_is_subset_of` разрешает переход `AskBeforeWrite -> AllowPaths`, позволяя дочернему агенту писать без спроса, а проектный `CapabilityGrant` может расширить права делегированной подзадачи шире ограничений родителя).
- **Точное решение:**
  1. Разделить семантику сопоставления целей в `policy.rs`:
     - Для `Deny`-правил оставить `rule_matches_any` (если хотя бы одна цель попала под запрет — весь вызов `Deny`).
     - Для `Allow`-правил ввести `rule_matches_all_for_allow(rule, tool_lower, targets)`: если у правила задан `target_pattern`, требовать `!targets.is_empty() && targets.iter().all(|t| glob_match(&pat, t))` (причём проверять только канонические пути `normalized_rel_targets` для файловых инструментов, без сырого `req.target`!).
  2. В `normalize_candidate_path` ([policy.rs:858-875](../../services/head/src/policy.rs)):
     - При разборе `file://` URI выполнять строгое percent-декодирование UTF-8; при некорректной `%`-последовательности, нулевом байте `%00` или декодированном слэше/точках обхода (`..`) возвращать `Err` (fail-closed!) до вызова `Path::components()`. В обычных путях (не `file://`) любые `%2e` трактуются буквально файловой системой, но `..` и симлинки отсекаются `resolve_within_workspace`.
  3. Для делегирования `A07`:
     - Удалить `(AskBeforeWrite, AllowPaths { .. }) => true` из `write_policy_is_subset_of` в [d2.rs](../../crates/dennett-contracts/src/d2.rs).
     - В `PolicyContext` добавить `parent_write_ceiling: Option<WorkspaceWritePolicy>` (загружается в `load_live_policy_inputs` по `task.parent_task_id -> parent_agent.workspace_write_policy`). При вычислении `PolicyEngine::evaluate_with_grants` сначала проверять потолок родителя (`DenyAll` или `AllowPaths`), чтобы ни один проектный `CapabilityGrant` не мог разрешить дочерней задаче запись за пределами `AllowPaths` родительского агента!

### 4.6. `D2-SEC-03` (HIGH) — Защита `/__dennett_dev_dirs` в Vite и loopback-защита в `dennett-memoryd`
- **Точные места дефекта:**
  - [vite.config.ts:13-24](../../apps/desktop/vite.config.ts) и [DevFolderPickerModal.tsx:28](../../apps/desktop/src/components/DevFolderPickerModal.tsx);
  - [services/memoryd/src/main.rs:421-455](../../services/memoryd/src/main.rs).
- **Точное решение:**
  1. В `apps/desktop/vite.config.ts` (`Slice R2`):
     - Проверять `req.method === "GET"`.
     - Проверять заголовок `Host` по строгому белому списку loopback (`127.0.0.1`, `localhost`, `[::1]` с опциональным `:port`); иначе `403`.
     - Требовать `req.headers["x-dennett-dev-picker"] === "1"`; иначе `403`.
     - Если присутствует `sec-fetch-site`, требовать строго `sec-fetch-site === "same-origin"` (отклонять `same-site`, `cross-site`, `none`); если присутствует `origin`, требовать loopback-origin.
     - В [DevFolderPickerModal.tsx:28](../../apps/desktop/src/components/DevFolderPickerModal.tsx) передавать `headers: { "X-Dennett-Dev-Picker": "1" }`.
  2. В `services/memoryd/src/main.rs` (`Slice R1`, **строго без импорта `dennett-head`**):
     - Сохранить `allow_non_loopback: bool` в `AppState`.
     - Добавить собственный middleware `enforce_loopback_request_origin` в Router `memoryd` (аналогично проверкам `Host` / `Origin` / `Sec-Fetch-Site` из `head/src/http.rs`), защищающий `dennett-memoryd` от DNS-rebinding и браузерных cross-origin запросов в стандартном loopback-режиме.

### 4.7. `D2-BUG-01` (HIGH) — Транзакционная сохранность ответов `reply_to_task` и сигналов `send_task_signal`
- **Точные места дефекта:** [tasks.rs:968-979](../../services/head/src/tasks.rs), [tasks.rs:1086-1093](../../services/head/src/tasks.rs), [tasks.rs:1612-1648](../../services/head/src/tasks.rs), [tasks.rs:2145-2158](../../services/head/src/tasks.rs).
- **Точное решение:**
  1. В `reply_to_task` (и `resolve_clarification_card` в `approvals.rs`) сохранять форматированный ответ владельца прямо в `tasks.pending_continuation_input` внутри `UPDATE tasks SET status = 'queued', pending_continuation_input = $..., ...` в PostgreSQL.
  2. В `send_task_signal`: строка сигнала уже пишется в `task_signals` с `consumed_in_generation = NULL` (или при пробуждении из `WaitingSignal` помечается поколением). Перестать полагаться на эфемерный `Option<String>` в памяти!
  3. В `execute_task_generation` при захвате задачи под `SELECT ... FROM tasks WHERE task_id = $1 FOR UPDATE`:
     - Атомарно забрать `tasks.pending_continuation_input` (обнулив колонку `SET pending_continuation_input = NULL` при старте поколения `status = 'running'`) И забрать все строки `UPDATE task_signals SET consumed_in_generation = $gen WHERE task_id = $1 AND (consumed_in_generation IS NULL OR consumed_in_generation = $gen) RETURNING signal_name, payload_text`.
     - Собрать из них единый детерминированный блок входного контекста для данного поколения `generation`. Если ход упал до доставки в модель или задача была вытеснена, данные остаются консистентными. Сигнатура `spawn_task_execution(task_id, expected_generation)` больше не нуждается в несохранённом `extra_input: Option<String>`.

### 4.8. `D2-BUG-02` (HIGH) — Унификация смены `workspace_path` и проверка активного `project.lifecycle_state` (закрытие TOCTOU)
- **Точные места дефекта:** [service.rs:1124-1225](../../services/head/src/service.rs), [lifecycle.rs:312](../../services/head/src/lifecycle.rs), [lifecycle.rs:419](../../services/head/src/lifecycle.rs), [approvals.rs:378-392](../../services/head/src/approvals.rs), мутации в `tasks.rs` и `approvals.rs`.
- **Точное решение:**
  1. В `update_project` при передаче нового `workspace_path`: использовать ту же логику привязки/перепривязки, что и в `rebind_project` — валидировать канонический каталог, переводить `detached -> active` (отклоняя `archived` и `deleted_record` с `409 Conflict`!) и закрывать живые сессии проекта через `self.runtime_router().close_session_all(sid)`, чтобы тёплый слот `agy acp` пересоздался с новым `cwd`.
  2. В `rebind_project` и `detach_project_workspace` заменить `self.runtime().close_session(sid)` на `self.runtime_router().close_session_all(sid)` (охватывая и `oauth-personal`, и `gemini-api-key`).
  3. Добавить единый хелпер `ensure_project_active_in_tx(&mut tx, project_id)` и вызывать его во всех мутациях задач, карточек и разрешений (`create_task`, `update_task`, `send_task_signal`, `reply_to_task`, `resolve_missed_schedule_card`, `resolve_permission_request`, `increase_task_budget_and_resume`, `resolve_inbox_card` — кроме `reconcile_effect` на `unknown_effect`, который обязан работать даже после `archive`/`remove`/`delete` по `D2-ARCH-03`!).
  4. В `load_live_policy_inputs` ([approvals.rs:378-392](../../services/head/src/approvals.rs)) загружать `projects.lifecycle_state` и, если проект перешёл в `archived` или `deleted_record` прямо посреди выполнения хода (mid-turn TOCTOU), немедленно отклонять любой инструмент (`PolicyEffect::Deny`, причина `project is no longer active`), а для файловых инструментов при `detached` или `workspace_path IS NULL` также возвращать `Deny`.

### 4.9. `D2-BUG-03` (HIGH) — Исключение времени ожидания ответа человека (`AskBeforeWrite`) из таймаута хода и `wall_ms_used`
- **Точные места дефекта:** [transport.rs:658](../../adapters/antigravity-acp/src/transport.rs), [transport.rs:908-970](../../adapters/antigravity-acp/src/transport.rs) и [tasks.rs:2482](../../services/head/src/tasks.rs), [tasks.rs:2579](../../services/head/src/tasks.rs), [tasks.rs:2921](../../services/head/src/tasks.rs).
- **Точное решение:**
  1. В `adapters/antigravity-acp/src/transport.rs` (`exchange_prompt_turn_io`): перед вызовом `args.policy.evaluate(eval_req).await` замерять `let approval_wait_started = tokio::time::Instant::now();`, а сразу после возврата прибавлять `deadline += approval_wait_started.elapsed();`! Кроме того, вернуть суммарное время ожидания `human_wait_ms: u64` в `PromptTurnOutput`.
  2. В `execute_session_turn_core` / `tasks.rs`: вычитать `turn_output.human_wait_ms` из `started_instant.elapsed().as_millis()` при начислении `wall_ms_used` бюджета задачи (`task.usage.wall_ms_used`), чтобы раздумья владельца над карточкой во `Входящих` никогда не сжигали машинный лимит `max_wall_ms` поручения и не роняли RPC-таймаут хода!

### 4.10. `D2-BUG-04` (HIGH) — Защита поколений в `complete_task_run_error`, возврат недоставленных `agent_messages`, ревизии `inbox_cards` и полнота `control_events`
- **Точные места дефекта:** [tasks.rs:3166-3203](../../services/head/src/tasks.rs), [service.rs:2040-2073](../../services/head/src/service.rs), [tasks.rs:2298-2390](../../services/head/src/tasks.rs), [approvals.rs:1247-1258](../../services/head/src/approvals.rs), [approvals.rs:1581-1627](../../services/head/src/approvals.rs).
- **Точное решение:**
  1. В `complete_task_run_error`: открывать транзакцию, брать `SELECT ... FROM tasks WHERE task_id = $1 FOR UPDATE`, проверять `task.generation == generation && !task.status.is_terminal() && task.status != TaskStatus::UnknownExternal`, и обновлять с `WHERE task_id = $1 AND generation = $5`. Если основная `complete_task_run` вернула ошибку транзакции PostgreSQL, вызывать аварийный fallback `park_task_on_completion_error`, чтобы задача не зависла вечно в `running`.
  2. **`agent_messages`:**
     - При фильтрации входящих сообщений агента в обычном чате сессии (`task_id.is_none()`) забирать только сообщения с `related_task_id IS NULL`; при выполнении задачи (`Some(tid)`) забирать `related_task_id IS NULL OR related_task_id = $tid`.
     - Если ход завершился с ошибкой до ответа модели, был брошен (`finalize_abandoned_run`) или восстановлен при старте демона (`recover_interrupted_runs`), сбрасывать `UPDATE agent_messages SET delivered_in_run_id = NULL, delivered_at = NULL WHERE delivered_in_run_id = $run_id`, чтобы сообщения между агентами не пропадали бесследно.
  3. **Ревизии `inbox_cards`:** в `resolve_inbox_card` и `resolve_permission_request` передавать `expected_revision: Option<i64>` внутрь транзакции, блокировать карточку `SELECT ... FROM inbox_cards WHERE card_id = $1 FOR UPDATE`, сверять `card.revision == expected_rev` под блокировкой и выполнять `UPDATE inbox_cards SET ..., revision = revision + 1 WHERE card_id = $1 AND status = 'open' AND revision = $expected`, возвращая `409 Conflict` при гонке.
  4. **Полнота журнала `control_events` (`V15`):** добавить публикацию `append_control_event_in_tx` в `resolve_missed_schedule_card` (`inbox_card_resolved` + `task_updated`), в старт `send_message` (`run_started` сразу после коммита строки `runs`), в `update_session` (`session_updated`) и в cancel-ветку `complete_task_run` (`task_cancelled`).

### 4.11. `D2-BUG-05` (MEDIUM) — Валидация `OnceAt`, маппинг SQLSTATE, проверка `increase_task_budget_and_resume`, `dennettctl`, очистка карт и `AcpProcess::drop`
- **Точные решения:**
  1. [d2.rs:452-454](../../crates/dennett-contracts/src/d2.rs): вызывать `fire_at.validate()?` для `ScheduleKind::OnceAt { fire_at }`.
  2. [store.rs:54-61](../../services/head/src/store.rs): в `map_pg_err` распознавать `err.as_db_error()` с SQLSTATE класса `22xxx` (`DataException`, например выход года за диапазон `TIMESTAMPTZ`) и `23514` (`CheckViolation`) и маппить их в `DennettError::InvalidInput` (`400 Bad Request`), а `23505` (`UniqueViolation`) — в `DennettError::Conflict` (`409`), оставляя `503 DatabaseUnavailable` только для реальных сетевых/пуловых/серверных отказов БД.
  3. [approvals.rs:1813-1947](../../services/head/src/approvals.rs): в `increase_task_budget_and_resume` после прибавления `additional_turns` / `additional_tokens` / `additional_wall_secs` вызывать `if let Some(still_exceeded) = next_budget.check_before_next_turn(&task.usage)` -> возвращать `DennettError::InvalidInput` с понятным указанием, какое именно измерение лимита (`max_turns`, `max_tokens` или `max_wall_ms`) всё ещё исчерпано, не закрывая карточку `budget_exceeded` вхолостую.
  4. [dennettctl.rs:1618-1670](../../crates/dennett-head-client/src/bin/dennettctl.rs): добавить в `dennettctl task update` флаги `--max-turns`, `--max-tokens`, `--max-wall-secs`, `--exceed-policy <pause_and_ask|stop_task>` и синхронизировать встроенную справку `--help` со всеми поддерживаемыми подкомандами (`weekly` расписание, флаги бюджетов и т.д.).
  5. [service.rs:347-350](../../services/head/src/service.rs): очищать записи из `stopped_tasks` после терминальной финализации задачи (или ограничивать LRU/размером с TTL) и удалять неиспользуемые `Arc::strong_count(&mutex) == 1` ключи из `session_locks` при завершении хода.
  6. [transport.rs:546-559](../../adapters/antigravity-acp/src/transport.rs): убрать блокирующий цикл `for _ in 0..10 { std::thread::sleep(Duration::from_millis(20)); }` из `AcpProcess::drop`! В `Drop` выполнять мгновенный неблокирующий `try_wait()` -> `kill()` и при необходимости передавать `Child` в `tokio::task::spawn_blocking(move || { let _ = child.wait(); })` (если есть активный Tokio runtime), никогда не блокируя рабочие потоки реактора.

### 4.12. `D2-OPS-01` (MEDIUM) — Надёжная остановка демонов в `ops/runner`, фоновый дренаж пайпов и расширение `.github/workflows/ci.yml`
- **Точные места дефекта:** [cli.py:1200-1235](../../ops/runner/src/dennett_runner/cli.py), [ci.yml](../../.github/workflows/ci.yml).
- **Точное решение:**
  1. В `ops/runner/src/dennett_runner/cli.py`:
     - При запуске дочернего демона (`dennett-head`, `dennett-memoryd`) запускать лёгкие фоновые daemon-потоки Python (`threading.Thread(..., daemon=True)`), читающие `proc.stdout` и `proc.stderr` в кольцевой буфер `collections.deque(maxlen=200)` и сигнализирующие `threading.Event`, как только встречена строка `listening on <addr>`. Это даёт честный таймаут ожидания старта даже при зависшем без вывода процессе и гарантирует, что заполнение 64-КБ буфера ОС на Windows никогда не заблокирует демон посреди длинного прогона!
     - В `_graceful_stop_daemon(proc, base_url=...)`: сначала отправлять HTTP `POST {base_url}/shutdown` с заголовком `X-Dennett-Shutdown: 1` (короткий таймаут `1.5s`) и ждать `proc.wait(timeout=4)`; если процесс ещё жив — вызывать `proc.terminate()`, а на `os.name == "nt"` при невыходе за `3s` выполнять `subprocess.run(["taskkill", "/F", "/T", "/PID", str(proc.pid)], check=False, capture_output=True)` для гарантированной зачистки дерева процессов (включая дочерние `agy acp`).
  2. В `.github/workflows/ci.yml`:
     - В job `desktop-frontend` (или отдельный шаг сборки Tauri) добавить `cargo check -p dennett-desktop --features custom-protocol` (после `pnpm --dir apps/desktop build`, когда `apps/desktop/dist` собран!), чтобы production-сборка Tauri проверялась на каждом пуше.
     - Добавить job `windows-postgres-integration` (`runs-on: windows-latest`): поднять встроенный на раннере GitHub Windows сервис PostgreSQL (`postgresql-x64-16` / `pg_ctl` или `choco`/предустановленный PostgreSQL на `windows-latest`), создать базы `dennett_control_test` и `dennett_memory_test`, выставить `DENNETT_TEST_CONTROL_PG_URL` / `DENNETT_TEST_MEMORY_PG_URL` и прогнать `uv run --project ops/runner dennett-check integration` и `dennett-demo-d2` на реальной Windows!

### 4.13. `D2-OVER-01` (HIGH) — Единый движок `ordered_outbox` вместо дублирующего `memory_commit_outbox`
- **Точные места:** [service.rs:622-632](../../services/head/src/service.rs), [service.rs:2655-3183](../../services/head/src/service.rs), [events.rs:321-512](../../services/head/src/events.rs), [tasks.rs:1443-1474](../../services/head/src/tasks.rs).
- **Точное решение:**
  1. В `commit_memory_note`: ставить запись в `ordered_outbox` через `enqueue_ordered_outbox_in_tx(&mut tx, "memoryd", req.scope.as_str(), &format!("mem-commit:{}", req.idempotency_key), &payload_json, now)` (строго сохраняя порядок по `ordering_key = scope`!).
  2. Сохранить 100% обратную совместимость контрактов HTTP/CLI/UI:
     - `GET /v1/memory/outbox` (`list_memory_outbox`), `GET /v1/diagnostics` (`outbox_items`) и `HeadHealth.pending_outbox_count` читают строки `ordered_outbox WHERE destination = 'memoryd'` и маппят их в `MemoryOutboxItem`.
     - `POST /v1/memory/outbox/flush` (`flush_memory_outbox`) дренирует `ordered_outbox WHERE destination = 'memoryd'` по очереди `seq_in_key` для каждого `ordering_key` (не нарушая FIFO внутри одного `scope` при временной недоступности `memoryd`).
  3. **Автоматический фоновый дренаж в `HeadScheduler`:** в цикл `spawn_background_scheduler` ([tasks.rs:1443](../../services/head/src/tasks.rs)) добавить периодический (раз в `2–5 с`, если есть `pending` записи для `destination = 'memoryd'`) неблокирующий вызов `flush_memory_outbox()`, чтобы отложенные при падении `memoryd` заметки доставлялись автоматически после его подъёма без ручного нажатия кнопки в настройках!

### 4.14. `D2-OVER-02` (HIGH) — Единое ядро выполнения хода `execute_session_turn_core` и компактный `RuntimeRouter`
- **Точные места:** [service.rs:334-345](../../services/head/src/service.rs), [service.rs:1900-2372](../../services/head/src/service.rs), [tasks.rs:2190-2683](../../services/head/src/tasks.rs).
- **Точное решение:**
  1. Выделить структуру `RuntimeRouter { interactive: Arc<dyn AgentRuntime>, background: Option<Arc<dyn AgentRuntime>> }` с методами:
     - `for_profile(&self, profile: RuntimeAuthProfile) -> Result<&Arc<dyn AgentRuntime>, DennettError>` (строго без тихого отката с `GeminiApiKey` на `OauthPersonal` — `A09`!),
     - `cancel_turn_all(&self, session_id: SessionId) -> Vec<Result<(), DennettError>>`,
     - `close_session_all(&self, session_id: SessionId)`.
  2. Выделить единый внутренний метод `HeadService::execute_session_turn_core(&self, params: TurnExecutionParams) -> Result<TurnExecutionResult, DennettError>` с типизированным `enum TurnTrigger { UserMessage { client_message_id: Option<String> }, TaskGeneration { task_id: TaskId, generation: i32, profile: RuntimeAuthProfile } }`:
     - Единая сборка `mcp_servers` (`dennett-memory` + новый `dennett-control`).
     - Единое форматирование входящих `agent_messages` (`[Inter-agent message from {sender_label}]: ...`).
     - Единая обработка событий потока (`PartialArtifactEmitted` сохраняет MIME по расширению/подсказке с дефолтом `text/markdown`, а не разнобой `text/plain` vs `text/markdown`).
     - Единая запись `audit_events` (включая `tool_observed` как для обычных чатов, так и для фоновых задач!).
     - Учёт `human_wait_ms` (`D2-BUG-03`) и сохранение честного `RunStatus::UnknownExternal` (`D2-ARCH-03`).

### 4.15. `D2-OVER-03` + `D2-BUG-06` (HIGH) — Надёжная синхронизация событий и офлайн-очереди на фронтенде (`Slice R2`)
- **Точные места:** [WorkspaceProvider.tsx:517-573](../../apps/desktop/src/state/WorkspaceProvider.tsx), [WorkspaceProvider.tsx:645-734](../../apps/desktop/src/state/WorkspaceProvider.tsx), [WorkspaceProvider.tsx:760-801](../../apps/desktop/src/state/WorkspaceProvider.tsx), [WorkspaceProvider.tsx:1426-1484](../../apps/desktop/src/state/WorkspaceProvider.tsx), [effects.rs:813-825](../../services/head/src/effects.rs).
- **Точное решение:**
  1. На бэкенде (`Slice R1`, [effects.rs:813-825](../../services/head/src/effects.rs)): добавить разумный верхний предел `LIMIT 250` (`ORDER BY created_at DESC LIMIT $...`) в `list_effects` и выборку эффектов внутри `get_state_snapshot` (всегда включая все открытые `status IN ('dispatching', 'unknown')` плюс последние завершённые), чтобы снимок состояния не рос бесконечно.
  2. В `WorkspaceProvider.tsx` (`Slice R2`):
     - **Single-flight + `snapshotSeqRef`:** если `applySnapshotCollections` уже летит без принудительного нового параметра, коалесцировать параллельные вызовы в один промис; перед применением ответа проверять `mySeq === snapshotSeqRef.current`, чтобы старый медленный ответ никогда не перетёр более свежий.
     - **Курсор потока строго ПОСЛЕ коммита состояния:** в `pollEventsNow` ([WorkspaceProvider.tsx:665](../../apps/desktop/src/state/WorkspaceProvider.tsx)) сдвигать `streamCursorRef.current = res.next_cursor` **только после** успешного завершения `applySnapshotCollections` / `loadSessionDetail`! Если подгрузка деталей упала по сети, следующий тик повторит чтение с прежнего курсора без потери событий.
     - **Спокойный адаптивный пуллинг:** в простое (нет активных `runningSessionIds` и нет `running`/`queued` задач) опрашивать `/v1/events` раз в `2500 ms` (вместо `1200 ms`), а при активном ходе — раз в `1000 ms`; при `document.visibilityState === "hidden"` замедлять до `5000 ms`.
     - **Исправление холодного реконнекта:** в [WorkspaceProvider.tsx:787](../../apps/desktop/src/state/WorkspaceProvider.tsx) заменить `if (!wasConnected || sessions.length === 0)` на отслеживание `wasConnectedRef.current`: вызывать полный `loadConnectedState()` ровно при переходе `offline -> online` (включая холодный старт, когда при первом запуске `Head` был офлайн и `sessions.length === 0`!).
     - **Безопасная офлайн-очередь с явным подтверждением владельца (`D2-BUG-06`):** удалить слепой `useEffect` авто-доставки при коннекте ([WorkspaceProvider.tsx:1477-1484](../../apps/desktop/src/state/WorkspaceProvider.tsx))! Когда соединение восстановлено И `loadConnectedState()` полностью завершил загрузку актуальной истории сессии, показывать понятную плашку/кнопку на карточке отложенного сообщения (`«Связь восстановлена — Отправить сейчас»` / `«Отменить»`), чтобы владелец видел актуальное состояние чата перед отправкой старого офлайн-сообщения. В `flushQueuedMessages` при получении терминальных ошибок `404 NotFound` или `409 Conflict` (проект в архиве/удалён) удалять элемент из очереди с показом причины в `errorBanner`, не зацикливая очередь бесконечно.

---

## 5. Агентно-нативный MCP-адаптер событий и расписаний (`adapters/control-mcp`)

Вместо ручного заполнения расписаний в UI агент сам создаёт, просматривает, обновляет и отменяет события проекта по естественной просьбе владельца в чате (например: *«Создай событие: мне необходимо каждое утро в 09:30 проверять упавшие тесты и писать короткую сводку»* или *«Поставь напоминание через 2 часа проверить деплой»*).

### 5.1. Архитектура и крейт `adapters/control-mcp` (`dennett-control-mcp`)

- **Расположение:** `adapters/control-mcp/Cargo.toml` + `adapters/control-mcp/src/main.rs` (бинарник `dennett-control-mcp`), зеркально [adapters/memory-mcp/src/main.rs](../../adapters/memory-mcp/src/main.rs).
- **Зависимости (тонкий клиент!):** ТОЛЬКО `dennett-contracts`, `dennett-head-client`, `serde`, `serde_json`, `tokio`. Никаких зависимостей от `dennett-head`, `tokio-postgres` или `dennett-memory-*`. В [ops/runner/src/dennett_runner/cli.py](../../ops/runner/src/dennett_runner/cli.py) добавить `"dennett-control-mcp"` в `FORBIDDEN_DESKTOP_EXACT_DEPS` и собирать `--bin dennett-control-mcp` рядом с `dennett-memory-mcp` в `check_integration`, `check_runtime_probe`, `run_demo_d1`, `run_demo_d2`.
- **Самообнаружение адреса `dennett-head`:**
  - В `services/head/src/main.rs` сразу после `TcpListener::bind(opts.addr)` и получения реального `bound_addr` (включая порт при `127.0.0.1:0`) вызывать `service.set_self_http_url(format!("http://{bound_addr}"))`.
  - Добавить CLI-флаг `--control-mcp-bin` / env `DENNETT_CONTROL_MCP_BIN` + автопоиск соседнего бинарника `find_sibling_control_mcp_bin()` по аналогии с `find_sibling_memory_mcp_bin()` ([main.rs:640](../../services/head/src/main.rs)). Если `self_http_url` не задан (например, чисто локальный unit-тест `HeadService` без HTTP-сокета), `build_control_mcp_spec` просто пропускает инжекцию `dennett-control`, не ломая существующие in-process тесты.
- **Контекст и криптографическая привязка к проекту (`Zero Cross-Project Escalation`):**
  - При сборке `McpServerSpec` (`name = "dennett-control"`) `HeadService` передаёт в env дочернего `dennett-control-mcp`:
    - `DENNETT_HEAD_URL` = `<self_http_url>`
    - `DENNETT_CONTROL_PROJECT_ID` = `<session.project_id>`
    - `DENNETT_CONTROL_AGENT_ID` = `<session.agent_id>`
    - `DENNETT_CONTROL_SESSION_ID` = `<session.session_id>`
    - `DENNETT_CONTROL_TOKEN` = короткоживущий HMAC-SHA256 токен `dct1.<payload_b64>.<sig_b64>` (TTL `2h`, включающий `project_id`, `agent_id`, `session_id`, `session_kind`, `exp`; учитывается в `decide_slot_reuse` вместе с `dnt1.`).
  - `HeadClient` получает метод `.with_control_token(token)` (добавляет заголовок `X-Dennett-Control-Token`). На стороне `dennett-head` (`http.rs`) если заголовок `X-Dennett-Control-Token` присутствует, middleware/хендлер проверяет HMAC-подпись, срок действия и жёстко сверяет `claims.project_id == target_project_id` (и блокирует создание новых расписаний из фоновых `session_kind == TaskRun` для защиты от бесконечной саморекурсии фоновых задач!).

### 5.2. Ровно 4 чистых MCP-инструмента (JSON-RPC 2.0 over stdio, `Content-Length` + NDJSON framing)

Никаких собственных графов/DAG/ветвлений — тонкая проекция поверх контрактов `TaskRecord` / `ScheduleSpec`:

1. **`dennett_event_create`:**
   - Входные поля JSON Schema:
     - `title: string` (краткое понятное название события для списка `Поручения`, до 120 символов)
     - `goal: string` (полная инструкция, что агент должен сделать при наступлении события)
     - `trigger_kind: "once_at" | "interval" | "daily_local_time" | "weekly_local_time" | "wait_for_signal" | "immediate"`
     - `fire_at_iso_or_ms?: string | number` (для `once_at`: принимает либо epoch ms, либо ISO-8601 строку `YYYY-MM-DDTHH:MM:SSZ` / с числовым смещением `+05:00`, конвертируемую чистым парсером в UTC `TimestampMs`)
     - `interval_secs?: number` (для `interval`, `>= 10`)
     - `hour?: number`, `minute?: number` (для `daily_local_time` и `weekly_local_time`)
     - `weekdays?: Array<"mon" | "tue" | "wed" | "thu" | "fri" | "sat" | "sun">` (для `weekly_local_time` -> конвертируется в `weekdays_mask: u8`)
     - `timezone?: string` (IANA часовой пояс, например `"Asia/Yekaterinburg"` или `"Europe/Moscow"`; если не указан — системный IANA часовой пояс хоста / `"UTC"`)
     - `misfire_policy?: "catch_up_once" | "skip" | "ask"` (по умолчанию `"catch_up_once"`)
     - `expected_signal_name?: string` (для `wait_for_signal`)
   - Поведение адаптера:
     1. Создаёт выделенную чистую сессию `HeadClient::create_session` в `DENNETT_CONTROL_PROJECT_ID` с тем же `agent_id`, `title = req.title` и **`session_kind = SessionKind::TaskRun`** (чтобы фоновые срабатывания не засоряли список чатов пользователя и никогда не блокировали текущий интерактивный диалог!).
     2. Вызывает `HeadClient::create_task` с соответствующим `ScheduleSpec` / `expected_signal_name` (для фоновых расписаний задавая разумный профиль и дефолтный `TaskBudget`).
     3. Возвращает структурированный JSON-ответ `{ task_id, session_id, title, status, next_fire_at, schedule_summary }` для подтверждения пользователю в чате.
2. **`dennett_event_list`:**
   - Входные поля: `status?: string` (опциональный фильтр), `include_completed?: boolean` (по умолчанию `false`).
   - Вызывает `HeadClient::list_tasks(Some(env_project_id), ...)` и возвращает список событий текущего проекта с `task_id`, `title`, `goal`, `status`, `wait_detail`, `generation`.
3. **`dennett_event_update`:**
   - Входные поля: `task_id: string`, `title?: string`, `goal?: string`, `max_turns?: number`, `max_tokens?: number`, `max_wall_secs?: number`.
   - Проверяет принадлежность задачи к `env_project_id` и вызывает `HeadClient::update_task`.
4. **`dennett_event_cancel`:**
   - Входные поля: `task_id: string`.
   - Проверяет принадлежность задачи к `env_project_id` и вызывает `HeadClient::cancel_task`.

### 5.3. Правила `PolicyEngine` и поддержка в `SimulatedRuntime`

- В `services/head/src/policy.rs` добавить шаг `4b` (`canonical_control_mcp_op`):
  - Инструменты `dennett_event_create`, `dennett_event_list`, `dennett_event_update`, `dennett_event_cancel` (`mcp:dennett-control`) подчиняются явным `Deny`-правилам на шаге 2.
  - По умолчанию разрешены (`Allow`) для активного проекта, если для `dennett_event_create` текущий контекст **не** является фоновой задачей (`self.ctx.task_id.is_none()`, защита от бесконтрольного порождения задач фоновыми запусками без явного `Allow`-правила). Если в аргументах передан чужой `project_id` — немедленный `Deny`.
- В `adapters/antigravity-acp/src/simulated.rs` добавить детерминированную директиву `[SIM:EVENT:CREATE title="..." interval_secs=60 goal="..."]`, которая в интеграционных и UI-тестах без платного/живого LLM реально запускает дочерний бинарник `dennett-control-mcp` по stdio (точно так же, как `SimulatedRuntime` тестирует `dennett-memory-mcp`), проверяя сквозной путь `Chat -> Runtime -> PolicyEngine -> dennett-control-mcp -> HeadClient -> PostgreSQL -> UI Поручения`!

---

## 6. Спецификация доработки пользовательского интерфейса (`Slice R2`)

> [!IMPORTANT]
> Жёсткие инварианты вёрстки и безопасности (`dennett-check arch` + тесты):
> - **`0` тегов `<select>`** во всём фронтенде;
> - **`0` свойств `backdrop-filter` / `-webkit-backdrop-filter`** в CSS;
> - **`0` вызовов `dangerouslySetInnerHTML` / `.innerHTML`**;
> - **`0` dev-путей (`/__dennett_dev_dirs`) в собранном `apps/desktop/dist`**.

### 6.1. Продуктовая директива 1 — Создание событий через чат вместо ручной формы в `TasksCard`
- В [TasksCard.tsx](../../apps/desktop/src/components/TasksCard.tsx):
  - **Полностью удалить** блок ручной формы `.d2-task-composer-shell` (строки `366–597`) и все локальные стейты `goal`, `title`, `mode`, `intervalSecs`, `delayMins`, `dailyTime`, `signalName`, `misfirePolicy`, `maxTurns`.
  - В правой части шапки `.d2-card-header__controls` разместить аккуратную квадратную кнопку-иконку `+` (`28x28`, `.icon-button`, иконка `<Plus size={16} weight="bold" />`, `title="Создать событие"`, `aria-label="Создать событие"`, `data-testid="task-create-plus-btn"`).
  - Также в пустом состоянии списка (`activeTasks.length === 0`) добавить вторичную кнопку `+ Создать событие в чате`, вызывающую то же действие.
  - В `WorkspaceProvider.tsx` добавить действие `createBlankChatWithDraft(projectId?: string, initialDraft?: string)`:
    1. Определяет целевой проект: выбранный проект фильтра (если в `Поручениях` выбран конкретный проект) -> `meta.activeProject` -> первый активный проект (а если активных проектов вообще нет — создаёт проект без папки `Новый проект`, чтобы кнопка `+` работала даже с нуля!).
    2. Создаёт новую сессию `session_kind = "user_chat"` через `bridge.createSession`.
    3. Записывает в `draftsBySession[created.session_id]` строку `"Создай событие: мне необходимо "`.
    4. Переключает `selectedSessionId = created.session_id`, `activeView = "chat"`, и в `requestAnimationFrame` ставит фокус на `composerInputRef.current` с установкой каретки в самый конец текста (`el.setSelectionRange(len, len)`).

### 6.2. Продуктовая директива 2 — Чистая геометрия `Входящих` и `Поручений` + полный селектор проектов
- **Автоматическое скрытие колонки `Проекты и чаты` вне режима `Чаты`:**
  - В `App.tsx` / `WorkspaceProvider.tsx` вычислить `const isSidebarVisible = state.isSidebarOpen && state.activeView === "chat";`.
  - Класс `.workbench.sidebar-collapsed` применяется, когда `!isSidebarVisible` (`grid-template-columns: 52px 0px minmax(0, 1fr)`). Пользовательская настройка `state.isSidebarOpen` при этом сохраняется: возврат на вкладку `Чаты` мгновенно возвращает боковую панель в прежнее состояние. Если пользователь нажал кнопку `SidebarSimple` в `TitleBar` или `Ctrl+B`, находясь во `Входящих` / `Поручениях`, приложение переключается в `activeView = "chat"` с открытой панелью `isSidebarOpen = true`.
  - Проверить нижнюю левую панель `.user-settings-bar` при `.workbench.sidebar-collapsed` ([desktop.css:837-845](../../apps/desktop/src/styles/desktop.css)): иконка шестерёнки настроек (30x30) остаётся идеально центрированной в `52px` колонке `1` без налезания на центральную карточку.
- **Полноценный селектор фильтра по проектам в `InboxRadarCard` и `TasksCard`:**
  - Заменить двухкнопочный переключатель `Все проекты | {activeProject}` на единый переиспользуемый компонент `ProjectScopeFilter` на базе порталированного `.floating-popover` (`data-testid="project-scope-filter-trigger"` / `data-testid="project-scope-filter-menu"`):
    - Пункт 1: `Все проекты` (по умолчанию выбран, со счётчиком элементов).
    - Разделитель `.popover-divider`.
    - Полный список всех неархивных и неудалённых проектов (`state.projects.filter(p => lc !== "archived" && lc !== "deleted_record")`) с иконкой папки, названием проекта и галочкой `<Check />` у выбранного `selectedProjectId`.
    - Если выбранный в фильтре проект был заархивирован или удалён, фильтр автоматически сбрасывается на `"all"`.

### 6.3. Продуктовая директива 3 + фильтрация фоновых сессий — Чистый `ActivityRail` и `Sidebar`
- В [ActivityRail.tsx:104-124](../../apps/desktop/src/components/ActivityRail.tsx): **удалить** кнопку `Workspace` (`<Browsers />`) и отключённую кнопку `<Plug disabled />`. Управление правой панелью `Workspace` остаётся в единственном каноническом месте — правом углу `TitleBar` (`Ctrl+\`) и в `CommandPalette`.
- В [Sidebar.tsx](../../apps/desktop/src/components/Sidebar.tsx) и [CommandPalette.tsx](../../apps/desktop/src/components/CommandPalette.tsx):
  - Основной список чатов проекта отображает сессии `(s.session_kind ?? "user_chat") === "user_chat"` (плюс текущую `state.selectedSessionId`, если владелец явно перешёл в `task_run`-сессию по кнопке «Открыть чат», с аккуратным бейджем `Фон` рядом с заголовком, чтобы было видно, где он находится). Фоновые прогоны расписаний больше никогда не захламляют дерево чатов!
- **Режим чтения / ответа в `Composer.tsx` для `task_run`-сессий (`D2-UX-06`):**
  - Если открыта сессия `session_kind === "task_run"` (или в сессии есть открытая карточка `clarification` привязанной задачи):
    - Когда связанная задача находится в статусе `waiting_clarification`: в `MessageTimeline` **не рендерится** дублирующий инлайн `<input>` внутри карточки уточнения (карточка показывает только вопрос агента и кнопку «Остановить поручение»), а основной нижний `Composer` переключает плейсхолдер на `«Ответьте на уточняющий вопрос агента…»` и отправляет введённый текст напрямую в `actions.replyToTask(task.task_id, text)`! Таким образом на экране всегда ровно **одно** поле ввода.
    - Когда `task_run`-сессия НЕ ждёт уточнения (`waiting_timer`, `succeeded`, `running` и т.д.): вместо обычной отправки хода нижняя панель показывает компактную плашку `«Это журнал фонового события — управление расписанием в разделе Поручения»` с кнопкой перехода к поручению (и кнопкой `Stop`, если запуск сейчас идёт).

### 6.4. Закрытие всех визуальных и интерактивных замечаний `D2-UX-01`..`D2-UX-06`

1. **`D2-UX-01` (Mica, плотность правой панели ~50% и адаптивность `960x620`):**
   - В [tokens.css:52-58](../../apps/desktop/src/styles/tokens.css) добавить отдельную переменную `--workspace-card-bg`:
     - В `:root` и `html.browser-mica-preview`: `--workspace-card-bg: rgba(34, 34, 37, 0.78);`
     - В `html.native-shell`: `--chat-card-bg: rgba(24, 24, 24, 0.36);`, `--workspace-card-bg: rgba(36, 36, 38, 0.50);` (ровно 50% плотность правой карточки поверх системной Mica по `WP-M01-003`!), `--raised-bg: rgba(45, 45, 45, 0.34);`. В `.workspace-panel` заменить `background: var(--raised-bg)` на `background: var(--workspace-card-bg); border: 1px solid rgba(255, 255, 255, 0.09);`.
   - В `apps/desktop/src-tauri/src/lib.rs` (`.setup(|app| ...)`): проверить версию ОС / результат применения эффекта окна; если ОС не поддерживает Windows 11 Mica (например, Windows 10 < 22000 или Linux), выставить на `WebviewWindow` непрозрачный тёмный фон или инжектировать класс `html.native-shell-solid-fallback` (`--shell-backdrop: #111215`), чтобы прозрачное окно никогда не рендерилось чёрной дырой.
   - **Узкое окно `960x620` (`< 1100px`):** в `WorkspaceProvider.tsx` добавить слушатель `window.matchMedia("(max-width: 1100px)")`: при сужении окна ниже `1100px`, если одновременно открыты И левый `Sidebar`, И правый `WorkspacePanel`, автоматически сворачивать правую панель (либо при ручном открытии `WorkspacePanel` на `< 1100px` сворачивать `Sidebar`), гарантируя центральному холсту минимум `>= 540px` чистой ширины без горизонтального скролла и обрезки кнопок в `TitleBar` (в `.titlebar` скорректировать `grid-template-columns: minmax(160px, 1fr) minmax(200px, 380px) minmax(180px, 1fr)`).
2. **`D2-UX-02` (`TitleBar`, порталированные меню, `.stop-button:hover`, тёмные скроллбары без стрелок):**
   - В [TitleBar.tsx:67-87](../../apps/desktop/src/components/TitleBar.tsx): исключить `.command-search, .stats-anchor, .window-controls` из `handleDragStart` и `onDoubleClick`; на контейнер `.command-search` повесить `onMouseDown={(e) => e.stopPropagation()}` и `onClick={() => meta.searchInputRef.current?.focus()}`, чтобы клик в любую точку поисковой капсулы мгновенно фокусировал ввод.
   - Создать лёгкий переиспользуемый компонент `PortalPopover.tsx` (`createPortal(..., document.body)`, `position: fixed`, расчёт `getBoundingClientRect()` триггера с автоматическим переворотом вверх/влево при нехватке места у краёв `window.innerHeight` / `window.innerWidth`, закрытие по `mousedown` снаружи, `Escape`, `resize` и внешнему `scroll`). Перевести на него меню действий проекта (`Sidebar`), меню жизненного цикла артефакта и пикер MIME (`WorkspacePanel`), фильтр проектов (`Inbox`/`Tasks`) и поповеры `Composer` / `StatsPopover` — ни один `overflow: hidden` / `mask-image` родителя больше никогда не обрежет меню!
   - В [desktop.css:1650-1654](../../apps/desktop/src/styles/desktop.css) прописать:
     ```css
     .send-button.stop-button {
       color: #ffd5d5;
       background: rgba(240, 138, 138, 0.2);
       border: 1px solid var(--danger-border);
     }
     .send-button.stop-button:hover:not(:disabled) {
       color: #ffffff;
       background: #c94a4a;
       border-color: #e06666;
     }
     ```
   - **Тёмные полосы прокрутки без стрелок Windows:** заменить `scrollbar-width: thin` на единые глобальные правила Chromium/WebView2 для всех скролл-областей (`.conversation`, `.sidebar-scroll`, `.resource-scroll`, `.d2-card-scroll`, `.popover-scroll-list`, `.composer__input`, `.artifact-preview__body`, `.command-results`, `.settings-sheet__body`, `.folder-picker__list`):
     ```css
     ::-webkit-scrollbar {
       width: 6px;
       height: 6px;
     }
     ::-webkit-scrollbar-button {
       display: none;
       width: 0;
       height: 0;
     }
     ::-webkit-scrollbar-track {
       background: transparent;
     }
     ::-webkit-scrollbar-thumb {
       background: rgba(255, 255, 255, 0.16);
       border-radius: 999px;
     }
     ::-webkit-scrollbar-thumb:hover {
       background: rgba(255, 255, 255, 0.28);
     }
     ::-webkit-scrollbar-corner {
       background: transparent;
     }
     ```
3. **`D2-UX-03` (Правая панель `WorkspacePanel`):**
   - Сделать заголовки секций `Файлы проекта` и `Артефакты сессии` сворачиваемыми (`aria-expanded`, шеврон `<CaretDown />`).
   - Преобразовать плоский список `state.projectFiles` в компактное сворачиваемое дерево папок и файлов (клик по папке раскрывает/сворачивает вложенные элементы или догружает `listProjectFiles(projectId, dir.rel_path)`).
   - Разделить активные артефакты и архивные: `archived`-артефакты вынести в отдельный свёрнутый по умолчанию подблок `Архив (N)` внизу секции артефактов (аналогично архиву проектов в `Sidebar`).
   - На `deleted`-заглушках (tombstones), у которых `allowedActions.length === 0`, **не рендерить** кнопку `⋯` (`artifact-menu-*`), чтобы никогда не открывалось пустое меню!
   - Перед необратимыми действиями над артефактом (`remove` и `delete`) показывать компактное инлайн-подтверждение / модальный диалог подтверждения (как для удаления проекта в `Sidebar`).
   - В блоке `.artifact-preview` добавить строку метаданных с коротким `SHA-256: {selectedArt.sha256.slice(0, 12)}…` (полный хэш в `title`), кнопкой «Копировать SHA-256» и кнопкой «Копировать содержимое», а для `mime_type === "text/markdown"` или имени `*.md` — переключатель `Просмотр | Исходник` с рендерингом через `<ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml>`.
4. **`D2-UX-04` (Карточки в `TasksCard`):**
   - Добавить чистую функцию `formatHumanCountdown(targetMs, nowMs)` с ежесекундным/15-секундным тиком: `через 45с`, `через 12м`, `через 2ч 15м`, `через 3д 4ч` (`tabular-nums`), отображаемую отдельной заметной меткой на карточках `waiting_timer`.
   - Для задач в `waiting_signal`: если задан `expected_signal_name`, показывать однокликовую кнопку `Отправить сигнал «{task.expected_signal_name}»` + опциональную кнопку `+ данные` для ввода текстового payload (без ручного перепечатывания технического имени сигнала!).
   - Для задач в `unknown_external`: показывать прямо на карточке поручения кнопку `Проверить у сервиса` (`actions.reconcileEffect`) и кнопку `Открыть во Входящих`, чтобы владельцу не приходилось вручную искать связанную карточку.
5. **`D2-UX-05` (`InboxRadarCard` и честный статус ожидания подтверждения):**
   - Если у активной сессии или задачи открыт `permission_request` (`status === "pending"`), в `StatusBanners`, `InboxRadarCard` (`radarRunningSessions`) и `MessageTimeline` отображать янтарный статус **«Ожидает подтверждения»** (без крутящегося спиннера и без вводящей в заблуждение подписи «Выполняется»!).
   - В карточках `permission_approval` (во `Входящих` и инлайн в `MessageTimeline`) отображать `linkedPerm.preview_snippet` (моноширинный блок до 6 строк с кнопкой `Показать полностью`, если сниппет длиннее), чтобы владелец видел, *что именно* инструмент собирается записать в файл.
   - При разрешении карточки `unknown_effect` сохранять и показывать не исчезающее бесследно уведомление/статус с явным вердиктом сверки (`«Сверка завершена: сервис подтвердил выполнение»` или `«Отмечено невыполненным — повтор безопасен»`).
   - Сохранять `scrollTop` контейнера `.d2-card-scroll` при обновлении списков `openCards` / `tasks`.
6. **`D2-UX-06` (`useModalFocusTrap` и доступность клавиатуры):**
   - Реализовать хук `useModalFocusTrap(dialogRef, isOpen, onClose)`: при открытии модального окна запоминает `document.activeElement`, переводит фокус внутрь диалога, удерживает `Tab` / `Shift+Tab` строго по фокусируемым элементам внутри модалки и при закрытии возвращает фокус на кнопку-инициатор. Подключить ко всем 4 модальным диалогам: `SettingsSheet`, `CommandPalette`, `DevFolderPickerModal` и диалогу подтверждения удаления в `Sidebar` / `WorkspacePanel`.

---

## 7. Матрица верификации, регрессий и проверки поломкой (`Slice R1` + `Slice R2`)

Перед докладом владельцу на воротах `H2` должны быть реально прогнаны и зелёными следующие проверки:

1. **Документация и архитектурные границы:**
   - `uv run --project ops/runner dennett-check docs`
   - `uv run --project ops/runner dennett-check arch` (проверяет чистые зависимости `dennett-contracts`, независимость `memoryd`, запрет `dennett-control-mcp` в `dennett-desktop`, `0 <select>`, `0 backdrop-filter`, `0 dangerouslySetInnerHTML`, `0` dev-эндпоинтов в `dist`).
2. **Форматирование, строгий Clippy и Unit / Property тесты (`proptest`):**
   - `uv run --project ops/runner dennett-check fmt`
   - `uv run --project ops/runner dennett-check lint`
   - `uv run --project ops/runner dennett-check unit` — включая новые unit/`proptest`-тесты на:
     - `D2-SEC-02`: `rule_matches_all_for_allow` (мульти-таргетная попытка обхода отклоняется), percent-encoded `file:///ws/%2e%2e/secret.txt` отклоняется fail-closed, `(AskBeforeWrite, AllowPaths)` запрещён в `write_policy_is_subset_of`, потолок `parent_write_ceiling` ограничивает проектный `CapabilityGrant`;
     - `D2-SEC-01`: пустой `allowed_memory_scopes` возвращает пустой `mcp_servers` и запрещает `send_agent_message`; `decide_slot_reuse` возвращает `RespawnAndResume` при смене скоупов или истечении TTL `< 10 мин`;
     - `D2-BUG-05`: валидация `OnceAt` и расчёт `WeeklyWallTime` по маске дней недели через DST/границу суток.
3. **Интеграционные тесты на реальном PostgreSQL 16 (`dennett-check integration`) и сквозное демо (`dennett-demo-d2`):**
   - `D2-ARCH-01`: обрыв клиентского запроса во время хода **не** отменяет серверный run (`V03`), повтор с тем же `client_message_id` присоединяется к активному run и получает тот же ответ, а `cancel_session_run` останавливает его;
   - `D2-ARCH-02`: `archive -> unarchive` отменяет активные ходы во всех профилях, ставит `pause_reason = 'archived_project'`, не трогает `misfire_ask`-таймеры при `unarchive` и не помечает периодическую задачу `succeeded` при архивации во время хода;
   - `D2-ARCH-03`: `dispatch_external_effect` проходит через `PolicyEngine`, `StopTask` не закрывает `unknown_effect`, `remove`/`delete` и `finalize_turn` сохраняют `unknown_external` и открытую карточку сверки, параллельный `reconcile_external_effect` сериализуется под `FOR UPDATE` (`409` второму);
   - `D2-BUG-01`..`04` и `D2-OVER-01`: ответы/сигналы переживают рестарт `dennett-head`, время ожидания `AskBeforeWrite` не съедает `turn_timeout` и `wall_ms_used`, миграция `v5 -> v6` переносит `memory_commit_outbox` в `ordered_outbox` и `HeadScheduler` автоматически доставляет их после подъёма `memoryd`;
   - **Сквозной тест `dennett-control-mcp`:** агент через stdio MCP создаёт событие (`dennett_event_create`), его сессия помечается `session_kind = 'task_run'`, попытка обратиться к чужому `project_id` отвергается fail-closed (`403`), список и отмена работают через `HeadClient`.
4. **Фронтенд и сборка Tauri (`Slice R2`):**
   - `pnpm --dir apps/desktop test` (Vitest: проверка кнопки `+` с префиллом `"Создай событие: мне необходимо "`, авто-скрытия `Sidebar` во `Входящих`/`Поручениях`, полного фильтра проектов, отсутствия `Workspace` в `ActivityRail`, фильтрации `task_run`-сессий, `PortalPopover`, `useModalFocusTrap`, предпросмотра `preview_snippet`, честного статуса «Ожидает подтверждения» и ручного подтверждения офлайн-очереди после `loadConnectedState`);
   - `pnpm --dir apps/desktop build` + `cargo check -p dennett-desktop --features custom-protocol`.
5. **Намеренная локальная поломка (проверка чувствительности критических тестов по `AGENTS.md`):**
   - Временно ослабить проверку `rule_matches_all_for_allow` или сохранить `runs.status = 'succeeded'` поверх `unknown_external` -> убедиться, что новые интеграционные тесты мгновенно краснеют -> вернуть фикс и повторно подтвердить полный зелёный прогон.
6. **Независимая живая проверка UI/UX на Carbon в браузере:**
   - Физически открыть работающий интерфейс поверх живого `dennett-head` + `memoryd` + PostgreSQL 16 (на `1280x800` и в узком окне `960x620`), прокликать создание события через `+` и диалог с агентом, проверить `Входящие`, `Поручения`, `Workspace`, поповеры, скроллбары без стрелок, `:hover` кнопки `Stop` и клавиатурную навигацию до сдачи владельцу.
