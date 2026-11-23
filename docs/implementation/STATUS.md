# Текущее состояние новой реализации

Обновлено 2026-11-24. Текущая ветка реализации D1: `d1/first-vertical-slice` (от принятой базы D0 `d0/foundation` `1a40e46`). Новый baseline не наследует завершённые milestones, тесты или approvals старой реализации.

## Текущая работа

Текущий этап: **D1** (первый настоящий путь: агент на `agy acp`, самостоятельный `memoryd` на PostgreSQL, сохраняемый разговор и новый desktop-чат). Состояние: **технически завершён и прошёл полный аудит H1; ожидает владельца (точка приёмки H1)**. До явного принятия H1 переход к D2 не начинается.

Запись приёмки **H0** (2026-11-23): владелец явно принял этап D0 (`d0/foundation` @ `1a40e46`, CI [#45179510052](https://github.com/Andrey-Good/dennett-agent-orchestrator/actions/runs/45179510052)) и разрешил запуск D1.
Запись дизайн-согласования **D1 Desktop Chat** (2026-11-23): владелец подтвердил визуальное направление `WP-M01-003` v5 (единый бесшовный Windows 11 Mica сверху и слева + отдельный скруглённый прямоугольник чата + отдельная выезжающая правая панель `Workspace` поверх того же Mica-фона, перенос визуального эталона поверх нового чистого бэкенда `HeadClient` → `dennett-head`).

Основные коммиты D1 в ветке `d1/first-vertical-slice`:
- `26c8c4e` — фиксация приёмки H0 и старта D1.
- `dc5e184` — **Slice 1** (проверено на Carbon): самостоятельный `dennett-memoryd` на PostgreSQL (`dennett-memory-core`, миграция `0001_memory_core_v1.sql`, `event_evidence_links` по V19, `StandaloneAuthVerifier`, `axum` API, `dennett-memory-client`, `dennett-memory-cli`, `dennett-memory-mcp`; `V01`, `V06`, `V19`; 3 дефекта закрыты, 3 мутации пойманы).
- `2c3d1b4` — **Slice 2** (проверено на Carbon): `dennett-head` (`dennett_control` БД, миграция `0001_control_core_v1.sql`, `PolicyEngine`, Agent/ConfigVersion/Project/Session/Messages/Artifacts, V02 `memory_commit_outbox`, `HeadClient`/`dennettctl`) + продуктовый `adapters/antigravity-acp` с реальным `dennett-memory-mcp`, `PreToolUse` защитой записи в workspace, `AllowOnce`/`RejectOnce`, `Stop`/`session/cancel`, явным `session_id`/rehydrate и строгим разделением `oauth-personal` vs `gemini-api-key` (`V02`, `V04`, `V05`, `V06`, `V29`, `A01–A06`, `A09`; 5 дефектов закрыты, 3 мутации пойманы).
- `85b8f76` / `b1432a1` / `0c019a7` — **Slice 3** (проверено на Carbon + зелёный CI [#45229373219](https://github.com/Andrey-Good/dennett-agent-orchestrator/actions/runs/45229373219)): новый `apps/desktop` (Tauri 2 + React 19/TS/Vite, Windows 11 Mica), `V03` независимость демонов при закрытии окна, `dennett-check demo-d1`, CI Linux + Windows + PG.
- `53a0d31` / `30dc620` — **UI/UX Rework & Dynamic Runtime Catalog** (проверено `ui-reviewer` в живом браузере и `code-verifier` с мутациями): эталонный dark glass UI `WP-M01-003` (3-частная левая геометрия, нативный `rfd` выбор папки, динамические тёмные меню без `<select>` и без `backdrop-filter`, `<QuotaRings />` слот, правая панель `Workspace` с реальными файлами и артефактами), живой `GET /v1/runtime/catalog` без захардкоженных моделей, проекты без папки (`0002_projects_nullable_workspace.sql`), `PATCH /v1/projects/{id}`, `GET /v1/projects/{id}/files`, `PATCH /v1/sessions/{id}`, авто-именование чатов, динамические HMAC `dnt1.` scope-токены без привязки `Head` к `memory-core` и структурное редактирование `dnt1.` токенов.
- **H1 Hardening Pass (`H1-01`..`H1-08`, поверх `30dc620` на `d1/first-vertical-slice`)**:
  - `H1-01`: сквозная проверка loopback `Host`, `Origin` и `Sec-Fetch-Site` на всех маршрутах `dennett-head` (защита от DNS-rebinding и cross-site CSRF; флаг `--allow-non-loopback` проброшен в `HeadAppState`).
  - `H1-02`: разумный дефолт `allow_workspace` для проектов с папкой (с апгрейдом агента `deny_all → allow_workspace` при привязке папки), строго рабочие D1-опции в `Composer` (`Запись в папку` / `Только чтение`) и явные карточки в ленте при отказе политики или пустом ответе модели.
  - `H1-03`: валидация введённого пути в `DevFolderPickerModal` через `/__dennett_dev_dirs` перед подтверждением (исключена тихая привязка `$HOME`).
  - `H1-04`: мгновенное оптимистичное отображение сообщения пользователя с очисткой поля ввода и сохранением нового черновика, по-сессионное состояние `runningSessionIds`/`cancellingSessionIds` (баннер и `Stop`/`Esc` строго для активного чата, фоновый индикатор в сайдбаре) и capture-phase закрытие поповеров по `Escape` без отмены активного запуска.
  - `H1-05`: каноническая Serde-сериализация `ThinkingLevel::XHigh` как `"xhigh"` (с `alias = "x_high"`).
  - `H1-06`: поиск проектов + навигация `ArrowDown`/`ArrowUp`/`Enter` и empty-state в `CommandPalette`, авто-раскрытие свёрнутого сайдбара при вводе в верхний поиск, чистое скрытие времени при наведении на чат (без наезда на карандаш), полноширинный inline-rename проектов без клиппинга первого символа и чистые хлебные крошки на `<= 960px`.
  - `H1-07`: потоковое SHA-256 хеширование файловых артефактов (`BufReader` в O(1) RAM) в `spawn_blocking` до взятия соединения из пула PG, forward-миграция `0003_artifacts_unique_revision.sql` (`LATEST_CONTROL_SCHEMA_VERSION = 3`, `UNIQUE (session_id, name, revision)`) и `SELECT ... FOR UPDATE` строки сессии для конкурентной сериализации ревизий.
  - `H1-08`: CLI-паритет `dennettctl` (`runtime catalog`, `project update`, `project files`, `session update`, актуальный `--help`) и синхронизация `STATUS.md` / `REQUIREMENTS_COVERAGE.md`.

## Подтверждённые проверки D1 (актуальная рабочая копия `d1/first-vertical-slice` поверх `30dc620`)

- **Локально на Windows 11 (2026-11-24):**
  - `uv run --project ops/runner dennett-check all --manage-pg` — PASS:
    - `docs`: 110 Markdown-файлов, 4144 локальных ссылок валидны;
    - `deps`: 504 locked crates, все границы сохранены (`dennett-contracts` direct = `['serde', 'thiserror', 'uuid']`, `dennett-head` не зависит от `dennett-memory-core`, `dennett-desktop` изолирован от `Head`/`memory-core`/PG/`antigravity-acp`, 0 `claude`/`codex`/`openai`/`anthropic`);
    - `fast`: `tsc --noEmit`, Vitest **20/20** (0 skipped), Vite production build (0 совпадений dev-proxy/picker в `apps/desktop/dist`), `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo doc --no-deps` (`-D warnings`), `cargo test --doc`, fast nextest **116/116** (0 skipped, 0 leaky);
    - `self-check`: 4/4 инварианта harness подтверждены;
    - `integration --manage-pg`: **29/29** реальных PostgreSQL 16 + мультипроцессных тестов (0 skipped, 0 leaky; схема `control` `v3`, включая `V01`, `V02`, `V03`, `V05`, `V06`, `V19`, `V29`, `H1-01` DNS-rebinding/CSRF, `H1-05` `"xhigh"`, `H1-07` конкурентные ревизии артефактов, `H1-08` `dennettctl` parity и per-project HMAC `dnt1.` scopes).
  - Нативная сборка Tauri 2: `cargo build -p dennett-desktop --features custom-protocol` — PASS (`target/debug/dennett-desktop.exe`).
  - `uv run --project ops/runner dennett-check runtime-probe` (живой `agy acp 1.5.15`, `Google AI Ultra`): PASS (**4/4** живых E2E теста — `acp_live_probe` 2/2 + `head_live_agy_e2e` 2/2; реальный `~/.gemini/antigravity-cli/conversations` не тронут: `before=25 after=25`, 0 висящих контейнеров).
  - Предыдущий зелёный прогон GitHub Actions CI (`Dennett CI`): [#45229373219](https://github.com/Andrey-Good/dennett-agent-orchestrator/actions/runs/45229373219) (после финального коммита и push H1-правок требуется свежий прогон CI).
  - Мутационные проверки Carbon-верификаторами: **13/13 пойманы и байт-в-байт откачены** (10 в Slice 1–3 + 3 в UI/Dynamic Catalog rework).

## Ограничения D1 (граница этапа)

- Реализовано сквозное ядро D1 (память с источниками/версиями/историей/scope-правами, `head` со схемой `v3` + `agy acp` с динамическим каталогом моделей, новый desktop-чат `WP-M01-003`, файлы проекта и обычные артефакты). Полнотекстовый/векторный поиск и консолидация памяти — D3; длительные фоновые поручения/расписания/таймеры и интерактивные approval-диалоги — D2; гибридные веб-поверхности L/MCP Apps — D4; multi-device и резервная реплика — D5; голос и сенсорика — D6.
- Живой LLM-прогон по платному `GEMINI_API_KEY` (фоновый профиль) остаётся на D2/D3; на D1 живьём подтверждены интерактивный `oauth-personal` (`Google AI Ultra`) и жёсткий fail-closed отказ `gemini-api-key` без ключа.
- Авторизация HTTP API `dennett-head` привязана к `loopback-only` со сквозными проверками `Host`/`Origin`/`Sec-Fetch-Site` (однопользовательский локальный D1); mTLS/peer auth для Node и нескольких устройств вводится на D5.
- На Windows внешний `SIGTERM` (`TerminateProcess`) в `demo-d1` обходит Python `finally`, а штатный интерактивный `Ctrl+C` корректно останавливает и убирает все демо-процессы и контейнер PG.

## Этапы и приёмка

- D0 — основа/проверки: **принят (H0 принят владельцем 2026-11-23 на `d0/foundation` `1a40e46`)**.
- D1 — агент (`agy acp`) + `memoryd` + реальный сохраняемый чат: **технически завершён (`d1/first-vertical-slice` поверх `30dc620`, схема `v3`); ожидает ревью правок H1 и приёмки H1 владельцем**.
- D2 — длительные поручения/ожидание/отмена: не начат; H2 не принят.
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

Открытых блокирующих дефектов D1 нет (все 11 замечаний Slice 1–3, 5 визуальных/3 security-замечания UI Rework и все 8 находок полного аудита `H1-01`..`H1-08` закрыты с тестами до приёмки H1).
