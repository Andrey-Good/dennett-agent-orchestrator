# Denet Implementation and Evolution Strategy

**Статус:** каноническая стратегия реализации и долгосрочного развития.  
**Версия:** 1.0  
**Дата:** 13 июля 2026 года  
**Область:** процесс разработки, интеграция работы coding-agents, качество, поддерживаемость, изменение архитектуры и выпуск продукта.  
**Не заменяет:** бизнес-спецификации, архитектурные тома, ADR, локальные `AGENTS.md` или конкретные work packages.

---

# 0. Назначение и итоговое решение

Denet слишком велик и долгоживущ, чтобы реализовывать его как одну огромную задачу, один непрерывный агентный сеанс или последовательность «сначала полностью построить backend, затем память, затем UI». Такой способ создаёт позднюю интеграцию, скрытые несовместимости и код, который выглядит завершённым по частям, но не работает как единый продукт.

Denet должен развиваться как **непрерывно работающая система вертикальных срезов**, где каждый принятый шаг:

- даёт наблюдаемое поведение;
- сохраняет сборку и основные сценарии в рабочем состоянии;
- имеет автоматические проверки;
- ограничивает область изменения;
- может быть откатан или заменён;
- оставляет понятный след для следующего человека или агента;
- не требует перечитывать весь проект для локального изменения.

Короткая формула:

> **Малые когерентные пакеты работы + короткоживущие worktree/ветки + автоматические quality gates + риск-зависимое ревью + постоянная трассируемость требований + постепенная замена через порты и feature flags.**

Стратегия оптимизирует не максимальный объём кода за один запуск агента, а **стоимость устойчивого прогресса**:

```text
sustainable_progress =
    verified_user_value
  + reduced_future_uncertainty
  + reusable_contracts
  - defects
  - rework
  - coordination_overhead
  - architectural_entropy
  - maintenance_burden
```

---

# 1. Главные законы разработки Denet

## 1.1. Main всегда должен оставаться пригодным для продолжения

После каждого merge:

- repository собирается в поддерживаемых конфигурациях;
- быстрые тесты проходят;
- миграции либо совместимы, либо полностью управляемы;
- незавершённая функция скрыта feature flag, capability state или недоступным маршрутом;
- предыдущий работающий вертикальный срез не повреждён;
- документация и generated contracts согласованы.

DORA связывает частую интеграцию небольших изменений, быстрые автоматические тесты и немедленное исправление сломанной сборки с более высокой скоростью и стабильностью поставки. Ветка, которая живёт много дней и накапливает сотни изменений, является исключением и требует явного обоснования. [[S07]] [[S08]]

## 1.2. Изменение не считается прогрессом до интеграции

Количество написанных строк, созданных файлов, commits или запущенных агентов не является мерой завершения.

Progress существует, когда:

1. новый результат доступен через реальный путь системы;
2. поведение проверено тестом или наблюдаемым доказательством;
3. failure path понятен;
4. изменения интегрированы либо готовы к безопасному merge;
5. следующий разработчик может продолжить без восстановления скрытого контекста.

## 1.3. Сначала вертикальный срез, затем расширение

Вертикальный срез проходит через реальные границы продукта, например:

```text
Desktop chat
→ Node
→ Head
→ Agent Runtime
→ Result Envelope
→ Memory Event
→ UI update
```

Сначала он может использовать fake provider и embedded storage. Затем отдельные части заменяются production-адаптерами при сохранении публичных контрактов.

Запрещённый путь:

```text
полностью построить все database tables
→ полностью построить все providers
→ полностью построить все UI screens
→ впервые попробовать соединить их через год
```

## 1.4. Один mutable state — один владелец

Новый код обязан указывать:

- кто владеет записью;
- кто может только читать;
- через какой port выполняется изменение;
- какая revision/idempotency семантика действует;
- как состояние восстанавливается после crash;
- как оно мигрирует.

Если для одной сущности существуют две «почти канонические» копии, работа останавливается до явного решения.

## 1.5. Сильный агент не отменяет engineering discipline

Даже GPT‑5.6 Sol с максимальным reasoning способен ошибиться в:

- неверно понятом контракте;
- незаметной смене семантики;
- граничных состояниях;
- безопасности;
- обратной совместимости;
- миграциях;
- редких конкурентных сценариях;
- качестве собственного ревью.

Исследования pull requests показывают, что тип задачи существенно влияет на успешность coding-agents и ни один агент не является лучшим во всех классах задач. Исследование AI code review также показывает низкий recall человеческих замечаний и ухудшение при избыточном контексте. Поэтому проверка строится вокруг независимых спецификаций, тестов и evidence, а не доверия к уверенности модели. [[S16]] [[S17]]

## 1.6. Скрытое рассуждение не является артефактом проекта

Reasoning context модели может повышать качество связанных turns, но архитектурные решения, предположения и причины изменений обязаны быть записаны в:

- work package;
- ADR;
- code comments только там, где они объясняют неочевидный локальный инвариант;
- test name/fixture;
- PR description;
- run result.

Модель можно сменить, сессию потерять, reasoning compaction изменить. Проект не должен потерять понимание собственного устройства.

## 1.7. Замена должна происходить постепенно

Крупные замены выполняются через:

- существующий port или новый узкий port;
- branch by abstraction;
- две реализации одновременно;
- shadow/dual-read/dual-write только при необходимости;
- feature flag или project profile;
- сравнение результатов;
- staged migration;
- удаление старого пути после доказательства.

Branch by Abstraction позволяет заменять supplier постепенно, сохраняя систему рабочей, и хорошо соответствует adapter-first архитектуре Denet. [[S11]]

## 1.8. Простота — контролируемый ресурс

Новый слой, crate, service, queue, state machine, agent или schema добавляется только если:

- закрывает конкретный failure mode;
- имеет владельца;
- имеет тестируемый контракт;
- даёт измеримую пользу;
- не может быть заменён существующим механизмом;
- имеет путь удаления.

---

# 2. Три непрерывных цикла

## 2.1. Цикл решения

Используется для неизвестных, дорогих и труднообратимых вопросов.

```text
неопределённость
→ исследование / spike
→ альтернативы
→ измерение
→ ADR или отказ от решения
→ обновление work packages
```

Примеры:

- Tauri vs другой desktop shell;
- embedded memory vs memoryd;
- pgvector vs отдельный vector plane;
- Screenpipe vs native capture;
- React Native vs native mobile;
- собственный Managed Run executor vs Restate/Temporal.

Цикл решения не должен превращаться в бесконечное исследование. Spike имеет:

- вопрос;
- ограничение времени;
- измеримые критерии;
- минимальный prototype;
- решение или явное продление.

## 2.2. Цикл поставки

```text
ready Work Package
→ isolated worktree
→ baseline verification
→ smallest coherent implementation
→ fast tests
→ checkpoint/commit
→ required test set
→ risk-based review
→ merge queue
→ post-merge verification
```

Это основной рабочий цикл coding-agent.

## 2.3. Цикл эволюции

```text
production evidence / usage / incident / new technology
→ evaluate impact
→ candidate replacement or improvement
→ shadow/canary
→ migrate
→ observe
→ remove old path
```

Цикл эволюции защищает проект от двух крайностей:

- архитектура навечно застывает на технологиях 2026 года;
- каждый новый AI tool вызывает переписывание ядра.

---

# 3. Иерархия работ

## 3.1. Program

Вся реализация Denet.

## 3.2. Milestone

Завершённый этап, создающий новую работающую способность продукта. Milestone имеет demo, acceptance gate и rollback/continuation plan.

Примеры:

- Local Desktop Conversation;
- Managed Runs;
- Memory Production Baseline;
- Personal Server and Devices;
- Mobile Trusted Remote;
- Voice and Ambient.

## 3.3. Vertical Slice

Сквозной пользовательский или системный путь, который можно реально выполнить и проверить.

Пример:

> Пользователь создаёт проект, пишет агенту, получает streamed ответ, перезапускает UI и продолжает session.

## 3.4. Work Package

Основная единица автономной реализации агентом. Work Package должен быть **минимальной когерентной единицей**, а не произвольной микрозадачей.

Нормальный размер:

- один ясный результат;
- один основной владелец;
- обычно 1–3 code roots;
- обычно несколько часов агентной работы;
- diff, который можно содержательно проверить;
- тесты, завершающиеся в разумное время;
- не более одного труднообратимого решения.

Work Package слишком велик, если:

- изменяет несколько независимых bounded contexts;
- содержит несколько migrations;
- его нельзя описать одним результатом;
- reviewer не может понять diff без повторной реализации;
- acceptance зависит от множества ещё не существующих частей;
- агенту нужен почти весь репозиторий и все документы одновременно.

Work Package слишком мал, если:

- не создаёт независимо полезный результат;
- требует отдельного агента только для переименования одной функции;
- теряет общую архитектурную или UX-идею;
- интегратор должен заново собрать десятки фрагментов.

## 3.5. Commit

Commit — атомарный технический checkpoint внутри Work Package. Он не обязан соответствовать отдельной пользовательской ценности, но должен:

- собираться или быть явно отмеченным как промежуточный в незамерженной ветке;
- иметь понятный intent;
- не смешивать независимые изменения;
- облегчать review и rollback.

---

# 4. Стратегия веток и интеграции

## 4.1. Short-lived worktree per Work Package

Каждый автономный пакет получает отдельный Git worktree и branch.

Преимущества:

- агенты не конфликтуют через stash/reset;
- один branch имеет одного writer;
- незавершённая работа не загрязняет основной checkout;
- можно параллельно проверять независимые пакеты;
- задача сохраняет собственную среду.

OpenAI Codex использует worktrees как отдельные background environments и поддерживает handoff между foreground checkout и worktree. [[S04]]

## 4.2. Mainline policy

- `main` защищён;
- прямые push запрещены, кроме emergency/repository-owner policy;
- required checks обязательны;
- merge выполняется squash или rebase по принятому ADR;
- branch удаляется после merge;
- merge queue включается, когда одновременно работают несколько агентов или PR throughput становится заметным.

GitHub merge queue повторно проверяет PR на последней версии target branch и с изменениями впереди него, снижая риск «зелёный PR ломает main после чужого merge». [[S09]]

## 4.3. Small batch rule

Ориентир:

- branch живёт часы, а не недели;
- один PR соответствует одному Work Package;
- большой milestone складывается из нескольких вертикально совместимых PR;
- incomplete behavior скрывается feature flag или недоступным route;
- generated code и mechanical refactor отделяются от semantic change, если это упрощает review.

## 4.4. Feature flags

Feature flag используется для:

- незавершённой функции на main;
- canary;
- shadow execution;
- быстрого kill switch;
- сравнения старого и нового backend;
- staged rollout.

Feature flag не должен:

- заменять permissions;
- храниться навсегда без owner/expiry;
- создавать десятки не протестированных комбинаций;
- скрывать несовместимую migration;
- становиться бизнес-правилом без документации.

OpenFeature даёт provider-neutral API и позволяет менять flag backend, не связывая domain code с конкретным сервисом. [[S10]]

Каждый flag имеет:

```yaml
flag:
  id: stable.id
  owner: module_or_team
  created_for: work_package_id
  default: false
  environments: []
  removal_condition: text
  expiry_review_at: date
  tests_for_both_states: true
```

## 4.5. CODEOWNERS и зоны повышенного риска

Даже если владелец проекта один, CODEOWNERS полезен как машинная карта ownership. Изменения в критических областях должны требовать специального review gate:

- trust/permissions;
- effects;
- sync;
- migrations;
- protocols;
- cryptography/secrets;
- update/recovery;
- deletion;
- sensor privacy.

GitHub позволяет автоматически запрашивать владельцев и требовать approval code owner для изменённых путей. [[S12]]

---

# 5. Риск-зависимая строгость

Одинаковый процесс для исправления опечатки и изменения key recovery является не безопасностью, а бюрократией.

## 5.1. R0 — механическое изменение

Примеры:

- форматирование;
- generated docs;
- typo;
- комментарий без semantic change;
- fixture rename.

Требования:

- static checks;
- affected tests;
- автоматический merge допустим.

## 5.2. R1 — локальное внутреннее поведение

Примеры:

- private helper;
- UI state внутри одного экрана;
- локальный parser;
- internal adapter fix без contract change.

Требования:

- implementer self-review;
- unit/application tests;
- PR checks;
- независимый AI-review опционален.

## 5.3. R2 — публичный модульный контракт

Примеры:

- public Rust trait;
- Protobuf field;
- adapter contract;
- shared command;
- cross-module event;
- persistent local format.

Требования:

- detached review в отдельном контексте;
- compatibility tests;
- integration test;
- docs update;
- ADR, если решение трудно обратимо.

## 5.4. R3 — целостность, безопасность и долговечность

Примеры:

- permission enforcement;
- external effect;
- deletion;
- sync merge;
- migration;
- backup/restore;
- identity/key recovery;
- Head authority;
- cryptography.

Требования:

- acceptance tests до implementation;
- два независимых вида проверки: tests + detached adversarial review;
- failure injection;
- human owner approval before merge;
- rollback/recovery evidence;
- security/privacy impact note.

## 5.5. R4 — изменение продукта или фундаментальной архитектуры

Примеры:

- новая семантика памяти;
- изменение модели автономности;
- новый источник канонической истины;
- смена deployment topology;
- необратимый data format;
- новые default ambient/consent rules.

Требования:

- обновление бизнес-спецификации;
- исследование альтернатив;
- ADR;
- prototype/spike;
- явное решение владельца **до** основной реализации.

---

# 6. Тестовая стратегия как основа разработки

## 6.1. Acceptance раньше implementation

Перед написанием R2–R4 кода должны существовать:

- текстовый acceptance scenario;
- нормальный путь;
- failure/recovery path;
- минимум одна проверка неверного поведения;
- ссылка на requirement.

Agent-generated tests после написания кода полезны как дополнительные probes, но не заменяют заранее заданные критерии. Исследование SWE-bench trajectories не обнаружило устойчивого выигрыша от простого увеличения числа создаваемых агентом тестов; качество зависит от того, проверяют ли они реально значимое поведение. [[S18]]

## 6.2. Test sizes

Denet использует измеримые категории:

### Small

- без сети;
- без внешней БД;
- без sleep;
- детерминирован;
- миллисекунды/секунды;
- pure domain/application logic.

### Medium

- localhost/process boundary;
- disposable DB/object store;
- generated protocol client;
- fake provider;
- минуты.

### Large

- несколько процессов;
- OS/UI/device/emulator;
- provider sandbox/canary;
- backup/restore;
- minutes to hours.

Google использует Small/Medium/Large по наблюдаемым свойствам, а не размытым названиям «unit/integration». Это облегчает параллельный запуск и enforceable ограничения. [[S13]]

## 6.3. PR gate

Целевой быстрый gate должен укладываться примерно в 10 минут или меньше:

- format;
- lint;
- typecheck;
- schema/protocol compatibility;
- architecture fitness;
- Small tests;
- affected Medium tests;
- documentation validation;
- secret/dependency checks.

Долгие проверки идут отдельно:

- nightly;
- pre-release;
- risk-specific;
- scheduled hardware/provider canaries.

## 6.4. Независимость тестов

Тест:

- не зависит от порядка;
- не требует предыдущего теста;
- использует virtual clock вместо sleep;
- имеет уникальные resources;
- не обращается к production account;
- может повторяться с одинаковым seed;
- очищает либо изолирует state.

## 6.5. Тесты архитектуры

CI проверяет:

- запрещённые dependencies;
- provider types outside adapters;
- прямой DB access;
- отсутствие owner для persistent state;
- generated files changed by hand;
- protocol compatibility;
- наличие local `AGENTS.md` в bounded roots;
- external effect bypass;
- feature flags без expiry;
- undocumented public API growth.

## 6.6. Flaky test policy

Flaky test не разрешается просто перезапускать до зелёного merge.

Он:

1. получает incident ID;
2. временно quarantine только с owner и expiry;
3. фиксирует seed/logs;
4. не считается coverage для blocking requirement;
5. исправляется или удаляется;
6. отслеживается как quality debt.

---

# 7. Стратегия использования GPT‑5.6 Sol

Официальная документация GPT‑5.6 различает reasoning efforts от `none` до `max`; `max` предназначен для hardest quality-first workloads, а multi-agent beta аналогичен Ultra mode и полезен для задач, которые чисто делятся на независимые потоки. Документация также поддерживает persisted reasoning across turns, когда цели и предположения стабильны. [[S01]]

## 7.1. Рекомендуемые режимы

### Sol `max` / Pro + `max`

Использовать для:

- milestone design;
- архитектурных ADR;
- R3/R4 root cause;
- migration/recovery design;
- review сложного cross-domain change;
- security threat analysis;
- планирования большого refactor.

Не использовать автоматически для каждой функции: latency и расход могут не окупаться.

### Sol `high` или `xhigh`

Default для:

- Work Package implementation;
- тестов;
- адаптеров;
- сложного debugging;
- integration work.

### Sol `medium`

Для:

- локальных R0/R1 изменений;
- documentation updates;
- mechanical refactors;
- generated scaffolding;
- routine issue triage.

Если пользователь планирует использовать только Sol Ultra/high, уровни всё равно сохраняются как **политика расхода reasoning**, а не требование разных моделей.

## 7.2. Ultra / multi-agent

Разрешён, когда workstreams:

- действительно независимы;
- не пишут в одни файлы;
- имеют отдельные artifacts;
- могут проверяться независимо;
- имеют одного integrator;
- выигрыш времени превышает дублирование context.

Хорошие случаи:

- параллельные adapter spikes;
- независимые platform investigations;
- generation разных test fixtures;
- review разных bounded modules;
- сравнение нескольких целостных вариантов.

Плохие случаи:

- единая архитектурная идея разделена по слоям;
- несколько agents меняют один crate;
- каждый worker требует весь контекст;
- интегратор должен заново выполнить всё;
- команда создаётся только потому, что Ultra доступен.

## 7.3. Persisted reasoning

Использовать `reasoning.context=all_turns` или эквивалент только внутри стабильного Work Package, где:

- цель не меняется;
- assumptions остаются актуальными;
- одна session связана с одной веткой;
- context не включает другое независимое изменение.

Новый Work Package получает новый reasoning lineage. Иначе старые решения и локальные assumptions начинают влиять на несвязанную работу.

## 7.4. Review в независимом контексте

Implementer не должен быть единственным reviewer собственного diff для R2+.

Detached reviewer получает:

- requirement;
- diff;
- relevant files;
- tests/results;
- architecture invariants;
- ограниченный контекст.

Он не получает длинное объяснение автора до первоначальной проверки, чтобы снизить confirmation bias. OpenAI Codex поддерживает detached review и отдельную review model; review должен читать фактический Git diff, а не только transcript агента. [[S03]]

---

# 8. Автономная работа и границы длительного запуска

## 8.1. Чёткий Outcome Contract

OpenAI рекомендует для long-running work задавать ясный outcome, constraints и definition of done и продолжать связанную работу в одной task/session. [[S02]]

Каждый автономный запуск получает:

```yaml
autonomy_envelope:
  batch_id: AB-...
  allowed_work_packages: []
  allowed_code_roots: []
  forbidden_roots: []
  max_risk: R1
  max_parallel_agents: 1
  max_new_dependencies: 0
  external_effects: forbidden
  architecture_changes: forbidden
  public_contract_changes: forbidden
  migration_changes: forbidden
  stop_on_decision_gate: true
  stop_on_red_ci: true
  stop_after_packages: 3
  evidence_required: true
```

## 8.2. Автономный batch

Агент может последовательно выполнять несколько Work Packages без человека, если:

- все пакеты имеют статус `READY`;
- зависимости закрыты;
- каждый пакет укладывается в envelope;
- следующий пакет не требует решения пользователя;
- CI предыдущего пакета зелёный;
- branches/worktrees не конфликтуют;
- merge policy определена;
- budget не исчерпан.

## 8.3. Обязательные остановки

Агент останавливается и создаёт Decision Request, если:

- спецификация противоречива;
- требуется R4 решение;
- изменяется пользовательская семантика;
- добавляется production dependency без предварительного разрешения;
- меняется public API/protocol/schema несовместимо;
- требуется migration с потерей данных;
- затрагиваются secrets/identity/permissions;
- failure не воспроизводится;
- baseline tests уже красные и причина не очевидна;
- два разных подхода не дали результата;
- предполагаемый diff существенно больше Work Package;
- обнаружен security incident;
- нужен внешний платный сервис;
- тест требует реального опасного external effect;
- next package зависит от пользовательской оценки UX.

## 8.4. Checkpoints

Checkpoint создаётся по событию, а не каждую минуту:

- baseline established;
- public contract added;
- first passing vertical path;
- migration applied in fixture;
- failure reproduced;
- fix passes focused tests;
- package ready for review.

Checkpoint содержит:

- commit;
- текущий статус;
- tests;
- open risks;
- next action;
- changed assumptions.

## 8.5. Почему не «реализуй весь проект два дня»

Без bounded packages агент может:

- накопить несовместимые assumptions;
- продолжить после скрытого test failure;
- изменить архитектуру без ADR;
- создать слишком много похожих abstractions;
- потерять связь с пользовательской целью;
- начать исправлять собственные следствия вместо исходной причины;
- сгенерировать огромный diff, который никто не способен проверить.

Длительность сама по себе не проблема. Проблема — отсутствие проверяемых границ.

---

# 9. Ревью и merge

## 9.1. Review packet

Каждый PR содержит:

- Work Package ID;
- outcome;
- requirement refs;
- code roots;
- authoritative state affected;
- risks;
- exact tests;
- failures/limitations;
- migrations;
- observability;
- screenshots/demo для UX;
- rollback.

## 9.2. Review order

Reviewer проверяет:

1. соответствует ли результат requirement;
2. не расширен ли scope;
3. сохраняется ли ownership;
4. есть ли более простой implementation;
5. корректны ли failures/cancellation;
6. не скрыт ли external effect;
7. тесты проверяют поведение, а не implementation trivia;
8. API/format совместимы;
9. код читаем без transcript автора;
10. можно ли удалить/заменить компонент позднее.

## 9.3. Автор исправляет findings

Detached reviewer обычно не вносит исправления в тот же branch. Findings возвращаются implementer, который:

- подтверждает или аргументированно отклоняет;
- исправляет;
- добавляет regression test;
- повторно запускает checks.

Это сохраняет одного writer и понятную ответственность.

## 9.4. Human review

Владелец не обязан читать весь Rust diff. Его review нужен для:

- пользовательского поведения;
- privacy/autonomy defaults;
- major architecture choices;
- irreversible data decisions;
- UX acceptance;
- provider cost/lock-in;
- milestone demo.

Техническую корректность доказывают tests, architecture checks и agent review, но решение «это именно тот продукт, который нужен» остаётся за владельцем.

---

# 10. Поддерживаемость на горизонте десяти лет

## 10.1. Stable core, replaceable edges

Устойчивыми считаются:

- domain IDs;
- ownership;
- permission/effect semantics;
- MemoryPort;
- AgentRuntimePort;
- Capability descriptors;
- protocol compatibility rules;
- artifact/evidence identity;
- Task/Run semantics.

Сменными считаются:

- providers;
- models;
- agent SDK;
- vector backend;
- screen capture;
- voice transport;
- computer-use;
- mobile shell;
- sync acceleration;
- observability backend.

## 10.2. Public API discipline

Rust public crates следуют SemVer-совместимости. Cargo отдельно классифицирует removal/rename public items, trait changes, enum variants и toolchain requirements как потенциально breaking changes. [[S14]]

Правила:

- public surface минимален;
- structs имеют private fields или `non_exhaustive`, когда evolution вероятна;
- protocol enums резервируют unknown/forward-compatible handling;
- deprecated API сохраняется минимум один release window;
- breaking change получает migration guide;
- `cargo-semver-checks` или эквивалент входит в CI для publishable crates.

## 10.3. Dependency policy

Каждая production dependency получает:

- purpose;
- owner;
- license;
- maintenance status;
- security posture;
- replaceability;
- data/access scope;
- update policy;
- exit plan.

Dependency update выполняется небольшими PR через Renovate/Dependabot-подобный механизм, но:

- major updates не auto-merge;
- adapters обновляются отдельно;
- lockfiles обязательны;
- supply-chain metadata проверяется;
- abandoned dependency заменяется до превращения в blocker.

## 10.4. Feature/debt expiry

Feature flags, compatibility shims и temporary adapters имеют expiry review.

Technical debt записывается как:

```yaml
debt:
  id: DEBT-...
  introduced_by: WP-...
  reason: text
  consequence: text
  owner: module
  repayment_trigger: text
  deadline_or_review: date
  tests_protecting_behavior: []
```

`TODO someday` без owner и trigger запрещён.

## 10.5. Architecture fitness

CI измеряет:

- dependency cycles;
- forbidden imports;
- public API growth;
- build/test duration;
- flaky rate;
- warning count;
- unsafe code inventory;
- dependency age/security;
- module coupling;
- migration count/health;
- feature flag age;
- source file size только как сигнал, не закон;
- duplicate semantic implementations.

## 10.6. Документация как часть change

Изменение считается неполным, если оно делает ложными:

- README;
- AGENTS;
- architecture diagram;
- public API docs;
- runbook;
- test catalogue;
- traceability.

Документы не обновляются «потом отдельным проектом».

---

# 11. Releases и эксплуатационная эволюция

## 11.1. Каналы

- `dev` — каждый green main build;
- `nightly` — автоматические интеграционные сборки;
- `alpha` — ручное продвижение проверенного build;
- `beta` — ограниченный rollout;
- `stable` — только после release gates;
- позднее `lts`, если появляется реальная потребность.

## 11.2. Release gates

Stable release требует:

- green merge commit;
- reproducible/traceable build;
- signed artifacts;
- SBOM/provenance;
- migration rehearsal;
- backup/restore verification;
- critical test catalogue green;
- upgrade from supported previous version;
- rollback либо forward-fix plan;
- release notes;
- no unresolved R3 blocker;
- tested safe mode.

SLSA формализует уровни build provenance и защиты от tampering. Для alpha достаточно происхождения и подписанных hosted builds; публичный stable должен стремиться к hardened build platform. [[S15]]

## 11.3. Canary и staged rollout

Новая capability/backend сначала применяется:

- в development fixture;
- в shadow;
- в одном project profile;
- на одном device;
- на ограниченной доле calls;
- затем шире.

Rollback не должен требовать новой миграции, если она могла быть отложена.

## 11.4. Compatibility windows

Client/Head/Node/protocol поддерживают ограниченное окно версий. Слишком старый клиент:

- получает понятное сообщение;
- сохраняет local drafts;
- не отправляет неизвестные destructive commands;
- обновляется либо переходит read-only;
- не удаляет поля, которых не понимает.

---

# 12. Метрики качества процесса

Главные показатели:

- lead time Work Package;
- размер PR;
- время до первого green test;
- merge wait;
- escaped defect rate;
- rollback/revert rate;
- flaky test rate;
- main red time;
- rework percentage;
- number of open R3/R4 decisions;
- number/age feature flags;
- number/age compatibility shims;
- test duration;
- change failure rate;
- mean recovery time;
- agent token/cost per accepted Work Package;
- owner interruption count;
- percentage of Work Packages completed without scope expansion;
- reviewer finding recurrence;
- documentation drift incidents.

Не оптимизировать отдельно:

- LOC;
- commits;
- number of agents;
- raw token output;
- number of tests;
- coverage percentage без анализа meaningful behavior.

---

# 13. Антипаттерны

Стратегия считается нарушенной, если:

- один агент получает milestone как единственный неограниченный prompt;
- branch живёт неделями без регулярной интеграции;
- main ломается и работа продолжается;
- architecture decision спрятан в commit message;
- reviewer получает только объяснение автора, но не diff/tests;
- тест написан только после обнаружения bug и не связан с requirement;
- все providers протекают в domain types;
- feature flag не имеет expiry;
- два агента пишут в один mutable root;
- UI демонстрирует fake state без Head acknowledgement;
- release собирается на личном компьютере без provenance;
- dependency добавляется «для удобства» без exit plan;
- migration не тестируется на копии предыдущей версии;
- flaky test лечится бесконечным retry;
- технический долг существует только в комментариях;
- agent-generated PR настолько велик, что reviewer может лишь поверить summary;
- пользователь вынужден принимать низкоуровневые решения, которые должны были быть проверены автоматически.

---

# 14. Этапы внедрения самой стратегии

## Phase A — Foundation

- добавить implementation docs;
- определить work-package schema;
- включить branch protection;
- настроить fast PR checks;
- создать CODEOWNERS;
- создать test catalogue skeleton;
- выполнить первый vertical slice.

## Phase B — Controlled autonomy

- worktree per package;
- batch runner;
- stop/decision gates;
- detached review for R2+;
- merge queue;
- traceability validation.

## Phase C — Production quality

- deterministic simulation;
- migration matrix;
- backup/restore drills;
- signed releases/SBOM/provenance;
- canaries;
- architecture fitness dashboard.

## Phase D — Long-term evolution

- automated dependency updates;
- deprecation dashboard;
- feature flag expiry;
- technical debt reviews;
- quarterly architecture health review;
- provider/backend replacement drills.

---

# 15. Definition of Done стратегии

Стратегия реально действует, когда:

1. coding-agent никогда не начинает работу без Work Package;
2. каждый Work Package имеет acceptance tests и risk class;
3. один Work Package имеет один branch/worktree и writer;
4. main защищён автоматическими checks;
5. R2+ получает независимое review;
6. R3/R4 останавливаются на human gate;
7. тесты делятся по размеру и fast gate действительно быстрый;
8. branch-by-abstraction применяется к большим заменам;
9. feature flags имеют owner и expiry;
10. public contracts проходят compatibility checks;
11. releases имеют provenance и recovery plan;
12. owner может принять milestone по demo и evidence, не читая весь код;
13. новые модели/providers подключаются через adapter, не переписывая core;
14. через год новый агент может понять состояние проекта по репозиторию, а не по старым чатам;
15. плохой Work Package можно откатить без потери всего milestone.

---

# Appendix A. Source Ledger

## OpenAI и agent workflow

**[S01] OpenAI — Using GPT‑5.6 / Model guidance.** `gpt-5.6-sol`, reasoning efforts до `max`, persisted reasoning и multi-agent beta, аналогичная Ultra mode. Accessed 13 July 2026.  
https://developers.openai.com/api/docs/guides/latest-model

**[S02] OpenAI — Long-running work.** Ясный outcome, constraints, definition of done; связанная работа в одной task/session; отдельные tasks для независимой параллельной работы.  
https://learn.chatgpt.com/docs/long-running-work

**[S03] OpenAI Codex — Code review.** Review Git diff, detached reviewer, line-specific feedback и отдельная review model.  
https://learn.chatgpt.com/docs/code-review

**[S04] OpenAI Codex — Git worktrees.** Изолированные task environments, handoff, snapshot и lifecycle worktree.  
https://learn.chatgpt.com/docs/environments/git-worktrees

**[S05] OpenAI Codex — AGENTS.md.** Иерархия project instructions, root-to-directory overrides и ограничение размера.  
https://learn.chatgpt.com/docs/agent-configuration/agents-md

**[S06] OpenAI Codex — Agent approvals and security.** Sandbox/approval profiles и отделение model request от OS-enforced boundary.  
https://learn.chatgpt.com/docs/agent-approvals-security

## Delivery и integration

**[S07] DORA — Continuous Integration.** Small batches, быстрые tests, green main и немедленное исправление broken build.  
https://dora.dev/capabilities/continuous-integration/

**[S08] DORA — Trunk-based development.** Short-lived branches, частая интеграция и избегание тяжёлых merge/review cycles.  
https://dora.dev/capabilities/trunk-based-development/

**[S09] GitHub — Merge Queue.** Required checks на latest base и queued changes.  
https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue

**[S10] OpenFeature.** Provider-neutral feature flag API, canary, safe degradation и runtime evaluation.  
https://openfeature.dev/docs/reference/intro/

**[S11] Martin Fowler — Branch by Abstraction.** Постепенная замена implementation через abstraction и параллельную проверку.  
https://martinfowler.com/bliki/BranchByAbstraction.html

**[S12] GitHub — CODEOWNERS.** Path ownership и required owner reviews.  
https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners

## Testing и compatibility

**[S13] Google Testing Blog — Test Sizes.** Small/Medium/Large по enforceable ограничениям, независимость и parallelism.  
https://testing.googleblog.com/2010/12/test-sizes.html

**[S14] Cargo Book — SemVer Compatibility.** Совместимые и breaking изменения Rust API.  
https://doc.rust-lang.org/cargo/reference/semver.html

**[S15] SLSA.** Build provenance и уровни защиты supply chain.  
https://slsa.dev/spec/v1.2/

## Исследования AI coding

**[S16] Comparing AI Coding Agents: A Task-Stratified Analysis of Pull Request Acceptance.** Тип задачи сильнее части inter-agent differences; нет универсально лучшего агента. 2026 preprint.  
https://arxiv.org/abs/2602.08915

**[S17] SWE-PRBench.** AI code review обнаруживает ограниченную долю human findings; больше контекста может ухудшать review. 2026 preprint.  
https://arxiv.org/abs/2603.26130

**[S18] Rethinking the Value of Agent-Generated Tests.** Простое увеличение числа созданных агентом тестов не дало устойчивого улучшения outcomes; важна диагностическая ценность. 2026 preprint.  
https://arxiv.org/abs/2602.07900

**[S19] On the Impact of AGENTS.md Files on the Efficiency of AI Coding Agents.** Repository instructions были связаны со снижением runtime/token use при сопоставимом completion behavior. 2026 preprint.  
https://arxiv.org/abs/2601.20404

**[S20] Multi-Agent Systems Failures.** Failure taxonomy для agent systems: specification, coordination, verification и termination.  
https://arxiv.org/abs/2503.13657

---

# Финальная нормативная формула

> **Denet реализуется не длинными автономными рывками, а цепочкой малых проверяемых вертикальных срезов. GPT‑5.6 Sol получает максимально цельную задачу внутри Work Package, но не право молча менять продукт и архитектуру. Main остаётся зелёным, критические решения проходят риск-зависимые ворота, тесты и evidence важнее уверенности агента, а большие замены выполняются через сменные реализации и постепенную миграцию. Такая система позволяет ускоряться с новыми моделями, не превращая кодовую базу в одноразовый продукт конкретного поколения AI.**
