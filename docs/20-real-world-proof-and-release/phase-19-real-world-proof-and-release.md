[English](#english) | [Russian](#russian)

<a id="english"></a>
# Phase 19 Real-World Proof And Release

Status: owner note for the Phase 19 release-readiness evidence slice.

## Goal

Phase 19 turns completed product surfaces into externally credible proof. The goal is to run realistic end-to-end flows against real runtimes and providers, preserve stress and regression evidence, document operational procedures, and make an explicit release decision based on evidence.

The next product-readiness pass uses [Release Scope Lock](./release-scope-lock.md) as the canonical boundary. Its selected release target is `local-cli-repository-readiness`, and the current final decision remains `defer` until later evidence proves every included capability at that boundary.

Phase 19 is not complete because a feature is implemented, a local test passes, or a dry run succeeds. It is complete only when the evidence log and release decision record show that the selected release target is either safe to release, blocked, or deliberately deferred with named residual risk.

## Evidence Model

Each evidence item must record:

- `id`: stable identifier, for example `P19-LIVE-001`.
- `type`: one of `live-proof`, `stress`, `regression`, `operational`, `security`, `compatibility`, or `manual-review`.
- `target`: runtime, provider, CLI flow, storage path, deployment path, or user workflow under proof.
- `environment`: local, staging, production-like, external provider account, or another named environment.
- `version`: commit SHA, package version, schema version, and relevant provider/runtime version where available.
- `operator`: role or redacted handle of the person or automation that ran the proof.
- `started_at` and `ended_at`: timestamps with timezone.
- `inputs`: redacted command, fixture, agent definition, provider account class, and configuration.
- `result`: one of `pass`, `fail`, `blocked`, `inconclusive`, or `not-run`.
- `decision_effect`: one of `supports-release`, `blocks-release`, `supports-defer`, or `informational`.
- `artifacts`: links or paths to logs, transcripts, screenshots, metrics, reports, or captured outputs.
- `redactions`: what was removed or masked, especially account identifiers, API keys, user data, and provider-specific PII.
- `residual_risk`: what the evidence does not prove.
- `follow_up`: required owner action when result is not a clean pass.

Evidence must be durable enough for a reviewer to rerun or audit it without access to hidden memory. Secrets and user PII must not be stored in the evidence log.

## Release Outcomes

The release decision must choose exactly one outcome:

- `release`: evidence satisfies the release criteria and residual risks are accepted.
- `block`: one or more unresolved defects, missing proofs, operational gaps, or external readiness failures prevent release.
- `defer`: the release proceeds only after explicitly removing or disabling a capability, target, or scenario from the release scope.

`defer` is not a softer word for `release`. It must name what is removed from scope, who owns the follow-up, and what user-visible limitation remains.

## Required Proof Categories

Phase 19 requires evidence in these categories before a release decision can be recorded:

- live end-to-end proof against real runtime and provider dependencies selected for the release target;
- stress evidence that exercises realistic concurrency, retries, interruptions, storage writes, and provider failure behavior;
- regression evidence that covers core contracts, previously accepted integrated flows, and release-blocking bug classes;
- operational evidence for setup, configuration, secrets handling, incident response, recovery, rollback, and support handoff;
- final human review that checks whether the evidence actually supports the requested release scope.

If a category cannot be run, the decision must be `block` unless the release scope is narrowed and the missing category is explicitly recorded as `defer`.

## Non-Goals

Phase 19 does not:

- invent new product scope to make a proof pass;
- convert dry-run evidence into live provider proof;
- hide blocked or inconclusive runs;
- claim provider reliability beyond the providers, accounts, regions, limits, and dates that were actually tested;
- certify security, compliance, or production operations beyond the evidence captured here;
- replace subsystem owner documents for runtime, memory, interaction, lifecycle, builder, or managed subagent behavior.

## Completion Criteria

Phase 19 is complete only when:

- required runbooks have been executed or explicitly marked blocked with evidence;
- the evidence log contains all release-relevant pass, fail, blocked, and inconclusive runs;
- operational procedures are sufficient for a new operator to run, observe, recover, and report the release target;
- the release decision record names `release`, `block`, or `defer` and explains why;
- any deferred scope is visible to users, maintainers, and future roadmap planning.

<a id="russian"></a>
# Phase 19: реальное доказательство и выпуск

Статус: владелец заметки для среза Phase 19 по доказательствам готовности к выпуску.

## Цель

Phase 19 превращает завершенные продуктовые поверхности во внешне убедимые доказательства. Цель состоит в том, чтобы выполнить реалистичные end-to-end потоки с реальными runtime и providers, сохранить stress и regression evidence, задокументировать операционные процедуры и принять явное решение о выпуске на основе доказательств.

Следующий product-readiness pass использует [Release Scope Lock](./release-scope-lock.md) как каноническую границу. Выбранная цель выпуска - `local-cli-repository-readiness`, а текущее финальное решение остается `defer`, пока более поздние доказательства не докажут каждую included capability в этой границе.

Phase 19 не считается завершенной только потому, что функция реализована, локальный тест проходит или dry run успешен. Она завершена только тогда, когда evidence log и release decision record показывают, что выбранная цель выпуска либо безопасна для выпуска, либо заблокирована, либо намеренно отложена с названным остаточным риском.

## Модель доказательств

Каждый evidence item должен фиксировать:

- `id`: стабильный идентификатор, например `P19-LIVE-001`.
- `type`: одно из `live-proof`, `stress`, `regression`, `operational`, `security`, `compatibility` или `manual-review`.
- `target`: runtime, provider, CLI flow, storage path, deployment path или пользовательский workflow, который доказывается.
- `environment`: local, staging, production-like, external provider account или другая именованная среда.
- `version`: commit SHA, package version, schema version и релевантная версия provider/runtime, если доступна.
- `operator`: роль или отредактированный идентификатор человека либо автоматизации, выполнившей proof.
- `started_at` и `ended_at`: timestamps с timezone.
- `inputs`: отредактированные command, fixture, agent definition, provider account class и configuration.
- `result`: одно из `pass`, `fail`, `blocked`, `inconclusive` или `not-run`.
- `decision_effect`: одно из `supports-release`, `blocks-release`, `supports-defer` или `informational`.
- `artifacts`: ссылки или пути к logs, transcripts, screenshots, metrics, reports или captured outputs.
- `redactions`: что было удалено или замаскировано, особенно account identifiers, API keys, user data и provider-specific PII.
- `residual_risk`: что это доказательство не доказывает.
- `follow_up`: обязательное действие владельца, если результат не является чистым `pass`.

Доказательство должно быть достаточно долговечным, чтобы reviewer мог повторить или проверить его без доступа к скрытой памяти. Secrets и пользовательская PII не должны сохраняться в evidence log.

## Результаты выпуска

Решение о выпуске должно выбрать ровно один результат:

- `release`: доказательства удовлетворяют критериям выпуска, а остаточные риски приняты.
- `block`: один или несколько unresolved defects, missing proofs, operational gaps или external readiness failures запрещают выпуск.
- `defer`: выпуск продолжается только после явного удаления или отключения capability, target или scenario из release scope.

`defer` не является более мягким словом для `release`. Он должен назвать, что удалено из scope, кто владеет follow-up и какое пользовательски видимое ограничение остается.

## Обязательные категории доказательств

Phase 19 требует доказательства в этих категориях до записи решения о выпуске:

- live end-to-end proof с реальными runtime и provider dependencies, выбранными для release target;
- stress evidence, проверяющее реалистичные concurrency, retries, interruptions, storage writes и provider failure behavior;
- regression evidence, покрывающее core contracts, ранее принятые integrated flows и классы release-blocking bugs;
- operational evidence для setup, configuration, secrets handling, incident response, recovery, rollback и support handoff;
- final human review, проверяющий, что evidence действительно поддерживает запрошенный release scope.

Если категорию невозможно выполнить, decision должен быть `block`, если только release scope не сужен, а отсутствующая категория явно не записана как `defer`.

## Не-цели

Phase 19 не:

- изобретает новый product scope, чтобы proof прошел;
- превращает dry-run evidence в live provider proof;
- скрывает blocked или inconclusive runs;
- заявляет reliability provider шире тех providers, accounts, regions, limits и dates, которые фактически тестировались;
- сертифицирует security, compliance или production operations шире доказательств, зафиксированных здесь;
- заменяет subsystem owner documents для runtime, memory, interaction, lifecycle, builder или managed subagent behavior.

## Критерии завершения

Phase 19 завершена только когда:

- обязательные runbooks выполнены или явно помечены как blocked с evidence;
- evidence log содержит все release-relevant pass, fail, blocked и inconclusive runs;
- operational procedures достаточны, чтобы новый operator мог запустить, наблюдать, восстановить и отчитаться по release target;
- release decision record называет `release`, `block` или `defer` и объясняет почему;
- любой deferred scope видим users, maintainers и future roadmap planning.
