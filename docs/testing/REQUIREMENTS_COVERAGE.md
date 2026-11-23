# Покрытие требований новой реализации

Инициализировано 2026-09-27; уточнено на D0 (2026-11-23) и обновлено по итогам реализации и аудита D1 (2026-11-24, ветка `d1/first-vertical-slice`). **Записи ниже честно разделяют реализованный и проверенный срез D1 от оставшихся этапов D2–D9 (для D2–D9 статус остаётся «не реализовано, не проверено»).** Единственное место статуса этапов и приёмки H0–H9 — [STATUS.md](../implementation/STATUS.md).

## 1. Как дополнять карту

В D0 сверено каждое нормативное семейство и каждый документ 00–70, F01–F10, E01–E10 и контрактов 00/A–M/90 с этапом реализации ниже. Конкретные обязательные пользовательские сценарии детализируются перед соответствующим этапом. По мере выполнения у каждой записи появляются ссылка на реальный тест/ручную проверку, commit/build и фактический результат. Формат остаётся Markdown; дублирующий JSON-реестр и собственный task engine не заводятся.

Отсутствующий тест остаётся отсутствующим, а не «не применимо» по желанию агента. Действительно неприменимый тест имеет причину и ссылку на границу требования. Каждый работающий обязательный путь входит в регрессии последующих этапов. Один сквозной тест может покрывать несколько связанных требований, но не заменяет специфические негативные и граничные случаи.

Примеры/исследовательские альтернативы не становятся автоматически обязательными backend-реализациями. И наоборот, требуемый результат нельзя объявить необязательным только потому, что его трудно сделать. Сокращение согласованного продукта требует отдельного решения владельца и не выдаётся за завершение исходной программы.

## 2. Семейства продукта → этапы (P01–P23)

- **P01. Установка, запуск и остановка.** D0/D1/D9. Чистое окружение, независимая жизнь UI/Node/Head/memoryd, безопасный тестовый профиль, явная диагностика отсутствующих возможностей. Источники: 50/60/61, архитектура 80/83. Статус: **D0 принят (H0); D1-срез проверен** (`memoryd_process_integration`, `head_control_e2e_integration`, `desktop_bridge_v03_integration`, `dennett-check demo-d1`, независимое закрытие окна `V03`, loopback DNS-rebinding/CSRF-защита Head; ожидает H1); D9 installer/packaging/multi-node — не реализовано, не проверено.
- **P02. Агенты, разговор и модели.** D1/D2/D3. Единая сущность Agent, прямой разговор, сохранение истории, native continuation/steer, смена настройки без новой личности, помощники и текстовая кооперация. Источники: F01–F05/F10, 20/41/82. Статус: **D1-срез проверен** (`head_control_e2e_integration`, `head_live_agy_e2e`, `DesktopApp.test.tsx`: единая сущность `AgentId` + версионированный `AgentConfigVersion`, динамический выбор модели/размышления `low|medium|high|xhigh`, сохраняемый разговор/перезапуск, `Stop`/`session/cancel`, явный `session_id`/rehydrate; ожидает H1); D2/D3 помощники, steer и текстовая кооперация — не реализовано, не проверено.
- **P03. Поручения, ожидания, события.** D2/D5. Durable tasks/runs, расписания, сигналы, пропущенное/позднее, отмена, бюджеты, Inbox/Radar. Источники: 20/50, I, F07. Статус: не реализовано, не проверено.
- **P04. Самостоятельная Memory Fabric.** D1/D3. Запись/чтение без Head, provenance, исходники и рабочие документы, текущая версия и история, ограничения области. Источники: 10/80/81. Статус: **D1-срез проверен** (`memory_core_pg_integration`, `memory_fabric_e2e_integration`, `head_live_agy_e2e`: самостоятельный `dennett-memoryd` на PostgreSQL схема `v1`, provenance + `event_evidence_links` (`V19`), версии/история/оптимистическая блокировка, `dnt1.` HMAC scope-токены без привязки `Head` к `memory-core`, `dennett-memory-cli` + `dennett-memory-mcp`; ожидает H1); D3 рабочие topic documents/консолидация — не реализовано, не проверено.
- **P05. Обработка и поиск памяти.** D3. Дедупликация, отбор, сжатие/обобщение, retrieval и актуальность, мультимодальные evidence, ссылки на источники, влияние инструкций, коррекция/конфликт/удаление. Индексы восстановимы и не единственный источник данных. Источник: 10. Статус: не реализовано, не проверено.
- **P06. Проекты и рабочие материалы.** D1/D2/D3/D9. Создание/открытие/архив/удаление/перепривязка, файлы и Git/workspaces, пользовательские правки, перенос и продолжение. Источники: C, 20/50/60. Статус: **D1-срез проверен** (`head_control_e2e_integration`, `workspace_files::tests`, `DesktopApp.test.tsx`: создание проекта с папкой или без папки `workspace_path = NULL`, inline-переименование и привязка папки `PATCH /v1/projects/{id}`, безопасный листинг `GET /v1/projects/{id}/files` с блокировкой относительных/UNC/symlink/NTFS-junction путей и лимитами, изоляция `memory_scope`; ожидает H1); D2/D3/D9 архив/удаление/worktree/перенос — не реализовано, не проверено.
- **P07. Артефакты.** D1/D4/D7/D9. Версии, редактируемые исходники, representations, просмотр/экспорт, публикация и отзыв, повторное открытие без повторного эффекта. Источники: D/J/L. Статус: **D1-срез проверен** (`head_control_e2e_integration`, `desktop_bridge_v03_integration`, `DesktopApp.test.tsx`: обычные текстовые/файловые артефакты с потоковым SHA-256, уникальностью `(session_id, name, revision)` в схеме `v3` и конкурентной сериализацией, безопасный рендер без XSS; ожидает H1); D4/D7/D9 редактируемые исходники, representations, публикация/отзыв — не реализовано, не проверено.
- **P08. Права, доверие, аккаунты и секреты.** С D1 на всех этапах, полный цикл D5/D7/D9. Allow/deny/expiry/revoke, границы исполнения native tools и инструментов Dennett, exact approval, recovery/step-up, недоверенные данные и минимизация раскрытия. Источники: 30, F, 82/83. Статус: **D1-срез проверен** (`head_control_e2e_integration`, `head_live_agy_e2e`, `redact::tests`, `http::tests`: внешний `PolicyEngine` вне модели, блокировка выхода за пределы workspace/symlink/junction, `DenyAll` для проектов без папки, структурное редактирование секретов и `dnt1.` токенов, loopback `Host`/`Origin`/`Sec-Fetch-Site` защита Head, `CLI`/`MCP`/`UI` паритет; ожидает H1); D2/D5/D7/D9 интерактивные approval-диалоги, grants/expiry/revoke, multi-device trust и recovery — не реализовано, не проверено.
- **P09. Нативные runtimes и каталог возможностей.** D0 (risk probe)/D1/D2/D3/D4/D7. Первым реализуется родной **Google Antigravity (`agy acp`)** с двумя профилями входа (`oauth-personal` Google AI Ultra для интерактивной работы и `gemini-api-key` для будущих безлюдных фоновых задач). **Claude Code (`R-CLAUDE`)** и **Codex (`R-CODEX`)** отложены решением владельца вне D0–D9. Локальные модели, skills/MCP/plugins/connectors, установка/отключение/обновление, health/version/cost, CLI без дублирования правил. Источники: 41/82/M. Статус: **D0 probe принят; D1-срез проверен** (`acp_live_probe`, `head_live_agy_e2e`, `dennett-runtime-antigravity` unit tests, `dennettctl runtime catalog`: живой `agy acp 1.5.15`, динамический каталог `GET /v1/runtime/catalog` с моделями и уровнями размышления по каждой модели, `dennett-memory-mcp`, `oauth-personal` vs fail-closed `gemini-api-key`, 0 захардкоженных моделей; ожидает H1); D2–D7 фоновый LLM-прогон по API key, локальные модели, skills/plugins/connectors lifecycle — не реализовано, не проверено.
- **P10. Постоянная работа и ресурсы.** D2/D5/D6/D9. Budget/quota, CPU/GPU/память/диск, backlog, retention, деградация, перезапуск/восстановление, наблюдаемые ошибки. Источники: G/50/83. Статус: не реализовано, не проверено (в D1 проверен только outbox/деградация `V02` и слот `<QuotaRings />` под будущие квоты).
- **P11. Устройства и транспорт.** D5. Pairing/revoke, scoped cache, offline edits, безопасные сообщения/уведомления, версия протокола, большой resumable object, handoff. Источники: 50/61/J. Статус: не реализовано, не проверено.
- **P12. Полный резерв и authority.** D5/D9. PG streaming replica плюс объекты/ключи/runtime, readiness/lag, единственный Head, fencing старых write/dispatch, rewind/reseed и восстановление. Источники: 80/81 и ADR-002/003/005. Статус: не реализовано, не проверено.
- **P13. Desktop.** D1/D2/D4/D9. Все обязательные команды и поверхности 60: чат, проекты, поиск, окна/навигация, Inbox/Radar/Library/Inspector, редактирование и артефакты, явные состояния, клавиатура и доступность. Статус: **D1-срез проверен** (`DesktopApp.test.tsx`, `desktop_bridge_v03_integration`, живой браузерный аудит на `1280x800` / `1024x720`: тонкий клиент Tauri 2 + React 19 в направлении `WP-M01-003`, 3-частная левая геометрия, нативный `rfd` выбор папки, динамические тёмные меню без `<select>` и без `backdrop-filter`, оптимистичная отправка и по-сессионный `Stop`/`Escape`, поиск и `CommandPalette`, правая панель `Workspace` с файлами и артефактами, честные состояния офлайн/`V02`/`SIMULATED`/policy-denial; ожидает H1); D2/D4/D9 Inbox/Radar/Library/Inspector, поверхности L, глобальный поиск и доступность — не реализовано, не проверено.
- **P14. Mobile.** D5/D6/D9. Все обязательные сценарии 61: быстрый обзор, capture, approve, continue, проектный/голосовой разговор, notifications/deep links/widgets, native background/biometric, offline/process death. Статус: не реализовано, не проверено.
- **P15. Гибридные интерфейсы.** D4/D5/D6. L.9 и архитектура 83: статические файлы, собственная инфографика, WebSurfaceHost, готовые UI-адаптеры, правильная передача выделения, user edits, изоляция, доступный fallback. Статус: не реализовано, не проверено.
- **P16. Голос.** D6. Turn-taking, прерывание, heard-output, realtime/cascade, обращение к сильному исполнителю, контекст экрана, несколько устройств, приватность/latency. Источник: 40/82. Статус: не реализовано, не проверено.
- **P17. Ambient/встречи/сенсорика.** D6/D9. Локальные capture/ring buffers, consent, исключения, voice/screen sources, time anchors/meeting notes, права ОС, ресурсы и удаление. Источники: A/40/50. Статус: не реализовано, не проверено.
- **P18. Внешняя переписка и computer-use.** D2 simulated, D7 real, D9 integrated. Черновик/получатель/аккаунт, разрешённая отправка, UNKNOWN/reconciliation, последствия отмены, UI/DOM/browser/native backends, вмешательство владельца. Источники: B/20/30/41/82. Статус: не реализовано, не проверено.
- **P19. Locale, календарное время и глобальный поиск.** D2/D3/D5/D6/D9. Язык/часовые пояса/DST, разные freshness/coverage, поиск разных доменов без выдачи части за всё. Источники: H/I/60/61. Статус: не реализовано, не проверено.
- **P20. Переносимые пакеты и recipes.** D3/D5/D7/D9. Импорт/экспорт, совместимость, provenance, отсутствие переноса чужих полномочий; составные сценарии через имеющиеся capabilities. Источники: J/K/C/D. Статус: не реализовано, не проверено.
- **P21. Проверяемое самоулучшение.** D8. E01–E10: old/new реальные запуски, независимые mutable environments, unseen/regression, защищённые критерии, бюджет и область принятия, canary/rollback. Статус: не реализовано, не проверено.
- **P22. Поставка, версии, backup и recovery.** С D0/D1, целиком D5/D9. Миграции, совместимые версии, подписи/происхождение, backup window и полные RecoveryCut, ключи и отзыв, инструкции ремонта. Источники: E/F/50/81/83. Статус: **D0 CI/сборка приняты; D1-срез проверен** (идемпотентные forward-only миграции `memory` v1 и `control` v1→v3 с контрольными суммами и advisory lock, `head_pg_migration_idempotency_and_append_only_triggers`, CI Linux+Windows+PG; ожидает H1); D5/D9 backup/recovery/signing/release — не реализовано, не проверено.
- **P23. Диагностика и качество.** Все этапы. Наблюдаемые границы, error codes/ids, actual test outcomes, отсутствие скрытых skipped checks, функциональные/доступные UI, нагрузка/стоимость, review и владельческая приёмка. Источники: 70/90/83 и стратегия тестов. Статус: **D0 runner/self-check приняты; D1-срез проверен** (`dennett-check all --manage-pg`, `runtime-probe`, аудит Carbon `code-verifier`/`ui-reviewer`/`h1-reviewer` с мутациями и живым браузерным проходом, `dennettctl` CLI-паритет, `0` skipped/leaky; ожидает H1); D2–D9 нагрузочные/soak/финальные проверки — не реализовано, не проверено.

## 3. Полная привязка разделов бизнес-спецификаций и контрактов (аудит D0)

Каждому нормативному документу бизнес-спецификаций и дополнений назначены семейства P01–P23 и этапы D0–D9. Исходный статус реализации на D0: **не реализовано, не проверено** (если прямо не указан D0-артефакт среды).

- **Основания `F01–F10`** (`docs/restart/01_FOUNDATIONS.md`): единая сущность Agent, текстовая кооперация, независимый `memoryd`, запрет принудительных графов/анкет → P02/P04/P09, проверяется непрерывно с D0/D1 по D9.
- **Самоулучшение `E01–E10`** (`docs/restart/02_CHANGE_EVALUATION.md`): реальные A/B-прогоны, изолированные среды, unseen/regression, запрет самоподмены рубрики, бюджет и rollback → P21, этап **D8** (регрессия D9).
- **00 — Функциональная концепция** (`00_Dennett_Functional_Concept.md`, §§1.1–1.11, §2–§3): конституция среды намерений, автономность, безопасность без кастрации, живая память, человекочитаемость, голос и multi-device → сквозная привязка P01–P23, этапы **D1–D9**.
- **01 — Индекс и общие контракты** (`01_Dennett_Specification_Index_and_Shared_Contracts.md`, §§0–3+): словарь, lowest sufficient layer, ownership, shared envelopes и статусы → P01/P02/P23, базовые контракты с **D0/D1**, полнота **D2–D9**.
- **10 — Memory Fabric** (`10_Dennett_Memory_Fabric.md`):
  - независимый `memoryd`, источники/provenance, базовое чтение/запись/исправление/история, scope-проверки → P04, этап **D1**;
  - дедупликация, admission, сжатие, рабочие topic documents, конфликты, мультимодальные evidence, восстановление индексов, forgetting/экспорт → P05, этап **D3**;
  - standby read-only и полная репликация памяти → P12, этап **D5**.
- **20 — Agentic Control Fabric** (`20_Dennett_Agentic_Control_Fabric.md`):
  - единый Agent, сессии, явный контекст и первый вызов runtime → P02, этап **D1**;
  - durable Tasks/Runs, ожидания, отмены, уточнение, текстовая кооперация и делегирование → P02/P03, этап **D2**;
  - процедуры, навыки, сложные автоматизации → P02/P09, этап **D3**;
  - внешние поручения и effects reconciliation → P18, этап **D7**.
- **30 — Trust, Identity, Autonomy and Permissions** (`30_Dennett_Trust_Identity_Autonomy_and_Permissions.md`):
  - базовая проверка identity/scope/allow/deny вне модели и без двойного исполнения → P08, этап **D1**;
  - действующие grants, отзыв/истечение во время работы, бюджеты и stop → P08/P10, этап **D2**;
  - секреты/device trust/multi-device approvals → P08/P11, этап **D5**;
  - внешние аккаунты, exact recipient approval и prompt-injection defense → P08/P18, этап **D7**;
  - полный цикл ownership recovery → P08/P22, этап **D9**.
- **40 — Voice and Ambient Interaction** (`40_Dennett_Voice_and_Ambient_Interaction_Fabric.md`, Части I–IX): Voice Session, push-to-talk, turn-taking/barge-in, heard-output, realtime/cascade, ambient/multimodal, multi-device privacy и latency → P16/P17, этап **D6** (полная интеграция D9).
- **41 — Capabilities, Providers and Integrations** (`41_Dennett_Capabilities_Providers_and_Integrations.md`):
  - первый native runtime (`agy acp`), совместимость, два auth-профиля, MCP-инструмент памяти → P09, **D0 (probe) / D1**;
  - каталог моделей/квот, skills/MCP/plugins lifecycle, локальные модели, нативные субагенты → P09, этап **D3**;
  - внешние коннекторы и computer-use backends → P09/P18, этап **D7**.
- **50 — Server Runtime, Events, Sync and Portability** (`50_Dennett_Server_Runtime_Events_Sync_and_Portability.md`, Части I–III):
  - постоянные процессы Head/Node/`memoryd` независимо от UI → P01, этап **D1**;
  - durable execution, Event Runtime, Action Inbox, Agent Radar, ordered outbox/cursor → P03/P10, этап **D2**;
  - устройства, pairing, offline staging, Head handoff/failover, streaming replica, backup/restore → P11/P12/P22, этап **D5** (полный release cycle D9).
- **60 — Desktop Application Business Logic** (`60_Dennett_Desktop_Application_Business_Logic.md`, Части I–III+):
  - базовый рабочий чат и обычный артефакт в согласованном визуальном направлении → P13, этап **D1**;
  - Inbox/Radar и управление длительными поручениями → P13/P03, этап **D2**;
  - полный Workbench Shell, гибридные поверхности L, Inspector, редактирование и навигация → P13/P15, этап **D4**;
  - все оставшиеся команды, настройки, глобальный поиск, доступность и clean install → P13/P19, этап **D9**.
- **61 — Mobile Application Business Logic** (`61_Dennett_Mobile_Application_Business_Logic.md`, Части I–III+):
  - доверенный remote, Entry Surfaces, offline capture/drafts, approvals, handoff, native background/biometric → P14/P11, этап **D5**;
  - мобильный голосовой и ambient capture → P14/P16/P17, этап **D6**;
  - полная приёмка всех сценариев и релиз → P14, этап **D9**.
- **70 — End-to-End Validation and Handoff** (`70_Dennett_End_to_End_Validation_and_Architecture_Handoff.md`): Feature Promise Ledger, cross-domain contracts, каталог сквозных сценариев → P23, наращивается на **D1–D8**, обязательное полное закрытие **D9**.
- **Контракт 00 — Shared Cross-Domain Rules** (`contracts/00_Shared_Cross_Domain_Rules.md`, §§12–24): один владелец, разделение events/commands/effects, UNKNOWN-состояние, provenance/trust, archive vs delete, interruption → сквозное правило P01–P23, с **D1** на всех этапах.
- **Контракт A — Ambient Sensory Capture** (`contracts/A_Ambient_Sensory_Capture_Contract.md`, A.1–A.13): локальные ring-буферы аудио, событийный экран, consent, исключения, приватность до загрузки → P17, этап **D6** (retention/cleanup D9).
- **Контракт B — External Communication Operation** (`contracts/B_External_Communication_Operation.md`, B.1–B.13): входящий pipeline, реконструкция контекста, режимы автономности, draft lifecycle, отправка/reconciliation/UNKNOWN, отмена → P18, симуляция **D2**, реальные коннекторы **D7**, сквозной **D9**.
- **Контракт C — Project Lifecycle** (`contracts/C_Project_Lifecycle_Contract.md`, C.1–C.4.9+): create/attach/clone/rebind/worktree/pause/archive/detach/remove → P06, базовый проект **D1**, жизненный цикл и worktree **D2**, перенос **D3**, полное закрытие **D9**.
- **Контракт D — Artifact Lifecycle** (`contracts/D_Artifact_Lifecycle_Contract.md`, D.1–D.13): создание, версии, варианты, approval/finalization, публикация/отзыв, связь с файлами/памятью, удаление → P07, базовый артефакт **D1**, версии/редактирование **D4**, внешняя публикация **D7**, полное закрытие **D9**.
- **Контракт E — Update, Compatibility and Migration** (`contracts/E_Update_Compatibility_and_Migration_Contract.md`, E.1–E.13): Version Manifest, compatibility negotiation, mixed-version, data migrations, adapter isolation → P22, основы версий/миграций с **D0/D1**, резерв **D5**, полные обновления/подписи **D9**.
- **Контракт F — Identity, Key and Ownership Recovery** (`contracts/F_Identity_Key_and_Ownership_Recovery_Contract.md`, F.1–F.13): Recovery Kit, потеря устройства/Head/ключа, compromise recovery, ротация ключей → P08/P22, потеря устройства/failover **D5**, полный recovery цикл **D9**.
- **Контракт G — Resource Pressure and Usage Accounting** (`contracts/G_Resource_Pressure_and_Usage_Accounting_Contract.md`, G.1–G.13): учёт usage, attribution, soft/hard budgets, storage/battery/network/quota pressure, attention budget → P10, базовые бюджеты **D2**, мобильная батарея/сенсорика **D6**, pressure/soak **D9**.
- **Контракт H — Federated Global Search** (`contracts/H_Federated_Global_Search_Contract.md`, H.1–H.13): Searchable Source Contract, fusion/ranking, freshness/authority, privacy filter, offline partial search → P19, поиск памяти **D3**, кросс-доменный и offline поиск **D5/D9**.
- **Контракт I — Locale, Timezone, Language and Travel** (`contracts/I_Locale_Timezone_Language_and_Travel_Contract.md`, I.1–I.13): IANA tz, wall-time vs instant, DST/travel, мультиязычность и перевод памяти/переписки → P19/P03, календарные расписания **D2**, multi-device/travel **D5**, полный охват **D9**.
- **Контракт J — Import, Export and Portable Packages** (`contracts/J_Import_Export_and_Portable_Package_Compatibility_Contract.md`, J.1–J.13): manifest, разделение integrity/trust/permission, path/archive safety, экспорт памяти/capabilities/настроек/артефактов → P20, память/проекты **D3**, перенос между устройствами **D5**, полный набор **D9**.
- **Контракт K — Composite Experience Recipes** (`contracts/K_Composite_Experience_Recipes.md`, K.1–K.13): Idea Incubator, Concept Distiller, Thinking Editor, Daily Briefing, Evening Debrief, AI News Monitor, Research Dossier, Meeting Summary, Taste Review, Red-Team Preset → P20, базовые сценарии на готовых capabilities **D3/D6/D7**, полный пакет готовых recipes **D9**.
- **Контракт L — Presentation and Interaction** (`contracts/L_Presentation_and_Interaction_Contract.md`, L.1–L.10): 5 равноправных путей вывода, WebSurfaceHost, MCP Apps/A2UI/AG-UI, выделение и совместное редактирование, изоляция, сценарии L.9.1–L.9.10 → P15, этап **D4** (расширения multi-device/voice **D5/D6**).
- **Контракт M (ред. 2.0) — Runtime Rollout** (`contracts/M_Runtime_Rollout_Contract.md`, M.1–M.4): Google Antigravity (`agy acp`) первым с двумя профилями входа (`oauth-personal` Google AI Ultra и `gemini-api-key`), отложенные `R-CLAUDE` и `R-CODEX` → P09, **D0/D1–D9**.
- **Сценарии 90 — Integrated End-to-End Scenarios** (`contracts/90_Integrated_End_to_End_Scenarios.md`, §§26–38):
  - §26 Ambient mic on phone, §27 Screen context on PC → **D6**;
  - §28 Incoming Telegram + response, §29 Quick reply mobile, §31 Delete remote repo → **D7** (mobile §29 после D5/D7);
  - §30 Project archive with active Run → **D2**;
  - §32 Transfer project, §38 Global search while offline → **D3/D5/D9**;
  - §33 Artifact generation/approval/publication → **D4/D7**;
  - §34 Mixed-version update, §35 Lost phone, §36 Lost Head and recovery, §37 Critical disk → **D5/D9**.

## 4. V01–V29 — назначение и текущий статус архитектурных сценариев

После реализации этапа к каждой затронутой записи добавляется ссылка на реальный тест, коммит и фактический результат. Сценарии этапов D2–D9 остаются «не реализовано, не проверено».

- V01/V02 — standalone `memoryd` и Head при отказе памяти: **проверено для D1** (`dc5e184`/`2c3d1b4`/`85b8f76`/`30dc620` + H1-hardening на `d1/first-vertical-slice`; `memory_fabric_e2e_integration`, `head_control_e2e_integration`, `desktop_bridge_v03_integration`; ожидает приёмки H1; регрессии D3+).
- V03 — закрытый UI не завершает систему: **проверено для D1** (`85b8f76`/`30dc620` + H1-hardening; `desktop_bridge_v03_integration::v03_closing_desktop_bridge_keeps_daemons_alive_and_restores_conversation_and_artifact`; ожидает приёмки H1; device-повтор D5).
- V04–V06 — агент (`agy acp`) использует память `memoryd`, один native executor, CLI/MCP/UI-паритет и права вне модели: **проверено для D1** (`dc5e184`/`2c3d1b4`/`30dc620` + H1-hardening; `head_live_agy_e2e`, `head_control_e2e_integration`, `memory_fabric_e2e_integration`, `dennettctl` parity; ожидает приёмки H1; расширение и real canary D3).
- V07 — уточнение/отмена общей работы без графа потребителей: D2, не реализовано.
- V08–V14 — read-only reserve, lag/files, transfer, partition, возврат primary, sync timeout и WAL pressure: D5, не реализовано.
- V15–V18 — committed cursor, per-key order, crash после доставки, snapshot/subscription: D2/D5, не реализовано.
- V19 — evidence link с optional ordinal и несколькими anchors: **проверено для D1** (`dc5e184`; `memory_core_pg_integration`, `memory_fabric_e2e_integration`; ожидает приёмки H1; регрессии D3).
- V20–V22 — край retention, незавершённый RecoveryCut, restore после revoke/forgetting: D5/D9, не реализовано.
- V23–V26 — файлы/HTML, выделение, совместное редактирование, несколько UI и устройств: D4/D5/D6, не реализовано.
- V27 — прерывание речи и ambient privacy: D6, не реализовано.
- V28 — плохое самоулучшение не принимается: D8, не реализовано.
- V29 — смена provider session/terms/version без ложного resume: **базовый D1 путь проверен** (`2c3d1b4`/`30dc620`; `head_control_e2e_integration` + `head_live_agy_e2e` rehydrate/mismatch/isolated auth; ожидает приёмки H1; полное расширение D3/D9).

L.9.1–L.9.10 полностью назначены D4; multi-device/voice части допроверяются D5/D6. Их статус не наследуется из одобрения самого контракта L.

## 5. Финальный аудит отсутствующих функций

Перед H9 пройти каждый действующий документ 00–70 и дополнения, а не только названия P01–P23. У каждого обязательного поведения должна быть запись реализации/проверки. Отсутствующее требование добавляется в карту с этапом и тестом, не заменяется общим «покрыто P13». Просмотр скриншота не засчитывается как проверка серверного эффекта.

Финальная запись содержит проверенный установочный пакет, целевые ОС/версии, миграцию/restore, пройденные critical V/L, незапущенные/падающие проверки и принятые ограничения. Демо-ядро можно использовать как пилот раньше, но это не закрывает H9.

## 6. Antigravity-first и граница отложенных адаптеров (R-CLAUDE, R-CODEX)

Применяются [M (ред. 2.0)](../specifications/contracts/M_Runtime_Rollout_Contract.md) и [A01–A12](ANTIGRAVITY_RUNTIME_ACCEPTANCE.md). Текущий V04 проверяет **Google Antigravity (`agy acp`) → `memoryd`** (интерактивный профиль подписки Google AI Ultra + отдельный профиль `gemini-api-key` для будущих фоновых задач). Варианты того же требования на Claude Code (`R-CLAUDE`, см. [CLAUDE_RUNTIME_ACCEPTANCE.md](CLAUDE_RUNTIME_ACCEPTANCE.md)) и Codex (`R-CODEX`) отложены решением владельца от 2026-11-23. Это не fake-прохождение и не skipped обязательный текущий тест. Второй провайдер не обязателен для H3/H9; все остальные обязательные возможности и H0–H9 сохраняются. Список supported означает только действительно проверенные маршруты, а planned — будущие.
