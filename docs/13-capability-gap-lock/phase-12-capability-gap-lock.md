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
| Builder draft authoring and Builder 2.0 public-contract-aware authoring | [Builder Agent](../08-extensions/builder-agent.md), [Phase 17 Builder 2.0](../18-builder-2-0/phase-17-builder-2-0.md) | `partial`: the current executable slice remains draft-first; Phase 17 docs define richer authoring boundaries for memory, runtime, interaction, and portable `orchestrator_agent` graph structure; managed-subagent task-package and review-loop details remain outside portable agent JSON | `implemented` for current draft-only slice; richer Builder 2.0 authoring needs focused validation before it can be raised beyond partial | `partial`: no Phase 18 integrated-flow proof is claimed | Phase 17 for builder authoring completion; Phase 18 for integrated product-flow proof |
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
| Managed subagent orchestration with roles, budgets, write scopes, and review loops | [Subagent Orchestration Model](../02-architecture/subagent-orchestration-model.md), [Subagent MCP Contract](../03-contracts/subagent-mcp-contract.md), [Subagent Task Lifecycle](../04-execution/subagent-task-lifecycle.md), [Phase 16 Managed Subagent Orchestration](../17-managed-subagent-orchestration/phase-16-managed-subagent-orchestration.md) | `partial`: managed subagents now cover worker/reviewer/final-review roles, `launch` / `wait` / `send` / `close`, durable findings and cancellation state, and sibling `write_set` conflict rejection while staying distinct from plain `orchestrator_agent` recursion | `partial`: targeted state and service coverage exists for the implemented contract-completion slice | `not_started`: no broader external or operator-facing proof path is claimed yet | Phase 16 for broader live proof and any remaining context-inheritance surface |
| Integrated product flows across lifecycle, builder, runtime features, interaction, memory, and subagents | [Phase 18 Integrated Product Flows](../19-integrated-product-flows/README.md) plus the subsystem owner docs it references | `partial`: Phase 18 defines cross-subsystem flow boundaries and now has a local/offline executable slice over the existing product surfaces; no new external-runtime product code is claimed here | `implemented` for local/offline automated evidence through `tests/integration/phase18-integrated-product-flows.test.ts`, which passes locally | `not_started`: Phase 19 still owns live external proof, stress proof, and release-readiness evidence | [Phase 19](../20-real-world-proof-and-release/README.md) for real-world external proof, stress proof, operational evidence, and release readiness |
| Real-world release proof beyond local and mocked slices | [Phase 19 Real-World Proof And Release](../20-real-world-proof-and-release/README.md), [Release Scope Lock](../20-real-world-proof-and-release/release-scope-lock.md), [Operational Readiness](../11-hardening/operational-readiness.md), [Release Gates](../11-hardening/release-gates.md) | `not_started`: no new product code is claimed for Phase 19 proof and no broad product release-ready state is claimed from local/offline evidence alone | `implemented` for the bounded local CLI/repository release target: current repository gates, Phase 18 local/offline integration evidence, TASK-287 live graph smoke, TASK-290 stress/regression evidence, TASK-291 local operational/recovery/cleanup evidence, TASK-292 bilingual documentation cleanup, TASK-454 deterministic local Stage 8 stress/recovery/regression gate rerun, and TASK-495 final candidate gates support only `local-cli-repository-readiness` on commit `c3ad3eafca28f4a602a6e44d1861054aabc96a03` | `implemented` for the bounded target only: TASK-280 proved runtime discovery and direct local Mem0 provider operations; TASK-287 proved minimal live Codex graph execution; TASK-454 confirms deterministic local-only stress/recovery/regression proof, including explicit retry/resume completion and exactly-once final output after crash/reopen, not live provider stress; TASK-495 passed final local release-candidate gates; the final release decision is bounded `release` for the locked local CLI/repository target | [Phase 19](../20-real-world-proof-and-release/README.md); included and deferred capabilities are locked by [Release Scope Lock](../20-real-world-proof-and-release/release-scope-lock.md); broader release claims remain deferred by the [release decision record](../20-real-world-proof-and-release/release-decision-record.md), including hosted/managed deployment, npm/public package publication, installer/container distribution, durable external provider cleanup, external-provider reliability, live provider stress, production-scale load, broad runtime-memory/provider support, native App Server memory, full App Server certification, full user interaction layer, operator-facing managed subagent readiness, and public Builder 2.0 readiness |

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

<a id="russian"></a>
Маршрутная заметка Phase 19: финальное решение теперь bounded `release` для суженного local CLI/repository scope на candidate commit `c3ad3eafca28f4a602a6e44d1861054aabc96a03`, а не broad product release. TASK-280 дал проходящие gates, доказательство runtime discovery и прямое доказательство локального Mem0 provider; TASK-287 добавил проходящий minimal live graph smoke; TASK-290, TASK-291 и TASK-292 поддерживают stress/regression, local operational/recovery/cleanup и bilingual documentation cleanup только для этого суженного scope; TASK-333 добавил узкий included proof for registered local Mem0 plus Codex prompt-rendered memory context, while TASK-334 superseded the initial default-gate failure with a passing `pnpm test` rerun; TASK-402 и TASK-495 supersede TASK-400 как current blocker для bounded release. Текущие доказательства все еще не поддерживают broader claims: hosted/managed deployment, npm/public package publication, installer/container distribution, production SaaS/readiness/load, live provider stress/reliability, broad runtime-memory/provider support, native App Server memory, full App Server certification, durable external provider cleanup, true restore / graph-store/provider-wide cleanup, public Builder 2.0 readiness, full user interaction layer и operator-facing managed subagent product readiness остаются deferred в [release decision record](../20-real-world-proof-and-release/release-decision-record.md). Для обеих языковых секций честный статус: код `not_started`, тесты/live proof `implemented` только для bounded local CLI/repository target, broader release readiness deferred.

# Phase 12 Capability Gap Lock

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
| Builder draft authoring and Builder 2.0 public-contract-aware authoring | [Builder Agent](../08-extensions/builder-agent.md), [Phase 17 Builder 2.0](../18-builder-2-0/phase-17-builder-2-0.md) | `partial`: текущий executable slice остается draft-first; Phase 17 docs определяют более богатые authoring boundaries для memory, runtime, interaction и переносимой graph structure `orchestrator_agent`; managed-subagent task-package и review-loop details остаются вне portable Agent JSON | `implemented` для текущего draft-only slice; более богатому Builder 2.0 authoring нужна focused validation, прежде чем его можно поднять выше partial | `partial`: Phase 18 integrated-flow proof не заявляется | Phase 17 для завершения builder authoring; Phase 18 для integrated product-flow proof |
| App Server-backed Codex execution | [Runtime Integration Model](../02-architecture/runtime-integration-model.md), [Runtime Adapter Contract](../03-contracts/runtime-adapter-contract.md) | `implemented` | `implemented` | `implemented` на поддерживаемых моделях | Phase 14 для richer native surface |
| Model discovery, richer model metadata, speed tiers, reasoning effort, rate-limit/account/config introspection | [Runtime Integration Model](../02-architecture/runtime-integration-model.md), [Runtime Adapter Contract](../03-contracts/runtime-adapter-contract.md), [Phase 14 Native Runtime Surface Completion](../15-native-runtime-surface/phase-14-native-runtime-surface-completion.md) | `implemented` для normalized local runtime surface | `implemented` | `partial`: built CLI live proof теперь существует для model discovery и runtime-environment introspection; richer runtime-option controls все еще в основном опираются на focused automated validation | Оставить как implemented; следующие фазы расширяют глубину proof |
| Live comments во время run | [Live Run Interaction](../06-interaction/live-run-interaction.md) | `implemented` для Codex App Server path | `implemented` | `partial` | Phase 15 для более широкого interaction proof |
| Built-in user chat MCP | [Interaction and Chat Contract](../03-contracts/agent-json/interaction-and-chat-contract.md), [Orchestrator User Chat MCP Contract](../03-contracts/orchestrator-user-chat-mcp-contract.md), [Phase 15 Full User Interaction Layer](../16-full-user-interaction-layer/phase-15-full-user-interaction-layer.md) | `implemented`: в core/state/CLI уже есть durable `waiting_for_user` / `pending_prompt` механика, а Codex adapter и CLI теперь обеспечивают честный reply/resume flow на поддерживаемом Codex adapter path | `implemented` | `partial`: покрыто focused adapter и graph-runner tests; broader external live proof в этой freeze не заявляется | Phase 15 |
| Top-level и node-level skills, plugins и MCP bindings во время исполнения | [Top-Level and Bindings Contract](../03-contracts/agent-json/top-level-and-bindings-contract.md) | `documented_only` для executable slice; текущий runner честно делает fail-fast | `partial` за счет validation/fail-fast coverage | `not_started` | Phase 14 и Phase 18 |
| Portable memory-binding contract | [Memory Binding Model Contract](../03-contracts/agent-json/memory-binding-model-contract.md), [Memory Bindings](../08-extensions/memory-bindings.md) | `implemented` на уровне schema/doc/runtime-validation и уже потребляется Core memory resolution | `implemented` | `implemented` через binding-driven provider resolution из Phase 13 | Оставить как implemented; следующие фазы только расширяют runtime-native usage |
| Реальная интеграция с внешним memory provider | [Memory Bindings](../08-extensions/memory-bindings.md), [Phase 13 Mem0-First Native Memory Integration](../14-native-memory-integration/phase-13-mem0-first-native-memory-integration.md) | `partial` с реальным provider-backed product slice | `partial` с targeted registry/adapter/service coverage | `partial` с live proof только для Mem0-first | Следующие фазы добавляют других provider-ов и runtime-native consumption |
| Mem0-first provider staging | [Memory Bindings](../08-extensions/memory-bindings.md), [Phase 13 Mem0-First Native Memory Integration](../14-native-memory-integration/phase-13-mem0-first-native-memory-integration.md), этот документ | `implemented` for direct local provider CRUD/search and verified scoped namespace cleanup | `implemented` | `implemented` for local Mem0 CRUD/search plus TASK-357 target-cleanup/control-survival proof | Оставить как completed for bounded local Mem0 path; следующие фазы расширяют provider coverage и не должны выводить true restore, graph-store cleanup, provider-wide cleanup или reliability из этой строки |
| Runtime sources, limits и source selection/introspection | [Runtime Sources](../08-extensions/runtime-sources.md), [Runtime Adapter Contract](../03-contracts/runtime-adapter-contract.md), [Phase 14 Native Runtime Surface Completion](../15-native-runtime-surface/phase-14-native-runtime-surface-completion.md) | `partial`: source selection существует, model/env introspection теперь существует, per-source introspection остается unsupported в текущем Codex adapter | `partial` | `partial`: global runtime-model и runtime-environment proof существует, но source-specific introspection все еще unsupported | Phase 14 и последующее interaction/runtime-source hardening |
| Managed subagent orchestration с ролями, бюджетами, write scopes и review loops | [Subagent Orchestration Model](../02-architecture/subagent-orchestration-model.md), [Subagent MCP Contract](../03-contracts/subagent-mcp-contract.md), [Subagent Task Lifecycle](../04-execution/subagent-task-lifecycle.md), [Phase 16 Managed Subagent Orchestration](../17-managed-subagent-orchestration/phase-16-managed-subagent-orchestration.md) | `partial`: managed subagents теперь покрывают роли worker/reviewer/final-review, `launch` / `wait` / `send` / `close`, durable findings и cancellation state, а также sibling `write_set` conflict rejection, оставаясь отдельными от plain `orchestrator_agent` recursion | `partial`: существует targeted state и service coverage для реализованного contract-completion slice | `not_started`: broader external или operator-facing proof path пока не заявляется | Phase 16 для broader live proof и любой оставшейся context-inheritance surface |
| Integrated product flows через lifecycle, builder, runtime features, interaction, memory и subagents | [Phase 18 Integrated Product Flows](../19-integrated-product-flows/README.md) плюс subsystem owner-docs, на которые он ссылается | `partial`: Phase 18 определяет cross-subsystem flow boundaries и теперь имеет local/offline executable slice поверх существующих product surfaces; новый external-runtime product code здесь не заявляется | `implemented` для local/offline automated evidence через `tests/integration/phase18-integrated-product-flows.test.ts`, который локально проходит | `not_started`: Phase 19 по-прежнему владеет live external proof, stress proof и release-readiness evidence | Phase 19 для real-world external proof, stress proof и release readiness |
| Real-world release proof beyond local и mocked slices | [Phase 19 Real-World Proof And Release](../20-real-world-proof-and-release/README.md), [Release Scope Lock](../20-real-world-proof-and-release/release-scope-lock.md), [Operational Readiness](../11-hardening/operational-readiness.md), [Release Gates](../11-hardening/release-gates.md) | `not_started`: для Phase 19 proof не заявляется новый product code, и local/offline evidence само по себе не дает broad product release-ready состояния | `implemented` для bounded local CLI/repository target: repository gates, Phase 18 local/offline integration evidence, TASK-287 live graph smoke, TASK-290 stress/regression evidence, TASK-291 local operational/recovery/cleanup evidence, TASK-292 bilingual documentation cleanup, TASK-454 deterministic local Stage 8 rerun и TASK-495 final candidate gates поддерживают только `local-cli-repository-readiness` на commit `c3ad3eafca28f4a602a6e44d1861054aabc96a03` | `implemented` только для bounded target: TASK-280 доказал runtime discovery и direct local Mem0 provider operations; TASK-287 доказал minimal live Codex graph execution; TASK-454 подтверждает deterministic local stress/recovery/regression proof; TASK-495 passed final local release-candidate gates; финальное release decision теперь bounded `release` для зафиксированной local CLI/repository цели | [Phase 19](../20-real-world-proof-and-release/README.md); included и deferred capabilities зафиксированы в [Release Scope Lock](../20-real-world-proof-and-release/release-scope-lock.md); broader release claims остаются deferred в [release decision record](../20-real-world-proof-and-release/release-decision-record.md), включая hosted/managed deployment, npm/public package publication, installer/container distribution, durable external provider cleanup, external-provider reliability, live provider stress, production-scale load, broad runtime-memory/provider support, native App Server memory, full App Server certification, full user interaction layer, operator-facing managed subagent readiness и public Builder 2.0 readiness |

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
