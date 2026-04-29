[English](#english) | [Russian](#russian)

<a id="english"></a>
# Managed Subagent Productization

Status: canonical Stage 8 public-launch readiness owner for the bounded managed-subagent operator surface. This document records what TASK-548 made visible to operators and what remains deferred. It does not claim complete managed orchestration, hosted orchestration, background subagent execution, or live runtime cancellation delivery.

Related documents:

- [Public Launch Scope](./public-launch-scope.md)
- [Baseline Gap And Forbidden Claims](./baseline-gap-and-forbidden-claims.md)
- [Phase 16 Managed Subagent Orchestration](../17-managed-subagent-orchestration/phase-16-managed-subagent-orchestration.md)
- [Phase 17 Builder 2.0](../18-builder-2-0/phase-17-builder-2-0.md)

## Stage 8 Decision

Stage 8 moves managed subagents from "internal-only contract slice" to a bounded local CLI/package operator surface.

The public-launch classification is:

`limited-local-cli-managed-subagent-operator-surface`

This classification is intentionally narrower than the full Phase 16 managed-subagent vision. It allows public docs to describe the implemented local CLI commands and their durable state semantics, while keeping richer orchestration deferred until implementation and evidence exist.

## Implemented Operator Commands

The implemented CLI commands are:

| Command | Implemented behavior | Boundary |
| --- | --- | --- |
| `subagent-launch` | Creates a managed subagent record, starts the child through the existing child-run path, and waits for terminal completion in the same CLI process. | Launch-and-wait only. It does not create a durable background worker or detach a live child runner. |
| `subagent-list` | Lists managed subagent records, with filters for `--parent-run-id`, `--parent-task-id`, and `--state`. | Reads local persisted state only. It is not a hosted fleet or live process monitor. |
| `subagent-show` | Shows one managed subagent record with lineage, task package, child agent metadata, terminal result when present, and operator semantics. | Reads local persisted state only. |
| `subagent-wait` | Reconciles or inspects persisted managed subagent state using `terminal_only` or `terminal_or_update`. | It can reconcile persisted terminal state; it does not attach to a live in-process subagent launched by another process. |
| `subagent-record-control` | Records bounded control messages such as `clarify_scope`, `narrow_constraints`, `update_budget`, `request_status`, and `cancel`. | Control is state-recorded for operator visibility. It is not live-delivered to the child runtime. |
| `subagent-close` | Records parent close disposition: `accepted_by_parent`, `cancelled_by_parent`, or `abandoned_by_parent`. | Close records boundary disposition. `cancelled_by_parent` does not prove runtime cancellation delivery. |

The commands use the local state database selected by `--state-db` or the default local state path. They are part of the CLI/package-first target and do not imply hosted or managed service operation.

## Implemented Semantics

The implemented surface supports these bounded semantics:

- roles accepted by the managed-subagent layer: `worker`, `reviewer`, and `final_review`;
- durable lineage linking managed children to parent run and parent task;
- task-package snapshots with objective, input message, acceptance criteria, prohibitions, `write_set`, and budgets;
- persisted terminal result and close disposition;
- reviewer-like findings in the managed subagent state when produced by the child result;
- sibling write-set conflict rejection for overlapping recorded resources;
- sibling caps, review-loop caps, and cancel/close state transitions in the core managed-subagent slice;
- operator-visible command output that explicitly reports launch, wait, delivery, cancellation, and close semantics.

The `write_set` rules are coordination metadata enforced by the managed-subagent service. They are not an OS filesystem sandbox and are not a substitute for process isolation.

## Evidence

The Stage 8 evidence boundary is:

- `src/interfaces/cli.ts` exposes the six operator commands listed above;
- `tests/unit/subagent-cli.test.ts` covers launch-and-wait semantics, list/show output, wait reconciliation, state-recorded control messages, state-recorded cancel, and close semantics;
- the CLI output includes explicit semantics flags such as `background_execution: false`, `live_execution_wait: false`, `live_delivery: false`, and `runtime_cancellation_delivered: false`.

This evidence supports only the bounded local CLI operator surface. It does not support live multi-process orchestration, hosted orchestration, or broad runtime-provider proof.

## Explicit Non-Claims

Do not claim:

- complete managed subagent orchestration is implemented;
- subagents run as durable background workers after `subagent-launch` returns;
- `subagent-wait` attaches to or controls a live child launched by a different process;
- `subagent-record-control` live-delivers messages to a running child runtime;
- `cancel` or `cancelled_by_parent` sends a live runtime cancellation signal;
- managed subagents provide hosted/UI orchestration, fleet management, uptime, multi-tenancy, or service-level behavior;
- review/fix loops are enforced as a complete product workflow;
- child interaction is surfaced through the parent boundary as a complete user interaction model;
- write-set enforcement is equivalent to a filesystem sandbox or security boundary;
- Builder 2.0 may rely on hidden managed-subagent internals.

## Builder 2.0 Boundary

Builder 2.0 may rely only on stable public managed-subagent contracts that are documented and tested.

For Stage 8, that means Builder 2.0 may reference the bounded CLI/operator semantics only as a public capability with the limitations above. It must not depend on hidden state fields, unpublished service internals, unimplemented background execution, or live cancellation behavior.

Any builder-authored managed-subagent flow must remain valid if implemented through the documented public contract, not through private shortcuts.

## Deferred Work

The following remain deferred:

- durable background subagent runner;
- live runtime cancellation delivery;
- live control-message delivery into running children;
- cross-process wait attachment to a running child;
- surfaced child-to-parent interaction;
- enforced end-to-end review/fix loops as product semantics;
- richer ownership, budget, and lineage policy beyond the current bounded state/service rules;
- hosted/UI managed orchestration;
- external live proof across real provider and runtime conditions.

## Public Claim Rule

Public docs may say:

- Dennett includes a limited local CLI managed-subagent operator surface.
- The surface can launch-and-wait, list, show, reconcile/wait, record control intent, and close managed subagent records.
- Control and cancellation are currently durable state semantics, not live runtime delivery.

Public docs must also say the limitations whenever managed subagents are presented as a launch feature.

<a id="russian"></a>
# Russian Translation Status

The previous localized duplicate section was removed because it contained mojibake. The English section above is the canonical public launch record until a reviewed Russian translation is restored.
