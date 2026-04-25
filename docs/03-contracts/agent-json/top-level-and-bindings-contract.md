[English](#english) | [Русский](#russian)

<a id="english"></a>
# Top-Level And Bindings Contract

## Purpose and Ownership

This document owns the portable agent-file root object and these top-level concerns:

- root object closure and allowed field set
- `graph_contract_version`
- `meta`
- `entry_node_id`
- `params`
- `initial_vars`
- `skills`
- `mcps`
- `plugins`
- `permissions`
- `final_output`
- `memory_bindings`
- `runtime_sources`

This document does not own `interaction`, `chat`, `nodes`, or `edges`. Those are owned by [interaction-and-chat-contract.md](./interaction-and-chat-contract.md) and [nodes-and-edges-contract.md](./nodes-and-edges-contract.md).

## Root Object Rules

The root object is closed. In the current contract version, the only allowed top-level fields are:

- `graph_contract_version`
- `meta`
- `entry_node_id`
- `params`
- `initial_vars`
- `skills`
- `mcps`
- `plugins`
- `permissions`
- `interaction`
- `chat`
- `final_output`
- `nodes`
- `edges`
- `memory_bindings`
- `runtime_sources`

Any other top-level field is invalid unless a newer contract version explicitly adds it.

## Global Invariants

- The agent file is the source of truth for agent definition.
- `graph_contract_version` is required and must be written explicitly.
- `graph_contract_version` is not the same thing as `meta.agent_version`, live revision, or product version.
- IDs must be unique within their own collection. Cross-collection uniqueness is not required unless another document explicitly says so.
- This document is closed by default: every object it owns is closed unless this document explicitly defines it as a keyed map or opaque pass-through object.
- `params` and `initial_vars` are keyed maps. The container objects are open to their declared keys, each `params.<name>` descriptor is closed, and each `initial_vars.<name>` value may be any JSON value.
- `permissions.extra`, `mcps[].config`, and `plugins[].config` are intentionally opaque pass-through objects.
- `memory_bindings[].config` is opaque for non-`runtime_memory` uses and is normatively extended by [memory-binding-model-contract.md](./memory-binding-model-contract.md) when `kind = runtime_memory`.

## `graph_contract_version`

- Type: `string`
- Required: yes
- Meaning: version of graph-processing logic required to interpret the file correctly

Rules:

- The orchestrator must reject a file whose `graph_contract_version` it does not support.
- The orchestrator must not attempt best-effort interpretation of unknown contract versions.

## `meta`

- Type: `object`
- Required: yes
- Closed: yes

Allowed fields:

- `id`: required string. Stable agent identifier inside the portable contract.
- `name`: required string. Human-readable display name.
- `description`: optional string.
- `agent_version`: optional string. Logical version of the agent artifact.

Rules:

- `meta.id` is the portable agent identity; it is not a database row ID.
- `meta.agent_version` must not be used as a substitute for `graph_contract_version`.

## `entry_node_id`

- Type: `string`
- Required: yes
- Meaning: ID of the node where graph execution starts

Rules:

- `entry_node_id` must resolve to exactly one node ID defined in `nodes`.
- A file with no matching node is invalid.

## `params`

- Type: `object`
- Required: no
- Meaning: parameter declarations that may be supplied without cloning the agent file

`params` is an open map keyed by parameter name. Each parameter descriptor is a closed object with these fields:

- `type`: required string. Allowed values are `string`, `number`, `boolean`, `object`, `array`, and `null`.
- `required`: required boolean.
- `default`: optional JSON literal.
- `description`: optional string.
- `mutable_in_ui`: optional boolean. Default is `true`.
- `allowed_values`: optional non-empty array of JSON literals. Every item must match the declared `type`.
- `constraints`: optional closed object with a small portable subset of simple constraints:
  - for `string`: `min_length`, `max_length`, `pattern`;
  - for `number`: `minimum`, `maximum`;
  - for `array`: `min_items`, `max_items`.

Rules:

- Parameter names are contract keys and must be unique within `params`.
- If `default` is present, it must be valid for the declared `type`.
- If `allowed_values` is present, every item must match the declared `type` and the set must be unique by value.
- If `allowed_values` is present and `default` is also present, `default` must be one of the declared `allowed_values`.
- If `constraints` is present, it may use only fields compatible with the declared `type`.
- If both lower and upper bounds are present in `constraints`, the lower bound must not exceed the upper bound.
- If `constraints` is present, every declared `default` and every declared `allowed_values` item must satisfy it.
- If `required = true` and `default` is absent, the caller must provide the parameter at launch time.
- `mutable_in_ui = false` means interfaces must treat the parameter as fixed unless another workflow explicitly owns mutation.
- `allowed_values` is the portable contract mechanism for explicit allowed variants. Interfaces may render it as a picker or similar control, but this contract does not require any particular UI.
- A parameter may indirectly control sensitive runtime behavior, for example if another layer maps `params.<name>` into a runtime model choice. This contract permits that declaration, but it does not guarantee that changing such a parameter inside an existing chat or resume path is safe. Automatic context compaction, transcript migration, or in-place safety handling is outside this contract and must not be assumed.

## `initial_vars`

- Type: `object`
- Required: no
- Meaning: initial values of graph runtime variables

Rules:

- `initial_vars` is an open map keyed by variable name.
- Values in `initial_vars` must be valid JSON values.
- `initial_vars` seeds runtime state; it does not define the full future variable set.

## `skills`

- Type: `array`
- Required: no

Each item is a closed object with these fields:

- `id`: required string.
- `codex_ref`: optional string.
- `inline_text`: optional string.
- `frozen`: optional boolean. Default is `false`.

Rules:

- `id` must be unique within `skills`.
- At least one of `codex_ref` or `inline_text` must be present.
- If `frozen = true`, `inline_text` must be present.
- This contract owns only the binding wrapper, not the internal skill format used by the runtime.

## `mcps`

- Type: `array`
- Required: no

Each item is a closed object with these fields:

- `id`: required string.
- `codex_ref`: required string.
- `config`: optional object. Opaque pass-through data.

Rules:

- `id` must be unique within `mcps`.
- The orchestrator must not interpret `config` beyond passing it to the runtime adapter.
- This contract owns only MCP binding metadata, not the MCP server's own protocol.

## `plugins`

- Type: `array`
- Required: no

Each item is a closed object with these fields:

- `id`: required string.
- `codex_ref`: required string.
- `config`: optional object. Opaque pass-through data.

Rules:

- `id` must be unique within `plugins`.
- Plugin internals remain runtime-defined.

## `permissions`

- Type: `object`
- Required: no
- Closed: yes, except for `extra`

Allowed fields:

- `profile`: optional string. Name of a runtime-native permission profile.
- `allow`: optional array of strings. Explicit allow tokens.
- `deny`: optional array of strings. Explicit deny tokens.
- `extra`: optional object. Opaque pass-through data.

Rules:

- If `permissions` is absent at both top level and node level, runtime default permissions apply.
- Plugins do not define a separate permission model in this contract.
- `allow` and `deny` tokens are runtime-native strings. The orchestrator may validate structure, but not vendor semantics.

## `final_output`

- Type: `object`
- Required: no
- Closed: yes

Allowed fields:

- `mode`: required string when `final_output` is present. Allowed values are `last_node_output` and `none`.

Rules:

- If `final_output` is absent, the effective default is `last_node_output`.
- `none` disables automatic creation of a final user-facing answer; it does not mean that the run has no result.

## `memory_bindings`

- Type: `array`
- Required: no

Each item is a closed object with these fields:

- `id`: required string.
- `kind`: required string. In the current contract version, the only allowed value is `runtime_memory`.
- `codex_ref`: required string.
- `config`: optional object. Opaque pass-through data.
- `scope`: required string. Allowed values are `agent` and `node`.

Rules:

- `id` must be unique within `memory_bindings`.
- `codex_ref` is a local handle that resolves against a user-owned memory-provider registration. It is not a portable provider identifier and it is not an instruction for Dennett to install or own the provider.
- `scope = agent` means the binding is globally available unless a node-level allowlist narrows availability.
- `scope = node` means the binding is unavailable by default and must be named explicitly by a node-level `memory_ids` list.
- When `kind = runtime_memory`, portable memory intent lives in `config` and is owned by [memory-binding-model-contract.md](./memory-binding-model-contract.md).
- If `config` is absent, the binding is only a local placeholder and Core must not invent required capabilities, transport preferences, or provider-specific settings.
- This contract owns only binding identity and scope, not the internal storage format or session model of the memory source.

## `runtime_sources`

- Type: `array`
- Required: no

Each item is a closed object with these fields:

- `id`: required string.
- `runtime_adapter`: required string.
- `source_ref`: required string. Opaque adapter-specific identifier of the concrete execution source.
- `description`: optional string.

Rules:

- `id` must be unique within `runtime_sources`.
- `runtime_adapter` ties the source to one adapter family. Nodes may reference only compatible sources.
- The orchestrator owns source selection constraints, but it does not own the internal account or quota model of the source.

## Fields Owned Elsewhere

- `interaction` and `chat` are owned by [interaction-and-chat-contract.md](./interaction-and-chat-contract.md).
- `nodes` and `edges` are owned by [nodes-and-edges-contract.md](./nodes-and-edges-contract.md).

<a id="russian"></a>
# Контракт Верхнего Уровня И Binding-Объектов

## Назначение И Владение

Этот документ владеет корневым объектом переносимого agent file и следующими top-level зонами:

- замкнутость корневого объекта и допустимым набором полей
- `graph_contract_version`
- `meta`
- `entry_node_id`
- `params`
- `initial_vars`
- `skills`
- `mcps`
- `plugins`
- `permissions`
- `final_output`
- `memory_bindings`
- `runtime_sources`

Этот документ не владеет `interaction`, `chat`, `nodes` и `edges`. Ими владеют [interaction-and-chat-contract.md](./interaction-and-chat-contract.md) и [nodes-and-edges-contract.md](./nodes-and-edges-contract.md).

## Правила Корневого Объекта

Корневой объект является закрытым. В текущей версии контракта допустимы только следующие top-level поля:

- `graph_contract_version`
- `meta`
- `entry_node_id`
- `params`
- `initial_vars`
- `skills`
- `mcps`
- `plugins`
- `permissions`
- `interaction`
- `chat`
- `final_output`
- `nodes`
- `edges`
- `memory_bindings`
- `runtime_sources`

Любое иное top-level поле невалидно, если только новая версия контракта явно не добавит его.

## Глобальные Инварианты

- Agent file является источником истины для определения агента.
- `graph_contract_version` обязателен и должен быть записан явно.
- `graph_contract_version` не равен `meta.agent_version`, live revision или версии продукта.
- Идентификаторы обязаны быть уникальными внутри своей коллекции. Межколлекционная уникальность не требуется, если другой документ не требует ее явно.
- В этом документе действует правило «закрыто по умолчанию»: каждый принадлежащий ему объект закрыт, если только документ явно не определяет его как индексируемую карту или opaque pass-through-объект.
- `params` и `initial_vars` являются индексируемыми картами. Их контейнеры открыты для объявленных ключей, каждый descriptor `params.<name>` закрыт, а каждое значение `initial_vars.<name>` может быть любым JSON-значением.
- `permissions.extra`, `mcps[].config`, `plugins[].config` и `memory_bindings[].config` намеренно оставлены opaque pass-through-объектами.

## `graph_contract_version`

- Тип: `string`
- Обязательное: да
- Назначение: версия логики обработки графа, необходимая для корректной интерпретации файла

Правила:

- Оркестратор обязан отклонять файл с неподдерживаемым `graph_contract_version`.
- Оркестратор не должен пытаться интерпретировать неизвестные версии контракта в режиме best effort.

## `meta`

- Тип: `object`
- Обязательное: да
- Закрытый объект: да

Допустимые поля:

- `id`: обязательная строка. Стабильный идентификатор агента внутри переносимого контракта.
- `name`: обязательная строка. Понятное человеку отображаемое имя.
- `description`: необязательная строка.
- `agent_version`: необязательная строка. Логическая версия артефакта агента.

Правила:

- `meta.id` является переносимой идентичностью агента, а не ID строки в БД.
- `meta.agent_version` не должен использоваться как замена `graph_contract_version`.

## `entry_node_id`

- Тип: `string`
- Обязательное: да
- Назначение: ID ноды, с которой стартует исполнение графа

Правила:

- `entry_node_id` обязан разрешаться ровно в один ID ноды, определенной в `nodes`.
- Файл без соответствующей ноды является невалидным.

## `params`

- Тип: `object`
- Обязательное: нет
- Назначение: декларации параметров, которые можно передать без клонирования agent file

`params` является открытой картой, индексируемой именем параметра. Каждый descriptor параметра является закрытым объектом со следующими полями:

- `type`: обязательная строка. Допустимые значения: `string`, `number`, `boolean`, `object`, `array`, `null`.
- `required`: обязательный boolean.
- `default`: необязательный JSON literal.
- `description`: необязательная строка.
- `mutable_in_ui`: необязательный boolean. Значение по умолчанию: `true`.
- `allowed_values`: необязательный непустой массив JSON literal. Каждый элемент обязан соответствовать объявленному `type`.
- `constraints`: необязательный закрытый объект с небольшим переносимым подмножеством простых ограничений:
  - для `string`: `min_length`, `max_length`, `pattern`;
  - для `number`: `minimum`, `maximum`;
  - для `array`: `min_items`, `max_items`.

Правила:

- Имена параметров являются контрактными ключами и должны быть уникальны внутри `params`.
- Если присутствует `default`, он обязан соответствовать объявленному `type`.
- Если присутствует `allowed_values`, каждый элемент обязан соответствовать объявленному `type`, а сам набор обязан быть уникален по значению.
- Если присутствует `allowed_values` и одновременно присутствует `default`, `default` обязан быть одним из объявленных `allowed_values`.
- Если присутствует `constraints`, он может использовать только поля, совместимые с объявленным `type`.
- Если в `constraints` присутствуют и нижняя, и верхняя границы, нижняя граница не должна превышать верхнюю.
- Если присутствует `constraints`, каждый объявленный `default` и каждый элемент объявленного `allowed_values` обязаны ему удовлетворять.
- Если `required = true` и `default` отсутствует, вызывающая сторона обязана передать параметр при запуске.
- `mutable_in_ui = false` означает, что интерфейсы должны считать параметр фиксированным, если иной workflow явно не владеет его изменением.
- `allowed_values` является переносимым контрактным механизмом для явного объявления допустимых вариантов. Интерфейсы могут отображать его как picker или похожий контрол, но этот контракт не требует конкретного UI.
- Параметр может косвенно управлять чувствительным runtime-поведением, например если другой слой отображает `params.<name>` на выбор runtime-модели. Этот контракт допускает такую декларацию, но не гарантирует, что изменение такого параметра внутри уже существующего чата или пути resume безопасно. Автоматическая компактификация контекста, миграция транскрипта или безопасная in-place-обработка находятся вне рамок этого контракта и не должны предполагаться.

## `initial_vars`

- Тип: `object`
- Обязательное: нет
- Назначение: начальные значения runtime-переменных графа

Правила:

- `initial_vars` является открытой картой, индексируемой именем переменной.
- Значения в `initial_vars` должны быть валидными JSON-значениями.
- `initial_vars` инициализирует runtime state, но не определяет весь будущий набор переменных.

## `skills`

- Тип: `array`
- Обязательное: нет

Каждый элемент является закрытым объектом со следующими полями:

- `id`: обязательная строка.
- `codex_ref`: необязательная строка.
- `inline_text`: необязательная строка.
- `frozen`: необязательный boolean. Значение по умолчанию: `false`.

Правила:

- `id` обязан быть уникален внутри `skills`.
- Должно присутствовать хотя бы одно из полей `codex_ref` или `inline_text`.
- Если `frozen = true`, поле `inline_text` обязано присутствовать.
- Этот контракт владеет только binding-wrapper, а не внутренним форматом skill в runtime.

## `mcps`

- Тип: `array`
- Обязательное: нет

Каждый элемент является закрытым объектом со следующими полями:

- `id`: обязательная строка.
- `codex_ref`: обязательная строка.
- `config`: необязательный object. Opaque pass-through-данные.

Правила:

- `id` обязан быть уникален внутри `mcps`.
- Оркестратор не должен интерпретировать `config` глубже, чем передача в runtime adapter.
- Этот контракт владеет только metadata binding-объекта, а не собственным протоколом MCP-сервера.

## `plugins`

- Тип: `array`
- Обязательное: нет

Каждый элемент является закрытым объектом со следующими полями:

- `id`: обязательная строка.
- `codex_ref`: обязательная строка.
- `config`: необязательный object. Opaque pass-through-данные.

Правила:

- `id` обязан быть уникален внутри `plugins`.
- Внутренности plugin остаются определяемыми runtime.

## `permissions`

- Тип: `object`
- Обязательное: нет
- Закрытый объект: да, кроме `extra`

Допустимые поля:

- `profile`: необязательная строка. Имя runtime-native permission profile.
- `allow`: необязательный массив строк. Явные allow-токены.
- `deny`: необязательный массив строк. Явные deny-токены.
- `extra`: необязательный object. Opaque pass-through-данные.

Правила:

- Если `permissions` отсутствует и на top-level, и на node-level, используются runtime default permissions.
- Plugins не определяют отдельную permission-модель в рамках этого контракта.
- Токены в `allow` и `deny` являются runtime-native строками. Оркестратор может проверять структуру, но не vendor-семантику.

## `final_output`

- Тип: `object`
- Обязательное: нет
- Закрытый объект: да

Допустимые поля:

- `mode`: обязательная строка, если `final_output` присутствует. Допустимые значения: `last_node_output` и `none`.

Правила:

- Если `final_output` отсутствует, эффективное значение по умолчанию равно `last_node_output`.
- `none` отключает автоматическое создание финального пользовательского ответа, но не означает отсутствие результата у run.

## `memory_bindings`

- Тип: `array`
- Обязательное: нет

Каждый элемент является закрытым объектом со следующими полями:

- `id`: обязательная строка.
- `kind`: обязательная строка. В текущей версии контракта допустимо только значение `runtime_memory`.
- `codex_ref`: обязательная строка.
- `config`: необязательный object. Opaque pass-through-данные.
- `scope`: обязательная строка. Допустимые значения: `agent` и `node`.

Правила:

- `id` обязан быть уникален внутри `memory_bindings`.
- `scope = agent` означает, что binding глобально доступен, если node-level allowlist не сужает доступность.
- `scope = node` означает, что binding по умолчанию недоступен и должен быть явно указан в node-level `memory_ids`.
- Этот контракт владеет только идентичностью и областью действия binding-объекта, а не внутренним форматом хранения memory source.

## `runtime_sources`

- Тип: `array`
- Обязательное: нет

Каждый элемент является закрытым объектом со следующими полями:

- `id`: обязательная строка.
- `runtime_adapter`: обязательная строка.
- `source_ref`: обязательная строка. Opaque adapter-specific идентификатор конкретного источника исполнения.
- `description`: необязательная строка.

Правила:

- `id` обязан быть уникален внутри `runtime_sources`.
- `runtime_adapter` привязывает источник к одному семейству адаптера. Ноды могут ссылаться только на совместимые источники.
- Оркестратор владеет ограничениями выбора источника, но не владеет внутренней моделью аккаунтов или квот этого источника.

## Поля, Которыми Владеют Другие Документы

- `interaction` и `chat` принадлежат [interaction-and-chat-contract.md](./interaction-and-chat-contract.md).
- `nodes` и `edges` принадлежат [nodes-and-edges-contract.md](./nodes-and-edges-contract.md).
