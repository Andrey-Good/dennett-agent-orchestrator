# Final Public Launch Gate Decision

Status: canonical Stage 17 final gate. Final decision: OSS v0.1 public launch blocked / local-package-readiness-only.

Current reviewed baseline commit: `c03c9ceb3141d4354026190bab79e68262508b75`
Package version: `0.0.0`
Package privacy: `private: true`
Decision date: `2026-04-29`
Decision owner: `TASK-OSS-LAUNCH-06 final gate worker`

## Decision

OSS v0.1 public launch is not approved.

The repository may continue bounded local checkout and local package-readiness work. The selected launch shape remains CLI/package-first, but public npm publication and public launch claims require the blockers below to be replaced by durable evidence and a later explicit approval decision.

This decision does not approve public npm publication, public registry installation, package namespace ownership, hosted or managed deployment, SaaS operation, general availability, production readiness, completed external beta, public provenance, retained SBOM publication, signed artifacts, release tags, pushed commits, GitHub releases, or any change from `private: true`.

## Current Evidence Matrix

| Gate | Evidence status | Decision effect |
| --- | --- | --- |
| Repository and local CLI readiness | Historical local-scope release evidence exists for commit `c3ad3eafca28f4a602a6e44d1861054aabc96a03`; current repository gates and docs continue to describe local checkout and local package proof boundaries. | Supports bounded local checkout and local package-readiness work only. |
| Package metadata and publication state | `package.json` has repository, issue-routing, homepage, and discovery metadata, but remains `private: true` with version `0.0.0`. | Blocks public npm publication and public registry install claims. |
| Package identity and registry | [Package Identity And Registry](./package-identity-and-registry.md) records no public namespace ownership proof, no approved `npm publish`, and no public registry install path. | Blocks package/public registry launch approval. |
| Local tarball distribution proof | [Stage 11 Distribution Proof](./distribution-proof.md) records controlled local `.tgz` install/uninstall proof, optional explicit two-tarball upgrade/rollback proof, and local SBOM validation. | Allows local proof claims only; does not prove public registry or retained release artifacts. |
| Supply chain | [Supply Chain Attestation](./supply-chain-attestation.md) records local SBOM validation but no retained canonical SBOM, no npm provenance, no signing, and no artifact hash manifest. | Blocks public provenance, signing, retained SBOM, and public artifact integrity claims. |
| External beta | [External Beta Readiness](./external-beta-readiness.md) remains `external-beta-not-run`; no real external participants, dated sessions, accepted beta evidence, or beta-exit review are recorded. | Blocks completed-beta, beta-user validation, and public-readiness approval claims. |
| Hosted/managed deployment | [Hosted And Managed Deployment Scope](./hosted-managed-deployment-scope.md) explicitly defers hosted and managed deployment. | Hosted/SaaS/production claims remain forbidden; this is not an OSS v0.1 CLI/package blocker if the later approval keeps launch non-hosted. |
| Public docs and onboarding | [Public Docs, Onboarding, And Claims](./public-docs-onboarding-and-claims.md) permits clean-checkout and local tarball proof wording only inside documented claim boundaries. | Allows bounded onboarding language; blocks launch, package, hosted, and production claims without matching evidence. |

## Remaining Blockers

### Package/Public Registry Evidence

- `package.json` still has `private: true`.
- `package.json` version remains the pre-publication placeholder `0.0.0`.
- Public package routing and discovery metadata is present, but publication remains blocked by `private: true`, the placeholder version, and missing registry proof.
- No npm namespace or package ownership proof is recorded.
- No approved `npm publish`, public package page, or equivalent public registry proof is recorded.
- No public registry install, upgrade, uninstall, or rollback proof is recorded.

### External Beta Evidence

- Stage 16 external beta remains `not-run`.
- No external participant roster aliases, dated beta sessions, accepted workflow evidence, beta bug-bar review, or beta-exit decision are recorded.

### Supply-Chain Evidence

- No retained canonical SBOM path or release attachment is recorded.
- npm provenance remains deferred.
- Package signing remains deferred.
- No artifact hash manifest is recorded for a public release artifact.

### Documentation And Metadata

- Public launch docs must continue to use the explicit blocked decision above until the package, beta, and supply-chain blockers have durable evidence.
- Public docs must keep local checkout, local tarball, live runtime, provider-backed, and hosted/managed claims separated.
- Release notes, changelog/versioning policy, final version approval, and public install documentation are not yet recorded for an approved OSS v0.1 release.

## Allowed Claims While Blocked

- The repository can document local source-checkout onboarding.
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
- retained SBOM, provenance/signing decision or implementation, artifact hashes, and publication attachment policy;
- completed external beta with real external participants, dated workflow evidence, privacy-safe artifacts, bug-bar triage, and accepted exit review;
- explicit non-hosted OSS v0.1 scope or a separate hosted/managed deployment evidence set if hosted claims are added;
- release notes, versioning policy, changelog expectations, and user-facing install documentation tied to the selected public artifact;
- updated release decision record tied to the then-current evidence baseline and explicit public-launch decision;
- automated claim guard passing against README, docs, package metadata, and evidence records.
