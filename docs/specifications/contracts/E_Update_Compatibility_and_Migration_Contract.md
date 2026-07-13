# Модуль E. Update, Compatibility and Migration Contract

> **Канонический cross-domain supplement · `E`**  
> **Primary owner:** 50 Server Runtime.  
> **Происхождение:** выделено из предархитектурного gap-аудита. Документ актуален и нормативен.  
> Маркеры источников вида `[[Sxx]]` раскрыты в [`REFERENCES.md`](REFERENCES.md). Ownership и порядок чтения описаны в [`README.md`](README.md).


## E.1. Назначение

Denet является распределённой персональной системой: Head Runtime, desktop, mobile, device agents, schemas, provider adapters, skills, plugins, project memory packs и backups могут обновляться в разное время. Поэтому обновление — не просто замена binary.

Этот модуль определяет:

- что версия означает;
- кто совместим с кем;
- как распространяется update;
- как мигрируют данные;
- как выполняется rollback;
- как изолируются внешние extensions;
- как система продолжает работать при mixed versions.

## E.2. Классы версионируемых объектов

1. Denet release.
2. Head Runtime protocol.
3. Desktop/mobile client.
4. Device agent.
5. Canonical data schema.
6. Event/command contract.
7. Memory projection/index schema.
8. Portable pack format.
9. Capability/provider adapter.
10. Skill/plugin/MCP package.
11. Workflow/procedure definition.
12. Artifact/export format.
13. Backup manifest.

Эти versions не обязаны совпадать.

## E.3. Version Manifest

```yaml
version_manifest:
  product_version: semver_or_channel_version
  build_id: immutable
  release_channel: stable | preview | nightly | pinned
  protocol_min: version
  protocol_max: version
  data_schema_version: version
  event_schema_versions: []
  pack_format_versions: []
  component_versions: {}
  migration_set: []
  signature_metadata: ref
  released_at: time
```

SemVer полезен только при объявленном публичном contract: incompatible change повышает MAJOR, compatible feature — MINOR, bug fix — PATCH; опубликованный package не изменяется на месте. [[S31]]

## E.4. Release channels

### Stable

Проверенный default.

### Preview/Beta

Новые функции с ограниченным support и явной возможностью возврата.

### Nightly/Development

Только для разработчиков/отдельного isolated installation.

### Pinned

Пользователь замораживает компонент или всю установку, понимая security/compatibility consequences.

Каналы могут различаться для core и extensions; нельзя автоматически перевести весь Denet на nightly из-за одного preview plugin.

## E.5. Подпись и supply-chain

Update package должен иметь:

- publisher identity;
- immutable build/content hashes;
- signed metadata;
- target platform;
- version;
- dependencies;
- rollback/revocation metadata.

TUF разработан для защиты update systems даже при компрометации части ключей или repository infrastructure; Denet architecture должна использовать TUF-подобную модель или зрелый platform updater с эквивалентными гарантиями, а не один скачанный JSON с URL. [[S30]]

## E.6. Compatibility negotiation

При соединении device/client и Head стороны обмениваются:

```yaml
compatibility_hello:
  component_id: id
  product_version: version
  protocol_range: [min, max]
  schema_capabilities: []
  feature_flags: []
  required_features: []
  migration_state: typed
```

Результат:

- full compatibility;
- compatible with feature downgrade;
- read-only compatibility;
- update required;
- migration required;
- unsupported/quarantine.

Kubernetes version-skew policies являются полезным precedent: компоненты имеют явно ограниченное окно совместимости, а не предположение, что любая версия может общаться с любой. [[S33]]

## E.7. Mixed-version operation

Во время rolling update:

- Head exposes negotiated protocol;
- old clients do not receive unsupported fields as required semantics;
- new fields are optional/defaulted until compatibility window closes;
- destructive migration waits for incompatible clients to disconnect/update or uses dual representation;
- UI marks hidden/unavailable features honestly.

Нельзя хранить canonical state, которое старый клиент при обычной записи молча уничтожит.

## E.8. Contract evolution

### E.8.1. Additive change

Preferred:

- add optional field;
- add new event type;
- add new capability facet;
- preserve unknown fields where format supports;
- old consumer ignores safely.

### E.8.2. Breaking change

Requires:

- new contract version;
- translation adapter;
- compatibility window;
- explicit migration;
- fallback/rollback plan.

Protocol Buffers documentation explicitly distinguishes wire-safe, unsafe and conditionally safe changes and preserves unknown fields; Denet adopts the principle even if another serialization format is chosen. [[S32]]

### E.8.3. Semantic change

Самая опасная: field name/type прежние, meaning changed. Она требует новой version/field/event, а не silent reinterpretation.

## E.9. Data migration lifecycle

```text
preflight inventory
→ verify backups and free space
→ quiesce affected writers or enable dual-write
→ create migration checkpoint
→ apply bounded migration
→ verify structural invariants
→ rebuild derived projections/indexes
→ semantic smoke tests
→ mark schema version
→ keep rollback/forward-fix window
```

### E.9.1. Canonical vs derived

- canonical events/evidence migrate conservatively;
- indexes/previews/cache rebuild instead of expensive in-place migration where possible;
- human-authored content not rewritten by model unless explicit semantic migration.

### E.9.2. Online migration

Allowed when:

- old/new readers coexist;
- dual read/write semantics clear;
- no ambiguity/loss.

Otherwise maintenance window is preferable to unsafe cleverness.

### E.9.3. Semantic migration

Например, changing project/permission meaning. Требует:

- source evidence;
- deterministic transform where possible;
- review of uncertain records;
- no mass LLM rewrite without audit/sample/rollback.

## E.10. Update order

Recommended dependency order may be:

1. backup/readiness;
2. Head compatibility layer;
3. server/data migration;
4. device agents;
5. desktop/mobile;
6. provider adapters/extensions;
7. optional features.

Но architecture may choose another order if contracts support it.

## E.11. Client updates

### Mandatory

Только если:

- security vulnerability;
- incompatible protocol after grace period;
- data corruption risk;
- revoked signing key/component.

### Recommended

Feature/fix, system remains usable.

### Deferred

User can postpone; background runs remain safe.

Update cannot interrupt consequential action without checkpoint/reconciliation.

## E.12. Extension/provider adapter isolation

Provider adapter/plugin update может:

- change OAuth scopes;
- change tool schemas;
- add executable hooks;
- alter endpoints;
- remove capability;
- change provider model IDs.

Therefore:

- adapter has independent version;
- new version staged/probed;
- active sessions may remain pinned;
- Trust re-review security-sensitive delta;
- rollback possible without core rollback;
- crash cannot take down Head if process/isolation chosen in architecture.

## E.13. Skills, MCP и project packages

- user-owned modifications never overwritten silently;
- upstream update uses three-way comparison/fork;
- package manifest declares compatibility;
- project may pin version;
- imported update remains untrusted until policy;
- executable delta gets stricter review than text.

## E.14. Rollback

Rollback types:

- binary rollback;
- adapter rollback;
- config rollback;
- data rollback;
- forward fix;
- restore from backup.

Data rollback is not always safe after new writes. Before migration, Denet records:

- rollback boundary;
- whether writes after boundary can be translated backward;
- whether old binaries may read new data;
- retention of migration snapshot.

If true rollback impossible, UI and operation plan must say `forward-fix only`.

## E.15. Failed update recovery

### Head fails before migration

Restart old version.

### Head fails during migration

Migration journal/checkpoint determines resume/rollback; never guess.

### Client updated, Head old

Feature downgrade or blocked connection according to negotiation.

### One device offline for months

On reconnect:

- authenticate;
- negotiate;
- update required/read-only;
- upload offline log through compatibility translator;
- never let obsolete client overwrite canonical newer structures.

### Extension causes crash

Quarantine extension, restore adapter version, keep core online.

## E.16. Update UX semantics

UI later shows:

- what updates;
- source/signature;
- restart impact;
- migration impact;
- required space/time;
- current backup status;
- affected devices;
- compatibility consequences;
- rollback availability.

`Update all` may exist only if plan is already computed and safe.

## E.17. Observability и evaluation

- update success/failure;
- migration duration;
- rollback rate;
- mixed-version errors;
- stale clients;
- extension crash isolation;
- schema invariant failures;
- data loss/corruption;
- protocol downgrade usage;
- update deferral;
- restore drill.

## E.18. Антиоверинижиниринговые ограничения

Не создавать:

- custom package manager for all ecosystems;
- universal backward compatibility forever;
- distributed rolling deploy complexity for single-device install;
- model-based migration where deterministic transform exists;
- independent protocol for every module;
- automatic major update without backup/readiness;
- microservice solely for version comparison.

## E.19. Критерии готовности

- signed immutable releases;
- compatibility range negotiated;
- mixed version behavior defined;
- canonical/derived migration separated;
- rollback limitations explicit;
- extension update isolated;
- offline old device scenario covered;
- migration backup and semantic smoke test required;
- no silent semantic contract change.

## E.20. Карта будущего переноса

- `50 Server`: updater, negotiation, migration runtime, device rollout.
- `30 Trust`: signatures, publisher trust, revoked packages.
- `41 Capability`: adapter/plugin/skill lifecycle.
- `10 Memory`: canonical/index migrations.
- `60/61 UI`: update controls/states.
- `01 Shared Contracts`: version/compatibility envelopes.
- architecture volumes: concrete package/protocol/schema choices.

---
