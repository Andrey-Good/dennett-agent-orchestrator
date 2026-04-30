# Final Public Launch Gate Decision

Status: canonical Stage 17 final gate plus accepted 2026-04-30 Stage 19 local RC evidence and repository public-preview evidence. Final decision: OSS v0.1 public launch blocked / local-package-readiness-only. Repository public-preview status: `repository-public-preview` achieved; package release status: `oss-v0.1-release` blocked.

Current public repository baseline commit: `3ddcb5e70a25969b492108c0cb33e695b87137ed`
Current public repository state: GitHub API reported `private: false`, `visibility: public`; `git ls-remote origin HEAD refs/heads/main` returned `3ddcb5e70a25969b492108c0cb33e695b87137ed` for both remote `HEAD` and `refs/heads/main`; local `HEAD` matched `origin/main` at the time of verification.
Previous local evidence baseline commit: `4085d647d03098ade18a3d1412333a08e55c8156`
Previous local evidence baseline state: tracked docs/evidence baseline with `pnpm release-candidate:check` passing locally; `release-artifacts/` remains local-only and untracked.
Accepted local RC artifact source baseline: `c52ad7f97f56a2dd155562af303b176db6ee6db5`
Previous reviewed final public-launch baseline commit: `c03c9ceb3141d4354026190bab79e68262508b75`
Package version: `0.1.0-rc.1`
Package privacy: `private: true`
Decision date: `2026-04-29`; local evidence rerun date: `2026-04-30`; repository public-preview verification date: `2026-04-30`
Decision owner: `TASK-OSS-LAUNCH-06 final gate worker`; local evidence rerun owner: `2026-04-30-stage19-local-release-evidence-worker`; public-preview docs owner: `2026-04-30-public-preview-status-docs-worker`
Public repository accessibility evidence: `git ls-remote https://github.com/Andrey-Good/dennett-agent-orchestrator HEAD` returned remote HEAD `716f694819c1e84af8de2dd6de46d913001d1e67` on 2026-04-29.
Current remote state evidence: GitHub API returned `private: false` and `visibility: public`; `git ls-remote origin HEAD refs/heads/main` returned `3ddcb5e70a25969b492108c0cb33e695b87137ed` for both remote `HEAD` and `refs/heads/main` on 2026-04-30, matching local `HEAD` at that time.

## Decision

`repository-public-preview` is approved as achieved: the GitHub repository is public, source-checkout onboarding may be shown to early technical users, and claims must stay inside the documented local checkout boundaries.

`oss-v0.1-release` is not approved.

The repository may continue bounded local checkout and local package-readiness work. The selected launch shape remains CLI/package-first, but public npm publication, public package-install claims, and full OSS v0.1 release claims require the blockers below to be replaced by durable evidence and a later explicit approval decision.

This decision does not approve public npm publication, public registry installation, package namespace ownership, hosted or managed deployment, SaaS operation, general availability, production readiness, completed external beta, public provenance, retained SBOM publication, signed artifacts, release tags, pushed commits, GitHub releases, or any change from `private: true`.

`package.json` `"private": true` is an npm/package-publication guard. It does not mean the GitHub repository is private. It must remain in place until the explicit removal gate below is closed.

The remote HEAD check proves the recorded public GitHub main/HEAD state at the time it was run. It does not approve npm/package launch, include untracked local `release-artifacts/`, or replace package, beta, supply-chain, registry, and publication evidence.

## Current Evidence Matrix

| Gate | Evidence status | Decision effect |
| --- | --- | --- |
| Repository and local CLI readiness | Historical local-scope release evidence exists for commit `c3ad3eafca28f4a602a6e44d1861054aabc96a03`. The accepted 2026-04-30 local evidence baseline is commit `4085d647d03098ade18a3d1412333a08e55c8156`; `pnpm release-candidate:check` passes locally. Current public repository baseline is `3ddcb5e70a25969b492108c0cb33e695b87137ed`. | Supports `repository-public-preview` and bounded local checkout/package-readiness work only; does not approve registry publication or public install claims. |
| Local RC artifact evidence | [Local RC Artifact Evidence - 2026-04-30](./local-rc-artifact-evidence-2026-04-30.md) records local-only tarball, SPDX SBOM, and SHA256SUMS evidence for source baseline `c52ad7f97f56a2dd155562af303b176db6ee6db5`, with artifact hashes retained under local `release-artifacts/`. | Supports local artifact inspection only; does not prove public registry publication, public artifact attachment, provenance, signing, or public install. |
| Public GitHub repository accessibility | GitHub API reported `private: false`, `visibility: public`; `git ls-remote origin HEAD refs/heads/main` returned `3ddcb5e70a25969b492108c0cb33e695b87137ed` for both remote `HEAD` and `refs/heads/main` on 2026-04-30. | Proves `repository-public-preview`; untracked local `release-artifacts/` are still not public release evidence. |
| Package metadata and publication state | `package.json` has repository, issue-routing, homepage, and discovery metadata, but remains `private: true` with version `0.1.0-rc.1`. | `private: true` blocks public npm publication and public registry install claims; it is not a GitHub repository visibility flag. |
| Package identity and registry | [Package Identity And Registry](./package-identity-and-registry.md) records no public namespace ownership proof, no approved `npm publish`, and no public registry install path. | Blocks package/public registry launch approval. |
| Local tarball distribution proof | [Stage 11 Distribution Proof](./distribution-proof.md) records controlled local `.tgz` install/uninstall proof, optional explicit two-tarball upgrade/rollback proof, and local SBOM validation. | Allows local proof claims only; does not prove public registry or retained release artifacts. |
| Supply chain | [Supply Chain Attestation](./supply-chain-attestation.md) records local SBOM validation but no retained canonical SBOM, no npm provenance, no signing, and no artifact hash manifest. | Blocks public provenance, signing, retained SBOM, and public artifact integrity claims. |
| User/admin release settings | [Release Settings User Checklist](./release-settings-user-checklist.md) records required external actions for npm ownership, npm Trusted Publisher, GitHub `npm-production` environment approval, protected release tags, and final release approval. | Blocks publication until external settings are configured and proven by named commands or evidence records. |
| External beta | [External Beta Readiness](./external-beta-readiness.md) remains `external-beta-not-run`; no real external participants, dated sessions, accepted beta evidence, or beta-exit review are recorded. | Blocks completed-beta, beta-user validation, and public-readiness approval claims. |
| Hosted/managed deployment | [Hosted And Managed Deployment Scope](./hosted-managed-deployment-scope.md) explicitly defers hosted and managed deployment. | Hosted/SaaS/production claims remain forbidden; this is not an OSS v0.1 CLI/package blocker if the later approval keeps launch non-hosted. |
| Public docs and onboarding | [Public Docs, Onboarding, And Claims](./public-docs-onboarding-and-claims.md) permits clean-checkout and local tarball proof wording only inside documented claim boundaries. | Allows bounded onboarding language; blocks launch, package, hosted, and production claims without matching evidence. |

## Remaining Blockers

### Package/Public Registry Evidence

- `package.json` still has `private: true`.
- `package.json` version is the private release-candidate version `0.1.0-rc.1`; no final public version approval is recorded.
- Public package routing and discovery metadata is present, but publication remains blocked by `private: true`, lack of final public version approval, and missing registry proof.
- No npm namespace or package ownership proof is recorded.
- No approved `npm publish`, public package page, or equivalent public registry proof is recorded.
- No public registry install, upgrade, uninstall, or rollback proof is recorded.

### `private: true` Removal Gate

Do not remove `"private": true` until a later explicit release-preparation decision records:

- npm package name and ownership proof for `dennett-agent-orchestrator`, or an approved package rename;
- final prerelease or release version approval;
- exact source commit, tag, and release notes approval;
- configured or explicitly approved publication path, preferably npm Trusted Publisher/OIDC rather than a long-lived `NPM_TOKEN`;
- package metadata, `bin`, `files`, `exports`, README, and packlist review;
- minimal supply-chain posture: retained SBOM plus `SHA256SUMS`, npm provenance preferred, and signing either explicitly deferred or implemented;
- post-publish public install, CLI smoke, uninstall, and later upgrade/rollback proof plan.

### Current Local Candidate Gate

- The accepted 2026-04-30 local evidence baseline records current commit `4085d647d03098ade18a3d1412333a08e55c8156`, package `0.1.0-rc.1`, and `private: true`.
- `pnpm public-release-foundation:check` passed while still reporting OSS v0.1 public launch `BLOCKED`.
- `pnpm packlist:check` passed and validated 94 package files.
- `pnpm release-candidate:check` passed locally.
- Local tarball, SPDX SBOM, and SHA256SUMS hashes are recorded in [Local RC Artifact Evidence - 2026-04-30](./local-rc-artifact-evidence-2026-04-30.md), while `release-artifacts/` remains untracked local evidence.

### External Beta Evidence

- Stage 16 external beta remains `not-run`.
- No external participant roster aliases, dated beta sessions, accepted workflow evidence, beta bug-bar review, or beta-exit decision are recorded.
- External beta is deferred for `repository-public-preview`; it remains relevant before stronger public-readiness or full `oss-v0.1-release` claims.

### Supply-Chain Evidence

- No retained canonical SBOM path or release attachment is recorded.
- npm provenance remains deferred.
- Package signing remains deferred.
- No artifact hash manifest is recorded for a public release artifact.

### User/Admin Release Settings

- npm package ownership and publisher authority are not recorded.
- npm Trusted Publisher settings for `Andrey-Good/dennett-agent-orchestrator`, workflow `release.yml`, and environment `npm-production` are not recorded as configured or proven.
- GitHub `npm-production` environment reviewers and deployment tag restrictions are not recorded as configured.
- Protected release tag rules for the selected `v*` release pattern are not recorded as configured.
- No final release approval records the exact public version, source commit, tag, publish run, or post-publish verification owner.

### Documentation And Metadata

- Public launch docs must continue to use the explicit blocked decision above until the package, beta, and supply-chain blockers have durable evidence.
- Public docs must keep local checkout, local tarball, live runtime, provider-backed, and hosted/managed claims separated.
- Release notes, changelog/versioning policy, final version approval, and public install documentation are not yet recorded for an approved OSS v0.1 release.

## Allowed Claims While Blocked

- The repository can document local source-checkout onboarding.
- The repository can state that `repository-public-preview` is achieved at public `origin/main` commit `3ddcb5e70a25969b492108c0cb33e695b87137ed`.
- Local package proof may be described only as controlled local `.tgz` install/uninstall, explicit local two-tarball upgrade/rollback harness behavior, and local SBOM validation.
- Runtime, memory, interaction, builder, and managed-subagent capabilities may be described only at their documented limited/local evidence levels.
- Hosted and managed deployment may be described only as deferred and out of current OSS v0.1 scope.
- Public-launch readiness work may continue as planning, local proof, blocker removal, and claim-boundary hardening.

## Forbidden Claims While Blocked

Do not claim:

- OSS v0.1 public launch approval, public-readiness approval, general availability, production readiness, production load, SLA, or hosted/managed service readiness;
- public npm availability, public registry installation, package publication, registry ownership, installer/container distribution, signing, provenance, retained SBOMs, or public rollback;
- completed external beta, beta-user validation, or public user validation;
- full App Server certification, broad provider reliability, native App Server memory, complete user interaction layer, complete managed-subagent product readiness, or complete public Builder 2.0 readiness.

## Future Approval Requirements

A later decision may approve OSS v0.1 public launch only after it records all of the following:

- package privacy change approval, final version approval, registry ownership proof, and public install/upgrade/uninstall/rollback proof;
- user/admin release settings evidence for npm Trusted Publisher, npm ownership, GitHub environment approval, protected release tags, and exact release approval as defined in [Release Settings User Checklist](./release-settings-user-checklist.md);
- retained SBOM, provenance/signing decision or implementation, artifact hashes, and publication attachment policy;
- completed external beta with real external participants, dated workflow evidence, privacy-safe artifacts, bug-bar triage, and accepted exit review;
- explicit non-hosted OSS v0.1 scope or a separate hosted/managed deployment evidence set if hosted claims are added;
- release notes, versioning policy, changelog expectations, and user-facing install documentation tied to the selected public artifact;
- updated release decision record tied to the then-current evidence baseline and explicit public-launch decision;
- automated claim guard passing against README, docs, package metadata, and evidence records.
