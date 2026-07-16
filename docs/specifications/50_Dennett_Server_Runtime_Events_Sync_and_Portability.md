# Dennett Server Runtime, Events, Sync and Portability

> **Repository edition · 2026-07-13 · `50`**  
> Это самостоятельный канонический документ репозитория Dennett. Начните с [карты документации](../README.md).  
> Related: [40_Dennett_Voice_and_Ambient_Interaction_Fabric.md](./40_Dennett_Voice_and_Ambient_Interaction_Fabric.md) · [41_Dennett_Capabilities_Providers_and_Integrations.md](./41_Dennett_Capabilities_Providers_and_Integrations.md).

## Интегрированные contract supplements

Следующие небольшие нормативные документы выделены из предархитектурного gap-аудита. Они являются частью текущего набора и обязательны для изменений, пересекающих указанные границы:

- [`A_Ambient_Sensory_Capture_Contract.md`](contracts/A_Ambient_Sensory_Capture_Contract.md)
- [`B_External_Communication_Operation.md`](contracts/B_External_Communication_Operation.md)
- [`C_Project_Lifecycle_Contract.md`](contracts/C_Project_Lifecycle_Contract.md)
- [`D_Artifact_Lifecycle_Contract.md`](contracts/D_Artifact_Lifecycle_Contract.md)
- [`E_Update_Compatibility_and_Migration_Contract.md`](contracts/E_Update_Compatibility_and_Migration_Contract.md)
- [`F_Identity_Key_and_Ownership_Recovery_Contract.md`](contracts/F_Identity_Key_and_Ownership_Recovery_Contract.md)
- [`G_Resource_Pressure_and_Usage_Accounting_Contract.md`](contracts/G_Resource_Pressure_and_Usage_Accounting_Contract.md)
- [`H_Federated_Global_Search_Contract.md`](contracts/H_Federated_Global_Search_Contract.md)
- [`I_Locale_Timezone_Language_and_Travel_Contract.md`](contracts/I_Locale_Timezone_Language_and_Travel_Contract.md)
- [`J_Import_Export_and_Portable_Package_Compatibility_Contract.md`](contracts/J_Import_Export_and_Portable_Package_Compatibility_Contract.md)

Эти supplements не дают одному lifecycle разойтись по нескольким большим файлам; canonical owner указан в заголовке каждого документа.


**Полная бизнес-логика постоянного control plane, распределённого исполнения, событий, синхронизации устройств, восстановления, резервного копирования и переносимости**

**Версия:** 1.0  
**Дата исследования:** 11 июля 2026 года  
**Статус:** канонический baseline бизнес-логики до выбора программной архитектуры и технологического стека.  
**Каноническое имя:** `50_Dennett_Server_Runtime_Events_Sync_and_Portability.md`.

Этот документ является самостоятельным. Для понимания основной модели не требуется знать историю обсуждения или читать предыдущие версии спецификаций. Он исходит из следующего определения продукта.

**Dennett** — персональная агентная операционная система. Пользователь работает с проектными агентами в папках и репозиториях, общается с постоянным главным оркестратором, использует голосовой режим, подключает внешние модели и инструменты, ведёт долговременную память, получает проактивную помощь и управляет системой с нескольких устройств. Dennett должен продолжать полезную работу при временной недоступности сети или основного сервера, но не должен создавать опасную иллюзию согласованности там, где устройства разошлись по состоянию.

Документ определяет, **что обязан делать серверный и распределённый runtime Dennett, как он связывает уже определённые подсистемы и какие гарантии предоставляет**. Он не выбирает язык программирования, базу данных, очередь сообщений, workflow engine, контейнерную платформу или UI-фреймворк. Логические компоненты этого документа не обязаны становиться отдельными микросервисами: первая практичная реализация может быть одним хорошо модульным приложением и несколькими локальными device agents.

Документ продолжает следующие канонические спецификации:

- функциональную концепцию Dennett — зачем существует продукт и какие пользовательские возможности обязательны; [[S01]]
- Dennett Specification Index — границы ответственности документов и общие контракты; [[S02]]
- Memory Fabric — каноническую память, доказательства, offline event log, project memory и синхронизацию памяти; [[S03]]
- Pragmatic Agentic Control Fabric — проектные сессии, Task, Run, уровни долговечности и работу агентов; [[S04]]
- Trust Fabric — identity, device trust, permissions, external effects и safety floor; [[S05]]
- Capability Fabric — providers, модели, tools, skills, MCP, connectors и локальные backends. [[S06]]

---

# Часть I. Назначение, границы и выбранная модель

## 0. Итоговое решение

### 0.1. Сервер Dennett — персональный control plane, а не единственное место, где «живёт» вся система

Лучшее решение для Dennett — **Personal Control Plane with Local-Capable Nodes**:

- один логически активный **Head Runtime** координирует систему;
- сервер хранит долговечное глобальное состояние, принимает события, планирует фоновую работу, ведёт authoritative registry активных процессов и связывает подсистемы;
- компьютеры, ноутбуки и телефоны являются не тупыми терминалами, а доверенными или ограниченными **узлами Dennett** с локальным кэшем, собственными capabilities, сенсорными источниками, журналом offline-операций и возможностью выполнять работу;
- data plane не обязан всегда проходить через сервер: большой файл, локальная модель, экран или рабочая папка могут обрабатываться на устройстве, где они находятся;
- control plane остаётся централизованным по смыслу, чтобы permissions, external effects, Task state, Action Inbox и глобальные решения не расходились между устройствами;
- при недоступности Head Runtime устройства продолжают разрешённую локальную работу, но не притворяются, что обладают свежей глобальной властью;
- доверенное устройство может стать временным Head Runtime по явной и проверяемой процедуре.

Это не классическая cloud-only архитектура и не полностью peer-to-peer multi-master система. Local-first исследования справедливо подчёркивают важность offline-работы, владения данными и отсутствия обязательной зависимости от чужого облака. Но Dennett также выполняет внешние действия, управляет разрешениями и запускает долгоживущие процессы — для этого нужен однозначный координационный центр. [[S20]]

Полезной аналогией является разделение control plane и data plane в Tailscale: централизованный координатор распространяет identity и policy, а основной трафик идёт непосредственно между узлами, не превращая сервер в узкое место. Dennett не копирует Tailscale, но принимает тот же принцип: **централизовать решения, которым нужна единая власть, и не централизовать тяжёлые данные без необходимости**. [[S19]]

### 0.2. Один активный Head Runtime по умолчанию

Dennett не должен начинать с кластера из трёх или пяти согласующихся серверов. Для персональной системы это создаст непропорциональную сложность развертывания, обновлений, диагностики и восстановления.

Default:

- один основной Head Runtime;
- одна или несколько реплик/резервных узлов;
- device-local журналы и кэши;
- автоматические backups;
- возможность планового переноса роли Head;
- ограниченный emergency режим при аварии;
- опциональный внешний witness/lease service для автоматического failover без split-brain.

Если пользователь позднее потребует высокую доступность уровня организации, архитектура должна позволять заменить single-head coordination на consensus-backed implementation. Но это не является обязательным baseline.

### 0.3. Не существует одной модели согласованности для всех данных

Главная ошибка серверного проектирования Dennett состояла бы в попытке синхронизировать всё одинаково.

- permission и факт отправленного платежа требуют строгой актуальности;
- Memory Event допускает append и последующее объединение;
- поисковый индекс может быть eventual и пересобираемым;
- файл кода имеет authority в Git/worktree;
- свободная заметка может сохранять две конфликтующие версии;
- screenshot может загружаться лениво;
- secret вообще не должен распространяться обычной синхронизацией;
- Agent Radar является временной проекцией и может быть пересоздан.

Опыт Figma особенно показателен: даже внутри одного продукта изменения графического документа и изменения comments/users/projects синхронизируются разными системами из-за разных требований к offline availability, security и performance. Dennett должен делать то же самое на уровне принципа. [[S21]]

### 0.4. Долговечность пропорциональна масштабу работы

Server Runtime не превращает каждый ответ модели в Temporal workflow.

- **Direct Turn** может завершиться без отдельной долговечной истории исполнения;
- **Adaptive Agent Session** хранит continuity и важные результаты, но стратегия остаётся внутри сильного агента;
- **Managed Run** получает durable state, checkpoint, cancel, budget и effect ledger;
- **Structured Automation** может использовать полноценный durable workflow engine, если процесс должен переживать рестарты, ждать дни, безопасно повторять activities или иметь строгий operational order.

Temporal демонстрирует сильные гарантии replay и восстановления Worker после outage, но за них приходится соблюдать deterministic workflow constraints и выносить API/LLM/DB-вызовы в Activities. Эти свойства полезны там, где нужна настоящая durability, но не должны навязываться обычному проектному диалогу. [[S10]] [[S04]]

### 0.5. События доставляются как минимум один раз, эффекты выполняются не более одного логического раза

В распределённой системе transport может повторить сообщение. Поэтому Dennett не обещает магическое `exactly once` для всех каналов.

Вместо этого:

- события имеют стабильные IDs и dedupe keys;
- обработчики являются идемпотентными;
- consumer state хранит acknowledgement/watermark;
- external effect получает отдельный idempotency key, точные параметры и Effect Receipt;
- после timeout результат может стать `UNKNOWN` и должен быть reconciled до повтора.

NATS JetStream, например, прямо описывает stateful consumers и at-least-once delivery; AWS Builders’ Library рекомендует caller-provided request IDs и атомарную фиксацию idempotency token вместе с изменением состояния. [[S14]] [[S12]]

### 0.6. Короткая формула

> **Dennett Server Runtime — это постоянный персональный control plane с одним активным Head Runtime, локально способными устройствами, пропорционально долговечным исполнением, типоспецифичной синхронизацией, безопасной обработкой событий и внешних эффектов, проверяемым восстановлением и переносимостью без обязательной зависимости от одного компьютера или поставщика.**

---

## 1. Область ответственности и границы

### 1.1. Что именно принадлежит Server Runtime

Этот документ является каноническим владельцем следующей бизнес-логики:

- постоянного существования установки Dennett;
- активного Head Runtime;
- регистрации и состояния устройств;
- защищённых каналов между Head и устройствами;
- authoritative runtime state активных Task, Run, Agent Instance и provider sessions;
- исполнения, pause, resume, cancellation и recovery долговечных процессов;
- event intake, timers, schedules и trigger evaluation runtime;
- authoritative состояния Action Inbox;
- materialized состояния Agent Radar;
- notification routing и delivery state;
- распределения вычислительных ресурсов между интерактивной и фоновой работой;
- синхронизации устройств и offline operation logs;
- consistency classes и conflict routing;
- head election, planned handoff, failover и split-brain containment;
- external-effect dispatch, idempotency и reconciliation;
- backup, restore verification и disaster recovery;
- server migration, installation export, portable client и emergency mode;
- системного обслуживания, обновлений и low-priority sleep windows;
- runtime observability и repair state.

### 1.2. Что Server Runtime не переопределяет

Server Runtime исполняет и хранит состояние, но не присваивает себе чужую бизнес-логику.

- Memory Fabric решает, что является Memory Event, Evidence, Claim, Current State, projection и deletion obligation.
- Agentic Control Fabric решает, когда локальная работа становится Task, как агент выбирает стратегию и что значит completion.
- Trust Fabric решает, кто имеет право выполнить Action Request.
- Capability Fabric решает, какие backends, models, connectors и tools доступны и здоровы.
- Voice Fabric будет решать turn-taking, речевой UX и ambient interaction.
- Desktop и Mobile документы будут решать, какие кнопки и экраны показывают серверное состояние.

Сервер не должен:

- самостоятельно решать смысл пользовательской цели вместо оркестратора;
- считать UI-кэш источником истины;
- выдавать permission из-за того, что модель убедительно попросила;
- менять пользовательскую модель в прямом чате без политики;
- делать каждый внутренний model call долговечной Task;
- хранить единственный экземпляр важного смысла только в очереди или логе;
- превращать логические модули этого документа в обязательный набор микросервисов.

### 1.3. Главные источники истины

| Вопрос | Источник истины |
|---|---|
| Кто сейчас является Head | Head Lease + Authority Epoch |
| Статус активной Task/Run | Runtime Registry, согласованный с Agentic state machine |
| Какое разрешение действует | Trust/Permission Registry |
| Что действительно отправлено внешнему сервису | Effect Receipt + provider reconciliation |
| Что произошло исторически | Memory/Event evidence по соответствующему домену |
| Какой код актуален | конкретный repository/worktree/commit |
| Какие capabilities доступны на устройстве | Capability Registry + свежий device health |
| Какой вопрос ждёт пользователя | Action Inbox canonical record |
| Что видно на Radar | пересобираемая проекция над runtime state |
| Что находится в backup | Backup Manifest конкретного snapshot |

---

## 2. Исследовательский протокол и критерии выбора

### 2.1. План исследования

Работа над моделью выполнялась в следующем порядке.

#### Этап A — восстановить реальные требования Dennett

Проверены сценарии:

1. обычный project chat на основном ПК;
2. фоновая задача на сервере;
3. агент на сервере, исполняющий действие на Windows-узле;
4. голосовое поручение с телефона;
5. сенсорное событие, записанное локально и синхронизированное позже;
6. серверный restart во время Managed Run;
7. provider outage;
8. offline laptop с изменениями проекта и памяти;
9. конфликт заметок с двух устройств;
10. повтор события или команды;
11. timeout после возможной отправки сообщения;
12. потеря основного сервера;
13. временное назначение другого устройства головным;
14. возвращение старого сервера после partition;
15. восстановление из backup;
16. перенос установки на другой сервер;
17. portable client на чужом компьютере;
18. удаление чувствительных данных во всех репликах;
19. системный sleep без влияния на интерактивную работу;
20. ситуация, где правильное действие — ничего не запускать.

**Критерий возврата:** если сценарий требует фразы «система как-нибудь поймёт», но не имеет authoritative state, отказоустойчивого пути и пользовательски понятного результата, модель считается неполной.

#### Этап B — сравнить четыре общие формы

Сравнивались:

1. cloud/server-centric система;
2. полностью local-first multi-master система;
3. один персональный server + тонкие клиенты;
4. personal control plane + local-capable nodes.

**Критерии:** offline utility, ownership, простота, безопасность внешних эффектов, возможность failover, нагрузка на сервер, стоимость реализации и способность постепенно усложняться.

#### Этап C — изучить проверенные паттерны

Использованы:

- durable execution Temporal/Restate; [[S10]] [[S11]]
- idempotent APIs AWS/Stripe; [[S12]] [[S13]]
- at-least-once streams NATS и event-time/watermarks Flink; [[S14]] [[S16]]
- CloudEvents и transactional outbox; [[S15]] [[S17]]
- Home Assistant event bus/state machine/service registry; [[S18]]
- local-first, Figma, Yjs, Automerge, Syncthing и CouchDB replication; [[S20]] [[S21]] [[S22]] [[S23]] [[S24]] [[S32]]
- Kubernetes leases/control plane communication и fencing tokens; [[S25]] [[S26]] [[S27]]
- restic/Borg и GitLab backup incident; [[S29]] [[S30]] [[S31]]
- Tailscale control-plane/data-plane separation; [[S19]]
- production sync cases Linear, ElectricSQL, PowerSync и Replicache. [[S33]] [[S34]] [[S35]] [[S36]]

#### Этап D — провести overengineering audit

Для каждого механизма задавались вопросы:

- можно ли сделать это обычным запросом к одному агенту;
- нужна ли долговечность или достаточно локального состояния;
- нужен ли LLM или хватит детерминированного фильтра;
- нужен ли CRDT или обычная версия/merge проще;
- нужна ли автоматическая HA или достаточно backup + handoff;
- можно ли использовать готовый sync/backup/runtime вместо собственного;
- насколько часто механизм реально будет использоваться;
- можно ли удалить его позднее без потери пользовательских данных.

#### Этап E — провести underengineering audit

Отдельно проверено, не исчезают ли из-за упрощения:

- защита от повторного платежа/сообщения;
- authoritative permission state;
- recovery долгого run;
- offline capture;
- история и конфликты;
- защита от split-brain;
- проверяемый restore;
- переносимость и право пользователя на данные.

### 2.2. Итог сравнения альтернатив

#### Только центральный сервер

**Плюсы:** простая authority, удобные события и фоновые процессы.  
**Минусы:** зависимость от сети и одного узла, плохая локальная отзывчивость, большие media/uploads, слабая автономность устройств.

**Вердикт:** отклонено как полная модель; центральный server остаётся control plane, но устройства должны уметь работать локально.

#### Полностью peer-to-peer multi-master

**Плюсы:** высокая offline-доступность и отсутствие одного центра.  
**Минусы:** сложные permissions, split-brain, duplicate effects, budget contention, трудный Action Inbox и неоднозначная текущая Task.

**Вердикт:** отклонено для глобального control state; отдельные данные могут синхронизироваться local-first.

#### Full HA consensus cluster как default

**Плюсы:** быстрый automatic failover и строгая authority.  
**Минусы:** эксплуатационная сложность, требования к нескольким всегда доступным узлам, обновления и диагностика непропорциональны персональной системе.

**Вердикт:** не default; должен быть возможен как поздний deployment profile.

#### Personal control plane + local-capable nodes

**Плюсы:** единая власть там, где она нужна; offline работа; локальные модели и файлы; масштабирование устройств; постепенное усложнение.  
**Минусы:** необходимо явно проектировать типы данных, failover и offline ограничения.

**Вердикт:** принят.

---

## 3. Единая логическая модель Server Runtime

### 3.1. Installation

**Installation** — одна персональная установка Dennett, принадлежащая одному главному пользователю или позднее явно определённой группе principals.

Она включает:

- уникальный `installation_id`;
- набор зарегистрированных устройств;
- активный Head Runtime;
- Memory Fabric;
- Runtime Registry;
- Trust и Capability registries;
- проекты, Task и Runs;
- event/schedule state;
- Action Inbox;
- backup и portability manifests.

Installation не обязана находиться на одном физическом устройстве. Но в каждый момент должен существовать один authoritative coordination epoch.

### 3.2. Логические части

Server Runtime состоит из шести крупных логических частей. Они описывают ответственность, а не deployment topology.

#### Control Kernel

Удерживает:

- Head Lease и Authority Epoch;
- Runtime Registry;
- device registry и health;
- Task/Run coordination state;
- permission/capability references;
- budgets и resource claims;
- effect claims;
- global configuration revisions.

#### Execution Plane

Запускает или координирует:

- provider sessions;
- локальные/серверные Agent Runs;
- device actions;
- Managed Runs;
- optional Structured Automations;
- checkpoints и recovery.

Execution может происходить на Head, на другом server node, на desktop, laptop или в provider cloud. Control Kernel не обязан сам выполнять вычисление.

#### Event and Attention Runtime

Отвечает за:

- intake событий;
- timers и schedules;
- trigger subscriptions;
- candidate filtering;
- запуск semantic evaluation при необходимости;
- создание Task, Action Inbox cards и notifications;
- cooldown, expiry и dedupe.

#### State and Sync Plane

Отвечает за:

- local operation logs;
- replication handshakes;
- watermarks;
- conflict routing;
- object transfer;
- read-your-writes;
- deletion propagation;
- device-specific replication policy.

#### Device Gateway

Связывает Head с узлами:

- идентифицирует устройство;
- принимает heartbeats;
- получает capability/health inventory;
- маршрутизирует команды;
- принимает результаты и receipts;
- организует direct или relayed data transfer;
- поддерживает user takeover и offline queue.

#### Maintenance and Recovery Plane

Планирует:

- backup;
- restore tests;
- compaction;
- index rebuild;
- data tiering;
- system sleep;
- provider refresh;
- migrations;
- repair jobs.

### 3.3. Deployment profiles

#### Standalone

Один компьютер выполняет роли Head, client, execution node и local storage.

Подходит для:

- ранней разработки;
- пользователя без отдельного сервера;
- offline-first режима.

Все контракты сохраняются, даже если network calls превращаются в локальные вызовы.

#### Personal Server — рекомендуемый основной профиль

Всегда включённый домашний или личный сервер является Head. Desktop, laptop и phone подключаются как nodes.

Преимущества:

- фоновые процессы не зависят от открытого приложения;
- память и events доступны постоянно;
- устройства могут отключаться;
- тяжёлые данные остаются на подходящих nodes.

#### Hybrid

Head находится на домашнем/личном server, а часть workloads запускается в cloud providers или remote compute. Backup может находиться в отдельном cloud storage.

#### Managed Cloud

Head размещён на VPS или управляемой инфраструктуре. Локальные nodes сохраняют offline capabilities и private/local-only data.

#### Emergency

При недоступности Head одно trusted устройство предоставляет ограниченный coordination runtime. Полнота режима зависит от свежести реплики и наличия witness.

---

### 3.4. Как через сервер проходит обычная работа Dennett

Server Runtime связывает подсистемы, но не поглощает их ответственность. Типовой путь выглядит так:

```text
пользователь говорит, пишет или нажимает действие на одном из устройств
→ Interaction Node передаёт намерение или command активному Head Runtime
→ Head восстанавливает current project/session/runtime context
→ главный оркестратор или project agent выбирает форму работы
→ Capability Fabric подбирает доступный provider/tool/node
→ Trust Fabric проверяет grant и допустимый эффект
→ Execution Plane запускает работу на server, provider cloud или конкретном устройстве
→ result, artifact и Effect Receipt возвращаются Head
→ Runtime Registry обновляет operational state
→ Memory Fabric получает только значимые события и evidence
→ Action Inbox, Radar и приложения получают новые projections
```

Для локальной операции путь может быть короче: доверенный node выполняет уже разрешённый project action сам и затем синхронизирует result. Для consequential external effect authority всё равно должна быть проверяема через актуальный epoch, grant и effect claim.

Этот flow является логическим. В standalone-установке все стрелки могут быть вызовами внутри одного процесса; в распределённой установке — защищёнными сообщениями между nodes.

# Часть II. Постоянный runtime, устройства и проактивная работа

## 4. Устройства, узлы и каналы связи

### 4.1. Node, Device и Client

**Device** — зарегистрированное физическое или виртуальное устройство с identity и trust state.  
**Node Runtime** — локальный процесс Dennett, работающий на устройстве.  
**Client UI** — интерфейс пользователя; он может быть запущен на устройстве без всех node capabilities.

Один device может одновременно быть:

- UI client;
- execution node;
- sensor node;
- local model host;
- project storage node;
- backup target;
- head candidate.

### 4.2. Роли узла

#### Interaction Node

Desktop/mobile UI, voice, Action Inbox, Radar и project chat.

#### Execution Node

Может запускать shell, local model, IDE, browser, computer-use, build или другие capabilities.

#### Sensor Node

Поставляет audio, screen, camera, clipboard, app state и другие разрешённые события.

#### Storage Node

Хранит project files, media, object replicas или backup snapshots.

#### Head Candidate

Способен временно или постоянно принять control plane.

#### Witness

Минимальный доверенный координационный узел, который может выдавать/подтверждать новый Authority Epoch, но не обязан хранить память и выполнять agents.

### 4.3. Device Manifest

Head хранит для каждого устройства:

```yaml
device:
  device_id: id
  identity_ref: ref
  trust_state_ref: ref
  roles: []
  platform: windows | linux | macos | android | ios | server | other
  protocol_versions: []
  capabilities_snapshot_ref: ref
  storage_replicas: []
  local_projects: []
  sync_watermarks: {}
  authority_epoch_seen: integer
  last_heartbeat_at: time
  network_state: direct | relayed | offline | unknown
  power_state: optional
  head_eligibility: none | emergency | full
  health: healthy | degraded | stale | offline | revoked | quarantined
```

Manifest является operational state. Он не заменяет Capability Registry и Trust Registry, а содержит ссылки на их актуальные решения.

### 4.4. Каналы связи

Логически разделяются:

- **control channel** — команды, grants, cancel, heartbeats, state revisions;
- **event channel** — события и acknowledgements;
- **streaming channel** — voice, interactive logs, progress;
- **object channel** — files, screenshots, artifacts, backup chunks;
- **direct peer channel** — большой data transfer между nodes после coordination;
- **notification channel** — push/local delivery.

Разделение не означает обязательные отдельные сетевые соединения. Оно нужно, чтобы:

- большой screenshot не блокировал cancel;
- потеря stream не означала потерю Task;
- control messages имели приоритет;
- media могло передаваться напрямую;
- retries имели разную семантику.

### 4.5. Инициирование соединения

Предпочтительно, чтобы device agent сам устанавливал исходящее защищённое соединение с Head. Это лучше работает за NAT и не требует открывать входящий порт на каждом устройстве.

Control plane pattern Kubernetes также строит большинство node-to-control-plane взаимодействий через центральный защищённый API endpoint, а Tailscale отделяет централизованный обмен policy/keys от peer data plane. Dennett использует этот опыт как принцип, но не требует Kubernetes или Tailscale как реализацию. [[S19]] [[S26]]

### 4.6. Heartbeat не равен доказательству исправности

Heartbeat показывает, что node process недавно мог связаться с Head. Он не доказывает, что:

- GPU работает;
- project disk доступен;
- browser session жива;
- секрет не отозван;
- конкретный tool здоров.

Поэтому имеются:

- lightweight heartbeat;
- capability health probes;
- workload-specific readiness;
- last successful execution.

Kubernetes использует leases для heartbeats и отдельно — для leader election; Dennett принимает это разделение. [[S25]]

---

## 5. Head Runtime, handoff и failover

### 5.1. Почему один Head

Только Head может окончательно:

- принять state-changing command глобального scope;
- продвинуть Task/Run state;
- выдать authoritative ordering для effect claims;
- разрешить global budget consumption;
- создать/закрыть canonical Action Inbox card;
- изменить глобальную configuration revision;
- зафиксировать новый Authority Epoch.

Локальные nodes могут автономно выполнять уже выданную работу, но результат принимается Head с проверкой epoch, grant и run context.

### 5.2. Head Lease и Authority Epoch

```yaml
head_authority:
  installation_id: id
  head_node_id: id
  authority_epoch: monotonic_integer
  lease_issued_at: time
  lease_expires_at: time
  state_watermark: ref
  witness_refs: []
  mode: normal | planned_handoff | emergency | recovery
  signature: bytes
```

**Lease** ограничивает время, когда node считает себя активным Head.  
**Authority Epoch** монотонно увеличивается при каждой передаче власти.

Каждая consequential server write, device command и external effect claim содержит epoch. Компонент, который уже видел больший epoch, отвергает старый.

Один timeout/lease недостаточен для корректности: старый процесс может проснуться после паузы и продолжить запись. Fencing tokens решают эту проблему, заставляя target отвергать операции с устаревшим монотонным номером. [[S27]]

### 5.3. Плановый handoff

```text
пользователь или система выбирает новый Head candidate
→ проверить доверие, storage и protocol compatibility
→ синхронизировать canonical state до target watermark
→ приостановить создание новых global effects на старом Head
→ дождаться/зафиксировать in-flight effects
→ создать snapshot и handoff manifest
→ выдать новый Authority Epoch
→ активировать новый Head
→ старый Head переходит в replica/read-only coordinator
→ devices переподключаются
→ выполнить health и consistency checks
→ закрыть handoff
```

Плановый handoff должен иметь rollback window, пока старый Head не потерял совместимую копию данных.

### 5.4. Автоматический failover с witness

Автоматический полный failover безопасен, если доступен общий witness или consensus-backed coordination source, который:

- видит истечение старого lease;
- выдаёт только один новый epoch;
- проверяет candidate identity;
- не позволяет двум nodes одновременно получить одинаковую власть.

Candidate выбирается по:

- trust;
- protocol/schema compatibility;
- freshness критических replicas;
- доступности keys;
- устойчивости питания/сети;
- runtime readiness;
- пользовательскому приоритету.

### 5.5. Emergency failover без witness

Если devices не могут связаться ни с Head, ни с общим witness, доказать отсутствие второго активного Head невозможно.

Вместо опасной иллюзии full failover запускается **Isolated Emergency Head**:

- локальные project sessions;
- чтение кэшированной/локальной памяти;
- capture и append local events;
- локальные модели;
- project-local reversible actions;
- подготовка drafts;
- создание offline Tasks;
- ограниченные заранее авторизованные local automations.

По умолчанию запрещены или откладываются:

- глобальные permission changes;
- массовые deletions;
- действия, расходующие общий финансовый лимит;
- отправки, которые могут быть одновременно выполнены другим Head;
- изменение global current state;
- promotion project data в global memory;
- операции над единственным удалённым ресурсом без idempotency/reconciliation.

Пользователь может явно выполнить **manual takeover**, понимая риск. Тогда устройство создаёт новый локальный epoch proposal и журналирует все эффекты для последующего reconciliation. Но до контакта с witness/старым Head строгая single-head гарантия отсутствует и интерфейс обязан это показывать.

### 5.6. Возвращение старого Head

1. Старый Head видит больший epoch.
2. Немедленно прекращает global writes и effect dispatch.
3. Отправляет незафиксированные local events/receipts новому Head.
4. In-flight effects reconcile по idempotency/provider state.
5. Conflicts сохраняются, а не скрываются last-write-wins.
6. Старый Head становится replica или требует repair.

### 5.7. Почему не полный consensus default

Kubernetes leases показывают простой pattern одного активного controller/scheduler среди standby instances. Но полноценная HA всё равно требует инфраструктуры coordination. Для Dennett baseline достаточно одного Head, backup и чётко ограниченного failover; correctness-critical multi-node automation можно добавить deployment profile позднее, используя consensus-backed coordination вроде Raft или managed equivalent. [[S25]] [[S28]]

---

## 6. Долговечное исполнение и распределение работы

### 6.1. Server Runtime хранит процесс, но не рассуждает вместо агента

Agentic Control Fabric определяет четыре уровня работы. Server Runtime обеспечивает им разную глубину persistence.

#### Direct Turn

Сервер хранит минимум:

- correlation;
- выбранную session/model configuration;
- итоговый ответ/артефакт при необходимости;
- consequential effects;
- важные memory commits.

Внутренний scratch и каждый model step не обязаны становиться durable state.

#### Adaptive Agent Session

Сохраняются:

- project/chat continuity;
- provider session handle;
- recent working summary;
- selected context handles;
- current work item при необходимости;
- artifacts и пользовательские corrections.

#### Managed Run

Сохраняются:

- Task и Run IDs;
- owner;
- goal/constraints;
- current phase;
- budget;
- cancellation state;
- checkpoint;
- pending waits;
- external-effect ledger;
- provider/device session handles;
- Result Envelope.

#### Structured Automation

Может хранить полную durable execution history, timers, node state, activities, compensation и versioned procedure.

### 6.2. Runtime Registry

```yaml
runtime_record:
  run_id: id
  task_ref: optional
  project_ref: optional
  owner_ref: ref
  execution_level: direct | adaptive_session | managed_run | structured
  state: queued | running | waiting | paused | completing | terminal
  wait_reason: optional
  assigned_node_refs: []
  provider_session_refs: []
  capability_plan_ref: optional
  permission_refs: []
  budget_state_ref: optional
  checkpoint_ref: optional
  effect_claim_refs: []
  artifact_refs: []
  last_meaningful_progress_at: time
  authority_epoch: integer
  revision: integer
```

Task semantics остаются в Agentic Control Fabric. Registry является authoritative operational представлением, на основе которого работают scheduler, recovery и Radar.

### 6.3. Checkpoint

Checkpoint — не полный transcript и не скрытый chain-of-thought.

Он содержит достаточно для продолжения:

- исходную цель;
- актуальные ограничения;
- краткое состояние плана;
- уже выполненные логические действия;
- authoritative observations;
- созданные artifacts;
- unresolved errors;
- pending actions;
- external effects и их статусы;
- provider continuation handle;
- memory handles;
- требования к следующему исполнителю.

### 6.4. Provider session failure

Если provider-native session доступна, Dennett продолжает её.

Если она потеряна:

1. Run не исчезает.
2. Сохраняется failure event.
3. Создаётся новый provider attempt/session.
4. Context собирается из checkpoint, project state, evidence и Memory Fabric.
5. Unknown effects reconcile до повторения.
6. Пользователь видит смену provider/model, если это прямой чат или поведение может заметно измениться.

Dennett не обещает бесшовный перенос скрытого внутреннего состояния между vendors.

### 6.5. Durable waits

Ожидание:

- времени;
- пользователя;
- permission;
- внешнего сообщения;
- webhook;
- device availability;
- provider quota;
- завершения другого Run

не должно удерживать модель или процесс в памяти.

Runtime сохраняет wait condition и освобождает compute. При signal условие переоценивается с текущими permissions и state.

### 6.6. Scheduler и приоритеты

Baseline scheduler не является универсальным Kubernetes-аналогом. Он использует несколько понятных классов:

1. **Interactive critical:** voice turn, cancel, user takeover, permission response.
2. **User waiting:** project chat tool call, requested status, foreground generation.
3. **Active project:** обычные Managed Runs.
4. **Background:** monitoring, research, scheduled jobs.
5. **Maintenance:** indexing, sleep, backup verification, eval.

Scheduler учитывает:

- CPU/GPU/RAM;
- локальность данных;
- provider rate limits и подписочные квоты;
- device battery/power policy;
- network;
- user quiet hours;
- project priority;
- deadline;
- maximum parallel agents;
- budget stop-loss.

Интерактивная работа может preempt или замедлять maintenance. Но уже выполняющийся consequential effect не прерывается без определения безопасной точки.

### 6.7. Resource claims

Claims нужны только для реально разделяемых ресурсов:

- GPU model slot;
- browser/computer-use session;
- project worktree;
- exclusive external account action;
- shared spending quota;
- microphone/camera ownership;
- head migration.

Один project agent не получает lease на каждый файл. Resource ownership включается при фактической конкуренции.

---

## 7. Event Runtime и проактивное поведение

### 7.1. Пять разных понятий

#### Event

Наблюдение о том, что произошло: сообщение пришло, файл изменён, пользователь вошёл в место, Run завершён.

#### State Update

Новое текущее operational состояние: device offline, provider quota-limited, Task waiting.

#### Trigger Subscription

Описание условия, при котором событие следует оценить.

#### Prospective Intent

Смысловое будущее намерение из Memory Fabric: «когда снова буду обсуждать X, поднять Y».

#### Command

Просьба изменить состояние или выполнить действие.

Эти сущности нельзя смешивать. Внешний Event не получает право выполнить Command сам по себе.

### 7.2. Event Envelope

Dennett использует небольшой стабильный envelope, близкий по духу к CloudEvents, но не обязан копировать весь стандарт во внутренний hot path. [[S15]]

```yaml
event:
  event_id: id
  source_ref: ref
  source_sequence: optional
  event_type: string
  observed_at: time
  ingested_at: time
  scope_ref: ref
  subject_ref: optional
  payload_ref: optional
  causation_ref: optional
  correlation_ref: optional
  dedupe_key: optional
  trust_origin_ref: ref
  authority_epoch: optional
  retention_class: typed
```

### 7.3. Уровни событий

Не каждый сигнал требует долговечного event ledger.

#### Ephemeral telemetry

CPU sample, cursor movement, partial speech frame, heartbeat detail. Имеет короткую retention или агрегируется.

#### Operational event

Task transition, device connection, permission decision, provider failure. Долговечен настолько, насколько нужен recovery/audit.

#### Memory-relevant event

Передаётся Memory Fabric как evidence/candidate.

#### Trigger candidate

Может запустить evaluation будущего условия.

Один event может иметь несколько ролей, но это явное решение pipeline, а не автоматическая запись всего навечно.

### 7.4. Intake pipeline

```text
source emits
→ проверить identity/signature/channel
→ нормализовать envelope
→ deduplicate
→ определить retention и scope
→ append durable event при необходимости
→ обновить operational projection
→ найти релевантные deterministic subscriptions
→ сформировать небольшой semantic candidate set
→ semantic evaluation только если нужна
→ Action Deliberation: act / ask / notify / remember / do nothing
→ создать command, Task, card или memory write
→ сохранить outcome
```

Home Assistant показывает практичную архитектуру event bus + state machine + service registry: occurrence, current state и действие разделены. Dennett принимает это разделение, но добавляет trust, durability, semantic triggers и multi-device ordering. [[S18]]

### 7.5. Не запускать LLM на каждый event

Дешёвые этапы:

- exact event type;
- source/scope filter;
- time window;
- project relation;
- threshold;
- cooldown;
- dedupe;
- known deterministic condition.

Модель вызывается только для кандидатов вроде:

- «похоже ли это на возвращение к теме»;
- «достаточно ли результат хорош, чтобы спросить пользователя»;
- «является ли новость релевантной активному проекту»;
- «стоит ли вмешаться или лучше ничего не делать».

При высоком потоке local lightweight model может выполнять предварительную классификацию.

### 7.6. Event time, ingest time и поздние события

Событие хранит минимум два времени:

- когда оно произошло/наблюдалось;
- когда Head его получил.

При offline sync событие за утро может прийти вечером. Оно должно корректно войти в историю, но не обязано задним числом запускать устаревшее внешнее действие.

Flink разделяет event time и processing time и использует watermarks, чтобы работать с out-of-order/late data. Dennett применяет упрощённый принцип: каждый source имеет watermark, а trigger определяет late-event policy. [[S16]]

Варианты late policy:

- `history_only`;
- `evaluate_if_still_relevant`;
- `coalesce_to_latest`;
- `missed-run-notify`;
- `execute_once_on-reconnect`;
- `expire`.

### 7.7. Timers и schedules

Schedule хранит:

- timezone;
- calendar expression/next time;
- missed-run policy;
- concurrency policy;
- jitter;
- expiry;
- maximum catch-up count;
- trigger/action reference;
- permission requirements.

Missed-run policy:

- пропустить;
- выполнить один раз после восстановления;
- выполнить все пропущенные bounded runs;
- спросить;
- пересчитать от текущего времени.

Clock time не используется как единственный ordering механизм для distributed writes.

### 7.8. Semantic trigger lifecycle

```text
candidate
→ active
↔ paused
→ triggered
→ cooldown
→ active | fulfilled | expired | cancelled
```

У trigger есть:

- scope;
- source filters;
- false-positive/false-negative cost;
- cooldown;
- expiry;
- attention policy;
- action ceiling;
- last evaluation;
- outcome history.

Если trigger регулярно создаёт бесполезные вмешательства, runtime снижает частоту или предлагает удалить его. Семантическое качество анализируется Agentic/Memory, а сервер обеспечивает lifecycle и бюджет.

### 7.9. Backpressure

При event storm:

- cancel/security/control events не теряются;
- explicit user capture имеет высокий приоритет;
- повторяющаяся telemetry агрегируется;
- screenshots дедуплицируются;
- heavy semantic processing откладывается;
- очередь показывает lag;
- система не утверждает, что всё обработано;
- retention policy может отбросить только заранее допустимый низкоценный поток.

---

## 8. Action Inbox, notifications и Agent Radar

### 8.1. Action Inbox — долговечная очередь решений

Action Inbox card создаётся, когда работа действительно требует пользовательского решения или полезного осознанного вмешательства.

```yaml
action_card:
  card_id: id
  subject: text
  reason_summary: text
  originating_refs: []
  project_ref: optional
  action_request_ref: optional
  options: []
  recommendation: optional
  risk_summary: optional
  artifact_refs: []
  status: open | snoozed | resolved | expired | superseded | withdrawn
  urgency: low | normal | high | critical
  created_at: time
  expires_at: optional
  resolved_by: optional
  resolution_ref: optional
  revision: integer
```

Card:

- не является permission token;
- не хранит единственный экземпляр Task;
- может объединять несколько одинаковых вопросов;
- закрывается автоматически, если причина исчезла;
- сохраняет lineage к Run, Trust decision или event.

### 8.2. Ответ с нескольких устройств

Пользователь может ответить с desktop, phone или voice.

Ответ является command с:

- card ID;
- base revision;
- выбранным option/текстом;
- authenticated session;
- idempotency key.

Head:

1. проверяет, что card ещё открыт;
2. revalidates action/permission;
3. атомарно фиксирует resolution;
4. создаёт нужную команду;
5. обновляет остальные устройства.

Поздний ответ другого устройства получает `already resolved` и фактический outcome.

### 8.3. Notification не равна card

Notification — попытка доставить внимание. Card — долговечное решение.

Notification routing учитывает:

- urgency;
- доступные устройства;
- quiet hours;
- user presence;
- annoyance budget;
- privacy экрана;
- deadline;
- предыдущие attempts;
- требуется ли DA3.

Delivery states:

```text
planned → sent → delivered → opened | dismissed | expired | failed
```

Если push не доставлен, card остаётся в Inbox.

### 8.4. Дедупликация и группировка

Система не создаёт десять карточек «provider недоступен» для десяти связанных Runs. Она может создать одну агрегированную карточку с impacted refs.

Group key учитывает:

- cause;
- project;
- decision type;
- deadline;
- возможное единое действие пользователя.

### 8.5. Agent Radar — materialized runtime view

Radar показывает:

- активные значимые Tasks/Runs;
- Agent Instances;
- assigned devices/providers;
- текущую фазу;
- waits и blockers;
- last meaningful progress;
- budget;
- external effects;
- risks;
- stale/degraded state;
- ближайший следующий шаг.

Radar не записывает свой отдельный «статус агента». Он пересобирается из Runtime Registry, Agentic state, device health, provider health и Action Inbox.

Если связь с Head потеряна, client показывает время последней актуальности, а не старый state как живой.

---

# Часть III. Распределённое состояние, offline, эффекты и сохранность данных

## 9. Синхронизация и согласованность данных

### 9.1. Главный принцип

> **Синхронизируется не “вся Dennett”, а разные классы данных по разным контрактам.**

CRDT, Git, object storage, revisioned records и authoritative single-writer state решают разные задачи.

### 9.2. Матрица данных

| Класс | Authority | Consistency | Offline write | Conflict strategy |
|---|---|---|---|---|
| Head/epoch, permissions, effect claims | Head/Trust registry | strong/current | обычно нет | reject stale epoch/revision |
| Task/Run operational state | Head Runtime | read-your-writes/strong transitions | локальный progress как proposal | reconcile with run revision |
| Memory Events/Evidence metadata | Memory Fabric ledger | append + eventual projections | да, signed local log | dedupe, preserve branches |
| Memory Current State/projections | derived | eventual/projection-complete on demand | нет прямой правки | recompute/resolver |
| Human notes | versioned document authority | eventual/read-your-writes | да | three-way/CRDT/preserve both |
| Project code/files | repo/worktree/filesystem | domain-native | да | Git/file conflict rules |
| Project Memory Pack | repository + Memory Fabric adapter | Git/versioned | да | Git merge + semantic review |
| Artifacts | content-addressed object + metadata | immutable/versioned | да | new version, no overwrite |
| Sensory raw media | originating device until committed | async/lazy | да | content hash/dedupe |
| Search/vector/graph indexes | rebuildable | eventual | нет | discard/rebuild |
| UI/Radar caches | disposable | eventual | optimistic command only | refresh from Head |
| Settings | owner-specific revision | field/revision based | ограниченно | deterministic merge or conflict |
| Secrets | Secret Broker/vault | authoritative | только специальный flow | no generic merge |
| Provider sessions | provider native | best available | provider dependent | recreate attempt/session |
| Capability availability | Registry + node health | current observation | device reports | newest verified observation |

### 9.3. Local operation log

Каждый offline-capable node ведёт append-only log операций, которые должны попасть на Head.

```yaml
local_operation:
  operation_id: id
  device_id: id
  device_sequence: integer
  base_watermark: ref
  observed_at: time
  operation_type: typed
  scope_ref: ref
  payload_ref: ref
  causation_ref: optional
  permission_snapshot_ref: optional
  signature: bytes
```

После acknowledgement старые segments могут compaction/archival по policy.

### 9.4. Sync handshake

```text
node authenticates
→ сообщает protocol/schema versions, epoch и watermarks
→ Head возвращает current epoch и required migrations
→ node отправляет unseen signed operations
→ Head проверяет grants/scope и deduplicates
→ canonical stores принимают допустимые operations
→ affected projections обновляются
→ Head отправляет node отсутствующие updates/manifests
→ objects загружаются eager или lazy по replication policy
→ обе стороны фиксируют watermarks
```

### 9.5. Read-your-writes

Пользователь не должен создать заметку offline и затем «не видеть» её в локальном agent session до серверной индексации.

Node поддерживает recent overlay:

- локальные operations;
- неподтверждённые artifacts;
- local project state;
- pending memory handles.

После sync overlay заменяется canonical references без визуального исчезновения данных.

### 9.6. Conflict policy

#### Append events

Объединяются; ordering восстанавливается по source sequence, causation, observed/ingested time. Wall clock не является абсолютной истиной.

#### Изменяемые notes

Если изменения независимы и не пересекаются — deterministic three-way merge. Если смысл пересекается:

- сохранить обе версии;
- создать conflict object;
- модель может предложить merge;
- material conflict показывается пользователю или ответственному агенту;
- исходники не удаляются до решения.

#### Settings

Каждое поле имеет owner/scope и revision. Security-sensitive settings не сливаются моделью.

#### Task/Run commands

Используют base revision и idempotency. Поздняя команда может стать stale/superseded.

#### Files

Dennett не пишет собственный универсальный merge engine поверх Git и filesystem. Для Git-проектов authority — Git/worktree. Для обычных папок предпочтителен готовый file-sync backend или explicit copy/versioning.

Syncthing сохраняет conflict copies вместо молчаливого overwriting, потому что sync engine не знает, какая версия лучше пользователю. Dennett принимает этот осторожный принцип. [[S24]]

### 9.7. Где оправдан CRDT

CRDT допустим для:

- совместно редактируемых заметок;
- небольших структурированных досок;
- draft text с реальной concurrent editing;
- append-like collections.

CRDT не используется для:

- permissions;
- Head epoch;
- financial limits;
- effect claims;
- secrets;
- Task completion;
- arbitrary repository files;
- всякой сущности «потому что она синхронизируется».

Yjs и Automerge демонстрируют mature local-first merge и network-agnostic sync, но добавляют metadata/semantic complexity. Dennett подключает подобный backend только там, где real concurrent editing оправдывает его. [[S22]] [[S23]]

### 9.8. Project files и готовые sync engines

Для project data применяются по порядку:

1. Git remote/worktrees для кодовых репозиториев;
2. provider-native cloud storage для соответствующих документов;
3. готовый bidirectional file-sync вроде Syncthing для пользовательских папок;
4. Dennett object transfer для artifacts/portable memory;
5. собственный file sync только если существующие решения не дают нужной semantics.

Dennett должен знать состояние sync и конфликты, но не обязан реализовывать блоковую репликацию файлов с нуля.

### 9.9. Media и большие объекты

Большие data objects:

- адресуются content hash;
- имеют metadata отдельно от bytes;
- могут существовать local-only;
- загружаются по tier/priority;
- передаются напрямую между nodes при возможности;
- имеют thumbnails/redacted variants;
- не блокируют control channel;
- подтверждают durable commit до удаления единственной локальной копии.

Replication policy:

- `local-only`;
- `metadata-global`;
- `on-demand`;
- `full-replica`;
- `backup-only`;
- `ephemeral`.

### 9.10. Secrets

Secrets не идут через обычный sync log.

Варианты:

- Secret Broker на Head;
- encrypted vault replica на выбранных devices;
- повторная OAuth authorization;
- workload identity/short-lived token;
- device-bound key.

Offline device не получает новые secret values только потому, что у него есть копия проекта.

### 9.11. Deletion propagation

Delete/forget создаёт obligation с dependency graph:

- canonical object;
- replicas;
- local pending logs;
- indexes;
- caches;
- backups согласно retention;
- portable exports, если контролируются Dennett.

Offline node при reconnect сначала получает revocation/tombstone. Он не должен воскресить удалённый объект своей старой записью.

---

## 10. Offline-режим

### 10.1. Offline — нормальное состояние, а не авария

Node должен явно показывать:

- связь с Head;
- последний sync;
- current epoch;
- какие данные свежие;
- какие команды pending;
- какие функции доступны локально;
- какие действия запрещены из-за stale authority.

### 10.2. Разрешённые offline-функции

Обычно доступны:

- чтение локальной/кэшированной памяти с freshness label;
- project work в локальной папке;
- local project chat с локальной или доступной моделью;
- создание notes, tasks и drafts;
- capture photo/audio/screen;
- локальная индексация;
- просмотр downloaded artifacts;
- запуск локальных tools в выданном project scope;
- queue команд на Head;
- limited emergency voice.

### 10.3. Offline external actions

Действие можно выполнить offline только если одновременно:

- target доступен напрямую;
- grant действовал до offline и разрешает offline use;
- действие не требует свежего global state;
- отсутствует риск параллельного исполнения другим Head;
- idempotency/reconciliation доступны;
- лимит локально зарезервирован или действие не расходует общий ограниченный ресурс;
- устройство имеет достаточный authentication assurance.

Иначе Dennett:

- готовит draft;
- сохраняет Action Request;
- показывает `will revalidate on reconnect`;
- не создаёт ложный Effect Receipt.

### 10.4. Revalidation очереди

При reconnect queued command проверяется заново:

- не отменён ли intent;
- не истёк ли grant;
- не изменился ли recipient/amount/resource;
- не выполнено ли действие другим node;
- не устарел ли проект;
- не превышен ли лимит;
- не изменился ли Head epoch;
- всё ещё ли действие полезно.

Старая offline команда не получает приоритет над свежим пользовательским решением.

### 10.5. Local models и cloud fallback

Capability Fabric определяет возможные backends. Server Runtime хранит policy:

- использовать local fallback автоматически;
- спросить;
- ждать cloud provider;
- выполнить partial/local-only;
- запретить fallback из-за качества/конфиденциальности.

В direct chat смена модели показывается пользователю согласно Capability Fabric.

---

## 11. External Effects, retries и reconciliation

### 11.1. Почему server отвечает за effect lifecycle

Agentic слой формирует Action Request, Trust разрешает его, connector/device выполняет. Server Runtime должен обеспечить, чтобы:

- два workers не выполнили один intent дважды;
- failover не повторил уже отправленное сообщение;
- timeout не превратился в скрытый duplicate;
- пользователь видел реальный статус.

### 11.2. Effect Claim

```yaml
effect_claim:
  effect_id: id
  idempotency_key: string
  actor_ref: ref
  on_behalf_of: principal_ref
  action_request_ref: ref
  exact_parameters_hash: hash
  target_ref: ref
  provider_ref: ref
  permission_decision_ref: ref
  authority_epoch: integer
  state: prepared | dispatching | confirmed | failed | unknown | compensating | compensated
  created_at: time
  updated_at: time
  provider_operation_ref: optional
  receipt_ref: optional
```

### 11.3. Execution protocol

```text
получить разрешённый Action Request
→ нормализовать точные параметры
→ создать/получить idempotency key
→ атомарно зарегистрировать Effect Claim
→ отправить connector/device command с epoch и key
→ получить result
→ записать Effect Receipt
→ обновить Task/Run
→ при timeout перейти в UNKNOWN
→ reconcile provider/environment
→ только после reconciliation решать о retry
```

AWS рекомендует caller-supplied unique IDs, атомарную фиксацию token вместе с mutation и отклонение повторного ID с другими параметрами. Dennett применяет тот же принцип к external effects. [[S12]]

### 11.4. Exactly once не обещается

Некоторые providers поддерживают idempotency keys, другие — нет.

Стратегии по убыванию надёжности:

1. provider-native idempotency;
2. query/reconciliation по stable external ID;
3. prepare/commit protocol;
4. single authoritative dispatcher + local ledger;
5. human verification;
6. запрет автоматического retry.

### 11.5. UNKNOWN — отдельное состояние

`UNKNOWN` означает: request мог быть выполнен, но подтверждение потеряно.

Запрещено:

- считать его `FAILED`;
- слепо повторять;
- обещать пользователю успех;
- терять запись при restart.

Reconciliation может:

- спросить provider;
- проверить sent folder/payment state/repository;
- запросить device screenshot/state;
- попросить пользователя;
- оставить incident pending.

### 11.6. Transactional outbox

Когда Server Runtime меняет local authoritative state и должен опубликовать событие, эти две операции не должны расходиться.

Логический pattern:

- state mutation и outbox record фиксируются атомарно;
- publisher позже доставляет событие;
- consumer deduplicates;
- outbox record хранит delivery state.

Конкретная реализация может использовать database outbox, event log или durable engine. Debezium Outbox Router является одним из production references этого pattern. [[S17]]

### 11.7. Compensation

Не каждое действие обратимо.

Effect metadata указывает:

- reversible;
- compensatable;
- irreversible;
- compensation capability;
- deadline;
- residual consequences.

Compensation сама является новым external effect с собственным разрешением и receipt.

---

## 12. Backup, restore и disaster recovery

### 12.1. Sync не является backup

Replication быстро распространяет:

- ошибочное удаление;
- corruption;
- malware;
- плохую migration;
- логическую ошибку агента.

GitLab database incident стал известным примером того, как наличие нескольких backup mechanisms не помогло, потому что они не работали как предполагалось и restore не был своевременно проверен. Следствие для Dennett: успешный backup job ничего не доказывает без restore test и явного owner. [[S31]]

### 12.2. Классы backup

#### Critical canonical

- Memory Event Ledger и canonical evidence metadata;
- Trust/identity/config state;
- Runtime/Task durable state;
- Action Inbox;
- effect claims/receipts;
- project/capability manifests;
- encryption/key metadata;
- portable memory manifests.

#### User content

- project memory packs;
- notes;
- artifacts;
- committed media;
- configuration exports.

#### Rebuildable

- vector/BM25/graph indexes;
- Radar projections;
- thumbnails;
- caches;
- downloaded model artifacts при наличии источника.

#### Ephemeral

- ring buffers;
- partial streams;
- transient telemetry;
- provider scratch.

Backup policy не обязана одинаково охватывать все классы.

### 12.3. Backup topology

Рекомендуемый baseline:

- primary canonical storage;
- локальная или отдельная device replica/snapshot;
- encrypted offsite backup в независимом failure domain;
- периодические portable exports критических данных.

Google Drive, S3-compatible storage, NAS или другой backend являются targets, а не источником истины.

### 12.4. Snapshot и incremental log

Backup Manifest содержит:

```yaml
backup_snapshot:
  snapshot_id: id
  installation_id: id
  created_at: time
  authority_epoch: integer
  canonical_watermarks: {}
  included_classes: []
  object_manifest_hash: hash
  encryption_key_ids: []
  schema_versions: {}
  application_version: text
  previous_snapshot_ref: optional
  verification_state: pending | integrity_checked | restored_tested | failed
  retention_policy_ref: ref
```

Snapshot должен быть logically consistent. Если разные stores не поддерживают общую transaction, manifest фиксирует их watermarks и recovery ordering.

### 12.5. Encryption и key recovery

Backup шифруется до или на trusted backup client.

Поддерживаются:

- несколько wrapped access keys;
- смена пароля без полного re-encryption, если backend позволяет;
- recovery key, который пользователь хранит отдельно;
- device-bound key как дополнительный, но не единственный путь;
- key revocation;
- проверка, что backup не содержит plaintext secrets вне policy.

Restic является полезным reference: encryption является first-class, а один repository может иметь несколько независимо управляемых access keys. [[S29]]

### 12.6. Restore verification

#### Integrity check

Проверяет object hashes, manifests и repository structure.

#### Test restore

В отдельную временную область восстанавливаются:

- critical registries;
- выборка памяти;
- project manifest;
- случайные artifacts;
- encrypted object.

#### Semantic smoke test

Проверяется:

- открывается ли Memory Fabric;
- восстанавливаются ли references;
- не потеряны ли Task/Effect receipts;
- можно ли запустить safe read-only Dennett;
- известны ли missing external dependencies.

#### Full drill

Периодически или перед крупным upgrade пользователь может выполнить полный restore на другом узле.

### 12.7. RPO и RTO

Dennett не навязывает одинаковые цифры.

- **RPO** — сколько последних данных допустимо потерять;
- **RTO** — сколько времени допустимо восстанавливаться.

Profiles:

- personal default;
- high-value project;
- travel/offline;
- rigorous archive.

UI позднее покажет человеческую формулировку, например: «последний проверенный backup — 8 часов назад; полный restore проверялся 21 день назад».

### 12.8. Backup failure

Backup считается unhealthy, если:

- job не запускался;
- target недоступен;
- snapshot неполон;
- integrity check failed;
- restore test просрочен;
- key recovery не проверен;
- storage retention неожиданно изменилась.

Значимая проблема создаёт Action Inbox card, а не только строку в логах.

---

## 13. Переносимость

### 13.1. Переносимость проекта

Project Memory Pack, repository, artifacts и capability requirements передаются по правилам Memory/Capability Fabric. Server Runtime:

- обнаруживает pack;
- монтирует его в Project Memory Space;
- разрешает недостающие capabilities;
- не переносит secrets;
- сохраняет provenance;
- регистрирует local replicas.

### 13.2. Перенос всей установки на новый сервер

```text
зарегистрировать новый trusted head candidate
→ проверить version/schema/storage
→ выполнить initial snapshot transfer
→ догнать incremental logs
→ провести read-only validation
→ временно заморозить global effect creation
→ передать in-flight state и effect ledger
→ выдать новый epoch
→ переключить devices
→ выполнить post-migration checks
→ сохранить rollback window
→ удалить/понизить старый Head после подтверждения
```

### 13.3. Installation Export

Экспорт может включать:

- canonical manifests;
- Memory data согласно scope;
- projects и portable packs;
- artifacts;
- Task/Run history;
- capabilities/config references;
- provider/account identifiers без secret values;
- encrypted vault export по отдельной явной процедуре;
- backup metadata;
- schema/protocol versions.

Форматы должны быть документированы и по возможности человекочитаемы или открыто специфицированы.

### 13.4. Portable USB client

Portable client — поздняя функция. Default — не полный сервер на флешке, а:

- подписанный portable binary;
- encrypted connection profile;
- bootstrap identity flow;
- возможность подключиться к Head;
- ограниченный локальный cache по выбору;
- offline capture/notes/task queue;
- emergency read-only memory subset;
- отсутствие постоянных secrets в открытом виде;
- очистка local traces после завершения.

На доверенном мощном компьютере portable client может предложить временный execution node. Он не получает full head authority автоматически.

### 13.5. Emergency head на переносном носителе

Возможен только если portable bundle содержит:

- достаточно свежий critical snapshot;
- encrypted keys;
- compatible runtime;
- явное сильное подтверждение пользователя;
- новый epoch через witness или manual takeover;
- понятные ограничения split-brain.

Иначе portable режим остаётся локальным client/emergency notebook.

### 13.6. Vendor portability

Provider session может быть непереносима, но Dennett-owned state должно позволять:

- создать новый provider attempt;
- восстановить goal/checkpoint/artifacts;
- заменить backup backend;
- заменить model runtime;
- экспортировать memory и project data;
- отказаться от конкретного cloud.

---

# Часть IV. Эксплуатация, наблюдаемость и проверка жизнеспособности

## 14. System maintenance, sleep и обновления

### 14.1. System Sleep — бюджетное окно обслуживания

Server Runtime не определяет смысл memory consolidation или skill evolution. Он предоставляет единый scheduling layer, который:

- собирает maintenance candidates;
- оценивает приоритет и ожидаемую пользу;
- выделяет budget;
- учитывает idle devices/providers;
- запускает задачи;
- preempts их ради interactive work;
- сохраняет outcomes и regressions.

Кандидаты:

- memory consolidation;
- index maintenance;
- capability/skill eval;
- provider catalogue refresh;
- backup verification;
- cleanup expired events;
- retrospective;
- restore drill;
- object compaction;
- health probing.

### 14.2. Не сканировать весь мир каждую ночь

Maintenance queue строится по сигналам:

- новые/изменённые данные;
- высокочастотный usage;
- жалобы;
- stale projection;
- failed probe;
- approaching retention deadline;
- low confidence;
- незавершённый migration;
- доступный дешёвый compute window.

### 14.3. Compaction

Compaction может:

- объединять acknowledged local log segments;
- создавать checkpoints;
- удалять expired telemetry;
- дедуплицировать media;
- переводить objects между hot/warm/cold;
- compact durable histories по поддерживаемой процедуре.

Она не должна удалять evidence, required audit или unresolved conflicts.

### 14.4. Protocol и schema compatibility

Каждый node сообщает поддерживаемые versions.

Правила:

- backward-compatible clients продолжают работу;
- несовместимый старый client получает read-only/degraded mode;
- migration может быть resumable;
- destructive migration требует verified backup;
- in-progress Managed Runs привязаны к совместимой runtime version;
- provider/session state не переписывается без adapter migration;
- rollback plan обязателен для critical storage changes.

### 14.5. Safe mode

После repeated crash, failed migration или corruption Head может запуститься в safe mode:

- read-only critical data;
- никаких background agents;
- connectors не отправляют effects;
- secret broker locked или restricted;
- доступна диагностика и restore;
- local project files не удаляются.

---

## 15. Observability, repair и пользовательское понимание

### 15.1. Наблюдаемость нужна для восстановления, а не ради dashboard

Без неё нельзя различить:

- provider outage;
- stale client;
- dead Run;
- lost acknowledgement;
- sync lag;
- split-brain;
- failed backup;
- index lag;
- permission rejection;
- реальный failure агента.

### 15.2. Основные показатели

#### Head и устройства

- current head/epoch;
- lease remaining;
- node last seen;
- sync watermarks;
- protocol versions;
- head candidate readiness;
- clock skew observation;
- storage pressure.

#### Runtime

- active/queued/waiting Runs;
- last meaningful progress;
- checkpoint age;
- cancellation latency;
- provider/session failures;
- resource claims;
- budget usage.

#### Events

- ingest rate;
- durable queue lag;
- source watermarks;
- duplicate rate;
- late events;
- semantic evaluation rate;
- trigger false positives;
- expired/coalesced events.

#### Effects

- prepared/dispatching/unknown effects;
- duplicate prevention;
- reconciliation time;
- compensation state;
- stale-epoch rejects.

#### Sync

- pending operations;
- conflict count;
- object transfer backlog;
- deletion obligations;
- last full sync;
- local-only unsafeguarded data.

#### Backup

- last successful snapshot;
- last integrity check;
- last test restore;
- RPO gap;
- key recovery status;
- target health.

### 15.3. Health states

Installation health не сводится к зелёному/красному.

- `healthy`;
- `degraded`;
- `offline-capable`;
- `head-unavailable`;
- `split-brain-risk`;
- `recovery-required`;
- `backup-at-risk`;
- `migration-incomplete`;
- `safe-mode`.

### 15.4. Repair objects

Повторяющаяся проблема создаёт structured repair item:

- что нарушено;
- impact;
- evidence;
- автоматическое исправление;
- безопасный preview;
- требуется ли пользователь;
- rollback;
- verification.

Не каждое warning становится Action Inbox card. Card создаётся, когда пользователь действительно способен или обязан принять решение.

### 15.5. Корреляция traces

Server связывает correlation IDs:

- user/voice request;
- Task/Run;
- memory query/influence;
- permission decision;
- capability/backend;
- device command;
- external effect;
- notification/card;
- outcome.

Это позволяет ответить: «почему Dennett это сделал и где именно возникла ошибка?» без сохранения скрытого chain-of-thought.

---

## 16. Сквозные сценарии

### 16.1. Обычный project chat на ПК

1. Desktop подключён к Head.
2. Пользователь открывает Project Session.
3. Head получает authoritative project/session config.
4. Project agent может выполняться на desktop, server или provider runtime.
5. Files остаются authoritative в worktree.
6. Progress streaming идёт в UI; durable state минимален.
7. Значимые artifacts/memory commits синхронизируются.
8. При кратком disconnect session продолжает локально/буферизуется.

Никакой отдельный workflow не создаётся.

### 16.2. Фоновая задача переживает restart

1. Агент повышает работу до Managed Run.
2. Runtime сохраняет checkpoint и effect ledger.
3. Server падает.
4. После restart Run восстанавливается в `waiting/recovery`.
5. Provider session продолжается или создаётся новый attempt.
6. Уже подтверждённые действия не повторяются.
7. Агент продолжает с checkpoint.

### 16.3. Серверный агент использует Windows

1. Run требует local Windows capability.
2. Scheduler выбирает online trusted node.
3. Trust выдаёт task-scoped grant.
4. Device Gateway отправляет command.
5. Node выполняет shell/computer-use.
6. Result и receipt возвращаются Head.
7. Большой artifact передаётся object channel или остаётся локально с reference.
8. При user takeover command pause становится authoritative.

### 16.4. Телефон offline сохраняет идею

1. Пользователь делает фото и голосовую заметку.
2. Телефон создаёт synchronized local event и object.
3. Локальный overlay сразу позволяет работать с записью.
4. При reconnect events загружаются, dedupe и связываются.
5. Media получает выбранную replication policy.
6. Memory projections обновляются.
7. Никакой устаревший trigger не выполняется автоматически без revalidation.

### 16.5. Два устройства изменили одну заметку

1. Обе операции ссылаются на одну base revision.
2. Head видит concurrent edits.
3. Непересекающиеся части объединяются автоматически.
4. Смысловой конфликт сохраняет обе версии.
5. Модель предлагает merge, но не уничтожает originals.
6. При material conflict появляется решение в проекте/Inbox.

### 16.6. Timeout после отправки Telegram

1. Effect Claim создан до отправки.
2. Connector отправляет message, response теряется.
3. Claim становится `UNKNOWN`.
4. Restart/failover не повторяет send.
5. Reconciler проверяет thread/provider message ID.
6. После обнаружения message создаётся confirmed receipt.
7. Если проверить нельзя, пользователь видит unknown state.

### 16.7. Основной сервер потерян

1. Devices замечают lease expiry.
2. Если witness доступен, наиболее свежий trusted candidate получает новый epoch.
3. Если witness нет, laptop запускает Isolated Emergency Head.
4. Local project work продолжается; global external effects ограничены.
5. После появления связи выполняется reconciliation.
6. Старый Head с меньшим epoch fenced.

### 16.8. Backup есть, но не восстанавливается

1. Integrity/test restore обнаруживает ошибку.
2. Backup health становится `at-risk`.
3. Используется другой snapshot/target.
4. Создаётся repair item и при необходимости Action Inbox card.
5. Новые backups не считаются достаточными, пока restore test не пройдёт.

### 16.9. Provider quota исчерпан

1. Capability health становится quota-limited.
2. Foreground direct chat не переключается молча.
3. Background run использует заранее разрешённый fallback или ждёт.
4. Scheduler перераспределяет низкоприоритетную работу.
5. Task state сохраняется независимо от provider.

### 16.10. Удаление чувствительного объекта при offline node

1. Head фиксирует deletion obligation/tombstone.
2. Online replicas удаляют bytes/derived indexes.
3. Offline node остаётся pending target.
4. При reconnect tombstone применяется до upload старых operations.
5. Устаревшая копия не воскресает объект.
6. Backup retention и residual limitations отражаются пользователю.

---

## 17. Пошаговое внедрение без оверинжиниринга

### Phase 1 — один Head и базовые clients

Сделать:

- standalone/personal server runtime;
- device registration/heartbeat;
- secure control channel;
- Runtime Registry;
- project session routing;
- simple priority scheduler;
- basic Action Inbox/Radar projections;
- manual backup/export.

Не делать:

- automatic failover;
- generic CRDT;
- полноценный durable workflow engine;
- direct peer object mesh;
- portable head.

### Phase 2 — Managed Runs и effects

Сделать:

- checkpoints;
- durable waits;
- cancellation/recovery;
- effect claims/receipts;
- idempotency/reconciliation;
- provider/device retry semantics;
- notification delivery state.

### Phase 3 — offline sync

Сделать:

- signed local operation logs;
- watermarks;
- recent overlay;
- conflict-preserving note merge;
- object replication policies;
- offline command revalidation;
- project/file sync adapters.

### Phase 4 — events and proactivity

Сделать:

- event intake;
- durable schedules;
- deterministic subscriptions;
- semantic candidate evaluation;
- cooldown/expiry;
- annoyance-aware notifications;
- missed/late event policies.

### Phase 5 — verified backup and migration

Сделать:

- encrypted incremental snapshots;
- independent targets;
- integrity checks;
- automated test restore;
- server migration/handoff;
- key recovery flow.

### Phase 6 — failover and portable modes

Сделать после реального спроса:

- witness/lease coordination;
- Authority Epoch/fencing;
- full head candidate;
- Isolated Emergency Head;
- portable client;
- split-brain recovery suites.

### Phase 7 — optional advanced sync/HA

Только при доказанной необходимости:

- CRDT collaborative notes;
- peer-to-peer object plane;
- multi-node consensus profile;
- organization/team deployment;
- geographically distributed replicas.

---

## 18. Acceptance, rejection и самокритика

### 18.1. Решение принимается, если

- обычный project chat остаётся быстрым и не превращается в workflow;
- server restart не теряет Managed Run;
- timeout не дублирует consequential effect;
- offline capture и project work полезны;
- conflict не исчезает молча;
- stale Head не может продолжать accepted writes после нового epoch;
- пользователь понимает границы emergency mode;
- backup реально восстанавливается;
- перенос на другой server не требует потери памяти;
- Radar отражает runtime, а не самоотчёт агента;
- semantic event evaluation не съедает постоянные токены;
- система может работать одной машиной в ранней версии.

### 18.2. Решение возвращается на переработку, если

- требуется LLM для маршрутизации каждого event;
- каждый model call становится Task/Run history;
- UI хранит параллельное authoritative state;
- sync использует blind last-write-wins для содержательных конфликтов;
- CRDT навязывается permissions, code и effects;
- backup никогда не проверяется restore;
- старый Head может писать после failover;
- offline queue выполняется без revalidation;
- `UNKNOWN` effect автоматически повторяется;
- сервер обязан проксировать все media и local compute;
- logical modules требуют обязательных микросервисов;
- HA cluster нужен для запуска персональной версии;
- пользователь не может экспортировать данные и сменить Head.

### 18.3. Главные риски предложенной модели

#### Head остаётся точкой временной недоступности

Смягчение: offline nodes, backup, planned handoff, поздний witness failover. Полностью устранить это без дополнительной coordination infrastructure нельзя.

#### Типоспецифичная sync logic сложнее одного общего механизма

Это осознанная сложность. Один механизм был бы проще в коде, но неверен по semantics. Ограничение: первая версия поддерживает небольшой набор data classes и готовые adapters.

#### Event system может засориться

Смягчение: ephemeral telemetry, candidate filtering, retention, backpressure и no-op как нормальный outcome.

#### Device nodes увеличивают attack surface

Identity, trust и permissions определены Trust Fabric. Server документ не ослабляет их ради удобства.

#### Backup может стать дорогим

Rebuildable data, dedupe, incremental snapshots и tier policies уменьшают стоимость. Пользователь контролирует media retention.

#### Emergency head может создать конфликт

Без witness строгая гарантия невозможна. Поэтому emergency mode ограничен и честно помечен.

### 18.4. Что намеренно не утверждается

Документ не утверждает, что:

- Temporal, Restate или конкретный engine обязательно будет выбран;
- CRDT является лучшим backend заметок;
- Head должен жить только в cloud или только дома;
- exactly-once достижим для любого provider;
- phone обязан быть full head;
- любой project folder должен синхронизироваться Dennett;
- все события должны быть event-sourced навсегда;
- один backup backend достаточен;
- автоматический failover всегда полезнее ручного.

---

# Часть V. Передача в архитектуру и источники исследования

## 19. Требования к будущей программной архитектуре

Будущая архитектура должна позволить:

1. запустить все logical modules в одном процессе для ранней версии;
2. позднее вынести тяжёлые части без изменения contracts;
3. заменить storage и queue implementation;
4. выбрать или не выбрать durable workflow engine;
5. использовать готовые file-sync и backup backends;
6. иметь один active Head и fencing epoch;
7. исполнять capabilities на remote/local devices;
8. хранить per-data consistency policies;
9. поддерживать signed offline logs и watermarks;
10. атомарно связывать state mutation с event/effect outbox;
11. сохранять idempotency state;
12. восстанавливать Managed Runs;
13. lazy-transfer large objects;
14. поддерживать safe protocol/schema migration;
15. выполнять automated test restore;
16. экспортировать installation/project data;
17. показывать freshness и degraded state приложениям;
18. не требовать network для локально разрешённой работы;
19. обеспечивать emergency stop и revoke из Trust Fabric;
20. хранить достаточную telemetry для root-cause без hidden chain-of-thought.

---

## 20. Каталог источников исследования

### Внутренние спецификации Dennett

**[S01] Dennett Functional Concept.** Видение продукта, серверное существование, устройства, backup, интерфейсы и исходные сценарии.  
`00_Dennett_Functional_Concept.md`

**[S02] Dennett Specification Index and Shared Contracts.** Ownership документов, общие envelopes, sources of truth и границы Server Runtime.  
`01_Dennett_Specification_Index_and_Shared_Contracts.md`

**[S03] Dennett Memory Fabric 1.2.** Event ledger, evidence, project memory, offline log, consistency, deletion и portable memory.  
`10_Dennett_Memory_Fabric.md`

**[S04] Dennett Pragmatic Agentic Control Fabric 1.1.** Strong-agent-first модель, Managed Run, proportional durability, checkpoints и external effects.  
`20_Dennett_Agentic_Control_Fabric.md`

**[S05] Dennett Trust, Identity, Autonomy and Permissions.** Device trust, Reference Monitor, grants, Secret Broker, computer-use boundaries и effect authorization.  
`30_Dennett_Trust_Identity_Autonomy_and_Permissions.md`

**[S06] Dennett Capabilities, Providers and Integrations 1.1.** Provider/runtime lifecycle, connectors, device capabilities, health, quota и fallback.  
`41_Dennett_Capabilities_Providers_and_Integrations.md`

### Durable execution и effects

**[S10] Temporal Workflow Definition.** Deterministic replay, Activities для внешних операций, recovery после Worker/Service outage и workflow versioning.  
https://docs.temporal.io/workflow-definition

**[S11] Restate documentation.** Journaling, durable execution, virtual objects/single-writer state, retries и suspension. Используется как альтернативный reference, а не выбранный стек.  
https://docs.restate.dev/

**[S12] AWS Builders’ Library — Making retries safe with idempotent APIs.** Caller request IDs, atomic token+mutation, semantic-equivalent responses и parameter mismatch.  
https://aws.amazon.com/builders-library/making-retries-safe-with-idempotent-APIs/

**[S13] Stripe Idempotent Requests.** Практическая модель повторяемого API-запроса с ключом и проверкой parameters.  
https://docs.stripe.com/api/idempotent_requests

**[S14] NATS JetStream Consumers.** Stateful consumers, acknowledgements и at-least-once delivery.  
https://docs.nats.io/nats-concepts/jetstream/consumers

**[S15] CloudEvents.** Общая минимальная форма описания событий и interoperability.  
https://cloudevents.io/

**[S16] Apache Flink — Timely Stream Processing.** Event time, processing time, watermarks и late events.  
https://nightlies.apache.org/flink/flink-docs-master/docs/concepts/time/

**[S17] Debezium Outbox Event Router.** Transactional outbox как связь state mutation и event publication.  
https://debezium.io/documentation/reference/stable/transformations/outbox-event-router.html

### Event-driven и distributed control plane

**[S18] Home Assistant Core Architecture and Events.** Event Bus, State Machine, Service Registry и flexible event model.  
https://developers.home-assistant.io/docs/architecture/core/  
https://developers.home-assistant.io/docs/dev_101_events/

**[S19] Tailscale — How it works.** Централизованный coordination server при distributed encrypted data plane и policy enforcement на nodes.  
https://tailscale.com/blog/how-tailscale-works

**[S25] Kubernetes Leases.** Heartbeats, leader election и один active control-plane component.  
https://kubernetes.io/docs/concepts/architecture/leases/

**[S26] Kubernetes Control Plane–Node Communication.** Hub-and-spoke control API, node-initiated secure communication и control-to-node channels.  
https://kubernetes.io/docs/concepts/architecture/control-plane-node-communication/

**[S27] Martin Kleppmann — How to do distributed locking.** Различие locks для efficiency/correctness и необходимость monotonic fencing tokens.  
https://martin.kleppmann.com/2016/02/08/how-to-do-distributed-locking.html

**[S28] The Raft Consensus Algorithm.** Reference для позднего HA deployment profile; не обязательный механизм первой версии.  
https://raft.github.io/

### Local-first и sync

**[S20] Ink & Switch — Local-first software.** Offline, ownership, collaboration, privacy и CRDT trade-offs.  
https://www.inkandswitch.com/essay/local-first/

**[S21] Figma — How multiplayer technology works.** Offline reapply, CRDT-inspired collaboration и отдельные sync systems для разных data types.  
https://www.figma.com/blog/how-figmas-multiplayer-technology-works/

**[S22] Yjs.** Network-agnostic CRDT shared types и offline merge для collaborative data.  
https://docs.yjs.dev/

**[S23] Automerge.** Local-first JSON-like CRDT и sync protocol.  
https://github.com/automerge/automerge

**[S24] Syncthing Synchronization and File Versioning.** Conflict copies, cautious file sync и staggered/versioned retention.  
https://docs.syncthing.net/users/syncing.html  
https://docs.syncthing.net/users/versioning.html

**[S32] CouchDB Replication Protocol.** Incremental replication, change tracking и conflict-preserving replication reference.  
https://docs.couchdb.org/en/stable/replication/protocol.html

**[S33] ElectricSQL.** Local-first sync engine and partial replication reference.  
https://github.com/electric-sql/electric

**[S34] PowerSync.** Offline-first client databases and server synchronization reference.  
https://github.com/powersync-ja/powersync-js

**[S35] Replicache.** Client-side optimistic/local-first sync pattern reference.  
https://github.com/rocicorp/replicache

**[S36] Linear — Scaling the Linear Sync Engine.** Production experience with a realtime client sync engine and evolving API.  
https://linear.app/now/scaling-the-linear-sync-engine

### Backup и recovery

**[S29] restic documentation.** Encrypted content-addressed backup, multiple repository keys, snapshots и integrity operations.  
https://restic.readthedocs.io/

**[S30] BorgBackup documentation.** Deduplicated encrypted archives, repository checks и extraction.  
https://borgbackup.readthedocs.io/

**[S31] GitLab database incident/postmortem.** Практический failure case неработающих backup-путей, отсутствия проверенного restore и необходимости автоматических restore tests/ownership.  
https://about.gitlab.com/blog/2017/02/01/gitlab-dot-com-database-incident/

---

## 21. Финальная нормативная формула

> **Server Runtime Dennett должен быть единым логическим персональным control plane, но не единственной вычислительной и файловой машиной. Он сохраняет authoritative coordination state, долговечно поддерживает только ту работу, которой это нужно, принимает и фильтрует события, синхронизирует разные данные по различным контрактам, предотвращает повтор внешних эффектов, честно ограничивает offline и split-brain режимы, проверяет backups восстановлением и позволяет перенести систему на другое доверенное устройство без потери пользовательской памяти и проектов.**

Конец документа.

---

# Repository integration decision: Head eligibility and canonical memory

## Runtime-семантика

`DeviceManifest.head_eligibility` принимает `none | emergency | full` и по умолчанию равен `none`. Runtime никогда не выводит eligibility из мощности hardware, прежней связности или самого наличия локального кэша.

Плановое или автоматическое повышение выполняет цепочку: owner/policy authorization → readiness компонентов → готовность канонических данных или восстановления → доступность ключей → lease/witness checks → новый Authority Epoch → fencing старого Head → semantic health checks. Автоматический failover использует только заранее разрешённые `full`-кандидаты.

`emergency`-кандидат может запустить изолированный ограниченный Head для локальных проектов и queued work, но помечает глобальное состояние как неполное и оставляет high-risk shared effects отключёнными, если отдельная явная policy не разрешает их.
