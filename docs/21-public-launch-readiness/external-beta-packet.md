# External Beta Packet

Status: prepared packet template for invited Stage 16 external beta testers. This document is not evidence that beta has run. The external beta remains `not-run` until real participant sessions are completed, reviewed, and recorded in the evidence log.

Related documents:

- [External Beta Readiness](./external-beta-readiness.md)
- [Live Proof Runbook](../20-real-world-proof-and-release/live-proof-runbook.md)
- [Evidence Log](../20-real-world-proof-and-release/evidence-log.md)
- [Security Policy](../../SECURITY.md)

## Maintainer Fill-In Before Invites

Maintainers must complete these fields before sending the packet to testers:

| Field | Required value |
| --- | --- |
| Beta packet date | `YYYY-MM-DD` |
| Maintainer owner | Name or handle responsible for triage and evidence acceptance |
| Response window | Target first response for `S0-security-privacy`, `S1-critical`, and ordinary feedback |
| Artifact mode | `source-checkout` or `future-npm-prerelease` |
| Artifact identity | Commit SHA for source checkout; package name/version/dist-tag for future npm prerelease |
| Required workflows | At minimum `EB-WF-01`, `EB-WF-02`, `EB-WF-03`, `EB-WF-08` |
| Optional workflows | Any of `EB-WF-04`, `EB-WF-05`, `EB-WF-06`, `EB-WF-07` that match current public claims |
| Participant alias | Stable alias such as `beta-user-1`; keep real identity private if needed |
| Feedback route | Public beta issue template for redacted feedback; `SECURITY.md` for sensitive reports |

Do not invite testers until the artifact identity is fixed. If the artifact changes, start a new packet date or clearly supersede the earlier packet.

## Artifact Modes

### Source Checkout Beta

Use this mode while public package publication is blocked or not chosen. It proves only that the invited tester can evaluate the named source checkout.

Tester setup:

```powershell
git clone https://github.com/Andrey-Good/dennett-agent-orchestrator.git
cd dennett-agent-orchestrator
git checkout <beta-commit-sha>
corepack enable
pnpm install --frozen-lockfile
pnpm build
node .\dist\src\interfaces\cli.js --help
```

Evidence to report:

- operating system and version, redacted as needed;
- Node.js version and pnpm version;
- exact commit SHA under test;
- whether install, build, and CLI help completed;
- short redacted error excerpt if any step failed.

### Future Npm Prerelease Beta

Use this mode only after a maintainer records that a public prerelease package path exists. This path is not required for the current beta packet while package publication remains blocked.

Tester setup:

```powershell
mkdir dennett-beta-consumer
cd dennett-beta-consumer
npm init -y
npm install --ignore-scripts --no-audit --fund=false dennett-agent-orchestrator@<prerelease-tag-or-version>
npx dennett-agent-orchestrator@<prerelease-tag-or-version> --help
```

Evidence to report:

- operating system and version, redacted as needed;
- Node.js version and npm version;
- package name, version, and dist-tag under test;
- whether install and CLI help completed;
- short redacted error excerpt if any step failed.

If the prerelease package is not available, record `EB-WF-01` as `blocked`; do not substitute an npm command for source-checkout evidence.

## Required Workflows

Run these workflows in order unless the maintainer packet marks one as out of scope before testing starts.

### `EB-WF-01` Onboarding

Goal: prove the tester can obtain the named artifact and run CLI help.

Source-checkout command summary:

```powershell
pnpm install --frozen-lockfile
pnpm build
node .\dist\src\interfaces\cli.js --help
```

Future npm-prerelease command summary:

```powershell
npm install --ignore-scripts --no-audit --fund=false dennett-agent-orchestrator@<prerelease-tag-or-version>
npx dennett-agent-orchestrator@<prerelease-tag-or-version> --help
```

Pass bar: install/build or install-only package setup succeeds, CLI help prints the `dennett-agent-orchestrator` command list, and no sensitive diagnostics are exposed.

Fail or blocked examples: dependency install failure, build failure, missing CLI entrypoint, package not found, unsupported Node.js version, or unredacted sensitive output.

### `EB-WF-02` CLI Inventory And Agent JSON Validation

Goal: prove the tester can inspect the CLI and validate public examples without live provider credentials.

Source-checkout commands:

```powershell
node .\dist\src\interfaces\cli.js --help
node .\dist\src\interfaces\cli.js support-bundle --help
pnpm test -- tests/unit/public-examples.test.ts
```

Future npm-prerelease commands:

```powershell
npx dennett-agent-orchestrator@<prerelease-tag-or-version> --help
npx dennett-agent-orchestrator@<prerelease-tag-or-version> support-bundle --help
```

Pass bar: command inventory matches the expected stable and experimental boundary, source-checkout public example validation passes when available, and output does not imply unsupported hosted, public package, or production claims.

Fail or blocked examples: missing stable commands, public example validation failure in source-checkout mode, command help contradicts the launch scope, or diagnostics include private data.

### `EB-WF-03` Local Graph Execution

Goal: prove one named graph can execute through a supported local runtime path, or record why it is blocked.

Source-checkout live command template:

```powershell
node .\dist\src\interfaces\cli.js run .\examples\agents\valid\phase5-codex-minimal.json --param topic="external beta smoke"
```

Prerequisites:

- tester owns and controls the runtime account used for the run;
- tester can safely redact prompts, outputs, account metadata, and local paths;
- maintainer confirms the selected model/runtime path is in beta scope.

Pass bar: the graph reaches a terminal success state and returns a final output for the supplied topic, with runtime/account details redacted.

Fail or blocked examples: no eligible runtime account, model unavailable, authentication failure, unsupported runtime option, graph crash, no final output, or sensitive data in diagnostics.

### `EB-WF-08` Support And Security Routing Drill

Goal: prove testers know how to route ordinary beta feedback and sensitive reports.

Commands and procedure:

```powershell
node .\dist\src\interfaces\cli.js support-bundle --help
```

Then open a beta feedback issue only if the report is safe and redacted. For security vulnerabilities, credential exposure, data leakage, unsafe redaction, or sensitive exploit details, follow `SECURITY.md` instead of creating a public issue.

Pass bar: tester can identify the correct public or private route, confirms redaction responsibilities, and no sensitive data is posted publicly.

Fail or blocked examples: sensitive data is included in a public issue, the issue lacks workflow/artifact/environment context, or the tester cannot identify the private security route.

## Optional Workflows

Run optional workflows only when the maintainer packet explicitly includes the capability in beta scope.

| Workflow | When to run | Minimum evidence |
| --- | --- | --- |
| `EB-WF-04` local Mem0 memory operation | Only when user-owned Mem0 setup is intentionally included. | Provider family/transport, operation type, outcome, cleanup note, and redactions. |
| `EB-WF-05` prompt wait/reply/resume | Only when mid-run interaction is included. | Prompt state, reply state, resume state, final outcome, and private content redaction. |
| `EB-WF-06` builder draft/audit | Only when audited builder draft flow is included. | Draft/audit command summary, result, diagnostics boundary, and claim boundary. |
| `EB-WF-07` managed-subagent operator flow | Only when local managed-subagent CLI surface is included. | Launch/list/show/wait/control/close summary and observed state. |

Optional workflow failure can still block beta exit if the capability remains in public-facing claims.

## Evidence To Submit

Each feedback item should include:

- workflow ID;
- participant alias;
- artifact mode and artifact identity;
- redacted environment summary;
- expected behavior;
- observed behavior;
- result proposal: `pass`, `fail`, `blocked`, or `inconclusive`;
- proposed severity if there was a problem;
- redaction statement;
- public artifact links or private-retention note;
- whether the issue blocks beta exit.

Use this result guidance:

| Result | Meaning |
| --- | --- |
| `pass` | The workflow completed within the documented boundary with redacted evidence. |
| `fail` | The workflow was attempted and did not meet the pass bar. |
| `blocked` | The workflow could not start because a prerequisite was missing, unsafe, or unavailable. |
| `inconclusive` | The workflow ran but evidence is insufficient to classify pass or fail. |

## Redaction Checklist

Remove or withhold:

- credentials, API keys, tokens, cookies, authorization headers, and secret file paths;
- raw provider configuration and provider storage contents;
- private prompts, replies, transcripts, model outputs, memory records, and proprietary data;
- account emails, billing details, profile metadata, and unredacted runtime inspection output;
- local paths that reveal personal, company, project, or customer identity.

Safe public evidence can use aliases, coarse environment summaries, command summaries, short error excerpts, and explicit statements that sensitive details were retained privately.

## Beta Exit Bar

External beta may exit only when:

- at least three external participant aliases have accepted dated evidence;
- every required workflow in the packet has accepted `pass` evidence, accepted `fail` evidence tied to a non-shipping decision, or explicit removal from public scope before launch;
- no unresolved `S0-security-privacy` or beta-scope `S1-critical` item remains;
- common or privacy-affecting `S2-major` items have documented workarounds and owners;
- feedback routing was exercised without public sensitive-data leakage;
- the evidence log records the final beta review decision.

Until then, beta status remains `not-run` or incomplete, and no completed-beta or beta-user-validation claim is allowed.
