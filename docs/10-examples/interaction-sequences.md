[English](#english) | [Русский](#russian)

<a id="english"></a>
# Interaction Sequences

Status: non-normative illustrative sequences.
Owns: nothing. These flows summarize how several owner docs work together over time.

Section map:

- [Examples index](./README.md)
- [Canonical agent JSON example](./canonical-agent-json-example.md)
- [Valid patterns](./valid-patterns.md)
- [Invalid patterns](./invalid-patterns.md)

## Sequence 1. Comment To An Active Runtime Node

1. Core starts `triage` as the current active `runtime_agent` node.
2. The interface checks that `triage` is in `interaction.comments.target_node_ids`.
3. The user sends a free-form note through the normal comment action.
4. Core calls the adapter's logical `deliverComment(...)` path only if live comments are both enabled and supported.
5. The runtime may emit visible intermediate messages, but those remain non-final run messages.
6. The final answer still comes only from the last successful node output chosen by `final_output.mode`.

Owner docs: [interaction-and-chat-contract.md](../03-contracts/agent-json/interaction-and-chat-contract.md), [runtime-adapter-contract.md](../03-contracts/runtime-adapter-contract.md), [live-run-interaction.md](../06-interaction/live-run-interaction.md), [presentation-rules.md](../06-interaction/presentation-rules.md), [outputs-outcomes-and-final-response.md](../04-execution/outputs-outcomes-and-final-response.md)

## Sequence 2. Pending Built-In User Chat Prompt With Explicit Reply

1. The active runtime node emits a `user_chat_request` through `orchestrator.user_chat`.
2. The request has `require_response = true`, so the active node becomes blocked on user input.
3. Core records the pending prompt as resumable blocked state instead of pretending the node already finished.
4. The interface presents the prompt as pending in the same conversation surface and keeps it visibly non-final.
5. If the user uses the explicit prompt-reply action, the reply goes to the MCP channel.
6. Any other free-form text during the same wait remains a comment or is rejected if comments are unavailable; it is not auto-bound to the prompt.
7. The node continues only after the explicit reply arrives or the run ends for another reason.

Owner docs: [orchestrator-user-chat-mcp-contract.md](../03-contracts/orchestrator-user-chat-mcp-contract.md), [interaction-and-chat-contract.md](../03-contracts/agent-json/interaction-and-chat-contract.md), [runtime-adapter-contract.md](../03-contracts/runtime-adapter-contract.md), [chat-and-resume.md](../05-state/chat-and-resume.md), [live-run-interaction.md](../06-interaction/live-run-interaction.md), [presentation-rules.md](../06-interaction/presentation-rules.md)

## Sequence 3. Explicit Resume After Reconnect

1. A run becomes resumable after a durable boundary or while a blocking built-in prompt is durably pending.
2. The interface disconnects or the process stops; the run does not auto-resume by itself later.
3. The user explicitly chooses resume.
4. Core reloads the stored chat/resume data and the same resolved revision identity that the run started with.
5. Core chooses native resume first only when policy and adapter support allow it; otherwise it performs local resume.
6. If the last durable state was a pending built-in prompt, Core restores that state as still waiting rather than inventing a success.
7. The interface restores pending prompts and intermediate messages as non-final items in the same conversation surface.

Owner docs: [chat-and-resume.md](../05-state/chat-and-resume.md), [local-storage-model.md](../05-state/local-storage-model.md), [runtime-adapter-contract.md](../03-contracts/runtime-adapter-contract.md), [presentation-rules.md](../06-interaction/presentation-rules.md), [versioning-axes.md](../07-lifecycle/versioning-axes.md)

## Sequence 4. Deploy Changes Future Launches, Not Existing Chats

1. Event `E1` resolves logical agent `A` to current live revision `R7` and starts run `Run-1`.
2. While `Run-1` exists, a user edits a draft `R8` and later deploys it.
3. Future opens and future event dispatches may now resolve `A` to `R8`.
4. `Run-1` and any resumable chat created from it remain bound to `R7`.
5. If `Run-1` is resumed later, Core must not silently retarget it to the newer live revision.
6. If a later event `E2` fires after deploy, `E2` resolves to `R8` because event dispatch uses the current live revision at dispatch time.

Owner docs: [agent-registry.md](../07-lifecycle/agent-registry.md), [draft-live-deploy.md](../07-lifecycle/draft-live-deploy.md), [events-and-triggers.md](../07-lifecycle/events-and-triggers.md), [chat-and-resume.md](../05-state/chat-and-resume.md)

<a id="russian"></a>
# Сценарии Взаимодействия

Статус: ненормативные иллюстративные сценарии.
Владение: ничем. Эти потоки суммируют то, как несколько профильных документов работают вместе во времени.

Карта раздела:

- [Индекс примеров](./README.md)
- [Канонический пример agent JSON](./canonical-agent-json-example.md)
- [Валидные паттерны](./valid-patterns.md)
- [Невалидные паттерны](./invalid-patterns.md)

## Сценарий 1. Комментарий В Активную Runtime-Ноду

1. Core запускает `triage` как текущую активную ноду `runtime_agent`.
2. Интерфейс проверяет, что `triage` входит в `interaction.comments.target_node_ids`.
3. Пользователь отправляет свободную заметку через обычное действие комментария.
4. Core вызывает логический путь `deliverComment(...)` у адаптера только если live comments и разрешены, и поддерживаются.
5. Runtime может испускать видимые промежуточные сообщения, но они остаются нефинальными сообщениями run-а.
6. Финальный ответ по-прежнему появляется только из последнего успешного node output, выбранного через `final_output.mode`.

Документы-владельцы: [interaction-and-chat-contract.md](../03-contracts/agent-json/interaction-and-chat-contract.md), [runtime-adapter-contract.md](../03-contracts/runtime-adapter-contract.md), [live-run-interaction.md](../06-interaction/live-run-interaction.md), [presentation-rules.md](../06-interaction/presentation-rules.md), [outputs-outcomes-and-final-response.md](../04-execution/outputs-outcomes-and-final-response.md)

## Сценарий 2. Ожидающий Built-In User Chat Prompt С Явным Reply

1. Активная runtime-нода испускает `user_chat_request` через `orchestrator.user_chat`.
2. У запроса установлено `require_response = true`, поэтому активная нода блокируется на пользовательском вводе.
3. Core записывает ожидающий prompt как resumable blocked state, а не делает вид, будто нода уже завершилась.
4. Интерфейс показывает prompt как ожидающий на той же поверхности диалога и сохраняет его явно нефинальным.
5. Если пользователь использует явное действие ответа на prompt, reply уходит в MCP-канал.
6. Любой другой свободный текст во время этого ожидания остается комментарием или отклоняется, если комментарии недоступны; он не привязывается к prompt автоматически.
7. Нода продолжает работу только после явного ответа или завершения run-а по другой причине.

Документы-владельцы: [orchestrator-user-chat-mcp-contract.md](../03-contracts/orchestrator-user-chat-mcp-contract.md), [interaction-and-chat-contract.md](../03-contracts/agent-json/interaction-and-chat-contract.md), [runtime-adapter-contract.md](../03-contracts/runtime-adapter-contract.md), [chat-and-resume.md](../05-state/chat-and-resume.md), [live-run-interaction.md](../06-interaction/live-run-interaction.md), [presentation-rules.md](../06-interaction/presentation-rules.md)

## Сценарий 3. Explicit Resume После Переподключения

1. Run становится resumable после durable boundary или пока durably pending blocking prompt остается неразрешенным.
2. Интерфейс отключается или процесс останавливается; позже run не выполняет auto-resume сам по себе.
3. Пользователь явно выбирает resume.
4. Core заново загружает сохраненные chat/resume data и ту же resolved revision identity, с которой run стартовал.
5. Core выбирает native resume первым только когда это разрешают policy и поддержка адаптера; иначе он выполняет local resume.
6. Если последним durable state был ожидающий built-in prompt, Core восстанавливает его как все еще ожидающий, а не придумывает success.
7. Интерфейс восстанавливает pending prompts и промежуточные сообщения как нефинальные элементы на той же поверхности диалога.

Документы-владельцы: [chat-and-resume.md](../05-state/chat-and-resume.md), [local-storage-model.md](../05-state/local-storage-model.md), [runtime-adapter-contract.md](../03-contracts/runtime-adapter-contract.md), [presentation-rules.md](../06-interaction/presentation-rules.md), [versioning-axes.md](../07-lifecycle/versioning-axes.md)

## Сценарий 4. Deploy Меняет Будущие Запуски, А Не Существующие Chats

1. Event `E1` разрешает логического агента `A` в текущую live-ревизию `R7` и запускает run `Run-1`.
2. Пока `Run-1` существует, пользователь редактирует draft `R8` и позже deploy-ит его.
3. Будущие opens и будущие event-dispatch теперь уже могут разрешать `A` в `R8`.
4. `Run-1` и любой resumable chat, созданный из него, остаются привязанными к `R7`.
5. Если `Run-1` позже resume-ится, Core не имеет права молча перенаправить его на более новую live-ревизию.
6. Если после deploy срабатывает более позднее событие `E2`, оно разрешается в `R8`, потому что event-dispatch использует текущую live-ревизию в момент dispatch.

Документы-владельцы: [agent-registry.md](../07-lifecycle/agent-registry.md), [draft-live-deploy.md](../07-lifecycle/draft-live-deploy.md), [events-and-triggers.md](../07-lifecycle/events-and-triggers.md), [chat-and-resume.md](../05-state/chat-and-resume.md)
