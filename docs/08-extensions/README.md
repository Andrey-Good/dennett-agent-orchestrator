[English](#english) | [Русский](#russian)

# English

## Extensions

Status: section index only. The normative rules in this section live in the leaf documents.

Related documents:

- [Canonical spec](../../agent_orchestrator_final_spec_v2.md)
- [Foundations](../01-foundations/README.md)
- [Runtime Integration Model](../02-architecture/runtime-integration-model.md)
- [Agent JSON contract](../03-contracts/agent-json/README.md)
- [Lifecycle](../07-lifecycle/README.md)
- [ADRs](../09-adrs/README.md)

This section owns optional model extensions that may be present in agent definitions or Core workflows without redefining the stable base architecture. An extension may add capabilities, but it must not silently rewrite the meaning of the core contract, the registry boundary, or the state model.

Normative documents in this section:

- [Builder Agent](./builder-agent.md): the Phase 10 system-agent slice that uses the existing runtime path to produce candidate agent JSON, validates contract and identity rules before persistence, stores accepted results as drafts by default, and leaves deploy to a separate explicit later action.
- [Memory Bindings](./memory-bindings.md): how Dennett models one vendor-neutral internal memory layer, how provider adapters hang off it, how portable bindings stay provider-neutral unless an explicit escape hatch is used, and how the current executable boundary includes only the narrow Stage 2 prompt-rendered Codex memory-context path rather than native or broad runtime memory.
- [Runtime Sources](./runtime-sources.md): how agent files can constrain concrete execution sources, accounts, sessions, limit-aware launch choices, and local model or account discovery around configured sources without turning that metadata into portable file truth.

For Phase 10, the builder document defines the first executable builder slice: a real system-agent resource invoked through the existing runtime path, checked for both portable-contract validity and create-versus-revise identity correctness, stored as drafts by default, and never deployed by builder execution itself.

Section boundary:

- The base lifecycle of drafts, live revisions, deploys, events, and registry truth boundaries remains in [Lifecycle](../07-lifecycle/README.md).
- The portable file contract remains in [Agent JSON contract](../03-contracts/agent-json/README.md).
- The portable memory-binding shape and provider escape hatch remain in [Memory Binding Model Contract](../03-contracts/agent-json/memory-binding-model-contract.md).
- The implemented Phase 13 Mem0-first slice is recorded in [Native Memory Integration](../14-native-memory-integration/README.md).
- The narrow Stage 2 runtime-memory slice is implemented only as registered-provider resolution, provider-neutral `memory_context`, Codex prompt rendering, and success-only provider writes; native App Server memory and broader runtime-memory behavior remain deferred.
- Runtime adapter boundaries remain in [Runtime Integration Model](../02-architecture/runtime-integration-model.md).
- Historical rationale stays in [ADRs](../09-adrs/README.md).

The base system stays coherent even when none of these extensions are used. That is the main architectural test for whether a rule belongs here.

# Russian

## Расширения

Статус: только индекс раздела. Нормативные правила этого раздела живут в профильных документах.

Связанные документы:

- [Каноническая спецификация](../../agent_orchestrator_final_spec_v2.md)
- [Foundations](../01-foundations/README.md)
- [Модель интеграции runtime](../02-architecture/runtime-integration-model.md)
- [Контракт Agent JSON](../03-contracts/agent-json/README.md)
- [Жизненный цикл](../07-lifecycle/README.md)
- [ADR](../09-adrs/README.md)

Этот раздел владеет опциональными расширениями модели, которые могут присутствовать в agent definitions или workflow Core, не переписывая при этом стабильную базовую архитектуру. Расширение может добавлять возможности, но не должно молча менять смысл core-contract, границы реестра или модели состояния.

Нормативные документы раздела:

- [Builder Agent](./builder-agent.md): системный агент, который в срезе Phase 10 вызывается как реальный system-agent resource через существующий runtime path, выдает кандидатный Agent JSON, проверяет контракт и правила идентичности до записи, сохраняет принятый результат как draft по умолчанию и оставляет deploy отдельным явным более поздним действием.
- [Memory Bindings](./memory-bindings.md): как Dennett моделирует единый vendor-neutral internal memory layer, как provider adapters подключаются к нему, как portable bindings остаются provider-neutral без явного escape hatch, и что текущая executable boundary включает только узкий Stage 2 prompt-rendered Codex memory-context path, а не native или broad runtime memory.
- [Runtime Sources](./runtime-sources.md): как agent files могут ограничивать конкретные execution sources, аккаунты, сессии, выбор запуска с учетом лимитов и локальное discovery моделей или аккаунтов вокруг настроенных sources без превращения этой metadata в переносимую file truth.

Для Phase 10 документ о builder задает первый исполнимый builder-срез: реальный системный ресурс агента, который использует существующий runtime path, проверяется и на portable contract, и на корректность идентичности для create/revise, по умолчанию сохраняется как draft и не выполняет deploy сам по себе.

Граница раздела:

- Базовый жизненный цикл drafts, live-ревизий, deploy, events и границ истины реестра остается в [Lifecycle](../07-lifecycle/README.md).
- Переносимый файловый контракт остается в [контракте Agent JSON](../03-contracts/agent-json/README.md).
- Portable memory-binding shape и provider escape hatch остаются в [Memory Binding Model Contract](../03-contracts/agent-json/memory-binding-model-contract.md).
- Узкий Stage 2 runtime-memory slice реализован только как registered-provider resolution, provider-neutral `memory_context`, Codex prompt rendering и success-only provider writes; native App Server memory и broader runtime-memory behavior остаются deferred.
- Границы runtime adapter остаются в [модели интеграции runtime](../02-architecture/runtime-integration-model.md).
- Историческая мотивация остается в [ADR](../09-adrs/README.md).

Базовая система должна оставаться согласованной даже тогда, когда ни одно из этих расширений не используется. Это главный архитектурный тест того, действительно ли правило относится к этому разделу.
