[English](#english) | [Русский](#russian)

<a id="english"></a>
# English

# Secret Markers

Status: normative.

Related documents:

- [`README.md`](./README.md)
- [`chat-and-resume.md`](./chat-and-resume.md)
- [`local-storage-model.md`](./local-storage-model.md)
- [`../04-execution/outputs-outcomes-and-final-response.md`](../04-execution/outputs-outcomes-and-final-response.md)
- [`../../agent_orchestrator_final_spec_v2.md`](../../agent_orchestrator_final_spec_v2.md)

## 1. Scope

This document defines the optional secret-marker mechanism referenced by the canonical spec.

It defines:

- when the feature is active;
- what gets parsed;
- what becomes visible transcript;
- what gets stored separately;
- what may be restored during explicit resume.

## 2. Feature Gating

The mechanism is disabled by default.

It is active only when `chat.secret_markers.enabled = true`.

When disabled:

- marker text has no special meaning;
- no extraction occurs;
- ordinary output rules apply unchanged.

## 3. Public Output Contract Does Not Change

Secret markers do not add a public `secrets` field to node output.

Mandatory rules:

- the base output contract remains `text` or `json`;
- secret fragments are not a public graph data channel;
- secret fragments do not become `vars`;
- secret fragments do not become readable through `node.<node_id>.*` references.

## 4. Marker Configuration

The marker configuration is:

- `open_marker`
- `close_marker`

The canonical defaults are:

- `[[SECRET]]`
- `[[/SECRET]]`

These markers are literal text delimiters, not JSON structure.

## 5. When Parsing Happens

Parsing happens after a user-visible assistant message or text output exists and before the visible version of that content is committed to chat state.

The parser must run only on text content governed by the chat secret-marker policy.

It must not attempt to parse JSON fields recursively.

## 6. Parsing Model

This document fixes the parsing model so implementations do not improvise it:

- parsing is left-to-right;
- markers are non-nesting;
- each `open_marker` must match the next `close_marker`;
- marker tokens themselves are removed from the visible persisted text.

If no complete marker pair exists, no extraction occurs for that span.

## 7. Malformed Marker Handling

Malformed marker structure is security-sensitive, so the implementation must fail closed.

Malformed structure includes:

- an `open_marker` without a later `close_marker`;
- a `close_marker` without a preceding unmatched `open_marker`;
- a nested `open_marker` before the current span closes.

For malformed structure in a message governed by enabled secret markers:

- the raw marked text must not be committed into visible transcript storage;
- the raw message body must be stored only as encrypted secret side data with a parse-error status;
- the visible stored message must become the exact placeholder `[SECRET_REDACTED]`.

This rule prevents accidental secret leakage through partial parsing.

## 8. Successful Extraction

For well-formed marker pairs:

- the enclosed fragment is extracted as secret side data;
- the enclosed fragment is removed from the visible stored text;
- the markers themselves are removed from the visible stored text;
- no automatic replacement token is inserted into the visible stored text;
- the extracted fragment must be stored encrypted and separately from visible transcript.

The visible stored result is therefore the surrounding text with the secret spans cut out.

## 9. Restore Semantics

Secret fragments may be restored only for explicit resume.

Mandatory rules:

- restoration is for runtime-facing continuation context, not for ordinary visible transcript by default;
- restoration must use the original span order for the message;
- if secret side data is unavailable, the visible redacted text remains authoritative for display and the resume layer must treat missing secret restoration as unavailable secret context, not as a license to invent it.

## 10. Storage Requirements

Secret side data must:

- remain separate from visible chat transcript and public node outputs;
- be encrypted at rest;
- be linked to the owning chat/run/message or node-attempt identity;
- record whether extraction succeeded or failed with malformed markers.

The local storage model may choose any concrete schema, but it must preserve these semantics.

## 11. Relationship To Final Output

When secret markers are enabled, the persisted visible final answer may differ from the raw runtime text because secret spans are removed.

For base execution semantics:

- the public committed node output is the redacted visible form;
- secret side data exists only for protected persistence and explicit resume restoration;
- graph dataflow must not consume hidden secret side data.

## 12. Acceptance Criteria For An Implementation

An implementation conforms to this document only if:

- the feature is off by default;
- enabled parsing is deterministic and non-nesting;
- malformed markers fail closed into `[SECRET_REDACTED]`;
- successful extraction stores secrets separately and encrypted;
- secret fragments are restorable only for explicit resume and not as public graph data.

<a id="russian"></a>
# Русский

# Secret markers

Статус: нормативный.

Связанные документы:

- [`README.md`](./README.md)
- [`chat-and-resume.md`](./chat-and-resume.md)
- [`local-storage-model.md`](./local-storage-model.md)
- [`../04-execution/outputs-outcomes-and-final-response.md`](../04-execution/outputs-outcomes-and-final-response.md)
- [`../../agent_orchestrator_final_spec_v2.md`](../../agent_orchestrator_final_spec_v2.md)

## 1. Область действия

Этот документ определяет optional-механизм secret markers, на который ссылается канонический spec.

Он задает:

- когда функция активна;
- что именно парсится;
- что становится видимым transcript;
- что сохраняется отдельно;
- что может быть восстановлено при explicit resume.

## 2. Включение функции

Механизм выключен по умолчанию.

Он активен только когда `chat.secret_markers.enabled = true`.

Когда функция выключена:

- marker text не имеет специального значения;
- извлечение не выполняется;
- обычные правила output применяются без изменений.

## 3. Публичный контракт output не меняется

Secret markers не добавляют публичное поле `secrets` в node output.

Обязательные правила:

- базовый контракт output остается `text` или `json`;
- secret fragments не являются публичным каналом данных графа;
- secret fragments не становятся `vars`;
- secret fragments не становятся читаемыми через ссылки `node.<node_id>.*`.

## 4. Конфигурация markers

Конфигурация markers состоит из:

- `open_marker`
- `close_marker`

Канонические значения по умолчанию:

- `[[SECRET]]`
- `[[/SECRET]]`

Эти markers являются literal text delimiters, а не JSON-структурой.

## 5. Когда происходит parsing

Parsing происходит после того, как появился user-visible assistant message или text output, и до того, как видимая версия этого контента будет committed в chat state.

Parser должен работать только на text content, подпадающем под policy secret markers для чата.

Он не должен пытаться рекурсивно парсить JSON-поля.

## 6. Модель parsing

Этот документ фиксирует модель parsing, чтобы реализации не импровизировали ее:

- parsing идет слева направо;
- markers не допускают вложенности;
- каждый `open_marker` обязан матчиться с ближайшим следующим `close_marker`;
- сами marker tokens удаляются из видимого сохраняемого текста.

Если полной marker pair нет, извлечение для этого span не происходит.

## 7. Обработка malformed markers

Malformed marker structure влияет на безопасность, поэтому реализация обязана fail closed.

Malformed structure включает:

- `open_marker` без последующего `close_marker`;
- `close_marker` без предыдущего unmatched `open_marker`;
- вложенный `open_marker` до закрытия текущего span.

Для malformed structure в сообщении, подпадающем под enabled secret markers:

- raw marked text нельзя commit-ить в visible transcript storage;
- raw message body должен сохраняться только как encrypted secret side data со статусом parse-error;
- видимое сохраненное сообщение должно стать точным placeholder `[SECRET_REDACTED]`.

Это правило предотвращает случайную утечку секретов через частичный parsing.

## 8. Успешное извлечение

Для корректных marker pairs:

- заключенный фрагмент извлекается как secret side data;
- заключенный фрагмент удаляется из visible stored text;
- сами markers удаляются из visible stored text;
- автоматический replacement token в visible stored text не вставляется;
- извлеченный фрагмент обязан сохраняться отдельно и в зашифрованном виде.

Следовательно, видимый сохраненный результат - это окружающий текст с вырезанными secret spans.

## 9. Семантика восстановления

Secret fragments могут восстанавливаться только для explicit resume.

Обязательные правила:

- восстановление предназначено для runtime-facing continuation context, а не для обычного visible transcript по умолчанию;
- восстановление обязано соблюдать исходный порядок spans внутри сообщения;
- если secret side data недоступны, видимый redacted text остается authoritative для display, а слой resume обязан трактовать отсутствие восстановления как недоступный secret context, а не как право его придумать.

## 10. Требования к хранению

Secret side data обязаны:

- быть отделены от visible chat transcript и публичных node outputs;
- быть зашифрованы при хранении;
- быть привязаны к owning chat/run/message или node-attempt identity;
- фиксировать, прошло ли извлечение успешно или завершилось malformed markers.

Локальная модель хранения может выбирать любую конкретную схему, но обязана сохранять эту семантику.

## 11. Связь с финальным output

Когда secret markers включены, persisted visible final answer может отличаться от raw runtime text, потому что secret spans удаляются.

Для базовой семантики исполнения:

- публичный committed node output - это redacted visible form;
- secret side data существуют только для защищенной персистентности и explicit resume restoration;
- graph dataflow не может потреблять скрытые secret side data.

## 12. Критерии приемки реализации

Реализация соответствует этому документу только если:

- функция выключена по умолчанию;
- enabled parsing детерминирован и не поддерживает вложенность;
- malformed markers fail closed в `[SECRET_REDACTED]`;
- успешное извлечение сохраняет секреты отдельно и в зашифрованном виде;
- secret fragments восстанавливаются только для explicit resume и не становятся публичными данными графа.
