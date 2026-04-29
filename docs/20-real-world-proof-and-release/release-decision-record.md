[English](#english) | [Russian](#russian)

<a id="english"></a>
# Release Decision Record

Status: historical Phase 19 decision for commit `c3ad3eafca28f4a602a6e44d1861054aabc96a03`. This record remains valid only for bounded `local-cli-repository-readiness` at that commit and is not current OSS v0.1 public launch approval. The current Stage 17 final gate is [Final Public Launch Gate Decision](../21-public-launch-readiness/final-public-launch-gate-decision.md), which records OSS v0.1 public launch blocked / local-package-readiness-only for commit `c03c9ceb3141d4354026190bab79e68262508b75`.

The historical canonical release target is locked in [Release Scope Lock](./release-scope-lock.md) as `local-cli-repository-readiness` on candidate commit `c3ad3eafca28f4a602a6e44d1861054aabc96a03`. Reviewed TASK-287 live runtime smoke evidence, TASK-290 stress/regression evidence, TASK-291 local operational/recovery/cleanup evidence, TASK-292 bilingual documentation cleanup, TASK-333 narrow Stage 2 runtime-memory proof, TASK-357 verified scoped Mem0 cleanup proof, TASK-384 deterministic provider matrix proof, TASK-386 final Stage 4 repository gates, TASK-394 build-local distribution proof, TASK-400 final Stage 5 gates, TASK-402 Mem0 Windows test hardening, TASK-434 Stage 7 local/offline integrated-flow gates, TASK-450 Stage 8 deterministic local stress/recovery/regression gates, TASK-454 Stage 8 final gate rerun after the crash/reopen recovery completion fix, and TASK-495 final clean local release-candidate gates support only the narrowed local scope; TASK-402 and the final TASK-495 gate set supersede the TASK-400 `pnpm test` blocker. Hosted/managed deployment, npm/public package publication, installer/container distribution, production SaaS/readiness/load, live provider stress/reliability, broad runtime-memory/provider support, native App Server memory, full App Server certification, durable provider cleanup beyond verified scoped Mem0 namespace cleanup, true restore / graph-store/provider-wide cleanup, public Builder 2.0 readiness, full user interaction layer, completed external beta, and operator-facing managed subagent product readiness remain deferred and must not be represented as proven.

For OSS v0.1 launch planning, this historical record is source evidence for local CLI/repository readiness only. It must not be read as public launch approval, but its hosted/managed deferrals are forbidden hosted/commercial claims rather than required implementation work for a later explicitly non-hosted OSS repository/package launch.

## Decision Summary

The selected bounded release target is [Release Scope Lock](./release-scope-lock.md) target `local-cli-repository-readiness`: a taggable repository state for contributors and local users that includes the source tree, contracts, documentation, examples, tests, and a build-local `dist` CLI generated from the checkout with `pnpm build`. Generated `dist` is not tracked source and is not promised to be already present in a clean checkout. It excludes package publication, installers, containers, hosted service deployment, managed deployment, commercial support, and production SaaS claims.

```yaml
decision_id: P19-RDR-2026-04-25-TASK-495
date: "2026-04-25"
decision: release
release_target: "local-cli-repository-readiness"
scope_lock: "docs/20-real-world-proof-and-release/release-scope-lock.md"
version:
  commit: "c3ad3eafca28f4a602a6e44d1861054aabc96a03"
  package: "0.0.0"
  schema: "current repository contracts"
decision_owner: "TASK-495 final release-doc update from reviewed TASK-494 clean candidate-gate evidence"
reviewers:
  - "TASK-287 live Codex App Server smoke review passed"
  - "TASK-290 stress/regression review passed"
  - "TASK-291 operational/rollback evidence review passed"
  - "TASK-292 bilingual documentation cleanup review passed"
  - "TASK-386 final Stage 4 repository gates passed; targeted-command instability documented"
  - "TASK-394 build-local distribution checks passed"
  - "TASK-400 final Stage 5 distribution gates passed, but full pnpm test gate failed in Mem0-backed Windows Chroma/SQLite tests"
  - "TASK-402 Mem0 Windows Chroma/SQLite test hardening passed the targeted Mem0 suites, full pnpm test, lint, and typecheck"
  - "TASK-434 Stage 7 local/offline integrated-flow targeted tests and full gates passed"
  - "TASK-450 Stage 8 deterministic local stress/recovery/regression targeted tests and full gates passed"
  - "TASK-454 Stage 8 final gates rerun passed after crash/reopen recovery completion proof was extended"
  - "TASK-494/TASK-495 final candidate gates passed on clean HEAD c3ad3eafca28f4a602a6e44d1861054aabc96a03"
scope_included:
  - "repository release gates"
  - "build-local dist CLI artifact proof"
  - "runtime discovery and environment inspection"
  - "minimal Codex runtime graph smoke through the local CLI"
  - "direct local Mem0 provider readiness/proof"
  - "verified scoped Mem0 namespace cleanup proof for the direct local provider path"
  - "narrow Stage 2 Codex runtime-memory proof with registered local Mem0 and prompt-rendered memory context"
  - "local graph-runner stress and regression proof with stub runtime"
  - "deterministic Stage 8 local recovery, SQLite pressure, and event-dispatch regression coverage, including explicit retry/resume completion and exactly-once final output after crash/reopen"
  - "deterministic provider reliability matrix with a local stub runtime and no latency gates"
  - "deterministic local/offline integrated-flow automated coverage"
  - "local CLI setup, disposable SQLite recovery, cleanup, and rollback classification"
scope_deferred:
  - "broader runtime-memory behavior beyond the narrow registered local Mem0 plus Codex prompt-rendering path"
  - "native App Server memory and full App Server certification"
  - "hosted or managed deployment and rollback"
  - "npm/public package publication, installer, container, or other artifact deployment and rollback"
  - "production SaaS/readiness/load and commercial support claims"
  - "durable external provider data cleanup and rollback beyond disposable local proof data and verified scoped Mem0 namespace cleanup"
  - "true restore, graph-store cleanup, and provider-wide cleanup"
  - "external provider reliability, throttling, volume claims, and live provider stress beyond the dated live smoke and direct local provider proofs"
  - "full user interaction layer beyond current local evidence"
  - "managed subagent orchestration as an operator-facing product surface"
  - "public Builder 2.0 readiness and live integrated multi-feature product flows as public authoring workflows"
blocking_items:
  - "No unresolved local CLI/repository release-gate blocker remains after TASK-402 superseded the TASK-400 Mem0-backed Windows Chroma/SQLite `pnpm test` failure and TASK-495 passed the final clean candidate gate set."
accepted_residual_risks:
  - "The local stress proof uses a stub runtime and does not prove external provider throttling reliability."
  - "The deterministic provider matrix proves Core behavior for scripted provider-style failures, resume, drain, and final-output integrity only; it does not prove live provider throttling or latency behavior."
  - "The Stage 2 runtime-memory proof covers only registered local Mem0, prompt-rendered Codex context, and success-only provider writes; it does not prove native App Server memory."
  - "The TASK-357 provider-operations proof covers only verified scoped delete for a configured Mem0 namespace and explicit scope; it does not prove true restore, graph-store cleanup, provider-wide cleanup, or provider reliability."
  - "The initial TASK-333 default `pnpm test` gate failed in this environment because existing Mem0-backed tests exceeded their test-local timeouts and hit Windows SQLite cleanup locks; the TASK-334 review rerun passed and supersedes that failure as the current default-gate status."
  - "The TASK-400 final `pnpm test` failure is preserved as failed evidence but superseded by TASK-402 and the final TASK-495 candidate gates. The hardened default suite still contains real local Mem0/Chroma tests and remains a local provider proof, not an external provider reliability or hosted/package readiness proof."
  - "The TASK-434 Stage 7 gates prove deterministic local/offline integrated-flow behavior only. They do not prove live external provider behavior, hosted deployment, managed deployment, or public integrated authoring readiness."
  - "The TASK-454 Stage 8 gate rerun proves deterministic local stress/recovery/regression behavior only, including explicit retry/resume completion and exactly-once final output after crash/reopen. It does not prove automatic live crash recovery, live provider stress, production-scale load, hosted deployment, or external provider reliability."
  - "The operational proof covers disposable local SQLite backup/restore and cleanup only, not hosted/package rollback or durable external provider cleanup."
evidence_items:
  - "P19-2026-04-24-GATES-001"
  - "P19-2026-04-24-RUNTIME-DISCOVERY-001"
  - "P19-2026-04-24-LIVE-SMOKE-001"
  - "P19-2026-04-24-LIVE-SMOKE-002"
  - "P19-2026-04-24-MEM0-001"
  - "P19-2026-04-24-MEM0-002"
  - "P19-2026-04-24-STRESS-001"
  - "P19-2026-04-24-REGRESSION-002"
  - "P19-2026-04-24-OPS-CONFIG-001"
  - "P19-2026-04-24-OPS-RECOVERY-001"
  - "P19-2026-04-24-ROLLBACK-001"
  - "P19-2026-04-25-STAGE2-GATES-001"
  - "P19-2026-04-25-STAGE2-GATES-002"
  - "P19-2026-04-25-STAGE2-MEMORY-001"
  - "P19-2026-04-25-STAGE2-MEMORY-002"
  - "P19-2026-04-25-STAGE3-GATES-001"
  - "P19-2026-04-25-STAGE3-CLEANUP-001"
  - "P19-2026-04-25-STAGE3-CLEANUP-002"
  - "P19-2026-04-25-STAGE4-PROVIDER-MATRIX-001"
  - "P19-2026-04-25-STAGE4-GATES-001"
  - "P19-2026-04-25-STAGE4-TARGETED-RERUN-001"
  - "P19-2026-04-25-STAGE5-DIST-001"
  - "P19-2026-04-25-STAGE5-FINAL-GATES-001"
  - "P19-2026-04-25-STAGE5-MEM0-HARDENING-001"
  - "P19-2026-04-25-STAGE7-FINAL-GATES-001"
  - "P19-2026-04-25-STAGE8-FINAL-GATES-001"
  - "P19-2026-04-25-STAGE8-FINAL-GATES-RERUN-001"
  - "P19-2026-04-25-RELEASE-CANDIDATE-001"
```

## Release Criteria

The decision may be `release` only when:

- all release-scope live proof scenarios pass or have an approved equivalent live proof;
- stress and regression evidence covers the release-scope capabilities;
- operational setup, observability, incident response, cleanup, and rollback are proven or explicitly accepted as residual risk;
- no unresolved `blocks-release` evidence item remains;
- user-visible limitations and deferred capabilities are documented;
- reviewers agree that the evidence supports the selected scope.

## Block Criteria

The decision must be `block` when:

- a required proof category is missing for an included capability;
- a live proof fails or is inconclusive and the capability remains in scope;
- operational rollback or recovery cannot be performed for the release target;
- evidence cannot be audited because artifacts are missing or unredacted data cannot be shared safely;
- a release-blocking defect has no accepted mitigation.

## Defer Criteria

The decision may be `defer` only when:

- the deferred capability, provider, runtime, scenario, or operational promise is removed from release scope;
- users and maintainers can see the limitation;
- evidence explains why the deferred area is not ready;
- a follow-up owner and expected proof path are named;
- remaining in-scope evidence still satisfies release criteria.

## Deferred Follow-Up Owners And Proof Paths

Each deferred scope below must stay outside release claims until its owner records the expected proof path in the evidence log and a later decision moves it into scope.

| Deferred scope | Follow-up owner | Expected proof path before inclusion | Rollback or cleanup expectation | User-visible limitation |
| --- | --- | --- | --- | --- |
| Broader runtime-memory behavior beyond the narrow Stage 2 proof | Memory integration owner and Runtime integration owner | Prove additional providers, native runtime memory primitives if a runtime exposes them, provider cleanup/idempotency behavior, retry semantics, and broader runtime/model option combinations beyond the TASK-333 registered local Mem0 plus Codex prompt-rendering path. | Clean provider proof data or use a disposable namespace; preserve redacted evidence that only intended proof data was removed. | The current graph proof is narrow: registered local Mem0, prompt-rendered `memory_context`, and success-only provider writes. It is not native App Server memory or broad provider readiness. |
| Hosted service deployment and rollback | Release engineering owner and Operations owner | Define the hosted deployment artifact and environment, deploy the recorded version, run hosted smoke and observability checks, roll back to a known previous version or disabled-capability mode, and verify state integrity plus post-rollback health. | Hosted rollback must be executed and audited before hosted readiness is claimed. | No hosted service operation, uptime, multi-tenant support, or hosted rollback promise exists in this release target. |
| npm/package publication, installer, container, or other artifact deployment and rollback | Release engineering owner | Define the artifact type and publish/install path, build from a recorded commit, install in a disposable environment, run CLI smoke and configuration inspection, then uninstall or roll back to a previous artifact and verify the prior version runs. | Artifact-specific uninstall, rollback, or previous-version verification is mandatory before inclusion. | Users should not expect an npm package, installer, Docker image, or packaged upgrade path. |
| Durable external provider data cleanup and rollback beyond disposable local proof data and verified scoped Mem0 namespace cleanup | Memory integration owner and Provider operations owner | Use an isolated provider namespace/account, create durable test data, prove backup or cleanup semantics beyond bounded scoped delete, verify via provider read/search/list that only intended proof data was removed or restored, and preserve redacted provider evidence. | Removal or restoration must be provider-verified; disposable local cleanup and TASK-357 scoped namespace cleanup are insufficient for true restore or provider-wide cleanup claims. | Durable external provider data cleanup beyond the configured namespace and explicit scope is not covered by the current cleanup guarantee. |
| External provider reliability, throttling, and volume claims beyond dated live smoke, direct local provider proofs, and deterministic stub-provider matrix | Runtime integration owner and Provider reliability owner | Run a quota-safe provider test matrix covering throttling, transient failures, retry behavior, volume limits, and final output integrity against the target providers; record metrics, request IDs when safe, and controlled-failure evidence. Use TASK-384 as local semantics coverage, not as a substitute for live provider proof. | Failed or throttled runs must leave no unbounded retry or cleanup debt. | Current evidence proves local failure semantics only and does not guarantee provider reliability under load. |
| Full user interaction layer beyond current local evidence | Interaction owner and CLI owner | Prove blocked prompts, replies, resume-after-reply, and risky mid-run change policy across CLI and supported runtime state. | Clean prompt/reply proof state and preserve redacted interaction evidence. | User chat and resume behavior must be described only at the proven local/focused level. |
| Managed subagent orchestration as an operator-facing product surface | Managed subagent owner and QA owner | Prove create/send/wait/status/close/cancel, roles, write-scope ownership, review/fix loops, lineage, and cancellation through user-visible flows. | Close or cancel child work and verify no leaked active child runs. | Managed subagents remain partially implemented and not broadly release-proven. |
| Builder 2.0 and integrated multi-feature product flows as public authoring workflows | Builder owner and Product integration owner | Prove builder output through validation, lifecycle publication, execution, memory/runtime/interaction/subagent boundaries, and integrated acceptance scenarios. | Clean drafts, live revisions, and proof state or prove safe rollback. | Builder and integrated flows must not be marketed as a complete public authoring system. |

## Decision Narrative

The final Phase 19 decision is bounded `release` for `local-cli-repository-readiness`.

This is the strongest truthful outcome for the current evidence. `block` is no longer accurate for the narrowed local CLI/repository scope because the reviewed upstream evidence resolved the previous in-scope blockers: TASK-287 proved the minimal Codex App Server graph smoke through the CLI, TASK-290 proved local graph-runner stress/regression behavior, TASK-291 proved local setup plus disposable SQLite recovery/cleanup and classified hosted/package rollback as `not-run`, TASK-292 verified bilingual documentation cleanup, TASK-402 superseded the TASK-400 Mem0-backed `pnpm test` blocker, and the final TASK-495 candidate gates passed on clean HEAD `c3ad3eafca28f4a602a6e44d1861054aabc96a03`.

This `release` would overclaim if read beyond the locked target. The current evidence does not prove hosted or managed deployment, npm/public package publication, installer/container distribution, production SaaS/readiness/load, live provider stress/reliability, broad runtime-memory/provider support, native App Server memory, full App Server certification, durable provider cleanup beyond TASK-357 scoped namespace cleanup, true restore, graph-store cleanup, provider-wide cleanup, public Builder 2.0 readiness, the full user interaction layer, or operator-facing managed subagent product readiness. Those areas are therefore explicitly removed from the current release scope and remain visible as deferred follow-up work with role owners, expected proof paths, cleanup or rollback expectations, and user-visible limitations.

The historical failed live smoke evidence, `P19-2026-04-24-LIVE-SMOKE-001`, remains preserved as a failed superseded run. It is no longer an unresolved current blocker because the passing retry `P19-2026-04-24-LIVE-SMOKE-002` provides the release-scope live graph proof. The failed Mem0 quoting attempt, `P19-2026-04-24-MEM0-001`, also remains visible as superseded operator evidence.

Hosted/package deployment rollback is not represented as proven. It is deferred because no hosted service, npm/package publication, installer, container, or other deployment artifact exists in the current local CLI/repository release target. If a later decision expands scope to any of those artifacts, artifact-specific deployment and rollback proof becomes release-blocking until completed. If a later decision stays within a non-hosted OSS repository/package scope, hosted rollback and hosted operations proof remain forbidden hosted/commercial claims rather than OSS implementation prerequisites.

## TASK-357 Stage 3 Decision Note

TASK-357 supports only the bounded local CLI/repository release decision. It adds a narrow included proof for verified scoped delete on the direct local Mem0 provider path: preview plus token-confirmed delete removed target namespace records for an explicit user scope, and a control namespace over the same disposable storage survived. This is not true restore, graph-store cleanup, provider-wide cleanup, durable external provider cleanup beyond the verified scope, or provider reliability.

## TASK-384 Stage 4 Decision Note

TASK-384 supports only the bounded local CLI/repository release decision. It adds deterministic local coverage for provider-style throttling, transient failures, interruption, waiting/resume boundaries, bounded concurrent active-execution drain, and final-output integrity through `tests/integration/stage4-provider-reliability.test.ts`. This is stub-runtime evidence only: it does not call live providers, does not use absolute latency gates, and does not prove external provider reliability under real throttling or load.

## TASK-386 Stage 4 Gates Note

TASK-386 supports only the bounded local CLI/repository release decision. It records that the full test suite, lint, typecheck, and build passed after formatter-only cleanup in Stage 4 touched test files. The exact multi-file targeted command passed once, but repeated later reruns failed on existing Mem0-backed `memory-service` tests at hard-coded 60s per-test timeouts with Windows Chroma SQLite cleanup locks; `memory-service.test.ts` passed in isolation and the full `pnpm test` gate passed afterward. This gate evidence supports local CLI/repository confidence only, documents a targeted-command stability risk for final review, and does not add live provider reliability proof.

## TASK-434 Stage 7 Gates Note

TASK-434 supports only the bounded local CLI/repository release decision. It records that the Stage 7 targeted local/offline integrated-flow tests, full test suite, lint, typecheck, and build passed. This supports deterministic local/offline integrated-flow confidence only; it does not add live external integrated-flow proof, hosted/managed deployment proof, external provider reliability proof, or public Builder 2.0 / managed subagent product readiness.

## TASK-450 Stage 8 Gates Note

TASK-450 supports only the bounded local CLI/repository release decision. It records that the Stage 8 targeted deterministic local stress/recovery/regression tests, full test suite, lint, typecheck, and build passed. This supports local state and regression confidence for deterministic stress cleanup, crash/reopen recovery, multi-store SQLite pressure, and repeated/near-concurrent event dispatch only; it does not add live provider stress proof, production-scale load proof, hosted deployment proof, or external provider reliability proof.

## TASK-454 Stage 8 Gates Rerun Note

TASK-454 supports only the bounded local CLI/repository release decision. It records that the Stage 8 targeted command, full test suite, lint, typecheck, and build passed again after the crash/reopen recovery proof was extended. The recovery proof now covers stale in-progress work after a fresh SQLite-store reopen, explicit terminal classification, retry/resume completion, completed run state, and exactly one final output for the recovered node. This remains deterministic local/offline evidence only and does not prove automatic live crash recovery, live provider stress, production-scale load, hosted deployment, or external provider reliability.

## TASK-495 Final Candidate Gate Note

TASK-495 records the authoritative clean candidate-gate evidence for bounded `release` of `local-cli-repository-readiness` on commit `c3ad3eafca28f4a602a6e44d1861054aabc96a03`. The final gate set passed: `pnpm install --frozen-lockfile`, `pnpm typecheck`, `pnpm lint`, `pnpm test` under an explicit `1200000ms` wrapper in `271880ms`, `pnpm build`, `node --no-warnings -e "await import('node:sqlite')"`, `node dist/src/interfaces/cli.js --help`, `pnpm dist:check`, `pnpm packlist:check`, `pnpm package:check`, and `pnpm release-candidate:check`. This does not expand the release beyond local CLI/repository readiness.

## Approval

This record approves only the final Phase 19 bounded `release` decision for the current local CLI/repository release scope. It does not approve OSS v0.1 public launch, public package publication, or any deferred hosted/managed deployment, installer/container distribution, durable external provider, external-provider-reliability, runtime-native memory, full App Server certification, live integrated-flow, live provider stress, production-scale load, commercial support, full user interaction layer, operator-facing managed subagent, or public Builder 2.0 claim.

<a id="russian"></a>
# Russian Translation Status

The previous localized duplicate section was removed because it contained mojibake. The English section above is the canonical public launch record until a reviewed Russian translation is restored.
