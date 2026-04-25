[English](#english) | [Russian](#russian)

<a id="english"></a>
# Stress And Regression Runbook

Status: runbook template plus deterministic local Stage 8 evidence guidance. It defines what must be measured before claiming release confidence; current completed evidence is limited to deterministic local stress/regression/recovery gates and does not claim live provider stress or production-scale load proof.

## Purpose

Use this runbook to gather stress and regression evidence for the release target. Stress proof checks behavior under realistic pressure and failure conditions. Regression proof checks that accepted contracts and previous integrated flows still behave as expected.

## Stress Coverage

Record planned and actual values for:

- concurrent graph runs;
- nested or managed child runs when in scope;
- provider request rate and retry behavior;
- storage write frequency and interruption timing;
- user prompts, blocked states, and resume events when in scope;
- active-runtime timeout boundaries, including startup, terminal wait, and resume-after-reply segments;
- memory read/write/search volume when in scope;
- cleanup, cancellation, and recovery volume.

## Regression Coverage

Regression evidence must include:

- contract/schema validation for release-scope agent definitions;
- core graph execution success and negative cases;
- outputs, outcomes, variables, persistence, resume, and interruption behavior;
- timeout regression for Codex App Server execution, catalog inspection, environment inspection, comment delivery, and prompt reply delivery;
- lifecycle draft/live/deploy identity where lifecycle is in scope;
- memory, runtime, interaction, builder, and managed-subagent regression suites only when those capabilities are included in release scope;
- previously accepted integrated product-flow acceptance scenarios that are still in scope.

## Execution Steps

1. Record baseline commit, configuration, machine or environment class, and provider limits.
2. Run narrow contract and unit checks first.
3. Run integration and accepted product-flow regression checks.
4. Run stress scenarios with increasing load until the planned target is reached or a blocking failure occurs.
5. Capture metrics for latency, error rate, retry count, storage consistency, finalization count, resource cleanup, and provider throttling.
6. Record every failure, timeout, blocked dependency, and inconclusive result in the evidence log.
7. Compare results with release criteria before changing the release decision record.

## Minimum Metrics

Each stress evidence item should record:

- run count and concurrency;
- duration;
- success, failure, timeout, cancellation, and retry counts;
- timeout codes by public operation, with confirmation that execution timeout is `runtime_error` rather than cancellation or interruption;
- p50, p95, and maximum observed latency when measurable;
- storage consistency checks;
- provider throttling and rate-limit events;
- memory growth, artifact growth, or cleanup debt when measurable;
- observed defects and owner routing.

## Deterministic Provider Matrix

Provider reliability matrix evidence may use local stub providers when the goal is to prove orchestration semantics rather than live-provider behavior. Such tests must gate on deterministic state transitions, outcome counts, active-execution drain, resume boundaries, and final-output integrity. They must not call live Codex, Mem0, embedding, LLM, or external providers, and they must not use absolute latency thresholds as pass/fail criteria.

Minimum deterministic matrix coverage:

- throttling-style provider failures such as `PROVIDER_RATE_LIMIT` are visible and resumable when mapped to `runtime_error`;
- transient runtime failures can resume to success without duplicate final output;
- interruption and waiting-for-user boundaries are not recorded as cancellation;
- bounded concurrent volume drains every active stub execution;
- no final output is exposed until all required successful nodes complete;
- memory provider failure coverage is linked to the accepted memory reliability or cleanup evidence instead of duplicated through live calls.

## Current Deterministic Local Stage 8 Coverage

The accepted Stage 8 local campaign is bounded to deterministic, offline evidence:

- stress/regression coverage uses counters, active-execution drain, persisted state, final-output integrity, controlled failure, interruption, and resume assertions instead of absolute wall-clock pass/fail gates;
- crash/reopen recovery is proven by closing a `SQLiteLocalStateStore`, reopening a fresh store over the same database, and verifying stale in-progress work cannot appear as committed success;
- SQLite pressure is proven with multiple same-file store instances, duplicate-active-attempt rejection, ordered boundary sequences, unique committed outputs, and consistent final snapshots across stores;
- event dispatch regression is proven with repeated and near-concurrent trigger dispatch records for success, runtime failure, and missing-live failure without corrupting lifecycle or graph state.

This coverage supports local CLI/repository confidence only. It is not live provider stress, provider throttling proof, production-scale/load proof, hosted deployment proof, or managed deployment proof.

## Pass Rules

Stress and regression evidence supports release only when:

- the run reaches the planned load or the release scope is narrowed;
- failures are expected, handled, and documented, not silent;
- state remains consistent after interruption, retry, cancellation, and cleanup;
- timed-out comments are not appended, and timed-out prompt replies remain resumable with the pending prompt intact;
- regression checks cover the release-scope capabilities;
- no unresolved release-blocking defect remains.

## Block Rules

Mark the release as blocked when:

- evidence cannot be reproduced or audited;
- hidden manual repair is required;
- final outputs are duplicated, lost, or inconsistent;
- persisted state cannot recover after interruption;
- provider throttling causes undefined behavior rather than controlled failure;
- regression coverage is missing for an included capability.

<a id="russian"></a>
# Runbook для stress и regression

Статус: шаблон runbook для доказательств выпуска. Он определяет, что должно быть измерено до заявления уверенности в выпуске; он не утверждает, что измерения прошли.

## Назначение

Используйте этот runbook, чтобы собрать stress и regression evidence для release target. Stress proof проверяет поведение под реалистичной нагрузкой и условиями отказа. Regression proof проверяет, что принятые contracts и предыдущие integrated flows все еще работают ожидаемо.

## Покрытие stress

Записывайте плановые и фактические значения для:

- concurrent graph runs;
- nested или managed child runs, если входят в scope;
- provider request rate и retry behavior;
- storage write frequency и interruption timing;
- user prompts, blocked states и resume events, если входят в scope;
- memory read/write/search volume, если входит в scope;
- cleanup, cancellation и recovery volume.

## Покрытие regression

Regression evidence должно включать:

- contract/schema validation для release-scope agent definitions;
- успешные и негативные случаи core graph execution;
- outputs, outcomes, variables, persistence, resume и interruption behavior;
- lifecycle draft/live/deploy identity, если lifecycle входит в scope;
- regression suites для memory, runtime, interaction, builder и managed-subagent только когда эти capabilities включены в release scope;
- ранее принятые integrated product-flow acceptance scenarios, которые все еще входят в scope.

## Шаги выполнения

1. Запишите baseline commit, configuration, machine или environment class и provider limits.
2. Сначала выполните узкие contract и unit checks.
3. Выполните integration и accepted product-flow regression checks.
4. Выполняйте stress scenarios с растущей нагрузкой, пока planned target не достигнут или не возникнет blocking failure.
5. Захватите metrics для latency, error rate, retry count, storage consistency, finalization count, resource cleanup и provider throttling.
6. Запишите каждый failure, timeout, blocked dependency и inconclusive result в evidence log.
7. Сравните результаты с release criteria до изменения release decision record.

## Минимальные метрики

Каждый stress evidence item должен записывать:

- run count и concurrency;
- duration;
- counts для success, failure, timeout, cancellation и retry;
- p50, p95 и maximum observed latency, когда измеримо;
- storage consistency checks;
- provider throttling и rate-limit events;
- memory growth, artifact growth или cleanup debt, когда измеримо;
- observed defects и owner routing.

## Правила pass

Stress и regression evidence поддерживает выпуск только когда:

- run достигает planned load или release scope сужен;
- failures ожидаемы, обработаны и задокументированы, а не silent;
- state остается consistent после interruption, retry, cancellation и cleanup;
- regression checks покрывают release-scope capabilities;
- не остается unresolved release-blocking defect.

## Правила block

Отмечайте выпуск как blocked, когда:

- evidence невозможно воспроизвести или проверить;
- требуется hidden manual repair;
- final outputs дублируются, теряются или inconsistent;
- persisted state не восстанавливается после interruption;
- provider throttling вызывает undefined behavior вместо controlled failure;
- regression coverage отсутствует для включенной capability.
