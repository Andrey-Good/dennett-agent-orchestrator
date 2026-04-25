[English](#english) | [Русский](#russian)

<a id="english"></a>
# English

This section owns user-visible interaction semantics while a run is active. It explains how the user, the interface, the orchestrator, and the built-in user communication channel behave at runtime.

## Document map

- [Live Run Interaction](./live-run-interaction.md) defines the behavioral rules for comments, the built-in user-chat MCP channel, routing of user input while a run is active, and the staging boundary for richer Codex-specific progress or diagnostic notifications.
- [Presentation Rules](./presentation-rules.md) defines how CLI and future interfaces must present intermediate messages, pending prompts, and the final answer.

## Ownership boundary

This section owns:

- live interaction behavior during an active run;
- routing rules for user input while a run is in progress;
- user-facing staging rules for richer runtime progress or diagnostic notifications during an active run;
- interface-facing presentation requirements for intermediate and final messages.

This section does not own:

- field-by-field `interaction` configuration schemas;
- wire payloads for the built-in user-chat MCP channel;
- execution internals, persistence internals, or resume storage details.

Use the contracts area for formal definitions:

- [Contracts](../03-contracts/README.md) is the owner area for formal contracts.
- [Interaction And Chat Contract](../03-contracts/agent-json/interaction-and-chat-contract.md) owns the top-level `interaction` configuration fields.
- [Built-in MCP Contract: `orchestrator.user_chat`](../03-contracts/orchestrator-user-chat-mcp-contract.md) owns the built-in user-chat payload contract.

## Neighboring sources

- [Architecture](../02-architecture/README.md) defines the boundary between core, interfaces, and runtime adapters.
- [Execution](../04-execution/README.md) owns how runs advance through the graph.
- [State](../05-state/README.md) owns chat persistence, resume state, and storage rules.

This README is an index and boundary declaration. The normative interaction rules live in the two leaf documents above.

<a id="russian"></a>
# Русский

Этот раздел владеет пользовательской семантикой взаимодействия во время активного run. Здесь описывается, как пользователь, интерфейс, оркестратор и встроенный канал общения с пользователем ведут себя во время исполнения.

## Карта документов

- [Live Run Interaction](./live-run-interaction.md) задаёт поведенческие правила для комментариев, встроенного user-chat MCP, маршрутизации пользовательского ввода во время активного run и этапной границы для более богатых Codex-specific progress или diagnostic notifications.
- [Presentation Rules](./presentation-rules.md) задаёт, как CLI и будущие интерфейсы должны показывать промежуточные сообщения, ожидающие вопросы и финальный ответ.

## Граница владения

Этот раздел владеет:

- поведением live-взаимодействия во время активного run;
- правилами маршрутизации пользовательского ввода, пока run не завершён;
- пользовательскими правилами этапного вывода более богатых runtime progress или diagnostic notifications во время активного run;
- требованиями к отображению промежуточных и финальных сообщений на стороне интерфейса.

Этот раздел не владеет:

- схемами конфигурации `interaction` по полям;
- wire-payload контрактами встроенного user-chat MCP;
- внутренностями исполнения, хранилища и resume.

Для формальных определений используйте раздел contracts:

- [Contracts](../03-contracts/README.md) остаётся зоной-владельцем формальных контрактов.
- [Interaction And Chat Contract](../03-contracts/agent-json/interaction-and-chat-contract.md) владеет top-level полями конфигурации `interaction`.
- [Built-in MCP Contract: `orchestrator.user_chat`](../03-contracts/orchestrator-user-chat-mcp-contract.md) владеет payload-контрактом встроенного user-chat.

## Смежные источники

- [Architecture](../02-architecture/README.md) задаёт границу между core, интерфейсами и runtime adapters.
- [Execution](../04-execution/README.md) владеет тем, как run движется по графу.
- [State](../05-state/README.md) владеет хранением чатов, resume state и правилами записи.

Этот README является картой раздела и декларацией границ. Нормативные правила взаимодействия живут в двух профильных документах выше.
