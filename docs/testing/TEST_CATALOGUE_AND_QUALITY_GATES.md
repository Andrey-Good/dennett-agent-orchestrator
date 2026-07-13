# Denet Test Catalogue and Quality Gates

**Статус:** каноническая стратегия каталога тестовых требований.  
**Цель:** сделать полноту тестирования видимой, трассируемой и пригодной для автоматического выбора coding-agents, не превращая один Markdown-файл в неуправляемую тысячу пунктов.

---

# 0. Итоговое решение

Denet действительно потребует порядка **850–1200 заранее специфицированных тестовых случаев** до выхода зрелой beta, а реальных исполняемых tests будет несколько тысяч за счёт unit/property/generated вариантов.

Но source of truth должен быть структурированным:

```text
tests/catalog/*.json или *.yaml
→ validator
→ generated TEST_CATALOGUE.md
→ generated COVERAGE_MATRIX.md
→ CI selection by tags/risk/module/milestone
```

Большой Markdown — удобное представление для человека, но не вручную редактируемый канонический источник.

---

# 1. Что является test case

Test Catalogue item описывает проверяемое обязательство, а не конкретную test function.

Один case может реализовываться:

- одним unit test;
- несколькими platform tests;
- property-based suite;
- deterministic scenario;
- manual UX evaluation;
- provider canary;
- backup drill.

---

# 2. Категории

## Product acceptance

Пользовательские outcomes и UX.

Целевой объём: 250–350.

## Domain and contract

State machines, ports, invariants, protocol behavior.

Целевой объём: 200–300.

## Integration

Processes, DB, object store, IPC, adapters.

Целевой объём: 100–150.

## Failure and recovery

Crash, timeout, duplicate, stale state, partition, disk full, restore.

Целевой объём: 100–150.

## Security and adversarial

Permissions, injection, secrets, imports, compromised provider, malicious project.

Целевой объём: 80–120.

## Client and accessibility

Desktop/mobile, keyboard, screen reader, large text, interruption, offline.

Целевой объём: 80–120.

## Migration, performance and operations

Upgrade, backup, scale, soak, cost, energy.

Целевой объём: 50–100.

Числа являются ориентиром покрытия, а не KPI ради количества.

---

# 3. Test case schema

```yaml
version: 1
id: TEST-EFFECT-UNKNOWN-004
title: Do not resend Telegram message after lost response
status: specified | automated | manual | quarantined | retired
priority: critical | high | normal | low
risk: R3

domains:
  - external-effects
  - connectors
levels:
  - contract
  - integration
  - scenario

requirement_refs:
  - SPEC-B:UnknownEffect
  - ARCH-81:EffectClaim
  - ARCH-82:ConnectorReconciliation

work_package_refs: []

preconditions:
  - fake connector accepts a stable idempotency key

stimulus:
  - dispatch message
  - provider commits send
  - response is lost
  - restart Head
  - reconcile

expected:
  - one logical send
  - state becomes UNKNOWN before reconciliation
  - state becomes CONFIRMED after provider lookup
  - no blind retry

evidence:
  - provider send count equals one
  - Effect Receipt exists

variants:
  platforms: [linux, windows]
  stores: [sqlite, postgres]

implementation:
  target_suite: tests/scenarios
  status: planned
  test_paths: []

execution:
  size: large
  hermetic: true
  network: fake-local
  timeout_seconds: 120
  required_on:
    - pull_request_when_affected
    - nightly

owner: effects
flaky_policy: blocking
```

---

# 4. Обязательные свойства каталога

## Уникальность

ID не переиспользуется.

## Трассируемость

Critical requirement должен иметь минимум один catalogue case.

## Независимость

Case не зависит от порядка других cases.

## Evidence

Expected формулируется через наблюдаемый результат.

## Отделение manual от automated

Manual UX test не маскируется как automated coverage.

## Variants

Один case может генерировать platform/storage/provider variants.

## Lifecycle

Retired case сохраняет причину и replacement.

---

# 5. Test levels и gates

## PR Fast Gate

- static;
- affected small;
- selected medium;
- architecture;
- schema/protocol;
- docs;
- security quick checks.

Цель: <=10 минут.

## Merge Queue Gate

PR checks на latest main + queued changes.

## Nightly

- all medium;
- deterministic simulations;
- database matrices;
- client component;
- migration fixtures;
- fuzz/property extended seeds.

## Weekly

- desktop/mobile E2E;
- hardware/provider canaries;
- backup restore subset;
- long-running soak;
- ambient resource tests.

## Release

- critical catalogue 100% green or explicit owner waiver;
- upgrade matrix;
- backup/full restore;
- security review;
- accessibility core flows;
- signed artifacts/provenance;
- no unknown external effect.

---

# 6. Property-based и model-based tests

Не описывать сотни ручных перестановок, если можно сформулировать invariant.

## Sync convergence

```text
Для одного набора допустимых операций любая доставка с reorder/duplicate/retry
должна приводить к одинаковому каноническому результату либо явному конфликту.
```

## Idempotency

```text
Повтор одной команды с тем же idempotency key и теми же parameters
не создаёт второй эффект; другие parameters отвергаются.
```

## Permission monotonicity

```text
Revoked grant не становится действующим из-за stale cache/offline replay.
```

## Deletion reachability

```text
После завершения deletion obligation объект не извлекается через любой
заявленный active index/cache/replica path.
```

## State machine

Генерировать invalid transitions и проверять отказ без mutation.

Property catalogue item содержит generator domains и oracle.

---

# 7. Test design до кода

Для R2+ package planner должен создать acceptance cases **до** implementation.

Implementer может добавить:

- unit tests;
- regression;
- edge variants;
- debug probes.

Но он не может считать собственный тест достаточным, если тест просто подтверждает выбранный implementation.

Хороший acceptance test:

- основан на requirement;
- переживает refactor;
- проверяет failure;
- имеет observable output;
- не знает private function names.

---

# 8. Flaky tests

Статусы:

- `stable`;
- `suspected_flaky`;
- `quarantined`;
- `fixed`;
- `retired`.

Quarantine требует:

- issue;
- owner;
- evidence;
- expiry;
- replacement coverage для critical requirement.

Retry используется для диагностики, но не превращает failing test в green evidence.

---

# 9. Coverage Matrix

Генерируемая матрица:

```text
Requirement
→ Architecture section
→ Work Package
→ Code root/owner
→ Test case
→ Automated test path
→ Release gate
```

CI ошибки:

- requirement без test;
- READY package с неизвестным test ID;
- merged package без automated/manual evidence;
- critical case без owner;
- retired test без replacement;
- test path не существует;
- public contract без compatibility case.

---

# 10. Доменные файлы каталога

```text
tests/catalog/
├── foundations.json
├── projects_sessions.json
├── agents_runs.json
├── memory_ingest.json
├── memory_retrieval.json
├── memory_deletion.json
├── trust_identity.json
├── external_effects.json
├── capabilities_adapters.json
├── voice.json
├── ambient_audio.json
├── screen_capture.json
├── sync_offline.json
├── backup_recovery.json
├── migration_updates.json
├── desktop.json
├── mobile.json
├── accessibility.json
├── performance.json
├── security.json
└── production_soak.json
```

Начать с 30–50 critical cases для Milestone 0–1. Каталог растёт перед каждым milestone, а не генерируется тысяча пустых формулировок заранее.

---

# 11. Приоритет каталогизации

Порядок:

1. data loss/security/external effects;
2. main vertical slice;
3. offline/restart;
4. protocol/migration;
5. memory correctness;
6. user control;
7. UI and accessibility;
8. performance/cost;
9. rare optional features.

---

# 12. Quality debt

Если test отсутствует временно:

```yaml
waiver:
  requirement: ...
  reason: ...
  risk: ...
  compensating_evidence: ...
  owner: ...
  expires_at: ...
```

Бессрочный waiver запрещён для critical cases.

---

# 13. Agent interaction

Coding-agent перед package:

- загружает required test cases;
- не загружает весь каталог;
- выполняет selectors по tags/module/risk;
- добавляет paths после automation;
- не меняет expected без spec gate;
- возвращает test evidence в Completion Packet.

Reviewer получает:

- required cases;
- actual results;
- missing variants;
- coverage delta.

---

# 14. Генерируемые документы

## TEST_CATALOGUE.md

Человекочитаемый список по domain/risk/status.

## COVERAGE_MATRIX.md

Трассируемость.

## RELEASE_GATES.md

Какие test IDs блокируют release channel.

## TEST_DEBT.md

Quarantined, waived, missing automation.

## MILESTONE_TEST_PLAN.md

Cases для текущего milestone.

Генерируемые файлы не редактируются вручную.

---

# 15. Definition of Done

Каталог считается рабочим, когда:

1. schema валидируется;
2. ID уникальны;
3. critical requirements покрыты;
4. Work Packages ссылаются на cases;
5. PR выбирает affected tests;
6. fast gate быстрый;
7. flake не скрывается retry;
8. manual tests видимы;
9. property tests используются для combinatorial state;
10. release gate генерируется;
11. owner видит, что именно ещё не доказано;
12. число test functions может расти без ручного изменения огромного Markdown.

---

# Финальная формула

> **Denet нужен не файл с тысячей галочек, а структурированный каталог проверяемых обязательств. Он связывает идеи, архитектуру, Work Packages, код и releases; позволяет агентам выбирать точные проверки; использует property-based generation там, где вариантов тысячи; и делает отсутствие доказательства таким же видимым, как failing test.**
