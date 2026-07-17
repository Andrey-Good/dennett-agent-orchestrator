# Dennett Milestone Dependency Map

**Статус:** нормативная карта порядка реализации.  
**Назначение:** связать продуктовый roadmap, Work Packages, risk spikes, тестовые ворота и решения владельца в одну исполняемую последовательность.

Этот файл не заменяет подробные Work Packages. Он отвечает на вопросы:

- почему milestone начинается именно сейчас;
- какие результаты предыдущих этапов он обязан использовать;
- какой минимальный вертикальный путь должен заработать;
- какие решения нельзя отдать агенту без владельца;
- какой уровень автономности допустим;
- по какому доказательству этап считается завершённым.

Подробные правила работы находятся в:

- [`00_IMPLEMENTATION_AND_EVOLUTION_STRATEGY.md`](00_IMPLEMENTATION_AND_EVOLUTION_STRATEGY.md);
- [`01_AGENT_EXECUTION_PROTOCOL.md`](01_AGENT_EXECUTION_PROTOCOL.md);
- [`02_OWNER_PLAYBOOK.md`](02_OWNER_PLAYBOOK.md);
- [`03_WORK_PACKAGE_SYSTEM.md`](03_WORK_PACKAGE_SYSTEM.md);
- [`../testing/TEST_CATALOGUE_AND_QUALITY_GATES.md`](../testing/TEST_CATALOGUE_AND_QUALITY_GATES.md).

---

# 1. Общая зависимость

```text
M00 Repository and Contracts
  ↓
M01 Local Desktop Conversation
  ↓
M02 Project Workspace and Review
  ↓
M03 Managed Runs and Control Surfaces
  ↓
M04 Memory Production Baseline
  ↓
M05 Personal Server and Devices
  ↓
M06 Mobile Trusted Remote
  ↓
M07 Capability Ecosystem
  ↓
M08 Voice and Ambient
  ↓
M09 External Communication and Computer-use
  ↓
M10 Production Hardening
```

Это основной путь, но не полный serial waterfall. После M01 допустимы ограниченные параллельные risk spikes. Реализация milestone начинается только после прохождения его входных gates. Независимые adapters могут разрабатываться параллельно, но новые канонические контракты не должны создаваться в нескольких ветках одновременно.

---

# 2. Общие правила перехода

Каждый milestone проходит пять состояний:

```text
PROPOSED
→ REFINED
→ ACTIVE
→ QUALIFYING
→ ACCEPTED
```

## PROPOSED

Есть цель, но Work Packages ещё крупные и приблизительные.

## REFINED

Ближайшие вертикальные срезы разложены на READY Work Packages; известны тесты, owner decisions и risk spikes.

## ACTIVE

Агенты реализуют пакеты небольшими worktree/PR. Main остаётся зелёным.

## QUALIFYING

Все обязательные пакеты интегрированы; запускаются milestone-level E2E, restore/failure и UX-проверки.

## ACCEPTED

Владелец увидел живой demo path, автоматические gates прошли, а открытые ограничения записаны явно.

Нельзя объявлять milestone завершённым по количеству написанного кода или закрытых задач.

---

# 3. M00 — Repository and Contracts

## Пользовательский результат

Репозиторий можно клонировать, проверить и безопасно изменять несколькими агентами без размывания границ архитектуры.

## Обязательные результаты

- Rust workspace и минимальные core crates компилируются;
- protocol schemas генерируют clients;
- root/nested `AGENTS.md` действуют как локальные инструкции;
- planning и test catalogue проходят validation;
- PR Fast gate работает;
- один fake vertical path доступен без cloud credentials.

## Критические зависимости

Нет. Это foundation для всех следующих этапов.

## Owner Gate

Владелец подтверждает только:

- названия продукта и основных пользовательских сущностей;
- лицензионную стратегию, если репозиторий публикуется;
- допустимый уровень публичности документации.

## Режим GPT-5.6 Sol

- `max`/Pro для изменения фундаментальных контрактов;
- `high`/`xhigh` для skeleton и CI;
- `medium` для механического заполнения manifests.

## Автономность

Высокая внутри READY Work Packages. Низкая для изменения repository topology и canonical names.

## Exit Gate

- clean clone проходит repository, docs, planning, Rust и TypeScript checks;
- generated files воспроизводимы;
- один кодовый путь исполняется end-to-end;
- нет прямого доступа UI к storage/provider internals.

---

# 4. M01 — Local Desktop Conversation

## Пользовательский результат

Пользователь открывает desktop Dennett, создаёт локальный проектный разговор, получает streaming-ответ агента и после перезапуска видит сохранённую session.

## Минимальный вертикальный срез

```text
Tauri UI
→ dennett-node
→ embedded Head
→ fake/one real runtime
→ Result Envelope
→ basic Memory Event
→ WatchDelta
→ UI
```

## Зависимости

M00 accepted.

## Риск-spikes

- Tauri ↔ daemon lifecycle;
- local gRPC/Named Pipe/UDS;
- stream resync;
- one real provider adapter continuation.

## Owner Gate

Владелец проверяет:

- ощущается ли проектный чат как прямой Codex/Claude-like workflow;
- понятны ли Stop, provider/model и project scope;
- не мешает ли инфраструктура обычному разговору.

## Автономность

Агент может самостоятельно довести отдельный slice до PR. UI/UX решения, меняющие основной путь, требуют короткого demo-review.

## Exit Gate

- окно можно закрыть без остановки Node;
- session восстанавливается;
- cancel не теряется;
- provider timeout видим;
- stream gap вызывает resync;
- trace соединяет user turn, runtime и memory event.

---

# 5. M02 — Project Workspace and Review

## Пользовательский результат

Агент работает в реальной папке/репозитории, меняет файлы, запускает проверки, а пользователь видит diff, tests, artifacts и checkpoints рядом с чатом.

## Зависимости

M01 accepted.

## Риск-spikes

- worktree lifecycle;
- project instruction resolution;
- filesystem authority и external changes;
- diff/checkpoint recovery.

## Owner Gate

Владелец проверяет один реальный маленький проект:

- удобно ли дать цель;
- понятно ли, что агент изменил;
- можно ли быстро отклонить/попросить исправить;
- не требуется ли читать технические логи ради обычного решения.

## Exit Gate

- изменения привязаны к конкретному workspace snapshot;
- tests относятся к конкретной версии;
- diff можно принять, исправить или откатить;
- project chat остаётся главным путем;
- parallel work не пишет в один mutable workspace без ownership.

---

# 6. M03 — Managed Runs and Control Surfaces

## Пользовательский результат

Долгая задача может уйти в background, пережить restart, быть остановлена и вернуться в Action Inbox/Radar с понятным результатом.

## Зависимости

M02 accepted.

## Риск-spikes

- checkpoint/restart;
- structured cancellation;
- unknown effect dummy connector;
- Inbox revision race;
- Radar projection rebuild.

## Owner Gate

Владелец определяет желаемую частоту вопросов и default execution profile; не проектирует state machine.

## Exit Gate

- Direct Turn не превращается в Task без причины;
- Managed Run переживает Head restart;
- Stop/Pause/Resume имеют честную семантику;
- Inbox и notifications не дублируют друг друга;
- UNKNOWN external effect не повторяется.

---

# 7. M04 — Memory Production Baseline

## Пользовательский результат

Dennett помнит проект и пользователя между сессиями, находит доказательства, показывает источник, позволяет исправить и удалить память.

## Зависимости

M03 accepted; storage/protocol contracts из тома 81 стабильны.

## Риск-spikes

- embedded/service parity;
- PostgreSQL/SQLite/object store path;
- realistic retrieval benchmark;
- deletion propagation;
- project memory pack round-trip;
- index rebuild.

## Owner Gate

Владелец проверяет реальные вопросы:

- вспомнил ли Dennett нужное;
- не вытащил ли лишнее или личное;
- можно ли понять, почему ответ такой;
- исправление действительно влияет на будущее поведение.

## Exit Gate

- одна логическая Memory Fabric работает embedded и service mode;
- canonical ingest не зависит от vector index;
- exact + lexical + semantic baseline измерен;
- correction и deletion проходят E2E;
- project memory переносима;
- restore возвращает память и evidence.

---

# 8. M05 — Personal Server and Devices

## Пользовательский результат

Dennett продолжает фоновые процессы на personal server, а ПК/ноутбук подключаются как local-capable nodes, работают offline и синхронизируются без второй конкурирующей памяти.

## Зависимости

M04 accepted.

## Риск-spikes

- device pairing и mTLS;
- Tailscale/Headscale optional transport;
- offline operation log;
- opt-in Head eligibility;
- Authority Epoch/fencing;
- large object transfer;
- split-brain emergency behavior.

## Owner Gate

Владелец явно выбирает устройства с `head_eligibility = emergency/full` и проверяет recovery instructions.

## Exit Gate

- обычное устройство не становится Head автоматически;
- offline edits не создают вторую canonical memory;
- permissions revalidate после reconnect;
- stale Head fenced;
- server loss имеет понятный degraded path;
- backup/restore хотя бы одного real profile доказан.

---

# 9. M06 — Mobile Trusted Remote

## Пользовательский результат

С телефона можно быстро увидеть существенное, захватить мысль, дать разрешение, управлять Run, поговорить с Dennett и вернуться после прерывания без потери контекста.

## Зависимости

M05 accepted.

## Риск-spikes

- React Native ↔ native Mobile Node;
- background execution ограничения;
- biometric step-up;
- offline command queue;
- widgets/notifications/deep links;
- voice/capture process death recovery.

## Owner Gate

Владелец проводит on-the-go acceptance: одна рука, короткие паузы, плохая сеть, блокировка экрана.

## Exit Gate

- capture сначала сохраняется локально;
- notification action идемпотентен;
- Resume Capsule восстанавливает намерение;
- сложная работа передаётся на desktop;
- phone является approval remote, но не master authority;
- battery/network usage измерены.

---

# 10. M07 — Capability Ecosystem

## Пользовательский результат

Можно подключать providers, local models, skills, MCP, plugins и connectors без изменения ядра и без загрузки всего каталога в каждый prompt.

## Зависимости

M03 accepted; M04 желательно accepted для истории качества; M05 нужен для device-bound capabilities.

## Риск-spikes

- Codex/Claude continuity;
- Python/Node adapter host packaging;
- MCP dynamic schema changes;
- local model tool calling;
- capability discovery/security;
- skill comparison/delta extraction.

## Owner Gate

Владелец определяет только policy автоматического discovery/install/promotion и вручную выбирает важные providers.

## Exit Gate

- новый adapter добавляется через port + descriptor + conformance suite;
- provider-native возможности не flatten;
- capability health/fallback видимы;
- project capability set минимален;
- executable third-party components изолированы;
- user-owned skill не переписывается молча.

---

# 11. M08 — Voice and Ambient

## Пользовательский результат

Dennett поддерживает быстрый голосовой разговор, сильный deliberative sidecar, постоянный локальный microphone path и событийный screen capture с понятными privacy-controls.

## Зависимости

M04, M05, M06 и базовая часть M07 accepted.

## Риск-spikes

- realtime vs chained voice;
- fast voice ↔ strong model bridge;
- barge-in/heard-output history;
- local VAD/wake/ring buffer;
- Android/iOS background restrictions;
- Screenpipe vs native capture;
- privacy/redaction/storage pressure.

## Owner Gate

Владелец определяет:

- ambient defaults;
- raw retention;
- public-place behavior;
- acceptable proactivity;
- voice personality/verbosity.

## Exit Gate

- stop/mute работает локально;
- partial transcript не вызывает dangerous action;
- late sidecar result не вклинивается в старый turn;
- microphone/screen не отправляют постоянный мусор в cloud;
- source can be provably disabled;
- capture связывается с проектом и памятью;
- resource usage остаётся в budget.

---

# 12. M09 — External Communication and Computer-use

## Пользовательский результат

Dennett может подготовить и при разрешении отправить сообщение, а также безопасно выполнить bounded GUI/browser action с точным target, evidence и takeover.

## Зависимости

M03, M05, M07 accepted; M08 для voice invocation.

## Риск-spikes

- Telegram/email unknown-result reconciliation;
- direct API vs MCP/native connector;
- Playwright/DevTools vs visual computer-use;
- user takeover;
- privacy localization;
- idempotency and delivery receipts.

## Owner Gate

Владелец утверждает preauthorized communication/effect patterns и испытывает critical approvals на реальных, но низкорисковых действиях.

## Exit Gate

- content/style/disclosure/send разделены;
- exact recipient и effect видны до исполнения;
- timeout не дублирует отправку;
- structured route предпочитается visual route;
- computer-use имеет bounded session и verification;
- user takeover немедленно прекращает agent input.

---

# 13. M10 — Production Hardening

## Пользовательский результат

Установка может безопасно обновляться, восстанавливаться и работать длительно без ручной поддержки разработчика.

## Зависимости

Все предыдущие milestone accepted на требуемом release scope.

## Обязательные работы

- signed update pipeline;
- protocol/schema migration;
- backup/restore drills;
- supply-chain provenance/SBOM;
- deterministic fault simulation;
- soak и storage-pressure tests;
- support bundle/doctor;
- release channels;
- privacy/security review;
- runbooks;
- performance and cost baselines.

## Owner Gate

Владелец участвует в:

- recovery drill;
- upgrade/rollback acceptance;
- privacy-default review;
- release candidate demo;
- окончательном go/no-go.

## Exit Gate

- clean installation и upgrade работают на поддерживаемых профилях;
- backup восстанавливается в изолированной среде;
- critical failure scenarios проходят;
- release можно откатить или forward-fix по документированному пути;
- no critical open findings;
- пользовательские данные экспортируемы;
- support/diagnostics не раскрывают private content.

---

# 14. Rolling-wave planning

Полный проект нельзя надёжно превратить в сотни неизменяемых задач в начале: реальная реализация даст новые данные о производительности, SDK и ограничениях ОС.

Используется rolling-wave планирование:

- активный milestone — Work Packages детализированы полностью;
- следующий milestone — известны slices, spikes и gates;
- следующие два — известны outcomes и major dependencies;
- дальние milestone — сохраняют только цель и архитектурные инварианты.

После каждого accepted milestone:

1. обновляется evidence;
2. пересматриваются риски;
3. следующий milestone переводится в REFINED;
4. лишние planned mechanisms удаляются;
5. новые требования получают owner decision или ADR;
6. roadmap меняется только с сохранением пользовательской цели.

---

# 15. Autonomous batch policy по этапам

| Этап | Допустимый автономный горизонт | Что обязательно останавливает batch |
|---|---:|---|
| M00–M01 | 1–4 часа | contract/topology/UX primary path decision |
| M02–M03 | 2–8 часов | data loss, public contract, ambiguous completion/effect |
| M04–M05 | 1–6 часов | migration, deletion, authority, crypto, split-brain decision |
| M06 | 2–6 часов | mobile UX choice, OS permission/privacy limitation |
| M07 | 2–12 часов | new trust scope, provider lock-in, executable third party |
| M08 | 1–6 часов | retention/consent, always-on behavior, dangerous voice path |
| M09 | 1–4 часа | recipient/effect policy, unknown effect, privacy ambiguity |
| M10 | 1–8 часов | release/recovery/security go/no-go |

Горизонт не является обещанием wall-clock времени. Он задаёт размер batch и частоту checkpoints. Агент завершает batch раньше, если выполнен outcome или сработал stop condition.

---

# 16. Финальная формула

> **Dennett реализуется как последовательность работающих вертикальных срезов. Каждый milestone заканчивается не слоем инфраструктуры, а наблюдаемым пользовательским результатом, подтверждённым автоматическими тестами, failure-path и коротким owner acceptance. Подробно планируется только ближайшая работа; архитектурные инварианты остаются стабильными, а конкретные инструменты и adapters могут заменяться по мере развития экосистемы.**
