[English](#english) | [Russian](#russian)

<a id="english"></a>
# Integrated Product Flows

Status: owner section for Phase 18 integrated product-flow documentation.

Phase 18 owns the integration layer that proves major subsystem surfaces can be composed into coherent product flows. It does not replace subsystem owner documents and does not claim real-world release readiness.

Documents:

- [Phase 18 Integrated Product Flows](./phase-18-integrated-product-flows.md)
- [Cross-Subsystem Conflict Rules](./conflict-rules.md)
- [Acceptance Scenarios](./acceptance-scenarios.md)

Primary subsystem inputs:

- [Phase 13 Native Memory Integration](../14-native-memory-integration/phase-13-mem0-first-native-memory-integration.md)
- [Phase 14 Native Runtime Surface Completion](../15-native-runtime-surface/phase-14-native-runtime-surface-completion.md)
- [Phase 15 Full User Interaction Layer](../16-full-user-interaction-layer/phase-15-full-user-interaction-layer.md)
- [Phase 16 Managed Subagent Orchestration](../17-managed-subagent-orchestration/phase-16-managed-subagent-orchestration.md)
- [Phase 17 Builder 2.0](../18-builder-2-0/phase-17-builder-2-0.md)
- [Draft, Live, Deploy](../07-lifecycle/draft-live-deploy.md)

## Scope

This section defines:

- integrated flow goals across builder, lifecycle, runtime, interaction, memory, and managed subagents;
- conflict rules for handoffs and precedence when those subsystems meet;
- acceptance scenarios that can be validated by local executable evidence, offline test doubles, or documented dry-run evidence;
- the boundary between Phase 18 integration confidence and Phase 19 external proof.

This section does not define new portable agent fields, provider behavior, runtime behavior, lifecycle state transitions, user-chat semantics, or managed-subagent primitives. Those remain owned by the relevant subsystem documents.

<a id="russian"></a>
# Интегрированные продуктовые потоки

Статус: раздел-владелец документации Phase 18 integrated product-flow.

Phase 18 отвечает за интеграционный слой, который доказывает, что основные поверхности подсистем можно объединять в согласованные продуктовые потоки. Он не заменяет документы-владельцы подсистем и не заявляет готовность к реальному выпуску.

Документы:

- [Phase 18 Integrated Product Flows](./phase-18-integrated-product-flows.md)
- [Cross-Subsystem Conflict Rules](./conflict-rules.md)
- [Acceptance Scenarios](./acceptance-scenarios.md)

Основные входные документы подсистем:

- [Phase 13 Native Memory Integration](../14-native-memory-integration/phase-13-mem0-first-native-memory-integration.md)
- [Phase 14 Native Runtime Surface Completion](../15-native-runtime-surface/phase-14-native-runtime-surface-completion.md)
- [Phase 15 Full User Interaction Layer](../16-full-user-interaction-layer/phase-15-full-user-interaction-layer.md)
- [Phase 16 Managed Subagent Orchestration](../17-managed-subagent-orchestration/phase-16-managed-subagent-orchestration.md)
- [Phase 17 Builder 2.0](../18-builder-2-0/phase-17-builder-2-0.md)
- [Draft, Live, Deploy](../07-lifecycle/draft-live-deploy.md)

## Область

Этот раздел определяет:

- цели интегрированных потоков между builder, lifecycle, runtime, interaction, memory и managed subagents;
- правила конфликтов для передач управления и приоритетов там, где эти подсистемы пересекаются;
- приемочные сценарии, которые можно подтвердить локальными исполнимыми доказательствами, офлайн test doubles или документированными dry-run доказательствами;
- границу между интеграционной уверенностью Phase 18 и внешним доказательством Phase 19.

Этот раздел не определяет новые переносимые поля agent, поведение providers, поведение runtime, переходы состояния lifecycle, семантику user-chat или primitives managed-subagent. Эти темы остаются во владении соответствующих документов подсистем.
