# Denet Coding-Agent Execution Protocol

**Статус:** канонический протокол работы coding-agents.  
**Версия:** 1.0  
**Цель:** дать сильному агенту достаточно свободы для качественной реализации, но не позволить длительной автономной работе разрушить архитектуру, скрыть ошибку или создать непроверяемый diff.

Читайте вместе с:

- [`00_IMPLEMENTATION_AND_EVOLUTION_STRATEGY.md`](00_IMPLEMENTATION_AND_EVOLUTION_STRATEGY.md);
- [`03_WORK_PACKAGE_SYSTEM.md`](03_WORK_PACKAGE_SYSTEM.md);
- корневым [`AGENTS.md`](../../AGENTS.md);
- ближайшим вложенным `AGENTS.md`;
- архитектурными томами 80–83.

---

# 0. Основной контракт

Coding-agent получает не абстрактную просьбу «развивай Denet», а один **Work Package** или заранее разрешённый **Autonomous Batch**.

Агент имеет право:

- изучать релевантную кодовую и документальную область;
- выбирать локальную реализацию внутри утверждённых контрактов;
- создавать private helpers;
- писать и исправлять тесты;
- рефакторить затронутую область, если это снижает сложность и не расширяет scope;
- делать checkpoints и commits;
- запускать разрешённые инструменты;
- предлагать ADR/decision request.

Агент не имеет права без явного gate:

- менять бизнес-семантику;
- менять источник истины;
- добавлять production dependency;
- делать breaking protocol/API migration;
- расширять permissions;
- выполнять реальные external effects;
- объединять несколько Work Packages в один огромный diff;
- исправлять unrelated failures «заодно»;
- объявлять completion только по собственному тексту;
- обходить failing tests;
- менять architecture volume задним числом, чтобы оправдать код.

---

# 1. Рекомендуемый профиль GPT‑5.6 Sol

## 1.1. Общий принцип

Высокий reasoning не означает, что агенту нужно давать больше scope. Он означает, что в пределах coherent scope агент может глубже исследовать причины, сравнить варианты и лучше проверить последствия.

## 1.2. Режимы

### Planning / Architecture

```text
model: gpt-5.6-sol
reasoning: max
mode: pro, если доступно и оправдано
multi-agent: off по умолчанию
```

Использовать для:

- Work Package decomposition;
- ADR;
- risk spike design;
- R3/R4 review;
- incident root cause.

### Implementation

```text
model: gpt-5.6-sol
reasoning: high или xhigh
persisted reasoning: только внутри текущего Work Package
```

### Detached review

```text
model: gpt-5.6-sol
reasoning: high/max по риску
fresh context: yes
implementer narrative: не давать до первого review pass
```

### Mechanical work

```text
model: gpt-5.6-sol
reasoning: medium
```

Если используется один интерфейс с режимом Ultra, Ultra разрешён только в случаях, описанных в разделе 11.

## 1.3. Reasoning lineage

Одна reasoning lineage соответствует:

- одному Work Package;
- одной ветке/worktree;
- одному набору assumptions;
- одному acceptance contract.

После merge, смены задачи или существенной смены цели создаётся новая lineage.

## 1.4. Что переносится между сессиями

Переносится:

- Work Package;
- commits;
- test results;
- ADR/decision requests;
- checkpoint summary;
- open risks;
- artifacts;
- exact current state.

Не переносится как authority:

- hidden chain-of-thought;
- уверенность модели без evidence;
- устные обещания из старого чата;
- незаписанные архитектурные предположения.

---

# 2. Входной пакет агента

Перед началом агент получает:

```yaml
agent_assignment:
  work_package_ref: WP-...
  repository_commit: sha
  worktree_path: path
  branch: agent/wp-...
  risk_class: R0 | R1 | R2 | R3 | R4
  canonical_docs: []
  local_agents_files: []
  allowed_roots: []
  forbidden_roots: []
  required_tests: []
  budget:
    max_wall_time: optional
    max_model_calls: optional
    max_parallel_helpers: integer
  autonomy_envelope_ref: optional
```

Если любой обязательный элемент отсутствует, агент не начинает semantic change. Он может выполнить read-only preflight и создать запрос на уточнение.

---

# 3. Жизненный цикл Work Package

```text
ASSIGNED
→ PREFLIGHT
→ BASELINE_VERIFIED
→ PLAN_READY
→ IMPLEMENTING
↔ CHECKPOINTED
→ VERIFYING
→ REVIEW_READY
→ CHANGES_REQUESTED | MERGE_READY
→ MERGED | STOPPED | SUPERSEDED
```

Это не глобальная Jira-машина для каждого шага. Статус хранится только для Work Package, который имеет реальную lifecycle value.

---

# 4. Preflight

Агент обязан выполнить до изменения кода.

## 4.1. Repository state

Проверить:

- текущий commit совпадает с assignment;
- worktree чистый либо содержит только declared changes;
- branch уникальна;
- ближайший `AGENTS.md` найден;
- generated files распознаны;
- нет чужого writer в том же mutable root;
- required tools доступны.

## 4.2. Требования

Агент формулирует своими словами:

- outcome;
- non-goals;
- authoritative state;
- главный инвариант;
- failure path;
- acceptance evidence.

Это записывается в work log кратко. Если формулировка противоречит Work Package, агент останавливается.

## 4.3. Baseline

Запустить минимальный набор:

- relevant unit tests;
- relevant contract tests;
- repository verification;
- compile/typecheck затронутого root.

Если baseline красный:

- сохранить exact failure;
- определить, связан ли он с текущим package;
- не начинать широкое исправление;
- либо создать маленький prerequisite package;
- либо запросить решение.

## 4.4. Dependency and schema impact

Отдельно отметить:

- новая dependency?;
- public API?;
- protocol/schema?;
- persistent data?;
- migration?;
- permissions/effect?;
- UI command?;
- feature flag?;
- observability?;
- documentation?;
- backup/recovery?

---

# 5. План реализации

План должен быть коротким и исполняемым.

Хороший план:

```text
1. Добавить domain value object и инварианты.
2. Добавить port, не меняя существующий adapter.
3. Реализовать fake adapter и contract tests.
4. Подключить use case к существующему Composition Root за выключенным flag.
5. Проверить normal/failure/cancel paths.
6. Обновить локальный README/AGENTS при изменении public surface.
```

Плохой план:

```text
1. Полностью реализовать подсистему.
2. Исправить всё, что сломается.
3. Оптимизировать.
```

План не должен заранее дробить каждую функцию на отдельного subagent.

---

# 6. Реализация

## 6.1. Сначала narrowest passing path

Агент создаёт минимальный путь, который:

- проходит через реальный public contract;
- возвращает корректный result;
- имеет fake/in-memory dependency;
- проверяется тестом;
- не притворяется production-ready.

Затем заменяет fakes/заглушки реальными adapters только если это входит в Work Package.

## 6.2. Functional core, imperative shell

Решения по возможности отделяются от эффекта:

```text
facts + state + policy
→ decision
→ proposed actions
→ adapter execution
→ observed outcome
→ state transition
```

Pure decision должен тестироваться без provider, filesystem или clock.

## 6.3. Ошибка не лечится расширением scope

Если найден unrelated bug:

1. воспроизвести;
2. записать issue/work-package candidate;
3. определить, блокирует ли он текущую работу;
4. исправить только если blocking и изменение мало;
5. иначе оставить отдельно.

## 6.4. Код должен быть понятен без чата

Агент проверяет:

- названия отражают domain;
- public type документирован;
- неочевидный invariant объяснён;
- error содержит actionable context;
- magic constants имеют owner/config;
- нет скрытого singleton/service locator;
- тест объясняет behavior;
- сложность не вынесена в бесформенный `utils`.

## 6.5. Не создавать speculative abstractions

Новый generic abstraction оправдан, если:

- есть минимум две реальные реализации или запланированная migration;
- contract устойчив;
- abstraction уменьшает coupling;
- tests можно написать на port;
- она не скрывает важные provider-specific differences.

---

# 7. Checkpoints и commits

## 7.1. Когда commit нужен

- baseline/fixture подготовлен;
- domain contract готов;
- first path проходит;
- adapter подключён;
- regression исправлена;
- review findings закрыты.

## 7.2. Commit message

```text
<type>(<scope>): <observable change>

Why:
- ...

Evidence:
- TEST-...
- command ... passed

Work-Package: WP-...
```

## 7.3. Checkpoint summary

```yaml
checkpoint:
  work_package: WP-...
  commit: sha
  achieved: []
  tests_passed: []
  current_risk: text
  remaining: []
  changed_assumptions: []
```

Checkpoint не заменяет commit и не хранится только в model conversation.

---

# 8. Verification

## 8.1. Порядок

1. focused Small tests;
2. module Small suite;
3. affected Medium contract/integration;
4. architecture/static checks;
5. package-specific scenario;
6. full PR gate;
7. Large/canary только если risk требует.

## 8.2. Нельзя менять тест под ошибочный код

Если test отражает каноническую спецификацию, агент не может просто переписать expected result. Он должен:

- доказать, что спецификация изменилась;
- получить нужный gate;
- обновить requirement;
- только затем изменить test.

## 8.3. Negative evidence

Агент обязан проверить хотя бы один сценарий, где действие не должно происходить:

- permission deny;
- stale revision;
- cancellation;
- duplicate event;
- missing capability;
- invalid package;
- timeout/UNKNOWN;
- offline refusal;
- wrong scope.

## 8.4. Self-review

Перед detached review агент читает полный diff как reviewer и проверяет:

- scope creep;
- duplication;
- dead code;
- missing errors;
- panic/unwrap;
- secret/logging;
- migration;
- compatibility;
- cancellation;
- test quality;
- docs.

---

# 9. Review protocol

## 9.1. Review input

Detached reviewer получает:

```yaml
review_packet:
  work_package: WP-...
  risk_class: R...
  requirement_refs: []
  base_commit: sha
  head_commit: sha
  diff_scope: []
  required_invariants: []
  tests_and_results: []
  known_limitations: []
```

## 9.2. Первый pass без авторского rationale

Reviewer сначала читает:

- requirement;
- code/diff;
- tests;
- architecture boundary.

После первичных findings можно открыть rationale автора.

## 9.3. Finding format

```yaml
finding:
  severity: blocker | high | medium | low | note
  location: path:line
  violated_requirement: optional
  issue: text
  consequence: text
  evidence: text
  minimal_fix: text
```

## 9.4. Review не генерирует шум

Не создавать finding для:

- личного stylistic preference, если formatter/lint уже определяет стиль;
- hypothetical future, не входящего в contract;
- изменения, которое не улучшает correctness/clarity/maintenance;
- «можно было бы» без measurable consequence.

## 9.5. Review gate по риску

- R0: automated checks;
- R1: self-review, optional detached;
- R2: detached review required;
- R3: detached adversarial review + owner approval;
- R4: owner decision before implementation and final acceptance.

---

# 10. Completion packet

Агент завершает пакет только с:

```yaml
work_package_result:
  id: WP-...
  status: completed | partial | blocked | superseded
  summary: text
  commits: []
  changed_files: []
  requirements_satisfied: []
  tests:
    passed: []
    not_run: []
    failed: []
  review:
    findings_closed: []
    findings_open: []
  migrations: []
  observability_changes: []
  known_limitations: []
  follow_up_candidates: []
  owner_decision_required: false
```

`completed` запрещён, если required test не выполнен и нет явного waiver.

---

# 11. Multi-agent / Ultra protocol

## 11.1. Marginal utility gate

Перед spawn helper основной агент отвечает:

- что helper создаст независимо?;
- нужен ли ему весь context?;
- какой файл/resource он пишет?;
- как проверяется result?;
- сколько context дублируется?;
- можно ли сделать это tool call/search instead?;
- каков deadline?;
- кто интегрирует?

Если ответы неясны, helper не создаётся.

## 11.2. Разрешённые topology

### One lead + one bounded helper

Default для:

- research;
- platform-specific check;
- independent test design;
- review.

### Independent alternatives

Несколько agents предлагают целостные варианты, не редактируя main branch.

### Sharded implementation

Только если:

- roots не пересекаются;
- contracts уже стабильны;
- один integration owner;
- отдельные worktrees;
- test ownership разделён.

## 11.3. Запрещено

- full mesh chatter;
- agents передают permissions текстом;
- два writers одного branch;
- subagent принимает R4 decision;
- helper работает без deadline/budget;
- reviewer сразу правит branch implementer;
- Ultra используется для микрозадачи.

## 11.4. Helper result

```yaml
contribution:
  assignment: text
  result: text
  artifacts: []
  evidence: []
  assumptions: []
  unresolved: []
  valid_for_revision: sha_or_turn
  confidence: bounded
```

---

# 12. Autonomous Batch protocol

## 12.1. Batch admission

Batch разрешён, если:

- Work Packages образуют последовательность или независимый набор;
- все READY;
- ни один не превышает max risk;
- между ними нет decision gate;
- merge/branch policy ясна;
- required tools доступны;
- stop policy определена.

## 12.2. Исполнение

```text
load batch
→ select next eligible package
→ create worktree
→ execute package protocol
→ verify
→ review by risk
→ merge or hold
→ refresh main
→ continue
```

## 12.3. Агент не продолжает, если

- merge не завершён;
- main red;
- previous package partial;
- next package assumptions изменились;
- test catalogue coverage отсутствует;
- owner decision pending;
- budget/timeout reached;
- repeated failure threshold reached.

## 12.4. Progress reporting

Не писать пользователю каждые пять минут. Создавать update:

- package started;
- package completed;
- blocker;
- decision needed;
- CI failed;
- batch finished.

Update содержит факт и следующий шаг, а не внутренний поток мыслей.

---

# 13. Stop and escalation conditions

Агент обязан остановиться при:

## Specification conflict

Два канонических документа требуют разное.

## Architecture leak

Реализация требует forbidden dependency или direct DB/provider access.

## Hidden data loss

Migration/delete/cleanup может потерять данные без recovery.

## Unbounded diff

Предполагаемое изменение превысило Work Package по roots или смыслу.

## Security boundary

Нужно расширить grant, secret scope, external effect или ambient collection.

## Non-reproducible failure

Баг нельзя стабильно воспроизвести после двух осмысленных попыток.

## False green

Тест проходит, но не проверяет заявленное behavior.

## Tool uncertainty

Сторонний tool/provider ведёт себя иначе, чем documented contract.

## User-value ambiguity

Есть несколько заметно разных UX/product outcomes.

## Cost/lock-in

Новая зависимость создаёт плату, hosted dependency или migration risk.

---

# 14. Context management

## 14.1. Минимальный context

Агент загружает:

1. Work Package;
2. root и nearest AGENTS;
3. конкретные architecture sections;
4. public traits/schemas;
5. tests;
6. похожий implementation.

Не загружать все 15 больших документов без причины.

## 14.2. Context refresh

Перед крупным изменением и после compaction:

- перечитать Work Package;
- проверить current commit;
- открыть current public contracts;
- сверить open findings;
- обновить checkpoint.

## 14.3. Stable references

В work log использовать:

- file anchors;
- test IDs;
- ADR IDs;
- schema names;
- commits;
- Work Package ID.

Не ссылаться только на «как обсуждали выше».

---

# 15. Готовые команды пользователя

## 15.1. Выполнить один пакет

```text
Работай по docs/implementation/01_AGENT_EXECUTION_PROTOCOL.md.
Выполни Work Package <ID> в отдельном worktree.
Не расширяй scope и остановись на любом decision gate.
Сначала проверь baseline, затем реализуй минимальный вертикальный результат,
выполни required tests, проведи self-review и верни Completion Packet.
```

## 15.2. Автономный batch

```text
Выполни Autonomous Batch <ID> строго по плану.
Продолжай к следующему Work Package только после green gate предыдущего.
Не принимай R3/R4 решения, не добавляй production dependencies,
не меняй public contracts и не выполняй внешние эффекты без отдельного разрешения.
Остановись при первом blocker/decision gate или после лимита batch.
```

## 15.3. Независимое ревью

```text
Проведи detached review Work Package <ID>.
Сначала не читай rationale implementer-а.
Проверь requirement, diff, tests, failure paths, compatibility и architecture boundaries.
Не изменяй branch. Верни findings в структурированном формате.
```

---

# 16. Definition of Done протокола

Протокол работает, если:

1. любой agent run связан с Work Package;
2. пакет начинается с baseline;
3. branch/worktree изолирован;
4. scope не расширяется молча;
5. high reasoning применяется по сложности, а не всегда;
6. Ultra создаёт только независимые contributions;
7. checkpoints существуют вне model context;
8. tests и evidence обязательны;
9. R2+ получает detached review;
10. R3/R4 останавливаются на owner gate;
11. completion packet позволяет продолжить другой моделью;
12. batch останавливается на красном main или decision;
13. code остаётся понятным без transcript;
14. каждый merge можно связать с requirement и test.

---

# Финальная формула

> **Агент Denet получает свободу внутри цельного Work Package, но не неограниченную власть над проектом. GPT‑5.6 Sol используется как сильный инженер: он планирует, реализует, проверяет и объясняет, однако канонические документы задают смысл, tests и runtime задают реальность, detached review снижает самоослепление, а owner принимает только действительно продуктовые и труднообратимые решения.**
