[English](#english)

<a id="english"></a>
# Runtime App Server Certification

Status: canonical Stage 5 Runtime/App Server certification record for the `cli-package-first-public-launch` target. This document certifies only the bounded local CLI/package subset named below. It is not a hosted, managed, production, broad-provider, or full Codex App Server certification.

Related documents:

- [Public Launch Scope](./public-launch-scope.md)
- [Security, Privacy, And Legal Foundation](./security-privacy-legal-foundation.md)
- [Release Engineering And Supply Chain](./release-engineering-and-supply-chain.md)
- [Runtime Adapter Contract](../03-contracts/runtime-adapter-contract.md)
- [Runtime Integration Model](../02-architecture/runtime-integration-model.md)
- [Phase 14 Native Runtime Surface Completion](../15-native-runtime-surface/phase-14-native-runtime-surface-completion.md)
- [Runtime Sources](../08-extensions/runtime-sources.md)

## Certification Boundary

Stage 5 certifies this runtime subset for the public CLI/package-first launch path:

- the `codex` runtime adapter boundary exists and is the only current runtime provider path in scope;
- the Codex adapter is App Server-native behind the runtime port and is not modeled as product-level Codex CLI subprocess orchestration;
- local graph execution can pass normalized requests to the adapter and classify terminal success, invalid output, runtime errors, interruption, cancellation, and blocked user prompts;
- deterministic checks cover capability gating for runtime options, timeout/error taxonomy, structured output validation, runtime model/environment normalization, and graph-runner request construction;
- local CLI commands expose model catalog and runtime-environment inspection surfaces when the user's Codex App Server environment supports them.

Stage 5 does not certify:

- hosted, managed, SaaS, multi-tenant, uptime, SLA, support, or production operation;
- all App Server protocol families or all Codex models;
- non-Codex runtime providers;
- Linux or macOS runtime proof;
- broad provider availability, rate-limit, retention, privacy, deletion, training, or confidentiality behavior;
- native App Server memory, managed subagents, Builder 2.0, or stable public CLI/API compatibility.

## Status Labels

| Label | Meaning |
| --- | --- |
| Certified for public CLI/package scope | The behavior is in the selected local CLI/package launch subset, has deterministic evidence, and may be documented as supported only inside this boundary. |
| Implemented but not public-certified | Code exists, but evidence, user-visible semantics, or operational boundary is not sufficient for a public support claim. |
| Deterministic-only | Automated tests or static checks cover the behavior without requiring a live App Server/provider call. |
| Live-proof-required | A real App Server/provider/environment proof is required before making a broader public claim. |
| Unsupported/deferred | The behavior is not supported for the public launch subset or is reserved for a later stage. |

## Capability Matrix

| Capability | Stage 5 status | Evidence and boundary |
| --- | --- | --- |
| Runtime provider family | Certified for public CLI/package scope | Only `runtime_adapter: "codex"` is in scope. Other providers are unsupported/deferred. |
| App Server-native adapter path | Certified for public CLI/package scope | The Codex adapter owns App Server protocol translation behind the runtime port. This is not a full App Server certification. |
| Minimal graph execution through adapter | Certified for public CLI/package scope | Deterministic graph-runner tests cover request construction, terminal classification, output validation, resume state, memory context boundaries, and unsupported-context failures. Live provider quality remains outside this claim. |
| Model discovery | Implemented but not public-certified; live-proof-required for user-facing support | `listModels()` and `runtime-model-list` exist and are covered by deterministic normalization/CLI tests. Public support depends on the user's local authenticated Codex App Server path and later user-facing docs. |
| Model metadata normalization | Deterministic-only | The adapter normalizes model id, display metadata, default status, modalities, reasoning efforts, speed tiers, and personality support. This does not guarantee every live model exposes every option. |
| Runtime environment introspection | Implemented but not public-certified; live-proof-required for account-specific claims | `inspectRuntimeEnvironment()` and `runtime-env-inspect` expose auth, account, rate-limit, config, approval policy, sandbox mode, profile, reasoning effort, and service tier snapshots. These are local diagnostics, not portable agent truth. |
| Account, auth, config, and rate-limit data | Deterministic-only for shape; live-proof-required for real account status | Account identifiers and rate-limit details must be treated as sensitive local diagnostics and redacted in public evidence. Dennett does not promise provider availability or limit behavior. |
| `runtime_options.model` | Certified for public CLI/package scope | The graph-runner allowlist accepts `model` and the Codex adapter passes it through App Server thread/turn parameters. Unsupported live model ids may still fail at runtime. |
| `runtime_options.reasoning_effort` | Certified for public CLI/package scope when adapter capability is true; deterministic-only for option behavior | The graph-runner now rejects this option unless `supports_reasoning_effort = true`; the Codex adapter advertises support and validates known values before App Server calls. Live model-specific support remains live-proof-required. |
| `runtime_options.speed_tier` | Certified for public CLI/package scope when adapter capability is true; deterministic-only for option behavior | The graph-runner now rejects this option unless `supports_speed_tiers = true`; the Codex adapter advertises support and validates known values before App Server calls. Live account/model availability remains live-proof-required. |
| `runtime_options.personality` | Certified for public CLI/package scope when adapter capability is true; deterministic-only for option behavior | The graph-runner now rejects this option unless `supports_personality = true`; the Codex adapter advertises support and validates known values before App Server calls. Live model support remains live-proof-required. |
| Unknown runtime options | Certified for public CLI/package scope | Unknown keys fail fast with `UNSUPPORTED_RUNTIME_CONTEXT` instead of being silently forwarded. |
| Explicit runtime source selection | Unsupported/deferred for current Codex adapter | The normalized contract exists, but the current Codex adapter reports `supports_explicit_runtime_source = false`. Agent files requiring runtime sources fail before execution. |
| Runtime source introspection | Unsupported/deferred for current Codex adapter | The current Codex adapter reports `supports_runtime_source_introspection = false`; source availability and per-source limit checks are not public-certified. |
| Runtime sources/source metadata as portable truth | Unsupported/deferred | Runtime source and account metadata are local diagnostics or configured source references, not portable agent-file truth. |
| Native resume | Implemented but not public-certified | Runtime/session handles are persisted and can be used when the adapter supports native resume, but full live cross-process App Server behavior needs later user-visible proof. |
| Live comments | Implemented but not public-certified | The `comment` CLI command and `deliverComment()` path exist for live runtime handles. Public support requires live proof for supported App Server versions and failure modes. |
| Built-in user-chat prompt detection | Implemented but not public-certified | Deterministic tests cover blocked waits, prompt persistence, reply payload validation, and resume behavior. Full user-visible interaction readiness is Stage 7. |
| Fresh-process live user-chat reply | Unsupported/partial | A reply can be recorded for later resume, but live reply delivery through a fresh adapter process is not certified because the original live App Server client is required for the active prompt. |
| Generic native App Server events | Unsupported/deferred as public product surface | The adapter may consume native notifications internally, but only normalized `comment` and `user_chat_request` runtime events have current port meaning. Rich turn/item/plan/token/model-reroute/MCP/account/config/filesystem events are not public-certified. |
| CLI cancellation | Unsupported/partial | The runtime port has `cancelExecution()` and the adapter maps it to App Server interruption, but a public CLI cancellation flow is not certified in this stage. Do not claim complete user-facing cancellation support. |
| Timeout taxonomy | Certified for public CLI/package scope | Deterministic tests cover operation-scoped timeout codes for execution, model catalog, environment inspection, comments, and prompt replies. Timeout is `runtime_error`, not cancellation. |
| Sandbox and approval policy exposure | Implemented but not public-certified | Runtime environment inspection may show sandbox and approval policy config. This is diagnostic visibility only and must not imply Dennett enforces a sandbox beyond the actual runtime/provider policy. |
| Memory through runtime | Implemented but not Stage 5-certified | Runtime memory is prompt-rendered context plus success-only provider writes. Memory productization and provider limits belong to Stage 6. |
| Managed subagents and Builder 2.0 | Unsupported/deferred for Stage 5 public runtime certification | These surfaces remain later-stage product claims and must not be inferred from runtime adapter evidence. |

## Certified Public Runtime Claim

After Stage 5 evidence is present, the allowed public claim is:

"Dennett's CLI/package-first launch target includes a limited Codex App Server runtime adapter subset for local use. The certified subset covers normalized graph execution through the `codex` adapter, selected runtime options gated by adapter capabilities, deterministic model/environment metadata normalization, timeout classification, and explicit unsupported-feature failures. Broader App Server, hosted, production, all-model, all-option, source-introspection, and full interaction claims remain out of scope."

No shorter claim may remove the words "limited", "local", or "subset" unless a later stage records stronger evidence and updates this document.

## Security And Data-Handling Caveats

Runtime execution may send prompts, resolved inputs, runtime options, structured-output schemas, prompt-rendered memory context, comments, and user-chat replies to the selected runtime provider. Runtime introspection may reveal local auth/account/config/rate-limit diagnostics.

Public docs and evidence must follow these rules:

- do not publish account identifiers, tokens, private paths, raw prompts, raw outputs, provider URLs with credentials, or user-specific rate-limit details;
- do not imply that Dennett controls provider retention, training, telemetry, deletion, residency, confidentiality, uptime, or rate-limit behavior;
- do not imply sandboxing beyond the effective runtime sandbox/approval policy reported by the user's local runtime;
- do not treat runtime environment snapshots, model catalogs, or account metadata as portable agent-file truth;
- do not use successful deterministic tests as proof that a live provider account supports every model, effort, speed tier, personality, or interaction path.

## Deterministic Evidence

The Stage 5 deterministic evidence set is:

- unit coverage for Codex App Server adapter request normalization, model metadata, environment metadata, timeout mapping, user-chat request normalization, prompt replies, live comments, cancellation mapping, and runtime option validation;
- unit coverage for graph-runner runtime-option allowlisting and capability gates for `reasoning_effort`, `speed_tier`, and `personality`;
- CLI/helper coverage for runtime model list, runtime environment inspect, comments, replies, and option parsing;
- release-candidate, lint/format, typecheck, build, and package-foundation guards where applicable.

No Stage 5 worker is required to call a live App Server. Any future live proof must record exact environment, command, redaction policy, result, and the claim it unlocks.

## Remaining Public-Launch Blockers

The runtime/App Server surface remains blocked from broader public claims until later stages or explicit follow-up tasks provide:

- live App Server proof for the documented package artifact and supported OS matrix;
- user-facing docs for runtime setup, authentication, expected local config, safe redaction, model selection, and failure recovery;
- evidence for selected live models and their supported reasoning-effort, speed-tier, and personality combinations;
- a decision and proof for fresh-process user-chat reply behavior or a documented unsupported path;
- a public cancellation command/flow with tests and user-visible semantics, or an explicit unsupported statement in CLI docs;
- source-selection and source-introspection implementation if those are to become public features;
- stable CLI/API compatibility inventory in Stage 10.
