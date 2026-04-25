[English](#english) | [Русский](#russian)

<a id="english"></a>
# Built-in MCP Contract: `orchestrator.user_chat`

## Purpose and Ownership

This document owns the only MCP contract defined by this repository: the built-in system MCP used by a running agent to contact the user. It owns server identity, payload shape, routing invariants, and invalid cases.

This document does not prescribe concrete MCP transport names such as tool names or resource names. It is normative about payload objects and behavior, not about one specific SDK surface.

## Availability

- The canonical server name is `orchestrator.user_chat`.
- The server is available only when `interaction.user_mcp.enabled = true` for the active run.
- In the current contract version, `interaction.user_mcp.server_name`, when present, MUST equal `orchestrator.user_chat`.
- The active runtime adapter must support built-in user-chat MCP exposure. Otherwise the run configuration is invalid.
- Messages sent through this MCP are intermediate run-time messages. They are not the agent's final output.

## Agent-to-User Request Payload

The outbound payload is a closed object with these fields:

- `prompt_id`: optional string. Stable identifier of the prompt instance.
- `kind`: required enum. Allowed values are `text` and `options`.
- `text`: required string. Human-readable message shown to the user.
- `require_response`: required boolean. Tells the orchestrator whether the active node must wait for a user reply.
- `options`: optional array. Allowed only when `kind = options`.

Each `options[]` item is a closed object with these fields:

- `id`: required string. Must be unique within the `options` array.
- `label`: required string. User-visible label.
- `value`: required JSON literal. Value returned back to the runtime when that option is chosen.

## Request Invariants

- `kind = text` means `options` must be absent.
- `kind = options` means `options` must be present and non-empty.
- `text` is required for both `kind` values. Ready-made options do not replace explanatory text.
- If multiple unresolved `require_response = true` prompts can exist at the same time, each one MUST carry a unique `prompt_id`.
- `require_response = false` means the runtime may continue immediately after emission.
- `require_response = true` means the active node must wait until the prompt is answered, the run is cancelled, or the run is interrupted.

## User-to-Agent Response Payload

The inbound payload is a closed object with these fields:

- `prompt_id`: optional string.
- `kind`: required enum. Allowed values are `text` and `option`.
- `text`: required string when `kind = text`; forbidden when `kind = option`.
- `option_id`: required string when `kind = option`; forbidden when `kind = text`.
- `value`: required JSON literal when `kind = option`; optional and normally absent when `kind = text`.

## Response Invariants

- If the original request carried `prompt_id`, the response to that request MUST echo the same `prompt_id`.
- If multiple unresolved prompts exist, `prompt_id` becomes mandatory in practice and the orchestrator must reject ambiguous responses.
- `kind = option` is valid only as a response to a request whose `kind = options`.
- `option_id` must match one of the option IDs from the original unresolved request.
- `value` in an option response must equal the selected option's declared `value`; the interface must not substitute a different payload.

## Routing Rules

- A built-in user-chat response is not the same thing as a general user comment during a run.
- If a run has an active unresolved MCP prompt and the user answers through the explicit prompt-response action, the reply goes to this MCP channel.
- Any other free-form user message during the run is treated as a run-time comment, not as an MCP reply.
- Free-form text must not be auto-bound to an MCP prompt unless the interface explicitly ties the message to that prompt.

## Invalid Cases

The orchestrator must reject the following as contract violations:

- Unknown fields in the request or response payload.
- `kind = options` with a missing or empty `options` array.
- Duplicate option IDs inside one request.
- A response to a non-existent, already resolved, or mismatched prompt.
- `kind = option` with an `option_id` not present in the original request.
- A text response that attempts to answer an options prompt as if it were a selected option.
- Automatic routing of arbitrary run-time text into this MCP channel without explicit prompt binding.

## Cross-Links

- Enablement lives in [agent-json/interaction-and-chat-contract.md](./agent-json/interaction-and-chat-contract.md).
- Runtime exposure obligations live in [runtime-adapter-contract.md](./runtime-adapter-contract.md).

<a id="russian"></a>
# Контракт Встроенного MCP: `orchestrator.user_chat`

## Назначение И Владение

Этот документ владеет единственным MCP-контрактом, который определяется данным репозиторием: встроенным системным MCP, через который работающий агент может обратиться к пользователю. Документ владеет именем сервера, формой payload, инвариантами маршрутизации и невалидными случаями.

Этот документ не предписывает конкретные MCP transport names, например имена tools или resources. Он нормативен относительно payload-объектов и поведения, а не относительно одной конкретной SDK-поверхности.

## Доступность

- Каноническое имя сервера: `orchestrator.user_chat`.
- Сервер доступен только если для активного run установлено `interaction.user_mcp.enabled = true`.
- В текущей версии контракта `interaction.user_mcp.server_name`, если поле указано, обязано быть равно `orchestrator.user_chat`.
- Активный runtime adapter обязан поддерживать открытие built-in user-chat MCP. Иначе конфигурация run является невалидной.
- Сообщения, отправленные через этот MCP, являются промежуточными сообщениями run. Они не являются финальным output агента.

## Payload Запроса От Агента К Пользователю

Исходящий payload является закрытым объектом со следующими полями:

- `prompt_id`: необязательная строка. Стабильный идентификатор экземпляра вопроса.
- `kind`: обязательный enum. Допустимые значения: `text` и `options`.
- `text`: обязательная строка. Понятное человеку сообщение, показываемое пользователю.
- `require_response`: обязательный boolean. Указывает оркестратору, должна ли активная нода ждать ответ пользователя.
- `options`: необязательный массив. Допустим только при `kind = options`.

Каждый элемент `options[]` является закрытым объектом со следующими полями:

- `id`: обязательная строка. Должна быть уникальной внутри массива `options`.
- `label`: обязательная строка. Отображаемая пользователю подпись.
- `value`: обязательный JSON literal. Значение, которое возвращается обратно в runtime при выборе варианта.

## Инварианты Запроса

- `kind = text` означает, что `options` должно отсутствовать.
- `kind = options` означает, что `options` обязано присутствовать и быть непустым.
- `text` обязателен для обоих значений `kind`. Готовые варианты ответа не заменяют поясняющий текст.
- Если одновременно могут существовать несколько неразрешенных prompt с `require_response = true`, каждый из них обязан иметь уникальный `prompt_id`.
- `require_response = false` означает, что runtime может продолжить выполнение сразу после отправки сообщения.
- `require_response = true` означает, что активная нода обязана ждать ответа, отмены run или его внешнего прерывания.

## Payload Ответа От Пользователя К Агенту

Входящий payload является закрытым объектом со следующими полями:

- `prompt_id`: необязательная строка.
- `kind`: обязательный enum. Допустимые значения: `text` и `option`.
- `text`: обязательная строка при `kind = text`; запрещена при `kind = option`.
- `option_id`: обязательная строка при `kind = option`; запрещена при `kind = text`.
- `value`: обязательный JSON literal при `kind = option`; необязателен и обычно отсутствует при `kind = text`.

## Инварианты Ответа

- Если исходный запрос содержал `prompt_id`, ответ на этот запрос обязан повторить тот же `prompt_id`.
- Если одновременно существуют несколько неразрешенных prompt, `prompt_id` фактически становится обязательным, и оркестратор обязан отклонять неоднозначные ответы.
- `kind = option` допустим только как ответ на запрос, у которого `kind = options`.
- `option_id` обязан совпадать с одним из идентификаторов варианта в исходном неразрешенном запросе.
- `value` в option-ответе обязано совпадать с объявленным `value` выбранного варианта; интерфейс не должен подменять payload.

## Правила Маршрутизации

- Ответ через built-in user-chat не равен обычному пользовательскому комментарию во время run.
- Если у run есть активный неразрешенный MCP-prompt и пользователь отвечает через явное действие интерфейса для ответа на prompt, ответ уходит в этот MCP-канал.
- Любое другое свободное пользовательское сообщение во время run трактуется как run-time comment, а не как MCP-ответ.
- Свободный текст не должен автоматически привязываться к MCP-prompt без явного связывания сообщения с этим prompt на стороне интерфейса.

## Невалидные Случаи

Оркестратор обязан отклонять следующие нарушения контракта:

- Неописанные поля в request или response payload.
- `kind = options` без массива `options` или с пустым массивом.
- Дублирующиеся option IDs внутри одного запроса.
- Ответ на несуществующий, уже закрытый или несовпадающий prompt.
- `kind = option` с `option_id`, которого не было в исходном запросе.
- Текстовый ответ, который пытается отвечать на options-prompt как на выбранный вариант.
- Автоматическую маршрутизацию произвольного run-time текста в этот MCP-канал без явной привязки к prompt.

## Перекрестные Ссылки

- Включение и верхнеуровневая конфигурация находятся в [agent-json/interaction-and-chat-contract.md](./agent-json/interaction-and-chat-contract.md).
- Обязанности runtime adapter по открытию этого MCP находятся в [runtime-adapter-contract.md](./runtime-adapter-contract.md).
