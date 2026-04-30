[English](#english) | [Russian](#russian)

<a id="english"></a>
# Acceptance Scenarios

Status: Phase 18 acceptance scenarios for integrated product-flow evidence.

These scenarios define the minimum product-flow coverage expected for Phase 18. They are intentionally evidence-focused and do not claim external release proof.

## Evidence Labels

- Local executable: a test or command runs locally against implemented code.
- Test double: a fake provider, fake runtime, or fixture replaces an external dependency while preserving the contract boundary.
- Dry run: the product validates, plans, or simulates a flow without claiming live external execution.
- Phase 19 required: the scenario needs real runtime, real provider, scale, or operational evidence before it can support release readiness.

## Scenario 1: Builder Draft To Live Run

Goal: prove that a builder-authored candidate can pass through validation, lifecycle persistence, deploy, runtime selection, and execution without bypassing owner gates.

Flow:

1. Builder produces portable Agent JSON that references supported runtime options and no private builder fields.
2. Core validates schema and invariants.
3. Lifecycle persists the candidate as a draft.
4. Deploy promotes the draft through the documented lifecycle path.
5. Execution resolves the live revision and selects a runtime through capability metadata.
6. The run records output or a runtime-owned capability failure.

Acceptance:

- draft and live identity are distinct and traceable;
- builder does not mutate live state directly;
- runtime options are accepted only if advertised by the selected runtime;
- failure identifies the owner gate that blocked the flow.

Evidence: local executable or dry run. Phase 19 is required for real external runtime proof.

Executable mapping:

- `tests/integration/phase18-integrated-product-flows.test.ts` -> `builds, deploys, prompts, resumes, delegates, and continues through managed subagents`
- `tests/integration/stage7-cli-integrated-flow.test.ts` -> `builds, registers, deploys, waits for user input, replies, and resumes offline`

## Scenario 2: Memory-Aware Run With Safe Provider Boundary

Goal: prove that memory bindings participate in execution without making provider readiness part of portable Agent JSON.

Flow:

1. A live agent references documented `memory_bindings`.
2. The product validates binding shape and scope.
3. Provider registration and capability negotiation occur outside the agent file.
4. The run performs authorized read, write, or search behavior through the memory port.
5. Missing provider readiness fails with a memory-owned error.

Acceptance:

- credentials and provider setup are absent from portable Agent JSON;
- unauthorized scope is rejected;
- fake-provider success is labeled as Phase 18 evidence only;
- real-provider readiness remains Phase 19 unless backed by live proof.

Evidence: local executable with a real local provider where available, or test double. Phase 19 is required for external provider reliability proof.

Executable mapping:

- `tests/integration/phase18-integrated-product-flows.test.ts` -> `builds, deploys, prompts, resumes, delegates, and continues through managed subagents`
- `tests/integration/phase18-integrated-product-flows.test.ts` -> `fails an unregistered runtime memory provider at the memory-owned gate before runtime launch without mutating lifecycle state`

## Scenario 3: User Interaction During Runtime Work

Goal: prove that comments, blocked prompts, replies, wait state, and resume behavior stay coherent while runtime work is pending.

Flow:

1. A live run reaches a documented prompt or wait state.
2. The durable interaction state records the blocked prompt.
3. A user reply is accepted only while the prompt remains valid.
4. Runtime work resumes through the documented resume path.
5. Late, duplicate, or superseded replies are rejected or routed according to interaction policy.

Acceptance:

- resume requires the durable reply;
- runtime execution cannot bypass the blocked prompt;
- risky mid-run parameter changes follow the interaction policy;
- errors name the interaction owner when reply state is invalid.

Evidence: local executable or test double. Phase 19 is required for cross-interface live proof.

Executable mapping:

- `tests/integration/phase18-integrated-product-flows.test.ts` -> `builds, deploys, prompts, resumes, delegates, and continues through managed subagents`
- `tests/integration/stage7-cli-integrated-flow.test.ts` -> `builds, registers, deploys, waits for user input, replies, and resumes offline`
- `tests/integration/stage7-interaction-edge-cases.test.ts` -> `rejects late replies after the prompt run has completed`
- `tests/integration/stage7-interaction-edge-cases.test.ts` -> `records duplicate prompt replies append-only until resume consumes the latest match`
- `tests/integration/stage7-interaction-edge-cases.test.ts` -> `uses the newest matching reply when a pending prompt is superseded before resume`
- `tests/integration/stage7-interaction-edge-cases.test.ts` -> `defers risky mid-run model changes by rejecting changed revision resumes`

## Scenario 4: Managed Subagent Review Flow

Goal: prove that managed subagent coordination can participate in a product flow without becoming portable Agent JSON.

Flow:

1. A parent run launches a managed worker with a task package and write scope.
2. A separate managed reviewer evaluates the worker result.
3. Valid required feedback routes to a fix worker before closure.
4. The parent explicitly closes each child boundary.
5. Findings, terminal state, budgets, lineage, and close disposition remain durable managed-subagent state.

Acceptance:

- sibling write-scope conflicts are rejected before launch;
- budgets and cancellation are enforced by the managed subagent owner;
- review/fix state is not serialized into portable Agent JSON;
- parent output distinguishes child terminal result from explicit close.

Evidence: local executable or test double. Phase 19 is required for broad external managed-subagent live proof.

Executable mapping:

- `tests/integration/phase18-integrated-product-flows.test.ts` -> `runs managed worker, reviewer, fix, and re-review through the managed subagent service boundary`
- `tests/unit/subagent-service.test.ts` -> `rejects a sibling managed subagent with an overlapping write_set before child start`
- `tests/unit/subagent-service.test.ts` -> `rejects a second sibling launch when max_children is exhausted`
- `tests/unit/subagent-service.test.ts` -> `returns reviewer findings and enforces the review-loop ceiling`
- `tests/unit/subagent-service.test.ts` -> `accepts bounded control messages and honors cancelled_by_parent close semantics`
- `tests/unit/subagent-service.test.ts` -> `launches, waits, and closes a worker-role managed subagent without touching plain orchestrator_agent behavior`

## Scenario 5: Combined Negative Capability Case

Goal: prove that a multi-subsystem flow fails at the correct owner gate when several unavailable capabilities are present.

Flow:

1. A draft requests unsupported runtime options and a memory provider that is not registered.
2. Schema validation passes because the fields are syntactically valid.
3. Lifecycle can persist the draft but deploy or execution must run capability gates.
4. The product reports the earliest violated gate using the conflict rules.

Acceptance:

- syntactic validity is not treated as capability readiness;
- runtime and memory errors are not collapsed into a generic execution failure;
- durable lifecycle state remains consistent after the failed attempt;
- the error message points to the subsystem that owns the missing capability.

Evidence: local executable or dry run. Phase 19 is required for live runtime/provider capability proof.

Executable mapping:

- `tests/integration/phase18-integrated-product-flows.test.ts` -> `fails unsupported runtime options at the runtime-owned gate before run creation without mutating lifecycle state`
- `tests/integration/phase18-integrated-product-flows.test.ts` -> `fails an unregistered runtime memory provider at the memory-owned gate before runtime launch without mutating lifecycle state`

## Scenario 6: Integrated Builder, Interaction, Memory, And Subagent Flow

Goal: prove that the major surfaces have executable handoff coverage and can be composed without hidden cross-owner state.

Flow:

1. Builder drafts an agent that references runtime options, memory bindings, user interaction, and a portable nested-graph pattern.
2. Lifecycle validates and deploys the draft.
3. A live run uses runtime capability metadata and memory capability negotiation.
4. The run reaches a user prompt and resumes after a durable reply.
5. The run launches managed worker and reviewer children for a bounded task.
6. The parent closes child boundaries and produces a final output with traceable evidence.

Acceptance:

- each subsystem owner has a clear handoff point;
- no subsystem stores hidden state in another subsystem's contract surface;
- conflict rules define the result of missing capabilities, blocked prompts, and write-scope collisions;
- the scenario records which evidence is local and which remains Phase 19.

Evidence: local executable with test doubles allowed. Phase 19 is required for real provider, real runtime, stress, and release proof.

Executable mapping:

- `tests/integration/phase18-integrated-product-flows.test.ts` -> `builds, deploys, prompts, resumes, delegates, and continues through managed subagents`
- `tests/integration/phase18-integrated-product-flows.test.ts` -> `runs managed worker, reviewer, fix, and re-review through the managed subagent service boundary`
- `tests/integration/phase18-integrated-product-flows.test.ts` -> `fails unsupported runtime options at the runtime-owned gate before run creation without mutating lifecycle state`
- `tests/integration/phase18-integrated-product-flows.test.ts` -> `fails an unregistered runtime memory provider at the memory-owned gate before runtime launch without mutating lifecycle state`
- `tests/integration/stage7-cli-integrated-flow.test.ts` -> `builds, registers, deploys, waits for user input, replies, and resumes offline`
- No single Stage 7 executable combines every product surface with real provider/runtime stress, concurrency, cancellation, and release-readiness evidence; that complete proof remains deferred to Phase 19.

<a id="russian"></a>
# Приемочные сценарии

Статус: приемочные сценарии Phase 18 для доказательств integrated product-flow.

Эти сценарии определяют минимальное покрытие product-flow, ожидаемое для Phase 18. Они намеренно сфокусированы на evidence и не заявляют external release proof.

## Метки evidence

- Local executable: test или command запускается локально против реализованного code.
- Test double: fake provider, fake runtime или fixture заменяет внешнюю зависимость, сохраняя contract boundary.
- Dry run: продукт validates, plans или simulates поток без заявления live external execution.
- Phase 19 required: сценарию нужны real runtime, real provider, scale или operational evidence, прежде чем он сможет поддерживать release readiness.

## Сценарий 1: Builder Draft To Live Run

Цель: доказать, что builder-authored candidate может пройти validation, lifecycle persistence, deploy, runtime selection и execution без обхода owner gates.

Поток:

1. Builder produces portable Agent JSON, который ссылается на supported runtime options и не содержит private builder fields.
2. Core validates schema and invariants.
3. Lifecycle persists candidate as a draft.
4. Deploy promotes draft through the documented lifecycle path.
5. Execution resolves live revision и selects runtime through capability metadata.
6. Run records output или runtime-owned capability failure.

Приемка:

- draft and live identity distinct and traceable;
- builder не mutates live state directly;
- runtime options accepted only if advertised by selected runtime;
- failure identifies the owner gate that blocked the flow.

Evidence: local executable или dry run. Phase 19 требуется для real external runtime proof.

Executable mapping:

- `tests/integration/phase18-integrated-product-flows.test.ts` -> `builds, deploys, prompts, resumes, delegates, and continues through managed subagents`
- `tests/integration/stage7-cli-integrated-flow.test.ts` -> `builds, registers, deploys, waits for user input, replies, and resumes offline`

## Сценарий 2: Memory-Aware Run With Safe Provider Boundary

Цель: доказать, что memory bindings участвуют в execution, не делая provider readiness частью portable Agent JSON.

Поток:

1. Live agent references documented `memory_bindings`.
2. Product validates binding shape and scope.
3. Provider registration and capability negotiation occur outside the agent file.
4. Run performs authorized read, write, or search behavior through the memory port.
5. Missing provider readiness fails with a memory-owned error.

Приемка:

- credentials and provider setup absent from portable Agent JSON;
- unauthorized scope rejected;
- fake-provider success labeled as Phase 18 evidence only;
- real-provider readiness remains Phase 19 unless backed by live proof.

Evidence: local executable with a real local provider where available или test double. Phase 19 требуется для external provider reliability proof.

Executable mapping:

- `tests/integration/phase18-integrated-product-flows.test.ts` -> `builds, deploys, prompts, resumes, delegates, and continues through managed subagents`
- `tests/integration/phase18-integrated-product-flows.test.ts` -> `fails an unregistered runtime memory provider at the memory-owned gate before runtime launch without mutating lifecycle state`

## Сценарий 3: User Interaction During Runtime Work

Цель: доказать, что comments, blocked prompts, replies, wait state и resume behavior остаются согласованными, пока runtime work ожидает завершения.

Поток:

1. Live run reaches documented prompt or wait state.
2. Durable interaction state records the blocked prompt.
3. User reply accepted only while the prompt remains valid.
4. Runtime work resumes through documented resume path.
5. Late, duplicate, or superseded replies rejected or routed according to interaction policy.

Приемка:

- resume requires the durable reply;
- runtime execution cannot bypass the blocked prompt;
- risky mid-run parameter changes follow the interaction policy;
- errors name the interaction owner when reply state is invalid.

Evidence: local executable или test double. Phase 19 требуется для cross-interface live proof.

Executable mapping:

- `tests/integration/phase18-integrated-product-flows.test.ts` -> `builds, deploys, prompts, resumes, delegates, and continues through managed subagents`
- `tests/integration/stage7-cli-integrated-flow.test.ts` -> `builds, registers, deploys, waits for user input, replies, and resumes offline`
- `tests/integration/stage7-interaction-edge-cases.test.ts` -> `rejects late replies after the prompt run has completed`
- `tests/integration/stage7-interaction-edge-cases.test.ts` -> `records duplicate prompt replies append-only until resume consumes the latest match`
- `tests/integration/stage7-interaction-edge-cases.test.ts` -> `uses the newest matching reply when a pending prompt is superseded before resume`
- `tests/integration/stage7-interaction-edge-cases.test.ts` -> `defers risky mid-run model changes by rejecting changed revision resumes`

## Сценарий 4: Managed Subagent Review Flow

Цель: доказать, что managed subagent coordination может участвовать в product flow, не становясь portable Agent JSON.

Поток:

1. Parent run launches managed worker with a task package and write scope.
2. Separate managed reviewer evaluates worker result.
3. Valid required feedback routes to a fix worker before closure.
4. Parent explicitly closes each child boundary.
5. Findings, terminal state, budgets, lineage, and close disposition remain durable managed-subagent state.

Приемка:

- sibling write-scope conflicts rejected before launch;
- budgets and cancellation enforced by managed subagent owner;
- review/fix state not serialized into portable Agent JSON;
- parent output distinguishes child terminal result from explicit close.

Evidence: local executable или test double. Phase 19 требуется для broad external managed-subagent live proof.

Executable mapping:

- `tests/integration/phase18-integrated-product-flows.test.ts` -> `runs managed worker, reviewer, fix, and re-review through the managed subagent service boundary`
- `tests/unit/subagent-service.test.ts` -> `rejects a sibling managed subagent with an overlapping write_set before child start`
- `tests/unit/subagent-service.test.ts` -> `rejects a second sibling launch when max_children is exhausted`
- `tests/unit/subagent-service.test.ts` -> `returns reviewer findings and enforces the review-loop ceiling`
- `tests/unit/subagent-service.test.ts` -> `accepts bounded control messages and honors cancelled_by_parent close semantics`
- `tests/unit/subagent-service.test.ts` -> `launches, waits, and closes a worker-role managed subagent without touching plain orchestrator_agent behavior`

## Сценарий 5: Combined Negative Capability Case

Цель: доказать, что multi-subsystem flow fails at the correct owner gate, когда присутствуют несколько unavailable capabilities.

Поток:

1. Draft requests unsupported runtime options and a memory provider that is not registered.
2. Schema validation passes because the fields are syntactically valid.
3. Lifecycle can persist the draft, but deploy or execution must run capability gates.
4. Product reports earliest violated gate using conflict rules.

Приемка:

- syntactic validity not treated as capability readiness;
- runtime and memory errors not collapsed into generic execution failure;
- durable lifecycle state remains consistent after failed attempt;
- error message points to subsystem that owns the missing capability.

Evidence: local executable или dry run. Phase 19 требуется для live runtime/provider capability proof.

Executable mapping:

- `tests/integration/phase18-integrated-product-flows.test.ts` -> `fails unsupported runtime options at the runtime-owned gate before run creation without mutating lifecycle state`
- `tests/integration/phase18-integrated-product-flows.test.ts` -> `fails an unregistered runtime memory provider at the memory-owned gate before runtime launch without mutating lifecycle state`

## Сценарий 6: Integrated Builder, Interaction, Memory, And Subagent Flow

Цель: доказать, что основные surfaces имеют executable handoff coverage и могут быть объединены без hidden cross-owner state.

Поток:

1. Builder drafts an agent that references runtime options, memory bindings, user interaction, and a portable nested-graph pattern.
2. Lifecycle validates and deploys the draft.
3. Live run uses runtime capability metadata and memory capability negotiation.
4. Run reaches a user prompt and resumes after a durable reply.
5. Run launches managed worker and reviewer children for a bounded task.
6. Parent closes child boundaries and produces a final output with traceable evidence.

Приемка:

- each subsystem owner has a clear handoff point;
- no subsystem stores hidden state in another subsystem's contract surface;
- conflict rules define result of missing capabilities, blocked prompts, and write-scope collisions;
- scenario records which evidence is local and which remains Phase 19.

Evidence: local executable with test doubles allowed. Phase 19 требуется для real provider, real runtime, stress, and release proof.

Executable mapping:

- `tests/integration/phase18-integrated-product-flows.test.ts` -> `builds, deploys, prompts, resumes, delegates, and continues through managed subagents`
- `tests/integration/phase18-integrated-product-flows.test.ts` -> `runs managed worker, reviewer, fix, and re-review through the managed subagent service boundary`
- `tests/integration/phase18-integrated-product-flows.test.ts` -> `fails unsupported runtime options at the runtime-owned gate before run creation without mutating lifecycle state`
- `tests/integration/phase18-integrated-product-flows.test.ts` -> `fails an unregistered runtime memory provider at the memory-owned gate before runtime launch without mutating lifecycle state`
- `tests/integration/stage7-cli-integrated-flow.test.ts` -> `builds, registers, deploys, waits for user input, replies, and resumes offline`
- Ни один Stage 7 executable не объединяет все product surfaces с real provider/runtime stress, concurrency, cancellation и release-readiness evidence; такой полный proof остается deferred to Phase 19.
