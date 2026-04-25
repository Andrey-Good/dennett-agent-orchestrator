[English](#english) | [Russian](#russian)

<a id="english"></a>
# Real-World Proof And Release

Status: owner section for Phase 19 real-world proof and release documentation. This section defines how release readiness is proven; it does not assert that readiness has already been achieved.

Documents:

- [Phase 19 Real-World Proof And Release](./phase-19-real-world-proof-and-release.md)
- [Release Scope Lock](./release-scope-lock.md)
- [Live Proof Runbook](./live-proof-runbook.md)
- [Stress And Regression Runbook](./stress-and-regression-runbook.md)
- [Operational Runbook](./operational-runbook.md)
- [Evidence Log](./evidence-log.md)
- [Release Decision Record](./release-decision-record.md)

## Scope

Phase 19 owns the evidence process for moving from internally coherent implementation to an externally credible release decision. It defines live proof requirements, stress and regression expectations, operational readiness checks, evidence recording, and the final release decision format.

The current canonical next release target is locked in [Release Scope Lock](./release-scope-lock.md) as `local-cli-repository-readiness`. That lock defines included and deferred capabilities, role owners, proof paths, rollback and cleanup expectations, and user-visible limitations. The current truthful decision remains `defer` until later evidence supports changing it.

This section does not define new product behavior, add runtime or provider capabilities, change subsystem contracts, or replace owner docs from earlier phases. When evidence exposes a product gap, the result is recorded as `block` or `defer`; the runbook must not rewrite the gap into success.

<a id="russian"></a>
# Доказательство в реальном мире и выпуск

Статус: раздел-владелец документации Phase 19 для доказательств в реальном мире и выпуска. Этот раздел определяет, как доказывается готовность к выпуску; он не утверждает, что готовность уже достигнута.

Документы:

- [Phase 19 Real-World Proof And Release](./phase-19-real-world-proof-and-release.md)
- [Release Scope Lock](./release-scope-lock.md)
- [Live Proof Runbook](./live-proof-runbook.md)
- [Stress And Regression Runbook](./stress-and-regression-runbook.md)
- [Operational Runbook](./operational-runbook.md)
- [Evidence Log](./evidence-log.md)
- [Release Decision Record](./release-decision-record.md)

## Область

Phase 19 владеет процессом доказательств для перехода от внутренне согласованной реализации к внешне убедительному решению о выпуске. Она определяет требования к live proof, ожидания для stress и regression, проверки операционной готовности, запись доказательств и формат финального решения о выпуске.

Текущая каноническая следующая цель выпуска зафиксирована в [Release Scope Lock](./release-scope-lock.md) как `local-cli-repository-readiness`. Эта фиксация определяет включенные и отложенные возможности, role owners, proof paths, rollback и cleanup expectations, а также user-visible limitations. Текущее правдивое решение остается `defer`, пока более поздние доказательства не поддержат его изменение.

Этот раздел не определяет новое продуктовое поведение, не добавляет возможности runtime или provider, не меняет контракты подсистем и не заменяет документы-владельцы предыдущих фаз. Когда доказательство выявляет продуктовый разрыв, результат записывается как `block` или `defer`; runbook не должен переписывать этот разрыв в успех.
