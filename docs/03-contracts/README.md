[English](#english) | [Русский](#russian)

<a id="english"></a>
# Contracts

This section is the normative home of file contracts and wire contracts. Documents here must be stable enough to drive JSON Schema, adapter interfaces, validation logic, and negative tests without re-inventing field meaning in code.

## Section Rules

- One rule has one owner document. Neighboring documents may reference a rule, but they must not redefine it.
- Documents in this section define contract shape, required fields, allowed values, defaults, invariants, and prohibitions.
- Architecture rationale stays in [02-architecture](../02-architecture/README.md).
- Execution flow beyond the minimum needed to interpret a contract stays in [04-execution](../04-execution/README.md).
- Persistence internals and storage layout stay in [05-state](../05-state/README.md).
- UI behavior and end-user interaction policy stay in [06-interaction](../06-interaction/README.md).
- Examples are non-normative and belong in [10-examples](../10-examples/README.md).

## Document Ownership

- [runtime-adapter-contract.md](./runtime-adapter-contract.md) owns the normalized orchestrator-to-runtime boundary: required request fields, capability advertisement, live event types, terminal results, adapter-side prohibitions, and the staged boundary between the stable port contract and the larger verified App Server surface.
- [orchestrator-user-chat-mcp-contract.md](./orchestrator-user-chat-mcp-contract.md) owns the built-in system MCP contract `orchestrator.user_chat`, including payload shape and routing invariants.
- [subagent-mcp-contract.md](./subagent-mcp-contract.md) owns the managed child-run MCP surface: launch, wait, send, and close semantics; bounded capability rules; and normalized return-shape rules.
- [agent-json/README.md](./agent-json/README.md) owns navigation and ownership boundaries inside the portable agent file contract.
- [agent-json/top-level-and-bindings-contract.md](./agent-json/top-level-and-bindings-contract.md) owns the agent root object, metadata, params, permissions, final output policy, and top-level binding objects.
- [agent-json/memory-binding-model-contract.md](./agent-json/memory-binding-model-contract.md) owns the portable memory-binding payload, required capability negotiation, provider-specific escape hatch, and local provider-registration boundary.
- [agent-json/nodes-and-edges-contract.md](./agent-json/nodes-and-edges-contract.md) owns nodes, input parts and references, node-level binding references, the explicit `output` contract, and edges.
- [agent-json/interaction-and-chat-contract.md](./agent-json/interaction-and-chat-contract.md) owns `interaction`, `chat`, resume-related chat policy, and secret-marker settings.

## Cross-Section Boundaries

- The portable agent file remains the source of truth for agent definition. Contracts here must not move that authority into runtime adapters or local storage.
- Skills, MCP servers, plugins, and memory backends keep their native runtime-defined internal contracts. This section owns only orchestrator-visible binding wrappers, portable intent fields, and IDs.
- The built-in user-chat MCP and the managed subagent MCP surface are the only MCP contracts owned by this repository. All other MCP contracts are referenced, not reproduced. MCP inside memory bindings is modeled as a transport or connection mode under a provider adapter, not as a separate memory family.
- If a future contract field is added, exactly one document in this section must become its owner before implementation starts.

## Non-Goals

- This section does not define database tables, migration plans, or cache formats.
- This section does not define end-user tutorials or examples-first prose.
- This section does not define runtime-vendor internals such as hidden traces, tool-call transcripts, or proprietary session storage.

<a id="russian"></a>
# Контракты

Этот раздел является нормативным домом для файловых и wire-контрактов. Документы здесь должны быть достаточно стабильными, чтобы по ним можно было строить JSON Schema, интерфейсы адаптеров, логику валидации и негативные тесты без повторного изобретения смысла полей в коде.

## Правила Раздела

- У каждого правила есть один документ-владелец. Соседние документы могут ссылаться на правило, но не должны переопределять его.
- Документы этого раздела определяют форму контракта, обязательные поля, допустимые значения, значения по умолчанию, инварианты и запреты.
- Архитектурная мотивация остается в [02-architecture](../02-architecture/README.md).
- Семантика исполнения сверх минимума, необходимого для понимания контракта, остается в [04-execution](../04-execution/README.md).
- Внутренности хранения и модель персистентности остаются в [05-state](../05-state/README.md).
- Поведение UI и пользовательская политика взаимодействия остаются в [06-interaction](../06-interaction/README.md).
- Примеры не являются нормой и относятся к [10-examples](../10-examples/README.md).

## Владение Документами

- [runtime-adapter-contract.md](./runtime-adapter-contract.md) владеет нормализованной границей между оркестратором и runtime: обязательными полями запроса, описанием возможностей, типами live-событий, терминальными результатами, запретами на стороне адаптера и этапной границей между стабильным port-контрактом и большей проверенной поверхностью App Server.
- [orchestrator-user-chat-mcp-contract.md](./orchestrator-user-chat-mcp-contract.md) владеет контрактом встроенного системного MCP `orchestrator.user_chat`, включая форму payload и инварианты маршрутизации.
- [subagent-mcp-contract.md](./subagent-mcp-contract.md) владеет managed child-run MCP surface: semantics launch, wait, send и close; правилами bounded capability; и нормализованной формой return.
- [agent-json/README.md](./agent-json/README.md) владеет навигацией и границами владения внутри контракта переносимого agent file.
- [agent-json/top-level-and-bindings-contract.md](./agent-json/top-level-and-bindings-contract.md) владеет корневым объектом агента, метаданными, params, permissions, политикой final output и top-level binding-объектами.
- [agent-json/nodes-and-edges-contract.md](./agent-json/nodes-and-edges-contract.md) владеет nodes, input parts и references, node-level binding references, явным контрактом `output` и edges.
- [agent-json/interaction-and-chat-contract.md](./agent-json/interaction-and-chat-contract.md) владеет `interaction`, `chat`, chat-политикой для resume и настройками secret markers.

## Границы Раздела

- Переносимый agent file остается источником истины для определения агента. Контракты этого раздела не должны переносить это право в runtime adapters или локальное хранилище.
- Skills, MCP servers, plugins и memory backends сохраняют свои нативные runtime-контракты. Этот раздел владеет только orchestrator-visible binding wrappers и идентификаторами.
- Встроенный user-chat MCP и managed subagent MCP surface являются единственными MCP-контрактами, которыми владеет этот репозиторий. Все остальные MCP-контракты только упоминаются, но не воспроизводятся.
- Если в будущем добавляется новое контрактное поле, до начала реализации у него должен появиться ровно один документ-владелец в этом разделе.

## Что Этот Раздел Не Определяет

- Этот раздел не определяет таблицы БД, план миграций и cache-форматы.
- Этот раздел не определяет пользовательские туториалы и examples-first описание.
- Этот раздел не определяет внутренности runtime-вендора, такие как скрытые трассировки, журналы tool calls и проприетарное session storage.
