[English](#english) | [Русский](#russian)

<a id="english"></a>
# Operational Readiness

Status: normative.
Owns: the current-stage operational-readiness envelope, release-claim boundaries, and residual-risk framing.
Does not own: subsystem semantics, hosted-service operations, or future support promises.
Primary sources: [hardening scope](./hardening-scope.md), [release gates](./release-gates.md), [atomic write policy](../05-state/atomic-write-policy.md), [draft-live-deploy](../07-lifecycle/draft-live-deploy.md), [builder agent](../08-extensions/builder-agent.md), [Phase 19 real-world proof and release](../20-real-world-proof-and-release/README.md).

## 1. Current-Stage Readiness Envelope

If the release gates pass, the repository may claim the following for the current stage:

- the codebase is intended to be a stable local orchestrator project for contributors and local users;
- the accepted current feature surface is guarded by explicit repository commands, CI, and focused tests;
- critical file-backed paths are expected to follow the crash-safe rules from [atomic write policy](../05-state/atomic-write-policy.md);
- local lifecycle behavior such as drafts, live revisions, deploy, and builder draft persistence is expected to remain coherent with the docs.

Phase 19 raises the bar for public release-readiness claims: real-world proof, stress/regression evidence, operational evidence, and a completed [release decision record](../20-real-world-proof-and-release/release-decision-record.md) must be present before the project claims release readiness beyond the local/offline evidence envelope.

## 2. What Must Not Be Claimed

Even after Phase 11, the repository must not claim:

- hosted multi-tenant service readiness;
- formal proof of crash safety across every platform and failure mode;
- full exposure of the entire Codex App Server feature surface;
- compatibility guarantees for agent sources or runtimes that do not yet exist in the repository;
- zero known limitations in interaction, lifecycle, or extension behavior.
- Phase 19 release readiness while live proof, stress/regression evidence, operational evidence, or the release decision record is still missing or blocked.

## 3. Release-Facing Operational Expectations

For the current stage, operational readiness means:

- a contributor can install dependencies on the canonical path and run the required commands successfully;
- a maintainer can rely on CI rather than private local state for basic release confidence;
- documented recovery-sensitive behavior is backed by explicit checks or manual review;
- the repository can be evolved without silently changing accepted contracts.

This is a repository-readiness standard, not a cloud-operations standard.

## 4. Residual Risk Discipline

Residual risk is allowed to exist, but it must stay visible.

Required rule:

- if a known limitation materially affects release confidence, the limitation must be tracked as residual risk rather than hidden behind a green command summary.

Examples of residual-risk categories include:

- platform-specific filesystem behavior not exhaustively tested;
- adapter-surface areas intentionally gated or rejected rather than fully implemented;
- manual review dependencies that are not yet fully automated.
- Phase 19 blockers such as unavailable live runtime/provider credentials, missing Mem0 or runtime proof, unresolved lint/runtime blockers, absent stress evidence, or an incomplete release decision record.

## 5. Relationship To Recovery And Durability

Operational readiness depends on, but does not replace, the state-layer guarantees.

- [atomic write policy](../05-state/atomic-write-policy.md) defines what the file layer must do.
- [local storage model](../05-state/local-storage-model.md) defines how local truth and derivative truth relate.
- [chat and resume](../05-state/chat-and-resume.md) defines when interrupted work may continue and when it may not.

This document only states the release-facing meaning of those guarantees: they must be respected, tested, and not overstated.

## 6. Relationship To Lifecycle And Builder

Operational readiness includes the current lifecycle and builder surfaces, but only within their accepted semantics.

- [draft-live-deploy](../07-lifecycle/draft-live-deploy.md) still owns publication meaning.
- [builder agent](../08-extensions/builder-agent.md) still owns builder behavior.

This document does not upgrade those features into a promise of automatic publishing, hosted governance, or autonomous release management.

## 7. Practical Communication Rule

When describing the project publicly, prefer language like:

- "stable current repository release"
- "release-ready for the accepted local workflow"
- "hardened against the currently documented regression and recovery risks"

Avoid language like:

- "fully production ready"
- "supports the full vendor runtime surface"
- "guaranteed crash-proof on every platform"

<a id="russian"></a>
Маршрутная заметка Phase 19: release readiness за пределами local/offline evidence envelope требует real-world proof, stress/regression evidence, operational evidence и завершенную [release decision record](../20-real-world-proof-and-release/release-decision-record.md). Отсутствующее live runtime/provider proof, нерешенные blockers или незавершенная decision record остаются видимым residual risk.

# Operational Readiness

Статус: нормативный.
Владеет: текущей рамкой operational readiness, границами релизных заявлений и способом оформления residual risks.
Не владеет: семантикой подсистем, hosted-service operations и будущими обещаниями поддержки.
Основные источники: [hardening scope](./hardening-scope.md), [release gates](./release-gates.md), [atomic write policy](../05-state/atomic-write-policy.md), [draft-live-deploy](../07-lifecycle/draft-live-deploy.md), [builder agent](../08-extensions/builder-agent.md), [Phase 19 real-world proof and release](../20-real-world-proof-and-release/README.md).

## 1. Текущая рамка готовности

Если release gates проходят, репозиторий может заявлять следующее для текущего этапа:

- кодовая база предназначена быть стабильным локальным проектом-оркестратором для контрибьюторов и локальных пользователей;
- текущая принятая поверхность возможностей защищена явными командами репозитория, CI и focused tests;
- критичные file-backed пути обязаны следовать crash-safe правилам из [atomic write policy](../05-state/atomic-write-policy.md);
- локальное lifecycle-поведение, такое как drafts, live revisions, deploy и builder draft persistence, обязано оставаться согласованным с документацией.

Phase 19 повышает планку для публичных заявлений о release readiness: real-world proof, stress/regression evidence, operational evidence и завершенная [release decision record](../20-real-world-proof-and-release/release-decision-record.md) должны существовать до того, как проект заявит release readiness за пределами local/offline evidence envelope.

## 2. Чего нельзя заявлять

Даже после Phase 11 репозиторий не должен заявлять:

- readiness hosted multi-tenant service;
- формальное доказательство crash safety на каждой платформе и при каждом failure mode;
- полное отражение всей поверхности возможностей Codex App Server;
- гарантии совместимости с agent sources или runtime, которых в репозитории еще нет;
- отсутствие любых известных ограничений в interaction, lifecycle или extension behavior.
- release readiness для Phase 19, пока live proof, stress/regression evidence, operational evidence или release decision record отсутствуют или заблокированы.

## 3. Релизные операционные ожидания

Для текущего этапа operational readiness означает:

- контрибьютор может установить зависимости по каноническому пути и успешно выполнить обязательные команды;
- мейнтейнер может опираться на CI, а не на приватное локальное состояние, для базовой уверенности в релизе;
- задокументированное recovery-sensitive поведение подтверждено явными проверками или manual review;
- репозиторий можно развивать без молчаливого изменения уже принятых контрактов.

Это стандарт готовности репозитория, а не стандарт cloud-operations.

## 4. Дисциплина residual risks

Residual risk может существовать, но он должен оставаться видимым.

Обязательное правило:

- если известное ограничение материально влияет на уверенность в релизе, его нужно учитывать как residual risk, а не прятать за зеленой сводкой команд.

Примеры категорий residual risk:

- platform-specific поведение файловой системы, которое не протестировано исчерпывающе;
- части adapter surface, которые намеренно gated или rejected, а не реализованы полностью;
- зависимости от manual review, которые еще не автоматизированы до конца.
- blockers Phase 19, такие как недоступные live runtime/provider credentials, отсутствующее Mem0 или runtime proof, нерешенные lint/runtime blockers, отсутствующее stress evidence или незавершенная release decision record.

## 5. Связь с recovery и durability

Operational readiness зависит от гарантий state-layer, но не заменяет их.

- [atomic write policy](../05-state/atomic-write-policy.md) определяет, что обязан делать файловый слой.
- [local storage model](../05-state/local-storage-model.md) определяет, как связаны local truth и derivative truth.
- [chat and resume](../05-state/chat-and-resume.md) определяет, когда interrupted work может продолжиться, а когда нет.

Этот документ фиксирует только релизный смысл этих гарантий: их нужно соблюдать, тестировать и не преувеличивать.

## 6. Связь с lifecycle и builder

Operational readiness включает текущие lifecycle и builder surfaces, но только в пределах их принятой семантики.

- [draft-live-deploy](../07-lifecycle/draft-live-deploy.md) по-прежнему владеет смыслом публикации.
- [builder agent](../08-extensions/builder-agent.md) по-прежнему владеет поведением builder-а.

Этот документ не повышает эти возможности до обещания automatic publishing, hosted governance или autonomous release management.

## 7. Практическое правило коммуникации

Описывая проект публично, предпочитайте формулировки вроде:

- "stable current repository release"
- "release-ready for the accepted local workflow"
- "hardened against the currently documented regression and recovery risks"

Избегайте формулировок вроде:

- "fully production ready"
- "supports the full vendor runtime surface"
- "guaranteed crash-proof on every platform"
