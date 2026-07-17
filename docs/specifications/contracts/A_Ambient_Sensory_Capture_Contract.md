# Модуль A. Ambient Sensory Capture Contract

> **Канонический cross-domain supplement · `A`**  
> **Primary owner:** 40 Voice / 50 Server / 10 Memory / 30 Trust.  
> **Происхождение:** выделено из предархитектурного gap-аудита. Документ актуален и нормативен.  
> Маркеры источников вида `[[Sxx]]` раскрыты в [`REFERENCES.md`](REFERENCES.md). Ownership и порядок чтения описаны в [`README.md`](README.md).


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
