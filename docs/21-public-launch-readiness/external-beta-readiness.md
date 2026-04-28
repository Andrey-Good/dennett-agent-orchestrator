# External Beta Readiness

Status: canonical Stage 16 public-launch readiness owner for the external beta plan and evidence gate. The beta status is `not-run`. No external participant, dated beta session, or completed-beta evidence is recorded in this repository.

Related documents:

- [Public Launch Readiness](./README.md)
- [Public Launch Scope](./public-launch-scope.md)
- [Public Docs, Onboarding, And Claims](./public-docs-onboarding-and-claims.md)
- [Observability, Support, And Operations](./observability-support-operations.md)
- [Integrated Public Environment Product Flows](./integrated-public-environment-product-flows.md)
- [Evidence Log](../20-real-world-proof-and-release/evidence-log.md)

## Stage 16 Decision

Stage 16 defines the external beta program that must happen before external-beta or public-readiness claims expand. It is not evidence that the beta has run.

Current classification:

`external-beta-not-run`

Limited or beta feature maturity inside earlier owner documents is not the same as a completed external beta program. Earlier stages may classify individual surfaces as limited/beta because their implementation and local evidence are bounded. Stage 16 requires real external participants, dated workflow execution, privacy-safe feedback intake, bug triage, and accepted evidence before it can move out of `not-run`.

## Non-Goals

- Do not claim a completed beta, public launch, general availability, production readiness, or hosted/managed service readiness.
- Do not collect secrets, credentials, raw provider configs, private prompts, full transcripts, memory records, account identifiers, or proprietary user data in public issues.
- Do not use internal-only runs, mocked tests, local maintainer smoke tests, or feature-level "limited/beta" labels as substitutes for external beta evidence.
- Do not require participants to share unredacted local state databases, provider storage, runtime account metadata, or private project files.
- Do not broaden support commitments beyond the bounded local CLI/package-first scope.

## Participant Criteria

External beta participants must be real users outside the implementation team. A private participant roster may exist outside the public repository, but public evidence must use stable aliases such as `beta-user-1`.

Minimum participant criteria before the beta can start:

- at least three external participants or organizations covering more than one machine/account environment;
- each participant has permission to use the beta on non-sensitive or intentionally disposable workflows;
- each participant can run local CLI commands and review/redact diagnostics before sharing;
- participants acknowledge that hosted operation, public package publication, SLA, production load, and broad provider reliability are not promised;
- at least one participant is willing to exercise support/feedback routing without sharing sensitive data;
- any participant using live runtime or memory providers owns the relevant account, storage, and cleanup responsibility.

## Workflow IDs

Beta evidence must identify at least one workflow ID per session.

| ID | Workflow | Required evidence shape | Boundary |
| --- | --- | --- | --- |
| `EB-WF-01` | Source checkout or local package onboarding | Environment summary, install/build command summary, redacted error or success outcome. | Does not prove public registry publication or installer support. |
| `EB-WF-02` | Local CLI inventory and Agent JSON validation | CLI help/version inventory, validation command, fixture or redacted agent shape. | Does not prove live runtime execution. |
| `EB-WF-03` | Local graph execution with supported runtime path | Redacted command summary, runtime/provider class, final status, and failure mode if any. | Does not prove all models/options, account/rate-limit behavior, or provider reliability. |
| `EB-WF-04` | Local Mem0 provider registration and memory operation | Redacted provider family/transport, operation type, outcome, and cleanup note. | User-owned provider only; not native App Server memory or provider-wide cleanup. |
| `EB-WF-05` | User prompt wait/reply/resume flow | Prompt/reply state transition summary with private content removed. | Does not prove the full user interaction layer or risky mid-run change policies. |
| `EB-WF-06` | Builder draft/audit flow | Draft creation/audit outcome and redacted diagnostics. | Does not prove complete Builder 2.0 public authoring or execution of every draft. |
| `EB-WF-07` | Managed-subagent operator flow | Launch/list/show/wait/control/close command summary and observed state. | Bounded local CLI surface only; no durable background execution or live cancellation delivery claim. |
| `EB-WF-08` | Support and security routing drill | Redacted support-bundle review outcome or security-route confirmation. | Does not create managed support, SLA, hosted telemetry, or public vulnerability disclosure. |

## Prerequisites To Start Beta

- The intended beta artifact or checkout path is named, reproducible, and tied to a commit or package artifact.
- Public docs identify supported and unsupported environments, including OS and Node.js requirements.
- Known forbidden claims remain visible in launch docs and onboarding materials.
- A privacy-safe feedback route exists for ordinary beta feedback and a separate private route exists for security issues.
- Maintainers define an owner for beta triage and an expected response window before inviting participants.
- Participants receive redaction instructions before running support, runtime, memory, or graph workflows.
- The evidence log has a ready schema and uses `not-run` until real dated sessions exist.

## Privacy And Data Handling

Participants must review and redact all shared output. Public beta issues must not include:

- credentials, API keys, tokens, cookies, authorization headers, or secret file paths;
- raw provider configuration, provider storage contents, memory records, or full local state databases;
- private prompts, replies, model outputs, transcripts, proprietary input data, or customer data;
- runtime account identifiers, emails, billing details, account/profile metadata, or unredacted `runtime-env-inspect` output;
- private local paths that reveal user, company, project, or customer identity.

Acceptable public evidence uses aliases, coarse environment summaries, redacted command summaries, minimal error excerpts, stable artifact paths, and explicit statements about withheld sensitive details.

## Support And Security Routing

- Ordinary beta defects and workflow feedback should use the beta feedback issue template when the information can be safely public and redacted.
- Setup questions that are not beta-session evidence may use the support request template.
- Documentation-only problems may use the documentation issue template.
- Security vulnerabilities, credential exposure, data leakage, unsafe redaction, or sensitive exploit details must follow `SECURITY.md`, not a public issue.
- If sensitive data is accidentally posted publicly, the reporter should rotate affected credentials and request removal through the repository maintainers.

## Feedback Intake

Every beta feedback item should include:

- workflow ID;
- participant alias, not real identity unless the participant explicitly chooses otherwise;
- environment summary;
- beta artifact or commit;
- expected behavior;
- observed behavior;
- severity proposal;
- redaction statement;
- whether the issue blocks beta exit.

Feedback may be public only when it is redacted. Private feedback may be summarized into public evidence only after removing sensitive details and replacing user identity with aliases.

## Bug Severity And Bug Bar

| Severity | Meaning | Beta bar |
| --- | --- | --- |
| `S0-security-privacy` | Credential leak, sensitive data exposure, unsafe diagnostic behavior, vulnerability, or unrecoverable destructive behavior. | Blocks beta continuation and exit until fixed or explicitly removed from beta scope with participant notification. |
| `S1-critical` | Supported workflow cannot complete, data/state corruption in supported local scope, crash with no actionable diagnostic, or repeated failure that prevents evaluation. | Blocks beta exit until fixed, deferred out of scope with clear docs, or accepted by final release owner as non-shipping. |
| `S2-major` | Workflow completes only with confusing workaround, misleading docs, incomplete diagnostics, or high-risk sharp edge. | Blocks beta exit if common, undocumented, or privacy-affecting; otherwise requires documented workaround and owner. |
| `S3-minor` | Cosmetic, wording, low-frequency usability issue, or non-blocking papercut. | Does not block exit if tracked or explicitly accepted. |

The bug bar is intentionally stricter for privacy, support routing, diagnostics, state safety, and claim accuracy than for cosmetic issues.

## Exit Criteria

Stage 16 may be marked completed only after all criteria are met:

- real external participants and dated sessions are recorded with aliases;
- each required workflow selected for beta has at least one accepted evidence item, or is explicitly removed from beta scope before launch;
- no open `S0-security-privacy` or unresolved beta-scope `S1-critical` items remain;
- any accepted `S2-major` issues have documented workarounds, owners, and public claim boundaries;
- feedback/support/security routing has been exercised without public sensitive-data leakage;
- evidence rows link to redacted artifacts or explain why artifacts are private;
- release docs distinguish completed external beta evidence from local/offline tests and feature-level limited/beta labels;
- a final beta review records whether the product may proceed, must defer, or must re-run selected workflows.

Until these criteria are met, Stage 16 remains `not-run` or `incomplete`; it must not be described as successful.

## Evidence Schema

Use this schema for each beta session summary before adding or accepting an evidence-log row:

```yaml
id: EB-YYYY-MM-DD-ALIAS-WORKFLOW-NNN
stage: 16
workflow_id: EB-WF-01
participant_alias: beta-user-1
participant_class: external-individual | external-organization | partner | maintainer
environment:
  os: ""
  node: ""
  package_manager: ""
  runtime_or_provider_class: ""
artifact_or_commit: ""
started_at: ""
ended_at: ""
commands_or_procedure: ""
result: pass | fail | blocked | inconclusive
severity: none | S0-security-privacy | S1-critical | S2-major | S3-minor
public_artifacts:
  - ""
private_artifacts:
  retained_by: ""
  reason_not_public: ""
redactions:
  - ""
claim_boundary: ""
follow_up: ""
review_status: unreviewed | accepted | rejected | superseded
```

Evidence-log rows for Stage 16 must remain `not-run` / `supports-defer` until at least one real external participant session is complete and reviewed.
