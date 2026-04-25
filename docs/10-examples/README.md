[English](#english) | [Русский](#russian)

<a id="english"></a>
# Examples

Status: non-normative illustrative section.
Owns: nothing. If any example in this directory disagrees with an owner document, the owner document wins.

Primary owner areas:

- [Foundations](../01-foundations/README.md)
- [Architecture](../02-architecture/README.md)
- [Contracts](../03-contracts/README.md)
- [Execution](../04-execution/README.md)
- [State](../05-state/README.md)
- [Interaction](../06-interaction/README.md)
- [Lifecycle](../07-lifecycle/README.md)
- [Extensions](../08-extensions/README.md)
- [ADRs](../09-adrs/README.md)

## How To Read This Section

- Treat these files as worked examples, not as a second contract.
- Use the links in each example to jump back to the normative owner.
- Read invalid samples as anti-patterns on purpose; they are here to show what must be rejected or avoided.
- Keep the boundary clear: user-facing examples live here, while machine-oriented negative fixtures belong outside this section.
- Use the ADR links here only for rationale. Current rules still live in the normative docs.

## Document Map

- [canonical-agent-json-example.md](./canonical-agent-json-example.md): one end-to-end agent file that stays inside the portable contract and points back to the owning sections.
- [valid-patterns.md](./valid-patterns.md): short patterns that align with the current contracts, execution model, state rules, lifecycle rules, and extensions.
- [invalid-patterns.md](./invalid-patterns.md): short anti-patterns and violations with links to the rules they break.
- [interaction-sequences.md](./interaction-sequences.md): step-by-step flows for comments, built-in user chat, explicit resume, and revision binding.

## Coverage Map

| Area | What still owns the rules | Where this section illustrates it |
| --- | --- | --- |
| Foundations | [glossary.md](../01-foundations/glossary.md), [invariants-and-defaults.md](../01-foundations/invariants-and-defaults.md), [source-of-truth-model.md](../01-foundations/source-of-truth-model.md) | [canonical-agent-json-example.md](./canonical-agent-json-example.md), [valid-patterns.md](./valid-patterns.md) |
| Architecture | [core-and-interfaces.md](../02-architecture/core-and-interfaces.md), [runtime-integration-model.md](../02-architecture/runtime-integration-model.md) | [canonical-agent-json-example.md](./canonical-agent-json-example.md), [interaction-sequences.md](./interaction-sequences.md) |
| Contracts | [agent-json/README.md](../03-contracts/agent-json/README.md), [runtime-adapter-contract.md](../03-contracts/runtime-adapter-contract.md), [orchestrator-user-chat-mcp-contract.md](../03-contracts/orchestrator-user-chat-mcp-contract.md) | all files in this section |
| Execution | [graph-execution.md](../04-execution/graph-execution.md), [dataflow-and-input-resolution.md](../04-execution/dataflow-and-input-resolution.md), [outputs-outcomes-and-final-response.md](../04-execution/outputs-outcomes-and-final-response.md) | [canonical-agent-json-example.md](./canonical-agent-json-example.md), [valid-patterns.md](./valid-patterns.md), [interaction-sequences.md](./interaction-sequences.md) |
| State | [chat-and-resume.md](../05-state/chat-and-resume.md), [local-storage-model.md](../05-state/local-storage-model.md), [secret-markers.md](../05-state/secret-markers.md) | [canonical-agent-json-example.md](./canonical-agent-json-example.md), [invalid-patterns.md](./invalid-patterns.md), [interaction-sequences.md](./interaction-sequences.md) |
| Interaction | [live-run-interaction.md](../06-interaction/live-run-interaction.md), [presentation-rules.md](../06-interaction/presentation-rules.md) | [valid-patterns.md](./valid-patterns.md), [invalid-patterns.md](./invalid-patterns.md), [interaction-sequences.md](./interaction-sequences.md) |
| Lifecycle | [agent-registry.md](../07-lifecycle/agent-registry.md), [draft-live-deploy.md](../07-lifecycle/draft-live-deploy.md), [events-and-triggers.md](../07-lifecycle/events-and-triggers.md), [versioning-axes.md](../07-lifecycle/versioning-axes.md) | [valid-patterns.md](./valid-patterns.md), [invalid-patterns.md](./invalid-patterns.md), [interaction-sequences.md](./interaction-sequences.md) |
| Extensions | [memory-bindings.md](../08-extensions/memory-bindings.md), [runtime-sources.md](../08-extensions/runtime-sources.md), [builder-agent.md](../08-extensions/builder-agent.md) | [canonical-agent-json-example.md](./canonical-agent-json-example.md), [valid-patterns.md](./valid-patterns.md) |
| ADRs | [README.md](../09-adrs/README.md), [ADR-0001-codex-first-not-codex-only.md](../09-adrs/ADR-0001-codex-first-not-codex-only.md), [ADR-0002-agent-file-vs-local-state.md](../09-adrs/ADR-0002-agent-file-vs-local-state.md), [ADR-0003-chat-resume-is-not-memory.md](../09-adrs/ADR-0003-chat-resume-is-not-memory.md) | brief rationale notes only in all files |

## Reading Order

1. Start with [canonical-agent-json-example.md](./canonical-agent-json-example.md) to see one contract-shaped file in context.
2. Read [valid-patterns.md](./valid-patterns.md) and [invalid-patterns.md](./invalid-patterns.md) together so the boundary between approved and rejected patterns stays sharp.
3. Read [interaction-sequences.md](./interaction-sequences.md) when the question is about behavior over time rather than field shape.

<a id="russian"></a>
# Примеры

Статус: ненормативный иллюстративный раздел.
Владение: ничем. Если какой-либо пример в этой директории расходится с профильным документом-владельцем, побеждает профильный документ.

Основные зоны-владельцы:

- [Foundations](../01-foundations/README.md)
- [Architecture](../02-architecture/README.md)
- [Contracts](../03-contracts/README.md)
- [Execution](../04-execution/README.md)
- [State](../05-state/README.md)
- [Interaction](../06-interaction/README.md)
- [Lifecycle](../07-lifecycle/README.md)
- [Extensions](../08-extensions/README.md)
- [ADRs](../09-adrs/README.md)

## Как Читать Этот Раздел

- Считайте эти файлы рабочими примерами, а не вторым контрактом.
- По ссылкам внутри каждого примера переходите к нормативному владельцу правила.
- Невалидные образцы здесь намеренно остаются анти-примерами; они нужны, чтобы показать, что должно отклоняться или избегаться.
- Граница должна оставаться ясной: пользовательские примеры живут здесь, а машинно-ориентированные негативные fixtures должны жить вне этого раздела.
- Ссылки на ADR здесь нужны только для мотивации. Актуальные правила по-прежнему живут в нормативных документах.

## Карта Документов

- [canonical-agent-json-example.md](./canonical-agent-json-example.md): один сквозной agent file, который остается внутри переносимого контракта и ссылается на профильные разделы.
- [valid-patterns.md](./valid-patterns.md): короткие паттерны, согласованные с текущими контрактами, моделью исполнения, правилами состояния, жизненного цикла и расширений.
- [invalid-patterns.md](./invalid-patterns.md): короткие анти-паттерны и нарушения с ссылками на правила, которые они ломают.
- [interaction-sequences.md](./interaction-sequences.md): пошаговые сценарии для комментариев, встроенного user chat, explicit resume и привязки к ревизии.

## Карта Покрытия

| Область | Кто по-прежнему владеет правилами | Где этот раздел их иллюстрирует |
| --- | --- | --- |
| Foundations | [glossary.md](../01-foundations/glossary.md), [invariants-and-defaults.md](../01-foundations/invariants-and-defaults.md), [source-of-truth-model.md](../01-foundations/source-of-truth-model.md) | [canonical-agent-json-example.md](./canonical-agent-json-example.md), [valid-patterns.md](./valid-patterns.md) |
| Architecture | [core-and-interfaces.md](../02-architecture/core-and-interfaces.md), [runtime-integration-model.md](../02-architecture/runtime-integration-model.md) | [canonical-agent-json-example.md](./canonical-agent-json-example.md), [interaction-sequences.md](./interaction-sequences.md) |
| Contracts | [agent-json/README.md](../03-contracts/agent-json/README.md), [runtime-adapter-contract.md](../03-contracts/runtime-adapter-contract.md), [orchestrator-user-chat-mcp-contract.md](../03-contracts/orchestrator-user-chat-mcp-contract.md) | все файлы этого раздела |
| Execution | [graph-execution.md](../04-execution/graph-execution.md), [dataflow-and-input-resolution.md](../04-execution/dataflow-and-input-resolution.md), [outputs-outcomes-and-final-response.md](../04-execution/outputs-outcomes-and-final-response.md) | [canonical-agent-json-example.md](./canonical-agent-json-example.md), [valid-patterns.md](./valid-patterns.md), [interaction-sequences.md](./interaction-sequences.md) |
| State | [chat-and-resume.md](../05-state/chat-and-resume.md), [local-storage-model.md](../05-state/local-storage-model.md), [secret-markers.md](../05-state/secret-markers.md) | [canonical-agent-json-example.md](./canonical-agent-json-example.md), [invalid-patterns.md](./invalid-patterns.md), [interaction-sequences.md](./interaction-sequences.md) |
| Interaction | [live-run-interaction.md](../06-interaction/live-run-interaction.md), [presentation-rules.md](../06-interaction/presentation-rules.md) | [valid-patterns.md](./valid-patterns.md), [invalid-patterns.md](./invalid-patterns.md), [interaction-sequences.md](./interaction-sequences.md) |
| Lifecycle | [agent-registry.md](../07-lifecycle/agent-registry.md), [draft-live-deploy.md](../07-lifecycle/draft-live-deploy.md), [events-and-triggers.md](../07-lifecycle/events-and-triggers.md), [versioning-axes.md](../07-lifecycle/versioning-axes.md) | [valid-patterns.md](./valid-patterns.md), [invalid-patterns.md](./invalid-patterns.md), [interaction-sequences.md](./interaction-sequences.md) |
| Extensions | [memory-bindings.md](../08-extensions/memory-bindings.md), [runtime-sources.md](../08-extensions/runtime-sources.md), [builder-agent.md](../08-extensions/builder-agent.md) | [canonical-agent-json-example.md](./canonical-agent-json-example.md), [valid-patterns.md](./valid-patterns.md) |
| ADRs | [README.md](../09-adrs/README.md), [ADR-0001-codex-first-not-codex-only.md](../09-adrs/ADR-0001-codex-first-not-codex-only.md), [ADR-0002-agent-file-vs-local-state.md](../09-adrs/ADR-0002-agent-file-vs-local-state.md), [ADR-0003-chat-resume-is-not-memory.md](../09-adrs/ADR-0003-chat-resume-is-not-memory.md) | только краткие ссылки на мотивацию во всех файлах |

## Порядок Чтения

1. Начните с [canonical-agent-json-example.md](./canonical-agent-json-example.md), чтобы увидеть один contract-shaped файл целиком.
2. Затем читайте [valid-patterns.md](./valid-patterns.md) и [invalid-patterns.md](./invalid-patterns.md) вместе, чтобы граница между допустимыми и отвергаемыми паттернами оставалась четкой.
3. Переходите к [interaction-sequences.md](./interaction-sequences.md), когда вопрос касается поведения во времени, а не формы полей.
