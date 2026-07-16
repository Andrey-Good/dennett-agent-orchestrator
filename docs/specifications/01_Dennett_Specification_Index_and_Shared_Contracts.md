# Dennett Specification Index and Shared Contracts

> **Repository edition · 2026-07-13 · `01`**  
> Это самостоятельный канонический документ репозитория Dennett. Начните с [карты документации](../README.md).  
> Related: [00_Dennett_Functional_Concept.md](./00_Dennett_Functional_Concept.md).

## Интегрированные contract supplements

Следующие небольшие нормативные документы выделены из предархитектурного gap-аудита. Они являются частью текущего набора и обязательны для изменений, пересекающих указанные границы:

- [`00_Shared_Cross_Domain_Rules.md`](contracts/00_Shared_Cross_Domain_Rules.md)

Эти supplements не дают одному lifecycle разойтись по нескольким большим файлам; canonical owner указан в заголовке каждого документа.


## Каноническая карта спецификации, общий словарь, границы ответственности и минимальные сквозные контракты

**Версия:** 1.0  
**Дата:** 11 июля 2026 года  
**Статус:** канонический метадокумент бизнес-логики.  
**Область:** документация Dennett до перехода к программной архитектуре.

Этот файл не описывает ещё одну подсистему Dennett. Он определяет, **какой документ чем владеет**, как разрешаются противоречия, какие термины имеют единое значение и какие минимальные логические контракты должны сохраняться на границах памяти, агентов, доверия, голосового режима, capabilities, сервера и приложений.

Он продолжает и связывает:

- `00_Dennett_Functional_Concept.md`;
- `10_Dennett_Memory_Fabric.md`;
- `20_Dennett_Agentic_Control_Fabric.md`;
- `README.md`.

---

# 0. Итоговый вердикт

## 0.1. Зачем нужен этот документ

По мере развития Dennett одни и те же слова начали встречаться в нескольких местах: «агент», «задача», «событие», «память проекта», «разрешение», «сессия», «результат», «состояние». Без единой карты поздние документы неизбежно начнут:

- давать одной сущности разные значения;
- создавать конкурирующие источники истины;
- повторно описывать уже принятую бизнес-логику;
- возвращать отклонённые идеи из старых версий;
- смешивать пользовательский интерфейс с каноническим состоянием;
- принимать содержимое памяти или prompt за реальное разрешение;
- превращать удобную проекцию в фундамент системы;
- усложнять Direct Turn так, будто это банковская транзакция.

Этот файл предотвращает расхождение, но сам не должен стать бюрократическим центром. Его задача — закрепить **несколько общих инвариантов и лёгкие границы**, а не заставить каждое действие Dennett проходить через один гигантский универсальный протокол.

## 0.2. Главный принцип спецификации

> **Каждое правило имеет одного канонического владельца. Остальные документы могут применять или пояснять его, но не создают конкурирующую версию.**

При этом:

- обзорный документ определяет продуктовую цель, а не низкоуровневую бизнес-логику;
- специализированный документ уточняет обзор и имеет приоритет в своей области;
- более новая совместимая версия документа заменяет более старую версию в своей области;
- общие сквозные контракты задаются здесь, но предметная семантика остаётся в доменном документе;
- неизвестное противоречие не разрешается молча — оно регистрируется как открытый вопрос спецификации.

## 0.3. Главный принцип минимальности

Общий контракт не означает, что каждая операция обязана создавать полный долгоживущий объект.

Например:

- короткий ответ проектного агента может остаться Direct Turn;
- чтение файла не становится отдельной Task;
- обычное сообщение между bounded subagent и lead может иметь только минимальный transport envelope;
- свободное наблюдение памяти не обязано получать отдельную строгую semantic schema;
- provider-native computer-use не оборачивается в собственный сложный движок без причины.

Полная форма контракта требуется, когда работа должна быть:

- долговечной;
- наблюдаемой;
- отменяемой;
- восстанавливаемой;
- разделяемой между подсистемами;
- связанной с внешним эффектом;
- проверяемой;
- переносимой между устройствами или пользователями.

---

# 1. Область и не-цели

## 1.1. Что определяет файл

Документ определяет:

- реестр канонических спецификаций;
- статус и приоритет документов;
- общий словарь Dennett;
- владельца каждой крупной сущности и правила;
- общие источники истины;
- минимальные логические envelopes на границах подсистем;
- правила ссылок, времени, scope, provenance и версий;
- общую модель конфигурации и precedence;
- правила документирования изменений;
- список открытых вопросов;
- критерии готовности комплекта к архитектурному проектированию.

## 1.2. Что файл не определяет

Он не выбирает:

- язык программирования;
- СУБД;
- workflow engine;
- очередь сообщений;
- RPC или REST;
- формат физических таблиц;
- конкретный desktop/mobile framework;
- cloud provider;
- контейнерный runtime;
- криптографическую библиотеку;
- конкретный способ сериализации каждого объекта.

Примеры YAML и полей в документах — **логические контракты**, а не обязательная физическая схема хранения.

## 1.3. Lowest sufficient layer

Перед созданием новой сущности, файла или подсистемы следует проверять следующий порядок:

1. ничего не добавлять;
2. уточнить prompt или инструкцию;
3. создать skill/procedure;
4. использовать provider-native capability;
5. добавить лёгкий adapter;
6. добавить минимальную runtime-запись или guard;
7. создать переиспользуемую подсистему;
8. создать отдельный сервис или самостоятельный крупный документ.

Переход выше оправдан только конкретной потребностью в надёжности, наблюдаемости, переносимости, безопасности или повторном использовании.

---

# 2. Реестр канонических документов

## 2.1. Текущий канонический комплект

### `00_Dennett_Functional_Concept.md`

**Текущий исходный файл:** `00_Dennett_Functional_Concept.md`  
**Владеет:** видением продукта, ценностями, пользовательскими возможностями, общей моделью Dennett и намерениями владельца проекта.  
**Не владеет:** окончательной логикой памяти, агентов, безопасности, voice runtime, серверного состояния и UI-контрактов.

При конфликте с более новым специализированным документом функциональная концепция трактуется как исходная цель, а специализированный файл — как актуальный способ её достижения.

### `01_Dennett_Specification_Index_and_Shared_Contracts.md`

**Владеет:** картой спецификации, общим словарём, document ownership, cross-domain envelopes, правилами precedence и изменениями документации.  
**Не владеет:** предметной логикой отдельных подсистем.

### `10_Dennett_Memory_Fabric.md`

**Текущий исходный файл:** `10_Dennett_Memory_Fabric.md`  
**Владеет:** долговременной и рабочей памятью, evidence, events памяти, claims, current state, Causal Trace, Prospective Intent, retrieval, context composition, federated spaces, portable project memory, deletion и memory influence.  
**Не владеет:** выдачей permissions, жизненным циклом Agent Run, UI памяти или физической архитектурой хранения.

### `20_Dennett_Agentic_Control_Fabric.md`

**Текущий исходный файл:** `20_Dennett_Agentic_Control_Fabric.md`  
**Владеет:** главным оркестратором, project agent, Direct Turn, Adaptive Agent Session, Work Item, Task, Run, subagents, teams, delegation, review, procedures, Managed Run, selective Structured Automation, completion и пользовательскими execution profiles.  
**Не владеет:** глобальной системой identity/authorization, долговременной памятью, provider catalogue, voice UX и физическим server runtime.

### `30_Dennett_Trust_Identity_Autonomy_and_Permissions.md`

**Владеет:** principals, identity, authentication, device/session trust, authorization, permission envelopes, risk/blast radius, safety floor, secrets, prompt injection, trust domains, действиями от имени пользователя, регулируемой автономностью и audit security decisions.

### `40_Dennett_Voice_and_Ambient_Interaction_Fabric.md`

**Владеет:** voice session, речевым и мыслительным слоями, streaming interaction, ambient behavior, meeting interaction, voice-to-orchestrator/project routing и пользовательской логикой capture.  
**Использует:** Trust для identity/permissions, Capabilities для ASR/TTS/sensors, Server для фонового выполнения, Memory для контекста.

### `41_Dennett_Capabilities_Providers_and_Integrations.md`

**Владеет:** providers, models, subscriptions, provider adapters, skills catalogue, MCP, plugins, computer-use adapters, connectors, speech/vision/web tools, capability health, limits и installation lifecycle.  
**Не владеет:** решением, разрешено ли конкретное действие, или агентной стратегией его применения.

### `50_Dennett_Server_Runtime_Events_Sync_and_Portability.md`

**Владеет:** постоянным control plane, runtime state, event intake, scheduler, durable runs, canonical Action Inbox/Radar state, notifications, sync, offline log, recovery, backup, failover, head-device logic, portable client и system maintenance.

### `60_Dennett_Desktop_Application_Business_Logic.md`

**Владеет:** пользовательскими экранами desktop, кнопками, меню, переходами, состояниями интерфейса, affordances и desktop-specific flows.  
**Не является источником истины** для Task, permission, memory или event state.

### `61_Dennett_Mobile_Application_Business_Logic.md`

**Владеет:** mobile flows, voice launch, widgets, capture, approvals, уведомления, Radar/Project views, privacy controls, offline queue и emergency stop.

### `70_Dennett_End_to_End_Validation_and_Architecture_Handoff.md`

**Владеет:** сквозными сценариями, acceptance criteria, интеграционными противоречиями, latency/scale classes, architecture-neutral service requirements и готовностью к архитектуре.

## 2.2. Неканонические и исторические файлы

Следующие файлы сохраняются как исследовательская история, но не используются как источник актуальной нормы, если противоречат текущей версии:

- `Dennett_memory_logic_v0.2.md`;
- `Dennett_memory_logic_v1.0_memory_fabric_2026.md`;
- `Dennett_memory_logic_v1.1_adaptive_federated_2026.md`;
- `Dennett_agent_orchestration_logic_v1.0_agentic_control_fabric_2026.md`;
- промежуточные версии функциональной концепции;
- `Dennett_функциональная_концепция_v0.1.2_terms_and_codex_base.md`, если она расходится с пользовательской редакцией `project_workflow(2)`;
- `README.md` после того, как этот индекс окончательно закрепит состав файлов.

Исторические документы полезны для rationale и regression analysis. Они не удаляются, но должны быть помечены `deprecated` или перемещены в будущую папку `docs/archive/`.

## 2.3. Приоритет документов

Приоритет разрешения конфликта:

1. явное актуальное решение пользователя;
2. более новый канонический документ в своей области;
3. этот Shared Contracts документ для сквозной формы и ownership;
4. специализированный доменный документ для предметной семантики;
5. функциональная концепция для продуктового намерения;
6. исторические версии и исследовательские материалы;
7. поведение текущей реализации, если оно не подтверждено спецификацией.

Пункты 2–4 не являются простой линейной иерархией. Правило такое:

- **этот файл** определяет, где живёт правило и как подсистемы ссылаются друг на друга;
- **доменный файл** определяет, что правило означает в своей области.

Если оба действительно противоречат, создаётся `Specification Conflict`, а не молчаливое толкование.

---

# 3. Статусы и версии спецификаций

## 3.1. Статусы

Каждый документ имеет один статус:

- `draft` — формируется и может существенно измениться;
- `research-baseline` — прошёл исследование и пригоден как текущая основа, но ожидает интеграционной проверки;
- `canonical` — принят как текущая норма своей области;
- `superseded` — заменён новой версией;
- `deprecated` — хранится только для истории;
- `experimental-appendix` — гипотеза, не являющаяся нормой.

Текущие Memory 1.2 и Agentic 1.1 считаются `canonical business-logic baseline`, пока End-to-End документ не обнаружит требующую ревизии проблему.

## 3.2. Версионирование

Рекомендуемая схема:

- **major** — меняются базовые сущности, ownership или несовместимые инварианты;
- **minor** — добавляется совместимая бизнес-логика, сценарии или derived mechanism;
- **patch** — исправляется формулировка без изменения смысла.

Версия файла не обязана совпадать с версией продукта Dennett.

## 3.3. Normative delta

Новая версия доменного файла должна в начале кратко указывать:

- что она заменяет;
- какие решения изменены;
- что остаётся неизменным;
- как читать унаследованные разделы;
- есть ли migration implications.

## 3.4. Нельзя исправлять проблему только в одном случайном месте

Если решение влияет на несколько документов, change proposal обязан перечислить:

- канонического владельца;
- зависимые документы;
- UI-проекции;
- изменения сценариев;
- migration/compatibility;
- tests, которые должны измениться.

---

# 4. Общий словарь

## 4.1. Система

### Dennett

Весь продукт: персональная агентная операционная система, включая сервер, приложения, память, агентов, голос, capabilities и интеграции.

### Главный оркестратор

Единственный логический управляющий центр Dennett. Он принимает намерения и события, выбирает форму работы, управляет глобальными и межпроектными решениями, но не является одним бесконечным prompt-контекстом и не исполняет каждое локальное действие.

### Runtime

Исполняющая среда, которая хранит значимое состояние процессов, применяет permissions, manages retries/checkpoints и связывает agent/provider с внешним миром. Runtime может быть физически распределён, но это архитектурный вопрос.

## 4.2. Люди, устройства и доверие

### Principal

Субъект, от имени которого существуют данные, permissions или действия: пользователь, другой человек, организация, agent identity, service identity или устройство в строго ограниченных случаях.

### User / владелец

Главный человеческий principal личной установки Dennett. Пользователь является владельцем целей и данных, но конкретное действие всё равно требует подтверждённой identity/session и соблюдения safety floor.

### Device

Зарегистрированный компьютер, сервер, ноутбук, телефон, wearable или portable client с собственным identity, capabilities и trust state.

### Session

Ограниченный во времени и контексте канал взаимодействия principal с Dennett. Типы session уточняются доменными документами: user session, voice session, project session, provider session.

### Trust domain

Граница происхождения и допустимого влияния: personal, project, imported, external-untrusted, provider, shared-team и другие. Trust domain не является простым уровнем «доверено/не доверено»; он ограничивает каналы влияния.

## 4.3. Проекты и работа

### Project

Видимая пользователю рабочая область вокруг папки, репозитория или набора артефактов. Проект может быть кодом, исследованием, презентацией, дизайном, автоматизацией или другой длительной работой.

### Project Session

Живой интерактивный канал пользователя с project agent, обычно с default working directory проекта. Это базовый Codex/Claude Code-like режим.

### Work Item

Небольшая единица работы внутри текущей agent session. Не обязана иметь отдельный долговечный registry object.

### Task

Значимая работа, которая требует самостоятельного состояния: фонового выполнения, ожидания, отдельного owner, бюджета, отмены, восстановления, внешнего эффекта, user visibility или независимого результата.

### Run

Конкретная попытка выполнить Task, Procedure или Automation с выбранным контекстом, provider/model и параметрами.

### Direct Turn

Короткое взаимодействие без отдельной durable Task.

### Adaptive Agent Session

Основной режим: один сильный агент сохраняет цельную картину и свободно меняет план, используя tools, память и bounded helpers.

### Managed Run

Лёгкая долговечная оболочка вокруг автономной работы: отмена, budget, status, checkpoint по необходимости, external-effect log и Result Envelope. Стратегия остаётся у агента.

### Structured Automation

Более формальный повторяемый процесс, оправданный durability, внешними эффектами, массовым масштабом, жёстким порядком или долгими ожиданиями. Не является default.

## 4.4. Агенты и возможности

### Agent Definition

Переиспользуемое описание роли, поведения, доступных capabilities и ограничений.

### Agent Instance

Логический участник конкретной работы.

### Provider Session

Конкретная сессия Codex, Claude Code, OpenAI Agents SDK, локальной модели или другого provider runtime. Не является сама по себе Task.

### Subagent

Bounded Agent Instance, созданный для Minimum Coherent Unit с самостоятельным результатом. Не должен создаваться только ради формального деления.

### Skill / Procedure

Человекочитаемая повторно используемая инструкция или pattern выполнения. Это предпочтительный слой повторяемой работы до создания Structured Automation.

### Capability

То, чем Dennett может воспользоваться: модель, provider, tool, MCP, plugin, connector, computer-use, speech, vision, web search, приложение или device function.

### Tool

Конкретный вызываемый интерфейс capability с определённым effect profile и permission requirements.

## 4.5. Память и контекст

### Memory Event

Каноническая запись о произошедшем или зафиксированном изменении с provenance, time и scope.

### Evidence Object

Первичный или сохранённый объект-доказательство: сообщение, файл, screenshot, tool result, audio segment, document snapshot и так далее.

### Claim

Смысловое утверждение, связанное с evidence и имеющее epistemic status.

### Current State Projection

Пересобираемое представление того, что Dennett должен считать актуальным для конкретного key/scope сейчас.

### Working Memory

Ограниченное состояние активной работы: цель, ограничения, текущий шаг, решения, открытые вопросы, handles и permissions. Не является долговременной истиной.

### Memory Space

Логически изолированное пространство памяти: personal, project, research, task/branch, world intelligence, imported/shared.

### Memory Handle

Стабильная ссылка, позволяющая агенту открыть evidence, claim, episode, procedure или другой объект по необходимости без вставки всего содержимого в prompt.

### Context Manifest

Описание того, какой контекст и почему был дан Agent Run или ответу: обязательные инструкции, current state, evidence, advisory preferences, permissions, untrusted data и handles.

### Causal Trace

История `intent → decision → action → observation → outcome → assessment`.

### Prospective Intent

Память о будущем условии и желаемой реакции. Срабатывание условия не равно разрешению действия.

## 4.6. События, результаты и интерфейсы

### Event

Сигнал, способный вызвать оценку системой: пользовательский ввод, время, сообщение, изменение файла, завершение Run, ambient signal или semantic condition. Результатом оценки может быть no-op.

### External Effect

Изменение за пределами внутреннего рассуждения: отправка сообщения, платёж, удаление, публикация, изменение файла, commit, click в приложении, изменение calendar и так далее.

### Outcome

Машинно значимый итог работы: success, partial, failed, cancelled, no-op, waiting и другие доменно уточнённые значения.

### Artifact

Сохранённый результат работы: файл, документ, изображение, diff, repository state, research dossier, presentation, audio/video, workflow definition и так далее.

### Result Envelope

Минимальный связующий итог значимой работы: outcome, summary, artifacts, evidence, unresolved items, external effects и next owner/action.

### Action Inbox

Пользовательская очередь значимых решений. Каноническое состояние хранит server runtime; причины появления принадлежат агентам, Trust, Events или Voice; отображение принадлежит приложениям.

### Agent Radar

Проекция значимых активных Tasks, Runs, Agents, waits, blockers, budgets и рисков. Не является отдельным источником состояния.

---

# 5. Ownership matrix

## 5.1. Канонический владелец сущностей

| Сущность или правило | Канонический документ | Другие документы могут |
|---|---|---|
| Видение и ценности продукта | Functional Concept | применять и уточнять реализацию |
| Memory Event, Evidence, Claim, retrieval | Memory Fabric | ссылаться и запрашивать |
| Project Session, Work Item, Task, Run | Agentic Control | отображать и исполнять через runtime |
| Agent Definition/Instance, delegation | Agentic Control | предоставлять providers/capabilities |
| Principal, authentication, permission | Trust | инициировать запрос или показывать решение |
| Execution profiles | Agentic Control | Trust накладывает safety floor; UI редактирует |
| Provider/model/tool lifecycle | Capabilities | Agentic выбирает; Trust разрешает вызов |
| Voice Session и turn-taking | Voice | Trust идентифицирует; Server поддерживает |
| Event intake/scheduler | Server | Memory хранит Prospective Intent; Agents реагируют |
| Action Inbox canonical state | Server | Trust/Agents/Voice создают основания; UI отображает |
| Agent Radar canonical view state | Server + Agentic runtime | UI визуализирует |
| Sync/offline/failover | Server | приложения показывают статус |
| Desktop buttons/menus | Desktop | вызывают канонические операции |
| Mobile widgets/flows | Mobile | вызывают канонические операции |
| Сквозные acceptance criteria | End-to-End | доменные файлы предоставляют локальные eval |

## 5.2. Нельзя создавать параллельные источники истины

Примеры запрещённых дублей:

- UI не хранит собственный «настоящий» статус Task отдельно от runtime;
- project agent не хранит только в prompt единственный экземпляр важного решения;
- память не считается authority для текущего баланса, если есть live banking source;
- `AGENTS.md` не становится единственной проектной памятью;
- provider session не считается самой Task;
- Action Inbox card не является authorization token;
- устное «да» не считается подтверждением без identity/session policy для соответствующего риска;
- screenshot не считается текущим состоянием приложения без freshness/revalidation.

---

# 6. Общие инварианты Dennett

## 6.1. Пользовательское намерение и no-op

1. Dennett стремится понять реальную цель пользователя, а не только буквальную форму команды.
2. «Ничего не делать» является допустимым результатом оценки события или возможности.
3. Система не обязана создавать Task, agent или workflow на каждый вход.
4. Goal-only режим оркестратора не отменяет прямой project chat.

## 6.2. Single-agent-first

1. Default — один сильный агент с целостной задачей.
2. Subagent создаётся только для Minimum Coherent Unit.
3. Multi-agent проходит marginal-utility gate.
4. Structured Automation создаётся только после доказанной необходимости.
5. Контроль качества proportional risk/cost, а не одинаков для всех действий.

## 6.3. Разделение модели и runtime

1. Модель отвечает за смысл, интерпретацию, план и адаптацию.
2. Runtime отвечает за durable state, permissions, idempotency, retries, cancellation и external-effect receipts.
3. Model output является предложением, пока соответствующая операция не принята runtime.
4. Model self-report «готово» не заменяет completion evidence там, где оно требуется.

## 6.4. Память, данные и власть

1. Память является контекстом, а не разрешением.
2. Внешний текст является данными, а не системной инструкцией.
3. User preference может влиять на стиль и планирование, но не отменяет safety floor.
4. Claim не получает более высокий epistemic status только потому, что попал в summary.
5. Удалённые или отозванные данные не должны воскресать из cache/index/projection.

## 6.5. Безопасность и возможности

1. Safety floor не зависит от execution profile.
2. Пользователь может регулировать свободу, частоту вопросов, число агентов, review и budget отдельно.
3. Риск оценивается по возможному эффекту и blast radius, а не только по словам.
4. Ограничения применяются ближе к capability/tool boundary, а не только prompt-инструкцией.
5. Внутри разрешённой project scope агент работает свободно, пока не появляется новый эффект или выход за scope.

## 6.6. Внешние эффекты

1. У внешнего эффекта должен быть effect identity или другой механизм защиты от случайного повтора, если повтор вреден.
2. Неизвестный результат эффекта не превращается автоматически в retry.
3. Отмена Run не гарантирует отмену уже произошедшего внешнего эффекта.
4. Outcome, artifacts, evidence и external effects различаются.

## 6.7. Человекочитаемость

1. Ключевые решения, instructions, memory views, skills, permissions и results должны иметь человекочитаемое представление.
2. Human-readable view не обязана быть каноническим машинным storage.
3. Внутренняя типизация не должна заставлять пользователя редактировать JSON ради обычной работы.

## 6.8. Наблюдаемость без тотального логирования

1. Значимые изменения состояния и внешние эффекты наблюдаемы.
2. Hidden chain-of-thought не является обязательным audit artifact.
3. Heartbeats и технический chatter могут иметь короткий retention и не загрязнять память.
4. Пользователь видит значимые процессы, а не каждую микрозадачу.

---

# 7. Минимальные сквозные логические контракты

## 7.1. Общие правила контрактов

Контракты ниже описывают смысловые поля. Реализация может:

- объединить несколько объектов в одном процессе;
- использовать разные serializations;
- опускать необязательные поля;
- хранить compact form для Direct Turn;
- расширять поля namespaced extensions.

Она не может терять критические свойства: identity, scope, provenance, time, effect status и lineage.

## 7.2. Stable Reference

Любая долговечная значимая сущность должна иметь устойчивую ссылку.

```yaml
ref:
  kind: project | task | run | agent | event | evidence | claim | artifact | capability | policy | permission | session | device | other
  id: opaque_stable_id
  version: optional
  scope_hint: optional
```

Требования:

- ID не зависит от отображаемого имени;
- rename не меняет identity;
- version/snapshot указывается там, где объект меняется;
- человеческое имя может быть неуникальным;
- ссылки между документами и runtime должны переживать restart.

## 7.3. Principal and Scope Envelope

```yaml
scope:
  owner_principal_id: id
  acting_principal_id: optional
  project_id: optional
  task_id: optional
  run_id: optional
  device_id: optional
  trust_domain: id
  visibility: private | project | shared | public | restricted
  policy_refs: []
```

Не все поля требуются для каждого объекта. Обязательны owner и trust domain для долговременных данных или действия от имени principal.

## 7.4. Temporal Envelope

```yaml
time:
  occurred_at: optional
  observed_at: optional
  committed_at: required_for_durable_record
  processed_at: optional
  valid_from: optional
  valid_to: optional
  expires_at: optional
  last_verified_at: optional
```

Один `created_at` недостаточен для событий, current state, imported data и permissions.

## 7.5. Correlation and Causality

```yaml
correlation:
  correlation_id: optional
  parent_refs: []
  caused_by_refs: []
  conversation_id: optional
  idempotency_key: optional
```

Используется для связывания input → Task/Run → tool actions → Result → memory commit.

## 7.6. Event Envelope

```yaml
event:
  event_id: id
  event_type: namespaced_string
  source_ref: ref
  scope: scope
  time: time
  payload: open_or_typed
  evidence_refs: []
  causality: correlation
  sensitivity: optional
  processing_status: optional
```

Правила:

- payload может быть свободным или типизированным;
- event не обязан создавать работу;
- raw external content сохраняет untrusted provenance;
- повторная доставка не должна незаметно создавать дубли значимых действий;
- доменный документ определяет типы событий и их обработку.

## 7.7. Action Request

```yaml
action_request:
  action_id: id
  requested_by: principal_or_agent_ref
  on_behalf_of: principal_ref
  goal: text
  capability_ref: optional
  tool_ref: optional
  target_refs: []
  proposed_parameters: open_or_typed
  scope: scope
  expected_effects: []
  reversibility: reversible | compensatable | irreversible | unknown
  risk_hint: optional
  evidence_refs: []
  permission_context_ref: optional
```

Модель может создать proposal. Исполнение начинается только после capability resolution и Trust decision.

## 7.8. External Effect Receipt

```yaml
external_effect:
  effect_id: id
  action_id: id
  capability_or_tool_ref: ref
  target_refs: []
  started_at: time
  completed_at: optional
  status: not_started | in_progress | succeeded | failed | unknown | compensated
  provider_receipt: optional
  idempotency_key: optional
  before_refs: []
  after_refs: []
  verification_refs: []
```

Если статус `unknown`, система сначала выполняет reconciliation, а не слепой retry.

## 7.9. Permission Request and Decision

```yaml
permission_request:
  request_id: id
  actor_ref: ref
  on_behalf_of: principal_ref
  action_ref: ref
  requested_capabilities: []
  target_scope: scope
  reason: text
  expected_effects: []
  risk_assessment_ref: optional
  alternatives: []
  requested_duration_or_use_count: optional
```

```yaml
permission_decision:
  decision_id: id
  request_id: id
  outcome: allow | allow_with_constraints | ask_user | deny | expired
  constraints: []
  granted_by: principal_or_policy_ref
  valid_scope: scope
  valid_until: optional
  remaining_uses: optional
  audit_reason: text
  evidence_refs: []
```

Предметная семантика и требования к подтверждению задаются Trust document.

## 7.10. Result Envelope

```yaml
result:
  result_id: id
  work_ref: task_or_run_or_turn_ref
  outcome: success | partial | failed | cancelled | no_op | waiting | unknown
  summary: text
  artifact_refs: []
  evidence_refs: []
  external_effect_refs: []
  unresolved_items: []
  verification_refs: []
  next_owner_ref: optional
  suggested_next_actions: []
  memory_commit_refs: []
```

Для Direct Turn envelope может существовать только логически и не сохраняться отдельным объектом. Для Managed Run он долговечен.

## 7.11. Artifact Descriptor

```yaml
artifact:
  artifact_id: id
  kind: file | document | image | audio | video | code_diff | repository_state | research | workflow | other
  owner_scope: scope
  created_by_ref: ref
  version_ref: optional
  content_location_or_handle: ref
  media_type: optional
  status: draft | candidate | accepted | final | archived | deleted
  provenance_refs: []
  evidence_refs: []
  sensitivity: optional
```

Сам blob или файл может храниться отдельно.

## 7.12. Error and Incident Envelope

```yaml
error:
  error_id: id
  category: user_input | provider | tool | permission | security | state | sync | timeout | validation | unknown
  scope: scope
  operation_ref: optional
  message: human_readable
  technical_details_ref: optional
  retryability: safe | unsafe | after_reconciliation | unknown
  user_action_required: boolean
  partial_artifact_refs: []
  external_effect_refs: []
  occurred_at: time
```

Ошибка не должна терять частичный полезный результат. Technical details не обязаны показываться пользователю по умолчанию.

## 7.13. Context Manifest

```yaml
context_manifest:
  manifest_id: id
  actor_or_run_ref: ref
  goal: text
  instruction_refs: []
  current_state_refs: []
  evidence_refs: []
  historical_refs: []
  advisory_preference_refs: []
  procedure_refs: []
  untrusted_content_refs: []
  permission_envelope_ref: optional
  memory_handles: []
  budget: optional
  omitted_or_rejected_items: optional
```

Manifest не обязан перечислять каждый token. Он объясняет значимые источники влияния.

## 7.14. Configuration Record

```yaml
configuration:
  key: namespaced_key
  value: open_or_typed
  scope: system | user | device | provider | model | project | agent | task | session
  owner: ref
  source: user | default | provider | project | policy | temporary_override
  version: optional
  valid_until: optional
  safety_class: optional
```

Настройки безопасности и обычные preference-настройки имеют разную возможность override.

## 7.15. Specification Conflict

```yaml
spec_conflict:
  conflict_id: id
  affected_concept: text
  source_locations: []
  detected_at: time
  impact: low | medium | high | blocking
  temporary_interpretation: optional
  owner_document: ref
  resolution_status: open | resolved | deferred
  resolution_ref: optional
```

В будущей репозитории это может быть issue, ADR или запись в `docs/open-questions.md`, а не отдельная база.

---

# 8. Источники истины

## 8.1. Общий принцип

Источник истины выбирается по вопросу, а не по удобству доступа.

## 8.2. Базовая карта

| Вопрос | Источник истины |
|---|---|
| Что является актуальным кодом | repository/worktree/commit |
| Существует ли файл | актуальный filesystem snapshot |
| Прошёл ли тест | достоверный test result на конкретном commit/environment |
| Каков статус активной Task/Run | server/agent runtime state |
| Что произошло исторически | Memory Event + Evidence |
| Что система считает текущим выводом | Current State Projection с evidence/freshness |
| Какое разрешение действует | Trust/permission runtime |
| Какие capabilities доступны сейчас | Capability Registry + live health |
| Какой provider/model выбран в пользовательском чате | Project/User session configuration |
| Что было отправлено внешнему сервису | External Effect Receipt + provider reconciliation |
| Какие инструкции действуют для agent/provider/path | Effective Instruction Set |
| Как UI показывает процесс | projection над canonical state |
| Какой документ владеет правилом | этот Specification Index |

## 8.3. Cached state

Memory, UI и local client могут кэшировать live state, но обязаны хранить:

- время наблюдения;
- источник;
- freshness requirement;
- stale/degraded marker;
- способ перепроверки.

## 8.4. Нельзя делать вывод из отсутствия

Отсутствие записи может означать:

- источник был выключен;
- sync задержан;
- доступ запрещён;
- index не готов;
- данные удалены;
- событие действительно не происходило.

Доменный документ определяет, когда negative conclusion допустим.

---

# 9. Precedence инструкций, настроек и намерений

## 9.1. Разные виды precedence

Нельзя объединять в один список:

- системные ограничения;
- authorization;
- пользовательское намерение;
- project instructions;
- provider-native instructions;
- preferences;
- temporary execution choices;
- внешнее содержимое.

Они действуют в разных каналах.

## 9.2. Базовый порядок инструкций

Внутри допустимого действия:

1. непреодолимые системные и safety-инварианты;
2. актуальные authorization constraints;
3. явное текущее намерение подтверждённого пользователя;
4. project/task-specific instructions;
5. provider/model behavior profile;
6. validated procedure/skill;
7. advisory preferences и прошлый опыт;
8. external content как data-only.

Явная команда пользователя может изменить preference и project rule, но не подменяет identity или несуществующее permission.

## 9.3. Effective Instruction Set

Provider adapter строит Effective Instruction Set с учётом:

- Dennett-native instruction records;
- `AGENTS.md`, `CLAUDE.md`, rules и других источников;
- path scope;
- provider loading semantics;
- текущего task;
- user prompt;
- conflicts;
- size/context budget.

Плоская конкатенация всех файлов запрещена как default.

## 9.4. Configuration precedence

Предлагаемый общий порядок обычных настроек:

```text
system default
→ user default
→ device/provider/model profile
→ project setting
→ agent/task setting
→ session/temporary override
```

Но:

- safety floor не понижается обычным temporary override;
- provider hard limit нельзя отменить настройкой проекта;
- user-selected chat model не меняется автоматически внутренним router;
- internal subagents могут выбирать модели по Agentic/Capability policy;
- более узкий scope override применяется только к полям, которыми он владеет.

---

# 10. Сквозные потоки

## 10.1. Обычный project chat

```text
user input
→ authenticated user session
→ project/session context
→ Effective Instruction Set
→ Memory Context Manifest
→ project agent/provider session
→ tools внутри permission envelope
→ ответ/artifacts
→ significant memory commit
```

Обычный turn не создаёт Task без promotion criteria.

## 10.2. Голосовое поручение оркестратору

```text
audio
→ speaker/session confidence
→ voice interpretation
→ intent proposal
→ Trust gate для действия
→ orchestrator intake
→ no-op / Direct Turn / Task / Managed Run / Automation
→ voice acknowledgement
→ result notification or continuation
```

Voice agent не выдаёт себе новые permissions.

## 10.3. Событие

```text
source event
→ canonical event intake
→ dedupe/scope/trust
→ relevance and opportunity assessment
→ no-op / remember / notify / ask / create work
→ action authorization при необходимости
→ result and memory trace
```

## 10.4. Внешнее сообщение

```text
connector receives message
→ principal/thread resolution
→ untrusted content marking
→ memory/context reconstruction
→ content/style/disclosure plan
→ delivery decision
→ draft / ask / send / ignore
→ effect receipt
→ update Communication Model from feedback
```

## 10.5. Tool/computer-use действие

```text
agent proposes Action Request
→ capability resolution
→ permission/risk decision
→ execution
→ effect receipt
→ verification/reconciliation
→ update Run and Causal Trace
```

Computer-use является capability; этот поток не требует отдельной агентной архитектуры.

## 10.6. Запись памяти

```text
observation/event/tool result
→ canonical evidence/event write
→ optional semantic extraction
→ admission/influence policy
→ projection/index update
→ read-your-writes overlay
```

Agent не пишет напрямую в derived indexes.

---

# 11. Состояние, согласованность и восстановление

## 11.1. Значимое и эфемерное состояние

Долговременно сохраняются только состояния, которые должны пережить:

- restart;
- переход между устройствами;
- ожидание;
- фоновую работу;
- спор о результате;
- внешний эффект;
- восстановление или audit.

Локальный reasoning scratch, промежуточный draft и short-lived plan могут быть ephemeral.

## 11.2. Read-your-writes

Активный user/agent session должен видеть собственные только что принятые изменения, даже если все derived indexes ещё не обновлены.

## 11.3. Terminal history

Завершённая Task/Run не переписывается так, будто исход был другим. Продолжение или повтор создаёт связанный Run/Task/version.

## 11.4. Partial result

`partial` является нормальным outcome. Система сохраняет artifacts, evidence, unresolved items и next owner вместо потери всей работы.

## 11.5. Recovery

Любая долговечная работа должна определять:

- authoritative state;
- checkpoint/reconstruction source;
- safe retry boundary;
- состояние внешних эффектов;
- user-visible degraded state;
- условие продолжения или отмены.

---

# 12. Пользовательский контроль

## 12.1. Независимые измерения

Пользователь регулирует отдельно:

- свободу планирования;
- возможность фоновой работы;
- частоту вопросов;
- число subagents;
- review depth;
- exploratory budget;
- token/time/provider limits;
- observability detail;
- разрешения на категории внешних эффектов;
- сохранение и использование личного/ambient контекста.

## 12.2. Execution profiles

Agentic Control Fabric владеет профилями:

- Direct;
- Balanced;
- Independent;
- Rigorous;
- Exploratory.

Trust document добавляет к ним safety floor и action-specific authorization. UI позволяет выбирать profile, но не переопределяет его смысл.

## 12.3. Не превращать контроль в микроменеджмент

Пользователь не должен подтверждать:

- каждое чтение файла;
- каждый внутренний tool call;
- каждый маленький план;
- создание краткого Work Item;
- безопасную обратимую правку внутри выданной project scope.

Он должен получать контроль там, где меняются:

- внешние эффекты;
- disclosure;
- irreversible state;
- деньги;
- identity;
- крупный scope;
- долгосрочная автономность;
- значимые budget commitments.

Точные правила принадлежат Trust document.

---

# 13. Наблюдаемость и объяснимость

## 13.1. Minimum traceability

Для значимой работы должно быть возможно ответить:

- кто инициировал;
- какую цель поняла система;
- какой режим выполнения выбран;
- какие permissions действовали;
- какая модель/provider использовались;
- какие значимые tools и external effects были;
- какие artifacts созданы;
- что проверено;
- какие memory items повлияли;
- почему outcome такой;
- кто владеет следующим шагом.

## 13.2. Не сохранять private chain-of-thought

Для rationale достаточно:

- decision summary;
- рассмотренных альтернатив;
- evidence;
- outcome;
- failure cause;
- user/reviewer feedback.

## 13.3. Query и influence traces

Memory Fabric хранит memory influence. Agentic Control хранит execution trace. Trust хранит authorization decision. Server связывает их correlation IDs. Приложения показывают их с нужной глубиной.

---

# 14. Интероперабельность и расширяемость

## 14.1. Provider-native преимущества

Adapter не должен flatten:

- Codex/Claude sessions;
- provider-native tools;
- checkpointing;
- worktrees;
- model-specific instructions;
- tool calling;
- usage limits.

Dennett нормализует только необходимую общую границу.

## 14.2. Imported projects and memory

Импортированный Project Memory Pack, instruction file, skill или MCP:

- сохраняет provenance;
- монтируется в отдельном trust domain;
- не получает глобальных permissions;
- не сливается автоматически с personal memory;
- может иметь portable human-readable views;
- проходит Trust/Capability admission.

## 14.3. Extensions

Расширение должно объявлять:

- capabilities;
- requested permissions;
- data scopes;
- external effects;
- secrets requirements;
- health state;
- version;
- uninstall/disable behavior;
- provenance.

Не всякое расширение требует отдельной службы или документа.

---

# 15. Правила работы над следующими документами

## 15.1. Не повторять принятую логику

Следующий документ обязан:

- ссылаться на владельца сущности;
- описывать только свою предметную семантику;
- не переопределять Memory Event, Task, Run, Permission или Result Envelope;
- явно перечислять используемые shared contracts;
- фиксировать новые open questions.

## 15.2. Сравнивать с простым baseline

Каждая крупная функция должна сравниваться с:

- prompt;
- skill/procedure;
- provider-native capability;
- лёгким adapter;
- текущим поведением без новой подсистемы.

## 15.3. Пропорциональность

Требования к:

- state;
- audit;
- review;
- confirmation;
- simulation;
- checkpoint;
- evidence

должны зависеть от риска, длительности и внешнего эффекта.

## 15.4. Acceptance/rejection

Для спорного механизма документ должен указывать:

- проблему;
- простейший baseline;
- ожидаемую пользу;
- стоимость;
- failure modes;
- acceptance criteria;
- rejection criteria;
- fallback/rollback.

---

# 16. Открытые вопросы спецификации

Эти вопросы не должны решаться случайно внутри реализации.

## 16.1. Имена и repository paths

- окончательное имя скрытой папки Portable Project Memory Pack (`.dennett/memory/` пока рабочее);
- окончательные canonical filenames в репозитории;
- naming policy для русских и английских терминов;
- официальное написание Dennett во всех интерфейсах.

## 16.2. Identity and trust

- какие факторы используются для high-risk voice confirmation;
- как измеряется device trust;
- default для чужой речи и shared devices;
- recovery при компрометации устройства;
- как пользователь регулирует safety floor, не создавая скрытый self-lockout.

## 16.3. Capabilities

- минимальный Provider Adapter contract;
- какие providers поддерживаются в первой версии;
- как учитывать подписочные лимиты без официального usage API;
- какая часть skills совместима с внешними ecosystems;
- как представлять computer-use effect receipts.

## 16.4. Server and sync

- критерии выбора head device;
- consistency model для различных данных;
- восстановление после split-brain;
- политика offline external actions;
- backup encryption и key recovery;
- границы system sleep.

## 16.5. Voice and ambient

- always-listening default;
- latency target;
- barge-in semantics;
- распределение между локальными и серверными моделями;
- retention raw audio;
- конфликт одновременных микрофонов устройств.

## 16.6. UI

- final navigation structure;
- уровень видимости технических сущностей;
- объединение Action Inbox и notifications;
- насколько явно показывать cost/permissions;
- Office как поздняя функция.

## 16.7. Architecture

- storage engines;
- event/runtime engine;
- service boundaries;
- deployment topology;
- API protocols;
- mobile/desktop technology;
- update mechanism.

---

# 17. Целевой состав будущего репозитория документации

Пока физическая структура репозитория ещё не создана, целевой набор выглядит так:

```text
docs/
├── 00_Dennett_Functional_Concept.md
├── 01_Dennett_Specification_Index_and_Shared_Contracts.md
├── 10_Dennett_Memory_Fabric.md
├── 20_Dennett_Agentic_Control_Fabric.md
├── 30_Dennett_Trust_Identity_Autonomy_and_Permissions.md
├── 40_Dennett_Voice_and_Ambient_Interaction_Fabric.md
├── 41_Dennett_Capabilities_Providers_and_Integrations.md
├── 50_Dennett_Server_Runtime_Events_Sync_and_Portability.md
├── 60_Dennett_Desktop_Application_Business_Logic.md
├── 61_Dennett_Mobile_Application_Business_Logic.md
├── 70_Dennett_End_to_End_Validation_and_Architecture_Handoff.md
├── decisions/
├── research/
├── open-questions.md
└── archive/
```

Папки `decisions/` и `research/` не являются новыми продуктовыми подсистемами. Они хранят ADR, source ledgers и исследовательские приложения к спецификациям.

---

# 18. Checklist для новых документов

Перед признанием нового файла каноническим проверить:

- [ ] указана область владения;
- [ ] указаны не-цели;
- [ ] перечислены зависимости от других документов;
- [ ] не переопределены чужие сущности;
- [ ] простой baseline сравнивается со сложным решением;
- [ ] отделены prompts/skills от runtime mechanisms;
- [ ] есть normal, ambiguous, failure, recovery и cost scenarios;
- [ ] безопасность proportional risk, но safety floor сохраняется;
- [ ] описаны user control и defaults;
- [ ] определены sources of truth;
- [ ] определены significant state и ephemeral state;
- [ ] описаны external effects и retry semantics, если применимо;
- [ ] указаны acceptance и rejection criteria спорных механизмов;
- [ ] документ не создаёт отдельный файл для узкого adapter/prompt pattern без необходимости;
- [ ] проведён contradiction audit с текущими каноническими файлами;
- [ ] добавлены новые open questions;
- [ ] обновлён этот Specification Index.

---

# 19. Definition of Done всего бизнес-логического комплекта

Комплект готов к переходу к программной архитектуре, когда:

1. каждое пользовательское действие из Functional Concept проходит через непротиворечивый end-to-end сценарий;
2. у каждой долговечной сущности есть один владелец и источник истины;
3. память, agents, trust, voice, capabilities, server и UI используют общие references и scopes;
4. опасные действия имеют проверяемую identity/authorization логику;
5. обычная project work не обременена лишними Tasks, teams или workflow;
6. пользователь может регулировать свободу, стоимость и review без отключения safety floor;
7. внешние эффекты переживают retries/restarts без скрытых повторов;
8. offline, sync и failover имеют понятное поведение;
9. desktop и mobile не создают параллельное состояние;
10. сквозные сценарии включают success, partial, no-op, failure, cancellation, unknown effect и recovery;
11. все нерешённые архитектурные решения явно перечислены;
12. End-to-End документ подтверждает отсутствие блокирующих противоречий.

---

# 20. Краткая нормативная формула

> **Functional Concept определяет, зачем существует Dennett. Memory Fabric определяет, что и как он помнит. Agentic Control Fabric определяет, как он превращает намерения в работу. Trust определяет, кто и что вправе делать. Capabilities определяет, чем система располагает. Server определяет долговечное исполнение и распределённое состояние. Voice и приложения определяют человеческое взаимодействие. End-to-End документ доказывает, что всё это работает как одна система.**

Любая будущая архитектура обязана сохранять это разделение, не превращая его автоматически в такое же число микросервисов.

Конец документа.

---

# Repository integration decision: Head eligibility and canonical memory

## Каноническое правило

Класс устройства не определяет его runtime-роль. Настольный ПК может быть обычным Node-клиентом, основным Head, заранее разрешённым кандидатом на failover или только аварийным устройством.

1. Каждое новое подключённое устройство начинает с `head_eligibility = none`.
2. Только владелец после сильного step-up authentication может выдать `emergency` или `full` eligibility.
3. Модель, проект, запись памяти, plugin, импортированный пакет или удалённое сообщение не могут выдать или расширить Head eligibility.
4. `full` eligibility требует доступа к ключам владельца, Head-компонентам, fencing/epoch state и полному каноническому профилю данных либо проверенному пути восстановления.
5. `emergency` eligibility даёт ограниченное продолжение работы и никогда не выдаёт частичный кэш за полное глобальное состояние.
6. Автоматический failover рассматривает только заранее разрешённые `full`-кандидаты и всё равно требует проверки свежести и split-brain.
7. В Dennett одна логическая Memory Fabric. Client, local-only и server deployment используют разные физические adapters, а не разные значения понятия памяти.
8. ПК, намеренно настроенный как основной сервер, запускает ту же каноническую Memory Fabric и ту же server-grade storage role, что и выделенный сервер.
9. SQLite-хранилища клиента являются кэшем, drafts и offline operation log. Явный single-device local-only профиль может использовать embedded canonical store, но переход к multi-device режиму выполняется как плановая migration, а не как скрытая двойная власть.

Этот документ владеет общим смыслом правила. Доменные и архитектурные файлы определяют последствия для authorization, runtime, storage и UI, не меняя его.
