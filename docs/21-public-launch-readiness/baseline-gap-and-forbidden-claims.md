[English](#english) | [Russian](#russian)

<a id="english"></a>
# Baseline Gap And Forbidden Claims

Status: canonical Stage 1 public-launch readiness baseline. This document locks the current evidence boundary before Part 1 stages 2-10 expand or reject scope.

Related documents:

- [Phase 12 Capability Gap Lock](../13-capability-gap-lock/phase-12-capability-gap-lock.md)
- [Release Scope Lock](../20-real-world-proof-and-release/release-scope-lock.md)
- [Release Decision Record](../20-real-world-proof-and-release/release-decision-record.md)
- [Phase 14 Native Runtime Surface Completion](../15-native-runtime-surface/phase-14-native-runtime-surface-completion.md)
- [Phase 15 Full User Interaction Layer](../16-full-user-interaction-layer/phase-15-full-user-interaction-layer.md)
- [Phase 16 Managed Subagent Orchestration](../17-managed-subagent-orchestration/phase-16-managed-subagent-orchestration.md)
- [Managed Subagent Productization](./managed-subagent-productization.md)
- [Phase 17 Builder 2.0](../18-builder-2-0/phase-17-builder-2-0.md)
- [Builder 2.0 Productization](./builder-2-0-productization.md)
- [Stable CLI/API Contract Freeze](./stable-cli-api-contract-freeze.md)

## Current Baseline

The only accepted release state is the bounded `release` for `local-cli-repository-readiness` on candidate commit `c3ad3eafca28f4a602a6e44d1861054aabc96a03`.

That target means a taggable repository state for contributors and local users, including source, contracts, documentation, examples, tests, and a build-local `dist` CLI produced from checkout with `pnpm build`.

It does not mean public product launch readiness. It does not prove hosted operation, managed deployment, package publication, installer or container distribution, production SaaS readiness, full App Server certification, broad provider reliability, native App Server memory, full interaction readiness, complete managed subagent orchestration, or public Builder 2.0 authoring readiness.

## Already Implemented Or Proven For The Bounded Local Target

These items may be described only inside the local CLI/repository boundary:

| Capability | Current evidence boundary | Public-launch consequence |
| --- | --- | --- |
| Repository gates and build-local CLI artifact | Final local candidate gates, build, `dist` smoke, packlist, and package dry-run checks are recorded in the release decision record. | Supports repository confidence only; does not imply package publication or artifact rollback. |
| Minimal Codex App Server graph execution | Minimal live Codex graph smoke is recorded for the accepted local path. | Supports a narrow runtime graph proof only; does not certify the full App Server surface. |
| Runtime discovery and environment inspection | Built CLI proof exists for model listing and environment inspection on the supported local authenticated path. | Supports local metadata visibility only; does not prove every model option or account/rate-limit policy under launch conditions. |
| Direct local Mem0 provider path | Local provider registration, direct memory CRUD/search, and scoped Mem0 namespace cleanup have proof. | Supports the bounded local Mem0 path only; does not prove durable provider cleanup, provider-wide cleanup, true restore, graph-store cleanup, or provider reliability. |
| Narrow Stage 2 runtime-memory graph path | Registered local Mem0 plus prompt-rendered Codex memory context and success-only provider writes have proof. | Not native App Server memory and not broad runtime-memory/provider support. |
| Local stress, recovery, and deterministic provider matrix | Stub-runtime and local SQLite tests prove selected local orchestration semantics. | Does not prove live provider stress, external throttling reliability, production-scale load, or automatic live crash recovery. |
| Supported user prompt wait/reply/resume slice | Focused adapter, Core, and CLI tests cover durable prompt state for supported prompt shapes. | Not full user interaction readiness across all prompt shapes, interfaces, or risky mid-run change policies. |
| First managed-subagent layer | Core service and state cover worker/reviewer/final-review roles, launch/wait/send/close, findings, cancellation state, budgets, sibling write-set conflict rejection, and the Stage 8 CLI commands `subagent-launch`, `subagent-list`, `subagent-show`, `subagent-wait`, `subagent-record-control`, and `subagent-close`. | Supports only the bounded local CLI operator surface. Launch is launch-and-wait only; control/cancel semantics are state-recorded, not live-delivered; there is no durable background runner or broad live orchestration proof. |
| Builder draft and Builder 2.0 authoring boundary | Builder remains draft-first and public-contract-only. TASK-557 adds a formal `builder-output.schema.json` wrapper and deterministic candidate audit for runtime options, capability gates, JSON output schema compilation, hidden managed-subagent field rejection, and local/secret provider-data rejection. Accepted output is persisted as a draft revision and candidate diagnostics are exposed outside Agent JSON. | Supports only bounded audited draft authoring. Not a public complete authoring system, not deploy proof, not provider registration, and not proof that builder-authored agents execute as integrated product flows. |

## Partial Or Deferred Public-Launch Gaps

| Part 1 stage | Gap locked by Stage 1 | Evidence required before the gap can move toward public-launch scope |
| --- | --- | --- |
| Stage 2: Public Launch Scope Decision | No public launch target is selected beyond the bounded local release. | Explicit scope document naming included/excluded launch surfaces, user personas, support boundary, rollback expectations, and public claim language. |
| Stage 3: Security, Privacy, Legal Foundation | No public security, privacy, or legal readiness claim is made. | Threat model, secret-handling review, privacy/data-retention statement, license/package review, disclosure process, and evidence that docs and examples do not expose secrets or unsupported data promises. |
| Stage 4: Release Engineering And Supply Chain | No npm/public package, installer, container, signed artifact, provenance, or artifact rollback readiness is proven. | Clean-environment install or deployment proof for each selected artifact, reproducible build expectations, artifact inventory, provenance/signing decision, rollback/uninstall proof, and CI parity with local gates. |
| Stage 5: Runtime/App Server Certification | No full App Server certification is claimed. | Supported runtime matrix, App Server primitive mapping, model/options compatibility evidence, timeout/failure-mode evidence, auth/account/rate-limit behavior, and redacted live runs for each certified surface. |
| Stage 6: Memory Productization | Memory remains bounded to local provider registration, Mem0-first direct use, and narrow prompt-rendered runtime memory. | Provider readiness matrix, durable cleanup/restore semantics where claimed, provider isolation proof, reliability/throttling proof, backup or deletion guarantees, and documented limits for every supported provider. |
| Stage 7: Full User Interaction Layer | Current interaction proof is narrower than a full public user-interaction surface. | End-to-end evidence for blocked prompts, replies, resume-after-reply, unsupported prompt errors, risky mid-run change policy, CLI/user-facing semantics, and redacted transcripts across supported runtimes. |
| Stage 8: Managed Subagent Product Surface | A bounded local CLI operator surface exists, but complete public orchestration remains deferred. | Evidence exists for launch-and-wait, list, show, wait/reconcile, record-control, and close. Evidence is still required for durable background runners, live control delivery, live runtime cancellation, cross-process attachment, complete review/fix loops, surfaced child interaction, cancellation cleanup, and leaked-child checks. |
| Stage 9: Builder 2.0 On Stable Contracts | Builder 2.0 is productized only as bounded audited draft-first authoring, not a public complete authoring workflow. | Remaining evidence is still required for live execution proof of representative drafts, integrated builder/lifecycle/runtime/memory/interaction/subagent scenarios, and user-facing rejected-candidate failure docs. |
| Stage 10: Stable CLI/API Contract Freeze | Bounded stable CLI/API compatibility is locked only for the explicitly labeled stable CLI commands, stable/safety-protocol cleanup flow, exported JSON schema artifacts, and no-JS-API package boundary. | Stable compatibility remains forbidden for experimental commands, JS/TS imports, hosted/managed APIs, unpublished package claims, and any surface not named in [Stable CLI/API Contract Freeze](./stable-cli-api-contract-freeze.md). |

## Forbidden Claims

Until a later stage records the required evidence and updates the release scope, do not claim:

- Dennett is publicly launched, fully released, generally available, or production ready.
- The current release is more than bounded `local-cli-repository-readiness`.
- Hosted service operation, managed deployment, uptime, multi-tenancy, or hosted rollback is proven.
- npm/public package publication, installer distribution, container distribution, signed artifacts, supply-chain provenance, or packaged rollback is proven.
- Full Codex App Server certification is complete.
- Native App Server memory is implemented or Mem0 is consumed through a native App Server memory primitive.
- Runtime memory is broader than registered local provider resolution, prompt-rendered Codex context, and success-only provider writes.
- Durable external provider cleanup, true restore, graph-store cleanup, provider-wide cleanup, delete-all, throttling behavior, or volume reliability is proven.
- Deterministic local stress/recovery tests prove live provider stress, production-scale load, or automatic live crash recovery.
- Full user interaction readiness is proven beyond the supported prompt wait/reply/resume slice.
- Managed subagents are a complete operator-facing product surface.
- Managed subagents provide durable background execution, live control-message delivery, live runtime cancellation, hosted/UI orchestration, or write-set sandboxing.
- Builder 2.0 is a public complete authoring system, deploy authority, provider/runtime account registry, live managed-subagent orchestrator, or proof that builder-authored agents are integrated product flows.
- Stable public CLI/API compatibility exists outside the bounded Stage 10 freeze, including for experimental commands or JS/TS imports.

## Unlock Rules

A forbidden claim can be unlocked only when all of the following are true:

1. a later Part 1 stage explicitly includes the capability in scope;
2. the owner document names exact user-visible behavior and non-goals;
3. tests cover expected success, failure, and unsupported cases;
4. live or artifact evidence exists when the claim depends on a real runtime, provider, deployment surface, or distribution artifact;
5. rollback, cleanup, or recovery is proven where the claim implies operational responsibility;
6. failed, blocked, inconclusive, and superseded attempts remain visible;
7. README and user-facing docs use the same bounded language as the updated scope lock.

Evidence must live in a durable documentation owner such as an evidence log, runbook, release-scope update, ADR, or acceptance test. A task summary alone is not enough.

## Relationship To Part 1 Stages 2-10

Stage 1 does not decide public launch scope. It creates the baseline that later stages must either satisfy or keep deferred.

- Stage 2 may narrow, defer, or select a public-launch scope, but it must not erase this forbidden-claim lock.
- Stage 3 must decide security, privacy, and legal prerequisites before public distribution or hosted claims.
- Stage 4 must prove each selected distribution artifact instead of inferring artifact readiness from repository gates.
- Stage 5 must certify supported runtime/App Server surfaces explicitly.
- Stage 6 must productize memory only for provider behavior that has real cleanup, reliability, and support evidence.
- Stage 7 must prove full interaction semantics before user-facing interaction claims expand.
- Stage 8 records the bounded local CLI managed-subagent operator surface; later stages must prove durable background execution, live cancellation/control delivery, and complete review/fix orchestration before broader claims expand.
- Stage 9 must prove Builder 2.0 on stable public contracts before public authoring claims expand.
- Stage 10 freezes only the bounded CLI/API compatibility surface named in [Stable CLI/API Contract Freeze](./stable-cli-api-contract-freeze.md); any expansion needs a later owner update and evidence.

<a id="russian"></a>
# Baseline Gap And Forbidden Claims

Статус: каноническая Stage 1 фиксация public-launch readiness baseline. Этот документ фиксирует текущую границу evidence перед тем, как stages 2-10 из Part 1 расширят или отклонят scope.

Связанные документы:

- [Phase 12 Capability Gap Lock](../13-capability-gap-lock/phase-12-capability-gap-lock.md)
- [Release Scope Lock](../20-real-world-proof-and-release/release-scope-lock.md)
- [Release Decision Record](../20-real-world-proof-and-release/release-decision-record.md)
- [Phase 14 Native Runtime Surface Completion](../15-native-runtime-surface/phase-14-native-runtime-surface-completion.md)
- [Phase 15 Full User Interaction Layer](../16-full-user-interaction-layer/phase-15-full-user-interaction-layer.md)
- [Phase 16 Managed Subagent Orchestration](../17-managed-subagent-orchestration/phase-16-managed-subagent-orchestration.md)
- [Managed Subagent Productization](./managed-subagent-productization.md)
- [Phase 17 Builder 2.0](../18-builder-2-0/phase-17-builder-2-0.md)
- [Builder 2.0 Productization](./builder-2-0-productization.md)
- [Stable CLI/API Contract Freeze](./stable-cli-api-contract-freeze.md)

## Текущий baseline

Единственное принятое состояние выпуска - bounded `release` для `local-cli-repository-readiness` на candidate commit `c3ad3eafca28f4a602a6e44d1861054aabc96a03`.

Эта цель означает taggable состояние репозитория для контрибьюторов и локальных пользователей, включая source, contracts, documentation, examples, tests и build-local `dist` CLI, созданный из checkout командой `pnpm build`.

Она не означает готовность к публичному запуску продукта. Она не доказывает hosted operation, managed deployment, package publication, installer или container distribution, production SaaS readiness, full App Server certification, broad provider reliability, native App Server memory, full interaction readiness, complete managed subagent orchestration или public Builder 2.0 authoring readiness.

## Уже реализовано или доказано для bounded local target

Эти пункты можно описывать только внутри границы local CLI/repository:

| Capability | Текущая evidence boundary | Следствие для public launch |
| --- | --- | --- |
| Repository gates и build-local CLI artifact | Final local candidate gates, build, `dist` smoke, packlist и package dry-run checks записаны в release decision record. | Поддерживает только уверенность в репозитории; не подразумевает package publication или artifact rollback. |
| Minimal Codex App Server graph execution | Minimal live Codex graph smoke записан для принятого local path. | Поддерживает только узкий runtime graph proof; не сертифицирует всю App Server surface. |
| Runtime discovery и environment inspection | Built CLI proof существует для model listing и environment inspection на поддерживаемом локальном authenticated path. | Поддерживает только local metadata visibility; не доказывает каждый model option или account/rate-limit policy в launch conditions. |
| Direct local Mem0 provider path | Local provider registration, direct memory CRUD/search и scoped Mem0 namespace cleanup имеют proof. | Поддерживает только bounded local Mem0 path; не доказывает durable provider cleanup, provider-wide cleanup, true restore, graph-store cleanup или provider reliability. |
| Narrow Stage 2 runtime-memory graph path | Registered local Mem0 plus prompt-rendered Codex memory context и success-only provider writes имеют proof. | Это не native App Server memory и не broad runtime-memory/provider support. |
| Local stress, recovery и deterministic provider matrix | Stub-runtime и local SQLite tests доказывают выбранные local orchestration semantics. | Не доказывает live provider stress, external throttling reliability, production-scale load или automatic live crash recovery. |
| Supported user prompt wait/reply/resume slice | Focused adapter, Core и CLI tests покрывают durable prompt state для поддерживаемых prompt shapes. | Не является full user interaction readiness для всех prompt shapes, interfaces или risky mid-run change policies. |
| First managed-subagent layer | Core service и state покрывают worker/reviewer/final-review roles, launch/wait/send/close, findings, cancellation state, budgets, sibling write-set conflict rejection и Stage 8 CLI commands `subagent-launch`, `subagent-list`, `subagent-show`, `subagent-wait`, `subagent-record-control` и `subagent-close`. | Поддерживает только bounded local CLI operator surface. Launch является только launch-and-wait; control/cancel semantics записываются в state, а не live-deliver-ятся; нет durable background runner или broad live orchestration proof. |
| Builder draft и Builder 2.0 authoring boundary | Builder остается draft-first и public-contract-only. TASK-557 добавляет formal `builder-output.schema.json` wrapper и deterministic candidate audit для runtime options, capability gates, JSON output schema compilation, hidden managed-subagent field rejection и local/secret provider-data rejection. Принятый output сохраняется как draft revision, а candidate diagnostics выводятся вне Agent JSON. | Поддерживается только bounded audited draft authoring. Это не public complete authoring system, не deploy proof, не provider registration и не proof, что builder-authored agents выполняются как integrated product flows. |

## Partial или deferred gaps для public launch

| Part 1 stage | Gap, зафиксированный Stage 1 | Evidence, нужное перед движением gap в public-launch scope |
| --- | --- | --- |
| Stage 2: Public Launch Scope Decision | Public launch target не выбран за пределами bounded local release. | Explicit scope document с included/excluded launch surfaces, user personas, support boundary, rollback expectations и public claim language. |
| Stage 3: Security, Privacy, Legal Foundation | Нет claim о public security, privacy или legal readiness. | Threat model, secret-handling review, privacy/data-retention statement, license/package review, disclosure process и evidence, что docs/examples не раскрывают secrets или unsupported data promises. |
| Stage 4: Release Engineering And Supply Chain | Нет доказанной готовности npm/public package, installer, container, signed artifact, provenance или artifact rollback. | Clean-environment install или deployment proof для каждого выбранного artifact, reproducible build expectations, artifact inventory, provenance/signing decision, rollback/uninstall proof и CI parity с local gates. |
| Stage 5: Runtime/App Server Certification | Full App Server certification не заявляется. | Supported runtime matrix, App Server primitive mapping, model/options compatibility evidence, timeout/failure-mode evidence, auth/account/rate-limit behavior и redacted live runs для каждой certified surface. |
| Stage 6: Memory Productization | Memory ограничена local provider registration, Mem0-first direct use и narrow prompt-rendered runtime memory. | Provider readiness matrix, durable cleanup/restore semantics where claimed, provider isolation proof, reliability/throttling proof, backup или deletion guarantees и documented limits для каждого supported provider. |
| Stage 7: Full User Interaction Layer | Текущий interaction proof уже, чем полный public user-interaction surface. | End-to-end evidence для blocked prompts, replies, resume-after-reply, unsupported prompt errors, risky mid-run change policy, CLI/user-facing semantics и redacted transcripts across supported runtimes. |
| Stage 8: Managed Subagent Product Surface | Bounded local CLI operator surface существует, но complete public orchestration остается deferred. | Evidence существует для launch-and-wait, list, show, wait/reconcile, record-control и close. Evidence все еще нужно для durable background runners, live control delivery, live runtime cancellation, cross-process attachment, complete review/fix loops, surfaced child interaction, cancellation cleanup и leaked-child checks. |
| Stage 9: Builder 2.0 On Stable Contracts | Builder 2.0 productized только как bounded audited draft-first authoring, а не public complete authoring workflow. | Все еще нужны live execution proof для representative drafts, integrated builder/lifecycle/runtime/memory/interaction/subagent scenarios и user-facing rejected-candidate failure docs. |
| Stage 10: Stable CLI/API Contract Freeze | Bounded stable CLI/API compatibility зафиксирована только для explicitly labeled stable CLI commands, stable/safety-protocol cleanup flow, exported JSON schema artifacts и no-JS-API package boundary. | Stable compatibility остается forbidden для experimental commands, JS/TS imports, hosted/managed APIs, unpublished package claims и любой surface, не названной в [Stable CLI/API Contract Freeze](./stable-cli-api-contract-freeze.md). |

## Forbidden Claims

Пока более поздний stage не запишет required evidence и не обновит release scope, нельзя заявлять:

- Dennett publicly launched, fully released, generally available или production ready.
- Текущий release является чем-то большим, чем bounded `local-cli-repository-readiness`.
- Hosted service operation, managed deployment, uptime, multi-tenancy или hosted rollback доказаны.
- npm/public package publication, installer distribution, container distribution, signed artifacts, supply-chain provenance или packaged rollback доказаны.
- Full Codex App Server certification завершена.
- Native App Server memory реализована или Mem0 потребляется через native App Server memory primitive.
- Runtime memory шире registered local provider resolution, prompt-rendered Codex context и success-only provider writes.
- Durable external provider cleanup, true restore, graph-store cleanup, provider-wide cleanup, delete-all, throttling behavior или volume reliability доказаны.
- Deterministic local stress/recovery tests доказывают live provider stress, production-scale load или automatic live crash recovery.
- Full user interaction readiness доказана сверх supported prompt wait/reply/resume slice.
- Managed subagents являются complete operator-facing product surface.
- Managed subagents предоставляют durable background execution, live control-message delivery, live runtime cancellation, hosted/UI orchestration или write-set sandboxing.
- Builder 2.0 является public complete authoring system, deploy authority, provider/runtime account registry, live managed-subagent orchestrator или proof, что builder-authored agents являются integrated product flows.
- Stable public CLI/API compatibility существует за пределами bounded Stage 10 freeze, including for experimental commands or JS/TS imports.

## Unlock Rules

Forbidden claim может быть разблокирован только когда выполнено все:

1. более поздний Part 1 stage явно включает capability в scope;
2. owner document называет точное user-visible behavior и non-goals;
3. tests покрывают expected success, failure и unsupported cases;
4. live или artifact evidence существует, если claim зависит от реального runtime, provider, deployment surface или distribution artifact;
5. rollback, cleanup или recovery доказаны там, где claim подразумевает operational responsibility;
6. failed, blocked, inconclusive и superseded attempts остаются видимыми;
7. README и user-facing docs используют тот же bounded language, что и updated scope lock.

Evidence должно жить в durable documentation owner, например evidence log, runbook, release-scope update, ADR или acceptance test. Одного task summary недостаточно.

## Связь со stages 2-10 из Part 1

Stage 1 не выбирает public launch scope. Он создает baseline, который поздние stages должны либо удовлетворить, либо оставить deferred.

- Stage 2 может сузить, отложить или выбрать public-launch scope, но не должен стирать этот forbidden-claim lock.
- Stage 3 должен решить security, privacy и legal prerequisites до claims о public distribution или hosted behavior.
- Stage 4 должен доказать каждый выбранный distribution artifact, а не выводить artifact readiness из repository gates.
- Stage 5 должен явно сертифицировать supported runtime/App Server surfaces.
- Stage 6 должен productize memory только для provider behavior с реальным cleanup, reliability и support evidence.
- Stage 7 должен доказать full interaction semantics перед расширением user-facing interaction claims.
- Stage 8 фиксирует bounded local CLI managed-subagent operator surface; later stages должны доказать durable background execution, live cancellation/control delivery и complete review/fix orchestration перед расширением broader claims.
- Stage 9 должен доказать Builder 2.0 на stable public contracts перед расширением public authoring claims.
- Stage 10 freezes only bounded CLI/API compatibility surface, named in [Stable CLI/API Contract Freeze](./stable-cli-api-contract-freeze.md); any expansion needs a later owner update and evidence.
