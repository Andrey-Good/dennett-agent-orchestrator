[English](#english) | [Русский](#russian)

<a id="english"></a>
# Nodes And Edges Contract

## Purpose and Ownership

This document owns the executable graph surface inside the agent file:

- `nodes`
- node-kind rules
- `input`
- allowed input references
- node-level binding references
- node output interpretation
- `edges`

This document does not own top-level binding definitions or chat policy objects. Those live in [top-level-and-bindings-contract.md](./top-level-and-bindings-contract.md) and [interaction-and-chat-contract.md](./interaction-and-chat-contract.md).

## `nodes`

- Type: `array`
- Required: yes

Rules:

- `nodes` must contain at least one item.
- Every node object is closed, except for `runtime_options`, which is an opaque pass-through object.
- `id` must be unique within `nodes`.
- `entry_node_id` from the top-level contract must resolve to one of these node IDs.

## Common Node Fields

Every node is a closed object with these common fields:

- `id`: required string.
- `title`: optional string.
- `kind`: required string. Allowed values are `runtime_agent` and `orchestrator_agent`.
- `input`: required object owned by this document.
- `output`: required object owned by this document.

## `output`

`output` is a closed object with one required field:

- `mode`: required string. Allowed values are `text` and `json`.

Rules:

- `output.mode = text` declares plain text output. In this mode, `schema` is forbidden.
- `output.mode = json` declares structured JSON output. In this mode, `schema` is required.
- `output.schema`, when required, must be a JSON Schema 2020-12 object whose root describes a JSON object output.
- There is no hidden rule where missing schema implies text output. Text output must stay explicit through `output.mode = text`.

## `runtime_agent` Node

For `kind = runtime_agent`, these fields are required or allowed:

- `runtime_adapter`: required string.
- `prompt`: required string.
- `skill_ids`: optional array of strings.
- `mcp_ids`: optional array of strings.
- `plugin_ids`: optional array of strings.
- `memory_ids`: optional array of strings.
- `permissions`: optional object.
- `runtime_options`: optional opaque object.
- `runtime_source_ids`: optional array of strings.
- `runtime_source_policy`: optional string. Allowed values are `inherit`, `restrict`, and `prefer_first`.

For `kind = runtime_agent`, `agent_ref` is forbidden.

Rules:

- `skill_ids`, `mcp_ids`, `plugin_ids`, `memory_ids`, and `runtime_source_ids` must not contain duplicates.
- Every referenced ID must resolve to an existing top-level binding or runtime source.
- `permissions`, when present, overrides the top-level permission profile for this node only.
- `runtime_options` stays an opaque portable object, but the current executable slice assigns normalized meaning only to `model`, `reasoning_effort`, `speed_tier`, and `personality`. Any other key remains outside the current execution slice unless another owner document expands that surface.
- If `runtime_source_policy` is absent, the effective policy is `inherit`.
- If `runtime_source_policy = inherit`, `runtime_source_ids` must be absent.
- If `runtime_source_policy = restrict`, `runtime_source_ids` must be present and non-empty. Only those compatible sources are eligible. Core resolves one of them deterministically under the runtime-source extension, and source-availability failure before launch is allowed only when supported runtime-source introspection explicitly reports every eligible source unavailable or exhausted.
- If `runtime_source_policy = prefer_first`, `runtime_source_ids` must be present and non-empty. Only the listed compatible sources are eligible, and Core resolves them in the declared order, skipping a listed source before launch only when supported runtime-source introspection explicitly reports it unavailable or exhausted. If supported introspection explicitly reports every listed source unavailable or exhausted, the node fails before launch.
- Every referenced runtime source must have `runtime_adapter` equal to the node's `runtime_adapter`.

## `orchestrator_agent` Node

For `kind = orchestrator_agent`, these fields are required or allowed:

- `agent_ref`: required string.
- `input`: required object.
- `output`: required object.

For `kind = orchestrator_agent`, these fields are forbidden in the current contract version:

- `runtime_adapter`
- `prompt`
- `skill_ids`
- `mcp_ids`
- `plugin_ids`
- `memory_ids`
- `permissions`
- `runtime_options`
- `runtime_source_ids`
- `runtime_source_policy`

Rules:

- `agent_ref` is the logical identity of the target agent. In the portable contract, it means the same identifier as the target agent's `meta.id`.
- `agent_ref` is not a file path, not a draft selector, not a local revision id, and not a registry row id.
- Core resolves `agent_ref` by the live-resolution rules from [../../07-lifecycle/agent-registry.md](../../07-lifecycle/agent-registry.md): by default the current live revision is selected at launch time, unless a higher-level workflow already pinned a different revision outside the portable contract.
- The child-return semantics of `orchestrator_agent` are owned by [../../04-execution/outputs-outcomes-and-final-response.md](../../04-execution/outputs-outcomes-and-final-response.md). This contract does not create a second hidden child-return channel.

## Memory Availability At Node Level

`memory_ids` refines the top-level `memory_bindings` contract:

- If `memory_ids` is absent, all bindings with `scope = agent` are available and bindings with `scope = node` are unavailable.
- If `memory_ids` is present, only listed bindings are available to that node.
- Every listed `memory_id` must resolve to an existing top-level memory binding.

## `input`

`input` is a closed object with one required field:

- `parts`: required array.

Each item in `parts` is a closed object of exactly one of these forms:

- text part: `{ "type": "text", "text": "..." }`
- reference part: `{ "type": "ref", "ref": "..." }`

Rules:

- `type = text` requires `text` and forbids `ref`.
- `type = ref` requires `ref` and forbids `text`.
- The orchestrator must preserve part order exactly.
- The orchestrator must not inject hidden undeclared data into the input message.

## Allowed References

Only these reference namespaces are valid:

- `params.<name>`
- `vars.<name>`
- `node.<node_id>.text`
- `node.<node_id>.json.<path>`
- `event.<path>`

Rules:

- Any other reference form is invalid.
- `params.<name>` must resolve to a declared parameter value.
- `vars.<name>` must resolve to the current runtime variable value.
- `node.<node_id>.text` is valid only if the referenced node has completed successfully with `output.mode = text`.
- `node.<node_id>.json.<path>` is valid only if the referenced node has completed successfully with `output.mode = json` and the path resolves inside the returned object.
- `event.<path>` is valid only when the run was started by an event and the requested path exists in the graph-visible `event` envelope defined in [../../04-execution/dataflow-and-input-resolution.md](../../04-execution/dataflow-and-input-resolution.md).
- References to missing names, missing paths, or not-yet-produced node outputs are invalid at evaluation time.

## Input Rendering Rule

The persisted contract defines ordered parts and legal reference sources. When the orchestrator builds the runtime input message:

- text parts are inserted verbatim
- resolved string values are inserted verbatim
- resolved non-string values are inserted as canonical JSON text

The adapter must receive already resolved input. It must not perform reference lookup by itself.

## Control Flow and Data Flow Separation

Edges control execution order only. They do not carry payload data.

Data may enter a node only through:

- `params`
- runtime `vars`
- prior node outputs referenced through `input`
- the graph-visible event envelope referenced through `input`

## Node Output Contract

A node output is only the agent's final output for that node. It is not the runtime's internal trace, tool history, or hidden reasoning.

Rules for `output.mode = text`:

- The final value must be a string.
- The value becomes the node's text output and may be referenced later through `node.<node_id>.text`.

Rules for `output.mode = json`:

- The final value must be a JSON object.
- The final value must satisfy `output.schema`.
- Arrays, strings, numbers, booleans, and `null` are invalid output for this mode.
- On success, object fields are merged into runtime `vars`.
- If multiple nodes write the same variable name, the last write wins.
- The full object remains addressable through `node.<node_id>.json.<path>`.

The orchestrator must not add hidden prompt text automatically in order to force JSON output. Structured JSON must come from the explicit `output` contract and supported runtime mechanisms, not from absence-of-schema conventions or prompt-formatting hacks.

## `edges`

- Type: `array`
- Required: no

Each edge is a closed object with these fields:

- `from`: required string.
- `to`: required string.
- `condition`: optional closed object.

`condition`, when present, is a closed object with one required field:

- `code`: required string containing Python source or a Python expression.

Rules:

- `from` and `to` must resolve to existing node IDs.
- Outgoing edges are evaluated in the order they appear in the `edges` array.
- The first edge whose condition evaluates to `true` is taken.
- If an edge has no `condition`, it is unconditional.
- If no outgoing edge matches, the graph terminates.
- Conditions execute against read-only names `params`, `vars`, `node`, and `event`.
- A condition evaluation failure is a run failure; it must not be treated as `false` silently.

## Invalid Cases

The following are contract violations:

- duplicate node IDs
- `entry_node_id` that does not resolve
- unknown binding IDs or runtime source IDs
- a `runtime_agent` node without `runtime_adapter`, `prompt`, `input`, or `output`
- an `orchestrator_agent` node without `agent_ref`
- forbidden fields appearing on the wrong node kind
- illegal reference syntax
- unresolved input reference
- `output.mode = json` without `output.schema`
- `output.mode = json` with a schema root that does not describe an object
- `output.mode = json` returning a value that is not an object
- edges that point to unknown nodes

## Cross-Links

- Top-level binding objects live in [top-level-and-bindings-contract.md](./top-level-and-bindings-contract.md).
- Runtime adapter behavior that consumes the normalized node call lives in [../runtime-adapter-contract.md](../runtime-adapter-contract.md).

<a id="russian"></a>
# Контракт Nodes И Edges

## Назначение И Владение

Этот документ владеет исполнимой графовой поверхностью внутри agent file:

- `nodes`
- правилами для видов нод
- `input`
- допустимыми input references
- node-level binding references
- интерпретацией node output
- `edges`

Этот документ не владеет top-level binding-объектами и chat policy-объектами. Они находятся в [top-level-and-bindings-contract.md](./top-level-and-bindings-contract.md) и [interaction-and-chat-contract.md](./interaction-and-chat-contract.md).

## `nodes`

- Тип: `array`
- Обязательное: да

Правила:

- `nodes` обязан содержать хотя бы один элемент.
- Каждый объект ноды является закрытым, кроме `runtime_options`, который остается opaque pass-through-объектом.
- `id` обязан быть уникален внутри `nodes`.
- `entry_node_id` из top-level контракта обязан разрешаться в один из этих node IDs.

## Общие Поля Ноды

Каждая нода является закрытым объектом со следующими общими полями:

- `id`: обязательная строка.
- `title`: необязательная строка.
- `kind`: обязательная строка. Допустимые значения: `runtime_agent` и `orchestrator_agent`.
- `input`: обязательный объект, которым владеет этот документ.
- `output`: обязательный объект, которым владеет этот документ.

## `output`

`output` является закрытым объектом с одним обязательным полем:

- `mode`: обязательная строка. Допустимые значения: `text` и `json`.

Правила:

- `output.mode = text` объявляет текстовый output. В этом режиме `schema` запрещено.
- `output.mode = json` объявляет structured JSON output. В этом режиме `schema` обязательно.
- Обязательное поле `output.schema` должно быть JSON Schema 2020-12 object, у которого корень описывает JSON object.
- Скрытого правила «если schema отсутствует, значит output текстовый» не существует. Текстовый output должен оставаться явным через `output.mode = text`.

## Нода `runtime_agent`

Для `kind = runtime_agent` обязательны или допустимы следующие поля:

- `runtime_adapter`: обязательная строка.
- `prompt`: обязательная строка.
- `skill_ids`: необязательный массив строк.
- `mcp_ids`: необязательный массив строк.
- `plugin_ids`: необязательный массив строк.
- `memory_ids`: необязательный массив строк.
- `permissions`: необязательный объект.
- `runtime_options`: необязательный opaque object.
- `runtime_source_ids`: необязательный массив строк.
- `runtime_source_policy`: необязательная строка. Допустимые значения: `inherit`, `restrict` и `prefer_first`.

Для `kind = runtime_agent` поле `agent_ref` запрещено.

Правила:

- `skill_ids`, `mcp_ids`, `plugin_ids`, `memory_ids` и `runtime_source_ids` не должны содержать дубликатов.
- Каждый указанный ID обязан разрешаться в существующий top-level binding или runtime source.
- `permissions`, если оно присутствует, переопределяет top-level permission profile только для этой ноды.
- Если `runtime_source_policy` отсутствует, эффективная политика равна `inherit`.
- Если `runtime_source_policy = inherit`, `runtime_source_ids` должно отсутствовать.
- Если `runtime_source_policy = restrict`, `runtime_source_ids` обязано присутствовать и быть непустым. Допустимы только эти совместимые источники. Core детерминированно разрешает один из них по правилам runtime-source extension, а ошибка до запуска по причине доступности source разрешена только тогда, когда поддерживаемая runtime-source introspection явно сообщает, что каждый допустимый source недоступен или исчерпан.
- Если `runtime_source_policy = prefer_first`, `runtime_source_ids` обязано присутствовать и быть непустым. Допустимы только перечисленные совместимые источники, и Core разрешает их в указанном порядке, пропуская source до запуска только тогда, когда поддерживаемая runtime-source introspection явно сообщает, что он недоступен или исчерпан. Если поддерживаемая introspection явно сообщает это для каждого перечисленного source, нода завершается ошибкой до запуска.
- Каждый указанный runtime source обязан иметь `runtime_adapter`, совпадающий с `runtime_adapter` ноды.

## Нода `orchestrator_agent`

Для `kind = orchestrator_agent` обязательны или допустимы следующие поля:

- `agent_ref`: обязательная строка.
- `input`: обязательный объект.
- `output`: обязательный объект.

Для `kind = orchestrator_agent` следующие поля запрещены в текущей версии контракта:

- `runtime_adapter`
- `prompt`
- `skill_ids`
- `mcp_ids`
- `plugin_ids`
- `memory_ids`
- `permissions`
- `runtime_options`
- `runtime_source_ids`
- `runtime_source_policy`

Правила:

- `agent_ref` — это логическая идентичность целевого агента. В переносимом контракте он означает тот же идентификатор, что и `meta.id` целевого агента.
- `agent_ref` не является файловым путем, селектором draft, идентификатором локальной ревизии или идентификатором строки реестра.
- Core разрешает `agent_ref` по правилам live-resolution из [../../07-lifecycle/agent-registry.md](../../07-lifecycle/agent-registry.md): по умолчанию при запуске выбирается текущая `live`-ревизия, если только более высокий workflow не зафиксировал другую ревизию вне переносимого контракта.
- Семантика возврата из дочернего вызова `orchestrator_agent` принадлежит [../../04-execution/outputs-outcomes-and-final-response.md](../../04-execution/outputs-outcomes-and-final-response.md). Этот контракт не создает второго скрытого канала возврата из child run.

## Доступность Памяти На Уровне Ноды

`memory_ids` уточняет top-level контракт `memory_bindings`:

- Если `memory_ids` отсутствует, доступны все binding-объекты с `scope = agent`, а binding-объекты с `scope = node` недоступны.
- Если `memory_ids` присутствует, ноде доступны только перечисленные binding-объекты.
- Каждый перечисленный `memory_id` обязан разрешаться в существующий top-level memory binding.

## `input`

`input` является закрытым объектом с одним обязательным полем:

- `parts`: обязательный массив.

Каждый элемент `parts` является закрытым объектом ровно одной из следующих форм:

- текстовая часть: `{ "type": "text", "text": "..." }`
- reference-часть: `{ "type": "ref", "ref": "..." }`

Правила:

- `type = text` требует `text` и запрещает `ref`.
- `type = ref` требует `ref` и запрещает `text`.
- Оркестратор обязан сохранять порядок частей без изменений.
- Оркестратор не должен внедрять во входное сообщение скрытые необъявленные данные.

## Допустимые Ссылки

Допустимы только следующие пространства имен ссылок:

- `params.<name>`
- `vars.<name>`
- `node.<node_id>.text`
- `node.<node_id>.json.<path>`
- `event.<path>`

Правила:

- Любая иная форма ссылки невалидна.
- `params.<name>` обязана разрешаться в объявленное значение параметра.
- `vars.<name>` обязана разрешаться в текущее значение runtime-переменной.
- `node.<node_id>.text` допустима только если указанная нода успешно завершилась с `output.mode = text`.
- `node.<node_id>.json.<path>` допустима только если указанная нода успешно завершилась с `output.mode = json` и путь разрешается внутри возвращенного объекта.
- `event.<path>` допустима только если run был запущен событием и запрошенный путь существует внутри graph-visible envelope `event`, определенного в [../../04-execution/dataflow-and-input-resolution.md](../../04-execution/dataflow-and-input-resolution.md).
- Ссылки на отсутствующие имена, отсутствующие пути или еще не произведенные outputs нод являются невалидными в момент вычисления.

## Правило Рендеринга Входа

Persisted-контракт определяет упорядоченные части и допустимые источники references. Когда оркестратор строит runtime input message:

- текстовые части вставляются как есть
- разрешенные строковые значения вставляются как есть
- разрешенные нестроковые значения вставляются как канонический JSON text

Адаптер обязан получать уже разрешенный input. Он не должен самостоятельно выполнять reference lookup.

## Разделение Control Flow И Data Flow

Edges управляют только порядком исполнения. Они не переносят payload.

Данные могут попадать в ноду только через:

- `params`
- runtime `vars`
- outputs предыдущих нод, на которые ссылается `input`
- graph-visible envelope `event`, на который ссылается `input`

## Контракт Node Output

Node output - это только финальный output агента для этой ноды. Это не внутренняя трасса runtime, не история tools и не скрытые рассуждения.

Правила для `output.mode = text`:

- Финальное значение обязано быть строкой.
- Значение становится text output этой ноды и может далее использоваться через `node.<node_id>.text`.

Правила для `output.mode = json`:

- Финальное значение обязано быть JSON object.
- Финальное значение обязано удовлетворять `output.schema`.
- Массивы, строки, числа, boolean и `null` являются невалидным output для этого режима.
- При успехе поля объекта сливаются в runtime `vars`.
- Если несколько нод записывают одно и то же имя переменной, побеждает последняя запись.
- Полный объект остается доступным через `node.<node_id>.json.<path>`.

Оркестратор не должен автоматически добавлять скрытый prompt text, чтобы заставить агента вернуть JSON. Structured JSON должен задаваться явным контрактом `output` и поддерживаемыми runtime-механизмами, а не соглашением «нет schema - значит текст» или prompt-formatting hacks.

## `edges`

- Тип: `array`
- Обязательное: нет

Каждый edge является закрытым объектом со следующими полями:

- `from`: обязательная строка.
- `to`: обязательная строка.
- `condition`: необязательный закрытый объект.

`condition`, если он присутствует, является закрытым объектом с одним обязательным полем:

- `code`: обязательная строка с Python source или Python expression.

Правила:

- `from` и `to` обязаны разрешаться в существующие node IDs.
- Исходящие edges проверяются в том порядке, в котором они записаны в массиве `edges`.
- Берется первый edge, условие которого вычислилось в `true`.
- Если у edge нет `condition`, он является безусловным.
- Если ни один исходящий edge не подошел, граф завершается.
- Conditions выполняются в контексте read-only имен `params`, `vars`, `node` и `event`.
- Ошибка вычисления condition является ошибкой run; она не должна молча трактоваться как `false`.

## Невалидные Случаи

Нарушениями контракта являются:

- дублирующиеся node IDs
- `entry_node_id`, который не разрешается
- неизвестные binding IDs или runtime source IDs
- нода `runtime_agent` без `runtime_adapter`, `prompt`, `input` или `output`
- нода `orchestrator_agent` без `agent_ref`
- запрещенные поля на неверном виде ноды
- недопустимый синтаксис ссылки
- неразрешенная input reference
- `output.mode = json` без `output.schema`
- `output.mode = json` со schema-корнем, который не описывает object
- `output.mode = json`, вернувший значение не-object
- edges, указывающие на неизвестные ноды

## Перекрестные Ссылки

- Top-level binding-объекты находятся в [top-level-and-bindings-contract.md](./top-level-and-bindings-contract.md).
- Поведение runtime adapter, которое использует нормализованный вызов ноды, находится в [../runtime-adapter-contract.md](../runtime-adapter-contract.md).
