[English](#english) | [Russian](#russian)

<a id="english"></a>
# Release Scope Lock

Status: canonical release-scope lock for the bounded `release` of `local-cli-repository-readiness` on candidate commit `c3ad3eafca28f4a602a6e44d1861054aabc96a03`. Broader hosted, managed, packaged, production, live-provider, runtime-memory, App Server certification, Builder 2.0, user-interaction, and operator-facing managed-subagent claims remain deferred.

## Selected Release Target

The concrete bounded release target is:

`local-cli-repository-readiness`

This means a taggable repository state for contributors and local users that includes the source tree, contracts, documentation, examples, tests, and a build-local `dist` CLI generated from the checkout with `pnpm build`. Generated `dist` is not treated as tracked source and is not promised to be already present in a clean checkout. It is not a hosted service, npm package, installer, container, managed deployment, or production SaaS release.

The release target is intentionally narrow because current evidence supports local CLI and repository readiness better than it supports broader product distribution or hosted operations. Narrowing the target does not delete future product scope; it only prevents unproven claims from entering the next release decision.

## Current Decision Status

The current truthful decision is bounded `release` for `local-cli-repository-readiness` only.

`block` is not the final decision label because the in-scope local CLI/repository gates passed on candidate commit `c3ad3eafca28f4a602a6e44d1861054aabc96a03`. TASK-400 remains visible as historical failed gate evidence, but TASK-402 and the final TASK-495 candidate-gate evidence superseded that blocker, so TASK-400 must no longer be described as the current blocker.

This `release` is bounded and does not prove hosted/managed deployment, npm/public package publication, installer/container distribution, production SaaS/readiness/load, live provider stress/reliability, broad runtime-memory/provider support, native App Server memory, full App Server certification, durable provider cleanup beyond the TASK-357 verified scoped Mem0 namespace proof, true restore / graph-store/provider-wide cleanup, public Builder 2.0 readiness, full user interaction layer, or operator-facing managed subagent product readiness. The earlier TASK-333 default `pnpm test` failure is superseded by a passing TASK-334 review rerun, and the TASK-400 full-suite `pnpm test` failure is superseded by TASK-402 and TASK-495 clean gates, but both failed and superseded evidence must remain visible.

## Public Repository Accessibility Note

On 2026-04-29, `git ls-remote https://github.com/Andrey-Good/dennett-agent-orchestrator HEAD` returned remote HEAD `716f694819c1e84af8de2dd6de46d913001d1e67`, which proves the GitHub repository URL was publicly reachable from the local environment at that time. This is repository visibility evidence only.

On 2026-04-30, a later repository public-preview check confirmed that GitHub API reported `private: false` and `visibility: public`, and `git ls-remote origin HEAD refs/heads/main` returned `3ddcb5e70a25969b492108c0cb33e695b87137ed` for both remote `HEAD` and `refs/heads/main`, matching local `HEAD` at that time. This supports `repository-public-preview` only. It does not prove npm/public package publication, public registry install, retained public artifact evidence, or final OSS v0.1 release approval.

The removed `package.json` `"private": true` field was an npm publication guard and does not describe GitHub repository visibility. Its removal is release preparation only; package-publication claims remain blocked until publication approval and evidence gates close.

## Included Capabilities

These capabilities are inside the bounded release target. Each one is accepted only for local CLI/repository readiness at the recorded candidate commit.

| Included capability | Owner | Required proof path | Rollback or cleanup expectation | User-visible limitation |
| --- | --- | --- | --- | --- |
| Repository release gates for the accepted local workflow | Release engineering owner | Run `pnpm lint`, `pnpm test`, `pnpm typecheck`, and `pnpm build`; confirm CI covers the same canonical path before any public release claim. | No deployment rollback is implied; if gates fail, do not tag or promote the repository state. | Green local gates mean repository confidence only, not hosted or packaged readiness. |
| Build-local `dist` CLI as local artifact proof | CLI owner and Release engineering owner | Build from the recorded commit with `pnpm build`, run `node dist/src/interfaces/cli.js --help`, run `pnpm dist:check`, run `pnpm packlist:check`, and run `pnpm package:check` from the repository root. | Rebuild or discard the candidate repository state; no published artifact rollback is claimed. | Users operate from the repository checkout and build `dist` locally before CLI artifact smoke; no tracked `dist`, installer, or published package is promised. |
| Minimal Codex App Server graph execution through the local CLI | Runtime integration owner | Run the accepted minimal Codex runtime graph fixture with a disposable state DB and record final output, run ID, and redacted runtime evidence. | Remove disposable state DBs and rerun with a fresh run ID after failures; keep failed attempts visible. | This proves a minimal graph path only, not every Codex App Server feature or model option. |
| Runtime discovery and environment inspection | Runtime integration owner | Run `runtime-env-inspect` and `runtime-model-list` against the supported local authenticated path; redact account identifiers. | No rollback beyond reverting local config or secrets; never store secret values in evidence. | Discovery proves current local account metadata visibility only. |
| Direct local Mem0 provider registration and memory read/write/search plus verified scoped cleanup through Core and CLI | Memory integration owner and Provider operations owner | Register disposable local Mem0 providers, run write/search/read or list, preview namespace-scoped cleanup with an explicit user/agent/run scope, run verified delete with the preview token, and prove a control namespace survives when cleanup isolation is claimed. | Use disposable state/storage; cleanup claims are limited to the configured namespace and explicit scope. Do not claim true restore, graph-store cleanup, provider-wide cleanup, or broad provider reliability. | Memory is direct local provider usage only. Verified cleanup is scoped Mem0 namespace cleanup, not provider-wide cleanup or runtime-native memory inside Codex graph execution. |
| Narrow Stage 2 Codex runtime-memory graph proof | Memory integration owner and Runtime integration owner | Register disposable local Mem0 as `primary_memory`, seed a run-scoped record, run `examples\agents\valid\stage2-codex-runtime-memory-mem0.json`, and verify both model output influenced by memory context and post-success `runtime_node_output` write metadata. | Use disposable state/storage; do not claim durable external cleanup unless separately provider-verified. | This is prompt-rendered memory context plus success-only provider write. It is not native App Server memory, broad provider support, or provider reliability. |
| Local graph-runner stress and regression over accepted core behavior | Core execution owner and QA owner | Run the accepted stress/regression suite that covers concurrency, controlled provider failure, interruption, resume, and storage consistency. | Clean temporary SQLite files and proof artifacts; failed stress attempts remain evidence, not hidden retries. | Stub-runtime stress evidence does not prove external provider throttling reliability. |
| Stage 8 deterministic local recovery, SQLite pressure, and event-dispatch regression | Core execution owner, Lifecycle owner, and QA owner | Run the accepted Stage 8 targeted command covering deterministic stress cleanup, crash/reopen recovery through a fresh SQLite store with explicit retry/resume completion and exactly-once final output, multi-store same-file SQLite pressure, and repeated or near-concurrent event dispatch records. | Clean temporary SQLite files and proof artifacts; failed or timed-out gate attempts remain evidence, not hidden retries. | This proves local deterministic state/regression behavior only. It does not prove automatic live crash recovery, live provider stress, production-scale load, hosted deployment, or external provider reliability. |
| Deterministic provider reliability matrix over local stub runtime behavior | Runtime integration owner and QA owner | Run the accepted Stage 4 provider-reliability matrix covering provider-style throttling, transient failure, interruption/waiting boundaries, resume, bounded active-execution drain, and final-output integrity without live providers or latency gates. | Clean temporary SQLite files and proof artifacts; preserve failed matrix attempts as evidence. | This proves local orchestration semantics only. It does not prove live provider reliability, real throttling behavior, or latency under load. |
| Local operational setup, disposable SQLite recovery, cleanup, and rollback classification | Operations owner | Run setup/config inspection plus the disposable SQLite backup/restore/cleanup procedure in the operational runbook. | Prove cleanup under `%TEMP%`; classify hosted/package rollback as `not-run` unless an artifact exists. | Rollback is local-state recovery only, not hosted, package, container, or durable provider rollback. |

## Deferred Capabilities

These capabilities remain outside the next release target. They must not be described as released, ready, or proven until a later scope decision explicitly includes them and the required evidence passes.

| Deferred capability | Owner | Proof path before inclusion | Rollback or cleanup expectation | User-visible limitation |
| --- | --- | --- | --- | --- |
| Runtime-memory behavior beyond the narrow Stage 2 Codex plus registered Mem0 proof | Memory integration owner and Runtime integration owner | Prove additional providers, native runtime surfaces if they exist, cleanup semantics, retries/idempotency against provider limitations, and broader runtime/model option combinations without secrets. | Clean provider proof data or use a disposable namespace; preserve redacted evidence of intended data removal. | The current proof covers only prompt-rendered context and success-only writes for registered local Mem0 through Core. |
| Hosted service deployment and rollback | Release engineering owner and Operations owner | Define the hosted artifact and environment, deploy a recorded version, run hosted smoke and observability checks, roll back or disable safely, then verify health and state integrity. | Hosted rollback must be executed and audited before hosted readiness is claimed. | No hosted service operation, uptime, multi-tenant support, or hosted rollback promise exists in this release target. |
| npm/package publication, installer, container, or other distributable artifact | Release engineering owner | Define the artifact, build from a recorded commit, install in a disposable environment, run CLI smoke/config inspection, uninstall or roll back to a prior artifact, and verify behavior. | Artifact-specific uninstall, rollback, or previous-version verification is mandatory before inclusion. | Users should not expect an npm package, installer, Docker image, or packaged upgrade path. |
| Durable external provider data cleanup and rollback beyond verified scoped Mem0 namespace cleanup | Memory integration owner and Provider operations owner | Use an isolated provider namespace/account, create durable proof data, prove backup or cleanup semantics beyond bounded scoped delete, verify via provider read/search/list, and record redacted evidence. | Removal or restoration must be provider-verified; disposable local cleanup and TASK-357 scoped namespace cleanup are insufficient for true restore or provider-wide cleanup claims. | Durable external provider data cleanup beyond the configured namespace and explicit scope is not covered by the current cleanup guarantee. |
| External provider reliability, throttling, and volume claims | Runtime integration owner and Provider reliability owner | Run quota-safe provider tests for throttling, transient failures, retries, limits, and final-output integrity against named providers and limits. | Failed or throttled runs must leave no unbounded retry or cleanup debt. | Current evidence does not guarantee provider reliability under load. |
| Full user interaction layer beyond current local evidence | Interaction owner and CLI owner | Prove blocked prompts, replies, resume-after-reply, and risky mid-run change policy across CLI and supported runtime state. | Clean prompt/reply proof state and preserve redacted interaction evidence. | User chat and resume behavior must be described only at the proven local/focused level. |
| Managed subagent orchestration as an operator-facing product surface | Managed subagent owner and QA owner | Prove create/send/wait/status/close/cancel, roles, write-scope ownership, review/fix loops, lineage, and cancellation through user-visible flows. | Close or cancel child work and verify no leaked active child runs. | Managed subagents remain partially implemented and not broadly release-proven. |
| Builder 2.0 and integrated multi-feature product flows as public authoring workflows | Builder owner and Product integration owner | Prove builder output through validation, lifecycle publication, execution, memory/runtime/interaction/subagent boundaries, and integrated acceptance scenarios. | Clean drafts, live revisions, and proof state or prove safe rollback. | Builder and integrated flows must not be marketed as a complete public authoring system. |

## OSS v0.1 Provider Claim Matrix

The following matrix is the public-claim boundary for provider language until a later release-scope update records new live evidence. It must be reconciled with [Live Proof Runbook](./live-proof-runbook.md) before any public launch approval task changes provider wording.

| Provider surface | Current allowed wording | Evidence status | Launch-blocking gap before broader claim |
| --- | --- | --- | --- |
| Local/offline runtime tests and examples | Local deterministic graph and CLI behavior are covered by repository tests and local proof only. | Historical local gates and deterministic tests exist. | Rerun local gates against the exact OSS candidate artifact before claiming current-candidate readiness. |
| Codex App Server runtime adapter | Limited Codex/App Server path with narrow local authenticated discovery and minimal graph proof. | Historical discovery and minimal live smoke exist, but not full certification. | Candidate-specific live Codex proof with redacted account/config metadata, model/options boundary, and final output. |
| Runtime model/options metadata | Runtime discovery may expose current local metadata when an authenticated local path is configured. | Historical discovery proof exists. | Candidate-specific discovery rerun and unsupported-fields review; no account, rate-limit, or all-models promise without provider evidence. |
| Direct Mem0 provider operations | User-owned/local Mem0 registration and memory operations may be described only as limited and opt-in. | Historical direct Mem0 proof and scoped cleanup proof exist. | Candidate-specific disposable Mem0 proof if public docs imply current OSS usability; cleanup must be provider-verified for the claimed scope. |
| Codex plus Mem0 runtime-memory flow | Prompt-rendered memory context plus success-only Core writes only. | Historical narrow proof exists. | Candidate-specific runtime-memory proof; native App Server memory remains forbidden unless separately implemented and proven. |
| External provider reliability and throttling | Not proven. | Deterministic stub reliability matrix exists, but it is not live provider reliability evidence. | Quota-safe named-provider reliability proof with throttling, retries, limits, failure handling, and cleanup debt recorded. |
| Native App Server memory | Not supported as a public claim. | No accepted evidence. | Implementation plus live proof through a documented native runtime surface. |
| Non-Codex runtimes, hosted provider operations, provider-wide cleanup, production load | Not supported as public claims. | No accepted evidence. | Separate implementation, release scope, live proof, rollback/cleanup evidence, and operations evidence. |

## Forbidden Claims

Until a later release-scope expansion changes this lock, do not claim:

- the product is fully released or fully production ready;
- hosted, package, installer, container, or artifact rollback is proven;
- external provider cleanup is durable beyond disposable local proof data and TASK-357 verified scoped Mem0 namespace cleanup;
- Stage 3 cleanup is true restore, graph-store cleanup, provider-wide cleanup, or delete-all;
- Mem0 or any memory provider is consumed through a native App Server memory primitive;
- Stage 2 runtime memory is broader than registered local provider resolution, prompt-rendered Codex context, and success-only provider writes;
- provider reliability, throttling behavior, or volume handling is proven beyond dated evidence;
- live provider stress or production-scale load is proven by deterministic local Stage 8 tests;
- automatic live crash recovery is proven by deterministic local Stage 8 retry/resume tests;
- all Codex App Server-native capabilities are exposed or certified.

## Evidence And Update Rules

- Evidence must stay in the evidence log or linked runbooks, not only in a task summary.
- Failed, blocked, inconclusive, and superseded evidence must remain visible.
- A deferred capability can move into scope only through a later release-scope update plus evidence that satisfies the relevant runbook.
- If the release target expands, rollback and cleanup proof for the expanded target becomes release-blocking.
- If a claim appears in README or user-facing docs, it must match this scope lock and the release decision record.

<a id="russian"></a>
# Фиксация области выпуска

Статус: каноническая фиксация области bounded `release` для `local-cli-repository-readiness` на candidate commit `c3ad3eafca28f4a602a6e44d1861054aabc96a03`. Более широкие hosted, managed, packaged, production, live-provider, runtime-memory, App Server certification, Builder 2.0, user-interaction и operator-facing managed-subagent claims остаются deferred.

## Выбранная цель выпуска

Конкретная bounded цель выпуска:

`local-cli-repository-readiness`

Это означает состояние репозитория, которое можно тегировать для контрибьюторов и локальных пользователей и которое включает исходный код, контракты, документацию, примеры, тесты и build-local `dist` CLI, созданный из checkout командой `pnpm build`. Generated `dist` не считается tracked source и не обещается как уже существующий в clean checkout. Это не hosted service, npm package, installer, container, managed deployment или production SaaS release.

Цель намеренно узкая, потому что текущие доказательства лучше поддерживают готовность локального CLI и репозитория, чем более широкую дистрибуцию продукта или hosted operations. Сужение цели не удаляет будущую продуктовую область; оно только не дает непроверенным заявлениям попасть в следующее release decision.

## Текущий статус решения

Текущее правдивое решение - bounded `release` только для `local-cli-repository-readiness`.

`block` не является финальной меткой решения, потому что in-scope local CLI/repository gates прошли на candidate commit `c3ad3eafca28f4a602a6e44d1861054aabc96a03`. TASK-400 остается видимым как historical failed gate evidence, но TASK-402 и final TASK-495 candidate-gate evidence superseded этот blocker, поэтому TASK-400 больше нельзя описывать как current blocker.

Этот `release` bounded и не доказывает hosted/managed deployment, npm/public package publication, installer/container distribution, production SaaS/readiness/load, live provider stress/reliability, broad runtime-memory/provider support, native App Server memory, full App Server certification, durable provider cleanup beyond TASK-357 verified scoped Mem0 namespace proof, true restore / graph-store/provider-wide cleanup, public Builder 2.0 readiness, full user interaction layer или operator-facing managed subagent product readiness. Более ранний TASK-333 default `pnpm test` failure superseded by passing TASK-334 review rerun, и TASK-400 full-suite `pnpm test` failure superseded by TASK-402 и TASK-495 clean gates, но failed и superseded evidence должны оставаться видимыми.

## Включенные возможности

Эти возможности входят в bounded цель выпуска. Каждая из них принята только для local CLI/repository readiness на recorded candidate commit.

| Включенная возможность | Владелец | Обязательный proof path | Ожидание rollback или cleanup | Пользовательское ограничение |
| --- | --- | --- | --- | --- |
| Repository release gates для принятого local workflow | Release engineering owner | Запустить `pnpm lint`, `pnpm test`, `pnpm typecheck` и `pnpm build`; подтвердить, что CI покрывает тот же канонический путь до любого публичного release claim. | Deployment rollback не подразумевается; если gates падают, не тегировать и не продвигать состояние репозитория. | Зеленые локальные gates означают только уверенность в репозитории, а не hosted или packaged readiness. |
| Build-local `dist` CLI как local artifact proof | CLI owner и Release engineering owner | Собрать из recorded commit командой `pnpm build`, запустить `node dist/src/interfaces/cli.js --help`, `pnpm dist:check`, `pnpm packlist:check` и `pnpm package:check` из корня репозитория. | Пересобрать или отбросить candidate repository state; rollback опубликованного artifact не заявляется. | Пользователи работают из checkout репозитория и локально собирают `dist` перед CLI artifact smoke; tracked `dist`, installer или опубликованный package не обещаются. |
| Minimal Codex App Server graph execution через локальный CLI | Runtime integration owner | Запустить принятую minimal Codex runtime graph fixture с disposable state DB и записать final output, run ID и отредактированное runtime evidence. | Удалять disposable state DBs и повторять со свежим run ID после failures; failed attempts оставлять видимыми. | Это доказывает только minimal graph path, а не каждую Codex App Server feature или model option. |
| Runtime discovery и environment inspection | Runtime integration owner | Запустить `runtime-env-inspect` и `runtime-model-list` против поддерживаемого локального authenticated path; редактировать account identifiers. | Rollback ограничен возвратом local config или secrets; secret values никогда не сохраняются в evidence. | Discovery доказывает только видимость metadata текущего local account. |
| Direct local Mem0 provider registration, memory read/write/search и verified scoped cleanup через Core и CLI | Memory integration owner и Provider operations owner | Зарегистрировать disposable local Mem0 providers, выполнить write/search/read или list, preview namespace-scoped cleanup с explicit user/agent/run scope, выполнить verified delete с preview token и доказать survival control namespace, если заявляется cleanup isolation. | Использовать disposable state/storage; cleanup claims ограничены configured namespace и explicit scope. Не заявлять true restore, graph-store cleanup, provider-wide cleanup, delete-all или broad provider reliability. | Memory является только direct local provider usage. Verified cleanup является scoped Mem0 namespace cleanup, а не provider-wide cleanup или runtime-native memory inside Codex graph execution. |
| Narrow Stage 2 Codex runtime-memory graph proof | Memory integration owner и Runtime integration owner | Зарегистрировать disposable local Mem0 as `primary_memory`, seed a run-scoped record, run `examples\agents\valid\stage2-codex-runtime-memory-mem0.json`, and verify both model output influenced by memory context and post-success `runtime_node_output` write metadata. | Использовать disposable state/storage; не заявлять durable external cleanup unless separately provider-verified. | This is prompt-rendered memory context plus success-only provider write. It is not native App Server memory, broad provider support, or provider reliability. |
| Local graph-runner stress и regression для принятого core behavior | Core execution owner и QA owner | Запустить принятую stress/regression suite, которая покрывает concurrency, controlled provider failure, interruption, resume и storage consistency. | Очищать temporary SQLite files и proof artifacts; failed stress attempts остаются evidence, а не скрытыми retries. | Stress evidence со stub runtime не доказывает external provider throttling reliability. |
| Deterministic provider reliability matrix для local stub runtime behavior | Runtime integration owner и QA owner | Запустить принятую Stage 4 provider-reliability matrix, которая покрывает provider-style throttling, transient failure, interruption/waiting boundaries, resume, bounded active-execution drain и final-output integrity without live providers or latency gates. | Очищать temporary SQLite files и proof artifacts; сохранять failed matrix attempts как evidence. | Это доказывает только local orchestration semantics. Это не доказывает live provider reliability, real throttling behavior или latency under load. |
| Local operational setup, disposable SQLite recovery, cleanup и rollback classification | Operations owner | Запустить setup/config inspection плюс disposable SQLite backup/restore/cleanup procedure из operational runbook. | Доказать cleanup под `%TEMP%`; hosted/package rollback классифицировать как `not-run`, если artifact не существует. | Rollback означает только local-state recovery, а не hosted, package, container или durable provider rollback. |

## Отложенные возможности

Эти возможности остаются вне следующей цели выпуска. Их нельзя описывать как released, ready или proven, пока более позднее scope decision явно не включит их и обязательные доказательства не пройдут.

| Отложенная возможность | Владелец | Proof path перед включением | Ожидание rollback или cleanup | Пользовательское ограничение |
| --- | --- | --- | --- | --- |
| Runtime-memory behavior beyond the narrow Stage 2 Codex plus registered Mem0 proof | Memory integration owner и Runtime integration owner | Prove additional providers, native runtime surfaces if they exist, cleanup semantics, retries/idempotency against provider limitations, and broader runtime/model option combinations without secrets. | Очищать provider proof data или использовать disposable namespace; сохранять отредактированное доказательство удаления intended data. | The current proof covers only prompt-rendered context and success-only writes for registered local Mem0 through Core. |
| Hosted service deployment и rollback | Release engineering owner и Operations owner | Определить hosted artifact и environment, развернуть recorded version, выполнить hosted smoke и observability checks, безопасно откатиться или отключиться, затем проверить health и state integrity. | Hosted rollback должен быть выполнен и проверен до заявления hosted readiness. | В этой цели выпуска нет обещания hosted service operation, uptime, multi-tenant support или hosted rollback. |
| npm/package publication, installer, container или другой distributable artifact | Release engineering owner | Определить artifact, собрать из recorded commit, установить в disposable environment, выполнить CLI smoke/config inspection, удалить или откатить к previous artifact и проверить поведение. | Artifact-specific uninstall, rollback или previous-version verification обязательны перед включением. | Пользователи не должны ожидать npm package, installer, Docker image или packaged upgrade path. |
| Durable external provider data cleanup и rollback beyond verified scoped Mem0 namespace cleanup | Memory integration owner и Provider operations owner | Использовать isolated provider namespace/account, создать durable proof data, доказать backup или cleanup semantics beyond bounded scoped delete, проверить через provider read/search/list и записать redacted evidence. | Removal или restoration должны быть provider-verified; disposable local cleanup и TASK-357 scoped namespace cleanup недостаточны для true restore или provider-wide cleanup claims. | Durable external provider data cleanup вне configured namespace и explicit scope не покрыта текущей cleanup guarantee. |
| External provider reliability, throttling и volume claims | Runtime integration owner и Provider reliability owner | Выполнить quota-safe provider tests для throttling, transient failures, retries, limits и final-output integrity против named providers и limits. | Failed или throttled runs не должны оставлять unbounded retry или cleanup debt. | Текущие доказательства не гарантируют provider reliability under load. |
| Full user interaction layer сверх текущих local evidence | Interaction owner и CLI owner | Доказать blocked prompts, replies, resume-after-reply и risky mid-run change policy через CLI и поддерживаемое runtime state. | Очищать prompt/reply proof state и сохранять redacted interaction evidence. | User chat и resume behavior нужно описывать только на proven local/focused уровне. |
| Managed subagent orchestration как operator-facing product surface | Managed subagent owner и QA owner | Доказать create/send/wait/status/close/cancel, roles, write-scope ownership, review/fix loops, lineage и cancellation через user-visible flows. | Закрыть или отменить child work и проверить отсутствие leaked active child runs. | Managed subagents остаются partially implemented и не доказаны широко для release. |
| Builder 2.0 и integrated multi-feature product flows как public authoring workflows | Builder owner и Product integration owner | Доказать builder output через validation, lifecycle publication, execution, memory/runtime/interaction/subagent boundaries и integrated acceptance scenarios. | Очищать drafts, live revisions и proof state или доказывать safe rollback. | Builder и integrated flows нельзя продвигать как complete public authoring system. |

## Запрещенные заявления

Пока более позднее release-scope expansion не изменит эту фиксацию, нельзя заявлять:

- продукт полностью выпущен или полностью production ready;
- hosted, package, installer, container или artifact rollback доказан;
- external provider cleanup является durable beyond disposable local proof data и TASK-357 verified scoped Mem0 namespace cleanup;
- Stage 3 cleanup является true restore, graph-store cleanup, provider-wide cleanup или delete-all;
- Mem0 или другой memory provider потребляется нативно inside Codex graph execution;
- provider reliability, throttling behavior или volume handling доказаны шире датированных evidence;
- все Codex App Server-native capabilities exposed или certified.

## Правила evidence и обновления

- Evidence должно оставаться в evidence log или linked runbooks, а не только в task summary.
- Failed, blocked, inconclusive и superseded evidence должны оставаться видимыми.
- Deferred capability может перейти в scope только через более позднее release-scope update плюс evidence, удовлетворяющее соответствующему runbook.
- Если release target расширяется, rollback и cleanup proof для расширенного target становится release-blocking.
- Если claim появляется в README или user-facing docs, он должен соответствовать этой scope lock и release decision record.

## TASK-333 Stage 2 Scope Note

Русский: TASK-333 добавляет только узкий included proof для Stage 2: локально зарегистрированный Mem0 provider, prompt-rendered `memory_context` в Codex App Server path и success-only запись node output через Core. Это не native App Server memory, не broad provider support, не provider reliability и не release readiness. TASK-334 review rerun of default `pnpm test` passed, so the earlier TASK-333 default-gate failure is historical superseded evidence rather than the current blocker.

## TASK-357 Stage 3 Scope Note

TASK-357 добавляет только verified scoped cleanup evidence для direct local Mem0 provider path. Принятый proof использует два namespace поверх общего disposable local Mem0 storage: target cleanup сообщает `verified_empty`, а control namespace сохраняется. Это остается уже, чем durable external provider cleanup, true restore, graph-store cleanup, provider-wide cleanup или provider reliability.
