[English](#english) | [Russian](#russian)

# English

## Builder Agent

Status: normative owner for the builder-agent extension.

Related documents:

- [Canonical spec](../../agent_orchestrator_final_spec_v2.md)
- [Core and Interfaces](../02-architecture/core-and-interfaces.md)
- [Runtime Integration Model](../02-architecture/runtime-integration-model.md)
- [Subagent Orchestration Model](../02-architecture/subagent-orchestration-model.md)
- [Subagent MCP Contract](../03-contracts/subagent-mcp-contract.md)
- [Subagent Task Lifecycle](../04-execution/subagent-task-lifecycle.md)
- [Subagent Context and Memory](../05-state/subagent-context-and-memory.md)
- [Agent JSON contract](../03-contracts/agent-json/README.md)
- [Draft, Live, and Deploy](../07-lifecycle/draft-live-deploy.md)
- [Agent Registry](../07-lifecycle/agent-registry.md)
- [Phase 17 Builder 2.0](../18-builder-2-0/phase-17-builder-2-0.md)
- [ADR-0001: Codex-First, Not Codex-Only](../09-adrs/ADR-0001-codex-first-not-codex-only.md)

## Role

The builder agent is a system agent dedicated to creating and revising agent definitions.

It remains an agent inside the same Core architecture as other agents. It is not hidden prompt code, a UI-only shortcut, or a side channel outside the documented runtime and lifecycle boundaries.

## Phase 10 Executable Slice

Phase 10 implements the first executable builder slice with these rules:

- builder exists as a real system-agent resource that Core and interfaces may invoke;
- the first user-facing entrypoint is an interface-backed flow, starting with CLI, and interfaces stay thin over Core;
- invocation uses the existing runtime path rather than a builder-only execution mechanism;
- builder returns candidate portable agent JSON for create or revise flows;
- Core validates the candidate against the supported contract before any persistence happens;
- an accepted candidate is stored as a draft revision by default through the normal lifecycle and registry surfaces;
- deploy remains a separate explicit action, not an automatic side effect of building.

This is the current implemented target for the builder extension. Anything broader must be described as later scope, not implied as already present.

## Builder 2.0 Authoring Scope

Phase 17 extends the documented builder target from the Phase 10 draft-only slice to a richer draft authoring assistant for the public portable contract.

Builder 2.0 may author candidate agent JSON that uses supported memory bindings, runtime-source intent, interaction settings, and portable `orchestrator_agent` graph structure. Managed-subagent task packages, roles, write scopes, budgets, findings, close semantics, and review/fix loop details belong to the managed Subagent MCP/product surface, not to portable agent JSON. Builder 2.0 may also use validation errors, owner-doc constraints, and review feedback to produce revised draft candidates.

This is still draft-first and public-contract-only:

- builder output remains candidate portable agent JSON;
- Core and lifecycle services still own validation, identity checks, draft persistence, and deploy;
- subsystem owner docs still own memory, runtime, interaction, and managed-subagent semantics;
- integrated product-flow proof remains later Phase 18 scope.

Builder 2.0 does not make the builder a hidden execution engine, provider registry, runtime selector, user-chat state owner, managed-subagent service, or deploy authority.

## What the Phase 10 Builder May Do

In the Phase 10 slice, the builder may:

- create a new logical agent by producing a validated draft revision;
- revise an existing logical agent by producing a validated draft revision tied to that agent;
- inspect permitted agent files, drafts, requirements documents, examples, and nearby repository context when that context is explicitly granted;
- recommend deploy after draft creation or revision.

The architecture allows builder behavior to participate in larger Core workflows, but Phase 10 requires only the executable system-resource flow described above. A portable graph-node builder surface is not implied by this document.

## Identity Rules for Create vs Revise

Phase 10 builder flows must preserve lifecycle and registry ownership of logical agent identity.

- In a `revise` flow, the target logical agent identity is already known before builder invocation.
- A revised candidate must keep that same logical identity in `meta.id`.
- A revise flow must fail before draft persistence if the returned candidate changes `meta.id` to a different logical agent.
- In a `create` flow, the candidate must provide a concrete `meta.id` that validates under the portable contract.
- A create flow must fail before draft persistence if that `meta.id` already belongs to an existing logical agent and the request was not an explicit revise flow.

The builder does not get to silently remap one logical agent into another. Identity conflicts and logical-agent ownership remain governed by the registry and lifecycle documents.

## Phase 10 Operating Model

The normal Phase 10 builder workflow is:

1. understand the user task, target agent, and constraints;
2. gather only the permitted read context needed for authoring;
3. invoke the builder system resource through the existing runtime path;
4. parse the returned candidate as portable agent JSON;
5. validate the candidate against the supported contract;
6. check that the candidate identity is valid for the requested create or revise mode;
7. persist a validated candidate as a draft revision through lifecycle services;
8. leave deploy to a separate explicit later action.

This workflow is intentionally more precise than "generate some JSON". It is also intentionally narrower than a full multi-candidate evaluation and ranking subsystem.

## Context Ingestion Boundary

The builder may consume context such as:

- the target agent file or current draft being revised;
- requirements documents, examples, or other owner-approved specification material;
- selected repository files or documentation needed to ground the requested agent behavior;
- summarized prior validation failures or user-supplied constraints.

That context is explicit and permission-gated. The builder does not gain ambient access to the whole workspace, to unrelated agents, or to hidden local state merely because it is a builder.

## Relationship to Runtime

Builder invocation uses the same runtime boundary as other runtime-backed work.

That means:

- Core still owns invocation, result handling, and validation;
- the runtime adapter boundary from [Runtime Integration Model](../02-architecture/runtime-integration-model.md) still applies;
- Codex-specific behavior remains behind the adapter boundary rather than becoming builder-specific Core semantics.

The builder system resource is therefore part of what gets run, not a license to bypass the existing runtime path.

When Builder 2.0 authors runtime-source or runtime-option intent, that intent must stay inside the public runtime-source and runtime-adapter contracts. Local runtime metadata and App Server-specific capability details are capability inputs, not portable file truth.

## Relationship to Lifecycle

The builder operates on drafts by default.

It must interact with lifecycle through the same registry and deploy semantics as any other editing flow:

- unvalidated builder output must not be stored as a draft or live revision;
- a revise flow must preserve the target logical agent identity rather than silently changing `meta.id`;
- a create flow must not silently collide with an existing logical agent identity;
- accepted builder output becomes a draft revision for one logical agent;
- normal opens, runs, and events keep using the current live revision until an explicit deploy happens;
- registry truth still comes from durable files and lifecycle state, not from builder memory or hidden local state.

Builder 2.0 may recommend review, validation, dry-run, or deploy as next steps, but it must not silently promote a draft to live or treat a successful draft as proof of deployment.

## Relationship to Memory

Builder 2.0 may author `memory_bindings` when the requested agent design needs memory and when the resulting fields are valid under the public memory contract.

The builder may describe provider capability requirements and may revise invalid bindings after validation feedback. It must not register providers, store secrets, claim a provider is live because a binding exists, or assume runtime-native memory consumption where the current runtime path has not implemented it.

Memory semantics remain owned by the memory-binding contract and memory extension docs.

## Relationship to Interaction

Builder 2.0 may author agents that use documented comments, prompt/reply, wait-state, and resume behavior.

It must keep risky mid-run changes, blocked prompts, replies, and user-visible state transitions inside the interaction owner docs. A builder-authored draft is not evidence that full cross-interface interaction behavior has been proven.

## Relationship to the Subagent System

This document consumes the managed subagent model; it does not redefine it.

If a builder workflow decomposes work, launches review passes, or requests bounded repair, it must use the managed subagent system documented in the architecture, contract, execution, and state owner docs. Those child runs remain interaction-silent at the parent user boundary in the base model.

Builder 2.0 may help author plans that reference managed-subagent roles, task packages, write scopes, budgets, findings, review loops, and close semantics, but those details must stay in the managed Subagent MCP/product surface. Portable agent JSON may include only the portable `orchestrator_agent` nested-graph primitive unless a future owner contract explicitly adds portable fields for richer managed-subagent semantics. Builder 2.0 must not redefine those semantics, assign overlapping write scopes, or spawn hidden subagents as an unstated side effect of draft creation.

## Relationship to Manual Editing

The builder accelerates authoring, but it does not replace direct JSON editing.

Portable agent files remain normal files that a user can inspect, diff, edit, commit, copy, and review without going through the builder. Any implementation that makes builder output inaccessible outside the builder workflow would violate the architecture.

## Relationship to Other Extensions

The builder may author `memory_bindings`, `runtime_sources`, skills, MCP references, plugins, and other allowed fields inside agent files when the requested agent design needs them.

Authoring those fields does not give the builder the right to redefine their semantics. Their meaning remains owned by the contract, lifecycle, and extension documents that govern them.

## Deferred Beyond Phase 10

The following capabilities are not implied as current Phase 10 behavior:

- broad managed subagent orchestration inside builder workflows;
- mandatory runtime execution of every candidate agent before draft persistence;
- a general candidate comparison or ranking subsystem across multiple revisions;
- a portable builder graph-node surface inside user agent files unless another owner doc defines it;
- hidden builder-only fields or state that bypass the portable contract or lifecycle.

## Deferred Beyond Phase 17

The following capabilities are not implied as Builder 2.0 behavior:

- integrated product-flow proof across builder, lifecycle, runtime features, interaction, memory, and managed subagents;
- automatic real-provider or real-runtime proof for every generated draft;
- builder-owned provider registration, runtime account configuration, or secret management;
- builder-owned cross-interface user interaction semantics;
- hidden managed-subagent orchestration that is not represented by public contracts and durable state.

## What the Builder Must Not Become

The builder must not become:

- the only supported path for creating or editing agents;
- a hidden registry authority that bypasses file truth;
- a silent deploy daemon;
- a substitute for the runtime adapter boundary;
- an excuse to smuggle undeclared fields into the portable contract;
- an overclaimed feature surface that pretends deferred behavior is already implemented.

# Russian

## Builder Agent

Статус: нормативный владелец расширения builder-agent.

Связанные документы:

- [Canonical spec](../../agent_orchestrator_final_spec_v2.md)
- [Core and Interfaces](../02-architecture/core-and-interfaces.md)
- [Runtime Integration Model](../02-architecture/runtime-integration-model.md)
- [Subagent Orchestration Model](../02-architecture/subagent-orchestration-model.md)
- [Subagent MCP Contract](../03-contracts/subagent-mcp-contract.md)
- [Subagent Task Lifecycle](../04-execution/subagent-task-lifecycle.md)
- [Subagent Context and Memory](../05-state/subagent-context-and-memory.md)
- [Agent JSON contract](../03-contracts/agent-json/README.md)
- [Draft, Live, and Deploy](../07-lifecycle/draft-live-deploy.md)
- [Agent Registry](../07-lifecycle/agent-registry.md)
- [Phase 17 Builder 2.0](../18-builder-2-0/phase-17-builder-2-0.md)
- [ADR-0001: Codex-First, Not Codex-Only](../09-adrs/ADR-0001-codex-first-not-codex-only.md)

## Роль

Builder agent - системный агент, предназначенный для создания и пересмотра определений агентов.

Он остается агентом внутри той же архитектуры Core, что и остальные агенты. Это не скрытый prompt-код, не UI-only shortcut и не side channel за пределами документированных границ runtime и lifecycle.

## Исполняемый срез Phase 10

Phase 10 реализует первый исполняемый builder slice со следующими правилами:

- builder существует как реальный system-agent resource, который могут вызывать Core и interfaces;
- первая пользовательская точка входа - interface-backed flow, начиная с CLI, а interfaces остаются тонкими оболочками над Core;
- invocation использует существующий runtime path, а не отдельный builder-only execution mechanism;
- builder возвращает candidate portable Agent JSON для create или revise flows;
- Core валидирует candidate по supported contract до любой persistence;
- принятый candidate по умолчанию сохраняется как draft revision через обычные lifecycle и registry surfaces;
- deploy остается отдельным явным действием, а не автоматическим side effect build.

Это текущая реализуемая цель расширения builder. Все, что шире, должно описываться как later scope, а не подразумеваться как уже существующее.

## Область authoring Builder 2.0

Phase 17 расширяет documented builder target от Phase 10 draft-only slice до более богатого draft authoring assistant для публичного portable contract.

Builder 2.0 может author candidate Agent JSON, который использует supported memory bindings, runtime-source intent, interaction settings и переносимую graph structure `orchestrator_agent`. Managed-subagent task packages, roles, write scopes, budgets, findings, close semantics и review/fix loop details относятся к managed Subagent MCP/product surface, а не к portable Agent JSON. Builder 2.0 также может использовать validation errors, owner-doc constraints и review feedback, чтобы produce revised draft candidates.

Это все еще draft-first и public-contract-only:

- builder output остается candidate portable Agent JSON;
- Core и lifecycle services по-прежнему владеют validation, identity checks, draft persistence и deploy;
- subsystem owner docs по-прежнему владеют memory, runtime, interaction и managed-subagent semantics;
- integrated product-flow proof остается later Phase 18 scope.

Builder 2.0 не превращает builder в hidden execution engine, provider registry, runtime selector, user-chat state owner, managed-subagent service или deploy authority.

## Что builder Phase 10 может делать

В срезе Phase 10 builder может:

- создавать новый logical agent, производя validated draft revision;
- пересматривать существующий logical agent, производя validated draft revision, связанную с этим agent;
- inspect permitted agent files, drafts, requirements documents, examples и nearby repository context, когда этот context явно предоставлен;
- рекомендовать deploy после draft creation или revision.

Архитектура допускает участие builder behavior в более крупных Core workflows, но Phase 10 требует только executable system-resource flow, описанный выше. Portable graph-node builder surface этим документом не подразумевается.

## Правила identity для create и revise

Builder flows в Phase 10 должны сохранять lifecycle и registry ownership logical agent identity.

- В `revise` flow target logical agent identity известна до invocation builder.
- Revised candidate обязан сохранить ту же logical identity в `meta.id`.
- Revise flow должен завершиться ошибкой до draft persistence, если returned candidate меняет `meta.id` на identity другого logical agent.
- В `create` flow candidate должен предоставить concrete `meta.id`, валидный по portable contract.
- Create flow должен завершиться ошибкой до draft persistence, если этот `meta.id` уже принадлежит existing logical agent и request не был explicit revise flow.

Builder не может silently remap один logical agent в другой. Identity conflicts и logical-agent ownership остаются governed by registry and lifecycle documents.

## Operating model Phase 10

Обычный workflow builder в Phase 10:

1. понять user task, target agent и constraints;
2. собрать только permitted read context, нужный для authoring;
3. вызвать builder system resource через existing runtime path;
4. разобрать returned candidate как portable Agent JSON;
5. validate candidate по supported contract;
6. проверить, что candidate identity valid для requested create или revise mode;
7. persist validated candidate как draft revision через lifecycle services;
8. оставить deploy отдельным explicit later action.

Этот workflow намеренно точнее, чем "generate some JSON". Он также намеренно уже, чем full multi-candidate evaluation and ranking subsystem.

## Граница context ingestion

Builder может consume context, такой как:

- target agent file или current draft, который пересматривается;
- requirements documents, examples или другие owner-approved specification material;
- selected repository files или documentation, нужные для grounding requested agent behavior;
- summarized prior validation failures или user-supplied constraints.

Этот context является explicit и permission-gated. Builder не получает ambient access ко всему workspace, unrelated agents или hidden local state только потому, что он builder.

## Связь с runtime

Builder invocation использует ту же runtime boundary, что и другая runtime-backed work.

Это означает:

- Core все еще владеет invocation, result handling и validation;
- runtime adapter boundary из [Runtime Integration Model](../02-architecture/runtime-integration-model.md) все еще применяется;
- Codex-specific behavior остается за adapter boundary, а не становится builder-specific Core semantics.

Builder system resource поэтому является частью того, что запускается, а не разрешением обходить existing runtime path.

Когда Builder 2.0 authors runtime-source или runtime-option intent, этот intent должен оставаться внутри public runtime-source и runtime-adapter contracts. Local runtime metadata и App Server-specific capability details являются capability inputs, а не portable file truth.

## Связь с lifecycle

Builder по умолчанию работает с drafts.

Он должен взаимодействовать с lifecycle через те же registry и deploy semantics, что и любой другой editing flow:

- unvalidated builder output нельзя сохранять как draft или live revision;
- revise flow должен preserve target logical agent identity, а не silently change `meta.id`;
- create flow не должен silently collide с existing logical agent identity;
- accepted builder output становится draft revision одного logical agent;
- normal opens, runs и events продолжают использовать current live revision до explicit deploy;
- registry truth все еще идет из durable files и lifecycle state, а не из builder memory или hidden local state.

Builder 2.0 может рекомендовать review, validation, dry-run или deploy как next steps, но он не должен silently promote draft to live или считать successful draft proof of deployment.

## Связь с memory

Builder 2.0 может author `memory_bindings`, когда requested agent design нуждается в memory и resulting fields valid по public memory contract.

Builder может описывать provider capability requirements и revise invalid bindings after validation feedback. Он не должен register providers, store secrets, claim provider is live because binding exists или assume runtime-native memory consumption там, где current runtime path это не реализовал.

Memory semantics остаются во владении memory-binding contract и memory extension docs.

## Связь с interaction

Builder 2.0 может author agents, которые используют documented comments, prompt/reply, wait-state и resume behavior.

Он должен держать risky mid-run changes, blocked prompts, replies и user-visible state transitions внутри interaction owner docs. Builder-authored draft не является evidence, что full cross-interface interaction behavior proven.

## Связь с subagent system

Этот документ consumes managed subagent model; он не переопределяет его.

Если builder workflow decomposes work, launches review passes или requests bounded repair, он должен использовать managed subagent system, documented in architecture, contract, execution и state owner docs. Эти child runs остаются interaction-silent на parent user boundary в базовой модели.

Builder 2.0 может помогать author plans, которые reference managed-subagent roles, task packages, write scopes, budgets, findings, review loops и close semantics, но эти details должны оставаться в managed Subagent MCP/product surface. Portable Agent JSON может включать только portable `orchestrator_agent` nested-graph primitive, если future owner contract явно не добавит portable fields для richer managed-subagent semantics. Builder 2.0 не должен redefine those semantics, assign overlapping write scopes или spawn hidden subagents as unstated side effect of draft creation.

## Связь с ручным редактированием

Builder ускоряет authoring, но не заменяет direct JSON editing.

Portable agent files остаются обычными файлами, которые пользователь может inspect, diff, edit, commit, copy и review без builder. Любая реализация, делающая builder output inaccessible outside builder workflow, нарушает architecture.

## Связь с другими extensions

Builder может author `memory_bindings`, `runtime_sources`, skills, MCP references, plugins и другие allowed fields внутри agent files, когда они нужны requested agent design.

Authoring этих fields не дает builder права redefine their semantics. Их meaning остается owned by contract, lifecycle и extension documents, которые ими управляют.

## Отложено за пределы Phase 10

Следующие capabilities не подразумеваются как current Phase 10 behavior:

- broad managed subagent orchestration inside builder workflows;
- mandatory runtime execution каждого candidate agent before draft persistence;
- general candidate comparison или ranking subsystem across multiple revisions;
- portable builder graph-node surface inside user agent files, если другой owner doc не определит ее;
- hidden builder-only fields или state, обходящие portable contract или lifecycle.

## Отложено за пределы Phase 17

Следующие capabilities не подразумеваются как Builder 2.0 behavior:

- integrated product-flow proof across builder, lifecycle, runtime features, interaction, memory и managed subagents;
- automatic real-provider или real-runtime proof для каждого generated draft;
- builder-owned provider registration, runtime account configuration или secret management;
- builder-owned cross-interface user interaction semantics;
- hidden managed-subagent orchestration, не представленная public contracts и durable state.

## Чем builder не должен становиться

Builder не должен становиться:

- единственным supported path для создания или редактирования agents;
- hidden registry authority, обходящей file truth;
- silent deploy daemon;
- substitute for runtime adapter boundary;
- excuse to smuggle undeclared fields into portable contract;
- overclaimed feature surface, которая делает вид, что deferred behavior уже implemented.