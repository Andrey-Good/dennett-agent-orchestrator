# Модуль C. Project Lifecycle Contract

> **Канонический cross-domain supplement · `C`**  
> **Primary owner:** 20 Agentic Control.  
> **Происхождение:** выделено из предархитектурного gap-аудита. Документ актуален и нормативен.  
> Маркеры источников вида `[[Sxx]]` раскрыты в [`REFERENCES.md`](REFERENCES.md). Ownership и порядок чтения описаны в [`README.md`](README.md).


## C.1. Назначение

Проект Denet — видимая рабочая область, обычно связанная с папкой, репозиторием или набором материалов и содержащая project sessions, память, capabilities, artifacts и настройки. Project lifecycle должен ясно отличать:

- состояние записи Denet;
- состояние локальных файлов;
- состояние Git checkout/worktree;
- состояние remote repository;
- project memory pack;
- связанные Tasks/Runs;
- credentials/connectors;
- exported/shareable representation.

Одна кнопка `Удалить проект` не может молча выполнять все эти действия сразу.

## C.2. Канонические сущности

### C.2.1. Project Record

```yaml
project_record:
  project_id: id
  title: text
  kind: code | research | design | document | media | automation | general
  owner_principal_ref: ref
  lifecycle_state: active | paused | archived | detached | transferring | deleted_record
  primary_workspace_ref: optional
  repository_bindings: []
  memory_space_ref: ref
  capability_set_ref: ref
  sessions: []
  artifact_collection_ref: ref
  created_at: time
  archived_at: optional
  tombstone_ref: optional
```

### C.2.2. Workspace Binding

Связь проекта с физической рабочей областью:

```yaml
workspace_binding:
  binding_id: id
  project_ref: ref
  node_ref: ref
  path_or_provider_ref: ref
  kind: folder | git_checkout | git_worktree | cloud_workspace | remote_runtime
  repository_identity: optional
  branch_or_revision: optional
  read_write_mode: typed
  availability: typed
  last_verified_at: time
  ownership: user | Denet | provider | external
```

Project Record может существовать без доступного Workspace Binding, например после отключения ноутбука или передачи проекта.

### C.2.3. Repository Binding

Содержит remote identity, default branch, repository UUID/URL, hosting account и trust state. Изменение URL не должно создавать новый проект автоматически, если repository identity подтверждена.

### C.2.4. Project Export

Версионируемый пакет переносимой части проекта, а не полный backup личной установки.

## C.3. Жизненные состояния проекта

### ACTIVE

Доступен для chat, runs, events и edits.

### PAUSED

Новые autonomous runs/events не запускаются, но проект видим и доступен для чтения/ручного продолжения.

### Project state: ARCHIVED

Проект скрыт из основных рабочих списков, фоновые automations отключены по умолчанию, состояние сохраняется. Archive обратим.

GitHub archive используется как полезный precedent: repository становится read-only и может быть unarchived; архивирование не равно удалению. [[S24]] В Denet archive не обязан делать Git repository read-only, если пользователь архивирует только Project Record, но UI должен объяснять выбранный scope.

### DETACHED

Project Record и память остаются, но текущий folder/repository binding отсутствует или намеренно отсоединён.

### TRANSFERRING

Идёт экспорт, перенос ownership или rebinding; destructive operations блокируются или координируются.

### DELETED_RECORD

Project Record удалён из активной системы и представлен tombstone по retention policy. Это не обязательно означает удаление файлов или remote repository.

## C.4. Операции жизненного цикла

## C.4.1. Create New

Источники:

- пустой project;
- существующая папка;
- clone repository;
- import project pack;
- artifact/research result;
- conversation/idea;
- duplicate/template.

Создание выполняет:

1. создаёт Project Record;
2. определяет или создаёт Workspace Binding;
3. создаёт project Memory Space;
4. обнаруживает instructions/capabilities;
5. устанавливает trust mode;
6. создаёт initial session только если пользователь сразу начинает работу;
7. не запускает тяжёлый анализ без причины.

## C.4.2. Attach Existing Folder

- folder не перемещается;
- ownership сохраняется;
- Denet анализирует structure read-only сначала;
- обнаруживает Git, project memory, AGENTS/CLAUDE files;
- пользователь выбирает trust;
- binding сохраняет canonical path/device;
- если folder недоступен позднее, project становится detached/stale, а не удаляется.

## C.4.3. Clone Repository

- создаётся новый local checkout;
- remote identity сохраняется;
- branch/revision фиксируется;
- credentials используются через connector/broker;
- project memory pack импортируется как отдельный trust domain;
- executable instructions не активируются до workspace trust.

## C.4.4. Rebind Path

Нужна при переносе папки, смене диска или device.

```text
select new candidate path
→ verify repository/content identity
→ compare expected markers
→ detect divergence
→ attach as same workspace / new replica / fork / reject
```

Нельзя связывать случайную папку только по одинаковому имени.

## C.4.5. Add Worktree/Replica

Git поддерживает несколько linked working trees одного repository. [[S26]] Denet использует это для изолированной параллельной работы, но Project Record остаётся один.

Каждый worktree имеет:

- branch;
- owner Run/Session;
- node;
- lifecycle;
- merge/discard state.

## C.4.6. Pause

Pause проекта:

- не отменяет текущие runs автоматически без выбора;
- default: stop new background starts, allow current reversible work to checkpoint;
- events переходят `suppressed-by-project-pause`;
- incoming messages/captures могут сохраняться, но не будят project agent;
- user can still open chat manually.

## C.4.7. Archive

Перед archive показываются:

- active Runs;
- schedules/events;
- unsaved/uncommitted changes;
- unresolved Inbox cards;
- project-local secrets/capabilities;
- sync status;
- export/backup state.

Варианты:

- archive record only;
- archive and pause automations;
- archive remote repository if connector supports it;
- create final snapshot/export.

Default — обратимый archive record + pause automations, без удаления файлов.

## C.4.8. Detach

Detach удаляет связь Denet с workspace, но не файлы.

Варианты:

- detach one node/path;
- detach all workspaces;
- keep project memory;
- keep/revoke project-local credentials;
- keep or disable automations.

## C.4.9. Remove Local Checkout

Это физическое удаление локальной папки, отличное от detach.

Перед выполнением:

- identify exact path;
- detect uncommitted/untracked files;
- check other worktrees;
- check backup/remote availability;
- offer archive/export;
- use recycle/trash/quarantine where possible;
- require Trust policy proportional to irreversibility.

## C.4.10. Delete Remote Repository

Отдельный high-consequence external effect.

GitHub предупреждает, что deletion permanently removes team permissions, private repository forks и может быть восстановлена только в некоторых случаях в ограниченное окно. [[S25]] Поэтому Denet:

- никогда не скрывает remote delete внутри общего `Delete Project`;
- показывает owner, repository, visibility и forks;
- требует fresh provider state;
- проверяет backup/export;
- создаёт exact Effect Claim;
- не повторяет при unknown result;
- сохраняет receipt/tombstone.

## C.4.11. Delete Project Record

Удаляет Denet metadata после проверки зависимостей.

Возможности:

- remove from Denet only;
- also delete project memory;
- keep portable memory pack;
- keep artifacts;
- detach files;
- revoke project grants;
- disable connectors/events.

Default не удаляет physical files и remote repository.

## C.4.12. Transfer to Another User

Transfer состоит из независимых операций:

1. создать shareable export;
2. исключить personal overlays/secrets;
3. определить artifact/project memory licensing;
4. передать repository ownership отдельно;
5. получатель импортирует pack как external trust domain;
6. новая установка создаёт собственные account bindings и grants;
7. source installation может оставить копию, archive или удалить по выбору.

Transfer не переносит автоматически:

- личную память;
- provider tokens;
- social context;
- global skills, если не включены явно;
- active agent sessions;
- standing permissions.

## C.4.13. Duplicate/Fork Project

Варианты:

- duplicate metadata and selected artifacts;
- clone repository to new remote;
- create branch/worktree;
- fork research/design project without code;
- include or exclude project memory history.

Новый проект получает новый `project_id`; lineage сохраняется.

## C.5. Project memory при lifecycle operations

### Project memory on Archive

Memory Space становится cold/readable; background consolidation может быть снижена.

### Project memory on Detach

Memory остаётся, repository facts marked stale until workspace available.

### Transfer

Создаётся sanitized portable projection, а не копия всего private space.

### Project memory on Delete

Запускается deletion graph с отдельными scopes:

- project record;
- project memory;
- raw evidence;
- shared artifacts;
- global promoted knowledge.

Продвинутые в global memory facts не удаляются автоматически только потому, что source project deleted; provenance становится unavailable/deleted according to policy, а privacy obligation обрабатывается отдельно.

## C.6. Project capabilities и connectors

При archive/pause:

- project-local capabilities disabled but retained;
- scheduled connectors stop;
- global capabilities unchanged.

При transfer:

- manifests/references export;
- executable packages optional;
- account bindings never export as active credentials;
- recipient sees missing/replacement capabilities.

При deletion:

- task-scoped grants revoke;
- project-scoped secrets revoke/delete according to ownership;
- shared global capability not removed.

## C.7. Active Runs и sessions

Любая destructive lifecycle operation first enumerates:

- foreground project sessions;
- Managed Runs;
- external effects in progress;
- worktrees;
- scheduled jobs;
- pending Inbox cards;
- provider sessions.

Possible policies:

- wait;
- checkpoint and pause;
- cancel reversible work;
- continue detached;
- transfer ownership;
- block operation due to unknown effect.

Не следует автоматически убивать один chat turn ради archive, если он может безопасно завершиться; но новые writes после lifecycle transition должны быть fenced.

## C.8. Import чужого проекта

```text
receive folder/repository/pack
→ establish source and integrity
→ register project as imported/untrusted
→ inspect memory/instructions/capabilities
→ show portability report
→ select local workspace
→ assign trust mode
→ rebuild indexes
→ connect user-owned providers/accounts
→ open project
```

Import никогда не наследует standing permissions предыдущего пользователя.

## C.9. Conflict scenarios

### Same repository attached as two projects

Denet предлагает:

- merge Project Records;
- keep separate views with shared workspace warning;
- bind branches/worktrees separately;
- detach duplicate.

### Folder moved while offline

Rebind via identity check; history not lost.

### Remote rewritten/replaced

Repository identity mismatch creates incident/choice, not silent rebind.

### Archive while automation running

Project transition waits/checkpoints and creates explicit decision if external effect pending.

### Delete record while artifacts shared elsewhere

Shared artifact ownership prevents cascading deletion unless explicit.

## C.10. UI-semantic requirements

UI later must expose distinct labels:

- Pause Project;
- Archive in Denet;
- Detach Folder;
- Remove Local Files;
- Delete Remote Repository;
- Delete Project Data;
- Export/Transfer.

Never place all under a visually ambiguous `Delete` without consequence summary.

## C.11. Observability

Log project lifecycle events:

- actor;
- exact scope;
- affected workspace/repository/memory/capabilities;
- precondition snapshot;
- result;
- reversible-until;
- external effect receipt;
- recovery path.

## C.12. Evaluation

Test:

- archive/unarchive;
- detach/rebind moved folder;
- missing device;
- uncommitted local files;
- active Run;
- unknown external effect;
- remote delete timeout;
- transfer sanitization;
- import by second user;
- duplicate repository detection;
- restore deleted record from backup where allowed.

## C.13. Антиоверинижиниринговые ограничения

Не создавать:

- enterprise project portfolio lifecycle;
- mandatory Kanban state for project lifecycle;
- separate microservice for archive;
- automatic remote delete as part of local cleanup;
- universal filesystem sync for every project;
- model call to distinguish exact path identity when hashes/Git metadata suffice.

## C.14. Критерии готовности

- project record, workspace, local files и remote repo независимы;
- archive обратим;
- detach не удаляет;
- physical and remote deletion explicit;
- active runs handled;
- transfer excludes private data and credentials;
- project memory behavior defined;
- rebind verifies identity;
- UI can state exact consequence in one sentence.

## C.15. Карта будущего переноса

- `20 Agentic`: Project Record lifecycle, sessions/runs interaction.
- `10 Memory`: project space archive/export/delete.
- `30 Trust`: destructive/transfer authorization.
- `41 Capability`: project capability/account detachment.
- `50 Server`: fencing, workspace availability, transfer/export execution.
- `60/61 UI`: command names and consequence previews.
- `01 Shared Contracts`: stable project/workspace references.


---
