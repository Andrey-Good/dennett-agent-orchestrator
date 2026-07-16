# Модуль J. Import, Export and Portable Package Compatibility Contract

> **Канонический cross-domain supplement · `J`**  
> **Primary owner:** 41 Capability Fabric and 50 Server staging.  
> **Происхождение:** выделено из предархитектурного gap-аудита. Документ актуален и нормативен.  
> Маркеры источников вида `[[Sxx]]` раскрыты в [`REFERENCES.md`](REFERENCES.md). Ownership и порядок чтения описаны в [`README.md`](README.md).


## J.1. Назначение

Dennett должен переносить данные и функциональные пакеты между:

- устройствами одного пользователя;
- разными установками Dennett;
- пользователями;
- проектами;
- версиями приложения;
- Dennett и внешними инструментами.

Но «экспорт» означает разные вещи для project memory, whole-installation backup, skill, artifact, settings и capability profile. Один универсальный ZIP без типизированного manifest быстро станет неразбираемым и небезопасным.

## J.2. Классы переносимых пакетов

### Project Package

Проектная память, instructions, capability requirements, selected artifacts и references.

### Artifact Package

Exact artifact version + representations + provenance.

### Skill/Capability Package

Skill/plugin/procedure с dependencies и origin.

### Settings Package

Пользовательские настройки, profiles и UI layout без secrets по умолчанию.

### Automation Package

Trigger + action/procedure + required capabilities + safety assumptions.

### Research Package

Sources, evidence, claims, conclusions и unresolved gaps.

### Installation Transfer Package

Полная или частичная миграция установки; не равна shareable export.

### Backup Snapshot

Recovery-oriented encrypted state; не предназначен для безопасного обмена между пользователями.

## J.3. Общий Portable Package Manifest

```yaml
portable_package_manifest:
  package_id: id
  package_type: typed
  format_version: version
  created_by_product_version: version
  created_at: time
  creator_principal_or_installation: optional
  intended_use: transfer | backup | share | publish | import
  payload_inventory: []
  content_hashes: []
  schema_refs: []
  dependencies: []
  optional_components: []
  sensitivity_classes: []
  encryption: optional
  signatures: []
  provenance_ref: optional
  compatibility:
    min_reader: optional
    max_reader: optional
    required_features: []
  import_policy_hint: optional
```

## J.4. Разделение integrity, trust, permission и truth

Проверенный checksum/signature доказывает целостность и publisher identity, но не:

- безопасность содержимого;
- истинность claims;
- отсутствие prompt injection;
- право исполнять scripts;
- право раскрывать included data;
- совместимость с текущим project.

Import проходит отдельные Trust и privacy gates.

## J.5. Packaging strategy

Dennett-native package может быть обычной директорией/архивом с:

- manifest;
- payload;
- metadata;
- checksums;
- optional signatures;
- human-readable index.

BagIt является полезным reference: directory layout, arbitrary payload, descriptive tag files и checksum manifests для надёжного хранения/переноса без необходимости понимать внутреннюю семантику payload. [[S47]]

RO-Crate полезен как optional projection для research/software artifacts и linked metadata; он не обязан быть внутренним hot-path format. [[S46]]

## J.6. Manifest/schema versioning

JSON Schema может описывать package manifests и validation rules, но package version всё равно задаёт business semantics. [[S48]]

Importer выполняет:

1. parse envelope/version;
2. verify integrity;
3. validate known fields;
4. preserve unknown optional metadata;
5. reject unknown required semantics;
6. choose migration adapter;
7. never execute during parse.

## J.7. Import lifecycle

```text
select/receive package
→ copy to quarantine/staging
→ verify size, paths, checksums and signatures
→ parse manifest without execution
→ inventory payload and dependencies
→ privacy/security scan
→ compatibility analysis
→ show import plan
→ user/policy selects scope
→ migrate/normalize
→ create imported trust domain
→ rebuild derived indexes
→ bind local accounts/capabilities separately
→ validate result
→ promote to active scope
```

## J.8. Path and archive safety

Importer blocks:

- path traversal;
- absolute paths outside staging;
- dangerous symlink resolution;
- device files;
- decompression bombs;
- undeclared executable hooks;
- fetch URLs without policy;
- case/canonicalization collisions;
- unsupported filenames with clear report.

BagIt itself notes URL/path and payload security considerations; integrity is not protection from malicious payload. [[S47]]

## J.9. Selective export

User chooses export classes:

- public/shareable;
- project-only;
- include raw sources;
- include generated artifacts;
- include history;
- include only current state;
- include capabilities by reference or payload;
- exclude private overlays;
- include encrypted recipient-specific subset.

Before export Dennett generates a privacy inventory:

- personal memory;
- contacts/messages;
- secrets/credentials;
- device paths;
- usernames/emails;
- hidden prompts/policies;
- copyrighted/licensed content;
- raw ambient media;
- external URLs and access requirements.

## J.10. Project Memory Package

Recommended components:

```text
.dennett/memory/
  manifest.yaml
  index.md
  events/
  notes/
  decisions/
  research/
  procedures/
  instructions/
  schemas/
  views/
```

Export may use this repository-resident pack directly or create a sanitized bundle.

Rules:

- derived indexes omitted;
- personal/global overlays omitted;
- secrets omitted;
- stable refs either resolved into package or listed external;
- recipient can rebuild search;
- Git-friendly segments reduce conflicts;
- imported pack mounts, not auto-merges globally.

## J.11. Capability package

Contains:

- human-readable description;
- required tools/providers;
- scripts/assets/references;
- version/origin/license;
- effects/scopes;
- compatibility;
- evaluation history optional;
- no active credentials.

User-owned manual import adds to Collection, while executable authorization remains separate.

## J.12. Settings export

Default includes:

- UI layouts;
- execution profiles;
- voice preferences;
- notification rules;
- provider aliases without secrets;
- project templates;
- keyboard shortcuts;
- accessibility.

Optional encrypted export may include connector/account metadata, but access tokens are normally reauthenticated.

Settings merge rules:

- preview diff;
- choose replace/merge/select;
- project-specific does not overwrite global silently;
- unknown settings preserved or reported;
- no automatic security weakening.

## J.13. Artifact export

Select exact version and representations. Manifest includes provenance and integrity. External references can be:

- embedded;
- linked;
- omitted with report.

Portable export never silently points to private local paths.

## J.14. Whole-installation transfer

Different from share/export:

- encrypted;
- intended for same owner;
- may include private memory and vault wrappers;
- requires recovery/ownership proof;
- includes Head/runtime metadata, but active external effects reconciled;
- old installation revoked after handoff according to policy.

## J.15. Unknown/newer package version

Options:

- import read-only metadata;
- preserve package unopened;
- update Dennett;
- use compatibility translator;
- reject with precise missing feature list.

Never partially import required semantics while reporting success.

## J.16. Migration and round-trip

For each format:

- `import(export(x))` preserves canonical meaning;
- order/nonsemantic formatting may differ;
- stable IDs preserved or mapped explicitly;
- provenance records transformation;
- private data exclusions testable;
- package can be exported again without silent loss of unknown fields where possible.

## J.17. Multi-platform variants

A package may contain several optional artifacts/backends. OCI image index is a useful precedent for selecting platform-specific manifests from a higher-level index; Dennett can use the principle for local model/runtime assets without adopting OCI for all packages. [[S50]]

Example:

- Windows script;
- macOS script;
- Linux script;
- generic instructions;
- no executable for mobile.

Importer selects compatible component and reports omitted variants.

## J.18. External object references

Reference records include:

- URI/provider;
- expected identity/hash/version;
- required auth;
- availability;
- whether export is complete without it;
- fetch policy.

Import does not automatically fetch remote executable content.

## J.19. Licensing and attribution

Package can declare:

- license;
- source attribution;
- redistribution constraints;
- model/data license;
- unknown status.

Dennett warns but does not pretend to provide legal judgment.

## J.20. Failure and recovery

### Corrupt package

Reject before activation; report exact files/checksums.

### Partial transfer

Resume by content hashes/chunks; no duplicate import.

### Import crash

Staging transaction can resume/rollback; active registry not partially mutated.

### Missing dependency

Import object inactive with resolution plan; data remains readable where possible.

### Malicious package

Quarantine, no execution, incident/report, allow inspect as data.

### Private data detected during export

Block or require explicit per-item decision; produce sanitized version.

## J.21. Evaluation

- round-trip fidelity;
- checksum detection;
- path traversal/decompression tests;
- secret/privacy leak scan;
- cross-version migration;
- unknown-field preservation;
- second-user import;
- missing capability substitution;
- large package resume;
- Git merge of project memory segments.

## J.22. Антиоверинижиниринговые ограничения

Не создавать:

- one universal package containing every Dennett concept;
- custom cryptographic format when standard primitives suffice;
- automatic execution on import;
- mandatory RO-Crate/OCI/BagIt for internal hot path;
- active credentials in ordinary project share;
- permanent support for all historical versions without migration policy.

## J.23. Критерии готовности

- package types separated;
- common manifest/version/integrity defined;
- selective export/privacy scan;
- import staged/quarantined;
- trust separate from signature;
- credentials not transferred by default;
- unknown required semantics fail clearly;
- round-trip tested;
- project pack interoperable and Git-friendly.

## J.24. Карта будущего переноса

- `10 Memory`: project/research packs, provenance.
- `41 Capability`: skill/plugin packages.
- `50 Server`: import/export execution, whole-install transfer.
- `30 Trust`: signature/trust/privacy.
- `D Artifact`: artifact packages.
- `60/61 UI`: import/export previews and controls.
- architecture data volume: physical formats/schemas.

---
