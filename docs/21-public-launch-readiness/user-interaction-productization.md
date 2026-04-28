# User Interaction Productization

Status: Stage 7 public-launch readiness owner for the bounded CLI/package interaction slice. This document does not unlock full user-interaction readiness.

Related documents:

- [Public Launch Scope](./public-launch-scope.md)
- [Baseline Gap And Forbidden Claims](./baseline-gap-and-forbidden-claims.md)
- [Live Run Interaction](../06-interaction/live-run-interaction.md)
- [Interaction And Chat Contract](../03-contracts/agent-json/interaction-and-chat-contract.md)
- [Built-in MCP Contract: `orchestrator.user_chat`](../03-contracts/orchestrator-user-chat-mcp-contract.md)
- [Phase 15 Full User Interaction Layer](../16-full-user-interaction-layer/phase-15-full-user-interaction-layer.md)

## Scope

The productized Stage 7 slice is limited to local CLI/package behavior for:

- blocking built-in user-chat prompts emitted by a supported runtime adapter;
- explicit prompt replies submitted through `dennett-agent-orchestrator reply`;
- durable local resume after a recorded reply;
- `dennett-agent-orchestrator run-status` inspection of run and interaction state;
- transcript persistence according to the local chat policy.

This is not hosted UI readiness, not a full cross-interface interaction layer, and not a guarantee that every live Codex App Server prompt shape is supported.

## CLI Workflow

1. Start a run with `run`, `run-live`, or `event-dispatch`.
2. If the runtime emits a required built-in user-chat prompt, the run returns `RUN_WAITING_FOR_USER`, stores `resume.pending_prompt`, and remains locally resumable.
3. Inspect state with `run-status --run-id <id> --state-db <path>`.
4. Submit an explicit reply with `reply <agent-file> --run-id <id> --prompt-id <prompt-id> --text <text>` or with `--option-id` plus `--value` for supported options prompts.
5. Resume with `resume <agent-file> --run-id <id> --state-db <path>`.

`reply` records the reply in first-class pending-prompt state before live delivery. If live delivery to the runtime prompt handle fails or is unavailable, the reply remains recorded for explicit resume.

## Durable Prompt And Reply Semantics

The first-class interaction state is `resume.pending_prompt` in the local state store.

- A blocking prompt records the run id, attempt id, prompt id, prompt payload, request handle, and blocking flags.
- A prompt reply is stored as `pending_prompt.reply` with a reply id, prompt id, reply payload, idempotency key, delivery status, and timestamps.
- Resume consumes `pending_prompt.reply`; it no longer searches visible transcript messages for the active reply.
- Successful or terminal node completion clears `pending_prompt`.
- Visible chat messages remain transcript output only; they are not the source of truth for resume.

## Ordering, Duplicates, And Staleness

Only one pending prompt can block a run in the current execution model.

- A reply is accepted only while the run is `waiting_for_user` and the supplied agent revision matches the pinned run revision.
- A reply with the wrong prompt id is rejected.
- Repeating the same reply is idempotent and does not append another transcript message or redeliver live.
- A conflicting second reply before resume is rejected with `PROMPT_REPLY_ALREADY_RECORDED`.
- Late replies after completion, cancellation, interruption, or failure are rejected.
- Risky mid-run parameter or runtime-option changes are not applied during resume; changed agent files are rejected by resolved-revision mismatch.

## Live Comments Boundary

Comments remain separate from prompt replies.

- `comment` is accepted only for active runs on nodes explicitly configured as comment targets.
- Comments require adapter support for real live comment delivery.
- Comments are not queued for future nodes.
- Comments do not satisfy blocking prompts.

## Status Output And Redaction Boundary

`run-status` is the CLI/package status surface for this slice. It reports run status, active attempt metadata, pending prompt id, prompt kind, reply delivery status, resume flags, and visible transcript count.

`run-status` intentionally omits prompt and reply payload content. This is a bounded status redaction only; it is not a full secret-handling system.

Current secret and transcript limits:

- Prompt and reply payloads are still persisted in the local SQLite state database.
- If `chat.store_visible_messages` is enabled, prompt and reply payloads may also appear in visible transcript records.
- There is no supported secret prompt type.
- There is no field-level redaction policy for prompt or reply payload persistence.
- Users must not put secrets in prompts or replies unless their local retention policy accepts persistence in the state database.

## Evidence

Focused coverage exists for the bounded CLI/package slice:

- `tests/unit/sqlite-state-store.test.ts` verifies first-class prompt reply persistence, idempotency, live-delivery status updates, and blocked prompt metadata.
- `tests/unit/graph-runner.test.ts` verifies blocking prompt/resume behavior consumes first-class reply state.
- `tests/integration/stage7-interaction-edge-cases.test.ts` verifies late reply rejection, duplicate reply idempotency, conflicting duplicate rejection, and revision mismatch for risky mid-run changes.
- `tests/integration/stage7-cli-integrated-flow.test.ts` verifies builder/register/deploy/run-live/reply/run-status/resume wiring with a deterministic offline runtime fixture.
- `tests/fixtures/stage7-cli-integrated-flow-transcript.md` records the normalized offline CLI transcript.

The integrated Stage 7 proof is offline and mocked for runtime delivery. It proves CLI wiring and durable local semantics, not live provider reliability.

## Deferred Blockers

The following remain out of scope for this stage:

- hosted UI;
- cross-interface prompt ownership and conflict resolution;
- full live Codex App Server delivery proof for standalone CLI sessions;
- non-text and richer prompt shapes beyond the supported text/options request contract;
- secret prompt handling and durable field-level redaction;
- user-visible cancellation of pending prompts;
- multi-prompt queues or concurrent active prompts;
- risky mid-run model or parameter mutation inside an existing live chat.

## Forbidden Claims

Do not claim:

- full user interaction readiness;
- hosted or managed interaction readiness;
- secret-safe prompt/reply handling;
- live provider reliability for every supported runtime;
- transcript redaction beyond `run-status` payload omission;
- compatibility guarantees for future UI or cross-interface behavior.

Allowed claim: the local CLI/package slice has first-class blocked prompt/reply state, idempotent duplicate reply handling, explicit status output, and deterministic resume-after-reply tests for supported prompt shapes.
