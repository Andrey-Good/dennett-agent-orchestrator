# Dennett Work Package and Autonomous Batch System

**Статус:** каноническая модель планирования исполняемой работы.  
**Цель:** превратить архитектурный roadmap в очередь ограниченных, проверяемых и переносимых заданий, которые coding-agents могут выполнять без повторного проектирования системы.

---

# 0. Почему нужен отдельный Work Package System

Roadmap отвечает, *куда* развивается продукт. Work Package отвечает, *что именно агент должен изменить сейчас*, на каких условиях и как доказать готовность.

Без него агент вынужден самостоятельно:

- определять границы задачи;
- восстанавливать требования;
- выбирать, что считать завершением;
- придумывать тесты;
- решать, можно ли менять архитектуру;
- угадывать, когда остановиться.

Это допустимо для небольшого проекта, но опасно для Dennett.

Короткая формула:

> **Work Package — не тикет с одной фразой, а компактный исполняемый контракт между продуктом, архитектурой, агентом и проверками.**

---

# 1. Иерархия планирования

```text
Program
└── Milestone
    ├── Vertical Slice
    │   ├── Work Package
    │   └── Work Package
    ├── Risk Spike
    └── Acceptance Gate
```

## 1.1. Milestone

Milestone имеет:

- user/system capability;
- demo path;
- non-goals;
- dependency milestones;
- vertical slices;
- release state;
- owner acceptance;
- rollback/continuation.

## 1.2. Vertical Slice

Vertical Slice связывает реальный путь через несколько слоёв. Он не обязан быть одним PR.

## 1.3. Work Package

Work Package является единицей branch/worktree, review и merge.

## 1.4. Risk Spike

Spike отвечает на вопрос и не обязан создавать production code. Его output:

- measurements;
- prototype;
- comparison;
- recommendation;
- ADR candidate;
- rejected assumptions.

## 1.5. Autonomous Batch

Batch — заранее разрешённая последовательность Work Packages, которую агент может выполнять без ответа пользователя между каждым пакетом.

---

# 2. Идентификаторы

Форматы:

```text
M00             milestone
VS-M00-01       vertical slice
WP-M00-001      work package
RS-M00-001      risk spike
AB-M00-001      autonomous batch
DEC-0001        decision request
DEBT-0001       technical debt
TEST-...        test catalogue item
ADR-0001        architecture decision
```

ID никогда не переиспользуется после удаления или supersede.

---

# 3. Статусы

## 3.1. Work Package

```text
DRAFT
→ REFINED
→ READY
→ IN_PROGRESS
→ VERIFYING
→ REVIEW
→ MERGE_READY
→ MERGED
```

Дополнительные terminal/interrupt states:

- `BLOCKED`;
- `STOPPED`;
- `SUPERSEDED`;
- `CANCELLED`;
- `PARTIAL`.

## 3.2. Ready Definition

Пакет получает `READY`, если:

- outcome однозначен;
- non-goals заданы;
- canonical refs существуют;
- dependencies закрыты;
- allowed roots определены;
- risk class назначен;
- required tests известны;
- acceptance evidence определено;
- нет unresolved Red decision;
- baseline environment доступна.

## 3.3. Done Definition

Пакет получает `MERGE_READY`, если:

- implementation завершена;
- required tests прошли;
- review gate пройден;
- docs/schema/ADR обновлены;
- completion packet создан;
- known limitations записаны;
- merge не создаёт скрытую незавершённость без flag.

---

# 4. Обязательные поля Work Package

```yaml
version: 1
id: WP-M01-001
title: Establish local project-chat command path
status: READY
milestone: M01
vertical_slice: VS-M01-01
risk: R2

outcome: >
  A desktop command reaches dennett-node and embedded Head and returns a streamed
  fake-agent response that survives UI restart.

non_goals:
  - real cloud provider
  - production memory retrieval
  - mobile support

requirement_refs:
  - SPEC-20:ProjectSession
  - ARCH-80:FirstVerticalSlice
  - ARCH-83:Milestone1

architecture_refs:
  - docs/architecture/80_Dennett_System_Architecture_and_Runtime_Topology.md
  - docs/architecture/83_Dennett_Client_Operations_Testing_and_Implementation_Blueprint.md

allowed_roots:
  - apps/desktop
  - services/node
  - services/head
  - crates/dennett-contracts

forbidden_roots:
  - crates/dennett-memory-core
  - crates/dennett-trust-core

state_ownership:
  reads:
    - local project session cache
  writes:
    - head session state through public API

contract_changes:
  public_api: true
  protocol: true
  persistent_schema: false
  migration: false

security_effects:
  permission_change: false
  external_effect: false
  secret_access: false

acceptance:
  - TEST-DESKTOP-CHAT-001
  - TEST-IPC-WATCH-001
  - TEST-RESTART-SESSION-001

required_commands:
  - python tools/verify_repo.py
  - cargo test -p dennett-contracts
  - cargo test -p dennett-node

observability:
  - trace desktop command to head result

rollback:
  - disable command route behind feature flag

owner_decision:
  required_before_start: false
  required_before_merge: false

estimate:
  target_agent_hours: 4-8
  max_diff_lines: 1200

notes: []
```

---

# 5. Смысл полей

## Outcome

Один проверяемый результат, а не список действий.

## Non-goals

Явно предотвращают scope creep.

## Requirement refs

Ссылки на бизнес-логику и acceptance.

## Architecture refs

Только релевантные разделы, не весь комплект.

## Allowed/forbidden roots

Граница записи. Read-only исследование соседних областей допустимо, если не нарушает privacy/security.

## State ownership

Показывает, кто канонически изменяет state.

## Contract changes

Определяет compatibility/review gates.

## Security/effects

Указывает, требуется ли Trust/Effect review.

## Acceptance

Список test IDs, а не общая фраза «добавить тесты».

## Estimate

Estimate используется как сигнал scope. Это не обещание точного времени.

---

# 6. Правила размера

## 6.1. Good package

- один outcome;
- coherent mental model;
- один основной writer;
- несколько часов или один рабочий день агента;
- один reviewable diff;
- independent acceptance;
- bounded failure modes.

## 6.2. Split package, если

- затрагивается более трёх bounded roots;
- есть две независимые migrations;
- UI и backend могут быть полезно интегрированы через fake contract отдельно;
- review требует разных specialists;
- один пакет имеет R1 и R4 части;
- acceptance можно разделить на independently passing slices;
- diff превышает разумный review threshold.

## 6.3. Не split, если

- части не имеют самостоятельного результата;
- workers потеряют общую идею;
- интеграция сложнее реализации;
- каждый child требует весь контекст;
- shared mutable files неизбежны;
- невозможно проверить child отдельно.

---

# 7. Dependency graph

Work Package dependencies типизированы:

- `requires_contract`;
- `requires_code`;
- `requires_decision`;
- `requires_spike`;
- `requires_test_fixture`;
- `requires_environment`;
- `requires_release`;
- `conflicts_with`.

Цикл является ошибкой planning.

Agent выбирает следующий package только если все hard dependencies закрыты.

---

# 8. Risk classification в пакете

Risk определяется не количеством файлов, а потенциальным ущербом.

Factors:

- data loss;
- security;
- external effect;
- public API;
- protocol;
- persistent schema;
- concurrency;
- offline/sync;
- migration;
- difficult rollback;
- user-facing semantics;
- cross-platform behavior;
- provider lock-in.

Автоматическая подсказка может предложить risk, но final risk для R2+ фиксируется planner/reviewer.

---

# 9. Work Package creation protocol

## 9.1. От milestone к slices

Planner сначала определяет 2–6 вертикальных paths.

## 9.2. От slice к packages

Каждый package создаёт следующий работающий шаг.

## 9.3. Test-first mapping

До READY создаются или выбираются acceptance IDs.

## 9.4. Architecture consistency

Проверяется:

- public ports;
- ownership;
- process boundary;
- data flow;
- cancellation;
- error/recovery;
- observability.

## 9.5. Owner gates

Red decisions выносятся до READY.

---

# 10. Autonomous Batch

## 10.1. Формат

```yaml
version: 1
id: AB-M01-001
title: Complete local conversation foundation
status: READY
packages:
  - WP-M01-001
  - WP-M01-002
  - WP-M01-003

execution:
  order: dependency
  max_parallel: 1
  stop_after_packages: 3
  merge_after_each: true
  require_green_main: true

limits:
  max_risk: R2
  new_production_dependencies: forbidden
  architecture_changes: forbidden
  external_effects: forbidden
  public_breaking_changes: forbidden

stop_conditions:
  - decision_required
  - test_failure_after_two_repair_attempts
  - scope_expansion
  - main_red
  - merge_conflict
  - budget_exhausted

reporting:
  on_package_start: true
  on_package_end: true
  on_decision: true
  periodic_noise: false
```

## 10.2. Batch sizing

Первоначально:

- 1–3 packages;
- один writer;
- max R1/R2;
- без migrations/effects;
- несколько часов.

После зрелого CI:

- 3–8 packages;
- независимые worktrees;
- detached review;
- merge queue;
- до одного дня автономии.

## 10.3. Нельзя включать

- unresolved R4;
- два packages, пишущих один root параллельно;
- реальный payment/message/delete;
- release stable;
- key recovery change;
- ambient privacy default;
- destructive migration;
- неоценённую dependency.

---

# 11. Decision Request

```yaml
id: DEC-0042
work_package: WP-M05-014
severity: yellow | red
question: text
context: text
options:
  - id: A
    description: text
    benefits: []
    costs: []
    risks: []
  - id: B
    description: text
recommendation: B
reason: text
default_if_deferred: stop | option_id | safe_noop
needed_by: date_or_milestone
```

Агент не должен присылать вопрос без вариантов, если варианты можно разумно сформировать.

---

# 12. Completion evidence

Work Package связывается с:

- commits;
- PR;
- test results;
- review findings;
- demo artifact;
- ADR/spec delta;
- telemetry screenshot/report;
- migration rehearsal;
- release containing change.

Трассируемость:

```text
Requirement
→ Work Package
→ Code owner/root
→ Commit/PR
→ Test IDs
→ Build/Release
```

---

# 13. Планирование сотен пакетов

Полный backlog Dennett может содержать 300–600 Work Packages. Его не следует генерировать полностью один раз и заморозить.

Правильный rolling horizon:

- ближайший milestone: детально READY;
- следующий milestone: REFINED;
- два следующих: DRAFT epics/slices;
- дальний roadmap: capabilities и dependencies.

Причины:

- risk spikes изменят решения;
- модели и providers изменятся;
- UX feedback изменит приоритет;
- слишком детальный дальний план устареет;
- агент начнёт выполнять устаревшие предположения.

После каждого milestone:

1. обновить architecture evidence;
2. пересмотреть roadmap;
3. детализировать следующий milestone;
4. удалить/объединить устаревшие packages;
5. обновить test catalogue;
6. сформировать autonomous batches.

---

# 14. Хранение в репозитории

```text
planning/
├── README.md
├── milestones/
│   ├── M00_repository_and_contracts.json
│   └── ...
├── work-packages/
├── batches/
├── decisions/
├── debt/
└── templates/
```

Machine-readable files являются source of truth для статусов. Markdown views могут генерироваться.

Git history хранит изменения плана. Нельзя использовать отдельный закрытый task manager как единственное место, где существует roadmap.

---

# 15. Валидация planning

CI должен проверять:

- уникальные IDs;
- schema;
- dependency cycles;
- missing refs;
- READY package без tests;
- R3/R4 без owner gate;
- batch с risk выше envelope;
- overlapping writers в parallel batch;
- obsolete feature flags/debt review dates;
- merged package без completion evidence;
- test ID без catalogue entry.

---

# 16. Пример decomposition

Milestone: Managed Runs survive restart.

```text
VS-1 Persist minimal Run state
  WP-1 Domain state and transitions
  WP-2 Persistence port + SQLite fake/adapter

VS-2 Restart and resume
  WP-3 Checkpoint contract
  WP-4 Node restart reconstruction
  WP-5 Fake runtime resume

VS-3 User control
  WP-6 Cancel/pause command path
  WP-7 Radar projection
  WP-8 Desktop state/demo

RS-1 Crash-point simulation

Gate
  restart during tool call
  no duplicate effect
  visible recovery
```

Неудачная decomposition:

```text
WP create enum
WP create table
WP add button
WP add log
WP add test
```

Она разделяет единый outcome на бессмысленные fragments.

---

# 17. Definition of Done системы пакетов

1. агент может начать package без старого чата;
2. пакет имеет один outcome;
3. scope и roots ограничены;
4. risk class определяет review;
5. acceptance test IDs существуют до implementation;
6. READY действительно исполняем;
7. batch имеет stop conditions;
8. dependencies не цикличны;
9. completion evidence машиночитаемо;
10. план хранится в Git;
11. дальний backlog не притворяется точным;
12. owner видит только Yellow/Red decisions;
13. coding-agent не выбирает случайную следующую задачу;
14. каждый merge связан с package;
15. package можно supersede без удаления истории.

---

# Финальная формула

> **Work Package System превращает Dennett из большой идеи в исполняемую программу разработки. Агент получает цельный результат, границы, тесты и право локально думать; пользователь получает только важные решения; repository получает трассируемый прогресс вместо набора длинных сессий и огромных непроверяемых веток.**
