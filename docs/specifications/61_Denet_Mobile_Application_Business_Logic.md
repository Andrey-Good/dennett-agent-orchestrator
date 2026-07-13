# Denet Mobile Application Business Logic

> **Repository edition · 2026-07-13 · `61`**  
> Это самостоятельный канонический документ репозитория Denet. Начните с [карты документации](../README.md).  
> Related: [50_Denet_Server_Runtime_Events_Sync_and_Portability.md](./50_Denet_Server_Runtime_Events_Sync_and_Portability.md).

## Интегрированные contract supplements

Следующие небольшие нормативные документы выделены из предархитектурного gap-аудита. Они являются частью текущего набора и обязательны для изменений, пересекающих указанные границы:

- [`A_Ambient_Sensory_Capture_Contract.md`](contracts/A_Ambient_Sensory_Capture_Contract.md)
- [`B_External_Communication_Operation.md`](contracts/B_External_Communication_Operation.md)
- [`C_Project_Lifecycle_Contract.md`](contracts/C_Project_Lifecycle_Contract.md)
- [`F_Identity_Key_and_Ownership_Recovery_Contract.md`](contracts/F_Identity_Key_and_Ownership_Recovery_Contract.md)
- [`G_Resource_Pressure_and_Usage_Accounting_Contract.md`](contracts/G_Resource_Pressure_and_Usage_Accounting_Contract.md)
- [`H_Federated_Global_Search_Contract.md`](contracts/H_Federated_Global_Search_Contract.md)
- [`I_Locale_Timezone_Language_and_Travel_Contract.md`](contracts/I_Locale_Timezone_Language_and_Travel_Contract.md)

Эти supplements не дают одному lifecycle разойтись по нескольким большим файлам; canonical owner указан в заголовке каждого документа.


## Полная бизнес-логика мобильного приложения, быстрых действий, экранов, меню, кнопок, состояний, системных поверхностей и сценариев использования на ходу

**Версия:** 1.0  
**Дата исследования:** 12 июля 2026 года  
**Статус:** канонический baseline бизнес-логики мобильного приложения до выбора UI-фреймворка и программной архитектуры.  
**Каноническое имя:** `61_Denet_Mobile_Application_Business_Logic.md`.

Этот документ является самостоятельным. Для понимания основной модели не требуется знать историю обсуждения или читать предыдущие версии спецификаций.

**Denet** — персональная агентная операционная система. Пользователь напрямую ведёт проектных агентов в папках и репозиториях, общается с постоянным главным оркестратором, использует долговременную память, голос, события, подключаемые модели и инструменты, получает проактивную помощь и управляет работой системы с нескольких устройств.

Мобильное приложение Denet не является уменьшенной копией desktop-приложения. Его главная задача — дать пользователю возможность за несколько секунд:

- понять, что изменилось;
- увидеть, где системе нужен его выбор;
- отдать короткую команду;
- продолжить голосовой разговор;
- зафиксировать мысль, фотографию, ссылку или документ;
- остановить, разрешить или перенаправить процесс;
- быстро вернуться к тому, от чего его прервали;
- передать сложную работу desktop, ноутбуку или серверу.

Документ продолжает и применяет следующие канонические спецификации:

- функциональную концепцию Denet — продуктовые цели и роль телефона как пульта; [[S01]]
- Specification Index — границы ответственности и источники истины; [[S02]]
- Memory Fabric — память, evidence, capture, correction и offline operation log; [[S03]]
- Pragmatic Agentic Control Fabric — Project Session, Work Item, Task, Run и минимально достаточную оркестрацию; [[S04]]
- Trust Fabric — identity, mobile step-up, permissions, grants, external effects и Emergency Stop; [[S05]]
- Voice Fabric — Voice Session, turn-taking, ambient interaction и handoff; [[S06]]
- Capability Fabric — providers, models, skills, MCP, connectors и speech/computer-use backends; [[S07]]
- Server Runtime — Head Runtime, канонические Action Inbox/Radar, sync, offline, backup и recovery; [[S08]]
- Desktop Application Logic — desktop workbench и общие пользовательские паттерны, которые mobile продолжает, но не копирует буквально. [[S09]]

---

# Часть I. Единая мобильная UX-модель и границы

## 0. Итоговое решение

### 0.1. Denet Mobile — Glance, Capture, Decide, Continue

Лучшее решение — не «полный Denet на маленьком экране» и не один гигантский голосовой экран.

Мобильное приложение Denet должно быть **Interruptible Agent Remote** с четырьмя основными циклами:

1. **Glance — увидеть.** За несколько секунд понять, что требует внимания, что работает, что завершено и насколько сведения свежие.
2. **Capture — зафиксировать.** Одним действием сохранить голос, фото, документ, ссылку, экран, выделенный текст или сырую мысль, не заставляя пользователя сразу всё раскладывать по папкам.
3. **Decide — решить.** Быстро принять, отклонить, изменить или отложить значимое решение с точным пониманием последствий.
4. **Continue — продолжить.** Вернуться к прерванной работе, продолжить project chat или voice session, либо передать её на другое устройство.

Короткая формула:

> **Denet Mobile — это быстрый доверенный пульт персональной агентной системы: glanceable состояние, voice-first управление, capture-first ввод, decision-first Inbox и interruption-safe продолжение работы.**

### 0.2. Mobile не является mini-desktop

Телефон не должен пытаться полноценно заменить:

- большой diff-review нескольких сотен строк;
- сложный редактор workflow;
- настройку всех provider-specific параметров;
- массовое управление памятью;
- просмотр длинных traces;
- разработку в многооконном workbench;
- архитектурное сравнение десятков artifacts;
- глубокую диагностику сервера.

Телефон должен позволять:

- прочитать компактную сводку;
- задать вопрос;
- оставить направление агенту;
- посмотреть ключевой diff;
- принять или отклонить изменение;
- открыть artifact;
- подтвердить действие;
- остановить процесс;
- перенести сложную работу на desktop;
- получить ссылку или QR/deep link для продолжения.

Если задача объективно требует большого экрана, mobile не притворяется удобным редактором. Он предлагает `Continue on Desktop`, `Open on Laptop`, `Run on Server` или `Send Review Link`.

### 0.3. Скорость измеряется не количеством анимаций, а временем до результата

Главные мобильные показатели:

- время от разблокировки до нужного состояния;
- количество обязательных taps;
- время до начала voice capture;
- время до локального сохранения заметки;
- время до решения Inbox card;
- resumption lag после возврата;
- число экранов, которые пришлось открыть;
- число ситуаций, где пользователь потерял draft или контекст;
- доля действий, выполненных из notification, widget, shortcut или share sheet без полного запуска приложения.

Интерфейс не считается быстрым, если он красиво открывается, но заставляет пользователя пройти пять sheet-экранов до команды.

### 0.4. Прерывание — нормальный режим, а не исключение

Мобильная сессия может быть прервана:

- блокировкой экрана;
- телефонным звонком;
- другим приложением;
- потерей сети;
- переходом в камеру;
- системным permission prompt;
- прибытием в нужное место;
- началом разговора с человеком;
- коротким использованием приложения на ходу;
- принудительным завершением процесса ОС.

Поэтому каждый значимый mobile-flow обязан иметь:

- мгновенное сохранение локального draft/state;
- явный committed/uncommitted статус;
- безопасное восстановление;
- короткую Resume Capsule;
- отсутствие скрытого подтверждения при уходе;
- понятный результат, если операция завершилась в фоне;
- возможность не продолжать устаревшее действие.

### 0.5. OS surfaces являются частью продукта

Для частых действий открытие полного приложения не должно быть обязательным.

Denet Mobile использует, где платформа позволяет:

- home-screen widgets;
- lock-screen widgets;
- Live Activities / Android Live Updates;
- notification actions и direct reply;
- app shortcuts;
- share extension / Android share target;
- Quick Settings / Control Center;
- Action button и hardware shortcuts;
- wearable actions;
- headset buttons;
- deep links;
- voice wake или push-to-talk.

Android рекомендует использовать widgets для небольших порций glanceable-информации, а подробности раскрывать уже в приложении. [[S10]] Live Updates предназначены только для активных, начатых пользователем и чувствительных ко времени процессов; они не должны превращаться в постоянный shortcut-dashboard. [[S11]]

### 0.6. Один объект — одно каноническое состояние

Mobile показывает те же Project, Session, Task, Run, Inbox Card, Memory Item, Artifact и Permission Decision, что desktop и server.

Mobile не создаёт собственный параллельный «настоящий» статус.

Примеры:

- локальная кнопка `Pause` сначала показывает `Sending…`, а потом подтверждённое состояние Head;
- карточка Inbox, разрешённая на desktop, исчезает на телефоне после revision update;
- старый notification action получает ответ `Already resolved`, а не повторяет решение;
- cached Radar item всегда показывает время свежести;
- локальный offline draft не выдаётся за отправленное сообщение.

### 0.7. Recognition и proximity важнее recall

Частые действия находятся рядом с объектом:

- `Pause` рядом с Run;
- `Reply` рядом с communication card;
- `Open Diff` рядом с изменениями;
- `Add to Project` рядом с capture;
- `Continue on Desktop` рядом со сложным artifact;
- `Why?` рядом с любым непонятным состоянием;
- `Undo` в snackbar сразу после обратимого действия.

Пользователь не должен помнить, в каком из десяти меню находится нужная команда.

---

## 1. Область ответственности

### 1.1. Документ определяет

- мобильную информационную архитектуру;
- системные entry points;
- bottom navigation и global action surface;
- Home;
- Projects и project overview;
- mobile project chat;
- Orchestrator/Quick Chat;
- Action Inbox;
- Activity/Radar/notifications;
- Voice и ambient controls;
- Quick Capture и staging;
- mobile review изменений;
- Runs и Tasks;
- Memory и Artifacts на мобильном устройстве;
- упрощённое управление Capabilities и Automations;
- Devices, handoff, sync, backup и system status;
- settings, onboarding и account/device trust;
- widgets, shortcuts, Live Activities/Live Updates, share targets и notification actions;
- каждое основное меню, кнопку, swipe-action, long-press action и состояние;
- interruption/resumption logic;
- one-handed, walking, accessibility и reduced-attention modes;
- mobile-specific performance, usability и acceptance criteria.

### 1.2. Документ не определяет

- логику памяти — Memory Fabric;
- когда задача становится Task — Agentic Control;
- permission semantics — Trust Fabric;
- Voice turn-taking — Voice Fabric;
- provider/tool lifecycle — Capability Fabric;
- серверные очереди, sync и canonical state — Server Runtime;
- desktop layout — Desktop specification;
- визуальный бренд, финальные цвета и иллюстративный стиль;
- конкретный UI framework;
- конкретный push-provider;
- обязательность одной мобильной ОС.

### 1.3. Mobile не выдаёт себе скрытую власть

Приложение может:

- отправить command;
- показать pending state;
- запросить biometric step-up;
- кэшировать view-state;
- сохранить offline draft;
- предложить локальное действие;
- выполнить разрешённую локальную capability.

Приложение не может самостоятельно:

- превратить notification tap в permission;
- объявить Run завершённым;
- молча повторить external effect;
- повысить workspace trust;
- выдать grant из-за локального UI toggle;
- удалить memory только из локального cache;
- считать старое cached состояние актуальным;
- отправить partial voice transcript как committed command;
- выполнить queued consequential action после reconnect без revalidation.

---

## 2. Исследовательский протокол

### 2.1. План для построения плана

Перед проектированием были зафиксированы вопросы:

1. Какие действия действительно совершаются на ходу?
2. Что должно быть доступно без открытия приложения?
3. Какие решения можно безопасно принять из notification?
4. Как минимизировать resumption lag после прерывания?
5. Где voice быстрее touch, а где опаснее?
6. Что пользователь должен видеть сразу, а что раскрывать по запросу?
7. Какие desktop-функции mobile должен только перенаправлять?
8. Как работать при плохой сети и агрессивном background-management ОС?
9. Как не превратить уведомления в постоянное давление?
10. Как обеспечить one-handed и situational accessibility?
11. Какие функции должны быть локальными для мгновенной реакции?
12. Какие OS surfaces отличаются между Android и iOS?
13. Как не дублировать бизнес-логику Server, Trust и Voice?
14. Как проверять интерфейс не по красоте, а по времени до результата?

### 2.2. Обязательные сценарии

Проектирование считается неполным, если не покрывает:

1. посмотреть за 3–5 секунд, что требует внимания;
2. ответить на Inbox card из notification;
3. подтвердить действие с biometric step-up;
4. голосом узнать статус проекта;
5. сфотографировать объект и связать с проектом;
6. сохранить мысль при заблокированной или нестабильной сети;
7. поделиться ссылкой из браузера в Denet;
8. вернуться к прерванному project chat;
9. продолжить desktop session на телефоне;
10. вернуть её обратно на desktop;
11. посмотреть краткий diff и оставить комментарий;
12. остановить зависший Run;
13. увидеть, что произошло, пока пользователь был занят;
14. работать в общественном месте без раскрытия private data;
15. переключиться в hands-free/walking mode;
16. использовать приложение одной рукой;
17. получить critical notification без общего notification spam;
18. работать offline;
19. восстановить незавершённый capture после process death;
20. выполнить Emergency Stop;
21. открыть приложение из widget, shortcut, share sheet, deep link и notification;
22. разобраться, почему действие запрещено или ожидает;
23. увидеть, что данные stale;
24. не потерять approval или draft при системном interruption;
25. продолжить работу на foldable/tablet с двумя панелями.

### 2.3. Изученные reference-системы и исследования

Использовались:

- Android widgets, notifications, shortcuts, Quick Settings, share targets, adaptive layouts, biometric и background-work guidance; [[S10]]–[[S18]]
- Apple WidgetKit, ActivityKit, App Intents, User Notifications и Shortcuts как платформенные поверхности; [[S19]] [[S20]]
- GitHub Mobile как пример «high-impact work quickly from anywhere», а не полной desktop-копии; [[S21]]
- Notion widgets как пример прямых входов в recent/favorite/AI chat-camera-voice; [[S22]]
- Linear Inbox как пример локальных быстрых действий и snooze; [[S23]]
- ChatGPT Mobile Voice как пример background conversation, единой voice session, text/image coexistence и быстрого старта; [[S24]]
- исследования interruption/resumption, notification timing и notification suppression; [[S25]]–[[S29]]
- исследования one-handed interaction и adaptive handedness; [[S30]] [[S31]]
- WCAG и Android accessibility guidance. [[S32]] [[S33]]

### 2.4. Критерии принятия решения

Мобильный механизм принимается, если он:

- сокращает время до полезного результата;
- уменьшает taps или cognitive recall;
- сохраняет контекст при прерывании;
- не создаёт новый источник истины;
- работает при плохой сети или честно показывает ограничение;
- безопасно деградирует;
- доступен одной рукой или имеет альтернативу;
- не дублирует desktop без мобильной ценности;
- имеет понятный failure/recovery path;
- не требует постоянного LLM-вызова;
- не увеличивает notification pressure без измеримой пользы.

### 2.5. Критерии отказа

Механизм отклоняется или становится optional, если он:

- требует нескольких modal dialogs для частой операции;
- заставляет открывать app ради простого ответа;
- превращает каждый background event в push;
- прячет важный Stop или privacy toggle;
- вызывает accidental destructive action через swipe;
- делает long-press единственным способом найти команду;
- теряет draft при process death;
- показывает старое состояние без freshness;
- требует полного desktop knowledge;
- пытается редактировать сложный код на маленьком экране;
- добавляет floating button поверх всех приложений без явной пользы;
- автоматически меняет интерфейс по handedness без возможности отключить;
- использует Live Activity/Live Update для неактивной фоновой информации;
- требует always-on cloud connection для локального capture;
- скрывает цену, external effect или target в quick action.

---

## 3. Неподвижные мобильные UX-инварианты

1. Основная роль телефона — glance, capture, decision, voice и handoff.
2. Любой частый сценарий должен иметь путь не длиннее необходимого минимума.
3. Capturing сначала сохраняет, а классификация может происходить позже.
4. Inbox отделён от notifications.
5. Activity показывает существенное состояние, а не каждый tool call.
6. Любой consequential action показывает точный target и effect.
7. Notification action не считается завершённым до server acknowledgement.
8. Offline command явно маркируется как queued.
9. Partial transcript не является committed command.
10. Drafts сохраняются автоматически.
11. Возврат после прерывания показывает дельту, а не заставляет перечитывать всю историю.
12. Primary actions доступны в удобной для большого пальца зоне или через системную поверхность.
13. Destructive swipe требует вторичного действия или Undo.
14. Long-press никогда не является единственным путём к важной функции.
15. Любой badge имеет текстовое объяснение.
16. Stale, offline, pending и unknown различаются.
17. Healthy infrastructure не занимает главный экран.
18. Mobile может предложить handoff вместо неудобной сложной работы.
19. Voice и touch являются равноправными, но не взаимозаменяемыми во всех рисках.
20. Critical confirmation использует trusted system authentication.
21. App launch не должен блокироваться полной синхронизацией.
22. Экран не должен прыгать из-за streaming update, когда пользователь читает или нажимает.
23. Пользователь может отключить proactivity, ambient и smart reordering.
24. OS surfaces не раскрывают private project names без разрешения.
25. Emergency Stop доступен быстро, но защищён от случайного нажатия.
26. Приложение честно объясняет, что продолжает работать на сервере после его закрытия.
27. Mobile UI не изменяет канонический state без подтверждённой операции.
28. Любое автоматическое персонализированное упрощение обратимо.
29. Повторная команда с внешним эффектом требует reconciliation.
30. Каждое состояние имеет ясный next action или объяснение, почему действие недоступно.


# Часть II. Системные входы и глобальная оболочка

## 4. Entry Surfaces — Denet до открытия приложения

### 4.1. Общий принцип

Частая мобильная операция должна начинаться с ближайшей доступной поверхности, а не всегда с иконки приложения.

Каждая entry surface вызывает стабильный `command_id` и передаёт `entry_context`:

- source surface;
- device/session;
- timestamp;
- selected project, если закреплён;
- shared content;
- authentication state;
- requested action;
- current app/lock state.

### 4.2. App icon tap

Обычный tap открывает:

- последний безопасный destination, если возврат произошёл быстро;
- Home, если прежний экран устарел или был sensitive;
- Voice сразу, если пользователь включил `Start with Voice`;
- pending deep link, если app запущен внешним действием.

App icon long-press / launcher shortcuts:

- `Talk to Denet`;
- `Quick Capture`;
- `Action Inbox`;
- `Recent Project` или настраиваемый четвёртый shortcut.

Android launchers обычно показывают ограниченное число shortcuts, поэтому Denet не должен пытаться вместить туда весь продукт. [[S12]]

### 4.3. Widgets

Denet предлагает несколько widgets, каждый с configurable privacy mode.

#### Compact Quick Action

Кнопки:

- Voice;
- Capture;
- Inbox count;
- optional custom action.

#### Status Glance

Показывает:

- `Needs You` count;
- running count;
- last significant result;
- sync freshness;
- tap opens Home/Activity.

#### Project Widget

Пользователь выбирает один проект.

Показывает:

- project name или privacy alias;
- current session state;
- latest meaningful update;
- `Open`;
- `Talk`;
- `Capture to Project`.

#### Resume Widget

Показывает максимум три объекта:

- last session;
- pending Inbox card;
- recent artifact/review.

#### Voice/Capture Widget

Прямые действия:

- Chat;
- Camera;
- Voice note;
- Live voice.

Notion использует похожий принцип для прямого доступа к recent/favorite pages и AI chat-camera-voice. [[S22]]

Widget rules:

- только glanceable информация;
- no secrets;
- optional project aliases;
- stale timestamp;
- no destructive action без открытия/step-up;
- configurable refresh;
- graceful `Offline` state;
- tap target всегда имеет label/accessibility description.

### 4.4. Lock-screen widgets

Допустимые действия:

- Push-to-talk;
- Quick voice note;
- Privacy Mode;
- Inbox count без деталей;
- active voice/run indicator;
- Emergency Stop entry.

Sensitive details раскрываются только после unlock согласно Trust policy.

### 4.5. Live Activity / Android Live Update

Используется только для действительно активного, начатого пользователем процесса:

- активная Voice Session;
- Managed Run, который пользователь явно решил отслеживать;
- computer-use session;
- активная передача большого artifact;
- migration/restore, за которой нужно следить;
- пользовательский timer/scheduled execution.

Не используется для:

- общего количества агентов;
- постоянного мониторинга новостей;
- обычного unread Inbox;
- фоновых рекомендаций;
- upcoming events без активного периода;
- рекламы capabilities.

Отображает:

- короткое название;
- текущую фазу;
- elapsed time;
- meaningful progress;
- `Pause/Resume`;
- `Stop`;
- `Open`;
- optional `Unpin`.

Android прямо ограничивает Live Updates активными, user-initiated и time-sensitive процессами и рекомендует widgets/Quick Settings для быстрого доступа. [[S11]]

### 4.6. Notifications

Notification может иметь до трёх быстрых действий на Android; direct reply допускает текстовый ответ без открытия приложения. [[S13]]

Denet использует notification только для:

- срочного решения;
- завершения пользовательски ожидаемого результата;
- значимого failure;
- security/system события;
- явного reminder;
- communication, которую пользователь выбрал отслеживать.

Примеры действий:

- `Approve` / `Deny`;
- `Reply`;
- `Snooze`;
- `Pause`;
- `Stop`;
- `Open Result`;
- `Continue Voice`;
- `Send to Inbox`;
- `Mute Source`.

Ограничения:

- high-risk approval открывает trusted confirmation;
- exact parameters должны быть видимы;
- notification payload не является authority;
- action имеет idempotency key;
- stale action возвращает актуальное состояние;
- notification может быть скрыта системой, поэтому каноническая Inbox card сохраняется.

### 4.7. Quick Settings / Control Center

Подходящие быстрые toggles/actions:

- Ambient Listening On/Off;
- Privacy Mode;
- Push-to-talk;
- Pause Background Work;
- DND;
- Emergency Stop entry.

Не подходят:

- длинная история;
- complex settings;
- выбор проекта из десятков;
- destructive operations.

На locked device безопасный toggle может сработать сразу; небезопасный требует unlock. [[S14]]

### 4.8. Hardware and system shortcuts

Поддерживаемые mappings, если ОС/устройство позволяют:

- Action button → Push-to-talk или Quick Capture;
- double press / back tap → Voice note;
- headset button long-press → PTT;
- stylus button → screenshot/capture;
- wearable complication → Inbox/Voice;
- car integration → voice-only safe subset.

Каждый mapping:

- настраивается;
- показывает preview;
- имеет safe locked-device behavior;
- не раскрывает private content в metadata.

Apple Shortcuts демонстрирует, что действия могут запускаться через Siri, Control Center, Action button, widget, Home Screen, Search, Apple Watch и Back Tap; Denet должен публиковать небольшой набор стабильных App Intents вместо сотен узких shortcuts. [[S20]]

### 4.9. Share extension / Android share target

Denet принимает:

- text;
- URL;
- image;
- video;
- PDF/document;
- selected files;
- contact/card, если поддерживается;
- screenshot;
- app-specific object reference.

После выбора Denet открывает компактный Share Capture Sheet:

- preview;
- destination;
- optional comment;
- action;
- privacy;
- `Save`.

Пользователь может подтвердить и отредактировать shared content перед использованием, что соответствует Android guidance. [[S15]]

### 4.10. Deep links

Типы:

- project;
- session;
- Run;
- Inbox card;
- artifact;
- memory item;
- device handoff;
- voice continuation;
- automation/event;
- settings subsection;
- approval.

Deep link flow:

1. validate source and reference;
2. authenticate if needed;
3. fetch current revision;
4. show stale/missing state when necessary;
5. never auto-execute consequential action merely from URL.

---

## 5. Global Mobile Shell

### 5.1. Compact phone navigation

Default bottom navigation:

1. **Home**;
2. **Projects**;
3. central **Quick Action**;
4. **Inbox**;
5. **Activity**.

`Quick Action` — не destination, а action launcher.

Почему не отдельные tabs для Memory, Artifacts, Capabilities и Settings:

- они используются реже на ходу;
- они доступны через global search, project pages и Profile/More;
- пять постоянных позиций легче узнавать и нажимать одной рукой;
- mobile не должен копировать Activity Rail desktop.

### 5.2. Expanded/foldable navigation

На medium/expanded width:

- bottom bar превращается в navigation rail;
- list-detail screens используют две панели;
- можно закрепить дополнительный destination `Library`;
- Quick Action остаётся заметным, но не перекрывает контент.

Android adaptive navigation также переключает navigation bar и rail по размеру окна. [[S16]]

### 5.3. Top App Bar

Общие элементы:

- screen title или compact breadcrumbs;
- global search;
- system freshness/status icon;
- profile/avatar;
- contextual overflow.

Дополнительные contextual buttons:

- `Back`;
- `Filter`;
- `Sort`;
- `Refresh`;
- `Share`;
- `More`.

Top bar не должна содержать более двух-трёх постоянных icon actions кроме navigation; остальные уходят в overflow или bottom sheet.

### 5.4. Global Search / Command

Открывается:

- кнопкой поиска;
- swipe-down на Home, если включено;
- launcher shortcut;
- keyboard shortcut на tablet;
- voice command.

Search scopes:

- All;
- Projects;
- Sessions;
- Commands;
- Memory;
- Artifacts;
- Inbox;
- Runs;
- Capabilities;
- People/Connectors.

Result actions:

- Open;
- Open in Project;
- Ask Denet About This;
- Add to Current Context;
- Share;
- Continue on Desktop;
- Pin;
- Copy Link.

Command examples:

- Start Voice;
- New Capture;
- Pause All Background;
- Open Project…;
- Ask Orchestrator…;
- Privacy Mode;
- System Status;
- Emergency Stop;
- Switch Device;
- Sync Now.

### 5.5. Central Quick Action Button

Tap opens `Action Dock` bottom sheet.

Long-press starts default action, usually Push-to-talk.

Swipe/drag gestures are optional and disabled by default to avoid hidden interaction.

Action Dock primary row:

- `Talk`;
- `Text`;
- `Camera`;
- `Voice Note`.

Secondary actions:

- Screenshot/Screen Capture;
- Scan Document;
- Share Clipboard;
- New Task;
- New Project;
- Ask Memory;
- Privacy Mode;
- Custom pinned actions.

Header of sheet:

- current destination chip: `Auto`, project or personal memory;
- `Change`;
- connectivity icon;
- capture privacy icon.

### 5.6. Mini Voice Bar

Появляется при background Voice Session.

Показывает:

- speaking/listening/thinking state;
- active project or orchestrator;
- elapsed time;
- microphone state.

Buttons:

- Mute;
- Expand;
- End.

Swipe down hides visual bar, но не завершает Voice Session; persistent OS surface остаётся по policy.

### 5.7. Mini Run Bar

Показывается только для Run, который пользователь явно открыл/закрепил.

Содержит:

- title;
- phase;
- progress/freshness;
- `Open`;
- `Pause/Resume`;
- `Stop` in overflow.

Несколько Runs не создают несколько bars. Они группируются в `Running N`.

### 5.8. Back behavior

Back выполняет в порядке:

1. закрыть keyboard;
2. закрыть transient sheet/menu;
3. выйти из selection mode;
4. вернуться на предыдущий detail;
5. вернуться к предыдущему destination;
6. свернуть app.

Back не:

- отправляет draft;
- разрешает card;
- отменяет незавершённый upload без вопроса;
- завершает Voice Session без явного действия.

### 5.9. Bottom sheets и dialogs

Bottom sheet используется для:

- quick actions;
- filter/sort;
- contextual menu;
- target selection;
- short preview;
- snooze;
- handoff;
- lightweight settings.

Full-screen page используется для:

- long form;
- multi-step configuration;
- high-risk confirmation;
- detailed diff;
- permission explanation;
- restore/recovery.

Dialog используется редко:

- irreversible final confirmation;
- choosing between a small number of mutually exclusive outcomes;
- system authentication flow.

### 5.10. Snackbars и Undo

После обратимого действия показывается snackbar:

- outcome;
- target;
- `Undo`;
- optional `View`.

Примеры:

- card snoozed;
- project pinned;
- capture moved;
- notification muted;
- automation disabled;
- task archived.

Не используется как единственная фиксация consequential result.

---

## 6. Universal Mobile Interaction Rules

### 6.1. Thumb-zone policy

Основные частые действия находятся:

- в bottom navigation;
- в bottom action bar;
- в floating/central action button;
- в нижней части detail screen;
- в notification action.

Top bar оставляет navigation/search/overflow, но не единственный Send/Approve/Stop.

Исследование one-handed action-bar adaptations показало улучшение скорости, comfort и grip stability по сравнению с традиционным placement в части сценариев. [[S30]]

### 6.2. Handedness

Settings:

- Right-handed;
- Left-handed;
- Automatic suggestion;
- No adaptation.

Адаптироваться могут:

- position of Quick Action;
- swipe direction hints;
- bottom action order;
- optional edge controls.

Automatic handedness никогда не переставляет destructive action прямо перед tap и всегда имеет `Undo/Lock Layout`.

### 6.3. Swipe actions

Swipe используется только для частых обратимых операций.

Примеры:

- Inbox card: Snooze / Resolve with primary safe option;
- notification: Read / Mute;
- project: Pin / Archive;
- capture: Route / Archive;
- task: Done / Snooze.

Destructive delete:

- не выполняется одним мгновенным swipe;
- требует full swipe + Undo или second confirmation;
- отключается в On-the-Go mode.

### 6.4. Long-press

Long-press открывает context menu или shortcut, но важная функция всегда имеет видимый альтернативный путь.

Применение:

- project card;
- session;
- capture;
- widget configuration;
- bottom nav account switch;
- Quick Action default.

### 6.5. Pull-to-refresh

Разрешён на списках, где ручная свежесть полезна:

- Home;
- Projects;
- Inbox;
- Activity;
- Memory search.

Refresh:

- не стирает cached content;
- показывает last updated;
- не повторяет external effect;
- не перезапускает Run.

### 6.6. Selection mode

Multi-select доступен для:

- notifications;
- low-risk Inbox cards;
- captures;
- artifacts;
- tasks.

Selection bar actions:

- Move/Route;
- Snooze;
- Mark Read;
- Archive;
- Share;
- Delete, если допустимо;
- More.

High-risk approvals нельзя batch-resolve вместе с неоднородными действиями.

### 6.7. Haptics

Haptics используются для:

- PTT start/stop;
- successful capture;
- biometric success/failure;
- user takeover;
- destructive threshold;
- completed swipe;
- critical alert.

Они не должны вибрировать на каждый streaming token или status update.

### 6.8. Scroll stability

При streaming update:

- текущая позиция чтения сохраняется;
- новые элементы не сдвигают пальцем выбранную кнопку;
- появляется `New updates` chip;
- auto-scroll происходит только если пользователь уже был внизу;
- approval parameters freeze на время confirmation.

# Часть III. Основные мобильные рабочие пространства

## 7. Home — быстрый ответ на «что мне нужно сейчас?»

### 7.1. Назначение

Home не является рекламным dashboard и не пытается показать всё.

Он отвечает на пять вопросов:

1. Что требует моего решения?
2. Что сейчас работает?
3. Что изменилось с последнего визита?
4. К чему вероятнее всего нужно вернуться?
5. Можно ли системе доверять текущее состояние?

### 7.2. Home Header

Содержит:

- compact greeting или current context;
- Search;
- System status icon;
- Profile;
- overflow.

Overflow:

- Customize Home;
- Manage Sections;
- Set On-the-Go Layout;
- Refresh;
- Hide Greeting;
- Privacy Curtain;
- Home Settings.

### 7.3. Resume Capsule

Показывается первой, если есть незавершённый контекст.

Содержит:

- объект: project/session/review/voice/inbox;
- где пользователь остановился;
- elapsed time;
- `What changed` count;
- local draft presence;
- primary `Resume`;
- `Preview`;
- `Dismiss from Home`.

Resume не автоматически отправляет draft и не возобновляет paused external effect.

### 7.4. What Changed Since I Left

Показывает компактную дельту:

- agent completed/failed;
- new artifact;
- changed files summary;
- new question;
- permission request;
- project upstream change;
- new memory correction;
- sync conflict.

Buttons:

- Open All;
- Open First Important;
- Mark Seen;
- Send to Digest;
- Mute This Kind.

### 7.5. Needs You

До пяти наиболее важных открытых Inbox Cards.

Каждая card показывает:

- why now;
- project/source;
- exact question;
- primary safe action;
- expiry;
- risk indicator.

Quick actions:

- recommended option;
- Open;
- Discuss;
- Snooze;
- Delegate to Orchestrator.

`View All` открывает Inbox.

### 7.6. Running Now

Показывает только significant Sessions/Runs:

- active Voice Session;
- user-followed Managed Run;
- computer-use;
- upload/migration/restore;
- background project task with attention.

Item actions:

- Open;
- Pause/Resume;
- Stop;
- Pin Live Status;
- Handoff.

### 7.7. Recent Projects

До пяти:

- favorite;
- recently active;
- recently changed;
- likely next based on explicit use, not opaque manipulation.

Project actions:

- Open;
- Talk;
- Capture;
- More.

### 7.8. Completed

Краткие результаты с момента последнего визита:

- title;
- outcome;
- project;
- one-line result;
- `Open`;
- `Dismiss`;
- `Notify differently`.

Healthy background success может быть свернут в один digest-card.

### 7.9. Briefing

Optional:

- morning briefing;
- today’s priorities;
- pending promises;
- selected news/research;
- device/server issues.

Buttons:

- Read;
- Listen;
- Customize;
- Skip Today.

### 7.10. Home states

- First use;
- Empty healthy;
- Cached offline;
- Syncing;
- Needs attention;
- Critical system issue;
- Privacy curtain;
- On-the-Go compact;
- No notification permission;
- Head unavailable;
- Local emergency mode.

---

## 8. Orchestrator and Quick Chat

### 8.1. Роль

Мобильный Orchestrator нужен для:

- короткого вопроса;
- поручения;
- проверки статуса;
- выбора следующего действия;
- обсуждения Inbox card;
- создания проекта;
- управления системой голосом или текстом.

Он не заменяет project chat, если работа явно относится к проекту.

### 8.2. Entry points

- Home `Ask Denet`;
- Quick Action `Text`;
- bottom navigation deep link;
- notification `Discuss`;
- Inbox `Ask Orchestrator`;
- global search command;
- widget;
- voice transcript switch to text.

### 8.3. Header

- `Denet`;
- current focus chip;
- model/provider indicator, compact;
- Voice;
- overflow.

Overflow:

- New Focus;
- Attach Current Screen;
- Choose Project Context;
- Execution Profile;
- Model;
- View Context;
- Clear Temporary Focus;
- Export Conversation;
- Open on Desktop.

### 8.4. Composer

Elements:

- text field;
- add/attach;
- project/context chip;
- microphone/dictation;
- send;
- background-send secondary action;
- stop if generating.

Add menu:

- Photo/Camera;
- File;
- Screenshot;
- Clipboard;
- Link;
- Memory item;
- Artifact;
- Project;
- Current location only when explicitly useful/allowed;
- Scan document.

### 8.5. Send semantics

Default `Send` starts a Direct Turn or continues the session.

Secondary send menu:

- Send;
- Send and Work in Background;
- Create Task;
- Create Project from This;
- Ask Without Saving to Memory, where supported;
- Use Local Model;
- Schedule/Remind.

During active work:

- Add to Queue;
- Steer;
- Stop and Send.

### 8.6. Response blocks

- normal text;
- short status;
- options;
- artifact preview;
- project suggestion;
- tool/action proposal;
- memory result;
- permission request;
- background task card;
- error/recovery card.

Actions on response:

- Copy;
- Listen;
- Continue Voice;
- Save Artifact;
- Add to Project;
- Create Task;
- Share;
- Correct;
- Explain Sources;
- More.

### 8.7. Interruption safety

- text draft autosaves locally;
- attachments remain staged;
- active model response may continue on server;
- returning user sees `New response ready` and summary;
- if user edits an old message, a branch is created;
- no half-sent command after app kill;
- queued message shows `Not sent yet` until Head acknowledges.

### 8.8. Quick Chat

Quick Chat is temporary:

- no forced project selection;
- short retention policy visible;
- can be promoted to orchestrator history, project session, note or Task;
- closing offers `Save`, `Discard`, `Keep as temporary` only if there is meaningful content.

---

## 9. Projects Hub

### 9.1. Header

- Projects title;
- Search;
- Filter;
- Sort;
- Add;
- overflow.

Add:

- New Project;
- Import Existing;
- Connect Repository;
- Create from Description;
- Create from Capture;
- Join/Import Portable Project Pack;
- Scan Nearby/Device Projects, if allowed.

Overflow:

- Manage Groups;
- Show Archived;
- Missing Projects;
- Sync Projects;
- Default Project Settings;
- Open Projects on Desktop.

### 9.2. Views

Phone default:

- Compact List;
- Cards;
- Favorites;
- Recent.

Tablet/foldable:

- list-detail;
- optional grid.

### 9.3. Filters

- Active;
- Needs You;
- Running;
- Recent;
- Favorites;
- Local on Device;
- Available Offline;
- Missing Capability;
- Sync Issue;
- Archived.

### 9.4. Sort

- Recent activity;
- Manual;
- Name;
- Needs attention;
- Running;
- Last opened;
- Last changed.

### 9.5. Project row/card

Displays:

- icon/name;
- privacy alias if enabled;
- current status;
- last meaningful update;
- active session/run count;
- pending decision count;
- offline availability;
- sync freshness.

Primary tap: Open Project Overview.

Visible quick actions:

- Talk;
- Capture;
- Resume.

Overflow:

- Favorite/Unfavorite;
- New Session;
- Ask Orchestrator About Project;
- View Runs;
- View Artifacts;
- Download for Offline;
- Continue on Desktop;
- Share/Export Pack;
- Settings;
- Archive;
- Remove Local Copy;
- Delete Project, if permitted.

### 9.6. Project import mobile flow

Mobile should support lightweight import, not full repository configuration.

Steps:

1. select folder/repository/link/pack;
2. show detected identity and source;
3. show trust state;
4. show missing capabilities;
5. choose local availability;
6. choose `Open Restricted`, `Trust Bounded`, or defer;
7. finish or hand off advanced setup to desktop.

### 9.7. Missing project

States:

- path unavailable;
- device offline;
- repository not cloned;
- permission denied;
- project deleted upstream;
- portable pack only;
- cached metadata only.

Actions:

- Locate;
- Clone/Download;
- Connect Device;
- Open Cached Memory;
- Handoff to Device;
- Remove from List;
- Diagnose.

---

## 10. Project Overview

### 10.1. Header

- Back;
- project name/privacy alias;
- status/freshness;
- Voice;
- overflow.

Overflow:

- New Session;
- Project Search;
- Add Capture;
- Download Offline;
- Handoff;
- Share;
- Project Settings;
- Open on Desktop;
- Archive.

### 10.2. Primary actions

Bottom/near-thumb action row:

- `Resume Chat`;
- `Talk`;
- `Capture`;
- `More`.

If no session exists:

- `Start Chat`;
- `Ask for Overview`;
- `Create First Task`.

### 10.3. Project summary

Shows:

- purpose;
- current focus;
- last decision;
- active branch/worktree/device;
- current direct-chat model;
- trust mode;
- last sync.

Expandable `Why/Details` opens sources and authoritative state.

### 10.4. Needs You

Project-scoped Inbox cards with inline primary safe action.

### 10.5. Running

Project-scoped sessions/runs:

- Open;
- Pause;
- Stop;
- Handoff;
- View Result.

### 10.6. Changes

Compact card:

- files changed;
- additions/deletions;
- tests;
- conflicts;
- `Review`;
- `Ask Agent to Summarize`.

### 10.7. Artifacts

Recent artifacts with preview and `Open`/`Share`/`Continue on Desktop`.

### 10.8. Memory

Recent decisions, current state and project notes.

Quick actions:

- Ask Project Memory;
- New Note;
- View Current State;
- Open Full Memory.

### 10.9. Sessions

Horizontal recent list or compact section:

- active;
- waiting;
- completed;
- archived.

Actions:

- Open;
- New;
- Rename;
- Pin;
- Archive;
- Fork;
- Handoff.

### 10.10. Project tabs/sections

On phone, sections are accessible through segmented control or `Project Menu`:

- Overview;
- Chat;
- Runs;
- Changes;
- Artifacts;
- Memory;
- Tasks;
- More.

`More`:

- Capabilities;
- Events;
- Files summary;
- Settings;
- Activity.

No horizontal tab strip with ten cramped labels.

---

## 11. Mobile Project Chat

### 11.1. Header

- Back;
- session title;
- project chip;
- current state;
- Voice;
- overflow.

State tap opens detail:

- agent/model;
- branch/worktree;
- task/run;
- last progress;
- trust;
- device;
- context freshness.

Overflow:

- Session List;
- New Session;
- Rename;
- Pin;
- Fork;
- Checkpoint;
- Project Overview;
- View Changes;
- View Context;
- Execution Profile;
- Model;
- Move to Background;
- Handoff;
- Open on Desktop;
- Archive;
- Export.

### 11.2. Message list

Blocks:

- user message;
- agent response;
- tool progress summary;
- action proposal;
- artifact;
- diff summary;
- test result;
- checkpoint;
- permission/Inbox link;
- error/recovery;
- handoff marker.

Mobile collapses verbose tool chatter by default.

`Show details` reveals:

- tools;
- files;
- logs;
- evidence;
- provider data.

### 11.3. Composer anatomy

- multiline text;
- Add;
- context chip;
- dictation/PTT;
- Send;
- active-work mode button.

Context chip examples:

- Auto;
- Current File;
- Changes;
- Artifact;
- Memory;
- Screenshot;
- Selected Run.

### 11.4. Add Context menu

- Camera/Photo;
- File;
- Screenshot;
- Clipboard;
- Link;
- Artifact;
- Memory Item;
- Run;
- Diff/File;
- Location, explicit;
- Scan document;
- Share current app content.

### 11.5. During agent work

Bottom controls:

- `Add to Queue`;
- `Steer`;
- `Stop`;
- `Background`;
- `More`.

More:

- Pause after current step;
- Change priority;
- Change budget;
- Ask for status;
- Open Radar;
- Continue Voice;
- Move execution.

### 11.6. Progress presentation

Collapsed:

- phase;
- elapsed;
- last meaningful progress;
- files/tools count;
- current blocker.

Expanded:

- timeline;
- tool calls;
- logs;
- artifacts;
- context;
- budget;
- evidence.

### 11.7. Response actions

On tap/long-press:

- Copy;
- Listen;
- Reply;
- Quote;
- Save as Artifact;
- Add to Memory;
- Create Task;
- Share;
- Correct;
- Branch from Here;
- View Sources;
- Report Problem.

### 11.8. Composer draft behavior

Draft saves:

- locally after short debounce;
- with session ID;
- attachments as staged references;
- voice transcript as editable draft when using dictation;
- across app restart;
- to server when allowed and connected.

Draft states:

- Local Draft;
- Syncing Draft;
- Synced Draft;
- Conflict;
- Attachment Missing;
- Sensitive Local Only.

### 11.9. Mobile-friendly command patterns

Quick chips may appear contextually:

- Explain simpler;
- Summarize status;
- Run tests;
- Show changes;
- Continue;
- Fix issue;
- Prepare options;
- Work in background.

Chips are suggestions, not mandatory fixed workflow.

### 11.10. Chat states

- Idle;
- Typing;
- Voice dictation;
- Sending;
- Queued offline;
- Agent working;
- Agent speaking;
- Waiting input;
- Waiting permission;
- Background;
- Paused;
- Completed;
- Partial;
- Failed;
- Stale session;
- Provider unavailable;
- Device unavailable;
- Context conflict;
- Archived;
- Read-only imported.

---

## 12. Mobile Changes and Review

### 12.1. Principle

Mobile review is **summary-first and decision-oriented**.

It should let the user:

- понять, что изменилось;
- увидеть риск;
- проверить ключевые участки;
- оставить comments;
- запросить исправление;
- принять bounded change;
- перенести полный review на desktop.

Mobile не должен рекламировать себя как полноценный IDE editor.

### 12.2. Review header

- Back;
- Changes count;
- branch/worktree;
- tests state;
- Search;
- overflow.

Overflow:

- Refresh;
- View Commit History;
- Compare Checkpoint;
- Share Review Link;
- Open on Desktop;
- Export Patch;
- Discard Review Draft.

### 12.3. Summary card

- agent explanation;
- files changed;
- additions/deletions;
- tests run/not run;
- known risks;
- unreviewed areas;
- conflicts;
- source Task/Run.

Buttons:

- Review Files;
- Ask Question;
- Request Changes;
- Run Tests;
- Open on Desktop.

### 12.4. File list

Each item:

- path;
- status;
- line counts;
- review status;
- comments;
- risk/importance;
- binary/large marker.

Swipe actions:

- Mark Reviewed;
- Add Comment;

Long-press:

- Open;
- Ask Agent About File;
- Revert File;
- Share;
- Open on Desktop.

### 12.5. Diff viewer

Modes:

- Unified default;
- Side-by-side on tablet;
- Summary only;
- Rendered preview where applicable.

Toolbar:

- previous/next change;
- file selector;
- line numbers;
- comments;
- whitespace toggle;
- wrap lines;
- ask agent;
- overflow.

Selection actions:

- Comment;
- Ask Agent;
- Copy;
- Revert Hunk;
- Add to Review Summary.

### 12.6. Bottom decision bar

Context-dependent:

- Request Changes;
- Approve Bounded Change;
- Merge/Commit, only when policy permits;
- Keep Isolated;
- Open on Desktop.

Consequential integration always shows exact branch/target.

### 12.7. Tests

- Not Run;
- Running;
- Passed;
- Failed;
- Partial;
- Stale relative to commit;
- Unavailable on current device.

Actions:

- Run Focused;
- Run Full on Server;
- View Failures;
- Ask Agent to Fix;
- Open Logs;
- Continue on Desktop.

### 12.8. Review interruption

- current file/line/scroll preserved;
- unsubmitted comments saved;
- reviewed markers sync;
- returning user gets `N new changes since review started`;
- approval invalidates if diff revision changed;
- app offers `Rebase Review`, `Compare`, or `Restart Review`.

---

## 13. Runs and Tasks

### 13.1. Runs list

Filters:

- Running;
- Needs You;
- Waiting;
- Completed;
- Failed;
- Followed;
- Project;
- Device;
- Provider.

Sort:

- Attention;
- Recent;
- Started;
- Duration;
- Project;
- Cost/budget warning.

### 13.2. Run row

- title;
- project;
- phase;
- status;
- last meaningful progress;
- device/provider;
- budget;
- freshness.

Quick actions:

- Open;
- Pause/Resume;
- Stop;
- Follow/Unfollow.

### 13.3. Run detail

Sections:

- Summary;
- Timeline;
- Artifacts;
- Evidence;
- Logs;
- Budget;
- Permissions;
- Context;
- Recovery.

Primary action bar:

- Steer;
- Pause/Resume;
- Stop;
- Open Session;
- More.

More:

- Change Priority;
- Change Budget;
- Move Execution;
- Retry;
- Reconcile;
- Compare Attempt;
- Create Follow-up;
- Handoff;
- Share Status.

### 13.4. Task list

Mobile default is list, not Kanban.

Task item:

- title;
- project;
- owner;
- status;
- due/trigger;
- attention;
- linked Run.

Actions:

- Open;
- Complete;
- Start;
- Assign to Denet;
- Snooze;
- Edit;
- Archive.

Optional board on tablets or when user explicitly enables it.

### 13.5. Unknown external effect

Mobile must prominently show:

- what may have happened;
- why status is unknown;
- exact target;
- last provider evidence;
- `Reconcile`;
- `Do Not Retry`;
- `Contact/Check Manually`;
- `Open Details`.

There is no generic `Retry` as primary action.

---

## 14. Quick Capture and Staging

### 14.1. Capture-first principle

User should be able to capture in one action and leave immediately.

Classification, OCR, project linking and enrichment may happen later.

### 14.2. Capture types

- Text Note;
- Voice Note;
- Live thought stream;
- Camera Photo;
- Existing Photo;
- Document Scan;
- Screenshot;
- Screen Recording snippet, if permitted;
- Clipboard;
- Link;
- Shared file;
- Location/event marker, explicit;
- Contact/message reference;
- Handwritten/stylus note on supported device.

### 14.3. Minimal Capture Sheet

Fields:

- preview/input;
- destination chip;
- optional comment;
- privacy/local-only;
- `Save`.

Secondary:

- Save and Create Task;
- Save and Ask Denet;
- Save and Start Project;
- Edit Details;
- Discard.

### 14.4. Destination

- Auto;
- Personal Inbox/Staging;
- Current Project;
- Recent Project;
- Memory Space;
- Existing Task;
- New Project.

Auto-routing never prevents immediate save. If confidence low, capture goes to Staging.

### 14.5. Voice Note

Controls:

- record;
- pause;
- stop/save;
- discard;
- mark important;
- choose destination.

After recording:

- save immediately;
- transcription can continue in background;
- user can leave;
- notification only if transcription fails materially or user requested completion notice.

### 14.6. Camera capture

Controls:

- shutter;
- flash;
- switch camera;
- document mode;
- add voice comment;
- multi-shot;
- privacy indicator.

After capture:

- retake;
- crop/rotate;
- redact;
- add comment;
- save;
- ask Denet;
- link project.

### 14.7. Share Capture

When content arrives from another app:

- preview it;
- allow editing text/comment;
- show source app/URL;
- choose destination;
- choose `Save`, `Ask`, `Create Task`, `Add to Project`;
- validate unsupported/missing content;
- preserve source provenance.

### 14.8. Staging Inbox

Unsorted captures list:

- preview;
- source;
- timestamp;
- inferred type;
- suggested project;
- processing state.

Actions:

- Accept Routing;
- Change Destination;
- Ask Denet;
- Create Task;
- Merge;
- Archive;
- Delete;
- Keep Local.

Batch actions:

- Route;
- Archive;
- Delete;
- Process Later.

### 14.9. Capture states

- Recording;
- Paused;
- Saving locally;
- Saved locally;
- Sync queued;
- Uploading;
- Processing;
- Transcribing;
- OCR;
- Routed;
- Needs review;
- Failed;
- Partial;
- Local-only;
- Sensitive;
- Duplicate candidate;
- Storage low.

### 14.10. Process death recovery

If app is killed:

- audio segment finalized when possible;
- temp media indexed in local recovery journal;
- unfinished text draft restored;
- next launch shows `Recovered Capture`;
- no automatic upload if privacy state uncertain;
- user can Save, Continue, Delete.

# Часть IV. Решения, внимание, голос и управление системой

## 15. Action Inbox

### 15.1. Роль

Action Inbox — долговечная очередь ситуаций, где Denet действительно нуждается в выборе пользователя или не имеет права безопасно продолжить сам.

Он не является:

- notification feed;
- backlog задач;
- историей всех событий;
- списком каждого вопроса агента;
- журналом ошибок.

### 15.2. Inbox header

- title;
- unresolved count;
- `Resolve Next`;
- Search;
- Filter;
- overflow.

Overflow:

- Sort;
- Saved Views;
- Show Snoozed;
- Show Resolved;
- DND/Attention Settings;
- Inbox Rules;
- Open on Desktop.

### 15.3. Default filters

- All Open;
- Urgent;
- Permissions;
- Choices;
- Messages;
- Reviews;
- Conflicts;
- Failed/Stalled;
- Project;
- Expiring Soon.

### 15.4. Inbox list item

Shows:

- icon/type;
- concise question;
- project/source;
- urgency;
- expiry;
- primary recommended safe option;
- risk;
- freshness.

Tap opens detail.

Swipe actions:

- right: primary safe action if reversible and homogeneous;
- left: Snooze;
- full swipe destructive disabled by default.

Long-press:

- Open;
- Snooze;
- Discuss;
- Delegate to Orchestrator;
- Mute Similar;
- Copy Link.

### 15.5. Card detail anatomy

1. **Question** — что требуется.
2. **Why now** — почему вопрос появился.
3. **Exact target/effect** — для consequential actions.
4. **Recommendation** — что предлагает Denet и с какой уверенностью.
5. **Options** — варианты с последствиями.
6. **Evidence/context** — раскрываемо.
7. **Related process** — Session/Run/project.
8. **Expiry/freshness**.
9. **Action bar**.

### 15.6. Standard actions

- primary option;
- secondary option;
- Edit/Custom Response;
- Discuss;
- Snooze;
- Delegate to Orchestrator;
- Pause Related Process;
- Open Related Object;
- More.

More:

- Explain Risk;
- Explain More;
- View Sources;
- Resolve on Desktop;
- Copy Link;
- Mute Similar;
- Close if no longer relevant;
- Report Incorrect Card.

### 15.7. Permission card

Buttons:

- Allow Once;
- Allow for This Run;
- Allow Bounded Pattern;
- Allow Until Time;
- Prepare Only;
- Edit Scope;
- Deny;
- Deny and Remember;
- Ask Orchestrator;
- Pause.

Before confirmation shown:

- actor;
- exact operation;
- resource/recipient;
- effects;
- expiry;
- reversibility;
- authentication requirement;
- requested grant scope.

High-risk action uses system biometric/device credential. Android recommends system biometric prompt for re-authorization; the trusted system dialog gives a consistent experience. [[S17]]

### 15.8. Choice card

For variants/design/strategy:

- visual previews;
- concise trade-offs;
- Denet recommendation;
- compare;
- select;
- combine/custom;
- ask for more options;
- discuss voice/text;
- open on desktop.

### 15.9. Communication card

Shows independently:

- recipient/account;
- thread context;
- content draft;
- disclosure summary;
- send timing;
- attachments;
- confidence.

Buttons:

- Send;
- Edit;
- Reply with Voice;
- Send Later;
- Do Not Send;
- Always Allow Bounded Pattern, if eligible;
- Open Thread.

### 15.10. Review card

- result summary;
- artifact/diff preview;
- tests/evidence;
- primary `Review`;
- Accept;
- Request Changes;
- Continue on Desktop;
- Discuss.

### 15.11. Conflict card

- objects/versions involved;
- why auto-merge failed;
- previews;
- choose version;
- merge manually;
- keep both;
- defer;
- open desktop.

### 15.12. Failure/Stall card

- what failed;
- partial progress;
- preserved artifacts;
- likely cause;
- safe next options.

Buttons:

- Retry Safely;
- Use Fallback;
- Continue from Checkpoint;
- Ask Agent;
- Open Logs;
- Stop;
- Resolve on Desktop.

### 15.13. Snooze

Quick values:

- 15 minutes;
- 1 hour;
- Tonight;
- Tomorrow;
- Next weekday;
- When I’m at Desktop;
- When Project Changes;
- Custom;
- Until Event.

Snooze hides card until condition, but does not change underlying permission/process.

Linear demonstrates the value of hiding a notification until a selected time and allowing it to reappear later. [[S23]]

### 15.14. Batch operations

Allowed for homogeneous low-risk cards:

- Snooze;
- Delegate to Orchestrator;
- Close obsolete;
- Apply same bounded decision;
- Mark reviewed.

Not allowed for mixed:

- payments;
- different recipients;
- destructive operations;
- unrelated permission scopes.

### 15.15. Inbox states

- Open;
- Urgent;
- Expiring;
- Awaiting authentication;
- Resolving;
- Resolved;
- Resolved elsewhere;
- Snoozed;
- Superseded;
- Withdrawn;
- Expired;
- Stale;
- Offline cached;
- Conflict revision;
- Related process ended;
- Policy changed;
- Unknown effect.

### 15.16. Resolve Next mode

Displays one card at a time with:

- full context;
- large thumb-friendly actions;
- swipe to next only after explicit skip;
- progress count;
- exit anytime;
- no gamification pressure.

---

## 16. Activity — Radar, Notifications and Recent Outcomes

### 16.1. Why one mobile Activity destination

На телефоне отдельные глобальные destinations для Radar, Notifications, History и System Events создают лишнюю навигацию.

`Activity` объединяет их как views одного времени-ориентированного пространства, но сохраняет различие типов.

Top segments:

- Now;
- Notifications;
- History.

### 16.2. Now

Lanes:

- Needs You link to Inbox;
- Running;
- Waiting;
- Stalled/Failed;
- Recently Completed;
- System/Devices.

Item actions:

- Open;
- Pause/Resume;
- Stop;
- Follow;
- Handoff;
- More.

### 16.3. Compact Radar item

- object type;
- project;
- title/goal;
- state;
- last meaningful progress;
- freshness;
- device/provider;
- blocker/risk.

Tap opens detail.

### 16.4. Radar detail

Sections:

- Overview;
- Timeline;
- Artifacts;
- Evidence;
- Resources;
- Permissions;
- Logs;
- Recovery.

Actions:

- Open Session;
- Steer;
- Pause/Resume;
- Stop;
- Stop after Current Step;
- Change Priority;
- Change Budget;
- Move Execution;
- Retry/Reconcile;
- Handoff;
- Share Status.

### 16.5. Notifications view

Categories:

- Completed;
- Failed;
- Mentions/Messages;
- System;
- Security;
- Updates;
- Usage;
- Background;
- Informational.

Actions:

- Open;
- Mark Read/Unread;
- Mark All Read;
- Mute Source;
- Snooze;
- Send to Inbox;
- Clear;
- Settings.

Notification is awareness. Inbox is unresolved decision.

### 16.6. History

Shows significant events:

- Run outcomes;
- decisions;
- project changes;
- handoffs;
- captures;
- sync conflicts;
- security actions;
- automation firings.

Filters:

- Project;
- Type;
- Actor;
- Device;
- Date;
- Outcome.

Actions:

- Open Object;
- Explain;
- Create Follow-up;
- Add to Memory;
- Share;
- Repeat Safely, when allowed.

### 16.7. Office view on mobile

Not a default destination.

Available as optional read-only/limited visualization:

- pinch/pan;
- tap agent/project;
- open details;
- pause/stop;
- no complex room editing.

If it does not improve mobile understanding, List/Timeline remains default.

### 16.8. Activity states

- Live;
- Cached;
- Stale;
- Offline;
- Head unavailable;
- No significant activity;
- Too many events grouped;
- Privacy hidden;
- Partial device visibility;
- History loading;
- Reconnecting stream.

---

## 17. Voice on Mobile

### 17.1. Entry modes

- Tap Voice;
- Long-press Quick Action;
- Push-to-talk;
- Live Voice;
- Continue recent Voice Session;
- Start in Project;
- Notification action;
- widget/shortcut;
- headset/wearable;
- wake-word, if enabled;
- Car mode, late/optional.

### 17.2. Voice start sheet

If context ambiguous:

- Denet/Orchestrator;
- Current Project;
- Recent Project;
- Meeting;
- Quick Note;
- Continue Last.

User can set default and skip sheet for fast launch.

### 17.3. Full Voice screen

Elements:

- active owner/project;
- listening/speaking/thinking state;
- live transcript preview;
- compact visual response/artifact;
- microphone;
- keyboard/text input;
- camera/image;
- More;
- End.

More:

- Push-to-talk / Live;
- Meeting mode;
- Mute output;
- Switch speaker/headphones;
- Share screen/camera where backend supports;
- Background conversation;
- Transfer device;
- Voice settings;
- Privacy mode;
- Report issue.

### 17.4. Compact Voice controls

- Mute/Unmute;
- Interrupt/Stop speaking;
- End;
- Text;
- Expand.

### 17.5. Background Voice

When allowed:

- persistent OS indicator;
- lock-screen controls;
- clear mic state;
- optional transcript hidden on lock screen;
- notification/live activity with End/Mute/Open;
- battery/network warning;
- no hidden switch to another conversational owner.

ChatGPT Voice provides a practical example of continuing a conversation while other apps are used or the phone is locked; Denet applies this through its own Voice/Trust/Server contracts. [[S24]]

### 17.6. Voice and project actions

For simple read/status:

- answer directly from current state.

For project modification:

- create Intent Proposal;
- route to project agent/orchestrator;
- show action state;
- allow background continuation.

For consequential action:

- show exact confirmation on trusted mobile UI;
- voice can discuss, but final step-up follows Trust.

### 17.7. Voice interruption and app interruption

If user receives call or app loses audio focus:

- pause/mute output;
- preserve spoken-output commitment;
- mark session interrupted;
- offer Resume;
- do not assume user heard queued speech;
- background tasks continue only if independent.

### 17.8. Voice notification/privacy

Public environment controls:

- answer visually;
- whisper/quiet voice where supported;
- request headphones;
- hide sensitive transcript;
- neutral lock-screen label;
- temporary private mode.

### 17.9. Voice states

- Idle;
- Starting;
- Listening;
- Detecting turn;
- User speaking;
- Processing;
- Waiting sidecar;
- Agent speaking;
- Interrupted;
- Muted;
- Background;
- Handoff pending;
- Reconnecting;
- Local fallback;
- Offline limited;
- Permission step-up;
- Ended;
- Failed;
- Recovered.

---

## 18. Handoff and Cross-Device Continuity

### 18.1. Handoff entry points

- object overflow;
- Voice screen;
- Run detail;
- Project Overview;
- notification;
- desktop `Continue on Phone`;
- device screen.

### 18.2. Handoff sheet

Shows compatible devices:

- current online state;
- capabilities;
- trust;
- battery/network, if relevant;
- last seen;
- whether object is locally available.

Actions:

- Open View on Device;
- Continue Voice;
- Transfer Control;
- Move Execution;
- Send Link Only;
- Download for Offline.

### 18.3. Explicitly show what moves

- view only;
- conversational control;
- voice media stream;
- Task/Run execution;
- project files;
- artifact;
- no data, only deep link.

### 18.4. Continue on Desktop

Mobile may package:

- current object;
- scroll/selection;
- draft;
- review comments;
- context handles;
- intended next action.

Desktop opens Resume Capsule rather than generic Home.

### 18.5. Continue on Phone

From desktop:

- push notification/deep link;
- current session summary;
- exact object;
- stale check;
- biometric if sensitive;
- optional local download.

### 18.6. Handoff failure

States:

- device offline;
- incompatible capability;
- project not available;
- trust insufficient;
- sync lag;
- voice backend unavailable;
- transfer timed out;
- source device disconnected.

Recovery:

- send link;
- use server execution;
- download required data;
- retry;
- keep on current device;
- open diagnostics.

---

## 19. System Status, Devices, Sync and Offline

### 19.1. System status entry

Available from:

- top status icon;
- Profile/More;
- Activity System lane;
- notification;
- Quick Settings tile;
- widget state.

### 19.2. Compact status sheet

Shows:

- Head Runtime;
- connection;
- sync freshness;
- offline queue;
- active voice;
- critical backup status;
- provider warning;
- local storage/battery warning.

Actions:

- Sync Now;
- View Devices;
- View Queue;
- Pause Background;
- Privacy Mode;
- Diagnostics;
- Emergency Stop.

### 19.3. Devices list

Each device:

- name/type;
- online/last seen;
- trust;
- capabilities summary;
- active work;
- sync;
- battery/network optional;
- head eligibility.

Actions:

- Open Detail;
- Send/Handoff;
- Ring/Locate, if explicitly supported;
- Rename;
- Revoke;
- Forget;
- Set Offline Data;
- Diagnostics.

### 19.4. Device detail

Sections:

- Identity/Trust;
- Capabilities;
- Active Sessions/Runs;
- Sync;
- Offline storage;
- Voice/Ambient;
- Security events;
- Diagnostics.

### 19.5. Sync screen

Shows by class:

- Memory Events;
- Notes;
- Projects;
- Artifacts;
- Captures/media;
- Settings;
- Offline commands.

States:

- Up to date;
- Syncing;
- Queued;
- Waiting Wi-Fi;
- Waiting charging;
- Conflict;
- Error;
- Paused;
- Local only;
- Server unavailable.

Actions:

- Sync Now;
- Pause/Resume;
- Retry;
- View Conflict;
- Change network policy;
- Keep Local;
- Remove Local Copy.

### 19.6. Offline queue

Each item:

- command/capture type;
- created time;
- destination;
- risk;
- current validity;
- required revalidation;
- size.

Actions:

- Send Now;
- Edit;
- Cancel;
- Keep Draft;
- Re-route;
- Delete.

Consequential commands never auto-send after reconnect if parameters or authorization may be stale.

### 19.7. Offline project mode

Supports:

- cached project overview;
- local files, if downloaded;
- local model/tools;
- drafts;
- captures;
- queued commands;
- local memory overlay.

Clearly unavailable:

- fresh global permissions;
- server-only provider sessions;
- remote artifacts not cached;
- exact global state;
- certain external actions.

### 19.8. Backup status

Mobile shows:

- last verified backup;
- current warning;
- recovery keys state;
- restore drill status;
- `View Details`;
- `Run Backup Now`, if policy permits;
- `Verify`;
- `Open on Desktop`.

Mobile does not expose a full restore by accidental tap; restore requires dedicated flow and strong authentication.

---

## 20. Privacy, Safety and Emergency Controls

### 20.1. Privacy Mode

Quick toggles:

- Ambient Off;
- Microphone Off;
- Screen Capture Off;
- Local Only;
- Hide Sensitive UI;
- DND;
- Duration.

Durations:

- Until turned off;
- 15 min;
- 1 hour;
- Until location/event, if user configured;
- Until device unlock/restart, optional.

### 20.2. Privacy Curtain

Hides:

- project names;
- personal contacts;
- private memory;
- notification content;
- billing/usage;
- secret values;
- sensitive captures.

It is UI privacy for public/screen-share situations, not cryptographic access control.

### 20.3. Emergency Stop

Entry:

- System Status;
- Quick Settings/Control Center;
- lock-screen safe entry;
- widget;
- hardware shortcut;
- voice local command.

Flow:

1. clear, unmistakable Emergency Stop screen;
2. user chooses scope or `Stop All New Effects`;
3. biometric/device credential if required by current state;
4. immediate local stop signal;
5. server revoke/pause;
6. summary of what stopped and what remains unknown.

Options:

- Stop all new Agent Runs;
- Pause consequential Runs;
- Block external sends/effects;
- Stop computer-use;
- End Voice/Ambient;
- Lock Secret Broker;
- Revoke temporary grants;
- Preserve state for recovery.

No data deletion is performed.

### 20.4. Lost phone / remote revoke

Mobile device can be:

- revoked from another trusted device;
- locally locked;
- stripped of offline private cache;
- prevented from sending queue;
- required to re-pair.

### 20.5. On-the-Go mode

Manual profile optimized for walking/standing/transit:

- larger actions;
- voice-first;
- compact summaries;
- no tiny diff controls;
- destructive actions hidden behind More;
- reduced animation;
- fewer notification details;
- one-tap Save/Continue Later;
- high contrast/haptic feedback;
- optional auto-read brief output through headphones.

System may suggest it based on explicit context, but never silently reduce functionality without a visible indicator.

### 20.6. Driving/Car mode

Late/optional, voice-only safe subset:

- ask status;
- capture note;
- listen to briefing;
- pause/stop Run;
- send preauthorized low-risk reply;
- postpone decision.

Disallowed or deferred:

- reading long text;
- diff review;
- complex approvals;
- entering secrets;
- arbitrary computer-use.

---

## 21. Notifications and Attention Policy

### 21.1. Categories and channels

- Critical Security;
- Action Required;
- User Waiting Result;
- Communication;
- Failure;
- Completed;
- Background Digest;
- System/Sync;
- Updates;
- Usage/Budget;
- Informational.

Each category independently controls:

- push;
- sound;
- vibration;
- lock-screen content;
- badge;
- watch;
- digest;
- quiet hours;
- project overrides.

### 21.2. Permission request timing

Notification permission should be requested after user sees value or explicitly enables alerts, not blindly on first launch. Android guidance similarly recommends asking in context after the user understands the benefit. [[S18]]

### 21.3. Timing

Before delivering a noncritical notification Denet may consider:

- current Voice Session;
- active call;
- Focus/DND;
- presentation;
- recent user interaction;
- time of day;
- project urgency;
- whether notification can wait for natural breakpoint;
- user’s learned preferences.

Model call is not required for every notification; most use deterministic rules and lightweight attention score.

### 21.4. Suppression and digest

Multiple events are grouped by:

- project;
- process;
- category;
- time window;
- causal chain.

Examples:

- `3 background tasks completed`;
- `Project Denet: agent finished, tests passed, one decision needed`;
- `5 low-priority captures processed`.

Notification suppression can improve sustained focus for many, though not all, users; therefore policy is configurable and adaptive rather than universal. [[S26]]

### 21.5. Notification actions

Actions chosen per type:

- Open;
- Approve/Deny;
- Reply;
- Snooze;
- Pause/Stop;
- View Result;
- Send to Inbox;
- Mute.

Maximum visible actions are prioritized by frequency and safety. Overflow requires app open.

### 21.6. Lock-screen privacy

Levels:

- Full;
- Project Alias only;
- Generic `Denet needs a decision`;
- Count only;
- Hidden.

Sensitive project, recipient, amount or memory details are hidden until unlock.

### 21.7. Notification states

- Scheduled;
- Delivered;
- Suppressed;
- Grouped;
- Read;
- Action pending;
- Action acknowledged;
- Stale;
- Permission denied;
- Delivery unknown;
- Muted;
- Snoozed;
- Converted to Inbox.

# Часть V. Знания, результаты, возможности и настройки

## 22. Mobile Memory Workspace

### 22.1. Роль

Мобильная память нужна прежде всего для:

- быстрого поиска;
- проверки факта;
- просмотра project/current state;
- создания и исправления заметки;
- просмотра evidence;
- работы с capture;
- передачи объекта агенту.

Она не должна копировать полный desktop graph editor.

### 22.2. Entry points

- Global Search;
- Library/More;
- Project Memory;
- Quick Action `Ask Memory`;
- capture result;
- Context Lens;
- notification/deep link;
- voice command.

### 22.3. Memory Home

Sections:

- Ask Memory;
- Recent;
- Current State;
- Project Memories;
- Captures;
- Needs Review;
- Offline Available;
- Saved Searches.

Header:

- Search;
- New Note;
- Filter;
- overflow.

Overflow:

- Exact Search;
- Timeline;
- Evidence;
- Conflicts;
- Deletion Requests;
- Download Offline;
- Open on Desktop.

### 22.4. Search

Modes:

- Natural Language;
- Exact;
- Project;
- People;
- Time;
- Media;
- Evidence;
- Current State.

Filters:

- scope;
- date;
- source;
- type;
- project;
- sensitivity;
- device;
- freshness.

Result actions:

- Open;
- Ask About This;
- Add to Chat;
- Add to Project;
- Pin;
- Share, if permitted;
- Correct;
- View Evidence;
- Download;
- Delete/Forget.

### 22.5. Memory item detail

Shows:

- human-readable content;
- scope;
- source/provenance;
- confidence/freshness;
- related project/people;
- evidence handles;
- history;
- influence restrictions.

Buttons:

- Ask;
- Edit/Correct;
- Add to Context;
- Link Project;
- Share;
- More.

More:

- View History;
- View Evidence;
- Mark Outdated;
- Restrict Use;
- Keep Local;
- Export;
- Forget/Delete;
- Open on Desktop.

### 22.6. Note editor

- title optional;
- body;
- scope;
- project;
- tags/facets;
- sensitivity;
- attachments;
- save.

Autosave draft; explicit `Save` commits.

### 22.7. Current State

Compact cards:

- current fact/decision;
- last updated;
- evidence count;
- conflicts;
- freshness;
- `Why`;
- `Correct`.

### 22.8. Evidence viewer

Mobile shows:

- source preview;
- exact cited fragment;
- timestamp;
- speaker/device;
- trust/sensitivity;
- related claim.

Media controls:

- play;
- transcript;
- keyframes;
- share, if permitted;
- redact;
- delete raw while preserving allowed derivative.

### 22.9. Memory states

- Available;
- Cached;
- Stale;
- Current;
- Historical;
- Conflicted;
- Needs Review;
- Sensitive;
- Local Only;
- Source Missing;
- Evidence Unavailable Offline;
- Deletion Pending;
- Deleted/Tombstone;
- Projection Rebuilding;
- Search Partial;
- Index Offline.

---

## 23. Mobile Artifacts

### 23.1. Home

Views:

- Recent;
- Project;
- Needs Review;
- Favorites;
- Downloaded;
- Shared;
- Archived.

Header:

- Search;
- Filter;
- Sort;
- More.

### 23.2. Artifact card

- preview;
- title;
- type;
- project;
- version;
- draft/final;
- source Run/Session;
- offline state.

Actions:

- Open;
- Share;
- Add to Chat;
- Save Offline;
- More.

### 23.3. Viewer

Supports:

- Markdown/text;
- PDF/document;
- image;
- audio;
- video;
- HTML preview;
- presentation preview;
- code/diff summary;
- diagram;
- research dossier;
- workflow/procedure summary.

Toolbar:

- Back;
- version;
- Search in Artifact;
- Share;
- More.

Bottom actions:

- Discuss;
- Create Follow-up;
- Add to Project;
- Continue on Desktop;
- Download.

### 23.4. Artifact overflow

- View Details;
- Versions;
- Compare;
- Mark Final/Draft;
- Rename;
- Favorite;
- Export;
- Publish, if capability/permission exists;
- Archive;
- Delete;
- Open Source Run;
- Open on Desktop.

### 23.5. Share

Before sharing:

- target/channel;
- exact version;
- file/link;
- disclosure preview;
- expiry/access;
- personal metadata included/excluded;
- confirmation for sensitive artifact.

### 23.6. Artifact states

- Draft;
- Final;
- Generating;
- Processing;
- Ready;
- Needs Review;
- Superseded;
- Archived;
- Downloaded;
- Remote Only;
- Uploading;
- Failed;
- Partial;
- Missing Source;
- Access Restricted;
- Share Link Expired;
- Version Conflict.

---

## 24. Mobile Capabilities

### 24.1. Principle

Mobile provides discovery, health, simple binding and decisions. Advanced raw configuration belongs on desktop unless mobile is the only available device.

### 24.2. Categories

- Providers/Connections;
- Models;
- Skills;
- MCP/Tools;
- Plugins/Extensions;
- Connectors;
- Local Models;
- Computer Use;
- Speech/Media;
- Candidates;
- Updates;
- Project Sets.

### 24.3. Overview

Shows:

- unhealthy/auth-required;
- quota warnings;
- updates needing review;
- new candidates;
- project missing capability;
- local model status;
- recently added.

### 24.4. Capability item

- name/type;
- origin;
- trust;
- installed/available;
- health;
- scope;
- projects;
- version/update;
- measured utility.

Actions:

- Open;
- Enable/Disable;
- Add to Project;
- Try Project-Local;
- Update;
- Reconnect;
- Remove;
- More.

### 24.5. Manual add

Entry:

- URL;
- repository;
- local file/folder;
- registry search;
- provider marketplace;
- existing config;
- QR/deep link.

Flow:

1. preview identity/source;
2. choose scope;
3. add to Collection;
4. show executable requirements;
5. request trust only when execution needed;
6. finish or continue advanced setup on desktop.

No forced utility evaluation for user-selected capability.

### 24.6. Candidate review

Options:

- Ignore;
- Save Link Only;
- Quarantine;
- Add Candidate;
- Try Project-Local;
- Add Global;
- Compare Existing;
- Extract Useful Delta;
- Never Suggest Source.

Shows:

- why found;
- unique value;
- nearest alternatives;
- risks;
- required tools/scopes;
- token/context overhead;
- evidence.

### 24.7. Project Capability Set

Mobile list groups:

- Required;
- Recommended;
- On-demand;
- Experimental;
- Fallback;
- Disabled/Forbidden.

Actions:

- Add;
- Remove;
- Pin;
- Move category;
- Set Preferred;
- Disable for Project;
- Explain Selection;
- Open on Desktop.

### 24.8. Provider/Connection

Shows:

- auth/subscription/API/local;
- status;
- available models;
- limits/usage;
- privacy/locality;
- last probe;
- projects using it.

Actions:

- Connect/Reconnect;
- Test;
- Set Default for Internal Tasks;
- View Models;
- Usage;
- Disable;
- Remove;
- Advanced on Desktop.

### 24.9. Local model mobile control

- installed artifacts on phone/other nodes;
- loaded/warm;
- RAM/storage/battery estimate;
- download progress;
- use cases;
- start/stop/evict;
- Wi-Fi/charging policy;
- delete artifact;
- move download to another device.

Mobile should not encourage running a model that will destroy battery/thermal comfort; it shows estimated impact and offers server/device routing.

### 24.10. Capability states

- Discovered;
- Candidate;
- Collection;
- Installed;
- Configured;
- Auth Required;
- Probing;
- Healthy;
- Degraded;
- Quota Limited;
- Offline;
- Update Available;
- Update Review;
- Incompatible;
- Quarantined;
- Disabled;
- Removed/Tombstone;
- Project Required Missing;
- Local Resource Insufficient.

---

## 25. Mobile Automations and Events

### 25.1. Scope

Mobile supports:

- view;
- enable/disable;
- run/test;
- snooze;
- simple natural-language creation/edit;
- change delivery;
- inspect `Why It Fired`;
- resolve errors.

Complex workflow graph editing belongs on desktop.

### 25.2. Automations list

Filters:

- Enabled;
- Needs Attention;
- Recent;
- Project;
- Scheduled;
- Event-triggered;
- Disabled.

Automation item:

- name/purpose;
- trigger;
- next run/check;
- project;
- last outcome;
- enabled state;
- attention.

Quick actions:

- Enable/Disable;
- Run;
- Snooze;
- Open.

### 25.3. Create Automation

Natural-language composer:

- “Когда агент закончит дизайн, покажи варианты в Inbox”;
- “Каждое утро дай краткий статус активных проектов”;
- “Когда появится важная AI-новость по экранной памяти, свяжи её с Denet”.

Preview shows:

- trigger;
- action;
- scope;
- project;
- permissions;
- cooldown;
- expiry;
- notification;
- cost/budget;
- failure policy.

Buttons:

- Test;
- Enable;
- Edit;
- Save Disabled;
- Open Advanced on Desktop.

### 25.4. Automation detail

Sections:

- Purpose;
- Trigger;
- Actions;
- Permissions;
- Runs;
- Why It Fired;
- History;
- Failures;
- Delivery.

Actions:

- Enable/Disable;
- Run Now;
- Test;
- Snooze;
- Edit;
- Duplicate;
- Export;
- Delete;
- Open Desktop.

### 25.5. Why It Fired

Shows:

- source event;
- matched conditions;
- semantic assessment;
- cooldown;
- permission;
- selected action;
- outcome;
- no-op alternatives considered, where available.

### 25.6. Automation states

- Draft;
- Disabled;
- Enabled;
- Monitoring;
- Triggered;
- Running;
- Waiting;
- Needs Permission;
- Snoozed;
- Expired;
- Failed;
- Degraded;
- Missing Capability;
- Offline Deferred;
- Superseded;
- Quarantined.

---

## 26. Profile, Settings and Administration

### 26.1. Entry

Profile/avatar opens:

- account/installation;
- status;
- devices;
- settings;
- privacy;
- usage;
- help.

### 26.2. Profile sheet

- user/installation name;
- current Head;
- connection;
- current execution profile;
- voice/ambient;
- notification/DND;
- sync;
- account switch where applicable.

Quick actions:

- Privacy Mode;
- DND;
- System Status;
- Switch Installation;
- Lock App;
- Settings.

### 26.3. Settings categories

#### General

- language;
- startup destination;
- start with Voice;
- default project behavior;
- recent history;
- analytics/telemetry choice.

#### Appearance

- system/light/dark;
- text size;
- density;
- contrast;
- motion;
- project aliases;
- app icon/widget style.

#### Mobile Navigation

- bottom nav customization within constraints;
- Quick Action default;
- handedness;
- swipe actions;
- long-press shortcuts;
- Home sections;
- tablet layout.

#### Voice and Ambient

- default voice mode;
- language;
- voice;
- speed/style;
- push-to-talk/live;
- background conversation;
- wake-word;
- ambient mode;
- raw audio retention;
- headphones/privacy;
- meeting behavior.

#### Notifications and Attention

- categories/channels;
- quiet hours;
- DND;
- lock-screen privacy;
- watch;
- digest;
- timing adaptation;
- critical bypass;
- notification permission status.

#### Capture

- default destination;
- photo quality;
- OCR/transcription;
- local-first;
- Wi-Fi upload;
- auto-delete raw;
- quick-save behavior;
- screenshot handling.

#### Projects

- default offline availability;
- recent project limit;
- mobile review settings;
- default chat model display;
- project alias.

#### Trust, Privacy and Autonomy

- app lock;
- biometric step-up;
- execution profile;
- ask frequency;
- external send policy;
- voice low-risk policy;
- new tool policy;
- mobile approvals;
- privacy mode defaults;
- sensitive content on lock screen.

#### Sync and Offline

- Wi-Fi/cellular;
- charging-only heavy work;
- offline cache size;
- project downloads;
- media retention;
- sync now;
- conflicts;
- queue.

#### Devices and Handoff

- trusted devices;
- default execution device;
- voice handoff;
- desktop continuation;
- wearable;
- car integration;
- lost-device revoke.

#### Usage and Budgets

- provider usage;
- local resource;
- mobile data;
- background budget;
- notifications about limits;
- per-project budget shortcuts.

#### Accessibility

- screen reader;
- large text;
- large controls;
- reduced motion;
- captions;
- haptics;
- color-independent states;
- one-handed;
- voice control;
- switch access.

#### Advanced

- logs;
- diagnostics;
- feature previews;
- raw config read-only/edit where safe;
- reset caches;
- export diagnostics;
- safe mode;
- developer mode.

### 26.4. Settings search

Search result shows:

- setting name;
- current value;
- scope;
- path;
- modified/default;
- open.

### 26.5. Scope and precedence

Every setting shows scope:

- Global;
- Device;
- Project;
- Voice Profile;
- Notification Category;
- Temporary Override.

Project setting cannot silently change global setting.

### 26.6. Reset actions

- Reset This Setting;
- Reset Section;
- Reset Mobile Layout;
- Clear Local Cache;
- Remove Offline Downloads;
- Reset Notification Rules;
- Re-pair Device;
- Factory Reset Mobile Client.

Factory reset does not delete server data unless explicitly selected in a separate destructive flow.

---

## 27. Onboarding

### 27.1. Goals

Onboarding must get user to first useful action quickly and request permissions contextually.

### 27.2. First launch flow

1. Welcome: existing installation or new setup.
2. Sign in/pair device.
3. Confirm device name/trust.
4. Choose quick role: Voice & Remote / Full Mobile / Capture Only.
5. Show Home with sample/existing state.
6. Ask notification permission only when user enables meaningful alerts.
7. Ask microphone/camera only when used.
8. Offer widgets/shortcuts after first successful action.
9. Offer biometric step-up after first consequential decision.
10. Finish with one real action: talk, capture, open project or resolve card.

### 27.3. Pair existing Denet

Methods:

- QR code;
- short pairing code;
- passkey/account;
- local network discovery, optional;
- recovery package.

Shows:

- installation identity;
- Head;
- permissions requested;
- offline data choice;
- device trust;
- biometric requirement.

### 27.4. Notification onboarding

Explain categories before system prompt:

- decisions;
- user-waiting results;
- critical security;
- optional digests.

Default does not enable all noisy categories.

### 27.5. Widget/shortcut onboarding

After user repeatedly uses action:

- suggest relevant widget/shortcut;
- show exactly what it displays;
- choose project/privacy;
- `Add`;
- `Not now`;
- `Never suggest this`.

### 27.6. Returning user/new phone

- restore account/device identity;
- choose data to download;
- verify trusted device;
- show what is not yet available offline;
- resume pending handoff;
- do not replay old notifications as new.

### 27.7. Onboarding states

- New installation;
- Pairing;
- Waiting approval from trusted device;
- Restoring;
- Partial sync;
- Notification denied;
- Microphone denied;
- Camera denied;
- Biometric unavailable;
- Head offline;
- Recovery mode;
- Existing device conflict;
- Completed.

# Часть VI. Полный каталог состояний, меню и interruption-safe поведения

## 28. Универсальная модель состояний мобильного интерфейса

### 28.1. Почему состояния описываются отдельно

Мобильный интерфейс Denet часто работает поверх распределённого и частично доступного состояния:

- Head Runtime может быть недоступен;
- device может иметь локальный cache;
- команда может быть отправлена, но ещё не подтверждена;
- процесс может продолжаться на сервере после закрытия приложения;
- объект мог измениться на desktop, пока пользователь смотрит старую карточку;
- локальный capture может быть сохранён, но ещё не синхронизирован;
- external effect может иметь состояние `UNKNOWN`;
- permission может потребовать системной reauthentication.

Поэтому простого набора `loading / success / error` недостаточно. Любой экран или component обязан показывать не только данные, но и **качество знания об этих данных**.

### 28.2. Базовые состояния любого data-bound component

#### Initial

Компонент ещё не пытался получить данные.

Допустимо только на первом открытии или после явного reset.

#### Loading Empty

Данных ещё нет, идёт первичная загрузка.

Показывается skeleton, а не spinner поверх пустого экрана, если структура заранее известна.

#### Loading Cached

Локальные данные уже показаны, а свежая версия загружается в фоне.

Пользователь может продолжать чтение. Refresh не сбрасывает scroll и selection.

#### Fresh

Состояние подтверждено authoritative source в пределах freshness policy.

#### Stale

Данные доступны, но Head, provider или device давно не подтверждал их.

Показываются:

- `Updated N min ago`;
- причина устаревания;
- `Refresh`;
- ограничения доступных действий.

#### Offline Cached

Данные получены из локального cache, сеть отсутствует.

Это не error-state. Пользователь может читать, создавать drafts и выполнять разрешённые local actions.

#### Optimistic Pending

Пользователь совершил обратимое действие, UI применил его локально и ждёт подтверждения.

Примеры:

- archive session;
- mute notification source;
- reorder Home card;
- локальная правка note.

Обязательно:

- индикатор pending;
- возможность Undo там, где это безопасно;
- rollback при отказе.

#### Command Sending

Command отправляется Head или node.

Кнопка блокируется от повторного tap, но рядом доступен `Cancel` только если транспорт ещё можно отменить.

#### Accepted

Command принят runtime, но эффект ещё не завершён.

UI показывает не «Done», а `Accepted`, `Starting` или доменное состояние.

#### Partial

Доступна часть данных или часть операции завершилась.

Экран показывает:

- что готово;
- что отсутствует;
- можно ли продолжать;
- `Retry Missing`;
- `Open Partial Result`.

#### Conflict

Существует несколько несовместимых версий или competing decisions.

Ни одна не скрывается молча.

#### Restricted

Данные или действие существуют, но текущая session, device trust или policy не позволяют раскрыть/выполнить их.

Показывается конкретная причина и допустимый следующий шаг.

#### Unavailable

Функция не поддерживается текущим device, provider, OS или project configuration.

Это отличается от временного error.

#### Error Recoverable

Известна безопасная recovery action:

- retry;
- reconnect;
- use local copy;
- switch provider;
- reauthenticate;
- continue on another device.

#### Error Blocking

Продолжение невозможно без внешнего исправления.

Экран сохраняет partial state и предлагает экспорт diagnostics.

#### Unknown Outcome

Команда с внешним эффектом могла выполниться, но подтверждение потеряно.

Кнопка `Retry` по умолчанию отсутствует. Вместо неё:

- `Check Status`;
- `Reconcile`;
- `Open Provider`;
- `Ask Denet`;
- `Mark Manually`, если policy допускает.

#### Removed / Revoked / Superseded

Объект больше не действует, но может оставаться в history.

UI объясняет, чем он заменён или почему отозван.

### 28.3. Empty-state taxonomy

Mobile различает:

- **Truly Empty:** объект ещё ни разу не создавался;
- **Filtered Empty:** filters скрыли все объекты;
- **Search Empty:** запрос ничего не нашёл;
- **Offline Empty:** нужные данные не загружены на устройство;
- **Permission Empty:** данные есть, но недоступны;
- **Completed Empty:** открытых решений нет — это положительное состояние;
- **Configuration Empty:** capability или account ещё не подключены.

Каждое состояние имеет отдельный текст и отдельный primary action.

### 28.4. Streaming-state rules

При streaming update:

- текущий scroll не прыгает;
- сообщение не отнимает focus у composer;
- новые элементы выше viewport показывают ненавязчивый `N updates`;
- текущая кнопка не меняет положение под пальцем;
- пользователь может `Pause Live Updates`;
- accessibility live region не зачитывает каждый token;
- transcript и agent output группируются смысловыми chunks;
- transient intermediate status не записывается как permanent notification.

### 28.5. Freshness presentation

Freshness показывается пропорционально риску.

Для обычной project summary достаточно:

- `Just now`;
- `5 min ago`;
- `Cached`.

Для permission, payment, send, delete или device control показываются:

- authoritative source;
- точное время;
- session/device;
- revision;
- необходимость refresh.

Пользователь не обязан видеть технический watermark в обычной работе, но может открыть `Why this state?`.

---

## 29. Полный lifecycle мобильного приложения

### 29.1. Application lifecycle states

#### Not Configured

Приложение установлено, но не связано с Denet.

Primary actions:

- `Pair Existing Denet`;
- `Create New Installation`;
- `Restore`;
- `Explore Demo`.

#### Pairing

Показываются:

- installation identity;
- Head address/profile;
- requested device trust;
- progress;
- `Cancel`;
- `Use Another Method`.

#### Locked

Доступно только:

- privacy-safe status;
- Push-to-Talk, если policy разрешает;
- Emergency Stop entry;
- biometric/passcode unlock.

#### Starting

Приложение сначала восстанавливает local shell и drafts, затем подключает live state.

Полная синхронизация не блокирует Home.

#### Online Healthy

Head доступен, sync fresh, primary capabilities работают.

Не показывается большой зелёный banner. Состояние видно в compact status indicator.

#### Online Degraded

Пример:

- Head доступен, но provider down;
- sync работает, но voice backend unavailable;
- project node offline;
- push token invalid.

Home показывает только затронутые функции и recovery.

#### Offline Local

Работают:

- cached views;
- local capture;
- local voice pipeline, если доступен;
- local project access;
- drafts;
- offline queue.

#### Syncing

Background sync не блокирует app. Detailed progress доступен в System Status.

#### Recovery

После crash/process death:

- drafts восстанавливаются;
- uncommitted confirmation не считается accepted;
- active external action reconciles;
- Voice Session не продолжает говорить автоматически;
- Resume Capsule объясняет, где произошёл interruption.

#### Safe Mode

Включается при:

- подозрительном capability;
- damaged local state;
- repeated startup crash;
- security incident;
- incompatible migration.

Работают read-only views, export diagnostics, recovery и revoke.

#### Emergency Stop Active

Все consequential controls визуально заблокированы. Пользователь видит, что именно остановлено и как безопасно восстановить.

#### Updating / Migration

Показывается:

- что обновляется;
- можно ли продолжать read-only;
- что требует app restart;
- progress только для значимой migration;
- `View Details`;
- rollback/recovery, если доступно.

### 29.2. Foreground/background transition

При уходе приложения в background:

1. сохраняется navigation state;
2. сохраняются drafts;
3. прекращаются неподходящие foreground-only streams;
4. активный voice режим следует Voice policy;
5. Managed Run продолжает работать на Head;
6. локальный upload передаётся подходящему background mechanism;
7. sensitive preview скрывается в task switcher, если включён Privacy Curtain;
8. создаётся lightweight Resume Capsule.

### 29.3. Process death

После уничтожения процесса ОС приложение не полагается на in-memory state.

Восстанавливаются:

- last destination;
- composer draft;
- pending attachments;
- capture staging;
- local queue;
- selected project;
- unresolved deep link;
- playback/listening state как `interrupted`, а не автоматически active.

Не восстанавливаются автоматически:

- подтверждение high-risk action;
- открытый biometric result;
- uncommitted partial voice command;
- secret reveal;
- destructive swipe.

### 29.4. Upgrade and schema mismatch

При несовместимой версии mobile/server:

- показывается понятное ограничение;
- read-only режим сохраняется, если безопасно;
- queued commands не отправляются через неизвестный contract;
- user получает `Update App`, `Update Server`, `Use Web/Desktop`;
- diagnostics показывают обе версии.

---

## 30. Полный каталог мобильных menus и commands

### 30.1. Принцип единой команды

Каждая операция имеет стабильный `command_id`.

Одна и та же операция, вызванная из:

- button;
- overflow;
- swipe;
- widget;
- notification;
- shortcut;
- voice;
- deep link;

должна использовать один канонический command и различаться только `entry_context`.

Это исключает ситуацию, когда `Pause` из notification работает иначе, чем `Pause` внутри Run screen.

### 30.2. Global Profile menu

Открывается avatar/profile action.

Содержит:

- Switch Installation;
- Switch Account/Profile;
- Current Device;
- Privacy Mode;
- Do Not Disturb;
- Mobile Role;
- Usage and Limits;
- System Status;
- Settings;
- Help;
- Send Feedback;
- Diagnostics;
- Lock App;
- Sign Out / Disconnect Device.

Последняя группа отделена визуально от частых настроек.

### 30.3. Global More menu

Доступен на Home и top-level destinations:

- Global Search;
- Command Search;
- Quick Capture;
- Talk to Denet;
- Scan QR / Pair Device;
- Manage Offline Data;
- Sync Now;
- Customize Home;
- Edit Navigation;
- Privacy Curtain;
- Emergency Stop;
- Help.

Команды, уже видимые на текущем экране, не дублируются без причины.

### 30.4. Home overflow

- Customize Home;
- Reorder Cards;
- Configure Pocket Brief;
- Refresh;
- Hide Greeting;
- Privacy on Home;
- Add Widget;
- Edit Shortcuts;
- Reset Home.

### 30.5. Projects Hub overflow

- New Project;
- Import Project;
- Join/Open Shared Project;
- Scan Repository QR/Link;
- Manage Downloads;
- Sort;
- Saved Filters;
- Show Archived;
- Project Settings Defaults;
- Continue on Desktop.

### 30.6. Project overflow

- Project Overview;
- New Session;
- Talk to Project;
- Capture to Project;
- Open Changes;
- Open Runs;
- Open Memory;
- Open Artifacts;
- Project Capabilities;
- Automations;
- Download for Offline;
- Continue on Desktop;
- Share Project Reference;
- Project Settings;
- Archive Project;
- Leave/Disconnect Project;
- Delete, только при authority и с отдельным confirmation flow.

### 30.7. Project Session overflow

- Session Info;
- Rename;
- Pin;
- Mark Done;
- Start Background Run;
- Fork from Here;
- Open Checkpoints;
- Add Current Context;
- Remove Context;
- Change Model, если разрешено;
- Change Execution Profile;
- Handoff to Device;
- Export/Share Transcript;
- Archive;
- Delete Local Drafts;
- Delete Session, если policy допускает.

### 30.8. Message context menu

Для user message:

- Copy;
- Edit and Resend;
- Fork from Message;
- Add to Memory;
- Create Task;
- Share;
- Report Recognition Error, если voice;
- Delete local representation, если допустимо.

Для agent message:

- Copy;
- Listen;
- Stop Listening;
- Ask Follow-up;
- Quote;
- Open Sources/Evidence;
- Add to Memory;
- Create Task;
- Save as Artifact;
- Compare;
- Mark Helpful/Not Helpful;
- Report Issue;
- Share.

Для tool/action block:

- Open Details;
- View Inputs;
- View Output;
- View Effect Receipt;
- Open on Device;
- Retry, только если безопасно;
- Reconcile;
- Ask Denet Why;
- Copy Reference.

### 30.9. Run context menu

- Open;
- Follow Live;
- Pause;
- Resume;
- Steer;
- Stop After Current Step;
- Stop Now;
- Change Priority;
- Change Budget;
- Move Execution;
- View Artifacts;
- View Changes;
- View Evidence;
- Open Logs;
- Retry Failed Part;
- Reconcile Unknown Effect;
- Duplicate as New Run;
- Archive.

Недоступные команды остаются видимыми только если объяснение полезно; иначе скрываются для снижения шума.

### 30.10. Inbox card context menu

- Open;
- Resolve with Recommended;
- Discuss with Denet;
- Snooze;
- Delegate to Orchestrator;
- Open Related Project;
- Open Related Run;
- View Sources;
- Mark Not Urgent;
- Mute Similar;
- Report Wrong Question;
- Withdraw/Dismiss, если разрешено.

### 30.11. Memory item context menu

- Open;
- Ask About This;
- Add to Current Context;
- Pin;
- Link to Project;
- Correct;
- Add Note;
- View Evidence;
- View History;
- Compare;
- Change Scope;
- Hide from Suggestions;
- Export;
- Forget/Delete;
- Track Deletion.

### 30.12. Artifact context menu

- Open;
- Preview;
- Listen/Summarize;
- Attach to Session;
- Add to Project;
- Create Task;
- Compare Versions;
- Download;
- Share;
- Continue on Desktop;
- Mark Final/Draft;
- Archive;
- Delete.

### 30.13. Capability context menu

- Open;
- Enable/Disable;
- Add to Project;
- Remove from Project;
- Try Project-Local;
- Set Preferred;
- Set Fallback;
- Compare;
- Inspect Source;
- Inspect Security;
- Update;
- Pin Version;
- Fork;
- Extract Useful Delta;
- Quarantine;
- Remove.

### 30.14. Notification context actions

Доступны через notification settings или long-press системного уведомления:

- Mute This Source;
- Reduce Priority;
- Move to Digest;
- DND for 1 hour;
- Keep Critical Only;
- Open Notification Settings;
- Explain Why I Was Notified.

### 30.15. Widget configuration menu

- Select Project/Scope;
- Choose Actions;
- Privacy Alias;
- Hide Counts;
- Show Freshness;
- Require Unlock;
- Refresh;
- Resize Help;
- Remove Widget.

### 30.16. Swipe rules

Swipe не применяется к high-risk операциям как единственный шаг.

Допустимые defaults:

- Inbox right: primary safe resolution или `Open`;
- Inbox left: `Snooze`;
- Notification right: mark read;
- Notification left: mute/snooze;
- Project/session: pin/archive;
- Capture staging: keep/delete с Undo;
- Task: complete только если это пользовательская простая задача, не Agentic Run.

Destructive swipe:

- показывает label до commit;
- имеет Undo;
- не используется для irreversible delete;
- отключается в accessibility settings.

### 30.17. Long-press rules

Long-press даёт ускорение, но не скрывает единственный путь.

Используется для:

- preview;
- multi-select;
- context menu;
- drag/reorder;
- quick project switch;
- widget configuration.

### 30.18. Search and command menu

Global search prefixes:

- без prefix — смешанный поиск;
- `>` — commands;
- `@` — projects/people/agents;
- `#` — memory/topics;
- `/` — skills/actions;
- `!` — runs/inbox;
- `:` — settings.

Results показывают:

- object type;
- scope;
- freshness;
- current state;
- primary action;
- `Continue on Desktop`, если mobile-view недостаточен.

---

## 31. Interruption and Resumption Fabric мобильного приложения

### 31.1. Главный принцип

Mobile Denet не должен ожидать, что пользователь завершит текущую мысль или экран за один непрерывный заход.

Каждая рабочая поверхность поддерживает:

- **departure snapshot** — где пользователь остановился;
- **return delta** — что изменилось;
- **resumption cue** — что делать дальше;
- **staleness check** — не устарело ли первоначальное действие.

Исследования мобильного чтения показывают, что короткие review после interruptions нравятся пользователям, а previews предстоящего содержания способны улучшать понимание. [[S25]] Поэтому Denet использует оба элемента: «что было» и «что дальше».

### 31.2. Resume Capsule

После возврата может появиться компактная capsule:

```text
Вы остановились на review файла auth.ts.
Пока вас не было:
• Agent исправил 2 замечания.
• Tests прошли.
• Нужен выбор по одному API.

[Продолжить review] [Открыть вопрос] [Кратко объясни]
```

Capsule содержит максимум:

- прежнюю цель;
- последний осмысленный position;
- одну-две значимые дельты;
- следующий recommended action;
- возможность закрыть.

Она не пересказывает всю историю.

### 31.3. Review + Preview

При возврате к длинному объекту:

- **Review:** кратко напоминает последнее прочитанное/решённое;
- **Preview:** объясняет ближайший следующий раздел или действие.

Примеры:

- chat: последнее пользовательское намерение + текущий ответ агента;
- diff: последний просмотренный файл + число оставшихся изменений;
- Inbox: что требовалось решить + изменилось ли основание;
- artifact: последняя версия + новая версия;
- voice: последняя завершённая тема + незавершённый parking lot.

### 31.4. What Changed While Away

Сводка строится только при material change:

- Run completed/failed;
- вопрос появился/закрылся;
- файлы изменились;
- новое evidence;
- provider/device state повлиял на работу;
- permission expired;
- upstream commit изменил project context.

Если ничего существенного не произошло, показывается `No meaningful changes` или capsule не появляется.

### 31.5. Save and Leave

В composer, note editor, capture staging и approval parameter editor доступно явное действие:

- `Save and Leave`;
- `Keep as Draft`;
- `Discard`.

Обычный system back автоматически сохраняет безопасный draft и показывает snackbar:

> Draft saved locally.

Высокорисковое подтверждение не сохраняется как accepted. Сохраняются только введённые параметры и explanation.

### 31.6. Interruption-safe composer

Composer хранит:

- текст;
- attachment references;
- selected project/session;
- mode;
- model/profile override;
- voice transcript draft;
- whether user intended queue/steer/stop-and-send.

При возврате приложение проверяет:

- существует ли session;
- не завершён ли Run;
- не изменился ли branch/recipient;
- актуальны ли attachments;
- не истекло ли permission.

### 31.7. Interruption of an agent action

Если пользователь меняет намерение, интерфейс различает:

- **Add:** добавить новое требование;
- **Revise:** изменить текущее требование;
- **Retract:** отменить прежнее требование;
- **Stop:** прекратить текущую работу;
- **New Topic:** не менять старую работу, открыть новую session.

Это важно, потому что InterruptBench показывает, что даже сильные long-horizon agents испытывают трудности при addition, revision и retraction во время выполнения. [[S29]] Mobile UI не должен прятать эти различия за одной кнопкой `Send`.

### 31.8. Return after notification

Перед открытием notification Denet сохраняет:

- current route;
- scroll anchor;
- unsent draft;
- selected object;
- current voice/listening state.

После завершения quick action приложение предлагает:

- `Return to [previous object]`;
- `Stay here`.

### 31.9. Return after external share/camera

Share extension и camera flow создают staging item до перехода между приложениями.

Если система убила приложение:

- media остаётся локально;
- upload возобновляется;
- classification может завершиться позже;
- user получает `Finish capture` только если действительно требуется решение.

### 31.10. Expiring Resume Capsules

Capsule исчезает, если:

- пользователь явно завершил объект;
- context устарел;
- связанную card разрешили на другом устройстве;
- project удалён/archived;
- прошло настроенное время;
- пользователь выбрал `Don't show for this type`.

---

# Часть VII. Скорость, удобство на ходу, доступность и новые функции

## 32. One-Handed and On-the-Go Interaction

### 32.1. Bottom Action Dock

Главные частые действия располагаются в нижней reachable zone:

- Voice;
- Capture;
- Send/Confirm;
- Pause/Stop;
- primary Inbox action.

Исследования one-handed action bar adaptations показывали преимущества по скорости, комфорту и стабильности хвата, особенно для недоминантной руки. [[S30]] Поэтому top-right overflow не может быть единственным местом частого действия.

### 32.2. Handedness

Пользователь выбирает:

- Right-handed;
- Left-handed;
- Adaptive;
- No adaptation.

Adaptive mode может перемещать secondary action dock и sheet handles, но:

- не меняет destructive/confirm positions прямо во время gesture;
- не переставляет интерфейс без hysteresis;
- показывает короткое объяснение;
- легко отключается.

Research по adaptive handedness подтверждает полезность такого направления, но Denet не должен автоматически угадывать руку ценой нестабильного layout. [[S31]]

### 32.3. Walking Mode

Walking Mode включает:

- крупные targets;
- меньше текста;
- voice-first actions;
- reduced animation;
- haptic confirmation;
- simplified Home;
- one-card-at-a-time Inbox;
- automatic handoff сложного review;
- запрет мелких destructive controls.

Активация:

- вручную;
- shortcut/widget;
- optional motion context;
- wearable;
- voice command.

Автоматическое предложение допускается, автоматическое включение без согласия — нет.

### 32.4. Glance Mode

Для очень короткой проверки:

- top three `Needs You`;
- top three active Runs;
- last important result;
- sync/head warning;
- no long feed;
- no promotional content.

Закрытие Glance не считается прочтением всех items.

### 32.5. Low-Attention Mode

Используется:

- в общественном транспорте;
- при разговоре с человеком;
- на улице;
- при усталости;
- при работе одной рукой.

Изменения:

- предложения становятся короче;
- один primary action;
- confirmation read-back для consequential action;
- complex content отправляется в Pocket Brief или desktop;
- голос не озвучивает sensitive data;
- reduced auto-scroll.

### 32.6. Driving/Car mode

Car mode не даёт полный доступ к Denet.

Разрешены:

- короткий hands-free status;
- capture voice note;
- postpone/snooze;
- pause/stop background Run;
- call trusted contact through OS integration;
- emergency actions.

Запрещены или откладываются:

- reading diff;
- complex approval;
- project configuration;
- text entry;
- reviewing sensitive artifacts.

### 32.7. Reachability alternatives

Любая top action имеет хотя бы одну альтернативу:

- bottom action;
- swipe;
- voice;
- shortcut;
- notification;
- accessibility action.

---

## 33. Accessibility as a Mobile Baseline

### 33.1. Общий стандарт

Denet Mobile проектируется по принципам WCAG 2.2 и нативных accessibility APIs платформ. WCAG 2.2 отдельно включает требования к target size, dragging alternatives, accessible authentication, focus visibility, timing и status messages. [[S32]]

### 33.2. Touch targets

- обычный interactive target не меньше platform-recommended minimum;
- критические actions больше минимального;
- adjacent destructive/positive actions имеют достаточный gap;
- icon-only controls имеют semantic label;
- tiny inline icons не являются единственным action target;
- expanded invisible hit area не перекрывает соседние controls.

### 33.3. Screen reader

Каждый экран имеет:

- логичный heading order;
- доступные labels;
- role/state/value;
- summary выбранного объекта;
- custom actions для swipe/overflow;
- announcement важных status transitions;
- отсутствие token-by-token chatter;
- возможность открыть technical details отдельно.

Agent streaming announcements группируются:

- `Agent started working`;
- `Agent needs permission`;
- `Agent completed`;
- meaningful paragraph finished.

### 33.4. Large text and reflow

При крупном шрифте:

- bottom navigation может перейти в labels-only/scrollable pattern;
- cards растут по высоте;
- buttons не обрезают смысл;
- charts получают textual summary;
- diff переходит в one-column;
- secondary metadata сворачивается;
- horizontal scrolling не требуется для основной функции.

### 33.5. Switch access and external input

Поддерживаются:

- full focus traversal;
- hardware keyboard;
- switch control;
- voice control;
- stylus;
- pointer on tablet/foldable;
- external headset buttons.

Dragging всегда имеет menu-based alternative.

### 33.6. Motion and sensory alternatives

- `Reduce Motion` отключает параллакс, animated agent office и aggressive transitions;
- haptic имеет visual/text alternative;
- sound имеет visual alternative;
- status не передаётся только цветом;
- voice transcript доступен текстом;
- media получает captions/transcript;
- flashing/rapid animation не используется.

### 33.7. Cognitive accessibility

- один экран — одна главная задача;
- plain-language labels;
- consistent command names;
- progressive disclosure;
- undo и preview;
- не более одного primary destructive decision в sheet;
- explicit next step;
- объяснение AI uncertainty;
- `Explain Simply`;
- `Read Aloud`;
- `Summarize`;
- configurable density.

### 33.8. Accessible authentication

Mobile использует platform biometric/passcode prompt.

Пользователь не обязан:

- запоминать отдельный Denet challenge;
- решать cognitive puzzle;
- перепечатывать длинный код, если возможен passkey/QR/biometric;
- раскрывать password ассистивной технологии нестандартным способом.

### 33.9. Accessibility diagnostics

Settings содержит:

- screen reader status;
- focus order test;
- touch target overlay;
- contrast preview;
- text scale preview;
- reduced motion preview;
- notification/haptic test;
- Voice transcript test;
- export accessibility report.

---

## 34. Performance, Battery, Data and Perceived Speed

### 34.1. Performance philosophy

Mobile-first speed означает:

- shell открывается из local state;
- network refresh не блокирует interaction;
- capture подтверждается локально;
- voice starts locally;
- heavy artifact preview загружается progressively;
- large sync не занимает foreground;
- server work продолжается после app closure;
- screen transitions не ждут LLM.

### 34.2. Initial performance targets

Это стартовые гипотезы для собственного pilot, а не вечные гарантии:

- cold shell usable: около 1–2 секунд на целевых устройствах;
- warm open: perceptually immediate;
- tap feedback: менее 100 мс;
- local capture commit: менее 300 мс после завершения ввода;
- push-to-talk recording: менее 300 мс;
- cached Home: без ожидания сети;
- Inbox local open: immediate;
- command acknowledgement: сразу после local queueing;
- sync freshness update: progressively;
- notification action feedback: instant local + server confirmation later.

Targets уточняются по hardware и OS.

### 34.3. Startup order

1. privacy/lock check;
2. local shell;
3. drafts and resume state;
4. critical Inbox count;
5. cached Home;
6. live Head connection;
7. background refresh;
8. noncritical media/indexes.

Нельзя ждать загрузки Memory graph или provider catalogue до показа Home.

### 34.4. Network policy

- compact structured state раньше media;
- delta sync раньше full refresh;
- thumbnails раньше originals;
- Wi-Fi-only option для raw media/models;
- cellular budget;
- roaming policy;
- manual `Download for Offline`;
- background uploads resumable;
- duplicate upload content-addressed;
- proxy/relay state visible.

### 34.5. Battery policy

Expensive background features имеют profiles:

- Normal;
- Battery Saver;
- Travel;
- Charging Only;
- Capture Priority;
- Voice Priority.

Ограничиваются:

- ambient semantic analysis;
- high-frequency location;
- camera processing;
- continuous waveform UI;
- local model residency;
- background prefetch;
- frequent polling.

Server/webhook/push предпочтительнее постоянного mobile polling.

### 34.6. Background work

На Android длительные/переживающие закрытие операции используют системный persistent work mechanism с constraints и retry, а срочная short user-initiated работа может использовать expedited execution в пределах OS quotas. [[S34]]

На iOS применяются platform background tasks, uploads, push и system-defined budgets; приложение не обещает постоянный background runtime, если ОС его не гарантирует.

### 34.7. Media and local model pressure

При нехватке памяти:

- preview cache эвиктится первым;
- local model unloads;
- draft/capture metadata сохраняются;
- raw capture не теряется;
- current voice path получает приоритет;
- UI объясняет fallback на cloud/server.

### 34.8. Perceived progress

Показывается не fake percentage, а meaningful phase:

- Preparing;
- Uploading;
- Waiting for device;
- Agent working;
- Verifying;
- Needs you;
- Finalizing;
- Syncing result.

Если точный progress неизвестен, используется elapsed time и last meaningful update.

### 34.9. Quiet completion

Background success по умолчанию:

- обновляет badge/activity;
- добавляет result в Home delta;
- не показывает toast, если пользователь не ждал;
- может войти в digest.

Это уменьшает extrinsic interruptions, которые исследования связывают с повышенной субъективной нагрузкой. [[S35]]

---

## 35. Новые функции с высокой практической ценностью

### 35.1. Pocket Brief

Одно короткое представление на 15–60 секунд:

- что требует решения;
- что изменилось;
- что завершилось;
- один recommended next action;
- `Listen`;
- `Open`;
- `Dismiss`.

Пользователь может вызвать:

- с Home;
- widget;
- notification digest;
- голосом;
- wearable.

Pocket Brief не является новой памятью или отдельным feed. Это временная проекция канонических объектов.

### 35.2. Capture First, Organize Later

Любой capture сначала получает надёжный local ID и timestamp.

Пользователь может сразу убрать телефон. Позже Denet:

- транскрибирует;
- связывает с проектом;
- предлагает title/tags;
- извлекает task/idea;
- просит clarification только при material ambiguity.

### 35.3. Quick Resolve Stack

Inbox может работать как одна карточка за раз:

- swipe/primary action;
- haptic;
- следующая карточка;
- progress `2 of 5`;
- `Stop`;
- `Discuss`;
- `Snooze`.

Подходит для короткого окна между делами.

### 35.4. One-Tap Response Macros

Для коммуникационных cards доступны динамические safe macros:

- `Принял, отвечу позже`;
- `Да`;
- `Нет`;
- `Сегодня не смогу`;
- `Подготовь черновик`;
- project-specific variants.

Macro показывает точный recipient и текст до отправки. Пользователь может закрепить свои варианты.

### 35.5. Follow Run

Пользователь может `Follow` значимый Run.

Тогда mobile показывает:

- Live Activity/Live Update только пока run действительно активен;
- milestone notifications;
- current blocker;
- Pause/Stop;
- результат.

Обычные Runs не занимают lock screen автоматически.

### 35.6. Send to Big Screen

На любом сложном объекте:

- `Continue on Desktop`;
- `Open on Nearby Device`;
- `Copy Secure Link`;
- `Queue for Later Review`.

При выборе приложение может заранее открыть нужный desktop workspace и сохранить mobile return link.

### 35.7. Nearby Device Resume

Если trusted desktop активен рядом, mobile предлагает:

- `Open diff on desktop`;
- `Move voice output to PC`;
- `Use phone as microphone`;
- `Keep phone as approval remote`.

Предложение не появляется постоянно и отключается.

### 35.8. Context Card

Перед быстрым consequential action показывается компактная карточка:

- что;
- кому/куда;
- почему;
- источник параметров;
- можно ли отменить;
- цена/риск;
- confirm.

Это быстрее длинного диалога и безопаснее blind button.

### 35.9. Safe Retry Assistant

При failure Denet объясняет:

- безопасно ли повторить;
- что уже произошло;
- есть ли idempotency;
- нужен ли reconciliation;
- какой альтернативный backend доступен.

Кнопки:

- Retry Safe Part;
- Check Status;
- Use Alternative;
- Keep Partial;
- Ask Denet;
- Cancel.

### 35.10. Offline Confidence Badge

Каждый offline object может показывать:

- `Cached — likely current`;
- `Cached — may be outdated`;
- `Local-only draft`;
- `Queued`;
- `Conflict possible`;
- `Fresh as of…`.

Это полезнее общего banner `You are offline`.

### 35.11. Smart Action Surface

Action Dock может адаптироваться к текущему объекту:

- Project: Talk / Capture / New Session;
- Inbox: primary choice / Discuss / Snooze;
- Run: Open / Pause / Stop;
- Memory: Ask / Edit / Link;
- Artifact: Open / Share / Desktop.

Пользователь может отключить adaptation или закрепить фиксированный dock.

### 35.12. Personal Interruption Policy Learning

Denet наблюдает:

- какие уведомления открываются;
- какие постоянно откладываются;
- какие actions выполняются из widget;
- когда user включает DND;
- в каких контекстах голос уместен.

На основе этого он **предлагает**, а не молча применяет:

- перенести category в digest;
- изменить время;
- добавить shortcut;
- скрыть card;
- создать bounded automation.

Research показывает, что notification attendance частично предсказуема по usage patterns, но individual interruptibility зависит от контекста и социальных ролей. [[S27]] [[S28]] Поэтому auto-tuning остаётся прозрачным и обратимым.

### 35.13. Return Ticket

Перед началом короткого ответвления пользователь может нажать `Return here later`.

Создаётся lightweight ticket с:

- object;
- exact position;
- short note;
- optional reminder;
- no Task by default.

### 35.14. Public Place Mode

Одно действие:

- скрыть project names;
- отключить voice output;
- показывать generic notifications;
- требовать unlock для details;
- не показывать private memory thumbnails;
- использовать haptics.

### 35.15. Capture Chain

После фото/voice/link пользователь может одним tap выбрать:

- Save only;
- Add to current project;
- Ask Denet;
- Create Task;
- Send to project agent;
- Remind later.

По умолчанию не открывается длинная classification form.

---

## 36. Mobile-specific security and privacy behavior

### 36.1. Trusted remote, not universal master key

Телефон может быть сильным trusted device для step-up, но потеря телефона не должна означать потерю всей системы.

Хранятся:

- device-bound keys;
- encrypted local cache;
- revocable sessions;
- minimal secrets;
- no raw API keys where brokered access is possible.

### 36.2. App lock

Options:

- follow device unlock;
- biometric on app open;
- biometric only for sensitive spaces;
- lock after N minutes;
- lock on app background;
- always lock in Public Place Mode.

### 36.3. Screenshot and task-switcher protection

Per-space policy:

- allow screenshot;
- blur sensitive details;
- block OS screenshot where platform supports and justified;
- hide app-switcher snapshot;
- watermark shared screenshots;
- explicit `Share Safe Screenshot`.

### 36.4. Notification privacy levels

- Full;
- Project alias only;
- Generic action needed;
- Count only;
- Hidden until unlock.

Separate policy for:

- lock screen;
- wearable;
- car;
- headset readout.

### 36.5. Biometric step-up

System biometric/passcode prompt используется для:

- permission approval высокого assurance;
- secret reveal;
- external send по policy;
- payment;
- destructive action;
- trusted-device changes;
- Head takeover.

Если biometric unavailable:

- device credential;
- another trusted device;
- defer;
- deny.

### 36.6. Clipboard

Sensitive copy:

- clear after timeout where platform allows;
- mark sensitive to OS where supported;
- show warning;
- prefer secure share/action;
- never auto-copy secret without explicit user command.

### 36.7. Lost device

Remote actions:

- revoke device;
- invalidate sessions;
- disable offline queue;
- block push details;
- rotate device-specific credentials;
- mark local data for cryptographic inaccessibility;
- preserve audit.

### 36.8. Rooted/jailbroken or compromised signal

Это risk signal, а не абсолютная истина.

Policy может:

- disallow secret reveal;
- restrict DA3;
- allow read-only;
- require another device;
- show warning;
- permit user override only within safety floor.

### 36.9. Share extension trust

Shared content считается external data.

Extension показывает:

- source app;
- content preview;
- target project;
- `Save`;
- `Ask Denet`;
- `Cancel`.

Текст из webpage не становится instruction автоматически.

---

# Часть VIII. Evaluation, реализация и полнота спецификации

## 37. Mobile UX Evaluation Program

### 37.1. Главная метрика

Основная метрика — **time-to-safe-useful-outcome**, а не session duration и не количество taps само по себе.

Она включает:

- время;
- число действий;
- вероятность ошибки;
- cognitive effort;
- interruption cost;
- recovery cost;
- необходимость открыть desktop;
- безопасность результата.

### 37.2. Core metrics

#### Speed

- app cold/warm start;
- time to Home usable;
- time to Push-to-Talk;
- time to local capture commit;
- time to open critical card;
- time to resolve card;
- time to pause Run;
- time to Emergency Stop;
- handoff setup time.

#### Interruption

- draft loss rate;
- resumption lag;
- wrong-context send rate;
- stale approval attempt rate;
- Resume Capsule usefulness;
- return-to-primary-task rate;
- number of forced rereads.

#### Attention

- notifications per day;
- toast open rate;
- mute/snooze rate;
- ignored urgent rate;
- false urgency;
- interruption acceptance;
- DND override rate;
- digest usefulness.

#### Quality

- correct target/project/session;
- correct action from notification;
- correct mobile/desktop handoff;
- capture classification accuracy;
- voice command commit accuracy;
- diff review error rate;
- offline conflict rate.

#### Reliability

- process-death recovery;
- queued command duplication;
- notification action idempotency;
- stale state display;
- unknown effect handling;
- sync recovery;
- upload resume;
- widget freshness.

#### Cost and resource

- battery drain;
- mobile data;
- local storage;
- background CPU;
- push volume;
- model/provider usage caused by UI;
- unnecessary refresh/model calls.

#### Accessibility metrics

- screen reader task success;
- switch access success;
- large text overflow;
- touch target misses;
- one-handed reach;
- reduced motion completeness;
- caption/transcript completeness.

### 37.3. Required usability tests

#### Five-second glance test

User should identify:

- whether attention is needed;
- whether important work runs;
- whether system is offline/degraded.

#### Ten-second decision test

For a simple Inbox card, user understands target, consequence and primary action without opening technical details.

#### Capture-while-walking test

User starts capture, records thought/photo and safely pockets device without organising metadata.

#### Interrupted composer test

Call/lock/process death occurs after text and attachments are entered. Draft returns correctly.

#### Cross-device test

Open mobile card, move diff to desktop, resolve from phone, verify one canonical result.

#### Offline test

Capture, note and command are created offline; after reconnect no duplicate or silent stale action occurs.

#### Notification test

Quick action works without opening app, handles already-resolved revision and gives feedback.

#### Accessibility test

All core tasks complete with screen reader, large text and external input.

#### Safety test

A malicious shared webpage cannot turn share text into permission or external command.

### 37.4. Comparative baselines

Compare:

- full app only;
- app + widgets;
- app + notification actions;
- app + voice;
- fixed Home;
- adaptive Home;
- no Resume Capsule;
- review-only capsule;
- review + preview;
- all notifications immediate;
- urgency/context-aware delivery;
- manual handedness;
- adaptive handedness.

### 37.5. Acceptance gates

Feature accepted when:

- improves target scenario;
- does not increase serious errors;
- does not create parallel state;
- has offline/degraded behavior;
- has accessibility path;
- has clear privacy behavior;
- has removal/disable path;
- does not materially increase notification fatigue.

### 37.6. Rejection gates

Reject or redesign if:

- user repeatedly opens desktop immediately because mobile view is unusable;
- important command requires hidden long-press;
- draft loss occurs;
- live update remains after operation ended;
- notification action gives ambiguous completion;
- adaptive layout moves controls during interaction;
- biometric prompts appear too early/often;
- background work drains battery disproportionately;
- one percent quality gain requires several extra taps on every action;
- a complex screen has no `Continue on Desktop` escape.

---

## 38. Поэтапная реализация

### Phase 1 — Fast trusted remote

- pairing;
- Home;
- Projects;
- project overview/chat;
- Action Inbox;
- notifications;
- Quick Capture;
- Push-to-Talk;
- basic Activity;
- System Status;
- offline drafts;
- biometric approvals;
- Emergency Stop.

### Phase 2 — Interruption-safe continuity

- Resume Capsule;
- What Changed While Away;
- process-death recovery;
- notification quick actions;
- widgets;
- shortcuts;
- share extension;
- handoff to desktop;
- offline queue and reconciliation.

### Phase 3 — Mobile project control

- Runs;
- compact diff review;
- artifact viewers;
- memory search/edit;
- project capabilities;
- automation control;
- device/sync management;
- Pocket Brief.

### Phase 4 — Rich voice and ambient

- background Voice Session;
- lock-screen controls;
- headset/wearable;
- local wake path;
- ambient privacy modes;
- meeting controls;
- cross-device voice handoff.

### Phase 5 — Advanced adaptive surfaces

- Live Activities/Live Updates;
- adaptive notification timing;
- handedness adaptation;
- foldable/tablet two-pane mode;
- car mode integration;
- contextual Action Dock;
- nearby device resume.

Later phases do not block a useful first release.

---

## 39. Screen and command completeness checklist

Каждый mobile screen до реализации обязан иметь:

1. purpose;
2. owner of canonical state;
3. entry points;
4. exit and back behavior;
5. top app bar actions;
6. bottom/primary actions;
7. overflow menu;
8. item context menu;
9. swipe actions;
10. long-press actions;
11. selection/multi-select;
12. search/filter/sort;
13. loading empty;
14. loading cached;
15. fresh;
16. stale;
17. offline;
18. partial;
19. empty;
20. search empty;
21. restricted;
22. conflict;
23. recoverable error;
24. blocking error;
25. process-death recovery;
26. permission/biometric path;
27. notification/deep-link path;
28. widget/shortcut path, если применимо;
29. accessibility semantics;
30. large text;
31. one-handed path;
32. privacy on lock screen/task switcher;
33. analytics/evaluation events;
34. `Continue on Desktop`, если mobile cannot complete well;
35. help/Explain State;
36. undo/cancel/reconciliation.

### 39.1. Core screen inventory

#### System surfaces

- launcher shortcuts;
- widgets;
- lock-screen widgets;
- Live Activity/Live Update;
- notification actions;
- Quick Settings/Control Center;
- share extension;
- deep links;
- wearable/car surfaces.

#### Global app

- Lock/Pairing;
- Home;
- Search/Command;
- Quick Action sheet;
- Profile;
- Settings;
- System Status;
- Emergency Stop.

#### Work

- Orchestrator/Quick Chat;
- Projects Hub;
- Project Overview;
- Project Chat;
- Sessions;
- Changes/Review;
- Runs/Tasks;
- Artifact viewer.

#### Decisions and activity

- Action Inbox;
- Inbox card types;
- Activity Now;
- Notifications;
- History;
- Run detail;
- Agent detail.

#### Capture and voice

- Quick Capture;
- Camera;
- Voice Note;
- Capture staging;
- Voice start sheet;
- Full Voice Session;
- Voice mini controls;
- Handoff.

#### Knowledge and capabilities

- Memory Home/Search/Item/Edit/Evidence;
- Artifacts;
- Capabilities;
- Capability candidate;
- Providers/connections;
- Automations/events.

#### Administration

- Devices;
- Device detail;
- Sync;
- Offline Queue;
- Backup;
- Usage;
- Diagnostics;
- Accessibility diagnostics;
- Onboarding/recovery.

---

## 40. Definition of Done

Mobile business logic считается готовой к программной архитектуре, когда:

1. пользователь может получить значимое состояние за несколько секунд;
2. частые действия доступны без полного запуска приложения там, где это безопасно;
3. все основные captures сначала надёжно сохраняются локально;
4. interruption не уничтожает draft, position или intent;
5. Resume Capsule возвращает пользователя в работу без полного перечитывания;
6. notifications не дублируют Action Inbox;
7. notification action является idempotent command, а не локальным fake state;
8. offline state честно показывает freshness;
9. queued external action revalidates after reconnect;
10. voice, touch, widget и shortcut вызывают одни command contracts;
11. phone может быть trusted approval remote без превращения в master secret store;
12. high-risk confirmation показывает exact target/effect и использует step-up;
13. mobile project chat сохраняет прямую модель работы с project agent;
14. сложная desktop-задача имеет понятный handoff;
15. one-handed и low-attention paths проверены;
16. screen reader, large text, reduced motion и external input покрывают core flows;
17. process death и OS background limits не приводят к скрытым потерям;
18. battery/data costs измерены;
19. every screen passes completeness checklist;
20. no mobile feature creates a parallel source of truth.

---

## 41. Итоговая нормативная формула

> **Denet Mobile — это interruption-safe trusted remote персональной агентной системы. Он оптимизирует не время, проведённое в приложении, а время до безопасного полезного результата: показывает только существенное, сохраняет любой capture до его организации, даёт быстро принять решение, позволяет голосом управлять Denet, честно работает offline, восстанавливает контекст после прерывания и без трения передаёт сложную работу подходящему устройству.**

---

# Appendix A. Source Ledger

## Internal Denet specifications

**[S01] Denet Functional Concept.** Роль телефона как пульта, voice, capture, projects, Action Inbox, Radar, memory и offline emergency mode.  
`00_Denet_Functional_Concept.md`

**[S02] Denet Specification Index and Shared Contracts.** Ownership документов, canonical state и граница mobile UI.  
`01_Denet_Specification_Index_and_Shared_Contracts.md`

**[S03] Denet Memory Fabric 1.2.** Offline event log, capture, memory search, correction, deletion и project memory.  
`10_Denet_Memory_Fabric.md`

**[S04] Denet Pragmatic Agentic Control Fabric 1.1.** Project Session, Work Item, Task, Run, direct/managed execution, interruptions and completion.  
`20_Denet_Agentic_Control_Fabric.md`

**[S05] Denet Trust, Identity, Autonomy and Permissions.** Trusted mobile, biometric step-up, grants, external effects, voice identity и Emergency Stop.  
`30_Denet_Trust_Identity_Autonomy_and_Permissions.md`

**[S06] Denet Voice and Ambient Interaction Fabric.** Voice Session, PTT/live/meeting, interruption, ambient, device handoff и UI requirements.  
`40_Denet_Voice_and_Ambient_Interaction_Fabric.md`

**[S07] Denet Capabilities, Providers and Integrations.** Providers, skills, MCP, connectors, mobile computer-use backends и lifecycle.  
`41_Denet_Capabilities_Providers_and_Integrations.md`

**[S08] Denet Server Runtime, Events, Sync and Portability.** Head Runtime, devices, Action Inbox/Radar authority, offline queue, sync, backup и effect reconciliation.  
`50_Denet_Server_Runtime_Events_Sync_and_Portability.md`

**[S09] Denet Desktop Application Business Logic.** Adaptive Agent Workbench, Resume Strip, Quick Capture, Inbox, Radar, Handoff и shared command semantics.  
`60_Denet_Desktop_Application_Business_Logic.md`

## Android platform surfaces

**[S10] Android Developers — App Widgets Overview.** Widgets как glanceable surface с раскрытием подробностей в приложении, size/adaptive guidance. Accessed 12 July 2026.  
https://developer.android.com/develop/ui/views/appwidgets/overview

**[S11] Android Developers — Live Updates.** Требования к ongoing, user-initiated и time-sensitive activity; запрет использования как общего shortcut/dashboard. Accessed 12 July 2026.  
https://developer.android.com/develop/ui/views/notifications/live-update

**[S12] Android Developers — App Shortcuts.** Static, dynamic и pinned shortcuts, launcher limits и direct action entry.  
https://developer.android.com/develop/ui/views/launch/shortcuts

**[S13] Android Developers — Build a Notification.** Action buttons, background action и direct reply.  
https://developer.android.com/develop/ui/views/notifications/build-notification

**[S14] Android Developers — Quick Settings Tiles.** Быстрые системные actions и поведение на locked device.  
https://developer.android.com/develop/ui/views/quicksettings-tiles

**[S15] Android Developers — Receive Simple Data from Other Apps.** Share target, preview и подтверждение перед использованием shared content.  
https://developer.android.com/training/sharing/receive

**[S16] Android Developers — Adaptive Navigation.** Navigation bar для compact и navigation rail для expanded window classes.  
https://developer.android.com/develop/ui/compose/layouts/adaptive/build-adaptive-navigation

**[S17] Android Developers — Biometric Authentication.** Platform biometric/device credential для step-up.  
https://developer.android.com/identity/sign-in/biometric-auth

**[S18] Android Developers — Notification Runtime Permission.** Contextual request и пользовательский контроль notification permission.  
https://developer.android.com/develop/ui/views/notifications/notification-permission

## Apple platform surfaces

**[S19] Apple Developer — WidgetKit, ActivityKit, App Intents and UserNotifications.** Widgets, Live Activities, shortcuts/actions и notifications. Accessed 12 July 2026.  
https://developer.apple.com/documentation/widgetkit  
https://developer.apple.com/documentation/activitykit  
https://developer.apple.com/documentation/appintents  
https://developer.apple.com/documentation/usernotifications

**[S20] Apple Shortcuts User Guide.** Siri, Action button, Control Center, Home Screen, Search, Watch и Back Tap как action surfaces.  
https://support.apple.com/guide/shortcuts/welcome/ios

## Product case studies

**[S21] GitHub Mobile.** Mobile как surface для high-impact work: notifications triage, issue/PR review, code/search и Copilot from anywhere.  
https://docs.github.com/en/get-started/using-github/github-mobile

**[S22] Notion — Mobile Widgets.** Прямой доступ к recent/favorite pages и AI chat/camera/voice.  
https://www.notion.com/help/mobile-widgets

**[S23] Linear — Inbox.** List/detail, reminders, snooze и быстрые локальные действия.  
https://linear.app/docs/inbox

**[S24] OpenAI — Voice Mode FAQ.** Mobile voice, background conversation, interruption, text/images и session controls. Current documentation accessed 12 July 2026.  
https://help.openai.com/en/articles/8400625-voice-mode-faq

## Interruption, attention and resumption research

**[S25] Mitigating the Effects of Reading Interruptions by Providing Reviews and Previews.** Reviews preferred after interruption; previews improved comprehension in the reported studies. 2021.  
https://arxiv.org/abs/2104.06603

**[S26] Student Programming Behavior with and without Phone Notification Suppression.** Notification suppression associated with lower break rates and longer focus for many, but effects were bimodal across participants. 2026 preprint.  
https://arxiv.org/abs/2605.22657

**[S27] Continual Prediction of Notification Attendance with Classical and Deep Network Approaches.** Notification attendance prediction from real mobile usage logs. 2017.  
https://arxiv.org/abs/1712.07120

**[S28] The Impact of Private and Work-Related Smartphone Usage on Interruptibility.** Interruptibility depends on roles and individual strategies. 2019.  
https://arxiv.org/abs/1907.04739

**[S29] When Users Change Their Mind: Evaluating Interruptible Agents in Long-Horizon Web Navigation / InterruptBench.** Addition, revision и retraction остаются сложными для сильных agents. 2026 preprint.  
https://arxiv.org/abs/2604.00892

**[S30] Action Bar Adaptations for One-Handed Use of Smartphones.** One-handed action placement improved speed, comfort and grip stability in the reported study. 2022.  
https://arxiv.org/abs/2208.08734

**[S31] Adaptive App Design by Detecting Handedness.** Dynamic handedness-aware UI methodology and design guidance. 2018.  
https://arxiv.org/abs/1805.08367

## Accessibility, background and workload

**[S32] W3C — Web Content Accessibility Guidelines 2.2.** Target size, dragging alternative, accessible authentication, focus, timing, status and multimodal accessibility. W3C Recommendation, 2024.  
https://www.w3.org/TR/WCAG22/

**[S33] Android Accessibility Guidance.** Platform accessibility APIs, touch targets, semantics and testing.  
https://developer.android.com/guide/topics/ui/accessibility

**[S34] Android Developers — WorkManager / Persistent Work.** Constraints, retries и system-managed persistent background work.  
https://developer.android.com/develop/background-work/background-tasks/persistent/getting-started/define-work

**[S35] On the Impact of Interruptions During Multi-Robot Supervision Tasks.** Extrinsic interruptions increased perceived workload and were harder to switch from than intrinsic interruptions. 2023.  
https://arxiv.org/abs/2306.16501

---

# Appendix B. Research conclusions mapped to design

1. **Widgets are glance surfaces, not full mini-apps** → compact state and direct entry only. [[S10]]
2. **Live status surfaces are reserved for active user-relevant work** → no permanent agent dashboard on lock screen. [[S11]] [[S19]]
3. **Notifications can perform actions without full launch** → quick resolve and direct reply, but server acknowledgement remains authoritative. [[S13]]
4. **Mobile interruption is normal** → drafts, departure snapshot, Resume Capsule and review/preview. [[S25]]
5. **Notification suppression has heterogeneous effects** → user-controlled and adaptive attention policy, not universal DND. [[S26]]
6. **Interruptibility is contextual** → timing learns from usage but remains explainable and reversible. [[S27]] [[S28]]
7. **Agent interruptions are semantically different** → Add, Revise, Retract and Stop are separate commands. [[S29]]
8. **One-handed reach matters** → bottom Action Dock, handedness settings and Walking Mode. [[S30]] [[S31]]
9. **High-impact mobile products focus on triage/review rather than complete desktop parity** → Mobile Denet acts as trusted remote, not mini-IDE. [[S21]]
10. **External interruptions increase workload** → quiet completion, digest and intrinsic/contextual notifications. [[S35]]

Конец документа.
