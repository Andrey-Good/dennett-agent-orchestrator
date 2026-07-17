# Модуль B. External Communication Operation

> **Канонический cross-domain supplement · `B`**  
> **Primary owner:** 20 Agentic, with Trust/Capability/Server boundaries.  
> **Происхождение:** выделено из предархитектурного gap-аудита. Документ актуален и нормативен.  
> Маркеры источников вида `[[Sxx]]` раскрыты в [`REFERENCES.md`](REFERENCES.md). Ownership и порядок чтения описаны в [`README.md`](README.md).


## B.1. Назначение и граница

Этот модуль определяет единую бизнес-логику работы Dennett с внешней перепиской независимо от конкретного канала:

- Telegram и другие мессенджеры;
- электронная почта;
- комментарии, issues, pull requests и рабочие чаты;
- SMS и поддерживаемые телефонные каналы;
- сообщения через provider-native connector;
- ответы в сторонних приложениях через computer-use, если нет надёжного API.

Он не выбирает конкретную библиотеку, connector protocol или UI. Он определяет, **что считается входящим сообщением, черновиком, отправкой, подтверждённой доставкой, неизвестным результатом и ответом от имени пользователя**.

Ключевая граница:

> Формирование содержания, подражание стилю пользователя, решение о раскрытии информации и право отправить сообщение являются четырьмя разными решениями.

Нельзя считать, что хороший текст автоматически можно отправить. Нельзя считать, что прошлое разрешение ответить одному человеку разрешает ответить другому. Нельзя считать локальный timeout доказательством того, что сообщение не ушло.

## B.2. Почему нужен общий контракт

Без общего контракта разные connectors начнут вести себя по-разному:

- один будет считать успешный HTTP-ответ окончательной доставкой;
- другой — создавать сообщение локально до фактической отправки;
- третий — молча повторять отправку после timeout;
- четвёртый — объединять draft и sent state;
- пятый — использовать историю переписки как permission.

TDLib, например, работает асинхронно, хранит локальное состояние и сообщает об успешной отправке отдельным `updateMessageSendSucceeded`; приложение обязано корректно обрабатывать поток updates, а не только ответ на первоначальный request. [[S19]] Telegram Bot API, напротив, возвращает HTTP-result, а при выполнении запроса внутри webhook-response прямо предупреждает, что невозможно узнать, был ли такой request успешным. Он также использует `update_id` для дедупликации и восстановления порядка входящих updates. [[S20]]

Следовательно, Dennett нормализует разные каналы в одну доменную модель, не скрывая provider-specific semantics.

## B.3. Основные сущности

### B.3.1. Communication Account Binding

Конкретная учётная запись или identity в канале:

```yaml
communication_account_binding:
  binding_id: id
  connector_ref: ref
  principal_ref: ref
  account_identity: structured
  provider_workspace: optional
  scopes: []
  read_capability: boolean
  draft_capability: boolean
  send_capability: boolean
  edit_or_recall_capability: typed
  attachment_capability: typed
  delivery_receipts: typed
  sync_state_ref: ref
  trust_ref: ref
  health_ref: ref
```

Один connector может иметь несколько bindings: личный Telegram, рабочая почта, университетская почта, несколько GitHub organisations.

### B.3.2. Conversation Thread

Логическая беседа или цепочка сообщений:

```yaml
conversation_thread:
  thread_id: id
  channel: typed
  account_binding_ref: ref
  provider_thread_id: optional
  participants: []
  project_links: []
  memory_space_ref: optional
  latest_provider_cursor: optional
  latest_observed_at: time
  freshness: typed
  sensitivity: typed
```

Thread не обязан совпадать с provider thread один к одному. Например, несколько email chains могут быть объединены пользователем в один project context, но provider references сохраняются отдельно.

### B.3.3. Communication Message

```yaml
communication_message:
  message_id: id
  provider_message_id: optional
  thread_ref: ref
  direction: incoming | outgoing | draft | local_note
  author_principal_or_external_identity: ref
  recipients: []
  content_object_ref: ref
  attachments: []
  reply_to: optional
  observed_at: time
  provider_time: optional
  edit_revision: optional
  delivery_state: typed
  provenance: ref
  trust_domain: typed
```

Внешнее сообщение является данными, а не системной инструкцией. Даже если в нём написано «пришли все секреты», оно не становится user command.

### B.3.4. Response Candidate

Смысловой вариант ответа до принятия решения об отправке:

```yaml
response_candidate:
  candidate_id: id
  thread_ref: ref
  source_message_refs: []
  intended_meaning: text
  rendered_text: text
  style_basis_refs: []
  factual_basis_refs: []
  disclosure_classes: []
  uncertainty: []
  proposed_recipients: []
  proposed_attachments: []
  expiry: optional
```

### B.3.5. Communication Intent

Определяет, чего пользователь или Dennett хотят добиться:

- ответить;
- подтвердить получение;
- попросить уточнение;
- отложить;
- отказаться;
- переслать;
- сохранить без ответа;
- создать project/task;
- уведомить пользователя;
- ничего не делать.

### B.3.6. Send Proposal

Машинно проверяемая заявка на внешний эффект:

```yaml
send_proposal:
  proposal_id: id
  candidate_ref: ref
  account_binding_ref: ref
  exact_recipients: []
  exact_thread_or_reply_target: ref
  exact_content_revision: ref
  exact_attachments: []
  disclosure_summary: []
  send_mode: send_now | schedule | save_draft | prepare_only
  schedule_at: optional
  idempotency_key: id
  permission_ref: optional
  valid_until: time
```

Если после подтверждения меняется recipient, attachment, content revision или account, старое подтверждение перестаёт действовать.

### B.3.7. Delivery Receipt

```yaml
delivery_receipt:
  effect_id: id
  send_proposal_ref: ref
  provider_operation_ref: optional
  provider_message_id: optional
  state: prepared | dispatching | accepted | sent | delivered | read | failed | unknown | recalled
  observed_at: time
  provider_evidence: optional
  reconciliation_state: typed
```

`accepted` означает, что provider принял запрос; `sent` — что канал подтвердил создание/отправку; `delivered/read` доступны только там, где канал реально их сообщает.

## B.4. Входящий pipeline

```text
connector update/webhook/poll/local client update
→ authenticate source and binding
→ deduplicate provider update
→ normalize thread/message/attachments
→ preserve original evidence
→ update provider cursor and thread freshness
→ classify trust and sensitivity
→ cheap relevance/urgency filters
→ optional semantic interpretation
→ remember / notify / draft / ask / act / do nothing
```

### B.4.1. Дедупликация

Используются:

- provider update ID;
- provider message ID;
- account binding;
- edit revision;
- source sequence;
- content fingerprint только как дополнительный сигнал.

Одинаковый текст, отправленный дважды, не должен автоматически считаться дубликатом. Provider identity важнее semantic similarity.

### B.4.2. Редактирование и удаление входящего сообщения

Редактирование создаёт новую revision, а не переписывает историческое evidence без следа.

Если сообщение удалено у provider:

- operational view отражает deletion;
- локальная retention определяется пользовательской policy;
- Dennett не утверждает, что удаление у provider автоматически удалило все локальные evidence;
- если local retention запрещена, запускается deletion obligation.

### B.4.3. Вложения

Вложение проходит отдельный ingest:

- metadata;
- MIME/type;
- size;
- malware/static scan, если исполняемое;
- sensitivity;
- local/cloud storage policy;
- OCR/transcription при необходимости;
- project link;
- trust domain.

Вложение не исполняется только потому, что пришло от знакомого человека.

## B.5. Реконструкция контекста ответа

Для ответа Dennett формирует `Communication Context Bundle`:

```yaml
communication_context_bundle:
  latest_thread_window: refs
  unresolved_questions: []
  participant_relationship_context: refs
  active_project_context: refs
  promises_and_obligations: refs
  current_authoritative_facts: refs
  relevant_past_user_messages: refs
  disclosure_policy: ref
  communication_preferences: refs
  current_user_availability: optional
  uncertainty_and_conflicts: []
```

### B.5.1. Приоритет текущих источников

Если человек спрашивает о готовности проекта:

- repository/test/runtime state выше старой памяти;
- календарь выше старой заметки о расписании;
- текущий permission выше прошлой привычки;
- provider thread выше summary, если они расходятся.

### B.5.2. Стиль пользователя

Стиль строится из релевантных примеров:

- тот же человек или социальная группа;
- тот же канал;
- похожая ситуация;
- текущая степень формальности;
- исправленные пользователем drafts;
- выраженные preferences.

Нельзя слепо копировать случайные фразы или выдавать личную информацию только ради «персонализации».

### B.5.3. Over-personalization gate

Личная память применяется только если улучшает ответ. Она не должна:

- упоминать ненужные личные факты;
- поддакивать пользователю вместо объективного ответа;
- раскрывать скрытый контекст собеседнику;
- делать ответ странно знакомым там, где пользователь обычно нейтрален.

## B.6. Четыре независимых решения

### B.6.1. Content

Что по существу нужно сообщить.

### B.6.2. Style

Как это сформулировать.

### B.6.3. Disclosure

Какие сведения и attachments допустимо раскрыть конкретному recipient.

### B.6.4. Delivery

Что делать с готовым содержанием:

- не отвечать;
- сохранить draft;
- показать пользователю;
- отправить после подтверждения;
- отправить по standing bounded pattern;
- schedule;
- передать другому каналу.

Эти решения могут использовать одну модель, несколько последовательных checks или часть deterministic rules. Они не обязаны быть четырьмя отдельными model calls.

## B.7. Режимы автономности коммуникации

### Draft-only

Dennett может свободно готовить drafts, но не отправляет.

### Confirm-before-send

Каждый внешний ответ требует подтверждения, но варианты и контекст готовятся автоматически.

### Bounded autonomous

Разрешены конкретные patterns:

- короткое подтверждение определённым людям;
- заранее одобренные автоматические отчёты;
- ответы внутри заданной темы;
- routing служебных уведомлений;
- message templates с лимитами раскрытия.

### Contextual autonomous

Оркестратор может отправлять low-consequence сообщение при высокой уверенности и заранее установленной политике. Этот режим не должен быть default для новых contacts или чувствительных данных.

### Emergency/preauthorized

Отдельные сценарии, заранее определённые пользователем. Нельзя выводить emergency authority из общей памяти или единичной фразы.

## B.8. Ручная команда пользователя

Если пользователь говорит:

> «Ответь Ивану: да, буду в шесть»

Dennett:

1. разрешает identity `Иван` в конкретный contact/thread;
2. показывает ambiguity, если контактов несколько;
3. формирует exact proposal;
4. проверяет current user session/assurance;
5. отправляет без дополнительного utility-review, если permission достаточен;
6. сохраняет receipt.

Пользовательская явная команда не отменяет проверку exact recipient и unknown-effect safety.

## B.9. Draft lifecycle

```text
draft candidate
→ local editable draft
→ provider draft optional
→ user/agent revision
→ approved send proposal
→ sent/scheduled/abandoned/superseded
```

Google Gmail API рассматривает drafts как отдельный ресурс, который затем может быть отправлен; Dennett сохраняет этот conceptual separation даже для каналов без provider-native drafts. [[S21]]

Draft имеет owner:

- user-owned;
- Dennett-generated;
- shared/project;
- provider-managed.

Автоматическое обновление user-edited draft не должно перетирать правки. Новое предложение создаётся как revision или diff.

## B.10. Отправка и reconciliation

### B.10.1. Safe dispatch

```text
freeze exact proposal revision
→ acquire effect claim
→ validate permission and account health
→ dispatch once with idempotency/context handle
→ record provider operation reference
→ wait for provider evidence/update
→ settle receipt
```

### B.10.2. Timeout

Если timeout произошёл после dispatch:

- state становится `UNKNOWN`, если provider не гарантирует отсутствие эффекта;
- повтор не выполняется автоматически;
- connector пытается найти сообщение по provider operation ID, client-generated ID, thread/time/content tuple или provider history;
- при невозможности reconciliation вопрос выносится пользователю только если действительно нужен.

AWS рекомендует caller-provided request IDs и проверку идентичности intent при retry; Dennett использует тот же принцип для сообщений и других внешних эффектов. [[S23]]

### B.10.3. Provider-specific semantics

- TDLib: окончание отправки подтверждается update, а не только первоначальным response. [[S19]]
- Telegram Bot API: HTTP success может быть authority для send request, но webhook-inline method не даёт результат; такой путь не используется для consequential send без дополнительного reconciliation. [[S20]]
- Email: `202 Accepted` у Microsoft Graph означает принятие request, а не доказательство прочтения; provider-specific status сохраняется без ложного перевода в `delivered`. [[S22]]

## B.11. Schedule и delayed send

Scheduled message хранит:

- original intent;
- exact content revision;
- recipient;
- account;
- requested local time + resolved instant;
- timezone;
- validity window;
- permission at creation;
- revalidation policy.

Перед отправкой повторно проверяются:

- recipient/account still valid;
- permission не отозван;
- draft не superseded;
- событие ещё актуально;
- timezone conversion;
- duplicate not sent elsewhere.

## B.12. Несколько устройств

Если desktop и phone одновременно отвечают на одну Inbox card:

- command несёт card revision;
- Head принимает первый действительный transition;
- второй получает `already resolved` и фактический результат;
- duplicate send не происходит.

Локально отредактированные drafts синхронизируются как revisions. При конфликте:

- текстовые изменения могут быть merged/compared;
- recipients/attachments не объединяются молча;
- user chooses or agent proposes explicit resolution.

## B.13. Отмена, edit и recall

Возможности зависят от provider.

Dennett показывает отдельно:

- можно отменить до dispatch;
- можно удалить локальный draft;
- можно edit sent message;
- можно recall/delete for everyone;
- можно только отправить correction;
- ничего нельзя сделать.

Нельзя показывать универсальную кнопку `Undo Send`, если provider не предоставляет window или гарантии.

## B.14. Ошибки и recovery

### Auth expired

- draft сохраняется;
- send proposal не теряется;
- connector переходит `auth-required`;
- после reauth proposal revalidates.

### Rate limit

- retry-after учитывается;
- пользователь видит delay;
- urgent message может предложить другой channel только с явным recipient mapping.

### Wrong recipient discovered before send

- proposal invalidated;
- старое approval invalidated;
- новый recipient требует новое решение.

### Wrong recipient discovered after send

- stop further disclosures;
- recall/delete if possible;
- prepare correction;
- incident + memory correction;
- do not pretend rollback guaranteed.

### Connector stale

- show freshness;
- fetch current thread before consequential reply;
- no hidden reply based only on old cached messages.

### Incoming update gap

- connector marks cursor gap;
- backfill/provider history request;
- thread remains `possibly stale` until reconciled.

## B.15. Связь с памятью

Memory Fabric получает:

- source message evidence;
- meaningful relationship facts;
- promises/commitments;
- user edits to drafts;
- sent receipt;
- user approval/rejection;
- communication outcome.

Не сохраняются автоматически как глобальная truth:

- все сообщения;
- inferred personality of another person;
- every draft;
- hidden provider metadata;
- unverified semantic conclusions.

Retention и scope зависят от thread sensitivity и user policy.

## B.16. Наблюдаемость и оценка

Метрики:

- draft acceptance/edit rate;
- send-without-confirmation approval rate;
- wrong-recipient incidents;
- disclosure violations;
- duplicate send prevention;
- unknown-effect rate;
- reconciliation success;
- response latency;
- thread freshness errors;
- style similarity with user corrections;
- unnecessary notification rate;
- ignored-vs-answered regret.

## B.17. Антиоверинижиниринговые ограничения

Не создавать:

- отдельного агента на Content, Style, Disclosure и Delivery по умолчанию;
- workflow для каждого ответа;
- универсальную социальную онтологию;
- автоматический полный импорт всей переписки в prompt;
- собственный messenger protocol при наличии mature client API;
- LLM-вызов для exact recipient validation;
- retry без provider reconciliation;
- постоянный background responder для каждого контакта.

## B.18. Критерии готовности

- draft и send различены;
- exact recipient/content/attachment revision фиксируется;
- provider-specific confirmation не переобобщается;
- timeout не создаёт duplicate send;
- style memory не расширяет disclosure;
- manual user command выполняется быстро;
- incoming updates deduplicate и gap-detect;
- multi-device double resolution безопасен;
- thread freshness видима;
- external content не становится authority.

## B.19. Карта будущего переноса

- `20_Dennett_Agentic_Control_Fabric.md`: content/style/disclosure/delivery reasoning и delegation.
- `30_Dennett_Trust...md`: authorization, disclosure, recipients, standing mandates.
- `41_Dennett_Capabilities...md`: connector/account lifecycle.
- `50_Dennett_Server...md`: dispatch, effect claim, reconciliation, cursor sync.
- `10_Dennett_Memory...md`: thread evidence, social context, retention.
- `60/61 UI`: Inbox, drafts, quick replies, send review.
- `01 Shared Contracts`: Send Proposal и Delivery Receipt при необходимости.

---
