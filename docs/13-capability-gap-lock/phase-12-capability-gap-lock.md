[English](#english) | [Russian](#russian)

<a id="english"></a>
# Phase 12 Capability Gap Lock

Status: normative owner for the Phase 12 capability freeze.

Related documents:

- [AGENTS.md](../../AGENTS.md)
- [Documentation Root](../README.md)
- [Hardening Validation Matrix](../11-hardening/validation-matrix.md)
- [Phase 19 Real-World Proof And Release](../20-real-world-proof-and-release/README.md)
- [Release Scope Lock](../20-real-world-proof-and-release/release-scope-lock.md)
- [Runtime Integration Model](../02-architecture/runtime-integration-model.md)
- [Memory Bindings](../08-extensions/memory-bindings.md)

## Purpose

Phase 12 exists to stop the project from claiming more implementation than it has.

The project completed stages 1 through 11 and now has a coherent foundation, but the documentation tree intentionally contains behavior that is broader than the current executable slice. Before further implementation work continues, Dennett needs one frozen matrix that says:

- what is implemented now;
- what is only partially implemented;
- what is only documented;
- what is blocked by runtime or provider prerequisites;
- which later stage owns each remaining gap.

This document is that freeze.

## Status Labels

Use these labels exactly:

- `implemented`: exists in code, has the expected contract shape, and is covered by meaningful tests.
- `partial`: exists in code or tests, but not yet at the full documented behavior level.
- `documented_only`: owned by docs and contracts, but not yet implemented in the executable slice.
- `runtime_blocked`: integration work started, but a real external prerequisite still blocks live proof.
- `not_started`: not materially implemented yet.

A capability may be strong in one column and weak in another. Phase 12 therefore tracks four evidence axes:

- `docs_owner`
- `code_status`
- `test_status`
- `live_proof_status`

## Evidence Rules

The project must not claim a capability is done unless all of the following are true:

1. an owner document exists;
2. the executable slice actually implements it;
3. tests verify the intended behavior at the right layer;
4. at least one honest live-proof path exists when the capability depends on a real runtime or provider.

'Package downloaded', 'types compiled', 'schema exists', or 'adapter stub created' do not count as live proof.

## Frozen Capability Matrix

| Capability family | Main owner docs | Code status | Test status | Live proof status | Gap owner |
| --- | --- | --- | --- | --- | --- |
| Portable graph execution for `runtime_agent` | [Graph Execution](../04-execution/graph-execution.md), [Outputs, Outcomes, and Final Response](../04-execution/outputs-outcomes-and-final-response.md) | `implemented` | `implemented` | `implemented` on supported Codex models | Keep under completed 5-7; only integrated regressions remain later |
| `orchestrator_agent` child runs as portable nested graph primitive | [Subagent Orchestration Model](../02-architecture/subagent-orchestration-model.md), [Graph Execution](../04-execution/graph-execution.md) | `partial` | `partial` | `partial` | Phase 16 |
| Draft/live/deploy lifecycle | [Agent Registry](../07-lifecycle/agent-registry.md), [Draft, Live, and Deploy](../07-lifecycle/draft-live-deploy.md) | `implemented` | `implemented` | `implemented` for local slice | Later integrated proof in Phase 18-19 |
| Lifecycle command app use cases and CLI delegation | [Agent Registry](../07-lifecycle/agent-registry.md), [Draft, Live, and Deploy](../07-lifecycle/draft-live-deploy.md), [Stable CLI/API Contract Freeze](../21-public-launch-readiness/stable-cli-api-contract-freeze.md) | `implemented` for local `register`, `status`, and `deploy` use-case delegation through `src/app`; no new lifecycle state semantics are claimed | `implemented` by focused app-facade delegation and state-store close-path coverage | `partial`: covered by the existing bounded local lifecycle slice; no new external or operator-facing live proof is claimed from the facade refactor alone | Later integrated proof in Phase 18-19 |
| Builder draft authoring and Builder 2.0 public-contract-aware authoring | [Builder Agent](../08-extensions/builder-agent.md), [Phase 17 Builder 2.0](../18-builder-2-0/phase-17-builder-2-0.md), [Stage 17 Contract Proof And Failure Modes](../18-builder-2/stage-17-contract-proof-and-failure-modes.md) | `partial`: the executable slice remains draft-first and public-contract-only; deterministic proof now covers representative builder-authored runtime controls, memory bindings, interaction/chat, and portable `orchestrator_agent` draft patterns; managed-subagent task-package and review-loop details remain outside portable Agent JSON | `implemented` for the bounded deterministic Builder 2.0 proof in `tests/unit/builder-service.test.ts` and `tests/unit/builder-output-schema.test.ts`, including wrapper/schema/audit rejection and one repair-attempt path | `partial`: no live builder generation, live provider, live runtime execution of the richer draft, or Phase 18 integrated-flow proof is claimed | Phase 18 for integrated product-flow proof; Phase 19 for live/external/public-readiness proof |
| Architecture safety gates and static boundary check | [Product Roadmap And Safety Gates](../product-roadmap-and-safety-gates.md), [Stage Safety Gates](../11-hardening/stage-safety-gates.md), [Runtime Integration Model](../02-architecture/runtime-integration-model.md) | `partial`: stage gates are documented and a static import-boundary checker exists with explicit current exceptions, but the checker is a guardrail and not proof that all architecture cleanup is complete | `implemented` for checker behavior, stale exception detection, forbidden core/interface edges, and concrete-technology import rules | `not_started`: no CI or live operational proof is claimed by this reconciliation | Continuous safety gates; existing exceptions need future cleanup or recorded acceptance |
| App facade boundary for local product use cases | [Product Roadmap And Safety Gates](../product-roadmap-and-safety-gates.md), [Graph Execution](../04-execution/graph-execution.md), [Agent Registry](../07-lifecycle/agent-registry.md) | `partial`: `src/app` now provides local run and lifecycle use-case seams used by the CLI, but it is an internal architecture seam, not a daemon, hosted app server, runtime host, or core-process implementation | `implemented` for focused local run and lifecycle delegation paths | `partial`: reuses existing local CLI/core proof; no separate live proof is claimed for `src/app` as an external surface | Future app/server or hosted surface work must be separately scoped and proven |
| Direct local `run` app use case and CLI delegation | [Graph Execution](../04-execution/graph-execution.md), [Runtime Integration Model](../02-architecture/runtime-integration-model.md), [Stable CLI/API Contract Freeze](../21-public-launch-readiness/stable-cli-api-contract-freeze.md) | `implemented` for loading, revision resolution, state-store creation, adapter delegation, and CLI handoff through the app use case; no new runtime semantics are claimed beyond the existing core runner | `implemented` by focused delegation and failure close-path coverage | `partial`: existing live graph proof still supports the underlying local run path, but this reconciliation does not add a new live run proof specific to the app facade | Keep under completed 5-7 for core behavior; Phase 19 owns broader proof |
| App Server-backed Codex execution | [Runtime Integration Model](../02-architecture/runtime-integration-model.md), [Runtime Adapter Contract](../03-contracts/runtime-adapter-contract.md) | `implemented` | `implemented` | `implemented` on supported models | Phase 14 for richer native surface |
| Model discovery, richer model metadata, speed tiers, reasoning effort, rate-limit/account/config introspection | [Runtime Integration Model](../02-architecture/runtime-integration-model.md), [Runtime Adapter Contract](../03-contracts/runtime-adapter-contract.md), [Phase 14 Native Runtime Surface Completion](../15-native-runtime-surface/phase-14-native-runtime-surface-completion.md) | `implemented` for the normalized local runtime surface | `implemented` | `partial`: built CLI live proof now exists for model discovery and runtime-environment introspection; the richer runtime-option controls still rely primarily on focused automated validation | Keep as implemented; later phases broaden proof depth |
| Live comments during run | [Live Run Interaction](../06-interaction/live-run-interaction.md) | `implemented` for Codex App Server path | `implemented` | `partial` | Phase 15 for broader interaction proof |
| Built-in user chat MCP | [Interaction and Chat Contract](../03-contracts/agent-json/interaction-and-chat-contract.md), [Orchestrator User Chat MCP Contract](../03-contracts/orchestrator-user-chat-mcp-contract.md), [Phase 15 Full User Interaction Layer](../16-full-user-interaction-layer/phase-15-full-user-interaction-layer.md) | `implemented`: core/state/CLI prompt machinery plus the Codex adapter now expose a durable `waiting_for_user` / `pending_prompt` path and honest reply/resume flow on the supported Codex adapter path | `implemented` | `partial`: covered by focused adapter and graph-runner tests; no broader external live proof claimed in this freeze | Phase 15 |
| Top-level and node-level skills, plugins, and MCP bindings during execution | [Top-Level and Bindings Contract](../03-contracts/agent-json/top-level-and-bindings-contract.md) | `documented_only` in the executable slice; current runner fails fast | `partial` by validation/fail-fast coverage | `not_started` | Phase 14 and Phase 18 |
| Portable memory-binding contract | [Memory Binding Model Contract](../03-contracts/agent-json/memory-binding-model-contract.md), [Memory Bindings](../08-extensions/memory-bindings.md) | `implemented` at schema/doc/runtime-validation level and now consumable by Core memory resolution | `implemented` | `implemented` through Phase 13 binding-driven provider resolution | Keep as implemented; later phases only extend runtime-native usage |
| Real external memory provider integration | [Memory Bindings](../08-extensions/memory-bindings.md), [Phase 13 Mem0-First Native Memory Integration](../14-native-memory-integration/phase-13-mem0-first-native-memory-integration.md) | `partial` with a real provider-backed product slice plus bounded Mem0 namespace cleanup | `partial` with targeted registry, adapter, service, CLI, and cleanup coverage | `partial` with Mem0-first live proof plus TASK-357 verified scoped namespace cleanup | Later phases add more providers, runtime-native consumption, true restore if required, graph-store cleanup, provider-wide cleanup only if explicitly specified, and reliability proof |
| Mem0-first provider staging | [Memory Bindings](../08-extensions/memory-bindings.md), [Phase 13 Mem0-First Native Memory Integration](../14-native-memory-integration/phase-13-mem0-first-native-memory-integration.md), this document | `implemented` for direct local provider CRUD/search and verified scoped namespace cleanup | `implemented` | `implemented` for local Mem0 CRUD/search plus TASK-357 target-cleanup/control-survival proof | Keep as completed for the bounded local Mem0 path; later phases broaden provider coverage and must not infer true restore, graph-store cleanup, provider-wide cleanup, or reliability from this row |
| Narrow Stage 2 runtime graph memory for Codex plus registered Mem0 | [Memory Bindings](../08-extensions/memory-bindings.md), [Phase 13 Mem0-First Native Memory Integration](../14-native-memory-integration/phase-13-mem0-first-native-memory-integration.md), [Evidence Log](../20-real-world-proof-and-release/evidence-log.md) | `partial`: implemented only for Core-resolved registered providers, provider-neutral `memory_context`, Codex prompt rendering, and success-only provider writes | `partial`: focused automated coverage exists; the initial TASK-333 default `pnpm test` failure is superseded by the passing TASK-334 review rerun | `implemented` for one disposable local Mem0 plus Codex App Server proof; not native App Server memory | Later phases must prove broader providers, native runtime surfaces if any, durability/cleanup, reliability, and continued green default gates |
| Runtime sources, limits, and source selection/introspection | [Runtime Sources](../08-extensions/runtime-sources.md), [Runtime Adapter Contract](../03-contracts/runtime-adapter-contract.md), [Phase 14 Native Runtime Surface Completion](../15-native-runtime-surface/phase-14-native-runtime-surface-completion.md) | `partial`: source selection exists, model/env introspection now exists, per-source introspection remains unsupported in current Codex adapter | `partial` | `partial`: global runtime-model and runtime-environment proof exists, but source-specific introspection is still unsupported | Phase 14 and later interaction/runtime-source hardening |
| Managed subagent orchestration with roles, budgets, write scopes, and review loops | [Subagent Orchestration Model](../02-architecture/subagent-orchestration-model.md), [Subagent MCP Contract](../03-contracts/subagent-mcp-contract.md), [Subagent Task Lifecycle](../04-execution/subagent-task-lifecycle.md), [Phase 16 Managed Subagent Orchestration](../17-managed-subagent-orchestration/phase-16-managed-subagent-orchestration.md) | `implemented` for the bounded local managed-subagent layer: task-package snapshots with `read_context`, `required_validations`, and `interaction_policy: "silent"`; worker/reviewer/explorer/integrator/final-review roles; `launch` / `status` / `wait` / `send` / `cancel` / `close`; durable status projection, review/fix workflow state, state-level cancellation with no live runtime cancellation claim, durable findings, budgets, and sibling `write_set` conflict rejection while staying distinct from plain `orchestrator_agent` recursion | `implemented` for the accepted local service/state/CLI slice by targeted subagent service and CLI coverage, including review-loop and budget-exhaustion paths | `partial`: bounded local proof is automated and accepted; no broader external, cross-process, live runtime cancellation/status-probe, hosted operator-platform, or public product proof is claimed | Stage 18/19 and future core-process/runtime-host work own daemon/background runners, live delivery/probes/cancellation, cross-process attachment, broader operator readiness, and external proof |
| Integrated product flows across lifecycle, builder, runtime features, interaction, memory, and subagents | [Phase 18 Integrated Product Flows](../19-integrated-product-flows/README.md) plus the subsystem owner docs it references | `partial`: Phase 18 defines cross-subsystem flow boundaries and now has a local/offline executable slice over the existing product surfaces; no new external-runtime product code is claimed here | `implemented` for local/offline automated evidence through `tests/integration/phase18-integrated-product-flows.test.ts`, which passes locally | `not_started`: Phase 19 still owns live external proof, stress proof, and release-readiness evidence | [Phase 19](../20-real-world-proof-and-release/README.md) for real-world external proof, stress proof, operational evidence, and release readiness |
| CLI/source checkout readiness and public launch boundary | [Stable CLI/API Contract Freeze](../21-public-launch-readiness/stable-cli-api-contract-freeze.md), [Public Docs, Onboarding, And Claims](../21-public-launch-readiness/public-docs-onboarding-and-claims.md), [Final Public Launch Gate Decision](../21-public-launch-readiness/final-public-launch-gate-decision.md) | `partial`: source-checkout CLI invocation remains the supported local path, while public launch, public npm/package publication, hosted deployment, and production claims remain blocked by their owner docs | `partial`: existing local/package-readiness gates and docs support bounded local use; this reconciliation did not rerun or expand public-launch gates | `partial`: bounded local checkout and local package-readiness evidence only; no current public launch approval is claimed | Public launch readiness owners must replace blockers with durable evidence before broader claims |
| Real-world release proof beyond local and mocked slices | [Phase 19 Real-World Proof And Release](../20-real-world-proof-and-release/README.md), [Release Scope Lock](../20-real-world-proof-and-release/release-scope-lock.md), [Operational Readiness](../11-hardening/operational-readiness.md), [Release Gates](../11-hardening/release-gates.md) | `not_started`: no new product code is claimed for Phase 19 proof and no broad product release-ready state is claimed from local/offline evidence alone | `implemented` for the bounded local CLI/repository release target: current repository gates, Phase 18 local/offline integration evidence, TASK-287 live graph smoke, TASK-290 stress/regression evidence, TASK-291 local operational/recovery/cleanup evidence, TASK-292 bilingual documentation cleanup, TASK-454 deterministic local Stage 8 stress/recovery/regression gate rerun, and TASK-495 final candidate gates support only `local-cli-repository-readiness` on commit `c3ad3eafca28f4a602a6e44d1861054aabc96a03` | `implemented` for the bounded target only: TASK-280 proved runtime discovery and direct local Mem0 provider operations; TASK-287 proved minimal live Codex graph execution; TASK-454 confirms deterministic local-only stress/recovery/regression proof, including explicit retry/resume completion and exactly-once final output after crash/reopen, not live provider stress; TASK-495 passed final local release-candidate gates; the final release decision is bounded `release` for the locked local CLI/repository target | [Phase 19](../20-real-world-proof-and-release/README.md); included and deferred capabilities are locked by [Release Scope Lock](../20-real-world-proof-and-release/release-scope-lock.md); broader release claims remain deferred by the [release decision record](../20-real-world-proof-and-release/release-decision-record.md), including hosted/managed deployment, npm/public package publication, installer/container distribution, durable external provider cleanup, external-provider reliability, live provider stress, production-scale load, broad runtime-memory/provider support, native App Server memory, full App Server certification, full user interaction layer, operator-platform managed subagent readiness, and public Builder 2.0 readiness |

## 2026-04-29 Stage 17 Builder Contract Proof

This proof records accepted bounded deterministic Builder 2.0 status for the local public-contract proof scope.

- `tests/unit/builder-output-schema.test.ts` now accepts a representative formal builder wrapper containing runtime options, runtime sources, memory bindings, interaction/chat fields, and a portable `orchestrator_agent` node.
- `tests/unit/builder-service.test.ts` covers accepted richer candidates, draft-only persistence, public-contract-only builder context, wrapper/schema/audit rejection before persistence, and one structured repair attempt.
- [Stage 17 Contract Proof And Failure Modes](../18-builder-2/stage-17-contract-proof-and-failure-modes.md) documents wrapper, schema, identity, audit, and repair failure modes for users.
- Accepted capabilities are public-contract rich draft proof, builder repair pass, failure docs, schema/audit gates, and draft-only persistence.
- No live builder generation, live richer-draft execution, real provider operation, native App Server memory, managed-subagent MCP authoring, public Builder 2.0 readiness, or Phase 18 integrated-flow proof is claimed by this deterministic slice.

## 2026-04-29 App Facade And Architecture Gate Reconciliation

This reconciliation records the current repository evidence after the app-facade and architecture-gate slices.

- `src/app` is an internal application/use-case seam for local CLI operations. It is not a daemon, hosted App Server, replacement core process, external runtime host, or public plugin surface.
- Direct local `run` and lifecycle command delegation are code-and-test supported at the facade boundary, but they intentionally reuse the existing core runner, lifecycle service, local state store, and runtime adapter contracts.
- The architecture boundary checker is a safety gate with explicit current exceptions. It prevents new undocumented boundary violations, but it does not erase or complete the existing exceptions.
- CLI/source-checkout readiness remains bounded to local checkout and local package-readiness claims. Public launch, public npm publication, installer/container distribution, hosted/managed deployment, and production readiness remain blocked unless their public-launch owner documents are later updated with durable evidence.
- Live proof claims are unchanged except where the matrix already names existing bounded local proof. Facade unit tests do not by themselves create new external runtime, provider, operator-facing, or public-launch proof.

## 2026-04-29 Stage 14 Runtime Surface Certification

This certification records evidence for the narrow Stage 14 native runtime surface only: normalized model discovery, runtime-environment introspection, runtime-option contract validation, CLI exposure, graph-runner capability gating, and schema coverage.

Deterministic proof passed on 2026-04-29:

- `pnpm exec vitest run tests/unit/codex-app-server-runtime-adapter.test.ts tests/unit/runtime-cli.test.ts tests/unit/runtime-adapter-capabilities-schema.test.ts tests/unit/runtime-adapter-request-schema.test.ts tests/unit/graph-runner.test.ts`: 5 test files passed, 83 tests passed.
- `pnpm architecture:check`: passed across 31 files with the three already allowlisted existing exceptions for `builder-service.ts`, `memory-service.ts`, and `sqlite-state-store.ts`.

Safe live CLI smoke passed on 2026-04-29 against the local Codex/App Server path:

- `pnpm dennett runtime-model-list --limit 5 --codex-app-server-model-catalog-timeout-ms 10000`: returned 5 normalized model records and `next_cursor: "5"`; the default returned model was `gpt-5.4`, and returned metadata included modalities, default reasoning effort, personality support, speed tiers, and upgrade targets where present.
- `pnpm dennett runtime-env-inspect --redacted --codex-app-server-environment-timeout-ms 10000`: returned redacted authenticated ChatGPT account metadata with available account status, empty `rate_limits`, and config values `model: "gpt-5.5"` and `model_reasoning_effort: "high"`.

Status boundary:

- Live proof is `partial`, not broad runtime certification. The live smoke proves model discovery and runtime-environment inspection through the built CLI on this local App Server path.
- `reasoning_effort`, `speed_tier`, `personality`, timeout classification, runtime-source gating, schema validation, and graph-runner allowlist behavior remain certified by focused deterministic tests, not by a separate live execution proof in this note.
- Per-source runtime introspection, per-source execution, rich native event contracts, native App Server memory, full user interaction, and managed subagent product readiness remain deferred.

## Mem0-First Readiness Freeze

Mem0 is the first external memory target for Dennett.

Phase 12 freezes the following facts:

### What is now true

- Mem0 source is staged locally and the Python package is installed in a local sandbox.
- Dennett now has a real `Mem0MemoryAdapter` in product code.
- Dennett now has local provider registration, capability negotiation, and direct provider-backed memory execution through Core and CLI.
- The repository now has Mem0-backed tests for the adapter and for the memory service slice.
- A real local Mem0 round-trip has been proven through both the test suite and the built CLI path.
- TASK-357 proved verified scoped delete for a configured Mem0 namespace and explicit user scope, with target cleanup and control namespace survival.

### What is still not true

- The current Codex `runtime_agent` execution path consumes memory only through the narrow Stage 2 prompt-rendered `memory_context` path. It is not native App Server memory.
- Provider families beyond Mem0 are still not implemented.
- MCP-backed memory transport remains future work, not a completed product lane.
- Cross-agent memory propagation rules remain unimplemented.
- True restore, graph-store cleanup, provider-wide cleanup, and provider reliability under load are still not proven.

### Consequence for the matrix

The honest status is no longer `runtime_blocked` for Mem0-first integration.

The remaining gap is now narrower:

- Mem0-first native memory integration is implemented;
- general multi-provider memory support remains partial;
- broader runtime-memory behavior beyond the Stage 2 registered local Mem0 plus Codex prompt-rendering proof remains later-phase work;
- Stage 3 cleanup is only verified scoped delete for the configured namespace and explicit scope, not a general provider lifecycle guarantee.

## Phase 12 Acceptance Criteria

Phase 12 is complete only when all of the following are true:

1. the status labels are canonical;
2. the capability matrix exists and is linked from the docs tree;
3. AGENTS.md canonically defines the post-11 roadmap;
4. the current Mem0-first staging status is frozen without pretending it is already a product feature;
5. the next phase can start from this matrix without reopening scope discovery.

## What Phase 12 Explicitly Does Not Do

Phase 12 does not:

- implement external memory in product code;
- claim live proof for memory when only package staging exists;
- complete the richer App Server feature surface;
- implement managed subagents;
- upgrade builder beyond the current slice;
- turn the project into a different product.

It is a freeze and handoff stage, not a hidden implementation stage.

## Next Phases After Phase 12

The next canonical stages used to be:

- **Phase 13: Native Memory Integration (Mem0 First)**
- **Phase 14: Native Runtime Surface Completion**

Those phases are now implemented and recorded in:

- [Phase 13 Mem0-First Native Memory Integration](../14-native-memory-integration/phase-13-mem0-first-native-memory-integration.md)
- [Phase 14 Native Runtime Surface Completion](../15-native-runtime-surface/phase-14-native-runtime-surface-completion.md)

## TASK-333 Stage 2 Freeze Note

Русский: Narrow Stage 2 runtime graph memory теперь имеет один live proof: registered local Mem0, provider-neutral `memory_context`, Codex prompt rendering и success-only provider write. Статус остается `partial`, потому что это не native App Server memory, не broad provider support, не durable cleanup и не reliability proof. Initial TASK-333 default `pnpm test` failure superseded by passing TASK-334 review rerun.

## TASK-357 Stage 3 Freeze Note

TASK-357 adds one live provider-operations proof for Mem0 namespace cleanup: two target records in `task357-target-namespace` were previewed and deleted with token confirmation for `task357-cleanup-user`, target list returned `[]`, and a control record in `task357-control-namespace` survived. This upgrades the Mem0-first local provider path only for bounded scoped cleanup. It does not upgrade the matrix to true restore, graph-store cleanup, provider-wide cleanup, multi-provider cleanup, or external provider reliability.

## 2026-04-29 Stage 13 Local Mem0 Proof Rerun

The local Mem0 proof was rerun against the repository-local Python sandbox at `.local/mem0-venv/Scripts/python.exe`. The first plain `pnpm test:mem0` attempt reached the real runner but timed out at the default 180000 ms. A focused real CRUD/search test passed after pinning FastEmbed and Hugging Face cache locations under `.local`, and the required full proof then passed with `FASTEMBED_CACHE_PATH=.local/fastembed_cache`, `HF_HOME=.local/huggingface`, `HUGGINGFACE_HUB_CACHE=.local/huggingface/hub`, and `DENNETT_MEM0_TEST_TIMEOUT_MS=600000`: three Mem0-related test files passed with 37 tests. This confirms the bounded local Mem0 CRUD/search, namespace cleanup, CLI helper, and memory-service proof path only; it does not add broader provider reliability, provider-wide cleanup, true restore, graph-store cleanup, native App Server memory, or multi-provider claims.

## 2026-04-30 Stage 19 Local Candidate Evidence Rerun

The Stage 19 local release-candidate evidence rerun records commit `1f27dce0005205b4ddb8621184cf1e0b441c0dd8`, package version `0.1.0-rc.1`, and `private: true`. The worktree was dirty before the rerun, with modified docs, package metadata, source, and tests plus visible untracked product paths. `pnpm public-release-foundation:check` passed while still reporting public launch blocked, and `pnpm packlist:check` passed with 94 files. `pnpm release-candidate:check` failed because visible untracked product paths exist under `docs/**`, `scripts/**`, `src/**`, and `tests/**`.

This does not change the frozen capability matrix to a public-launch-ready or clean current-candidate state. It preserves the existing bounded local/package-readiness claim boundary and adds a current blocker: the intended product files must be tracked, staged, or otherwise resolved through the normal review flow before the current checkout can be treated as a frozen local release-candidate baseline. Optional local package, SBOM, and hash evidence was intentionally not produced from the dirty blocked state.

<a id="russian"></a>

# Phase 12 Capability Gap Lock

Localization note: if any localized duplicate row below diverges from the English matrix, the English row is authoritative. Stage 17 Builder 2.0 is accepted for the bounded deterministic public-contract proof scope only: public-contract rich draft proof, builder repair pass, failure docs, schema/audit gates, and draft-only persistence. Live generation, live execution of generated drafts, real provider behavior, native App Server memory, managed-subagent MCP authoring, full integrated product flows, and public readiness remain deferred.

Статус: нормативный owner-документ для заморозки возможностей на Phase 12.

Связанные документы:

- [AGENTS.md](../../AGENTS.md)
- [Корень документации](../README.md)
- [Hardening Validation Matrix](../11-hardening/validation-matrix.md)
- [Phase 19 Real-World Proof And Release](../20-real-world-proof-and-release/README.md)
- [Release Scope Lock](../20-real-world-proof-and-release/release-scope-lock.md)
- [Модель интеграции runtime](../02-architecture/runtime-integration-model.md)
- [Memory Bindings](../08-extensions/memory-bindings.md)

## Назначение

Phase 12 нужен для того, чтобы проект перестал приписывать себе больше реализации, чем у него есть на самом деле.

Этапы 1-11 завершены, и у проекта уже есть самосогласованный фундамент, но дерево документации намеренно описывает поведение шире, чем текущий executable slice. Прежде чем идти дальше, Dennett нужен один зафиксированный документ, который честно говорит:

- что уже реализовано;
- что реализовано только частично;
- что пока только описано;
- что упирается во внешние runtime/provider prerequisites;
- какой следующий этап владеет каждым оставшимся разрывом.

Этот документ и есть такая фиксация.

## Метки Статуса

Используйте только эти метки:

- `implemented`: есть в коде, имеет ожидаемую форму контракта и покрыто meaningful tests.
- `partial`: есть в коде или тестах, но еще не дотягивает до полного документированного поведения.
- `documented_only`: описано в owner-docs и контрактах, но еще не реализовано в executable slice.
- `runtime_blocked`: интеграция начата, но для live proof все еще не хватает реальной внешней предпосылки.
- `not_started`: материальной реализации еще нет.

У одной возможности могут быть сильные и слабые стороны по разным осям. Поэтому Phase 12 фиксирует четыре оси доказательства:

- `docs_owner`
- `code_status`
- `test_status`
- `live_proof_status`

## Правила Доказательства

Проект не должен заявлять, что возможность готова, если не выполнено все сразу:

1. существует owner-документ;
2. executable slice действительно это реализует;
3. тесты проверяют ожидаемое поведение на правильном уровне;
4. есть хотя бы один честный live-proof путь, если возможность зависит от реального runtime или provider.

'Пакет скачан', 'типы компилируются', 'схема существует' или 'создан adapter stub' не считаются live proof.

## Зафиксированная Матрица Возможностей

| Семейство возможностей | Основные owner-docs | Статус кода | Статус тестов | Статус live proof | Следующий владелец разрыва |
| --- | --- | --- | --- | --- | --- |
| Portable graph execution для `runtime_agent` | [Graph Execution](../04-execution/graph-execution.md), [Outputs, Outcomes, and Final Response](../04-execution/outputs-outcomes-and-final-response.md) | `implemented` | `implemented` | `implemented` на поддерживаемых Codex-моделях | Остается в завершенных 5-7; дальше только integrated regressions |
| `orchestrator_agent` child runs как portable primitive для nested graph | [Subagent Orchestration Model](../02-architecture/subagent-orchestration-model.md), [Graph Execution](../04-execution/graph-execution.md) | `partial` | `partial` | `partial` | Phase 16 |
| Lifecycle draft/live/deploy | [Agent Registry](../07-lifecycle/agent-registry.md), [Draft, Live, and Deploy](../07-lifecycle/draft-live-deploy.md) | `implemented` | `implemented` | `implemented` для local slice | Дальше integrated proof в Phase 18-19 |
| Lifecycle command app use cases и CLI delegation | [Agent Registry](../07-lifecycle/agent-registry.md), [Draft, Live, and Deploy](../07-lifecycle/draft-live-deploy.md), [Stable CLI/API Contract Freeze](../21-public-launch-readiness/stable-cli-api-contract-freeze.md) | `implemented` для локальной `register`, `status` и `deploy` use-case delegation через `src/app`; новые lifecycle state semantics не заявляются | `implemented` за счет focused app-facade delegation и state-store close-path coverage | `partial`: покрыто существующим bounded local lifecycle slice; новый external или operator-facing live proof от одного facade refactor не заявляется | Дальше integrated proof в Phase 18-19 |
| Builder draft authoring and Builder 2.0 public-contract-aware authoring | [Builder Agent](../08-extensions/builder-agent.md), [Phase 17 Builder 2.0](../18-builder-2-0/phase-17-builder-2-0.md) | `partial`: текущий executable slice остается draft-first; Phase 17 docs определяют более богатые authoring boundaries для memory, runtime, interaction и переносимой graph structure `orchestrator_agent`; managed-subagent task-package и review-loop details остаются вне portable Agent JSON | `implemented` для текущего draft-only slice; более богатому Builder 2.0 authoring нужна focused validation, прежде чем его можно поднять выше partial | `partial`: Phase 18 integrated-flow proof не заявляется | Phase 17 для завершения builder authoring; Phase 18 для integrated product-flow proof |
| Architecture safety gates и static boundary check | [Product Roadmap And Safety Gates](../product-roadmap-and-safety-gates.md), [Stage Safety Gates](../11-hardening/stage-safety-gates.md), [Runtime Integration Model](../02-architecture/runtime-integration-model.md) | `partial`: stage gates задокументированы, и существует static import-boundary checker с явными текущими exceptions, но checker является guardrail, а не proof того, что вся architecture cleanup завершена | `implemented` для checker behavior, stale exception detection, forbidden core/interface edges и concrete-technology import rules | `not_started`: эта reconciliation не заявляет CI или live operational proof | Continuous safety gates; existing exceptions требуют future cleanup или recorded acceptance |
| App facade boundary для local product use cases | [Product Roadmap And Safety Gates](../product-roadmap-and-safety-gates.md), [Graph Execution](../04-execution/graph-execution.md), [Agent Registry](../07-lifecycle/agent-registry.md) | `partial`: `src/app` теперь предоставляет local run и lifecycle use-case seams, используемые CLI, но это internal architecture seam, а не daemon, hosted app server, runtime host или core-process implementation | `implemented` для focused local run и lifecycle delegation paths | `partial`: переиспользует существующий local CLI/core proof; отдельный live proof для `src/app` как external surface не заявляется | Future app/server или hosted surface work должны быть отдельно scoped и proven |
| Direct local `run` app use case и CLI delegation | [Graph Execution](../04-execution/graph-execution.md), [Runtime Integration Model](../02-architecture/runtime-integration-model.md), [Stable CLI/API Contract Freeze](../21-public-launch-readiness/stable-cli-api-contract-freeze.md) | `implemented` для loading, revision resolution, state-store creation, adapter delegation и CLI handoff через app use case; новые runtime semantics сверх существующего core runner не заявляются | `implemented` за счет focused delegation и failure close-path coverage | `partial`: existing live graph proof по-прежнему поддерживает underlying local run path, но эта reconciliation не добавляет новый live run proof специально для app facade | Оставить в completed 5-7 для core behavior; Phase 19 владеет broader proof |
| App Server-backed Codex execution | [Runtime Integration Model](../02-architecture/runtime-integration-model.md), [Runtime Adapter Contract](../03-contracts/runtime-adapter-contract.md) | `implemented` | `implemented` | `implemented` на поддерживаемых моделях | Phase 14 для richer native surface |
| Model discovery, richer model metadata, speed tiers, reasoning effort, rate-limit/account/config introspection | [Runtime Integration Model](../02-architecture/runtime-integration-model.md), [Runtime Adapter Contract](../03-contracts/runtime-adapter-contract.md), [Phase 14 Native Runtime Surface Completion](../15-native-runtime-surface/phase-14-native-runtime-surface-completion.md) | `implemented` для normalized local runtime surface | `implemented` | `partial`: built CLI live proof теперь существует для model discovery и runtime-environment introspection; richer runtime-option controls все еще в основном опираются на focused automated validation | Оставить как implemented; следующие фазы расширяют глубину proof |
| Live comments во время run | [Live Run Interaction](../06-interaction/live-run-interaction.md) | `implemented` для Codex App Server path | `implemented` | `partial` | Phase 15 для более широкого interaction proof |
| Built-in user chat MCP | [Interaction and Chat Contract](../03-contracts/agent-json/interaction-and-chat-contract.md), [Orchestrator User Chat MCP Contract](../03-contracts/orchestrator-user-chat-mcp-contract.md), [Phase 15 Full User Interaction Layer](../16-full-user-interaction-layer/phase-15-full-user-interaction-layer.md) | `implemented`: в core/state/CLI уже есть durable `waiting_for_user` / `pending_prompt` механика, а Codex adapter и CLI теперь обеспечивают честный reply/resume flow на поддерживаемом Codex adapter path | `implemented` | `partial`: покрыто focused adapter и graph-runner tests; broader external live proof в этой freeze не заявляется | Phase 15 |
| Top-level и node-level skills, plugins и MCP bindings во время исполнения | [Top-Level and Bindings Contract](../03-contracts/agent-json/top-level-and-bindings-contract.md) | `documented_only` для executable slice; текущий runner честно делает fail-fast | `partial` за счет validation/fail-fast coverage | `not_started` | Phase 14 и Phase 18 |
| Portable memory-binding contract | [Memory Binding Model Contract](../03-contracts/agent-json/memory-binding-model-contract.md), [Memory Bindings](../08-extensions/memory-bindings.md) | `implemented` на уровне schema/doc/runtime-validation и уже потребляется Core memory resolution | `implemented` | `implemented` через binding-driven provider resolution из Phase 13 | Оставить как implemented; следующие фазы только расширяют runtime-native usage |
| Реальная интеграция с внешним memory provider | [Memory Bindings](../08-extensions/memory-bindings.md), [Phase 13 Mem0-First Native Memory Integration](../14-native-memory-integration/phase-13-mem0-first-native-memory-integration.md) | `partial` с реальным provider-backed product slice | `partial` с targeted registry/adapter/service coverage | `partial` с live proof только для Mem0-first | Следующие фазы добавляют других provider-ов и runtime-native consumption |
| Mem0-first provider staging | [Memory Bindings](../08-extensions/memory-bindings.md), [Phase 13 Mem0-First Native Memory Integration](../14-native-memory-integration/phase-13-mem0-first-native-memory-integration.md), этот документ | `implemented` for direct local provider CRUD/search and verified scoped namespace cleanup | `implemented` | `implemented` for local Mem0 CRUD/search plus TASK-357 target-cleanup/control-survival proof | Оставить как completed for bounded local Mem0 path; следующие фазы расширяют provider coverage и не должны выводить true restore, graph-store cleanup, provider-wide cleanup или reliability из этой строки |
| Runtime sources, limits и source selection/introspection | [Runtime Sources](../08-extensions/runtime-sources.md), [Runtime Adapter Contract](../03-contracts/runtime-adapter-contract.md), [Phase 14 Native Runtime Surface Completion](../15-native-runtime-surface/phase-14-native-runtime-surface-completion.md) | `partial`: source selection существует, model/env introspection теперь существует, per-source introspection остается unsupported в текущем Codex adapter | `partial` | `partial`: global runtime-model и runtime-environment proof существует, но source-specific introspection все еще unsupported | Phase 14 и последующее interaction/runtime-source hardening |
| Managed subagent orchestration с ролями, бюджетами, write scopes и review loops | [Subagent Orchestration Model](../02-architecture/subagent-orchestration-model.md), [Subagent MCP Contract](../03-contracts/subagent-mcp-contract.md), [Subagent Task Lifecycle](../04-execution/subagent-task-lifecycle.md), [Phase 16 Managed Subagent Orchestration](../17-managed-subagent-orchestration/phase-16-managed-subagent-orchestration.md) | `implemented` для bounded local managed-subagent layer: task-package snapshots с `read_context`, `required_validations` и `interaction_policy: "silent"`; роли worker/reviewer/explorer/integrator/final-review; `launch` / `status` / `wait` / `send` / `cancel` / `close`; durable status projection, review/fix workflow state, state-level cancellation без заявления live runtime cancellation, durable findings, budgets и sibling `write_set` conflict rejection, оставаясь отдельными от plain `orchestrator_agent` recursion | `implemented` для accepted local service/state/CLI slice за счет targeted subagent service и CLI coverage, включая review-loop и budget-exhaustion paths | `partial`: bounded local proof automated and accepted; broader external, cross-process, live runtime cancellation/status-probe, hosted operator-platform или public product proof не заявляются | Stage 18/19 и future core-process/runtime-host work владеют daemon/background runners, live delivery/probes/cancellation, cross-process attachment, broader operator readiness и external proof |
| Integrated product flows через lifecycle, builder, runtime features, interaction, memory и subagents | [Phase 18 Integrated Product Flows](../19-integrated-product-flows/README.md) плюс subsystem owner-docs, на которые он ссылается | `partial`: Phase 18 определяет cross-subsystem flow boundaries и теперь имеет local/offline executable slice поверх существующих product surfaces; новый external-runtime product code здесь не заявляется | `implemented` для local/offline automated evidence через `tests/integration/phase18-integrated-product-flows.test.ts`, который локально проходит | `not_started`: Phase 19 по-прежнему владеет live external proof, stress proof и release-readiness evidence | Phase 19 для real-world external proof, stress proof и release readiness |
| CLI/source checkout readiness и public launch boundary | [Stable CLI/API Contract Freeze](../21-public-launch-readiness/stable-cli-api-contract-freeze.md), [Public Docs, Onboarding, And Claims](../21-public-launch-readiness/public-docs-onboarding-and-claims.md), [Final Public Launch Gate Decision](../21-public-launch-readiness/final-public-launch-gate-decision.md) | `partial`: source-checkout CLI invocation остается поддерживаемым local path, а public launch, public npm/package publication, hosted deployment и production claims остаются blocked их owner-docs | `partial`: существующие local/package-readiness gates и docs поддерживают bounded local use; эта reconciliation не rerun и не расширяет public-launch gates | `partial`: только bounded local checkout и local package-readiness evidence; current public launch approval не заявляется | Public launch readiness owners должны заменить blockers durable evidence перед broader claims |
| Real-world release proof beyond local и mocked slices | [Phase 19 Real-World Proof And Release](../20-real-world-proof-and-release/README.md), [Release Scope Lock](../20-real-world-proof-and-release/release-scope-lock.md), [Operational Readiness](../11-hardening/operational-readiness.md), [Release Gates](../11-hardening/release-gates.md) | `not_started`: для Phase 19 proof не заявляется новый product code, и local/offline evidence само по себе не дает broad product release-ready состояния | `implemented` для bounded local CLI/repository target: repository gates, Phase 18 local/offline integration evidence, TASK-287 live graph smoke, TASK-290 stress/regression evidence, TASK-291 local operational/recovery/cleanup evidence, TASK-292 bilingual documentation cleanup, TASK-454 deterministic local Stage 8 rerun и TASK-495 final candidate gates поддерживают только `local-cli-repository-readiness` на commit `c3ad3eafca28f4a602a6e44d1861054aabc96a03` | `implemented` только для bounded target: TASK-280 доказал runtime discovery и direct local Mem0 provider operations; TASK-287 доказал minimal live Codex graph execution; TASK-454 подтверждает deterministic local stress/recovery/regression proof; TASK-495 passed final local release-candidate gates; финальное release decision теперь bounded `release` для зафиксированной local CLI/repository цели | [Phase 19](../20-real-world-proof-and-release/README.md); included и deferred capabilities зафиксированы в [Release Scope Lock](../20-real-world-proof-and-release/release-scope-lock.md); broader release claims остаются deferred в [release decision record](../20-real-world-proof-and-release/release-decision-record.md), включая hosted/managed deployment, npm/public package publication, installer/container distribution, durable external provider cleanup, external-provider reliability, live provider stress, production-scale load, broad runtime-memory/provider support, native App Server memory, full App Server certification, full user interaction layer, operator-platform managed subagent readiness и public Builder 2.0 readiness |

## 2026-04-29 Сверка App Facade и Architecture Gate

Эта reconciliation фиксирует текущие repository evidence после app-facade и architecture-gate slices.

- `src/app` является internal application/use-case seam для local CLI operations. Это не daemon, hosted App Server, replacement core process, external runtime host или public plugin surface.
- Direct local `run` и lifecycle command delegation поддержаны code-and-test на facade boundary, но намеренно переиспользуют existing core runner, lifecycle service, local state store и runtime adapter contracts.
- Architecture boundary checker является safety gate с явными current exceptions. Он предотвращает новые undocumented boundary violations, но не стирает и не завершает existing exceptions.
- CLI/source-checkout readiness остается bounded to local checkout и local package-readiness claims. Public launch, public npm publication, installer/container distribution, hosted/managed deployment и production readiness остаются blocked, пока их public-launch owner documents не будут обновлены durable evidence.
- Live proof claims не меняются, кроме мест, где matrix уже называет existing bounded local proof. Facade unit tests сами по себе не создают новый external runtime, provider, operator-facing или public-launch proof.

## Зафиксированный Статус Mem0-First

Mem0 — это первый внешний memory target для Dennett.

Phase 12 фиксирует следующие факты:

### Что уже верно

- Mem0 source staged локально, а Python package установлен в local sandbox.
- В product code Dennett теперь есть реальный `Mem0MemoryAdapter`.
- В Dennett теперь есть local provider registration, capability negotiation и direct provider-backed memory execution через Core и CLI.
- В репозитории теперь есть Mem0-backed tests для adapter и memory service slice.
- Реальный local Mem0 round-trip доказан через test suite и built CLI path.

### Что все еще не верно

- Текущий Codex `runtime_agent` execution path еще не потребляет memory bindings нативно во время model execution.
- Provider families кроме Mem0 все еще не реализованы.
- MCP-backed memory transport остается future work, а не завершенной product lane.
- Cross-agent memory propagation rules остаются нереализованными.

### Следствие для матрицы

Честный статус для Mem0-first integration больше не `runtime_blocked`.

Оставшийся gap теперь уже:

- Mem0-first native memory integration реализована;
- general multi-provider memory support остается partial;
- runtime-native memory use внутри текущего Codex execution остается later-phase work.

## Критерии Приемки Phase 12

Phase 12 считается завершенной только если выполнено все:

1. канонизированы status labels;
2. матрица возможностей существует и связана с деревом docs;
3. AGENTS.md канонически определяет roadmap после этапа 11;
4. текущий Mem0-first staging status зафиксирован без притворства, что это уже готовая product feature;
5. следующий этап можно начинать из этой матрицы, не открывая заново discovery scope.

## Чего Phase 12 Явно Не Делает

Phase 12 не:

- реализует внешнюю память в product code;
- заявляет live proof для памяти там, где есть только package staging;
- завершает richer App Server feature surface;
- реализует managed subagents;
- улучшает builder сверх текущего slice;
- превращает проект в другой продукт.

Это этап заморозки и передачи, а не скрытый этап реализации.

## Следующие фазы после Phase 12

Следующие канонические фазы раньше были:

- **Phase 13: Native Memory Integration (Mem0 First)**
- **Phase 14: Native Runtime Surface Completion**

Эти фазы теперь реализованы и записаны в:

- [Phase 13 Mem0-First Native Memory Integration](../14-native-memory-integration/phase-13-mem0-first-native-memory-integration.md)
- [Phase 14 Native Runtime Surface Completion](../15-native-runtime-surface/phase-14-native-runtime-surface-completion.md)
