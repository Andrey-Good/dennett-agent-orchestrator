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

## Current OSS v0.1 Gate Snapshot

The later public-launch readiness documents now select a CLI/package-first OSS v0.1 launch shape, but [Final Public Launch Gate Decision](./final-public-launch-gate-decision.md) keeps that launch blocked as `local-package-readiness-only`.

The current blocker categories are:

| Category | Current state | Required before the category can clear |
| --- | --- | --- |
| Package/public registry evidence | `package.json` remains `private: true`, version is `0.0.0`, registry ownership is not proven, and public registry install/upgrade/uninstall/rollback proof is absent. | Final version approval, approved privacy change, registry ownership proof, public artifact proof, and lifecycle proof for the selected public package. |
| External beta evidence | Stage 16 is `external-beta-not-run`; no accepted external participant evidence or beta-exit review exists. | Real external participant sessions, accepted workflow evidence, bug-bar triage, privacy-safe artifacts, and beta-exit decision. |
| Supply-chain evidence | Local SBOM validation exists, but retained SBOM, provenance, signing, and artifact hash manifest evidence are absent. | Retained SBOM and hashes, provenance/signing implementation or explicit release decision, and publication attachment policy. |
| Documentation and metadata | Public docs are bounded, but public install docs, release notes, changelog/versioning policy, and final package metadata are not yet launch-ready. | Public docs and package metadata that match the proven artifact and claim boundaries. |

Hosted and managed deployment remain explicitly deferred. That deferral does not have to block a later non-hosted OSS v0.1 package approval, but it must continue to block hosted, SaaS, managed-service, uptime, SLA, telemetry, and production-load claims.

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
| Stage 4: Release Engineering And Supply Chain | Stage 4 proves only the private package and release-engineering foundation; no npm/public package, installer, container, signed artifact, provenance, or public artifact rollback readiness is proven. | Private packaging boundaries, repository-local release gates, artifact inventory inputs, and reproducible-build expectations. Clean-environment install/deployment proof for selected distribution artifacts, provenance/signing/publication decisions, rollback/uninstall proof, and CI OS parity remain Stage 11 or later release-approval evidence. |
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
- Stage 4 owns the private package foundation only; Stage 11 must prove each selected distribution artifact instead of inferring artifact readiness from repository gates.
- Stage 5 must certify supported runtime/App Server surfaces explicitly.
- Stage 6 must productize memory only for provider behavior that has real cleanup, reliability, and support evidence.
- Stage 7 must prove full interaction semantics before user-facing interaction claims expand.
- Stage 8 records the bounded local CLI managed-subagent operator surface; later stages must prove durable background execution, live cancellation/control delivery, and complete review/fix orchestration before broader claims expand.
- Stage 9 must prove Builder 2.0 on stable public contracts before public authoring claims expand.
- Stage 10 freezes only the bounded CLI/API compatibility surface named in [Stable CLI/API Contract Freeze](./stable-cli-api-contract-freeze.md); any expansion needs a later owner update and evidence.

<a id="russian"></a>
# Russian Translation Status

The previous localized duplicate section was removed because it contained mojibake. The English section above is the canonical public launch record until a reviewed Russian translation is restored.
