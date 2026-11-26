# Текущее состояние новой реализации

Обновлено 2026-11-26. Текущая ветка реализации D3: `d3/memory-fabric-and-capabilities` (от принятой базы D2 `a391eb1`). Новый baseline не наследует завершённые milestones, тесты или approvals старой реализации.

## Текущая работа

Текущий этап: **D3 («Полноценная Memory Fabric и заменяемые возможности» + D3+ Live Agent Polish & Hardening)**. Состояние: **завершён и верифицирован полностью (`d3/memory-fabric-and-capabilities`, схемы `memory` `v3` и `control` `v10`)** — включая самостоятельную Memory Fabric v2 (многоязычный FTS на стоковом PG16 + свежий хвост, утверждения/история/конфликты, досье Markdown, прерываемая консолидация с сохранением `caveats`, `forget`-очистка с V15-монотонным курсором томбстоунов `tombstone_seq`, переносимый экспорт/импорт по контракту J), write-only `SecretBroker`, `dennett-runtime-local` (`V29`), `A07`/`A08` и провайдер-независимую доставку скиллов (`41 §7.10`, `[Dennett Active Skills Index]` + `dennett_skill_list`/`dennett_skill_read` в `dennett-control-mcp`), чистый стриминг видимого текста в реальном времени без ленты вызовов инструментов, серверную долговечную очередь отложенных сообщений (`session_followups`, автоотправка `V03`, безопасная пауза на `Stop`/ошибке), живой steering без `409 Conflict`, OS-уровень изоляции дерева процессов `agy acp` (Win32 Job Object / Unix process group + сброс на отмене), content-addressed неизменяемые снапшоты файловых артефактов (`artifact_blobs`) и немедленное затирание текста в `ordered_outbox` с синхронизацией томбстоунов. Пройдены полный `dennett-check all --manage-pg`, реальный живой боевой прогон `agy acp 1.5.18` поверх изолированной копии реального проекта `G0oD_auto_meets`, два независимых код-ревью на Claude Opus 4.6 и живой браузерный UI/UX-аудит на Carbon. Ожидается приёмка **H3** перед стартом **D4**.

Запись приёмки **H0** (2026-11-23): владелец явно принял этап D0 (`d0/foundation` @ `1a40e46`, CI [#45179510052](https://github.com/Andrey-Good/dennett-agent-orchestrator/actions/runs/45179510052)) и разрешил запуск D1.
Запись приёмки **H1** (2026-11-24): этап D1 (`d1/first-vertical-slice` @ `7496ee2`, `H1-01`..`H1-08` закрыты) принят владельцем; разрешён запуск этапа D2 на ветке `d2/durable-runs-and-stop`.
Запись приёмки **H2** (2026-11-24): владелец явно принял этап D2 (`d2/durable-runs-and-stop` @ `a391eb1`, CI [#45414196229](https://github.com/Andrey-Good/dennett-agent-orchestrator/actions/runs/45414196229) 4/4 зелёные, включая одиночные чаты `≤ 2/3 : ≥ 1/3`, пины и `Copy`) и разрешил запуск этапа D3 на ветке `d3/memory-fabric-and-capabilities`.

Основные срезы этапа D3 в ветке `d3/memory-fabric-and-capabilities`:
- `717673d` — **Stage 0 (контракты и тонкие клиенты)**: `crates/dennett-contracts/src/{memory.rs, d3.rs, control.rs, lib.rs}` (прямые зависимости строго `serde`, `thiserror`, `uuid`), каркас `adapters/local-runtime` (`dennett-runtime-local`, 0 запрещённых крейтов), правила изоляции в `ops/runner`, тонкие клиенты `MemoryClient` и `HeadClient`.
- `a84709b` — **Memory Fabric v2 (`dennett-memory-core`, `dennett-memoryd`, `dennett-memory-cli`, `dennett-memory-mcp`)**: миграция `0002_memory_fabric_v2.sql` на стоковом PG16 (`tsvector`/`GIN` `simple`/`english`/`russian`, без `pgvector`), сохранение различия эпизодов при content-dedup, `memory_claims` с защитой от перезаписи новых решений старыми и автодетекцией `memory_conflicts`, версионированные Markdown-досье (`memory_documents`), отделяемый перестраиваемый `memory_search_index` + свежий неиндексированный хвост + честный `index_status`, прерываемая консолидация с сохранением `caveats`, `forget`-очистка ревизий/индекса с content-free tombstones и `residual_backup_notice`, детерминированный экспорт без секретов и fuzz-устойчивый недоверенный импорт (`external_untrusted`, `instruction_active = false`), полный CLI и 16 MCP-инструментов без зависимости от `Head`.
- `d2ae8b8` + `adff784` + `e5a8d05` — **Capabilities, SecretBroker, LocalHttpRuntime, `A07`/`A08`/`V29`, Head Memory Proxy и `dennettctl`**: миграция `0008_d3_capabilities_and_secrets.sql` (`control` `v8`), чистый Rust AEAD `SecretBroker` (`chacha20poly1305` + `zeroize`, write-only, live-редактирование секретов), `dennett-runtime-local` (`LocalHttpRuntime` loopback-by-default с Ollama/LM Studio discovery, graceful tool fallback и общим `run_runtime_conformance_suite`), `V29` mismatch-guard и честный rehydrate при смене адаптера/профиля/версии (с уважением стартового `--auth-profile gemini-api-key` без тихого fallback и с записью `RunStatus::Failed`), классификация `ProviderQuotaExceeded`, `A08` Capabilities Registry (read-only скан `~/.gemini/` без записей, `candidate_unattached` изоляция, реальная инспекция папок с `SKILL.md`/манифестами плагинов, YAML block scalars `>-`/`|`, rug-pull сброс `trust_state`, 1-click rollback), `A07` правила субагентов + текстовая кооперация в `dennett-control-mcp`, V02-безопасный `/v1/memory/*` прокси, `dennettctl` паритет и `demo-d3`.
- `33b1a36` + `daaf6b6` — **Desktop UI D3 (`WP-M01-003` v5, 0 `<select>`, 0 `backdrop-filter`, 0 `dangerouslySetInnerHTML`, 0 `window.confirm`)**: экран «Память» (`Alt+4` / `Brain`, автоскрытие `Sidebar`, поиск + «История», вкладки Знания/Конфликты/Досье, provenance-переходы, встроенное «Забыть» и экспорт/импорт), 2-вкладочный `SettingsSheet` («Провайдеры и ключи» с write-only `SecretBroker`, локальным сервером и статическими плашками `R-CLAUDE`/`R-CODEX` без кнопок входа + «Навыки, плагины и MCP» с нативным пикером папок), живой `StatsPopover` и Tauri IPC команды; закрыты `D3-UI-01..07`.
- `ae312b3` + `66917fe` + `2f60108` + `2ee4f85` + `75c032b` — **D3+ Live Agent Polish & Provider-Independent Skills (`control` `v9`)**: живой Markdown-стриминг видимого текста в чате без шума вызовов инструментов (`LiveRunSnapshot`), контекстные токены в `StatsPopover`, долговечная серверная очередь `session_followups` (`V03`-автоотправка на `Succeeded`, безопасная пауза на `Stop`/ошибке, правка/промоут/продолжение), живой steering `POST /steer` без `409 Conflict`, провайдер-независимая доставка скиллов (`41 §7.10`: `[Dennett Active Skills Index]` + `dennett_skill_list`/`dennett_skill_read` для проектных и одиночных чатов), выравнивание `PolicyEngine::MEMORY_MCP_OPS` под все 13 D3-инструментов `dennett-memory-mcp` (поймано реальным боевым прогоном `agy acp 1.5.18` на `G0oD_auto_meets`).
- `d7f4bc4` + `02b4e1b` — **Укрепление надёжности и приватности (`memory` `v3` / `control` `v10`)**: OS-изоляция дерева процессов `agy acp` (Win32 Job Object `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` / Unix `process_group(0)` + сброс процесса на отмене, 0 сиротских подпроцессов и NTFS-блокировок), content-addressed неизменяемые снапшоты файловых артефактов (`artifact_blobs` + `GET .../content`, контракт D §D.10), немедленное затирание открытого текста в `ordered_outbox` после доставки (`[delivered]`), V15-монотонный курсор томбстоунов `tombstone_seq`/`commit_xid` в `memoryd` + синхронизация `[forgotten]` и защита от воскрешения при lost-ACK в `Head`, устранение рассинхронизации часов PG/host в `flush_memory_outbox`.

## Подтверждённые проверки D3 (`d3/memory-fabric-and-capabilities`, `memory` `v3` / `control` `v10`)

- **Локально на Windows 11 (2026-11-26):**
  - `dennett-check all --manage-pg`: `docs` OK, `deps` OK (517 locked crates, 0 запрещённых/отложенных зависимостей), Desktop `tsc --noEmit` + Vitest **69/69** + Vite build, `rustfmt`, `clippy -D warnings`, `rustdoc -D warnings`, doctests, fast nextest **180/180** (включая реальный Win32 Job Object тест на освобождение NTFS-блокировки дочернего процесса), `self-check` 4/4, `integration --manage-pg` **88/88** реальных PostgreSQL 16 тестов (`0 failed, 0 skipped, 0 leaky`);
  - Живой `agy acp 1.5.18` canary (`dennett-check runtime-probe`): **4/4** PASS (`~/.gemini/antigravity-cli/conversations`: `29 -> 29`);
  - **Реальный боевой прогон `agy acp 1.5.18` на изолированной копии `G0oD_auto_meets`:** отключённый скилл скрыт (`count: 0`), включённый скилл прочитан через `dennett_skill_read`, живой стриминг (`98` live-сэмплов, `1421` симв., контекст `34408 / 1048576`), заполнение Memory Fabric v2 (`2` claims с caveats + Markdown-досье) и воспроизведение памяти в новом чате без чтения диска, автоотправка отложенного follow-up, живой steering без `409`, пауза очереди на `Stop` (`~/.gemini`: `29 -> 29`);
  - Нативная сборка Tauri 2: `cargo build -p dennett-desktop --features custom-protocol` — PASS;
  - Независимое ревью на Claude Opus 4.6 (закрыты все P1/P2) и живой браузерный UI/UX-обход на Carbon поверх `demo-d3` (`1280x800` и `960x620`, `0` консольных ошибок, `0` P0/P1, закрыты оба P2).

## Ограничения D3 (граница этапа)

- Реализованы полная самостоятельная Memory Fabric v2 (`10`, `H`, `J`), SecretBroker, `LocalHttpRuntime` + `AntigravityAcpRuntime` (`V29`, `A07`, `A08`, `A09`), Skills/Plugins/MCP lifecycle, Desktop UI («Память», `SettingsSheet`, `StatsPopover`), полный CLI/MCP паритет и `demo-d3`.
- Живой фоновый LLM-прогон по платному `GEMINI_API_KEY` проверяется с ключом окружения/хранилища; без ключа профиль честно fail-closed с `AUTH_REQUIRED` без тихого переключения на `oauth-personal`. `Claude Code (R-CLAUDE)` и `Codex (R-CODEX)` остаются отложенными (`planned`, без зависимостей и кнопок входа).
- Гибридные веб-поверхности контракта L / MCP Apps / A2UI — **D4**; multi-device, pairing и полная потоковая реплика PG — **D5**; голос и сенсорика — **D6**; реальные внешние коннекторы и computer-use — **D7**; самоулучшение E01–E10 — **D8**; релизный инсталлятор и soak — **D9**.

## Этапы и приёмка

- D0 — основа/проверки: **принят (H0 принят владельцем 2026-11-23 на `d0/foundation` `1a40e46`)**.
- D1 — агент (`agy acp`) + `memoryd` + реальный сохраняемый чат: **принят (H1 принят владельцем 2026-11-24 на `d1/first-vertical-slice` `7496ee2`)**.
- D2 — длительные поручения/ожидание/отмена: **принят (H2 принят владельцем 2026-11-24 на `d2/durable-runs-and-stop` `a391eb1`, CI `#45414196229`)**.
- D3 — полная память и подключения: **завершён и верифицирован на `d3/memory-fabric-and-capabilities` (схемы `memory` `v3` и `control` `v10`, live `agy 1.5.18` canary 4/4 + реальный боевой прогон `G0oD_auto_meets`, Claude Opus 4.6 и Carbon Chromium ревью закрыты); готов к приёмке H3 и переходу к D4**.
- D4 — графика и рабочие экраны: не начат; H4 не принят.
- D5 — устройства и полный резерв: не начат; H5 не принят.
- D6 — голос и сенсорика: не начат; H6 не принят.
- D7 — внешние действия и computer-use: не начат; H7 не принят.
- D8 — проверяемое самоулучшение: не начат; H8 не принят.
- D9 — весь продукт и выпуск: не начат; H9 не принят.

## Среда и внешние условия

Локальная Windows 11 среда (2026-11-25):
- Rust stable `1.100.0` (`rustc`, `cargo`, `rustfmt`, `clippy`, `cargo-nextest 0.9.152`), MSVC Build Tools / VS 2022.
- Python `3.13.5` + `uv 0.11.23`, Node `v22.17.1` / `pnpm 10.33.0`.
- Docker `29.4.3` (`postgres:16-alpine`).
- Antigravity CLI `agy 1.5.15` (подписка `Google AI Ultra`, `g1-ultra-tier`).
- Архивная ветка старого кода: `archive/before-code-reset-2026-09-27`.

## Как обновлять эту запись

После связного шага кратко указать commit, результат, выполненные и оставшиеся проверки, препятствие, следующую задачу. Перед Hn дать ссылку на build, test evidence, ошибки и личную демонстрацию. Принятие фиксируется только по явному ответу владельца с областью и версией. Не публиковать приватные логи и токены.

## Открытые дефекты новой программы

Открытых блокирующих дефектов D0–D3 нет; все дефекты H1, H2, внешнего аудита D2, а также все находки живого canary (`LIVE-A01-01`) и браузерного ревью D3 (`D3-UI-01..07`) закрыты и подтверждены тестами, живым браузером и ревью на Claude Opus 4.6 (`[]`).
