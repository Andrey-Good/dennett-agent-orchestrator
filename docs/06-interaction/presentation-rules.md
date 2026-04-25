[English](#english) | [Русский](#russian)

<a id="english"></a>
# English

This document defines how active-run interaction must be presented in CLI and future interfaces. It turns the behavioral rules from [Live Run Interaction](./live-run-interaction.md) and [the root specification](../../agent_orchestrator_final_spec_v2.md), sections `24` through `26`, into interface requirements without restating formal payload contracts.

## 1. Scope

This document governs:

- where live messages appear;
- how intermediate messages differ from the final answer;
- how a user explicitly sends a comment versus explicitly replies to a built-in user-chat prompt;
- what availability and waiting states an interface must show during an active run.

This document does not define JSON payload rendering rules or persistence formats.

## 2. One canonical conversation surface

During an active run, the canonical place for user-visible interaction is the same conversation surface where the user normally reads the final answer and writes ordinary messages.

Required rules:

- built-in user-chat messages must appear in that same surface;
- final output, comments, and intermediate system messages must stay in chronological order inside that surface;
- an interface may add secondary indicators such as badges, toasts, or side panels, but those must not replace the main conversation surface as the canonical location of the interaction.

## 3. Distinguishing final and intermediate messages

Interfaces must make the difference between the agent's final answer and mid-run messages obvious.

Required rules:

- the final answer must have the primary presentation in the conversation;
- built-in user-chat messages must be styled as non-final system messages;
- intermediate messages must be compact and must not be presented as if they were the node's final output;
- if the agent is configured with no automatic final answer, the interface must still not promote an intermediate built-in user-chat message into a fake final result.

## 4. Presenting a pending built-in user-chat prompt

When the active node sends a built-in user-chat message that requires a reply, the interface must present it as a pending prompt.

Required rules:

- the pending prompt must remain visibly pending until the user replies or the run stops;
- the action used to send the reply must be explicit and bound to that specific prompt;
- when the prompt offers predefined options, the interface must expose direct selectable actions for those options;
- if the interface also allows a typed reply, it must make clear that the text is being sent as a prompt reply, not as a generic comment.

The interface-agnostic form of the explicit reply action may be a button, command, shortcut, or mode switch. What matters is unambiguous binding, not widget choice.

## 5. Presenting comments

Comments are the generic user-to-run channel. Their presentation must preserve that role.

Required rules:

- if comments are currently available for the active node and runtime, the interface must expose a normal comment send path;
- if comments are not currently available, the interface must disable or withhold that send path and communicate why;
- sending a generic text message during an active run must never silently switch into prompt-reply mode.

A pending built-in user-chat prompt does not remove the comment channel by itself. If comments remain enabled and deliverable for the active node, the interface may continue to expose both actions at the same time.

## 6. Routing cues and explicit user intent

Whenever both of these actions are possible at once:

- send a comment;
- reply to a pending built-in user-chat prompt;

the interface must distinguish them clearly.

Acceptable approaches include:

- separate buttons or commands;
- a visible mode switch;
- a reply action attached directly to the pending prompt.

Forbidden presentation patterns:

- one generic send action whose routing changes implicitly;
- auto-binding free text to the most recent pending prompt;
- hidden prompt-reply behavior that the user cannot inspect before sending.

## 7. Availability and waiting states

The interface must reflect interaction state changes while the run is active.

Required rules:

- if the active node is waiting for a required prompt reply, the interface must show that the run is blocked on user input;
- if comment delivery is unavailable because the active node is not eligible or the runtime lacks support, the interface must not pretend that comments can be sent successfully;
- when the run ends, any controls tied to that active run's live interaction must become inactive or read-only.

How those states are stored across reconnects belongs to [State](../05-state/README.md). The presentation requirement is only that restored UI state must keep the same semantic distinctions.

## 8. Reconnect and close-policy implications

Presentation attaches to core-owned run state, not to a single interface process.

Implications:

- if an interface detaches and later reconnects to an active run, it must restore pending prompts as pending and intermediate messages as non-final;
- if the system is configured to keep core running after interface closure, the absence of one interface window does not cancel the run or change routing semantics;
- if the system stops core when the interface closes, live interaction ends because the run ends, not because the presentation layer chose a different rule.

The close-policy source of truth remains [the root specification](../../agent_orchestrator_final_spec_v2.md) and the [architecture section](../02-architecture/README.md).

## 9. Related documents

- [Live Run Interaction](./live-run-interaction.md)
- [Architecture](../02-architecture/README.md)
- [Execution](../04-execution/README.md)
- [State](../05-state/README.md)
- [Contracts](../03-contracts/README.md)

<a id="russian"></a>
# Русский

Этот документ задаёт, как live-взаимодействие должно быть представлено в CLI и будущих интерфейсах. Он переводит поведенческие правила из [Live Run Interaction](./live-run-interaction.md) и [корневой спецификации](../../agent_orchestrator_final_spec_v2.md), разделы `24`-`26`, в требования к интерфейсу, не повторяя формальные payload-контракты.

## 1. Область действия

Этот документ регулирует:

- где показываются live-сообщения;
- как промежуточные сообщения отличаются от финального ответа;
- как пользователь явно отправляет комментарий и как явно отвечает на built-in user-chat prompt;
- какие состояния доступности и ожидания интерфейс обязан показывать во время активного run.

Этот документ не определяет правила рендеринга JSON payload и форматы хранения.

## 2. Одна каноническая поверхность диалога

Во время активного run каноническим местом пользовательского взаимодействия является та же поверхность диалога, где пользователь обычно читает финальный ответ и пишет обычные сообщения.

Обязательные правила:

- built-in user-chat сообщения должны появляться на этой же поверхности;
- финальный output, комментарии и промежуточные системные сообщения должны сохранять хронологический порядок внутри этой поверхности;
- интерфейс может добавлять вторичные индикаторы вроде badge, toast или боковой панели, но они не должны заменять основную поверхность диалога как каноническое место взаимодействия.

## 3. Различение финальных и промежуточных сообщений

Интерфейс обязан явно отличать финальный ответ агента от mid-run сообщений.

Обязательные правила:

- финальный ответ должен иметь основную презентацию в диалоге;
- built-in user-chat сообщения должны быть оформлены как нефинальные системные сообщения;
- промежуточные сообщения должны оставаться компактными и не должны подаваться как финальный output ноды;
- если агент настроен без автоматического финального ответа, интерфейс всё равно не должен повышать промежуточное built-in user-chat сообщение до фиктивного финального результата.

## 4. Показ ожидающего built-in user-chat prompt

Когда активная нода отправляет built-in user-chat сообщение, требующее ответа, интерфейс обязан показывать его как ожидающий prompt.

Обязательные правила:

- ожидающий prompt должен оставаться явно ожидающим, пока пользователь не ответит или run не завершится;
- действие ответа должно быть явным и привязанным к конкретному prompt;
- если prompt предлагает готовые варианты, интерфейс должен показывать прямые действия выбора этих вариантов;
- если интерфейс также допускает текстовый ответ, он обязан явно показывать, что текст отправляется именно как reply на prompt, а не как обычный комментарий.

Агностичная к типу интерфейса форма такого явного действия может быть кнопкой, командой, shortcut или переключателем режима. Важна не конкретная форма элемента управления, а однозначная привязка.

## 5. Показ комментариев

Комментарии — это общий канал user-to-run. Их отображение обязано сохранять эту роль.

Обязательные правила:

- если комментарии в данный момент доступны для активной ноды и текущего runtime, интерфейс должен предоставлять обычный путь отправки комментария;
- если комментарии сейчас недоступны, интерфейс должен отключать или скрывать этот путь отправки и сообщать причину;
- отправка обычного текстового сообщения во время активного run никогда не должна молча переключаться в режим reply на prompt.

Ожидающий built-in user-chat prompt сам по себе не убирает канал комментариев. Если комментарии для активной ноды по-прежнему включены и доставляемы, интерфейс может одновременно показывать оба действия.

## 6. Подсказки маршрутизации и явное намерение пользователя

Всякий раз, когда одновременно возможны оба действия:

- отправить комментарий;
- ответить на ожидающий built-in user-chat prompt;

интерфейс обязан ясно различать их.

Допустимые подходы:

- отдельные кнопки или команды;
- видимый переключатель режима;
- действие ответа, прикреплённое прямо к ожидающему prompt.

Запрещённые паттерны отображения:

- одно универсальное действие отправки, чья маршрутизация неявно меняется;
- автоматическая привязка свободного текста к последнему ожидающему prompt;
- скрытая логика reply на prompt, которую пользователь не может проверить до отправки.

## 7. Состояния доступности и ожидания

Интерфейс обязан отражать изменения состояния взаимодействия, пока run активен.

Обязательные правила:

- если активная нода ждёт обязательный reply на prompt, интерфейс должен показывать, что run заблокирован на пользовательском вводе;
- если доставка комментариев недоступна, потому что активная нода не имеет такого права или runtime не поддерживает эту функцию, интерфейс не должен делать вид, что комментарий можно успешно отправить;
- когда run завершается, любые контролы, привязанные к live-взаимодействию этого run, должны становиться неактивными или только для чтения.

То, как эти состояния сохраняются между переподключениями, относится к [State](../05-state/README.md). Требование к отображению состоит только в том, что восстановленный интерфейс обязан сохранять те же семантические различия.

## 8. Переподключение и последствия политики закрытия

Отображение привязано к состоянию run, которым владеет core, а не к одному процессу интерфейса.

Следствия:

- если интерфейс отключился и позже подключился к ещё активному run, он обязан восстановить ожидающие prompt как ожидающие, а промежуточные сообщения как нефинальные;
- если система настроена сохранять core после закрытия интерфейса, отсутствие окна интерфейса не отменяет run и не меняет правила маршрутизации;
- если система останавливает core при закрытии интерфейса, live-взаимодействие заканчивается потому, что закончился run, а не потому что слой presentation выбрал другое правило.

Источником истины для политики закрытия остаются [корневая спецификация](../../agent_orchestrator_final_spec_v2.md) и раздел [Architecture](../02-architecture/README.md).

## 9. Связанные документы

- [Live Run Interaction](./live-run-interaction.md)
- [Architecture](../02-architecture/README.md)
- [Execution](../04-execution/README.md)
- [State](../05-state/README.md)
- [Contracts](../03-contracts/README.md)
