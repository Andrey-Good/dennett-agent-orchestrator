[English](#english) | [Русский](#russian)

<a id="english"></a>
# Hardening Scope

Status: normative.  
Owns: the scope boundary of Phase 11 hardening work and the rule for turning accepted product behavior into release-facing quality requirements.  
Does not own: new product features, runtime semantics, or lower-level behavior already owned elsewhere.  
Primary sources: [documentation root](../README.md), [technology stack](../01-foundations/technology-stack.md), [runtime integration model](../02-architecture/runtime-integration-model.md), [state section](../05-state/README.md).

## 1. Purpose

Phase 11 exists to make the accepted product slices harder to break, easier to verify, and safer to release. It is not the place to expand the product scope or invent new orchestration behavior.

In practice, this phase is allowed to tighten:

- repository-wide validation discipline;
- CI expectations;
- crash and recovery verification;
- backward-compatibility checks for the current contract surface;
- release criteria and operational-readiness framing;
- packaging and workflow hygiene needed to support the current repository shape.

## 2. What Counts As Hardening

The following work belongs to Phase 11 when it strengthens already accepted behavior:

- making canonical commands reliable, repeatable, and enforced in CI;
- adding tests for failure, interruption, recovery, and regression-sensitive paths already described by the docs;
- removing tooling debt that blocks the locked workflow, such as repository-wide lint failure;
- documenting exact release gates and required sign-offs;
- documenting what the current project stage can and cannot claim operationally.

Hardening is allowed to expose implementation gaps in previously accepted phases. Fixing such a gap belongs to Phase 11 only when the fix preserves the accepted semantics rather than changing them.

## 3. What Is Out Of Scope

The following do not become Phase 11 work merely because they are useful:

- introducing new product features that were not already accepted;
- broadening the portable contract beyond the accepted canon;
- replacing the locked stack from [technology stack](../01-foundations/technology-stack.md);
- moving vendor logic outside the boundaries already defined in [runtime integration model](../02-architecture/runtime-integration-model.md);
- changing lifecycle meaning owned by [draft-live-deploy](../07-lifecycle/draft-live-deploy.md);
- redefining resume, memory, or subagent semantics already owned elsewhere.

If a change would alter accepted product behavior rather than harden it, the change belongs to an earlier owner document or a new ADR, not here.

## 4. Current-Stage Hardening Targets

For the current repository stage, hardening must cover at least:

- locked workflow commands based on `pnpm`, TypeScript, Vitest, and Biome;
- buildability of the TypeScript CLI and runtime-facing code;
- correctness of local persistence, atomic-write ordering, and explicit resume boundaries;
- correctness of current lifecycle surfaces such as registry, drafts, live revisions, and deploy;
- correctness of the current extension surface, including builder and runtime-source gating;
- Codex App Server adapter integration as implemented today, without claiming full vendor-surface support.

## 5. Relationship To Existing Owner Documents

Phase 11 does not replace the earlier sections. It consumes them.

- [Execution](../04-execution/README.md) still defines what correct behavior is.
- [State](../05-state/README.md) still defines what must survive crashes and resume.
- [Lifecycle](../07-lifecycle/README.md) still defines draft/live/deploy meaning.
- [Extensions](../08-extensions/README.md) still define builder, runtime-source, and other non-core rules.

This document only decides which of those already-owned promises must be defended by tests, CI, release gates, and explicit review.

## 6. Non-Goals For This Phase

Phase 11 does not make the project a hosted service, a distributed system, or a formally verified runtime.

Specifically, this phase does not promise:

- multi-machine coordination;
- zero-downtime upgrades across distributed workers;
- network-partition tolerance;
- universal compatibility with every future agent source;
- automatic proof that every App Server capability is exposed in the portable contract.

## 7. Acceptance Standard For Phase 11 Scope Decisions

A proposed hardening item belongs in this phase only if all of the following are true:

1. It protects behavior already accepted by an earlier owner document.
2. It does not require inventing a new product feature to justify itself.
3. It improves release confidence, regression resistance, or operator clarity.
4. It stays inside the locked stack and architectural boundaries.

If any of those are false, the item belongs elsewhere.

<a id="russian"></a>
# Область Hardening

Статус: нормативный.  
Владеет: границей scope для Phase 11 hardening-работ и правилом, по которому уже принятые продуктовые возможности превращаются в релизные требования к качеству.  
Не владеет: новыми продуктовыми возможностями, runtime-семантикой и lower-level поведением, которым уже владеют другие документы.  
Основные источники: [корень документации](../README.md), [technology stack](../01-foundations/technology-stack.md), [runtime integration model](../02-architecture/runtime-integration-model.md), [раздел state](../05-state/README.md).

## 1. Назначение

Phase 11 существует для того, чтобы уже принятые продуктовые срезы было сложнее сломать, проще проверить и безопаснее выпускать. Это не то место, где нужно расширять scope продукта или придумывать новую orchestration-логику.

На практике эта фаза вправе ужесточать:

- repository-wide дисциплину проверок;
- ожидания к CI;
- верификацию crash и recovery;
- backward-compatibility checks для текущей поверхности контрактов;
- релизные критерии и рамку operational readiness;
- packaging и workflow hygiene, необходимые для поддержки текущей формы репозитория.

## 2. Что считается hardening

Следующие работы относятся к Phase 11, если они усиливают уже принятые возможности:

- сделать канонические команды надежными, повторяемыми и enforce-ить их через CI;
- добавить тесты на failure, interruption, recovery и regression-sensitive пути, уже описанные документацией;
- убрать toolchain debt, который блокирует locked workflow, например repository-wide failure линтера;
- задокументировать точные release gates и required sign-offs;
- задокументировать, что текущий этап проекта может и не может обещать с операционной точки зрения.

Hardening вправе обнаруживать implementation gaps в ранее принятых фазах. Исправление такого gap относится к Phase 11 только тогда, когда оно сохраняет уже принятую семантику, а не меняет ее.

## 3. Что находится вне scope

Следующие вещи не становятся Phase 11 только потому, что они полезны:

- введение новых продуктовых возможностей, которые еще не были приняты;
- расширение portable contract beyond accepted canon;
- замена locked stack из [technology stack](../01-foundations/technology-stack.md);
- вынос vendor-логики за границы, уже определенные в [runtime integration model](../02-architecture/runtime-integration-model.md);
- изменение смысла lifecycle, которым владеет [draft-live-deploy](../07-lifecycle/draft-live-deploy.md);
- переопределение resume, memory или subagent-семантики, уже принадлежащей другим owner-docs.

Если изменение меняет уже принятую продуктовую семантику вместо ее укрепления, оно должно жить в другом owner-doc или в новом ADR, а не здесь.

## 4. Текущие hardening-цели этапа

Для текущего этапа репозитория hardening обязан охватывать как минимум:

- locked workflow-команды на базе `pnpm`, TypeScript, Vitest и Biome;
- собираемость TypeScript CLI и runtime-facing кода;
- корректность local persistence, порядка atomic-write и explicit resume boundaries;
- корректность текущих lifecycle-поверхностей, таких как registry, drafts, live revisions и deploy;
- корректность текущей extension-поверхности, включая builder и runtime-source gating;
- интеграцию Codex App Server adapter в том виде, как она реализована сейчас, без заявлений о полном покрытии vendor-surface.

## 5. Связь с существующими owner-docs

Phase 11 не заменяет предыдущие разделы. Он опирается на них.

- [Execution](../04-execution/README.md) по-прежнему определяет, что считается корректным поведением.
- [State](../05-state/README.md) по-прежнему определяет, что обязано переживать crash и resume.
- [Lifecycle](../07-lifecycle/README.md) по-прежнему определяет смысл draft/live/deploy.
- [Extensions](../08-extensions/README.md) по-прежнему определяют builder, runtime-source и другие non-core правила.

Этот документ решает только то, какие из этих уже принадлежащих другим документов обещаний должны быть защищены тестами, CI, release gates и явным review.

## 6. Non-goals этой фазы

Phase 11 не превращает проект в hosted service, distributed system или formally verified runtime.

В частности, эта фаза не обещает:

- multi-machine coordination;
- zero-downtime upgrades across distributed workers;
- устойчивость к network partition;
- универсальную совместимость с любым будущим agent source;
- автоматическое доказательство того, что каждая возможность App Server отражена в portable contract.

## 7. Критерий приемки для scope-решений Phase 11

Предлагаемый hardening-item относится к этой фазе только если одновременно верно следующее:

1. Он защищает поведение, уже принятое более ранним owner-doc.
2. Для его обоснования не требуется придумывать новую продуктовую возможность.
3. Он повышает уверенность в релизе, устойчивость к регрессиям или ясность для оператора.
4. Он остается внутри locked stack и архитектурных границ.

Если хоть одно из этих условий нарушено, item должен жить где-то еще.
