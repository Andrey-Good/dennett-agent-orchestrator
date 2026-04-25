[English](#english) | [Русский](#russian)

<a id="english"></a>
# English

# Chat And Resume

Status: normative.

Related documents:

- [`README.md`](./README.md)
- [`local-storage-model.md`](./local-storage-model.md)
- [`secret-markers.md`](./secret-markers.md)
- [`../04-execution/graph-execution.md`](../04-execution/graph-execution.md)
- [`../06-interaction/README.md`](../06-interaction/README.md)
- [`../../agent_orchestrator_final_spec_v2.md`](../../agent_orchestrator_final_spec_v2.md)

## 1. Source Of Truth

Chat state and resume state are local-core truth.

That means:

- the agent definition still comes from the agent JSON file;
- chat and resume data live in local orchestrator storage;
- each chat and resumable run is bound to the immutable resolved revision identity chosen when that run started;
- local chat/resume state exists to continue work, not to redefine the agent graph.

Local chat/resume truth must never be treated as graph-definition truth.

## 2. What Chat State Is For

Chat state exists for:

- explicit resume;
- normal user-visible chat continuity;
- retaining the effective context needed to continue a conversation safely;
- remembering runtime/session facts needed to continue the same work.

Chat state is not a full forensic archive of everything the runtime did.

## 3. Minimum Stored Chat Data

The local store may contain only what is needed for resume and normal user experience.

The minimum allowed categories are:

- user messages;
- visible agent messages;
- the current effective context window, when chat policy allows it;
- the immutable resolved revision identity bound to this chat/run;
- selected runtime identity or adapter information needed for continuation;
- run parameters if they affect resume;
- a native session handle when the runtime supports native resume;
- fallback-resume data needed when native resume is unavailable;
- an unresolved blocking built-in user-chat prompt record, when one currently gates forward progress;
- secret-marker side records governed by [`secret-markers.md`](./secret-markers.md).

Visible messages may include final answers and other user-visible run messages, but they must preserve their message kind so that non-final messages are not mistaken for final node outputs.

## 4. Data That Must Not Be Stored By Default

The following data must not be part of ordinary chat/resume state by default:

- chain-of-thought;
- internal tool traces;
- hidden intermediate reasoning;
- complete runtime-internal action history;
- file-change history as a surrogate for chat state.

If a future extension wants additional memory or history, that belongs to a separate extension axis, not to base chat/resume state.

## 5. Explicit Resume Only

Resume is always an explicit user action.

Mandatory consequences:

- there is no auto-resume after core restart;
- there is no auto-resume after interface restart;
- a stored resumable chat may be offered to the user, but it may not continue itself;
- if interface shutdown policy interrupts a run, later continuation still requires explicit user intent.

## 6. Resume Modes

Two resume modes are supported:

- native resume;
- local resume.

### 6.1. Native Resume

Native resume means the runtime itself can continue an existing session.

When native resume is available and `chat.prefer_native_resume = true`, core must prefer native resume over local reconstruction.

### 6.2. Local Resume

Local resume means core reconstructs enough runtime-facing state from local storage to continue work without runtime-native session continuation.

Local resume is the fallback when:

- the runtime does not support native resume;
- native resume is unavailable for this chat;
- `chat.prefer_native_resume = false`.

## 7. Resume Capability Must Be Recorded Separately From Run Status

Run status and resume capability are not the same thing.

A run may be:

- completed and not resumable;
- interrupted and resumable;
- cancelled and not resumable;
- failed but still locally resumable at the last durable boundary.

Implementations must store enough metadata to distinguish terminal status from resume capability.

## 8. Revision Binding For Resume

Resume must continue against the same resolved revision identity that the original run used.

Mandatory rules:

- the stored chat/resume records must point to the immutable resolved revision identity captured when the run started;
- resume must not re-resolve the logical agent to the current `live` revision;
- a later deploy may change future opens, runs, and event dispatches, but it must not retarget an existing chat;
- if the original resolved revision is unavailable or invalid, resume must fail explicitly instead of silently switching to a newer revision.

## 9. Durable Resume Boundary

Resume may continue only from a durable execution boundary.

This document fixes the rule:

- a durable boundary exists only after the local store has atomically committed either:
  - the node outcome and all allowed derived state updates for a completed node attempt; or
  - the blocked node-attempt state together with unresolved required built-in user-chat prompt metadata.

Consequences:

- a node attempt with neither a committed terminal record nor a committed blocked-prompt wait record must be treated as in-flight and uncommitted;
- local resume must never assume that an in-flight node finished successfully;
- if the latest durable boundary is a blocking prompt wait state, local resume may restore that attempt only as still waiting for explicit reply;
- local resume may re-enter work only from the last durable boundary visible in storage.

This prevents the system from inventing successful execution or skipping unresolved user-gated state that never actually committed.

## 10. Local Resume Behavior

For local resume, core must:

1. Load the stored chat and resume metadata.
2. Load the immutable resolved revision identity bound to that chat/run and verify that the same revision is still available for continuation.
3. Determine the last durable execution boundary.
4. Reconstruct the effective context permitted by chat policy.
5. Restore any durably recorded blocking prompt as pending, or continue from that boundary without fabricating missing node results.

If the last active node has no committed terminal record, local resume must treat that attempt as unfinished work. If the last durable boundary is an unresolved blocking prompt, local resume must restore the run as still waiting on that prompt. It must not commit a success retroactively.

## 11. Pending Built-In User-Chat Prompt State

A blocking built-in user-chat prompt is part of resumable state, not just a transient UI detail.

When the active node is waiting for a required built-in user-chat reply, local state must preserve enough data to distinguish that condition from ordinary execution progress.

At minimum, the stored pending-prompt state must identify:

- the blocked run and node-attempt it belongs to;
- that the prompt is unresolved and blocks forward progress;
- the prompt payload or a durable reference to the prompt payload shown to the user;
- any runtime-facing prompt/request handle needed to deliver the explicit reply, when native resume keeps the prompt alive.

Mandatory consequences:

- reconnecting interfaces must restore the prompt as pending while the run remains active or resumable;
- local resume must restore the blocked state as blocked until an explicit prompt reply is delivered or the run terminates;
- comments remain comments and do not clear the pending prompt unless explicitly sent through the prompt-reply path;
- once the run becomes completed, cancelled, failed, or interrupted without continuation, the pending prompt becomes inactive and must no longer accept replies.

## 12. Chat Policy Fields

The chat policy affects what is stored and how continuation is chosen.

### 12.1. `prefer_native_resume`

- `true`: prefer native runtime continuation when possible.
- `false`: local resume may be selected even if a native handle exists.

### 12.2. `store_visible_messages`

- `true`: persist user-visible messages in local chat state.
- `false`: do not persist a full visible transcript beyond what is strictly required by other enabled policies and resume metadata.

### 12.3. `store_context_window`

- `true`: persist the current effective context window needed for local continuation.
- `false`: local continuation may rely on other stored state or may be impossible if no sufficient continuation state exists.

### 12.4. `allow_fresh_start`

This document fixes the missing operational meaning:

- `true`: the user may start a new chat instead of resuming an existing resumable chat.
- `false`: interfaces must not silently fork a fresh chat when a resumable continuation exists; they must require an explicit choice to resume or to abandon that prior continuation path.

This flag does not authorize auto-resume. It only governs whether fresh-start branching is allowed.

## 13. Interaction With Shutdown Policy

If the interface closes with `keep_core_running`, the run may continue normally outside that interface.

If the interface closes with `stop_core`, active runs become interrupted. The next application launch must not continue them automatically. Later continuation is still an explicit resume action.

## 14. Secret Fragments And Resume

Secret-marker fragments are not normal visible chat text.

If secret markers are enabled:

- visible stored chat remains redacted;
- secret fragments may be restored only into resume-facing runtime context, not into ordinary visible transcript by default;
- the feature remains optional and disabled by default.

See [`secret-markers.md`](./secret-markers.md) for the extraction and restore rules.

## 15. Memory Is A Separate Axis

Chat/resume state is not the same thing as memory.

Long-term memory sources, if present, are separate extensions. They must not be conflated with:

- `params`;
- `vars`;
- chat history;
- resume state.

## 16. Acceptance Criteria For An Implementation

An implementation conforms to this document only if:

- chat/resume state is stored locally and separately from the agent file truth;
- resume never starts automatically after restart;
- resume stays pinned to the original resolved revision identity and never silently switches to a newer `live` revision;
- native and local resume are distinguishable in stored metadata;
- unfinished node attempts are not reclassified as successful during resume;
- unresolved blocking built-in user-chat prompts remain distinguishable and restorable as blocked state;
- chat policy flags change storage and continuation behavior deterministically.

<a id="russian"></a>
# Русский

# Чат и resume

Статус: нормативный.

Связанные документы:

- [`README.md`](./README.md)
- [`local-storage-model.md`](./local-storage-model.md)
- [`secret-markers.md`](./secret-markers.md)
- [`../04-execution/graph-execution.md`](../04-execution/graph-execution.md)
- [`../06-interaction/README.md`](../06-interaction/README.md)
- [`../../agent_orchestrator_final_spec_v2.md`](../../agent_orchestrator_final_spec_v2.md)

## 1. Источник истины

Chat state и resume state являются локальной истиной core.

Это означает:

- определение агента по-прежнему приходит из agent JSON file;
- chat и resume data живут в локальном storage оркестратора;
- каждый chat и resumable run привязан к неизменяемой resolved revision identity, выбранной при старте этого run;
- локальное chat/resume state существует для продолжения работы, а не для переопределения графа агента.

Локальная chat/resume truth никогда не должна трактоваться как truth графового определения.

## 2. Для чего нужен chat state

Chat state существует для:

- explicit resume;
- нормальной пользовательской непрерывности чата;
- сохранения effective context, нужного для безопасного продолжения разговора;
- запоминания runtime/session facts, нужных для продолжения той же работы.

Chat state не является полным forensic archive всего, что делал runtime.

## 3. Минимальный набор данных чата

Локальное хранилище может содержать только то, что нужно для resume и нормального пользовательского опыта.

Минимально допустимые категории:

- сообщения пользователя;
- видимые сообщения агента;
- текущее effective context window, если это разрешено chat policy;
- неизменяемая resolved revision identity, к которой привязан этот chat/run;
- selected runtime identity или adapter information, нужные для продолжения;
- параметры запуска, если они влияют на resume;
- native session handle, если runtime поддерживает native resume;
- fallback-resume data, нужные при отсутствии native resume;
- неразрешенная запись о blocking built-in user-chat prompt, если именно она сейчас блокирует дальнейшее продвижение;
- side records для secret markers по правилам [`secret-markers.md`](./secret-markers.md).

Видимые сообщения могут включать финальные ответы и другие user-visible run messages, но обязаны сохранять свой kind сообщения, чтобы non-final messages не принимались за финальные node outputs.

## 4. Какие данные по умолчанию хранить нельзя

Следующие данные не должны входить в обычный chat/resume state по умолчанию:

- chain-of-thought;
- internal tool traces;
- скрытые промежуточные рассуждения;
- полная runtime-внутренняя история действий;
- история изменений файлов как суррогат chat state.

Если будущему расширению понадобится дополнительная память или история, это относится к отдельной extension axis, а не к базовому chat/resume state.

## 5. Только явный resume

Resume всегда является явным действием пользователя.

Обязательные следствия:

- после рестарта core нет auto-resume;
- после рестарта интерфейса нет auto-resume;
- пользователю можно предложить сохраненный resumable chat, но он не может продолжиться сам;
- если shutdown интерфейса прервал run, последующее продолжение все равно требует явного намерения пользователя.

## 6. Режимы resume

Поддерживаются два режима resume:

- native resume;
- local resume.

### 6.1. Native resume

Native resume означает, что runtime сам умеет продолжать существующую сессию.

Когда native resume доступен и `chat.prefer_native_resume = true`, core обязан предпочитать native resume локальной реконструкции.

### 6.2. Local resume

Local resume означает, что core восстанавливает из локального storage достаточно runtime-facing state, чтобы продолжить работу без runtime-native продолжения сессии.

Local resume является fallback-путем, когда:

- runtime не поддерживает native resume;
- native resume недоступен для этого чата;
- `chat.prefer_native_resume = false`.

## 7. Возможность resume должна храниться отдельно от статуса run

Статус run и возможность resume - не одно и то же.

Run может быть:

- завершенным и нерезюмируемым;
- прерванным и резюмируемым;
- отмененным и нерезюмируемым;
- неуспешным, но все еще локально резюмируемым от последней durable boundary.

Реализация обязана хранить достаточно метаданных, чтобы различать terminal status и resume capability.

## 8. Привязка revision для resume

Resume обязан продолжать работу на той же resolved revision identity, которую использовал исходный run.

Обязательные правила:

- сохраненные chat/resume records обязаны указывать на неизменяемую resolved revision identity, зафиксированную при старте run;
- resume не имеет права заново резолвить логического агента в текущую `live` revision;
- последующий deploy может менять будущие opens, runs и event dispatches, но не имеет права перепривязывать уже существующий chat;
- если исходная resolved revision больше недоступна или стала невалидной, resume обязан завершаться явной ошибкой, а не молча переключаться на более новую revision.

## 9. Durable resume boundary

Resume может продолжать работу только с durable execution boundary.

Этот документ фиксирует правило:

- durable boundary существует только после того, как local store атомарно зафиксировал либо:
  - node outcome и все разрешенные производные обновления состояния для завершенной попытки ноды; либо
  - blocked-state попытки ноды вместе с metadata неразрешенного required built-in user-chat prompt.

Следствия:

- попытка ноды без committed terminal record и без committed blocked-prompt wait record должна считаться in-flight и незафиксированной;
- local resume никогда не может предполагать, что in-flight нода завершилась успешно;
- если последняя durable boundary является состоянием ожидания blocking prompt, local resume может восстанавливать эту попытку только как все еще ожидающую явный reply;
- local resume может заходить в работу только с последней durable boundary, видимой в storage.

Это не позволяет системе придумывать успешное исполнение или перескакивать через неразрешенное user-gated state, которое фактически не было committed.

## 10. Поведение local resume

Для local resume core обязан:

1. Загрузить сохраненные chat и resume metadata.
2. Загрузить неизменяемую resolved revision identity, к которой привязан этот chat/run, и убедиться, что именно эта revision все еще доступна для продолжения.
3. Определить последнюю durable execution boundary.
4. Восстановить effective context, разрешенный chat policy.
5. Восстановить любой durably recorded blocking prompt как ожидающий или продолжить с этой boundary, не фабрикуя отсутствующие результаты нод.

Если у последней активной ноды нет committed terminal record, local resume обязан трактовать эту попытку как незавершенную работу. Если последняя durable boundary является неразрешенным blocking prompt, local resume обязан восстановить run как все еще ожидающий этот prompt. Задним числом фиксировать `success` запрещено.

## 11. Состояние ожидающего built-in user-chat prompt

Блокирующий built-in user-chat prompt является частью resumable state, а не просто временной деталью UI.

Когда активная нода ждет обязательный built-in user-chat reply, локальное состояние обязано хранить достаточно данных, чтобы отличать это состояние от обычного продвижения исполнения.

Как минимум, сохраненное состояние ожидающего prompt обязано идентифицировать:

- blocked run и node-attempt, к которым он относится;
- тот факт, что prompt остается неразрешенным и блокирует дальнейшее продвижение;
- payload prompt или надежную ссылку на payload prompt, который был показан пользователю;
- любой runtime-facing prompt/request handle, нужный для доставки явного ответа, если native resume сохраняет этот prompt живым.

Обязательные последствия:

- переподключившиеся интерфейсы обязаны восстанавливать prompt как ожидающий, пока run остается активным или resumable;
- local resume обязан восстанавливать blocked state именно как blocked до тех пор, пока не будет доставлен явный reply на prompt или пока run не завершится;
- комментарии остаются комментариями и не снимают ожидающий prompt, если они не отправлены явно по пути ответа на prompt;
- как только run становится completed, cancelled, failed или interrupted без продолжения, ожидающий prompt становится неактивным и больше не должен принимать ответы.

## 12. Поля chat policy

Chat policy влияет на то, что хранится и как выбирается путь продолжения.

### 12.1. `prefer_native_resume`

- `true`: по возможности предпочитать native runtime continuation.
- `false`: можно выбирать local resume даже при наличии native handle.

### 12.2. `store_visible_messages`

- `true`: сохранять user-visible messages в local chat state.
- `false`: не сохранять полный видимый transcript сверх того, что строго требуется другими включенными policy и resume metadata.

### 12.3. `store_context_window`

- `true`: сохранять текущее effective context window, нужное для local continuation.
- `false`: local continuation может опираться на другие сохраненные данные или может оказаться невозможным, если достаточного состояния продолжения нет.

### 12.4. `allow_fresh_start`

Этот документ фиксирует ранее неоперационализированный смысл:

- `true`: пользователь может начать новый chat вместо resume существующего resumable chat.
- `false`: интерфейсы не имеют права молча форкать новый chat при наличии resumable continuation; они обязаны требовать явный выбор между resume и отказом от предыдущего continuation path.

Этот флаг не разрешает auto-resume. Он только управляет тем, разрешено ли ветвление через fresh start.

## 13. Взаимодействие с shutdown policy

Если интерфейс закрывается с `keep_core_running`, run может нормально продолжаться вне этого интерфейса.

Если интерфейс закрывается с `stop_core`, активные runs становятся interrupted. Следующий запуск приложения не должен продолжать их автоматически. Любое последующее продолжение все равно является explicit resume-действием.

## 14. Secret fragments и resume

Фрагменты из secret markers не являются обычным видимым chat text.

Если secret markers включены:

- видимый сохраненный чат остается redacted;
- secret fragments могут восстанавливаться только в resume-facing runtime context, но не в обычный видимый transcript по умолчанию;
- функция остается optional и выключенной по умолчанию.

Правила извлечения и восстановления см. в [`secret-markers.md`](./secret-markers.md).

## 15. Memory - отдельная ось

Chat/resume state не равно memory.

Long-term memory sources, если они есть, являются отдельными расширениями. Их нельзя смешивать с:

- `params`;
- `vars`;
- историей чата;
- resume state.

## 16. Критерии приемки реализации

Реализация соответствует этому документу только если:

- chat/resume state хранится локально и отдельно от truth agent file;
- resume никогда не стартует автоматически после рестарта;
- resume остается привязанным к исходной resolved revision identity и никогда молча не переключается на более новую `live` revision;
- native и local resume различаются в stored metadata;
- незавершенные попытки нод не переклассифицируются в успешные во время resume;
- неразрешенные блокирующие built-in user-chat prompts остаются различимыми и восстанавливаемыми как blocked state;
- флаги chat policy детерминированно меняют поведение хранения и продолжения.
