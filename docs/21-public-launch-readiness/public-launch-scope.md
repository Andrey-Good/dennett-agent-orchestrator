[English](#english) | [Russian](#russian)

<a id="english"></a>
# Public Launch Scope

Status: canonical Stage 2 OSS v0.1 public-launch scope decision for Part 1. This document selects the public-launch target shape only; it does not claim the target is ready, published, hosted, commercially supported, or generally available.

Related documents:

- [Baseline Gap And Forbidden Claims](./baseline-gap-and-forbidden-claims.md)
- [Release Scope Lock](../20-real-world-proof-and-release/release-scope-lock.md)
- [Release Decision Record](../20-real-world-proof-and-release/release-decision-record.md)
- [Phase 12 Capability Gap Lock](../13-capability-gap-lock/phase-12-capability-gap-lock.md)
- [Release Gates](../11-hardening/release-gates.md)
- [Managed Subagent Productization](./managed-subagent-productization.md)
- [Builder 2.0 Productization](./builder-2-0-productization.md)
- [Stable CLI/API Contract Freeze](./stable-cli-api-contract-freeze.md)
- [Stage 11 Distribution Proof](./distribution-proof.md)
- [Install, Upgrade, Uninstall, And Rollback](./install-upgrade-uninstall-rollback.md)
- [Package Identity And Registry](./package-identity-and-registry.md)
- [Supply Chain Attestation](./supply-chain-attestation.md)
- [Hosted And Managed Deployment Scope](./hosted-managed-deployment-scope.md)
- [Observability, Support, And Operations](./observability-support-operations.md)

## Decision

The selected Part 1 public-launch target is:

`cli-package-first-public-launch`

This target now has two explicitly separated statuses:

- `repository-public-preview`: achieved as of 2026-04-30. The GitHub repository is public, and source-checkout onboarding may be shown to early technical users within the documented local checkout boundaries.
- `oss-v0.1-release`: blocked. This stronger status requires package/npm publication gates, public install proof, retained public artifact evidence, supply-chain decisions, and final release approval.

This means the first public shape is an open-source local product path: a public repository preview now, plus an optional public CLI/package distribution later once package gates pass. Repository checkout support remains available for contributors and early technical users.

The launch target is selected because current evidence supports a local CLI/repository product shape and controlled local package proof better than it supports any hosted, managed, commercial, SLA-backed, or cloud service. Stage 4 owns the release-prep package and supply-chain foundation. Stage 11 owns local `.tgz` distribution proof. Stage 12 owns the canonical hosted/managed deployment deferral lock in [Hosted And Managed Deployment Scope](./hosted-managed-deployment-scope.md). Public registry publication remains blocked until a later release-approval task records registry ownership, publication controls, retained evidence, and public install proof.

As of 2026-04-29, the GitHub repository URL was publicly reachable: `git ls-remote https://github.com/Andrey-Good/dennett-agent-orchestrator HEAD` returned remote HEAD `716f694819c1e84af8de2dd6de46d913001d1e67`. As of the current 2026-04-30 public-preview verification, GitHub API reported `private: false` and `visibility: public`, and `git ls-remote origin HEAD refs/heads/main` returned `3ddcb5e70a25969b492108c0cb33e695b87137ed` for both remote `HEAD` and `refs/heads/main`, matching local `HEAD` at that time. Earlier 2026-04-30 local evidence records documentation/evidence baseline `4085d647d03098ade18a3d1412333a08e55c8156`, passing local `pnpm release-candidate:check`, and local-only untracked `release-artifacts/`. That evidence supports repository preview and local readiness only; it does not approve npm/package release or prove public artifact publication.

The removed `package.json` `"private": true` field was an npm publication guard, not GitHub repository visibility. Removing it prepares the package for a later approved publication path, but [Package Identity And Registry](./package-identity-and-registry.md) and [Release Settings User Checklist](./release-settings-user-checklist.md) still block publication until registry, ownership, approval, and proof gates close.

The OSS v0.1 scope is intentionally non-hosted. Hosted/managed gaps are launch blockers only for hosted or commercial claims; they are not implementation prerequisites for an OSS local CLI/package launch when the public docs explicitly preserve the non-hosted boundary.

## Hosted And Managed Status

Hosted and managed product launch is explicitly deferred. [Hosted And Managed Deployment Scope](./hosted-managed-deployment-scope.md) is the canonical owner for the hosted gap matrix, code/config evidence, and forbidden hosted claims.

Deferred hosted/managed scope includes:

- hosted SaaS operation;
- managed deployments;
- uptime, availability, or service-level promises;
- multi-tenant isolation claims;
- hosted rollback or disablement;
- hosted observability, incident response, and support operations.
- hosted telemetry, audit logs, deletion/export/legal-hold operations, production load readiness, and operational ownership.

Hosted or managed launch may enter scope only through a later scope decision that names the deployment artifact, runtime environment, rollback path, operational owner, security/privacy/legal posture, and live evidence requirements.

## Supported Matrix For The Selected Launch Target

| Area | Stage 2 launch-scope status | Public-launch boundary |
| --- | --- | --- |
| Launch form | Selected target: OSS v0.1 CLI/package-first, split into `repository-public-preview` now and `oss-v0.1-release` later. | Repository public preview is achieved; npm/package release is not ready or published until security/legal, release-engineering, Stage 11 distribution proof, and later release-approval gates pass for the chosen package artifact. |
| Repository visibility | GitHub API reported `private: false`, `visibility: public`; `git ls-remote origin HEAD refs/heads/main` returned `3ddcb5e70a25969b492108c0cb33e695b87137ed` for both remote `HEAD` and `refs/heads/main` on 2026-04-30. | Supports `repository-public-preview`; local-only `release-artifacts/` and package publication claims still require separate evidence. |
| Repository checkout | Supported as contributor and local-user path by the bounded `local-cli-repository-readiness` evidence. | Users build from checkout with `pnpm build`; generated `dist` is not promised in a clean checkout. |
| Package distribution | Local tarball proof exists; public registry publication is not proven. | Stage 11 proves controlled local `.tgz` install/uninstall, explicit two-tarball upgrade/rollback smoke, local SBOM validation, and CI package-proof job configuration. It does not prove public npm publication, signing, provenance, retained SBOMs, or public registry install. |
| Hosted/managed service | Deferred by Stage 12 and outside OSS v0.1 scope. | No hosted, managed, SaaS, uptime, multi-tenant, cloud deployment, hosted telemetry/audit, production load, hosted support operations, status page, SLA, or hosted rollback claim. These gaps block hosted/commercial claims, not the non-hosted OSS scope. |
| Support diagnostics | Local-only support bundle and redacted runtime environment inspection exist for Stage 13. | Diagnostics are generated locally and reviewed by the user before sharing. They do not upload data, create hosted telemetry, or prove managed support operations. |
| Primary OS evidence | Windows local evidence. | Windows is the only evidenced OS baseline for current local release proof. |
| Linux and macOS | CI package-proof jobs are configured as evidence candidates. | No public support claim until green package proof, gates, CLI smoke, and runtime/provider proof are recorded for each claimed OS. |
| Node.js | `package.json` requires `>=22.13.0`; evidence records Node v22.17.1 and `node:sqlite` import proof. | Public launch must require Node.js `>=22.13.0` unless Stage 4 changes package metadata through its own approved task. |
| Package manager | Canonical workflow is `pnpm`; package metadata records `pnpm@10.33.0`. | Contributors and source-checkout users should use `pnpm 10.33.0` or a compatible pnpm 10.x path proven by later evidence. `npm` may be used only where the package/install path explicitly requires it. |
| npm package manager | Consumer-package tool for local tarball proof. | Do not claim npm is the canonical repository workflow. Do not claim npm publication or public `npm install dennett-agent-orchestrator` support before later registry proof and release approval. |
| Runtime provider | Codex App Server adapter path only, with narrow local proof for runtime discovery, environment inspection, and minimal graph execution. | Do not claim full App Server certification, all model/options support, or broad runtime-provider reliability. |
| Memory provider | Direct local Mem0 provider path, plus narrow prompt-rendered Codex memory context and success-only provider writes. | Do not claim native App Server memory, broad memory-provider support, durable cleanup beyond verified scoped Mem0 namespace cleanup, true restore, provider-wide cleanup, or provider reliability. |
| Local state | SQLite local metadata and run state. | SQLite remains local and derivative, not hosted storage or a distributed operational backend. |

## Capability Scope

| Capability | Public-launch classification | Boundary |
| --- | --- | --- |
| CLI execution from installed package or local checkout | Included target for repository checkout and local `.tgz` proof. | Only commands and outputs frozen by Stage 10 may be called stable. Public registry install remains unproven. |
| Agent JSON validation and contract examples | Included target. | Must stay within documented schemas and examples; no hidden builder-only or hosted-only behavior. |
| Local graph execution | Included target for proven local CLI paths. | Does not imply hosted execution, production load, or automatic live crash recovery. |
| Codex App Server runtime | Limited/beta for the certified subset. | Stage 5 must name supported models/options and unsupported cases before public claims expand. |
| Runtime discovery and environment inspection | Limited/beta for local authenticated Codex path. | Redact account data and avoid account/rate-limit promises. |
| Support bundle and redacted diagnostics | Included as local support safety protocols for CLI/package-first triage. | `support-bundle` summarizes package, environment, CLI inventory, git status, and state DB metadata with redaction. `runtime-env-inspect --redacted` is required before sharing runtime diagnostics. Neither command creates a hosted support or telemetry path. |
| Direct local Mem0 memory operations | Limited/beta for registered local Mem0 provider path. | Stage 6 must lock provider limits, cleanup guarantees, reliability boundaries, and unsupported cases. |
| Runtime memory with Codex plus Mem0 | Limited/beta for prompt-rendered context and success-only writes. | Not native App Server memory and not broad provider support. |
| User prompt wait/reply/resume | Limited/beta for currently tested prompt shapes. | Stage 7 must prove full user-visible interaction semantics before broader claims. |
| Managed subagent primitives | Limited/beta for the bounded local CLI operator surface. | Stage 8 supports `subagent-launch`, `subagent-list`, `subagent-show`, `subagent-wait`, `subagent-record-control`, and `subagent-close` only within the limits in [Managed Subagent Productization](./managed-subagent-productization.md). Launch is launch-and-wait only; control and cancellation are state-recorded, not live-delivered. |
| Builder 2.0 authoring | Limited/beta for audited draft-first authoring only. | Stage 9 supports formal builder output wrapper validation, deterministic candidate audit, diagnostics outside Agent JSON, and draft-only persistence as documented in [Builder 2.0 Productization](./builder-2-0-productization.md). It does not prove full public authoring readiness, deploy, provider registration, live managed orchestration, or execution of every draft. |
| Stable CLI/API compatibility | Frozen only for the bounded Stage 10 surface. | Only commands labeled `[stable]`, the `[stable/safety-protocol]` cleanup flow, exported JSON schema artifacts, and the no-stable-JS-API package boundary are stable under [Stable CLI/API Contract Freeze](./stable-cli-api-contract-freeze.md). Experimental commands remain unstable. |
| External beta program | Not run; deferred for repository public preview. | Stage 16 is governed by [External Beta Readiness](./external-beta-readiness.md). Feature-level `Limited/beta` classifications above do not prove external beta completion, participant validation, or full `oss-v0.1-release` approval. |
| Containers, installers, signed binaries, hosted deployments | Deferred. | Each requires a separate artifact, proof, rollback/uninstall path, and security/release decision. |

## Public Claims Allowed After Required OSS Gates

These claims are allowed only after the later Part 1 stages produce the required evidence for the selected non-hosted OSS CLI/package-first target:

- Dennett provides an OSS v0.1 public repository preview plus documented local CLI/package-first checkout path for the supported environment.
- The documented CLI package or install path has passed clean-environment install and smoke validation.
- The documented command set is supported within the Stage 10 compatibility policy.
- Codex App Server integration is supported only for the certified subset named by Stage 5.
- Mem0 memory integration is supported only for the provider behavior named by Stage 6.
- Limitations, unsupported providers, unsupported OSes, and beta/limited features are public and visible.

## Forbidden Claims

Do not claim:

- Dennett is already publicly launched, generally available, fully released, commercially supported, or production ready because Stage 2 selected a launch target.
- Hosted service operation, managed deployment, SaaS readiness, uptime, multi-tenancy, hosted rollback, hosted telemetry/audit readiness, cloud deployment, production hosted/load readiness, or hosted support operations are in scope.
- npm publication, installer distribution, container distribution, signed artifacts, provenance, retained SBOMs, or public package rollback are proven by Stage 11 local tarball proof.
- local support diagnostics imply SLA, managed support, status-page monitoring, automatic telemetry, or hosted incident response.
- Linux or macOS are publicly supported before OS-specific evidence exists.
- Full Codex App Server certification is complete.
- Any non-Codex runtime provider is publicly supported.
- Native App Server memory is implemented or Mem0 is consumed through a native App Server memory primitive.
- Memory behavior is broader than registered local provider resolution, prompt-rendered Codex context, and success-only provider writes.
- Durable external provider cleanup, true restore, graph-store cleanup, provider-wide cleanup, delete-all, throttling behavior, or volume reliability is proven.
- Full user interaction readiness, complete managed-subagent orchestration, durable background subagent execution, live subagent cancellation delivery, complete public Builder 2.0 readiness beyond audited draft-first authoring, stable compatibility for experimental CLI commands, or any stable JS/TS API is complete.
- Feature-level `Limited/beta` classifications prove that a completed external beta program, beta-user validation, or public-readiness approval exists.

## OSS v0.1 Launch Blockers And Deferrals

The OSS v0.1 CLI/package-first public launch remains blocked until all required OSS repository/package gates pass:

- Stage 3 security, privacy, legal, secret-handling, license/package, disclosure, and data-retention decisions are complete.
- Stage 4 records the release-prep package and supply-chain foundation.
- Stage 11 records controlled local `.tgz` install/uninstall proof, explicit two-tarball upgrade/rollback smoke, local SBOM validation, and CI package-proof job configuration.
- A later release-approval task records registry ownership, public publication controls, retained evidence, signing/provenance decisions, and public install proof.
- The release-preparation task records removal of `"private": true` for `0.1.0-rc.1`, intended eventual tag `v0.1.0-rc.1`, npm registry lookup `E404`, local npm auth `ENEEDAUTH`, package metadata/packlist review, minimal supply-chain posture, and post-publish proof plan; actual publication still requires owner/admin approval and evidence.
- The public repository/package decision is tied to the then-current commit, package metadata, artifact hashes, retained evidence, and explicit claim-review result.
- README and user-facing docs use the same OSS v0.1 non-hosted scope language as this document.

The following capabilities are included only as bounded local or limited/beta claims and must stay within their narrower evidence records:

- Stage 5 certifies the exact Codex App Server runtime subset and unsupported model/option cases.
- Stage 6 productizes the exact memory provider boundary and cleanup/reliability limits.
- Stage 7 records user-visible interaction semantics and unsupported prompt/reply cases.
- Stage 8 records the bounded local CLI managed-subagent operator surface and keeps broader orchestration explicitly deferred.
- Stage 9 records the bounded audited draft-first Builder 2.0 authoring surface and keeps complete public Builder readiness explicitly deferred.
- Stage 10 freezes only the bounded public CLI/API contract and compatibility policy documented in [Stable CLI/API Contract Freeze](./stable-cli-api-contract-freeze.md).
- Stage 13 keeps support and operations local-only, documents `support-bundle`, `runtime-env-inspect --redacted`, support/security routing, and incident runbooks in [Observability, Support, And Operations](./observability-support-operations.md).

The following are deferred hosted/commercial capabilities. They must remain forbidden claims, but they are not required implementation work for a non-hosted OSS v0.1 local CLI/package launch:

- Stage 12 keeps hosted/managed deployment explicitly out of current public-launch scope and records the hosted gap matrix in [Hosted And Managed Deployment Scope](./hosted-managed-deployment-scope.md).
- Hosted service operation, managed deployments, cloud deployment, uptime/SLA, status page, hosted telemetry/audit, hosted incident response, production hosted/load readiness, and hosted rollback remain deferred until a later hosted scope decision records implementation and live evidence.
- Commercial support, managed support operations, and long-term service commitments remain deferred unless a later release decision explicitly adds and proves them.

## Decision Criteria For Later Scope Changes

A later stage may expand the scope only when:

1. the expansion names exact user-visible behavior and non-goals;
2. implementation, docs, and tests match the same boundary;
3. artifact, runtime, provider, or hosted claims have live or clean-environment evidence;
4. rollback, uninstall, cleanup, or recovery is proven where the claim implies operational responsibility;
5. failed, blocked, inconclusive, and superseded evidence remains visible;
6. public docs include limitations and forbidden claims with no contradictory marketing language.

<a id="russian"></a>
# Russian Translation Status

The previous localized duplicate section was removed because it contained mojibake. The English section above is the canonical public launch record until a reviewed Russian translation is restored.
