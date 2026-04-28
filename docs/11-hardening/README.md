[English](#english) | [Русский](#russian)

<a id="english"></a>
# Hardening And Release Readiness

Status: section index only. Normative rules live in the leaf documents in this section.

Related documents:

- [Documentation root](../README.md)
- [Foundations](../01-foundations/README.md)
- [Architecture](../02-architecture/README.md)
- [Execution](../04-execution/README.md)
- [State](../05-state/README.md)
- [Lifecycle](../07-lifecycle/README.md)
- [Extensions](../08-extensions/README.md)

This section defines what "Phase 11 complete" means for the repository. It does not add new product features. Instead, it owns the release-facing quality layer around the already accepted core, interaction, lifecycle, extensions, and builder slices.

Normative documents in this section:

- [Hardening Scope](./hardening-scope.md): what Phase 11 is allowed to tighten, and what remains out of scope.
- [Release Gates](./release-gates.md): the required checks and sign-offs before calling the current repository state release-ready.
- [Validation Matrix](./validation-matrix.md): which risks must be checked, how they are checked, and whether the check is automated or manual at the current stage.
- [Operational Readiness](./operational-readiness.md): the honest current-stage readiness envelope, residual risks, and what this project still does not promise.

Related launch-readiness operations owner:

- [Observability, Support, And Operations](../21-public-launch-readiness/observability-support-operations.md): Stage 13 local support bundle, redacted runtime diagnostics, support/security routing, telemetry boundary, and local incident runbooks.

Section boundary:

- Product meaning and stack lock remain owned by [Foundations](../01-foundations/README.md).
- Runtime boundaries remain owned by [Architecture](../02-architecture/README.md).
- File and runtime contracts remain owned by [Contracts](../03-contracts/README.md).
- Execution, state, interaction, lifecycle, and extensions remain owned by their existing sections.
- This section may define release gates over those areas, but it must not silently redefine their semantics.

How to use this section:

- Read [Hardening Scope](./hardening-scope.md) before turning a quality idea into a Phase 11 requirement.
- Read [Release Gates](./release-gates.md) before declaring a commit, tag, or branch ready for a repository release.
- Read [Validation Matrix](./validation-matrix.md) when deciding which checks must exist in CI and which still require explicit human review.
- Read [Operational Readiness](./operational-readiness.md) before making claims about stability, safety, or maturity in user-facing material.

<a id="russian"></a>
# Hardening и готовность к релизу

Статус: только индекс раздела. Нормативные правила живут в профильных документах этого раздела.

Связанные документы:

- [Корень документации](../README.md)
- [Foundations](../01-foundations/README.md)
- [Architecture](../02-architecture/README.md)
- [Execution](../04-execution/README.md)
- [State](../05-state/README.md)
- [Lifecycle](../07-lifecycle/README.md)
- [Extensions](../08-extensions/README.md)

Этот раздел определяет, что означает "Phase 11 complete" для репозитория. Он не добавляет новые продуктовые возможности. Вместо этого он владеет качественным и релизным слоем вокруг уже принятых срезов core, interaction, lifecycle, extensions и builder.

Нормативные документы раздела:

- [Hardening Scope](./hardening-scope.md): что Phase 11 вправе ужесточать и что остается вне scope.
- [Release Gates](./release-gates.md): обязательные проверки и sign-off перед тем, как считать текущее состояние репозитория готовым к релизу.
- [Validation Matrix](./validation-matrix.md): какие риски нужно проверять, как именно они проверяются и автоматизирована ли проверка на текущем этапе.
- [Operational Readiness](./operational-readiness.md): честная рамка текущей готовности, остаточных рисков и того, что проект пока не обещает.

Граница раздела:

- Смысл продукта и lock по стеку по-прежнему принадлежат [Foundations](../01-foundations/README.md).
- Runtime-границы по-прежнему принадлежат [Architecture](../02-architecture/README.md).
- Файловые и runtime-контракты по-прежнему принадлежат [Contracts](../03-contracts/README.md).
- Execution, state, interaction, lifecycle и extensions остаются во владении своих существующих разделов.
- Этот раздел может задавать релизные критерии поверх этих областей, но не должен молча переопределять их семантику.

Как пользоваться этим разделом:

- Читайте [Hardening Scope](./hardening-scope.md) до того, как превращать идею про качество в требование Phase 11.
- Читайте [Release Gates](./release-gates.md) перед тем, как объявлять commit, tag или branch готовыми к релизу репозитория.
- Читайте [Validation Matrix](./validation-matrix.md), когда решаете, какие проверки обязаны существовать в CI, а какие пока требуют явного human review.
- Читайте [Operational Readiness](./operational-readiness.md) до того, как делать заявления о стабильности, безопасности или зрелости в user-facing материалах.
