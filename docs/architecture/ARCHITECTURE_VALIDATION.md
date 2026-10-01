# Согласованность архитектуры 2.0: требования, стыки и проверки

Дата: 2026-09-26. Исходный commit: `eccfb596a3f8b178cebf4be12ba76f8cab40df67`. Этот отчёт фиксирует **документальную проверку**, а не запуск приложения и не формальное доказательство отсутствия всех ошибок.

## 1. Объём и метод

В предшествующем полном проходе этого диалога прочитаны 4 архитектурных тома, 11 бизнес-документов, общие supplements, ADR-001–006 и текстовые owner-direction. В этой редактуре использован тот же зафиксированный снимок; SHA исходных архитектурных томов повторно проверены через GitHub. Перечитаны изменяемые границы и нормативные решения F/E/L. Все новые тома сверены между собой и с картой требований ниже; выбранные внешние механизмы перепроверены по AR01–AR20.

Работа выполнялась в порядке: сформировать критерии и список конфликтов → составить план по владельцам → перенести только согласованные решения → проверить нормальные и отказные сценарии → проверить ссылки/индексы/статусы → собрать один documentation commit. Детальные локальные планы сохранены в комплекте сдачи; они не являются новыми продуктовым workflow.

Весь исходный код, архивные иллюстрации, каждая ссылка старой библиографии и every production claim не проверялись заново. Полный checkout не получен из-за сетевого/DNS ограничения контейнера. Старый архив сохраняется exact Git blobs, а не пересказом. Новые архитектурные тома сокращают дублирование бизнес-описаний и исторических альтернатив; исходные пользовательские возможности не удаляются.

## 2. Кто владеет правилом

80 — процессы, deployment, authority/fencing. 81 — canonical data, transactions, delivery cursors, memory service contract, replication/recovery. 82 — native harness integration, context/tools/voice и eval execution. 83 — UI/native client lifecycle, surface adapters, packaging и tests.

F/E/L и бизнес-контракты определяют смысл. Архитектура определяет способ исполнения, не меняя молча значения Stop, Unknown, Delete, project archive или sender approval. Current status определён в restart/README; этот отчёт не создаёт конкурирующего нормативного приоритета.

## 3. Трассировка предметной бизнес-логики

### 00. Назначение и функциональная концепция

Постоянная операционная среда, проекты, прямые агенты, голос, инструменты и несколько устройств: 80 §§1–5, 9–12; 82 §§1–2, 9–15; 83 §§1–6. Closed window не завершает принятые задачи; provider/harness не заменяет Agent Dennett. Нового отдельного продукта «восстановить старый проект» нет.

### 01. Общие контракты и владельцы

Authority/identity/scope, команды, provenance и наблюдаемое состояние: 80 §§3, 6–9; 81 §§2, 6–9; 82 §§3, 7–11; 83 §§1, 6–8. Смысловое задание текстовое; machine schema относится к transport/доступу/данным. Старые illustrative agent-envelope поля не становятся обязательной анкетой модели.

### 10. Memory Fabric

Sources/events, notes/dossiers, claims/current/history, admission/dedup, processing/consolidation, retrieval lanes, scope, correction/deletion, portable packs и evaluation: 81 §§2–6, 12–14; 82 §9 и §16. Memory отдельна от Head и сохраняет full semantics. Индексы не объявлены источником истины. Отказ processing не теряет оригинал. Нет нового требования сначала экспериментально перепроектировать память до архитектуры.

### 20. Agentic Control

Один вид участника, Agent/Task/Run/Session различены, dynamic plan, native delegation, steering, significant waits, budgets, completion: 80 §§1, 3, 10; 82 §§1–3, 10–11, 16. Связи работы хранятся текстом. Техническая отмена конкретного процесса/его child tool не порождает собственный consumer graph. Передача поручения и уточнения используются нативно; global coordinator не вызывается на каждое слово project chat.

### 30. Trust, identity и автономность

Actual grants, exact side-effect parameters, revoke/stop, native hook limits, scoped secrets, auth recovery: 80 §§6–8, 11; 81 §§6–7, 9, 11–13; 82 §§4–8, 13; 83 §§3, 7–11. Промпт не выдаёт права. Native event «tool started» не выдаётся за pre-execution gate. Shared UI-state и CLI agent-id не являются principal proof.

### 40. Voice и ambient

Committed input, heard output, interruption, optional slow sidecar, capture policy, multiple devices: 82 §§14–15; 80 §§9, 12; 83 §§3, 6–7. Голос ссылается на выбор панели без полного DOM/всей памяти. Late answer не озвучивается по устаревшему turn. Mobile shutdown не объявляется услышанным ответом.

### 41. Capabilities/providers/integrations

Native-specific capabilities, connection/model/runtime/skill distinction, manual vs discovered, ownership/fork, lazy activation, health/quota, update: 82 §§3–9, 12–13; 83 §§4–7, 9–11. CLI и SDK — интерфейсы, не идеология. Полезный host допускается; forwarding-only не обязателен. Native login управляет своим credential, not token exfiltration.

### 50. Server/runtime/sync

One Head, local utility, events, operational state, failover, portability и backup: 80 §§3–13; 81 §§7–13. Client operations не заменяют native PG standby replication. Full candidate имеет данные и runnable stack, а не только кэш. Нет ложной гарантии «сеть пропала — значит можно автоматически получить власть».

### 60. Desktop

Project chat, files/diff, Inbox, Radar, selection/context, command center, accessibility, independent backgrounds, UI additions: 83 §§1–2, 4–9, 12–13. UI через application layer; память не обходится. Детальный каталог меню остаётся в бизнес-документе. Owner-direction v5 для native Mica не заменена самодельной проекцией wallpaper. Radar graph — представление известных данных, не новый движок управления зависимостями.

### 61. Mobile

Glance/capture/decide/continue, native background, offline drafts, notification idempotency, biometric approvals, privacy и handoff: 83 §§1, 3, 6–10, 13; 81 §9. Local data не требует server PG на телефоне и не гарантирует full Head. Возможность открыть весь разрешённый corpus отделена от непрерывного серверного исполнения.

### 70. E2E и handoff

Normal/failure/recovery/observability paths: 80 §14; 81 §14; 82 §17; 83 §§13–15 и сценарии ниже. Исторический gap ledger в 70 не возвращён как незакрытые бизнес-блокеры. Прототипы не выполняются в documentation phase; обязательные live/conformance проверки остаются до заявленной поддержки.

## 4. Трассировка supplements и новых норм

**Shared 00:** раздельные источники истины, observable outcomes и exact effect — 80 §6, 81 §§7–8, 82 §§8, 11, 13.

**A Ambient:** локальные buffers, source exclusions, timestamps, sensor commit и pressure — 82 §15, 80 §§11–12, 83 §§3, 12–13.

**B Communication:** incoming thread/account context, prepare/send, permissions, UNKNOWN/receipt — 82 §13 и 81 §§7–8. Retry callback не повторяет вслепую отправку.

**C Project:** registration/path/archive/remove/delete/worktree distinction — 82 §§2, 10–13 и 83 §2; файл остаётся filesystem-owned. Удаление агента не удаляет автоматически все общие результаты по новой кодовой топологии.

**D Artifact:** stable content/version, editing, representations/publication/deletion — 81 §§2–3, 7, 11–13; 83 §§4, 6–7. Закрытие панели не удаляет результат; новый render не перезаписывает пользовательский исходник.

**E Updates:** signed packages, version negotiation, migrations/recovery — 80 §13, 81 §13, 83 §§9–11. PG standby migrations не запускаются второй раз.

**F Identity recovery:** отдельно данные, ключи, provider login, device revoke и Head authority — 80 §§5–8; 81 §§10–12; 83 §§3, 10–11. Наличие backup не делает доступным недоступный ключ.

**G Resources:** bounded CPU/RAM/VRAM/disk/usage, native unit estimates, WAL pressure — 80 §11; 81 §§10, 12, 14; 82 §§6, 11, 16; 83 §12.

**H Search:** federated/scoped/current/historical/partial — 81 §5 и 83 §2. Неполный индекс не скрывает recent source; простая таблица поиска не подменяет весь Memory contract.

**I Time/locale:** IANA, wall-time/instant, travel/DST, late events — 80 §10; 81 §§2, 9; 82 §§11, 14; 83 §§2–3.

**J Portability:** version manifests, permitted scope, staged validation, no credential/instruction trust by hash — 81 §13; 83 §§4, 9–11.

**K Recipes:** creative/research/briefing patterns остаются skills/prompts поверх tools и schedules — 80 §10, 82 §§7, 10, 12. Нет отдельных IdeaIncubator/Briefing микросервисов.

**L Presentation:** все пять путей вывода, обратный выбор, свои исходники, native trusted shell, fallback — 83 §§4–8. A2UI/MCP Apps/AG-UI не заменяют agent identity, Task database или Memory.

**90 Integrated scenarios:** сохранены с распределением по 80–83; исходные «closed at business level» не равны пройденным runtime tests.

**F01–F10:** один Agent, текстовые связи, гибкая стратегия, самостоятельные обязанности и code/prompt distinction — 80 §§1–6, 82 §§1–3, 7–11. Никакой обязательной анкеты/собственного графа/stepper не добавлено.

**E01–E10:** actual paired trials, protected criteria, unseen cases, proportional budgets, apply/rollback — 82 §16 и 83 §13. Изоляция относится к mutable env, shared immutable fixtures допустимы. Судья и автор не получают возможность подменить свой тест.

## 5. Какие противоречия разрешены, а не замаскированы

**Memory in-process vs independent:** старые 80 §0.4, 81 embedded parity и ADR-002/005 заменены. Новое правило — отдельный memoryd во всех полноценных конфигурациях, API/auth/processing без Head.

**Canonical SQLite vs full standby:** серверный SQLite adapter и матрица server parity удалены из новой архитектуры. Локальное клиентское хранилище сохранено. Старые «все devices синхронизируются только operations» не распространяются на полные физические PostgreSQL-резервы.

**SDK forwarding vs full Dennett agent:** исключён только ненужный промежуточный процесс. Agent/Session/context/memory/grant слой остаётся. Встроенные tools не выполняются повторно через наш broker; наши tools не исполняются ещё раз native harness.

**Text coordination vs old typed envelopes/cancellation graphs:** новая архитектура и F04 заменяют интерпретацию старых примеров 20/70/82 как обязательной смысловой модели. Допустимы transport identity и actual process resource cancellation, не автоматическая отмена по custom registry of consumers.

**Native model settings vs single agent identity:** RuntimeBinding не новая Agent-сущность; смена модели не обнуляет задачи. User-selected direct model не меняется молча, внутренний автоматический выбор действует лишь в согласованных пределах.

**Independent Memory vs auth/scheduler coupling:** собственный verifier и memory jobs не требуют Head. Fresh global policy имеет expiry/fail-closed границы; standalone auth не превращается в обход интегрированных прав. Memory reads на standby не пишут в readonly БД.

**Outbox statement vs implementation:** SKIP LOCKED дополнен eligibility earliest key, lease generation и consumer dedup. Sequence allocation заменена committed counter в узком потоке. Snapshot+cursor читаются согласованно; epoch/retention mismatch ведёт к resync.

**Nullable ordinal in PK:** ключом стал link_id; nullable ordinal остаётся только позицией. Повтор операции сохраняет ID, multiple anchors не запрещены общей pair uniqueness.

**30-day PITR vs two weekly full chains:** time retention сохраняет подходящую base/WAL; complete RecoveryCut учитывает объекты/конфигурацию/ключи. DB five-minute RPO не выдаётся за full-system RPO.

**UI shared state vs canonical state:** общий host/adapters не создают вторую Task/Memory/permission систему. Selection привязан к версии и моменту вопроса, не всему DOM. Native Stop/confirmation не отданы недоверенному HTML.

**No hidden code authorization:** новый том83 содержит архитектуру проверок, не окончательную систему задач разработки. Старые milestones/CI/source code не считаются реализованной архитектурой и не получают разрешение на запуск.

## 6. Сценарная проверка на уровне документов

Для каждого случая ниже проверены наличие ответственного, данные, нормальный путь, отказ и восстановление в текущих томах. Статус всех случаев — **design walkthrough, не исполненный тест**.

**V01. memoryd без Head.** Внешний клиент пишет и читает заметку, запускает разрешённую обработку; auth и jobs независимы. 80 §3.3; 81 §§4–6.

**V02. Head без memoryd.** Stop/результаты работают, MemoryCommit остаётся в durable outbox; ложной памяти нет. 80 §11; 81 §7.

**V03. UI закрыт.** Node/Head/memoryd продолжают разрешённую работу, процесс не зависит от окна. 80 §12; 83 §§1, 9.

**V04. Claude читает память Dennett (текущий выпуск).** По решению 2026-09-27 и [M](../specifications/contracts/M_Runtime_Rollout_Contract.md) первой проверяется Claude SDK/CLI-интеграция; Codex-вариант сохранён для R-CODEX и не блокирует D0–D9. Это обновление сценария, не выполненный runtime-тест. Собственный Agent+контекст сохранены; tool memory вызывает memoryd; UI не шлёт prompt напрямую мимо Dennett. 82 §§1, 5, 7–9.

**V05. Native tool получает approval.** Исполняет только native runtime, broker не запускает вторую копию. Hook coverage и внешние ограничения имеют тест. 82 §§4–5, 8.

**V06. Наш CLI/MCP инструмент.** Одинаковая domain implementation, identity по credential, аргумент --agent не выдаёт права. 82 §7.

**V07. Изменение поручения/отмена общего результата.** Агент пересматривает текст и сообщает исполнителям; служебный stop не вычисляет custom graph. 82 §§10–11.

**V08. Standby читает память и пользователь пишет offline draft.** PostgreSQL replica read-only; draft сохраняется отдельно; read-audit не ломает запрос попыткой SQL write. 81 §§5–6, 9–10.

**V09. Полный резерв догнал SQL, но не файл.** Readiness частичная; нет заявления полной защищённости. 80 §5.3; 81 §10.

**V10. Плановый перенос на ПК.** Срез/bytes/keys, fencing, promotion, authority, restart/reconcile; сервер возвращается резервом. 80 §8.

**V11. Сеть разорвана, оба узла живы.** Без доказанного fencing новый full Head не самоназначается; локальная работа остаётся ограниченной. 80 §7.

**V12. Старый primary вернулся.** Сначала защита от активного dispatch, потом compatible rewind или re-seed; без merge SQL. 80 §8.3; 81 §13.

**V13. Асинхронный lag и отсутствующий sync standby.** Различены durable acknowledgement и полная RPO; timeout не доказывает rollback. 81 §10.

**V14. WAL slot заполняет диск.** Bounded retention, warning, stale/reseed вместо разрушения primary. 80 §11; 81 §10.

**V15. Меньший номер задержал commit.** Ordinary transactional counter удерживает следующий номер до фиксации; max(sequence) не cursor. 81 §8.1.

**V16. Два workers одного ordering key.** Следующий item не обгоняет занятый; другие keys параллельны. Lease generation не разрешает stale ack. 81 §8.2.

**V17. Crash после доставки до ack.** Идемпотентный consumer/effect reconciliation предотвращает повтор, не обещается exactly-once сеть. 81 §§7–8; 82 §13.

**V18. Snapshot/subscription, доступ и retention.** Согласованный snapshot/cursor, hidden entries не утечка и не ложный gap, expired cursor требует resync. 81 §8.3; 83 §8.

**V19. Evidence link без позиции и с несколькими anchors.** Независимый stable ID, nullable ordinal вне ключа, повтор operation id проверяет fingerprint. 81 §3.3.

**V20. Восстановление у границы 30 дней.** Есть подходящая retained base/WAL и objects нужного cut. Две новые полные копии не удаляют необходимую старую только по количеству. 81 §§11–12.

**V21. Coordinator cut упал или внешняя папка меняется.** Незавершённый срез не healthy; bounded barrier снимается управляемо; filesystem snapshot проверяется отдельно. 81 §11.

**V22. Restore после forgetting/revoke.** Изолированный запуск, актуальные ограничения, no live dispatch; residual backup limitation видима. 81 §12; 83 §10.

**V23. Python→PNG и новый HTML.** Обычный файл работает без протоколов; свободная графика не требует нового plugin. 83 §4.

**V24. Выделение A, вопрос, затем B.** Сообщение остаётся привязанным к A/version; UI не заявляет magic DOM access. 83 §6.

**V25. Пользователь редактирует, агент стримит, web surface падает.** Нет silent overwrite; shell/Stop сохраняются; reopen не повторяет внешнее действие. 83 §§6–7.

**V26. Два UI-протокола/два устройства.** Нет второго canonical state; адресат выбора/контекст точны, fallback честный. 83 §§3–6.

**V27. Прерывание речи/ambient privacy.** Сохраняется услышанное, не весь generated TTS; raw не уходит в cloud до policy. 82 §§14–15.

**V28. Улучшатель подгоняет prompt под инцидент.** Candidate/criteria разделены; actual old/new attempts независимы; проверяются unseen/regression/budget; можно отклонить/insufficient. 82 §16.

**V29. Provider session/terms изменились.** Capability version/auth route проверены; нет hidden switch, обхода credentials или неподдержанного resume. 82 §§3–6, 11–12.

## 7. Реально выполненное и невыполненное

Выполнены подготовка согласованной редакции, сверка входящих решений, walkthrough V01–V29, проверка ссылок/section references затронутого комплекта, поиск старых конфликтных терминов с ручной оценкой контекста, генерация navigation текущих архитектурных Markdown штатной функцией. Публикуемый состав проверяется по Git tree и blob SHA; старые тома архивируются по неизменным blob SHA.

Не выполнены actual PostgreSQL schema migration/replication/backup restore, native runtime/UI запуск, runtime security tests, нагрузочные испытания, полный repository CI и генерация всех root/docs manifests/checksums. Независимая SQLite-проверка предыдущего аудита была проверкой семантики СУБД, не испытанием приложения; она не выдаётся за текущий PostgreSQL test.

Полный checkout недоступен; старые metadata остаются отмеченным техническим долгом, не вручную подделанными результатами. Перед интеграцией нужен штатный regenerate и --check на полном дереве. Архивные относительные ссылки сохранены исторически; их полная перезапись не утверждается. Старые ссылки на номера разделов v1 нужно читать в архивном snapshot, а не считать совпадающими с v2.

В проверенном актуальном архитектурном комплекте не оставлены известные конкурирующие решения по согласованным вопросам. Это **не гарантия**, что совместимость внешних средств или реализация не выявят новых проблем. Для рискованных границ указан тест и безопасный отказ/fallback; незнание не маскируется обещанием абсолютной реализуемости.
