[English](#english) | [Russian](#russian)

<a id="english"></a>
# Operational Runbook

Status: operational runbook with TASK-291 local CLI setup, recovery, cleanup, rollback-classification evidence, and Stage 13 local support diagnostics. It does not assert final release approval, hosted support operations, SLA, or managed incident response.

## Purpose

Use this runbook to prove that the release target can be configured, run, observed, supported, recovered, and rolled back by someone other than the implementer.

## Current Release Target

The current Phase 19 release target is [Release Scope Lock](./release-scope-lock.md) target `local-cli-repository-readiness`: a local CLI and repository-state release for contributors and local users. It is not a hosted service deployment and it does not yet have a package publishing, installer, container, or hosted rollout artifact. Stage 12 keeps hosted/managed deployment explicitly deferred in [Hosted And Managed Deployment Scope](../21-public-launch-readiness/hosted-managed-deployment-scope.md).

Operational evidence for this target is therefore split into:

- local CLI setup and configuration inspection;
- disposable local SQLite state setup, backup, restore, and cleanup;
- live runtime smoke evidence from the live-proof runbook and evidence log;
- explicit non-applicability of hosted deployment rollback until a hosted or packaged release artifact exists.

Do not record hosted deployment rollback as proven for this release target. If a later release scope includes an npm package, installer, container, hosted service, or managed deployment, that release must add artifact-specific deploy and rollback procedures before approval.

## Operator Inputs

Before operating the release target, record:

- release scope and disabled or deferred capabilities;
- commit SHA, package version, schema version, and migration status;
- supported runtime sources, provider accounts, regions, and limits;
- required environment variables and secret names, without secret values;
- storage paths, retention expectations, and cleanup responsibilities;
- known provider limitations and expected controlled failures;
- support owner and escalation path.

## Setup Checklist

- Install or build the release artifact from the recorded version.
- Configure runtime and provider credentials through approved secret handling.
- Validate configuration without printing secrets.
- Confirm storage directories or services are writable and recoverable.
- Confirm observability output location and retention.
- Run a smoke command that does not mutate external production data.
- Record setup evidence or the blocking reason.

## Local CLI Setup And Configuration Procedure

Run from the repository root:

```powershell
node --version
pnpm --version
git rev-parse --short HEAD
(Get-Content -Raw -LiteralPath package.json | ConvertFrom-Json).version
node .\dist\src\interfaces\cli.js --help
node .\dist\src\interfaces\cli.js runtime-env-inspect --redacted
node .\dist\src\interfaces\cli.js support-bundle
```

Expected result:

- Node satisfies the package engine requirement of `>=22.13.0`.
- pnpm matches the package manager line, currently `pnpm@10.33.0`.
- package version is recorded.
- CLI help lists runtime, memory, lifecycle, run, interaction, and resume commands.
- redacted runtime inspection returns authentication and runtime config metadata without printing private account/config fields.
- `support-bundle` returns local-only redacted JSON with package, environment, CLI inventory, git summary, state DB metadata, support boundary, and redaction metadata.

Redaction rule: never paste account email, account IDs, tokens, cookies, authorization headers, private prompts, memory contents, provider config, local SQLite contents, or full provider transcripts into shared evidence. Use `runtime-env-inspect --redacted` and review `support-bundle` output locally before sharing.

## Disposable State Recovery And Cleanup Procedure

Use a disposable state database under `%TEMP%`; never run this proof against the default `.dennett\local-state.sqlite` unless intentionally testing a real operator backup.

```powershell
$proofRoot = Join-Path $env:TEMP ("dennett-task291-operational-proof-" + (Get-Date -Format 'yyyyMMdd-HHmmss'))
New-Item -ItemType Directory -Path $proofRoot | Out-Null
$stateDb = Join-Path $proofRoot 'operational.sqlite'
$backupDb = Join-Path $proofRoot 'pre-mutation.backup.sqlite'

node .\dist\src\interfaces\cli.js memory-provider-list --state-db $stateDb
Copy-Item -LiteralPath $stateDb -Destination $backupDb

node .\dist\src\interfaces\cli.js memory-provider-register task291-mem0 --family mem0 --codex-ref task291_memory --display-name "TASK-291 Disposable Mem0 Registration" --transport sdk --config '{\"storage\":\"disposable\",\"secrets\":\"none\"}' --capability read --capability write --state-db $stateDb
node .\dist\src\interfaces\cli.js memory-provider-show task291-mem0 --state-db $stateDb

Copy-Item -LiteralPath $backupDb -Destination $stateDb -Force
node .\dist\src\interfaces\cli.js memory-provider-list --state-db $stateDb

$resolvedProofRoot = (Resolve-Path -LiteralPath $proofRoot).Path
$resolvedTemp = (Resolve-Path -LiteralPath $env:TEMP).Path
if (-not $resolvedProofRoot.StartsWith($resolvedTemp, [System.StringComparison]::OrdinalIgnoreCase)) { throw "Refusing cleanup outside TEMP: $resolvedProofRoot" }
Remove-Item -LiteralPath $resolvedProofRoot -Recurse -Force
Test-Path -LiteralPath $resolvedProofRoot
```

Expected result:

- Initial provider list is `[]` for the disposable state database.
- A backup SQLite file exists before mutation.
- `memory-provider-register` and `memory-provider-show` return a configured `mem0` provider with no secrets in config.
- Restoring the backup returns provider list to `[]`, proving local state restore for this mutation class.
- Cleanup returns `False` for the proof directory existence check.

PowerShell quoting note: pass JSON CLI option quotes as escaped quotes, for example `'{\"storage\":\"disposable\",\"secrets\":\"none\"}'`. Plain single-quoted JSON can be received by Node as `{storage:disposable,...}` on this environment and should be treated as a failed operator quoting attempt, not as a product rollback result.

## Normal Operation

Operators must be able to:

- start a release-scope run;
- locate run identity, state, logs, provider request IDs, and final output;
- distinguish success, controlled failure, blocked user input, cancellation, and crash;
- inspect deferred capability behavior without mistaking it for a defect;
- clean up disposable provider and memory data created by proof runs.

## Incident Response

These local incident procedures are support runbooks, not hosted incident-response promises.

| Incident | Detection signal | Immediate containment | Recovery or rollback action | Evidence to preserve |
| --- | --- | --- | --- | --- |
| Install, uninstall, or package proof failure | `pnpm package:local-install:proof`, `pnpm package:upgrade-rollback:proof`, `pnpm package:check`, or installed CLI smoke fails. | Stop release, public package, and rollback claims; do not commit generated tarballs or temp consumers. | Re-run from a clean checkout; verify `pnpm build`, Node `>=22.13.0`, pnpm `10.33.0`, and Stage 11 proof docs; defer package/publication claims if unresolved. | Command, exit code, redacted logs, commit, package version, Node/npm/pnpm versions, support bundle. |
| Runtime auth, model, rate-limit, or quota failure | `runtime-env-inspect --redacted`, model list, or run command reports auth, quota, timeout, rate-limit, or model unavailable. | Stop retry loops; do not paste unredacted account data. | Verify local runtime authentication privately; collect redacted runtime inspection; use only a documented supported runtime path; classify provider limits as provider-owned when applicable. | Redacted runtime inspection, command, error code, timestamp, safe provider request id if available. |
| Local SQLite state corruption or partial write | SQLite open/query error, missing state after crash, duplicate active run, or inconsistent `run-status`. | Stop mutating the affected DB; make a private copy; do not attach the DB publicly. | Restore from a known-good local backup if available; reproduce against disposable state; classify unproven recovery as residual risk. | Support bundle, DB size/path hash, command sequence, crash timing, private DB copy when required for private triage. |
| Memory provider failure or cleanup issue | Memory read/write/search/cleanup fails, returns unexpected scoped results, or provider rejects credentials. | Stop additional provider mutations; do not run broad provider cleanup outside documented scope. | Verify provider registration/capabilities; use preview-before-delete for supported cleanup; use provider-owned tools for data outside Dennett scope. | Redacted command output, provider family/id without secrets, cleanup preview metadata when safe, support bundle. |
| Stuck prompt or resume | `run-status` shows a pending prompt, `reply` does not unblock, or `resume` rejects state. | Avoid duplicate replies; inspect state before retrying; do not edit SQLite manually. | Use `run-status`; record or deliver exactly one intended reply; retry `resume`; classify incompatible active state through documented local controls where supported. | Redacted `run-status`, command sequence, run id, support bundle. |
| Managed subagent stuck state | `subagent-wait` times out, child remains active, control is recorded but not live-delivered, or close fails. | Do not launch overlapping workers with conflicting write scopes; record state before intervention. | Use `subagent-list` and `subagent-show`; record control/cancel state; close only through documented CLI; keep durable background runner/live cancellation gaps deferred. | Redacted subagent output, parent/child ids, role, write-scope summary, support bundle. |
| Accidental sensitive log disclosure | Secret, token, private prompt, memory content, account data, or unredacted provider config is posted or committed. | Remove public content if possible; rotate affected credentials immediately; stop public discussion if exploit details are involved. | Recreate the report with `support-bundle` and reviewed redacted excerpts; use the private security disclosure path for vulnerabilities or credential exposure. | What was exposed, where it was exposed, rotation/removal action, safe redacted replacement. |

Each incident must name an owner to notify. In the current scope the owner is a project maintainer or local operator, not a hosted on-call team.

## Rollback And Recovery

Rollback readiness requires:

- a known previous version or disabled-capability mode;
- a way to stop new runs without corrupting active state;
- documented handling for in-flight runs;
- storage backup or recovery procedure where applicable;
- cleanup steps for partially created provider resources;
- a post-rollback verification command.

If rollback cannot be proven, the release decision must be `block` or the affected capability must be `defer`.

## Rollback Classification For Current Scope

Current proven recovery:

- Disposable local SQLite state backup and restore is proven for a memory-provider registration mutation.
- Disposable proof directory cleanup is proven.
- Stage 8 deterministic crash/reopen recovery is proven for stale in-progress local run state: a fresh SQLite store reopen preserves committed boundaries and variables, does not fabricate committed success for stale active work, rejects duplicate active attempts, and allows explicit terminal classification plus retry.

Current not proven:

- Hosted deployment rollback is `not-run` and not applicable to the current local CLI/repository release scope because there is no hosted deployment artifact and Stage 12 explicitly defers hosted/managed deployment.
- Package, installer, container, or published artifact rollback is `not-run` because the repository is `private`, has no publish workflow, and no release packaging artifact is defined.
- Live external provider data rollback is not proven by the disposable registration proof; provider-specific cleanup must be proven when a release scope creates durable external provider resources.
- Production-scale crash recovery, live provider crash recovery, and hosted service recovery are not proven by the deterministic local Stage 8 test.

Decision rule:

- For the current local CLI/repository release scope, missing hosted or package rollback should be recorded as `supports-defer` for those out-of-scope deployment promises.
- If a release decision expands scope to include a hosted service, npm/package publication, installer, container, or durable external provider mutations, the missing artifact-specific rollback proof becomes `blocks-release`.

## Support Handoff

The release package must include:

- user-visible limitations;
- known issues and deferred capabilities;
- evidence log location;
- commands or procedures for collecting support diagnostics;
- redaction rules for logs and transcripts;
- escalation contacts or owner roles.

Stage 13 support handoff uses [Observability, Support, And Operations](../21-public-launch-readiness/observability-support-operations.md) as the canonical support-bundle, redacted diagnostics, support/security routing, telemetry-boundary, and local incident-runbook owner.

<a id="russian"></a>
# Операционный runbook

Статус: операционный runbook с доказательствами TASK-291 для локальной настройки CLI, восстановления, очистки и классификации rollback. Он не утверждает финальное одобрение выпуска.

## Назначение

Используйте этот runbook, чтобы доказать, что release target можно настроить, запустить, наблюдать, поддерживать, восстановить и откатить силами оператора, который не был implementer.

## Текущий release target

Текущий release target Phase 19 - [Release Scope Lock](./release-scope-lock.md) target `local-cli-repository-readiness`: локальный CLI и repository-state release для contributors и local users. Это не hosted service deployment, и в нем пока нет package publishing, installer, container или hosted rollout artifact.

Операционные доказательства для этого target делятся на:

- настройку локального CLI и проверку конфигурации;
- создание, backup, restore и cleanup одноразового локального SQLite state;
- live runtime smoke evidence из live-proof runbook и evidence log;
- явную неприменимость hosted deployment rollback, пока не существует hosted или packaged release artifact.

Не записывайте hosted deployment rollback как доказанный для этого release target. Если более поздний release scope включает npm package, installer, container, hosted service или managed deployment, этот release должен добавить artifact-specific deploy и rollback procedures до approval.

## Входные данные оператора

Перед эксплуатацией release target запишите:

- release scope и отключенные или deferred capabilities;
- commit SHA, package version, schema version и migration status;
- supported runtime sources, provider accounts, regions и limits;
- required environment variables и secret names, без secret values;
- storage paths, retention expectations и cleanup responsibilities;
- known provider limitations и expected controlled failures;
- support owner и escalation path.

## Setup checklist

- Установите или соберите release artifact из записанной version.
- Настройте runtime и provider credentials через approved secret handling.
- Проверьте configuration без вывода secrets.
- Подтвердите, что storage directories или services доступны для записи и восстановления.
- Подтвердите observability output location и retention.
- Выполните smoke command, которая не изменяет external production data.
- Запишите setup evidence или blocking reason.

## Процедура локальной настройки CLI и проверки конфигурации

Запускайте из корня репозитория:

```powershell
node --version
pnpm --version
git rev-parse --short HEAD
(Get-Content -Raw -LiteralPath package.json | ConvertFrom-Json).version
node .\dist\src\interfaces\cli.js --help
node .\dist\src\interfaces\cli.js runtime-env-inspect --redacted
node .\dist\src\interfaces\cli.js support-bundle
```

Ожидаемый результат:

- Node удовлетворяет package engine requirement `>=22.13.0`.
- pnpm совпадает со строкой package manager, сейчас `pnpm@10.33.0`.
- package version записана.
- CLI help перечисляет runtime, memory, lifecycle, run, interaction и resume commands.
- redacted runtime inspection возвращает authentication и runtime config metadata без вывода private account/config fields.
- `support-bundle` возвращает локальный redacted JSON с package, environment, CLI inventory, git summary, state DB metadata, support boundary и redaction metadata.

Правило редактирования: никогда не вставляйте account email, account IDs, tokens, cookies, authorization headers, private prompts, memory contents, provider config, local SQLite contents или полные provider transcripts в shared evidence. Используйте `runtime-env-inspect --redacted` и локально проверяйте вывод `support-bundle` перед публикацией.

## Процедура disposable state recovery и cleanup

Используйте одноразовую state database под `%TEMP%`; никогда не запускайте это доказательство против default `.dennett\local-state.sqlite`, если только вы намеренно не тестируете real operator backup.

```powershell
$proofRoot = Join-Path $env:TEMP ("dennett-task291-operational-proof-" + (Get-Date -Format 'yyyyMMdd-HHmmss'))
New-Item -ItemType Directory -Path $proofRoot | Out-Null
$stateDb = Join-Path $proofRoot 'operational.sqlite'
$backupDb = Join-Path $proofRoot 'pre-mutation.backup.sqlite'

node .\dist\src\interfaces\cli.js memory-provider-list --state-db $stateDb
Copy-Item -LiteralPath $stateDb -Destination $backupDb

node .\dist\src\interfaces\cli.js memory-provider-register task291-mem0 --family mem0 --codex-ref task291_memory --display-name "TASK-291 Disposable Mem0 Registration" --transport sdk --config '{\"storage\":\"disposable\",\"secrets\":\"none\"}' --capability read --capability write --state-db $stateDb
node .\dist\src\interfaces\cli.js memory-provider-show task291-mem0 --state-db $stateDb

Copy-Item -LiteralPath $backupDb -Destination $stateDb -Force
node .\dist\src\interfaces\cli.js memory-provider-list --state-db $stateDb

$resolvedProofRoot = (Resolve-Path -LiteralPath $proofRoot).Path
$resolvedTemp = (Resolve-Path -LiteralPath $env:TEMP).Path
if (-not $resolvedProofRoot.StartsWith($resolvedTemp, [System.StringComparison]::OrdinalIgnoreCase)) { throw "Refusing cleanup outside TEMP: $resolvedProofRoot" }
Remove-Item -LiteralPath $resolvedProofRoot -Recurse -Force
Test-Path -LiteralPath $resolvedProofRoot
```

Ожидаемый результат:

- Initial provider list равен `[]` для disposable state database.
- Backup SQLite file существует до mutation.
- `memory-provider-register` и `memory-provider-show` возвращают configured `mem0` provider без secrets в config.
- Restore backup возвращает provider list к `[]`, доказывая local state restore для этого mutation class.
- Cleanup возвращает `False` для проверки существования proof directory.

Примечание по PowerShell quoting: передавайте quotes для JSON CLI option как escaped quotes, например `'{\"storage\":\"disposable\",\"secrets\":\"none\"}'`. Plain single-quoted JSON в этой environment может быть получен Node как `{storage:disposable,...}`; это нужно считать failed operator quoting attempt, а не product rollback result.

## Нормальная эксплуатация

Операторы должны уметь:

- запускать release-scope run;
- находить run identity, state, logs, provider request IDs и final output;
- различать success, controlled failure, blocked user input, cancellation и crash;
- проверять deferred capability behavior, не принимая его за defect;
- очищать disposable provider и memory data, созданные proof runs.

## Incident response

Запишите ожидаемую реакцию для:

- provider authentication failure;
- provider rate limit или quota exhaustion;
- runtime model unavailable;
- interrupted graph execution;
- duplicate или missing final output;
- storage corruption или partial write;
- blocked user prompt, который cannot resume;
- managed child run, который does not close;
- memory provider read/write/search failure.

Каждый incident path должен назвать detection signal, immediate containment, recovery или rollback action, evidence to preserve и owner to notify.

## Rollback и recovery

Rollback readiness требует:

- known previous version или disabled-capability mode;
- способ остановить new runs без повреждения active state;
- documented handling для in-flight runs;
- storage backup или recovery procedure, где применимо;
- cleanup steps для partially created provider resources;
- post-rollback verification command.

Если rollback невозможно доказать, release decision должно быть `block`, либо affected capability должна быть `defer`.

## Классификация rollback для текущего scope

Текущее доказанное recovery:

- Disposable local SQLite state backup and restore доказаны для memory-provider registration mutation.
- Disposable proof directory cleanup доказан.

Сейчас не доказано:

- Hosted deployment rollback имеет статус `not-run` и неприменим к текущему local CLI/repository release scope, потому что hosted deployment artifact отсутствует.
- Package, installer, container или published artifact rollback имеет статус `not-run`, потому что repository `private`, publish workflow отсутствует, и release packaging artifact не определен.
- Live external provider data rollback не доказан disposable registration proof; provider-specific cleanup должен быть доказан, когда release scope создает durable external provider resources.

Правило решения:

- Для текущего local CLI/repository release scope отсутствующий hosted или package rollback нужно записывать как `supports-defer` для этих out-of-scope deployment promises.
- Если release decision расширяет scope до hosted service, npm/package publication, installer, container или durable external provider mutations, отсутствующий artifact-specific rollback proof становится `blocks-release`.

## Support handoff

Release package должен включать:

- user-visible limitations;
- known issues и deferred capabilities;
- evidence log location;
- commands или procedures для сбора support diagnostics;
- redaction rules для logs и transcripts;
- escalation contacts или owner roles.
