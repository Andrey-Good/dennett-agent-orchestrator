# Модуль D. Artifact Lifecycle Contract

> **Канонический cross-domain supplement · `D`**  
> **Primary owner:** 20 Agentic Control and Artifact surfaces.  
> **Происхождение:** выделено из предархитектурного gap-аудита. Документ актуален и нормативен.  
> Маркеры источников вида `[[Sxx]]` раскрыты в [`REFERENCES.md`](REFERENCES.md). Ownership и порядок чтения описаны в [`README.md`](README.md).


## D.1. Назначение

Artifact — это сохраняемый результат работы пользователя, агента, workflow или внешнего инструмента, который имеет самостоятельную ценность вне одной реплики чата.

Примеры:

- документ или Markdown-файл;
- исследовательское досье;
- презентация;
- изображение, видео или аудио;
- diagram;
- source-code patch;
- build/package;
- design variant;
- report;
- dataset;
- notebook;
- workflow definition;
- export bundle;
- screenshot или selected capture, если он сохранён как результат;
- final answer, явно материализованный пользователем.

Artifact не равен:

- transient model output;
- internal reasoning;
- tool log;
- Memory Evidence Object, хотя один объект может быть связан с artifact;
- provider attachment;
- project file автоматически, если он не зарегистрирован как результат.

## D.2. Главная модель

> Artifact — это версионируемая сущность с содержимым, происхождением, владельцем, статусом пригодности и правилами распространения. Сам файл является payload, а не всей сущностью.

W3C PROV полезно разделяет Entity, Activity и Agent и позволяет описывать происхождение, derivation и ответственность; Denet использует этот принцип облегчённо, не навязывая PROV как hot-path format. [[S27]]

## D.3. Канонические сущности

### D.3.1. Artifact Record

```yaml
artifact_record:
  artifact_id: id
  title: text
  kind: typed
  owner_ref: principal_or_project
  project_ref: optional
  created_by_run_or_session: optional
  lifecycle_state: draft | candidate | approved | final | superseded | archived | revoked | deleted
  current_version_ref: ref
  visibility: private | project | shared | public
  sensitivity: typed
  retention_policy_ref: ref
  created_at: time
  updated_at: time
```

### D.3.2. Artifact Version

```yaml
artifact_version:
  version_id: id
  artifact_ref: ref
  version_label: optional
  content_objects: []
  manifest_ref: optional
  format: typed
  content_hashes: []
  created_by: ref
  derived_from: []
  source_evidence: []
  validation_results: []
  created_at: time
  immutable_after_publish: boolean
```

Released/shared version не изменяется на месте; новая правка создаёт новую version. Этот принцип соответствует SemVer rule о неизменности опубликованной версии и практикам object versioning. [[S31]] [[S28]]

### D.3.3. Artifact Representation

Один artifact может иметь несколько представлений:

- editable source;
- rendered preview;
- thumbnail;
- PDF export;
- HTML;
- image frames;
- transcript;
- compressed/mobile rendition;
- redacted shareable rendition.

Representation не становится отдельным artifact, если это только техническое представление той же версии.

### D.3.4. Artifact Relationship

- derived-from;
- variant-of;
- supersedes;
- translates;
- summarizes;
- bundles;
- references;
- validates;
- published-as.

### D.3.5. Publication Record

Фиксирует внешний effect:

- куда опубликовано;
- exact version;
- visibility;
- provider ID/URL;
- timestamp;
- permission;
- ability to revoke/edit;
- receipt.

## D.4. Как artifact создаётся

Artifact может появиться:

- по явной команде пользователя;
- как completion contract Task/Run;
- через `Save as Artifact` из чата;
- из project files;
- из research synthesis;
- из capture;
- из provider-generated output;
- из import;
- из automation.

Создание выполняет минимально:

1. назначить owner/scope;
2. сохранить payload;
3. связать source Session/Run/evidence;
4. создать first version;
5. определить draft/candidate state;
6. не объявлять artifact финальным без основания.

## D.5. Жизненные состояния

### DRAFT

Редактируемая работа, не заявленная как готовая.

### CANDIDATE

Вариант, предлагаемый для выбора или review.

### APPROVED

Пользователь или authorised process подтвердил пригодность для заданной цели, но artifact может ещё не быть опубликован.

### FINAL

Текущая каноническая версия результата в данном scope. `FINAL` не означает вечную неизменность artifact record; следующая версия может supersede.

### SUPERSEDED

Существует новая выбранная версия. Старый artifact остаётся доступным для history/provenance.

### Artifact state: ARCHIVED

Не участвует в обычных suggestions и workflows, но сохранён.

### REVOKED

Artifact или publication больше не должен использоваться/распространяться, хотя исторический record может сохраняться.

### DELETED

Payload удалён согласно policy; остаётся content-free tombstone/provenance при необходимости.

## D.6. Versioning

### D.6.1. Autosave revisions

Частые editor autosaves могут храниться как lightweight revisions/checkpoints, а не полноценные named versions.

### D.6.2. Meaningful versions

Создаются при:

- пользовательском save milestone;
- agent completion;
- approval;
- publication;
- format conversion with semantic differences;
- branch/variant;
- external update/import.

### D.6.3. Version labels

Для human-facing artifact допустимы:

- v1, v2;
- draft-3;
- approved-2026-07-12;
- semantic version для пакетов/API;
- provider revision.

Не все artifacts обязаны использовать SemVer.

### D.6.4. Object versioning

Content store может сохранять несколько versions для восстановления после overwrite/delete, как S3 Versioning. [[S28]] Но product lifecycle не должен зависеть от конкретного object store.

## D.7. Варианты и сравнение

Несколько candidate artifacts могут:

- существовать параллельно;
- иметь common parent;
- сравниваться по content-aware diff;
- объединяться в новую version;
- быть отклонены без удаления;
- хранить user feedback.

Для дизайна, текста и архитектуры предпочтительнее сравнивать целостные варианты, а не разрезать их на не связанные микрорезультаты.

## D.8. Approval и finalization

Approval всегда имеет scope:

- approved for internal use;
- approved for project;
- approved for sharing with named recipients;
- approved for publication;
- approved as final deliverable;
- approved for automation reuse.

Approval одного scope не переносится автоматически в другой.

Agent может предложить `FINAL`, но authoritative transition выполняет:

- пользователь;
- completion contract с объективной проверкой;
- preauthorized automation;
- designated reviewer.

## D.9. Sharing и публикация

Перед share/export:

- выбирается exact version;
- проверяются sensitivity и embedded secrets;
- выявляются linked private objects;
- создаётся redacted rendition при необходимости;
- проверяются license/attribution;
- фиксируется recipient/audience;
- создаётся Publication Record.

### D.9.1. Share link

Link может быть:

- immutable snapshot;
- latest-version pointer;
- expiring;
- authenticated;
- downloadable;
- view-only;
- revocable.

UI должен объяснять, изменится ли видимый контент при новой version.

### D.9.2. Public release

GitHub Releases — пример представления named release с assets, notes и связанным tag; Denet использует такую модель для software deliverables, но не навязывает её каждому artifact. [[S29]]

### D.9.3. Revocation

Revocation может:

- закрыть Denet-managed link;
- удалить remote object, если provider поддерживает;
- пометить superseded/revoked;
- уведомить recipients;
- не гарантирует удаление уже скачанной копии.

## D.10. Artifact и project files

Если artifact основан на файле проекта:

- source path/commit фиксируется;
- artifact version может ссылаться на file snapshot;
- последующее изменение файла не переписывает artifact history;
- user может выбрать `Track latest` или `Snapshot`;
- удаление project file не обязательно удаляет final artifact.

Code patch может быть artifact до применения; после merge он сохраняет relation к commit/PR.

## D.11. Artifact и память

Memory хранит:

- факт создания;
- purpose;
- user choice;
- artifact summary;
- evidence/lineage;
- feedback;
- publication outcome.

Memory не должна копировать весь payload в каждый note. Используются stable handles.

Artifact может быть evidence для будущих claims, но approval не делает каждое утверждение внутри artifact истинным навсегда.

## D.12. Import artifact

```text
receive file/bundle/link
→ identify format and source
→ preserve original bytes
→ inspect active content/macros/scripts
→ extract metadata
→ choose owner/project
→ create imported version
→ quarantine executable parts if needed
→ build previews/indexes
```

Import не означает publication или trust.

## D.13. Delete semantics

Separate operations:

- remove from recent/library view;
- archive;
- delete one representation;
- delete version;
- delete artifact payload;
- revoke publications;
- delete all lineage-sensitive data where allowed.

Если version опубликована или использована другим artifact:

- dependency graph показывается;
- deletion may preserve tombstone;
- derived artifacts не удаляются автоматически, но provenance updates.

## D.14. Storage tiers

- hot: active drafts/current previews;
- warm: project artifacts/recent versions;
- cold: superseded/archived versions;
- external: provider/remote repository;
- ephemeral: generated previews/caches.

Canonical payload cannot be silently evicted. Rebuildable representations may be dropped first.

## D.15. Failures

### Generation interrupted

Partial artifact remains draft with completeness status.

### Rendering failed

Source version remains valid; representation marked failed/retryable.

### Publication timeout

Publication becomes `UNKNOWN`; reconcile provider before retry.

### Imported artifact changed upstream

New import version, not silent overwrite.

### Broken external link

Artifact remains with stale external representation and available local metadata/snapshot according to policy.

### Validation failed

Version remains candidate/draft; validation result linked.

## D.16. Observability и evaluation

- artifact creation success;
- version explosion;
- render latency;
- lost draft rate;
- wrong version shared;
- secret leakage;
- provenance completeness;
- restore success;
- publication reconciliation;
- user approval/rejection;
- duplicate artifacts;
- storage by tier.

## D.17. Антиоверинижиниринговые ограничения

Не создавать:

- DAM enterprise taxonomy для личной установки;
- отдельный workflow для каждого save;
- mandatory SemVer для документа/изображения;
- полную копию каждого autosave навсегда;
- автоматическую публикацию из `FINAL`;
- one-size-fits-all diff;
- отдельный artifact service как обязательный микросервис.

## D.18. Критерии готовности

- payload отделён от record/version;
- exact version shareable;
- draft/candidate/final различены;
- publication is external effect;
- revoke limitations honest;
- provenance preserved;
- deletion dependency-aware;
- project file changes do not rewrite history;
- partial generation recoverable.

## D.19. Карта будущего переноса

- `01 Shared Contracts`: Artifact Descriptor/Version/Publication reference.
- `20 Agentic`: artifact as output/completion/variant.
- `10 Memory`: provenance, evidence, retention.
- `30 Trust`: sharing, publication, secrets.
- `50 Server`: storage, rendering, effect reconciliation.
- `60/61 UI`: gallery, preview, compare, approval/share.

---
