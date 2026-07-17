
> **HISTORICAL · DISTRIBUTED · NON-CANONICAL**  
> The normative sections of this document were split into `docs/specifications/contracts/`. Use those files and the canonical domain documents. This copy is retained only for provenance and diff review.

# Dennett — временное завершение бизнес-логики перед программной архитектурой

## Единый нормативный файл для закрытия обнаруженных пробелов и последующего разнесения по каноническим спецификациям

**Каноническое временное имя:** `71_Dennett_Pre_Architecture_Business_Logic_Completion.md`  
**Версия:** 1.0  
**Дата исследования:** 12 июля 2026 года  
**Статус:** временный нормативный документ. Его положения обязательны для архитектурного этапа, но после стабилизации архитектуры должны быть распределены по каноническим файлам бизнес-логики согласно карте переноса в конце документа.  
**Область:** только бизнес-логика, пользовательски значимые гарантии, жизненные циклы и междокументные контракты. Документ не выбирает язык программирования, БД, очередь, UI-фреймворк, формат RPC или конкретную deployment-платформу.

---

# Краткий итог

Перед проектированием архитектуры Dennett оставалось не создать ещё несколько крупных подсистем, а **связать уже принятые части продукта в несколько недостающих сквозных контрактов**. Главная опасность состояла в двух крайностях:

1. оставить важные действия недоопределёнными и позволить будущей архитектуре самой изобретать продуктовую логику;
2. превратить каждый пробел в новый сервис, сложную state machine или жёсткую предметную модель.

Лучшее решение — один временный файл из самостоятельных нормативных модулей. Каждый модуль:

- объясняет проблему без знания истории обсуждения;
- задаёт минимально достаточные сущности и жизненный цикл;
- показывает основной путь, ошибки и восстановление;
- определяет связь с уже существующими Memory, Agentic, Trust, Voice, Capability, Server, Desktop и Mobile Fabric;
- содержит критерии принятия и отказа;
- заканчивается точной картой будущего переноса.

Закрываются следующие области:

1. единый контракт постоянной сенсорики: микрофон, экран, камера, clipboard и activity context;
2. внешняя коммуникация: получение, подготовка, отправка, доставка и исправление сообщений;
3. полный жизненный цикл проекта;
4. полный жизненный цикл артефакта;
5. обновления, совместимость протоколов и миграции;
6. восстановление владельца, устройств и ключей;
7. давление ресурсов и единый учёт потребления;
8. федеративный глобальный поиск;
9. locale, язык, часовой пояс, поездки и календарное время;
10. переносимые пакеты, import/export и совместимость форматов;
11. адаптивные функции высокого уровня — идеи, обзоры, исследования и технологическая разведка — как композиции существующих возможностей, а не отдельные платформы.

Главная формула этого документа:

> **Dennett должен быть богатым по возможностям, но экономным по фундаментальным сущностям. Новая долговечная сущность вводится только тогда, когда без неё невозможно однозначно сохранить состояние, восстановиться после отказа, защитить пользователя или перенести данные. Всё остальное по возможности собирается из памяти, skills, prompts, событий, capabilities и артефактов.**

---

# Часть I. Назначение, метод и нормативные границы

## 1. Почему существует этот временный документ

Комплект бизнес-логики Dennett уже определяет:

- функциональное видение персональной агентной операционной системы; [[S01]]
- карту канонических владельцев и общие контракты; [[S02]]
- доказательную, адаптивную и федеративную память; [[S03]]
- single-agent-first работу проектов, задач и агентов; [[S04]]
- доверие, идентичность, разрешения и регулируемую автономность; [[S05]]
- голосовой и ambient-режим; [[S06]]
- providers, модели, skills, MCP, plugins, connectors и другие capabilities; [[S07]]
- постоянный серверный runtime, события, sync, offline и backup; [[S08]]
- desktop- и mobile-интерфейсы; [[S09]] [[S10]]
- end-to-end проверку готовности к архитектуре. [[S11]]

Итоговый аудит обнаружил, что ряд функций описан в нескольких файлах одновременно, а некоторые операции имеют UI-команды и отдельные фрагменты поведения, но ещё не имеют единого канонического жизненного цикла. [[S11]]

Этот файл не заменяет существующие спецификации. Он временно владеет только недостающими нормативными дельтами. При последующем формировании репозитория каждый модуль будет перенесён в указанный канонический документ, а этот файл останется в архиве исследования.

## 2. План для построения плана

Перед исследованием были зафиксированы вопросы, которые не позволяют превратить работу в формальное заполнение пробелов.

### 2.1. Что должен позволить итоговый файл

После его чтения архитектор должен без продуктовых догадок ответить:

- что именно означает «постоянно слушать» и «сохранять экран» на разных устройствах;
- как отличить входящее сообщение, черновик, разрешённую отправку и подтверждённую доставку;
- чем архивирование проекта отличается от отвязки, удаления checkout и удаления remote repository;
- как версия артефакта становится финальной, публикуется, заменяется или отзывается;
- как обновить один компонент Dennett, не сломав старое устройство, данные или plugins;
- как владелец восстановит систему после потери устройств, не открывая простой путь захвату аккаунта;
- что происходит при заполнении диска, исчерпании API-бюджета, перегреве или слабой сети;
- как один поиск объединяет проекты, память, артефакты, сообщения, capabilities и команды без утечки данных;
- как должны интерпретироваться «завтра в 9», смена часового пояса и смешанный язык;
- как безопасно передать проект, skill, настройки или исследование другому пользователю;
- как реализовать Idea Incubator, Briefing или AI News Radar без новой жёсткой подсистемы под каждую функцию.

### 2.2. Какие решения особенно трудно изменить позднее

К высокоимпактным относятся:

- смысл project deletion и artifact deletion;
- различие канонического и производного состояния;
- модель ключей и recovery;
- правила сохранения ambient-данных;
- формат переносимого пакета и его version negotiation;
- семантика отправленного внешнего эффекта;
- режимы времени и повторяющихся расписаний;
- граница между глобальным поиском и Memory Fabric;
- правила совместимости клиентов, сервера и extensions.

### 2.3. Какие гипотезы проверялись

1. Постоянный microphone и screen capture можно реализовать одинаково на всех ОС.  
   **Результат:** неверно. ОС имеют разные модели разрешений, фоновой работы и пользовательских индикаторов. [[S12]] [[S13]] [[S14]]

2. Достаточно хранить весь поток и разбираться с ним позже.  
   **Результат:** отклонено из-за стоимости, приватности, батареи, duplicate content и низкого signal-to-noise. Сохраняется локальный ring buffer, а долговечное хранение начинается после commit decision.

3. Все внешние каналы можно представить одной операцией `send(text)`.  
   **Результат:** неверно. Каналы различают drafts, threads, edits, acknowledgements, unknown delivery, local state и update-driven reconciliation. [[S19]] [[S20]] [[S21]] [[S22]]

4. Project lifecycle можно оставить UI-командам.  
   **Результат:** неверно. Archive, detach, remove checkout, delete local files и delete remote имеют разный blast radius и восстановимость. [[S24]] [[S25]]

5. Artifact можно описать одним полем `status`.  
   **Результат:** недостаточно. Мaturity, publication, availability и trust — независимые измерения.

6. Одного номера версии приложения достаточно для совместимости.  
   **Результат:** неверно. Независимо версионируются приложение, protocol, event/schema, portable package, plugin API, capability и данные. [[S30]] [[S31]] [[S32]] [[S33]]

7. Recovery можно свести к reset password.  
   **Результат:** неверно для локально и end-to-end зашифрованной персональной системы. Нужно отдельно восстанавливать identity, device trust, data keys, Head authority и Secret Broker. [[S34]] [[S35]] [[S36]]

8. Global search должен копировать всё в один индекс.  
   **Результат:** отклонено. Правильнее federated query planning с единым SearchHit и несколькими authority-preserving источниками.

9. Все высокоуровневые функции требуют отдельных подсистем.  
   **Результат:** отклонено. Большинство собирается как Composite Experience Recipe поверх skills, memory queries, automations и artifacts. Похожую идею переиспользуемого шаблона с последующей пользовательской настройкой демонстрируют Home Assistant blueprints. [[S49]]

## 3. Исследовательский протокол

### 3.1. Слои evidence

Использовались:

- актуальные канонические документы Dennett;
- официальная документация ОС и платформ;
- стандарты и спецификации;
- production-паттерны зрелых систем;
- научные работы по voice consent, privacy и security;
- реальные ограничения API, delivery и recovery;
- примеры интерфейсных и операционных жизненных циклов.

### 3.2. Критерий принятия решения

Механизм принят, если он:

- закрывает конкретный наблюдаемый failure mode;
- имеет более простой baseline и объясняет, почему его недостаточно;
- не требует LLM-вызова на каждом обычном действии;
- сохраняет пользовательский контроль без микроменеджмента;
- позволяет восстановление или честно объявляет необратимость;
- имеет одного владельца состояния;
- не дублирует существующую Memory, Agentic, Trust или Server сущность;
- может быть реализован постепенно;
- не ухудшает cost-of-success ради формальной красоты.

### 3.3. Критерий отмены или упрощения

Механизм должен быть удалён, ослаблен или оставлен optional, если он:

- создаёт больше состояний, чем реальных пользовательских различий;
- требует отдельной модели для нормального fast path;
- заставляет пользователя обслуживать систему;
- копирует данные в новый «источник истины»;
- не даёт измеримого улучшения над prompt/skill/простым adapter;
- невозможно объяснить пользователю;
- не может корректно обработать offline, retry, deletion или update;
- превращает персональную систему в enterprise-платформу без соответствующей потребности.

## 4. Общие нормативные принципы

1. **Наблюдение не является памятью.** Raw sensor frame, partial transcript или network update становятся долговечными только после явного commit decision.
2. **Релевантность не является authority.** Найденный текст, письмо, skill или imported package не выдаёт права.
3. **Один объект может иметь несколько представлений, но один канонический identity.**
4. **Удаление, архивирование, отвязка и скрытие всегда различаются.**
5. **Версия payload неизменяема; логическая сущность может указывать на новую версию.**
6. **Нельзя считать запрос выполненным только потому, что connector принял HTTP-запрос.** Delivery и reconciliation являются отдельными этапами.
7. **Обновление не получает права ломать данные молча.** Перед необратимой миграцией нужен проверяемый recovery point.
8. **Пользовательский manual import принимается без utility-бюрократии, но не превращается автоматически в trusted executable code.**
9. **Система не обещает одинаковые возможности на всех платформах.** Она показывает capability state и допустимый degraded path.
10. **Высокоуровневая функция сначала реализуется как композиция существующих механизмов.** Новая подсистема создаётся только после повторяющегося failure mode.

## 5. Формат модулей и будущий перенос

Каждый модуль имеет неизменную структуру:

- проблема и граница;
- итоговое решение;
- сущности;
- инварианты;
- основной жизненный цикл;
- управление пользователем;
- сбои и восстановление;
- observability и evaluation;
- антиоверинижиниринговые ограничения;
- карта будущего переноса.

В будущем текст переносится не копированием вслепую, а по нормативным блокам. При конфликте действует более новая явная дельта этого документа до момента её переноса в канонический файл.

---

# Часть II. Недостающие нормативные модули

# Модуль A. Ambient Sensory Capture Contract

## A.1. Назначение и граница

Ambient Sensory Capture объединяет фоновое восприятие:

- микрофона телефона и компьютера;
- экрана и активного окна;
- accessibility tree, DOM и selected text;
- clipboard;
- камеры или фотографии по явной активации;
- состояния приложения, focus и device context.

Это **не новая память** и не новая агентная система. Контракт определяет путь от источника до `Ambient Candidate`; Memory Fabric определяет долговременное представление, Trust — допустимость, Server — исполнение и sync, Voice — разговор и audio semantics, UI — управление.

## A.2. Итоговое решение

> **Каждый сенсорный источник работает локально по ступенчатому pipeline. По умолчанию сохраняется только короткий перезаписываемый buffer и дешёвые метаданные. Долговечный объект создаётся только после событийного, пользовательского или семантического commit decision.**

Для audio и screen существуют два равноправных runtime flow. Они используют общий control contract, но не обязаны иметь одинаковый backend. Screenpipe демонстрирует практически полезный вариант event-driven локального screen/audio capture с OCR/accessibility и поиском; Dennett использует этот опыт как reference, но не делает конкретный проект обязательной зависимостью. [[S15]]

## A.3. Ограничения платформ являются частью бизнес-логики

Dennett не должен показывать единый переключатель «записывать всё», если ОС не может выполнить обещание.

- На Android длительный microphone capture требует foreground service соответствующего типа и видимого пользователю состояния; while-in-use ограничения влияют на запуск из background. [[S12]]
- Android MediaProjection требует пользовательского consent для projection session; на новых версиях projection token одноразовый, поэтому постоянный невидимый screen capture телефона нельзя считать базовой гарантией. [[S13]]
- Windows Graphics Capture использует системный picker и визуальную рамку вокруг захватываемого объекта, что поддерживает явную осведомлённость пользователя. [[S14]]
- На iOS background-аудио и запись экрана зависят от системных background modes, microphone permissions и ReplayKit-session; Dennett не обещает скрытый бесконечный recorder там, где платформа разрешает только явно активную сессию. [[S51]] [[S52]]
- На macOS ScreenCaptureKit является предпочтительным native-кандидатом, но permission и пользовательская осведомлённость остаются обязательными. [[S53]]
- На Linux и других системах capability определяется текущими desktop portal/OS permissions, API и deployment policy; отсутствие capability не маскируется.

Каждый источник имеет `support_level`:

```text
native-continuous
native-session
foreground-only
manual-only
structured-context-only
unsupported
blocked-by-policy
```

## A.4. Канонические сущности

### Sensor Source

Стабильное описание источника:

```yaml
sensor_source:
  source_id: id
  source_type: microphone | screen | camera | clipboard | accessibility | app_activity
  device_ref: ref
  support_level: typed
  os_permission_state: typed
  physical_state: available | muted | blocked | disconnected
  capture_profile_ref: ref
  privacy_profile_ref: ref
  resource_budget_ref: ref
  health: typed
```

### Capture Profile

Определяет:

- local-only или sync-eligible;
- ring buffer duration;
- raw capture policy;
- structured-first или pixel/audio-first;
- event signals;
- deduplication threshold;
- commit threshold;
- retention class;
- exclusions;
- model/backend policy;
- battery/network/thermal constraints.

### Ephemeral Buffer

Короткая локальная область, которая:

- автоматически перезаписывается;
- не индексируется глобально;
- не считается Memory Event;
- не доступна обычному project agent;
- может быть committed по явной команде «запомни последние N секунд»;
- уничтожается при выключении source, истечении времени или policy change.

### Ambient Candidate

Первый объект, который может быть передан Memory/Events:

```yaml
ambient_candidate:
  candidate_id: id
  source_refs: []
  observed_interval: time_range
  source_scope: device/app/window/conversation
  speaker_hypotheses: []
  structured_context_refs: []
  raw_object_refs: []
  reason_for_candidate: explicit | trigger | anomaly | project_relevance | meeting
  relevance_confidence: number
  privacy_class: typed
  suggested_action: discard | commit | wake | notify | create_event | ask
  expires_at: time
```

### Committed Sensory Evidence

После commit Memory Fabric создаёт Evidence Object/Event. Original raw и derived transcript/OCR сохраняются раздельно и могут иметь разные retention.

## A.5. Жизненный цикл источника

```text
UNAVAILABLE
→ PERMISSION_REQUIRED
→ STOPPED
→ STARTING
→ ACTIVE_LOCAL
↔ ACTIVE_COMMITTING
↔ DEGRADED
↔ PAUSED_BY_USER
↔ PAUSED_BY_POLICY
↔ PAUSED_BY_RESOURCE
→ INTERRUPTED_BY_OS
→ STOPPED
→ REVOKED / ERROR
```

Инварианты:

- physical mute и OS revocation всегда сильнее app state;
- UI не показывает `Active`, пока runtime не подтвердил реальный источник;
- restart не включает источник молча, если OS требует новое consent;
- смена профиля фиксируется как policy event;
- источник не продолжает capture после revoke, даже если модель «считает это полезным».

## A.6. Ambient Audio flow

```text
OS microphone
→ acoustic echo cancellation / noise suppression where available
→ local VAD
→ short ring buffer
→ speaker association hypothesis
→ wake-word / explicit address detection
→ cheap transcript or acoustic event detection
→ duplicate and low-value suppression
→ candidate creation only when justified
→ privacy and consent gate
→ discard / wake Voice Session / commit / create Event
```

### A.6.1. Что считается полезным сигналом

- явное обращение к Dennett;
- «запомни» или другая configured command;
- пользователь наговаривает идею;
- разговор связан с активным проектом;
- обещание, решение или срок;
- meeting profile активен;
- пользователь просит сохранить предыдущий фрагмент;
- событие имеет высокую срочность или заранее определённый trigger;
- неожиданный сбой устройства или приложения создаёт диагностический audio context.

### A.6.2. Что не должно постоянно уходить в модель

- тишина;
- постоянный фон;
- повторяющийся звук;
- уже обработанная неизменившаяся речь из overlap buffer;
- чужой разговор без связи с пользователем;
- media playback, если не включён специальный режим;
- один и тот же фрагмент, увиденный несколькими устройствами.

### A.6.3. Multi-device arbitration

Если телефон и ПК слышат одну речь:

1. источники обмениваются lightweight fingerprints и time ranges;
2. выбирается primary source по качеству, proximity, active-device context и privacy policy;
3. secondary source может дать acoustic evidence, но не создаёт дубликат;
4. при неопределённости события объединяются через shared candidate;
5. Voice Session имеет одного conversational owner.

## A.7. Event-driven Screen Context flow

```text
OS/app activity event
→ active app/window/document identity
→ structured context first: accessibility tree / DOM / selected text
→ visual change detection
→ screenshot/keyframe only when structured data insufficient or visual state meaningful
→ sensitive-region detection/redaction
→ duplicate suppression
→ project/task association
→ Ambient Candidate
→ discard / commit / create Event / attach to active Run
```

### A.7.1. Сигналы захвата

- смена приложения или окна;
- открытие нового документа/страницы;
- существенный visual diff;
- error/dialog;
- selection/copy/paste;
- пауза после действия;
- начало или конец Task;
- UI test checkpoint;
- explicit hotkey;
- просьба агента сохранить evidence;
- пользователь возвращается к ранее важному экрану.

### A.7.2. Structured-first

Если accessibility/DOM даёт:

- текст;
- структуру элементов;
- URL/document identity;
- выбранный объект;
- control state;

то screenshot не обязателен. Pixel capture используется для canvas, изображения, layout, диаграммы, видео, visual bug или проверки фактического рендера.

### A.7.3. Screen exclusions

Поддерживаются:

- app/window blacklist;
- private browsing profile;
- password/payment/authentication screen detection;
- full-screen protected content;
- per-project exclusion;
- «не сохранять следующие N минут»;
- local-only encrypted capture;
- capture без OCR/without cloud processing;
- manual-only source.

Чувствительный screenshot может сохраняться только при явной политике и получает отдельную access/retention class. Безопасное текстовое описание может быть доступно поиску, а raw — только после step-up.

## A.8. Cross-modal correlation

Audio, screen, clipboard и activity связываются общим `Observation Window`, если:

- временные интервалы пересекаются;
- активны тот же project/task/app;
- source identity согласована;
- объединение улучшает смысл.

Пример:

```text
пользователь видит ошибку в IDE
+ проговаривает «это появилось после обновления»
+ копирует stack trace
= один составной sensory episode
```

Объединение не копирует raw objects в один blob. Оно создаёт связи и shared episode handle.

## A.9. Consent и privacy boundary

Dennett не является юридическим советником и не может иметь один мировой закон в коде. Он обязан поддержать policy dimensions, необходимые для разных условий. NIST Privacy Framework рассматривает privacy как управляемый риск, который должен сочетаться с инновацией, а EDPB отдельно анализирует требования к voice assistants. [[S16]] [[S17]]

Раздельно управляются:

1. **Collection** — можно ли получать сигнал от источника.
2. **Transient processing** — можно ли локально анализировать ring buffer.
3. **Cloud processing** — можно ли отправлять фрагмент внешнему provider.
4. **Commit** — можно ли сохранить долговечно.
5. **Identification** — можно ли связывать голос/лицо с человеком.
6. **Sharing** — можно ли передавать другим людям или проектам.
7. **Retention** — как долго хранить raw и derived.
8. **Model improvement/training** — отдельно и по умолчанию запрещено для приватных данных, если provider contract не гарантирует обратное.

### A.9.1. Consent Record

```yaml
consent_record:
  consent_id: id
  principals_or_context: []
  source_types: []
  purposes: []
  processing_locations: []
  retention: policy
  obtained_by: explicit_ui | spoken_confirmation | meeting_notice | organization_policy
  obtained_at: time
  expires_at: optional
  policy_version: version
  evidence_ref: optional
  revoked_at: optional
```

Молчание другого человека не считается автоматически информированным согласием. Исследования verbal-consent UX также показывают, что смысл согласия зависит от ясности, контекста и понятного последствия, а не от формального наличия голосовой фразы. [[S18]] При неоднозначности Dennett выбирает более ограниченный режим: local transient processing, private user notes без raw recording, либо явный запрос.

### A.9.2. Видимые индикаторы

- OS indicator и системное consent UI не скрываются;
- приложение показывает активные sources;
- meeting profile показывает, ведётся ли запись или только private notes;
- user can mute/pause locally;
- индикатор не должен утверждать «не записывается», если другой registered source активен;
- privacy mode распространяется на все nodes через Server, но локальный stop применяется немедленно до sync.

## A.10. Связь с другими подсистемами

- **Voice:** владеет turn-taking, wake и live conversation; получает committed audio turn, не весь buffer.
- **Memory:** владеет Event/Evidence, search, retention, deletion и multimodal lineage.
- **Trust:** определяет source policy, disclosure и access.
- **Server:** регистрирует source state, event flow, sync и resource pressure.
- **Capabilities:** предоставляет VAD/ASR/OCR/screen adapters.
- **Desktop/Mobile:** показывают controls и state, но не являются authority.
- **Agentic:** agent may request capture for evidence, но request проходит policy.

## A.11. Сбои и восстановление

### Permission revoked

- source немедленно останавливается;
- open buffer стирается по policy;
- UI обновляет реальный state;
- no automatic permission prompt loop;
- existing committed evidence не удаляется автоматически, если пользователь не запросил deletion.

### Source crashes

- состояние становится `DEGRADED`/`ERROR`;
- restart допускается по policy;
- gap interval фиксируется;
- система не создаёт выдуманную continuity.

### Model/provider unavailable

- local gate продолжает cheap processing;
- candidates queue bounded;
- raw buffer не растёт бесконечно;
- low-value candidates expire;
- user can switch local-only or pause.

### Duplicate source

- fingerprint/time correlation объединяет;
- raw duplicate может быть удалён после integrity check;
- provenance обоих sources сохраняется, если важно.

### Sensitive content accidentally committed

- quarantine;
- access revoke;
- deletion graph;
- derived indexes purge/rebuild;
- incident record без сохранения secret content.

## A.12. Observability и evaluation

Измеряются:

- false wake / missed wake;
- доля raw, превращённого в candidate и committed evidence;
- duplicate suppression;
- battery/CPU/GPU/network;
- capture latency;
- privacy policy violations;
- source state accuracy;
- accidental capture rate;
- retrieval usefulness;
- deletion correctness;
- количество ненужных model calls;
- user overrides и disable rate.

## A.13. Антиоверинижиниринговые ограничения

Не создавать:

- отдельного LLM на каждый источник;
- отдельную Task на каждый screenshot;
- единую тяжёлую multimodal session 24/7;
- собственный OS screen-capture framework, если native adapter достаточен;
- обязательное распознавание всех людей;
- вечное хранение raw;
- юридический rule engine с захардкоженными законами всех стран;
- global graph update на каждый audio chunk.

## A.14. Критерии готовности

Модуль готов к архитектуре, если:

- для каждой target OS известен support level;
- источник можно локально остановить без сети;
- ring buffer не считается долговременной памятью;
- duplicate microphone/screen sources объединяются;
- raw/derived retention разделены;
- privacy и cloud processing управляются отдельно;
- storage pressure не приводит к молчаливой потере canonical evidence;
- full pipeline проходит E2E тест от source до deletion.

## A.15. Карта будущего переноса

- `40_Dennett_Voice_and_Ambient_Interaction_Fabric.md`: A.1–A.9 audio/ambient semantics и multi-device floor.
- `50_Dennett_Server_Runtime_Events_Sync_and_Portability.md`: Sensor Source lifecycle, resource state, sync и failure.
- `10_Dennett_Memory_Fabric.md`: committed evidence/retention details и cross-modal episode.
- `30_Dennett_Trust_Identity_Autonomy_and_Permissions.md`: consent/privacy policy and access.
- `60/61 UI`: controls, indicators and settings.
- `70 E2E`: acceptance scenarios.

---

# Модуль B. External Communication Operation

## B.1. Назначение и граница

Этот модуль определяет единую бизнес-логику работы Dennett с внешней перепиской независимо от конкретного канала:

- Telegram и другие мессенджеры;
- электронная почта;
- комментарии, issues, pull requests и рабочие чаты;
- SMS и поддерживаемые телефонные каналы;
- сообщения через provider-native connector;
- ответы в сторонних приложениях через computer-use, если нет надёжного API.

Он не выбирает конкретную библиотеку, connector protocol или UI. Он определяет, **что считается входящим сообщением, черновиком, отправкой, подтверждённой доставкой, неизвестным результатом и ответом от имени пользователя**.

Ключевая граница:

> Формирование содержания, подражание стилю пользователя, решение о раскрытии информации и право отправить сообщение являются четырьмя разными решениями.

Нельзя считать, что хороший текст автоматически можно отправить. Нельзя считать, что прошлое разрешение ответить одному человеку разрешает ответить другому. Нельзя считать локальный timeout доказательством того, что сообщение не ушло.

## B.2. Почему нужен общий контракт

Без общего контракта разные connectors начнут вести себя по-разному:

- один будет считать успешный HTTP-ответ окончательной доставкой;
- другой — создавать сообщение локально до фактической отправки;
- третий — молча повторять отправку после timeout;
- четвёртый — объединять draft и sent state;
- пятый — использовать историю переписки как permission.

TDLib, например, работает асинхронно, хранит локальное состояние и сообщает об успешной отправке отдельным `updateMessageSendSucceeded`; приложение обязано корректно обрабатывать поток updates, а не только ответ на первоначальный request. [[S19]] Telegram Bot API, напротив, возвращает HTTP-result, а при выполнении запроса внутри webhook-response прямо предупреждает, что невозможно узнать, был ли такой request успешным. Он также использует `update_id` для дедупликации и восстановления порядка входящих updates. [[S20]]

Следовательно, Dennett нормализует разные каналы в одну доменную модель, не скрывая provider-specific semantics.

## B.3. Основные сущности

### B.3.1. Communication Account Binding

Конкретная учётная запись или identity в канале:

```yaml
communication_account_binding:
  binding_id: id
  connector_ref: ref
  principal_ref: ref
  account_identity: structured
  provider_workspace: optional
  scopes: []
  read_capability: boolean
  draft_capability: boolean
  send_capability: boolean
  edit_or_recall_capability: typed
  attachment_capability: typed
  delivery_receipts: typed
  sync_state_ref: ref
  trust_ref: ref
  health_ref: ref
```

Один connector может иметь несколько bindings: личный Telegram, рабочая почта, университетская почта, несколько GitHub organisations.

### B.3.2. Conversation Thread

Логическая беседа или цепочка сообщений:

```yaml
conversation_thread:
  thread_id: id
  channel: typed
  account_binding_ref: ref
  provider_thread_id: optional
  participants: []
  project_links: []
  memory_space_ref: optional
  latest_provider_cursor: optional
  latest_observed_at: time
  freshness: typed
  sensitivity: typed
```

Thread не обязан совпадать с provider thread один к одному. Например, несколько email chains могут быть объединены пользователем в один project context, но provider references сохраняются отдельно.

### B.3.3. Communication Message

```yaml
communication_message:
  message_id: id
  provider_message_id: optional
  thread_ref: ref
  direction: incoming | outgoing | draft | local_note
  author_principal_or_external_identity: ref
  recipients: []
  content_object_ref: ref
  attachments: []
  reply_to: optional
  observed_at: time
  provider_time: optional
  edit_revision: optional
  delivery_state: typed
  provenance: ref
  trust_domain: typed
```

Внешнее сообщение является данными, а не системной инструкцией. Даже если в нём написано «пришли все секреты», оно не становится user command.

### B.3.4. Response Candidate

Смысловой вариант ответа до принятия решения об отправке:

```yaml
response_candidate:
  candidate_id: id
  thread_ref: ref
  source_message_refs: []
  intended_meaning: text
  rendered_text: text
  style_basis_refs: []
  factual_basis_refs: []
  disclosure_classes: []
  uncertainty: []
  proposed_recipients: []
  proposed_attachments: []
  expiry: optional
```

### B.3.5. Communication Intent

Определяет, чего пользователь или Dennett хотят добиться:

- ответить;
- подтвердить получение;
- попросить уточнение;
- отложить;
- отказаться;
- переслать;
- сохранить без ответа;
- создать project/task;
- уведомить пользователя;
- ничего не делать.

### B.3.6. Send Proposal

Машинно проверяемая заявка на внешний эффект:

```yaml
send_proposal:
  proposal_id: id
  candidate_ref: ref
  account_binding_ref: ref
  exact_recipients: []
  exact_thread_or_reply_target: ref
  exact_content_revision: ref
  exact_attachments: []
  disclosure_summary: []
  send_mode: send_now | schedule | save_draft | prepare_only
  schedule_at: optional
  idempotency_key: id
  permission_ref: optional
  valid_until: time
```

Если после подтверждения меняется recipient, attachment, content revision или account, старое подтверждение перестаёт действовать.

### B.3.7. Delivery Receipt

```yaml
delivery_receipt:
  effect_id: id
  send_proposal_ref: ref
  provider_operation_ref: optional
  provider_message_id: optional
  state: prepared | dispatching | accepted | sent | delivered | read | failed | unknown | recalled
  observed_at: time
  provider_evidence: optional
  reconciliation_state: typed
```

`accepted` означает, что provider принял запрос; `sent` — что канал подтвердил создание/отправку; `delivered/read` доступны только там, где канал реально их сообщает.

## B.4. Входящий pipeline

```text
connector update/webhook/poll/local client update
→ authenticate source and binding
→ deduplicate provider update
→ normalize thread/message/attachments
→ preserve original evidence
→ update provider cursor and thread freshness
→ classify trust and sensitivity
→ cheap relevance/urgency filters
→ optional semantic interpretation
→ remember / notify / draft / ask / act / do nothing
```

### B.4.1. Дедупликация

Используются:

- provider update ID;
- provider message ID;
- account binding;
- edit revision;
- source sequence;
- content fingerprint только как дополнительный сигнал.

Одинаковый текст, отправленный дважды, не должен автоматически считаться дубликатом. Provider identity важнее semantic similarity.

### B.4.2. Редактирование и удаление входящего сообщения

Редактирование создаёт новую revision, а не переписывает историческое evidence без следа.

Если сообщение удалено у provider:

- operational view отражает deletion;
- локальная retention определяется пользовательской policy;
- Dennett не утверждает, что удаление у provider автоматически удалило все локальные evidence;
- если local retention запрещена, запускается deletion obligation.

### B.4.3. Вложения

Вложение проходит отдельный ingest:

- metadata;
- MIME/type;
- size;
- malware/static scan, если исполняемое;
- sensitivity;
- local/cloud storage policy;
- OCR/transcription при необходимости;
- project link;
- trust domain.

Вложение не исполняется только потому, что пришло от знакомого человека.

## B.5. Реконструкция контекста ответа

Для ответа Dennett формирует `Communication Context Bundle`:

```yaml
communication_context_bundle:
  latest_thread_window: refs
  unresolved_questions: []
  participant_relationship_context: refs
  active_project_context: refs
  promises_and_obligations: refs
  current_authoritative_facts: refs
  relevant_past_user_messages: refs
  disclosure_policy: ref
  communication_preferences: refs
  current_user_availability: optional
  uncertainty_and_conflicts: []
```

### B.5.1. Приоритет текущих источников

Если человек спрашивает о готовности проекта:

- repository/test/runtime state выше старой памяти;
- календарь выше старой заметки о расписании;
- текущий permission выше прошлой привычки;
- provider thread выше summary, если они расходятся.

### B.5.2. Стиль пользователя

Стиль строится из релевантных примеров:

- тот же человек или социальная группа;
- тот же канал;
- похожая ситуация;
- текущая степень формальности;
- исправленные пользователем drafts;
- выраженные preferences.

Нельзя слепо копировать случайные фразы или выдавать личную информацию только ради «персонализации».

### B.5.3. Over-personalization gate

Личная память применяется только если улучшает ответ. Она не должна:

- упоминать ненужные личные факты;
- поддакивать пользователю вместо объективного ответа;
- раскрывать скрытый контекст собеседнику;
- делать ответ странно знакомым там, где пользователь обычно нейтрален.

## B.6. Четыре независимых решения

### B.6.1. Content

Что по существу нужно сообщить.

### B.6.2. Style

Как это сформулировать.

### B.6.3. Disclosure

Какие сведения и attachments допустимо раскрыть конкретному recipient.

### B.6.4. Delivery

Что делать с готовым содержанием:

- не отвечать;
- сохранить draft;
- показать пользователю;
- отправить после подтверждения;
- отправить по standing bounded pattern;
- schedule;
- передать другому каналу.

Эти решения могут использовать одну модель, несколько последовательных checks или часть deterministic rules. Они не обязаны быть четырьмя отдельными model calls.

## B.7. Режимы автономности коммуникации

### Draft-only

Dennett может свободно готовить drafts, но не отправляет.

### Confirm-before-send

Каждый внешний ответ требует подтверждения, но варианты и контекст готовятся автоматически.

### Bounded autonomous

Разрешены конкретные patterns:

- короткое подтверждение определённым людям;
- заранее одобренные автоматические отчёты;
- ответы внутри заданной темы;
- routing служебных уведомлений;
- message templates с лимитами раскрытия.

### Contextual autonomous

Оркестратор может отправлять low-consequence сообщение при высокой уверенности и заранее установленной политике. Этот режим не должен быть default для новых contacts или чувствительных данных.

### Emergency/preauthorized

Отдельные сценарии, заранее определённые пользователем. Нельзя выводить emergency authority из общей памяти или единичной фразы.

## B.8. Ручная команда пользователя

Если пользователь говорит:

> «Ответь Ивану: да, буду в шесть»

Dennett:

1. разрешает identity `Иван` в конкретный contact/thread;
2. показывает ambiguity, если контактов несколько;
3. формирует exact proposal;
4. проверяет current user session/assurance;
5. отправляет без дополнительного utility-review, если permission достаточен;
6. сохраняет receipt.

Пользовательская явная команда не отменяет проверку exact recipient и unknown-effect safety.

## B.9. Draft lifecycle

```text
draft candidate
→ local editable draft
→ provider draft optional
→ user/agent revision
→ approved send proposal
→ sent/scheduled/abandoned/superseded
```

Google Gmail API рассматривает drafts как отдельный ресурс, который затем может быть отправлен; Dennett сохраняет этот conceptual separation даже для каналов без provider-native drafts. [[S21]]

Draft имеет owner:

- user-owned;
- Dennett-generated;
- shared/project;
- provider-managed.

Автоматическое обновление user-edited draft не должно перетирать правки. Новое предложение создаётся как revision или diff.

## B.10. Отправка и reconciliation

### B.10.1. Safe dispatch

```text
freeze exact proposal revision
→ acquire effect claim
→ validate permission and account health
→ dispatch once with idempotency/context handle
→ record provider operation reference
→ wait for provider evidence/update
→ settle receipt
```

### B.10.2. Timeout

Если timeout произошёл после dispatch:

- state становится `UNKNOWN`, если provider не гарантирует отсутствие эффекта;
- повтор не выполняется автоматически;
- connector пытается найти сообщение по provider operation ID, client-generated ID, thread/time/content tuple или provider history;
- при невозможности reconciliation вопрос выносится пользователю только если действительно нужен.

AWS рекомендует caller-provided request IDs и проверку идентичности intent при retry; Dennett использует тот же принцип для сообщений и других внешних эффектов. [[S23]]

### B.10.3. Provider-specific semantics

- TDLib: окончание отправки подтверждается update, а не только первоначальным response. [[S19]]
- Telegram Bot API: HTTP success может быть authority для send request, но webhook-inline method не даёт результат; такой путь не используется для consequential send без дополнительного reconciliation. [[S20]]
- Email: `202 Accepted` у Microsoft Graph означает принятие request, а не доказательство прочтения; provider-specific status сохраняется без ложного перевода в `delivered`. [[S22]]

## B.11. Schedule и delayed send

Scheduled message хранит:

- original intent;
- exact content revision;
- recipient;
- account;
- requested local time + resolved instant;
- timezone;
- validity window;
- permission at creation;
- revalidation policy.

Перед отправкой повторно проверяются:

- recipient/account still valid;
- permission не отозван;
- draft не superseded;
- событие ещё актуально;
- timezone conversion;
- duplicate not sent elsewhere.

## B.12. Несколько устройств

Если desktop и phone одновременно отвечают на одну Inbox card:

- command несёт card revision;
- Head принимает первый действительный transition;
- второй получает `already resolved` и фактический результат;
- duplicate send не происходит.

Локально отредактированные drafts синхронизируются как revisions. При конфликте:

- текстовые изменения могут быть merged/compared;
- recipients/attachments не объединяются молча;
- user chooses or agent proposes explicit resolution.

## B.13. Отмена, edit и recall

Возможности зависят от provider.

Dennett показывает отдельно:

- можно отменить до dispatch;
- можно удалить локальный draft;
- можно edit sent message;
- можно recall/delete for everyone;
- можно только отправить correction;
- ничего нельзя сделать.

Нельзя показывать универсальную кнопку `Undo Send`, если provider не предоставляет window или гарантии.

## B.14. Ошибки и recovery

### Auth expired

- draft сохраняется;
- send proposal не теряется;
- connector переходит `auth-required`;
- после reauth proposal revalidates.

### Rate limit

- retry-after учитывается;
- пользователь видит delay;
- urgent message может предложить другой channel только с явным recipient mapping.

### Wrong recipient discovered before send

- proposal invalidated;
- старое approval invalidated;
- новый recipient требует новое решение.

### Wrong recipient discovered after send

- stop further disclosures;
- recall/delete if possible;
- prepare correction;
- incident + memory correction;
- do not pretend rollback guaranteed.

### Connector stale

- show freshness;
- fetch current thread before consequential reply;
- no hidden reply based only on old cached messages.

### Incoming update gap

- connector marks cursor gap;
- backfill/provider history request;
- thread remains `possibly stale` until reconciled.

## B.15. Связь с памятью

Memory Fabric получает:

- source message evidence;
- meaningful relationship facts;
- promises/commitments;
- user edits to drafts;
- sent receipt;
- user approval/rejection;
- communication outcome.

Не сохраняются автоматически как глобальная truth:

- все сообщения;
- inferred personality of another person;
- every draft;
- hidden provider metadata;
- unverified semantic conclusions.

Retention и scope зависят от thread sensitivity и user policy.

## B.16. Наблюдаемость и оценка

Метрики:

- draft acceptance/edit rate;
- send-without-confirmation approval rate;
- wrong-recipient incidents;
- disclosure violations;
- duplicate send prevention;
- unknown-effect rate;
- reconciliation success;
- response latency;
- thread freshness errors;
- style similarity with user corrections;
- unnecessary notification rate;
- ignored-vs-answered regret.

## B.17. Антиоверинижиниринговые ограничения

Не создавать:

- отдельного агента на Content, Style, Disclosure и Delivery по умолчанию;
- workflow для каждого ответа;
- универсальную социальную онтологию;
- автоматический полный импорт всей переписки в prompt;
- собственный messenger protocol при наличии mature client API;
- LLM-вызов для exact recipient validation;
- retry без provider reconciliation;
- постоянный background responder для каждого контакта.

## B.18. Критерии готовности

- draft и send различены;
- exact recipient/content/attachment revision фиксируется;
- provider-specific confirmation не переобобщается;
- timeout не создаёт duplicate send;
- style memory не расширяет disclosure;
- manual user command выполняется быстро;
- incoming updates deduplicate и gap-detect;
- multi-device double resolution безопасен;
- thread freshness видима;
- external content не становится authority.

## B.19. Карта будущего переноса

- `20_Dennett_Agentic_Control_Fabric.md`: content/style/disclosure/delivery reasoning и delegation.
- `30_Dennett_Trust...md`: authorization, disclosure, recipients, standing mandates.
- `41_Dennett_Capabilities...md`: connector/account lifecycle.
- `50_Dennett_Server...md`: dispatch, effect claim, reconciliation, cursor sync.
- `10_Dennett_Memory...md`: thread evidence, social context, retention.
- `60/61 UI`: Inbox, drafts, quick replies, send review.
- `01 Shared Contracts`: Send Proposal и Delivery Receipt при необходимости.

---

# Модуль C. Project Lifecycle Contract

## C.1. Назначение

Проект Dennett — видимая рабочая область, обычно связанная с папкой, репозиторием или набором материалов и содержащая project sessions, память, capabilities, artifacts и настройки. Project lifecycle должен ясно отличать:

- состояние записи Dennett;
- состояние локальных файлов;
- состояние Git checkout/worktree;
- состояние remote repository;
- project memory pack;
- связанные Tasks/Runs;
- credentials/connectors;
- exported/shareable representation.

Одна кнопка `Удалить проект` не может молча выполнять все эти действия сразу.

## C.2. Канонические сущности

### C.2.1. Project Record

```yaml
project_record:
  project_id: id
  title: text
  kind: code | research | design | document | media | automation | general
  owner_principal_ref: ref
  lifecycle_state: active | paused | archived | detached | transferring | deleted_record
  primary_workspace_ref: optional
  repository_bindings: []
  memory_space_ref: ref
  capability_set_ref: ref
  sessions: []
  artifact_collection_ref: ref
  created_at: time
  archived_at: optional
  tombstone_ref: optional
```

### C.2.2. Workspace Binding

Связь проекта с физической рабочей областью:

```yaml
workspace_binding:
  binding_id: id
  project_ref: ref
  node_ref: ref
  path_or_provider_ref: ref
  kind: folder | git_checkout | git_worktree | cloud_workspace | remote_runtime
  repository_identity: optional
  branch_or_revision: optional
  read_write_mode: typed
  availability: typed
  last_verified_at: time
  ownership: user | Dennett | provider | external
```

Project Record может существовать без доступного Workspace Binding, например после отключения ноутбука или передачи проекта.

### C.2.3. Repository Binding

Содержит remote identity, default branch, repository UUID/URL, hosting account и trust state. Изменение URL не должно создавать новый проект автоматически, если repository identity подтверждена.

### C.2.4. Project Export

Версионируемый пакет переносимой части проекта, а не полный backup личной установки.

## C.3. Жизненные состояния проекта

### ACTIVE

Доступен для chat, runs, events и edits.

### PAUSED

Новые autonomous runs/events не запускаются, но проект видим и доступен для чтения/ручного продолжения.

### Project state: ARCHIVED

Проект скрыт из основных рабочих списков, фоновые automations отключены по умолчанию, состояние сохраняется. Archive обратим.

GitHub archive используется как полезный precedent: repository становится read-only и может быть unarchived; архивирование не равно удалению. [[S24]] В Dennett archive не обязан делать Git repository read-only, если пользователь архивирует только Project Record, но UI должен объяснять выбранный scope.

### DETACHED

Project Record и память остаются, но текущий folder/repository binding отсутствует или намеренно отсоединён.

### TRANSFERRING

Идёт экспорт, перенос ownership или rebinding; destructive operations блокируются или координируются.

### DELETED_RECORD

Project Record удалён из активной системы и представлен tombstone по retention policy. Это не обязательно означает удаление файлов или remote repository.

## C.4. Операции жизненного цикла

## C.4.1. Create New

Источники:

- пустой project;
- существующая папка;
- clone repository;
- import project pack;
- artifact/research result;
- conversation/idea;
- duplicate/template.

Создание выполняет:

1. создаёт Project Record;
2. определяет или создаёт Workspace Binding;
3. создаёт project Memory Space;
4. обнаруживает instructions/capabilities;
5. устанавливает trust mode;
6. создаёт initial session только если пользователь сразу начинает работу;
7. не запускает тяжёлый анализ без причины.

## C.4.2. Attach Existing Folder

- folder не перемещается;
- ownership сохраняется;
- Dennett анализирует structure read-only сначала;
- обнаруживает Git, project memory, AGENTS/CLAUDE files;
- пользователь выбирает trust;
- binding сохраняет canonical path/device;
- если folder недоступен позднее, project становится detached/stale, а не удаляется.

## C.4.3. Clone Repository

- создаётся новый local checkout;
- remote identity сохраняется;
- branch/revision фиксируется;
- credentials используются через connector/broker;
- project memory pack импортируется как отдельный trust domain;
- executable instructions не активируются до workspace trust.

## C.4.4. Rebind Path

Нужна при переносе папки, смене диска или device.

```text
select new candidate path
→ verify repository/content identity
→ compare expected markers
→ detect divergence
→ attach as same workspace / new replica / fork / reject
```

Нельзя связывать случайную папку только по одинаковому имени.

## C.4.5. Add Worktree/Replica

Git поддерживает несколько linked working trees одного repository. [[S26]] Dennett использует это для изолированной параллельной работы, но Project Record остаётся один.

Каждый worktree имеет:

- branch;
- owner Run/Session;
- node;
- lifecycle;
- merge/discard state.

## C.4.6. Pause

Pause проекта:

- не отменяет текущие runs автоматически без выбора;
- default: stop new background starts, allow current reversible work to checkpoint;
- events переходят `suppressed-by-project-pause`;
- incoming messages/captures могут сохраняться, но не будят project agent;
- user can still open chat manually.

## C.4.7. Archive

Перед archive показываются:

- active Runs;
- schedules/events;
- unsaved/uncommitted changes;
- unresolved Inbox cards;
- project-local secrets/capabilities;
- sync status;
- export/backup state.

Варианты:

- archive record only;
- archive and pause automations;
- archive remote repository if connector supports it;
- create final snapshot/export.

Default — обратимый archive record + pause automations, без удаления файлов.

## C.4.8. Detach

Detach удаляет связь Dennett с workspace, но не файлы.

Варианты:

- detach one node/path;
- detach all workspaces;
- keep project memory;
- keep/revoke project-local credentials;
- keep or disable automations.

## C.4.9. Remove Local Checkout

Это физическое удаление локальной папки, отличное от detach.

Перед выполнением:

- identify exact path;
- detect uncommitted/untracked files;
- check other worktrees;
- check backup/remote availability;
- offer archive/export;
- use recycle/trash/quarantine where possible;
- require Trust policy proportional to irreversibility.

## C.4.10. Delete Remote Repository

Отдельный high-consequence external effect.

GitHub предупреждает, что deletion permanently removes team permissions, private repository forks и может быть восстановлена только в некоторых случаях в ограниченное окно. [[S25]] Поэтому Dennett:

- никогда не скрывает remote delete внутри общего `Delete Project`;
- показывает owner, repository, visibility и forks;
- требует fresh provider state;
- проверяет backup/export;
- создаёт exact Effect Claim;
- не повторяет при unknown result;
- сохраняет receipt/tombstone.

## C.4.11. Delete Project Record

Удаляет Dennett metadata после проверки зависимостей.

Возможности:

- remove from Dennett only;
- also delete project memory;
- keep portable memory pack;
- keep artifacts;
- detach files;
- revoke project grants;
- disable connectors/events.

Default не удаляет physical files и remote repository.

## C.4.12. Transfer to Another User

Transfer состоит из независимых операций:

1. создать shareable export;
2. исключить personal overlays/secrets;
3. определить artifact/project memory licensing;
4. передать repository ownership отдельно;
5. получатель импортирует pack как external trust domain;
6. новая установка создаёт собственные account bindings и grants;
7. source installation может оставить копию, archive или удалить по выбору.

Transfer не переносит автоматически:

- личную память;
- provider tokens;
- social context;
- global skills, если не включены явно;
- active agent sessions;
- standing permissions.

## C.4.13. Duplicate/Fork Project

Варианты:

- duplicate metadata and selected artifacts;
- clone repository to new remote;
- create branch/worktree;
- fork research/design project without code;
- include or exclude project memory history.

Новый проект получает новый `project_id`; lineage сохраняется.

## C.5. Project memory при lifecycle operations

### Project memory on Archive

Memory Space становится cold/readable; background consolidation может быть снижена.

### Project memory on Detach

Memory остаётся, repository facts marked stale until workspace available.

### Transfer

Создаётся sanitized portable projection, а не копия всего private space.

### Project memory on Delete

Запускается deletion graph с отдельными scopes:

- project record;
- project memory;
- raw evidence;
- shared artifacts;
- global promoted knowledge.

Продвинутые в global memory facts не удаляются автоматически только потому, что source project deleted; provenance становится unavailable/deleted according to policy, а privacy obligation обрабатывается отдельно.

## C.6. Project capabilities и connectors

При archive/pause:

- project-local capabilities disabled but retained;
- scheduled connectors stop;
- global capabilities unchanged.

При transfer:

- manifests/references export;
- executable packages optional;
- account bindings never export as active credentials;
- recipient sees missing/replacement capabilities.

При deletion:

- task-scoped grants revoke;
- project-scoped secrets revoke/delete according to ownership;
- shared global capability not removed.

## C.7. Active Runs и sessions

Любая destructive lifecycle operation first enumerates:

- foreground project sessions;
- Managed Runs;
- external effects in progress;
- worktrees;
- scheduled jobs;
- pending Inbox cards;
- provider sessions.

Possible policies:

- wait;
- checkpoint and pause;
- cancel reversible work;
- continue detached;
- transfer ownership;
- block operation due to unknown effect.

Не следует автоматически убивать один chat turn ради archive, если он может безопасно завершиться; но новые writes после lifecycle transition должны быть fenced.

## C.8. Import чужого проекта

```text
receive folder/repository/pack
→ establish source and integrity
→ register project as imported/untrusted
→ inspect memory/instructions/capabilities
→ show portability report
→ select local workspace
→ assign trust mode
→ rebuild indexes
→ connect user-owned providers/accounts
→ open project
```

Import никогда не наследует standing permissions предыдущего пользователя.

## C.9. Conflict scenarios

### Same repository attached as two projects

Dennett предлагает:

- merge Project Records;
- keep separate views with shared workspace warning;
- bind branches/worktrees separately;
- detach duplicate.

### Folder moved while offline

Rebind via identity check; history not lost.

### Remote rewritten/replaced

Repository identity mismatch creates incident/choice, not silent rebind.

### Archive while automation running

Project transition waits/checkpoints and creates explicit decision if external effect pending.

### Delete record while artifacts shared elsewhere

Shared artifact ownership prevents cascading deletion unless explicit.

## C.10. UI-semantic requirements

UI later must expose distinct labels:

- Pause Project;
- Archive in Dennett;
- Detach Folder;
- Remove Local Files;
- Delete Remote Repository;
- Delete Project Data;
- Export/Transfer.

Never place all under a visually ambiguous `Delete` without consequence summary.

## C.11. Observability

Log project lifecycle events:

- actor;
- exact scope;
- affected workspace/repository/memory/capabilities;
- precondition snapshot;
- result;
- reversible-until;
- external effect receipt;
- recovery path.

## C.12. Evaluation

Test:

- archive/unarchive;
- detach/rebind moved folder;
- missing device;
- uncommitted local files;
- active Run;
- unknown external effect;
- remote delete timeout;
- transfer sanitization;
- import by second user;
- duplicate repository detection;
- restore deleted record from backup where allowed.

## C.13. Антиоверинижиниринговые ограничения

Не создавать:

- enterprise project portfolio lifecycle;
- mandatory Kanban state for project lifecycle;
- separate microservice for archive;
- automatic remote delete as part of local cleanup;
- universal filesystem sync for every project;
- model call to distinguish exact path identity when hashes/Git metadata suffice.

## C.14. Критерии готовности

- project record, workspace, local files и remote repo независимы;
- archive обратим;
- detach не удаляет;
- physical and remote deletion explicit;
- active runs handled;
- transfer excludes private data and credentials;
- project memory behavior defined;
- rebind verifies identity;
- UI can state exact consequence in one sentence.

## C.15. Карта будущего переноса

- `20 Agentic`: Project Record lifecycle, sessions/runs interaction.
- `10 Memory`: project space archive/export/delete.
- `30 Trust`: destructive/transfer authorization.
- `41 Capability`: project capability/account detachment.
- `50 Server`: fencing, workspace availability, transfer/export execution.
- `60/61 UI`: command names and consequence previews.
- `01 Shared Contracts`: stable project/workspace references.


---

# Модуль D. Artifact Lifecycle Contract

## D.1. Назначение

Artifact — это сохраняемый результат работы пользователя, агента, workflow или внешнего инструмента, который имеет самостоятельную ценность вне одной реплики чата.

Примеры:

- документ или Markdown-файл;
- исследовательское досье;
- презентация;
- изображение, видео или аудио;
- diagram;
- source-code patch;
- build/package;
- design variant;
- report;
- dataset;
- notebook;
- workflow definition;
- export bundle;
- screenshot или selected capture, если он сохранён как результат;
- final answer, явно материализованный пользователем.

Artifact не равен:

- transient model output;
- internal reasoning;
- tool log;
- Memory Evidence Object, хотя один объект может быть связан с artifact;
- provider attachment;
- project file автоматически, если он не зарегистрирован как результат.

## D.2. Главная модель

> Artifact — это версионируемая сущность с содержимым, происхождением, владельцем, статусом пригодности и правилами распространения. Сам файл является payload, а не всей сущностью.

W3C PROV полезно разделяет Entity, Activity и Agent и позволяет описывать происхождение, derivation и ответственность; Dennett использует этот принцип облегчённо, не навязывая PROV как hot-path format. [[S27]]

## D.3. Канонические сущности

### D.3.1. Artifact Record

```yaml
artifact_record:
  artifact_id: id
  title: text
  kind: typed
  owner_ref: principal_or_project
  project_ref: optional
  created_by_run_or_session: optional
  lifecycle_state: draft | candidate | approved | final | superseded | archived | revoked | deleted
  current_version_ref: ref
  visibility: private | project | shared | public
  sensitivity: typed
  retention_policy_ref: ref
  created_at: time
  updated_at: time
```

### D.3.2. Artifact Version

```yaml
artifact_version:
  version_id: id
  artifact_ref: ref
  version_label: optional
  content_objects: []
  manifest_ref: optional
  format: typed
  content_hashes: []
  created_by: ref
  derived_from: []
  source_evidence: []
  validation_results: []
  created_at: time
  immutable_after_publish: boolean
```

Released/shared version не изменяется на месте; новая правка создаёт новую version. Этот принцип соответствует SemVer rule о неизменности опубликованной версии и практикам object versioning. [[S31]] [[S28]]

### D.3.3. Artifact Representation

Один artifact может иметь несколько представлений:

- editable source;
- rendered preview;
- thumbnail;
- PDF export;
- HTML;
- image frames;
- transcript;
- compressed/mobile rendition;
- redacted shareable rendition.

Representation не становится отдельным artifact, если это только техническое представление той же версии.

### D.3.4. Artifact Relationship

- derived-from;
- variant-of;
- supersedes;
- translates;
- summarizes;
- bundles;
- references;
- validates;
- published-as.

### D.3.5. Publication Record

Фиксирует внешний effect:

- куда опубликовано;
- exact version;
- visibility;
- provider ID/URL;
- timestamp;
- permission;
- ability to revoke/edit;
- receipt.

## D.4. Как artifact создаётся

Artifact может появиться:

- по явной команде пользователя;
- как completion contract Task/Run;
- через `Save as Artifact` из чата;
- из project files;
- из research synthesis;
- из capture;
- из provider-generated output;
- из import;
- из automation.

Создание выполняет минимально:

1. назначить owner/scope;
2. сохранить payload;
3. связать source Session/Run/evidence;
4. создать first version;
5. определить draft/candidate state;
6. не объявлять artifact финальным без основания.

## D.5. Жизненные состояния

### DRAFT

Редактируемая работа, не заявленная как готовая.

### CANDIDATE

Вариант, предлагаемый для выбора или review.

### APPROVED

Пользователь или authorised process подтвердил пригодность для заданной цели, но artifact может ещё не быть опубликован.

### FINAL

Текущая каноническая версия результата в данном scope. `FINAL` не означает вечную неизменность artifact record; следующая версия может supersede.

### SUPERSEDED

Существует новая выбранная версия. Старый artifact остаётся доступным для history/provenance.

### Artifact state: ARCHIVED

Не участвует в обычных suggestions и workflows, но сохранён.

### REVOKED

Artifact или publication больше не должен использоваться/распространяться, хотя исторический record может сохраняться.

### DELETED

Payload удалён согласно policy; остаётся content-free tombstone/provenance при необходимости.

## D.6. Versioning

### D.6.1. Autosave revisions

Частые editor autosaves могут храниться как lightweight revisions/checkpoints, а не полноценные named versions.

### D.6.2. Meaningful versions

Создаются при:

- пользовательском save milestone;
- agent completion;
- approval;
- publication;
- format conversion with semantic differences;
- branch/variant;
- external update/import.

### D.6.3. Version labels

Для human-facing artifact допустимы:

- v1, v2;
- draft-3;
- approved-2026-07-12;
- semantic version для пакетов/API;
- provider revision.

Не все artifacts обязаны использовать SemVer.

### D.6.4. Object versioning

Content store может сохранять несколько versions для восстановления после overwrite/delete, как S3 Versioning. [[S28]] Но product lifecycle не должен зависеть от конкретного object store.

## D.7. Варианты и сравнение

Несколько candidate artifacts могут:

- существовать параллельно;
- иметь common parent;
- сравниваться по content-aware diff;
- объединяться в новую version;
- быть отклонены без удаления;
- хранить user feedback.

Для дизайна, текста и архитектуры предпочтительнее сравнивать целостные варианты, а не разрезать их на не связанные микрорезультаты.

## D.8. Approval и finalization

Approval всегда имеет scope:

- approved for internal use;
- approved for project;
- approved for sharing with named recipients;
- approved for publication;
- approved as final deliverable;
- approved for automation reuse.

Approval одного scope не переносится автоматически в другой.

Agent может предложить `FINAL`, но authoritative transition выполняет:

- пользователь;
- completion contract с объективной проверкой;
- preauthorized automation;
- designated reviewer.

## D.9. Sharing и публикация

Перед share/export:

- выбирается exact version;
- проверяются sensitivity и embedded secrets;
- выявляются linked private objects;
- создаётся redacted rendition при необходимости;
- проверяются license/attribution;
- фиксируется recipient/audience;
- создаётся Publication Record.

### D.9.1. Share link

Link может быть:

- immutable snapshot;
- latest-version pointer;
- expiring;
- authenticated;
- downloadable;
- view-only;
- revocable.

UI должен объяснять, изменится ли видимый контент при новой version.

### D.9.2. Public release

GitHub Releases — пример представления named release с assets, notes и связанным tag; Dennett использует такую модель для software deliverables, но не навязывает её каждому artifact. [[S29]]

### D.9.3. Revocation

Revocation может:

- закрыть Dennett-managed link;
- удалить remote object, если provider поддерживает;
- пометить superseded/revoked;
- уведомить recipients;
- не гарантирует удаление уже скачанной копии.

## D.10. Artifact и project files

Если artifact основан на файле проекта:

- source path/commit фиксируется;
- artifact version может ссылаться на file snapshot;
- последующее изменение файла не переписывает artifact history;
- user может выбрать `Track latest` или `Snapshot`;
- удаление project file не обязательно удаляет final artifact.

Code patch может быть artifact до применения; после merge он сохраняет relation к commit/PR.

## D.11. Artifact и память

Memory хранит:

- факт создания;
- purpose;
- user choice;
- artifact summary;
- evidence/lineage;
- feedback;
- publication outcome.

Memory не должна копировать весь payload в каждый note. Используются stable handles.

Artifact может быть evidence для будущих claims, но approval не делает каждое утверждение внутри artifact истинным навсегда.

## D.12. Import artifact

```text
receive file/bundle/link
→ identify format and source
→ preserve original bytes
→ inspect active content/macros/scripts
→ extract metadata
→ choose owner/project
→ create imported version
→ quarantine executable parts if needed
→ build previews/indexes
```

Import не означает publication или trust.

## D.13. Delete semantics

Separate operations:

- remove from recent/library view;
- archive;
- delete one representation;
- delete version;
- delete artifact payload;
- revoke publications;
- delete all lineage-sensitive data where allowed.

Если version опубликована или использована другим artifact:

- dependency graph показывается;
- deletion may preserve tombstone;
- derived artifacts не удаляются автоматически, но provenance updates.

## D.14. Storage tiers

- hot: active drafts/current previews;
- warm: project artifacts/recent versions;
- cold: superseded/archived versions;
- external: provider/remote repository;
- ephemeral: generated previews/caches.

Canonical payload cannot be silently evicted. Rebuildable representations may be dropped first.

## D.15. Failures

### Generation interrupted

Partial artifact remains draft with completeness status.

### Rendering failed

Source version remains valid; representation marked failed/retryable.

### Publication timeout

Publication becomes `UNKNOWN`; reconcile provider before retry.

### Imported artifact changed upstream

New import version, not silent overwrite.

### Broken external link

Artifact remains with stale external representation and available local metadata/snapshot according to policy.

### Validation failed

Version remains candidate/draft; validation result linked.

## D.16. Observability и evaluation

- artifact creation success;
- version explosion;
- render latency;
- lost draft rate;
- wrong version shared;
- secret leakage;
- provenance completeness;
- restore success;
- publication reconciliation;
- user approval/rejection;
- duplicate artifacts;
- storage by tier.

## D.17. Антиоверинижиниринговые ограничения

Не создавать:

- DAM enterprise taxonomy для личной установки;
- отдельный workflow для каждого save;
- mandatory SemVer для документа/изображения;
- полную копию каждого autosave навсегда;
- автоматическую публикацию из `FINAL`;
- one-size-fits-all diff;
- отдельный artifact service как обязательный микросервис.

## D.18. Критерии готовности

- payload отделён от record/version;
- exact version shareable;
- draft/candidate/final различены;
- publication is external effect;
- revoke limitations honest;
- provenance preserved;
- deletion dependency-aware;
- project file changes do not rewrite history;
- partial generation recoverable.

## D.19. Карта будущего переноса

- `01 Shared Contracts`: Artifact Descriptor/Version/Publication reference.
- `20 Agentic`: artifact as output/completion/variant.
- `10 Memory`: provenance, evidence, retention.
- `30 Trust`: sharing, publication, secrets.
- `50 Server`: storage, rendering, effect reconciliation.
- `60/61 UI`: gallery, preview, compare, approval/share.

---

# Модуль E. Update, Compatibility and Migration Contract

## E.1. Назначение

Dennett является распределённой персональной системой: Head Runtime, desktop, mobile, device agents, schemas, provider adapters, skills, plugins, project memory packs и backups могут обновляться в разное время. Поэтому обновление — не просто замена binary.

Этот модуль определяет:

- что версия означает;
- кто совместим с кем;
- как распространяется update;
- как мигрируют данные;
- как выполняется rollback;
- как изолируются внешние extensions;
- как система продолжает работать при mixed versions.

## E.2. Классы версионируемых объектов

1. Dennett release.
2. Head Runtime protocol.
3. Desktop/mobile client.
4. Device agent.
5. Canonical data schema.
6. Event/command contract.
7. Memory projection/index schema.
8. Portable pack format.
9. Capability/provider adapter.
10. Skill/plugin/MCP package.
11. Workflow/procedure definition.
12. Artifact/export format.
13. Backup manifest.

Эти versions не обязаны совпадать.

## E.3. Version Manifest

```yaml
version_manifest:
  product_version: semver_or_channel_version
  build_id: immutable
  release_channel: stable | preview | nightly | pinned
  protocol_min: version
  protocol_max: version
  data_schema_version: version
  event_schema_versions: []
  pack_format_versions: []
  component_versions: {}
  migration_set: []
  signature_metadata: ref
  released_at: time
```

SemVer полезен только при объявленном публичном contract: incompatible change повышает MAJOR, compatible feature — MINOR, bug fix — PATCH; опубликованный package не изменяется на месте. [[S31]]

## E.4. Release channels

### Stable

Проверенный default.

### Preview/Beta

Новые функции с ограниченным support и явной возможностью возврата.

### Nightly/Development

Только для разработчиков/отдельного isolated installation.

### Pinned

Пользователь замораживает компонент или всю установку, понимая security/compatibility consequences.

Каналы могут различаться для core и extensions; нельзя автоматически перевести весь Dennett на nightly из-за одного preview plugin.

## E.5. Подпись и supply-chain

Update package должен иметь:

- publisher identity;
- immutable build/content hashes;
- signed metadata;
- target platform;
- version;
- dependencies;
- rollback/revocation metadata.

TUF разработан для защиты update systems даже при компрометации части ключей или repository infrastructure; Dennett architecture должна использовать TUF-подобную модель или зрелый platform updater с эквивалентными гарантиями, а не один скачанный JSON с URL. [[S30]]

## E.6. Compatibility negotiation

При соединении device/client и Head стороны обмениваются:

```yaml
compatibility_hello:
  component_id: id
  product_version: version
  protocol_range: [min, max]
  schema_capabilities: []
  feature_flags: []
  required_features: []
  migration_state: typed
```

Результат:

- full compatibility;
- compatible with feature downgrade;
- read-only compatibility;
- update required;
- migration required;
- unsupported/quarantine.

Kubernetes version-skew policies являются полезным precedent: компоненты имеют явно ограниченное окно совместимости, а не предположение, что любая версия может общаться с любой. [[S33]]

## E.7. Mixed-version operation

Во время rolling update:

- Head exposes negotiated protocol;
- old clients do not receive unsupported fields as required semantics;
- new fields are optional/defaulted until compatibility window closes;
- destructive migration waits for incompatible clients to disconnect/update or uses dual representation;
- UI marks hidden/unavailable features honestly.

Нельзя хранить canonical state, которое старый клиент при обычной записи молча уничтожит.

## E.8. Contract evolution

### E.8.1. Additive change

Preferred:

- add optional field;
- add new event type;
- add new capability facet;
- preserve unknown fields where format supports;
- old consumer ignores safely.

### E.8.2. Breaking change

Requires:

- new contract version;
- translation adapter;
- compatibility window;
- explicit migration;
- fallback/rollback plan.

Protocol Buffers documentation explicitly distinguishes wire-safe, unsafe and conditionally safe changes and preserves unknown fields; Dennett adopts the principle even if another serialization format is chosen. [[S32]]

### E.8.3. Semantic change

Самая опасная: field name/type прежние, meaning changed. Она требует новой version/field/event, а не silent reinterpretation.

## E.9. Data migration lifecycle

```text
preflight inventory
→ verify backups and free space
→ quiesce affected writers or enable dual-write
→ create migration checkpoint
→ apply bounded migration
→ verify structural invariants
→ rebuild derived projections/indexes
→ semantic smoke tests
→ mark schema version
→ keep rollback/forward-fix window
```

### E.9.1. Canonical vs derived

- canonical events/evidence migrate conservatively;
- indexes/previews/cache rebuild instead of expensive in-place migration where possible;
- human-authored content not rewritten by model unless explicit semantic migration.

### E.9.2. Online migration

Allowed when:

- old/new readers coexist;
- dual read/write semantics clear;
- no ambiguity/loss.

Otherwise maintenance window is preferable to unsafe cleverness.

### E.9.3. Semantic migration

Например, changing project/permission meaning. Требует:

- source evidence;
- deterministic transform where possible;
- review of uncertain records;
- no mass LLM rewrite without audit/sample/rollback.

## E.10. Update order

Recommended dependency order may be:

1. backup/readiness;
2. Head compatibility layer;
3. server/data migration;
4. device agents;
5. desktop/mobile;
6. provider adapters/extensions;
7. optional features.

Но architecture may choose another order if contracts support it.

## E.11. Client updates

### Mandatory

Только если:

- security vulnerability;
- incompatible protocol after grace period;
- data corruption risk;
- revoked signing key/component.

### Recommended

Feature/fix, system remains usable.

### Deferred

User can postpone; background runs remain safe.

Update cannot interrupt consequential action without checkpoint/reconciliation.

## E.12. Extension/provider adapter isolation

Provider adapter/plugin update может:

- change OAuth scopes;
- change tool schemas;
- add executable hooks;
- alter endpoints;
- remove capability;
- change provider model IDs.

Therefore:

- adapter has independent version;
- new version staged/probed;
- active sessions may remain pinned;
- Trust re-review security-sensitive delta;
- rollback possible without core rollback;
- crash cannot take down Head if process/isolation chosen in architecture.

## E.13. Skills, MCP и project packages

- user-owned modifications never overwritten silently;
- upstream update uses three-way comparison/fork;
- package manifest declares compatibility;
- project may pin version;
- imported update remains untrusted until policy;
- executable delta gets stricter review than text.

## E.14. Rollback

Rollback types:

- binary rollback;
- adapter rollback;
- config rollback;
- data rollback;
- forward fix;
- restore from backup.

Data rollback is not always safe after new writes. Before migration, Dennett records:

- rollback boundary;
- whether writes after boundary can be translated backward;
- whether old binaries may read new data;
- retention of migration snapshot.

If true rollback impossible, UI and operation plan must say `forward-fix only`.

## E.15. Failed update recovery

### Head fails before migration

Restart old version.

### Head fails during migration

Migration journal/checkpoint determines resume/rollback; never guess.

### Client updated, Head old

Feature downgrade or blocked connection according to negotiation.

### One device offline for months

On reconnect:

- authenticate;
- negotiate;
- update required/read-only;
- upload offline log through compatibility translator;
- never let obsolete client overwrite canonical newer structures.

### Extension causes crash

Quarantine extension, restore adapter version, keep core online.

## E.16. Update UX semantics

UI later shows:

- what updates;
- source/signature;
- restart impact;
- migration impact;
- required space/time;
- current backup status;
- affected devices;
- compatibility consequences;
- rollback availability.

`Update all` may exist only if plan is already computed and safe.

## E.17. Observability и evaluation

- update success/failure;
- migration duration;
- rollback rate;
- mixed-version errors;
- stale clients;
- extension crash isolation;
- schema invariant failures;
- data loss/corruption;
- protocol downgrade usage;
- update deferral;
- restore drill.

## E.18. Антиоверинижиниринговые ограничения

Не создавать:

- custom package manager for all ecosystems;
- universal backward compatibility forever;
- distributed rolling deploy complexity for single-device install;
- model-based migration where deterministic transform exists;
- independent protocol for every module;
- automatic major update without backup/readiness;
- microservice solely for version comparison.

## E.19. Критерии готовности

- signed immutable releases;
- compatibility range negotiated;
- mixed version behavior defined;
- canonical/derived migration separated;
- rollback limitations explicit;
- extension update isolated;
- offline old device scenario covered;
- migration backup and semantic smoke test required;
- no silent semantic contract change.

## E.20. Карта будущего переноса

- `50 Server`: updater, negotiation, migration runtime, device rollout.
- `30 Trust`: signatures, publisher trust, revoked packages.
- `41 Capability`: adapter/plugin/skill lifecycle.
- `10 Memory`: canonical/index migrations.
- `60/61 UI`: update controls/states.
- `01 Shared Contracts`: version/compatibility envelopes.
- architecture volumes: concrete package/protocol/schema choices.

---

# Модуль F. Identity, Key and Ownership Recovery Contract

## F.1. Назначение

Recovery должен позволять владельцу вернуть доступ после потери устройства, пароля, Head server или credentials, но не превращать backup/recovery flow в более лёгкий путь захвата системы.

Нужно различать:

- восстановление application access;
- восстановление device trust;
- восстановление encrypted data keys;
- восстановление backup;
- восстановление provider accounts;
- смену owner credentials;
- аварийный доступ доверенного лица;
- восстановление после компрометации.

## F.2. Главный принцип

> Dennett не должен обладать магическим master key, который одновременно позволяет восстановить всё без участия владельца и при этом якобы не создаёт централизованную точку компрометации.

Apple Advanced Data Protection прямо указывает: при end-to-end encryption provider не имеет ключей и пользователь обязан настроить recovery contact или recovery key; 1Password использует отдельный Secret Key и Emergency Kit, а provider не может восстановить Secret Key за пользователя. [[S35]] [[S36]]

Следовательно, Dennett предлагает несколько recovery methods и честно объясняет trade-off.

## F.3. Recovery domains

### Identity domain

Кто является владельцем установки.

### Device domain

Какие устройства доверены.

### Encryption domain

Какие keys открывают memory, artifacts, backups и secrets.

### Provider domain

Внешние accounts/tokens; Dennett часто не может восстановить их без provider reauthentication.

### Head authority domain

Кто может назначить новый Head и повысить Authority Epoch.

## F.4. Recovery Kit

При onboarding Dennett предлагает создать Recovery Kit:

```yaml
recovery_kit:
  installation_id: id
  owner_identity_hint: nonsecret
  recovery_method_set: []
  encrypted_recovery_material: ref
  backup_locations: []
  kit_version: version
  created_at: time
  last_tested_at: optional
```

Формы:

- printable recovery code/key;
- encrypted file on offline media;
- hardware/passkey recovery credential;
- trusted recovery contact;
- threshold split among several locations/people;
- trusted existing device approval.

Default personal installation should support at least:

1. one offline recovery key/file;
2. recovery from existing trusted device;
3. optional trusted contact or second independent copy.

## F.5. Recovery method policy

### Strongest privacy

Provider/Dennett cannot recover without user-held key. Highest loss risk.

### Balanced

User-held key + one trusted device/contact path.

### Convenience-oriented

Encrypted escrow under user-controlled cloud/passkey policy. Higher central compromise risk, must be explicit.

Пользователь выбирает, но UI не скрывает consequences.

## F.6. Потеря одного устройства

```text
sign in from remaining trusted device
→ mark lost device
→ revoke sessions, device credentials and grants
→ rotate affected keys/tokens where needed
→ block offline queued consequential actions
→ update Head/device registry
→ preserve encrypted local data as inaccessible
→ incident summary
```

Если lost device позднее возвращается, он не восстанавливает trust автоматически.

## F.7. Потеря Head server

```text
select trusted recovery device/new server
→ authenticate owner with sufficient assurance
→ obtain latest verified backup + device logs
→ recover encryption material
→ restore installation in isolated validation mode
→ reconcile outstanding external effects
→ establish new Authority Epoch
→ revoke old Head credentials/fencing
→ reconnect devices
→ run semantic smoke tests
```

Новый Head не начинает отправлять queued external effects до reconciliation.

## F.8. Потеря всех обычных устройств

Нужны:

- Recovery Kit;
- fresh owner authentication per chosen method;
- optional waiting period/notifications to recovery contacts;
- restore into new trusted device;
- revoke old device identities;
- reauthenticate external providers;
- regenerate device and session keys.

NIST 800-63B требует пропорциональной assurance и recovery controls; конкретная реализация должна соответствовать выбранному assurance profile. [[S34]]

## F.9. Потеря recovery key

Если есть trusted device/secondary method:

- authenticate;
- rotate recovery material;
- invalidate old kit;
- produce new kit;
- verify backup decrypt.

Если нет ни одного метода, Dennett не должен обещать невозможное. Некоторые E2EE data may be permanently unrecoverable.

## F.10. Compromise recovery

Отличается от обычной потери.

```text
Emergency Stop
→ isolate suspected devices/head/provider bindings
→ freeze new external effects
→ preserve incident evidence
→ authenticate owner through independent factor
→ rotate identity/device/session/encryption credentials
→ inspect grants and account bindings
→ restore clean runtime or verified backup
→ reconcile effects and data changes
→ selectively re-admit devices/capabilities
```

Не следует восстанавливать из backup, созданного после compromise, без проверки.

## F.11. Trusted recovery contact

Подобно Bitwarden Emergency Access, trusted contact может иметь заранее заданную роль и waiting period. [[S37]]

Варианты:

- `view/recovery assist` — помогает восстановить key/identity;
- `takeover` — для наследования/длительной недоступности, поздняя optional feature;
- `incident notify only`.

Ограничения:

- contact не получает обычный доступ заранее;
- activation logged/notified;
- configurable waiting period;
- owner can reject while available;
- contact cannot silently bypass personal vault exclusions unless explicitly configured.

## F.12. Recovery и Secret Broker

После restore:

- vault master keys восстановлены/rotated;
- provider tokens often reauthenticated;
- short-lived credentials not restored;
- unknown secret state marked unavailable;
- agents do not receive raw recovery material;
- Recovery Kit never enters model context.

## F.13. Backup key rotation

New recovery/encryption key may require:

- rewrap data keys, not reencrypt all data where envelope encryption supports;
- rotate backup manifests;
- update future backups;
- retain old wrapped keys only during bounded transition;
- verify at least one restore.

## F.14. Ownership transfer/death/incapacity

Not baseline, but architecture should not forbid future policy.

Possible later feature:

- designated successor;
- waiting period;
- limited export;
- selected projects/artifacts only;
- no automatic access to all personal memory;
- legal/manual verification outside agent autonomy.

## F.15. Social engineering resistance

Recovery UI never trusts:

- voice alone;
- email text claiming owner;
- support agent prompt;
- imported project;
- memory statement;
- external caller.

High assurance uses independent factors, exact installation ID, rate limits, delays and notifications.

## F.16. Recovery test

User can run non-destructive drill:

- verify Recovery Kit readable;
- verify backup decrypts in sandbox;
- verify contact reachable;
- verify device revoke works;
- do not expose full secret in UI/logs.

Suggested periodic reminder is optional and snoozable.

## F.17. Failure scenarios

### Backup stale

Restore, then merge trusted device logs with conflict preservation; show data-loss interval.

### Recovery contact unavailable

Use other configured method; no automatic weakening.

### Old Head comes online

Fencing/epoch rejects writes; device quarantined until re-paired.

### Attacker starts recovery

Notify existing trusted devices/contacts; waiting period; allow deny/revoke; rate limit.

### Recovery succeeds but providers unavailable

Core data opens; connectors marked reauth-required; project remains usable locally.

## F.18. Observability

- recovery attempts;
- method used;
- failed/aborted attempts;
- device/key rotations;
- backup age;
- restore verification;
- data-loss interval;
- outstanding unknown effects;
- post-recovery security review.

Sensitive recovery material never logged.

## F.19. Антиоверинижиниринговые ограничения

Не создавать:

- blockchain identity;
- own PKI hierarchy for every agent;
- mandatory multi-person threshold for ordinary users;
- hidden provider escrow presented as E2EE;
- voice-based recovery;
- automatic inheritance in first version;
- recovery flow that requires running LLM.

## F.20. Критерии готовности

- loss and compromise separated;
- no magic provider recovery claim;
- at least two independent methods supported conceptually;
- lost device revoke defined;
- Head recovery includes epoch/fencing;
- external effects reconciled before resume;
- provider reauth separated from data recovery;
- recovery drill possible;
- unrecoverable case stated honestly.

## F.21. Карта будущего переноса

- `30 Trust`: owner identity, assurance, recovery contact, revoke.
- `50 Server`: restore, Head re-establishment, device re-pairing.
- `10 Memory`: encryption/deletion/restore semantics.
- `41 Capability`: provider reauthentication.
- `60/61 UI`: Recovery Kit, drills, lost device flow.
- architecture: key hierarchy, envelope encryption, platform authenticators.


---

# Модуль G. Resource Pressure and Usage Accounting Contract

## G.1. Назначение

Dennett постоянно использует ограниченные ресурсы:

- дисковое пространство;
- RAM/VRAM;
- CPU/GPU/NPU;
- battery и thermal budget;
- network traffic;
- provider token/API cost;
- subscription quotas;
- rate limits;
- background execution time;
- пользовательское внимание.

Ресурсная логика не должна превращаться в корпоративный billing-service, но система обязана:

- не терять данные молча;
- не разряжать телефон незаметно;
- не сжигать provider limits на низкоприоритетный фон;
- объяснять стоимость проекта/Run;
- адаптировать sensory capture;
- сохранять interactive responsiveness;
- позволять пользователю задавать бюджеты.

## G.2. Главная модель

> Dennett измеряет ресурсы в их нативных единицах, нормализует их для сравнения и связывает с Project, Task, Run, Capability, Device и Source. Точная стоимость, оценка и неизвестность не смешиваются.

FOCUS предлагает vendor-neutral normalization billing data across AI, cloud, SaaS and data center; Dennett использует тот же принцип для provider cost, но расширяет его local compute, storage и battery. [[S38]]

## G.3. Resource Dimensions

### Monetary

- provider-reported API cost;
- estimated subscription consumption;
- cloud compute/storage/network;
- paid plugin/service.

### Compute

- CPU time/load;
- GPU time/utilization;
- NPU/accelerator;
- RAM/VRAM residency;
- local model load/eviction.

### Storage

- canonical events;
- evidence/media;
- artifacts;
- projects;
- backups;
- indexes/cache;
- temporary worktrees/models.

### Network

- upload/download;
- metered/mobile;
- peer sync;
- cloud media;
- provider streaming.

### Device

- battery;
- thermal pressure;
- foreground/background restrictions;
- microphone/camera/screen active duration.

### Attention

- notifications;
- approval prompts;
- voice interruptions;
- review workload.

## G.4. Usage Observation

```yaml
usage_observation:
  observation_id: id
  source: provider | runtime | device | estimate
  subject_refs: []
  resource_dimension: typed
  quantity: number
  unit: canonical_unit
  monetary_value: optional
  currency: optional
  confidence: exact | provider_reported | estimated | unknown
  interval: start_end
  observed_at: time
  attribution_quality: typed
```

Provider invoice/report remains authority for billable cost. Dennett estimates are explicitly marked.

## G.5. Attribution

Usage can attach to:

- installation;
- user;
- project;
- session;
- Task/Run;
- agent/provider session;
- capability/tool;
- ambient source;
- device;
- maintenance job.

Shared overhead may be:

- attributed proportionally;
- kept as system overhead;
- not falsely assigned to one project.

## G.6. Budgets

```yaml
resource_budget:
  budget_id: id
  scope_ref: ref
  dimensions: {}
  soft_limits: {}
  hard_limits: {}
  reset_period: optional
  priority: typed
  exceed_policy: warn | degrade | pause | ask | stop
  owner_ref: ref
```

Budgets may be:

- global monthly API;
- per-project token/time;
- per-Run;
- ambient battery/network;
- storage reserve;
- local GPU concurrency;
- provider subscription reserve.

## G.7. Soft и hard limits

### Soft

- warning;
- cheaper/local backend suggestion;
- reduced helper agents;
- defer maintenance;
- lower capture quality;
- summary instead of full report.

### Hard

- stop/pause according to completion safety;
- never interrupt mid external effect without reconciliation;
- preserve checkpoint and partial artifact;
- user can grant bounded extension.

## G.8. Storage pressure policy

States:

```text
NORMAL
→ WATCH
→ PRESSURE
→ CRITICAL
→ EMERGENCY_READ_ONLY
```

Thresholds depend on absolute free space, percentage, growth rate and reserved recovery space.

### G.8.1. Reclamation order

1. disposable UI/cache/temp;
2. regenerable previews/thumbnails;
3. stale downloaded model cache if recoverable;
4. rebuildable indexes with rebuild plan;
5. duplicate raw media after integrity/dedup check;
6. expired ring buffers/candidates;
7. cold media according to retention/offload policy;
8. user-visible decision for canonical/valuable data.

Нельзя автоматически удалять:

- canonical Memory Events;
- only copy of artifact;
- unsynced project data;
- recovery keys;
- pending effect receipts;
- evidence under legal/user retention.

### G.8.2. Sensory degradation

При pressure:

- lower screenshot frequency/resolution;
- prefer accessibility/DOM metadata;
- shorten raw audio retention;
- commit transcript/structured event and discard low-value raw where policy allows;
- pause nonessential source;
- offload cold encrypted media;
- notify without spam.

Система показывает конкретно, что изменилось.

### G.8.3. Canonical append failure

Если durable append не гарантирован:

- source enters degraded/paused;
- no false indication that capture continues;
- local emergency buffer bounded;
- user notified if data may be lost;
- never drop silently.

## G.9. Battery and thermal policy

Mobile profiles:

- charging/unmetered;
- normal battery;
- low power;
- thermal pressure;
- user active/navigation/call.

Adaptive actions:

- local VAD remains, heavy ASR deferred;
- reduce screen/audio semantic analysis;
- stop preloading large models;
- sync metadata first, media later;
- use server node;
- keep emergency commands local.

## G.10. Network policy

- direct/peer transfer preferred for large local objects where safe;
- metered network can defer media/backups/models;
- control/permissions/cancel retain priority;
- user can force sync/download;
- background upload resumable;
- no repeated full upload after reconnect if chunk/content IDs exist.

## G.11. Provider quota policy

For each provider:

- known remaining quota if exposed;
- rate limit state;
- billing unit;
- subscription estimate;
- reset time;
- priority reserve;
- fallback policy.

Unknown subscription consumption remains estimate; Dennett does not invent exact remaining messages.

Interactive user chat may reserve provider capacity over background work.

## G.12. Cost-aware agent execution

Before spawning helper/reviewer/deep mode:

- estimate marginal utility;
- estimate token/time/provider impact;
- respect execution profile;
- reuse existing context/results;
- avoid duplicate research;
- stop no-progress loops.

User sees budget consequences in plain language, not raw token count only.

## G.13. Attention budget

Resource accounting includes:

- prompts per task;
- voice interruptions;
- Inbox backlog;
- notification frequency;
- review minutes estimated/observed.

If Dennett repeatedly asks same low-risk decision, it proposes bounded policy rather than continuing to consume attention.

## G.14. Resource Coordinator

Logical function, not necessarily a service or agent.

Responsibilities:

- collect usage observations;
- maintain budget state;
- emit pressure events;
- provide eligibility constraints to Capability/Agentic;
- execute deterministic degradation policy;
- request user only when trade-off meaningful.

No LLM required for ordinary thresholds.

## G.15. Resource-aware scheduler

Priority order remains:

1. stop/cancel/permission/voice;
2. user waiting;
3. active project;
4. background;
5. maintenance.

Under pressure:

- maintenance and speculative tasks yield first;
- checkpoint long tasks;
- avoid simultaneous local models exceeding VRAM;
- protect Head/runtime health.

## G.16. Usage history and forecast

Dennett may show:

- daily/monthly trend;
- project breakdown;
- provider/model breakdown;
- local vs cloud;
- ambient cost;
- projected exhaustion.

Forecast is marked estimated and should use simple statistical projection before model-generated narrative.

## G.17. Failure and recovery

### Provider reports delayed cost

Backfill usage and update estimates; never rewrite historical estimate as if it was exact without provenance.

### Metric missing

Mark unknown; do not assume zero.

### Counter reset/provider change

Segment observation by source version/account.

### Device offline

Local counters sync later with IDs/intervals and dedup.

### Disk fills during migration

Migration aborts safely, rollback/checkpoint; reserved recovery space protected.

## G.18. Observability

OpenTelemetry semantic conventions provide common naming across traces, metrics and logs, including GenAI, hardware, devices and system resources. Dennett should align where practical, while keeping product usage records separate from raw telemetry. [[S39]]

## G.19. Evaluation

- billing error vs provider invoice;
- unknown usage rate;
- project attribution coverage;
- budget enforcement correctness;
- interactive latency under background load;
- storage pressure recovery;
- battery impact ambient modes;
- unnecessary agent cost;
- resource-related user prompts;
- data loss under disk full tests.

## G.20. Антиоверинижиниринговые ограничения

Не создавать:

- accounting ledger уровня банка;
- exact subscription prediction where provider hides data;
- LLM cost reviewer per call;
- one resource microservice per dimension;
- arbitrary automatic deletion to hit budget;
- optimization that hides quality/provider changes.

## G.21. Критерии готовности

- resource dimensions explicit;
- exact vs estimated separated;
- soft/hard budget behavior defined;
- storage reclamation order safe;
- ambient degradation visible;
- canonical data protected;
- attention included;
- interactive work prioritized;
- offline usage deduplicates.

## G.22. Карта будущего переноса

- `50 Server`: coordinator, scheduler, pressure handling.
- `41 Capability`: provider quota/model/local hardware measurements.
- `20 Agentic`: marginal cost and budget behavior.
- `10 Memory`: retention/tiering/rebuildable data.
- `40 Voice`: ambient resource adaptation.
- `60/61 UI`: usage dashboards/warnings/settings.

---

# Модуль H. Federated Global Search Contract

## H.1. Назначение

Пользователь должен иметь один быстрый поиск по Dennett, но данные остаются распределены по разным authoritative domains:

- проекты и файлы;
- project sessions/messages;
- Memory Fabric;
- artifacts;
- visual/audio captures;
- Tasks/Runs;
- Action Inbox;
- capabilities/skills/MCP;
- commands/settings;
- contacts/communication threads;
- events/automations;
- remote/offline devices.

Global Search не должен копировать всё в одну бесконтрольную vector database и не должен выдавать stale cached result как authority.

## H.2. Главная модель

> Global Search — query federation и result fusion над domain-specific indexes. Каждый результат сохраняет source, authority, scope, freshness и способ открыть канонический объект.

## H.3. Searchable Source Contract

Каждый domain adapter объявляет:

```yaml
search_source:
  source_id: id
  domain: typed
  scopes: []
  query_modes: [exact, lexical, semantic, structured, temporal]
  freshness_model: typed
  offline_behavior: typed
  result_schema: ref
  open_resolver: command_or_uri
  authority_description: text
  health: typed
```

## H.4. Query Intent

Search query может содержать:

- free text;
- exact quoted term;
- type filters;
- project/person/device;
- time range;
- current vs historical;
- source scope;
- privacy/local-only;
- desired action: navigate, answer, compare, command.

Query planner starts cheap:

1. command/entity exact search;
2. lexical indexes;
3. structured filters;
4. semantic lanes only if needed;
5. remote/cold expansion on demand.

## H.5. Query classes

### Navigation

«Открой проект Dennett», «найди run X».

### Exact lookup

File path, commit, ID, contact, error string.

### Semantic memory

«Где я говорил, что мне не нравится этот стиль?»

### Current state

«Какая версия модели сейчас выбрана?»

### Historical

«Что мы решили в июне?»

### Cross-domain

«Покажи статью, после которой мы изменили архитектуру, и связанный commit».

### Action/command

«Создать проект» should resolve to command, not a random memory note.

## H.6. Result Envelope

```yaml
federated_search_result:
  result_id: id
  source_ref: ref
  object_ref: ref
  object_type: typed
  title: text
  snippet_or_preview: text
  match_reasons: []
  score_components: {}
  scope: ref
  freshness: typed
  authority: typed
  observed_at: optional
  sensitivity: typed
  open_command: ref
  availability: local | remote | offline_cached | unavailable
```

## H.7. Fusion and ranking

Different indexes have incomparable scores. Hybrid-search systems обычно объединяют lexical и semantic retrieval как разные источники кандидатов; Dennett принимает это как baseline, не как единственный engine. [[S41]] Reciprocal Rank Fusion combines ranked result sets without requiring calibrated scores and is a useful baseline. [[S40]]

Dennett ranking considers:

- exactness;
- intent/type match;
- project/current context;
- semantic similarity;
- recency/freshness where relevant;
- authority;
- user history/pins;
- source availability;
- sensitivity and permission;
- dedup/relationship diversity.

No single global score becomes permanent truth.

## H.8. Deduplication and grouping

Same underlying object may appear as:

- project file;
- artifact snapshot;
- memory evidence;
- chat attachment;
- screenshot OCR.

Search groups related representations and offers:

- canonical object;
- relevant version;
- evidence/source;
- derived views.

It must not erase meaningful distinct versions.

## H.9. Freshness and authority

Result displays:

- current/fresh;
- observed as of time;
- cached/offline;
- historical;
- possibly stale;
- deleted/revoked;
- unavailable source.

For current-state questions, search may retrieve candidate but final answer checks authoritative live source when required.

## H.10. Permissions and private results

Search applies Trust/Memory scopes before ranking/rendering.

- result count should not leak existence of hidden vault/project where policy forbids;
- snippets are redacted;
- local-only source may return only on corresponding device or secure peer channel;
- external untrusted content labelled;
- voice/public mode suppresses sensitive spoken results.

## H.11. Offline and partial search

Global search may return:

```text
12 local results
3 cached remote results
2 sources unavailable
```

User can:

- search available now;
- request remote fetch;
- queue search for reconnect;
- handoff to device holding source.

Partial failure is visible and does not invalidate available results.

## H.12. Search and answer

Search itself returns objects. «Ask Dennett» can build answer from selected results with evidence.

Separation prevents:

- model hallucination hidden behind search UI;
- inability to open source;
- expensive model call for simple navigation.

## H.13. Index lifecycle

Each source maintains:

- watermark/version;
- last indexed event;
- model/index version;
- rebuild state;
- lag;
- deletion obligations.

Derived global index can store routing metadata, but domain-specific content remains rebuildable and governed by source.

## H.14. Commands and settings search

Commands have:

- stable command ID;
- title/synonyms;
- context availability;
- permission/effect preview.

Search distinguishes `Run command` from `Open documentation/result`.

Dangerous commands never execute on Enter without exact configured UX/confirmation.

## H.15. Personalization

Allowed:

- recent projects;
- pinned items;
- frequent commands;
- current context.

Not allowed:

- burying exact result because model predicts another intent;
- exposing sensitive personal result without query relevance;
- making ranking unexplainable.

`Why this result` shows major reasons on demand.

## H.16. Failure modes

### Index stale

Show stale and offer refresh; direct open may validate current object.

### Index corrupted

Source remains usable; rebuild; search partial.

### Embedding model changed

Parallel rebuild/dual index; no all-or-nothing outage.

### Source deleted

Remove active index entries; preserve tombstone where allowed.

### Duplicate object IDs

Stable namespace/source IDs prevent collision.

### Query too broad

Progressive refinement/type chips; do not dump thousands of results into LLM.

## H.17. Evaluation

Benchmark queries across:

- exact names;
- typos;
- semantic memory;
- temporal current/history;
- cross-domain chains;
- privacy exclusions;
- offline partial;
- deleted items;
- commands.

Metrics:

- MRR/nDCG/recall;
- exact top-1;
- stale-current error;
- open success;
- latency;
- model calls;
- sensitive leak rate;
- duplicate rate;
- user reformulations.

## H.18. Антиоверинижиниринговые ограничения

Не создавать:

- one giant vector store as authority;
- graph traversal for every query;
- LLM router for exact command/file lookup;
- mandatory global reindex before app usable;
- hidden remote fetch that leaks data;
- independent copy of full object in search index;
- universal score calibration before RRF baseline measured.

## H.19. Критерии готовности

- source adapters defined;
- exact/lexical/semantic lanes coexist;
- authority/freshness visible;
- permission applied before result disclosure;
- partial/offline works;
- command search separate from knowledge answer;
- result opens canonical object;
- indexes rebuildable/deletion-aware.

## H.20. Карта будущего переноса

- `10 Memory`: memory search lanes/evidence.
- `50 Server`: federation, source health, indexing jobs.
- `60/61 UI`: command/global search UX.
- `41 Capability`: external search/connectors.
- architecture data volume: physical indexes/query API.

---

# Модуль I. Locale, Timezone, Language and Travel Contract

## I.1. Назначение

Dennett работает с голосом, schedules, events, messages, projects, memories and multiple devices. Поэтому нельзя использовать:

- implicit server timezone;
- UI language как язык пользователя во всех contexts;
- fixed UTC offset вместо timezone;
- «завтра в 8» без reference context;
- автоматический перевод без сохранения original.

## I.2. Четыре независимых понятия

### Locale

Формат дат, чисел, валюты, plural rules, units.

### Language

Язык текста/речи/content.

### Timezone

IANA zone rule set, например `Europe/Helsinki`, а не только `UTC+03:00`.

### Region/Policy

Региональные provider/legal/storage defaults. Не выводится надёжно только из языка.

CLDR предоставляет locale data для дат, чисел, units и plural rules; BCP 47 используется для language tags; IANA tzdb обновляется при политических изменениях offsets/DST. [[S42]] [[S43]] [[S44]]

## I.3. User and Device Profile

```yaml
locale_profile:
  preferred_ui_locales: []
  preferred_response_languages: []
  default_timezone: iana_zone
  home_timezone: optional
  date_time_style: typed
  number_currency_preferences: {}
  measurement_system: typed
  translation_policy: typed
```

Device reports local zone/locale as signal, not automatically global owner preference.

## I.4. Timestamp storage

Every instant stores:

- UTC/RFC3339-compatible instant;
- source timezone if user-facing/scheduled;
- original local expression where meaningful;
- tzdb version/interpretation where reproducibility matters.

RFC 3339 provides interoperable internet timestamp representation; recurrence and local scheduling still need IANA timezone semantics. [[S45]]

## I.5. Natural language time

For «завтра в восемь» resolve using:

- speaker/session device timezone;
- current date at utterance;
- user travel state;
- project/event timezone;
- conversation context;
- ambiguity policy.

Stored as:

```yaml
temporal_intent:
  original_expression: text
  reference_instant: timestamp
  reference_timezone: iana_zone
  resolved_instant: optional
  recurrence_rule: optional
  ambiguity: []
  resolution_basis: []
```

## I.6. Travel

When device timezone changes:

- update current presence signal;
- do not rewrite home timezone;
- upcoming schedules categorized:
  - anchored to local wall time;
  - anchored to absolute instant;
  - anchored to project/location timezone;
  - ask on ambiguity.

Examples:

- «каждый день в 9 утра» usually follows current/local or chosen home policy;
- flight at 14:00 airport local time belongs to location;
- server backup at 02:00 server zone may stay fixed;
- deadline UTC remains absolute.

## I.7. DST and tzdb updates

- recurrence stores timezone ID, not future precomputed UTC forever;
- next occurrences recalculate using current tzdb;
- already executed historical events retain interpreted instant;
- tzdb update that changes future schedule produces review only if material;
- ambiguous/nonexistent local times follow explicit policy (earlier/later/skip/ask).

## I.8. Multi-language conversation

Voice/text can:

- detect language per turn;
- maintain chosen response language;
- switch when user switches intentionally;
- preserve names/code/quotes;
- avoid changing UI locale automatically.

Low-confidence detection asks or follows session default.

## I.9. Memory and translation

Memory preserves original content.

Derived translations:

- linked to source;
- language/model/version/date;
- not treated as exact quote;
- searchable across languages;
- retranslated if quality improves.

User correction updates translation preference, not original evidence.

## I.10. External communication

Response language chosen from:

- thread language;
- relationship history;
- explicit user command;
- recipient preference;
- current content.

Do not translate sensitive/legal text automatically without indication.

## I.11. Locale-sensitive tools

Tool/action parameters use canonical formats:

- amounts with currency code;
- decimal separator normalized;
- units explicit;
- dates structured;
- addresses retain locale.

Rendered UI can localize, but Action Request exact values remain unambiguous.

## I.12. Scheduling across devices

Head is authority for resolved schedule. Device may create offline temporal intent and sync later with original expression/reference time.

If sync after intended time:

- execute late only if policy;
- notify;
- skip;
- reschedule;
- never silently pretend it ran on time.

## I.13. Failure scenarios

### Incorrect device clock

Use authenticated server time where available; preserve source observed time and confidence.

### Timezone unavailable/offline

Use last known zone and mark; do not infer solely from IP for consequential schedule.

### User moves during voice session

Session timezone fixed unless user says otherwise; future commands can use updated context.

### Mixed-language ASR

Store audio/original hypotheses; avoid translating code/identifiers.

## I.14. UI requirements

UI later must show timezone for:

- scheduled external action;
- deadline with remote participants;
- recurring automation;
- imported event;
- travel ambiguity.

Routine local timestamps can remain concise with timezone on detail.

## I.15. Evaluation

- DST transition tests;
- ambiguous/nonexistent local time;
- travel zone changes;
- offline late sync;
- multilingual turns;
- translated search;
- amount/date parsing;
- user correction;
- provider timestamps.

## I.16. Антиоверинижиниринговые ограничения

Не создавать:

- own timezone database;
- LLM for standard date formatting;
- automatic global language switch from one foreign phrase;
- permanent translation copies for everything;
- legal region inference solely from GPS/IP.

## I.17. Критерии готовности

- IANA timezone stored;
- original temporal expression preserved;
- absolute vs wall-time semantics explicit;
- travel/DST covered;
- BCP47 languages and CLDR locale concept separated;
- original content retained alongside translations;
- structured action parameters locale-safe.

## I.18. Карта будущего переноса

- `50 Server`: schedules/event time/tzdb update.
- `40 Voice`: per-turn language/time interpretation.
- `10 Memory`: original/translation/time provenance.
- `60/61 UI`: locale rendering/travel prompts.
- `B communication`: thread language/scheduled send.


---

# Модуль J. Import, Export and Portable Package Compatibility Contract

## J.1. Назначение

Dennett должен переносить данные и функциональные пакеты между:

- устройствами одного пользователя;
- разными установками Dennett;
- пользователями;
- проектами;
- версиями приложения;
- Dennett и внешними инструментами.

Но «экспорт» означает разные вещи для project memory, whole-installation backup, skill, artifact, settings и capability profile. Один универсальный ZIP без типизированного manifest быстро станет неразбираемым и небезопасным.

## J.2. Классы переносимых пакетов

### Project Package

Проектная память, instructions, capability requirements, selected artifacts и references.

### Artifact Package

Exact artifact version + representations + provenance.

### Skill/Capability Package

Skill/plugin/procedure с dependencies и origin.

### Settings Package

Пользовательские настройки, profiles и UI layout без secrets по умолчанию.

### Automation Package

Trigger + action/procedure + required capabilities + safety assumptions.

### Research Package

Sources, evidence, claims, conclusions и unresolved gaps.

### Installation Transfer Package

Полная или частичная миграция установки; не равна shareable export.

### Backup Snapshot

Recovery-oriented encrypted state; не предназначен для безопасного обмена между пользователями.

## J.3. Общий Portable Package Manifest

```yaml
portable_package_manifest:
  package_id: id
  package_type: typed
  format_version: version
  created_by_product_version: version
  created_at: time
  creator_principal_or_installation: optional
  intended_use: transfer | backup | share | publish | import
  payload_inventory: []
  content_hashes: []
  schema_refs: []
  dependencies: []
  optional_components: []
  sensitivity_classes: []
  encryption: optional
  signatures: []
  provenance_ref: optional
  compatibility:
    min_reader: optional
    max_reader: optional
    required_features: []
  import_policy_hint: optional
```

## J.4. Разделение integrity, trust, permission и truth

Проверенный checksum/signature доказывает целостность и publisher identity, но не:

- безопасность содержимого;
- истинность claims;
- отсутствие prompt injection;
- право исполнять scripts;
- право раскрывать included data;
- совместимость с текущим project.

Import проходит отдельные Trust и privacy gates.

## J.5. Packaging strategy

Dennett-native package может быть обычной директорией/архивом с:

- manifest;
- payload;
- metadata;
- checksums;
- optional signatures;
- human-readable index.

BagIt является полезным reference: directory layout, arbitrary payload, descriptive tag files и checksum manifests для надёжного хранения/переноса без необходимости понимать внутреннюю семантику payload. [[S47]]

RO-Crate полезен как optional projection для research/software artifacts и linked metadata; он не обязан быть внутренним hot-path format. [[S46]]

## J.6. Manifest/schema versioning

JSON Schema может описывать package manifests и validation rules, но package version всё равно задаёт business semantics. [[S48]]

Importer выполняет:

1. parse envelope/version;
2. verify integrity;
3. validate known fields;
4. preserve unknown optional metadata;
5. reject unknown required semantics;
6. choose migration adapter;
7. never execute during parse.

## J.7. Import lifecycle

```text
select/receive package
→ copy to quarantine/staging
→ verify size, paths, checksums and signatures
→ parse manifest without execution
→ inventory payload and dependencies
→ privacy/security scan
→ compatibility analysis
→ show import plan
→ user/policy selects scope
→ migrate/normalize
→ create imported trust domain
→ rebuild derived indexes
→ bind local accounts/capabilities separately
→ validate result
→ promote to active scope
```

## J.8. Path and archive safety

Importer blocks:

- path traversal;
- absolute paths outside staging;
- dangerous symlink resolution;
- device files;
- decompression bombs;
- undeclared executable hooks;
- fetch URLs without policy;
- case/canonicalization collisions;
- unsupported filenames with clear report.

BagIt itself notes URL/path and payload security considerations; integrity is not protection from malicious payload. [[S47]]

## J.9. Selective export

User chooses export classes:

- public/shareable;
- project-only;
- include raw sources;
- include generated artifacts;
- include history;
- include only current state;
- include capabilities by reference or payload;
- exclude private overlays;
- include encrypted recipient-specific subset.

Before export Dennett generates a privacy inventory:

- personal memory;
- contacts/messages;
- secrets/credentials;
- device paths;
- usernames/emails;
- hidden prompts/policies;
- copyrighted/licensed content;
- raw ambient media;
- external URLs and access requirements.

## J.10. Project Memory Package

Recommended components:

```text
.dennett/memory/
  manifest.yaml
  index.md
  events/
  notes/
  decisions/
  research/
  procedures/
  instructions/
  schemas/
  views/
```

Export may use this repository-resident pack directly or create a sanitized bundle.

Rules:

- derived indexes omitted;
- personal/global overlays omitted;
- secrets omitted;
- stable refs either resolved into package or listed external;
- recipient can rebuild search;
- Git-friendly segments reduce conflicts;
- imported pack mounts, not auto-merges globally.

## J.11. Capability package

Contains:

- human-readable description;
- required tools/providers;
- scripts/assets/references;
- version/origin/license;
- effects/scopes;
- compatibility;
- evaluation history optional;
- no active credentials.

User-owned manual import adds to Collection, while executable authorization remains separate.

## J.12. Settings export

Default includes:

- UI layouts;
- execution profiles;
- voice preferences;
- notification rules;
- provider aliases without secrets;
- project templates;
- keyboard shortcuts;
- accessibility.

Optional encrypted export may include connector/account metadata, but access tokens are normally reauthenticated.

Settings merge rules:

- preview diff;
- choose replace/merge/select;
- project-specific does not overwrite global silently;
- unknown settings preserved or reported;
- no automatic security weakening.

## J.13. Artifact export

Select exact version and representations. Manifest includes provenance and integrity. External references can be:

- embedded;
- linked;
- omitted with report.

Portable export never silently points to private local paths.

## J.14. Whole-installation transfer

Different from share/export:

- encrypted;
- intended for same owner;
- may include private memory and vault wrappers;
- requires recovery/ownership proof;
- includes Head/runtime metadata, but active external effects reconciled;
- old installation revoked after handoff according to policy.

## J.15. Unknown/newer package version

Options:

- import read-only metadata;
- preserve package unopened;
- update Dennett;
- use compatibility translator;
- reject with precise missing feature list.

Never partially import required semantics while reporting success.

## J.16. Migration and round-trip

For each format:

- `import(export(x))` preserves canonical meaning;
- order/nonsemantic formatting may differ;
- stable IDs preserved or mapped explicitly;
- provenance records transformation;
- private data exclusions testable;
- package can be exported again without silent loss of unknown fields where possible.

## J.17. Multi-platform variants

A package may contain several optional artifacts/backends. OCI image index is a useful precedent for selecting platform-specific manifests from a higher-level index; Dennett can use the principle for local model/runtime assets without adopting OCI for all packages. [[S50]]

Example:

- Windows script;
- macOS script;
- Linux script;
- generic instructions;
- no executable for mobile.

Importer selects compatible component and reports omitted variants.

## J.18. External object references

Reference records include:

- URI/provider;
- expected identity/hash/version;
- required auth;
- availability;
- whether export is complete without it;
- fetch policy.

Import does not automatically fetch remote executable content.

## J.19. Licensing and attribution

Package can declare:

- license;
- source attribution;
- redistribution constraints;
- model/data license;
- unknown status.

Dennett warns but does not pretend to provide legal judgment.

## J.20. Failure and recovery

### Corrupt package

Reject before activation; report exact files/checksums.

### Partial transfer

Resume by content hashes/chunks; no duplicate import.

### Import crash

Staging transaction can resume/rollback; active registry not partially mutated.

### Missing dependency

Import object inactive with resolution plan; data remains readable where possible.

### Malicious package

Quarantine, no execution, incident/report, allow inspect as data.

### Private data detected during export

Block or require explicit per-item decision; produce sanitized version.

## J.21. Evaluation

- round-trip fidelity;
- checksum detection;
- path traversal/decompression tests;
- secret/privacy leak scan;
- cross-version migration;
- unknown-field preservation;
- second-user import;
- missing capability substitution;
- large package resume;
- Git merge of project memory segments.

## J.22. Антиоверинижиниринговые ограничения

Не создавать:

- one universal package containing every Dennett concept;
- custom cryptographic format when standard primitives suffice;
- automatic execution on import;
- mandatory RO-Crate/OCI/BagIt for internal hot path;
- active credentials in ordinary project share;
- permanent support for all historical versions without migration policy.

## J.23. Критерии готовности

- package types separated;
- common manifest/version/integrity defined;
- selective export/privacy scan;
- import staged/quarantined;
- trust separate from signature;
- credentials not transferred by default;
- unknown required semantics fail clearly;
- round-trip tested;
- project pack interoperable and Git-friendly.

## J.24. Карта будущего переноса

- `10 Memory`: project/research packs, provenance.
- `41 Capability`: skill/plugin packages.
- `50 Server`: import/export execution, whole-install transfer.
- `30 Trust`: signature/trust/privacy.
- `D Artifact`: artifact packages.
- `60/61 UI`: import/export previews and controls.
- architecture data volume: physical formats/schemas.

---

# Модуль K. Composite Experience Recipes

## K.1. Назначение

Некоторые важные пользовательские функции Dennett не должны становиться отдельными подсистемами. Они являются **recipes** поверх существующих primitives:

```text
prompt/behavior profile
+ skill/procedure
+ memory query
+ optional event/automation
+ artifact template
+ project/context bindings
```

Recipe описывает user-visible behavior и requirements, но не создаёт отдельную canonical database.

## K.2. Recipe Definition

```yaml
experience_recipe:
  recipe_id: id
  name: text
  purpose: text
  activation: manual | context | schedule | event
  required_capabilities: []
  context_query: optional
  prompt_or_skill_refs: []
  output_artifact: optional
  memory_commit_policy: typed
  autonomy_policy: typed
  budget_profile: optional
  user_customizable: boolean
```

Home Assistant blueprints являются полезным precedent reusable automation templates with inputs, import and user customization; Dennett recipes расширяют идею AI skills/context, но не превращают любую recipe в strict automation graph. [[S49]]

## K.3. Idea Incubator

### Purpose

Сохранять сырые идеи без немедленного превращения в проект или Task.

### Inputs

- voice note;
- text;
- screenshot/photo;
- link;
- fragment from conversation;
- ambient candidate explicitly committed.

### Behavior

- save original;
- short optional title/tags/project link;
- detect duplicates/related ideas lazily;
- no forced schema;
- periodic review optional;
- statuses are lightweight facets, not mandatory workflow.

### Actions

- develop;
- merge with idea;
- create project;
- research;
- archive;
- ignore;
- remind later.

## K.4. Concept Distiller

Transforms long voice/chat stream into selectable projections:

- cleaned transcript;
- concept map;
- functions;
- principles;
- assumptions;
- contradictions;
- unresolved questions;
- requirements;
- architecture input;
- document draft.

Original stream remains evidence. Distilled result is artifact/projection and user can correct it.

## K.5. Thinking Editor

Behavior profile for collaborative reasoning:

- catches contradiction;
- distinguishes desire vs implementation;
- asks high-value questions;
- identifies hidden assumptions;
- proposes alternative formulation;
- preserves user's ownership of conclusion;
- does not spawn team by default.

Can run in text or voice. No special storage beyond conversation + optional artifact/memory.

## K.6. Daily Briefing

Automation/recipe:

```text
scheduled or user-invoked event
→ retrieve current projects, Inbox, calendar, promises, notifications and overnight outcomes
→ rank by user attention policy
→ generate compact text/voice artifact
→ deliver through chosen channel
→ no action unless separately authorized
```

User controls sections, time, length and delivery. If nothing important, briefing can be empty/silent.

## K.7. Evening Debrief and Monthly Retrospective

Uses history/current outcomes to surface:

- completed work;
- unresolved commitments;
- patterns;
- changed interests/preferences;
- repeated friction;
- potential skill/policy improvements.

Conclusions remain proposals with evidence; no psychological diagnosis.

## K.8. AI News Monitor / Technology Radar

Recipe over World Intelligence Memory:

```text
source subscriptions/search schedule
→ deduplicate articles/posts/papers/releases
→ extract claims and versions
→ assess source quality/freshness
→ match to active projects and interests
→ store evidence
→ notify only high-value delta or include digest
→ allow project experiment/research
```

Important rules:

- tweet is signal, not truth;
- product facts revalidate before use;
- news item does not auto-install capability;
- project-fit ranking uses requirements;
- no separate `News Database` if World Intelligence already holds claims/evidence.

## K.9. Research Dossier

Skill/preset that creates artifact with:

- question;
- decision context;
- sources;
- claims;
- support/contradictions;
- uncertainty;
- gaps;
- recommendation;
- freshness.

Execution remains one strong agent by default; parallel helpers only for independent source branches.

## K.10. Meeting Summary

Voice profile + skill:

- silent capture/diarization;
- transcript evidence;
- decisions/promises/action candidates;
- private vs shareable notes;
- participant review optional;
- create tasks only after policy.

No separate meeting subsystem.

## K.11. Taste Review

Retrieves relevant examples/preferences/negative reactions, then evaluates artifact separately from objective quality.

Output distinguishes:

- objective constraints;
- alignment with user's taste;
- uncertainty;
- references.

Does not make global taste schema mandatory.

## K.12. Red-Team Preset

Behavior profile/skill applied to selected artifact/plan/code:

- failure modes;
- abuse cases;
- assumptions;
- verification gaps;
- rollback.

Does not run after every trivial response. User/project policy determines when.

## K.13. Recipe discovery and creation

Dennett can notice repeated user sequence and propose recipe/skill:

- evidence of repetition;
- expected saved effort;
- simplest representation;
- project-local first;
- user can reject;
- no automatic proliferation.

## K.14. Recipe customization

User may edit natural language, source list, schedule, output and autonomy. Advanced implementation details remain hidden unless needed.

## K.15. Evaluation

Each recipe evaluated by its outcome:

- briefing usefulness/dismissal;
- news project relevance;
- concept correction rate;
- research citation quality;
- meeting action accuracy;
- token/attention cost.

Unused/noisy recipe is disabled or simplified.

## K.16. Антиоверинижиниринговые ограничения

- no dedicated microservice/database per recipe;
- no fixed schemas for all ideas/taste/people;
- no mandatory workflow builder;
- no always-on research swarm;
- no notification for every news item;
- no global skill promotion from one success.

## K.17. Карта будущего переноса

- `20 Agentic`: behavior profiles and execution choice.
- `41 Capability`: skills/recipe packages.
- `10 Memory`: World Intelligence, queries, evidence.
- `50 Server`: schedules/events/delivery.
- `40 Voice`: voice recipes.
- `60/61 UI`: presets and customization.


---

# Часть III. Сквозные нормативные правила

## 12. Один объект — один канонический владелец

Временный файл не меняет ownership, заданный Specification Index.

| Объект/решение | Канонический владелец |
|---|---|
| Memory Event, Evidence, Claim, retention | Memory Fabric |
| Project Session, Task, Run, Agent | Agentic Control Fabric |
| Permission, grant, identity, consent | Trust Fabric |
| Voice turn, floor, ambient conversation behavior | Voice Fabric |
| Provider/tool/skill/connector availability | Capability Fabric |
| Runtime state, dispatch, sync, update execution | Server Runtime |
| Desktop/mobile interaction | соответствующий UI document |
| Project/Artifact/Communication lifecycle delta из этого файла | позднее распределяется по владельцам согласно карте |

Ни один будущий модуль не создаёт параллельную «истину» только ради удобства реализации.

## 13. Общая цепочка значимого действия

Любое значимое действие проходит концептуальную цепочку:

```text
source signal
→ normalized intent/event/observation
→ identity and trust context
→ current authoritative state
→ context/evidence assembly
→ decision or proposal
→ capability resolution
→ permission/effect validation
→ execution
→ receipt/outcome/artifact
→ memory/provenance update
→ user-visible state
→ recovery path
```

Простая операция может пройти её внутри одного процесса и без LLM. Цепочка является контрактом ответственности, а не требованием строить workflow.

## 14. Events, commands, observations and effects не смешиваются

- **Observation** сообщает, что source что-то увидел/услышал.
- **Event** сообщает, что произошло значимое изменение.
- **Command** просит изменить состояние или выполнить действие.
- **Proposal** предлагает command, но ещё не имеет authority.
- **Effect** изменяет внешний мир.
- **Receipt** подтверждает наблюдаемый результат эффекта.

Внешняя страница, сообщение, голос третьего лица или imported package могут создать observation/event, но не command от владельца.

## 15. Human intent и authority

Пользователь может быстро и явно:

- добавить capability;
- отправить сообщение;
- прикрепить проект;
- сохранить artifact;
- включить trusted scope;
- настроить automation.

Система не должна навязывать utility-review ручному выбору. Но техническая безопасность, exact target и external-effect idempotency остаются.

Память о том, что пользователь «обычно разрешает», помогает предложить bounded policy, но не является current permission.

## 16. Быстрый путь без лишней модели

Без LLM должны выполняться, когда возможно:

- exact permission/grant check;
- path/repository identity;
- update compatibility range;
- checksum/signature verification;
- event deduplication;
- storage thresholds;
- schedule calculation;
- exact search/navigation;
- provider health/quota lookup;
- cancel/stop/mute/privacy controls;
- basic import validation.

Модель подключается для неоднозначного смысла, сравнения, synthesis и адаптации, а не как обязательный посредник каждого действия.

## 17. Неизвестный результат — отдельное состояние

Для внешней отправки, публикации, удаления remote repository, payment, push/release и других consequential effects:

```text
SUCCESS != TIMEOUT
FAILURE != TIMEOUT
TIMEOUT after dispatch → UNKNOWN
```

`UNKNOWN` требует reconciliation. Retry без reconciliation запрещён, если может дублировать эффект.

## 18. Source, trust и permission сохраняются при трансформации

При переходах:

- audio → transcript;
- screen → OCR;
- message → summary;
- research source → claim;
- package → imported objects;
- artifact → export;
- memory → context;

должны сохраняться:

- origin;
- transformation/version;
- trust domain;
- sensitivity;
- owner/scope;
- evidence handle.

Derived text не становится user instruction только потому, что модель его сформулировала.

## 19. Архивирование, удаление, отзыв и отключение различаются

### Shared semantic: Archive

Скрыть/заморозить с сохранением и возможностью возврата.

### Disable/Pause

Остановить активное использование, сохранив объект.

### Shared semantic: Detach

Убрать связь с внешним/физическим ресурсом, не удаляя его.

### Revoke

Запретить дальнейшее использование/доступ или распространение.

### Shared semantic: Delete

Удалить payload/state согласно retention и dependency graph.

### Forget/Hide from context

Не использовать в обычной персонализации, не обязательно удалить bytes.

UI и APIs не используют один глагол для этих разных эффектов.

## 20. User-owned, Dennett-managed и provider-managed

Любой изменяемый package, skill, artifact, draft, project file или setting имеет ownership.

- User-owned: Dennett proposes patch/fork; no silent rewrite.
- Dennett-managed: system may version/update/rollback within policy.
- Provider-managed: native lifecycle preserved.
- Project-shared: repository/version rules apply.
- Imported: separate trust domain until promotion.

## 21. Freshness and authority

Cached state always has observation time. Current-state decision checks live authority when effect/risk requires it.

Examples:

- repository/worktree authority for code;
- provider receipt for sent message;
- Trust registry for active permission;
- Head epoch for coordination;
- Memory ledger for historical event;
- search result is pointer, not authority.

## 22. Progressive disclosure and bounded context

Dennett can know much but show/send little:

- mobile gets summaries and handles;
- voice gets compact answer context;
- project agent gets project-relevant context;
- search reveals details on demand;
- capability descriptions loaded lazily;
- imported package inspected before activation.

This reduces tokens, leaks and cognitive overload.

## 23. User interruption and resumability

Every meaningful long operation specifies:

- can user interrupt;
- what stops immediately;
- what checkpoints;
- what external effect may already have happened;
- what partial artifact remains;
- how to resume;
- what changed while away.

## 24. Resource proportionality

Formal durability, review, provenance and model depth increase only with:

- duration;
- irreversibility;
- external effect;
- data sensitivity;
- concurrency;
- need for reproducibility.

A quick note, exact search or simple project message does not become a Managed Run.

## 25. Platform truth over product fantasy

If OS/provider forbids a background mode:

- feature reports unsupported/restricted;
- alternate mode offered;
- no documentation claim that the feature is guaranteed.

Examples:

- screen capture requiring explicit projection session;
- mobile microphone background restrictions;
- provider lacking message recall;
- local model not fitting hardware.

---

# Часть IV. Обязательные сквозные сценарии дополнений

## 26. Ambient microphone on phone

### Initial state

- owner enabled `Wake + Contextual Capture`;
- microphone permission granted;
- device on battery;
- Head online but cloud semantic analysis disabled.

### Ambient audio flow

```text
local VAD/wake
→ rolling encrypted ring buffer
→ speaker/activity detection
→ cheap duplicate/relevance gate
→ user says “запомни это для Dennett”
→ committed turn window selected
→ local ASR
→ Ambient Candidate
→ Trust/privacy policy
→ project association proposal
→ Memory Event + Evidence
→ raw retention timer
→ selective sync
```

### Success

- unrelated background not uploaded;
- user can retrieve note;
- source/ASR confidence known;
- battery budget respected;
- mute works locally.

### Failure

If OS kills service, indicator/status shows source inactive; no fake continuous recording.

## 27. Event-driven screen context on PC

### Screen context flow

```text
window/app change
→ accessibility/DOM metadata
→ visual diff threshold
→ screenshot only when useful
→ redact excluded app/region
→ deduplicate static frames
→ link active project/task
→ commit evidence or expire candidate
```

When a UI error appears during project work, agent can retrieve exact screenshot and surrounding actions. Password manager/financial app excluded per policy.

### Storage pressure

Frequency/resolution drops first; canonical selected evidence preserved.

## 28. Incoming Telegram message and response

```text
TDLib update
→ deduplicate and update thread
→ reconstruct sender/project/current facts
→ decide content/style/disclosure/delivery
→ draft candidate
→ user says “отправь” or standing pattern applies
→ exact Send Proposal
→ Trust validation
→ TDLib sendMessage
→ wait updateMessageSendSucceeded
→ Delivery Receipt
→ memory update
```

If send status unknown, no automatic duplicate.

## 29. Manual quick reply from mobile notification

- notification action includes thread/message revision;
- phone authenticates user session;
- exact text/recipient shown;
- command goes to Head;
- Head rejects if thread/card superseded;
- optimistic UI becomes confirmed only after provider evidence.

## 30. Project archive with active Run

```text
user chooses Archive in Dennett
→ enumerate active Run/worktree/events/unsynced files
→ offer checkpoint-and-pause current Run
→ disable future automations
→ keep files and remote repo
→ archive Project Record and Memory Space
→ clear from default list
→ create reversible lifecycle receipt
```

No local or remote deletion occurs.

## 31. Delete remote repository

- separate command from project deletion;
- fresh provider metadata and exact repository shown;
- uncommitted/local-only data warning;
- backup/export status;
- strong confirmation;
- Effect Claim;
- provider delete;
- timeout → UNKNOWN and reconciliation;
- Project Record becomes detached/archive according to user choice, not silently deleted.

## 32. Transfer project to another user

```text
select Project Export
→ inventory shareable project memory/artifacts/instructions/capabilities
→ remove personal memory, credentials, private messages and local paths
→ integrity/privacy validation
→ package + manifest
→ recipient imports in quarantine
→ rebuild indexes
→ bind own repo/accounts/providers
→ choose trust mode
→ project opens with provenance
```

Source can remain active, archive or delete separately.

## 33. Artifact generation, approval and publication

```text
agent creates candidate report
→ Artifact Record/Draft Version
→ user compares candidate variants
→ approves v2 for project
→ chooses public export
→ privacy/license scan
→ exact publication version frozen
→ publish effect
→ receipt/URL
→ v2 remains immutable; edits create v3
```

Publication timeout reconciles before retry.

## 34. Mixed-version update

- Head upgraded with compatibility layer;
- desktop latest, laptop old, phone offline;
- old laptop connects within supported protocol range with feature downgrade;
- phone returns after compatibility window and enters update-required/read-only mode;
- its offline append log translated/imported without overwriting newer state;
- derived indexes rebuild;
- no silent loss of unknown fields.

## 35. Lost phone

```text
owner uses trusted desktop
→ mark phone lost
→ revoke device/session/grants
→ block queued effects
→ rotate affected credentials
→ phone local encrypted state inaccessible
→ phone later appears and is quarantined
```

## 36. Lost Head and recovery

- use Recovery Kit + trusted device;
- restore verified backup to new server;
- collect trusted offline logs;
- reconcile external effects;
- establish new Authority Epoch;
- fence old Head;
- providers reauth where needed;
- semantic smoke test before background automations resume.

## 37. Disk reaches critical threshold

- stop speculative indexing/candidates;
- clear caches/previews;
- reduce sensory capture;
- offload cold encrypted media;
- protect canonical append reserve;
- show exact storage categories/actions;
- if reserve exhausted, pause source and warn, never pretend capture continues.

## 38. Global search while laptop offline

User asks for an architecture note stored on laptop.

Search returns:

- memory summary cached on Head;
- artifact copy if available;
- result saying source file unavailable on laptop;
- last freshness;
- action `Queue open/search when laptop reconnects`.

It does not fabricate file contents.

## 39. Travel timezone change

User creates «напомни каждый день в девять» in Helsinki, then travels.

Dennett stores recurrence policy. Depending on configured semantics:

- follow local time;
- stay at home time;
- ask once on travel.

Existing absolute appointments remain fixed. DST update recalculates future occurrences.

## 40. Import malicious project package

- package copied to quarantine;
- checksum valid but script/hook suspicious;
- integrity passes, trust does not;
- content/notes inspectable as data;
- executable components disabled;
- user can import safe subset;
- no credentials/permissions inherited.

## 41. AI News Monitor finds a tool

```text
scheduled recipe finds release/article
→ stores source claim with freshness
→ matches active project requirement
→ compares with existing capability
→ candidate says one unique advantage
→ create Capability Delta Proposal
→ project-local trial if user policy
→ measured result
→ no global install/promotion unless useful
```

No separate news subsystem or automatic installation.

## 42. Daily Briefing has nothing important

Recipe retrieves current state and finds no meaningful delta. It produces no noisy voice/push, or a minimal «ничего срочного» only if user requested daily confirmation.

---

# Часть V. Закрытие gap ledger

## 43. Статус каждого пробела

| ID | Пробел | Решение в этом файле | Статус перед архитектурой |
|---|---|---|---|
| G-01 | Ambient microphone + screen fragmented | Module A + scenarios 26–27 | CLOSED-NORMATIVELY |
| G-02 | External communication lifecycle | Module B + scenarios 28–29 | CLOSED-NORMATIVELY |
| G-03 | Project lifecycle | Module C + scenarios 30–32 | CLOSED-NORMATIVELY |
| G-04 | Artifact lifecycle | Module D + scenario 33 | CLOSED-NORMATIVELY |
| G-05 | Update/schema/protocol compatibility | Module E + scenario 34 | CLOSED-NORMATIVELY |
| G-06 | Identity/key recovery | Module F + scenarios 35–36 | CLOSED-NORMATIVELY |
| G-07 | Ambient consent/legal boundary | Module A.9 + platform profiles | CLOSED AS POLICY CONTRACT; jurisdiction mapping remains deployment concern |
| G-08 | Storage pressure | Module G.8 + scenario 37 | CLOSED-NORMATIVELY |
| G-09 | Usage accounting | Module G | CLOSED-NORMATIVELY |
| G-10 | Federated search | Module H + scenario 38 | CLOSED-NORMATIVELY |
| G-11 | Locale/timezone/language | Module I + scenario 39 | CLOSED-NORMATIVELY |
| G-12 | Import/export compatibility | Module J + scenario 40 | CLOSED-NORMATIVELY |
| G-13 | Idea/Thinking modes | Module K.3–K.5 | CLOSED AS RECIPE, not subsystem |
| G-14 | Briefing/retrospective | Module K.6–K.7 | CLOSED AS RECIPE/automation |
| G-15 | AI news radar | Module K.8 | CLOSED AS World Intelligence recipe |

`CLOSED-NORMATIVELY` означает, что бизнес-логика определена. Конкретная технология, schema и code layout выбираются архитектурой.

## 44. Что всё ещё не является бизнес-логическим пробелом

Следующие вопросы сознательно переходят в архитектурный этап:

- конкретные OS APIs и mobile background feasibility matrix;
- физический key hierarchy и crypto primitives;
- concrete database/object store/index engine;
- sync protocol and CRDT selection;
- updater technology;
- package serialization/container;
- message bus/API transports;
- exact query engine;
- cost metrics collection implementation;
- process isolation of adapters;
- data retention default numbers per device profile.

Они не должны заново менять продуктовые semantics этого файла без ADR и явной корректировки бизнес-логики.

---

# Часть VI. Требования к архитектуре после дополнений

## 45. Новые обязательные architecture views

Кроме views, уже заданных в `70_...`, architecture должна показать:

1. Sensor Source Runtime с audio/screen/camera/clipboard adapters.
2. Communication connector inbound/outbound + reconciliation.
3. Project lifecycle and workspace bindings.
4. Artifact storage/version/publication.
5. Update/compatibility/migration topology.
6. Recovery/key/device/head flow.
7. Resource Coordinator and pressure signals.
8. Federated Search query/data flows.
9. Locale/time service and schedule semantics.
10. Import/export quarantine and package pipeline.

## 46. Новые обязательные critical contracts

Architecture/code-level contracts should include:

- `SensorSourceDescriptor`;
- `AmbientCandidate`;
- `ConsentPolicyRef`;
- `CommunicationMessageRef`;
- `SendProposal`;
- `DeliveryReceipt`;
- `ProjectRecord`;
- `WorkspaceBinding`;
- `ArtifactRecord`/`ArtifactVersion`;
- `PublicationRecord`;
- `VersionManifest`;
- `CompatibilityHello`;
- `MigrationJournal`;
- `RecoveryKitManifest`;
- `UsageObservation`/`ResourceBudget`;
- `FederatedSearchResult`;
- `TemporalIntent`;
- `PortablePackageManifest`.

These names may change in code, but semantics must remain.

## 47. Обязательные architecture risk spikes

### R-01. Ambient audio on target Android and Windows

Measure:

- OS/background viability;
- battery;
- VAD/wake false positives;
- privacy indicators;
- recovery after app/service kill.

### R-02. Event-driven Windows screen context

Measure:

- Windows Graphics Capture/accessibility integration;
- excluded apps;
- visual diff/dedup;
- CPU/GPU/storage;
- user trust/visibility.

### R-03. Telegram user-account connector

Using TDLib or other chosen legal/technical path:

- inbound updates;
- drafts;
- exact sent confirmation;
- timeout reconciliation;
- multi-device consistency.

### R-04. Artifact version/publication

- large source + preview;
- exact version sharing;
- publication unknown result;
- secret scan;
- revoke.

### R-05. Mixed-version protocol migration

- old client/new Head;
- offline operation log;
- unknown fields;
- feature downgrade;
- rollback.

### R-06. Full owner recovery drill

- encrypted backup;
- new Head;
- key recovery;
- device revoke;
- external effect reconciliation;
- provider reauth.

### R-07. Disk pressure chaos test

Fill disk during:

- ambient capture;
- memory append;
- migration;
- artifact generation;
- backup.

Prove no silent canonical data loss.

### R-08. Federated search prototype

- exact + lexical + semantic + command;
- project/memory/artifact;
- offline source;
- privacy filters;
- RRF baseline.

### R-09. Portable project round-trip

- export by user A;
- sanitize;
- import by user B;
- Git merge/update;
- no credentials/private overlays;
- rebuild indexes.

## 48. Architecture acceptance deltas

The four architecture volumes are not ready if:

- ambient capture is described only as «use Screenpipe»;
- communication retry semantics omitted;
- `Delete Project` remains ambiguous;
- artifact is treated only as a file path;
- update assumes all components same version;
- recovery depends on provider-held magic key;
- disk full has no explicit state transition;
- global search is one vector query;
- schedule stores only UTC offset;
- import activates scripts before trust review.

---

# Часть VII. Карта последующего разнесения по каноническим файлам

## 49. Правило разнесения

При формировании итогового repository этот файл не остаётся вторым источником истины. Каждый нормативный блок переносится в canonical owner, а здесь остаётся archive/reference или удаляется после проверки diff.

Перенос выполняется по section IDs, сохраняя смысл, source references и acceptance criteria.

## 50. Точная карта

### В `01_Dennett_Specification_Index_and_Shared_Contracts.md`

- Sections 12–25: shared cross-domain invariants в сжатом виде.
- Minimal schemas/references from B.3.6–B.3.7, C.2, D.3, E.3/E.6, J.3 where truly cross-domain.
- Updated ownership matrix and canonical file list.

### В `10_Dennett_Memory_Fabric.md`

- A: committed sensory evidence, cross-modal episode, retention/deletion.
- B.15: communication evidence/social memory.
- C.5: project memory lifecycle.
- D.11/D.13/D.14: artifact provenance/deletion/tiering.
- H: memory search adapter/authority.
- I.9: original/translation/time provenance.
- J: project/research package semantics.
- G storage retention hooks.

### В `20_Dennett_Agentic_Control_Fabric.md`

- B.5–B.7: response reasoning and delivery proposal.
- C: project lifecycle ownership around sessions/runs.
- D.4–D.8: artifact outputs/approval.
- G.12: cost-aware execution.
- K: recipe/skill execution rules.

### В `30_Dennett_Trust_Identity_Autonomy_and_Permissions.md`

- A.9 consent/privacy policy;
- B disclosure/send authorization;
- C destructive/transfer actions;
- D publication/sharing;
- E signatures/update trust;
- F full recovery;
- J package trust/import;
- shared rules 15, 18, 19.

### В `40_Dennett_Voice_and_Ambient_Interaction_Fabric.md`

- A audio/screen as equal Ambient Sensory Sources;
- I multilingual/time interpretation;
- K voice recipes;
- scenario 26–27.

### В `41_Dennett_Capabilities_Providers_and_Integrations.md`

- B connector account/update semantics;
- C project capability detach/transfer;
- E adapter/plugin update compatibility;
- F provider reauth;
- G provider/local usage;
- J capability package;
- K recipe packaging.

### В `50_Dennett_Server_Runtime_Events_Sync_and_Portability.md`

- A Sensor Source Runtime;
- B inbound/outbound dispatch/reconciliation;
- C workspace/project lifecycle execution;
- D artifact storage/publication execution;
- E updater/migration/version negotiation;
- F recovery/Head restoration;
- G resource pressure/accounting runtime;
- H search federation/index jobs;
- I timezone/schedule service;
- J import/export staging;
- scenarios/risk spikes as runtime requirements.

### В `60_Dennett_Desktop_Application_Business_Logic.md`

- distinct ambient source controls;
- communication draft/send/reconcile states;
- project lifecycle commands;
- artifact version/publish/revoke;
- update/recovery/usage/search/time/import screens;
- exact consequence language.

### В `61_Dennett_Mobile_Application_Business_Logic.md`

- background source feasibility/status;
- quick communication actions;
- project/archive/delete distinctions;
- recovery kit/device loss;
- storage/battery pressure;
- global search partial/offline;
- timezone travel prompts;
- import/share entry points.

### В `70_Dennett_End_to_End_Validation_and_Architecture_Handoff.md`

- replace gap ledger statuses with closed references;
- add scenarios 26–42;
- add risk spikes R-01–R-09;
- architecture acceptance deltas.

## 51. Проверка разнесения

Before temporary file is retired:

1. every section mapped;
2. target file contains normative content;
3. no conflicting older rule remains;
4. cross-links updated;
5. E2E scenarios still resolve;
6. Specification Index versions updated;
7. temporary file marked `DISTRIBUTED` with commit references;
8. automated search verifies all IDs/terms accounted for.

---

# Часть VIII. Definition of Done

## 52. Документ завершён, если

1. Все G-01–G-15 имеют нормативное решение.
2. Каждый blocker имеет entities, lifecycle, main path, failures and acceptance.
3. Manual user intent remains low-friction.
4. Security does not require model call on normal fast path.
5. External effects have exact target and reconciliation.
6. Project and artifact deletion semantics are unambiguous.
7. Update and recovery support distributed/offline Dennett.
8. Ambient sources are platform-aware and local-first.
9. Storage/resource pressure never silently discards canonical data.
10. Search remains federated and authority-aware.
11. Time/language semantics survive travel and offline sync.
12. Import/export separates integrity, trust and permission.
13. Composite features remain recipes, not unnecessary subsystems.
14. Architecture risk spikes are explicit.
15. Every section has future redistribution owner.

## 53. Итоговая нормативная формула

> **Перед архитектурой Dennett получает недостающий слой целостности: сенсорные источники становятся управляемыми и platform-aware; внешняя коммуникация получает draft/send/reconciliation contract; проекты и artifacts — однозначные жизненные циклы; обновления и recovery — совместимость и проверяемое восстановление; ресурсы, поиск, время и переносимые пакеты — общие правила; а высокоуровневые пользовательские функции остаются адаптивными recipes поверх уже существующих primitives.**

---

# Appendix A. Source Ledger

## Internal Dennett specifications

**[S01] Dennett Functional Concept.** Product vision, ambient microphone/screen, projects, voice, messages, portability and adaptive principles.  
`Dennett_функциональная_концепция_v0.1_project_workflow(2).md`

**[S02] Dennett Specification Index and Shared Contracts.** Canonical ownership and boundary contracts.  
`01_Dennett_Specification_Index_and_Shared_Contracts.md`

**[S03] Dennett Memory Fabric 1.2.** Evidence, event ledger, project memory, sensory ingest, retention, deletion and retrieval.  
`Dennett_memory_logic_v1.2_pragmatic_semantic_2026.md`

**[S04] Dennett Agentic Control Fabric 1.1.** Project sessions, single-agent-first execution, Tasks/Runs, effects and completion.  
`Dennett_agent_orchestration_logic_v1.1_pragmatic_control_2026.md`

**[S05] Dennett Trust, Identity, Autonomy and Permissions.** Identity, grants, effects, voice assurance, secrets, import trust and recovery foundations.  
`30_Dennett_Trust_Identity_Autonomy_and_Permissions.md`

**[S06] Dennett Voice and Ambient Interaction Fabric.** Voice sessions, ambient edge, turn-taking and source behavior.  
`40_Dennett_Voice_and_Ambient_Interaction_Fabric.md`

**[S07] Dennett Capabilities, Providers and Integrations.** Connectors, skills, packages, providers, local backends and capability lifecycle.  
`41_Dennett_Capabilities_Providers_and_Integrations.md`

**[S08] Dennett Server Runtime, Events, Sync and Portability.** Head, devices, events, effects, sync, backup and recovery runtime.  
`50_Dennett_Server_Runtime_Events_Sync_and_Portability.md`

**[S09] Dennett Desktop Application Business Logic.** Desktop workbench, projects, Inbox, Radar, memory, artifacts and system controls.  
`60_Dennett_Desktop_Application_Business_Logic.md`

**[S10] Dennett Mobile Application Business Logic.** Interruptible mobile remote, capture, approvals, offline and device controls.  
`61_Dennett_Mobile_Application_Business_Logic.md`

**[S11] Dennett End-to-End Validation and Architecture Handoff.** Gap ledger, E2E requirements, quality scenarios and architecture readiness.  
`70_Dennett_End_to_End_Validation_and_Architecture_Handoff.md`

## Ambient capture, privacy and consent

**[S12] Android foreground service types — microphone.** Platform constraints for background microphone and while-in-use permissions.  
https://developer.android.com/develop/background-work/services/fgs/service-types

**[S13] Android MediaProjection.** User consent and lifecycle constraints for screen capture/projection sessions.  
https://developer.android.com/media/grow/media-projection

**[S14] Windows Graphics Capture.** Windows APIs and user-visible selection/capture model.  
https://learn.microsoft.com/en-us/windows/apps/develop/media-authoring-processing/screen-capture

**[S15] Screenpipe.** Event-driven local screen/audio capture reference with OCR/accessibility and search. Used as implementation evidence, not mandatory dependency.  
https://github.com/mediar-ai/screenpipe

**[S16] NIST Privacy Framework.** Risk-based privacy management enabling product utility and innovation.  
https://www.nist.gov/privacy-framework

**[S17] EDPB Guidelines on Virtual Voice Assistants.** Privacy/data-protection considerations for voice assistants.  
https://www.edpb.europa.eu/our-work-tools/our-documents/guidelines/guidelines-022021-virtual-voice-assistants_en

**[S18] Meaningful verbal consent research.** Evidence that spoken consent UX requires clarity and context rather than treating silence as agreement.  
https://dl.acm.org/doi/10.1145/3544548.3580711

**[S51] Apple `UIBackgroundModes`.** Platform declaration for supported background execution categories; actual microphone behavior remains permission- and lifecycle-bound.  
https://developer.apple.com/documentation/bundleresources/information-property-list/uibackgroundmodes

**[S52] Apple ReplayKit.** iOS/iPadOS screen recording and broadcast capture surface.  
https://developer.apple.com/documentation/replaykit

**[S53] Apple ScreenCaptureKit.** Native macOS screen and audio capture framework.  
https://developer.apple.com/documentation/screencapturekit

## External communication and idempotency

**[S19] TDLib Getting Started.** Asynchronous Telegram client, local storage/data consistency, updates and `updateMessageSendSucceeded`.  
https://core.telegram.org/tdlib/getting-started

**[S20] Telegram Bot API.** Request/update semantics, update IDs, webhook limitations and result states.  
https://core.telegram.org/bots/api

**[S21] Gmail API Drafts.** Draft as a distinct resource before sending.  
https://developers.google.com/gmail/api/guides/drafts

**[S22] Microsoft Graph `sendMail`.** Provider send acceptance semantics and permissions.  
https://learn.microsoft.com/en-us/graph/api/user-sendmail

**[S23] AWS Builders’ Library — Making retries safe with idempotent APIs.** Caller-provided request IDs, same-intent checks and safe retries.  
https://aws.amazon.com/builders-library/making-retries-safe-with-idempotent-APIs/

## Project and artifact lifecycle

**[S24] GitHub Archiving Repositories.** Reversible read-only archive semantics distinct from deletion.  
https://docs.github.com/en/repositories/archiving-a-github-repository/archiving-repositories

**[S25] GitHub Deleting a Repository.** Consequences of remote deletion, private forks and limited restore.  
https://docs.github.com/en/repositories/creating-and-managing-repositories/deleting-a-repository

**[S26] Git Worktree.** Multiple linked working trees and branch isolation.  
https://git-scm.com/docs/git-worktree

**[S27] W3C PROV Overview.** Entities, activities, agents, derivation and provenance interoperability.  
https://www.w3.org/TR/prov-overview/

**[S28] Amazon S3 Versioning.** Retaining multiple object versions and recovery from overwrite/deletion.  
https://docs.aws.amazon.com/AmazonS3/latest/userguide/Versioning.html

**[S29] GitHub Releases.** Named software releases, tags, notes and assets.  
https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases

## Updates and compatibility

**[S30] The Update Framework.** Secure software update metadata and compromise-resilient distribution principles.  
https://theupdateframework.io/

**[S31] Semantic Versioning 2.0.0.** Public API compatibility signalling and immutable released versions.  
https://semver.org/

**[S32] Protocol Buffers — Updating a Message Type.** Wire-safe, unsafe and conditionally compatible changes and unknown fields.  
https://protobuf.dev/programming-guides/proto3/#updating

**[S33] Kubernetes Version Skew Policy.** Explicit compatibility windows between distributed components.  
https://kubernetes.io/releases/version-skew-policy/

## Identity and recovery

**[S34] NIST SP 800-63B.** Authentication assurance, authenticators, recovery and lifecycle principles.  
https://pages.nist.gov/800-63-4/sp800-63b.html

**[S35] Apple Platform Security — Advanced Data Protection for iCloud.** E2EE recovery responsibility, recovery contacts and recovery keys.  
https://support.apple.com/guide/security/advanced-data-protection-for-icloud-sec973254c5f/web

**[S36] 1Password Secret Key.** User-held secret/recovery kit and limits of provider recovery.  
https://support.1password.com/secret-key-security/

**[S37] Bitwarden Emergency Access.** Trusted emergency contacts, waiting period and access policy.  
https://bitwarden.com/help/emergency-access/

## Resources, telemetry and search

**[S38] FOCUS — FinOps Open Cost & Usage Specification.** Vendor-neutral normalization across AI, cloud, SaaS and other technology usage/cost data.  
https://focus.finops.org/

**[S39] OpenTelemetry Semantic Conventions.** Common names and meanings for traces, metrics, logs, GenAI, hardware and devices.  
https://opentelemetry.io/docs/specs/semconv/

**[S40] Elasticsearch Reciprocal Rank Fusion.** Rank fusion across different retrievers without calibrated scores.  
https://www.elastic.co/docs/reference/elasticsearch/rest-apis/reciprocal-rank-fusion

**[S41] OpenSearch Hybrid Search.** Reference implementation pattern for combining keyword and semantic search.  
https://opensearch.org/docs/latest/vector-search/ai-search/hybrid-search/index/

## Locale and time

**[S42] IANA Time Zone Database.** Canonical timezone rule data updated for political/DST changes.  
https://www.iana.org/time-zones

**[S43] Unicode CLDR.** Locale data for dates, numbers, units, plurals and language/region display.  
https://cldr.unicode.org/

**[S44] BCP 47 / RFC 5646.** Language tags.  
https://www.rfc-editor.org/rfc/rfc5646

**[S45] RFC 3339.** Internet date/time timestamp representation.  
https://www.rfc-editor.org/rfc/rfc3339

## Portable packages and recipes

**[S46] RO-Crate Specification 1.3.** Portable linked metadata packaging for research/software artifacts.  
https://www.researchobject.org/ro-crate/specification/1.3/

**[S47] RFC 8493 — BagIt.** Directory payload, metadata tags and checksum manifests for reliable arbitrary-content transfer.  
https://www.rfc-editor.org/rfc/rfc8493

**[S48] JSON Schema Draft 2020-12.** Machine-readable JSON document/schema validation.  
https://json-schema.org/draft/2020-12/json-schema-core

**[S49] Home Assistant Core and Automation Blueprints.** Event/state/service separation and reusable user-customizable automation templates.  
https://developers.home-assistant.io/docs/architecture/core/  
https://www.home-assistant.io/docs/automation/using_blueprints/

**[S50] OCI Image Index Specification.** Higher-level manifest selecting platform-specific component manifests.  
https://github.com/opencontainers/image-spec/blob/main/image-index.md

---

# Appendix B. Redistribution Checklist

```text
[ ] A Ambient module distributed
[ ] B Communication module distributed
[ ] C Project lifecycle distributed
[ ] D Artifact lifecycle distributed
[ ] E Update/compatibility distributed
[ ] F Recovery distributed
[ ] G Resource/usage distributed
[ ] H Search distributed
[ ] I Locale/time/language distributed
[ ] J Import/export distributed
[ ] K Recipes distributed
[ ] Shared invariants merged into 01
[ ] E2E scenarios merged into 70
[ ] Risk spikes linked from architecture volumes
[ ] Source ledger references retained
[ ] Temporary document marked DISTRIBUTED
```

Конец документа.
