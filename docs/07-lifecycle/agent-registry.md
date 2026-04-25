[English](#english) | [Русский](#russian)

# English

## Agent Registry

Status: normative owner for the local agent registry working surface.

Related documents:

- [Canonical spec](../../agent_orchestrator_final_spec_v2.md)
- [Agent JSON contract](../03-contracts/agent-json/README.md)
- [Draft, Live, and Deploy](./draft-live-deploy.md)
- [Versioning Axes](./versioning-axes.md)
- [State](../05-state/README.md)
- [ADR-0002: Agent File vs Local State](../09-adrs/ADR-0002-agent-file-vs-local-state.md)

## Purpose

The agent registry exists so Core can work with agents as a local operating surface without moving the source of truth away from portable agent files. It allows interfaces and background processes to discover known agents, resolve the current live revision, attach drafts, and link agents to chats and events.

The registry is a local index. It is not a second canonical model of the agent.

## Ownership Boundary

The agent file owns:

- `meta.id`, `meta.name`, `meta.description`, `meta.agent_version`, and every other portable field inside the file;
- the graph structure, node configuration, bindings, and runtime-facing semantics defined by the contract;
- whether the file is valid under the supported `graph_contract_version`.

The registry owns only local working-surface facts:

- which valid agent identities are known to Core;
- which local revision is currently marked `live`;
- which local draft revisions belong to the same logical agent;
- local revision ids, timestamps, fingerprints, and validation snapshots;
- associations from agents or revisions to chats, resume records, events, and UI shortcuts;
- local availability or conflict state discovered during indexing.

Cached metadata derived from the file remains derivative. If the registry cache disagrees with the file on disk, the file wins after the next refresh or validation pass.

## Minimum Registry Model

A compliant registry should expose at least the following concepts:

- A logical agent record keyed by the validated agent identity from `meta.id`.
- Portable references such as `orchestrator_agent.agent_ref` and event agent bindings resolve against that same logical identity.
- Exactly zero or one `live` revision for that logical agent.
- Zero or more draft revisions attached to the same logical agent.
- A local availability state for each tracked revision.
- Links from the logical agent or a specific revision to chats, resume records, and event bindings.

This model intentionally separates logical identity from local revisions. A logical agent can outlive any single file path.

## Local Availability States

At minimum, the registry should distinguish these local operating states:

- `available`: the referenced file exists and the last successful validation matched the stored fingerprint.
- `missing`: the registry expects a file or revision, but the current path no longer resolves.
- `invalid`: the file exists, but the latest validation failed.
- `conflicted`: Core found multiple incompatible local candidates for the same logical agent identity and cannot choose safely.

These states are local operating facts. They are not portable contract fields and must not be written back into the agent file.
If the tracked live file is edited out of band and its bytes no longer match the registry's live fingerprint, the registry must treat that live artifact as `conflicted` until an explicit lifecycle reconciliation or deploy restores alignment.

## Discovery and Indexing Rules

Registering or refreshing an agent starts from the file system, not from SQLite or another local store.

Core should perform these steps:

1. Read the candidate file.
2. Validate it against the supported contract.
3. Extract the canonical identity and portable metadata from the validated bytes.
4. Update the registry entry and cached metadata only after successful validation.

If validation fails:

- Core may keep a path-scoped local error record so the user can inspect the problem.
- Core must not promote the invalid file to the live definition of the agent.
- Core must not invent fallback metadata to keep the registry looking healthy.

## Identity Conflicts

The registry must treat duplicate `meta.id` values as an explicit conflict when they point to different local revisions that cannot be explained by the current draft/live model.

Core must not silently merge such files, overwrite one path with another, or pick the newest timestamp as truth. Conflict resolution requires an explicit lifecycle action from the user or another higher-level workflow.

## Live Resolution

Normal open and run flows resolve a logical agent reference to its current `live` revision.

In this document, a logical agent reference includes:

- a direct open or run request for an agent;
- an event binding that names a logical agent;
- `orchestrator_agent.agent_ref`.

Drafts are never selected implicitly for ordinary runs, event dispatch, or user shortcuts. A draft is used only when the caller explicitly asks to inspect, test, or continue work on that draft.

If the tracked live file has been edited out of band and no longer matches the registry's live fingerprint, ordinary live resolution must fail as `conflicted` rather than implicitly adopting the edited bytes as a new live revision.

If a different revision is used, that choice must come from a higher-level workflow that explicitly pinned it, such as a chat or resume record, an explicit draft-targeted action, or another explicit revision-scoped workflow. The portable `agent_ref` value itself never encodes that pin.

This rule protects events, chats, and routine launches from accidental attachment to unfinished edits.

## Registry Updates and Atomicity

The registry must follow successful file operations, not lead them.

That means:

- the registry may stage local pending state during an edit or deploy workflow;
- the current live pointer changes only after the new live file has been written atomically and validated again;
- a failed write, failed validation, or interrupted deploy leaves the previous live revision active.

The registry is therefore downstream from the durable file operation, even when both are updated within one Core command.

## Associations to Chats, Resume, and Events

Associations have different levels of granularity:

- chats and resume records should bind to a specific local revision, because an in-flight conversation must remain reproducible even after later deploys;
- event bindings, triggers, and most interface shortcuts should bind to the logical agent identity and resolve the current live revision at dispatch time;
- registry associations must never become hidden extensions of the portable agent contract.

## Failure Semantics

The registry must fail visibly instead of synthesizing a replacement truth source.

Examples:

- If the live file disappears, the logical agent becomes unavailable for new runs until the user repairs or reindexes it.
- If the current live file becomes invalid, Core blocks new launches instead of falling back to an arbitrary draft.
- If an imported file is valid but unsupported because of `graph_contract_version`, Core may index it as a known artifact with an incompatible state, but it must not run or deploy it as the current live revision.

## What the Registry Must Never Become

The registry must not become:

- the only surviving copy of a live agent;
- a place where hidden agent fields are added outside the contract;
- a policy engine that rewrites file meaning;
- a portability mechanism that assumes local SQLite rows can replace the JSON artifact across machines.

# Russian

## Реестр агентов

Статус: нормативный владелец локальной рабочей поверхности реестра агентов.

Связанные документы:

- [Каноническая спецификация](../../agent_orchestrator_final_spec_v2.md)
- [Контракт Agent JSON](../03-contracts/agent-json/README.md)
- [Draft, Live и Deploy](./draft-live-deploy.md)
- [Оси версионирования](./versioning-axes.md)
- [Состояние](../05-state/README.md)
- [ADR-0002: Agent File vs Local State](../09-adrs/ADR-0002-agent-file-vs-local-state.md)

## Назначение

Реестр агентов нужен для того, чтобы Core мог работать с агентами как с локальной рабочей поверхностью, не перенося источник истины из переносимых agent files. Он позволяет интерфейсам и фоновым процессам находить известных агентов, разрешать текущую live-ревизию, привязывать drafts и связывать агентов с чатами и событиями.

Реестр является локальным индексом. Он не является второй канонической моделью агента.

## Граница владения

Agent file владеет:

- `meta.id`, `meta.name`, `meta.description`, `meta.agent_version` и всеми остальными переносимыми полями файла;
- структурой графа, конфигурацией нод, bindings и runtime-facing семантикой, определенной контрактом;
- фактом валидности файла под поддерживаемый `graph_contract_version`.

Реестр владеет только локальными фактами рабочей поверхности:

- какие валидные идентичности агентов известны Core;
- какая локальная ревизия сейчас помечена как `live`;
- какие локальные draft-ревизии принадлежат тому же логическому агенту;
- локальными revision ids, timestamps, fingerprints и снимками результата валидации;
- связями агентов или ревизий с чатами, resume records, событиями и UI-shortcuts;
- локальным состоянием доступности или конфликта, обнаруженным при индексации.

Кэшированные метаданные, производные от файла, остаются производными. Если кэш реестра расходится с файлом на диске, после следующего refresh или validation побеждает файл.

## Минимальная модель реестра

Совместимый реестр должен как минимум предоставлять следующие понятия:

- Логическая запись агента, ключом которой является валидированный идентификатор агента из `meta.id`.
- Переносимые ссылки вроде `orchestrator_agent.agent_ref` и event bindings агента разрешаются относительно той же самой логической идентичности.
- Ровно ноль или одна `live`-ревизия для этого логического агента.
- Ноль или более draft-ревизий, принадлежащих тому же логическому агенту.
- Локальное состояние доступности для каждой отслеживаемой ревизии.
- Связи логического агента или конкретной ревизии с чатами, resume records и event bindings.

Эта модель сознательно разделяет логическую идентичность и локальные ревизии. Логический агент может пережить любой конкретный путь к файлу.

## Локальные состояния доступности

Как минимум реестр должен различать следующие локальные рабочие состояния:

- `available`: целевой файл существует, а последняя успешная валидация совпала с сохраненным fingerprint.
- `missing`: реестр ожидает файл или ревизию, но текущий путь больше не разрешается.
- `invalid`: файл существует, но последняя валидация завершилась ошибкой.
- `conflicted`: Core обнаружил несколько несовместимых локальных кандидатов для одной и той же логической идентичности и не может безопасно выбрать один.

Эти состояния являются локальными операционными фактами. Они не являются переносимыми полями контракта и не должны записываться обратно в agent file.

## Правила discovery и indexing

Регистрация или refresh агента начинается с файловой системы, а не с SQLite или любого другого локального хранилища.

Core должен выполнять следующие шаги:

1. Прочитать кандидатный файл.
2. Провалидировать его против поддерживаемого контракта.
3. Извлечь каноническую идентичность и переносимые метаданные из валидированных байтов.
4. Обновить запись реестра и кэшированные метаданные только после успешной валидации.

Если валидация не прошла:

- Core может хранить path-scoped локальную запись об ошибке, чтобы пользователь мог ее изучить.
- Core не должен продвигать невалидный файл в роль live-определения агента.
- Core не должен придумывать fallback-метаданные только для того, чтобы реестр выглядел здоровым.

## Конфликты идентичности

Реестр обязан трактовать дубли `meta.id` как явный конфликт, если они указывают на разные локальные ревизии, которые нельзя объяснить текущей моделью draft/live.

Core не должен молча сливать такие файлы, перезаписывать один путь другим или выбирать в качестве истины самый новый timestamp. Разрешение конфликта требует явного lifecycle-действия пользователя или другого более высокого workflow.

## Разрешение live-версии

Обычные сценарии открытия и запуска должны разрешать логическую ссылку на агента в его текущую `live`-ревизию.

В этом документе логическая ссылка на агента включает:

- прямой запрос на открытие или запуск агента;
- event binding, который называет логического агента;
- `orchestrator_agent.agent_ref`.

Drafts никогда не выбираются неявно для обычных run-ов, event-dispatch или пользовательских shortcuts. Draft используется только тогда, когда вызывающая сторона явно попросила просмотреть, протестировать или продолжить работу именно с этим draft.

Если используется другая ревизия, такой выбор должен исходить из более высокого workflow, который явно зафиксировал ее, например из chat/resume record, явного draft-targeted действия или другого явного revision-scoped workflow. Само переносимое значение `agent_ref` никогда не кодирует такую фиксацию.

Это правило защищает события, чаты и типовые запуски от случайной привязки к незавершенным правкам.

## Обновления реестра и атомарность

Реестр должен следовать за успешными файловыми операциями, а не опережать их.

Это означает:

- реестр может держать локальное pending-состояние во время workflow редактирования или deploy;
- текущий live-pointer меняется только после того, как новый live-файл атомарно записан и заново провалидирован;
- неудачная запись, неудачная валидация или прерванный deploy оставляют предыдущую live-ревизию активной.

Тем самым реестр является downstream относительно надежной файловой операции, даже если оба обновления происходят внутри одной команды Core.

## Связи с chat, resume и events

Связи имеют разную степень детализации:

- chats и resume records должны привязываться к конкретной локальной ревизии, потому что активный разговор должен оставаться воспроизводимым даже после следующих deploy;
- event bindings, triggers и большинство interface shortcuts должны привязываться к логической идентичности агента и разрешать текущую live-ревизию в момент dispatch;
- связи реестра никогда не должны становиться скрытыми расширениями переносимого контракта агента.

## Семантика отказов

Реестр обязан отказывать явно, а не синтезировать заменяющий источник истины.

Примеры:

- Если live-файл исчез, логический агент становится недоступным для новых run-ов, пока пользователь не восстановит его или не выполнит reindex.
- Если текущий live-файл стал невалидным, Core блокирует новые запуски вместо fallback на произвольный draft.
- Если импортированный файл валиден, но несовместим по `graph_contract_version`, Core может индексировать его как известный артефакт в несовместимом состоянии, но не должен запускать его или делать текущей live-ревизией.

## Чем реестр никогда не должен становиться

Реестр не должен превращаться:

- в единственную оставшуюся копию live-агента;
- в место, где вне контракта добавляются скрытые поля агента;
- в policy engine, который переписывает смысл файла;
- в механизм переносимости, предполагающий, что локальные SQLite-строки могут заменить JSON-артефакт между машинами.
