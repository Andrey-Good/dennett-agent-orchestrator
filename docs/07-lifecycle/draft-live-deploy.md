[English](#english) | [Русский](#russian)

# English

## Draft, Live, and Deploy

Status: normative owner for the local editing and publication lifecycle of agent revisions.

Related documents:

- [Canonical spec](../../agent_orchestrator_final_spec_v2.md)
- [Agent Registry](./agent-registry.md)
- [Versioning Axes](./versioning-axes.md)
- [State](../05-state/README.md)
- [Builder Agent](../08-extensions/builder-agent.md)
- [ADR-0002: Agent File vs Local State](../09-adrs/ADR-0002-agent-file-vs-local-state.md)

## Purpose

Core needs an explicit editing lifecycle so unfinished work does not silently replace the working agent. This document defines how editable drafts relate to the current live revision and what it means to deploy a new live version.

## Core Concepts

- `draft`: an editable local candidate revision of a logical agent.
- `live`: the local revision currently treated as the default working version for normal opens and runs.
- `deploy`: an explicit publication action that promotes one chosen draft or validated candidate into the next live revision.

These concepts are local lifecycle concepts. They do not replace `graph_contract_version` or `meta.agent_version`.

## Lifecycle Shape

A logical agent may have:

- zero or one current `live` revision;
- zero or more draft revisions;
- zero or more historical revisions retained by local implementation policy.

A new agent may exist as drafts only before its first deploy. Once a live revision exists, it is the default target for normal opens, manual runs, event dispatch, and interface shortcuts unless the caller explicitly asks for a draft.

## What a Draft Is

A draft is editable working state inside Core and the local environment.

A draft:

- may be represented by a file, temporary file, managed file path, or another local revision surface;
- may be edited directly, including when the chosen surface is an existing agent file;
- must remain attributable to one logical agent identity once it validates successfully;
- may diverge from the current live revision without affecting live behavior;
- is allowed to be incomplete, invalid, or unsupported while the user or builder is still working on it.

A draft is not automatically a portable publication artifact just because it can be serialized to JSON.
Likewise, directly editing the tracked live file path does not by itself count as deploy.

## What Live Means

`live` means only one thing: this is the local default revision for new work.

`live` does not mean:

- newest file on disk;
- highest `meta.agent_version`;
- highest `graph_contract_version`;
- version of the product itself.

Only one live revision may be active per logical agent at a time.

## Normal Resolution Rules

The system should resolve targets as follows:

- normal open and run commands target the current live revision;
- explicit draft commands target the selected draft revision;
- chats and resume records stay bound to the revision they started on;
- event dispatch resolves the logical agent to the current live revision at dispatch time.

Drafts must never become the implicit default for events or routine launches.
If a user or tool edits the tracked live file out of band, that byte change does not silently become the new live revision; Core must treat it as a lifecycle mismatch that still requires explicit reconciliation or deploy before ordinary live use.

## Deploy Semantics

Deploy is an explicit publication step. A compliant deploy flow should:

1. Select the source draft or validated candidate revision.
2. Validate the candidate bytes against the supported contract.
3. Confirm that its `graph_contract_version` is runnable by the current Core.
4. Materialize the new live file or live revision artifact.
5. Write it atomically.
6. Revalidate the bytes that became durable.
7. Advance the registry live pointer only after the durable write succeeds.

If any step fails before step 7, the previous live revision remains active.

## Atomicity Requirement

Deploy and other lifecycle writes must follow the atomic file-write rule from the canonical spec:

1. write to a temporary file in the same directory;
2. flush the file buffer;
3. fsync the file;
4. atomically replace the target;
5. fsync the directory when the platform allows it.

The lifecycle model depends on this rule because registry state must never point at a half-written live artifact.

## Relationship to Builder Work

Builder workflows should produce drafts by default.

A builder may recommend a deploy or execute one only when the user or system policy explicitly grants that authority. Builder experimentation, candidate comparison, and intermediate validation must not silently publish a new live revision.

## Relationship to Chats and Resume

An active run or resumable chat stays attached to the revision it resolved when the run began.

Later deploys do not rewrite that binding. If the original revision becomes unavailable, resume should fail explicitly rather than silently switching the chat to a newer live revision.

This preserves reproducibility and prevents conversational drift.

## Relationship to Events

Events and triggers bind to the logical agent identity, not to whichever draft was last edited.

When an event is dispatched:

- Core resolves the current live revision.
- That revision is what runs.
- Later deploys affect only future event dispatches.

This rule keeps background or external launches independent from unfinished edits.

## Deleting or Replacing Drafts

Deleting a draft does not affect the current live revision.

Replacing one draft with another does not count as deploy. A draft becomes authoritative for routine work only through an explicit deploy.

## Non-Goals

This document does not require:

- a particular folder layout for drafts or live files;
- a mandatory history UI;
- automatic promotion of drafts after successful validation;
- automatic rollback systems beyond preserving the previous live revision until deploy completes.

Those capabilities may be added later, but they must preserve the lifecycle semantics defined here.

# Russian

## Draft, Live и Deploy

Статус: нормативный владелец локального жизненного цикла редактирования и публикации ревизий агента.

Связанные документы:

- [Каноническая спецификация](../../agent_orchestrator_final_spec_v2.md)
- [Реестр агентов](./agent-registry.md)
- [Оси версионирования](./versioning-axes.md)
- [Состояние](../05-state/README.md)
- [Builder Agent](../08-extensions/builder-agent.md)
- [ADR-0002: Agent File vs Local State](../09-adrs/ADR-0002-agent-file-vs-local-state.md)

## Назначение

Core нужен явный жизненный цикл редактирования, чтобы незавершенная работа не подменяла молча рабочий агент. Этот документ определяет, как editable drafts соотносятся с текущей live-ревизией и что именно означает deploy новой live-версии.

## Базовые понятия

- `draft`: редактируемая локальная кандидатная ревизия логического агента.
- `live`: локальная ревизия, которая сейчас считается версией по умолчанию для обычного открытия и запуска.
- `deploy`: явное действие публикации, которое продвигает один выбранный draft или другой валидированный кандидат в следующую live-ревизию.

Эти понятия относятся к локальному lifecycle. Они не заменяют `graph_contract_version` или `meta.agent_version`.

## Форма жизненного цикла

Логический агент может иметь:

- ноль или одну текущую `live`-ревизию;
- ноль или более draft-ревизий;
- ноль или более исторических ревизий, если этого требует локальная политика реализации.

Новый агент может существовать только в виде drafts до первого deploy. После появления live-ревизии именно она становится целью по умолчанию для обычного открытия, ручных запусков, event-dispatch и interface shortcuts, если вызывающая сторона явно не попросила draft.

## Что такое draft

Draft — это редактируемое рабочее состояние внутри Core и локального окружения.

Draft:

- может представляться файлом, временным файлом, управляемым file path или другой локальной поверхностью ревизии;
- после успешной валидации должен оставаться привязанным к одной логической идентичности агента;
- может расходиться с текущей live-ревизией, не влияя на live-поведение;
- имеет право быть неполным, невалидным или несовместимым, пока пользователь или builder продолжают с ним работать.

Draft не становится автоматически переносимым publish-артефактом только потому, что его можно сериализовать в JSON.

## Что означает live

`live` означает только одно: это локальная ревизия по умолчанию для новой работы.

`live` не означает:

- самый новый файл на диске;
- самый высокий `meta.agent_version`;
- самый высокий `graph_contract_version`;
- версию самого продукта.

Для одного логического агента одновременно может быть активна только одна live-ревизия.

## Правила нормального разрешения

Система должна разрешать цели следующим образом:

- обычные команды открытия и запуска адресуют текущую live-ревизию;
- явные draft-команды адресуют выбранную draft-ревизию;
- chats и resume records остаются привязанными к той ревизии, с которой был начат run;
- event-dispatch разрешает логического агента в текущую live-ревизию в момент dispatch.

Drafts никогда не должны становиться неявным default для событий или рутинных запусков.

## Семантика deploy

Deploy — это явный шаг публикации. Совместимый deploy workflow должен:

1. Выбрать исходный draft или другую валидированную кандидатную ревизию.
2. Провалидировать байты кандидата против поддерживаемого контракта.
3. Подтвердить, что его `graph_contract_version` запускаем текущим Core.
4. Материализовать новый live-файл или live-артефакт ревизии.
5. Записать его атомарно.
6. Повторно провалидировать байты, ставшие надежно записанными.
7. Сдвинуть live-pointer в реестре только после успешной надежной записи.

Если любой шаг до шага 7 завершается ошибкой, предыдущая live-ревизия остается активной.

## Требование атомарности

Deploy и другие lifecycle-записи обязаны следовать правилу атомарной записи файлов из канонической спецификации:

1. запись во временный файл в той же директории;
2. flush файлового буфера;
3. fsync файла;
4. атомарная замена целевого файла;
5. fsync директории, когда платформа это позволяет.

Модель жизненного цикла опирается на это правило, потому что состояние реестра никогда не должно указывать на наполовину записанный live-артефакт.

## Связь с работой builder

Builder workflow должен по умолчанию производить drafts.

Builder может рекомендовать deploy или выполнять его только тогда, когда пользователь или системная политика явно выдали ему такое право. Эксперименты builder, сравнение кандидатов и промежуточная валидация не должны молча публиковать новую live-ревизию.

## Связь с chat и resume

Активный run или resumable chat остаются привязанными к той ревизии, которая была разрешена в момент запуска.

Последующие deploy не переписывают эту привязку. Если исходная ревизия перестала быть доступной, resume должен завершаться явной ошибкой, а не молча переключать чат на более новую live-ревизию.

Это сохраняет воспроизводимость и предотвращает дрейф разговора.

## Связь с событиями

События и триггеры привязываются к логической идентичности агента, а не к тому draft, который редактировали последним.

Когда событие dispatch-ится:

- Core разрешает текущую live-ревизию.
- Запускается именно она.
- Последующие deploy влияют только на будущие dispatch событий.

Это правило сохраняет независимость фоновых и внешних запусков от незавершенных правок.

## Удаление и замена drafts

Удаление draft не влияет на текущую live-ревизию.

Замена одного draft другим не считается deploy. Draft становится авторитетным для обычной работы только через явный deploy.

## Что не требуется

Этот документ не требует:

- конкретной структуры каталогов для drafts или live-файлов;
- обязательного UI истории;
- автоматического продвижения drafts после успешной валидации;
- автоматических rollback-систем сверх сохранения предыдущей live-ревизии активной до завершения deploy.

Такие возможности могут появиться позже, но они должны сохранять lifecycle-семантику, определенную здесь.
