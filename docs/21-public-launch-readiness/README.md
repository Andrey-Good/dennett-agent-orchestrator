# Public Launch Readiness

Status: canonical owner section for public-launch readiness planning. No stage expands the current release target or claims public readiness by itself.

This section governs the public-launch readiness path after the bounded `local-cli-repository-readiness` release. It exists to prevent broader public claims from being inferred from local repository, local CLI, or local tarball evidence.

## Documents

- [Public Launch Scope](./public-launch-scope.md)
- [Security, Privacy, And Legal Foundation](./security-privacy-legal-foundation.md)
- [Release Engineering And Supply Chain](./release-engineering-and-supply-chain.md)
- [Runtime App Server Certification](./runtime-app-server-certification.md)
- [Memory Productization](./memory-productization.md)
- [User Interaction Productization](./user-interaction-productization.md)
- [Managed Subagent Productization](./managed-subagent-productization.md)
- [Builder 2.0 Productization](./builder-2-0-productization.md)
- [Stable CLI/API Contract Freeze](./stable-cli-api-contract-freeze.md)
- [Stage 11 Distribution Proof](./distribution-proof.md)
- [Install, Upgrade, Uninstall, And Rollback](./install-upgrade-uninstall-rollback.md)
- [Package Identity And Registry](./package-identity-and-registry.md)
- [Supply Chain Attestation](./supply-chain-attestation.md)
- [Hosted And Managed Deployment Scope](./hosted-managed-deployment-scope.md)
- [Observability, Support, And Operations](./observability-support-operations.md)
- [Public Docs, Onboarding, And Claims](./public-docs-onboarding-and-claims.md)
- [Integrated Public Environment Product Flows](./integrated-public-environment-product-flows.md)
- [External Beta Readiness](./external-beta-readiness.md)
- [Final Public Launch Gate Decision](./final-public-launch-gate-decision.md)
- [Baseline Gap And Forbidden Claims](./baseline-gap-and-forbidden-claims.md)
- [Release Scope Lock](../20-real-world-proof-and-release/release-scope-lock.md)
- [Release Decision Record](../20-real-world-proof-and-release/release-decision-record.md)
- [Phase 12 Capability Gap Lock](../13-capability-gap-lock/phase-12-capability-gap-lock.md)

## Stage List

- Stage 1 locks baseline gaps and forbidden claims.
- Stage 2 selects the CLI/package-first public-launch target and keeps hosted/managed launch deferred.
- Stage 3 records security, privacy, legal, and trust boundaries for CLI/package-first launch.
- Stage 4 records the private package and supply-chain foundation.
- Stage 5 records the limited Runtime/App Server certification subset.
- Stage 6 records the limited local Mem0-first memory productization subset.
- Stage 7 records the bounded CLI/package interaction productization subset.
- Stage 8 records the bounded local CLI managed-subagent operator subset.
- Stage 9 records the bounded audited draft-first Builder 2.0 authoring subset.
- Stage 10 records the bounded stable CLI/API contract freeze.
- Stage 11 records local tarball distribution proof, controlled install/uninstall proof, upgrade/rollback harness boundaries, private package identity, and local SBOM validation.
- Stage 12 records the canonical hosted/managed deployment deferral lock and gap matrix.
- Stage 13 records the local-only support bundle, redacted runtime diagnostics, support/security routing, support matrix, telemetry boundary, and local incident runbooks.
- Stage 14 records public docs, onboarding, examples, and claim-review rules in [Public Docs, Onboarding, And Claims](./public-docs-onboarding-and-claims.md).
- Stage 15 records local integrated public-environment product-flow evidence and claim boundaries in [Integrated Public Environment Product Flows](./integrated-public-environment-product-flows.md).
- Stage 16 records the external beta plan, participant/workflow criteria, bug bar, privacy-safe feedback routing, and `not-run` evidence gate in [External Beta Readiness](./external-beta-readiness.md).
- Stage 17 records the final evidence gate in [Final Public Launch Gate Decision](./final-public-launch-gate-decision.md): public launch blocked / local-package-readiness-only.

## Current Truthful Scope

- The current accepted release state is bounded `release` for `local-cli-repository-readiness` only.
- The current Stage 17 final gate blocks public launch and allows only bounded local/package readiness continuation.
- Local checkout users build with `pnpm build`; generated `dist` is not promised in a clean checkout.
- Local support diagnostics are provided by `support-bundle` and redacted runtime diagnostics by `runtime-env-inspect --redacted`.
- Local tarball proof is limited to controlled `.tgz` install/uninstall and optional explicit two-tarball upgrade/rollback smoke.
- Public docs and examples must keep model, runtime, memory, package, hosted, support, and production claims inside the boundaries in this section.
- Integrated public-environment flow claims must distinguish local/offline proof from live runtime, public package, provider, hosted, builder, and managed-subagent readiness.
- Feature-level `limited/beta` labels are not external beta completion claims; Stage 16 remains `not-run` until real external participants and dated accepted evidence exist.

## Still Forbidden

Do not claim:

- hosted, managed, SaaS, uptime, SLA, production-load, cloud deployment, hosted telemetry, hosted audit, status-page, or managed incident-response readiness;
- public npm publication, public registry install, installer distribution, container distribution, signing, provenance, retained SBOMs, or public package rollback;
- full Codex App Server certification, all models/options support, account/rate-limit guarantees, or broad runtime-provider reliability;
- native App Server memory, broad memory-provider support, durable provider cleanup beyond documented scoped behavior, provider-wide cleanup, true restore, or provider reliability;
- full user interaction readiness, complete managed-subagent orchestration, durable background subagent execution, live subagent cancellation delivery, complete public Builder 2.0 readiness, stable compatibility for experimental CLI commands, or any stable JS/TS API.
- completed external beta, beta-user validation, or public-readiness approval before Stage 16 records real participant evidence and accepted exit review.
- public launch approval before Stage 17 is replaced by a later evidence-backed approval decision.

Public-facing docs should use [Final Public Launch Gate Decision](./final-public-launch-gate-decision.md), [Public Docs, Onboarding, And Claims](./public-docs-onboarding-and-claims.md), [Public Launch Scope](./public-launch-scope.md), and [Baseline Gap And Forbidden Claims](./baseline-gap-and-forbidden-claims.md) as starting locks before changing scope or claiming evidence.
