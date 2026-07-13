# Denet Capabilities, Providers and Integrations

> **Repository edition · 2026-07-13 · `41`**  
> Это самостоятельный канонический документ репозитория Denet. Начните с [карты документации](../README.md).  
> Related: [30_Denet_Trust_Identity_Autonomy_and_Permissions.md](./30_Denet_Trust_Identity_Autonomy_and_Permissions.md).

## Интегрированные contract supplements

Следующие небольшие нормативные документы выделены из предархитектурного gap-аудита. Они являются частью текущего набора и обязательны для изменений, пересекающих указанные границы:

- [`B_External_Communication_Operation.md`](contracts/B_External_Communication_Operation.md)
- [`E_Update_Compatibility_and_Migration_Contract.md`](contracts/E_Update_Compatibility_and_Migration_Contract.md)
- [`G_Resource_Pressure_and_Usage_Accounting_Contract.md`](contracts/G_Resource_Pressure_and_Usage_Accounting_Contract.md)
- [`J_Import_Export_and_Portable_Package_Compatibility_Contract.md`](contracts/J_Import_Export_and_Portable_Package_Compatibility_Contract.md)
- [`K_Composite_Experience_Recipes.md`](contracts/K_Composite_Experience_Recipes.md)

Эти supplements не дают одному lifecycle разойтись по нескольким большим файлам; canonical owner указан в заголовке каждого документа.


## Полная бизнес-логика обнаружения, приобретения, сравнения, подключения, развития и эксплуатации моделей, agent runtimes, skills, MCP, plugins, connectors, локальных backend и computer-use возможностей

**Версия:** 1.1  
**Дата исследования:** 11 июля 2026 года  
**Статус:** канонический исследовательский baseline бизнес-логики Capability Fabric.  
**Каноническое имя:** `41_Denet_Capabilities_Providers_and_Integrations.md`.

Документ продолжает и не заменяет:

- `00_Denet_Functional_Concept.md`;
- `01_Denet_Specification_Index_and_Shared_Contracts.md`;
- `10_Denet_Memory_Fabric.md` в актуальной версии 1.2;
- `20_Denet_Agentic_Control_Fabric.md` в актуальной версии 1.1;
- `30_Denet_Trust_Identity_Autonomy_and_Permissions.md`.

Он определяет **абстрактную бизнес-логику всей системы возможностей Denet**:

- откуда возможности берутся;
- как Denet узнаёт, что они существуют;
- как различается ручное добавление пользователем, автоматическое обнаружение, импорт проекта и обучение на опыте;
- как capability идентифицируется, сравнивается с уже имеющимися и получает область применения;
- как выбирается между нативным provider tool, MCP, connector, CLI, API, skill и computer-use;
- как capability подключается к пользователю, проекту, агенту или одному Run;
- как оценивается её полезность без превращения системы в постоянный benchmark-конвейер;
- как capabilities обновляются, форкаются, объединяются, откатываются, устаревают и удаляются;
- как Denet использует нативные возможности разных провайдеров и локальных runtimes;
- как динамический рынок отделяется от устойчивой логики продукта.

Документ **не описывает**:

- расположение кнопок и экранов — это Desktop и Mobile specifications;
- очереди, таблицы БД, процессы и deployment — это Server Runtime и будущая архитектура;
- разрешено ли конкретное действие — этим владеет Trust Fabric;
- сколько агентов запускать и как решать задачу — этим владеет Agentic Control Fabric;
- долговременную память — этим владеет Memory Fabric;
- turn-taking и речевое поведение — этим будет владеть Voice Fabric.

Главный принцип границы:

> **Capability Fabric определяет, что доступно, откуда это взялось, насколько подходит, как подключается и в каком состоянии находится. Agentic Control решает, зачем и когда это применить. Trust решает, можно ли совершить конкретное действие. Server исполняет жизненный цикл. UI даёт человеку управление.**

---

# 0. Итоговый вердикт

## 0.1. Главная модель

Denet должен иметь не каталог ссылок и не один плоский список установленных инструментов, а **Federated Capability Fabric** — живую систему, которая соединяет:

1. модели и provider connections;
2. полноценные agent runtimes;
3. локальные model artifacts и serving runtimes;
4. skills и процедурные пакеты;
5. MCP servers и их tools/resources/prompts;
6. provider-native plugins, extensions и apps;
7. connectors и account bindings;
8. browser/computer-use backends;
9. speech, vision, image, video, OCR и media backends;
10. web/search, code execution, shell и retrieval capabilities;
11. capabilities, созданные самим Denet из опыта;
12. project-local capabilities, которые не обязаны становиться глобальными.

Capability Fabric не стремится сделать всё одинаковым. Он использует небольшой общий contract и сохраняет provider-specific semantics в нативных адаптерах.

Короткая формула:

> **Denet Capability Fabric — это федеративная, origin-aware и project-aware система возможностей, которая умеет находить, приобретать, сравнивать, безопасно подключать, минимально активировать и постепенно улучшать capabilities, не превращая каждое действие в бюрократию и не привязывая Denet к одному поставщику.**

## 0.2. Главное изменение относительно версии 1.0

Версия 1.0 хорошо описывала рынок, provider adapters, источники skills/MCP и общий installation lifecycle, но недостаточно подробно отвечала на вопросы:

- что происходит после автоматического обнаружения;
- чем ручной импорт отличается от автоматического;
- как сравнить новый skill или MCP с существующим;
- как извлечь один полезный элемент из в целом худшего capability;
- кто владеет локальными изменениями;
- как capabilities появляются в проекте и исчезают из него;
- когда capability действует один Run, а когда закрепляется;
- как опыт проекта превращается в project-local skill;
- как и при каких доказательствах capability продвигается в общую коллекцию;
- как обрабатывать upstream update, fork, merge, rollback и removal;
- как выбирать native integration, MCP, CLI или computer-use для одной функции.

Версия 1.1 делает эту логику центральной. Датированный каталог рынка сохраняется, но становится приложением к устойчивой бизнес-логике.

## 0.3. Минимально достаточная сложность

Capability Fabric не должен запускать отдельного «куратора возможностей» при каждом действии.

Обычный fast path:

```text
известная capability
→ уже подключена к проекту или разрешена on-demand
→ совместима с текущей моделью и средой
→ здорова
→ Trust grant подходит
→ использовать без нового model call
```

Модельный анализ нужен только при:

- автоматическом обнаружении неизвестной capability;
- смысловом сравнении с существующими;
- выборе между несколькими существенно разными кандидатами;
- извлечении уникального полезного delta;
- создании skill из опыта;
- неоднозначном конфликте или обновлении;
- маршрутизации, которую нельзя решить накопленными измерениями и правилами.

Статические manifest checks, hashes, dependency inspection, compatibility filters, health probes и policy lookup выполняются без LLM.

## 0.4. Origin-aware поведение

Одна и та же папка `SKILL.md` должна обрабатываться по-разному в зависимости от происхождения.

### Пользователь добавил вручную

Пользовательское действие является достаточным основанием **немедленно добавить capability в коллекцию**. Denet не запускает обязательный utility review, не сравнивает её со всей библиотекой и не пытается заменить выбор пользователя.

При этом:

- технический parse выполняется;
- provenance и revision сохраняются;
- executable parts перечисляются;
- право исполнять scripts/hooks всё равно определяется Trust Fabric;
- capability получает признак `user-selected`;
- Denet не переписывает её автоматически;
- любые улучшения оформляются как patch proposal или fork.

### Capability найдена автоматически

Автоматическое обнаружение не является установкой. Candidate проходит:

```text
cheap inspection
→ origin/security metadata
→ relevance triage
→ comparison with collection
→ disposition
```

Возможные disposition:

- проигнорировать;
- сохранить ссылку;
- поместить в quarantine;
- предложить пользователю;
- использовать один Run в ограниченном режиме;
- добавить project-local candidate;
- протестировать;
- извлечь полезный delta;
- заменить/дополнить существующую capability;
- продвинуть в global collection после доказанной пользы.

### Capability пришла с проектом

Она импортируется в **Project Capability Space**, не в глобальную библиотеку. Проект может использовать её в своём scope после Trust handoff. Global promotion требует отдельного решения.

### Capability возникла из опыта Denet

Сначала система решает, что именно было обнаружено:

- факт для памяти;
- project instruction;
- короткий prompt pattern;
- skill;
- script/tool;
- MCP/connector requirement;
- workflow/procedure;
- одноразовый workaround.

Default — самая лёгкая форма. Новый skill создаётся только если процедура действительно повторно применима и не помещается разумно в короткую instruction или memory note.

## 0.5. Не всё найденное нужно копировать

Новый capability может быть:

- дубликатом;
- полной заменой;
- специализацией;
- дополнением;
- fallback;
- wrapper/adapter;
- конфликтующей реализацией;
- устаревшим fork;
- уникальной capability;
- в целом худшим решением с одним полезным свойством.

В последнем случае Denet создаёт **Capability Delta Proposal**:

```text
новый capability
→ выделить уникальное преимущество
→ проверить, независимо ли оно от остального пакета
→ определить подходящий target capability
→ подготовить patch/fork/extension
→ проверить на representative tasks
→ принять, отклонить или оставить как optional fragment
```

Исходный capability не копируется целиком без необходимости, provenance delta сохраняется.

## 0.6. Пять логических пространств

Вместо одного списка используются пять уровней:

1. **Registry** — всё, о чём Denet знает, включая неустановленные кандидаты.
2. **Collection** — capabilities, которые пользователь или Denet приобрели и могут повторно использовать.
3. **Candidate Pool** — автоматически найденные, предложенные или экспериментальные capabilities.
4. **Project Capability Set** — capabilities конкретного проекта: required, pinned, on-demand, local, forbidden.
5. **Run Capability Plan** — минимальный набор, реально активированный для текущего Turn/Task/Run.

Это снижает context bloat: наличие capability в Registry не означает, что её описание попадёт в prompt.

## 0.7. Пользователь регулирует автономность управления capabilities

Настраиваются независимо:

- автоматический поиск;
- автоматическая установка project-local capabilities;
- автоматический restricted test;
- создание project skills;
- изменение Denet-managed skills;
- глобальное продвижение;
- автоматические updates;
- provider fallback;
- загрузка executable components;
- использование community registries.

Базовые профили:

### Manual

Denet ничего не устанавливает и не закрепляет без явной команды. Может показывать найденное.

### Suggest

Denet ищет, сравнивает и предлагает. Это рекомендуемый default для глобальной коллекции.

### Project-Autonomous

В доверенном проекте Denet может добавлять и тестировать project-local candidates в пределах бюджета и Trust policy, не продвигая их глобально.

### Curated-Autonomous

Denet может автоматически обновлять Denet-managed capabilities и продвигать проверенные candidates по установленным acceptance rules. User-owned assets остаются защищёнными от автоматического переписывания.

---

# 1. Область ответственности и границы

## 1.1. Чем документ владеет

Документ канонически описывает:

- Provider Definition, Connection и Model Endpoint;
- Agent Runtime Adapter;
- Local Runtime и Model Artifact;
- Capability Definition, Candidate, Installation, Binding и Relation;
- Capability Registry, Collection и Project Capability Set;
- Skill lifecycle;
- MCP server/component lifecycle;
- plugin/extension/app lifecycle;
- connector и account binding lifecycle;
- computer-use/backend lifecycle;
- media backend lifecycle;
- discovery, inspection, comparison и disposition;
- ownership, update, fork, merge, rollback и removal;
- project assembly и run activation;
- health, quota, fallback и measured utility;
- dated provider/backend catalogue.

## 1.2. Чем документ не владеет

Он не определяет:

- permission decision;
- workspace trust;
- secret storage;
- agent decomposition;
- task state machine;
- memory ingestion/retrieval;
- server process topology;
- UI navigation;
- exact API/DB implementation.

## 1.3. Правило отсутствия дублирования

Если capability должна появиться в интерфейсе, UI document описывает экран и действия пользователя, но не переопределяет lifecycle.

Если capability устанавливается сервером, Server document описывает durable execution, но не решает, должна ли capability быть установлена.

Если capability вызывает внешний effect, Trust document разрешает или запрещает invocation, но не владеет её catalog metadata и measured utility.

---

# 2. Исследовательский протокол

## 2.1. План для построения плана

Исследование разделено на семь независимых вопросов:

1. Какой устойчивый internal model нужен независимо от рынка?
2. Какие lifecycle patterns уже доказали практическую ценность в package/plugin ecosystems?
3. Как skills реально влияют на качество, токены и безопасность?
4. Как выбирать между native tools, MCP, CLI/API и computer-use?
5. Как поддерживать локальные модели без ложной эквивалентности runtimes?
6. Как сохранять provider-native преимущества и одновременно иметь общий выбор?
7. Как сделать саморазвитие capabilities полезным, но не превратить его в постоянный дорогой агентный процесс?

## 2.2. Классы источников

Приоритет источников:

1. официальные specifications и provider documentation;
2. официальные repositories и reference implementations;
3. peer-reviewed papers и arXiv preprints с понятной методологией;
4. production case studies и крупные open-source ecosystems;
5. issue trackers как evidence конкретных failure modes;
6. community catalogues только как discovery hints.

Официальная документация подтверждает наличие функции, но не доказывает её качество. Benchmark показывает результат в конкретных условиях, но не превращается в универсальную истину Denet.

## 2.3. Проверяемые гипотезы

### H1. Чем больше skills подключено, тем лучше агент

**Отклонено.** SWE-Skills-Bench обнаружил нулевой прирост у большинства проверенных skills, значительный token overhead у части и деградацию от version mismatch. Skills активируются лениво и должны иметь measured marginal utility. [[S01]]

### H2. Автоматически найденный skill можно ставить после одной LLM-оценки

**Отклонено.** Skills могут содержать executable code, hooks и скрытые instructions. Нужны provenance, static inspection, Trust handoff и ограниченный test; одна модель не является security boundary. Масштабные исследования 2026 года обнаружили существенную долю уязвимых и подтверждённо вредоносных skills. [[S02]] [[S03]]

### H3. Пользовательский ручной импорт надо задерживать тем же review

**Отклонено.** Пользователь должен иметь право немедленно добавить capability. Utility/security review не является admission gate в Collection. Но добавление и право исполнения — разные операции.

### H4. Лучший найденный skill надо копировать целиком

**Отклонено.** Часто полезен только один operational contract или verification pattern. Нужны relation analysis и delta extraction.

### H5. Один MCP server на домен достаточно выбрать по популярности

**Отклонено.** Servers могут иметь разные auth, tools, resources, reliability, update cadence и blast radius. Marketplace rank — только discovery signal.

### H6. MCP должен быть default для каждой integration

**Отклонено.** Native API/SDK/CLI часто лучше по latency, semantics, security и context cost. MCP — важный open transport, но не догма.

### H7. Generic OpenAI-compatible adapter достаточно для всех models

**Отклонено.** Совпадение endpoint shape не гарантирует tool calling, reasoning, multimodal, streaming или state semantics.

### H8. Local model — это просто URL localhost

**Отклонено.** Capability зависит от artifact, quantization, runtime, hardware, context, loaders и version.

### H9. Capability self-improvement должно быть непрерывным

**Отклонено.** Оно запускается по evidence: повторяющийся успех/провал, user correction, version drift или явный запрос. Background review предлагает candidates, но не обязан переписывать библиотеку после каждого Turn.

## 2.4. Критерии принятия механизма

Механизм принимается, если он:

- решает наблюдаемый failure mode;
- улучшает cost-of-success относительно более простого варианта;
- не требует постоянного LLM overhead;
- имеет понятный ownership;
- допускает rollback/removal;
- не смешивает discovery с trust;
- не загружает нерелевантные capabilities в context;
- сохраняет user choice;
- работает для project-local и global scopes;
- не привязывает Denet к одному marketplace.

## 2.5. Критерии отказа

Решение пересматривается, если:

- capability library начинает расти быстрее полезности;
- выбор требует отдельной модели при каждом tool call;
- автоматическое merge повреждает user-owned content;
- project-local знания бесконтрольно продвигаются глобально;
- generic skills увеличивают токены без результата;
- provider adapter скрывает потерю native semantics;
- update молча расширяет effects/scopes;
- removal ломает project artifacts или историю;
- каталог становится источником устаревшей «истины»;
- capability с низкой measured utility остаётся pinned по умолчанию.

---

# 3. Основные проблемы наивной capability-системы

## 3.1. Каталог не равен жизненному циклу

Список providers и tools отвечает только на вопрос «что существует». Для реальной системы нужно знать:

- откуда capability пришла;
- кому принадлежит;
- можно ли менять;
- к какому scope относится;
- с чем конфликтует;
- что доказало полезность;
- какая версия использовалась;
- что произойдёт при update/remove.

## 3.2. Установка не равна доверию и активации

Capability может быть:

- известна, но не установлена;
- установлена, но отключена;
- подключена, но не доверена;
- доверена, но не разрешена текущей Task;
- доступна проекту, но не активирована в Run;
- активна, но нездорова;
- здорова, но хуже альтернативы.

Эти оси нельзя сводить к одному статусу.

## 3.3. Tool explosion

Если агент видит сотни tool descriptions или десятки skills целиком, качество выбора падает, контекст дорожает, а дублирующиеся interfaces конкурируют. Tool retrieval и progressive disclosure должны быть частью Fabric, а не prompt-трюком. Исследования tool retrieval отдельно показывают, что overlap/redundancy осложняют выбор и что filtering/merging могут улучшать результат. [[S04]] [[S05]]

## 3.4. Неявное владение создаёт потерю данных

Без ownership upstream update может затереть ручные правки пользователя, а Denet-generated update — изменить imported skill. Нужны managed, user-owned и forked варианты.

## 3.5. Автоматический discovery легко превращается в supply-chain канал

Проект, сайт или marketplace может предложить package, который выглядит полезным и одновременно содержит malicious script, hidden hook или prompt injection. Auto-discovery не должен означать auto-execution.

## 3.6. Один provider может иметь несколько разных surfaces

Например, subscription runtime, API account, cloud deployment и desktop plugin marketplace не являются одной Connection. Model identity и capability set зависят от surface.

## 3.7. Полезность capability контекстна

Skill может быть лучшим для одного framework version и вредным для другого. Computer-use backend может быть лучшим для одного приложения и плохим для другого. «Лучший глобально» часто не существует.

---

# 4. Неподвижные инварианты

1. Capability Registry не является permission registry.
2. Ручной выбор пользователя добавляет capability без обязательного utility gate.
3. Executable capability не получает право исполнения только из факта добавления.
4. Auto-discovered capability не устанавливается глобально без policy.
5. Project-imported capability остаётся project-local по умолчанию.
6. User-owned capability не переписывается Denet автоматически.
7. Denet-managed capability всегда versioned и rollbackable.
8. Provider-managed component обновляется по нативным правилам provider, но Denet хранит observed diff.
9. Native provider semantics сохраняются.
10. MCP не считается автоматически лучше native API/SDK.
11. Capability selection учитывает measured utility и overhead.
12. Нерелевантные capabilities не попадают в Context Manifest.
13. Registry хранит dated observations, а не вечные provider facts.
14. Обновление security-relevant facets повторно проходит Trust handoff.
15. Removal не удаляет исторические receipts и созданные artifacts.
16. Один Run может временно использовать capability без постоянного attachment.
17. Глобальное продвижение требует более сильного evidence, чем project-local включение.
18. Capability relation и provenance сохраняются при fork/merge/delta extraction.
19. Public benchmark является prior, а реальные Denet outcomes — локальным evidence.
20. Самоулучшение должно уметь удалить или упростить capability, а не только добавить новую.

---

# 5. Канонические сущности Capability Fabric

## 5.1. Capability Definition

Стабильное описание того, **какую способность** предоставляет компонент, независимо от конкретной установки.

Примеры:

- `web.search`;
- `browser.structured_control`;
- `code.agent_runtime`;
- `skill.github_pr_workflow`;
- `connector.telegram.messages`;
- `speech.streaming_asr`;
- `model.reasoning_text`.

```yaml
capability_definition:
  capability_id: stable_id
  capability_class: model | runtime | skill | mcp | plugin | connector | computer_use | speech | media | tool
  purpose: text
  inputs: schema_or_description
  outputs: schema_or_description
  declared_effects: []
  preconditions: []
  portability: typed
  common_facets: {}
  native_extensions: {}
```

## 5.2. Capability Source

Откуда Denet узнал о capability:

- user path/URL/repository;
- project repository;
- provider marketplace;
- official registry;
- community catalogue;
- provider API discovery;
- local scan;
- Denet experience;
- agent proposal;
- imported project pack.

Source хранит URI, revision, publisher identity, source authority, fetch time и licensing information.

## 5.3. Capability Candidate

Неустановленная или непроверенная capability, которая может быть полезна.

```yaml
capability_candidate:
  candidate_id: id
  source_ref: ref
  discovered_by: user | project_scan | agent | registry_sync | web_search | provider | experience
  proposed_scope: global | user | project | run
  reason: text
  cheap_inspection: {}
  relation_candidates: []
  recommended_dispositions: []
  expires_or_recheck_at: optional
```

## 5.4. Capability Artifact

Конкретный физический объект:

- skill folder;
- plugin package;
- model weights;
- binary;
- MCP package;
- connector adapter;
- configuration bundle;
- script;
- container image.

Artifact identity включает hash/revision, потому что название недостаточно.

## 5.5. Capability Installation

Факт приобретения capability данной установкой Denet.

```yaml
capability_installation:
  installation_id: id
  definition_ref: ref
  artifact_refs: []
  source_ref: ref
  installed_version: text
  installation_mode: user_manual | project_import | provider_managed | denet_managed | external_reference
  ownership: user | denet | provider | project | shared
  local_modification_state: pristine | modified | forked | conflicted
  possession_state: reference | cached | installed | connected
  trust_ref: ref
  utility_state: unknown | candidate | validated | degraded | rejected
  health_state: ref
  update_policy: ref
```

## 5.6. Capability Binding

Связь Installation с конкретным scope:

- user;
- project;
- agent definition;
- device;
- provider;
- Task/Run.

```yaml
capability_binding:
  binding_id: id
  installation_ref: ref
  scope_ref: ref
  activation: disabled | discoverable | on_demand | active | pinned
  priority: normal | preferred | fallback | required
  overrides: {}
  conflict_policy: ref
  created_by: user | agent | project_import | policy
  expires_at: optional
```

## 5.7. Capability Relation

```yaml
capability_relation:
  left_ref: ref
  right_ref: ref
  relation: duplicate | substitute | specialization | complement | wrapper | fallback | conflict | fork | successor | contains_delta
  applicable_context: text
  evidence_refs: []
  confidence: typed
```

Relation не означает автоматического удаления одного элемента.

## 5.8. Capability Delta Proposal

Описание полезного фрагмента, который стоит перенести:

```yaml
capability_delta:
  source_capability_ref: ref
  target_capability_ref: optional
  unique_value: text
  affected_sections_or_files: []
  dependencies: []
  provenance_refs: []
  patch_or_extension_artifact: optional
  validation_plan: text
  ownership_constraints: []
  status: proposed | testing | accepted | rejected | superseded
```

## 5.9. Capability Evaluation Record

Хранит не абстрактный рейтинг, а контекстный outcome:

- project/task class;
- model/runtime;
- environment/version;
- baseline;
- success/failure;
- quality;
- latency;
- tokens/cost;
- user/reviewer feedback;
- compatibility issues;
- security incidents.

## 5.10. Provider Definition, Connection и Endpoint

Остаются различными:

- Provider Definition — тип сервиса;
- Connection — конкретная subscription/API/cloud/local configuration;
- Model Endpoint — конкретная model/deployment;
- Agent Runtime — stateful environment поверх provider;
- Model Artifact — локальные weights/config/tokenizer;
- Runtime Instance — запущенная локальная/удалённая serving instance.

## 5.11. Project Capability Set

```yaml
project_capability_set:
  project_ref: ref
  requirements: []
  required_bindings: []
  pinned_bindings: []
  on_demand_bindings: []
  project_local_installations: []
  forbidden_or_incompatible: []
  temporary_experiments: []
  fallback_relations: []
  last_reconciled_at: time
```

## 5.12. Run Capability Plan

Минимальный набор, выбранный для текущего Run:

```yaml
run_capability_plan:
  run_ref: ref
  resolved_requirements: []
  selected_bindings: []
  deferred_candidates: []
  tool_exposure_strategy: lazy | eager_minimal
  native_provider_extensions: {}
  trust_grants: []
  fallback_order: []
  resolution_evidence: []
```

---

# 6. Ортогональные состояния вместо монструозной state machine

Capability не должна проходить одну линейную цепочку из двадцати статусов. Используются независимые facets.

## 6.1. Discovery state

- unknown;
- discovered;
- inspected;
- normalized;
- stale discovery.

## 6.2. Possession state

- reference-only;
- cached;
- installed;
- connected;
- provider-managed.

## 6.3. Trust state

Владеет Trust Fabric:

- unreviewed;
- restricted;
- trusted-bounded;
- elevated;
- revoked.

## 6.4. Utility state

- unknown;
- candidate;
- promising;
- validated;
- context-specific;
- degraded;
- rejected.

## 6.5. Activation state

- disabled;
- discoverable;
- on-demand;
- active;
- pinned.

## 6.6. Ownership state

- user-owned;
- Denet-managed;
- provider-managed;
- project-shared;
- external-read-only;
- forked.

## 6.7. Health state

- healthy;
- slow;
- quota-limited;
- partially degraded;
- auth-required;
- incompatible;
- offline;
- unknown.

Эти facets позволяют сказать: «skill установлен пользователем, доверен в project sandbox, utility пока неизвестна, активируется on-demand и имеет upstream update» без искусственного линейного статуса.

---

# 7. Универсальный origin-aware lifecycle

## 7.1. Этап 1: Need Detection

Потребность может появиться из:

- прямого запроса пользователя;
- требований проекта;
- ошибки текущего capability;
- отсутствия нужного backend;
- повторяющейся ручной процедуры;
- provider outage;
- импорта чужого проекта;
- agent proposal;
- proactive discovery;
- update/deprecation.

Need описывает результат, а не бренд:

```text
нужен локальный streaming ASR с русским языком и низкой задержкой
```

лучше, чем:

```text
установить package X
```

Если пользователь явно назвал package, его выбор имеет высокий приоритет.

## 7.2. Этап 2: Discovery

Поиск идёт от наиболее авторитетных и дешёвых источников:

1. уже установленная Collection;
2. project-local capabilities;
3. provider-native capabilities;
4. официальные registries/marketplaces;
5. private organization sources;
6. official repositories;
7. community catalogues;
8. общий web search.

Discovery может быть:

- explicit;
- need-driven;
- project scan;
- periodic catalogue refresh;
- experience-driven.

Автоматический общий web search не запускается постоянно. Он включается, когда локальные/официальные источники не покрывают need или пользователь просит поиск.

## 7.3. Этап 3: Identity normalization

Denet пытается понять, являются ли два результата одной capability:

- canonical package/repository identity;
- publisher/namespace;
- hash/revision;
- marketplace aliases;
- forks;
- mirrors;
- renamed projects;
- wrapped versions.

Нельзя дедуплицировать только по названию.

## 7.4. Этап 4: Cheap Inspection

Без модели проверяются:

- manifest/frontmatter;
- files и размеры;
- executable scripts/hooks/binaries;
- dependencies;
- requested tools/secrets/network;
- license;
- supported providers/platforms;
- schemas;
- hashes/signatures;
- version/revision;
- obvious dangerous patterns;
- compatibility claims.

Для instruction-only skill cheap inspection может быть почти мгновенным. Для package с install script inspection глубже.

## 7.5. Этап 5: Origin policy

### Manual user add

```text
register
→ add to Collection immediately
→ mark user-selected/user-owned
→ technical metadata extraction
→ Trust handles first execution
```

Никакого обязательного сравнения и utility model call.

### Project import

```text
register in Project Space
→ preserve source/revision
→ do not promote global
→ reconcile project requirements
→ Trust handles execution
```

### Automatic discovery

```text
cheap inspection
→ relevance triage
→ if promising: semantic/safety analysis
→ collection comparison
→ disposition
```

### Experience-derived

```text
classify learned object
→ choose lightest representation
→ create project-local candidate
→ optional validation
→ later promotion
```

### Provider-managed discovery

Provider marketplace/update semantics сохраняются; Denet фиксирует components и changes, но не пытается управлять package как обычным local folder, если provider этого не допускает.

## 7.6. Этап 6: Comparison

Comparison выполняется только если candidate автоматически найден, пользователь просит рекомендацию или возникает конфликт.

Сравниваются:

- purpose и trigger conditions;
- scope;
- prerequisites;
- model/provider/platform assumptions;
- tools и external effects;
- scripts/hooks;
- output contract;
- verification;
- token/context footprint;
- measured outcomes;
- maintenance/update cadence;
- license;
- security surface;
- failure modes.

Сначала применяются deterministic features и retrieval. LLM используется для смыслового delta, а не для повторного чтения всей библиотеки.

## 7.7. Этап 7: Disposition

Допустимые решения:

- `ignore`;
- `bookmark`;
- `quarantine`;
- `notify_user`;
- `acquire_reference`;
- `install_project_local`;
- `install_global_candidate`;
- `run_restricted_trial`;
- `attach_for_one_run`;
- `attach_on_demand`;
- `pin_to_project`;
- `replace`;
- `keep_as_specialization`;
- `keep_as_fallback`;
- `extract_delta`;
- `fork`;
- `reject`.

## 7.8. Этап 8: Acquisition

Acquisition может означать:

- сохранить ссылку;
- clone/download;
- установить package;
- подключить remote endpoint;
- авторизовать account;
- зарегистрировать provider-native plugin;
- импортировать model artifact;
- добавить external directory;
- создать Denet-managed fork.

Acquisition не означает activation и не означает permission.

## 7.9. Этап 9: Binding

Capability может привязываться:

- глобально;
- конкретному пользователю;
- проекту;
- agent definition;
- устройству;
- одному Run;
- provider surface.

Минимальный scope предпочтительнее глобального.

## 7.10. Этап 10: Lazy Activation

В Context Manifest попадают:

- названия/краткие descriptors потенциально релевантных skills;
- tool search handle или ограниченный tool set;
- provider-native capabilities;
- project-pinned capability;
- обязательные fallback relations.

Полное содержание и schemas раскрываются только при выборе.

OpenAI, Agent Skills и Hermes используют progressive disclosure: сначала краткая metadata, затем полный `SKILL.md` или reference по необходимости. Это принимается как базовый паттерн Denet. [[S06]] [[S07]]

## 7.11. Этап 11: Outcome Observation

После использования сохраняются:

- удалось ли выполнить задачу;
- что именно дала capability;
- overhead;
- errors;
- fallback;
- user feedback;
- environment/version;
- conflicts;
- security signals.

Один успех не делает capability глобально validated.

## 7.12. Этап 12: Evolution

По evidence Denet может:

- изменить binding;
- обновить version;
- создать patch;
- выделить specialization;
- объединить дублирующиеся instructions;
- понизить приоритет;
- выключить capability;
- продвинуть project candidate;
- удалить неиспользуемую capability;
- предложить альтернативу.

## 7.13. Этап 13: Deprecation и removal

Removal:

- прекращает новые activations;
- отменяет или завершает runs по policy;
- отзывает credentials/bindings;
- удаляет package/cache по выбору;
- не удаляет созданные artifacts;
- сохраняет historical receipts;
- оставляет tombstone и replacement relation;
- проверяет, какие проекты потеряют requirement.

---

# 8. Ownership, версии, fork и merge

## 8.1. User-owned

Пользователь создал или явно импортировал capability.

Правила:

- Denet не меняет содержимое автоматически;
- update показывается как proposal;
- автоматическая адаптация создаёт fork;
- пользователь может передать ownership Denet;
- manual overwrite всегда возможен с diff/backup.

## 8.2. Denet-managed

Создано Denet или явно передано под управление.

Denet может:

- patch;
- version;
- rollback;
- test;
- deprecate;
- promote;

в пределах пользовательской policy.

## 8.3. Provider-managed

Plugin/extension/model endpoint управляется provider marketplace или cloud platform.

Denet:

- хранит installed version/channel;
- наблюдает update;
- фиксирует component/effect diff;
- повторно probes изменённые facets;
- не обещает локальный merge, если provider не поддерживает.

## 8.4. Project-shared

Находится в portable project tree/repository.

- Git/history может быть authority для файла;
- проектные изменения не продвигаются глобально автоматически;
- global collection может подключить read-only reference или fork;
- конфликт с upstream решается как обычный project merge плюс capability reconciliation.

## 8.5. Three-way update

Если upstream обновился, а local copy изменена:

```text
base version
+ upstream version
+ local version
→ classify changes
→ auto-merge only non-conflicting declarative parts
→ semantic review for instructions
→ no automatic executable merge
→ conflict/fork when uncertain
```

## 8.6. Capability fork

Fork создаётся, когда:

- user хочет сохранить локальные изменения;
- upstream incompatible;
- project нужна специализация;
- лицензия позволяет;
- delta полезен, но не подходит upstream package;
- Denet не владеет оригиналом.

Fork хранит lineage и update relation.

## 8.7. Rollback

Rollback должен вернуть:

- artifact version;
- bindings;
- provider config;
- associated skill index;
- measured utility pointer;

но не удалять outcomes, полученные новой версией.

---

# 9. Discovery, comparison и Capability Curator

## 9.1. Curator — логическая функция, не постоянный агент

Capability Curator вызывается эпизодически. Реализация может быть:

- deterministic scanner;
- lightweight model;
- основной проектный агент;
- отдельный bounded analysis Run;
- сочетание.

Он не живёт постоянно и не читает интернет без причины.

## 9.2. Когда запускать semantic analysis

- auto-discovered candidate прошёл cheap filters;
- найдено несколько близких capabilities;
- candidate содержит сложные natural-language instructions;
- нужно извлечь delta;
- update существенно меняет смысл;
- проектный опыт предлагается превратить в skill;
- user запросил сравнение.

## 9.3. Дубликаты и near-duplicates

Denet строит candidate neighborhood по:

- names/aliases;
- embeddings descriptions;
- required tools;
- outputs;
- source repository;
- file hashes;
- procedure steps;
- measured task classes.

Затем модель сравнивает только top candidates, а не всю Collection.

## 9.4. Canonical capability

«Основной» skill — это не всегда один global winner. Canonical может быть:

- global default;
- project-specific preferred;
- provider-specific preferred;
- platform-specific preferred;
- version-specific preferred.

## 9.5. Извлечение уникального преимущества

Пример:

- Skill A лучше и проверен;
- Skill B хуже в целом, но содержит хороший verification step;
- Denet не устанавливает B как default;
- создаёт Delta Proposal на добавление verification step в fork A;
- проверяет A и A+delta;
- при выигрыше принимает новую версию;
- source B остаётся в provenance.

Для scripts/code delta требует sandbox/test и license compatibility.

## 9.6. Срок жизни candidates

Candidate Pool не должен расти бесконечно.

Candidates могут:

- expire;
- быть архивированы;
- повторно появиться при новом need;
- сохранять bookmark без полного package;
- удаляться при исчезновении source;
- оставаться pinned пользователем.

---

# 10. Project Capability Assembly

## 10.1. Создание или импорт проекта

Denet анализирует:

- цель проекта;
- типы expected work;
- repository/files;
- languages/frameworks;
- Effective Instruction Set;
- portable project memory;
- declared capability requirements;
- доступные providers/devices;
- privacy/locality requirements;
- user-selected execution profile.

На выходе формируется минимальный Project Capability Set.

## 10.2. Категории рекомендаций

### Required

Без capability проект не может нормально работать.

### Recommended

Вероятно полезно, но не обязательно.

### On-demand

Не активировать до соответствующей задачи.

### Fallback

Использовать, если preferred capability недоступна.

### Experimental

Только ограниченный test/branch.

### Forbidden/Incompatible

Не применять в этом проекте.

## 10.3. Минимальный набор раньше богатого

При создании проекта Denet не прикрепляет десятки generic skills «на всякий случай».

Default:

- provider/runtime, выбранный пользователем;
- обязательные project tools;
- один-два реально релевантных skills;
- project-native instructions;
- fallback только для критических функций.

Остальное discoverable/on-demand.

## 10.4. Ручное управление пользователем

Пользователь может в любой момент:

- прикрепить skill/tool/MCP/connector;
- снять attachment;
- pin;
- запретить;
- выбрать preferred implementation;
- указать fallback;
- ограничить одним Run;
- заменить model/runtime;
- запретить автоматический discovery.

Capability Fabric хранит смысл действия, UI позже определит кнопку.

## 10.5. Запрос capability агентом

Project agent может запросить capability:

```text
need description
→ Registry resolution
→ existing project/on-demand candidate
→ if found, bind for Run
→ if not found, discovery according to policy
→ Trust handoff for invocation/install
```

Если capability нужна один раз, она не становится project-pinned автоматически.

## 10.6. Повторное использование

Если одна и та же capability полезна в нескольких Runs:

- Denet повышает её project prior;
- может предложить pin;
- может автоматически pin в Project-Autonomous profile;
- сохраняет version compatibility;
- не делает её глобальной только из-за одного проекта.

## 10.7. Project-local creation

Project agent может создать:

- skill;
- helper script;
- prompt template;
- connector config;
- capability requirement;

если это легче и полезнее, чем поиск внешнего решения.

Default — хранить в project capability space и portable project pack.

## 10.8. Global promotion

Project capability продвигается в global Collection, если:

- полезна за пределами одного project-specific context;
- не содержит private/project secrets;
- прошла несколько representative uses или пользователь явно приказал;
- dependencies переносимы;
- ownership/license позволяют;
- conflicts с global capabilities разрешены;
- есть описание applicability и limitations.

## 10.9. Project portability

Portable pack может хранить:

- requirements;
- source/revision references;
- project-local skills;
- optional MCP declarations;
- provider-neutral capability IDs;
- preferred/fallback relations;
- minimum versions;
- notes о measured compatibility.

Он не содержит credentials и не гарантирует, что другой пользователь имеет те же subscriptions.

## 10.10. Reconciliation после clone/import

Receiving Denet показывает логически:

- available;
- missing;
- replaceable;
- requires installation;
- requires account binding;
- incompatible;
- blocked by Trust;
- local alternative.

Проект может работать в degraded mode, если optional capabilities отсутствуют.

---

# 11. Обучение Capability Fabric на реальной работе

## 11.1. Что считать evidence

- end-task success;
- tests/verifications;
- user correction;
- reviewer finding;
- tool failure;
- token/latency overhead;
- repeated manual sequence;
- fallback success;
- environment/version;
- user preference.

## 11.2. Public benchmark и локальный опыт

Public benchmark задаёт initial prior. Реальный опыт Denet постепенно уточняет его для:

- конкретного пользователя;
- проекта;
- model/runtime;
- hardware;
- task class.

Один локальный success не отменяет широкий benchmark; множество релевантных outcomes могут перевесить общий prior.

## 11.3. Когда создавать skill

Candidate skill возникает, если:

- процедура повторилась;
- было 5+ содержательных tool steps и найден устойчивый путь;
- агент преодолел характерный dead end;
- пользователь исправил подход;
- есть проверяемый outcome;
- процедура достаточно общая для повторения.

Hermes использует похожие triggers для agent-managed skills и отделяет короткую memory от длинной procedural memory. Denet принимает паттерн, но делает project-local candidate default и не обязан свободно переписывать user-owned library. [[S08]]

## 11.4. Что не превращать в skill

- единичный факт;
- краткое пользовательское предпочтение;
- одноразовый workaround;
- неуспешную догадку;
- скрытый chain-of-thought;
- инструкции, уже принадлежащие проекту;
- маленькую deterministic функцию, которую лучше оформить tool/script;
- процесс с внешним effect, если он требует formal automation contract.

## 11.5. Validation proportional to risk

Instruction-only project skill можно попробовать в следующей похожей задаче.

Skill с scripts/hooks/secrets требует более строгого Trust review и sandbox.

Global skill требует более сильного evidence, чем project-local.

## 11.6. Paired evaluation

При наличии хорошего replay:

- baseline без skill;
- current skill;
- candidate version;
- quality/cost comparison.

Если replay дорог или недоступен, применяется canary на реальной низкорисковой задаче.

## 11.7. Prune и simplify

Self-improvement обязан уметь:

- удалить устаревший skill;
- разделить слишком широкий;
- слить точные дубли;
- оставить specialization отдельно;
- сократить токены;
- убрать инструкции, которые сильная модель и так выполняет;
- понизить project pin до on-demand.

SkillFoundry показывает практическую пользу циклов expand/repair/merge/prune для domain skill libraries; Denet использует эту идею выборочно, а не как постоянный глобальный процесс. [[S09]]

---
# 12. Providers, Connections, Models и Agent Runtimes

## 12.1. Четыре разные сущности

### Provider Definition

Описывает организацию/экосистему и доступные surfaces.

### Provider Connection

Конкретное подключение:

- подписка;
- API project/key;
- cloud deployment;
- aggregator account;
- local runtime;
- organization workspace.

У одного provider может быть несколько connections с разными правами, регионами, billing и model catalog.

### Model Endpoint

Конкретный model ID/deployment с датированными observations.

### Agent Runtime

Среда, которая предоставляет больше, чем model call:

- sessions;
- tool loop;
- subagents;
- hooks;
- worktrees;
- provider memory/context;
- background jobs;
- permissions;
- native plugins.

Codex runtime и OpenAI API/Agents SDK — разные surfaces. Claude Code и Anthropic API — разные surfaces. Gemini CLI и Gemini API/ADK — разные surfaces.

## 12.2. Connection lifecycle

```text
provider discovered
→ connection proposed
→ authentication configured
→ provider inventory fetched
→ safe probes
→ available
→ health/usage monitoring
→ reauth/update
→ disabled/disconnected
```

Manual connection пользователя регистрируется немедленно. Denet не обязан сначала оценивать «лучший ли это provider».

## 12.3. Multiple accounts и deployments

Connection имеет identity/account/workspace/region. Denet не смешивает:

- личную и рабочую subscription;
- два API projects;
- direct Anthropic и Bedrock deployment;
- direct Google и Vertex deployment;
- local and remote endpoint;
- разные GitHub organizations.

Project binding может выбрать конкретную Connection.

## 12.4. Live model discovery

Model catalogue обновляется через:

- официальный list-models endpoint;
- provider runtime inventory;
- cloud deployment listing;
- local runtime listing;
- official docs/changelog;
- user-defined endpoint.

Запись модели хранит `observed_at` и `source`. Статический документ не считается authority для текущей доступности.

## 12.5. Capability probes

Probe Suite проверяет минимальными безопасными вызовами:

- basic text;
- streaming;
- tool calling;
- parallel tools;
- structured output;
- image/audio input;
- reasoning control;
- state continuation;
- cancellation;
- context limit observation;
- provider-native tools;
- usage reporting.

Probe не запускается перед каждым использованием. Повтор нужен при:

- новой модели;
- changed version;
- provider update;
- длительной давности;
- observed failure;
- смене deployment.

## 12.6. Native controls

Capability Fabric хранит provider-native controls без ложной унификации:

- reasoning effort;
- thinking budget;
- service tier;
- prompt caching;
- web/X search;
- computer-use environment;
- safety settings;
- data region;
- background mode;
- tool search;
- voice;
- provider session options.

Универсальная policy может выражать намерение `fast/balanced/deep`, но mapping прозрачно указывает, является ли функция native, approximated или unavailable.

## 12.7. Direct-chat model lock

В прямом пользовательском чате выбранная модель не меняется молча.

При недоступности:

- ждать;
- повторить;
- предложить fallback;
- применить заранее выбранную fallback policy;
- продолжить локально только после разрешённого правила.

Internal agents могут маршрутизироваться автоматически.

## 12.8. Model suitability learning

Для task classes Denet накапливает:

- success rate;
- user/reviewer satisfaction;
- latency;
- token/cost;
- tool reliability;
- context failures;
- provider outages;
- model-specific strengths/weaknesses.

Routing сначала использует deterministic eligibility filters, затем score. Отдельный router-model не нужен для обычных случаев.

Пример:

```text
eligible models
→ user/provider/privacy constraints
→ native feature match
→ health/quota
→ local measured quality
→ cost/latency preference
→ select
```

## 12.9. Retirements и migration

При исчезновении model ID:

- existing configuration не переписывается без записи;
- model получает `retired/unavailable` observation;
- Denet предлагает successor candidates;
- reproducible projects могут pin old endpoint, если доступен;
- live session migration учитывает provider continuation limits;
- Task/Run state остаётся в Denet независимо от provider session.

## 12.10. Agent Runtime Adapter

Adapter сохраняет:

- provider session handle;
- native tools;
- hooks/events;
- subagent semantics;
- continuation/resume;
- usage;
- native permissions;
- native artifacts;
- errors/cancellation;
- provider plugin identity.

Denet normalization не должна превращать first-class runtime в простой `chat.completions` loop.

## 12.11. Subscription и API

Subscription access используется только официальными supported surfaces. Нельзя предполагать, что consumer subscription даёт право тайно использовать private API.

Usage хранится раздельно:

- subscription allowance;
- premium request count;
- API spend;
- cloud quota;
- local compute.

---

# 13. Локальные модели и serving runtimes

## 13.1. Model Artifact

```yaml
model_artifact:
  artifact_id: id
  source_uri: uri
  revision_hash: hash
  family: text
  architecture: text
  tokenizer_ref: ref
  format: safetensors | gguf | onnx | openvino | mlx | engine | other
  quantization: optional
  license: text
  model_card_ref: optional
  custom_code_required: boolean
  declared_modalities: []
  files: []
```

## 13.2. Manual import

Если пользователь указывает локальный файл или repository:

- artifact немедленно регистрируется;
- hash и metadata вычисляются;
- Denet не требует benchmark до добавления;
- совместимость показывается best-effort;
- custom executable model code не запускается автоматически;
- пользователь может выбрать runtime вручную.

## 13.3. Source acquisition

Источники:

- Hugging Face Hub;
- Ollama Library;
- LM Studio discovery;
- ModelScope;
- vendor repository;
- local file;
- organization registry;
- converted artifact.

Revision/hash обязателен для воспроизводимости.

## 13.4. Safe loader policy

Предпочтение отдаётся data-only artifacts и стандартным loaders.

Если модель требует custom Python/code:

- это отдельная executable capability;
- origin и revision фиксируются;
- Trust решает execution;
- возможен sandbox/isolated environment;
- Denet не включает `trust_remote_code`-подобное поведение молча.

Hugging Face Hub поддерживает custom model code, что полезно, но делает source trust частью capability lifecycle. [[S10]]

## 13.5. Hardware compatibility

Hardware Profiler учитывает:

- CPU/GPU/NPU;
- VRAM/RAM;
- architecture;
- supported dtypes/quantizations;
- OS;
- driver/runtime versions;
- desired context;
- concurrency;
- latency/power limits.

Compatibility определяется комбинацией:

```text
model artifact + runtime + hardware + configuration
```

а не только названием модели.

## 13.6. Runtime selection

Примерная policy:

- Ollama/LM Studio — удобный desktop/local default;
- llama.cpp — CPU/edge/GGUF и максимальная переносимость;
- vLLM/SGLang — серверный throughput;
- MLX-LM — Apple Silicon;
- OpenVINO — Intel;
- TensorRT-LLM/NIM — NVIDIA specialization;
- TGI/KServe/BentoML/Modal/RunPod — managed deployment needs.

Пользователь может pin runtime независимо от recommendation.

## 13.7. Install, load, warm и evict

Отдельные состояния:

- artifact downloaded;
- runtime installed;
- model compatible;
- model loaded;
- warm;
- evicted;
- unavailable.

Denet не держит все модели загруженными. Warm policy зависит от частоты, startup time, VRAM и active Runs.

## 13.8. Quantization variants

Variants связываются relation `same_model_variant`.

Выбор учитывает:

- memory fit;
- quality;
- speed;
- context;
- tool/vision support;
- runtime support.

Variant может быть project-specific preferred.

## 13.9. Local evaluation

Первое использование не требует полного benchmark.

Evaluation запускается:

- при выборе default локальной модели;
- сравнении variants;
- заметном failure;
- новой важной task class;
- пользовательском запросе;
- автоматическом routing policy update.

## 13.10. Update и removal

- новые revision не заменяют pinned artifact молча;
- можно хранить несколько revisions;
- conversion artifact сохраняет lineage;
- removal проверяет active bindings и loaded instances;
- cache можно удалить отдельно от metadata/history.

---

# 14. Skills Fabric

## 14.1. Что такое skill

Skill — переносимый пакет процедурного контекста, предназначенный для повторно применимого класса задач. Он может содержать:

- `SKILL.md`;
- references;
- templates;
- assets;
- scripts;
- verification instructions;
- optional provider metadata.

Базовая совместимая форма следует открытому Agent Skills format. [[S07]]

Skill не является автоматически:

- agent runtime;
- tool permission;
- workflow;
- memory fact;
- trusted code.

## 14.2. Источники skills

- ручная папка/URL/repository пользователя;
- project repository;
- OpenAI/Codex skills/plugins;
- Claude plugin marketplace;
- Gemini CLI extensions;
- GitHub/Copilot-compatible locations;
- OpenClaw/ClawHub;
- Hermes Skills Hub;
- skills.sh и well-known endpoints;
- private organization catalogue;
- Denet-generated candidate;
- external shared skill directory.

Единого исчерпывающего и одновременно доверенного мирового registry нет.

## 14.3. Skill source identity

Сохраняются:

- source type;
- repository/path/URL;
- publisher/namespace;
- revision/hash;
- format/version;
- license;
- upstream channel;
- installed/forked lineage.

## 14.4. Ручное добавление

```text
user selects source
→ register and add to Collection immediately
→ mark user-owned/user-selected
→ parse metadata
→ index name/description
→ do not require utility comparison
→ executable use still follows Trust
```

Если пользователь явно говорит «доверять и разрешить в этом project scope», соответствующий Trust grant может убрать последующие prompts в пределах scope.

## 14.5. Автоматическое обнаружение

Sources:

- project scan;
- provider marketplace sync;
- internet/registry search;
- task failure;
- missing capability;
- background review;
- upstream reference;
- agent discovery.

Flow:

```text
candidate
→ static parse
→ source/revision/license
→ executable surface scan
→ relevance triage
→ near-duplicate retrieval
→ semantic comparator if needed
→ disposition
```

Модель не вызывается, если candidate явно нерелевантен, несовместим или exact duplicate.

## 14.6. Skill Comparator

Сравнивает:

- когда использовать;
- когда не использовать;
- procedure;
- tool dependencies;
- project/framework versions;
- scripts/hooks;
- verification;
- failure modes;
- token footprint;
- outcomes;
- maintenance freshness.

Результат — relation, а не один общий score.

## 14.7. Skill disposition examples

### Exact duplicate

Сохранить один canonical installation и aliases/sources.

### Better substitute

Предложить заменить default; старый остаётся pinned для legacy projects при необходимости.

### Specialization

Оставить оба; selector выбирает по applicability.

### Complement

Не сливать автоматически; можно создать bundle или composite guidance, если часто используются вместе.

### Worse overall, unique delta

Создать Capability Delta Proposal и patch/fork основного skill.

### Conflicting skill

Не загружать одновременно без explicit resolution.

## 14.8. Security analysis

Для auto-discovered skills:

- instruction-only skill: prompt/data-flow inspection;
- scripts/hooks: static scan, dependencies, obfuscation, network/secret usage;
- direct install commands: flag;
- hidden/undocumented capabilities: flag;
- marketplace verification: provenance signal, не trust authority;
- restricted test только после Trust policy.

Масштабные исследования community skill ecosystems обнаружили как высокий уровень уязвимых packages, так и подтверждённые malicious skills; scripts повышают attack surface. Поэтому автоматический discovery требует triage, но пользовательский manual import не блокируется utility gate. [[S02]] [[S03]]

## 14.9. Progressive disclosure

В agent context сначала попадают:

- name;
- description;
- applicability;
- source/scope;
- file handle.

Полный `SKILL.md` загружается при выборе. References/assets — ещё позже.

OpenAI, Agent Skills и Hermes используют этот pattern для снижения context overhead. [[S06]] [[S07]] [[S08]]

## 14.10. Skill selection for Run

Selector учитывает:

- explicit user invocation;
- project pin;
- task match;
- provider/model compatibility;
- required tools;
- framework/version;
- conflicts;
- measured gain;
- token overhead;
- Trust/availability;
- existing native capability.

Explicit user invocation имеет высокий приоритет, если capability доступна и разрешена.

## 14.11. Skill stacking и bundles

Несколько skills можно загрузить вместе, если:

- их scopes не конфликтуют;
- общая длина разумна;
- каждый приносит различимую пользу;
- order/priority определены.

Повторяющаяся комбинация может стать lightweight bundle, а не новым monolithic skill.

Hermes реализует bundles и conditional activation; Denet принимает идею, но выбирает bundle только при повторной совместной пользе. [[S08]]

## 14.12. Conditional activation

Skill может иметь:

- requires tools/toolsets;
- fallback-for tools;
- platform constraints;
- provider constraints;
- project/framework constraints;
- version ranges;
- local/cloud preference.

Fallback skill скрывается, когда лучший native capability доступен.

## 14.13. Project attachment

При создании проекта:

- selected/pinned skills пользователя сохраняются;
- Denet предлагает минимальный набор;
- project repository skills импортируются project-local;
- generic skills не активируются без need;
- agent может запросить on-demand skill;
- repeated successful use может предложить pin.

## 14.14. Project-local skill creation

Flow:

```text
repeatable project experience
→ classify as skill candidate
→ author SKILL.md + applicability + verification
→ project-local installation
→ optional next-use canary
→ validate/degrade/reject
```

Не требуется глобальный benchmark для низкорискового project-local instruction skill.

## 14.15. Promotion to global

Требует:

- portability;
- absence of private data;
- clear applicability;
- stable dependencies;
- repeated benefit or explicit user command;
- no unresolved conflict;
- ownership/license compatibility.

## 14.16. Update semantics

### Upstream pristine

Можно обновить по policy после diff/probe.

### User-modified

Не перезаписывать. Предложить:

- keep;
- three-way merge;
- fork;
- reset to upstream;
- ignore update.

### Denet-managed

Patch/replay/rollback возможны автоматически по profile.

### Provider-managed

Следовать marketplace/channel semantics, сохраняя observed version.

Hermes, например, сохраняет user-modified bundled skills при sync/remove и поддерживает inspect/install/check/update/audit/reset; этот практический pattern полезен для ownership semantics Denet. [[S08]]

## 14.17. Skill mutation policy

- `patch` предпочтительнее полной перезаписи;
- user-owned — только proposal/fork;
- Denet-managed project skill — auto patch по policy;
- executable changes требуют усиленной проверки;
- любой accepted change создаёт version и rollback point.

## 14.18. Skill quality lifecycle

```text
unknown
→ project candidate
→ project validated
→ cross-project candidate
→ global validated
↔ degraded
→ deprecated/rejected
```

Это utility facet, а не обязательная installation state machine.

## 14.19. Skill deletion

Удаление:

- снимает bindings;
- проверяет bundles;
- сохраняет historical versions/receipts;
- не удаляет artifacts, созданные skill;
- user-owned deletion требует explicit user action;
- upstream reference может остаться как bookmark/tombstone.

---

# 15. MCP Fabric

## 15.1. MCP как transport и package boundary

MCP подключает:

- tools;
- resources;
- prompts;
- optional apps/UI.

Server identity, Connection и отдельные components не смешиваются.

## 15.2. Sources

- Official MCP Registry;
- provider marketplace/plugin;
- private organization registry;
- project configuration;
- Git repository/package;
- community catalogue;
- manual command/URL;
- existing client config.

Official Registry — preferred public discovery source, но не trust authority. [[S11]]

## 15.3. Manual MCP add

Пользователь может немедленно зарегистрировать:

- stdio command;
- remote URL;
- package;
- repository;
- OAuth metadata;
- provider-native plugin.

Connection добавляется без utility review. Запуск/auth/effects следуют Trust policy.

## 15.4. Automatic discovery

Project-declared MCP не подключается автоматически к credentials.

Flow:

```text
manifest discovered
→ identity/version normalization
→ enumerate tools/resources/prompts without broad secrets
→ compare native/installed alternatives
→ candidate disposition
→ restricted connect/probe if policy allows
```

## 15.5. Components enabled independently

Один server может иметь:

- полезные resources;
- дублирующие tools;
- нежелательные prompts;
- experimental app.

Denet может включить компоненты частично. Установка server не означает exposure всех tools.

## 15.6. Native vs MCP resolution

При одной функции сравниваются:

1. provider-native connector/tool;
2. Denet native connector;
3. MCP;
4. CLI/SDK;
5. computer-use.

Критерии:

- semantics/coverage;
- latency;
- stability;
- auth;
- context overhead;
- security boundary;
- local/offline;
- measured outcome;
- portability.

MCP выбирается, если он действительно лучший или самый переносимый, а не потому, что это единый протокол.

## 15.7. MCP Comparator

Сравнивает servers по:

- tool coverage;
- schemas;
- resource quality;
- auth/scopes;
- update cadence;
- package provenance;
- local/remote mode;
- health;
- latency;
- error semantics;
- idempotency;
- provider integration;
- measured use.

## 15.8. Tool identity и aliases

Tool ID включает server identity/version. Два `search` tools не считаются одним tool.

Denet может создать semantic capability relation между ними и выбрать preferred implementation.

## 15.9. Lazy tool exposure

Agent получает:

- relevant server summaries;
- tool search handle;
- ограниченный tool subset;

а не schemas всех MCP servers.

OpenAI tool search и исследования tool retrieval подтверждают пользу динамического раскрытия больших tool libraries. [[S05]] [[S12]]

## 15.10. Project и Run binding

MCP может быть:

- global available;
- project-attached;
- run-only;
- device-specific;
- provider-managed;
- forbidden.

Project portability сохраняет declaration, но не credentials.

## 15.11. Authentication и secrets

Capability Fabric хранит auth requirements и account binding; Trust/Secret Broker выдаёт credentials.

MCP security best practices требуют предотвращать token passthrough, confused deputy, SSRF, session hijacking и чрезмерные scopes. Denet adapter должен сохранять эти boundary. [[S13]]

## 15.12. Updates

При изменении server version/tools list:

- diff components;
- сохранить removed/renamed tools;
- повторно проверить changed schemas/effects/auth;
- не ломать pinned workflow молча;
- обновить aliases;
- re-probe затронутые capabilities;
- Trust re-review при scope/effect expansion.

## 15.13. Failure и fallback

Если MCP недоступен:

- retry/reconnect;
- native connector;
- alternative MCP;
- CLI/API;
- computer-use;
- partial result;
- ask user.

Не повторять unknown external effect без reconciliation.

## 15.14. Removal

- disconnect;
- revoke credentials;
- remove project/run bindings;
- preserve receipts;
- warn affected projects;
- retain source bookmark optionally;
- do not delete remote data.

---

# 16. Plugins, Extensions и Apps

## 16.1. Package, не единая capability

Provider plugin может содержать:

- skills;
- agents;
- hooks;
- MCP servers;
- commands/prompts;
- LSP;
- UI apps;
- connectors;
- configuration.

Denet регистрирует package и components отдельно.

## 16.2. Native semantics

Claude marketplace, OpenAI/Codex plugins, Gemini extensions и IDE extensions имеют разные manifests, update channels и scopes. Denet не flatten-ит их в один урезанный format.

Claude plugin marketplaces, например, поддерживают central discovery, versions/updates и packages с skills, agents, hooks, MCP и LSP. Gemini extensions могут объединять prompts, MCP, commands, hooks, subagents и skills. [[S14]] [[S15]]

## 16.3. Manual install

User install:

- package регистрируется немедленно;
- provider-native install flow используется;
- components inventory сохраняется;
- user может включать package целиком или отдельные components, если provider позволяет;
- utility comparison не обязателен;
- executable/effect permissions остаются у Trust.

## 16.4. Automatic discovery

Auto-found plugin проходит:

- official/community/source identity;
- package contents;
- dependency/install scripts;
- requested permissions/scopes;
- overlap with installed components;
- project relevance;
- update channel;
- disposition.

## 16.5. Partial enable

Если package содержит полезный skill и нежелательный hook, Denet должен по возможности:

- установить package restricted;
- включить только skill;
- оставить hook disabled;
- показать provider limitation, если partial enable невозможен.

## 16.6. Dependencies и lock

Сохраняются:

- package version;
- component versions;
- dependencies;
- source revision;
- update channel;
- lock state;
- provider compatibility.

Project может pin plugin version.

## 16.7. Updates

Update diff рассматривается по components:

- instruction-only change;
- new skill;
- hook/script change;
- MCP endpoint change;
- OAuth scope change;
- binary/dependency change;
- removed component.

Security-sensitive changes возвращаются в Trust review.

## 16.8. Conflicts

- command/skill name collision;
- duplicate MCP;
- incompatible hooks;
- competing LSP;
- provider version mismatch;
- conflicting project instructions.

Conflict может решаться priority, disable component, project-local override или separate provider profile.

## 16.9. Removal

Provider-native uninstall сохраняет:

- history;
- user-created artifacts;
- config export по выбору;
- project impact report;
- tombstone/version.

---

# 17. Connectors and Account Bindings

## 17.1. Definition vs Account Binding

Connector Definition описывает service capability. Account Binding связывает конкретный account/workspace/tenant.

Пример:

- `telegram.connector`;
- account binding конкретного пользователя/bot;
- project binding только к одному chat/thread.

## 17.2. Источники connector

- Denet-native;
- provider-managed connector;
- MCP;
- official SDK/API;
- plugin/extension;
- user-supplied adapter;
- computer-use fallback.

## 17.3. Manual connect

Пользователь выбирает service/account:

- connection создаётся;
- scopes объясняются Trust/UI;
- account identity сохраняется;
- initial sync policy определяется;
- никакой utility model review не нужен.

## 17.4. Project-declared connector

Project manifest может требовать GitHub/Notion/Slack, но не может автоматически авторизовать account.

Denet:

- показывает requirement;
- находит существующий binding;
- предлагает нужный scope;
- разрешает выбрать account;
- сохраняет project resource scope.

## 17.5. Multiple accounts

Connector binding хранит:

- account/user/org/workspace;
- environment/tenant;
- scopes;
- data region;
- credential handle;
- project bindings;
- webhook/polling state;
- health/reauth.

Agent не должен выбирать account только по похожему имени, если effect consequential.

## 17.6. Read и write разделены

Connector может иметь:

- read metadata;
- read content;
- draft;
- write/update;
- send/publish;
- admin.

Capability availability и actual grants различаются.

## 17.7. Sync mode

- webhook;
- polling;
- on-demand;
- incremental sync;
- event stream;
- no sync, direct query.

Server document позже определит durable runtime. Здесь фиксируется abstract behavior и authoritative source.

## 17.8. Connector resolution

При нескольких implementations:

- native connector предпочтителен при лучшей semantics;
- provider-managed может быть удобнее внутри provider runtime;
- MCP полезен для portability;
- CLI/API может быть проще;
- computer-use — последний слой.

## 17.9. Health и reauthorization

Состояния:

- healthy;
- auth-expiring;
- scope-insufficient;
- rate-limited;
- webhook-broken;
- partial;
- offline;
- revoked.

Reauth не должен менять account identity незаметно.

## 17.10. Disconnect

- stop new calls/sync;
- revoke credentials;
- unregister webhooks;
- preserve imported memory according to retention;
- preserve external effect receipts;
- show affected projects;
- optionally keep Definition without Binding.

---

# 18. Computer-Use and Browser Backend Fabric

## 18.1. Capability ladder

Denet выбирает самый структурированный достаточный слой:

1. native API/connector;
2. SDK/CLI;
3. browser DOM/DevTools/accessibility;
4. provider-native computer use;
5. adaptive browser agent;
6. local visual/accessibility desktop agent;
7. собственный glue/backend.

Visual clicking не является default, если доступен точный interface.

## 18.2. Backend profile

```yaml
computer_use_backend:
  backend_id: id
  surface: browser | desktop | mobile | remote_vm
  control_modes: [dom, accessibility, devtools, pixels, hybrid]
  supported_os_apps: []
  session_persistence: typed
  screenshot_support: boolean
  takeover_support: boolean
  structured_state_support: boolean
  file_transfer: typed
  latency_class: typed
  provider_or_local: ref
  measured_outcomes: []
```

## 18.3. Selection

Учитываются:

- target app/site;
- task type;
- authentication state;
- available structured interface;
- reliability history;
- latency/cost;
- privacy/locality;
- user control/takeover;
- session continuity;
- Trust constraints.

## 18.4. Hybrid control

Один Run может:

- использовать DOM для navigation;
- DevTools для network/performance;
- screenshot model для visual verification;
- accessibility для native dialog;
- user takeover на critical step.

Это лучше, чем выбор одного backend навсегда.

## 18.5. Project/device binding

Backend может быть доступен:

- только на конкретном device;
- только в VM;
- project browser profile;
- authenticated user session;
- provider cloud environment.

Run Capability Plan связывает backend с device/session.

## 18.6. Outcome learning

Measured per:

- app/site/version;
- task class;
- backend version;
- model;
- device;
- structured vs visual mode.

Failure одного сайта не делает backend глобально плохим.

## 18.7. Fallback

```text
structured backend fails
→ refresh/recover session
→ alternative structured backend
→ provider-native visual
→ local visual
→ user takeover/partial result
```

Fallback не должен повторять неизвестный external effect.

## 18.8. Ready implementations

Первая версия Denet должна поддерживать готовые решения:

- Playwright CLI/skills и/или Playwright MCP;
- Chrome DevTools MCP;
- browser-use или Skyvern для adaptive web automation;
- provider-native computer-use;
- один local GUI/accessibility backend при необходимости.

Playwright MCP использует accessibility snapshots; Chrome DevTools MCP предоставляет DevTools-oriented debugging/performance; browser-use и Skyvern предлагают adaptive browser automation. Поддержка нескольких backends оправдана, потому что они сильны в разных задачах. [[S16]] [[S17]] [[S18]] [[S19]]

## 18.9. Когда писать своё

Не писать foundation vision-action model.

Писать собственный слой только для:

- unified session contract;
- backend resolution;
- device routing;
- takeover;
- screenshots/receipts;
- Trust integration;
- unsupported application glue;
- cross-backend state transfer.

---

# 19. Speech, Voice, Vision и Media Backends

## 19.1. Граница с Voice Fabric

Capability Fabric описывает backends и их lifecycle. Voice Fabric позже определит разговор, turn-taking, interruption и orchestration.

## 19.2. Capability classes

- streaming ASR;
- batch transcription;
- diarization;
- speaker embedding;
- wake word;
- VAD;
- TTS;
- realtime speech-to-speech;
- vision understanding;
- OCR/document AI;
- image generation/editing;
- video generation;
- multimodal embeddings;
- media processing.

## 19.3. Backend profile

- languages;
- streaming/batch;
- latency;
- quality history;
- local/cloud;
- hardware;
- cost/limits;
- voice options;
- timestamps/diarization;
- privacy/data region;
- file formats;
- provider-native session integration.

## 19.4. Manual selection и fallback

Пользователь может pin voice/model backend. Automatic fallback допускается по policy:

- cloud ASR → local faster-whisper;
- provider TTS → local Piper/Kokoro;
- realtime → batch/degraded;
- multimodal model → OCR + text model.

Изменение голоса в активном пользовательском разговоре не должно происходить молча, если заметно меняет опыт.

## 19.5. Local media models

Применяется тот же Model Artifact lifecycle: source, hash, license, runtime, hardware, load/evict, version.

## 19.6. Quality learning

Отдельно по:

- языку;
- микрофону;
- noise conditions;
- speaker;
- document type;
- image style;
- latency class.

---

# 20. Web, Search, Retrieval, Code и Shell Capabilities

## 20.1. Web/search

Backends различаются:

- provider built-in search;
- search API;
- browser research;
- domain-specific index;
- social/X search;
- local knowledge base.

Selection учитывает freshness, citations, source access, cost, privacy и task.

## 20.2. Retrieval

File search/RAG/embeddings/rerank являются capabilities, но Memory Fabric владеет memory retrieval policy. Capability Fabric лишь предоставляет backends и measured properties.

## 20.3. Code execution

Варианты:

- provider code interpreter;
- local shell;
- container;
- remote dev environment;
- notebook;
- cloud job.

Trust определяет execution. Agentic определяет стратегию. Capability Fabric выбирает compatible backend.

## 20.4. Shell/terminal

Backend profile включает:

- local/container/SSH/WSL/remote;
- cwd/worktree;
- OS/shell;
- timeout;
- file transfer;
- environment/secret brokering;
- output limits;
- cancellation.

---

# 21. Health, Quota, Fallback и Degraded Operation

## 21.1. Health dimensions

- authentication;
- endpoint reachability;
- model/tool availability;
- latency;
- error rate;
- quota/rate limit;
- version compatibility;
- local hardware fit;
- loaded model state;
- connector sync;
- marketplace/update availability.

## 21.2. Health sources

- active probe;
- provider status/API;
- recent Run outcomes;
- local process/runtime;
- user report;
- adapter errors.

## 21.3. Capability Resolution cache

Fast path uses cached resolution keyed by:

- requirements;
- project/run;
- provider/model lock;
- health watermark;
- quota;
- device;
- policy/version;
- binding changes.

Не нужен новый model call при каждом invocation.

## 21.4. Fallback classes

### Transparent internal

Для bounded internal tasks при совместимой semantics.

### User-visible direct-chat

Требует уведомления/выбора при смене model character/provider.

### Feature fallback

Меняется backend одной capability.

### No fallback

Для reproducible, provider-specific, local-only или confidential work.

## 21.5. Quota-aware routing

Учитываются:

- subscription allowance;
- API budget;
- provider rate limits;
- local load;
- urgency;
- continuity;
- user preference.

Самый дешёвый не всегда лучший.

## 21.6. Unknown effect

Provider/tool failure после consequential call не ведёт к слепому fallback/retry. Сначала reconciliation через Trust/Server.

---

# 22. Handoffs к другим Fabric

## 22.1. Agentic Control

Agentic передаёт Capability Requirement. Fabric возвращает candidates и Run Capability Plan. Agentic не устанавливает package напрямую текстовым сообщением.

## 22.2. Trust

При регистрации/изменении передаются:

- origin/publisher/version;
- scripts/hooks/binaries;
- declared effects;
- resource/network scopes;
- secret requirements;
- auth/scopes;
- rollback/idempotency;
- update diff.

Trust решает execution/authorization.

## 22.3. Memory

Memory хранит:

- historical outcomes;
- preferences;
- project capability requirements;
- procedures/skills provenance;
- portable references;
- corrections.

Capability Registry остаётся authority для текущей availability/health/version.

## 22.4. Server

Server позже реализует:

- durable installation jobs;
- sync;
- probes;
- schedules;
- provider sessions;
- updates;
- background discovery;
- health.

Этот документ определяет desired transitions, а не конкретные queues/services.

## 22.5. UI

UI должен позволить manual add/attach/pin/fork/update/remove, но не переопределяет смысл этих операций.

---

# 23. Observability and Evaluation

## 23.1. Query trace

Для resolution сохраняется:

```text
need
→ eligible sources
→ candidates
→ filters
→ relations
→ selected backend
→ activation
→ outcome
→ fallback
```

## 23.2. Capability attribution

Итог должен позволять понять:

- какие skills реально были загружены;
- какие tools exposed;
- какой provider/runtime использован;
- какой fallback сработал;
- что дало положительный/отрицательный эффект;
- сколько стоил overhead.

## 23.3. Skills metrics

- task success delta;
- token delta;
- latency;
- selection precision;
- unused loaded skill rate;
- version mismatch incidents;
- user corrections;
- security findings;
- project/global promotion accuracy.

## 23.4. MCP/tools metrics

- tool selection accuracy;
- duplicate/redundant exposure;
- schema errors;
- auth failures;
- external effect failures;
- server health;
- native-vs-MCP outcome;
- context overhead.

## 23.5. Provider/model metrics

- end-task success;
- latency/cost;
- tool success;
- context failures;
- quota/outages;
- user satisfaction;
- fallback frequency;
- local hardware utilization.

## 23.6. Evaluation proportionality

Не нужно benchmark каждой capability до первого простого использования.

Обязательное усиление evaluation для:

- global default;
- auto-promotion;
- expensive capability;
- security-sensitive scripts/hooks;
- replacement of validated capability;
- routing policy change;
- user-reported regression.

## 23.7. Regression and rollback

Каждый auto-managed update должен иметь:

- previous version;
- diff;
- representative cases;
- rollback;
- owner;
- acceptance threshold.

---

# 24. Пошаговое внедрение без оверинжиниринга

## Phase 1 — manual-first foundation

- Registry/Collection/Project Set;
- manual providers/models/local runtimes;
- manual skill import;
- manual MCP import;
- basic connectors;
- project attach/detach/pin;
- lazy activation;
- health and fallback;
- Trust handoff.

## Phase 2 — origin-aware discovery

- project scan;
- official registry/marketplace discovery;
- Candidate Pool;
- cheap inspection;
- duplicate retrieval;
- suggest mode;
- project-local candidates.

## Phase 3 — comparison and learning

- Skill Comparator;
- Capability Relations;
- Delta Proposals;
- measured utility;
- experience-derived project skills;
- fork/update/rollback;
- project capability reconciliation.

## Phase 4 — broader ecosystem

- provider plugins/extensions;
- multiple MCP alternatives;
- connectors/accounts;
- computer-use backend selection;
- local hardware routing;
- optional global auto-curation.

## Release gates

Не переходить к auto-curation, пока:

- manual lifecycle ненадёжен;
- ownership не защищён;
- project-local/global scopes смешиваются;
- removal ломает projects;
- utility attribution отсутствует;
- Trust handoff не работает;
- automatic discovery создаёт лишний шум.

---

# 25. Отклонённые подходы

## 25.1. Один плоский список installed tools

Не отражает candidates, scope, ownership, utility и activation.

## 25.2. Автоматически устанавливать всё найденное

Создаёт supply-chain risk и мусор.

## 25.3. Обязательный AI-review ручного импорта

Нарушает пользовательский контроль. Добавление немедленное; execution отдельно.

## 25.4. Никогда не проверять manual import

Тоже неверно: дешёвая metadata extraction и Trust boundary нужны, но не должны блокировать addition.

## 25.5. Один глобальный лучший skill

Полезность контекстна; допускаются project/provider/platform specializations.

## 25.6. Сливать похожие skills автоматически

Natural-language и executable merge может изменить смысл. Нужны relation и patch/fork.

## 25.7. Skill после каждого сложного Turn

Создаёт библиотеку одноразового мусора.

## 25.8. MCP для всего

Нативные integrations часто лучше.

## 25.9. Marketplace как trust authority

Official/verified status улучшает provenance, но не выдаёт permission.

## 25.10. Все tools/skills в prompt

Дорого и ухудшает выбор.

## 25.11. Скрывать provider differences

Убирает нативные преимущества и вводит пользователя в заблуждение.

## 25.12. Полный benchmark перед первым использованием

Слишком дорого. Evaluation усиливается по impact.

## 25.13. Своя GUI foundation model в первой версии

Готовые backends уже покрывают основу; своё нужно только как glue.

## 25.14. Один local runtime на всё

Hardware и artifact formats различаются.

## 25.15. Непрерывный Curator-agent

Сжигает токены. Curator запускается по событию/need.

---

# 26. Сквозные сценарии бизнес-логики

## 26.1. Пользователь вручную добавляет skill

1. Указывает folder/URL/repository.
2. Skill немедленно появляется в Collection как `user-selected`.
3. Denet извлекает metadata и scripts list.
4. Utility comparison не запускается.
5. Пользователь прикрепляет skill к проекту или оставляет on-demand.
6. При первом executable action Trust применяет grant.
7. Denet не редактирует skill автоматически.

## 26.2. Skill найден в скачанном проекте

1. Project scan обнаруживает `SKILL.md`.
2. Регистрирует project-local candidate с repository revision.
3. Не переносит в global Collection.
4. Проверяет applicability к проекту.
5. В Trusted project может активировать on-demand по policy.
6. Outcomes сохраняются.
7. Global promotion возможен позднее.

## 26.3. В интернете найден новый skill

1. Need-driven search находит candidate.
2. Cheap inspection и provenance.
3. Candidate сравнивается с top близкими skills.
4. Выясняется: текущий skill лучше, но новый имеет сильный verification step.
5. Denet не устанавливает новый как default.
6. Создаёт Delta Proposal.
7. Проверяет fork основного skill.
8. При выигрыше предлагает/принимает новую version по ownership policy.

## 26.4. Пользователь вручную прикрепляет слабый skill

1. Binding создаётся без спора.
2. Skill применяется по user intent.
3. Denet может позже показать measured regression, но не снимает pin сам в Manual profile.
4. Пользователь решает оставить, заменить или ограничить.

## 26.5. Агент научился процедуре в проекте

1. Успешный trace содержит повторяемую процедуру.
2. Denet проверяет, не является ли это memory note/instruction/script.
3. Создаёт project-local skill candidate.
4. Следующее релевантное использование служит canary.
5. После нескольких успехов skill становится project validated.
6. Только затем рассматривается global promotion.

## 26.6. Upstream skill обновился, локальная версия изменена

1. Denet получает diff.
2. Видит local modifications.
3. Не перезаписывает.
4. Предлагает keep/merge/fork/reset.
5. Declarative non-conflicts можно merge.
6. Executable conflicts требуют review.
7. Сохраняется rollback.

## 26.7. Найдены два MCP для Notion

1. Registry получает candidates из Official Registry/community.
2. Сравнивает tools/resources/auth/update/health.
3. Проверяет, есть ли native connector.
4. Выбирает native для common actions, MCP B как on-demand для уникального tool.
5. MCP A сохраняется bookmark/rejected relation.
6. Project видит минимальный tool set.

## 26.8. Plugin содержит skill, MCP и hook

1. Provider marketplace install preview перечисляет components.
2. Пользователь или policy включает skill и MCP.
3. Hook остаётся disabled.
4. Update добавляет новый OAuth scope.
5. Trust re-review только затронутого component.
6. Project bindings не теряются.

## 26.9. Provider убрал модель

1. Live discovery помечает endpoint unavailable.
2. Direct chat не переключается молча.
3. Denet предлагает successors/fallback.
4. Internal Task может перейти по заранее разрешённой policy.
5. Task state и memory сохраняются.

## 26.10. Пользователь импортирует GGUF

1. Artifact регистрируется сразу.
2. Hash/metadata/license сохраняются.
3. Hardware Profiler предлагает llama.cpp/Ollama.
4. Benchmark не обязателен.
5. После использования outcome привязывается к artifact+runtime+hardware.

## 26.11. Connector имеет два аккаунта

1. GitHub Connector Definition один.
2. Два Account Bindings различаются organization/scopes.
3. Project pin-ит рабочий account.
4. Agent не может использовать личный account из-за похожего repo name.
5. Reauth сохраняет identity.

## 26.12. Browser backend падает

1. Playwright structured session не может обработать canvas.
2. Run переключается на visual backend только для нужного шага.
3. Authenticated session и state сохраняются, если возможно.
4. Outcome записывает hybrid path.
5. В будущем selector знает, что для этого app нужен hybrid backend.

## 26.13. Project pack передан другому пользователю

1. Denet читает capability requirements/references.
2. Показывает missing/incompatible/replacements.
3. Project-local skills остаются project-local.
4. Credentials не передаются.
5. User выбирает providers/accounts.
6. Project работает degraded до установки optional parts.

## 26.14. Автоматический Curator ошибся

1. Candidate был неверно признан substitute.
2. User или eval фиксирует regression.
3. Relation исправляется на specialization/conflict.
4. Default binding откатывается.
5. Ошибка добавляется в regression cases.
6. Curator threshold повышается только для этого класса, а не глобально.

---

# 27. Требования к будущей архитектуре

Будущая реализация должна поддержать:

- stable IDs для definitions/artifacts/installations/bindings;
- origin/revision/provenance;
- orthogonal state facets;
- scoped Registry/Collection/Project/Run views;
- lazy metadata and content loading;
- provider-native adapters;
- live discovery/probes;
- candidate comparison;
- ownership/fork/merge/rollback;
- project portability;
- measured utility;
- health/quota/fallback;
- Trust handoff;
- durable install/update/remove;
- local hardware/runtime inventory;
- component-level plugin enablement;
- connector account identity;
- capability relations and delta proposals;
- audit without model call on fast path.

Конкретная БД, package manager, scheduler и service topology определяются позже.

---

# 28. Definition of Done

Документ достаточен, если для любого provider/model/tool/skill/MCP/plugin/connector/backend можно ответить:

- что именно это за сущность;
- откуда она пришла;
- кто владеет;
- как ручной импорт отличается от автоматического;
- как она попадает в Registry, Collection, Project и Run;
- как проверяется compatibility;
- как сравнивается с альтернативами;
- как обрабатывается unique delta;
- как активируется лениво;
- как измеряется utility;
- как update взаимодействует с local edits;
- как fork/merge/rollback работают;
- как project-local capability продвигается глобально;
- как она отключается/удаляется;
- как health/quota/fallback работают;
- что принадлежит Trust, Agentic, Memory, Server и UI;
- какие динамические сведения требуют live revalidation.

---

# Part III. Датированный каталог рынка и готовых реализаций

**Снимок проверен:** 11 июля 2026 года.  
Этот каталог служит evidence и исходной картой адаптеров. Он не заменяет live discovery.


## A. First-class agent runtimes

Эти integrations дают больше, чем model API, поэтому для них оправданы нативные адаптеры.

### 12.1. OpenAI: Codex и Agents SDK

#### Что подтверждено на дату исследования

Официальная документация OpenAI объединяет:

- Agents SDK с agent definitions, orchestration, guardrails, results/state, observability, evals и voice agents;
- web search;
- MCP и connectors;
- skills;
- shell/local shell;
- computer use;
- file search/retrieval;
- tool search и programmatic tool calling;
- image generation и code interpreter;
- conversation state, background mode, streaming, WebSocket, multi-agent и webhooks;
- Realtime/audio;
- Codex SDK, App Server и MCP Server;
- Codex plugins, hooks, skills, local/cloud environments, worktrees и third-party integrations.

Источники:

- [OpenAI tools and Agents SDK documentation](https://developers.openai.com/api/docs/guides/tools)
- [OpenAI Agents SDK](https://developers.openai.com/api/docs/guides/agents)
- [Codex documentation](https://developers.openai.com/codex/)

#### Denet adapter

First-class adapter должен уметь использовать два разных surface:

1. **Codex runtime** для project/coding sessions, worktrees, code changes и provider-native capabilities.
2. **OpenAI API/Agents SDK** для server-side agents, built-in tools, Realtime и model calls.

Они не считаются одним connection автоматически: subscription, API project, cloud deployment и workspace policy различаются.

#### Что не переносить в common layer

- Codex-specific plugin semantics;
- provider approvals;
- background job handles;
- exact reasoning controls;
- provider-native citations/tool output;
- ChatGPT Apps/Workspace Agents semantics.

### 12.2. Anthropic: Claude Code и Agent SDK

#### Что подтверждено

Claude Agent SDK предоставляет agent loop, built-in tools, context management и программируемые extensions, основанные на той же среде, что Claude Code.

Claude Code поддерживает нативные:

- sessions;
- subagents;
- hooks;
- MCP;
- skills;
- permissions/sandbox;
- plugins и plugin marketplaces;
- provider deployments;
- monitoring/costs;
- project instruction semantics;
- agent teams и background work в соответствующих surfaces.

Claude plugins могут включать skills, agents, hooks, MCP servers и LSP servers; marketplaces поддерживают GitHub, Git, subdirectories, npm, private sources, version pinning и updates.

Источники:

- [Claude Agent SDK overview](https://code.claude.com/docs/en/agent-sdk/overview)
- [Claude Code plugin marketplaces](https://code.claude.com/docs/en/plugin-marketplaces)
- [Claude Code documentation](https://code.claude.com/docs/)

#### Denet adapter

Должен сохранять:

- Claude session continuation;
- native subagents;
- hooks;
- permission events;
- plugin/marketplace identity;
- CLAUDE.md/rules semantics через Effective Instruction Set;
- native usage/cost, если доступно;
- Bedrock/Vertex/Foundry deployment identity отдельно от direct Anthropic.

### 12.3. Google: Gemini CLI и Agent Development Kit

#### Что подтверждено

Google ADK позиционируется как framework для agents и workflows, поддерживает разные model providers, tools, MCP, OpenAPI, authentication, skills/plugins, sessions/memory, A2A и live/bidirectional experiences.

Gemini CLI имеет extension mechanism для bundles of prompts, MCP servers и custom commands; provider-specific capabilities включают thinking controls и built-in tools в соответствующих Gemini APIs.

Источники:

- [Google Agent Development Kit](https://adk.dev/)
- [Gemini CLI extensions](https://geminicli.com/docs/extensions/)
- [Gemini API documentation](https://ai.google.dev/gemini-api/docs)

#### Denet adapter

First-class adapter оправдан для:

- Gemini CLI coding/project sessions;
- ADK agent/runtime integration;
- Gemini Live/voice;
- Google built-in search/code/URL/file/computer tools, если доступны выбранной модели;
- Google Cloud Vertex deployment;
- extension discovery.

### 12.4. GitHub Copilot

#### Роль

GitHub Copilot является не только model endpoint, а интегрированной developer surface:

- IDE agent mode;
- cloud coding agent;
- CLI;
- skills;
- MCP;
- repository instructions;
- GitHub issue/PR workflow;
- premium request/subscription accounting;
- provider/model choice в поддерживаемых surfaces.

Источник:

- [GitHub Copilot documentation](https://docs.github.com/en/copilot)

#### Denet adapter

Поддержка полезна, если пользователь уже оплачивает Copilot или хочет запускать work через GitHub-native issue/PR.

Adapter не должен считать Copilot premium request эквивалентом API tokens.

#### GitHub Models — отдельная capability

GitHub Models не следует смешивать с Copilot. Это отдельный model catalogue и experimentation/evaluation surface: он позволяет прототипировать с несколькими моделями, хранить prompts и сравнивать результаты. Для Denet это удобный discovery/evaluation connection, но не замена Copilot coding agent и не гарантия production-доступности каждого model endpoint.

Источник: [GitHub Models](https://docs.github.com/en/github-models).

### 12.5. Mistral Agents

Mistral Agents and Conversations API заявляет:

- persistent state;
- single and multiple agents;
- multimodal models;
- built-in code execution, web search, image generation и document library;
- handoffs;
- custom tools;
- managed MCP connectors;
- structured output и citations.

Источник: [Mistral Agents documentation](https://docs.mistral.ai/studio-api/agents/introduction).

Для Denet это кандидат first-class server-side agent runtime, особенно если пользователь использует Mistral models или managed connectors.

### 12.6. AWS Bedrock Agents и Strands

#### Bedrock Agents

Подходит для AWS-native deployments, knowledge bases, action groups, guardrails, tracing, IAM и enterprise data boundaries.

Источник: [Amazon Bedrock Agents](https://docs.aws.amazon.com/bedrock/latest/userguide/agents.html).

#### Strands Agents

Strands — open-source agent SDK, ориентированный на provider flexibility, tools/MCP, multi-agent и AWS integrations.

Источник: [Strands Agents](https://strandsagents.com/).

#### Решение

Bedrock connection регистрируется как cloud-managed deployment. Strands может быть runtime adapter, но не обязателен, если Agentic Control уже использует другой SDK.

### 12.7. Microsoft Agent Framework

Microsoft Agent Framework объединяет agent primitives, tools, memory/workflows, hosting, providers и enterprise integrations в экосистеме Microsoft.

Источник: [Microsoft Agent Framework](https://learn.microsoft.com/en-us/agent-framework/).

Он является кандидатом для Microsoft/Azure-centric installation, но Denet не должен зависеть от него глобально.

### 12.8. OpenRouter Agent SDK

OpenRouter предоставляет unified API, model routing, fallbacks, provider selection, reasoning-token controls, tool calling и собственный Agent SDK.

Источник: [OpenRouter documentation](https://openrouter.ai/docs/quickstart).

Преимущество — быстрый доступ к сотням моделей и routing. Недостаток — дополнительный слой provider semantics и privacy. Для Denet это first-class aggregator connection, но не замена direct adapters.

### 12.9. OpenClaw и Hermes как внешние runtime/reference systems

Они рассматриваются отдельно ниже. Denet может:

- подключать их как external agent runtime;
- импортировать channels/skills/MCP/config;
- заимствовать patterns;
- не встраивать целиком.

---

## B. Hosted model providers: датированный каталог

Этот раздел является **snapshot на 11 июля 2026 года**. Перед реализацией или выбором модели Denet проверяет live docs/API.

### 13.1. Tier A: нативные адаптеры высокой ценности

#### OpenAI

Классы возможностей:

- reasoning/general/coding models;
- vision, image, audio, video;
- Responses API;
- Agents SDK;
- built-in tools;
- Realtime;
- Codex;
- MCP/connectors/skills/plugins.

Рекомендация: нативный adapter обязателен.

#### Anthropic

Классы возможностей:

- Claude API;
- extended/adaptive thinking;
- vision;
- tool use;
- Claude Code/Agent SDK;
- subagents/hooks/MCP/skills/plugins;
- cloud deployments.

Рекомендация: нативный adapter обязателен.

#### Google

Классы возможностей:

- Gemini text/vision/audio/live;
- thinking controls;
- built-in search/code/URL/file/computer tools в поддерживаемых моделях;
- Gemini CLI;
- ADK;
- Vertex AI.

Рекомендация: нативный adapter обязателен.

#### GitHub Copilot

Рекомендация: нативный adapter для subscription coding surface; не использовать как generic API provider.

### 13.2. Tier B: сильные direct providers

#### xAI

Официальная документация на дату исследования перечисляет:

- text generation;
- configurable reasoning;
- structured output;
- streaming;
- multi-agent;
- image/video;
- voice/STT/TTS;
- function calling;
- web и X search;
- code execution;
- collections/RAG;
- remote MCP;
- async/deferred/batch/WebSocket;
- OpenAI-compatible usage.

Источник: [xAI documentation](https://docs.x.ai/overview).

Рекомендация: сначала native/OpenAI-compatible hybrid adapter; отдельный native extension для X search, remote MCP, voice и media.

#### Mistral AI

Помимо Agents, Mistral предоставляет model API, vision/document understanding, audio, embeddings, moderation и managed connectors.

Источник: [Mistral documentation](https://docs.mistral.ai/).

Рекомендация: native adapter полезен.

#### Cohere

Сильные области:

- chat/generation;
- enterprise retrieval;
- embeddings;
- rerank;
- citations/tool use в поддерживаемых APIs;
- private deployment options.

Источник: [Cohere documentation](https://docs.cohere.com/).

Рекомендация: отдельные capabilities для rerank/embed и model generation; generic model adapter допустим.

#### DeepSeek

Документация на дату исследования заявляет OpenAI/Anthropic-compatible API, thinking mode, JSON output, tool calls, context caching и agent integrations.

Источник: [DeepSeek API documentation](https://api-docs.deepseek.com/).

Рекомендация: generic OpenAI/Anthropic adapters плюс native facets для thinking. Model IDs и deprecations всегда live.

#### Alibaba Cloud Model Studio / Qwen

Платформа предоставляет Alibaba/Qwen и другие models, OpenAI-compatible APIs, function calling и cloud services.

Источник: [Alibaba Cloud Model Studio](https://www.alibabacloud.com/help/en/model-studio/).

Рекомендация: cloud/native adapter при использовании; generic compatibility для базового inference.

#### Moonshot AI / Kimi

Кандидат direct provider для long-context/reasoning/coding моделей. Подключается через официальный API или через aggregator. Конкретные features проверяются live.

Источник: [Moonshot platform documentation](https://platform.moonshot.ai/docs/).

#### Zhipu AI / GLM

Кандидат direct provider для GLM family, tool use и multimodal APIs.

Источник: [Zhipu BigModel documentation](https://docs.bigmodel.cn/).

#### MiniMax

Кандидат для text, speech, music/image/video и agent APIs в зависимости от текущего offering.

Источник: [MiniMax platform documentation](https://platform.minimax.io/docs/).

#### Perplexity

Полезен как search/research-oriented API provider и model endpoint. Denet должен отличать provider search result от собственного Research workflow.

Источник: [Perplexity API documentation](https://docs.perplexity.ai/).

#### AI21 Labs

Кандидат general enterprise model provider. Подключается generic/native по текущим API capabilities.

Источник: [AI21 documentation](https://docs.ai21.com/).

#### Writer

Кандидат enterprise model/agent/knowledge platform, особенно для корпоративной установки.

Источник: [Writer developer documentation](https://dev.writer.com/).

### 13.3. Региональные и дополнительные экосистемы, которые стоит поддерживать

Эта группа особенно важна для установки, где доступность, язык, data residency или способы оплаты делают глобальные providers неудобными. Она не означает, что каждому нужен first-class adapter: многие подключаются generic API или cloud adapter плюс live probes.

#### Yandex AI Studio

На дату исследования платформа объединяет Model Gallery, Agent Atelier, MCP Hub, AI Search, SpeechKit, Search API, Vision OCR, Translate и Yandex Workflows. Для русскоязычной установки это важный кандидат не только как model endpoint, но и как источник speech/search/OCR/MCP capabilities.

Источник: [Yandex AI Studio documentation](https://aistudio.yandex.ru/docs/en/).

Рекомендация: отдельный Yandex Cloud connection с IAM/region/quota, но нативный agent adapter создавать только если Agent Atelier или managed runtime реально нужен Denet.

#### GigaChat API

Официальная документация на дату исследования заявляет generation и embeddings, streaming, structured output, reasoning mode, image и 3D generation, file handling, function calling, batch processing и OpenAI compatibility.

Источник: [GigaChat API documentation](https://developers.sber.ru/docs/ru/gigachat/guides/main).

Рекомендация: generic OpenAI-compatible path для базового inference плюс native facets для reasoning, files/media и специфичной authentication.

#### Baidu Qianfan

Qianfan позиционируется как model service и Agent development platform с model services, Agent engine, tools/MCP и application components. Это важный кандидат для китайского рынка и enterprise deployments внутри Baidu Cloud.

Источник: [Baidu Qianfan documentation](https://cloud.baidu.com/doc/qianfan/index.html).

#### Volcengine Ark / Doubao

Документация Ark на дату исследования включает text и multimodal understanding, image/video/3D generation, embeddings, deep thinking, function calling, web/knowledge tools, cloud/remote MCP, Responses API, managed Agents, Skills, sessions, persistent memory и multi-agent capabilities.

Источник: [Volcengine Ark documentation](https://docs.volcengine.com/docs/82379?lang=zh).

Рекомендация: cloud-native adapter имеет смысл только при реальном использовании региона/ecosystem; иначе достаточно generic compatibility и live capability observations.

#### Meta model APIs

Meta предоставляет developer model surfaces, но продуктовая линейка и доступность API могут быстро меняться. Denet регистрирует Meta как динамический direct provider только после live discovery конкретного официального endpoint; downloadable Meta weights отдельно относятся к local/hosted-open-model path.

Источник: [Meta AI developer portal](https://ai.developer.meta.com/).

#### Tencent Hunyuan и другие региональные clouds

Tencent Hunyuan, Huawei Cloud Pangu, VK Cloud AI, regional sovereign clouds и будущие providers должны подключаться по тому же принципу: Provider Definition, официальный connection, live model discovery, probes и отдельная data-residency policy. Их отсутствие в initial adapters не должно требовать изменения core.

### 13.4. Tier C: optional direct providers

В эту группу могут входить региональные или специализированные providers, добавляемые через generic adapter и live probe.

Правило: отсутствие имени в этом документе не требует изменения ядра Denet. Достаточно нового Provider Definition и adapter configuration, если API совместим.

---

## C. Cloud model platforms

### 14.1. Amazon Bedrock

Дает доступ к нескольким model families, IAM, regions, private networking, guardrails, knowledge bases и agents.

Использовать при:

- AWS-native инфраструктуре;
- data residency;
- enterprise IAM;
- Bedrock-only deployment.

Direct Anthropic/OpenAI-like provider connection и Bedrock deployment остаются разными endpoints.

### 14.2. Microsoft Foundry / Azure AI

Полезен для Azure identity, private networking, model catalog, agents, speech и enterprise governance.

Источники:

- [Microsoft Foundry](https://learn.microsoft.com/en-us/azure/ai-foundry/)
- [Azure Speech](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/)

### 14.3. Google Vertex AI

Отдельный connection для Gemini и partner/open models с GCP identity, region и enterprise policies.

Источник: [Vertex AI generative AI documentation](https://cloud.google.com/vertex-ai/generative-ai/docs).

### 14.4. IBM watsonx.ai

Кандидат enterprise/private deployment.

Источник: [IBM watsonx documentation](https://www.ibm.com/docs/en/watsonx/).

### 14.5. Oracle OCI Generative AI

Кандидат для Oracle cloud environments.

Источник: [OCI Generative AI documentation](https://docs.oracle.com/en-us/iaas/Content/generative-ai/home.htm).

### 14.6. Databricks Mosaic AI

Полезен, когда models, vector search, data governance и enterprise data platform уже находятся в Databricks.

Источник: [Databricks generative AI documentation](https://docs.databricks.com/en/generative-ai/).

### 14.7. Snowflake Cortex

Кандидат для данных и AI внутри Snowflake.

Источник: [Snowflake Cortex documentation](https://docs.snowflake.com/en/user-guide/snowflake-cortex/llm-functions).

### 14.8. Cloudflare Workers AI

Кандидат для edge/serverless inference и глобально распределённых applications.

Источник: [Cloudflare Workers AI](https://developers.cloudflare.com/workers-ai/).

### 14.9. Правило cloud platform adapter

Cloud adapter должен сохранять:

- region;
- deployment ID;
- identity mode;
- actual underlying model;
- data governance;
- quota;
- network restrictions;
- provider-specific error semantics.

---

## D. Aggregators и hosted open-model inference

### 15.1. OpenRouter

Плюсы:

- единый endpoint к большому каталогу;
- routing/fallback/provider selection;
- model listing;
- budgets/presets;
- reasoning token controls;
- tool calling и structured output;
- собственный Agent SDK;
- remote MCP для live metadata.

Минусы:

- дополнительный посредник;
- downstream provider может меняться;
- не все native features передаются;
- privacy зависит от route/provider.

Рекомендация: first-class aggregator adapter.

### 15.2. Hugging Face Inference Providers

Дает единый interface к нескольким inference providers и Hugging Face ecosystem.

Источник: [Hugging Face Inference Providers](https://huggingface.co/docs/inference-providers/index).

Полезно для:

- broad open-model discovery;
- быстрых экспериментов;
- unified auth;
- model repository linkage.

### 15.3. Together AI

На дату исследования предоставляет OpenAI-compatible inference для open models, text/vision/image/video/STT/TTS/embeddings/rerank, fine-tuning, dedicated endpoints, code interpreter/sandbox и GPU clusters.

Источник: [Together AI documentation](https://docs.together.ai/intro).

### 15.4. Fireworks AI

Предоставляет serverless/dedicated inference, open models, tool calling, reasoning, vision, embeddings/rerank, fine-tuning и integrations для coding agents.

Источник: [Fireworks AI documentation](https://docs.fireworks.ai/getting-started/introduction).

### 15.5. Groq

Кандидат для low-latency inference, speech/OCR и provider-built tools/agentic systems в текущем offering.

Источник: [Groq documentation](https://console.groq.com/docs/overview).

### 15.6. Cerebras Inference

Кандидат для very high token throughput на поддерживаемых open models.

Источник: [Cerebras Inference documentation](https://inference-docs.cerebras.ai/).

### 15.7. NVIDIA NIM

NIM предоставляет контейнеризированные inference microservices для NVIDIA ecosystem, локального или cloud deployment.

Источник: [NVIDIA NIM documentation](https://docs.nvidia.com/nim/).

### 15.8. Replicate

Полезен для широкого каталога моделей и media/experimental workloads через unified prediction API.

Источник: [Replicate documentation](https://replicate.com/docs).

### 15.9. Другие кандидаты

- Nebius AI Studio/Token Factory;
- SambaNova Cloud;
- Baseten;
- Modal;
- RunPod;
- Anyscale;
- BentoML-hosted endpoints;
- dedicated KServe/Triton deployments.

Они подключаются по необходимости через generic or deployment adapters. Реализация каждого не является требованием MVP.

---

## E. Локальные модели как first-class capability

### 16.1. Зачем они нужны

- приватность;
- offline;
- отсутствие per-token billing;
- низкая latency для маленьких models;
- фоновые classifiers;
- постоянный ambient triage;
- embeddings/rerank;
- control над model version;
- возможность работать при outage providers.

### 16.2. Почему локальное не всегда дешевле

Учитываются:

- потребление энергии;
- загрузка GPU/CPU;
- latency;
- модельный storage;
- обслуживание runtimes;
- качество;
- ограничение context;
- цена hardware;
- потеря времени на настройку.

### 16.3. Hardware Profiler

Denet обнаруживает:

- OS;
- CPU и instruction support;
- RAM;
- GPU vendor/model;
- VRAM/shared memory;
- drivers/runtime;
- available disk;
- thermal/power mode;
- текущую нагрузку;
- remote local nodes.

Он предлагает model/runtime fit, но не скачивает большие models без согласия или policy.

### 16.4. Capability зависит от комбинации

```text
model weights
+ quantization
+ runtime
+ chat template
+ tool parser
+ hardware
+ context configuration
= фактический local endpoint
```

Каждая комбинация имеет отдельный probe record.

---

## F. Каталог локальных runtimes

### 17.1. Ollama

Преимущества:

- простая установка на Windows/macOS/Linux/Docker;
- local/cloud model management;
- API и SDK;
- streaming;
- thinking;
- structured outputs;
- vision;
- embeddings;
- tool calling;
- интеграции с apps/editors/agents.

Источник: [Ollama documentation](https://docs.ollama.com/).

Рекомендация: default simple local runtime.

### 17.2. LM Studio / llmster

Преимущества:

- удобный GUI;
- headless daemon;
- REST;
- OpenAI-compatible Responses/Chat/Embeddings;
- Anthropic Messages compatibility;
- structured output;
- tool use;
- MCP via API;
- model download/load/unload management.

Источник: [LM Studio developer documentation](https://lmstudio.ai/docs/developer).

Рекомендация: default local runtime для пользователя, который хочет GUI и easy management.

### 17.3. llama.cpp

Преимущества:

- portable C/C++ inference;
- GGUF ecosystem;
- широкий hardware support;
- quantization;
- local server;
- edge/CPU-friendly deployment.

Источник: [llama.cpp repository](https://github.com/ggml-org/llama.cpp).

Рекомендация: низкоуровневый portable backend и основа для edge runtimes.

### 17.4. vLLM

Преимущества:

- high-throughput serving;
- OpenAI-compatible server;
- tool calling;
- structured output;
- reasoning outputs;
- multimodal;
- embeddings/classification/reward/scoring;
- distributed deployment;
- observability;
- широкие integrations;
- поддержка GPU/CPU/TPU/Intel XPU в соответствующих configurations.

Источник: [vLLM documentation](https://docs.vllm.ai/).

Рекомендация: основной server-grade runtime на GPU, если hardware подходит.

### 17.5. SGLang

Преимущества:

- высокопроизводительный serving;
- structured generation;
- advanced scheduling/caching;
- distributed/open-model workloads.

Источник: [SGLang documentation](https://docs.sglang.io/).

Рекомендация: альтернатива vLLM, выбираемая по benchmark на конкретных models/hardware.

### 17.6. Hugging Face Transformers / TGI

Преимущества:

- максимальная совместимость с model ecosystem;
- custom architectures;
- research/fine-tuning;
- broad modalities.

Недостаток: не всегда лучший production throughput без дополнительной настройки.

Источники:

- [Transformers](https://huggingface.co/docs/transformers/)
- [Text Generation Inference](https://huggingface.co/docs/text-generation-inference/)

### 17.7. MLX-LM

Оптимизирован для Apple Silicon и MLX ecosystem.

Источник: [MLX-LM repository](https://github.com/ml-explore/mlx-lm).

Рекомендация: first-class Mac backend.

### 17.8. OpenVINO GenAI

Полезен для Intel CPU/GPU/NPU и OpenVINO ecosystem, включая text, vision, speech и optimized pipelines в поддерживаемых моделях.

Источник: [OpenVINO GenAI documentation](https://docs.openvino.ai/2026/openvino-workflow-generative/inference-with-genai.html).

Рекомендация: first-class candidate для Intel devices, включая Intel Arc.

### 17.9. TensorRT-LLM / NVIDIA NIM

Преимущества:

- NVIDIA-optimized inference;
- deployment tooling;
- high performance на поддерживаемых GPUs;
- containerized NIM services.

Источники:

- [TensorRT-LLM](https://nvidia.github.io/TensorRT-LLM/)
- [NVIDIA NIM](https://docs.nvidia.com/nim/)

### 17.10. LocalAI

All-in-one OpenAI-compatible local stack, ориентированный на LLM, vision, speech, image/video и разные backends.

Источник: [LocalAI](https://localai.io/).

Рекомендация: optional all-in-one backend; проверять сложность и качество по сравнению со специализированными runtimes.

### 17.11. ONNX Runtime GenAI

Кандидат для Windows/DirectML и portable ONNX deployments.

Источник: [ONNX Runtime GenAI](https://onnxruntime.ai/docs/genai/).

### 17.12. Local runtime selection

Примерная политика:

- простая personal installation: Ollama или LM Studio;
- NVIDIA server: vLLM/SGLang/TensorRT-LLM;
- Intel device: OpenVINO GenAI, при необходимости llama.cpp/Ollama;
- Apple Silicon: MLX-LM, Ollama или llama.cpp;
- CPU/edge: llama.cpp;
- broad multimodal all-in-one: LocalAI после eval;
- custom research model: Transformers.

Это стартовые heuristics, а не жёсткая маршрутизация.

---

## G. Источники локальных моделей

### 18.1. Hugging Face Hub

Главный broad ecosystem для model weights, model cards, datasets и adapters.

Источник: [Hugging Face Hub](https://huggingface.co/models).

Denet может:

- искать models;
- читать model card/license;
- выбирать compatible artifact;
- pin revision;
- проверять hash;
- скачивать через runtime;
- сохранять provenance.

### 18.2. Ollama Library

Удобный curated/distribution layer для Ollama models.

Источник: [Ollama model library](https://ollama.com/search).

### 18.3. LM Studio model discovery

LM Studio умеет искать и загружать local models; источник сохраняется в endpoint metadata.

### 18.4. ModelScope

Дополнительный model hub, особенно для азиатских providers и моделей.

Источник: [ModelScope](https://modelscope.cn/models).

### 18.5. Vendor repositories

Некоторые providers публикуют weights в собственных GitHub/Hugging Face organizations.

Правило: Denet предпочитает официальный publisher/revision, но не считает model безопасной только из-за популярности.

### 18.6. Model import lifecycle

```text
search
→ inspect license/model card/files
→ choose runtime-compatible artifact
→ estimate hardware fit
→ download/pin/hash
→ malware/archive scan where applicable
→ load in isolated probe
→ capability/quality probes
→ make available
```

---



## H. Computer use: каталог и сравнительная карта

### 22.1. Не один backend

Универсального лучшего computer-use решения нет.

Правильная иерархия:

1. direct API/connector;
2. structured app API/CLI;
3. DOM/accessibility/DevTools automation;
4. provider-native visual computer use;
5. local/open visual computer-use stack;
6. собственный low-level adapter только для glue.

### 22.2. Почему structured-first

Structured actions обычно:

- быстрее;
- дешевле;
- легче проверить;
- дают точные targets;
- проще ограничить;
- меньше зависят от theme/layout/resolution.

Visual use нужен, когда:

- API нет;
- приложение нативное;
- DOM/accessibility недостаточны;
- интерфейс динамический;
- нужно проверить реальный UX;
- пользователь хочет работать в существующей session.

### 22.3. Common ComputerUseBackend

Denet нормализует:

- target device/session;
- observe state;
- structured tree, если есть;
- screenshot;
- pointer/keyboard action;
- app/window scope;
- current URL/process;
- artifact capture;
- effect proposal;
- user takeover;
- pause/stop;
- backend continuation state;
- result/evidence.

Trust Fabric владеет разрешениями.

### 22.4. Backend classes

#### Browser structured

- Playwright CLI/skills;
- Playwright MCP;
- Chrome DevTools MCP;
- Selenium/WebDriver/CDP adapter;
- provider browser tool.

#### Browser adaptive AI

- Skyvern;
- browser-use;
- provider-native browser agents.

#### Arbitrary GUI provider-native

- OpenAI computer use;
- Anthropic computer use;
- Gemini computer use;
- другие provider-native tools.

#### Local/open GUI

- UI-TARS Desktop;
- OmniParser plus action model;
- Windows/macOS/Linux accessibility/UI Automation adapters;
- research/open CUA models after evaluation.

### 22.5. Selection policy

Пример:

- изменить GitHub issue через connector/API;
- протестировать веб-форму через Playwright;
- посмотреть network/performance через Chrome DevTools MCP;
- пройти unfamiliar visual webpage через Skyvern/browser-use или visual model;
- работать в Photoshop/native app через provider/local visual CU;
- приватный desktop task — local backend, если качество достаточно;
- точная покупка — structured extraction + Trust confirmation before effect.

---

## I. Browser automation catalogue

### 23.1. Playwright CLI + Skills

Лучший default для coding agents, если задача может быть выражена concise commands/tests.

Преимущества:

- низкий token overhead;
- воспроизводимый script;
- integration с code/test;
- подходит для repeated browser tasks.

### 23.2. Playwright MCP

Использует accessibility snapshots вместо обязательного vision и даёт persistent/rich browser interaction.

Преимущества:

- structured browser state;
- deterministic action application;
- no vision requirement;
- широкий MCP client support;
- persistent profiles/isolated contexts/extension connection.

Источник: [Microsoft Playwright MCP](https://github.com/microsoft/playwright-mcp).

### 23.3. Chrome DevTools MCP

Полезен для:

- live Chrome inspection/control;
- console/network;
- debugging;
- performance analysis;
- screenshots;
- authenticated existing browser.

Источник: [Chrome DevTools MCP](https://github.com/ChromeDevTools/chrome-devtools-mcp).

Это не полная замена Playwright: DevTools сильнее для диагностики, Playwright — для robust automation/testing.

### 23.4. Skyvern

Сочетает browser automation, LLM и computer vision, предлагает Playwright-compatible SDK/no-code workflows и AI fallback для динамических страниц.

Источник: [Skyvern](https://github.com/Skyvern-AI/skyvern).

Использовать для:

- brittle third-party sites;
- workflows, где selectors часто меняются;
- form/data extraction;
- managed cloud/local browser tasks.

### 23.5. browser-use

Open-source AI browser automation framework, поддерживающий разные models/providers.

Источник: [browser-use](https://github.com/browser-use/browser-use).

Подходит как flexible agentic browser backend; сравнивать с Skyvern и structured Playwright на реальных Denet cases.

### 23.6. Собственный browser backend

Denet не пишет браузерный движок. Возможная собственная часть:

- backend selector;
- session manager;
- state/effect normalization;
- auth/profile broker;
- screenshot/evidence capture;
- provider fallback;
- common user takeover.

---

## J. Arbitrary GUI and local computer use

### 24.1. Provider-native backends

Преимущество:

- model и action protocol уже согласованы;
- provider может лучше обрабатывать screenshots;
- меньше собственного ML.

Недостатки:

- cloud privacy;
- provider-specific action semantics;
- cost;
- model availability;
- слабая переносимость.

Denet поддерживает несколько provider-native adapters, а не выбирает один навсегда.

### 24.2. UI-TARS Desktop

Open-source multimodal agent stack для computer/browser interaction.

Источник: [UI-TARS Desktop](https://github.com/bytedance/UI-TARS-desktop).

Кандидат для local/open backend и reference implementation. Перед adoption нужны platform-specific evals, resource measurement и security review.

### 24.3. OmniParser

Screen parsing tool для выделения интерактивных элементов и grounded UI understanding.

Источник: [Microsoft OmniParser](https://github.com/microsoft/OmniParser).

OmniParser не является полным agent runtime; его можно использовать как perception component собственного/local backend.

### 24.4. OS-native accessibility

Denet должен использовать, где возможно:

- Windows UI Automation;
- macOS Accessibility;
- Linux AT-SPI;
- browser DOM/accessibility;
- Android accessibility/device APIs.

Это даёт structured targets и уменьшает зависимость от vision.

### 24.5. Нужно ли писать своё

Своё оправдано для:

- unified desktop session adapter;
- OS-specific capture/control;
- ownership/takeover overlay;
- combination accessibility + screenshot;
- connection to remote Denet nodes;
- evidence and Trust handoff.

Не оправдано первой версией:

- собственная foundation GUI model;
- собственный browser engine;
- обучение на миллионах trajectories.

---

## K. Speech and voice backends

Voice document позднее определит conversation logic. Здесь описываются только доступные capabilities.

### 25.1. Required capability classes

- voice activity detection;
- noise suppression;
- streaming STT;
- batch STT;
- diarization;
- speaker embedding/association;
- punctuation/turn recovery;
- language detection;
- TTS streaming;
- low-latency speech-to-speech;
- voice selection;
- forced alignment;
- local/offline mode.

### 25.2. OpenAI Realtime/audio

Кандидат first-class cloud backend для realtime voice, STT и TTS в OpenAI ecosystem.

Источник: [OpenAI audio and Realtime docs](https://developers.openai.com/api/docs/guides/realtime).

### 25.3. Google Gemini Live / Google Cloud Speech

Кандидат для live multimodal interaction и Google cloud deployment.

Источники:

- [Gemini Live API](https://ai.google.dev/gemini-api/docs/live)
- [Google Cloud Speech-to-Text](https://cloud.google.com/speech-to-text/docs)
- [Google Cloud Text-to-Speech](https://cloud.google.com/text-to-speech/docs)

### 25.4. xAI Voice

Документация xAI на дату исследования перечисляет Voice Agent API, STT, TTS и realtime capabilities.

Источник: [xAI documentation](https://docs.x.ai/overview).

### 25.5. ElevenLabs

Сильный specialized provider для:

- expressive TTS;
- low-latency TTS;
- STT;
- voice agents;
- voices/cloning;
- dubbing/alignment.

Источник: [ElevenLabs documentation](https://elevenlabs.io/docs/overview/intro).

### 25.6. AssemblyAI

Специализированный STT/voice infrastructure provider с batch/realtime transcription, voice agent APIs, speech understanding и integrations.

Источник: [AssemblyAI documentation](https://www.assemblyai.com/docs/).

### 25.7. Azure Speech

Поддерживает STT, TTS, translation, custom speech/voice, Voice Live и containers.

Источник: [Azure Speech documentation](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/).

### 25.8. Другие cloud candidates

- Deepgram;
- AWS Transcribe/Polly;
- Google Cloud Speech;
- Speechmatics;
- Cartesia;
- PlayHT.

Каждый подключается специализированным adapter, если его latency/quality/language/value оправдывают поддержку.

### 25.9. Local STT

#### faster-whisper

CTranslate2-based Whisper implementation, ориентированная на более быстрый и memory-efficient inference.

Источник: [faster-whisper](https://github.com/SYSTRAN/faster-whisper).

#### whisper.cpp

Portable C/C++ Whisper runtime, полезный для edge/CPU/local applications.

Источник: [whisper.cpp](https://github.com/ggml-org/whisper.cpp).

#### Другие

- NVIDIA Riva/NIM speech;
- sherpa-onnx;
- Vosk;
- provider/local models through vLLM/OpenVINO where supported.

### 25.10. Local TTS

Кандидаты:

- Piper;
- Kokoro implementations;
- Coqui XTTS forks/projects;
- MeloTTS;
- sherpa-onnx TTS;
- OS system voices.

Piper является быстрым local neural TTS reference.

Источник: [Piper](https://github.com/rhasspy/piper).

### 25.11. Voice backend selection

- ambient always-on triage: local VAD/STT;
- immediate simple response: low-latency local or cloud TTS;
- expressive long-form: specialized cloud TTS;
- sensitive conversation: local-only if acceptable;
- multilingual meeting: best measured STT/diarization backend;
- offline: whisper.cpp/faster-whisper + local TTS;
- voice cloning: explicit user-controlled capability with Trust policy.

---

## L. Vision, image, video and document backends

### 26.1. Vision understanding

Может предоставляться:

- general multimodal models OpenAI/Anthropic/Google/xAI/Mistral/open models;
- local VLM through vLLM/Ollama/LM Studio/Transformers;
- OCR/document tools;
- specialized screen parsers.

### 26.2. Image generation/editing

Provider candidates:

- OpenAI image generation;
- Google image models;
- xAI Imagine;
- Stability ecosystem;
- Black Forest Labs;
- Replicate/Together/Fireworks-hosted models;
- local ComfyUI/Stable Diffusion/FLUX pipelines.

Denet adapter normalizes:

- prompt/input images;
- edit mask/reference;
- output artifact;
- seed/settings when available;
- provider/model/version;
- safety/error;
- usage.

### 26.3. Video

Candidates:

- OpenAI video models;
- Google video models;
- xAI video;
- Runway;
- Luma;
- Kling;
- MiniMax;
- Replicate/hosted open video;
- local pipelines where practical.

Список является dynamic. Denet не должен first-class реализовывать каждого provider до появления задачи.

### 26.4. OCR and document AI

Candidates:

- provider multimodal models;
- Tesseract/PaddleOCR;
- Azure/Google/AWS document services;
- Mistral Document AI;
- Unstructured/docling-like parsers;
- accessibility/DOM before OCR for screens.

### 26.5. ComfyUI

Для локальных generative media workflows Denet может подключать ComfyUI через API/workflow files вместо написания собственного image pipeline.

Источник: [ComfyUI](https://github.com/Comfy-Org/ComfyUI).

---

## M. Web, search, retrieval and code capabilities

### 27.1. Web search sources

Возможные backends:

- provider built-in web search;
- Bing/Google/Brave/Exa/Tavily/Serper/Perplexity APIs;
- browser automation;
- direct site APIs/RSS;
- custom search engines;
- local indexed World Intelligence.

Research не является отдельной hard-coded subsystem здесь. Project agent или skill выбирает search capability.

### 27.2. Search backend selection

- current fact with authoritative source: direct official source/API;
- broad discovery: general search;
- semantic company/research search: Exa-like backend;
- news monitoring: RSS/news APIs/search;
- social signal: X search/provider connector;
- private corpus: Memory Fabric retrieval;
- site requiring interaction: browser backend.

### 27.3. File search/RAG

Может быть provider-built-in или Denet Memory Fabric.

Provider file search не заменяет Denet memory, потому что:

- scope/provenance/governance differ;
- project portability;
- cross-provider use;
- current-state logic.

Но provider-native file search может использоваться как local capability внутри конкретной run.

### 27.4. Embeddings/rerank

Sources:

- OpenAI;
- Cohere;
- Google;
- Voyage-like specialized providers;
- Jina AI;
- Mistral;
- local sentence-transformers;
- vLLM/TEI/TGI/local endpoints.

Memory Fabric выбирает retrieval architecture; Capability Fabric предоставляет endpoints и measured quality/latency.

### 27.5. Code execution

Предпочтительные classes:

- local project shell;
- sandbox/container;
- provider code interpreter;
- Together code sandbox;
- remote dev environment;
- SSH/container/VM backend;
- notebook execution.

Tool output и side effects проходят Agentic/Trust contracts.

### 27.6. Shell and terminal

Denet не нуждается в отдельном MCP, если project runtime уже имеет native shell. MCP/connector оправдан для remote environment или standardized external service.

---

## N. Connectors and integrations catalogue

### 28.1. Categories

- messaging;
- email;
- calendar;
- contacts;
- source control;
- issue tracking;
- cloud storage;
- documents/office;
- databases;
- observability;
- smart home/devices;
- social/news;
- media/design;
- payments/commerce;
- remote execution.

### 28.2. First-priority personal connectors

- Telegram;
- Gmail/IMAP/SMTP where needed;
- Google Calendar;
- Google Contacts;
- Google Drive/Docs/Sheets/Slides;
- GitHub;
- local filesystem;
- browser;
- desktop/device nodes;
- notifications.

### 28.3. Development connectors

- GitHub;
- GitLab;
- Linear;
- Jira;
- Slack/Discord;
- CI systems;
- cloud deployments;
- Sentry/Datadog/Grafana;
- package registries.

### 28.4. Connector implementation priority

1. official API/app connector;
2. provider-supported OAuth integration;
3. MCP server;
4. CLI;
5. browser/computer-use;

UI automation не должна заменять официальный API без причины.

### 28.5. Provider-managed connectors vs Denet connectors

Provider-managed connector удобен, но:

- может быть недоступен другим models;
- имеет provider-specific scopes;
- data may transit provider;
- session/state may be non-portable.

Denet-native connector предпочтителен для core personal services, если нужен cross-provider use и единая policy.

### 28.6. Connector discovery

Sources:

- official provider connector catalogue;
- official MCP Registry;
- provider plugin marketplaces;
- organization catalogue;
- user import;
- Denet built-ins.

---

## O. Что взять из OpenClaw

OpenClaw является ценным production-oriented case, но не источником абсолютной истины.

### 29.1. Полезные patterns

#### Local-first Gateway

Один control plane для sessions, channels, tools и events хорошо соответствует Denet Server Runtime.

#### Channel adapters

OpenClaw поддерживает широкий набор messaging surfaces и показывает ценность единого channel abstraction.

#### Onboarding wizard и doctor

Команды onboarding/doctor снижают сложность настройки providers, channels и skills. Denet следует иметь:

- guided setup;
- connection test;
- doctor report;
- repair suggestions;
- visible risky configuration.

#### Pairing для входящих каналов

DM pairing/allowlist — практичный default для личного assistant.

#### Companion nodes/apps

Отдельные Windows/macOS/mobile nodes, подключённые к Gateway, соответствуют модели Denet server + device capabilities.

#### Model failover

OpenClaw поддерживает auth profiles/model fallbacks. Denet заимствует идею, но применяет собственные privacy/trust/provider-lock rules.

#### Skills registry

ClawHub демонстрирует полезность discovery, но marketplace не становится trust authority.

#### Voice wake, Canvas, nodes, cron/webhooks

Эти capabilities показывают целостный assistant surface и должны учитываться в Voice/Server/UI документах.

Источник: [OpenClaw repository](https://github.com/openclaw/openclaw).

### 29.2. Что не копировать как default

OpenClaw README описывает full host access для main session как default personal mode. Denet не должен автоматически копировать это для всех contexts.

В Denet пользователь может выдать широкий Trusted-Elevated grant, но boundary остаётся явной и scoped.

### 29.3. Integration strategy

Варианты:

- импорт channel configuration;
- использовать OpenClaw как external gateway;
- импорт Agent Skills;
- подключить tools через MCP/local API;
- заимствовать UX patterns;
- не встраивать весь runtime внутрь Denet.

---

## P. Что взять из Hermes Agent

### 30.1. Полезные patterns

Hermes Agent демонстрирует:

- provider/model independence;
- messaging gateway для нескольких channels;
- natural-language scheduling/cron;
- self-learning skills;
- Skills Hub;
- MCP integration;
- isolated subagents;
- multiple terminal backends: local, Docker, SSH и cloud sandboxes;
- model switching;
- cross-platform continuity;
- setup/doctor tooling.

Источники:

- [Hermes Agent](https://github.com/NousResearch/hermes-agent)
- [Hermes optional skills](https://github.com/NousResearch/hermes-agent/tree/main/optional-skills)
- [Hermes optional MCPs](https://github.com/NousResearch/hermes-agent/tree/main/optional-mcps)

### 30.2. Что заимствовать

#### Backend abstraction

Terminal/runtime backend выбирается независимо от модели. Это хорошо совпадает с Capability Resolution.

#### Natural-language schedule

Пользователь может сформулировать расписание, а Server/Event layer создаёт событие. Capability document лишь регистрирует scheduler/channel tools.

#### Skill learning candidate loop

Повторяющийся опыт может создать candidate skill, но Denet добавляет eval, versioning и rollback из своих документов.

#### Channel continuity

Одна система может продолжать session через CLI/Telegram/Discord и другие surfaces. Denet реализует это через server canonical state, а не через копирование Hermes.

#### Optional integrations directories

Полезный pattern для curated optional capabilities, которые не включаются в core installation.

### 30.3. Что не копировать автоматически

- не позволять self-created skill включаться без проверки;
- не считать broad model compatibility доказательством равного качества;
- не создавать subagent, если один agent сохраняет цель лучше;
- не передавать user secrets в model context ради простоты.

### 30.4. Integration strategy

Hermes может быть:

- external agent runtime;
- источник skills/MCP candidates;
- channel gateway candidate;
- reference для terminal backends;
- comparative test system.

---


# Part IV. Обслуживание каталога, источники и контроль полноты

# 29. Dated Catalogue Maintenance

## 29.1. Что остаётся стабильным

Стабильны:

- сущности Capability Fabric;
- origin-aware lifecycle;
- ownership;
- scopes;
- comparison/disposition;
- project/run binding;
- health/fallback;
- Trust handoff;
- правила user control.

## 29.2. Что является динамическим

- model IDs;
- context limits;
- pricing;
- subscription allowances;
- reasoning controls;
- provider tools;
- marketplaces;
- supported modalities;
- local runtime compatibility;
- package versions;
- project activity/maintenance;
- provider regions and quotas.

## 29.3. Refresh process

```text
fetch official inventory/docs/changelog
→ compare snapshot
→ update Capability Observations
→ run only affected probes
→ classify breaking/security changes
→ preserve history
→ update recommendations
→ notify only meaningful changes
```

## 29.4. Security-relevant refresh

Re-review при:

- new OAuth scope;
- new executable/hook;
- changed endpoint/domain;
- tool schema expands effects;
- package publisher/repository changes;
- model loader starts requiring custom code;
- remote MCP changes auth/session behavior;
- connector account identity changes.

## 29.5. Utility-relevant refresh

Re-evaluate при:

- framework/version mismatch;
- major skill rewrite;
- model/runtime upgrade;
- repeated regressions;
- new significantly better alternative;
- token/context overhead change;
- provider deprecation.

## 29.6. Human-readable snapshot

Документ может содержать датированный каталог для проектирования, но live Registry является источником текущего состояния. Каждый catalogue entry в будущей системе должен показывать `last_observed` и источник.

---

# 30. Каталог ключевых источников исследования

Ниже перечислены sources, поддерживающие устойчивые решения и датированный каталог. Научные работы 2026 года часто являются preprints; они используются как design evidence и должны подтверждаться собственными Denet-specific evals.

## Skills, lifecycle и utility

**[S01] SWE-Skills-Bench — Do Agent Skills Actually Help in Real-World Software Engineering?** Контролируемое paired evaluation: ограниченная средняя польза, большой разброс overhead, вред от version mismatch. 2026.  
https://arxiv.org/abs/2603.15401

**[S02] Malicious Agent Skills in the Wild.** Поведенчески подтверждённые malicious skills и supply-chain patterns. 2026.  
https://arxiv.org/abs/2602.06547

**[S03] Agent Skills in the Wild: Security Vulnerabilities at Scale.** Масштабный анализ vulnerabilities, scripts и attack surface. 2026.  
https://arxiv.org/abs/2601.10338

**[S07] Agent Skills open specification.** `SKILL.md`, scripts/references/assets и progressive disclosure.  
https://agentskills.io/home

**[S08] Hermes Agent Skills System.** Source-of-truth directory, progressive disclosure, conditional activation, agent-managed skills, write approval, bundles, Hub install/update/audit/reset и external directories. Актуально на дату исследования.  
https://hermes-agent.nousresearch.com/docs/user-guide/features/skills

**[S09] SkillFoundry.** Mining operational contracts и closed-loop expand/repair/merge/prune для domain skill libraries. 2026.  
https://arxiv.org/abs/2604.03964

**[S06] OpenAI/Codex Build Skills.** Skills как reusable workflow packages, plugins для distribution, progressive disclosure. Актуально на дату исследования.  
https://developers.openai.com/codex/skills/

## Tool retrieval, comparison и context control

**[S04] ToolScope: Enhancing LLM Agent Tool Use through Tool Merging and Context-Aware Filtering.** Overlap/redundancy, audited merging и retrieval ограниченного tool set. 2025.  
https://arxiv.org/abs/2510.20036

**[S05] Dynamic tool retrieval family.** Evidence в пользу retrieval вместо полной tool exposure и повторного дорогого model routing. 2025–2026.  
- MassTool: https://arxiv.org/abs/2507.00487  
- Dynamic System Instructions and Tool Exposure / ITR: https://arxiv.org/abs/2602.17046  
- AutoTool: https://arxiv.org/abs/2511.14650

**[S12] OpenAI Tool Search / programmatic tool calling.** Provider-native динамическое раскрытие tools.  
https://developers.openai.com/api/docs/guides/tools

## MCP

**[S11] Official MCP Registry.** Public discovery/metadata, но не trust authority.  
https://registry.modelcontextprotocol.io/

**[S13] MCP Security Best Practices.** OAuth, confused deputy, token passthrough prohibition, SSRF, sessions, local server compromise и scope minimization.  
https://modelcontextprotocol.io/docs/tutorials/security/security_best_practices

**Model Context Protocol specification and docs.**  
https://modelcontextprotocol.io/

## Provider plugins and extensions

**[S14] Claude Code plugin marketplaces.** Central discovery, versions/updates и packages с skills, agents, hooks, MCP и LSP.  
https://code.claude.com/docs/en/plugin-marketplaces

**[S15] Gemini CLI extensions.** Bundles of prompts, MCP, commands, hooks, subagents и skills.  
https://geminicli.com/docs/extensions/

**OpenAI/Codex Plugins.** Skills/connectors distribution и provider-native package semantics.  
https://developers.openai.com/codex/plugins/

## Local models and artifacts

**[S10] Hugging Face Hub documentation.** Repository revisions, model cards, artifacts и custom model code.  
https://huggingface.co/docs/hub/models

**Ollama.** Local model runtime/import.  
https://docs.ollama.com/

**LM Studio.** Desktop/local server and model management.  
https://lmstudio.ai/docs/developer

**llama.cpp.** Portable local inference/GGUF.  
https://github.com/ggml-org/llama.cpp

**vLLM.** High-throughput serving.  
https://docs.vllm.ai/

**SGLang.** Serving/runtime.  
https://docs.sglang.io/

**MLX-LM.** Apple Silicon.  
https://github.com/ml-explore/mlx-lm

**OpenVINO GenAI.** Intel-oriented local inference.  
https://docs.openvino.ai/2026/openvino-workflow-generative/inference-with-genai.html

## Browser and computer use

**[S16] Playwright MCP.** Accessibility-snapshot browser automation и MCP interface.  
https://github.com/microsoft/playwright-mcp

**[S17] Chrome DevTools MCP.** DevTools-based browser control, debugging и performance.  
https://github.com/ChromeDevTools/chrome-devtools-mcp

**[S18] browser-use.** Adaptive browser automation.  
https://github.com/browser-use/browser-use

**[S19] Skyvern.** AI-assisted browser workflow automation.  
https://github.com/Skyvern-AI/skyvern

**UI-TARS Desktop.** Local visual desktop agent reference.  
https://github.com/bytedance/UI-TARS-desktop

**OmniParser.** Screen parsing reference.  
https://github.com/microsoft/OmniParser

## Reference ecosystems

**OpenClaw.** Local-first gateway, channels, skills, onboarding/doctor, model failover, companion nodes и security defaults.  
https://github.com/openclaw/openclaw

**Hermes Agent.** Provider-independent agent, Skills Hub, procedural memory, MCP, channels, migration и self-improvement patterns.  
https://github.com/NousResearch/hermes-agent

## Provider and platform documentation

### OpenAI

- https://developers.openai.com/api/docs/guides/tools
- https://developers.openai.com/api/docs/guides/agents
- https://developers.openai.com/codex/

### Anthropic

- https://code.claude.com/docs/en/agent-sdk/overview
- https://code.claude.com/docs/
- https://code.claude.com/docs/en/plugin-marketplaces

### Google and GitHub

- https://adk.dev/
- https://ai.google.dev/gemini-api/docs
- https://geminicli.com/docs/extensions/
- https://docs.github.com/en/copilot
- https://docs.github.com/en/github-models

### Other direct providers

- https://docs.x.ai/overview
- https://docs.mistral.ai/studio-api/agents/introduction
- https://docs.cohere.com/
- https://api-docs.deepseek.com/
- https://www.alibabacloud.com/help/en/model-studio/
- https://platform.moonshot.ai/docs/
- https://docs.bigmodel.cn/
- https://platform.minimax.io/docs/
- https://docs.perplexity.ai/
- https://docs.ai21.com/
- https://dev.writer.com/
- https://aistudio.yandex.ru/docs/en/
- https://developers.sber.ru/docs/ru/gigachat/guides/main
- https://cloud.baidu.com/doc/qianfan/index.html
- https://docs.volcengine.com/docs/82379?lang=zh

### Cloud and aggregators

- https://docs.aws.amazon.com/bedrock/latest/userguide/agents.html
- https://learn.microsoft.com/en-us/agent-framework/
- https://openrouter.ai/docs/quickstart
- https://docs.together.ai/intro
- https://docs.fireworks.ai/getting-started/introduction
- https://console.groq.com/docs/overview
- https://inference-docs.cerebras.ai/
- https://huggingface.co/docs/inference-providers/index
- https://docs.nvidia.com/nim/
- https://replicate.com/docs

### Voice/media

- https://elevenlabs.io/docs/overview/intro
- https://www.assemblyai.com/docs/
- https://learn.microsoft.com/en-us/azure/ai-services/speech-service/
- https://github.com/SYSTRAN/faster-whisper
- https://github.com/ggml-org/whisper.cpp
- https://github.com/rhasspy/piper

---

# 31. Краткий чек-лист реализации

Denet обязан:

1. Иметь Registry, Collection, Candidate Pool, Project Capability Set и Run Capability Plan.
2. Разделять Definition, Source, Artifact, Installation и Binding.
3. Иметь origin-aware behavior.
4. Добавлять user-selected capability немедленно без utility gate.
5. Не путать добавление с правом исполнения.
6. Не устанавливать auto-discovered capability глобально без policy.
7. Сохранять project-imported capabilities project-local по умолчанию.
8. Иметь cheap inspection до model analysis.
9. Иметь relation types: duplicate/substitute/specialization/complement/fallback/conflict/fork.
10. Поддерживать Capability Delta Proposal.
11. Защищать user-owned capabilities от автоматического rewrite.
12. Поддерживать provider-managed и Denet-managed ownership.
13. Поддерживать three-way update, fork и rollback.
14. Загружать skills/tools лениво.
15. Не считать skill полезным по факту установки.
16. Собирать minimal project capability set.
17. Позволять пользователю вручную attach/detach/pin/forbid.
18. Поддерживать run-only capability.
19. Создавать project-local skill из повторяемого опыта только при необходимости.
20. Не превращать каждый факт или workaround в skill.
21. Продвигать global только переносимые и проверенные capabilities.
22. Поддерживать skill bundles/conditional activation без context bloat.
23. Поддерживать manual и official/community skill sources.
24. Инспектировать auto-found executable skills.
25. Поддерживать MCP tools/resources/prompts раздельно.
26. Сравнивать native connector и MCP.
27. Не считать Official Registry permission/trust authority.
28. Поддерживать partial plugin enablement.
29. Сохранять provider-native package/update semantics.
30. Разделять Connector Definition и Account Binding.
31. Поддерживать несколько accounts и project resource scopes.
32. Использовать API/CLI/structured browser раньше visual computer use.
33. Поддерживать несколько computer-use backends и hybrid mode.
34. Не писать собственную foundation GUI model первой версией.
35. Поддерживать local models как artifact+runtime+hardware combination.
36. Не запускать custom model code молча.
37. Поддерживать model revisions, quantizations, load/warm/evict.
38. Разделять provider, connection, endpoint и agent runtime.
39. Хранить provider facts как dated observations.
40. Не менять direct-chat model молча.
41. Учитывать local measured outcomes при internal routing.
42. Иметь health/quota/fallback и unknown-effect protection.
43. Не запускать Curator-model на каждом вызове.
44. Оценивать capabilities пропорционально impact.
45. Уметь prune/simplify/remove, а не только добавлять.
46. Передавать Trust только security-relevant metadata и invocation request.
47. Не дублировать UI/server implementation details.
48. Сохранять portable project requirements без credentials.
49. Объяснять, почему capability выбрана и что было альтернативой.
50. Проверять business logic на сквозных сценариях раздела 26.

Конец документа.
