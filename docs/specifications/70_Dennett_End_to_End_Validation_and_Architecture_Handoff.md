# Dennett End-to-End Validation and Architecture Handoff


## Repository edition closure status

The former G-01–G-15 gaps are now **closed at the business-contract level** by the files under [`contracts/`](contracts/README.md), especially [`90_Integrated_End_to_End_Scenarios.md`](contracts/90_Integrated_End_to_End_Scenarios.md). The original gap-audit language below is preserved as history; it no longer means that architecture is blocked.

Technology choices and risk spikes may remain open, but product semantics are no longer delegated to implementation code.



> **Repository edition · 2026-07-13 · `70`**  
> Это самостоятельный канонический документ репозитория Dennett. Начните с [карты документации](../README.md).  
> Related: [60_Dennett_Desktop_Application_Business_Logic.md](./60_Dennett_Desktop_Application_Business_Logic.md) · [61_Dennett_Mobile_Application_Business_Logic.md](./61_Dennett_Mobile_Application_Business_Logic.md).

## Интегрированные contract supplements

Следующие небольшие нормативные документы выделены из предархитектурного gap-аудита. Они являются частью текущего набора и обязательны для изменений, пересекающих указанные границы:

- [`90_Integrated_End_to_End_Scenarios.md`](contracts/90_Integrated_End_to_End_Scenarios.md)

Эти supplements не дают одному lifecycle разойтись по нескольким большим файлам; canonical owner указан в заголовке каждого документа.


## Сквозная проверка полноты бизнес-логики, каталог обязательных сценариев, реестр пробелов и требования к следующему этапу архитектуры

**Версия:** 1.0  
**Дата:** 12 июля 2026 года  
**Статус:** канонический завершающий документ бизнес-логического этапа.  
**Каноническое имя:** `70_Dennett_End_to_End_Validation_and_Architecture_Handoff.md`.

Этот документ является самостоятельным. Для понимания его основной логики не требуется знать историю обсуждения проекта.

**Dennett** — персональная агентная операционная система. Пользователь напрямую работает с проектными агентами в папках и репозиториях, общается с постоянным главным оркестратором, использует голосовой и ambient-режимы, подключает облачные и локальные модели, skills, MCP и другие инструменты, ведёт долговременную доказательную память, получает проактивную помощь и управляет системой с нескольких устройств.

К моменту написания этого файла отдельно определены:

- функциональная концепция продукта;
- Memory Fabric;
- Agentic Control Fabric;
- Trust, Identity, Autonomy and Permissions;
- Voice and Ambient Interaction Fabric;
- Capabilities, Providers and Integrations;
- Server Runtime, Events, Sync and Portability;
- Desktop Application Business Logic;
- Mobile Application Business Logic.

Этот файл не создаёт ещё одну конкурирующую подсистему. Он отвечает на четыре завершающих вопроса:

1. **Описана ли вся обещанная пользователю функциональность?**
2. **Стыкуются ли отдельные документы в одну непротиворечивую систему?**
3. **Можно ли проверить каждый значимый сценарий от намерения до результата, отказа и восстановления?**
4. **Достаточно ли определены требования, чтобы перейти от бизнес-логики к программной архитектуре, не додумывая продукт случайно во время кодирования?**

---

# Часть I. Итоговый вердикт и метод проверки

## 0. Итоговый вердикт

### 0.1. Комплект бизнес-логики в основном готов к архитектурному этапу

Принятые документы уже определяют ядро Dennett достаточно глубоко:

- один главный логический оркестратор;
- прямые project chats по принципу Codex/Claude Code;
- single-agent-first выполнение и пропорциональная оркестрация;
- долговременную, мультимодальную и переносимую память;
- task-scoped permissions и регулируемую автономность;
- provider-neutral Capability Fabric;
- живой голосовой режим с локальным ambient edge;
- сервер как personal control plane с local-capable nodes;
- полноценный desktop workbench;
- мобильный trusted remote, оптимизированный для быстрых действий и прерываний.

На момент исходного аудита комплект ещё нельзя было считать абсолютно закрытым. Аудит обнаружил три типа пробелов; в repository edition они закрыты contract supplements, перечисленными в начале файла:

1. **Функция существует, но слишком разнесена по документам.** Её легко не заметить или реализовать неполно. Главные примеры — постоянное локальное прослушивание микрофона и событийный захват экрана.
2. **Составляющие функции описаны, но не собраны в единый предметный lifecycle.** Главные примеры — внешние переписки, полный жизненный цикл проекта и публикация артефактов.
3. **Функция заявлена в концепции как полезный пользовательский режим, но должна быть реализована через prompt/skill/automation, а не отдельную подсистему.** Примеры — Idea Incubator, Concept Distiller, редактор мышления, ежедневные обзоры и мониторинг AI-новостей.

Этот документ:

- закрепляет обязательные end-to-end сценарии;
- показывает, какие пробелы блокируют архитектуру, а какие можно не превращать в отдельные системы;
- формирует quality-attribute scenarios;
- задаёт критерии перехода к архитектуре;
- определяет план архитектурной документации из четырёх крупных томов.

### 0.2. Что считается «описано полностью»

Функция считается полностью описанной только тогда, когда можно определить:

- кто или что её инициирует;
- какая сущность владеет её текущим состоянием;
- какие данные и контекст используются;
- какие permissions необходимы;
- какие capabilities выбираются;
- где выполняется работа;
- что видит пользователь;
- что считается успехом;
- что считается частичным результатом;
- как действует Stop/Cancel;
- что происходит при offline, timeout, restart и provider failure;
- что записывается в память;
- как избежать повторного внешнего эффекта;
- как объяснить результат;
- как функция тестируется.

Одного упоминания кнопки, модели или события недостаточно.

### 0.3. Статусы полноты

В этом документе используются статусы:

- **COVERED** — предметная логика и сквозной путь определены достаточно;
- **COVERED-BUT-FRAGMENTED** — логика распределена корректно, но её нужно связать явным сценарием;
- **PARTIAL** — существенная часть есть, но implementation team пришлось бы додумывать поведение;
- **PROMPT/SKILL FEATURE** — функция нужна, но отдельная подсистема не оправдана;
- **DEFERRED** — функция сознательно отложена и не блокирует первую архитектуру;
- **OUT OF SCOPE** — функция не входит в текущую личную установку Dennett;
- **BLOCKER** — без уточнения архитектура может закрепить неправильную бизнес-логику.

### 0.4. Главный критерий архитектурной готовности

Переход к архитектуре разрешён, когда:

> Для каждого обязательного пользовательского сценария существует один понятный маршрут через канонические бизнес-сущности, известны источники истины, границы безопасности, классы согласованности, условия завершения и восстановление, а оставшиеся неопределённости явно вынесены в ADR/architecture spikes, а не скрыты в формулировке «реализация разберётся».

---

## 1. Область ответственности

### 1.1. Этот документ определяет

- карту полноты продукта;
- связь обещаний функциональной концепции с каноническими документами;
- end-to-end сценарии;
- scenario template;
- cross-domain contracts;
- quality tree;
- предварительные измеримые quality targets;
- failure, recovery и degraded-mode проверки;
- обязательные исправления бизнес-логики;
- non-goals;
- архитектурно значимые требования;
- план крупных архитектурных документов;
- необходимый уровень архитектурной детализации;
- risk spikes и ADR backlog;
- Definition of Done до начала реализации.

### 1.2. Этот документ не выбирает

- язык программирования;
- конкретную базу данных;
- конкретный workflow engine;
- конкретную очередь;
- desktop/mobile UI-фреймворк;
- окончательную deployment topology;
- конкретный vector database;
- единственный agent SDK;
- конкретный формат RPC;
- окончательный набор providers первой версии;
- окончательный художественный стиль интерфейса.

Такие решения относятся к архитектурному этапу, но этот файл задаёт, **каким требованиям они обязаны соответствовать**.

### 1.3. Этот документ не создаёт параллельные источники истины

- Memory Fabric остаётся владельцем памяти.
- Agentic Control Fabric остаётся владельцем Project Session, Task, Run и Agent.
- Trust Fabric остаётся владельцем permissions и identity.
- Capability Fabric остаётся владельцем providers, models, skills, MCP, plugins и connectors.
- Server Runtime остаётся владельцем долговечного operational state, events, sync, backup и failover.
- Voice Fabric остаётся владельцем Voice Session, turn-taking и ambient interaction.
- Desktop и Mobile остаются владельцами пользовательских экранов и действий.

Сценарии этого файла связывают эти области, но не переопределяют их внутреннюю логику.

---

## 2. Исследовательский и валидационный протокол

### 2.1. План для построения плана

Перед созданием документа были зафиксированы пять результатов, которые он обязан дать:

1. доказать или опровергнуть полноту бизнес-логики;
2. найти функции, которые были потеряны, спрятаны или размазаны;
3. превратить абстрактные обещания в проверяемые сценарии;
4. определить уровень детализации будущей архитектуры;
5. не создать новую бюрократию поверх уже написанных документов.

Для этого аудит проводился по четырём представлениям:

- **feature view** — что обещано пользователю;
- **lifecycle view** — какие сущности живут и изменяются;
- **runtime view** — как компоненты взаимодействуют во времени;
- **failure view** — что происходит при ошибке, offline и неопределённом эффекте.

### 2.2. Использованный метод

Процесс:

1. Восстановить полный набор функций из функциональной концепции.
2. Определить канонический документ-владелец каждой области.
3. Проверить, существует ли не только упоминание, но и lifecycle.
4. Мысленно выполнить нормальный сценарий.
5. Выполнить тот же сценарий при offline, restart, stale data и cancellation.
6. Проверить identity, permission, memory influence и external effects.
7. Проверить desktop/mobile/voice пути.
8. Проверить cost-of-success и отсутствие лишних LLM-вызовов.
9. Отделить архитектурный пробел от функции, которую достаточно сделать skill/prompt.
10. Зафиксировать acceptance, rejection и recovery.

### 2.3. Почему сценарная проверка важнее списка функций

Список функций может выглядеть полным и всё равно не отвечать на вопросы:

- кто хранит состояние после перезапуска;
- что произойдёт при одновременном действии с двух устройств;
- какой transcript считается окончательным;
- что делать после timeout отправки сообщения;
- как удалить screenshot из индексов и backup;
- может ли найденная web-инструкция повлиять на recipient;
- как продолжить проект после смены provider.

arc42 рекомендует описывать runtime view через важные сценарии, включая критические интерфейсы, operation, administration и error behavior; quality requirements также формулируются как сценарии. [[S10]]

### 2.4. Архитектурная оценка через trade-offs

Будущая архитектура не будет оцениваться по количеству компонентов или модности технологий. Для каждого крупного решения требуется:

- business driver;
- quality attribute;
- сценарий;
- sensitivity point;
- trade-off;
- risk;
- simpler baseline;
- proof-of-concept или измерение;
- ADR.

Это соответствует сценарио-ориентированной логике ATAM: архитектура проверяется через приоритетные quality attributes, risks, sensitivity points и trade-offs, а не только через статическую диаграмму. [[S11]]

### 2.5. Уровни архитектурного представления

Для будущих документов используется иерархия C4:

- system context;
- containers/runtime applications;
- components;
- code-level view только для критических частей.

C4 специально разделяет software system, container, component и code и дополняет их dynamic и deployment diagrams. [[S09]]

### 2.6. Критерии отказа от механизма

Механизм должен быть отклонён или упрощён, если он:

- существует только ради красивой схемы;
- требует LLM call на обычном fast path;
- создаёт новый источник истины;
- заставляет микрошаг становиться Task;
- повторяет логику provider runtime без выгоды;
- ухудшает offline utility;
- скрывает failure или uncertainty;
- требует сложной согласованности для данных, которым достаточно eventual consistency;
- добавляет несколько процентов качества ценой кратного роста стоимости;
- не имеет способа удаления или rollback;
- не может быть протестирован end-to-end.

---

## 3. Неподвижные сквозные инварианты

1. Пользовательский project chat остаётся прямым взаимодействием с project agent.
2. Один сильный агент является default.
3. Микрошаг не обязан становиться Task.
4. Модель не является источником permission.
5. Память не является permission token.
6. Capability registration не означает trust или authorization.
7. UI не является источником operational truth.
8. Provider Session не является самой Task.
9. External effect не повторяется вслепую после timeout.
10. `UNKNOWN` является отдельным состоянием.
11. Stop/Cancel имеет приоритет над обычной фоновой работой.
12. Offline data всегда показывает freshness.
13. Устройство может выполнять локальную работу без Head, но не притворяется свежим глобальным authority.
14. Один активный Head является default.
15. Raw sensory stream не становится долговременной памятью автоматически.
16. Экран, речь и действия связываются по времени и контексту.
17. Внешнее содержимое является data, а не instruction authority.
18. User-selected capability не проходит обязательный utility review, но executable effects остаются под Trust.
19. Автоматически найденная capability не устанавливается молча.
20. Direct-chat model не меняется скрытно.
21. Healthy infrastructure не требует внимания пользователя.
22. Notification не является решением Action Inbox.
23. Голосовое сходство не достаточно для опасного действия.
24. Partial ASR transcript не является committed command.
25. Непроигранный TTS-текст не считается сказанным пользователю.
26. Удаление должно распространяться на derived indexes, replicas и backups в пределах policy.
27. Portable project memory не содержит скрытую личную память по умолчанию.
28. Сложная архитектурная деталь должна закрывать реальный failure mode.
29. Сквозная наблюдаемость не означает сохранение private chain-of-thought.
30. Правильным результатом может быть `do nothing`.

---

# Часть II. Карта полноты продукта

## 4. Feature Promise Ledger

### 4.1. Главный оркестратор и намерения — COVERED

Обязательства:

- постоянный канал общения;
- краткая цель без детального плана;
- выбор Direct Turn, Agent Session, Managed Run или Automation;
- глобальные и межпроектные решения;
- действия по событиям;
- no-op;
- передача работы проектным агентам;
- Action Inbox при необходимости пользователя.

Владельцы:

- Agentic Control Fabric;
- Server Runtime;
- Memory Fabric;
- Trust Fabric;
- Desktop/Mobile.

Оставшийся архитектурный вопрос: как физически разделить persistent orchestrator identity, runtime process и provider sessions без одного бесконечного prompt.

### 4.2. Проекты и прямые project chats — COVERED

Обязательства:

- папка/репозиторий как working directory;
- один или несколько project sessions;
- работа как в Codex/Claude Code;
- files, diff, tests, worktrees;
- project memory;
- capabilities;
- background runs;
- handoff между устройствами.

Пробел: lifecycle самого Project как объекта описан в UI лучше, чем в доменной бизнес-логике. Требуется компактный канонический контракт rename/move/archive/remove/delete/clone/export/import/missing-path/rebind. Статус: **PARTIAL**.

### 4.3. Агенты, задачи и workflow — COVERED

Обязательства:

- single-agent-first;
- bounded subagents;
- сохранение целостной цели;
- свободная коммуникация с минимальным envelope;
- Task только при lifecycle value;
- Run, checkpoint, cancel, retry;
- optional structured automation;
- user-controlled execution profiles.

Архитектурный риск: provider-native sessions и Dennett canonical state не должны расходиться.

### 4.4. Memory Fabric — COVERED

Обязательства:

- evidence-first event ledger;
- open semantic memory;
- current state и history;
- temporal correctness;
- hybrid retrieval;
- project spaces;
- portable memory packs;
- multimodal evidence;
- correction, deletion, replay;
- offline merge.

Архитектурные spikes:

- storage/index split;
- scale sensory memory;
- deletion graph;
- retrieval quality;
- background consolidation cost.

### 4.5. Голосовой режим — COVERED

Обязательства:

- push-to-talk и live voice;
- realtime/cascade/hybrid path;
- interruption и barge-in;
- spoken-output commitment;
- optional deliberative sidecar;
- project/orchestrator routing;
- meeting profile;
- multi-device handoff;
- local/offline fallback.

### 4.6. Постоянный микрофон телефона и ПК — COVERED-BUT-FRAGMENTED

Функция присутствует, но должна быть сделана явно архитектурно обязательной.

Требуемый контракт:

```text
physical/OS microphone permission
→ local VAD/wake/speaker activity
→ short local overwrite ring buffer
→ cheap dedupe/relevance/privacy filtering
→ optional local ASR or semantic candidate
→ discard | wake Voice Session | commit evidence | create event
→ selective sync and retention
```

Инварианты:

- микрофон может быть активен часами без постоянной тяжёлой модели;
- одинаковый шум или повторный фрагмент не отправляется многократно;
- raw buffer локален и перезаписывается;
- пользователь видит активное состояние;
- физический mute и OS revoke имеют приоритет;
- чужая речь не является командой;
- committed capture отделён от ambient observation;
- несколько устройств не должны создавать дубликаты без source identity;
- privacy exclusions применяются до cloud upload;
- battery/network budget измеряются.

Этот контракт относится совместно к Voice, Memory, Server, Trust, Desktop и Mobile. В архитектуре он должен получить отдельный runtime sequence, а не быть спрятан внутри общей voice pipeline.

### 4.7. Событийный захват экрана ПК — COVERED-BUT-FRAGMENTED

Функция тоже существует, но разнесена между Memory Fabric, device nodes и Quick Capture UI.

Требуемый контракт:

```text
OS/app events
→ active app/window/display context
→ structured source first: accessibility tree / DOM / selected text
→ change detection
→ screenshot/keyframe only when useful
→ OCR fallback
→ secret/redaction/privacy filter
→ content and temporal dedupe
→ project/session association
→ memory candidate or committed evidence
→ hot/warm/cold retention
```

Сигналы захвата:

- смена приложения или окна;
- значимое изменение экрана;
- navigation;
- click/scroll/typing pause;
- открытие документа;
- error dialog;
- user selection;
- explicit hotkey;
- начало computer-use;
- связанная голосовая фраза;
- активный meeting/project context.

Инварианты:

- нельзя бездумно снимать одинаковый экран каждые N секунд;
- структурные данные предпочтительнее OCR;
- парольные менеджеры, banking и исключённые приложения блокируются до записи;
- screenshot не является инструкцией;
- user can pause by app, display, time or mode;
- capture overhead не должен заметно ухудшать foreground work;
- screenshot и nearby speech должны иметь общий temporal anchor;
- raw bytes, OCR, visual embedding и summary удаляются согласованно.

До архитектуры это считается **обязательным E2E сценарием**, а не поздней необязательной функцией.

### 4.8. Другие сенсорные источники — PARTIAL/EXTENSIBLE

Уже есть generic capability/device model для:

- camera;
- clipboard;
- selected text;
- active application;
- location;
- wearables;
- headset;
- notifications;
- browser/DOM;
- OS activity.

Не требуется отдельная подсистема для каждого сенсора. Архитектура должна предоставить единый Sensor Source Adapter и per-source privacy/retention policy.

Location, wearable и VR не являются обязательным first-release scope. Статус: **DEFERRED**, но расширяемость обязательна.

### 4.9. События, расписания и проактивность — COVERED

Обязательства:

- deterministic и semantic events;
- trigger lifecycle;
- cooldown;
- dedupe;
- prospective intent;
- attention budget;
- no-op;
- Action Inbox;
- notification routing;
- offline late events;
- event time и processing time.

### 4.10. Action Inbox и Agent Radar — COVERED

Обязательства:

- authoritative card state;
- revision и multi-device answer;
- snooze/expiry/supersede;
- permission/choice/conflict/failure cards;
- Radar как materialized projection;
- freshness;
- stop/pause/steer;
- desktop/mobile/voice paths.

### 4.11. Capabilities, providers, skills, MCP и plugins — COVERED

Обязательства:

- dynamic market snapshot;
- native adapters;
- local models;
- manual vs automatic acquisition;
- candidate comparison;
- project-local capabilities;
- ownership/fork/update;
- health/quota/fallback;
- lazy activation;
- skill delta extraction;
- native connector before weaker alternatives when justified.

### 4.12. External communication — PARTIAL, архитектурно значимый пробел

Составляющие уже есть:

- connector lifecycle;
- Communication Model;
- content/style/disclosure/delivery separation;
- recipient provenance;
- draft/send/ignore;
- permissions;
- idempotent effect receipts;
- thread context;
- mobile quick reply.

Но нет одного компактного канонического lifecycle для Telegram/email/other messenger operation.

До реализации требуется закрепить:

```text
incoming channel event
→ account/thread/participant resolution
→ sync/freshness
→ message and attachment safety classification
→ social/project/memory context reconstruction
→ content plan
→ style realization
→ disclosure check
→ choose ignore | draft | ask | send
→ exact recipient/account/thread permission
→ dispatch with idempotency
→ delivery reconciliation
→ user correction and memory feedback
```

В архитектуре connector, message model, thread cache, outbound queue и effect reconciliation нельзя проектировать независимо.

### 4.13. Computer-use — COVERED

Обязательства:

- prefer API/DOM/accessibility before pixels;
- multiple backends;
- device and window scope;
- takeover;
- screenshots/receipts;
- Trust gate before consequential click;
- unknown-effect handling;
- project/voice invocation.

### 4.14. Server, devices, sync, backup and failover — COVERED

Обязательства:

- one active Head;
- local-capable nodes;
- control/data separation;
- offline operation logs;
- data-specific consistency;
- effect claims;
- backup and restore drills;
- planned handoff;
- emergency head;
- portable client.

### 4.15. Desktop and mobile applications — COVERED

Desktop содержит полный workbench, а mobile — glance/capture/decide/continue UX.

Архитектурный вопрос: как shared command IDs, canonical state projections, local drafts и optimistic UI будут реализованы без дублирования доменной логики.

### 4.16. Artifacts — PARTIAL

Artifacts представлены в agents, server и UI, но их полный lifecycle не имеет одного канонического предметного владельца.

Нужно закрепить:

- immutable content/version identity;
- draft/final/published/archived status;
- relation to Task, Run, Session and Project;
- provenance/evidence;
- local and remote storage;
- preview generation;
- share/export/publish;
- access scope;
- deletion/retention;
- create-new-project/task;
- conflict between provider artifact and local edited version.

Архитектура должна включить это как отдельный cross-cutting data contract, но новый бизнес-документ необязателен: достаточно normative amendment в Agentic Control или Shared Contracts.

### 4.17. Project lifecycle — PARTIAL

До реализации требуется одна предметная таблица:

- create;
- attach existing folder;
- clone repository;
- relocate/rebind path;
- rename display identity;
- change remote;
- archive;
- remove from Dennett without deleting files;
- delete files with recovery/cooling period;
- duplicate/fork;
- export/share memory pack;
- import on another installation;
- missing path;
- detached device;
- permission/trust downgrade;
- ownership transfer.

UI уже содержит многие кнопки, но доменный результат каждой операции должен быть определён до архитектуры.

### 4.18. Idea Incubator, Concept Distiller и Thinking Editor — PROMPT/SKILL FEATURES

Эти функции не потеряны, но им не нужна отдельная service architecture.

Их правильная форма:

- Dennett-native или user skill;
- prompt preset;
- optional project template;
- artifact output;
- project-local memory link;
- быстрый вход из desktop/mobile/voice.

Архитектура обязана поддержать skill/preset execution и artifact creation; отдельные сущности `IdeaIncubatorService` или `ThinkingEditorEngine` запрещены без реального доказательства необходимости.

### 4.19. Daily briefing, monthly review, system sleep — COVERED-BUT-FRAGMENTED

Правильная форма:

- scheduled event;
- configurable skill/procedure;
- query Memory + Runtime + Inbox;
- generate artifact/notification/voice summary;
- user-defined scope and timing;
- cost and interruption budget;
- no output when nothing useful changed.

Не требуется отдельная подсистема briefing.

### 4.20. AI news and World Intelligence — COVERED-BUT-NOT-PRODUCTIZED

Memory Fabric и research procedures поддерживают World Intelligence, claims, freshness и project relevance.

Не хватает только готового product preset:

- sources;
- schedule;
- dedupe;
- claim extraction;
- trust/freshness;
- technology radar;
- project matching;
- digest threshold;
- feedback.

Это automation/skill template, а не новый фундаментальный модуль.

### 4.21. Office — DEFERRED

Office остаётся визуализацией Radar/Workflow state и не блокирует первую архитектуру.

### 4.22. Multi-user live collaboration — OUT OF SCOPE ДЛЯ 1.x

Перенос project memory между людьми поддерживается. Полноценные simultaneous team permissions, shared organization policy, comments, billing и collaborative editing не должны случайно появиться в первой архитектуре.

Архитектура должна не запрещать последующее расширение principals, но не обязана строить enterprise multi-tenancy сейчас.

---

## 5. Cross-domain execution contract

Любой значимый путь Dennett должен раскладываться так:

```text
1. Intake
2. Identity and source classification
3. Intent or event interpretation
4. Current authoritative state lookup
5. Memory/context reconstruction
6. Capability resolution
7. Permission/autonomy decision
8. Execution placement
9. Work/Run lifecycle
10. External-effect handling
11. Result/evidence/artifact
12. User presentation
13. Memory feedback
14. Observability and recovery
```

Не каждый шаг требует отдельного сервиса, агента или model call.

### 5.1. Intake

Источники:

- desktop text;
- mobile text;
- voice committed turn;
- ambient candidate;
- project agent request;
- incoming connector event;
- schedule;
- filesystem/device event;
- capability discovery;
- completion/failure другого Run.

### 5.2. Source and identity

Нужно определить:

- user/device/provider/external source;
- trust domain;
- current session;
- project scope;
- freshness;
- speaker/participant confidence;
- whether content is data or instruction.

### 5.3. State and context

Live source имеет приоритет над memory projection там, где состояние изменчиво.

Примеры:

- repository вместо старого code summary;
- provider message thread вместо memory-only history;
- Trust registry вместо прошлого разрешения;
- Runtime Registry вместо Radar cache;
- current device capture policy вместо старой preference note.

### 5.4. Capability resolution

Capability Fabric возвращает небольшой набор реально доступных кандидатов. Agentic layer выбирает стратегию. Trust разрешает конкретное действие. Server размещает исполнение.

### 5.5. Execution and effect

Различаются:

- pure reasoning;
- local reversible state;
- project file change;
- remote reversible operation;
- external communication;
- destructive/financial/security effect.

### 5.6. Result

Значимый Result Envelope включает:

- outcome;
- summary;
- artifacts;
- evidence;
- changes;
- unresolved items;
- external effects;
- confidence/unknowns;
- next owner/action.

---

## 6. Lifecycle coverage matrix

| Сущность | Создание | Активная работа | Pause/Wait | Завершение | Archive/Delete | Recovery | Канонический владелец |
|---|---|---|---|---|---|---|---|
| Project | частично | да | да | не всегда применимо | **уточнить** | missing/rebind частично | Agentic + Server |
| Project Session | да | да | да | done/archive | да | provider continuation/fork | Agentic |
| Work Item | да | да | локально | consumed | ephemeral | session state | Agentic |
| Task | да | да | да | success/partial/fail/cancel | archive | checkpoint/retry | Agentic + Server |
| Run | да | да | да | result/effect | history | resume/reconcile | Agentic + Server |
| Agent Instance | да | да | wait/pause | complete/fail | terminate | recreate/provider session | Agentic |
| Voice Session | да | duplex | mute/background | end/result | transcript retention | fallback/handoff | Voice + Server |
| Memory Event | append | immutable | n/a | n/a | tombstone/erase | rebuild | Memory |
| Claim/Projection | derive | update | contested | supersede | forget | rebuild | Memory |
| Capability | discover/import | use/health | disabled | deprecated | remove | reinstall/fallback | Capability |
| Permission Grant | request/issue | active | suspended | expire/revoke | audit remains | step-up/reissue | Trust |
| Event/Trigger | create | monitor | snooze/cooldown | fire/expire | remove | late event/re-eval | Server + Memory intent |
| Inbox Card | create | open/revise | snooze | resolve/expire | history | supersede/reopen | Server |
| Artifact | create | version/edit | draft | final/publish | **уточнить** | rehydrate/version | Shared/Agentic amendment |
| Device | pair | healthy/degraded | offline | revoke | remove | re-pair/recover | Server + Trust |
| Backup | create | verify | n/a | complete/fail | retention | restore | Server |

Жирные места являются обязательными documentation deltas до окончательной архитектуры.

---

# Часть III. Каталог обязательных end-to-end сценариев

## 7. Scenario template

Каждый архитектурный сценарий должен иметь:

```yaml
scenario:
  id: stable_id
  title: text
  user_value: text
  preconditions: []
  trigger: text
  actors: []
  authoritative_state: []
  required_context: []
  required_capabilities: []
  required_permissions: []
  normal_flow: []
  user_visible_result: text
  success_evidence: []
  partial_result: text
  cancellation: text
  failure_modes: []
  recovery: []
  offline_behavior: text
  observability: []
  quality_targets: []
  owner_documents: []
```

Ниже сценарии описаны компактно. Архитектурные тома позднее превратят критические из них в sequence diagrams, API/event contracts и tests.

---

## 8. Установка, запуск и восстановление владельца

### INST-01. Первая личная установка

Пользователь устанавливает Head Runtime и desktop client, создаёт владельца, добавляет trusted device, подключает один provider и выполняет первый project chat.

Проверяется:

- bootstrap identity;
- key storage;
- head registration;
- local project folder;
- provider connection;
- first backup prompt;
- no mandatory enterprise setup;
- ability to skip optional ambient features.

### INST-02. Добавление телефона

Desktop показывает pairing request. Телефон проходит device authentication, получает ограниченный scope и появляется в Devices. Потеря push во время pairing не создаёт полу-доверенное устройство.

### INST-03. Восстановление после утери телефона

Владелец с другого trusted device отзывает телефон, блокирует токены и pending commands. Local encrypted cache остаётся недоступен без device key/biometric.

### INST-04. Потеря всех обычных устройств

Используется recovery mechanism. Требуется документированный баланс между восстановлением и защитой от захвата. Recovery не должен зависеть от одного provider account.

### INST-05. Обновление Dennett при разных версиях clients

Head обновляется раньше телефона. Protocol negotiation сохраняет safe degraded behavior; несовместимая команда не выполняется как будто поддерживается.

### INST-06. Rollback неудачного обновления

Миграция данных имеет backup/checkpoint. Возврат приложения не должен читать новую схему без compatibility path.

---

## 9. Проекты и project chats

### PRJ-01. Создание проекта из пустой папки

Пользователь создаёт проект, выбирает папку, получает project session и начинает прямой диалог. Не создаются лишние agents или workflow.

### PRJ-02. Импорт существующего репозитория

Dennett обнаруживает Git, инструкции, project memory, capabilities и workspace trust. До доверия код не исполняется. После Trusted-Bounded обычная работа идёт без постоянных prompts.

### PRJ-03. Продолжение проекта голосом

Пользователь говорит: «Продолжи Dennett, разберись с последней ошибкой». Voice создаёт Intent Proposal; Orchestrator находит project/session; project agent получает context и работает в своей директории.

### PRJ-04. Изменение проекта в интерактивном чате

Agent читает файлы, меняет их, запускает tests, показывает diff. User steering не обязан создавать новую Task.

### PRJ-05. Background Run из project chat

Пользователь отправляет работу в фон. Создаётся Managed Run, сохраняется checkpoint, UI показывает progress, completion возвращает artifact/diff и notification.

### PRJ-06. Project agent запрашивает bounded subagent

Subagent создаётся только после context-coupling и marginal-utility проверки, получает ограниченный scope и возвращает artifact/evidence, а не бесконечный чат.

### PRJ-07. Несколько agents меняют код

Используются worktrees или явные partitions. Integration owner видит conflicts. Один shared mutable directory без ownership запрещён.

### PRJ-08. Project path пропал

UI показывает Missing. User может Locate, Rebind, Clone Again, Keep Metadata или Remove from Dennett. Никакой memory/project history не удаляется автоматически.

### PRJ-09. Archive проекта

Archive прекращает проактивные events и скрывает project from default views, но сохраняет files/memory/history согласно policy.

### PRJ-10. Remove from Dennett

Удаляется регистрация и локальные projections, но исходная папка не удаляется. Разница должна быть очевидной.

### PRJ-11. Delete Project Files

Точное path preview, backup/recovery state, DA3 при high consequence, cooling period или recycle/archive strategy. Memory deletion является отдельным выбором.

### PRJ-12. Перенос проекта другому пользователю

Экспортируется repository + Portable Project Memory Pack без personal overlay/secrets. Получатель монтирует imported trust domain и строит собственные indexes.

### PRJ-13. Upstream изменил AGENTS/CLAUDE instructions

Dennett видит diff, пересобирает Effective Instruction Set, отмечает stale projections и не перезаписывает human content молча.

---

## 10. Агенты, задачи и завершение

### AGT-01. Direct Turn

Простой вопрос о статусе не создаёт Task. Ответ берётся из Runtime Registry и project state.

### AGT-02. Adaptive Agent Session

Один агент самостоятельно меняет план, использует tools и удерживает bounded working context.

### AGT-03. Promotion to Task

Работа становится Task только когда у неё появляется durable lifecycle: фон, ожидание, бюджет, отдельный owner, external effect или необходимость resume.

### AGT-04. Cancellation

Stop сначала блокирует новые эффекты, затем отменяет provider/tool execution где возможно, сохраняет partial artifacts и объясняет, что уже произошло.

### AGT-05. Provider session потеряна

Task не теряется. Dennett создаёт новую provider session из checkpoint/Context Manifest и маркирует возможный semantic drift.

### AGT-06. Ложное completion

Agent утверждает «готово», но tests не запускались. Completion остаётся unverified, UI показывает missing evidence.

### AGT-07. Partial success

Часть результата полезна, часть failed. Outcome не сводится к binary fail; artifacts сохраняются, next steps видимы.

### AGT-08. Multi-agent не окупается

Eval показывает минимальный прирост при кратном росте token cost. Strategy policy возвращается к single agent.

### AGT-09. Workflow извлечён из повторяющейся работы

Сначала сохраняется skill/procedure. Structured Automation создаётся только при необходимости durability/order.

---

## 11. Память и контекст

### MEM-01. Запомнить явный факт

User statement сохраняется как evidence, затем может стать claim/current projection. Exact quote остаётся доступной.

### MEM-02. Пользователь меняет мнение

Новое утверждение не стирает историю. Current state обновляется с valid/transaction time и provenance.

### MEM-03. Исправление памяти

User correction создаёт новое событие, invalidates affected claims и пересобирает projections. Нельзя вручную править каждый index.

### MEM-04. Project context для coding agent

Context включает effective instructions, current branch/commit, active task/session, relevant decisions/errors и минимальную личную память.

### MEM-05. Не найдено доказательство

Система не выдумывает память. Она показывает uncertainty и может открыть source search.

### MEM-06. Переполнение контекста

Используются handles, summaries и iterative retrieval. Архив не вставляется целиком.

### MEM-07. Удаление чувствительного screenshot

Удаляются raw object, OCR, visual vector, episode references, caches и backup copies по retention. Остаётся допустимый content-free tombstone.

### MEM-08. Offline memory update

Device append log синхронизируется; concurrent semantic changes сохраняются как conflict, а не last-write-wins.

### MEM-09. Пересборка index

Canonical event/evidence остаётся доступным. Новый embedding/index строится параллельно и переключается после проверки.

### MEM-10. Imported project memory содержит вредную instruction

Imported content остаётся external-shared data. Instruction activation проходит adapter/trust policy.

---

## 12. Сенсорная память: микрофон и экран

### SNS-01. Постоянное локальное прослушивание телефона

Phone работает в `LOCAL_ACTIVITY`:

1. OS microphone permission активен.
2. Local VAD обнаруживает речь.
3. Ring buffer перезаписывается.
4. Повторяющийся шум и одинаковые фрагменты отбрасываются.
5. Lightweight speaker/activity detector определяет вероятного владельца.
6. Только явное обращение, «запомни», active meeting/project pattern или high-value candidate проходит дальше.
7. Raw audio не загружается в облако без activation/commit policy.
8. UI показывает состояние.
9. Battery and false-capture metrics измеряются.

Failure cases:

- OS убил background process;
- microphone занят звонком;
- два устройства слышат одну речь;
- wake false-positive;
- сеть offline;
- пользователь отключил source физически.

Success: полезная команда/факт не потеряны, мусор не создаёт постоянные записи, privacy state честен.

### SNS-02. Постоянное локальное прослушивание ПК

Аналог SNS-01, но источник связывается с active application, project, window, headset and device ownership. Echo от собственного TTS исключается.

### SNS-03. «Запомни то, что я только что сказал»

Voice command обращается к ring buffer. Выбирается достаточный interval, пользователь может увидеть transcript preview, затем создаётся committed evidence.

### SNS-04. Событийный захват экрана ПК

1. Source adapter получает app/window change.
2. Пытается получить accessibility tree/DOM/selected text.
3. Change detector сравнивает с предыдущим state.
4. При значимом изменении создаётся screenshot/keyframe.
5. Sensitive-app policy применяется до записи.
6. OCR используется как fallback.
7. Capture связывается с active project/session и nearby voice.
8. Content hash дедуплицирует bytes, но не уничтожает разные эпизоды.
9. Retention tier определяется policy.

### SNS-05. Экран не изменился

Сотни одинаковых кадров не сохраняются и не отправляются на модель. Сохраняется только activity metadata, если она полезна.

### SNS-06. Пароль появился на экране

Password-manager/banking/excluded-app policy блокирует или redacts capture. Derived OCR не должен получить secret.

### SNS-07. Пользователь вручную делает region capture

Hotkey открывает overlay, local object получает ID до анализа, user может добавить voice comment и сразу закрыть окно. Classification происходит позже.

### SNS-08. Фото + голосовая фраза

Phone photo и speech interval имеют общий temporal anchor и project link. Memory хранит original image и advisory interpretation.

### SNS-09. Сенсорный источник выключен на час

Command создаёт time-bounded policy. UI показывает countdown; source включается обратно только по policy, а не скрыто.

### SNS-10. Юридически/социально чувствительная встреча

Meeting capture требует соответствующей policy/consent. Dennett должен поддержать visual indicator, participant notice configuration и source exclusion. Конкретные правовые правила зависят от юрисдикции и требуют отдельной проверки перед эксплуатацией.

---

## 13. Голос и ambient

### VOI-01. Push-to-talk вопрос

Local capture, committed turn, context lookup, short answer. Нет Task.

### VOI-02. Live conversation with interruption

User interrupts; playback stops; only heard portion remains in context; beginning of user speech preserved by pre-roll.

### VOI-03. Filler/backchannel

«Угу» не всегда останавливает agent. Turn Manager uses content and timing.

### VOI-04. Self-correction

«Создай проект… нет, открой старый» produces one committed intent.

### VOI-05. Strong-model sidecar

Realtime conversational model calls a strong model through structured bridge. Sidecar returns result/evidence, not hidden chain-of-thought. Late result obeys turn revision.

### VOI-06. Project voice dialogue

Voice routes to existing project session without creating duplicate agent identity.

### VOI-07. Voice command with external effect

Partial transcript cannot dispatch. Exact committed parameters go through Trust. Critical confirmation occurs on trusted device.

### VOI-08. Voice provider failure

Fallback occurs between turns; late audio is discarded; session continuity remains.

### VOI-09. Multi-device wake race

One device becomes conversational owner. Others suppress playback and can act as microphone/approval remote only explicitly.

### VOI-10. Meeting scribe

Diarization, candidate decisions, private vs shareable notes, no autonomous participant commitments.

---

## 14. Events, proactivity and attention

### EVT-01. Deterministic schedule

Timer fires once logically despite restart. Missed schedule policy decides catch-up or skip.

### EVT-02. Semantic trigger

Cheap filters produce candidate; model evaluates only candidate, then no-op/remember/notify/act.

### EVT-03. Repeated event storm

Deduplication, cooldown and aggregation prevent many agents/notifications.

### EVT-04. Proactive useful suggestion

System finds relevant tool for active project, but may save, monitor, suggest or test project-locally instead of installing automatically.

### EVT-05. Interruption budget

Low-value event becomes digest instead of voice/notification.

### EVT-06. Expired trigger

Trigger stops and is archived/removed. It does not live forever.

### EVT-07. Daily briefing

Scheduled skill queries Inbox, Runtime and Memory. If no meaningful delta exists, it produces no verbose report.

### EVT-08. Monthly retrospective

Optional procedure creates artifact with trends and evidence. It does not rewrite user profile as absolute truth.

### EVT-09. AI news monitor

Sources are deduped, claims tracked with freshness, useful items matched to projects, digest threshold respected.

---

## 15. External communication

### COM-01. Incoming Telegram message

Connector resolves account/thread/sender; Memory reconstructs context; agent prepares content/style/disclosure; policy selects ignore/draft/ask/send.

### COM-02. Draft reply

Draft can be highly autonomous. User sees exact recipient, text, attachments and source context.

### COM-03. Preauthorized short reply

Standing bounded mandate allows a specific type of low-consequence response without new prompt. Effect gets idempotency receipt.

### COM-04. Recipient ambiguity

Two contacts have similar names. System does not guess for sensitive disclosure.

### COM-05. Attachment contains injection

Attachment is data. It cannot change recipient, request secret or issue permission.

### COM-06. Timeout after send

Outcome becomes UNKNOWN. Reconciler queries provider/thread before any retry.

### COM-07. User edits Dennett’s draft

Correction updates Communication Model as evidence, without blindly generalizing one edit globally.

### COM-08. Multiple accounts

Project binding chooses exact connector account. Personal and university email are not interchangeable.

---

## 16. Trust, permissions and incidents

### TRU-01. Trusted project normal work

Exact grant + trusted workspace + normal effect = deterministic fast path without LLM security call.

### TRU-02. Agent exits project scope

Reference Monitor blocks or requests bounded expansion.

### TRU-03. Imported malicious skill

Static inspection, restricted first use, no automatic secrets/network; candidate may remain quarantined.

### TRU-04. Voice deepfake requests deletion

Voice alone insufficient. Step-up on trusted device.

### TRU-05. Temporary elevated mode

Scoped by project/device/time, visible and audited; expires automatically.

### TRU-06. Emergency Stop

Stops new effects, pauses consequential Runs, revokes temporary grants, stops computer-use and preserves state.

### TRU-07. Secret Broker

Agent requests operation, not secret value. Connector uses ephemeral credential and returns redacted result.

### TRU-08. Prompt injection on webpage

Factual content remains usable, imperative fragment cannot expand authority; task continues safely when possible.

### TRU-09. Permission answered from two devices

Revision/idempotency permits one canonical resolution. Stale second answer gets explicit feedback.

---

## 17. Capabilities and providers

### CAP-01. User manually imports skill

Added immediately as user-owned; technical inspection only; no mandatory utility review.

### CAP-02. Skill automatically discovered in project

Candidate is inspected, compared and not globally installed. Project-local test may be offered.

### CAP-03. Worse skill has one useful delta

Delta proposal patches/forks existing skill with provenance and replay.

### CAP-04. Provider quota exhausted

Internal Task may fallback under policy; direct chat model does not silently switch.

### CAP-05. Local model selected

Hardware profiler checks artifact/runtime/quantization fit. Failure to load does not corrupt model registration.

### CAP-06. MCP update adds OAuth scope

Capability becomes re-review required; old broad trust not copied silently.

### CAP-07. Native connector vs MCP

Resolution chooses according to capability quality, security, portability and measured outcomes, not protocol fashion.

### CAP-08. Computer-use backend fails

Fallback from structured browser to another backend only if state can be safely reconstructed. Consequential click is not repeated blindly.

### CAP-09. Provider removed model

Registry marks unavailable, migration candidate is shown, reproducible sessions retain historical model identity.

---

## 18. Server, sync, backup and portability

### SRV-01. Head restart during Managed Run

Checkpoint restores goal/state/artifacts/effects/provider handle. External effect status reconciled.

### SRV-02. Device offline capture

Local capture receives stable ID and syncs later without duplicate.

### SRV-03. Concurrent note edits

Conflict-preserving merge; model may propose merge, user asked only if material.

### SRV-04. Split-brain risk

Old Head epoch cannot commit authoritative effect after new Head fencing token.

### SRV-05. Emergency Head without witness

Local work allowed, global consequential operations deferred by default.

### SRV-06. Planned Head migration

Snapshot, catch-up, validate, freeze effects, increment epoch, switch devices, rollback window.

### SRV-07. Provider outage

Task remains in Dennett state; wait/retry/fallback/local/partial/cancel are explicit.

### SRV-08. Backup restore drill

Restore is performed into isolated target, integrity verified, semantic smoke tests run, not merely “backup file exists”.

### SRV-09. Storage pressure from sensory data

Retention tiers, dedupe, compression and user policy apply. Canonical metadata is not silently lost because disk is full.

### SRV-10. Deletion across replicas and backup

Deletion obligation tracks active replicas, derived indexes and backup expiry; UI shows residual limitations.

### SRV-11. Portable client on чужом ПК

Encrypted connection profile, minimal cache, no automatic secrets/head authority, easy cleanup.

### SRV-12. Clock skew

Ordering uses source sequence, transaction time and causal parents, not wall clock alone.

---

## 19. Desktop/mobile continuity

### UI-01. User leaves desktop for Inbox and returns

Resume Strip preserves exact project/session/diff position and shows meaningful delta.

### UI-02. User resolves card from phone

Desktop state updates from canonical revision; no duplicate local resolution.

### UI-03. Mobile app process killed during capture

Local capture staging recovers raw data and draft.

### UI-04. Continue diff on desktop

Phone transfers navigation/control link, not necessarily execution or entire media.

### UI-05. Stale offline view

Object shows freshness and limits actions whose correctness depends on current state.

### UI-06. Accessibility path

All critical commands have keyboard/screen-reader or accessible mobile alternatives; no drag-only required operation.

### UI-07. Privacy Curtain

Sensitive UI is hidden for screen sharing, but underlying permission/data remains unchanged.

### UI-08. Safe command history

Repeating a prior command rebuilds current Action Request; it does not replay old external effect receipt.

---

## 20. Self-improvement and maintenance

### EVO-01. User complains about wrong behavior

System attributes issue to prompt/context/memory/skill/model/tool/runtime and preserves regression case.

### EVO-02. Skill improvement candidate

Replay/shadow/canary before promotion; rollback on regression.

### EVO-03. Memory retrieval regression

Same benchmark corpus compares index/model changes; canonical evidence unchanged.

### EVO-04. Nightly maintenance budget

Maintenance works on changed/high-value areas, not entire history every night.

### EVO-05. System learns to ask less

Repeated approvals may propose bounded pattern; one approval does not silently become global permission.

### EVO-06. Deprecated capability cleanup

Historical provenance preserved, active projects receive migration warning, removal does not delete artifacts.

---

# Часть IV. Quality attributes и проверяемые цели

## 21. Quality tree

### 21.1. Correctness and goal integrity

- project result matches user intent;
- no silent goal drift;
- current state uses authoritative sources;
- claims have evidence;
- completion is verified where possible.

### 21.2. User agency

- user can Stop, Pause, Steer and Undo where physically possible;
- autonomy settings are multidimensional;
- no hidden model/provider switch in direct chat;
- no forced workflow for simple work;
- no excessive approval fatigue.

### 21.3. Security and privacy

- least privilege by task scope;
- fast deterministic permission path;
- voice is not sole critical authenticator;
- external content cannot grant authority;
- secrets are brokered;
- ambient capture is visible and locally filtered;
- deletion is traceable.

### 21.4. Reliability and recoverability

- durable Runs survive restart;
- unknown effects are reconciled;
- offline data is preserved;
- split-brain is fenced;
- backups are test-restored;
- partial results remain usable.

### 21.5. Latency and responsiveness

- local stop/mute/capture are immediate-feeling;
- voice remains conversational;
- UI shell appears before full sync;
- user sees acknowledgement before long work;
- background maintenance never blocks interactive work.

### 21.6. Cost efficiency

- single-agent-first;
- no LLM call for deterministic fast paths;
- ambient processing local by default;
- tool descriptions load lazily;
- deep retrieval and reviews are proportional;
- provider/subscription limits are visible.

### 21.7. Portability and local-first utility

- projects remain normal folders/repos;
- memory packs are exportable;
- local work continues offline;
- providers and storage can be replaced;
- human-readable projections exist;
- user owns backups and keys.

### 21.8. Evolvability

- provider-specific semantics preserved behind adapters;
- canonical data independent of indexes/models;
- schemas/version migrations explicit;
- ADRs explain decisions;
- modules can begin in one process and split only when justified.

### 21.9. Observability

- traces, metrics and logs correlate without storing private reasoning;
- user-visible explanation exists;
- health/freshness are honest;
- important effects have receipts;
- telemetry is vendor-neutral where practical.

OpenTelemetry provides vendor-neutral conventions for traces, metrics and logs and deliberately separates instrumentation from the chosen observability backend. [[S13]]

### 21.10. Accessibility and interruption tolerance

- desktop keyboard-first path;
- mobile one-handed and resumable flows;
- screen readers;
- large text;
- captions;
- no focus theft by streaming updates;
- drafts/captures survive process interruption.

---

## 22. Candidate quality scenarios

Targets below are **initial architecture budgets**, not immutable promises. Они должны быть измерены на реальном hardware и могут измениться через ADR.

### Q-01. Local emergency control

**Stimulus:** user presses Stop/Mute/Emergency Stop.  
**Environment:** high load or provider stall.  
**Response:** local control is accepted immediately; new effects are blocked before remote cleanup.  
**Measure:** no LLM dependency; interaction latency within human-immediate class.

### Q-02. Mobile local capture

**Stimulus:** user finishes voice/photo/text capture and pockets phone.  
**Response:** local durable ID and bytes/draft saved before background classification.  
**Measure:** target local commit under a few hundred milliseconds on supported device; zero loss under process kill tests.

### Q-03. Voice first response

**Stimulus:** normal committed user turn.  
**Response:** acknowledgement/first meaningful audio starts within interactive conversational budget.  
**Measure:** P50/P95 by backend, with separate network and model components.

### Q-04. Project chat visibility

**Stimulus:** user sends request.  
**Response:** UI shows accepted/working state quickly even if substantive answer is long.  
**Measure:** no silent wait; first status under one second under normal local conditions.

### Q-05. Permission fast path

**Stimulus:** trusted project action matches exact grant.  
**Response:** deterministic allow.  
**Measure:** no model call; negligible overhead relative to tool invocation.

### Q-06. Event storm

**Stimulus:** thousands of repeated screen/audio/filesystem events.  
**Response:** dedupe/aggregation prevent model storm and notification flood.  
**Measure:** bounded queue, bounded model calls, no interactive degradation.

### Q-07. Online control-state sync

**Stimulus:** Inbox card resolved on phone.  
**Response:** Head accepts one revision and desktop updates.  
**Measure:** interactive sync within low-seconds budget; stale second action rejected.

### Q-08. Offline capture recovery

**Stimulus:** device reconnects after hours offline.  
**Response:** captures/events sync once logically, preserving source order and project association.  
**Measure:** no duplicate committed evidence; conflicts explicit.

### Q-09. Head failure

**Stimulus:** Head process crashes during Managed Run.  
**Response:** service recovers from checkpoint; effects reconciled.  
**Measure:** target RTO and RPO set per installation profile; no duplicate external effect.

### Q-10. Backup restore

**Stimulus:** primary server lost.  
**Response:** restore owner, projects, memory, grants, artifacts and runtime metadata according to manifest.  
**Measure:** periodic successful isolated restore drill; documented residual loss window.

### Q-11. Sensory overhead

**Stimulus:** ambient mic and screen capture enabled during normal work.  
**Response:** no visible UI lag, bounded CPU/GPU/battery/network usage.  
**Measure:** explicit resource budgets per device class and source mode.

### Q-12. Memory retrieval

**Stimulus:** query requiring exact name, semantic context and temporal current state.  
**Response:** relevant evidence bundle with provenance.  
**Measure:** benchmark recall/precision, stale-current error, latency and token cost.

### Q-13. Provider outage

**Stimulus:** active provider becomes unavailable.  
**Response:** direct chat explains; internal Run follows fallback policy; state preserved.  
**Measure:** no lost Task or hidden model switch.

### Q-14. Deletion

**Stimulus:** user deletes sensitive media.  
**Response:** active indexes/caches/replicas lose access; backup expiry tracked.  
**Measure:** deletion audit and non-retrievability tests for declared threat model.

---

# Часть V. Gap audit и обязательные исправления

## 23. Классификация пробелов

### 23.1. Исторические BLOCKER — закрыты нормативными contract supplements

#### G-01. Ambient Sensory Capture должен стать явным cross-domain contract

Постоянный микрофон и событийный экран описаны, но слишком разрозненно. Архитектурный том обязан включить два отдельных runtime flows: `Ambient Audio` и `Event-driven Screen Context`.

Рекомендуемая документационная правка до начала реализации:

- добавить в Voice/Ambient document отдельный верхнеуровневый раздел **Ambient Sensory Sources**, где audio и screen являются равноправными источниками;
- добавить в Server document explicit Sensor Source Runtime lifecycle;
- в Desktop/Mobile settings явно разделить microphone ambient и screen ambient;
- Memory document уже достаточно определяет ingest/storage.

#### G-02. External Communication Operation

Нужно закрепить compact lifecycle из раздела 4.12 в Agentic Control или Shared Contracts. Без этого connector architecture рискует смешать context generation, send policy и delivery.

#### G-03. Project lifecycle

Нужно определить результаты archive/remove/delete/rebind/clone/transfer. UI-кнопок недостаточно.

#### G-04. Artifact lifecycle

Нужно определить canonical status/version/share/delete semantics и владельца записи.

#### G-05. Update and schema compatibility

Документы упоминают updates, но до архитектуры нужно решить:

- signed packages;
- channels;
- compatibility window;
- client/server protocol negotiation;
- DB/event migration;
- rollback;
- plugin/provider adapter update isolation;
- migration backup.

#### G-06. Identity/key recovery end-to-end

Trust и Server определяют части, но должен быть один recovery scenario: потеря устройств, восстановление owner, backup keys, revoke старого access, предотвращение takeover.

#### G-07. Ambient legal/consent policy boundary

Программная архитектура должна поддержать configurable consent indicators, per-source exclusions, retention and region policy. Конкретное право нельзя «зашить» одним глобальным default.

### 23.2. Архитектурно значимые, но не блокирующие product gaps

#### G-08. Storage pressure policy

Memory tiers описаны, но UX и runtime reaction на disk pressure должны быть едины: warn, reduce raw retention, pause source, offload, never silently lose canonical event.

#### G-09. Usage accounting

Нужно определить authoritative usage model для API cost, subscription estimates, local compute and per-project budget. Не обязательно строить billing service.

#### G-10. Global search indexing

Desktop/mobile search описаны. Архитектура должна решить federated query across projects, memory, artifacts and commands с permissions и freshness.

#### G-11. Locale, timezone and language

Voice, schedules, event time, notifications and dates должны использовать explicit locale/timezone. Travel and clock changes need scenario.

#### G-12. Import/export compatibility contract

Portable packs, settings, skills and artifacts имеют разные formats. Нужны version manifests and migration rules.

### 23.3. Функции, которые не должны стать отдельными подсистемами

- Idea Incubator;
- Concept Distiller;
- Thinking Editor;
- Daily Briefing;
- Monthly Retrospective;
- AI News Monitor;
- Technology Radar;
- Meeting summary template;
- Research dossier;
- Taste review;
- Red-team preset.

Они реализуются через skills, prompts, procedures, automations, memory queries and artifacts.

### 23.4. Осознанно отложенные функции

- animated 2D Office;
- полноценная VR surface;
- universal wearable client;
- enterprise multi-user tenancy;
- automatic emergency-service calls;
- собственная GUI foundation model;
- собственный universal sync engine, если готовые решения достаточны;
- consensus cluster by default;
- full ambient cloud analysis 24/7.

---

# Часть VI. Передача к архитектуре

## 24. Что должна дать программная архитектура

Архитектурный комплект обязан определить:

1. system context и внешние actors;
2. top-level containers/processes;
3. deployment profiles;
4. security/trust zones;
5. data ownership;
6. authoritative stores;
7. consistency classes;
8. event and command transport;
9. APIs and contracts;
10. provider/agent runtime adapters;
11. device agents;
12. voice/media data path;
13. ambient sensor path;
14. memory physical model;
15. sync and backup;
16. external effects;
17. client state architecture;
18. observability;
19. updates/migrations;
20. testing and rollout;
21. repository/module structure;
22. implementation phases.

## 25. Насколько низкоуровневой должна быть архитектура

### 25.1. Обязательный уровень

Архитектура должна дойти до уровня, на котором разработчик может:

- создать repository modules;
- определить process boundaries;
- выбрать storage roles;
- реализовать API/event contracts;
- понять transactional boundaries;
- написать provider adapter;
- реализовать device pairing;
- провести end-to-end test;
- определить deployment;
- восстановить систему после failure.

### 25.2. Не нужно заранее описывать каждый класс

Не следует писать 400 страниц UML-классов, если модель ещё будет меняться.

Code-level detail обязателен только для критических contracts:

- Event Envelope;
- Action Request;
- Permission Decision;
- Effect Claim/Receipt;
- Runtime checkpoint;
- Context Manifest;
- Capability manifest/resolution;
- Memory event/object references;
- sync operation log;
- project/agent/session identifiers;
- Voice committed turn/spoken output;
- sensor capture candidate;
- Inbox revision;
- Artifact descriptor;
- backup manifest.

Для остального достаточно component responsibilities, interfaces, sequences and invariants.

### 25.3. Диаграммы

Обязательны:

- C4 System Context;
- C4 Container;
- component diagrams для major runtimes;
- deployment diagrams для single-device, personal-server and hybrid profiles;
- trust-zone diagram;
- data ownership diagram;
- event/command map;
- critical sequence diagrams;
- storage/data lifecycle diagram;
- sync/failover diagram;
- voice fast/slow pipeline;
- ambient audio and screen pipeline.

### 25.4. Architecture decisions

Каждое дорогое, рискованное или трудно обратимое решение получает ADR:

- status;
- context;
- options;
- decision;
- consequences;
- validation;
- rollback/migration.

ADR не заменяет основной том; он объясняет, почему выбран конкретный вариант. [[S12]]

### 25.5. Interface specifications

После выбора protocols:

- synchronous APIs документируются в machine-readable contract, например OpenAPI, где это подходит; [[S16]]
- asynchronous channels — AsyncAPI или эквивалент; [[S15]]
- event envelope может использовать CloudEvents-compatible semantics там, где это выгодно; [[S14]]
- internal binary/streaming channels не обязаны искусственно становиться REST.

---

## 26. Рекомендуемые четыре крупных архитектурных тома

Пользователь предпочитает сначала 3–4 крупных файла, которые позднее можно разделить. Рекомендуется **четыре** тома по ориентировочно 300–450 КБ каждый. Размер не является целью сам по себе; каждый том должен оставаться цельным по ответственности.

### 26.1. `80_Dennett_System_Architecture_and_Runtime_Topology.md`

**Вопрос:** из каких исполняемых частей состоит Dennett и как они живут вместе?

Содержание:

- architecture drivers;
- stakeholders and quality goals;
- constraints;
- system context;
- solution strategy;
- modular monolith vs processes/services;
- Head Runtime;
- device agent;
- desktop/mobile clients;
- agent runtimes;
- scheduler and durable execution;
- control/event/stream/object channels;
- deployment profiles;
- local-only/personal-server/hybrid;
- lifecycle startup/shutdown/update;
- head handoff/failover;
- security zones;
- resource management;
- runtime sequence diagrams;
- operational failure domains;
- ADR index;
- architecture risk register.

Критические сценарии:

- project chat;
- Managed Run restart;
- device offline/reconnect;
- head migration;
- emergency stop;
- voice routing;
- external effect.

### 26.2. `81_Dennett_Data_Memory_Storage_Sync_and_Protocol_Architecture.md`

**Вопрос:** где и в каком виде живут данные, как они синхронизируются, удаляются и пересобираются?

Содержание:

- canonical data model;
- IDs and references;
- event ledger physical architecture;
- evidence/object storage;
- current projections;
- operational database;
- search/index layer;
- graph/lexical/vector/late interaction;
- artifact storage;
- secret storage boundary;
- project memory pack;
- device caches;
- sync logs;
- consistency matrix;
- conflict resolution;
- migrations/versioning;
- retention/tiering;
- sensory media capacity;
- deletion graph;
- backup/restore;
- API schemas;
- command/event contracts;
- OpenAPI/AsyncAPI/CloudEvents choices;
- data observability;
- load/capacity model.

Критические sequences:

- memory write/read;
- screenshot/audio ingest;
- offline merge;
- index rebuild;
- exact deletion;
- portable export/import;
- backup restore.

### 26.3. `82_Dennett_Agent_Voice_Capability_and_Integration_Architecture.md`

**Вопрос:** как модели, агенты, голос, tools и внешние сервисы реально исполняют работу?

Содержание:

- orchestrator runtime;
- project agent sessions;
- provider adapter interface;
- Context Manifest building;
- Working Memory;
- Direct/Adaptive/Managed/Structured execution;
- cancellation/checkpoints;
- tool invocation;
- Capability Registry and resolver;
- skills/MCP/plugins;
- local model runtimes;
- computer-use adapters;
- connector/account bindings;
- external communication pipeline;
- Effect Claim integration;
- voice transport and turn manager;
- realtime/cascade/hybrid pipeline;
- strong-model sidecar;
- local ambient audio pipeline;
- event-driven screen capture pipeline;
- meeting/diarization;
- provider fallback;
- quota/cost;
- sandbox and Trust enforcement points;
- observability and evaluation.

### 26.4. `83_Dennett_Client_Operations_Testing_and_Implementation_Blueprint.md`

**Вопрос:** как desktop/mobile/local nodes реализуются, поставляются, наблюдаются и превращаются в кодовую базу?

Содержание:

- desktop architecture;
- mobile architecture;
- shared command/state layer;
- local drafts and caches;
- UI projections;
- notifications/widgets/live activities;
- voice/capture OS integration;
- accessibility;
- client/head protocol;
- authentication/pairing UX implementation;
- packaging and installers;
- signed updates;
- schema/protocol compatibility;
- configuration;
- deployment and secrets;
- logging/tracing/metrics;
- OpenTelemetry plan;
- testing pyramid;
- contract tests;
- provider integration tests;
- E2E tests from this document;
- chaos/failure tests;
- privacy/security tests;
- benchmark harness;
- CI/CD;
- release channels;
- repository structure;
- implementation milestones;
- migration from prototype to production.

Security remains cross-cutting in all four volumes; it не переносится целиком в четвёртый файл.

---

## 27. Порядок написания архитектуры

### Phase A — Volume 80

Сначала определить system boundaries, containers, processes, devices, deployment profiles и runtime placement. Без этого нельзя честно выбрать stores и protocols.

### Phase B — Volume 81

После процессов зафиксировать data ownership, storage roles, consistency, schemas, sync and backup.

### Phase C — Volume 82

Затем спроектировать agent/provider/voice/capability execution поверх уже понятных runtime and data boundaries.

### Phase D — Volume 83

Последним собрать clients, packaging, operations, testing and implementation repository blueprint.

Работа итеративная: Volume 81 или 82 может выявить, что Volume 80 нуждается в ADR/delta. Но нельзя начинать с выбора UI framework или vector DB до top-level architecture drivers.

---

## 28. Architecture risk spikes до окончательного выбора технологий

### R-01. Ambient screen capture spike

Проверить на Windows:

- accessibility/UI Automation;
- event sources;
- screenshot change detection;
- OCR fallback;
- resource overhead;
- exclusions/redaction;
- Screenpipe integration vs own adapter.

### R-02. Ambient microphone spike

Проверить phone and desktop:

- background OS restrictions;
- VAD/wake/ring buffer;
- speaker/echo handling;
- battery/CPU;
- duplicate multi-device capture;
- privacy indicators.

### R-03. Provider-native agent sessions

Prototype Codex/Claude/OpenAI/Gemini adapter continuity, cancellation, tool events, session restore and usage visibility.

### R-04. Voice fast/slow bridge

Prototype realtime voice + strong textual model + project session, including stale result cancellation and spoken-output commitment.

### R-05. Memory retrieval at scale

Use real mixed corpus: text, code, screenshots, audio, projects, social context. Compare simple hybrid baseline against graph/advanced lanes.

### R-06. Offline sync and head fencing

Simulate laptop offline, head loss, manual takeover, old head return and duplicate external commands.

### R-07. Exact external effect

Implement one Telegram/email send with idempotency, timeout, UNKNOWN and reconciliation.

### R-08. Portable project pack

Round-trip repository between two installations, checking provenance, privacy and instruction compatibility.

### R-09. Local model serving

Measure user's actual PC/laptop hardware, latency, concurrency, VRAM/RAM and fallback value.

### R-10. Backup restore

Restore a complete synthetic installation and validate semantic integrity, not only database startup.

---

## 29. Initial ADR backlog

1. Modular monolith vs multiple processes.
2. Head Runtime technology and durability model.
3. Operational database.
4. Event ledger representation.
5. Object store and content addressing.
6. Search/index stack.
7. Graph storage strategy.
8. Sync protocol and local log.
9. Device transport.
10. Fencing and head lease.
11. Secret store and key recovery.
12. Provider adapter boundary.
13. Agent runtime integration.
14. Voice transport/backend strategy.
15. Ambient audio implementation.
16. Screen capture implementation.
17. Desktop framework.
18. Mobile platform scope: Android-first vs simultaneous iOS.
19. Shared domain/client code.
20. Plugin/capability isolation.
21. Observability stack.
22. Update and migration strategy.
23. Backup tooling.
24. Portable project pack format.
25. Data retention defaults.

---

# Часть VII. Architecture readiness gates

## 30. Gate 0 — Canonical document set

Pass if:

- latest files are identified;
- historical versions archived;
- canonical names resolved;
- Specification Index updated.

## 31. Gate 1 — Blocking documentation deltas

Pass if G-01 through G-07 have explicit owner and normative delta.

Не обязательно создавать новые большие файлы. Допустимы точечные amendments существующих документов.

## 32. Gate 2 — Quality budgets

Pass if:

- architecture drivers ranked;
- candidate SLOs defined;
- device profiles defined;
- storage/cost assumptions stated;
- privacy/retention profiles stated.

## 33. Gate 3 — Risk spikes

Pass if high-risk unknowns have prototypes or explicit fallback.

## 34. Gate 4 — Four architecture volumes

Pass if:

- context/container/component/deployment views agree;
- data ownership is unambiguous;
- critical sequences covered;
- ADRs explain choices;
- no source-of-truth duplication;
- security enforcement points external to models;
- E2E scenarios map to components.

## 35. Gate 5 — Executable skeleton

Pass if one thin vertical slice works:

```text
desktop or mobile request
→ project/orchestrator
→ provider agent
→ tool/local file
→ result/artifact
→ memory event
→ UI state
→ trace
```

Then add:

- permission;
- Managed Run recovery;
- voice;
- offline capture;
- connector effect.

## 36. Gate 6 — Implementation roadmap

Pass if repository structure, module owners, CI, tests, release channel, migration and incremental milestones are defined.

---

# Часть VIII. Definition of Done бизнес-логического этапа

Комплект бизнес-логики готов к архитектуре, когда:

1. все канонические документы доступны под стабильными именами;
2. функциональная концепция имеет traceability к domain documents;
3. project lifecycle уточнён;
4. artifact lifecycle уточнён;
5. external communication lifecycle уточнён;
6. ambient microphone path является явным;
7. event-driven PC screen capture является явным;
8. update/migration/recovery contracts определены;
9. identity/key recovery описан end-to-end;
10. ambient consent/legal boundary вынесена в explicit policy requirement;
11. все critical scenarios имеют owner state and failure path;
12. quality budgets определены как стартовые measurable hypotheses;
13. архитектурные risks имеют spikes;
14. четыре architecture volumes утверждены;
15. implementation team не должна угадывать, что означает Stop, Unknown, Offline, Delete, Archive, Send, Trust или Capture;
16. prompt/skill features не превращены в лишние services;
17. first release scope отделён от late features;
18. каждый внешний эффект имеет idempotency/reconciliation strategy;
19. sensory data имеет privacy, retention and capacity policy;
20. End-to-End test catalogue может быть превращён в executable suites.

---

# 37. Финальная нормативная формула

> **Dennett готов к архитектурному этапу не тогда, когда написано много документов, а когда каждое обещание продукта проходит через одну непротиворечивую цепочку: источник намерения или события → актуальный контекст → разрешённая capability → наблюдаемое исполнение → проверяемый результат → память и восстановление. Архитектура должна реализовать эту цепочку с минимально достаточной сложностью, сохраняя прямую работу пользователя с сильными агентами, локальную полезность, переносимость, регулируемую автономность и честное поведение при неопределённости.**

---

# Appendix A. Канонические внутренние источники

**[D01] Functional Concept.**  
`00_Dennett_Functional_Concept.md`

**[D02] Specification Index and Shared Contracts.**  
`01_Dennett_Specification_Index_and_Shared_Contracts.md`

**[D03] Memory Fabric 1.2.**  
`10_Dennett_Memory_Fabric.md`

**[D04] Pragmatic Agentic Control Fabric 1.1.**  
`20_Dennett_Agentic_Control_Fabric.md`

**[D05] Trust, Identity, Autonomy and Permissions.**  
`30_Dennett_Trust_Identity_Autonomy_and_Permissions.md`

**[D06] Voice and Ambient Interaction Fabric.**  
`40_Dennett_Voice_and_Ambient_Interaction_Fabric.md`

**[D07] Capabilities, Providers and Integrations.**  
`41_Dennett_Capabilities_Providers_and_Integrations.md`

**[D08] Server Runtime, Events, Sync and Portability.**  
`50_Dennett_Server_Runtime_Events_Sync_and_Portability.md`

**[D09] Desktop Application Business Logic.**  
`60_Dennett_Desktop_Application_Business_Logic.md`

**[D10] Mobile Application Business Logic.**  
`61_Dennett_Mobile_Application_Business_Logic.md`

---

# Appendix B. Внешние источники и методы

**[S09] C4 Model.** Иерархия system, container, component, code; context, dynamic and deployment diagrams.  
https://c4model.com/

**[S10] arc42.** Architecture communication template: goals, constraints, context, solution strategy, building blocks, runtime, deployment, cross-cutting concepts, decisions, quality scenarios, risks and glossary.  
https://arc42.org/overview

**[S11] Carnegie Mellon SEI — Architecture Tradeoff Analysis Method.** Scenario-driven analysis of quality attributes, trade-offs, risks and sensitivity points.  
https://resources.sei.cmu.edu/asset_files/TechnicalReport/2000_005_001_13706.pdf

**[S12] Architectural Decision Records.** Сохранение контекста, решения и последствий архитектурных выборов.  
https://adr.github.io/

**[S13] OpenTelemetry.** Vendor-neutral generation, collection and export of traces, metrics and logs with semantic conventions.  
https://opentelemetry.io/docs/what-is-opentelemetry/

**[S14] CloudEvents.** Vendor-neutral event metadata format useful as reference for interoperable event envelopes.  
https://cloudevents.io/

**[S15] AsyncAPI.** Machine-readable description of event-driven APIs and channels.  
https://www.asyncapi.com/docs/concepts/asyncapi-document

**[S16] OpenAPI Initiative.** Vendor-neutral interface description for HTTP APIs.  
https://www.openapis.org/

---

# Appendix C. Краткий gap ledger

> [!IMPORTANT]
> Это исторический реестр обнаруженных пробелов. В repository edition G-01–G-15 закрыты нормативными файлами из `contracts/`; колонка Severity показывает исходную серьёзность, а не текущий открытый статус.

| ID | Пробел | Исходная severity | Нормативное закрытие |
|---|---|---:|---|
| G-01 | Ambient microphone + screen capture слишком фрагментированы | BLOCKER | `contracts/A_*`, 40, 50, 60/61, 80–83 |
| G-02 | External communication lifecycle | BLOCKER | `contracts/B_*`, 20, 30, 41, 50, 82 |
| G-03 | Project lifecycle | BLOCKER | `contracts/C_*`, 20, 60/61 |
| G-04 | Artifact lifecycle | BLOCKER | `contracts/D_*`, 20, 60/61, 81 |
| G-05 | Update/schema/protocol compatibility | BLOCKER | `contracts/E_*`, 50, 81, 83 |
| G-06 | Identity/key recovery | BLOCKER | `contracts/F_*`, 30, 50, runbooks |
| G-07 | Ambient consent/legal policy boundary | BLOCKER before deployment | `contracts/A_*`, 30, 40, 60/61 |
| G-08 | Sensory storage pressure | High | `contracts/G_*`, 10, 50, 60/61, 81 |
| G-09 | Usage accounting | Medium | `contracts/G_*`, 41, 50, 60/61 |
| G-10 | Federated global search | Medium | `contracts/H_*`, 10, 60/61, 81 |
| G-11 | Locale/timezone/travel | Medium | `contracts/I_*`, 40, 50, 60/61 |
| G-12 | Import/export version compatibility | High | `contracts/J_*`, 10, 41, 50, 81 |
| G-13 | Idea/Concept/Thinking modes | Skill feature | `contracts/K_*`, 20, 41, 60/61 |
| G-14 | Briefing/retrospective | Automation feature | `contracts/K_*`, 20, 41, 50 |
| G-15 | AI news radar | Automation feature | `contracts/K_*`, 10, 20, 41 |

Конец документа.
