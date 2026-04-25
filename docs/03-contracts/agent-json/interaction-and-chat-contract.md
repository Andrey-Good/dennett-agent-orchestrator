[English](#english) | [Русский](#russian)

<a id="english"></a>
# Interaction And Chat Contract

## Purpose and Ownership

This document owns the top-level `interaction` and `chat` objects in the portable agent file. It also owns the routing rule between run-time comments and the built-in user-chat MCP.

This document does not own the built-in MCP payload shape itself. That lives in [../orchestrator-user-chat-mcp-contract.md](../orchestrator-user-chat-mcp-contract.md).

## `interaction`

- Type: `object`
- Required: no
- Closed: yes

Allowed fields:

- `comments`: optional closed object
- `user_mcp`: optional closed object

If `interaction` is absent, both features default to disabled.

## `interaction.comments`

Allowed fields:

- `enabled`: required boolean when `comments` is present
- `target_node_ids`: required non-empty array of strings when `enabled = true`

Rules:

- `target_node_ids` must not contain duplicates.
- Every ID in `target_node_ids` must resolve to an existing `runtime_agent` node.
- Comments are delivered only to the currently active node, and only when that active node appears in `target_node_ids`.
- If comments are enabled for a node whose runtime adapter does not support live comments, comment delivery for that node is unavailable and comment injection must be rejected explicitly.

## `interaction.user_mcp`

Allowed fields:

- `enabled`: required boolean when `user_mcp` is present
- `server_name`: optional string. The effective default is `orchestrator.user_chat`.

Rules:

- In the current contract version, `server_name`, when present, must equal `orchestrator.user_chat`.
- `enabled = true` grants the active runtime node access to the built-in system MCP during the run.
- A run that enables `user_mcp` on an adapter without built-in user-chat MCP support is invalid.

## Interaction Routing Rule

The system supports two independent interaction channels during a run:

- user comments injected into the active runtime session
- built-in MCP prompts and responses through `orchestrator.user_chat`

When both are enabled:

- an explicit response to a pending MCP prompt is routed to the MCP channel
- any other free-form user message during the run is treated as a comment

Free-form text must not be auto-interpreted as an MCP response without explicit prompt binding.

## Nested `orchestrator_agent` Runs

These interaction rules apply to one active run at a time.

When a parent node launches a child run through `orchestrator_agent` in the base model:

- the child run's live comments must not be surfaced through the parent run;
- the child run's `orchestrator.user_chat` traffic must not be surfaced through the parent run;
- if the selected child revision depends on nested surfaced live interaction, the launch is invalid and must be rejected before the child run starts.

## `chat`

- Type: `object`
- Required: no
- Closed: yes

Allowed fields:

- `prefer_native_resume`: optional boolean. Effective default is `true`.
- `store_visible_messages`: optional boolean. Effective default is `true`.
- `store_context_window`: optional boolean. Effective default is `true`.
- `allow_fresh_start`: optional boolean. Effective default is `true`.
- `secret_markers`: optional closed object.

## Chat Policy Rules

- Resume is always explicit. The system must not auto-resume after a restart.
- `prefer_native_resume = true` means native resume should be attempted first when a compatible native session handle exists.
- `prefer_native_resume = false` means the orchestrator may choose local resume first even when native resume is available.
- `store_visible_messages = true` means visible user and agent messages should be persisted as part of chat state.
- `store_visible_messages = false` does not forbid storing technical resume metadata; it only disables persistence of the visible transcript by policy.
- `store_context_window = true` means the effective visible context window should be stored for local resume and user experience.
- `store_context_window = false` permits omitting that stored context window, which may reduce local-resume fidelity.
- `allow_fresh_start = true` means a user may explicitly abandon resume data and start a fresh run.
- `allow_fresh_start = false` means interfaces must not silently replace an available resume path with a fresh start.

## `chat.secret_markers`

`secret_markers` is a closed object with these fields:

- `enabled`: required boolean when `secret_markers` is present
- `open_marker`: optional string. Effective default is `[[SECRET]]`
- `close_marker`: optional string. Effective default is `[[/SECRET]]`

Rules:

- `open_marker` and `close_marker` must be non-empty and must not be equal.
- Secret markers are optional and disabled by default.
- Secret content is not part of the base node output contract.
- When enabled, text between the markers is removed from stored visible text, encrypted, and stored separately.
- Secret fragments may be restored only as part of an explicit resume path.

## What Chat State Must Preserve

The source of truth for chat and resume state is local core storage, not the portable agent file. The policy owned by this document says chat state may preserve:

- user messages
- visible agent replies
- the effective context window, when enabled
- selected runtime information
- run parameters that affect resume
- native session handles, when supported
- local fallback data needed for local resume

## What Chat State Must Not Preserve By Default

By default, chat state must not become a full forensic archive. It must not store:

- chain-of-thought
- internal tool traces
- history of every internal runtime action
- file-change history
- hidden intermediate reasoning

## Invalid Cases

The following are contract violations:

- unknown fields inside `interaction`, `comments`, `user_mcp`, `chat`, or `secret_markers`
- `comments.enabled = true` without a non-empty `target_node_ids`
- duplicate or unknown IDs in `target_node_ids`
- a `target_node_ids` entry that points to a non-`runtime_agent` node
- `user_mcp.server_name` with a value other than `orchestrator.user_chat`
- empty or equal secret markers

## Cross-Links

- Built-in MCP payloads live in [../orchestrator-user-chat-mcp-contract.md](../orchestrator-user-chat-mcp-contract.md).
- Runtime support obligations live in [../runtime-adapter-contract.md](../runtime-adapter-contract.md).

<a id="russian"></a>
# Контракт Interaction И Chat

## Назначение И Владение

Этот документ владеет top-level объектами `interaction` и `chat` в переносимом agent file. Он также владеет правилом маршрутизации между комментариями во время run и встроенным user-chat MCP.

Этот документ не владеет самой формой payload встроенного MCP. Ею владеет [../orchestrator-user-chat-mcp-contract.md](../orchestrator-user-chat-mcp-contract.md).

## `interaction`

- Тип: `object`
- Обязательное: нет
- Закрытый объект: да

Допустимые поля:

- `comments`: необязательный закрытый объект
- `user_mcp`: необязательный закрытый объект

Если `interaction` отсутствует, обе возможности считаются выключенными по умолчанию.

## `interaction.comments`

Допустимые поля:

- `enabled`: обязательный boolean, если `comments` присутствует
- `target_node_ids`: обязательный непустой массив строк, если `enabled = true`

Правила:

- `target_node_ids` не должен содержать дубликатов.
- Каждый ID в `target_node_ids` обязан разрешаться в существующую ноду `runtime_agent`.
- Комментарии доставляются только активной в данный момент ноде и только если эта активная нода входит в `target_node_ids`.
- Если комментарии разрешены для ноды, чей runtime adapter не поддерживает live comments, доставка комментариев для этой ноды недоступна и попытка внедрения комментария должна явно отклоняться.

## `interaction.user_mcp`

Допустимые поля:

- `enabled`: обязательный boolean, если `user_mcp` присутствует
- `server_name`: необязательная строка. Эффективное значение по умолчанию: `orchestrator.user_chat`.

Правила:

- В текущей версии контракта `server_name`, если оно указано, обязано быть равно `orchestrator.user_chat`.
- `enabled = true` дает активной runtime-ноде доступ к встроенному системному MCP во время run.
- Run, который включает `user_mcp` на адаптере без поддержки built-in user-chat MCP, является невалидным.

## Правило Маршрутизации Взаимодействия

Во время run система поддерживает два независимых канала взаимодействия:

- пользовательские комментарии, внедряемые в активную runtime-сессию
- встроенные MCP-вопросы и ответы через `orchestrator.user_chat`

Когда включены оба канала:

- явный ответ на ожидающий MCP-prompt маршрутизируется в MCP-канал
- любое другое свободное пользовательское сообщение во время run трактуется как комментарий

Свободный текст не должен автоматически интерпретироваться как MCP-ответ без явной привязки к prompt.

## `chat`

- Тип: `object`
- Обязательное: нет
- Закрытый объект: да

Допустимые поля:

- `prefer_native_resume`: необязательный boolean. Эффективное значение по умолчанию: `true`.
- `store_visible_messages`: необязательный boolean. Эффективное значение по умолчанию: `true`.
- `store_context_window`: необязательный boolean. Эффективное значение по умолчанию: `true`.
- `allow_fresh_start`: необязательный boolean. Эффективное значение по умолчанию: `true`.
- `secret_markers`: необязательный закрытый объект.

## Правила Chat Policy

- Resume всегда является явным. Система не должна выполнять auto-resume после перезапуска.
- `prefer_native_resume = true` означает, что native resume следует пробовать первым, если существует совместимый native session handle.
- `prefer_native_resume = false` означает, что оркестратор может предпочесть local resume даже при наличии native resume.
- `store_visible_messages = true` означает, что видимые сообщения пользователя и агента должны сохраняться как часть chat state.
- `store_visible_messages = false` не запрещает хранить технические resume-метаданные; это только отключает сохранение видимого транскрипта на уровне политики.
- `store_context_window = true` означает, что эффективное видимое контекстное окно должно сохраняться для local resume и пользовательского опыта.
- `store_context_window = false` разрешает не хранить это контекстное окно, что может снизить точность local resume.
- `allow_fresh_start = true` означает, что пользователь может явно отказаться от resume-данных и начать новый run.
- `allow_fresh_start = false` означает, что интерфейсы не должны молча заменять доступный путь resume на fresh start.

## `chat.secret_markers`

`secret_markers` является закрытым объектом со следующими полями:

- `enabled`: обязательный boolean, если `secret_markers` присутствует
- `open_marker`: необязательная строка. Эффективное значение по умолчанию: `[[SECRET]]`
- `close_marker`: необязательная строка. Эффективное значение по умолчанию: `[[/SECRET]]`

Правила:

- `open_marker` и `close_marker` обязаны быть непустыми и не должны совпадать.
- Secret markers являются опциональным механизмом и выключены по умолчанию.
- Secret content не входит в базовый контракт node output.
- Если механизм включен, текст между markers вырезается из сохраняемого видимого текста, шифруется и хранится отдельно.
- Secret fragments могут восстанавливаться только в рамках явного пути resume.

## Что Chat State Обязан Сохранять

Источником истины для chat и resume state является локальное core storage, а не переносимый agent file. Политика, которой владеет этот документ, говорит, что chat state может сохранять:

- пользовательские сообщения
- видимые ответы агента
- эффективное контекстное окно, если оно включено
- сведения о выбранном runtime
- параметры запуска, влияющие на resume
- native session handles, если они поддерживаются
- local fallback-данные, нужные для local resume

## Что Chat State Не Должен Сохранять По Умолчанию

По умолчанию chat state не должен превращаться в полный форензик-архив. Он не должен хранить:

- chain-of-thought
- внутренние tool traces
- историю каждого внутреннего runtime-действия
- историю изменений файлов
- скрытые промежуточные рассуждения

## Невалидные Случаи

Нарушениями контракта являются:

- неизвестные поля внутри `interaction`, `comments`, `user_mcp`, `chat` или `secret_markers`
- `comments.enabled = true` без непустого `target_node_ids`
- дубликаты или неизвестные IDs в `target_node_ids`
- элемент `target_node_ids`, указывающий на ноду, не являющуюся `runtime_agent`
- `user_mcp.server_name` со значением, отличным от `orchestrator.user_chat`
- пустые или совпадающие secret markers

## Перекрестные Ссылки

- Built-in MCP payloads находятся в [../orchestrator-user-chat-mcp-contract.md](../orchestrator-user-chat-mcp-contract.md).
- Обязанности runtime по поддержке этих возможностей находятся в [../runtime-adapter-contract.md](../runtime-adapter-contract.md).
