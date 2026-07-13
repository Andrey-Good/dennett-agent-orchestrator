# Модуль F. Identity, Key and Ownership Recovery Contract

> **Канонический cross-domain supplement · `F`**  
> **Primary owner:** 30 Trust and 50 Server Runtime.  
> **Происхождение:** выделено из предархитектурного gap-аудита. Документ актуален и нормативен.  
> Маркеры источников вида `[[Sxx]]` раскрыты в [`REFERENCES.md`](REFERENCES.md). Ownership и порядок чтения описаны в [`README.md`](README.md).


## F.1. Назначение

Recovery должен позволять владельцу вернуть доступ после потери устройства, пароля, Head server или credentials, но не превращать backup/recovery flow в более лёгкий путь захвата системы.

Нужно различать:

- восстановление application access;
- восстановление device trust;
- восстановление encrypted data keys;
- восстановление backup;
- восстановление provider accounts;
- смену owner credentials;
- аварийный доступ доверенного лица;
- восстановление после компрометации.

## F.2. Главный принцип

> Denet не должен обладать магическим master key, который одновременно позволяет восстановить всё без участия владельца и при этом якобы не создаёт централизованную точку компрометации.

Apple Advanced Data Protection прямо указывает: при end-to-end encryption provider не имеет ключей и пользователь обязан настроить recovery contact или recovery key; 1Password использует отдельный Secret Key и Emergency Kit, а provider не может восстановить Secret Key за пользователя. [[S35]] [[S36]]

Следовательно, Denet предлагает несколько recovery methods и честно объясняет trade-off.

## F.3. Recovery domains

### Identity domain

Кто является владельцем установки.

### Device domain

Какие устройства доверены.

### Encryption domain

Какие keys открывают memory, artifacts, backups и secrets.

### Provider domain

Внешние accounts/tokens; Denet часто не может восстановить их без provider reauthentication.

### Head authority domain

Кто может назначить новый Head и повысить Authority Epoch.

## F.4. Recovery Kit

При onboarding Denet предлагает создать Recovery Kit:

```yaml
recovery_kit:
  installation_id: id
  owner_identity_hint: nonsecret
  recovery_method_set: []
  encrypted_recovery_material: ref
  backup_locations: []
  kit_version: version
  created_at: time
  last_tested_at: optional
```

Формы:

- printable recovery code/key;
- encrypted file on offline media;
- hardware/passkey recovery credential;
- trusted recovery contact;
- threshold split among several locations/people;
- trusted existing device approval.

Default personal installation should support at least:

1. one offline recovery key/file;
2. recovery from existing trusted device;
3. optional trusted contact or second independent copy.

## F.5. Recovery method policy

### Strongest privacy

Provider/Denet cannot recover without user-held key. Highest loss risk.

### Balanced

User-held key + one trusted device/contact path.

### Convenience-oriented

Encrypted escrow under user-controlled cloud/passkey policy. Higher central compromise risk, must be explicit.

Пользователь выбирает, но UI не скрывает consequences.

## F.6. Потеря одного устройства

```text
sign in from remaining trusted device
→ mark lost device
→ revoke sessions, device credentials and grants
→ rotate affected keys/tokens where needed
→ block offline queued consequential actions
→ update Head/device registry
→ preserve encrypted local data as inaccessible
→ incident summary
```

Если lost device позднее возвращается, он не восстанавливает trust автоматически.

## F.7. Потеря Head server

```text
select trusted recovery device/new server
→ authenticate owner with sufficient assurance
→ obtain latest verified backup + device logs
→ recover encryption material
→ restore installation in isolated validation mode
→ reconcile outstanding external effects
→ establish new Authority Epoch
→ revoke old Head credentials/fencing
→ reconnect devices
→ run semantic smoke tests
```

Новый Head не начинает отправлять queued external effects до reconciliation.

## F.8. Потеря всех обычных устройств

Нужны:

- Recovery Kit;
- fresh owner authentication per chosen method;
- optional waiting period/notifications to recovery contacts;
- restore into new trusted device;
- revoke old device identities;
- reauthenticate external providers;
- regenerate device and session keys.

NIST 800-63B требует пропорциональной assurance и recovery controls; конкретная реализация должна соответствовать выбранному assurance profile. [[S34]]

## F.9. Потеря recovery key

Если есть trusted device/secondary method:

- authenticate;
- rotate recovery material;
- invalidate old kit;
- produce new kit;
- verify backup decrypt.

Если нет ни одного метода, Denet не должен обещать невозможное. Некоторые E2EE data may be permanently unrecoverable.

## F.10. Compromise recovery

Отличается от обычной потери.

```text
Emergency Stop
→ isolate suspected devices/head/provider bindings
→ freeze new external effects
→ preserve incident evidence
→ authenticate owner through independent factor
→ rotate identity/device/session/encryption credentials
→ inspect grants and account bindings
→ restore clean runtime or verified backup
→ reconcile effects and data changes
→ selectively re-admit devices/capabilities
```

Не следует восстанавливать из backup, созданного после compromise, без проверки.

## F.11. Trusted recovery contact

Подобно Bitwarden Emergency Access, trusted contact может иметь заранее заданную роль и waiting period. [[S37]]

Варианты:

- `view/recovery assist` — помогает восстановить key/identity;
- `takeover` — для наследования/длительной недоступности, поздняя optional feature;
- `incident notify only`.

Ограничения:

- contact не получает обычный доступ заранее;
- activation logged/notified;
- configurable waiting period;
- owner can reject while available;
- contact cannot silently bypass personal vault exclusions unless explicitly configured.

## F.12. Recovery и Secret Broker

После restore:

- vault master keys восстановлены/rotated;
- provider tokens often reauthenticated;
- short-lived credentials not restored;
- unknown secret state marked unavailable;
- agents do not receive raw recovery material;
- Recovery Kit never enters model context.

## F.13. Backup key rotation

New recovery/encryption key may require:

- rewrap data keys, not reencrypt all data where envelope encryption supports;
- rotate backup manifests;
- update future backups;
- retain old wrapped keys only during bounded transition;
- verify at least one restore.

## F.14. Ownership transfer/death/incapacity

Not baseline, but architecture should not forbid future policy.

Possible later feature:

- designated successor;
- waiting period;
- limited export;
- selected projects/artifacts only;
- no automatic access to all personal memory;
- legal/manual verification outside agent autonomy.

## F.15. Social engineering resistance

Recovery UI never trusts:

- voice alone;
- email text claiming owner;
- support agent prompt;
- imported project;
- memory statement;
- external caller.

High assurance uses independent factors, exact installation ID, rate limits, delays and notifications.

## F.16. Recovery test

User can run non-destructive drill:

- verify Recovery Kit readable;
- verify backup decrypts in sandbox;
- verify contact reachable;
- verify device revoke works;
- do not expose full secret in UI/logs.

Suggested periodic reminder is optional and snoozable.

## F.17. Failure scenarios

### Backup stale

Restore, then merge trusted device logs with conflict preservation; show data-loss interval.

### Recovery contact unavailable

Use other configured method; no automatic weakening.

### Old Head comes online

Fencing/epoch rejects writes; device quarantined until re-paired.

### Attacker starts recovery

Notify existing trusted devices/contacts; waiting period; allow deny/revoke; rate limit.

### Recovery succeeds but providers unavailable

Core data opens; connectors marked reauth-required; project remains usable locally.

## F.18. Observability

- recovery attempts;
- method used;
- failed/aborted attempts;
- device/key rotations;
- backup age;
- restore verification;
- data-loss interval;
- outstanding unknown effects;
- post-recovery security review.

Sensitive recovery material never logged.

## F.19. Антиоверинижиниринговые ограничения

Не создавать:

- blockchain identity;
- own PKI hierarchy for every agent;
- mandatory multi-person threshold for ordinary users;
- hidden provider escrow presented as E2EE;
- voice-based recovery;
- automatic inheritance in first version;
- recovery flow that requires running LLM.

## F.20. Критерии готовности

- loss and compromise separated;
- no magic provider recovery claim;
- at least two independent methods supported conceptually;
- lost device revoke defined;
- Head recovery includes epoch/fencing;
- external effects reconciled before resume;
- provider reauth separated from data recovery;
- recovery drill possible;
- unrecoverable case stated honestly.

## F.21. Карта будущего переноса

- `30 Trust`: owner identity, assurance, recovery contact, revoke.
- `50 Server`: restore, Head re-establishment, device re-pairing.
- `10 Memory`: encryption/deletion/restore semantics.
- `41 Capability`: provider reauthentication.
- `60/61 UI`: Recovery Kit, drills, lost device flow.
- architecture: key hierarchy, envelope encryption, platform authenticators.


---
