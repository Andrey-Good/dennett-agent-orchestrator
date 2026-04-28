[English](#english) | [Russian](#russian)

<a id="english"></a>
# Public Launch Readiness

Status: canonical owner section for Part 1 public-launch readiness planning. Stage 1 is only a baseline gap and forbidden-claim lock; Stage 2 selects a CLI/package-first launch target and keeps hosted/managed launch deferred; Stage 3 records security/privacy/legal boundaries; Stage 4 records private package and supply-chain foundation; Stage 5 records the limited Runtime/App Server certification subset; Stage 6 records the limited local Mem0-first memory productization subset; Stage 7 records the bounded CLI/package interaction productization subset. No stage expands the current release target or claims public readiness by itself.

Documents:

- [Public Launch Scope](./public-launch-scope.md)
- [Security, Privacy, And Legal Foundation](./security-privacy-legal-foundation.md)
- [Release Engineering And Supply Chain](./release-engineering-and-supply-chain.md)
- [Runtime App Server Certification](./runtime-app-server-certification.md)
- [Memory Productization](./memory-productization.md)
- [User Interaction Productization](./user-interaction-productization.md)
- [Baseline Gap And Forbidden Claims](./baseline-gap-and-forbidden-claims.md)
- [Release Scope Lock](../20-real-world-proof-and-release/release-scope-lock.md)
- [Release Decision Record](../20-real-world-proof-and-release/release-decision-record.md)
- [Phase 12 Capability Gap Lock](../13-capability-gap-lock/phase-12-capability-gap-lock.md)

## Scope

This section governs the public-launch readiness path after the bounded `local-cli-repository-readiness` release. It exists to prevent broader public claims from being inferred from the local release evidence.

The current truthful release state remains:

- bounded `release` for `local-cli-repository-readiness` only;
- no hosted, managed, packaged, installer, container, production SaaS, broad provider, full App Server certification, full user interaction, operator-facing managed-subagent, or public Builder 2.0 readiness claim;
- Stage 5 certifies only the limited local CLI/package Codex App Server subset in [Runtime App Server Certification](./runtime-app-server-certification.md);
- Stage 6 productizes only the limited local Mem0-first memory subset in [Memory Productization](./memory-productization.md);
- Stage 7 productizes only the bounded local CLI/package prompt wait/reply/status/resume subset in [User Interaction Productization](./user-interaction-productization.md);
- no change to source, tests, package metadata, CI, tags, commits, or publication state from Stage 1 or Stage 2.

Part 1 stages 3-10 must use [Public Launch Scope](./public-launch-scope.md) and [Baseline Gap And Forbidden Claims](./baseline-gap-and-forbidden-claims.md) as the starting locks before changing scope or claiming evidence.

<a id="russian"></a>
# Готовность к публичному запуску

Статус: канонический раздел-владелец для планирования Part 1 public-launch readiness. Stage 1 является только фиксацией baseline gaps и forbidden claims; Stage 2 выбирает CLI/package-first launch target и оставляет hosted/managed launch deferred. Ни один stage не расширяет текущую цель выпуска и не заявляет public readiness.

Документы:

- [Public Launch Scope](./public-launch-scope.md)
- [Security, Privacy, And Legal Foundation](./security-privacy-legal-foundation.md)
- [Release Engineering And Supply Chain](./release-engineering-and-supply-chain.md)
- [Runtime App Server Certification](./runtime-app-server-certification.md)
- [Memory Productization](./memory-productization.md)
- [User Interaction Productization](./user-interaction-productization.md)
- [Baseline Gap And Forbidden Claims](./baseline-gap-and-forbidden-claims.md)
- [Release Scope Lock](../20-real-world-proof-and-release/release-scope-lock.md)
- [Release Decision Record](../20-real-world-proof-and-release/release-decision-record.md)
- [Phase 12 Capability Gap Lock](../13-capability-gap-lock/phase-12-capability-gap-lock.md)

## Область

Этот раздел управляет путем готовности к публичному запуску после bounded `local-cli-repository-readiness` release. Он нужен, чтобы более широкие публичные заявления не выводились из локальных release evidence.

Текущее правдивое состояние выпуска остается таким:

- bounded `release` только для `local-cli-repository-readiness`;
- нет claims о hosted, managed, packaged, installer, container, production SaaS, broad provider, full App Server certification, full user interaction, operator-facing managed-subagent или public Builder 2.0 readiness;
- Stage 7 productizes only the bounded local CLI/package prompt wait/reply/status/resume subset in [User Interaction Productization](./user-interaction-productization.md);
- Stage 1 и Stage 2 не меняют source, tests, package metadata, CI, tags, commits или publication state.

Stages 3-10 из Part 1 должны использовать [Public Launch Scope](./public-launch-scope.md) и [Baseline Gap And Forbidden Claims](./baseline-gap-and-forbidden-claims.md) как начальные фиксации перед изменением scope или заявлением evidence.
