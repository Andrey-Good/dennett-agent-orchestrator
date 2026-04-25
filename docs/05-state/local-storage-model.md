[English](#english) | [Русский](#russian)

<a id="english"></a>
# English

# Local Storage Model

Status: normative.

Related documents:

- [`README.md`](./README.md)
- [`chat-and-resume.md`](./chat-and-resume.md)
- [`atomic-write-policy.md`](./atomic-write-policy.md)
- [`../02-architecture/runtime-integration-model.md`](../02-architecture/runtime-integration-model.md)
- [`../07-lifecycle/README.md`](../07-lifecycle/README.md)
- [`../../agent_orchestrator_final_spec_v2.md`](../../agent_orchestrator_final_spec_v2.md)

## 1. Scope

This document defines what local storage may contain, what it must never replace, and what logical records must exist so execution and resume remain deterministic.

## 2. Source-Of-Truth Matrix

The base matrix is:

- agent graph definition: file truth from the agent JSON file;
- chat state: local-core truth;
- resume state: local-core truth;
- trigger/event configuration: separate local truth outside the agent file;
- agent registry and indexes: derived local metadata, never canonical graph truth.

If local metadata disagrees with the agent file about agent definition, the file wins.

## 3. Allowed Storage Layers

The base model may use:

- canonical JSON files for portable agent artifacts;
- SQLite for derived operational state;
- local sidecar files for critical file-backed artifacts when explicitly governed by policy.

The base model must not introduce a hidden remote source of truth for the same responsibilities.

## 4. Logical Records The Local Store Must Support

The implementation may choose any concrete schema, but it must support the following logical records.

### 4.1. Chat Record

A chat record must identify:

- the chat itself;
- the agent or run it belongs to;
- the immutable resolved revision identity the chat is bound to;
- the effective chat policy snapshot;
- whether visible transcript and context window are stored;
- current resume capability metadata.

### 4.2. Run Record

A run record must identify:

- the logical agent reference, when one exists;
- the immutable resolved revision identity actually launched for this run;
- the entry node used for the run;
- whether the run started directly, from an event, or from explicit resume;
- the current terminal or non-terminal run status;
- timestamps needed to order attempts and resumptions.

### 4.3. Node Attempt Record

A node attempt record must identify:

- run identity;
- `node_id`;
- attempt sequence within the run;
- declared `output` contract;
- classified node outcome;
- whether the attempt is durably blocked on an unresolved built-in user-chat prompt;
- runtime-facing handles needed for native resume, if any;
- references to committed output and resume metadata, if committed.

Because a node id may be revisited, the local store must support more than one attempt record per `node_id` and run.

### 4.4. Node Output Journal

The node output journal must preserve committed final outputs only.

It must allow the system to answer:

- what was the latest committed successful output of a node in this run;
- what explicit `output` contract that result satisfied;
- which committed result feeds `node.<node_id>.*` references.

### 4.5. Current Vars Materialization

The local store must preserve the current committed `vars` state for the run.

It may store:

- the full current map;
- a mutation log plus a recoverable materialized view;
- or both.

Whatever representation is chosen, it must deterministically produce the current committed `vars` snapshot at any resume boundary.

### 4.6. Resume Metadata Record

Resume metadata must identify:

- the immutable resolved revision identity that continuation must keep using;
- whether native resume is available;
- whether local resume is available;
- the last durable execution boundary;
- any unresolved blocking built-in user-chat prompt state needed to restore a blocked continuation;
- any runtime-native session handle needed for native continuation;
- any local context snapshot needed for local continuation.

### 4.7. Secret Side Records

If secret markers are enabled, the store must support secret side records that remain separate from visible transcript and public node outputs.

## 5. Derived Metadata Is Allowed, Canonical Rewrite Is Not

The local store may keep derived metadata such as:

- agent indexes;
- file paths and digests;
- cached parse results;
- chat-to-agent links;
- run summaries.

The local store must not:

- become the canonical definition of an agent graph;
- silently override agent-file fields;
- redefine execution order;
- turn vendor session internals into public portable contract fields.

## 6. Commit Boundaries

The local store must expose clear commit boundaries.

At minimum:

- run creation must commit a run record before the run becomes discoverable;
- node-attempt start must commit enough metadata to show that work began;
- node-attempt completion must atomically commit the node outcome and all allowed derived state updates;
- a blocking built-in user-chat prompt must atomically commit the blocked-attempt state and pending-prompt metadata before the run is treated as durably waiting on user input;
- resume metadata must advance in the same durable step as the state it depends on.

A resume pointer must never point past the last durable state update.

## 7. File Truth vs Local Truth

File truth and local truth solve different problems.

Mandatory rules:

- file truth owns portable agent definition;
- local truth owns chat and resume continuity;
- local truth may index draft/live lifecycle entities, but lifecycle meaning stays outside this document;
- disagreement between file definition and local derived metadata is resolved in favor of the file for agent-definition questions.

## 8. SQLite Boundary

SQLite is the permitted local operational infrastructure in the locked base stack.

It is permitted for:

- chat metadata;
- resume metadata;
- node attempt bookkeeping;
- local indexes and operational summaries.

It is forbidden as:

- the canonical agent definition store;
- the portable transport format for agents;
- a place where graph meaning is rewritten independently of the file contract.

## 9. Crash-Recovery Rules

Crash recovery must preserve these invariants:

- readers observe only committed state;
- a node success is visible only after its output and derived updates are durable;
- unfinished writes never masquerade as committed execution;
- orphaned temporary files do not become canonical state automatically.

File-backed crash safety is governed further by [`atomic-write-policy.md`](./atomic-write-policy.md).

## 10. Memory Is Not Local Resume State

If the product later supports memory sources, they remain a separate extension axis.

The local storage model defined here must not collapse memory into:

- `vars`;
- chat transcript;
- resume metadata.

## 11. Acceptance Criteria For An Implementation

An implementation conforms to this document only if:

- it preserves file truth for agent definitions;
- it stores chat/resume truth locally;
- it supports multiple node attempts for the same `node_id` in one run;
- it keeps resumable chats and runs bound to their stored resolved revision identity;
- it can reconstruct the latest committed `vars` and node outputs at a durable boundary;
- it can restore a durably recorded blocking built-in user-chat prompt as pending instead of inventing node completion;
- it never promotes derived metadata into canonical graph truth.

<a id="russian"></a>
# Русский

# Модель локального хранилища

Статус: нормативный.

Связанные документы:

- [`README.md`](./README.md)
- [`chat-and-resume.md`](./chat-and-resume.md)
- [`atomic-write-policy.md`](./atomic-write-policy.md)
- [`../02-architecture/runtime-integration-model.md`](../02-architecture/runtime-integration-model.md)
- [`../07-lifecycle/README.md`](../07-lifecycle/README.md)
- [`../../agent_orchestrator_final_spec_v2.md`](../../agent_orchestrator_final_spec_v2.md)

## 1. Область действия

Этот документ определяет, что локальное хранилище может содержать, что оно никогда не должно заменять и какие логические записи обязаны существовать, чтобы execution и resume оставались детерминированными.

## 2. Матрица источников истины

Базовая матрица такова:

- определение графа агента: файловая истина из agent JSON file;
- chat state: локальная истина core;
- resume state: локальная истина core;
- конфигурация trigger/event: отдельная локальная истина вне agent file;
- agent registry и indexes: производные локальные метаданные, но никогда не каноническая truth графа.

Если локальные метаданные расходятся с agent file в вопросе определения агента, побеждает файл.

## 3. Разрешенные слои хранения

Базовая модель может использовать:

- канонические JSON files для переносимых agent artifacts;
- SQLite для производного операционного состояния;
- локальные sidecar files для критичных file-backed artifacts, когда это явно разрешено policy.

Базовая модель не должна вводить скрытый удаленный источник истины для тех же зон ответственности.

## 4. Логические записи, которые должно поддерживать локальное хранилище

Реализация может выбрать любую конкретную схему, но обязана поддерживать следующие логические записи.

### 4.1. Chat record

Chat record обязан идентифицировать:

- сам chat;
- агент или run, к которому он относится;
- неизменяемую resolved revision identity, к которой привязан этот chat;
- effective snapshot chat policy;
- факт хранения видимого transcript и context window;
- текущие metadata о возможности resume.

### 4.2. Run record

Run record обязан идентифицировать:

- ссылку на логического агента, если она есть;
- неизменяемую resolved revision identity, которая фактически была запущена для этого run;
- `entry_node_id`, использованный в run;
- способ старта: прямой запуск, запуск от события или explicit resume;
- текущее terminal или non-terminal состояние run;
- timestamps, нужные для упорядочивания попыток и возобновлений.

### 4.3. Node attempt record

Node attempt record обязан идентифицировать:

- идентичность run;
- `node_id`;
- порядковый номер попытки внутри run;
- объявленный контракт `output`;
- классифицированный node outcome;
- факт того, что попытка durably blocked неразрешенным built-in user-chat prompt;
- runtime-facing handles, нужные для native resume, если они есть;
- ссылки на committed output и resume metadata, если они зафиксированы.

Поскольку `node_id` может посещаться повторно, локальное хранилище обязано поддерживать более одной attempt record на одну `node_id` в рамках одного run.

### 4.4. Журнал outputs нод

Журнал outputs нод обязан хранить только committed final outputs.

Он должен позволять системе ответить:

- каков последний committed successful output ноды в этом run;
- какому явному контракту `output` соответствовал этот результат;
- какой committed result питает ссылки `node.<node_id>.*`.

### 4.5. Текущее materialized состояние `vars`

Локальное хранилище обязано сохранять текущее committed состояние `vars` для run.

Оно может хранить:

- полную текущую карту;
- mutation log плюс восстанавливаемое materialized view;
- или обе формы.

Какое бы представление ни было выбрано, оно обязано детерминированно восстанавливать текущий committed snapshot `vars` на любой resume boundary.

### 4.6. Resume metadata record

Resume metadata обязаны идентифицировать:

- неизменяемую resolved revision identity, которую continuation обязано продолжать использовать;
- доступен ли native resume;
- доступен ли local resume;
- последнюю durable execution boundary;
- любое неразрешенное состояние blocking built-in user-chat prompt, нужное для восстановления blocked continuation;
- любой runtime-native session handle, нужный для native continuation;
- любой local context snapshot, нужный для local continuation.

### 4.7. Secret side records

Если secret markers включены, хранилище обязано поддерживать отдельные secret side records, физически отделенные от видимого transcript и публичных node outputs.

## 5. Производные метаданные разрешены, канонический rewrite запрещен

Локальное хранилище может держать производные метаданные, такие как:

- agent indexes;
- file paths и digests;
- cached parse results;
- связи chat-to-agent;
- summaries runs.

Локальное хранилище не имеет права:

- становиться каноническим определением agent graph;
- молча переопределять поля agent file;
- переопределять execution order;
- превращать внутренности vendor session в публичные portable contract fields.

## 6. Границы commit

Локальное хранилище обязано иметь ясные commit boundaries.

Как минимум:

- создание run должно фиксировать run record до того, как run станет discoverable;
- старт node attempt должен фиксировать достаточно metadata, чтобы было видно, что работа началась;
- завершение node attempt должно атомарно фиксировать node outcome и все разрешенные производные обновления состояния;
- blocking built-in user-chat prompt обязан атомарно фиксировать blocked-state попытки и metadata ожидающего prompt до того, как run считается durably waiting на user input;
- resume metadata должны продвигаться в том же durable шаге, что и состояние, от которого они зависят.

Resume pointer никогда не может указывать дальше последнего durable state update.

## 7. Файловая истина против локальной истины

Файловая truth и локальная truth решают разные задачи.

Обязательные правила:

- file truth владеет переносимым определением агента;
- local truth владеет непрерывностью chat и resume;
- local truth может индексировать lifecycle-сущности draft/live, но их смысл остается вне этого документа;
- расхождение между файловым определением и локальными производными metadata разрешается в пользу файла во всех вопросах определения агента.

## 8. Граница SQLite

SQLite является разрешенной локальной операционной инфраструктурой в рамках зафиксированного base stack.

Они разрешены для:

- metadata чатов;
- metadata resume;
- учета node attempts;
- локальных индексов и операционных summaries.

Они запрещены как:

- каноническое хранилище определения агента;
- переносимый transport format для агентов;
- место, где смысл графа переписывается независимо от файлового контракта.

## 9. Правила восстановления после сбоев

Восстановление после сбоев обязано сохранять следующие инварианты:

- читатели видят только committed state;
- node success становится видимым только после того, как его output и производные обновления стали durable;
- незавершенные записи никогда не маскируются под committed execution;
- осиротевшие temporary files не становятся каноническим состоянием автоматически.

Дополнительные правила file-backed crash safety задаются в [`atomic-write-policy.md`](./atomic-write-policy.md).

## 10. Memory не равно local resume state

Если продукт позже поддержит memory sources, они останутся отдельной extension axis.

Локальная модель хранения, заданная здесь, не может схлопывать memory в:

- `vars`;
- chat transcript;
- resume metadata.

## 11. Критерии приемки реализации

Реализация соответствует этому документу только если:

- она сохраняет file truth для определений агентов;
- она хранит chat/resume truth локально;
- она поддерживает несколько node attempts для одной `node_id` в одном run;
- chats и runs, которые можно resume-ить, остаются привязанными к сохраненной resolved revision identity;
- она умеет восстанавливать последние committed `vars` и node outputs на durable boundary;
- она умеет восстанавливать durably recorded blocking built-in user-chat prompt как ожидающий, а не придумывать завершение ноды;
- она никогда не повышает производные metadata до канонической truth графа.
