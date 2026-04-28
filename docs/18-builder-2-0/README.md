[English](#english) | [Russian](#russian)

<a id="english"></a>
# Builder 2.0

Status: navigation and ownership boundary for Phase 17 Builder 2.0 documentation.

Related documents:

- [Builder Agent](../08-extensions/builder-agent.md)
- [Phase 17 Builder 2.0](./phase-17-builder-2-0.md)
- [Builder 2.0 Productization](../21-public-launch-readiness/builder-2-0-productization.md)
- [Phase 12 Capability Gap Lock](../13-capability-gap-lock/phase-12-capability-gap-lock.md)
- [Runtime Integration Model](../02-architecture/runtime-integration-model.md)
- [Memory Bindings](../08-extensions/memory-bindings.md)
- [Phase 16 Managed Subagent Orchestration](../17-managed-subagent-orchestration/phase-16-managed-subagent-orchestration.md)

## Scope

This section owns the Phase 17 Builder 2.0 authoring upgrade: how the builder may draft and revise portable agent definitions that reference the richer public surfaces delivered by earlier phases.

The current implemented Stage 9 boundary is audited draft-first authoring. Builder output must use the formal wrapper `{"agent_file": <portable-agent-json>}` documented by `contracts/json-schema/builder-output.schema.json`; Core validates the embedded agent file, runs deterministic candidate audit, and persists accepted output only as a draft revision. Candidate diagnostics are host output, not portable Agent JSON.

Portable agent definitions may contain only contract-supported portable fields, including the portable `orchestrator_agent` nested-graph primitive. Managed-subagent task packages, roles, write scopes, budgets, findings, close semantics, and review/fix loop details remain owned by the managed Subagent MCP/product surface and are not portable agent JSON fields.

It does not own lifecycle semantics, runtime adapter behavior, memory-provider behavior, user-interaction state, or managed subagent execution. Those remain owned by their subsystem documents.

## Reading Order

1. Read [Builder Agent](../08-extensions/builder-agent.md) for the enduring builder extension contract.
2. Read [Phase 17 Builder 2.0](./phase-17-builder-2-0.md) for the current upgrade boundary and non-goals.
3. Read [Builder 2.0 Productization](../21-public-launch-readiness/builder-2-0-productization.md) before making public-readiness claims.
4. Return to the subsystem owner docs before changing memory, runtime, interaction, lifecycle, or managed subagent behavior.

<a id="russian"></a>
# Builder 2.0

Статус: навигация и граница владения документацией Phase 17 Builder 2.0.

Связанные документы:

- [Builder Agent](../08-extensions/builder-agent.md)
- [Phase 17 Builder 2.0](./phase-17-builder-2-0.md)
- [Builder 2.0 Productization](../21-public-launch-readiness/builder-2-0-productization.md)
- [Phase 12 Capability Gap Lock](../13-capability-gap-lock/phase-12-capability-gap-lock.md)
- [Runtime Integration Model](../02-architecture/runtime-integration-model.md)
- [Memory Bindings](../08-extensions/memory-bindings.md)
- [Phase 16 Managed Subagent Orchestration](../17-managed-subagent-orchestration/phase-16-managed-subagent-orchestration.md)

## Область

Этот раздел отвечает за авторское обновление Phase 17 Builder 2.0: как builder может создавать и пересматривать переносимые определения агентов, которые ссылаются на более богатые публичные поверхности, поставленные предыдущими фазами.

Переносимые определения агентов могут содержать только поддержанные контрактом переносимые поля, включая переносимый примитив вложенного графа `orchestrator_agent`. Пакеты задач managed subagent, роли, области записи, бюджеты, findings, семантика close и детали циклов review/fix остаются во владении managed Subagent MCP/product surface и не являются полями переносимого Agent JSON.

Этот раздел не владеет семантикой lifecycle, поведением runtime adapter, поведением memory provider, состоянием user interaction или исполнением managed subagent. Эти темы остаются во владении документов соответствующих подсистем.

## Порядок чтения

1. Прочитайте [Builder Agent](../08-extensions/builder-agent.md) как постоянный контракт расширения builder.
2. Прочитайте [Phase 17 Builder 2.0](./phase-17-builder-2-0.md) как текущую границу обновления и non-goals.
3. Вернитесь к owner docs подсистем перед изменением memory, runtime, interaction, lifecycle или managed subagent behavior.
