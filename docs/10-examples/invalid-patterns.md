[English](#english) | [Русский](#russian)

<a id="english"></a>
# Invalid Patterns

Status: non-normative anti-pattern catalogue.
Owns: nothing. These examples stay invalid or forbidden on purpose.

Section map:

- [Examples index](./README.md)
- [Canonical agent JSON example](./canonical-agent-json-example.md)
- [Valid patterns](./valid-patterns.md)
- [Interaction sequences](./interaction-sequences.md)

## 1. Smuggling Local Lifecycle State Into The Portable File

```json
{
  "graph_contract_version": "1.0",
  "meta": {
    "id": "review-assistant",
    "name": "Review Assistant"
  },
  "entry_node_id": "triage",
  "live_revision": "r17",
  "registry_status": "available",
  "nodes": []
}
```

Why this is invalid:

- The root object is closed, so `live_revision` and `registry_status` do not belong there.
- Live revision and registry status are local lifecycle facts, not portable contract fields.

Owner docs: [top-level-and-bindings-contract.md](../03-contracts/agent-json/top-level-and-bindings-contract.md), [agent-registry.md](../07-lifecycle/agent-registry.md)

## 2. Combining `inherit` With An Explicit Source List

```json
{
  "runtime_source_policy": "inherit",
  "runtime_source_ids": ["primary_codex"]
}
```

Why this is invalid:

- `inherit` means “do not narrow beyond the inherited eligible set”.
- Adding `runtime_source_ids` at the same time tries to narrow and not narrow in one declaration.

Owner docs: [nodes-and-edges-contract.md](../03-contracts/agent-json/nodes-and-edges-contract.md), [runtime-sources.md](../08-extensions/runtime-sources.md)

## 3. Targeting Comments At A Non-`runtime_agent` Node

```json
{
  "interaction": {
    "comments": {
      "enabled": true,
      "target_node_ids": ["specialist_review"]
    }
  }
}
```

Why this is invalid:

- Comment delivery targets only existing `runtime_agent` nodes.
- An `orchestrator_agent` node is not a valid live-comment target.

Owner docs: [interaction-and-chat-contract.md](../03-contracts/agent-json/interaction-and-chat-contract.md), [live-run-interaction.md](../06-interaction/live-run-interaction.md)

## 4. Expecting Explicit JSON Output To Accept Arrays Or Strings

Anti-example:

1. The node declares `output.mode = json` with an explicit schema.
2. The runtime returns `["todo", "review"]` or `"done"`.
3. The implementation silently coerces that value into an object.

Why this is invalid:

- `output.mode = json` requires a top-level JSON object that satisfies the declared schema.
- Array, scalar, and string roots are invalid output for that mode.
- The adapter must not silently coerce them into a fake object.

Owner docs: [nodes-and-edges-contract.md](../03-contracts/agent-json/nodes-and-edges-contract.md), [outputs-outcomes-and-final-response.md](../04-execution/outputs-outcomes-and-final-response.md), [runtime-adapter-contract.md](../03-contracts/runtime-adapter-contract.md)

## 5. Auto-Routing Free Text Into A Pending MCP Prompt

Anti-example:

1. A built-in `orchestrator.user_chat` prompt is pending.
2. The user types free-form text into the normal composer.
3. The interface silently routes that text as a prompt reply.

Why this is invalid:

- MCP replies require explicit prompt binding.
- Free-form text remains a comment or must be rejected if comments are unavailable.
- One generic send action whose routing changes invisibly is forbidden presentation.

Owner docs: [orchestrator-user-chat-mcp-contract.md](../03-contracts/orchestrator-user-chat-mcp-contract.md), [live-run-interaction.md](../06-interaction/live-run-interaction.md), [presentation-rules.md](../06-interaction/presentation-rules.md)

## 6. Treating Saved Chat As Implicit Memory

Anti-example:

1. The file declares no `memory_bindings`.
2. The node still relies on “whatever the previous chat remembered”.
3. Core silently treats chat transcript as long-term memory.

Why this is invalid:

- Chat/resume continuity and memory bindings are separate concerns.
- Memory access must be declared explicitly instead of inferred from stored conversation state.

Owner docs: [chat-and-resume.md](../05-state/chat-and-resume.md), [memory-bindings.md](../08-extensions/memory-bindings.md)

## 7. Falling Back To A Draft Or Newer Live Revision Without Explicit Authority

Anti-example:

1. An event resolves a logical agent, but the current live file is missing or invalid.
2. Core silently launches the newest draft instead.

Why this is invalid:

- Events resolve the current live revision at dispatch time.
- Missing or invalid live state must fail visibly.
- Drafts are never silent fallback targets for ordinary runs or event dispatch.

Owner docs: [agent-registry.md](../07-lifecycle/agent-registry.md), [draft-live-deploy.md](../07-lifecycle/draft-live-deploy.md), [events-and-triggers.md](../07-lifecycle/events-and-triggers.md)

## 8. Treating ADR Or Example Text As The Rule Owner

Anti-example:

1. A reviewer cites an ADR or this examples section as the final rule source.
2. The linked normative contract says something narrower or more current.
3. The implementation follows the example or ADR anyway.

Why this is invalid:

- ADRs preserve rationale, not the live rule set.
- This section is explicitly non-normative.
- The owning contract or profile document still wins.

Owner docs: [source-of-truth-model.md](../01-foundations/source-of-truth-model.md)

<a id="russian"></a>
# Невалидные Паттерны

Статус: ненормативный каталог анти-паттернов.
Владение: ничем. Эти примеры намеренно остаются невалидными или запрещенными.

Карта раздела:

- [Индекс примеров](./README.md)
- [Канонический пример agent JSON](./canonical-agent-json-example.md)
- [Валидные паттерны](./valid-patterns.md)
- [Сценарии взаимодействия](./interaction-sequences.md)

## 1. Протаскивать Локальное Lifecycle-Состояние В Переносимый Файл

```json
{
  "graph_contract_version": "1.0",
  "meta": {
    "id": "review-assistant",
    "name": "Review Assistant"
  },
  "entry_node_id": "triage",
  "live_revision": "r17",
  "registry_status": "available",
  "nodes": []
}
```

Почему это невалидно:

- Корневой объект закрыт, поэтому `live_revision` и `registry_status` там неуместны.
- Live revision и registry status являются локальными lifecycle-фактами, а не переносимыми contract-полями.

Документы-владельцы: [top-level-and-bindings-contract.md](../03-contracts/agent-json/top-level-and-bindings-contract.md), [agent-registry.md](../07-lifecycle/agent-registry.md)

## 2. Комбинировать `inherit` С Явным Списком Sources

```json
{
  "runtime_source_policy": "inherit",
  "runtime_source_ids": ["primary_codex"]
}
```

Почему это невалидно:

- `inherit` означает «не сужать выбор сверх унаследованного допустимого множества».
- Одновременное добавление `runtime_source_ids` пытается и сузить, и не сузить декларацию сразу.

Документы-владельцы: [nodes-and-edges-contract.md](../03-contracts/agent-json/nodes-and-edges-contract.md), [runtime-sources.md](../08-extensions/runtime-sources.md)

## 3. Направлять Комментарии В Ноду, Которая Не Является `runtime_agent`

```json
{
  "interaction": {
    "comments": {
      "enabled": true,
      "target_node_ids": ["specialist_review"]
    }
  }
}
```

Почему это невалидно:

- Доставка комментариев адресуется только существующим нодам `runtime_agent`.
- Нода `orchestrator_agent` не является допустимой целью для live comments.

Документы-владельцы: [interaction-and-chat-contract.md](../03-contracts/agent-json/interaction-and-chat-contract.md), [live-run-interaction.md](../06-interaction/live-run-interaction.md)

## 4. Ожидать, Что Явный JSON-Output Примет Массивы Или Строки

Анти-пример:

1. Нода объявляет `output.mode = json` с явной schema.
2. Runtime возвращает `["todo", "review"]` или `"done"`.
3. Реализация молча приводит это значение к объекту.

Почему это невалидно:

- `output.mode = json` требует top-level JSON object, который удовлетворяет объявленной schema.
- Корни-массивы, скаляры и строки невалидны для этого режима.
- Адаптер не должен молча приводить их к фиктивному объекту.

Документы-владельцы: [nodes-and-edges-contract.md](../03-contracts/agent-json/nodes-and-edges-contract.md), [outputs-outcomes-and-final-response.md](../04-execution/outputs-outcomes-and-final-response.md), [runtime-adapter-contract.md](../03-contracts/runtime-adapter-contract.md)

## 5. Автоматически Маршрутизировать Свободный Текст В Ожидающий MCP-Prompt

Анти-пример:

1. Ожидает built-in prompt через `orchestrator.user_chat`.
2. Пользователь печатает свободный текст в обычный composer.
3. Интерфейс молча отправляет этот текст как reply на prompt.

Почему это невалидно:

- MCP-replies требуют явной привязки к prompt.
- Свободный текст остается комментарием или должен быть отклонен, если комментарии недоступны.
- Один универсальный send-action с невидимо меняющейся маршрутизацией запрещен.

Документы-владельцы: [orchestrator-user-chat-mcp-contract.md](../03-contracts/orchestrator-user-chat-mcp-contract.md), [live-run-interaction.md](../06-interaction/live-run-interaction.md), [presentation-rules.md](../06-interaction/presentation-rules.md)

## 6. Считать Сохраненный Chat Неявной Memory

Анти-пример:

1. Файл не объявляет ни одного `memory_bindings`.
2. Нода все равно зависит от «того, что запомнил прошлый chat».
3. Core молча трактует transcript чата как long-term memory.

Почему это невалидно:

- Continuity чата/resume и memory bindings являются разными concerns.
- Доступ к памяти должен объявляться явно, а не выводиться из сохраненного conversation state.

Документы-владельцы: [chat-and-resume.md](../05-state/chat-and-resume.md), [memory-bindings.md](../08-extensions/memory-bindings.md)

## 7. Делать Fallback На Draft Или Более Новую Live-Ревизию Без Явного Полномочия

Анти-пример:

1. Event разрешает логического агента, но текущий live-файл отсутствует или невалиден.
2. Core молча запускает самый новый draft вместо явного отказа.

Почему это невалидно:

- Events разрешают текущую live-ревизию в момент dispatch.
- Отсутствующее или невалидное live-состояние должно завершаться явным отказом.
- Drafts никогда не являются молчаливой fallback-целью для обычных run-ов или event-dispatch.

Документы-владельцы: [agent-registry.md](../07-lifecycle/agent-registry.md), [draft-live-deploy.md](../07-lifecycle/draft-live-deploy.md), [events-and-triggers.md](../07-lifecycle/events-and-triggers.md)

## 8. Считать Текст ADR Или Этого Раздела Владельцем Правила

Анти-пример:

1. Reviewer ссылается на ADR или этот раздел примеров как на окончательный rule source.
2. Связанный нормативный контракт говорит нечто более узкое или более актуальное.
3. Реализация все равно следует примеру или ADR.

Почему это невалидно:

- ADR сохраняют мотивацию, а не актуальный rule set.
- Этот раздел явно является ненормативным.
- Побеждает профильный контракт или профильный документ-владелец.

Документы-владельцы: [source-of-truth-model.md](../01-foundations/source-of-truth-model.md)
