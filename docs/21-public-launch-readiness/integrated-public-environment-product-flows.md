# Integrated Public Environment Product Flows

Status: canonical Stage 15 public-launch readiness owner for integrated public-environment product-flow evidence. Stage 15 records how the already documented local checkout, local tarball, examples, support, runtime, memory, builder, and managed-subagent surfaces compose. It does not expand the release target or convert local/offline evidence into live, public, hosted, or provider-readiness claims.

Related documents:

- [Public Launch Readiness](./README.md)
- [Public Docs, Onboarding, And Claims](./public-docs-onboarding-and-claims.md)
- [Stage 11 Distribution Proof](./distribution-proof.md)
- [Observability, Support, And Operations](./observability-support-operations.md)
- [Memory Productization](./memory-productization.md)
- [User Interaction Productization](./user-interaction-productization.md)
- [Managed Subagent Productization](./managed-subagent-productization.md)
- [Builder 2.0 Productization](./builder-2-0-productization.md)
- [Evidence Log](../20-real-world-proof-and-release/evidence-log.md)

## Stage 15 Decision

Stage 15 is a boundary lock for integrated public-environment flows, not a new launch approval. The current integrated proof is:

```powershell
pnpm stage15:integrated-flow:proof
```

That command runs a local proof path implemented by `scripts/check-stage15-integrated-flow.js`:

- builds the checkout with `pnpm build`;
- smokes the built CLI help and verifies the `support-bundle` command is present;
- runs `support-bundle` against a missing temporary state DB and verifies local-only redacted output;
- deliberately runs the Phase 5 example without its required `topic` parameter and verifies the flow fails with validation/resolution error semantics;
- runs `support-bundle` again against the failed-flow state DB and verifies local-only redacted output for an existing state DB;
- reruns `tests/unit/public-examples.test.ts` and `tests/integration/stage7-cli-integrated-flow.test.ts`.

This proves a local, public-facing composition path for checkout build, CLI inventory, support diagnostics, failed-flow diagnostics, public example validation, and offline Stage 7 builder/lifecycle/user-reply/resume wiring. It does not prove live Codex execution, public npm publication, hosted deployment, provider reliability, complete Builder 2.0 deploy/run readiness, or complete managed-subagent orchestration.

## Integrated Flow Evidence Matrix

| Scenario | Current status | Evidence | Boundary |
| --- | --- | --- | --- |
| Clean checkout build and CLI smoke | `passed` | `pnpm stage15:integrated-flow:proof` runs `pnpm build` and `node dist/src/interfaces/cli.js --help`. | Proves a local checkout can generate and smoke the built CLI; generated `dist` remains build-local. |
| Local tarball proof | `passed` | `pnpm package:local-install:proof` from Stage 11, plus TASK-607 installed support-bundle proof. | Local `.tgz` install/uninstall only; not public npm, registry availability, signing, provenance, or retained release artifact proof. |
| Installed support-bundle | `passed` | `pnpm package:local-install:proof` now smokes installed `dennett-agent-orchestrator support-bundle --state-db <temp>`. | Proves installed local tarball diagnostics for a temporary consumer project only; users must still review output before sharing. |
| Public examples validation and offline run | `passed` | `tests/unit/public-examples.test.ts` validates Agent JSON examples, builder wrapper examples, invalid wrapper examples, and an offline mocked Phase 5 CLI run. | Offline mocked runtime only; does not prove live Codex execution or model/account availability. |
| Support-bundle after failed or blocked flow | `passed` | `pnpm stage15:integrated-flow:proof` deliberately triggers a missing-parameter Phase 5 failure, then runs `support-bundle` against the resulting state DB. | Proves local diagnostics remain available after this validation failure shape; not a full crash dump, transcript export, or recovery guarantee. |
| Live Codex run status | `not-run` | No new Stage 15 live Codex graph run was executed by TASK-607. Earlier narrow live evidence remains in the evidence log. | Stage 15 does not certify live runtime readiness, model availability, account/rate-limit behavior, or full App Server support. |
| Mem0 runtime-memory example status | `not-run` | TASK-607 added `test:mem0` as an opt-in Mem0 test path; Stage 15 integrated proof does not call Mem0. Earlier narrow Mem0 and runtime-memory evidence remains bounded in the evidence log. | Mem0 setup is user-owned provider registration. Stage 15 does not prove native App Server memory, provider reliability, durable provider cleanup, or broad runtime-memory readiness. |
| Builder draft/deploy/run status | `passed` for offline Stage 7 fixture; `not-run` for live representative drafts | `tests/integration/stage7-cli-integrated-flow.test.ts` uses mocked Codex adapter calls to cover builder draft creation, register, deploy, run-live wait, reply, status, and resume. | Builder validation and offline deploy/run fixture do not prove live builder-authored agents execute in public environments. |
| Managed subagent flow status | `deferred` for integrated Stage 15 | Stage 8 owner docs and unit tests cover bounded local CLI managed-subagent commands; TASK-607 Stage 15 proof does not run a managed-subagent integrated flow. | Managed-subagent CLI surface remains bounded: launch-and-wait and state-recorded control semantics only, not durable background execution or live cancellation/control delivery. |
| Cleanup and rollback limits | `deferred` beyond local temporary artifacts | Stage 15 proof removes its temporary proof root. Stage 11 upgrade/rollback requires two distinct local tarballs. | No public package rollback, provider-wide cleanup, true restore, hosted rollback, or durable external cleanup is proven by Stage 15. |

## Conflict Rules

- Offline tests do not prove live readiness. Mocked Codex adapter execution and local SQLite assertions are valid only for local wiring and deterministic semantics.
- Local tarball proof is not public npm proof. A `.tgz` produced from the current checkout does not prove public registry publication, namespace ownership, signing, provenance, or public install availability.
- Builder validation is not deploy/run proof unless the evidence explicitly includes deploy/run behavior. The current Stage 7 integrated fixture proves offline deploy/run/resume wiring only.
- Mem0 provider registration is user-owned. Dennett can register and call a local provider configuration, but it does not install, host, operate, back up, or guarantee Mem0.
- Support redaction reduces accidental disclosure risk but does not make all private data safe. Users must still review output and must not share private prompts, memory records, credentials, account data, full transcripts, provider configs, or proprietary outputs.
- Managed-subagent CLI behavior is bounded. State-recorded control, close, and cancellation intent must not be described as live control-message delivery, live runtime cancellation, hosted orchestration, or durable background execution.
- Support-bundle after a failed flow is diagnostics evidence, not recovery evidence. It does not prove automatic repair, successful resume, provider cleanup, or local state rollback.
- Cleanup of temporary proof directories is not product rollback. Stage 15 cleanup removes disposable local files created by the proof harness only.

## Allowed Claims After Stage 15

These claims are allowed when the referenced commands remain accurate:

- `pnpm stage15:integrated-flow:proof` provides a local integrated proof for build, CLI help, support-bundle availability, support-bundle after a deliberate validation failure, public example validation, and offline Stage 7 integrated CLI wiring.
- The installed local tarball proof includes an installed `support-bundle` smoke in a temporary consumer project.
- Public examples have schema/offline validation coverage, including a mocked Phase 5 CLI run and builder wrapper validation.
- The offline Stage 7 integrated fixture exercises builder draft creation, register, deploy, run-live wait, reply, status, and resume through local state and mocked Codex adapter behavior.
- Stage 15 keeps live Codex, Mem0 provider, Builder 2.0 public authoring, managed-subagent, hosted, and public package claims bounded by their owner documents and evidence rows.

## Forbidden Claims After Stage 15

Do not claim:

- Stage 15 proves public npm publication, public registry install, package signing, provenance, retained release artifacts, installers, containers, hosted deployment, SaaS readiness, uptime, SLA, production load, or managed operations;
- Stage 15 proves live Codex runtime readiness, every supported model, account/rate-limit behavior, full App Server certification, or broad runtime-provider reliability;
- the Mem0 runtime-memory example was run as part of `pnpm stage15:integrated-flow:proof`;
- Mem0 provider setup, persistence, backup, cleanup, reliability, or native App Server memory is owned or guaranteed by Dennett;
- builder wrapper validation or offline Stage 7 builder flow proves complete Builder 2.0 authoring, public deploy authority, live representative draft execution, or integrated builder/runtime/memory/subagent product readiness;
- managed subagents are a complete product surface, provide durable background execution, live cancellation/control delivery, cross-process attachment, or hosted/UI orchestration;
- `support-bundle` output is safe to share without user review or can replace private security disclosure for sensitive reports;
- temporary proof cleanup, local uninstall, or local upgrade/rollback harnesses prove provider-wide cleanup, true restore, hosted rollback, or public package rollback.

## Future Evidence Required

Future expansion must add new evidence rows before claims broaden:

- live Codex integrated flow with redacted runtime diagnostics and exact model/account prerequisites;
- Mem0 runtime-memory integrated flow run through the documented user-owned provider registration path;
- representative builder-authored draft deploy/run proof against live runtime and, where applicable, memory;
- managed-subagent integrated flow covering the bounded CLI surface and any later live control/cancellation behavior separately;
- failed/blocked flow diagnostics beyond missing-parameter validation, including explicitly documented recovery limits;
- public package, hosted, rollback, cleanup, or support claims only after their owner documents define the artifact/surface and record direct evidence.
