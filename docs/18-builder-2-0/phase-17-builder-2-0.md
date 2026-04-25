[English](#english) | [Russian](#russian)

<a id="english"></a>
# Phase 17 Builder 2.0

Status: owner note for the Phase 17 Builder 2.0 documentation slice.

Related documents:

- [Builder Agent](../08-extensions/builder-agent.md)
- [Agent JSON Contract](../03-contracts/agent-json/README.md)
- [Memory Binding Model Contract](../03-contracts/agent-json/memory-binding-model-contract.md)
- [Runtime Sources](../08-extensions/runtime-sources.md)
- [Runtime Adapter Contract](../03-contracts/runtime-adapter-contract.md)
- [Interaction and Chat Contract](../03-contracts/agent-json/interaction-and-chat-contract.md)
- [Phase 16 Managed Subagent Orchestration](../17-managed-subagent-orchestration/phase-16-managed-subagent-orchestration.md)

## Goal

Phase 17 upgrades the builder from a first draft-producing slice into a richer authoring assistant for the current public portable contract.

The important boundary is "authoring assistant". Builder 2.0 may produce and revise candidate agent JSON that uses documented memory, runtime, interaction, and portable `orchestrator_agent` graph structure. Managed-subagent task packages, roles, write scopes, budgets, findings, close semantics, and review/fix loop details belong to the managed Subagent MCP and product surface; they are not portable agent JSON fields. Builder 2.0 must not invent private builder-only semantics, bypass validation, mutate live revisions directly, or claim that a drafted agent has been proven as an integrated product flow.

## Supported Authoring Boundary

Builder 2.0 may draft or revise:

- portable `memory_bindings` that point at documented provider requirements and capability constraints;
- runtime-source and runtime-option intent that is valid under the public runtime-source and runtime-adapter contracts;
- interaction settings that rely on documented comments, prompt/reply, wait-state, and resume behavior;
- portable `orchestrator_agent` graph patterns that remain valid under the agent JSON contract, while routing managed-subagent task-package and review-loop details to the Subagent MCP owner docs instead of embedding them as portable fields;
- validation-aware revisions that respond to schema, lifecycle, or owner-doc feedback by producing a new draft candidate.

Builder 2.0 must treat these as references to public contracts. The builder does not become the owner of the provider registry, model discovery, user-chat state machine, managed-subagent service, or lifecycle deploy rules.

## Draft-First Lifecycle Discipline

All Builder 2.0 output remains draft-first:

1. the builder returns candidate portable agent JSON;
2. Core parses and validates the candidate against supported schemas and invariants;
3. create and revise identity rules are checked before persistence;
4. accepted output is persisted as a draft revision;
5. deploy remains a separate explicit lifecycle action;
6. live runs continue to use the current live revision until deploy occurs.

Builder 2.0 may recommend validations, dry runs, review passes, or deployment. It must not silently execute deploy, mark a draft as live, or treat its own confidence as proof that the lifecycle is complete.

## Public-Contract-Only Rule

Builder-authored behavior must be expressible through documented contracts that a user can inspect, diff, edit, validate, and run without hidden builder state.

Forbidden Builder 2.0 shortcuts include:

- private fields that only the builder understands;
- hidden memory of draft intent that changes execution after persistence;
- runtime-specific instructions that bypass the runtime adapter boundary;
- provider-specific assumptions that bypass memory capability negotiation;
- subagent orchestration instructions that ignore write-scope, budget, or close semantics;
- interaction behavior that depends on a UI-only side channel outside the documented interaction contract.

## Memory Authoring Boundary

Builder 2.0 may author memory bindings only as portable configuration and design intent.

It may:

- include bindings that reference documented memory provider requirements;
- explain provider capability prerequisites that must be satisfied outside the agent file;
- avoid memory fields when a requested design can work without memory;
- revise invalid bindings in response to validation or provider-capability feedback.

It must not:

- register providers by editing the agent file;
- claim a memory provider is live merely because a binding exists;
- place secrets, credentials, or local provider setup inside portable agent JSON;
- assume runtime-native memory consumption where the current runtime path has not implemented it.

## Runtime Authoring Boundary

Builder 2.0 may author runtime-source and runtime-option intent that is valid under the public runtime contracts.

It may describe preferred models, reasoning effort, speed tier, or source selection only when those concepts are represented by the supported contract and gated by runtime capability metadata.

It must not:

- treat local runtime metadata as portable file truth;
- bypass source-selection or capability checks;
- assume every runtime supports App Server-native options;
- make builder-specific runtime behavior part of Core semantics.

## Interaction Authoring Boundary

Builder 2.0 may draft agents that use the documented user-interaction model.

It may author instructions that expect comments, prompt/reply, wait-state, and resume behavior where those surfaces are supported. It must keep risky mid-run changes, blocked prompts, and reply handling inside the owner interaction contracts.

It must not claim that a single builder-authored draft proves full cross-interface interaction behavior. Broader user-visible flow proof belongs to Phase 18 and Phase 19.

## Managed Subagent Authoring Boundary

Builder 2.0 may help users author managed-subagent patterns, but it must consume the Phase 16 managed layer rather than redefining it.

Portable agent JSON may describe only the portable `orchestrator_agent` nested-graph primitive. The richer managed-subagent surface is a separate product/MCP boundary: task packages, worker/reviewer roles, write scopes, budgets, findings, close semantics, cancellation, and enforced review/fix loops are managed Subagent MCP concepts and must not be serialized into agent JSON unless a future owner contract explicitly adds portable fields.

It may propose:

- worker, reviewer, and final-review roles where the public managed-subagent contract supports them;
- task packages with clear goals, read/write boundaries, budgets, and acceptance criteria;
- review-and-repair structures that preserve explicit close semantics;
- write-scope partitions that avoid sibling conflicts.

It must not:

- spawn managed subagents as a hidden side effect of draft creation unless the current public builder flow explicitly supports that execution step;
- assign overlapping write scopes;
- ignore budget or cancellation state;
- expose child interaction through the parent user boundary unless the interaction owner docs define that behavior.

## Validation And Self-Review

Builder 2.0 should become more validation-aware, but validation remains an external gate owned by Core and the relevant contracts.

A Builder 2.0 flow may use schema errors, owner-doc constraints, lifecycle identity checks, and reviewer findings as input for a revised candidate. The durable result is still a new candidate draft, not an unverifiable internal repair.

## What Phase 17 Does Not Claim

Phase 17 does not claim:

- integrated product flows that combine builder, lifecycle, runtime, interaction, memory, and managed subagents end to end;
- successful execution of every builder-authored agent;
- real-provider or real-runtime proof beyond the evidence recorded in the subsystem owner docs;
- broad external live proof for managed subagent workflows;
- a builder-only authority over registry, deployment, provider setup, runtime selection, or user-chat state.

Those claims belong to later integrated product-flow and release-proof phases when backed by executable evidence.

<a id="russian"></a>
# Phase 17 Builder 2.0

Статус: owner note для документационного среза Phase 17 Builder 2.0.

Связанные документы:

- [Builder Agent](../08-extensions/builder-agent.md)
- [Agent JSON Contract](../03-contracts/agent-json/README.md)
- [Memory Binding Model Contract](../03-contracts/agent-json/memory-binding-model-contract.md)
- [Runtime Sources](../08-extensions/runtime-sources.md)
- [Runtime Adapter Contract](../03-contracts/runtime-adapter-contract.md)
- [Interaction and Chat Contract](../03-contracts/agent-json/interaction-and-chat-contract.md)
- [Phase 16 Managed Subagent Orchestration](../17-managed-subagent-orchestration/phase-16-managed-subagent-orchestration.md)

## Цель

Phase 17 обновляет builder из первого draft-producing slice в более богатого authoring assistant для текущего публичного переносимого контракта.

Важная граница - "authoring assistant". Builder 2.0 может создавать и пересматривать candidate Agent JSON, который использует документированные memory, runtime, interaction и переносимую структуру графа `orchestrator_agent`. Пакеты задач managed subagent, роли, области записи, бюджеты, findings, семантика close и детали циклов review/fix относятся к managed Subagent MCP и product surface; они не являются полями переносимого Agent JSON. Builder 2.0 не должен изобретать приватную builder-only semantics, обходить validation, напрямую менять live revisions или заявлять, что drafted agent доказан как integrated product flow.

## Поддерживаемая граница authoring

Builder 2.0 может создавать или пересматривать:

- переносимые `memory_bindings`, которые указывают на документированные требования provider и capability constraints;
- runtime-source и runtime-option intent, валидный по публичным runtime-source и runtime-adapter contracts;
- interaction settings, которые опираются на документированные comments, prompt/reply, wait-state и resume behavior;
- переносимые graph patterns `orchestrator_agent`, которые остаются валидными по Agent JSON contract, при этом task-package и review-loop детали managed subagent маршрутизируются в owner docs Subagent MCP вместо встраивания их как переносимых полей;
- validation-aware revisions, которые отвечают на schema, lifecycle или owner-doc feedback созданием нового draft candidate.

Builder 2.0 должен относиться к этому как к ссылкам на публичные контракты. Builder не становится владельцем provider registry, model discovery, user-chat state machine, managed-subagent service или lifecycle deploy rules.

## Draft-first дисциплина lifecycle

Весь вывод Builder 2.0 остается draft-first:

1. builder возвращает candidate portable Agent JSON;
2. Core разбирает и валидирует candidate по поддержанным schemas и invariants;
3. правила идентичности create и revise проверяются до persistence;
4. принятый output сохраняется как draft revision;
5. deploy остается отдельным явным lifecycle action;
6. live runs продолжают использовать текущую live revision, пока не произойдет deploy.

Builder 2.0 может рекомендовать validations, dry runs, review passes или deployment. Он не должен молча выполнять deploy, помечать draft как live или считать собственную уверенность доказательством завершения lifecycle.

## Правило public-contract-only

Поведение, созданное builder, должно выражаться через документированные контракты, которые пользователь может inspect, diff, edit, validate и run без скрытого builder state.

Запрещенные shortcuts Builder 2.0 включают:

- приватные поля, понятные только builder;
- скрытую память draft intent, которая меняет execution после persistence;
- runtime-specific instructions, обходящие runtime adapter boundary;
- provider-specific assumptions, обходящие memory capability negotiation;
- subagent orchestration instructions, игнорирующие write-scope, budget или close semantics;
- interaction behavior, зависящее от UI-only side channel вне документированного interaction contract.

## Граница authoring для memory

Builder 2.0 может author memory bindings только как переносимую configuration и design intent.

Он может:

- включать bindings, которые ссылаются на документированные требования memory provider;
- объяснять provider capability prerequisites, которые должны быть выполнены вне agent file;
- избегать memory fields, когда запрошенный design может работать без memory;
- пересматривать invalid bindings в ответ на validation или provider-capability feedback.

Он не должен:

- регистрировать providers через редактирование agent file;
- заявлять, что memory provider live только потому, что binding существует;
- помещать secrets, credentials или local provider setup в переносимый Agent JSON;
- предполагать runtime-native memory consumption там, где current runtime path этого не реализовал.

## Граница authoring для runtime

Builder 2.0 может author runtime-source и runtime-option intent, валидный по публичным runtime contracts.

Он может описывать preferred models, reasoning effort, speed tier или source selection только когда эти concepts представлены поддержанным contract и gated через runtime capability metadata.

Он не должен:

- считать local runtime metadata переносимой file truth;
- обходить source-selection или capability checks;
- предполагать, что каждый runtime поддерживает App Server-native options;
- делать builder-specific runtime behavior частью Core semantics.

## Граница authoring для interaction

Builder 2.0 может draft agents, использующие документированную user-interaction model.

Он может author instructions, которые ожидают comments, prompt/reply, wait-state и resume behavior там, где эти surfaces поддержаны. Он должен оставлять risky mid-run changes, blocked prompts и reply handling внутри owner interaction contracts.

Он не должен заявлять, что один builder-authored draft доказывает полное cross-interface interaction behavior. Более широкое user-visible flow proof относится к Phase 18 и Phase 19.

## Граница authoring для managed subagent

Builder 2.0 может помогать пользователям author managed-subagent patterns, но должен потреблять managed layer Phase 16, а не переопределять его.

Portable Agent JSON может описывать только переносимый nested-graph primitive `orchestrator_agent`. Более богатая managed-subagent surface является отдельной product/MCP boundary: task packages, worker/reviewer roles, write scopes, budgets, findings, close semantics, cancellation и enforced review/fix loops являются concepts managed Subagent MCP и не должны сериализоваться в Agent JSON, если будущий owner contract явно не добавит переносимые поля.

Он может предлагать:

- роли worker, reviewer и final-review там, где публичный managed-subagent contract их поддерживает;
- task packages с ясными goals, read/write boundaries, budgets и acceptance criteria;
- review-and-repair structures, сохраняющие явную close semantics;
- partitions write-scope, которые избегают sibling conflicts.

Он не должен:

- запускать managed subagents как скрытый side effect draft creation, если текущий публичный builder flow явно не поддерживает такой execution step;
- назначать overlapping write scopes;
- игнорировать budget или cancellation state;
- выводить child interaction через parent user boundary, если interaction owner docs не определяют такое behavior.

## Validation и self-review

Builder 2.0 должен стать более validation-aware, но validation остается внешним gate во владении Core и соответствующих contracts.

Flow Builder 2.0 может использовать schema errors, owner-doc constraints, lifecycle identity checks и reviewer findings как вход для revised candidate. Durable result все еще является новым candidate draft, а не unverifiable internal repair.

## Чего Phase 17 не заявляет

Phase 17 не заявляет:

- integrated product flows, которые end to end объединяют builder, lifecycle, runtime, interaction, memory и managed subagents;
- successful execution каждого builder-authored agent;
- real-provider или real-runtime proof сверх evidence, записанного в subsystem owner docs;
- broad external live proof для managed subagent workflows;
- builder-only authority над registry, deployment, provider setup, runtime selection или user-chat state.

Эти claims относятся к более поздним integrated product-flow и release-proof phases, когда они будут подкреплены executable evidence.
