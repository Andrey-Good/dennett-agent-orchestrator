# Dennett Desktop Application Business Logic

> **Repository edition · 2026-07-13 · `60`**  
> Это самостоятельный канонический документ репозитория Dennett. Начните с [карты документации](../README.md).  
> Related: [50_Dennett_Server_Runtime_Events_Sync_and_Portability.md](./50_Dennett_Server_Runtime_Events_Sync_and_Portability.md).

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

Эти supplements не дают одному lifecycle разойтись по нескольким большим файлам; canonical owner указан в заголовке каждого документа.


## Полная бизнес-логика desktop-приложения, навигации, экранов, меню, команд, кнопок, состояний и пользовательских сценариев

**Версия:** 1.0  
**Дата исследования:** 12 июля 2026 года  
**Статус:** канонический baseline бизнес-логики desktop-приложения до выбора UI-фреймворка, дизайн-системы и программной архитектуры.  
**Каноническое имя:** `60_Dennett_Desktop_Application_Business_Logic.md`.

Этот документ является самостоятельным. Для понимания основной модели не требуется знать историю обсуждения или читать предыдущие версии спецификаций.

**Dennett** — персональная агентная операционная система. [[S01]] Пользователь напрямую работает с проектными агентами в папках и репозиториях, общается с постоянным главным оркестратором, использует голос, долговременную память, внешние модели и инструменты, запускает фоновые процессы и управляет системой с нескольких устройств. Desktop-приложение является основной полноценной кабиной управления этой системой.

Desktop не является источником истины для памяти, разрешений, задач, событий или внешних эффектов. Границы ответственности следуют Specification Index. [[S02]] Он отображает и изменяет каноническое состояние через операции соответствующих подсистем:

- Memory Fabric определяет память, evidence, current state, correction и forgetting; [[S03]]
- Agentic Control Fabric определяет Project Session, Work Item, Task, Run, Agent Instance и completion; [[S04]]
- Trust Fabric определяет identity, permissions, grants, confirmations и safety floor; [[S05]]
- Voice Fabric определяет Voice Session, turn-taking, ambient interaction и голосовой UX; [[S06]]
- Capability Fabric определяет providers, models, skills, MCP, plugins, connectors и backends; [[S07]]
- Server Runtime определяет долговечное состояние, Action Inbox, Agent Radar, sync, offline, backup и recovery. [[S08]]

Desktop отвечает за то, чтобы все эти возможности были **понятными, быстрыми, наблюдаемыми и не перегружали пользователя лишней административной работой**.

---

# Часть I. Единая UX-модель и границы

## 0. Итоговое решение

### 0.1. Desktop Dennett — Adaptive Agent Workbench

Лучшее решение — не отдельное приложение-чата, не IDE-клон, не dashboard из десятков карточек и не «админка агентной инфраструктуры».

Desktop Dennett должен быть **Adaptive Agent Workbench** — постоянной рабочей оболочкой, где:

- проекты и разговоры занимают центральное место;
- Action Inbox показывает только решения, действительно требующие пользователя;
- Agent Radar показывает живую работу системы, но не заставляет следить за каждым tool call;
- память, artifacts, capabilities и automations доступны как полноценные рабочие пространства;
- технические детали раскрываются по запросу;
- layout адаптируется к текущему типу работы;
- все действия доступны мышью, клавиатурой и через Command Center;
- состояние приложения восстанавливается после перезапуска;
- пользователь может работать в одном окне, нескольких окнах и на нескольких мониторах;
- обычная работа над проектом остаётся прямым диалогом с проектным агентом по принципу Codex App / Claude Code;
- UI никогда не выдаёт локальный кэш за свежее каноническое состояние.

Короткая формула:

> **Dennett Desktop — это настраиваемый agent-first workbench: один глобальный shell, несколько контекстных рабочих пространств, постоянная видимость существенного состояния и минимальное количество прерывающих пользователя решений.**

### 0.2. Основная геометрия

Рабочее окно логически состоит из семи зон:

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ 1. Title Bar + Global Command Center + Global Status Actions                │
├────┬──────────────────┬──────────────────────────────┬───────────────────────┤
│ 2  │ 3. Context       │ 4. Main Workspace           │ 5. Context Inspector  │
│ A  │    Sidebar       │    tabs / panes             │                       │
│ c  │                  │                              │                       │
│ t  │                  │                              │                       │
│ i  │                  │                              │                       │
│ v  │                  │                              │                       │
│ i  │                  │                              │                       │
│ t  │                  │                              │                       │
│ y  │                  │                              │                       │
├────┴──────────────────┴──────────────────────────────┴───────────────────────┤
│ 6. Bottom Panel: terminal / logs / tests / progress / diagnostics           │
├──────────────────────────────────────────────────────────────────────────────┤
│ 7. Status Bar: project / branch / sync / runs / voice / trust / providers   │
└──────────────────────────────────────────────────────────────────────────────┘
```

Это логические зоны, а не обязательные постоянные панели. Любую вторичную зону можно скрыть, переместить, закрепить, открыть во втором окне или вызвать временно.

VS Code показывает практическую ценность layout из editor area, primary и secondary sidebars, activity bar, panel и status bar, включая восстановление расположения после перезапуска, split views и floating windows. Dennett использует этот паттерн, но заменяет code-first центр на контекстно меняющийся agent workbench. [[S09]] [[S10]]

### 0.3. Не двадцать равноправных разделов

Default Activity Rail содержит только основные входы:

1. **Home**;
2. **Orchestrator**;
3. **Projects**;
4. **Action Inbox**;
5. **Agent Radar**;
6. **Library**.

Внизу закреплены:

- Voice;
- Quick Capture;
- System Status;
- Settings / Profile.

`Library` открывает подпространства:

- Memory;
- Artifacts;
- Capabilities;
- Automations and Events.

Пользователь может закрепить любое из них непосредственно в Activity Rail. Так новый пользователь видит шесть понятных направлений, а опытный строит собственную конфигурацию.

Office не является обязательным отдельным глобальным разделом. По умолчанию это один из режимов визуализации Agent Radar. Его можно закрепить отдельно, если пользователь реально им пользуется.

### 0.4. Один объект — несколько представлений

Task, Run, Agent, Memory Item или Artifact не копируются между экранами как независимые сущности. Один канонический объект может быть показан:

- в project chat;
- в Inbox;
- на Radar;
- в Library;
- в Inspector;
- в global search;
- в уведомлении;
- на Home.

Любое изменение должно сразу отражаться во всех открытых представлениях либо показывать, что обновление ещё синхронизируется.

### 0.5. Progressive disclosure

Пользователь сначала видит:

- результат;
- текущее состояние;
- следующий полезный шаг;
- риск или проблему, если она существует.

Далее он может раскрыть:

- краткое объяснение;
- использованные sources;
- изменения файлов;
- tool calls;
- memory influence;
- permission decision;
- полный trace;
- raw diagnostics.

Технические traces не должны занимать conversation по умолчанию. Но они всегда доступны через `Why?`, `Details`, `Open trace` или Context Inspector.

### 0.6. Recognition раньше recall

Интерфейс не заставляет пользователя помнить:

- где был запущен агент;
- к какому проекту относится вопрос;
- что означает статус;
- какие файлы менялись;
- какие permissions активны;
- какой provider используется;
- где лежит созданный artifact.

Эта информация показывается рядом с действием или легко раскрывается. Это следует общему usability-принципу recognition rather than recall. [[S22]]

### 0.7. User control и emergency exits

У каждого долгого или изменяющего состояние действия есть понятный путь:

- cancel;
- stop;
- pause;
- undo;
- restore checkpoint;
- revert file;
- retry;
- continue manually;
- move to background;
- take over;
- emergency stop для всей установки.

Пользователь не должен проходить длинный wizard, чтобы остановить действие. Поддержка undo, redo и ясного выхода является базовым usability-инвариантом. [[S21]]

---

## 1. Область ответственности

### 1.1. Документ определяет

- global navigation и layout desktop-приложения;
- title bar, Activity Rail, sidebars, Main Workspace, Inspector, Panel и Status Bar;
- все baseline menus и Command Center;
- Home;
- главный чат оркестратора;
- Projects Hub;
- Project Workspace;
- project chats и session management;
- Run, changes, tasks, memory, artifacts, capabilities и events внутри проекта;
- Action Inbox;
- Agent Radar и Office view;
- глобальные Memory, Artifact, Capability и Automation workspaces;
- Voice и Capture overlays;
- notifications;
- Settings, Devices, Sync, Backup, Usage и Diagnostics;
- window management, tabs, split panes и floating windows;
- keyboard-first работу;
- drag-and-drop и context menus;
- empty, loading, degraded, offline, conflict и error states;
- accessibility;
- user customization;
- desktop-specific onboarding;
- UX-метрики, acceptance criteria и этапы внедрения.

### 1.2. Документ не определяет

- визуальный бренд, цветовую палитру и окончательный художественный стиль;
- UI-фреймворк;
- язык реализации;
- физические API;
- схему БД;
- алгоритм retrieval;
- agent strategy;
- provider catalogue;
- security policy;
- sync algorithm;
- мобильный интерфейс.

Он определяет бизнес-поведение, которое выбранная архитектура и дизайн обязаны сохранить.

### 1.3. UI не создаёт скрытую власть

Desktop может:

- инициировать command;
- показывать preview;
- редактировать parameters;
- запрашивать canonical state;
- отображать decision;
- кэшировать view state.

Desktop не может сам:

- выдать permission;
- объявить Task завершённой;
- считать notification подтверждением;
- считать локально изменённый status каноническим до подтверждения runtime;
- повторить UNKNOWN external effect;
- повысить workspace trust без Trust operation;
- молча сменить direct-chat model;
- удалить память только из локального view.

---

## 2. Исследовательский протокол

### 2.1. План для построения плана

Перед проектированием были зафиксированы вопросы:

1. Как сохранить прямую project work и не превратить приложение в monitoring dashboard?
2. Как показать несколько агентов и процессов, не заставляя пользователя управлять микрошагами?
3. Как соединить agent-first и file/project-first способы работы?
4. Где хранить решения пользователя, а где обычные уведомления?
5. Как сделать десятки подсистем доступными без двадцати постоянных пунктов навигации?
6. Как пользователь понимает, что свежо, что offline, что завершено и что только заявлено моделью?
7. Как быстро перейти от результата агента к diff, artifact, source, memory и permission?
8. Какие действия должны быть доступны мышью, клавиатурой и голосом?
9. Как восстановиться после ошибки или ложного действия?
10. Что требуется accessibility с первого релиза?

### 2.2. Обязательные сценарии

Проверены как минимум:

- запуск приложения утром;
- продолжение вчерашнего project chat;
- создание проекта из folder/repository;
- одновременная работа нескольких sessions;
- agent просит решение;
- agent меняет файлы;
- пользователь комментирует diff;
- Run уходит в background;
- пользователь возвращается через несколько часов;
- provider недоступен;
- desktop offline;
- sync conflict;
- permission confirmation;
- voice interaction поверх текущего проекта;
- memory correction;
- import skill/MCP;
- attach capability к проекту;
- create event по natural language;
- backup failure;
- emergency stop;
- работа с клавиатуры без мыши;
- screen reader;
- несколько мониторов;
- маленькое окно;
- первая установка без проектов;
- advanced user с десятками проектов и сотнями sessions.

### 2.3. Изученные reference-системы

Использованы:

- VS Code Workbench: стабильная геометрия, настраиваемые views, tabs, split panes, floating windows, Command Palette, workspace settings и DND; [[S09]] [[S10]]
- VS Code Agents window: agent-first и code-first surfaces, общие sessions, sessions list, changes panel, parallel sessions, trust и local validation; [[S11]] [[S12]]
- OpenAI Codex app: project-organized threads, review changes, worktrees, automations и review queue; [[S16]]
- Linear: Inbox, snooze, keyboard navigation, global/current-view search, favorites и saved organization; [[S17]] [[S18]] [[S19]]
- Microsoft Guidelines for Human-AI Interaction и HAX Toolkit: поведение при неопределённости, ошибках и долгосрочной адаптации; [[S20]]
- Nielsen usability heuristics: system status, control, recognition, efficiency и recovery; [[S21]] [[S22]]
- WCAG 2.2: keyboard access, focus visibility, status messages, interruption control, target size и error prevention. [[S23]]

### 2.4. Критерии принятия решения

UX-механизм принимается, если он:

- сокращает путь к важному действию;
- уменьшает когнитивную нагрузку;
- показывает каноническое состояние и freshness;
- не требует помнить hidden state;
- работает клавиатурой;
- имеет понятный error/recovery path;
- не добавляет постоянную панель ради редкого сценария;
- масштабируется от одного проекта до десятков;
- не создаёт второй источник истины;
- не требует отдельного LLM-вызова только ради UI;
- может быть отключён или переставлен без потери данных.

### 2.5. Критерии отказа

Механизм отклоняется или становится optional, если:

- требует изучить внутреннюю архитектуру Dennett до первого полезного действия;
- дублирует существующее представление без новой задачи;
- скрывает Stop, Undo или source;
- превращает каждый agent step в notification;
- показывает зелёный статус без актуального подтверждения;
- заставляет пользователя вести Kanban ради простого chat;
- автоматически открывает шумные панели;
- требует модального диалога для частой операции;
- полагается только на цвет;
- нельзя использовать с клавиатуры;
- ломает focus при streaming update;
- делает advanced режим default для новичка;
- повышает плотность интерфейса без измеримой пользы.

---

## 3. Неподвижные UX-инварианты

1. Главный рабочий путь проекта — прямой chat с project agent.
2. Пользователь всегда видит, к какому Project, Session и Run относится действие.
3. Значимый процесс имеет Stop или Cancel, если физически ещё можно остановить.
4. Изменение файлов ведёт к доступному diff.
5. Consequential action показывает exact target и effect.
6. Action Inbox отделён от обычных notifications.
7. Agent Radar является проекцией, а не отдельным состоянием.
8. Healthy infrastructure не занимает центр внимания.
9. Stale и offline data явно маркируются.
10. Все базовые команды доступны через Command Center.
11. Все важные действия имеют keyboard path.
12. Контекстные действия расположены рядом с объектом.
13. Destructive operation не является соседней неотличимой иконкой без label/tooltip.
14. UI не скрывает неопределённость модели.
15. Technical detail доступен, но свернут по умолчанию.
16. Пользователь может закреплять, скрывать и переставлять views.
17. Layout и открытые tabs восстанавливаются после restart.
18. Проектные настройки не меняют global settings без явного indication.
19. Provider-specific capability показывается только там, где она доступна.
20. Любой badge имеет текстовое объяснение.
21. Streaming updates не отнимают keyboard focus.
22. Background completion не открывает окно поверх текущей работы без срочности.
23. Notification не считается решением.
24. UI показывает, что было действительно выполнено, а что только предложено.
25. Любая автоматическая персонализация layout обратима.

---

# Часть II. Глобальный Workbench Shell

## 4. Title Bar и Global Command Center

### 4.1. Состав Title Bar

Слева направо:

- кнопка главного application menu на платформах без классического menu bar;
- Back;
- Forward;
- Current Location / breadcrumbs;
- Global Command Center;
- installation/head indicator;
- global running indicator;
- voice button;
- Action Inbox badge;
- notification bell;
- profile/account button;
- стандартные window controls.

### 4.2. Back и Forward

Back/Forward работают по history навигации между:

- проектами;
- sessions;
- memory items;
- artifacts;
- Inbox cards;
- Radar objects;
- settings pages;
- diff views.

Они не отменяют действие. Undo является отдельной командой.

Long press или context menu открывает историю последних переходов.

### 4.3. Breadcrumbs

Breadcrumbs показывают логическое местоположение:

```text
Installation > Project > Session > Artifact
Installation > Inbox > Permission request
Installation > Library > Memory > Topic note
Installation > Radar > Managed Run
```

Каждый сегмент кликабелен и имеет context menu.

Breadcrumbs можно скрыть в Focus Mode.

### 4.4. Global Command Center

Одно поле выполняет четыре функции:

- поиск сущностей;
- переход;
- запуск command;
- создание нового объекта.

Режим определяется prefix:

```text
без prefix     semantic/global search
>              command
p:             projects
s:             sessions/chats
t:             tasks and runs
m:             memory
a:             artifacts
c:             capabilities
e:             events/automations
@              agents, people, devices
/              filter current view
```

Command Center показывает:

- recent items;
- pinned favorites;
- suggested next actions;
- exact matches;
- semantic matches;
- source category;
- project scope;
- freshness;
- keyboard hint.

Search не раскрывает объекты, которых principal не вправе видеть.

### 4.5. Search result actions

Для выбранного результата доступны:

- Open;
- Open to Side;
- Open in New Window;
- Pin;
- Copy Link;
- Add to Current Context;
- Send to Orchestrator;
- Reveal Source;
- Context Menu.

### 4.6. Installation / Head indicator

Показывает только необычное состояние:

- Online — обычно скрыт или нейтрален;
- Connecting;
- Degraded;
- Offline;
- Emergency Head;
- Split-brain risk;
- Safe Mode;
- Update/restart required.

Нажатие открывает System Status popover.

### 4.7. Global running indicator

Компактно показывает число:

- активных foreground operations;
- background Runs;
- процессов, ожидающих пользователя;
- stalled/failed процессов.

Нажатие открывает filtered Radar.

### 4.8. Voice button

Состояния:

- idle;
- listening;
- processing;
- speaking;
- muted;
- unavailable;
- local-only;
- active on another device.

Primary click начинает или открывает Voice Session. Context menu:

- Push to Talk;
- Start Live Conversation;
- Continue in Current Project;
- Talk to Orchestrator;
- Mute Microphone;
- Mute Output;
- End Session;
- Voice Settings;
- Privacy Mode.

### 4.9. Action Inbox badge

Badge показывает только unresolved cards, а не все notifications.

Цвет/shape различает:

- обычное решение;
- срочное;
- permission;
- conflict;
- failed/stalled process.

Нажатие открывает Inbox. Secondary action открывает маленький quick-resolution popover для последней срочной карточки.

### 4.10. Notification bell

Открывает Notification Center. Badge не смешивается с Inbox count.

Context menu:

- Do Not Disturb;
- DND for 1 hour;
- DND until tomorrow;
- Allow critical only;
- Notification Settings.

---

## 5. Activity Rail

### 5.1. Default entries

- Home;
- Orchestrator;
- Projects;
- Action Inbox;
- Agent Radar;
- Library.

Bottom anchors:

- Voice;
- Capture;
- System Status;
- Settings/Profile.

### 5.2. Badges

Activity entries могут показывать:

- unresolved count;
- running count;
- error dot;
- sync conflict;
- update available.

Badge скрывается, если значение равно нулю.

### 5.3. Настройка Activity Rail

Context menu:

- Pin View;
- Unpin View;
- Move Up;
- Move Down;
- Move to Bottom;
- Show Label;
- Hide Label;
- Open in New Window;
- Reset Rail.

Drag-and-drop меняет порядок.

### 5.4. Dynamic entries

Можно закрепить:

- конкретный проект;
- конкретную session;
- saved Radar view;
- memory collection;
- artifact collection;
- capabilities;
- automations;
- Office;
- Devices;
- Usage;
- Diagnostics.

Dynamic entry показывает icon исходного типа и optional project color/icon.

---

## 6. Context Sidebar

### 6.1. Назначение

Sidebar показывает навигацию и objects текущего Activity, а не глобально одинаковый список.

Примеры:

- Projects: группы проектов и filters;
- Project Workspace: sessions, project sections и recent items;
- Inbox: card list;
- Radar: run list;
- Memory: collections и filters;
- Capabilities: categories;
- Settings: settings tree.

### 6.2. Общие элементы Sidebar

- title;
- primary create/action button;
- search/filter;
- saved views;
- grouped tree/list;
- collapse all;
- overflow menu;
- optional footer summary.

### 6.3. List item anatomy

Каждый item может содержать:

- icon/avatar;
- title;
- subtitle;
- state indicator;
- badge;
- freshness;
- pin;
- hover actions;
- context menu.

Hover actions не должны быть единственным способом выполнить критичное действие; те же commands доступны в context menu и Command Center.

### 6.4. Resizing и placement

Sidebar можно:

- resize;
- hide;
- move left/right;
- detach into floating window;
- preserve width per workspace profile.

---

## 7. Main Workspace

### 7.1. Tabs

Любой значимый объект может открываться в tab:

- project chat;
- run details;
- diff;
- file;
- memory note;
- artifact;
- capability;
- event;
- settings page;
- diagnostics trace.

Tab содержит:

- icon/type;
- title;
- project badge;
- dirty/unsaved indicator;
- running/waiting indicator;
- close;
- optional pin.

### 7.2. Preview tabs

Single-click может открыть preview tab, который заменяется следующим preview. Double-click, edit или Pin превращает его в постоянный tab.

Для accessibility preview behavior можно отключить.

### 7.3. Split panes

Поддерживаются:

- split right;
- split down;
- open to side;
- move tab to group;
- duplicate view;
- synchronized compare;
- focus group;
- maximize group.

Типичные сценарии:

- chat + diff;
- chat + artifact;
- two sessions side by side;
- memory source + current note;
- Radar + Run details;
- settings + capability manifest.

### 7.4. Floating windows

Можно вынести:

- tab;
- Inspector;
- terminal;
- Radar;
- Voice transcript;
- Office;
- artifact preview.

Floating window может иметь `Always on Top` и сохранять положение на мониторе.

### 7.5. Focus Mode

Focus Mode скрывает:

- Activity Rail;
- Sidebar;
- Inspector;
- Bottom Panel;
- noncritical notifications.

Остаются:

- active workspace;
- minimal title controls;
- emergency/critical status;
- voice indicator.

Double Escape или configured shortcut выходит из режима.

---

## 8. Context Inspector

### 8.1. Назначение

Inspector отвечает на вопрос:

> «Что именно относится к выбранному объекту и почему система действует так?»

### 8.2. Общие вкладки Inspector

- Overview;
- Context;
- Activity;
- Sources;
- Permissions;
- Details.

Набор вкладок зависит от объекта.

### 8.3. Overview

Показывает:

- state;
- owner;
- project;
- current action;
- next expected action;
- freshness;
- risk;
- linked objects.

### 8.4. Context Lens

`Context` показывает, что получает agent/run:

- goal;
- effective instructions;
- selected memory handles;
- project facts;
- attached files;
- active capabilities;
- permission envelope;
- excluded/sensitive sources;
- estimated context footprint.

Действия:

- Add Context;
- Remove Advisory Context;
- Pin for Session;
- Open Source;
- Explain Why Included;
- Refresh;
- Compare with Previous Turn.

Обязательные safety/instruction items нельзя удалить обычной кнопкой; UI объясняет источник и путь изменения.

### 8.5. Activity

Показывает human-readable timeline значимых действий. Raw trace открывается отдельно.

### 8.6. Sources

Показывает evidence, citations, file/commit/version и freshness.

### 8.7. Permissions

Показывает active grants и boundaries, но не позволяет расширить их обходом Trust Fabric.

Действия:

- View Grant;
- Request Change;
- Revoke;
- Set Expiry;
- Open Trust Settings.

### 8.8. Why This Happened

Для любого выбранного AI-result доступна команда `Why This Happened`, собирающая:

- user intent;
- selected context;
- model/provider;
- capabilities;
- significant decisions;
- permission decision;
- evidence;
- uncertainty;
- outcome.

Она не раскрывает скрытый chain-of-thought. Она показывает проверяемое rationale и causal trace.

---

## 9. Bottom Panel

### 9.1. Default tabs

- Terminal;
- Output;
- Problems;
- Tests;
- Run Progress;
- Logs.

Optional:

- Browser;
- Database/Query;
- Device Console;
- Voice Diagnostics;
- Sync;
- Provider Logs.

### 9.2. Auto-open policy

Panel автоматически открывается только если:

- foreground command требует terminal interaction;
- test/build failed и пользователь ожидает результат;
- action заблокирован и panel содержит необходимую причину;
- пользовательская настройка разрешает auto-open.

Background log или обычный success не должен красть экран.

### 9.3. Panel controls

- New Terminal;
- Kill Terminal;
- Split Terminal;
- Clear;
- Scroll Lock;
- Follow Output;
- Filter;
- Export;
- Open in Editor;
- Move Panel;
- Maximize;
- Hide.

### 9.4. Output ownership

Каждый stream показывает source:

- agent/run;
- device;
- provider;
- tool;
- project;
- timestamp.

---

## 10. Status Bar

### 10.1. Default philosophy

Status Bar показывает то, что помогает текущей работе. Healthy global state не превращается в постоянную гирлянду.

### 10.2. Left side

Контекстно:

- active project;
- branch/worktree;
- current device/execution node;
- sync state;
- problems count;
- changes count.

### 10.3. Right side

Контекстно:

- active model/provider;
- execution profile;
- permission/trust mode;
- active runs count;
- usage/quota warning;
- voice/ambient state;
- notifications/DND;
- update/backup warning.

### 10.4. Interaction

Каждый item открывает соответствующий popover или command.

Context menu позволяет:

- Hide Item;
- Pin Item;
- Move Left/Right;
- Show Label/Icon Only;
- Reset Status Bar.

### 10.5. Freshness

Stale cached status показывает age или `last updated`. Нельзя отображать `Online`, если heartbeat старый и сервер считает node stale.

---

## 11. Layout Profiles и Workspaces

### 11.1. Built-in profiles

- Default;
- Project Chat;
- Review;
- Research;
- Monitoring;
- Memory;
- Focus;
- Presentation;
- Accessibility Large Targets;
- Multi-Monitor.

### 11.2. Project-specific layout

Project может запоминать:

- открытые tabs;
- panel positions;
- sidebar mode;
- last session;
- preferred views;
- window placement.

Это presentation state, а не project memory authority.

### 11.3. Save Layout

Действия:

- Save Current Layout;
- Save As New Profile;
- Update Profile;
- Reset to Default;
- Export Layout;
- Import Layout;
- Set as Project Default.

### 11.4. Presentation Mode

Скрывает:

- secrets;
- token/usage details;
- private memory;
- personal notifications;
- hidden projects;
- raw logs.

Полезен для screen sharing. Это UI-защита, а не изменение underlying permissions.


# Часть III. Основные рабочие пространства

## 12. Home — точка возвращения, а не рекламный dashboard

### 12.1. Назначение

Home отвечает на четыре вопроса:

1. Что требует моего внимания?
2. Что сейчас работает?
3. К чему я, вероятно, хочу вернуться?
4. Есть ли системная проблема, которую нельзя игнорировать?

Home не должен показывать декоративные метрики, советы ради советов или все существующие подсистемы одновременно.

### 12.2. Header Home

Содержит:

- приветствие или текущий focus;
- date/time только если полезно;
- `New Project`;
- `Quick Chat`;
- `Talk to Dennett`;
- `Quick Capture`;
- overflow menu.

Overflow menu:

- Customize Home;
- Set Daily Briefing Layout;
- Hide Greeting;
- Refresh;
- Open Home in New Window;
- Reset Home.

### 12.3. Resume

Верхний блок предлагает максимум три наиболее вероятных продолжения:

- последняя активная project session;
- Run, который недавно потребовал ответа;
- artifact/diff, оставленный открытым;
- interrupted voice discussion;
- recent orchestrator focus.

Карточка содержит:

- object title;
- project;
- last meaningful action;
- elapsed time;
- what changed since user left;
- state;
- primary `Resume`;
- `Open to Side`;
- `Dismiss from Resume`.

### 12.4. Requires You

Это проекция Action Inbox, но не копия отдельной очереди.

Показывает до пяти cards с наивысшим attention score. Действия:

- primary recommended option;
- Open;
- Discuss;
- Snooze;
- Delegate to Orchestrator;
- Dismiss, если допустимо.

`View All` открывает Action Inbox.

### 12.5. Running Now

Показывает значимые active Runs и Sessions:

- title;
- project;
- agent/provider;
- progress phase;
- last meaningful progress;
- current wait/blocker;
- budget warning;
- primary `Open`;
- `Pause`;
- `Stop`;
- `Steer`.

Микрозадачи и короткие tool calls сюда не попадают.

### 12.6. Recent Projects

Поддерживает:

- grid/list;
- pin;
- sort by recent/attention/name;
- hide archived;
- quick new session;
- open project;
- reveal folder;
- context menu.

### 12.7. Completed Since Last Visit

Компактный список:

- completed Runs;
- failed Runs;
- new artifacts;
- closed Inbox cards;
- automations with result.

Действия:

- Open Result;
- Review Changes;
- Save Artifact;
- Archive;
- Mark as Seen.

### 12.8. Briefing

Optional блок, генерируемый не при каждом открытии Home, а по configured schedule или по запросу.

Содержит:

- важные события;
- обещания/дедлайны;
- project changes;
- limits;
- recommended focus.

Действия:

- Ask Follow-up;
- Turn Into Tasks;
- Edit Briefing Skill;
- Hide Today;
- Configure.

### 12.9. System Health

Если всё здорово, показывается одна нейтральная строка `All systems operational` либо блок скрыт.

При проблеме показывается:

- affected subsystem;
- user impact;
- whether work can continue;
- recommended action;
- `Open System Status`;
- `Retry`;
- `Work Offline`;
- `Dismiss`, только если проблема некритична.

### 12.10. Empty Home

Первый запуск предлагает не tour из десятков слайдов, а четыре понятных действия:

- Open or Create Project;
- Connect a Provider;
- Talk to Dennett;
- Import Existing Installation.

Дополнительно:

- `Use Local Model Only`;
- `Explore with Demo Project`;
- `Open Documentation`.

---

## 13. Orchestrator Workspace

### 13.1. Основная идея

Главный оркестратор имеет один постоянный пользовательский канал, но это не означает бесконечный prompt и не запрещает создавать отдельные focus segments.

Интерфейс выглядит как conversation workspace, а не как control dashboard.

### 13.2. Header

Показывает:

- `Orchestrator`;
- current focus;
- connection/head state;
- selected direct-chat model;
- execution profile;
- Voice;
- New Focus;
- History;
- Context Inspector;
- overflow.

### 13.3. New Focus

`New Focus`:

- закрывает текущий temporary working set;
- не удаляет историю;
- не очищает долговременную память;
- создаёт новый focus marker;
- предлагает перенести незавершённые items;
- сохраняет предыдущий segment в history.

Dialog содержит:

- optional title;
- carry over: selected projects, open questions, pinned context;
- start clean;
- cancel.

### 13.4. Current Focus Strip

Над conversation может отображаться компактная strip:

- focus title;
- active projects;
- open delegated work;
- pinned context;
- unresolved question.

Действия:

- Edit Focus;
- Add Project;
- Clear Temporary Context;
- Open in Inspector;
- Collapse.

### 13.5. Composer

Содержит:

- multiline input;
- Add/Attach;
- `@` mention;
- slash/command access;
- context indicator;
- execution profile;
- direct-chat model;
- Voice;
- Send;
- Send as Background Work;
- Schedule;
- More.

#### Add/Attach

- File;
- Folder;
- Project;
- Session;
- Memory Item;
- Artifact;
- Screenshot;
- Clipboard;
- Camera/Screen Capture;
- URL;
- Connector Object;
- Current Selection.

#### `@` mention

- project;
- agent;
- session;
- task/run;
- person/contact;
- device;
- capability;
- memory handle.

#### More

- Start from Template;
- Request Multiple Options;
- Force Local Only;
- Require Sources;
- Prepare Only;
- Set Time/Budget;
- Advanced Context;
- Clear Draft.

### 13.6. Send semantics

`Enter` behavior configurable:

- Enter sends / Shift+Enter newline;
- Ctrl/Cmd+Enter sends;
- always explicit Send.

While current response is running, Send transforms contextually into:

- `Add to Queue`;
- `Steer Now`;
- `Stop and Send`.

This follows current agent-session UX where a user can queue the next request, steer ongoing work or stop and replace it. [[S12]]

### 13.7. Message blocks

User message shows:

- actual text;
- attachments;
- scope/project;
- timestamp optional;
- edit/retry/fork actions;
- delivery state.

Agent message may contain:

- concise answer;
- progress block;
- action block;
- question block;
- source/evidence block;
- artifact preview;
- file changes summary;
- result envelope;
- warning/uncertainty.

### 13.8. Agent response actions

Hover/context toolbar:

- Copy;
- Quote Reply;
- Continue From Here;
- Fork Session;
- Save as Note;
- Save as Artifact;
- Create Project;
- Create Task;
- Add to Project;
- Open Sources;
- Why This Happened;
- Report Problem;
- Regenerate, если действие не имело внешних эффектов;
- More.

### 13.9. Tool/action blocks

По умолчанию показывают:

- action label;
- state;
- target;
- result summary;
- duration;
- effect badge.

Можно раскрыть:

- exact parameters;
- stdout/stderr;
- permission decision;
- evidence;
- retry history;
- raw event.

Кнопки зависят от состояния:

- Open;
- Stop;
- Retry;
- Reconcile;
- View Receipt;
- Copy Command;
- Open Terminal;
- Report Unexpected Behavior.

### 13.10. Orchestrator history

History открывает:

- focus segments;
- dates;
- summaries;
- linked projects;
- unresolved items;
- archived discussions.

Действия:

- Open;
- Resume as New Focus;
- Search;
- Rename;
- Export;
- Archive;
- Delete local presentation only;
- Request memory deletion for underlying content.

### 13.11. Quick Chat

Quick Chat не относится к проекту и не загрязняет постоянный orchestrator focus автоматически.

Он может быть:

- ephemeral;
- saved;
- converted to orchestrator focus;
- converted to project;
- attached to existing project.

Кнопки:

- Save;
- Convert;
- Close and Forget Presentation;
- Export.

---

## 14. Projects Hub

### 14.1. Header

- `Projects`;
- New Project;
- Import;
- Search;
- Filter;
- Sort;
- View mode;
- Saved Views;
- overflow.

### 14.2. Primary actions

#### New Project

Открывает lightweight creation sheet:

- name or description;
- location: new folder / existing folder / Git clone / no files yet;
- optional project type hint;
- optional initial goal;
- create.

Advanced options раскрывают:

- target device;
- provider/model preference;
- trust mode;
- capability profile;
- portable memory;
- Git initialization;
- template.

#### Import

- Existing Folder;
- Existing Repository;
- Clone URL;
- Project Memory Pack;
- Archive/Export;
- Provider Workspace;
- Artifact as Project;
- Recent Folder.

#### Create from Description

Пользователь описывает проект; оркестратор предлагает folder, initial structure, capabilities и first session. До подтверждения ничего внешнего не создаётся, если пользователь выбрал preview mode.

### 14.3. Views

- Cards;
- Compact List;
- Table-like Dense List;
- Groups;
- Timeline of recent activity.

Default — Cards при малом числе проектов, Compact List при большом.

### 14.4. Filters

- Active;
- Needs Attention;
- Running;
- Recent;
- Pinned;
- Sleeping;
- Archived;
- Local Only;
- Shared/Imported;
- Device;
- Trust state;
- Provider;
- Has conflicts;
- Missing capabilities.

### 14.5. Sort

- Last active;
- Needs attention;
- Name;
- Created;
- Running count;
- Manual order.

### 14.6. Project card

Содержит:

- name/icon;
- path/repository;
- short current summary;
- last active;
- active sessions/runs;
- attention badge;
- sync/trust problem only if relevant;
- primary `Open`;
- hover `New Chat`;
- overflow.

### 14.7. Project card overflow

- Open;
- New Chat;
- Talk About Project;
- Start Background Task;
- Open Folder;
- Open in Editor;
- Open Terminal;
- Reveal in File Manager;
- Copy Path;
- Pin/Unpin;
- Rename Display Name;
- Move to Group;
- Export Project Memory Pack;
- Refresh/Rescan;
- Trust Settings;
- Archive;
- Remove from Dennett;
- Delete Files, отдельная destructive operation.

### 14.8. Groups and favorites

Пользователь может создавать группы:

- Work;
- University;
- Dennett;
- Experiments;
- Archive;
- произвольные.

Project можно drag-and-drop в группу. Группа влияет только на UI organization, если явно не настроена как policy scope.

### 14.9. Missing project state

Если folder/repository недоступен:

- card остаётся;
- состояние `Location unavailable`;
- показывается last known state;
- кнопки `Locate`, `Reconnect Device`, `Open Memory Only`, `Remove`;
- agent execution блокируется или переносится на доступный replica.

### 14.10. Import review

После открытия чужого repository показывается concise trust preview:

- source;
- detected executable integrations;
- instructions;
- hooks/scripts;
- MCP/plugins;
- secrets risk;
- recommended mode.

Действия:

- Open Restricted;
- Trust Bounded;
- Review Details;
- Cancel.

---

## 15. Project Workspace — основная рабочая среда

### 15.1. Project Header

Постоянно показывает:

- project name;
- logical path/breadcrumb;
- current branch/worktree, если применимо;
- current session;
- sync/trust issue, только если есть;
- New Chat;
- Voice;
- Run/Task;
- Open Folder/Editor;
- Project Command Menu;
- More.

Optional compact indicators:

- changes count;
- active run;
- Inbox count;
- current model;
- execution node.

### 15.2. Project Sidebar modes

Верхний segmented switch:

- **Sessions**;
- **Project**.

#### Sessions mode

Показывает:

- active chats;
- background runs;
- completed recent;
- archived;
- custom groups.

#### Project mode

Показывает sections:

- Overview;
- Files;
- Changes;
- Runs;
- Tasks;
- Artifacts;
- Memory;
- Capabilities;
- Events;
- Settings.

Каждый section можно открыть в Main Workspace и pin как tab.

### 15.3. Project Overview

Показывает:

- project goal/current summary;
- Resume latest session;
- Needs attention;
- Running;
- recent changes;
- recent artifacts;
- important memory;
- missing capabilities;
- project health only if non-normal.

Actions:

- New Chat;
- Continue Last;
- Start Task;
- Talk;
- Open Files;
- Review Changes;
- Edit Project Summary;
- Customize Overview.

### 15.4. Session list

Session item содержит:

- title;
- type: chat / quick / managed run / review / voice-linked;
- model/runtime;
- state;
- file change count;
- last meaningful activity;
- unread/completed badge;
- branch/worktree;
- pin.

VS Code Agents показывает полезность единого sessions list, группировки по workspace/time/custom groups и быстрого переключения между параллельными sessions. [[S11]]

### 15.5. Session list actions

Primary:

- Open;
- New Session.

Context menu:

- Rename;
- Pin;
- Mark Done;
- Mark Unread;
- Open to Side;
- Open in New Window;
- Fork;
- Duplicate Settings;
- Continue in Another Provider;
- Move to Group;
- Export;
- Archive;
- Delete Session Presentation;
- Request Underlying Data Deletion.

### 15.6. Session grouping

- By Project section;
- Active / Waiting / Done;
- Today / Previous 7 Days / Older;
- By Agent;
- Custom.

User can drag session between custom groups. Runtime state remains unchanged.

### 15.7. New Session dialog

Minimum fields:

- optional title;
- agent/runtime;
- model;
- execution profile;
- workspace/worktree choice;
- first prompt.

Advanced:

- capability set override;
- context selection;
- local/cloud;
- budget;
- permissions request;
- start in background;
- isolated branch/worktree;
- use existing Run.

Default fields inherit project configuration and remain collapsed.

---

## 16. Project Chat

### 16.1. Chat Header

- session title;
- agent type;
- model/provider;
- execution profile;
- branch/worktree;
- state;
- Voice;
- Context Inspector;
- Changes;
- More.

Clicking model opens session-specific picker, but user-visible direct model is never switched silently.

### 16.2. Header state actions

If idle:

- Start Background Work;
- Run Tests;
- Review Changes.

If running:

- Steer;
- Queue;
- Pause, если supported;
- Stop;
- Open Progress.

If waiting:

- Answer;
- Open Inbox Card;
- Let Orchestrator Decide;
- Cancel.

If failed:

- Retry;
- Change Provider;
- Open Error;
- Continue Manually;
- Fork from Checkpoint.

### 16.3. Composer anatomy

- drag/drop target;
- multiline text;
- Add Context;
- Mention;
- Slash Commands;
- Current Scope chips;
- Model/Profile controls;
- Voice input;
- Send;
- Background Send;
- Queue indicator.

### 16.4. Add Context menu

- Current File/Selection;
- Files;
- Folder;
- Image/Screenshot;
- Artifact;
- Memory Item;
- Diff/Commit;
- Terminal Output;
- Test Result;
- URL;
- Connector Item;
- Project;
- Session;
- Clipboard;
- Capture Screen Region.

### 16.5. Context chips

Chip показывает:

- type;
- short label;
- source/project;
- scope;
- remove;
- inspect.

Large object attached by handle, not duplicated into composer.

### 16.6. Slash commands

Baseline:

- `/help`;
- `/plan`;
- `/review`;
- `/test`;
- `/explain`;
- `/fix`;
- `/research` as skill/prompt, not separate subsystem;
- `/memory`;
- `/context`;
- `/model`;
- `/profile`;
- `/background`;
- `/schedule`;
- `/voice`;
- `/clear-temporary-context`;
- `/checkpoint`;
- `/stop`.

Provider/skill-specific commands appear dynamically and show origin.

### 16.7. During generation

Composer offers three explicit semantics:

- **Queue** — send after current logical step completes;
- **Steer** — update current direction without discarding valid work;
- **Stop and Send** — cancel current generation/run where possible and begin new request.

Queued messages are visible above composer and can be reordered, edited or removed.

### 16.8. Progress presentation

Default compact block:

```text
Working: reviewing authentication flow
Last progress: read 4 relevant files
Next: patch and tests
```

Expand shows:

- significant actions;
- active tool;
- files touched;
- elapsed time;
- budget;
- current node/device;
- stop reason;
- raw logs.

### 16.9. Thinking display

Desktop does not expose hidden chain-of-thought. It can show:

- plan;
- current phase;
- evidence being inspected;
- decisions;
- uncertainty;
- status.

`Thinking…` without informative progress must not remain indefinitely. If no meaningful progress is observed, UI changes to `No recent progress` and offers diagnostics/stop.

### 16.10. Message edit and branch

User can edit previous request:

- `Edit and Continue` creates a branch/checkpoint;
- original history remains available;
- changes after branch are not silently destroyed;
- affected external effects are clearly marked as already occurred and cannot be undone by editing text.

### 16.11. Completion card

At logical completion:

- outcome;
- summary;
- files/artifacts;
- tests/evidence;
- unresolved items;
- next actions;
- costs/usage, collapsed;
- `Review Changes`;
- `Open Result`;
- `Continue`;
- `Save Artifact`;
- `Create Follow-up`;
- `Mark Done`.

### 16.12. Inaccurate completion

`Report Not Done` action:

- asks for short correction;
- reopens Task/session if appropriate;
- preserves previous completion claim;
- marks feedback for evaluation;
- offers Continue in same session or Fork.

---

## 17. Files and Project Explorer

### 17.1. File tree

Supports:

- create;
- rename;
- move;
- delete;
- duplicate;
- drag/drop;
- multi-select;
- filter;
- fuzzy search;
- reveal hidden files;
- reveal in OS;
- open terminal here;
- copy path;
- attach to chat;
- send to agent;
- compare selected;
- show history;
- open with external app.

### 17.2. Safe file operations

Delete behavior:

- reversible trash when available;
- Git-tracked files show diff/recovery path;
- permanent delete requires explicit choice;
- deleting outside project follows Trust policy.

### 17.3. File preview/editor

Dennett may include text/Markdown/code preview and lightweight editing, but is not required to replace a full IDE.

Header actions:

- Save;
- Open in External Editor;
- Open to Side;
- Compare;
- History;
- Ask Agent;
- Add to Context;
- Copy Link;
- More.

### 17.4. File timeline

Shows:

- Git commits;
- local saves/checkpoints;
- agent edits;
- imported changes;
- conflict resolutions.

Actions:

- Compare;
- Restore;
- Open Commit;
- Copy Reference;
- Ask Agent About Change.

### 17.5. Large/binary file state

UI shows appropriate viewer or:

- metadata;
- preview availability;
- open externally;
- download/sync state;
- attach as artifact;
- no false text rendering.

---

## 18. Changes, Review and Checkpoints

### 18.1. Changes workspace

Modes:

- Working Changes;
- Session Changes;
- Run Changes;
- Branch Changes;
- Commit;
- Compare.

### 18.2. Changes list

Each file:

- added/modified/deleted/renamed;
- lines +/-;
- conflict;
- author/source session;
- test relation;
- review state.

### 18.3. Diff toolbar

- Previous Change;
- Next Change;
- Side-by-Side/Inline;
- Ignore Whitespace;
- Collapse Unchanged;
- Comment;
- Ask Agent;
- Revert Hunk;
- Revert File;
- Stage/Unstage, если Git;
- Open File;
- Open External Editor;
- Copy Patch.

### 18.4. Review comments

Comment can be:

- local note;
- sent to active agent;
- queued as follow-up;
- converted to Task;
- marked resolved.

Comment includes exact file/hunk version. If file changed later, UI shows outdated context.

### 18.5. Review summary

- files reviewed / remaining;
- unresolved comments;
- tests;
- blocking issues;
- agent response;
- `Approve for Integration`;
- `Request Changes`;
- `Continue Reviewing`.

Approval here is project review state, not Trust permission.

### 18.6. Integration actions

Depending on project:

- Apply to Working Tree;
- Merge Worktree;
- Create Commit;
- Push Feature Branch;
- Create Pull Request;
- Export Patch;
- Keep Isolated;
- Discard Changes.

Each action shows exact branch/target.

Codex and VS Code Agents demonstrate the usefulness of project-organized threads, isolated worktrees, side-by-side diff review and validation before integration. [[S11]] [[S16]]

### 18.7. Checkpoints

Checkpoint entry: [[S14]]

- created time;
- user request;
- state summary;
- files/artifacts;
- external effects note;
- provider session handle.

Actions:

- Compare to Current;
- Restore Working State;
- Fork From Here;
- Rename;
- Pin;
- Delete Checkpoint Metadata.

Restore cannot undo already-completed external effects and must say so explicitly.

---

## 19. Runs and Background Work

### 19.1. Runs list

Filters:

- Active;
- Waiting;
- Needs User;
- Completed;
- Failed;
- Cancelled;
- Scheduled;
- By Agent;
- By Device;
- By Provider.

### 19.2. Run row

- title;
- status;
- project;
- owner agent;
- current phase;
- last progress;
- elapsed;
- budget;
- attention/risk;
- primary action.

### 19.3. Run details

Sections:

- Summary;
- Timeline;
- Artifacts;
- Changes;
- Evidence;
- Resources;
- Context;
- Permissions;
- Logs;
- Related.

### 19.4. Run controls

- Open Session;
- Steer;
- Add Instruction;
- Pause;
- Resume;
- Cancel;
- Stop After Current Step;
- Change Budget;
- Move to Device;
- Change Provider, if continuation safe;
- Retry Failed Step;
- Retry as New Run;
- Fork;
- Create Checkpoint;
- Mark Priority;
- Open Logs;
- Export Result.

### 19.5. Unknown state

If external effect or provider status unknown:

- state explicitly `Unknown — reconciling`;
- `Do Not Retry` default;
- actions `Check Provider`, `Reconcile`, `Open Receipt`, `Resolve Manually`;
- no generic Retry until state is known or user explicitly accepts risk.

### 19.6. Stalled state

Shows:

- last meaningful progress;
- suspected reason;
- provider/tool/device status;
- automatic recovery attempts;
- `Wait`, `Steer`, `Change Provider`, `Stop`, `Open Diagnostics`.

### 19.7. Compare Runs

Select two or more Runs:

- outcome;
- artifacts;
- changes;
- tests;
- sources;
- cost;
- latency;
- model/provider;
- user feedback.

Actions:

- Set Preferred Result;
- Merge Useful Parts through agent;
- Save Comparison;
- Promote Procedure Candidate.

---

## 20. Tasks and Optional Kanban

### 20.1. Principle

Tasks view is optional. A project with one interactive chat must not look incomplete without Kanban.

### 20.2. Views

- List;
- Board;
- Timeline;
- Calendar, if dates exist;
- Hidden/Disabled.

### 20.3. Task creation

Fields:

- title;
- description;
- project;
- status;
- priority;
- owner user/agent;
- due date;
- dependency;
- related session/run;
- artifact;
- event;
- budget;
- visibility.

Only title is mandatory for manual quick add.

### 20.4. Task actions

- Open;
- Edit;
- Start with Agent;
- Continue in Chat;
- Run in Background;
- Assign/Reassign;
- Add Dependency;
- Add to Event;
- Convert to Project;
- Duplicate;
- Mark Done;
- Cancel;
- Archive;
- Delete.

### 20.5. Board interactions

Drag updates task state through Agentic Control, not only UI. If transition invalid, card returns and explains why.

### 20.6. Auto-created tasks

Auto-created task displays origin:

- user request;
- agent suggestion;
- event;
- conversation extraction;
- imported system.

User can merge duplicate tasks or mark auto-creation undesirable.


# Часть IV. Пространства решений, наблюдения и библиотеки

## 21. Action Inbox

### 21.1. Роль

Action Inbox — не список уведомлений и не backlog. Это долговечная очередь ситуаций, где Dennett действительно не должен или не может безопасно продолжить без решения пользователя.

Default layout — split view:

- Context Sidebar: card list;
- Main Workspace: выбранная card;
- Inspector: sources, related objects, permission/context details.

Linear Inbox подтверждает полезность list/detail, keyboard navigation, snooze и локальных быстрых действий без ухода с текущего экрана. [[S17]]

### 21.2. Inbox Header

- `Action Inbox`;
- unresolved count;
- Resolve Next;
- Search;
- Filter;
- Sort;
- Saved Views;
- DND/attention settings;
- overflow.

### 21.3. Default filters

- All Open;
- Urgent;
- Permissions;
- Choices;
- Conflicts;
- Failed/Stalled;
- Communication;
- Project Review;
- Snoozed;
- Resolved History.

### 21.4. Filter dimensions

- project;
- type;
- risk;
- source agent/system;
- due/expiry;
- can delegate;
- device;
- offline availability;
- created time;
- confidence;
- unread.

### 21.5. Sort

- Recommended;
- Urgency;
- Expiry;
- Newest;
- Oldest;
- Project;
- Risk;
- Manual.

`Recommended` учитывает urgency, cost of delay, current user focus и card staleness, но пользователь может увидеть объяснение ranking.

### 21.6. Inbox row

Показывает:

- type icon;
- concise question/action;
- project;
- requester;
- urgency/expiry;
- risk;
- recommended option, если она существует;
- unread;
- state.

Hover actions:

- Apply Recommended;
- Snooze;
- Open;
- More.

### 21.7. Card anatomy

В Main Workspace:

1. **Question or Decision**;
2. **Why now**;
3. **Context**;
4. **Options**;
5. **Recommendation**;
6. **Consequences**;
7. **Sources / freshness**;
8. **Related process**;
9. **Response controls**.

### 21.8. Standard actions

- Choose option;
- Edit parameters;
- Write custom response;
- Discuss with Orchestrator;
- Discuss by Voice;
- Let Orchestrator Decide;
- Prepare Only;
- Snooze;
- Pause Related Process;
- Stop Related Process;
- Open Context;
- Open Source;
- Mark Not Relevant;
- Close if no longer needed.

### 21.9. Permission card actions

По Trust Fabric:

- Allow Once;
- Allow for This Run;
- Allow Bounded Pattern;
- Allow Until Time;
- Prepare Only;
- Edit Scope/Parameters;
- Deny;
- Deny and Remember;
- Ask Orchestrator;
- Pause.

Card обязана показывать:

- actor;
- exact resource;
- operation;
- external recipient;
- amount/limit, если есть;
- expiry;
- why current grant insufficient;
- safer alternative.

Нельзя показывать бессодержательное `Разрешить продолжить?`.

### 21.10. Choice card

Для дизайна/вариантов:

- visual previews;
- concise differences;
- pros/cons;
- agent/reviewer recommendation;
- `Choose`;
- `Combine`;
- `Ask for Another Option`;
- `Open Fullscreen Compare`;
- `Discuss`;
- `Defer`.

### 21.11. Communication card

- recipient;
- channel/account;
- thread context;
- draft;
- attachments;
- disclosure summary;
- tone;
- actions `Send`, `Edit`, `Reply Later`, `Do Not Send`, `Authorize Pattern`.

### 21.12. Conflict card

- conflicting objects;
- source/device/version;
- common base;
- automatic merge proposal;
- affected projections;
- actions `Use A`, `Use B`, `Merge`, `Keep Both`, `Ask Agent`, `Open Diff`, `Resolve Later`.

### 21.13. Stale card

Если underlying state изменилось:

- prominent `Situation changed`;
- old options disabled;
- `Refresh`;
- `Open Updated State`;
- `Close as Superseded`.

No stale approval can be submitted.

### 21.14. Snooze

Presets:

- 15 minutes;
- 1 hour;
- Tonight;
- Tomorrow;
- Next Workday;
- Custom;
- Until Related Event;
- Until I Open Project.

Snoozed card disappears from default view and returns at due condition. This follows a proven attention-management pattern in Linear Inbox. [[S17]]

### 21.15. Batch operations

Допустимы только для homogeneous low-risk cards:

- mark seen;
- snooze;
- close superseded;
- allow same bounded project action, if exact scope visible;
- deny same pattern.

Нельзя batch-confirm:

- heterogeneous permissions;
- multiple recipients;
- purchases;
- deletion plus sending;
- cards with different effects hidden under one label.

### 21.16. Keyboard operation

- Up/Down — navigate;
- Enter — open/primary action depending setting;
- E — expand details;
- S — snooze;
- V — voice discuss;
- R — resolve/recommended;
- X — close if allowed;
- Shift+Up/Down — multi-select;
- Esc — return to list.

Shortcuts shown in tooltips and Command Center; letters are configurable and disabled while typing.

### 21.17. Inbox states

- Open;
- Unread;
- Viewed;
- Snoozed;
- In Discussion;
- Answer Submitted;
- Resolving;
- Resolved;
- Expired;
- Superseded;
- Withdrawn;
- Failed to Submit;
- Offline Pending.

### 21.18. Empty state

`No decisions waiting for you.`

Optional secondary actions:

- View Recently Resolved;
- Configure Attention Policy;
- Open Radar.

No confetti or invented work.

---

## 22. Agent Radar

### 22.1. Роль

Radar отвечает:

> «Что сейчас происходит в Dennett и где может понадобиться моё вмешательство?»

Он не является raw process monitor. Он показывает значимые Tasks, Runs, Sessions, waits, blockers, devices and events.

### 22.2. Default views

- Overview;
- List;
- Timeline;
- Graph;
- Office;
- History.

Overview является default. Office — optional presentation над тем же state.

### 22.3. Radar Header

- `Agent Radar`;
- active count;
- attention count;
- Search;
- Filter;
- Group;
- View Mode;
- Saved View;
- Pause Background;
- Emergency Stop;
- overflow.

`Emergency Stop` visually separated and requires specific confirmation consistent with Trust Fabric.

### 22.4. Overview lanes

- Needs You;
- Running;
- Waiting;
- Stalled/Failed;
- Recently Completed.

### 22.5. Radar item

- title;
- project;
- agent/runtime;
- state;
- current phase;
- last meaningful progress;
- device/provider;
- elapsed time;
- risk/attention;
- budget warning;
- freshness;
- primary action.

### 22.6. Filters

- project;
- agent;
- task/run/session;
- state;
- device;
- provider;
- risk;
- foreground/background;
- external effect;
- scheduled/event-triggered;
- changed files;
- cost threshold;
- stale.

### 22.7. Grouping

- Project;
- State;
- Agent;
- Device;
- Provider;
- Priority;
- None.

### 22.8. Run/Agent detail

Main Workspace detail includes:

- current purpose;
- state and freshness;
- progress summary;
- current activity;
- dependencies;
- related sessions;
- changed resources;
- artifacts;
- permissions;
- budget;
- event source;
- timeline;
- recovery.

### 22.9. Controls

- Open Session;
- Open Project;
- Steer;
- Add Instruction;
- Pause;
- Resume;
- Stop;
- Stop After Current Step;
- Change Priority;
- Change Budget;
- Move Execution;
- Ask for Status;
- Open Logs;
- Open Context;
- Open Permission;
- Retry;
- Reconcile;
- Archive Completed.

### 22.10. Stalled diagnosis

Shows separately:

- `No meaningful progress`;
- `Waiting for tool`;
- `Provider unavailable`;
- `Permission needed`;
- `Device offline`;
- `Dependency incomplete`;
- `Potential loop`;
- `Unknown`.

User sees recommended recovery, not only status code.

### 22.11. Timeline

Displays significant transitions:

- created;
- started;
- checkpoint;
- user steer;
- permission request;
- provider switch;
- artifact;
- completion;
- failure;
- retry;
- cancellation.

Zoom levels:

- minutes;
- hours;
- days.

### 22.12. Graph view

Graph nodes:

- Task;
- Run;
- Agent;
- Event;
- Artifact;
- User decision.

Edges:

- spawned;
- depends on;
- produced;
- waiting for;
- superseded;
- reviewed by.

Graph never becomes required workflow editor by default. It is navigation and diagnosis.

### 22.13. Office view

Office maps:

- project → room/building;
- agent/run → occupant/activity;
- waiting → waiting area;
- review → review room;
- restricted boundary → visual barrier;
- communication → temporary link.

Actions are the same canonical commands as List/Graph. Office cannot create hidden state.

### 22.14. Freshness

Radar item may be:

- Live;
- Delayed;
- Stale;
- Device Offline;
- Unknown.

Stale item cannot show animated activity as if it were current.

### 22.15. Empty state

`No significant active work.`

Actions:

- Open Projects;
- Start Quick Task;
- View Recent History;
- Configure Saved View.

---

## 23. Library Shell

### 23.1. Purpose

Library is a container for reusable and accumulated system assets:

- Memory;
- Artifacts;
- Capabilities;
- Automations and Events.

It has a local navigation list and global search limited to selected category by default.

### 23.2. Library Home

Shows:

- recently opened;
- pinned;
- recently created;
- needs review;
- updates;
- storage summary only if relevant.

Actions:

- Open Memory;
- Open Artifacts;
- Open Capabilities;
- Open Automations;
- Search Library;
- Pin Category to Activity Rail.

---

## 24. Memory Workspace

### 24.1. Core layout

- Sidebar: spaces, collections, saved queries, review queues;
- Main: search/list/note/timeline/graph;
- Inspector: source, provenance, influence, access, history.

### 24.2. Default Sidebar

- Search;
- Current;
- Timeline;
- Project Memories;
- People;
- World Intelligence;
- Captures;
- Pinned;
- Needs Review;
- Conflicts;
- Recently Used;
- Portable Packs;
- Trash/Deletion Status, permission-dependent.

These are views and queries, not necessarily separate physical databases.

### 24.3. Memory Header actions

- New Note;
- Capture;
- Ask Memory;
- Search;
- Filter;
- View Mode;
- Review;
- Import;
- Export;
- More.

### 24.4. Search modes

- Natural language;
- Exact text;
- Current state;
- Historical/as-of;
- Evidence only;
- Within project/person/source;
- Visual/audio;
- Advanced query.

Results show:

- type;
- snippet;
- source;
- time;
- current/historical status;
- confidence;
- project/scope;
- freshness;
- why matched.

### 24.5. Memory result actions

- Open;
- Open Evidence;
- Add to Context;
- Pin;
- Link to Project;
- Correct;
- Create Current Note;
- Compare History;
- Hide from Suggested Context;
- Change Scope;
- Share/Export;
- Forget/Delete;
- Copy Stable Link.

### 24.6. Note editor

Supports:

- Markdown/text;
- links/mentions;
- attachments;
- source citations;
- tags/facets;
- project/scope;
- sensitivity;
- current/historical marker;
- human-authored lock/ownership;
- version history.

Header buttons:

- Save;
- Save and Pin;
- Add to Current Context;
- Link;
- View Source;
- History;
- Share/Export;
- More.

### 24.7. Evidence viewer

Shows:

- original object;
- exact excerpt/anchor;
- observed/ingested time;
- device/source;
- transformations;
- redactions;
- linked claims;
- retention policy.

Actions:

- Open Original;
- Reveal Location;
- Compare Transcript;
- Correct Metadata;
- Redact;
- Delete Raw;
- Preserve Summary Only;
- Export Evidence.

### 24.8. Current-state view

For a preference/project fact:

- current statement;
- supporting evidence;
- conflicting evidence;
- effective time;
- freshness;
- resolver;
- history link.

Actions:

- Confirm;
- Correct;
- Mark Unknown;
- View History;
- Add Counterexample;
- Prevent Use in Actions;
- Request Re-evaluation.

### 24.9. Timeline

Filters by:

- date;
- project;
- device;
- person;
- event type;
- modality;
- importance;
- saved/not saved;
- deleted/raw unavailable.

Zoom and navigation preserve location. Timeline should virtualize large histories.

### 24.10. Graph view

Optional. Shows typed relationships, but default memory use does not require graph manipulation.

Actions:

- Expand neighbors;
- Follow relation;
- Pin node;
- Open evidence;
- Create link;
- Remove relation;
- Filter types;
- Return to list.

### 24.11. Needs Review

Contains:

- low-confidence conclusions;
- conflicts;
- auto-generated notes pending promotion;
- deletion obligations;
- imported memory trust questions;
- suggested merges.

Actions vary:

- Accept;
- Edit;
- Reject;
- Keep as Observation;
- Defer;
- Ask Orchestrator;
- Open Sources.

### 24.12. Forget/Delete flow

UI first asks desired outcome:

- Stop using in future context;
- Remove current projection;
- Delete raw source;
- Delete all derived copies;
- Remove from project export;
- Full forget request under policy.

Then shows:

- replicas/indexes/backups affected;
- what can be immediate;
- what awaits backup expiry;
- retained content-free audit fact;
- recovery window if allowed.

Buttons:

- Preview Impact;
- Request Forget;
- Cancel;
- Track Deletion.

### 24.13. Ask Memory

A local query composer produces answer with:

- evidence;
- uncertainty;
- historical/current distinction;
- Add Results to Project;
- Create Note;
- Open Sources.

It does not implicitly start a separate research subsystem.

### 24.14. Memory states

- Available;
- Indexing;
- Partially Indexed;
- Stale Projection;
- Conflict;
- Restricted;
- Source Missing;
- Raw Deleted;
- Deletion Pending;
- Local Only;
- Syncing;
- Imported Untrusted;
- Rebuilding.

---

## 25. Artifacts Workspace

### 25.1. Definition in UI

Artifact is a durable output worth opening, comparing, exporting or reusing. [[S15]] Ordinary chat text is not automatically an artifact.

### 25.2. Views

- Recent;
- By Project;
- Pinned;
- Drafts;
- Final;
- Images;
- Documents;
- Code/Patches;
- Research;
- Audio/Video;
- Generated Views;
- Archived.

### 25.3. Artifact card

- preview;
- title;
- type;
- project;
- source session/run;
- version;
- draft/final;
- created/updated;
- sync/local availability;
- primary Open;
- overflow.

### 25.4. Artifact actions

- Open;
- Open to Side;
- Preview Fullscreen;
- Edit;
- Open External App;
- Compare Versions;
- Set Final;
- Pin;
- Attach to Chat;
- Add to Project;
- Create New Project;
- Create Task;
- Export;
- Publish/Share through connector;
- Reveal File;
- Duplicate;
- Fork;
- Archive;
- Delete.

### 25.5. Artifact viewer

Header:

- breadcrumb;
- version picker;
- status;
- Edit/Open externally;
- Compare;
- Attach;
- Export;
- More.

Side Inspector:

- origin;
- evidence;
- versions;
- comments;
- related artifacts;
- usage;
- permissions;
- portability.

### 25.6. Version compare

- visual/text/diff according to type;
- select left/right version;
- change summary;
- source run/model;
- choose preferred;
- restore/fork;
- ask agent to merge.

### 25.7. Dynamic viewers

Unsupported type state:

- metadata;
- download/open externally;
- install viewer capability;
- convert through agent/tool;
- do not render unsafe active content automatically.

### 25.8. Artifact states

- Draft;
- Generating;
- Ready;
- Final;
- Superseded;
- Failed;
- Partial;
- Local Only;
- Uploading;
- Missing Source;
- Restricted;
- Archived;
- Deleted Pending Sync.

---

## 26. Capabilities Workspace

### 26.1. Principle

Capabilities UI exposes lifecycle defined by Capability Fabric. It does not reduce all integrations to one generic list.

### 26.2. Sidebar categories

- Overview;
- Providers and Connections;
- Models;
- Agent Runtimes;
- Skills;
- MCP;
- Plugins and Extensions;
- Connectors;
- Computer Use;
- Speech and Media;
- Local Runtimes;
- Project Sets;
- Candidates;
- Updates;
- Health;
- Removed/History.

### 26.3. Overview

Shows:

- active defaults;
- unhealthy connections;
- expiring auth;
- available updates;
- project missing capabilities;
- recently added;
- candidates worth review;
- local hardware state.

Healthy categories stay collapsed.

### 26.4. Global actions

- Add;
- Discover;
- Import;
- Connect Provider;
- Add Local Model;
- Refresh Registry;
- Run Health Checks;
- Update Selected;
- Manage Project Set;
- Open Trust Settings;
- Export Configuration.

### 26.5. Add flow

Chooser:

- Provider/API;
- Subscription Runtime;
- Local Model;
- Skill;
- MCP Server;
- Plugin/Extension;
- Connector;
- Computer-use Backend;
- Speech/Media Backend;
- Custom Tool/CLI;
- Import Package/Folder/URL.

Manual user add proceeds immediately to collection/registration according to Capability Fabric; utility evaluation is optional. Technical safety requirements remain visible.

### 26.6. Capability list item

- name/icon;
- category;
- source/origin;
- version;
- ownership;
- installed/connected;
- active/on-demand/disabled;
- trust;
- health;
- update;
- project bindings;
- measured utility, if known.

### 26.7. Capability detail

Tabs:

- Overview;
- Configuration;
- Components;
- Projects;
- Evaluation;
- Security;
- Versions;
- Logs.

### 26.8. Common capability actions

- Install/Connect;
- Configure;
- Authenticate;
- Probe/Test;
- Enable;
- Disable;
- Set On-Demand;
- Attach to Project;
- Use for One Run;
- Set Preferred;
- Set Fallback;
- Compare;
- Fork;
- Update;
- Rollback;
- Reauthenticate;
- Open Source;
- Export;
- Remove;
- Revoke Trust.

### 26.9. Skills screen

Filters:

- Global Verified;
- User-Owned;
- Project-Local;
- Candidates;
- Provider-Native;
- Quarantined;
- Deprecated;
- Disabled.

Skill actions:

- Open SKILL.md;
- Edit;
- Inspect Scripts/Assets;
- Try in Project;
- Attach;
- Make On-Demand;
- Compare;
- View Delta Proposal;
- Apply Suggested Patch;
- Fork;
- Promote Global;
- Return to Project Scope;
- Update;
- Rollback;
- Disable;
- Remove.

### 26.10. Candidate review

Shows:

- how discovered;
- intended purpose;
- closest existing capabilities;
- duplicate/relation assessment;
- unique delta;
- required tools/scripts;
- security findings;
- expected utility;
- suggested disposition.

Actions:

- Ignore;
- Save Link Only;
- Quarantine;
- Add Candidate;
- Try Project-Local;
- Add Global;
- Extract Useful Delta;
- Compare Manually;
- Never Suggest Source.

### 26.11. MCP screen

Server detail:

- transport;
- package/endpoint;
- publisher/source;
- tools/resources/prompts/apps;
- auth;
- scopes;
- health;
- version/schema changes;
- project bindings.

Actions:

- Connect;
- List Components;
- Enable/Disable Individual Component, if supported;
- Test Tool;
- Open Resources;
- Reauthenticate;
- Attach Project;
- Restrict Scope;
- Compare Native Connector;
- Update;
- Disconnect;
- Remove.

### 26.12. Provider Connections

Connection card:

- provider;
- surface: API/subscription/cloud/local;
- account/project/region;
- auth;
- health;
- models;
- native features;
- usage/limits;
- privacy/locality;
- default roles.

Actions:

- Connect/Reconnect;
- Test;
- Manage Models;
- Set Defaults;
- View Usage;
- Provider Settings;
- Open Native App;
- Disable;
- Remove.

### 26.13. Models

Model card:

- provider/endpoint;
- model ID/alias;
- modalities;
- reasoning controls;
- context observed;
- tool support;
- latency/quality history;
- usage/cost;
- local resource fit;
- health/freshness.

Actions:

- Test Prompt;
- Benchmark for Task Class;
- Set Direct Chat Default;
- Set Internal Role;
- Add Fallback;
- Pin Version;
- Configure Native Settings;
- Disable;
- Remove Local Artifact.

### 26.14. Local model manager

Shows:

- artifact;
- revision/hash;
- quantization;
- runtime;
- hardware fit;
- loaded/warm state;
- RAM/VRAM;
- throughput/latency;
- license;
- custom code requirement.

Actions:

- Download;
- Import File;
- Verify Hash;
- Load;
- Unload;
- Convert/Quantize through capability;
- Benchmark;
- Move Storage;
- Delete Artifact;
- Open Model Card;
- Trust Custom Code separately.

### 26.15. Project Capability Set editor

Sections:

- Required;
- Recommended;
- On-Demand;
- Fallback;
- Experimental;
- Forbidden/Incompatible.

Actions:

- Add;
- Remove;
- Change category;
- Set preferred/fallback;
- Test set;
- Explain recommendation;
- Restore project defaults;
- Export with portable project memory.

### 26.16. Capability states

Orthogonal badges:

- Discovery: discovered/inspected;
- Possession: not acquired/installed/connected;
- Trust: unreviewed/restricted/trusted/revoked;
- Utility: unknown/candidate/verified/degraded;
- Activation: active/on-demand/disabled;
- Ownership: user/Dennett/provider/project;
- Health: healthy/slow/quota-limited/offline/incompatible.

UI must not collapse these into one ambiguous green/red status.

---

## 27. Automations and Events Workspace

### 27.1. Scope

Это UI для schedules, event subscriptions, prospective intents, monitors и optional structured automations. Обычный prompt/skill не обязан становиться Automation.

### 27.2. Sidebar

- Active;
- Needs Attention;
- Scheduled;
- Event-based;
- Monitors;
- Paused;
- History;
- Templates;
- System Maintenance;
- Project groups.

### 27.3. Header actions

- New;
- Describe in Natural Language;
- From Template;
- Import;
- Run Now;
- Pause Background;
- Search;
- Filter;
- More.

### 27.4. Create Automation

Default natural-language sheet:

- what should happen;
- when/under what condition;
- project/scope;
- output/delivery;
- active immediately or preview.

Dennett compiles a readable summary, not necessarily a rigid workflow graph.

Advanced editor exposes:

- trigger;
- conditions;
- action/task/skill;
- capabilities;
- permissions;
- schedule/time zone;
- cooldown;
- expiry;
- deduplication;
- budget;
- notification;
- failure policy;
- test mode.

### 27.5. Automation card

- name;
- plain-language purpose;
- trigger/schedule;
- next check/run;
- project;
- state;
- last outcome;
- attention;
- primary Enable/Run/Open.

### 27.6. Actions

- Enable;
- Disable;
- Run Now;
- Test;
- Preview Next Run;
- Edit;
- Duplicate;
- Change Schedule;
- Snooze;
- View History;
- View Created Tasks;
- Open Skill/Procedure;
- Export;
- Archive;
- Delete.

### 27.7. Why It Fired

Every event-triggered item offers:

- source event;
- matched conditions;
- semantic assessment, if used;
- cooldown/dedup state;
- action selected;
- permission decision;
- outcome.

### 27.8. Templates

Examples:

- daily project summary;
- CI failure digest;
- news monitor;
- backup verification;
- provider quota warning;
- weekly memory review;
- remind after agent completion;
- monitor folder/repository change.

Template is a starting point and never receives credentials or permission automatically.

### 27.9. Structured view

For genuinely multi-stage durable automation, optional views:

- Steps;
- Graph;
- Runs;
- Versions.

They appear only when automation actually has such structure.

### 27.10. Event states

- Draft;
- Active;
- Paused;
- Snoozed;
- Running;
- Waiting;
- Triggered;
- Cooldown;
- Expired;
- Failed;
- Superseded;
- Disabled by Policy;
- Missing Capability;
- Offline Deferred.

---

## 28. Notifications Center

### 28.1. Difference from Inbox

Notification is transient awareness. Inbox is unresolved decision.

A notification may link to Inbox card but does not duplicate its state.

### 28.2. Categories

- Completed;
- Failed;
- Mention/Message;
- System;
- Security;
- Update;
- Usage;
- Background;
- Informational.

### 28.3. Notification actions

- Open;
- Mark Read;
- Mark All Read;
- Mute Source;
- DND;
- Send to Inbox, if user wants to act later;
- Clear;
- Notification Settings.

### 28.4. Toast policy

Toast only for:

- user waiting result;
- consequential failure;
- permission/decision urgency;
- explicit requested notification;
- critical system/security.

Background success can use badge/center only.

### 28.5. DND

Modes:

- Off;
- 1 hour;
- Until tomorrow;
- While Focus Mode;
- While presenting;
- Schedule;
- Critical only.

Notifications remain in center.

---

## 29. Voice Overlay

### 29.1. Modes

- Compact Orb/Bar;
- Expanded Transcript;
- Docked Panel;
- Floating Always-on-Top;
- Full Voice Workspace.

### 29.2. Compact controls

- listening/speaking indicator;
- mute microphone;
- mute output;
- stop speaking;
- end session;
- expand;
- current target: Orchestrator/Project;
- transfer device.

### 29.3. Expanded controls

- live transcript;
- heard-output transcript;
- current project/session;
- voice mode/profile;
- backend/provider;
- background contributor status;
- pending action;
- context chips;
- privacy state.

Buttons:

- Interrupt;
- Push to Talk;
- Pause Listening;
- Resume;
- Switch Project;
- Send to Orchestrator;
- Show Answer as Text;
- Open Result;
- Cancel Background Thought;
- Save Note;
- Do Not Save This;
- End.

### 29.4. Voice confirmation

High-risk confirmation is shown as trusted structured card. Voice can discuss it, but final action follows Trust policy.

### 29.5. Multiple devices

Overlay shows `Voice active on Phone` with actions:

- Take Over Here;
- Open Transcript;
- End Remote Session;
- Keep Remote.

No simultaneous hidden playback.

---

## 30. Quick Capture Overlay

### 30.1. Invocation

Global shortcut, tray, Activity Rail or screenshot action.

### 30.2. Capture types

- Text Note;
- Voice Note;
- Screenshot Full Screen;
- Current Window;
- Region;
- Clipboard;
- File;
- Photo from connected device;
- Link;
- Selection from another app.

### 30.3. Minimal capture sheet

- preview/content;
- optional comment;
- project picker;
- destination: Memory / Project / Inbox / New Task / Artifact;
- privacy/retention;
- Save.

### 30.4. Smart routing

Dennett may suggest destination, but user can override. Saving must not wait for deep analysis; semantic processing can occur later.

### 30.5. Actions

- Save;
- Save and Ask;
- Attach to Current Chat;
- Create Project;
- Create Task;
- Copy;
- Retake;
- Cancel;
- Do Not Retain Raw.

---

## 31. System Status, Devices, Sync, Backup and Usage

### 31.1. System Status Overview

Sections:

- Head Runtime;
- Devices;
- Sync;
- Providers;
- Storage;
- Backup;
- Background Work;
- Security;
- Updates.

Healthy section collapsed. Problem section expanded with impact and recovery.

### 31.2. Global actions

- Refresh;
- Run Diagnostics;
- Enter Safe Mode;
- Restart Runtime;
- Pause Background Work;
- Emergency Stop;
- Export Diagnostics;
- Check Updates.

Restart action explains affected sessions and saves checkpoints where possible.

### 31.3. Devices screen

Device row:

- name/type;
- trust;
- online/freshness;
- roles;
- capabilities;
- storage;
- sync;
- voice/ambient;
- active work.

Actions:

- Open;
- Rename;
- Pair New Device;
- Trust/Restrict;
- Set Roles;
- Send Task;
- Transfer Voice;
- Sync Now;
- Wake/Connect, if supported;
- Make Head Candidate;
- Planned Handoff;
- Revoke;
- Remove.

### 31.4. Device detail

Tabs:

- Overview;
- Capabilities;
- Projects;
- Sync;
- Voice/Sensors;
- Security;
- Logs.

### 31.5. Sync screen

Shows by data class:

- memory events;
- project metadata;
- files;
- artifacts;
- media;
- settings;
- deletion operations.

States:

- Synced;
- Syncing;
- Pending Upload;
- Pending Download;
- Conflict;
- Stale;
- Paused;
- Local Only;
- Error;
- Device Offline.

Actions:

- Sync Now;
- Pause;
- Resume;
- Resolve Conflict;
- Retry;
- View Queue;
- Clear Rebuildable Cache;
- Open Data Location;
- Change Policy.

### 31.6. Conflicts screen

Groups conflicts by:

- notes;
- settings;
- files;
- project pack;
- unknown duplicate action.

Actions:

- Auto Merge;
- Review Diff;
- Use Local;
- Use Head;
- Keep Both;
- Ask Agent;
- Defer.

Permissions and external effects never use generic last-write-wins conflict UI.

### 31.7. Backup screen

Shows:

- last successful backup;
- backup location;
- covered data classes;
- encryption/key state;
- last verification;
- last restore test;
- pending retention/deletion.

Actions:

- Backup Now;
- Verify;
- Test Restore;
- Browse Snapshots;
- Restore;
- Add Destination;
- Rotate Key;
- Export Recovery Kit;
- Pause;
- Remove Destination.

### 31.8. Restore flow

Steps:

- choose snapshot;
- inspect manifest;
- select full/selective;
- preview overwrite/conflict;
- verify key;
- restore to temporary/new/current installation;
- post-restore checks;
- result report.

### 31.9. Head Runtime screen

Shows:

- current head;
- epoch;
- lease/freshness;
- candidate devices;
- witness;
- handoff readiness;
- split-brain warnings.

Actions:

- Planned Handoff;
- Prepare Candidate;
- Enter Emergency Head;
- Reconcile Old Head;
- Open Migration Wizard.

Dangerous actions use Trust confirmations.

### 31.10. Usage and Limits

Views:

- Current;
- By Provider;
- By Project;
- By Agent/Run;
- By Capability;
- Local Resources;
- Trends.

Metrics:

- subscription usage if available;
- API cost;
- tokens/audio/image units;
- latency;
- local GPU/CPU time;
- storage;
- background budget;
- failed/retried waste.

Actions:

- Set Budget;
- Set Warning;
- Pause Expensive Background;
- Change Routing Policy;
- Export;
- Open Related Runs.

Usage view must distinguish exact provider data from estimates.

### 31.11. Diagnostics

- Health Check;
- Runtime Logs;
- Provider Logs;
- Sync Logs;
- Voice Diagnostics;
- Capability Probes;
- Network;
- Storage Integrity;
- Database/Index Status;
- Recent Incidents;
- Repair Objects.

Actions:

- Run Selected Test;
- Copy Summary;
- Export Bundle;
- Redact Sensitive Data;
- Open Issue Template;
- Retry Repair;
- Enter Safe Mode.

Raw logs are advanced and searchable. Default diagnostic summary remains human-readable.


# Часть V. Настройки, универсальные взаимодействия и восстановление

## 32. Settings and Administration

### 32.1. Settings geometry

Settings открываются как полноразмерный workspace tab или modal editor, который можно maximize и detach. Нельзя прятать сложные настройки в узком popover.

Layout:

- Sidebar categories;
- search;
- User / Installation / Device / Project scope switch;
- Main settings page;
- `Modified` filter;
- `Reset`;
- `Open Raw Configuration` для advanced users.

### 32.2. Scope and precedence

Каждая setting показывает:

- effective value;
- source scope;
- inherited value;
- whether overridden;
- policy lock;
- restart requirement;
- link to controlling setting.

Scopes:

- Installation;
- User;
- Device;
- Project;
- Session/Run override;
- Provider/Capability specific.

### 32.3. Settings categories

#### General

- startup behavior;
- language/locale;
- date/time format;
- default project location;
- link handling;
- confirmation preferences;
- recent-item retention;
- update channel.

#### Appearance

- theme;
- system/light/dark/high contrast;
- font and scale;
- density;
- icon size;
- animations;
- reduced motion;
- transparency;
- layout profile;
- sidebar/inspector defaults;
- status bar items.

#### Navigation and Keyboard

- shortcuts;
- chord behavior;
- Command Center;
- mouse/trackpad;
- preview tabs;
- tab close behavior;
- drag/drop;
- back/forward history;
- command aliases.

#### Orchestrator and Agents

- execution profile default;
- ask frequency;
- foreground/background preference;
- review depth;
- maximum agents;
- spawn depth;
- time/token budget;
- progress verbosity;
- completion presentation;
- default direct-chat model, delegated to Capability config.

#### Projects

- project discovery;
- Git/worktree defaults;
- project trust default;
- session grouping;
- auto-open changes;
- Kanban visibility;
- project memory portability;
- capabilities recommendation mode;
- archive behavior.

#### Memory

- capture suggestions;
- auto-save thresholds;
- raw media retention;
- indexing;
- review frequency;
- source visibility;
- cross-project retrieval;
- privacy defaults;
- deletion tracking.

#### Voice and Ambient

- push-to-talk/live;
- wake word;
- endpointing;
- interruption;
- brevity;
- proactive speech;
- local/cloud;
- voice;
- transcript;
- ambient mode;
- retention;
- microphone/speaker device;
- meeting behavior.

#### Providers and Capabilities

Opens or embeds Capability Workspace settings:

- connections;
- models;
- routing;
- local runtimes;
- updates;
- discovery;
- skills/MCP/plugins/connectors;
- project-autonomous capability policy.

#### Trust, Privacy and Autonomy

- workspace trust;
- permissions;
- bounded grants;
- elevated mode;
- external sends;
- secrets;
- device trust;
- voice low-risk policy;
- audit detail;
- screen-share privacy;
- emergency stop.

#### Notifications and Attention

- toast categories;
- Inbox priorities;
- snooze defaults;
- DND schedule;
- sound;
- badge counts;
- critical bypass;
- completion summaries;
- annoyance budget.

#### Sync and Offline

- data class policies;
- media replication;
- offline behavior;
- conflict handling;
- bandwidth limits;
- metered network;
- local cache;
- project sync engine.

#### Backup and Recovery

- destinations;
- schedule;
- encryption;
- retention;
- verification;
- recovery keys;
- restore test;
- installation export.

#### Devices

- current device identity;
- roles;
- capabilities;
- head eligibility;
- local paths;
- sensors;
- battery/power policy;
- remote access.

#### Usage and Budgets

- provider budgets;
- warnings;
- background limits;
- local resource limits;
- storage;
- cost display currency;
- usage estimates.

#### Accessibility

- screen reader optimization;
- keyboard-only mode;
- focus thickness;
- target size;
- reduced motion;
- captions;
- audio cues;
- color filters;
- high contrast;
- text scaling;
- live region verbosity.

#### Advanced

- raw config;
- feature flags;
- experimental capabilities;
- telemetry;
- log level;
- cache/index controls;
- protocol versions;
- developer tools;
- safe mode;
- reset options.

### 32.4. Settings search

Search matches:

- setting name;
- description;
- synonyms;
- command;
- provider-specific field;
- current value.

Results show scope and category.

### 32.5. Modified settings

`Modified` view lists deviations from default grouped by scope. Actions:

- Reset Item;
- Reset Group;
- Copy Setting Link;
- Move Override to Another Scope;
- Export Profile.

### 32.6. Raw config

Advanced editor provides:

- schema validation;
- autocomplete;
- diff;
- preview effective changes;
- apply;
- rollback;
- open documentation.

Invalid config never silently replaces last valid runtime configuration.

### 32.7. Reset operations

Different actions:

- Reset Layout;
- Reset UI Preferences;
- Reset Project Overrides;
- Reset Provider Config;
- Clear Cache;
- Rebuild Indexes;
- Sign Out/Disconnect;
- Reset Installation, high-risk.

Each states what is preserved.

---

## 33. Global Menu Bar — complete baseline

Menu labels adapt to OS conventions, but command semantics remain stable. Every menu command is also available through Command Center and may have a keybinding.

### 33.1. File

#### Create

- New Project…
- New Quick Chat
- New Orchestrator Focus…
- New Project Chat…
- New Task…
- New Note…
- New Artifact from Clipboard…
- New Automation…
- New Window

#### Open and Import

- Open Project…
- Open Folder as Project…
- Clone Repository…
- Import Project Memory Pack…
- Import Installation Export…
- Open File…
- Open Artifact…
- Open Recent ▶
- Reopen Closed Tab

#### Save and Export

- Save
- Save All
- Save Current Result as Artifact…
- Export Current View…
- Export Session…
- Export Project Memory Pack…
- Export Installation…

#### Window/Close

- Close Tab
- Close Other Tabs
- Close Project
- Close Window
- Exit/Quit Dennett

### 33.2. Edit

- Undo
- Redo
- Cut
- Copy
- Copy Link to Object
- Copy as Markdown
- Paste
- Paste and Attach
- Paste as New Note
- Select All
- Find in Current View
- Find Next
- Find Previous
- Global Search
- Replace, where editor supports it
- Edit Previous Request, in chat context
- Clear Draft

### 33.3. View

#### Navigation surfaces

- Show/Hide Activity Rail
- Show/Hide Context Sidebar
- Show/Hide Context Inspector
- Show/Hide Bottom Panel
- Show/Hide Status Bar
- Show/Hide Menu Bar, platform-dependent
- Show/Hide Breadcrumbs
- Show/Hide Notifications Center

#### Layout

- Split Right
- Split Down
- Move Tab to Next Group
- Move View to Sidebar
- Move View to Inspector
- Move Panel Left/Right/Bottom
- Open in New Window
- Always on Top
- Maximize Active Pane
- Reset Layout
- Layout Profiles ▶

#### Modes

- Focus Mode
- Full Screen
- Presentation Mode
- Compact Density
- Comfortable Density
- High Contrast
- Reduce Motion
- Zoom In
- Zoom Out
- Reset Zoom

### 33.4. Navigate

- Back
- Forward
- Home
- Orchestrator
- Projects
- Action Inbox
- Agent Radar
- Library
- Memory
- Artifacts
- Capabilities
- Automations
- System Status
- Go to Project…
- Go to Session…
- Go to Run…
- Go to Memory Item…
- Go to Artifact…
- Next Tab
- Previous Tab
- Next Pane
- Previous Pane
- Next Attention Item
- Previous Attention Item
- Open Recent…
- Show Navigation History

### 33.5. Project

- New Project Chat…
- New Background Task…
- Continue Last Session
- Talk About Project
- Open Project Overview
- Open Files
- Open Changes
- Open Runs
- Open Tasks
- Open Project Memory
- Open Project Artifacts
- Open Project Capabilities
- Open Project Events
- Run Tests ▶
- Open Terminal
- Open in External Editor ▶
- Reveal in File Manager
- Refresh/Rescan Project
- Sync Project Now
- Manage Worktrees/Branches…
- Add Capability…
- Export Project Memory Pack…
- Trust Settings…
- Project Settings…
- Pin Project
- Archive Project
- Remove Project from Dennett…
- Delete Project Files…, separate and protected

### 33.6. Agent

- New Quick Agent Session…
- New Session in Current Project…
- Send Current Request
- Send to Background
- Add Message to Queue
- Steer Current Run…
- Stop and Send
- Pause Run
- Resume Run
- Stop Run
- Stop After Current Step
- Retry
- Retry with Different Provider…
- Fork Session
- Fork from Checkpoint…
- Create Checkpoint
- Compare Attempts…
- Request Review
- Run Tests
- Open Changes
- Open Progress
- Open Context Inspector
- Show Why This Happened
- Change Model…
- Change Execution Profile…
- Change Budget…
- Manage Session Capabilities…
- Manage Session Permissions…
- Mark Session Done
- Archive Session
- Export Session…

### 33.7. Memory

- New Note…
- Quick Capture…
- Ask Memory…
- Search Memory…
- Open Current State
- Open Timeline
- Open Needs Review
- Open Conflicts
- Add Selection to Memory…
- Add Selection to Current Context
- Pin Memory Item
- Link to Current Project
- Open Evidence
- Correct…
- Compare History
- Change Scope…
- Hide from Suggested Context
- Export…
- Forget/Delete…
- Track Deletion Requests
- Rebuild Selected Projection, advanced

### 33.8. Capabilities

- Open Capabilities
- Add Provider Connection…
- Add Local Model…
- Add Skill…
- Add MCP Server…
- Add Plugin/Extension…
- Add Connector…
- Add Custom Tool…
- Discover Capabilities…
- Review Candidates
- Run Health Checks
- Manage Current Project Set…
- Enable Selected
- Disable Selected
- Set On-Demand
- Compare Selected…
- Test Selected
- Update Selected
- Update All Eligible
- Roll Back…
- Reauthenticate…
- Remove…
- Open Capability Source
- Open Capability Trust

### 33.9. Automation

- New Automation…
- Describe Automation…
- New Event Trigger…
- New Schedule…
- New Monitor…
- From Template…
- Run Now
- Test/Preview
- Enable
- Disable
- Snooze…
- Pause All Background Automations
- Resume Background Automations
- View Run History
- View Why It Fired
- Import…
- Export…
- Open System Sleep
- Run Maintenance Now…, scoped

### 33.10. Voice

- Start Live Conversation
- Push to Talk
- Talk to Orchestrator
- Talk to Current Project
- Continue Remote Voice Session
- Stop Speaking
- Mute Microphone
- Mute Output
- Pause Listening
- End Voice Session
- Show Transcript
- Show Voice Overlay
- Save Current Voice Note
- Do Not Save Current Segment
- Switch Voice Mode ▶
- Switch Backend ▶
- Switch Input Device ▶
- Switch Output Device ▶
- Ambient Mode ▶
- Voice Settings…
- Privacy Mode…

### 33.11. System

- Open System Status
- Open Devices
- Pair Device…
- Open Sync
- Sync Now
- Open Backups
- Backup Now
- Verify Backup
- Test Restore…
- Open Usage and Limits
- Open Diagnostics
- Open Logs
- Check for Updates
- Restart Runtime…
- Enter Safe Mode…
- Exit Safe Mode
- Planned Head Handoff…
- Emergency Head…
- Lock Secret Broker
- Pause Background Work
- Emergency Stop…

### 33.12. Window

- New Window
- Duplicate Current Window Layout
- Move Current Tab to New Window
- Copy Current Tab to New Window
- Always on Top
- Minimize
- Zoom
- Next Window
- Previous Window
- Merge All Windows, if supported
- Close Window
- Window List ▶

### 33.13. Help

- Getting Started
- What Is Dennett?
- Documentation
- Keyboard Shortcuts
- Command Reference
- Feature Search
- Show Contextual Help
- Run Guided Tour
- Troubleshooting
- Collect Diagnostics…
- Report Issue…
- Request Feature…
- Privacy and Data
- View Licenses
- Check Updates
- About Dennett

---

## 34. Context Menus — common contract

### 34.1. General ordering

Context menu groups follow stable order:

1. primary/open;
2. create/continue;
3. object-specific actions;
4. organization;
5. share/export;
6. diagnostics/details;
7. archive/remove/delete.

Destructive actions are separated by divider and use text labels.

### 34.2. Common item actions

Where applicable:

- Open;
- Open to Side;
- Open in New Window;
- Pin/Unpin;
- Rename;
- Copy Link;
- Add to Current Context;
- Send to Orchestrator;
- Create Task;
- Export;
- Show Details;
- Archive;
- Remove;
- Delete.

### 34.3. Multi-selection

Context menu displays only actions valid for all selected objects. Mixed selections can still offer:

- Open;
- Pin;
- Export;
- Add to Collection;

but not hidden heterogeneous destructive action.

### 34.4. Provider-specific extensions

Provider/plugin actions appear in a labeled submenu with origin, for example:

`Claude Plugin: ...`  
`Codex: ...`  
`MCP Server: ...`

They do not silently insert indistinguishable global commands.

---

## 35. Drag-and-Drop

### 35.1. Supported targets

- file/artifact → chat composer;
- memory item → context;
- session → custom group;
- project → group/favorite;
- capability → Project Capability Set;
- Inbox card → Snoozed/Project, only when semantically valid;
- tab → pane/window;
- artifact → project;
- screenshot → memory/project/chat;
- external file/folder → project/import/capture.

### 35.2. Drop preview

Before commit, target displays action:

- `Attach as context`;
- `Move to group`;
- `Create project from folder`;
- `Add capability to project`;
- `Copy file into project`;
- `Link only`.

If more than one meaning is plausible, a small chooser appears after drop.

### 35.3. Accessibility alternative

Every drag operation has command/menu alternative. WCAG requires operations not depend solely on dragging. [[S23]]

---

## 36. Undo, Redo, Checkpoints and Recovery

### 36.1. Undo domains

Different actions have different reversal mechanisms:

- text/UI edit → Undo;
- file edit → editor/Git/checkpoint;
- session branch → restore/fork;
- project configuration → config rollback;
- capability update → rollback;
- external effect → compensation if possible;
- memory deletion → recovery window only if policy permits;
- sent message/payment → cannot be represented as ordinary Undo.

### 36.2. Undo feedback

After reversible action, toast/status includes:

- what changed;
- `Undo`;
- expiry, if limited.

### 36.3. Destructive confirmation

Confirmation shows:

- exact object count;
- exact scope;
- permanent/recoverable;
- backups/checkpoints;
- external effects;
- safer alternative;
- required authentication.

### 36.4. Error recovery sheet

For recoverable errors:

- plain-language problem;
- what succeeded;
- what did not;
- whether retry is safe;
- recommended action;
- Retry;
- Change Provider;
- Work Offline;
- Open Diagnostics;
- Save Partial Result;
- Cancel.

Nielsen guidance requires errors in plain language with constructive recovery rather than opaque codes. [[S21]]

### 36.5. Partial success

UI never collapses Partial into Failed or Success. It shows:

- completed outcomes;
- missing items;
- preserved artifacts;
- next options.

---

## 37. Multi-window, Deep Links and External Apps

### 37.1. Multi-window

User can dedicate windows to:

- one project;
- Radar;
- Memory;
- Review;
- Voice;
- Office;
- settings/diagnostics.

State remains shared through canonical runtime.

### 37.2. Deep links

Stable links for:

- project;
- session;
- run;
- Inbox card;
- memory handle;
- artifact;
- capability;
- settings page.

Opening link checks identity, availability and permissions before navigation.

### 37.3. External editor integration

Actions:

- Open File;
- Open Project;
- Open at Line;
- Open Diff;
- Open Terminal;
- Copy CLI Command.

Returning from external editor triggers refresh, not blind overwrite.

### 37.4. Protocol handler

`dennett://...` links may open app. External source is untrusted; link cannot directly execute consequential command without user-visible confirmation/policy.

---

## 38. Tray and Global Hotkeys

### 38.1. Tray menu

- Open Dennett;
- Quick Chat;
- Push to Talk;
- Quick Capture;
- Action Inbox count;
- Running count;
- Pause Background;
- Ambient Mode;
- Privacy Mode;
- DND;
- System Status;
- Emergency Stop;
- Quit.

### 38.2. Global hotkeys

Default suggestions, configurable:

- Show/Hide Dennett;
- Command Center;
- Push to Talk;
- Start/End Voice;
- Quick Capture;
- Screenshot to Memory;
- Region Capture;
- Open Inbox;
- Emergency Stop.

Conflicts with OS/app shortcuts are detected during assignment.

### 38.3. Privacy indicator

Tray/status shows ambient microphone/screen capture state continuously and opens privacy controls on click.

---

# Часть VI. Полный каталог состояний

## 39. Universal Component States

Every interactive control supports where applicable:

- Default;
- Hover;
- Focused;
- Pressed;
- Selected;
- Checked;
- Mixed;
- Disabled;
- Read-only;
- Loading;
- Pending;
- Success;
- Warning;
- Error;
- Destructive;
- Offline;
- Stale.

Focus must remain visible. Status cannot rely only on color. [[S23]]

## 40. Application States

### 40.1. Starting

Shows shell quickly, then loads cached view and canonical updates. User can see which content is cached.

### 40.2. Connecting

Cached content accessible; modifying global state may queue or wait according to Server policy.

### 40.3. Online

Normal.

### 40.4. Degraded

Banner or status strip states:

- affected function;
- what remains usable;
- freshness;
- recovery action.

### 40.5. Offline

Prominent but nonblocking indicator. Local project work, memory cache, local models and queued commands remain available where allowed.

### 40.6. Safe Mode

- third-party capabilities disabled/restricted;
- background automations paused;
- diagnostics foregrounded;
- projects open read-only or bounded according to cause.

### 40.7. Emergency Head

Persistent warning includes split-brain risk and disabled global effects.

### 40.8. Updating

- update available;
- downloading;
- ready to restart;
- migrating;
- rollback;
- failed update.

No forced restart during active consequential work without policy.

### 40.9. Locked

App hides sensitive content and requires authentication. Background work follows policy.

## 41. Data View States

Every list/workspace defines:

- Initial Loading;
- Cached Loading;
- Empty;
- Populated;
- Filtering No Results;
- Partial;
- Syncing;
- Stale;
- Conflict;
- Read-only;
- Restricted;
- Source Missing;
- Error;
- Offline Available;
- Offline Unavailable;
- Rebuilding;
- Deleted/Tombstone.

### 41.1. Empty vs no results

Empty: object class contains nothing; show create/import action.  
No results: filter/search eliminated items; show Clear Filters.

### 41.2. Loading

Skeletons preserve layout. Spinner alone cannot hide indefinite operation; after threshold show explanation and cancel/retry.

### 41.3. Stale

Shows last updated and Refresh. Consequential buttons may be disabled until revalidation.

## 42. Project States

- New/Uninitialized;
- Ready;
- Active;
- Running;
- Needs Attention;
- Sleeping/Inactive;
- Archived;
- Location Missing;
- Device Offline;
- Sync Conflict;
- Restricted;
- Capability Missing;
- Indexing;
- Importing;
- Removing;
- Error.

These are UI summaries from canonical signals, not a mandatory single project state machine.

## 43. Session States

- Draft;
- Idle;
- Generating;
- Tool Running;
- Background;
- Queued Message;
- Waiting User;
- Waiting Permission;
- Waiting Dependency;
- Paused;
- Stopping;
- Completed;
- Partial;
- Failed;
- Cancelled;
- Archived;
- Provider Lost;
- Device Lost;
- Stale;
- Read-only History.

## 44. Task and Run States

- Queued;
- Running;
- Waiting;
- Paused;
- Verifying;
- Completed;
- Partial;
- Failed;
- Cancelled;
- Unknown Effect;
- Reconciling;
- Superseded;
- Scheduled;
- Deferred Offline;
- Budget Exhausted;
- Policy Blocked.

Waiting reason always displayed separately.

## 45. Agent States

- Proposed;
- Starting;
- Active;
- Speaking/Interacting;
- Tool Use;
- Waiting;
- Handing Off;
- Reviewing;
- Stalled;
- Stopping;
- Completed;
- Failed;
- Disconnected;
- Unknown.

UI does not animate `Active` if last heartbeat/progress stale.

## 46. Voice States

- Off;
- Wake-only;
- Idle;
- Listening;
- Endpointing;
- User Turn Committed;
- Thinking;
- Speaking;
- Interrupted;
- Muted Input;
- Muted Output;
- Paused;
- Background Thought;
- Waiting Tool;
- Handoff;
- Reconnecting;
- Local Fallback;
- Active Elsewhere;
- Failed;
- Ending.

## 47. Capability States

Display orthogonal dimensions from Capability Fabric, not one status:

- source/discovery;
- installed/connected;
- trusted;
- utility;
- active/on-demand;
- ownership;
- health;
- update.

## 48. Sync and Backup States

Sync:

- Synced;
- Syncing;
- Pending;
- Paused;
- Conflict;
- Stale;
- Offline;
- Local Only;
- Error;
- Deletion Pending.

Backup:

- Healthy;
- Running;
- Verification Due;
- Verification Failed;
- Destination Offline;
- Key Missing;
- Restore Testing;
- Retention Pending;
- Failed.

## 49. Permission and Trust States

- Allowed by Grant;
- Ask;
- Denied;
- Pending;
- Expired;
- Revoked;
- Step-up Required;
- Workspace Restricted;
- Trusted-Bounded;
- Trusted-Elevated;
- Quarantined;
- Policy Conflict.

## 50. Notification States

- Unread;
- Read;
- Dismissed;
- Muted;
- Converted to Inbox;
- Expired;
- Delivery Pending;
- Delivery Failed.

---

## 51. State transition feedback

### 51.1. Immediate feedback

Every user action changes visible state immediately:

- button busy/pending;
- optimistic local marker only where safe;
- operation appears in progress;
- final canonical confirmation updates state.

### 51.2. Optimistic updates

Allowed for:

- pin/unpin;
- layout;
- local draft;
- view filters;
- noncritical organization.

Not allowed to pretend complete for:

- permissions;
- external sends;
- payments;
- Task completion;
- delete propagation;
- backup verification;
- head handoff.

### 51.3. Status messages and screen readers

State changes use accessible status messages without moving focus. WCAG 2.2 explicitly treats status messages as programmatically determinable. [[S23]]


# Часть VII. Интуитивность, доступность и функции, которых раньше не хватало

## 52. Progressive Onboarding

### 52.1. Первый запуск

Первый запуск не требует настроить всю систему до использования.

Минимальный путь:

1. выбрать или создать installation;
2. подтвердить владельца/устройство;
3. подключить хотя бы один provider либо local model;
4. открыть проект или Quick Chat;
5. выполнить тестовый безопасный turn.

Остальные настройки предлагаются в момент первой реальной необходимости.

### 52.2. Welcome state

Карточки:

- Open a Project;
- Talk to Dennett;
- Connect Provider;
- Set Up Local Model;
- Import Existing Dennett;
- Explore Demo.

`Skip Setup` доступен, если local/offline baseline готов.

### 52.3. Contextual teaching

Вместо длинного обязательного tour:

- одноразовые hints;
- keyboard hints в tooltips;
- empty-state actions;
- `What can I do here?`;
- Command Center suggestions;
- dismiss/never show again.

### 52.4. Capability discovery

Когда пользователь впервые делает действие, для которого capability отсутствует, UI объясняет:

- что требуется;
- варианты providers/local tools;
- privacy/cost difference;
- `Connect`, `Use Alternative`, `Not Now`.

### 52.5. Returning expert

Onboarding never resets customized layout or reopens dismissed hints after update unless behavior materially changed.

---

## 53. Accessibility

### 53.1. Keyboard completeness

Desktop behavior should also follow platform conventions and accessible multi-input design principles. [[S24]]

Все interactive elements reachable by keyboard. Нет keyboard trap. Focus visible. Эти требования соответствуют WCAG 2.2. [[S23]]

### 53.2. Focus rules

- opening dialog moves focus to first meaningful field;
- closing returns focus to invoker;
- streaming update does not steal focus;
- list reorder does not move focused item unexpectedly;
- validation error moves focus only on explicit submit;
- new notification uses live region, not focus jump;
- Escape closes transient overlays in reverse order;
- emergency action has global reachable shortcut.

### 53.3. Screen reader semantics

Every control has:

- accessible name;
- role;
- value/state;
- description when needed;
- keyboard hint;
- error relationship.

Agent states and streaming output use live regions with configurable verbosity:

- Minimal: only completion/attention;
- Standard: phase changes;
- Verbose: significant actions.

Raw token streaming is not announced character by character.

### 53.4. Lists and virtualization

Large lists maintain:

- stable logical order;
- item count;
- position;
- accessible group headings;
- search result announcement;
- keyboard selection.

Virtualization cannot make screen reader lose selected item.

### 53.5. Visual accessibility

- high contrast;
- no state by color alone;
- scalable text;
- reflow at narrow widths;
- visible focus;
- adjustable density;
- minimum target sizes;
- icon labels/tooltips;
- reduced transparency;
- reduced motion;
- distinguish links/buttons.

### 53.6. Motion

Animations are functional:

- panel transition;
- relationship movement;
- progress.

Reduced Motion removes nonessential animation. Radar/Office never require following motion to understand state.

### 53.7. Audio and Voice

- live captions;
- transcript;
- visual listening/speaking state;
- mute separate for input/output;
- no critical information only in audio;
- user-selectable audio cues;
- volume-independent text confirmation;
- push-to-talk alternative.

### 53.8. Cognitive accessibility

- plain language;
- short primary labels;
- advanced details collapsed;
- consistent verbs;
- no unexplained acronyms;
- confirmation repeats exact action;
- option consequences visible;
- stable layout;
- saved profiles.

### 53.9. Accessibility Inspector

Optional diagnostic page checks:

- missing accessible names;
- shortcut conflicts;
- contrast/theme;
- focus order;
- reduced motion;
- screen reader mode;
- captions backend.

---

## 54. Keyboard Shortcuts — recommended baseline

All shortcuts are configurable. Platform-specific conventions override where necessary.

### 54.1. Global

```text
Ctrl/Cmd+K                 Global Command Center
Ctrl/Cmd+Shift+P           Commands only
Ctrl/Cmd+P                 Quick Open / entity search
Ctrl/Cmd+,                 Settings
Ctrl/Cmd+N                 Contextual New
Ctrl/Cmd+Shift+N           New Window
Ctrl/Cmd+W                 Close Tab
Ctrl/Cmd+Shift+W           Close Window
Alt+Left / Alt+Right       Back / Forward
Ctrl/Cmd+Tab               Recent Tabs
Ctrl/Cmd+1..9              Focus Pane/Primary Area by profile
Ctrl/Cmd+B                 Toggle Context Sidebar
Ctrl/Cmd+J                 Toggle Bottom Panel
Ctrl/Cmd+Shift+I           Toggle Inspector
Ctrl/Cmd+Shift+F           Focus Mode
F11                        Full Screen
```

### 54.2. Navigation

```text
Ctrl/Cmd+Alt+H             Home
Ctrl/Cmd+Alt+O             Orchestrator
Ctrl/Cmd+Alt+P             Projects
Ctrl/Cmd+Alt+I             Action Inbox
Ctrl/Cmd+Alt+R             Agent Radar
Ctrl/Cmd+Alt+M             Memory
Ctrl/Cmd+Alt+A             Artifacts
```

These are suggested and may conflict on some OS; command chords may replace them.

### 54.3. Chat and Agents

```text
Ctrl/Cmd+Enter             Send
Alt+Enter                  Send to Background / New background session
Shift+Enter                New line
Esc                        Stop current visible generation or close overlay, contextual
Ctrl/Cmd+Shift+Enter       Stop and Send
Ctrl/Cmd+Alt+Enter         Add to Queue
Ctrl/Cmd+Shift+K           Steer Current Run
Ctrl/Cmd+Shift+C           Add Context
Ctrl/Cmd+Shift+V           Start Voice in current context
Alt+Up / Alt+Down          Previous / Next prompt
```

### 54.4. Review

```text
Alt+F5 / F5                Previous / Next change
Ctrl/Cmd+Shift+D           Open Changes
Ctrl/Cmd+Alt+Z             Restore/Fork checkpoint chooser
Ctrl/Cmd+Shift+T           Run Project Tests
```

### 54.5. Inbox

```text
J / K or Down / Up         Next / Previous card
Enter                      Open
S                          Snooze
V                          Discuss by Voice
R                          Apply recommended action when safe
E                          Expand details
X                          Close where allowed
```

Single-letter shortcuts work only when focus is not in text input and can be disabled.

### 54.6. Voice and Capture

```text
Configurable global key    Push to Talk
Configurable global key    Start/End Live Voice
Configurable global key    Quick Capture
Configurable global key    Region Screenshot to Memory
Configurable global key    Emergency Stop
```

---

## 55. Newly Added High-Value Functions

These functions were not all explicitly required in the original concept, but improve real daily use without creating separate subsystems.

### 55.1. Resume Strip

After user opens Inbox, Radar or another project, a small optional strip offers:

- `Return to [previous session]`;
- elapsed time away;
- whether state changed.

It prevents context loss from necessary interruptions.

### 55.2. What Changed Since I Left

When returning to a project/session after meaningful absence, Dennett provides a compact delta:

- agent completed/failed;
- files changed;
- new artifacts;
- pending question;
- upstream project changes;
- capability/provider change;
- sync conflict.

Actions:

- Continue;
- Review Changes;
- Open Full Timeline;
- Dismiss.

This is generated from canonical events, not invented narrative.

### 55.3. Selection-to-Agent

Any selected text/file/artifact/memory item can be sent to:

- Current Session;
- New Project Chat;
- Orchestrator;
- New Quick Chat;
- Background Task.

The chooser shows where context will go and whether source remains linked.

### 55.4. Context Lens

Already defined in Inspector, this is elevated as a first-class usability feature because agent mistakes often originate from wrong or missing context.

User can see:

- what agent saw;
- what it did not see;
- why item was included;
- freshness;
- authority/advisory status.

### 55.5. Explain State

Every nontrivial state has command:

- Why Waiting?;
- Why Restricted?;
- Why Stale?;
- Why This Capability?;
- Why Did This Event Fire?;
- Why Is This in Inbox?;
- Why Can’t I Retry?

Explanation uses structured runtime facts.

### 55.6. Smart Favorites and Saved Views

User can favorite:

- project;
- session;
- memory query;
- Radar filter;
- artifact collection;
- capability category;
- settings page.

Favorites can be grouped and reordered, similar to Linear’s user-organized favorite folders. [[S19]]

### 55.7. Cross-Object Compare

Generic compare command supports compatible pairs:

- sessions;
- Runs;
- artifacts;
- memory versions;
- capabilities;
- project configurations;
- model results.

### 55.8. Privacy Curtain

One action temporarily obscures:

- notifications;
- sensitive project names;
- secret values;
- private memory;
- personal contacts;
- usage/billing.

Useful for screen sharing and public environments. It never claims to be cryptographic privacy.

### 55.9. Handoff to Device

For compatible objects:

- Continue on Phone;
- Continue Voice on Phone;
- Open on Laptop;
- Run on Server;
- Open File on Desktop Node.

UI shows what moves:

- control;
- session view;
- execution;
- media stream;
- nothing/only link.

### 55.10. Safe Command History

Command Center offers recent commands with parameter preview. Re-run is disabled or requires reconciliation for external effects.

### 55.11. Guided Recovery

After failure, Dennett offers a recovery path instead of dumping logs:

- switch provider;
- reconnect device;
- use local model;
- save partial work;
- restore checkpoint;
- open diagnosis.

### 55.12. Temporary Workspace

User can create a scratch workspace for:

- comparing artifacts;
- collecting sources;
- temporary notes;
- ad hoc chat.

It can later become a project or be discarded. It does not pollute Projects Hub by default.

### 55.13. Attention Digest

Instead of many background completion notifications, Dennett can group them into a digest:

- completed;
- failed;
- needs review;
- suggested next steps.

User controls cadence and urgency exceptions.

### 55.14. One-click Takeover

When computer-use is active, a persistent but compact control enables:

- Take Over;
- Pause Agent;
- Limit to Window;
- Do Not Click Send/Pay/Delete;
- Return Control.

### 55.15. UI Help Through Orchestrator

`Ask Dennett about this screen` attaches current UI location and selected object to a help question. It does not grant agent permission to click automatically.

---

## 56. Dynamic UI Contributions from Capabilities

### 56.1. Allowed contribution types

A capability/plugin may contribute:

- command;
- context menu action;
- artifact viewer;
- Inspector section;
- Bottom Panel tab;
- project section;
- settings page;
- status item;
- capture source.

### 56.2. Constraints

- namespaced origin visible;
- declared permissions;
- no silent top-level Activity item;
- disabled in untrusted workspace unless allowed;
- no overriding core Stop/Emergency commands;
- keyboard accessible;
- uninstall removes UI contribution without deleting user artifacts;
- provider-specific settings remain scoped.

### 56.3. Clutter control

New contribution defaults to:

- Command Center and relevant context menu;
- not pinned globally;
- suggested pin after repeated use.

### 56.4. Failure isolation

If extension UI crashes:

- core shell remains usable;
- contribution disabled;
- state preserved;
- diagnostics available;
- no infinite reload loop.

---

## 57. Localization and Internationalization

- all labels localizable;
- no hard-coded text in layouts;
- date/time locale-aware;
- RTL-ready layout where feasible;
- project/code content language independent;
- search supports transliteration/synonyms where available;
- keyboard shortcuts shown by OS;
- commands have stable internal IDs independent of language;
- provider-native untranslated field may show original plus explanation;
- voice language can differ from UI language.

---

## 58. Performance and Responsiveness Targets

These are pilot targets, not immutable guarantees.

### 58.1. Warm startup

- shell visible quickly from cached presentation state;
- no blocking wait for all providers/devices;
- canonical refresh streams in;
- stale marker shown until updated.

### 58.2. Navigation

- switching cached views feels immediate;
- large lists virtualized;
- search progressively returns results;
- no full memory scan on UI thread;
- opening Inspector does not trigger heavy LLM call.

### 58.3. Command Center

Local/recent commands and entities should appear before remote semantic results.

### 58.4. Streaming

- batch UI updates;
- preserve scroll position;
- follow output only when user is at bottom;
- `New messages` marker when user scrolled up;
- no rendering every token as separate layout operation.

### 58.5. Resource modes

- Normal;
- Battery Saver;
- Low Bandwidth;
- Presentation;
- Accessibility;
- Diagnostics.

---

# Часть VIII. Evaluation, implementation and completeness

## 59. Usability Evaluation

### 59.1. Core metrics

- time to resume previous work;
- time to start project chat;
- time to locate active Run;
- time to understand why agent waits;
- number of unnecessary dialogs;
- number of missed Inbox cards;
- notification interruption rate;
- successful undo/recovery;
- mistaken external action rate;
- stale-state misinterpretation rate;
- keyboard completion coverage;
- screen-reader task completion;
- cross-project navigation time;
- user correction burden;
- percentage of sessions where Inspector/trace was needed;
- layout customization retention.

### 59.2. AI-specific evaluation

Following HAX guidance, tests cover initial interaction, ordinary interaction, AI failure and long-term use. [[S20]]

Measure:

- whether capability is described honestly;
- whether uncertainty is visible;
- whether user can correct;
- whether correction is remembered appropriately;
- whether AI action is distinguishable from suggestion;
- whether user understands scope;
- whether errors have recovery;
- whether personalization becomes intrusive.

### 59.3. Notification evaluation

- completions noticed without constant checking;
- focus not interrupted unnecessarily;
- DND respected;
- urgent items still delivered;
- Inbox vs notification distinction understood.

### 59.4. Dense-user test

Dataset:

- 50+ projects;
- 500+ sessions;
- 100+ artifacts;
- 1000+ memory items;
- multiple devices/providers;
- dozens of capabilities;
- several active Runs.

UI must remain navigable through search, grouping, filters and saved views.

### 59.5. Failure tests

- Head offline;
- stale Radar;
- permission expired;
- provider outage;
- sync conflict;
- UNKNOWN effect;
- missing project folder;
- capability update failure;
- extension UI crash;
- voice active elsewhere;
- screen reader while streaming;
- app restart with active Runs;
- update rollback.

---

## 60. Acceptance and Rejection Gates

### 60.1. Accept desktop model if

- a new user can open/import a project and chat without learning infrastructure;
- an experienced user can reach every command by keyboard;
- active work and pending decisions are discoverable within seconds;
- direct project chat remains central;
- technical details are available without dominating;
- stale/offline state is not mistaken for live;
- Inbox and notifications are not confused;
- multiple sessions can be organized without context loss;
- dangerous actions show exact effects;
- layout works on one and multiple monitors;
- accessibility baseline passes;
- no subsystem creates parallel truth in UI.

### 60.2. Reject or redesign if

- default Activity Rail becomes a list of every subsystem;
- Home turns into metrics dashboard;
- user must open Radar to know a foreground agent completed;
- user cannot find Stop;
- session list becomes unusable after hundreds of items;
- diff review requires leaving the project context entirely;
- every AI action opens a modal;
- UI says success based only on model text;
- notification dismissal resolves Inbox card;
- project works only with Kanban/workflow;
- local/offline state silently overwrites canonical state;
- plugin can insert untrusted top-level UI without origin;
- screen reader receives every token stream;
- layout changes unexpectedly due to AI personalization.

---

## 61. Phased Implementation

### Phase 1 — Core Workbench

- global shell;
- Command Center;
- Home minimal;
- Orchestrator;
- Projects Hub;
- Project Chat;
- session list;
- files/changes;
- basic terminal/tests;
- settings shell;
- online/offline indicators.

### Phase 2 — Durable Control

- Runs;
- Action Inbox;
- Agent Radar list;
- notifications;
- approvals;
- checkpoints;
- background completion;
- system status.

### Phase 3 — Knowledge and Results

- Memory Workspace;
- Artifacts;
- Context Inspector;
- Why This Happened;
- project overview;
- compare/version flows.

### Phase 4 — Capabilities and Operations

- Capability Workspace;
- providers/models;
- skills/MCP/plugins/connectors;
- automations/events;
- devices/sync/backup/usage/diagnostics.

### Phase 5 — Voice and Capture

- Voice Overlay;
- Quick Capture;
- desktop ambient controls;
- cross-device voice handoff;
- screen selection context.

### Phase 6 — Advanced Workbench

- floating windows;
- saved layout profiles;
- Office view;
- graph views;
- dynamic UI contributions;
- advanced accessibility diagnostics;
- multi-monitor polish.

A later phase does not justify blocking a useful first release.

---

## 62. Screen Completeness Checklist

Every workspace implementation must answer:

1. What is its purpose in one sentence?
2. What is the authoritative state source?
3. What is the primary action?
4. What is the empty state?
5. What is the loading state?
6. What is the offline state?
7. What is stale?
8. What can fail?
9. How does user recover?
10. What is reversible?
11. What is destructive?
12. What appears in Sidebar?
13. What appears in Header?
14. What appears in Inspector?
15. What appears in context menu?
16. What appears in Command Center?
17. What is keyboard path?
18. What is screen reader announcement?
19. What notifications can it emit?
20. What can be pinned/favorited?
21. What can open to side/new window?
22. What state survives restart?
23. What does not belong in UI cache?
24. What provider/capability extensions may add?
25. What telemetry proves usability?

A screen is incomplete if any materially relevant answer is missing.

---

## 63. Command Completeness Contract

Every command has:

```yaml
command:
  command_id: stable.namespaced.id
  label: localized
  description: localized
  category: text
  applicable_contexts: []
  required_selection: optional
  required_capability: optional
  required_permission: optional
  external_effect: boolean
  undo_strategy: optional
  default_keybinding: optional
  menu_placements: []
  command_center_visible: boolean
  origin: core | provider | plugin | project
  offline_behavior: allow | queue | deny | conditional
```

Buttons and menu items invoke command IDs. This prevents two UI surfaces from implementing subtly different logic.

---

## 64. Final Normative Formula

> **Dennett Desktop is an Adaptive Agent Workbench. It keeps project chats and user intent at the center, uses Action Inbox for real decisions, Radar for meaningful live state, Library for reusable knowledge and capabilities, and a configurable workbench shell for all other views. It exposes every important action through visible controls and a global Command Center, restores context after interruption, provides evidence and recovery for AI failures, remains useful offline, and never turns technical observability into mandatory micromanagement.**

---

# Appendix A. Source Ledger

## Internal Dennett specifications

**[S01] Dennett Functional Concept.** Product vision, desktop as cockpit, direct project chats, Action Inbox, Radar, voice, memory and capabilities.  
`00_Dennett_Functional_Concept.md`

**[S02] Dennett Specification Index and Shared Contracts.** Ownership, sources of truth, common references and UI boundary.  
`01_Dennett_Specification_Index_and_Shared_Contracts.md`

**[S03] Dennett Memory Fabric 1.2.** Memory objects, context, evidence, correction, deletion, project memory and offline behavior.  
`10_Dennett_Memory_Fabric.md`

**[S04] Dennett Pragmatic Agentic Control Fabric 1.1.** Project sessions, single-agent-first execution, Task/Run, background work, review and completion.  
`20_Dennett_Agentic_Control_Fabric.md`

**[S05] Dennett Trust, Identity, Autonomy and Permissions.** Authentication, grants, approvals, workspace trust, external effects and emergency controls.  
`30_Dennett_Trust_Identity_Autonomy_and_Permissions.md`

**[S06] Dennett Voice and Ambient Interaction Fabric.** Voice Session, overlay states, interruption, ambient and multi-device voice.  
`40_Dennett_Voice_and_Ambient_Interaction_Fabric.md`

**[S07] Dennett Capabilities, Providers and Integrations.** Capability lifecycle, project sets, providers, models, skills, MCP, connectors and UI handoff.  
`41_Dennett_Capabilities_Providers_and_Integrations.md`

**[S08] Dennett Server Runtime, Events, Sync and Portability.** Head runtime, canonical Action Inbox/Radar state, devices, offline, sync, backup and recovery.  
`50_Dennett_Server_Runtime_Events_Sync_and_Portability.md`

## Workbench and agent product references

**[S09] Visual Studio Code — User Interface.** Workbench geometry, tabs, split views, floating windows, Command Palette, settings, Zen Mode and notifications. Current documentation accessed 12 July 2026.  
https://code.visualstudio.com/docs/editing/userinterface

**[S10] Visual Studio Code — Custom Layout.** Movable views, secondary sidebar, panels and layout customization.  
https://code.visualstudio.com/docs/configure/custom-layout

**[S11] Visual Studio Code — Agents Window.** Agent-first window, cross-workspace sessions list, customizations, chat, changes panel, local validation and shared sessions. Updated 8 July 2026.  
https://code.visualstudio.com/docs/agents/agents-window

**[S12] Visual Studio Code — Use Chat.** Queue, steer, stop-and-send, context attachments, review, notifications and diagnostics. Updated 8 July 2026.  
https://code.visualstudio.com/docs/chat/chat-overview

**[S13] Visual Studio Code — Chat Sessions.** Parallel sessions, pinning, grouping, custom groups, archive, fork, export and state indicators.  
https://code.visualstudio.com/docs/chat/chat-sessions

**[S14] Visual Studio Code — Checkpoints.** File snapshots and recovery around agent interactions.  
https://code.visualstudio.com/docs/chat/chat-checkpoints

**[S15] Visual Studio Code — Artifacts Panel.** Agent-generated artifacts and review surfaces.  
https://code.visualstudio.com/docs/chat/chat-artifacts

**[S16] OpenAI — Introducing the Codex App.** Agent command center, project threads, parallel agents, worktrees, skills, automations and review queue.  
https://openai.com/index/introducing-the-codex-app/

## Attention and navigation references

**[S17] Linear — Inbox.** Split attention queue, actions, snooze and reminders.  
https://linear.app/docs/inbox

**[S18] Linear — Search.** Global search, current-view filter, recent items and keyboard navigation.  
https://linear.app/docs/search

**[S19] Linear — Favorites.** User-defined favorites, folders and drag-and-drop organization.  
https://linear.app/docs/favorites

## Human-AI and general UX references

**[S20] Microsoft — Guidelines for Human-AI Interaction / HAX Toolkit.** Evidence-based guidelines, design patterns, failures and long-term AI interaction.  
https://www.microsoft.com/en-us/haxtoolkit/ai-guidelines/  
https://www.microsoft.com/en-us/haxtoolkit/

**[S21] Nielsen Norman Group — 10 Usability Heuristics.** Visibility, user control, consistency, error prevention, minimalist design and error recovery.  
https://www.nngroup.com/articles/ten-usability-heuristics/

**[S22] Nielsen Norman Group — Recognition Rather Than Recall and Flexibility/Efficiency.** Used for visible context, favorites, shortcuts and customization.  
https://www.nngroup.com/articles/recognition-and-recall/  
https://www.nngroup.com/articles/flexibility-efficiency-heuristic/

**[S23] W3C — Web Content Accessibility Guidelines 2.2.** Keyboard access, no keyboard traps, focus, interruptions, status messages, target size and error prevention. Desktop implementation is not necessarily a web app, but the interaction requirements remain useful.  
https://www.w3.org/TR/WCAG22/

**[S24] Microsoft — Windows App Design Overview.** Intuitive, accessible and consistent desktop experience across input types and form factors.  
https://learn.microsoft.com/en-us/windows/apps/design/

---

# Appendix B. Research Conclusions Mapped to Design

1. VS Code-style stable workbench geometry is accepted; code-specific assumptions are not.
2. Agent-first and project/code-first work share the same sessions and state.
3. Session list is a first-class navigation object and scales through grouping, pinning, search and archive. [[S13]]
4. Diff, tests and artifact review stay adjacent to chat.
5. Queue/Steer/Stop are separate semantics.
6. Action Inbox uses snooze and keyboard navigation; notifications remain separate.
7. Command Center is the universal fallback for discoverability and expert speed.
8. User can customize layout, favorites and profiles without changing canonical project state.
9. Status is visible, freshness explicit and errors actionable.
10. AI uncertainty, sources and context are inspectable without exposing hidden reasoning.
11. Accessibility is part of baseline, not a later theme.
12. Office and graphs remain optional projections.
13. Providers and plugins extend UI through constrained namespaced contribution points.
14. Healthy infrastructure remains quiet.
15. User can always stop, undo where possible, or continue manually.

---

# Definition of Done

This specification is complete enough to guide architecture and visual design when:

- every baseline workspace has purpose, layout, actions and states;
- every baseline top-level menu is defined;
- every significant operation has a command path;
- every consequential operation reaches Trust Fabric;
- every dynamic object has loading, empty, stale, offline, error and recovery behavior where applicable;
- project chat remains the shortest route to productive work;
- Inbox, Radar and notifications have non-overlapping responsibilities;
- all major functions are keyboard reachable;
- screen-reader and focus behavior is specified;
- layout supports customization and multiple monitors;
- provider/plugin additions cannot overwhelm or compromise the core shell;
- implementation can begin without inventing missing business semantics inside individual screens.
