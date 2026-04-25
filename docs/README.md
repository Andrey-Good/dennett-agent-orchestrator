[English](#english) | [Russian](#russian)

## English

# Documentation Map

Status: documentation root and ownership model.
Owns: documentation reading order, section responsibilities, and escalation rules for new normative decisions.
Does not own: product behavior that belongs to a narrower specification.
Primary sources: [repository root](../README.md), [canonical specification](../agent_orchestrator_final_spec_v2.md), [ADR index](./09-adrs/README.md).

This tree turns the canonical specification into smaller implementation-facing documents. The goal is not to duplicate the canon in many places, but to assign one owner document to each important rule so code changes know where to look first.

## Source Hierarchy

Use the documentation tree in this order:

1. The [canonical specification](../agent_orchestrator_final_spec_v2.md) defines the high-level meaning of the project and remains the anti-contradiction anchor.
2. A focused document inside `docs/` may refine one specific topic if it explicitly owns that topic and stays consistent with the canonical spec.
3. README files define navigation, section scope, and ownership boundaries. They should summarize, route, and constrain, but they should not silently introduce detailed contract rules that belong in leaf documents.
4. If a change is significant, contested, or has long-lived tradeoffs, capture the decision in [`09-adrs`](./09-adrs/README.md).
5. Non-normative reference targets live in [`12-reference-targets`](./12-reference-targets/README.md). They are implementation references only and do not override canonical Dennett ownership.
6. Post-11 capability freezing and evidence-based status tracking live in [`13-capability-gap-lock`](./13-capability-gap-lock/README.md). That section does not redefine subsystem behavior; it freezes what is and is not actually done.
7. The first executable native-memory slice after that freeze lives in [`14-native-memory-integration`](./14-native-memory-integration/README.md). That section owns what Phase 13 actually implemented without redefining the portable memory contract itself.
8. The first executable native-runtime-surface slice after that memory work lives in [`15-native-runtime-surface`](./15-native-runtime-surface/README.md). That section owns what Phase 14 actually implemented without turning local runtime metadata into portable agent truth.
9. The Builder 2.0 authoring upgrade lives in [`18-builder-2-0`](./18-builder-2-0/README.md). That section owns Phase 17 builder authoring boundaries without claiming Phase 18 integrated product flows.
10. Integrated product-flow ownership lives in [`19-integrated-product-flows`](./19-integrated-product-flows/README.md). That section owns Phase 18 cross-subsystem flow definitions and acceptance boundaries without claiming Phase 19 external proof or release readiness.
11. Real-world proof and release decision ownership lives in [`20-real-world-proof-and-release`](./20-real-world-proof-and-release/README.md). That section owns Phase 19 live-proof evidence, stress/regression proof, operational runbooks, evidence logs, and the release decision record required before release-readiness claims.

If two documents appear to disagree, treat the more canonical and more specific owner as authoritative only if it does not contradict the top-level canon. Otherwise the mismatch is a documentation defect that must be corrected, not a license to pick whichever text is convenient.

## Reading Order For Implementation

| Step | Read this | Why it comes first |
| --- | --- | --- |
| 1 | [`01-foundations`](./01-foundations/README.md) | Locks meaning, boundaries, terms, truth sources, defaults, and stack. |
| 2 | [`02-architecture`](./02-architecture/README.md) | Defines module boundaries and dependency rules. |
| 3 | [`03-contracts`](./03-contracts/README.md) | Defines machine-checkable schemas and formal invariants. |
| 4 | [`04-execution`](./04-execution/README.md) | Governs graph execution behavior. |
| 5 | [`05-state`](./05-state/README.md) | Governs chats, resume, and persisted operational state. |
| 6 | [`06-interaction`](./06-interaction/README.md) | Governs comments, built-in user chat MCP, and user-facing interaction. |
| 7 | [`07-lifecycle`](./07-lifecycle/README.md) | Governs registry, drafts, live revisions, and deploy semantics. |
| 8 | [`08-extensions`](./08-extensions/README.md) | Governs memory, runtime sources, limits, and other non-core axes. |
| 9 | [`09-adrs`](./09-adrs/README.md) | Records why contested architectural choices were made. |
| 10 | [`10-examples`](./10-examples/README.md) | Illustrates valid usage without becoming normative. |
| 11 | [`11-hardening`](./11-hardening/README.md) | Governs release gates, hardening scope, validation matrix, and current-stage operational readiness. |
| 12 | [`12-reference-targets`](./12-reference-targets/README.md) | Reference material only; useful for deliberate reproduction work, but not canonical ownership. |
| 13 | [`13-capability-gap-lock`](./13-capability-gap-lock/README.md) | Governs the truthful post-11 capability freeze, Mem0-first readiness framing, and the handoff to later implementation stages. |
| 14 | [`14-native-memory-integration`](./14-native-memory-integration/README.md) | Governs the implemented Mem0-first native-memory slice, local provider registry, direct provider-backed memory operations, and the boundary to future runtime-attached memory support. |
| 15 | [`15-native-runtime-surface`](./15-native-runtime-surface/README.md) | Governs the implemented Phase 14 runtime surface: model discovery, runtime-environment introspection, and the first executable App Server-native runtime-option controls. |
| 17 | [`17-managed-subagent-orchestration/phase-16-managed-subagent-orchestration.md`](./17-managed-subagent-orchestration/phase-16-managed-subagent-orchestration.md) | Governs the current Phase 16 managed-subagent contract-completion slice and its remaining proof boundary. |
| 18 | [`18-builder-2-0`](./18-builder-2-0/README.md) | Governs the Phase 17 Builder 2.0 authoring boundary for public-contract-only drafts that may reference memory, runtime, interaction, and managed-subagent surfaces. |
| 19 | [`19-integrated-product-flows`](./19-integrated-product-flows/README.md) | Governs Phase 18 integrated product-flow definitions across lifecycle, builder, runtime, interaction, memory, and subagents without promoting those flows to Phase 19 external proof or release readiness. |
| 20 | [`20-real-world-proof-and-release`](./20-real-world-proof-and-release/README.md) | Governs Phase 19 real-world proof and release readiness: live runtime/provider evidence, stress and regression proof, operational runbooks, evidence logs, and the release decision record. |

## Section Responsibilities

- [`01-foundations`](./01-foundations/README.md) owns product identity, scope, boundaries, terminology, truth sources, defaults, and stack lock.
- [`02-architecture`](./02-architecture/README.md) owns layers, module boundaries, dependency constraints, and the staged boundary for App Server-native capability families.
- [`03-contracts`](./03-contracts/README.md) owns formal schemas, type-level contracts, validation constraints, and the stable normalized boundary that is narrower than the full App Server surface.
- [`04-execution`](./04-execution/README.md) owns run behavior, graph traversal, node outcomes, and execution semantics.
- [`05-state`](./05-state/README.md) owns persisted operational state such as chats and resume.
- [`06-interaction`](./06-interaction/README.md) owns user comments, live interaction, the built-in communication MCP, and the user-facing staging boundary for richer runtime notifications.
- [`07-lifecycle`](./07-lifecycle/README.md) owns registry behavior, drafts, live state, deploy, and version axes tied to working copies.
- [`08-extensions`](./08-extensions/README.md) owns features intentionally kept outside the minimal stable core, including runtime-source-local capability metadata that must not become portable file truth.
- [`09-adrs`](./09-adrs/README.md) owns the history and rationale of contested decisions, not the detailed operational contract itself.
- [`10-examples`](./10-examples/README.md) owns worked examples and anti-pattern illustrations, never the underlying rule.
- [`11-hardening`](./11-hardening/README.md) owns release-readiness rules for the current product stage, including hardening scope, release gates, validation expectations, and operational-readiness framing without redefining lower-level behavior.
- [`12-reference-targets`](./12-reference-targets/README.md) owns navigation for non-normative reference material that can inform future implementation work without becoming canonical behavior ownership.
- [`13-capability-gap-lock`](./13-capability-gap-lock/README.md) owns the truthful capability matrix after stages 1-11, the evidence model for claiming later features are done, and the Mem0-first readiness freeze without redefining subsystem behavior.
- [`14-native-memory-integration`](./14-native-memory-integration/README.md) owns the implemented Phase 13 Mem0-first slice, including the local provider registry and direct memory-operation boundary, without redefining the portable memory-binding contract.
- [`15-native-runtime-surface`](./15-native-runtime-surface/README.md) owns the implemented Phase 14 runtime surface, including normalized model discovery, runtime-environment introspection, and the first executable runtime-option overrides, without redefining portable agent-file truth.
- [`18-builder-2-0`](./18-builder-2-0/README.md) owns the Phase 17 Builder 2.0 authoring boundary and routes detailed behavior back to the public contracts that own memory, runtime, interaction, lifecycle, and managed subagents.
- [`19-integrated-product-flows`](./19-integrated-product-flows/README.md) owns Phase 18 integrated product-flow acceptance definitions and subsystem handoff rules without owning Phase 19 external proof, release runbooks, or release readiness.
- [`20-real-world-proof-and-release`](./20-real-world-proof-and-release/README.md) owns Phase 19 real-world proof, stress/regression proof, operational runbooks, evidence logging, and the release decision record; no release-readiness claim is valid without that evidence and decision.

The subagent system is intentionally split across several owner docs: [`02-architecture/subagent-orchestration-model.md`](./02-architecture/subagent-orchestration-model.md) owns the overall model, [`03-contracts/subagent-mcp-contract.md`](./03-contracts/subagent-mcp-contract.md) owns the managed child-run MCP surface, [`04-execution/subagent-task-lifecycle.md`](./04-execution/subagent-task-lifecycle.md) owns delegated-task sequencing, [`05-state/subagent-context-and-memory.md`](./05-state/subagent-context-and-memory.md) owns lineage and persisted child context, and [`08-extensions/builder-agent.md`](./08-extensions/builder-agent.md) consumes that model without redefining it.

## Authoring Rules

- One important rule should have one owner document.
- When a README starts growing detailed behavioral rules, split that topic into a dedicated document and link to it.
- Examples, fixtures, and tests may demonstrate behavior, but they should point back to the owner document.
- Code should not become the first place where undocumented business logic appears. If behavior had to be invented, document it in the correct section or an ADR.

## Russian

Маршрутная заметка Phase 19: [`20-real-world-proof-and-release`](./20-real-world-proof-and-release/README.md) владеет реальными доказательствами, stress/regression proof, operational runbooks, evidence logs и release decision record. Никакое заявление о release readiness не валидно без этих доказательств и завершенной decision record.

# Карта документации

Статус: корень документации и модель владения.
Владеет: порядком чтения документации, ответственностью разделов и правилами эскалации для новых нормативных решений.
Не владеет: поведением продукта, которое принадлежит более узкой спецификации.
Основные источники: [корень репозитория](../README.md), [каноническая спецификация](../agent_orchestrator_final_spec_v2.md), [индекс ADR](./09-adrs/README.md).

Это дерево превращает каноническую спецификацию в более узкие документы для реализации. Цель не в том, чтобы дублировать канон во многих местах, а в том, чтобы назначить один owner document для каждого важного правила, чтобы изменения кода знали, где искать сначала.

## Иерархия источников

Используйте дерево документации в таком порядке:

1. [Каноническая спецификация](../agent_orchestrator_final_spec_v2.md) определяет высокоуровневый смысл проекта и остается anti-contradiction anchor.
2. Фокусированный документ внутри `docs/` может уточнять одну конкретную тему, если он явно владеет этой темой и остается согласованным с canonical spec.
3. README-файлы определяют навигацию, scope раздела и ownership boundaries. Они должны резюмировать, маршрутизировать и ограничивать, но не должны молча вводить детальные contract rules, которые принадлежат leaf documents.
4. Если изменение значимое, спорное или имеет долгоживущие tradeoffs, фиксируйте решение в [`09-adrs`](./09-adrs/README.md).
5. Ненормативные reference targets находятся в [`12-reference-targets`](./12-reference-targets/README.md). Они являются только implementation references и не переопределяют canonical Dennett ownership.
6. Post-11 capability freezing и evidence-based status tracking находятся в [`13-capability-gap-lock`](./13-capability-gap-lock/README.md). Этот раздел не переопределяет subsystem behavior; он фиксирует, что сделано и что не сделано.
7. Первый executable native-memory slice после этой freeze находится в [`14-native-memory-integration`](./14-native-memory-integration/README.md). Этот раздел владеет тем, что Phase 13 фактически реализовала, не переопределяя сам portable memory contract.
8. Первый executable native-runtime-surface slice после memory work находится в [`15-native-runtime-surface`](./15-native-runtime-surface/README.md). Этот раздел владеет тем, что Phase 14 фактически реализовала, не превращая local runtime metadata в portable agent truth.
9. Authoring upgrade Builder 2.0 находится в [`18-builder-2-0`](./18-builder-2-0/README.md). Этот раздел владеет Phase 17 builder authoring boundaries, не заявляя Phase 18 integrated product flows.
10. Integrated product-flow ownership находится в [`19-integrated-product-flows`](./19-integrated-product-flows/README.md). Этот раздел владеет Phase 18 cross-subsystem flow definitions и acceptance boundaries, не заявляя Phase 19 external proof или release readiness.
11. Real-world proof и release decision ownership находятся в [`20-real-world-proof-and-release`](./20-real-world-proof-and-release/README.md). Этот раздел владеет Phase 19 live-proof evidence, stress/regression proof, operational runbooks, evidence logs и release decision record, обязательной перед заявлениями о release readiness.

Если два документа кажутся противоречащими друг другу, считайте более каноничный и более specific owner авторитетным только если он не противоречит top-level canon. Иначе это documentation defect, который нужно исправить, а не разрешение выбрать более удобный текст.

## Порядок чтения для реализации

| Шаг | Что читать | Почему это идет первым |
| --- | --- | --- |
| 1 | [`01-foundations`](./01-foundations/README.md) | Фиксирует смысл, границы, термины, sources of truth, defaults и stack. |
| 2 | [`02-architecture`](./02-architecture/README.md) | Определяет module boundaries и dependency rules. |
| 3 | [`03-contracts`](./03-contracts/README.md) | Определяет machine-checkable schemas и formal invariants. |
| 4 | [`04-execution`](./04-execution/README.md) | Управляет graph execution behavior. |
| 5 | [`05-state`](./05-state/README.md) | Управляет chats, resume и persisted operational state. |
| 6 | [`06-interaction`](./06-interaction/README.md) | Управляет comments, built-in user chat MCP и user-facing interaction. |
| 7 | [`07-lifecycle`](./07-lifecycle/README.md) | Управляет registry, drafts, live revisions и deploy semantics. |
| 8 | [`08-extensions`](./08-extensions/README.md) | Управляет memory, runtime sources, limits и другими non-core axes. |
| 9 | [`09-adrs`](./09-adrs/README.md) | Записывает, почему были приняты contested architectural choices. |
| 10 | [`10-examples`](./10-examples/README.md) | Иллюстрирует valid usage, не становясь normative. |
| 11 | [`11-hardening`](./11-hardening/README.md) | Управляет release gates, hardening scope, validation matrix и current-stage operational readiness. |
| 12 | [`12-reference-targets`](./12-reference-targets/README.md) | Только reference material; полезен для deliberate reproduction work, но не для canonical ownership. |
| 13 | [`13-capability-gap-lock`](./13-capability-gap-lock/README.md) | Управляет truthful post-11 capability freeze, Mem0-first readiness framing и handoff к later implementation stages. |
| 14 | [`14-native-memory-integration`](./14-native-memory-integration/README.md) | Управляет implemented Mem0-first native-memory slice, local provider registry, direct provider-backed memory operations и boundary к future runtime-attached memory support. |
| 15 | [`15-native-runtime-surface`](./15-native-runtime-surface/README.md) | Управляет implemented Phase 14 runtime surface: model discovery, runtime-environment introspection и first executable App Server-native runtime-option controls. |
| 17 | [`17-managed-subagent-orchestration/phase-16-managed-subagent-orchestration.md`](./17-managed-subagent-orchestration/phase-16-managed-subagent-orchestration.md) | Управляет current Phase 16 managed-subagent contract-completion slice и его remaining proof boundary. |
| 18 | [`18-builder-2-0`](./18-builder-2-0/README.md) | Управляет Phase 17 Builder 2.0 authoring boundary for public-contract-only drafts, которые могут ссылаться на memory, runtime, interaction и managed-subagent surfaces. |
| 19 | [`19-integrated-product-flows`](./19-integrated-product-flows/README.md) | Управляет Phase 18 integrated product-flow definitions across lifecycle, builder, runtime, interaction, memory и subagents, не повышая эти flows до Phase 19 external proof или release readiness. |
| 20 | [`20-real-world-proof-and-release`](./20-real-world-proof-and-release/README.md) | Управляет Phase 19 real-world proof и готовностью к релизу: live runtime/provider evidence, stress and regression proof, operational runbooks, evidence logs и release decision record. |

## Ответственность разделов

- [`01-foundations`](./01-foundations/README.md) владеет product identity, scope, boundaries, terminology, truth sources, defaults и stack lock.
- [`02-architecture`](./02-architecture/README.md) владеет layers, module boundaries, dependency constraints и staged boundary для App Server-native capability families.
- [`03-contracts`](./03-contracts/README.md) владеет formal schemas, type-level contracts, validation constraints и stable normalized boundary, которая уже полной App Server surface.
- [`04-execution`](./04-execution/README.md) владеет run behavior, graph traversal, node outcomes и execution semantics.
- [`05-state`](./05-state/README.md) владеет persisted operational state, таким как chats и resume.
- [`06-interaction`](./06-interaction/README.md) владеет user comments, live interaction, built-in communication MCP и user-facing staging boundary для richer runtime notifications.
- [`07-lifecycle`](./07-lifecycle/README.md) владеет registry behavior, drafts, live state, deploy и version axes, связанными с working copies.
- [`08-extensions`](./08-extensions/README.md) владеет features, намеренно вынесенными за пределы minimal stable core, включая runtime-source-local capability metadata, которая не должна становиться portable file truth.
- [`09-adrs`](./09-adrs/README.md) владеет history и rationale contested decisions, а не detailed operational contract.
- [`10-examples`](./10-examples/README.md) владеет worked examples и anti-pattern illustrations, но никогда underlying rule.
- [`11-hardening`](./11-hardening/README.md) владеет release-readiness rules для current product stage, включая hardening scope, release gates, validation expectations и operational-readiness framing без переопределения lower-level behavior.
- [`12-reference-targets`](./12-reference-targets/README.md) владеет navigation для non-normative reference material, который может информировать future implementation work, не становясь canonical behavior ownership.
- [`13-capability-gap-lock`](./13-capability-gap-lock/README.md) владеет truthful capability matrix after stages 1-11, evidence model for claiming later features are done и Mem0-first readiness freeze без переопределения subsystem behavior.
- [`14-native-memory-integration`](./14-native-memory-integration/README.md) владеет implemented Phase 13 Mem0-first slice, включая local provider registry и direct memory-operation boundary, без переопределения portable memory-binding contract.
- [`15-native-runtime-surface`](./15-native-runtime-surface/README.md) владеет implemented Phase 14 runtime surface, включая normalized model discovery, runtime-environment introspection и first executable runtime-option overrides, без переопределения portable agent-file truth.
- [`18-builder-2-0`](./18-builder-2-0/README.md) владеет Phase 17 Builder 2.0 authoring boundary и маршрутизирует detailed behavior обратно к public contracts, которые владеют memory, runtime, interaction, lifecycle и managed subagents.
- [`19-integrated-product-flows`](./19-integrated-product-flows/README.md) владеет Phase 18 integrated product-flow acceptance definitions и subsystem handoff rules, не владея Phase 19 external proof, release runbooks или release readiness.
- [`20-real-world-proof-and-release`](./20-real-world-proof-and-release/README.md) владеет Phase 19 real-world proof, stress/regression proof, operational runbooks, evidence logging и release decision record; никакое заявление о release readiness не валидно без этих evidence и decision.

Subagent system намеренно разделена между несколькими owner docs: [`02-architecture/subagent-orchestration-model.md`](./02-architecture/subagent-orchestration-model.md) владеет overall model, [`03-contracts/subagent-mcp-contract.md`](./03-contracts/subagent-mcp-contract.md) владеет managed child-run MCP surface, [`04-execution/subagent-task-lifecycle.md`](./04-execution/subagent-task-lifecycle.md) владеет delegated-task sequencing, [`05-state/subagent-context-and-memory.md`](./05-state/subagent-context-and-memory.md) владеет lineage и persisted child context, а [`08-extensions/builder-agent.md`](./08-extensions/builder-agent.md) потребляет эту модель, не переопределяя ее.

## Правила authoring

- У одного важного правила должен быть один owner document.
- Когда README начинает разрастаться detailed behavioral rules, вынесите тему в dedicated document и дайте ссылку на него.
- Examples, fixtures и tests могут демонстрировать behavior, но должны указывать обратно на owner document.
- Code не должен становиться первым местом, где появляется undocumented business logic. Если behavior пришлось изобрести, документируйте его в правильном section или ADR.
