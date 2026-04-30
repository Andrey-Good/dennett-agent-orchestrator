# Documentation Map

Status: documentation root and ownership model.
Owns: documentation reading order, section responsibilities, and escalation rules for new normative decisions.
Does not own: product behavior that belongs to a narrower specification.
Primary sources: [repository root](../README.md), [canonical specification](../agent_orchestrator_final_spec_v2.md), [ADR index](./09-adrs/README.md), and [public docs and claims owner](./21-public-launch-readiness/public-docs-onboarding-and-claims.md).

This tree turns the canonical specification into smaller topic-focused documents. The goal is not to duplicate the canon in many places, but to assign one owner document to each important rule so contributors know where to look first.

## Start Here

If you are learning what the product does, start with the [Functionality Learning Guide](./functionality-learning-guide.md). It gives a short route through examples, core concepts, operator commands, experimental surfaces, and contributor architecture docs before you use the owner-document map below.

| Need | Start with | Then read |
| --- | --- | --- |
| Learn the product functionality | [Functionality Learning Guide](./functionality-learning-guide.md) | [Examples](./10-examples/README.md), especially the canonical agent JSON example and interaction sequences. |
| Understand the product roadmap and stage gates | [Product Roadmap And Safety Gates](./product-roadmap-and-safety-gates.md) | [Capability Gap Lock](./13-capability-gap-lock/phase-12-capability-gap-lock.md), [Release Gates](./11-hardening/release-gates.md), and [Phase 19 Real-World Proof And Release](./20-real-world-proof-and-release/README.md). |
| Understand core concepts | [Foundations](./01-foundations/README.md) | [Agent JSON contract](./03-contracts/agent-json/README.md), [graph execution](./04-execution/graph-execution.md), [chat and resume](./05-state/chat-and-resume.md), and [draft/live/deploy](./07-lifecycle/draft-live-deploy.md). |
| Operate the local CLI | [Stable CLI/API Contract Freeze](./21-public-launch-readiness/stable-cli-api-contract-freeze.md) | [Lifecycle](./07-lifecycle/README.md), [interaction sequences](./10-examples/interaction-sequences.md), and [operational readiness](./11-hardening/operational-readiness.md). |
| Explore optional surfaces | [Extensions](./08-extensions/README.md) | [memory bindings](./08-extensions/memory-bindings.md), [runtime sources](./08-extensions/runtime-sources.md), [builder agent](./08-extensions/builder-agent.md), [subagent model](./02-architecture/subagent-orchestration-model.md), and [events and triggers](./07-lifecycle/events-and-triggers.md). |
| Contribute to architecture or contracts | [Architecture](./02-architecture/README.md) | [Contracts](./03-contracts/README.md), [ADRs](./09-adrs/README.md), [hardening](./11-hardening/README.md), and [public launch readiness](./21-public-launch-readiness/README.md). |

## Source Hierarchy

Use the documentation tree in this order:

1. The [canonical specification](../agent_orchestrator_final_spec_v2.md) defines the high-level meaning of the project and remains the anti-contradiction anchor.
2. A focused document inside `docs/` may refine one specific topic if it explicitly owns that topic and stays consistent with the canonical spec.
3. README files define navigation, section scope, and ownership boundaries. They should summarize, route, and constrain, but they should not silently introduce detailed contract rules that belong in leaf documents.
4. If a change is significant, contested, or has long-lived tradeoffs, capture the decision in [`09-adrs`](./09-adrs/README.md).
5. Non-normative reference targets live in [`12-reference-targets`](./12-reference-targets/README.md). They are implementation references only and do not override canonical Dennett ownership.
6. Capability freezing and evidence-based status tracking live in [`13-capability-gap-lock`](./13-capability-gap-lock/README.md). That section records what is and is not actually done.
7. Owner documents for sections `14` through `20` record bounded feature areas without expanding public launch claims by themselves.
8. Public launch readiness ownership lives in [`21-public-launch-readiness`](./21-public-launch-readiness/README.md). That section owns public-launch boundaries, onboarding claims, forbidden public claims, and the bounded stable CLI/API freeze.

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
| 11 | [`11-hardening`](./11-hardening/README.md) | Governs release readiness rules, hardening scope, validation matrix, and operational readiness. |
| 12 | [`12-reference-targets`](./12-reference-targets/README.md) | Reference material only; useful for deliberate reproduction work, but not canonical ownership. |
| 13 | [`13-capability-gap-lock`](./13-capability-gap-lock/README.md) | Governs the truthful capability freeze, Mem0-first readiness framing, and handoff to later roadmap work. |
| 14 | [`14-native-memory-integration`](./14-native-memory-integration/README.md) | Governs the implemented Mem0-first native-memory slice and local provider registry boundary. |
| 15 | [`15-native-runtime-surface`](./15-native-runtime-surface/README.md) | Governs model discovery, runtime-environment introspection, and executable App Server-native runtime-option controls. |
| 16 | [`16-full-user-interaction-layer`](./16-full-user-interaction-layer/) | Governs the user interaction layer boundary. |
| 17 | [`17-managed-subagent-orchestration`](./17-managed-subagent-orchestration/) | Governs the managed-subagent contract-completion slice and remaining proof boundary. |
| 18 | [`18-builder-2-0`](./18-builder-2-0/README.md) | Governs the Builder 2.0 authoring boundary for public-contract-only drafts. |
| 19 | [`19-integrated-product-flows`](./19-integrated-product-flows/README.md) | Governs integrated product-flow definitions without promoting them to external proof or release readiness. |
| 20 | [`20-real-world-proof-and-release`](./20-real-world-proof-and-release/README.md) | Governs real-world proof, stress/regression proof, operational runbooks, evidence logs, and release decisions. |
| 21 | [`21-public-launch-readiness`](./21-public-launch-readiness/README.md) | Governs public-launch readiness boundaries, forbidden public claims, onboarding docs, and the bounded stable CLI/API freeze. |

## Section Responsibilities

- [`01-foundations`](./01-foundations/README.md) owns product identity, scope, boundaries, terminology, truth sources, defaults, and stack lock.
- [`02-architecture`](./02-architecture/README.md) owns layers, module boundaries, dependency constraints, and the bounded roadmap for App Server-native capability families.
- [`03-contracts`](./03-contracts/README.md) owns formal schemas, type-level contracts, validation constraints, and the stable normalized boundary.
- [`04-execution`](./04-execution/README.md) owns run behavior, graph traversal, node outcomes, and execution semantics.
- [`05-state`](./05-state/README.md) owns persisted operational state such as chats and resume.
- [`06-interaction`](./06-interaction/README.md) owns user comments, live interaction, the built-in communication MCP, and user-facing interaction staging.
- [`07-lifecycle`](./07-lifecycle/README.md) owns registry behavior, drafts, live state, deploy, and version axes tied to working copies.
- [`08-extensions`](./08-extensions/README.md) owns features intentionally kept outside the minimal stable core, including runtime-source-local capability metadata that must not become portable file truth.
- [`09-adrs`](./09-adrs/README.md) owns the history and rationale of contested decisions, not the detailed operational contract itself.
- [`10-examples`](./10-examples/README.md) owns worked examples and anti-pattern illustrations, never the underlying rule.
- [`11-hardening`](./11-hardening/README.md) owns release-readiness rules for the current product maturity point.
- [`12-reference-targets`](./12-reference-targets/README.md) owns navigation for non-normative reference material.
- [`13-capability-gap-lock`](./13-capability-gap-lock/README.md) owns the truthful capability matrix after the foundational roadmap work and the evidence model for later feature claims.
- [`14-native-memory-integration`](./14-native-memory-integration/README.md) owns the Mem0-first native-memory slice.
- [`15-native-runtime-surface`](./15-native-runtime-surface/README.md) owns the native runtime surface.
- [`16-full-user-interaction-layer`](./16-full-user-interaction-layer/) owns the user interaction layer.
- [`17-managed-subagent-orchestration`](./17-managed-subagent-orchestration/) owns the managed-subagent orchestration slice.
- [`18-builder-2-0`](./18-builder-2-0/README.md) owns the Builder 2.0 authoring boundary.
- [`19-integrated-product-flows`](./19-integrated-product-flows/README.md) owns integrated product-flow acceptance definitions and subsystem handoff rules.
- [`20-real-world-proof-and-release`](./20-real-world-proof-and-release/README.md) owns real-world proof, operational runbooks, evidence logging, and release decisions.
- [`21-public-launch-readiness`](./21-public-launch-readiness/README.md) owns public-launch readiness boundaries, public docs onboarding, forbidden claims, and the bounded stable CLI/API contract freeze.

The subagent system is intentionally split across several owner docs: [`02-architecture/subagent-orchestration-model.md`](./02-architecture/subagent-orchestration-model.md) owns the overall model, [`03-contracts/subagent-mcp-contract.md`](./03-contracts/subagent-mcp-contract.md) owns the managed child-run MCP surface, [`04-execution/subagent-task-lifecycle.md`](./04-execution/subagent-task-lifecycle.md) owns delegated-task sequencing, [`05-state/subagent-context-and-memory.md`](./05-state/subagent-context-and-memory.md) owns lineage and persisted child context, and [`08-extensions/builder-agent.md`](./08-extensions/builder-agent.md) consumes that model without redefining it.

## Authoring Rules

- One important rule should have one owner document.
- When a README starts growing detailed behavioral rules, split that topic into a dedicated document and link to it.
- Examples, fixtures, and tests may demonstrate behavior, but they should point back to the owner document.
- Code should not become the first place where undocumented business logic appears. If behavior had to be invented, document it in the correct section or an ADR.
