[English](#english)

<a id="english"></a>
# Observability, Support, And Operations

Status: canonical Stage 13 owner document for CLI/package-first observability, support diagnostics, and local operations readiness. This document describes the current local support surface after TASK-590. It does not create a hosted support program, SLA, managed operations promise, status page, automatic telemetry claim, or public npm publication claim.

Related documents:

- [Public Launch Scope](./public-launch-scope.md)
- [Security, Privacy, And Legal Foundation](./security-privacy-legal-foundation.md)
- [Hosted And Managed Deployment Scope](./hosted-managed-deployment-scope.md)
- [Operational Runbook](../20-real-world-proof-and-release/operational-runbook.md)
- [Operational Readiness](../11-hardening/operational-readiness.md)
- [Security Policy](../../SECURITY.md)

## Stage 13 Decision

Stage 13 productizes a local-only support and operations boundary for the CLI/package-first target:

- users and maintainers may collect redacted local diagnostics through `support-bundle`;
- runtime account/config inspection must use `runtime-env-inspect --redacted` before sharing;
- GitHub issue templates route ordinary bugs, documentation issues, and support questions separately from security disclosures;
- local incidents are handled through repository runbooks and user-owned state/provider cleanup, not through hosted operations.

This stage keeps all hosted or managed operations deferred. Do not claim uptime, availability, SLA, managed incident response, hosted rollback, hosted telemetry, audit readiness, status-page monitoring, public npm support, signing/provenance, or long-term support versions from Stage 13.

## Support Bundle Command

From a built repository checkout:

```powershell
node .\dist\src\interfaces\cli.js support-bundle
```

For a non-default local state database:

```powershell
node .\dist\src\interfaces\cli.js support-bundle --state-db <path-to-local-state.sqlite>
```

For an installed CLI package, use the installed binary name with the same arguments:

```powershell
dennett-agent-orchestrator support-bundle --state-db <path-to-local-state.sqlite>
```

Current behavior:

- writes formatted JSON to stdout;
- does not upload anything;
- does not create a file by itself;
- marks the bundle as `local_only: true`;
- reports a redacted summary even when the state database path does not exist.

Users must review the JSON locally before pasting it into a public issue or support request.

## Included Diagnostics

The support bundle currently includes:

| Area | Included fields |
| --- | --- |
| Generation metadata | `generated_at`, `local_only` |
| Package metadata | package name, version, `private`, license, package manager, engines, bin names, repository metadata, support metadata when present |
| Environment | Node version, npm version, pnpm version, OS platform, CPU architecture, OS release |
| CLI inventory | command names, stability labels, summaries, and commands grouped by stability |
| Git summary | whether the current directory is a git worktree, short commit, clean/dirty summary, changed-file count grouped by status |
| Local state database | existence, file size, redacted path flag, hashed resolved path |
| Support boundary | stable commands, stable/safety-protocol commands, experimental commands, and the experimental-command caveat |
| Redaction metadata | redaction mode, path handling, and omitted payload categories |

The bundle is intentionally an operational summary. It is not a full log export, trace export, crash dump, state database dump, runtime transcript, provider transcript, memory export, or account export.

## Excluded Data

The support bundle must not include:

- prompt payloads, reply payloads, memory contents, transcripts, or runtime handles;
- provider configuration, provider credentials, API keys, tokens, passwords, cookies, authorization headers, or private keys;
- credentialed URLs or raw provider endpoints that embed credentials;
- account email addresses or account identifiers in shareable form;
- full local file paths;
- raw SQLite database contents;
- raw agent JSON files, builder drafts, run outputs, or final outputs;
- third-party runtime, memory-provider, MCP, plugin, or skill logs unless the user separately adds reviewed redacted excerpts.

If a user chooses to attach extra logs or excerpts, those attachments are outside the support bundle guarantee and must be reviewed and redacted separately.

## Redaction Policy

Current redaction behavior is defense-in-depth, not a promise that arbitrary user-added text is safe:

- sensitive keys such as token, secret, password, authorization, cookie, credential, API key, and private key values are replaced with redaction markers;
- prompt, reply, memory, transcript, and runtime-handle payload keys are replaced with an omitted-payload object;
- provider-config-shaped objects are omitted with a redaction reason;
- email-like strings are replaced with `[REDACTED_EMAIL]`;
- common secret token shapes and `Bearer` or `Basic` authorization strings are replaced with `[REDACTED_SECRET]`;
- Windows and common Unix-like absolute paths are replaced with `[REDACTED_PATH]`;
- credentialed URL userinfo is replaced with `[REDACTED_URL_CREDENTIALS]`;
- the state database path is not printed, but a SHA-256 hash of the resolved path is included for correlation.

Redaction helpers reduce accidental disclosure risk. They do not make it safe to paste secrets, private prompts, private memory records, proprietary outputs, or full provider transcripts into public issues.

## Runtime Environment Diagnostics

Use redacted runtime inspection for shareable runtime diagnostics:

```powershell
node .\dist\src\interfaces\cli.js runtime-env-inspect --redacted
```

The unredacted command may expose local account/config metadata and must be treated as private local diagnostics. Share only reviewed redacted output, and never paste account email, account IDs, tokens, cookies, authorization headers, provider handles, or full provider transcripts into public issues.

Third-party runtime and provider data handling remains provider-owned. Dennett can document what its adapter sends or redacts locally, but it does not guarantee provider retention, telemetry, training, audit, deletion, residency, confidentiality, rate-limit behavior, or availability.

## Safe Sharing Rules

Safe-by-default issue sharing:

- prefer `support-bundle` output over ad hoc logs;
- use `runtime-env-inspect --redacted` when runtime configuration is relevant;
- review all JSON before posting publicly;
- replace business data, prompts, outputs, memory records, local usernames, and private project names with synthetic examples;
- include command lines, exit codes, and error codes when they do not reveal secrets;
- use the security disclosure path instead of a public issue if the report requires exploit details or sensitive data.

Do not share publicly:

- credentials, tokens, cookies, authorization headers, or secret values;
- private prompts, private memory contents, proprietary generated outputs, or full transcripts;
- unredacted provider config or runtime account data;
- full local paths when the path itself reveals private names;
- vulnerability reproduction details that would help exploitation.

## Retention Guidance

Support diagnostics are local artifacts controlled by the user or maintainer who generates them:

- keep support bundles only as long as needed for triage;
- delete local copies after the issue is resolved unless they are needed as release evidence;
- do not commit support bundles, logs, SQLite databases, provider config, or transcripts to the repository;
- if a support bundle is attached to a public issue and later found to contain sensitive data, remove it if possible and rotate any affected credential;
- security reports and sensitive reproductions should use a private disclosure path and follow [SECURITY.md](../../SECURITY.md).

Stage 13 does not define a hosted support-data retention policy because Dennett has no hosted support-data system in the current scope.

## Support Matrix

| Area | Current Stage 13 support boundary |
| --- | --- |
| Primary OS evidence | Windows local evidence remains the primary recorded baseline for the current local release proof. |
| Linux and macOS | CI package-proof jobs are evidence candidates only. Do not claim public OS support before OS-specific green package proof, CLI smoke, runtime/provider proof, and release approval are recorded. |
| Node.js | Package engines require Node.js `>=22.13.0`; current evidence records a Node 22 path and `node:sqlite` import proof. |
| pnpm | Canonical repository workflow is pnpm; package metadata records `pnpm@10.33.0`. |
| npm | npm is used for controlled local package-consumer proof. It is not the canonical repository workflow and does not imply public npm publication. |
| Stable CLI | Commands labeled `[stable]` are the support baseline within the Stage 10 compatibility policy. |
| Stable/safety-protocol CLI | `memory-cleanup-preview`, `memory-cleanup-verified-delete`, and `support-bundle` are safety/support protocols; their safety handshakes and redaction/local-only expectations are the supported part. |
| Experimental CLI | Experimental commands can be used for diagnostics and issue reproduction, but names, options, output shapes, and semantics may change without a deprecation window. |
| Runtime provider | Codex App Server adapter path only, with narrow local proof. Runtime discovery and environment inspection are diagnostics, not account/rate-limit guarantees. |
| Memory provider | Registered local Mem0 provider path only within the documented bounded behavior. No broad provider reliability, provider-wide cleanup, true restore, or native App Server memory claim. |
| Distribution | Local checkout and controlled local `.tgz` proof only. Public npm publication, signing, provenance, retained SBOMs, installers, containers, and hosted deployment remain deferred. |

## Product Support Vs Security Disclosure

Use product support or bug routes for:

- local install/build/package-proof failures without a vulnerability;
- CLI command errors, docs confusion, support-bundle questions, and expected-behavior questions;
- runtime auth/rate-limit problems that do not expose a vulnerability in Dennett;
- memory provider setup or cleanup issues without leaked secrets;
- stuck prompt/resume or managed-subagent state that does not expose a security impact.

Use the security disclosure path for:

- credential exposure, secret leakage, or redaction bypass with real sensitive data;
- vulnerability details that would help exploitation;
- privilege escalation, unsafe filesystem/process/network access, or sandbox bypass claims;
- supply-chain, package, or dependency issues that could compromise users;
- any report that requires private prompts, private memory records, account data, exploit details, or unredacted logs to explain.

Public security issues must not include exploit details or sensitive data. Follow [SECURITY.md](../../SECURITY.md).

## Local Incident Runbooks

These runbooks are local triage procedures. They do not imply a hosted incident-response promise.

| Incident | Detection signal | Immediate containment | Recovery or rollback | Evidence to preserve |
| --- | --- | --- | --- | --- |
| Install, uninstall, or package proof failure | `pnpm package:local-install:proof`, `pnpm package:upgrade-rollback:proof`, `pnpm package:check`, or CLI smoke fails. | Stop release/publish claims; keep generated tarballs and temp paths out of commits. | Re-run from clean checkout; rebuild with `pnpm build`; verify Node/pnpm versions; compare against Stage 11 package-proof docs; defer public package claims if unresolved. | Command, exit code, redacted logs, package version, commit, Node/npm/pnpm versions, support bundle. |
| Runtime auth or rate-limit failure | `runtime-env-inspect --redacted`, model list, or run command reports auth, account, quota, timeout, or rate-limit error. | Do not retry aggressively; avoid pasting unredacted account data. | Verify local runtime auth outside shared logs; use redacted inspection; switch only to a documented supported runtime path; classify provider outage/rate-limit as provider-owned when applicable. | Redacted runtime inspection, command, error code, timestamp, provider request ID if safe. |
| Local SQLite state corruption | SQLite open/query error, missing state after crash, duplicate active run, or inconsistent run/status output. | Stop mutating the affected state DB; copy it to a private backup location; do not attach the DB publicly. | Restore from a known-good local backup when available; use disposable reproduction if possible; classify unproven recovery as residual risk. | Support bundle, DB size/path hash, command sequence, crash timing, private DB copy if needed for private triage. |
| Memory provider failure or cleanup issue | Memory read/write/search/cleanup command fails, returns unexpected scoped results, or provider rejects credentials. | Stop additional provider mutations; do not run broad provider cleanup outside documented scope. | Verify provider registration and capabilities; use preview-before-delete for supported cleanup; clean provider-owned data through provider tools when outside Dennett scope. | Redacted command output, provider family/id without secrets, cleanup preview token metadata when safe, support bundle. |
| Stuck prompt or resume | `run-status` shows pending prompt, `reply` does not unblock, or `resume` rejects state. | Avoid duplicate replies until state is inspected; do not edit SQLite manually. | Use `run-status`; record or deliver exactly one intended reply; retry `resume`; if incompatible active state exists, classify terminal state through documented local controls where supported. | `run-status` redacted output, command sequence, run id, support bundle. |
| Managed subagent stuck state | `subagent-wait` times out, child remains active, control is recorded but not live-delivered, or close fails. | Do not launch overlapping workers with conflicting write scopes; record state before manual intervention. | Use `subagent-show` and `subagent-list`; record control/cancel state; close only through documented CLI; treat live cancellation delivery and durable background execution as deferred if they are the blocker. | Redacted subagent output, parent/child ids, role, write-scope summary, support bundle. |
| Accidental sensitive log disclosure | Secret, token, private prompt, memory content, account data, or unredacted provider config is posted or committed. | Remove the public content if possible; rotate affected credentials immediately; stop sharing the thread publicly if vulnerability details are involved. | Recreate the report with `support-bundle` and redacted excerpts; use private security disclosure if exploitability or credential exposure is involved. | What was exposed, where it was exposed, rotation/cleanup action, safe redacted replacement. |

## Telemetry Boundary

Current Stage 13 claim:

- Dennett has no documented Dennett-owned automatic product telemetry in the CLI/package-first scope.
- `support-bundle` and `runtime-env-inspect --redacted` are local diagnostics commands; they do not upload diagnostics by themselves.
- Local logs, terminal output, generated bundles, and issue attachments are user/maintainer controlled artifacts, not automatic telemetry.
- Third-party runtimes, memory providers, package registries, MCP servers, plugins, skills, and dependency tools may collect their own telemetry or logs under their own policies.
- Hosted observability, analytics, audit logs, support tooling, incident monitoring, and status pages remain out of scope until a later hosted scope decision.

Do not broaden "no Dennett-owned telemetry" into a claim about third-party providers or tools.

## Completion Criteria For Future Stage 13 Expansion

Future support/operations expansion requires a new owner-doc update and evidence when it changes any of:

- support-bundle fields, redaction behavior, or sharing guidance;
- supported OS matrix or package-manager matrix;
- stable versus experimental support commitments;
- runtime/provider support boundaries;
- local state recovery procedures;
- memory cleanup, restore, or provider reliability claims;
- telemetry, diagnostics upload, hosted support tooling, or status-page behavior.

Any hosted support or SLA claim requires a later hosted/managed scope decision first.
