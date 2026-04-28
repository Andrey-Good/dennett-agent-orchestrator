[English](#english) | [Russian](#russian)

<a id="english"></a>
# Public Launch Readiness

Status: canonical owner section for Part 1 public-launch readiness planning. No stage expands the current release target or claims public readiness by itself.

Documents:

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

## Scope

This section governs the public-launch readiness path after the bounded `local-cli-repository-readiness` release. It exists to prevent broader public claims from being inferred from the local release evidence.

The current truthful release state remains:

- bounded `release` for `local-cli-repository-readiness` only;
- no hosted, managed, packaged, installer, container, production SaaS, broad provider, full App Server certification, full user interaction, complete managed-subagent orchestration, or public Builder 2.0 readiness claim;
- Stage 5 certifies only the limited local CLI/package Codex App Server subset in [Runtime App Server Certification](./runtime-app-server-certification.md);
- Stage 6 productizes only the limited local Mem0-first memory subset in [Memory Productization](./memory-productization.md);
- Stage 7 productizes only the bounded local CLI/package prompt wait/reply/status/resume subset in [User Interaction Productization](./user-interaction-productization.md);
- Stage 8 productizes only the bounded local CLI/package managed-subagent operator subset in [Managed Subagent Productization](./managed-subagent-productization.md);
- Stage 9 productizes only the bounded audited draft-first Builder 2.0 authoring subset in [Builder 2.0 Productization](./builder-2-0-productization.md);
- Stage 10 freezes only the explicitly labeled stable CLI commands, stable/safety-protocol cleanup flow, package schema exports, and no-JS-API boundary in [Stable CLI/API Contract Freeze](./stable-cli-api-contract-freeze.md);
- Stage 11 proves only controlled local `.tgz` install/uninstall, explicit two-tarball upgrade/rollback smoke, local SPDX SBOM validation, and CI package-proof job configuration in [Stage 11 Distribution Proof](./distribution-proof.md);
- Stage 11 does not approve production code changes, npm publication, public package metadata finalization, tags, commits, pushes, releases, or hosted deployment;
- Stage 12 keeps hosted/managed deployment explicitly out of current public-launch scope in [Hosted And Managed Deployment Scope](./hosted-managed-deployment-scope.md).
- Stage 13 provides only local diagnostics and support routing in [Observability, Support, And Operations](./observability-support-operations.md); it does not create hosted support, SLA, automatic telemetry, status-page, public npm, signing, provenance, or managed-operations claims.

Part 1 stages 3-13 must use [Public Launch Scope](./public-launch-scope.md) and [Baseline Gap And Forbidden Claims](./baseline-gap-and-forbidden-claims.md) as starting locks before changing scope or claiming evidence.

<a id="russian"></a>
# Готовность к публичному запуску

Статус: канонический раздел-владелец для планирования Part 1 public-launch readiness. Ни один stage сам по себе не расширяет текущую цель выпуска и не заявляет public readiness.

Документы:

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
- [Baseline Gap And Forbidden Claims](./baseline-gap-and-forbidden-claims.md)
- [Release Scope Lock](../20-real-world-proof-and-release/release-scope-lock.md)
- [Release Decision Record](../20-real-world-proof-and-release/release-decision-record.md)
- [Phase 12 Capability Gap Lock](../13-capability-gap-lock/phase-12-capability-gap-lock.md)

## Список stages

- Stage 1 фиксирует baseline gaps и forbidden claims.
- Stage 2 выбирает CLI/package-first public-launch target и оставляет hosted/managed launch deferred.
- Stage 3 фиксирует security, privacy, legal и trust boundaries для CLI/package-first launch.
- Stage 4 фиксирует private package и supply-chain foundation.
- Stage 5 фиксирует ограниченный Runtime/App Server certification subset.
- Stage 6 фиксирует ограниченный локальный Mem0-first memory productization subset.
- Stage 7 фиксирует bounded CLI/package interaction productization subset.
- Stage 8 фиксирует bounded local CLI managed-subagent operator subset.
- Stage 9 фиксирует bounded audited draft-first Builder 2.0 authoring subset.
- Stage 10 фиксирует bounded stable CLI/API contract freeze.
- Stage 11 фиксирует local tarball distribution proof, controlled install/uninstall proof, upgrade/rollback harness boundaries, private package identity и local SBOM validation.
- Stage 12 фиксирует канонический hosted/managed deployment deferral lock и gap matrix.

## Область

Этот раздел управляет путем public-launch readiness после bounded `local-cli-repository-readiness` release. Он нужен, чтобы более широкие public claims не выводились из локальных release evidence.

Текущее правдивое состояние выпуска:

- bounded `release` только для `local-cli-repository-readiness`;
- нет claims о hosted, managed, packaged, installer, container, production SaaS, broad provider, full App Server certification, full user interaction, complete managed-subagent orchestration или public Builder 2.0 readiness;
- Stage 5 сертифицирует только limited local CLI/package Codex App Server subset в [Runtime App Server Certification](./runtime-app-server-certification.md);
- Stage 6 productizes only the limited local Mem0-first memory subset in [Memory Productization](./memory-productization.md);
- Stage 7 productizes only the bounded local CLI/package prompt wait/reply/status/resume subset in [User Interaction Productization](./user-interaction-productization.md);
- Stage 8 productizes only the bounded local CLI/package managed-subagent operator subset in [Managed Subagent Productization](./managed-subagent-productization.md);
- Stage 9 productizes only the bounded audited draft-first Builder 2.0 authoring subset in [Builder 2.0 Productization](./builder-2-0-productization.md);
- Stage 10 freezes only the explicitly labeled stable CLI commands, stable/safety-protocol cleanup flow, package schema exports, and no-JS-API boundary in [Stable CLI/API Contract Freeze](./stable-cli-api-contract-freeze.md);
- Stage 11 доказывает только controlled local `.tgz` install/uninstall, explicit two-tarball upgrade/rollback smoke, local SPDX SBOM validation и CI package-proof job configuration in [Stage 11 Distribution Proof](./distribution-proof.md);
- Stage 11 не одобряет production code changes, npm publication, public package metadata finalization, tags, commits, pushes, releases или hosted deployment;
- Stage 12 явно оставляет hosted/managed deployment вне текущего public-launch scope в [Hosted And Managed Deployment Scope](./hosted-managed-deployment-scope.md).

Stages 3-12 из Part 1 должны использовать [Public Launch Scope](./public-launch-scope.md) и [Baseline Gap And Forbidden Claims](./baseline-gap-and-forbidden-claims.md) как начальные locks перед изменением scope или заявлением evidence.
