[English](#english)

<a id="english"></a>
# Stage Safety Gates

Status: normative operational checklist.
Owns: the practical entry, execution, and exit gates used while executing roadmap stages.
Does not own: product roadmap sequencing, subsystem behavior, release readiness, or implementation status claims.
Primary sources: [Product Roadmap And Safety Gates](../product-roadmap-and-safety-gates.md), [Release Gates](./release-gates.md), [Validation Matrix](./validation-matrix.md), and [Operational Readiness](./operational-readiness.md).

This document turns the roadmap safety policy into a small operating system for future stage work. It is meant to be used before implementation starts, during worker/reviewer loops, and before a stage is called complete.

## 1. Required Records

Every significant stage or stage-sized task must have records for:

- stage entry review;
- implementation task documents when work is delegated;
- reviewer findings and fix-loop decisions;
- stage exit report.

Use the templates in [templates](./templates/) unless a narrower owner document already provides a stricter format.

## 2. Stage Entry Gate

Before code is written for a significant stage, the stage owner must complete an entry review.

The entry review must identify:

- canonical owner documents and affected contracts;
- in-scope behavior, out-of-scope behavior, non-goals, and invariants;
- expected changes to docs, code, tests, examples, and live-proof evidence;
- design questions that require review before implementation;
- whether an ADR or architecture boundary exception is required;
- subagent decomposition, write scopes, sequencing, and reviewer ownership;
- platform, runtime, framework, or provider primitives that should be reused instead of rebuilt.

Code work must not start while an unresolved design question could change public contracts, storage semantics, runtime boundaries, lifecycle behavior, or user-visible claims.

## 3. Execution Gate

During implementation, substantial tasks must use separate worker and reviewer roles.

Minimum loop:

1. Worker implements inside the assigned write scope.
2. Reviewer checks correctness, architecture fit, tests, docs, and shortcut risk.
3. Worker fixes valid findings.
4. Reviewer rechecks until remaining findings are rejected with recorded rationale or resolved.

The same subagent should not be both primary worker and final reviewer for a substantial change. If the task is too small for this loop, the task document or final report should say why direct handling was acceptable.

## 4. Evidence Separation Gate

Stage status must distinguish these evidence classes:

- Docs: owner documents, ADRs, task documents, and user-facing claims.
- Code: implemented product behavior, adapters, interfaces, or internal contracts.
- Tests: automated regression, contract, integration, fixture, or golden coverage.
- Examples: committed examples or reproducible local walkthroughs.
- Live proof: recorded proof against real runtimes, providers, or external surfaces when required.

No capability may be marked complete by collapsing these classes into one claim. A feature can be documented-only, code-present, test-covered, locally proven, or live-proven; those states are not interchangeable.

## 5. No Hidden Shortcut Gate

Hidden shortcuts are forbidden. A shortcut is hidden when implementation depends on behavior that is not visible through public contracts, owner docs, tests, task records, or explicit exceptions.

Blocked examples:

- relying on mock behavior while claiming real provider readiness;
- using builder-only or runtime-only magic that bypasses public contracts;
- treating untracked local files as release or live-proof evidence;
- skipping capability gates because a happy path works locally;
- leaving invented business logic only in code comments or implementation details.

If a temporary shortcut is unavoidable, it must be recorded as a gap, exception, or deferred task before the stage can exit.

## 6. Architecture Boundary Exception Gate

Use an [architecture boundary exception](./templates/architecture-boundary-exception.md) when a change crosses or bends an established boundary, including:

- core logic depending on storage, interface, runtime, or provider details;
- runtime-specific behavior leaking outside an adapter;
- lifecycle or registry behavior being changed from outside its owner area;
- tests or examples depending on non-public internals as if they were product behavior;
- stage work introducing a new owner for an existing invariant.

An approved exception must include the reason, risk, compensating checks, expiry condition, and cleanup owner.

## 7. Stage Exit Gate

A stage may exit only when the exit report records:

- owner docs, code, tests, examples, and live proof status separately;
- exact validation commands and manual checks performed;
- remaining gaps, blockers, deferred work, and intentional non-goals;
- reviewer loop result and unresolved finding rationale;
- capability matrix or owner-doc updates required by changed status;
- final system-level review for cross-stage conflicts.

If any required evidence is absent, the exit status must say "blocked", "deferred", or "not required" with rationale. Silence is not an acceptable status.

## 8. Templates

- [Stage Entry Review](./templates/stage-entry-review.md)
- [Stage Exit Report](./templates/stage-exit-report.md)
- [Subagent Reviewer Checklist](./templates/subagent-reviewer-checklist.md)
- [Architecture Boundary Exception](./templates/architecture-boundary-exception.md)
