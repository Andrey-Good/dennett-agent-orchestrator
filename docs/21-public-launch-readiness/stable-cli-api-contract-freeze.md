[English](#english)

<a id="english"></a>
# Stable CLI/API Contract Freeze

Status: canonical current-prerelease owner document for the bounded public CLI/API contract freeze.

Related documents:

- [Public Launch Scope](./public-launch-scope.md)
- [Baseline Gap And Forbidden Claims](./baseline-gap-and-forbidden-claims.md)
- [Release Engineering And Supply Chain](./release-engineering-and-supply-chain.md)
- [Security, Privacy, And Legal Foundation](./security-privacy-legal-foundation.md)
- [Repository README](../../README.md)
- [Security Policy](../../SECURITY.md)

## Scope

This document freezes only the bounded local CLI/package public surface that is explicitly labeled stable by the CLI and backed by package metadata checks after TASK-566.

This freeze does not make the project publicly launched, generally available, production ready, hosted, managed, published to npm, available through installers or containers, certified for the full Codex App Server surface, or supported for broad providers.

The package still has no stable JavaScript or TypeScript API. Built `dist` files are included for the executable `bin` path, but package exports intentionally prevent stable deep imports.

## Stable Command Inventory

These commands are stable within the bounded local CLI/package surface:

| Command | Stability class | Frozen user-facing contract |
| --- | --- | --- |
| `help` | stable | Displays CLI help for the program or a named command. |
| `register` | stable | Registers a portable agent file as a draft revision. |
| `status` | stable | Inspects registered agent lifecycle status. |
| `deploy` | stable | Publishes a portable agent file as the live revision. |
| `run-live` | stable | Runs the current live revision for a registered agent. |
| `run` | stable | Runs a portable agent file locally. |
| `run-status` | stable | Inspects durable run and interaction state. |
| `reply` | stable | Records or delivers a reply to a waiting user prompt. |
| `resume` | stable | Resumes a durable local run. |

These commands are stable only as safety protocols:

| Command | Stability class | Frozen user-facing contract |
| --- | --- | --- |
| `memory-cleanup-preview` | stable/safety-protocol | Previews the bounded memory cleanup safety envelope before deletion. |
| `memory-cleanup-verified-delete` | stable/safety-protocol | Deletes only candidates verified from the preview safety envelope. |
| `support-bundle` | stable/safety-protocol | Emits the documented local-only redacted support-bundle JSON summary for user-reviewed diagnostics. |

The safety-protocol label freezes the cleanup handshake, explicit scope requirement, preview-before-delete flow, and confirmation-token semantics. It does not certify provider-wide cleanup, true restore, provider reliability, throttling behavior, or broad external memory support.

For `support-bundle`, the safety/support protocol freezes only the documented local-only support-bundle schema and redaction boundary backed by tests. It does not create hosted telemetry, remote upload, SLA, managed support operations, complete redaction guarantees for unknown future fields, or a promise that user-added logs and attachments are safe to share without separate review.

## Experimental Command Inventory

These commands are intentionally not stable, even when implemented and useful:

| Command family | Commands |
| --- | --- |
| Runtime metadata | `runtime-model-list`, `runtime-env-inspect` including `runtime-env-inspect --redacted` |
| Memory provider registry | `memory-provider-register`, `memory-provider-list`, `memory-provider-show` |
| Direct memory operations | `memory-write`, `memory-read`, `memory-search`, `memory-list`, `memory-update`, `memory-delete` |
| Managed subagent operator surface | `subagent-launch`, `subagent-list`, `subagent-show`, `subagent-wait`, `subagent-record-control`, `subagent-close` |
| Builder authoring | `builder` |
| Lifecycle triggers and events | `trigger-register`, `trigger-list`, `event-dispatch` |
| Live comments | `comment` |

Experimental commands may change names, arguments, options, output shapes, error codes, and semantics without a deprecation window. They must remain visibly labeled `[experimental]` in top-level CLI help until a later owner document promotes them.

## Option And Argument Stability Rules

For stable commands:

- command names are stable;
- required positional arguments keep their meaning and order;
- documented option names keep their accepted spelling and meaning;
- new optional options may be added when they do not break existing invocations;
- existing optional options may accept additional values only when old values continue to work;
- cwd-dependent defaults, such as the rendered absolute `--state-db` help default, are not stable byte-for-byte output;
- aliases not shown in help are not public contract.

For stable/safety-protocol commands, any change that weakens explicit scope, preview-before-delete, candidate verification, or confirmation-token safety requires an update to this owner document before release claims can rely on it.

For experimental commands, help labels are the only frozen part: they must not be documented as stable while they remain labeled experimental.

## stdout And stderr Contract

The CLI keeps machine-readable result data on stdout for commands that produce JSON or final output. Operational metadata, run identifiers, warnings, and errors may use stderr.

Stable command output rules:

- `run`, `run-live`, and `resume` write the final agent output to stdout on success and write the run id to stderr;
- JSON-producing stable commands write formatted JSON to stdout;
- `run-status` omits prompt and reply payload content and reports redaction metadata instead;
- failure messages use stderr;
- help text is user-facing text, not a byte-for-byte stable machine contract.

Experimental command stdout and stderr behavior is not frozen.

## Exit-Code And Error Policy

The stable CLI uses `0` for successful command completion. Validation failures, unsupported operations, runtime failures surfaced through the CLI, missing local state, and unhandled command failures exit nonzero, normally `1`.

Application errors are printed to stderr as:

```text
CODE: message
```

The error code prefix is more stable than the human-readable message. Message wording may change for clarity unless a later contract explicitly snapshots it. Commander parser failures, Node.js process failures, and environment failures may use their own nonzero exit behavior.

## JSON Output Compatibility And Versioning

This document freezes command-local JSON shapes only for stable commands and the stable/safety-protocol cleanup flow. There is no global CLI JSON envelope and no stable JSON output promise for experimental commands.

Compatibility rules for frozen JSON outputs:

- top-level fields documented by tests or this owner doc are additive-compatible;
- existing stable fields must not be removed, renamed, or change meaning without a deprecation window;
- new fields may be added;
- field order and indentation are not semantic compatibility guarantees;
- sensitive payload redaction fields in `run-status` are part of the stable privacy contract;
- schema artifacts exported from the package follow the schema policy below, not the CLI stdout policy.

The current package version is `0.1.0-rc.1` and the package remains private. If publication or semver policy changes later, this document must be updated before public compatibility claims rely on a versioned artifact.

## Deprecation And Removal Policy

Stable command names, required arguments, documented options, stable JSON fields, package schema export paths, and safety-protocol semantics require a documented deprecation path before removal or incompatible change.

A deprecation must:

1. identify the affected stable surface;
2. document the replacement or migration path;
3. keep old behavior working for at least one compatibility window after the deprecation is documented;
4. include tests or checks that prove both the old and replacement behavior where practical;
5. update this owner document before the removal happens.

Experimental commands and undocumented internals do not require this process.

## Public JS/API Boundary And Package Import Policy

There is no stable JS/TS package API in the current prerelease CLI/package contract.

The only exported package paths are:

```json
{
  "./package.json": "./package.json",
  "./contracts/json-schema/*.schema.json": "./contracts/json-schema/*.schema.json"
}
```

The package includes `dist/src/**` so the `bin` entry can execute, but `dist` modules are not exported as stable imports. Consumers must not rely on deep imports such as `dennett-agent-orchestrator/dist/src/...`, internal TypeScript modules, or source files under `src/**` as public API.

Future JS/TS API claims require a separate API owner document, exported entrypoint, semver policy, and compatibility tests.

## Schema Artifact Stability

JSON Schema files under `contracts/json-schema/*.schema.json` are public package artifacts once included by the selected package path. Their export path is stable within this bounded freeze.

Schema compatibility is constrained as follows:

- schemas may become more permissive in additive ways;
- incompatible tightening, field removal, or semantic redefinition requires deprecation or a new schema/versioning decision;
- invalid examples may be added freely;
- schema internals such as `$defs` organization should not be treated as stable unless referenced by a public `$id` or export path.

The exported schema artifacts are not a broad JS runtime API.

## Compatibility Test Evidence

TASK-566 added test and metadata evidence for this bounded freeze:

- CLI top-level help shows `[stable]`, `[stable/safety-protocol]`, and `[experimental]` labels.
- The command inventory and stability labels are locked by `tests/unit/distribution-package.test.ts`.
- `run-status` has a stable output envelope snapshot with payload redaction.
- `package.json` exports only package metadata and JSON schema artifacts.
- `package.json` files include `dist/src/**` and `contracts/json-schema/*.schema.json`.
- `scripts/check-packlist.js` rejects non-allowlisted package inventory and verifies no stable JS internals are exported.

Validation for this docs update should include `git diff --check` and a targeted Markdown link check where practical.

## Forbidden Inferences

Do not infer any of the following from this bounded local CLI/package contract freeze:

- public launch, production readiness, general availability, hosted operation, managed deployment, SLA, or support operations;
- npm publication, installer distribution, container distribution, signed artifact, provenance, or rollback readiness;
- full Codex App Server certification;
- broad runtime provider support;
- native App Server memory;
- broad external memory provider reliability or provider-wide cleanup;
- complete user interaction readiness beyond the stable prompt reply/status/resume slice;
- complete managed subagent orchestration;
- complete public Builder 2.0 readiness;
- stable JavaScript or TypeScript API imports.
