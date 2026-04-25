[English](#english) | [Русский](#russian)

<a id="english"></a>
# Agent JSON Contract

This subsection splits the canonical portable agent file into non-overlapping ownership areas. The split is strict: a field or invariant owned by one leaf document must be referenced from the others, not duplicated.

## Ownership Map

- [top-level-and-bindings-contract.md](./top-level-and-bindings-contract.md) owns the root object and every top-level field except `interaction`, `chat`, `nodes`, and `edges`.
- [nodes-and-edges-contract.md](./nodes-and-edges-contract.md) owns `nodes`, `input`, node-level binding references, node output interpretation, and `edges`.
- [interaction-and-chat-contract.md](./interaction-and-chat-contract.md) owns `interaction`, `chat`, and the routing relationship between run-time comments and the built-in user-chat MCP.
- [memory-binding-model-contract.md](./memory-binding-model-contract.md) owns the portable memory-binding payload, capability negotiation, provider escape hatch, and local provider-registration boundary.

## Shared Invariants

- The agent file is the source of truth for agent definition and remains a portable JSON artifact.
- `graph_contract_version` is mandatory and independent from `meta.agent_version` and product version.
- Top-level unknown fields are invalid in the current contract version unless another contract document explicitly owns them.
- IDs are local contract keys, not database primary keys.
- Triggers, event subscriptions, chat storage internals, and registry metadata are outside the portable agent file contract.
- Memory-provider registration, provider-specific config, and MCP transport choice are outside the portable agent file contract unless they are explicitly surfaced through the memory-binding model contract.

## Neighboring Documents

- General contract ownership rules live in [../README.md](../README.md).
- Runtime adapter behavior that consumes this file lives in [../runtime-adapter-contract.md](../runtime-adapter-contract.md).
- The built-in MCP payload contract referenced from `interaction.user_mcp` lives in [../orchestrator-user-chat-mcp-contract.md](../orchestrator-user-chat-mcp-contract.md).
- Portable memory binding shape and provider-specific escape hatches live in [./memory-binding-model-contract.md](./memory-binding-model-contract.md).
- Execution semantics that are not required to interpret the file contract live in [../../04-execution/README.md](../../04-execution/README.md).
- Draft/live/deploy lifecycle rules live in [../../07-lifecycle/README.md](../../07-lifecycle/README.md).

<a id="russian"></a>
# Контракт Agent JSON

Этот подраздел разбивает канонический переносимый agent file на непересекающиеся зоны владения. Разделение строгое: поле или инвариант, принадлежащий одному leaf-документу, должен только упоминаться в остальных документах, а не дублироваться.

## Карта Владения

- [top-level-and-bindings-contract.md](./top-level-and-bindings-contract.md) владеет корневым объектом и всеми top-level полями, кроме `interaction`, `chat`, `nodes` и `edges`.
- [nodes-and-edges-contract.md](./nodes-and-edges-contract.md) владеет `nodes`, `input`, node-level binding references, интерпретацией node output и `edges`.
- [interaction-and-chat-contract.md](./interaction-and-chat-contract.md) владеет `interaction`, `chat` и правилом маршрутизации между комментариями во время run и встроенным user-chat MCP.

## Общие Инварианты

- Agent file является источником истины для определения агента и остается переносимым JSON-артефактом.
- `graph_contract_version` обязателен и независим от `meta.agent_version` и версии продукта.
- Неописанные top-level поля недопустимы в текущей версии контракта, если ими явно не владеет другой контрактный документ.
- Идентификаторы являются локальными контрактными ключами, а не первичными ключами БД.
- Триггеры, подписки на события, внутренности хранения чатов и метаданные реестра находятся вне контракта переносимого agent file.

## Соседние Документы

- Общие правила владения контрактами находятся в [../README.md](../README.md).
- Поведение runtime adapter, которое использует этот файл, находится в [../runtime-adapter-contract.md](../runtime-adapter-contract.md).
- Контракт payload встроенного MCP, на который ссылается `interaction.user_mcp`, находится в [../orchestrator-user-chat-mcp-contract.md](../orchestrator-user-chat-mcp-contract.md).
- Семантика исполнения, не необходимая для интерпретации файлового контракта, находится в [../../04-execution/README.md](../../04-execution/README.md).
- Правила жизненного цикла draft/live/deploy находятся в [../../07-lifecycle/README.md](../../07-lifecycle/README.md).
