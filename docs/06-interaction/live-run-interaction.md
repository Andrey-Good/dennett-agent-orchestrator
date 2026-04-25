[English](#english) | [Русский](#russian)

<a id="english"></a>
# English

This document defines behavioral rules for live interaction while a run is active. It expands the canonical model from [the root specification](../../agent_orchestrator_final_spec_v2.md), sections `22` through `26`, without restating field-level payload contracts from [Contracts](../03-contracts/README.md).

## 1. Scope

These rules apply only while a run is active. They govern:

- user comments sent into an already running node;
- the built-in user-chat MCP channel exposed by the orchestrator to the active node;
- routing of user input when both mechanisms are enabled at the same time.

These rules do not define JSON payloads, schema validation, or storage formats.
They also do not create a second surfaced interaction surface for nested child runs launched through `orchestrator_agent`; in the base model, those child-run interactions remain internal to that child run.

## 2. Active-run interaction model

The interaction model is sequential:

- only one node is active at a time;
- live user input is always evaluated against that current active node;
- interaction rights change as the active node changes;
- when the run finishes, is interrupted, or is cancelled, live interaction for that run stops immediately.

A future interface may reconnect to an existing active run, but the interaction semantics remain attached to the run held by core, not to the lifetime of one CLI or UI process. Close-policy details remain part of the canonical spec and architecture boundary.

## 3. Enablement and defaults

The orchestrator exposes two independent live channels:

- user comments;
- the built-in user-chat MCP channel.

Behavioral rules:

- both channels are disabled by default;
- enabling one channel does not enable the other;
- comment delivery is node-specific;
- the built-in user-chat MCP channel, when enabled, is available to the current active node as a system capability of the orchestrator.
- a child run launched through `orchestrator_agent` does not surface its live comments or built-in user-chat channel through the parent run.

Formal configuration fields for `interaction` remain contract-owned in [Interaction And Chat Contract](../03-contracts/agent-json/interaction-and-chat-contract.md).

## 4. User comments

A user comment is free-form user input sent during an active run that is not an explicit reply to a built-in user-chat prompt.

Behavioral rules:

- a comment may be accepted only while a run is active;
- a comment may be delivered only if the current active node is one of the nodes allowed to receive comments;
- if the current active node is not allowed to receive comments, the comment must be rejected rather than queued for a later node;
- comment eligibility is recalculated every time the active node changes.

Delivery rules:

- core must use the runtime's native live-comment mechanism when that mechanism exists;
- core must not invent a separate orchestrator-only injection path just to emulate comments;
- if the active runtime cannot deliver live comments for the current node, comments are unavailable for that node even if the agent configuration would otherwise allow them.

A comment is advisory input into an ongoing run. It is not the final answer and it is not an answer to a pending built-in user-chat prompt unless the user takes an explicit prompt-reply action.

## 5. Built-in user-chat MCP channel

The orchestrator provides a system MCP channel for mid-run communication between the active node and the user.

Behavioral rules:

- the built-in user-chat channel is available only when it is enabled for the agent;
- messages sent through this channel are intermediate run messages, not the agent's final answer;
- the active node may use this channel either to inform the user or to ask for a reply;
- a message that does not require a reply must not pause run execution;
- a message that requires a reply blocks the active node until the user provides an explicit reply or the run stops for another reason.

This channel is distinct from free-form comments. It exists so that the active node can deliberately open a mid-run interaction with the user instead of passively receiving comments.

The exact wire shape of prompts and replies is owned by [Built-in MCP Contract: `orchestrator.user_chat`](../03-contracts/orchestrator-user-chat-mcp-contract.md).

## 6. Routing precedence while a run is active

When comments and built-in user chat are both enabled, routing must follow one rule set:

1. If there is an active pending built-in user-chat prompt and the user uses an explicit reply action bound to that prompt, the input goes to the built-in user-chat channel.
2. Any other user input sent during the active run is treated as a comment.

Required consequences:

- free-form text in the main composer must never be auto-bound to a pending built-in user-chat prompt;
- the existence of a pending prompt does not redefine the generic text channel;
- if comments are enabled and deliverable for the current node, non-reply user text remains a comment even while a prompt is pending;
- if comments are not enabled or cannot be delivered for the current node, that non-reply input must be rejected instead of being silently converted into a prompt reply.

## 7. Blocking and unblocking semantics

A required built-in user-chat reply changes run behavior in one specific way: it stops forward progress of the current active node until the reply arrives.

The interaction layer must preserve the following distinctions:

- informational built-in user-chat messages are non-blocking;
- required built-in user-chat prompts are blocking;
- comments do not satisfy blocking prompts unless they are explicitly submitted through the prompt-reply path;
- once the run is no longer active, any previously pending prompt becomes inactive and must no longer accept replies.

How the blocked state is stored or restored belongs to [State](../05-state/README.md). How execution resumes after the reply belongs to [Execution](../04-execution/README.md).

## 8. Richer App Server Event Taxonomy

For the Codex path, the App Server emits a much richer notification stream than the two live user channels owned by this document.

Verified notification families include:

- thread and turn lifecycle updates;
- item lifecycle, agent-message, plan, command-execution, file-change, and MCP-progress updates;
- model-reroute, token-usage, account and rate-limit, config-warning, app-list, filesystem-change, and MCP-server-status notifications;
- optional reasoning-summary, raw-response-item, auto-approval-review, and realtime audio or transcript notifications.

Boundary and staging rules:

- these notifications are Codex-specific runtime events, not portable agent-file semantics;
- safe current-stage use is read-only local presentation, diagnostics, or adapter-internal state mapping when another owner document already defines the behavior being realized;
- exposing them as first-class user-facing run surfaces beyond comments and built-in user chat is later-stage unless a more specific owner document defines meaning, persistence, and failure behavior;
- reasoning-summary events may be surfaced only under explicit product policy, while raw reasoning text deltas and raw response items must not silently become mandatory persisted history or final-answer content;
- review, realtime, or detached-thread notifications must not create new user-input routes by implication.

## 9. Negative rules

The following behaviors are forbidden:

- auto-promoting arbitrary user text to a built-in user-chat reply;
- queueing comments for some future node that has not become active yet;
- treating built-in user-chat messages as the agent's final answer;
- fabricating comment support when the runtime does not provide a real live-comment path.

## 10. Related documents

- [Presentation Rules](./presentation-rules.md)
- [Architecture](../02-architecture/README.md)
- [Execution](../04-execution/README.md)
- [State](../05-state/README.md)
- [Contracts](../03-contracts/README.md)

<a id="russian"></a>
# Русский

Этот документ задаёт поведенческие правила live-взаимодействия во время активного run. Он разворачивает каноническую модель из [корневой спецификации](../../agent_orchestrator_final_spec_v2.md), разделы `22`-`26`, не повторяя payload-контракты и схемы из [Contracts](../03-contracts/README.md).

## 1. Область действия

Эти правила действуют только пока run активен. Они регулируют:

- пользовательские комментарии, отправляемые в уже выполняющуюся ноду;
- встроенный user-chat MCP-канал, который оркестратор открывает активной ноде;
- маршрутизацию пользовательского ввода, когда оба механизма включены одновременно.

Эти правила не определяют JSON payload, schema validation и форматы хранения.

## 2. Модель взаимодействия в активном run

Модель взаимодействия последовательная:

- в каждый момент времени активна только одна нода;
- любой live-ввод пользователя всегда оценивается относительно текущей активной ноды;
- права на взаимодействие меняются вместе со сменой активной ноды;
- когда run завершается, прерывается или отменяется, live-взаимодействие для этого run прекращается сразу.

Будущий интерфейс может переподключиться к уже активному run, но семантика взаимодействия привязана к run в core, а не к жизни конкретного процесса CLI или UI. Детали политики закрытия интерфейса остаются частью канонической спецификации и архитектурной границы.

## 3. Включение и значения по умолчанию

Оркестратор предоставляет два независимых live-канала:

- пользовательские комментарии;
- встроенный user-chat MCP-канал.

Поведенческие правила:

- оба канала по умолчанию выключены;
- включение одного канала не включает другой;
- доставка комментариев зависит от конкретной ноды;
- встроенный user-chat MCP-канал, если он включён, доступен текущей активной ноде как системная возможность оркестратора.

Формальные поля конфигурации `interaction` остаются в зоне владения [Interaction And Chat Contract](../03-contracts/agent-json/interaction-and-chat-contract.md).

## 4. Пользовательские комментарии

Пользовательский комментарий — это свободный ввод пользователя во время активного run, который не является явным ответом на вопрос из встроенного user-chat.

Поведенческие правила:

- комментарий может быть принят только пока run активен;
- комментарий может быть доставлен только если текущая активная нода входит в список нод, которым разрешено принимать комментарии;
- если текущей активной ноде комментарии не разрешены, комментарий должен быть отклонён, а не поставлен в очередь до следующей ноды;
- право принимать комментарии пересчитывается при каждой смене активной ноды.

Правила доставки:

- core обязан использовать нативный live-comment механизм runtime, если такой механизм существует;
- core не должен придумывать отдельный orchestrator-only путь внедрения комментариев только ради эмуляции этой функции;
- если активный runtime не умеет доставлять live-комментарии в текущую ноду, комментарии для этой ноды недоступны, даже если конфигурация агента их в принципе разрешает.

Комментарий — это совет или уточнение для идущего run. Это не финальный ответ и не ответ на ожидающий user-chat prompt, если пользователь не выполнил явное действие ответа на prompt.

## 5. Встроенный user-chat MCP-канал

Оркестратор предоставляет системный MCP-канал для общения между активной нодой и пользователем посреди run.

Поведенческие правила:

- встроенный user-chat канал доступен только если он включён для агента;
- сообщения через этот канал являются промежуточными сообщениями run, а не финальным ответом агента;
- активная нода может использовать этот канал либо для информирования пользователя, либо для запроса ответа;
- сообщение, которое не требует ответа, не должно останавливать выполнение run;
- сообщение, которое требует ответа, блокирует активную ноду до тех пор, пока пользователь не даст явный ответ или run не завершится по другой причине.

Этот канал отделён от свободных комментариев. Он нужен для случаев, когда активная нода осознанно открывает диалог с пользователем посреди run, а не просто пассивно получает комментарии.

Точная wire-форма prompt и reply находится в зоне владения [Built-in MCP Contract: `orchestrator.user_chat`](../03-contracts/orchestrator-user-chat-mcp-contract.md).

## 6. Приоритет маршрутизации во время активного run

Когда комментарии и встроенный user-chat включены одновременно, маршрутизация должна следовать одному набору правил:

1. Если существует активный ожидающий built-in user-chat prompt и пользователь использует явное действие ответа, связанное именно с этим prompt, ввод отправляется во встроенный user-chat канал.
2. Любой другой пользовательский ввод во время активного run считается комментарием.

Обязательные следствия:

- свободный текст в основном composer не должен автоматически привязываться к ожидающему built-in user-chat prompt;
- наличие ожидающего prompt не переопределяет общий текстовый канал;
- если комментарии для текущей ноды включены и реально доставляемы, не-ответный пользовательский текст остаётся комментарием даже при ожидающем prompt;
- если комментарии для текущей ноды не включены или не доставляются runtime, такой не-ответный ввод должен быть отклонён, а не молча превращён в reply на prompt.

## 7. Блокировка и разблокировка

Обязательный ответ через built-in user-chat меняет поведение run ровно одним способом: останавливает дальнейшее продвижение текущей активной ноды до прихода ответа.

Слой взаимодействия обязан сохранять следующие различия:

- информационные built-in user-chat сообщения не блокируют выполнение;
- built-in user-chat prompts с обязательным ответом блокируют выполнение;
- комментарии не удовлетворяют блокирующий prompt, если они не отправлены по явному пути ответа на prompt;
- как только run перестаёт быть активным, любой ранее ожидавший prompt становится неактивным и больше не должен принимать ответы.

То, как блокирующее состояние хранится и восстанавливается, относится к [State](../05-state/README.md). То, как после ответа возобновляется исполнение, относится к [Execution](../04-execution/README.md).

## 8. Более богатая App Server taxonomy событий

Для Codex path App Server испускает гораздо более богатый поток notifications, чем два live-канала пользователя, которыми владеет этот документ.

Проверенные семейства notifications включают:

- lifecycle-обновления thread и turn;
- обновления lifecycle item, agent-message, plan, command-execution, file-change и MCP-progress;
- notifications о model reroute, token usage, account/rate limits, config warnings, app list, filesystem changes и статусе MCP server;
- optional-семейства reasoning summary, raw response item, auto-approval review и realtime audio/transcript.

Правила границы и этапности:

- эти notifications являются Codex-specific runtime-events, а не portable semantics agent file;
- безопасное current-stage использование — это read-only local presentation, diagnostics или adapter-internal state mapping, когда другой документ-владелец уже задает смысл реализуемого поведения;
- вывод их в first-class user-facing run surfaces сверх comments и built-in user chat остается later-stage, пока более узкий owner-документ не задаст meaning, persistence и failure behavior;
- события reasoning summary можно surface-ить только по явной product policy, а raw reasoning text deltas и raw response items не должны незаметно становиться обязательной persisted history или содержимым final answer;
- review-, realtime- или detached-thread-notifications не должны по умолчанию создавать новые маршруты пользовательского ввода.

## 9. Негативные правила

Следующее поведение запрещено:

- автоматически повышать произвольный пользовательский текст до built-in user-chat reply;
- ставить комментарии в очередь для будущей ноды, которая ещё не стала активной;
- трактовать built-in user-chat сообщения как финальный ответ агента;
- имитировать поддержку комментариев там, где runtime не предоставляет реального live-comment пути.

## 10. Связанные документы

- [Presentation Rules](./presentation-rules.md)
- [Architecture](../02-architecture/README.md)
- [Execution](../04-execution/README.md)
- [State](../05-state/README.md)
- [Contracts](../03-contracts/README.md)
