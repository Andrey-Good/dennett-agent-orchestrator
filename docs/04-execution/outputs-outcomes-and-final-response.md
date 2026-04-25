[English](#english) | [Русский](#russian)

<a id="english"></a>
# English

# Outputs, Outcomes, And Final Response

Status: normative.

Related documents:

- [`README.md`](./README.md)
- [`graph-execution.md`](./graph-execution.md)
- [`dataflow-and-input-resolution.md`](./dataflow-and-input-resolution.md)
- [`../05-state/chat-and-resume.md`](../05-state/chat-and-resume.md)
- [`../../agent_orchestrator_final_spec_v2.md`](../../agent_orchestrator_final_spec_v2.md)

## 1. Scope

This document defines:

- what counts as a node output;
- how a node output is validated;
- how a node attempt becomes one node outcome;
- how the run derives the final agent response.

## 2. What A Node Output Is

A node output is only the final output returned by the called agent for that node attempt.

A node output is not:

- chain-of-thought;
- tool-call history;
- live commentary displayed during the run;
- built-in MCP messages to the user;
- hidden reasoning or intermediate runtime events.

Only the final node output participates in graph dataflow.

## 2.1. Child Run Return For `orchestrator_agent`

An `orchestrator_agent` node does not read hidden child internals.

The only payload it may receive from the child run is the child agent's run-level final response after the child applies its own `final_output.mode`.

Mandatory rules:

- if the child run finishes successfully and exposes a final response payload, that payload is the candidate node output of the parent node;
- if the child run finishes successfully but exposes no final response payload, the parent node has no candidate output. This includes child agents whose `final_output.mode = none`;
- the parent node must never read a hidden "last child node output" or any other undeclared child-return channel.

## 3. Declared Output Contract

Every node must declare `output` explicitly.

Mandatory rules:

- `output` is a closed object.
- `output.mode` is required and the only supported values are `text` and `json`.
- `output.schema` is forbidden when `output.mode = text`.
- `output.schema` is required when `output.mode = json`.
- `output.schema` must be a JSON Schema 2020-12 object whose root describes a JSON object.
- There is no implicit default and no hidden rule where missing schema means text output.

## 4. Validation Rules For `output.mode = text`

When `output.mode = text`, the node result is treated as text.

Validation rules:

- the committed result must be a string;
- a JSON object, array, number, boolean, or `null` is invalid for `output.mode = text`;
- if valid, the string is committed as the node output;

On `success`, the committed result may be used as:

- the text output of that node;
- the final agent response when `final_output.mode` chooses it;
- an input source for later nodes through `node.<node_id>.text`.

`output.mode = text` does not automatically update `vars`.

## 5. Validation Rules For `output.mode = json`

When `output.mode = json`, the node result is treated as schema-driven JSON object output.

Validation rules:

- the top-level value must be a JSON object;
- an array, string, number, boolean, or `null` at the top level is invalid;
- the object must satisfy the declared `output.schema`;
- if valid, the object is committed as the node output;
- if valid, its top-level fields are copied into `vars` as defined in [`dataflow-and-input-resolution.md`](./dataflow-and-input-resolution.md).

Core must not secretly rewrite the prompt to force JSON output. If structured JSON is required, the graph author must make that requirement explicit through the `output` contract and supported runtime mechanisms.

## 6. Node Outcome Classification

Each node attempt ends in exactly one of the following outcomes:

- `success`
- `invalid_output`
- `runtime_error`
- `interrupted`
- `cancelled`

The meanings are fixed:

- `success`: the node finished correctly and returned an output valid for the declared `output` contract;
- `invalid_output`: the runtime finished the call, but the final output does not match the declared `output` contract;
- `runtime_error`: the external runtime could not complete the call correctly;
- `interrupted`: execution was stopped by external shutdown of core or interface;
- `cancelled`: execution was explicitly cancelled by the user.

## 7. Commit Rules Per Outcome

Only `success` produces a committed node output.

Commit rules:

- `success` commits the final node output;
- successful `output.mode = json` output also commits the allowed `vars` updates;
- `invalid_output` commits no node output and no `vars` updates;
- `runtime_error` commits no node output and no `vars` updates;
- `interrupted` commits no node output and no `vars` updates;
- `cancelled` commits no node output and no `vars` updates.

The execution layer must never synthesize a fake node output for a non-success outcome.

## 7.1. Outcome Mapping For `orchestrator_agent`

An `orchestrator_agent` node classifies the child boundary through the same standard outcomes.

Mandatory rules:

- if the child run ends in `runtime_error`, the parent node outcome is `runtime_error`;
- if the child run ends in `interrupted`, the parent node outcome is `interrupted`;
- if the child run ends in `cancelled`, the parent node outcome is `cancelled`;
- if the child run ends in `invalid_output`, the parent node outcome is `invalid_output`;
- if the child run ends successfully but provides no final response payload, the parent node outcome is `invalid_output`;
- if the child final response payload exists but fails the parent's declared `output` validation, the parent node outcome is `invalid_output`;
- only a child final response payload that passes the parent's declared validation may commit as the parent node output.

## 8. Secret Markers Are Not Base Output

Secret markers do not create a second public output channel.

Mandatory rules:

- there is no base `secrets` field in the node output contract;
- secret fragments are not copied into `vars`;
- secret fragments are not exposed through `node.<node_id>.text` or `node.<node_id>.json.<path>`;
- secret-fragment handling is governed separately by [`../05-state/secret-markers.md`](../05-state/secret-markers.md).

## 9. Final Agent Response

The final agent response is a run-level presentation rule, not a second node execution.

The supported modes are:

- `last_node_output`
- `none`

### 9.1. `last_node_output`

The final response of the agent equals the output of the last successfully completed node after which the graph terminated.

Consequences:

- if the last successful node produced text, the final response payload is text;
- if the last successful node produced JSON, the final response payload is that JSON object;
- the interaction layer may render that payload differently, but it must not replace it with another semantic result.

### 9.2. `none`

The run has no automatic final user-facing response generated by core.

This does not mean the run has no result. It means only that core does not publish an automatic final chat answer from the execution result.

An agent with `final_output.mode = none` remains a valid top-level contract. When such an agent is called through an `orchestrator_agent` node, the child run has no final-response payload for the call boundary, so the parent node cannot complete with `success`.

## 10. Runs Without A Successful Node

If the run terminates without any successful node, there is no automatic final response, even when `final_output.mode = last_node_output`.

In that case, the caller must rely on run outcome metadata rather than on a synthesized final output.

## 11. Live Messages Are Not Final Output

Messages shown during execution through comment channels or the built-in user-chat MCP are not node outputs and must not be persisted as if they were final node outputs.

They may be visible in chat history under chat-state rules, but they remain non-final interaction artifacts.

## 12. Acceptance Criteria For An Implementation

An implementation conforms to this document only if:

- every node attempt receives exactly one classified outcome;
- only `success` commits a node output;
- an `orchestrator_agent` node reads only the child run's final response payload and treats absent or mismatched payload as `invalid_output`;
- text validation rejects non-string success for `output.mode = text`;
- JSON validation rejects non-object or schema-invalid roots for `output.mode = json`;
- the final response is derived only through `final_output.mode`;
- live messages are not misclassified as final node output.

<a id="russian"></a>
# Русский

# Outputs, outcomes и финальный ответ

Статус: нормативный.

Связанные документы:

- [`README.md`](./README.md)
- [`graph-execution.md`](./graph-execution.md)
- [`dataflow-and-input-resolution.md`](./dataflow-and-input-resolution.md)
- [`../05-state/chat-and-resume.md`](../05-state/chat-and-resume.md)
- [`../../agent_orchestrator_final_spec_v2.md`](../../agent_orchestrator_final_spec_v2.md)

## 1. Область действия

Этот документ определяет:

- что считается node output;
- как node output проходит валидацию;
- как попытка ноды превращается в node outcome;
- как run выводит финальный ответ агента.

## 2. Что такое node output

Node output - это только финальный output, который вызванный агент вернул для данной попытки ноды.

Node output не является:

- chain-of-thought;
- историей tool calls;
- live-комментариями, показанными во время run;
- встроенными MCP-сообщениями пользователю;
- скрытыми рассуждениями или промежуточными runtime events.

Только финальный node output участвует в dataflow графа.

## 2.1. Возврат child run-а для `orchestrator_agent`

Нода `orchestrator_agent` не читает скрытые внутренности дочернего агента.

Единственный payload, который она может получить от child run-а, — это run-level payload финального ответа дочернего агента после того, как child применил собственный `final_output.mode`.

Обязательные правила:

- если child run завершается успешно и публикует payload финального ответа, этот payload становится candidate node output родительской ноды;
- если child run завершается успешно, но не публикует payload финального ответа, у родительской ноды нет candidate output. Сюда входят и дочерние агенты с `final_output.mode = none`;
- родительская нода никогда не должна читать скрытый "last child node output" или любой другой необъявленный child-return channel.

## 3. Объявленный output-контракт

Каждая нода обязана явно объявлять `output`.

Обязательные правила:

- `output` является закрытым объектом.
- `output.mode` обязательно, а поддерживаемые значения только `text` и `json`.
- `output.schema` запрещено при `output.mode = text`.
- `output.schema` обязательно при `output.mode = json`.
- `output.schema` должно быть JSON Schema 2020-12 object, у которого корень описывает JSON object.

Не существует ни неявного значения по умолчанию, ни скрытого правила «нет schema - значит текст».

## 4. Правила валидации для `output.mode = text`

При `output.mode = text` результат ноды трактуется как текст.

Правила валидации:

- зафиксированный результат обязан быть строкой;
- JSON object, массив, число, boolean или `null` являются невалидными для `output.mode = text`;
- если значение валидно, строка фиксируется как node output;

При `success` зафиксированный результат может использоваться как:

- text output этой ноды;
- финальный ответ агента, если его выбирает `final_output.mode`;
- источник входа для последующих нод через `node.<node_id>.text`.

`output.mode = text` не обновляет `vars` автоматически.

## 5. Правила валидации для `output.mode = json`

При `output.mode = json` результат ноды трактуется как schema-driven JSON object output.

Правила валидации:

- значение верхнего уровня обязано быть JSON object;
- массив, строка, число, boolean или `null` на верхнем уровне являются невалидными;
- объект обязан удовлетворять объявленному `output.schema`;
- если значение валидно, object фиксируется как node output;
- если значение валидно, его top-level поля копируются в `vars` по правилам из [`dataflow-and-input-resolution.md`](./dataflow-and-input-resolution.md).

Core не должен тайно переписывать prompt, чтобы заставить ноду вернуть JSON. Если structured JSON требуется, автор графа должен сделать это требование явным через контракт `output` и поддерживаемые runtime-механизмы.

## 6. Классификация outcome ноды

Каждая попытка ноды заканчивается ровно одним из следующих outcomes:

- `success`
- `invalid_output`
- `runtime_error`
- `interrupted`
- `cancelled`

Значения зафиксированы:

- `success`: нода завершилась корректно и вернула output, валидный для объявленного контракта `output`;
- `invalid_output`: runtime завершил вызов, но финальный output не соответствует объявленному контракту `output`;
- `runtime_error`: внешний runtime не смог корректно завершить вызов;
- `interrupted`: исполнение было остановлено внешним shutdown core или интерфейса;
- `cancelled`: исполнение было явно отменено пользователем.

## 7. Правила фиксации по outcome

Только `success` порождает committed node output.

Правила фиксации:

- `success` фиксирует финальный node output;
- успешный output при `output.mode = json` дополнительно фиксирует разрешенные обновления `vars`;
- `invalid_output` не фиксирует ни node output, ни обновления `vars`;
- `runtime_error` не фиксирует ни node output, ни обновления `vars`;
- `interrupted` не фиксирует ни node output, ни обновления `vars`;
- `cancelled` не фиксирует ни node output, ни обновления `vars`.

Execution layer не имеет права синтезировать фиктивный node output для non-success outcome.

## 7.1. Отображение outcomes для `orchestrator_agent`

Нода `orchestrator_agent` классифицирует границу child run-а через те же стандартные outcomes.

Обязательные правила:

- если child run завершается с `runtime_error`, outcome родительской ноды равен `runtime_error`;
- если child run завершается с `interrupted`, outcome родительской ноды равен `interrupted`;
- если child run завершается с `cancelled`, outcome родительской ноды равен `cancelled`;
- если child run завершается с `invalid_output`, outcome родительской ноды равен `invalid_output`;
- если child run завершается успешно, но не предоставляет payload финального ответа, outcome родительской ноды равен `invalid_output`;
- если payload финального ответа child run-а существует, но не проходит валидацию объявленного родительского `output`, outcome родительской ноды равен `invalid_output`;
- только payload финального ответа child run-а, прошедший объявленную родительскую валидацию, может быть зафиксирован как node output родительской ноды.

## 8. Secret markers не являются базовым output

Secret markers не создают второго публичного канала output.

Обязательные правила:

- в базовом контракте node output нет поля `secrets`;
- secret fragments не копируются в `vars`;
- secret fragments не раскрываются через `node.<node_id>.text` или `node.<node_id>.json.<path>`;
- обработка secret fragments регулируется отдельно в [`../05-state/secret-markers.md`](../05-state/secret-markers.md).

## 9. Финальный ответ агента

Финальный ответ агента является run-level правилом представления результата, а не вторым исполнением ноды.

Поддерживаются режимы:

- `last_node_output`
- `none`

### 9.1. `last_node_output`

Финальный ответ агента равен output последней успешно завершившейся ноды, после которой граф остановился.

Следствия:

- если последняя успешная нода выдала текст, payload финального ответа является текстом;
- если последняя успешная нода выдала JSON, payload финального ответа является этим JSON object;
- interaction layer может по-разному рендерить этот payload, но не может заменять его другим семантическим результатом.

### 9.2. `none`

Run не получает автоматически создаваемого core финального пользовательского ответа.

Это не означает, что у run нет результата. Это означает только то, что core не публикует автоматический финальный chat answer из execution result.

Агент с `final_output.mode = none` остается валидным top-level контрактом. Но когда такой агент вызывается через ноду `orchestrator_agent`, у child run-а нет payload финального ответа для этой границы вызова, поэтому родительская нода не может завершиться с `success`.

## 10. Runs без успешной ноды

Если run завершился без единой успешной ноды, автоматического финального ответа нет даже при `final_output.mode = last_node_output`.

В этом случае вызывающая сторона обязана опираться на metadata run outcome, а не на синтезированный final output.

## 11. Live-сообщения не являются финальным output

Сообщения, показываемые во время исполнения через comment channels или встроенный user-chat MCP, не являются node outputs и не могут сохраняться так, будто это финальные outputs нод.

Они могут быть видимыми в chat history по правилам состояния чата, но остаются non-final interaction artifacts.

## 12. Критерии приемки реализации

Реализация соответствует этому документу только если:

- каждая попытка ноды получает ровно один классифицированный outcome;
- только `success` фиксирует node output;
- нода `orchestrator_agent` читает только payload финального ответа child run-а и трактует отсутствующий или несовместимый payload как `invalid_output`;
- валидация `text` отвергает не-строковый `success` для `output.mode = text`;
- JSON validation отвергает не-object или schema-invalid roots для `output.mode = json`;
- финальный ответ выводится только через `final_output.mode`;
- live-сообщения не классифицируются ошибочно как финальный node output.
