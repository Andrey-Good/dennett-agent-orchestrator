# Часть IV. Обязательные сквозные сценарии дополнений

> **Канонический validation supplement.** Primary owner: `70_Denet_End_to_End_Validation_and_Architecture_Handoff.md`.  
> Файл закрывает предархитектурные сценарии и требования risk spikes. Источники раскрыты в [`REFERENCES.md`](REFERENCES.md).


## 26. Ambient microphone on phone

### Initial state

- owner enabled `Wake + Contextual Capture`;
- microphone permission granted;
- device on battery;
- Head online but cloud semantic analysis disabled.

### Ambient audio flow

```text
local VAD/wake
→ rolling encrypted ring buffer
→ speaker/activity detection
→ cheap duplicate/relevance gate
→ user says “запомни это для Denet”
→ committed turn window selected
→ local ASR
→ Ambient Candidate
→ Trust/privacy policy
→ project association proposal
→ Memory Event + Evidence
→ raw retention timer
→ selective sync
```

### Success

- unrelated background not uploaded;
- user can retrieve note;
- source/ASR confidence known;
- battery budget respected;
- mute works locally.

### Failure

If OS kills service, indicator/status shows source inactive; no fake continuous recording.

## 27. Event-driven screen context on PC

### Screen context flow

```text
window/app change
→ accessibility/DOM metadata
→ visual diff threshold
→ screenshot only when useful
→ redact excluded app/region
→ deduplicate static frames
→ link active project/task
→ commit evidence or expire candidate
```

When a UI error appears during project work, agent can retrieve exact screenshot and surrounding actions. Password manager/financial app excluded per policy.

### Storage pressure

Frequency/resolution drops first; canonical selected evidence preserved.

## 28. Incoming Telegram message and response

```text
TDLib update
→ deduplicate and update thread
→ reconstruct sender/project/current facts
→ decide content/style/disclosure/delivery
→ draft candidate
→ user says “отправь” or standing pattern applies
→ exact Send Proposal
→ Trust validation
→ TDLib sendMessage
→ wait updateMessageSendSucceeded
→ Delivery Receipt
→ memory update
```

If send status unknown, no automatic duplicate.

## 29. Manual quick reply from mobile notification

- notification action includes thread/message revision;
- phone authenticates user session;
- exact text/recipient shown;
- command goes to Head;
- Head rejects if thread/card superseded;
- optimistic UI becomes confirmed only after provider evidence.

## 30. Project archive with active Run

```text
user chooses Archive in Denet
→ enumerate active Run/worktree/events/unsynced files
→ offer checkpoint-and-pause current Run
→ disable future automations
→ keep files and remote repo
→ archive Project Record and Memory Space
→ clear from default list
→ create reversible lifecycle receipt
```

No local or remote deletion occurs.

## 31. Delete remote repository

- separate command from project deletion;
- fresh provider metadata and exact repository shown;
- uncommitted/local-only data warning;
- backup/export status;
- strong confirmation;
- Effect Claim;
- provider delete;
- timeout → UNKNOWN and reconciliation;
- Project Record becomes detached/archive according to user choice, not silently deleted.

## 32. Transfer project to another user

```text
select Project Export
→ inventory shareable project memory/artifacts/instructions/capabilities
→ remove personal memory, credentials, private messages and local paths
→ integrity/privacy validation
→ package + manifest
→ recipient imports in quarantine
→ rebuild indexes
→ bind own repo/accounts/providers
→ choose trust mode
→ project opens with provenance
```

Source can remain active, archive or delete separately.

## 33. Artifact generation, approval and publication

```text
agent creates candidate report
→ Artifact Record/Draft Version
→ user compares candidate variants
→ approves v2 for project
→ chooses public export
→ privacy/license scan
→ exact publication version frozen
→ publish effect
→ receipt/URL
→ v2 remains immutable; edits create v3
```

Publication timeout reconciles before retry.

## 34. Mixed-version update

- Head upgraded with compatibility layer;
- desktop latest, laptop old, phone offline;
- old laptop connects within supported protocol range with feature downgrade;
- phone returns after compatibility window and enters update-required/read-only mode;
- its offline append log translated/imported without overwriting newer state;
- derived indexes rebuild;
- no silent loss of unknown fields.

## 35. Lost phone

```text
owner uses trusted desktop
→ mark phone lost
→ revoke device/session/grants
→ block queued effects
→ rotate affected credentials
→ phone local encrypted state inaccessible
→ phone later appears and is quarantined
```

## 36. Lost Head and recovery

- use Recovery Kit + trusted device;
- restore verified backup to new server;
- collect trusted offline logs;
- reconcile external effects;
- establish new Authority Epoch;
- fence old Head;
- providers reauth where needed;
- semantic smoke test before background automations resume.

## 37. Disk reaches critical threshold

- stop speculative indexing/candidates;
- clear caches/previews;
- reduce sensory capture;
- offload cold encrypted media;
- protect canonical append reserve;
- show exact storage categories/actions;
- if reserve exhausted, pause source and warn, never pretend capture continues.

## 38. Global search while laptop offline

User asks for an architecture note stored on laptop.

Search returns:

- memory summary cached on Head;
- artifact copy if available;
- result saying source file unavailable on laptop;
- last freshness;
- action `Queue open/search when laptop reconnects`.

It does not fabricate file contents.

## 39. Travel timezone change

User creates «напомни каждый день в девять» in Helsinki, then travels.

Denet stores recurrence policy. Depending on configured semantics:

- follow local time;
- stay at home time;
- ask once on travel.

Existing absolute appointments remain fixed. DST update recalculates future occurrences.

## 40. Import malicious project package

- package copied to quarantine;
- checksum valid but script/hook suspicious;
- integrity passes, trust does not;
- content/notes inspectable as data;
- executable components disabled;
- user can import safe subset;
- no credentials/permissions inherited.

## 41. AI News Monitor finds a tool

```text
scheduled recipe finds release/article
→ stores source claim with freshness
→ matches active project requirement
→ compares with existing capability
→ candidate says one unique advantage
→ create Capability Delta Proposal
→ project-local trial if user policy
→ measured result
→ no global install/promotion unless useful
```

No separate news subsystem or automatic installation.

## 42. Daily Briefing has nothing important

Recipe retrieves current state and finds no meaningful delta. It produces no noisy voice/push, or a minimal «ничего срочного» only if user requested daily confirmation.

---


# Часть VI. Требования к архитектуре после дополнений

## 45. Новые обязательные architecture views

Кроме views, уже заданных в `70_...`, architecture должна показать:

1. Sensor Source Runtime с audio/screen/camera/clipboard adapters.
2. Communication connector inbound/outbound + reconciliation.
3. Project lifecycle and workspace bindings.
4. Artifact storage/version/publication.
5. Update/compatibility/migration topology.
6. Recovery/key/device/head flow.
7. Resource Coordinator and pressure signals.
8. Federated Search query/data flows.
9. Locale/time service and schedule semantics.
10. Import/export quarantine and package pipeline.

## 46. Новые обязательные critical contracts

Architecture/code-level contracts should include:

- `SensorSourceDescriptor`;
- `AmbientCandidate`;
- `ConsentPolicyRef`;
- `CommunicationMessageRef`;
- `SendProposal`;
- `DeliveryReceipt`;
- `ProjectRecord`;
- `WorkspaceBinding`;
- `ArtifactRecord`/`ArtifactVersion`;
- `PublicationRecord`;
- `VersionManifest`;
- `CompatibilityHello`;
- `MigrationJournal`;
- `RecoveryKitManifest`;
- `UsageObservation`/`ResourceBudget`;
- `FederatedSearchResult`;
- `TemporalIntent`;
- `PortablePackageManifest`.

These names may change in code, but semantics must remain.

## 47. Обязательные architecture risk spikes

### R-01. Ambient audio on target Android and Windows

Measure:

- OS/background viability;
- battery;
- VAD/wake false positives;
- privacy indicators;
- recovery after app/service kill.

### R-02. Event-driven Windows screen context

Measure:

- Windows Graphics Capture/accessibility integration;
- excluded apps;
- visual diff/dedup;
- CPU/GPU/storage;
- user trust/visibility.

### R-03. Telegram user-account connector

Using TDLib or other chosen legal/technical path:

- inbound updates;
- drafts;
- exact sent confirmation;
- timeout reconciliation;
- multi-device consistency.

### R-04. Artifact version/publication

- large source + preview;
- exact version sharing;
- publication unknown result;
- secret scan;
- revoke.

### R-05. Mixed-version protocol migration

- old client/new Head;
- offline operation log;
- unknown fields;
- feature downgrade;
- rollback.

### R-06. Full owner recovery drill

- encrypted backup;
- new Head;
- key recovery;
- device revoke;
- external effect reconciliation;
- provider reauth.

### R-07. Disk pressure chaos test

Fill disk during:

- ambient capture;
- memory append;
- migration;
- artifact generation;
- backup.

Prove no silent canonical data loss.

### R-08. Federated search prototype

- exact + lexical + semantic + command;
- project/memory/artifact;
- offline source;
- privacy filters;
- RRF baseline.

### R-09. Portable project round-trip

- export by user A;
- sanitize;
- import by user B;
- Git merge/update;
- no credentials/private overlays;
- rebuild indexes.

## 48. Architecture acceptance deltas

The four architecture volumes are not ready if:

- ambient capture is described only as «use Screenpipe»;
- communication retry semantics omitted;
- `Delete Project` remains ambiguous;
- artifact is treated only as a file path;
- update assumes all components same version;
- recovery depends on provider-held magic key;
- disk full has no explicit state transition;
- global search is one vector query;
- schedule stores only UTC offset;
- import activates scripts before trust review.

---
