[English](#english) | [Russian](#russian)

<a id="english"></a>
# Memory Bindings

Status: normative owner for the memory-binding extension.

Related documents:

- [Canonical spec](../../agent_orchestrator_final_spec_v2.md)
- [Agent JSON contract](../03-contracts/agent-json/README.md)
- [Memory Binding Model Contract](../03-contracts/agent-json/memory-binding-model-contract.md)
- [Runtime Integration Model](../02-architecture/runtime-integration-model.md)
- [State](../05-state/README.md)
- [Draft, Live, and Deploy](../07-lifecycle/draft-live-deploy.md)
- [ADR-0003: Chat Resume Is Not Memory](../09-adrs/ADR-0003-chat-resume-is-not-memory.md)

## Purpose

Dennett does not pretend there is one universal external memory standard.

Dennett instead owns one vendor-neutral internal memory layer. Provider-specific systems are attached through adapters under that layer. Memory bindings are the portable declaration of intent that Core uses to expose that internal capability to agents and nodes.

## Current Executable Slice

The current executable slice is narrower than the full design.

Today Dennett really implements:

- local memory-provider registration in product state;
- provider resolution through portable `codex_ref`;
- capability and transport negotiation through the internal memory layer;
- a first real provider adapter for Mem0;
- direct provider-backed memory operations through Core and CLI.
- Stage 2 runtime graph memory for the narrow Codex App Server adapter path, where Core resolves registered memory providers, builds provider-neutral `memory_context`, renders that context into Codex developer instructions, and writes successful node output back through the internal memory layer.
- Stage 3 provider-operations cleanup for Mem0 namespace-scoped preview plus verified delete, limited to explicit user, agent, or run scope.

Today Dennett does not yet implement:

- automatic inheritance of memory across child runs;
- provider families beyond Mem0;
- finished MCP-backed memory execution as a separate product lane.
- native App Server memory storage or provider-managed runtime memory; the Stage 2 Codex path is prompt-rendered context, not an App Server memory primitive.
- true provider backup/restore, graph-store cleanup, provider-wide delete-all, or broad provider reliability guarantees.

That means memory is now a real subsystem and the first runtime graph path exists, but only for registered-provider resolution plus Codex prompt rendering, success-only provider writes, and bounded namespace-scoped provider cleanup. It is not universal provider support, native App Server memory, true restore, graph-store cleanup, provider-wide cleanup, or release-ready provider reliability.

## What The Internal Layer Is

The internal memory layer is a Core-owned abstraction for long-lived context that is separate from chat history, resume state, parameters, and graph variables.

It provides one stable question for Core:

- what memory intent is being requested?
- what capabilities are required?
- which local provider registration can satisfy it?
- which transport mode, if any, is allowed?

It does not provide a single shared external schema. That would be fiction across the providers Dennett is expected to support.

## Provider Adapter Model

Providers are handled by separate adapters under the memory layer.

That means:

- Core speaks in portable intent, capability, and scope terms;
- adapters translate that into provider-native API, SDK, or transport-specific behavior;
- the provider remains user-owned and locally registered outside the portable agent file;
- Core must not silently install, own, or replace the user's provider.

The portable file may point at a local provider registration, but that registration is local configuration, not portable truth.

## MCP Placement

MCP is not a separate top-level memory family.

For memory, MCP is modeled as a transport or connection mode under the provider-adapter layer. It is one possible way for a provider adapter to reach the provider's memory surface when that provider exposes MCP, but it is not the memory abstraction itself.

That distinction matters:

- Mem0: API/SDK is primary; MCP exists but is narrower, so MCP is only a transport variant.
- Zep: API/SDK is primary; the docs MCP surface is not memory, so memory and docs transport stay separate.
- Supermemory: API/SDK is primary; MCP exists but is narrower, so MCP remains a transport variant.
- Graphiti: the graph engine is the core abstraction; MCP is experimental, so graph and temporal semantics stay explicit.
- Letta: the full stateful agent runtime owns memory natively; MCP is not the memory abstraction.

## Portable Binding Rules

The portable shape is owned by [Memory Binding Model Contract](../03-contracts/agent-json/memory-binding-model-contract.md).

In this extension, the important architectural rules are:

- `memory_bindings` declare intent, not a hidden provider default;
- `scope` controls availability inside the agent definition, not provider selection;
- `required_capabilities` describe what the selected provider adapter must actually support;
- provider-specific behavior is allowed only through the explicit escape hatch;
- the escape hatch must never override local user-owned provider registration fields;
- in the current Mem0-first slice, the only portable override path is `provider_extension.config.mem0_config.graph_store`;
- inside that graph-store override, only provider selection and optional empty config are portable;
- nested provider-auth and account-bearing fields under `llm`, `embedder`, and `vector_store` remain local-only and are not portable override inputs;
- `graph_store.provider` is required whenever portable `graph_store` is present;
- nested keys under `graph_store.config` also remain local-only and are not portable override inputs;
- `mcp` is a transport preference, never a second memory family.

Core may accept a local placeholder binding, but a binding is only truly portable when it carries the standardized memory payload and capability requirements.

## Capability Negotiation

Core must negotiate memory capabilities before launch whenever the binding requires more than a bare local reference.

Negotiation order:

1. Resolve the local provider registration referenced by `codex_ref` or the provider escape hatch.
2. Check that the selected provider adapter supports the requested capabilities.
3. Check that the requested transport is permitted.
4. Apply the provider-specific extension only when it is present and supported.
5. Fail explicitly if the provider cannot satisfy the binding.

Core must not interpret a failed negotiation as permission to fall back to chat history or to a different hidden provider.

## Local Provider Registration

The real provider choice belongs to the user.

Dennett treats provider selection as local configuration, not as portable file truth. A local registration may contain:

- the provider family or adapter name;
- account or credential references;
- transport preferences;
- provider-specific config;
- any status data the adapter wants to cache locally.

If the user has not registered a provider, or if the selected provider is unavailable, the run must fail explicitly. Dennett must not quietly replace the provider with another one.

In the current Phase 13 slice, this local registration is executable product state rather than documentation-only intent.

## Failure Semantics

Memory failures must be clear and immediate:

- unknown local provider reference;
- unsupported capability;
- unsupported transport;
- missing local registration for an explicitly named provider;
- provider-specific config the adapter cannot interpret;
- any attempt to treat MCP as a different memory family;
- any attempt to silently reuse chat or resume state as a substitute for missing memory.

These are launch-time or validation-time failures. They are not hints to continue with a weaker hidden behavior.

## What Core May And May Not Assume

Core may assume:

- memory is a first-class but optional capability;
- providers differ materially in capability and surface area;
- the selected provider adapter is responsible for provider-specific translation;
- the user owns the actual provider registration and credentials.

Core may not assume:

- one universal memory standard;
- one universal transport model;
- that MCP implies memory support;
- that chat history is memory;
- that Dennett owns the provider lifecycle.

## Provider Facts To Preserve

The design direction is not abstract preference; it is grounded in the providers Dennett needs to handle.

| Provider | Primary surface | Key fact | Dennett rule |
| --- | --- | --- | --- |
| Mem0 | API/SDK first | entity-scoped memory, infer/extract path, optional graph memory; MCP exists but is narrower | Model MCP as transport only. Keep entity memory and optional graph memory explicit in provider-specific config. |
| Zep | API/SDK first | memory plus graph/context engine; session/thread/user/group semantics; docs MCP is not memory | Keep docs MCP separate from memory bindings. Do not flatten session semantics into a generic memory family. |
| Supermemory | API/SDK first | memory plus RAG plus user profile synthesis; containerTag/customId/versioning style features; MCP exists but is narrower | Surface versioning and profile synthesis through the escape hatch when needed. |
| Graphiti | Graph engine/framework | graph and temporal semantics are central; MCP exists and is experimental | Model graph and temporal capabilities directly. Treat MCP as optional transport plumbing only. |
| Letta | Full stateful agent platform | memory is native to the runtime; MCP is not the memory abstraction | Treat Letta as a provider adapter family, not as a second top-level memory model. |

## Boundary With Chat And Resume

Memory is not a different spelling for saved chat.

These concepts stay separate:

- chat and resume state preserve continuity for an existing run or session;
- memory bindings expose external or durable context that an agent may consult during work;
- params and vars stay part of the explicit graph input and data flow model.

Nothing in this extension allows Core to silently promote chat transcripts into memory or to treat memory as hidden resume state.

## Cross-Agent Boundaries

Memory bindings belong to the agent definition currently being executed.

When one agent launches another through an `orchestrator_agent` node, the callee does not automatically inherit the caller's memory bindings. Any future cross-agent propagation rule would require its own explicit specification.

## Storage Boundary

Core may store local metadata about configured bindings, availability checks, or last-seen adapter status.

Core must not:

- copy the full external memory contents into local chat storage by default;
- treat memory content as canonical agent data;
- use SQLite or another local store to replace the declared binding itself;
- infer provider installation or lifecycle ownership from the portable file.

## What This Extension Does Not Require

This document does not require:

- memory to exist for every agent;
- one shared memory model across all runtimes;
- deep inspection of memory contents by Core;
- automatic synchronization between memory and chat history;
- MCP to become a top-level memory family.

The extension is optional by design.

## Stage 2 Runtime Memory Context Contract

Stage 2 runtime-native memory uses a normalized `memory_context` request field. This field is part of the orchestrator-to-runtime request contract, not persisted agent JSON.

Core owns construction of `memory_context`. A runtime adapter may consume it, render it into a runtime-native prompt, context block, MCP/tool surface, or other adapter-private mechanism, but the adapter must not resolve providers, read provider credentials, or bypass the internal memory layer.

When a `runtime_agent` node has effective memory bindings, Core must either:

- resolve every effective binding through the local provider registry and pass a normalized `memory_context`; or
- fail before runtime launch if the adapter or provider cannot satisfy the declared requirements.

The normalized `memory_context` object is provider-neutral and closed for Stage 2:

- `bindings`: required array. One entry per effective memory binding available to the node.
- `bindings[].binding_id`: required string. The portable `memory_bindings[].id`.
- `bindings[].codex_ref`: required string. The portable lookup key resolved through the local registry.
- `bindings[].intent`: required object copied from portable binding config.
- `bindings[].required_capabilities`: required array copied from portable binding config after validation.
- `bindings[].scope`: required object. Provider-neutral operation scope selected by Core for this node invocation.
- `bindings[].read`: optional object. Present only for read-eligible bindings after Core has retrieved provider records.
- `bindings[].read.records`: required array when `read` is present. Records are normalized memory records from the internal memory port.
- `bindings[].write`: required object.
- `bindings[].write.enabled`: required boolean. `true` only for write-eligible bindings.
- `bindings[].write.mode`: required string when `enabled = true`. Stage 2 allows only `node_success_output`.
- `bindings[].write.disabled_reason`: optional string when `enabled = false`.

`memory_context` must not contain provider credentials, local executable paths, account identifiers, provider SDK clients, raw chat history, hidden runtime traces, or full provider-native response payloads. Provider-native details may remain inside `records[].provider_data` only when the internal memory port already exposes them as opaque adapter data and the runtime adapter is allowed to pass them through.

## Runtime Memory Read Semantics

A binding is read-eligible when all of the following are true:

- the binding is effective for the node by `scope` and `memory_ids`;
- the binding has `kind = runtime_memory`;
- `required_capabilities` contains `read` or `rag_retrieval`;
- the selected provider adapter satisfies capability and transport negotiation.

Before launching the runtime node, Core performs provider retrieval for each read-eligible binding through the internal memory layer. The Stage 2 default retrieval query is the resolved node `input_message` rendered as stable text: strings are used as-is, JSON objects and arrays are serialized with deterministic key ordering, JSON numbers and booleans use their JSON literal form, and `null` renders as `null`.

The Stage 2 default retrieval limit is `5` records per read-eligible binding unless a later owner document defines an explicit portable override. A read-eligible binding that cannot be resolved, searched, or listed according to its declared capabilities fails the node before runtime launch. Core must not replace that failure with chat history, resume state, or another hidden provider.

Runtime adapters may summarize or format retrieved records for the runtime, but they must preserve the distinction between memory context and chat/history. Retrieved records are context for this invocation only; they are not graph variables, node outputs, or canonical local copies of provider state.

## Runtime Memory Write Semantics

A binding is write-eligible when all of the following are true:

- the binding is effective for the node by `scope` and `memory_ids`;
- the binding has `kind = runtime_memory`;
- `required_capabilities` contains `write`;
- the selected provider adapter satisfies capability and transport negotiation.

Core writes memory only after the runtime node reaches terminal `success` and the final output has satisfied the node `output` contract. Core must not write memory for `invalid_output`, `runtime_error`, `interrupted`, `cancelled`, partial streaming output, comments, user-chat prompts, tool traces, or hidden runtime history.

For each write-eligible binding, Stage 2 writes exactly the node's final successful output as the durable memory content:

- for `output.mode = text`, `content` is the exact `output_text`;
- for `output.mode = json`, `content` is the deterministic compact JSON serialization of `output_json`.

The write request must use this provider-neutral scope:

- `scope.agent_id`: required. The current agent `meta.id`.
- `scope.run_id`: required. The current graph run ID.
- `scope.user_id`: optional. Present only when Core has an explicit authenticated or caller-supplied user identity for the run. Core must not invent a user ID.

The write request metadata must include:

- `dennett_kind`: `runtime_node_output`;
- `agent_id`;
- `run_id`;
- `node_id`;
- `binding_id`;
- `attempt_id`;
- `output_mode`;
- `output_hash`;
- `dennett_write_key`.

`dennett_write_key` is the deterministic idempotency key for the Stage 2 mitigation. It must be derived from `run_id`, `node_id`, `binding_id`, and `output_hash`. It must not include provider credentials, timestamps, random values, or user-visible prompt text.

If the binding also requires `infer_extract`, Core may set the internal memory-port `infer` flag for that write. If `infer_extract` is absent, Core must not require provider-side extraction behavior.

Post-success memory write is part of node completion for write-eligible bindings. Core must not advance downstream nodes or mark the node fully completed until required provider writes have either succeeded or failed visibly. If a memory write fails, the graph run must expose a memory-write failure instead of pretending the node completed normally.

Stage 2 does not claim transactional atomicity with external memory providers. If several write-eligible bindings exist and one provider write succeeds before another fails, Core must not pretend it rolled back the successful external write. The mitigation is visible failure plus deterministic metadata and `dennett_write_key` so retries can detect or reduce duplicate writes where the provider supports metadata search, idempotent create, or update-by-key semantics.

Retry behavior must be explicit:

- retrying a node after a failed, interrupted, or cancelled runtime outcome must not write memory for the failed attempt;
- retrying after a runtime success whose memory write failed may write again, but must reuse the same `dennett_write_key` when the final output hash is the same;
- if the retried runtime produces different successful output, it produces a different `output_hash` and therefore a different write key;
- providers without idempotent write or metadata-query support may still duplicate records, and Stage 2 must document that as a provider limitation rather than hiding it.

## Runtime `memory_context` Implementation Surface List

The Stage 2 implementation must update every surface below consistently. This list is the contract handoff for implementation workers.

- Source runtime port: `src/ports/runtime.ts` must add the normalized `memory_context` request shape, keep `supports_memory_bindings` capability-gated, and keep provider details out of the public adapter request.
- Graph execution: `src/core/graph-runner.ts` must resolve effective memory bindings, construct `memory_context`, perform pre-launch read retrieval, enforce adapter capability failure, and perform post-success writes before downstream advancement.
- Memory service boundary: `src/core/memory-service.ts` and the internal memory port must remain the only provider read/search/write path used by graph execution.
- Codex adapter rendering: `src/adapters/codex/codex-app-server-runtime-adapter.ts` must render `memory_context` into the App Server execution path without hardcoding Mem0 semantics into core or leaking local provider config.
- Public TypeScript contracts: `contracts/typescript/runtime-adapter.ts` must expose the same `memory_context` request shape used by `src/ports/runtime.ts`.
- JSON Schema contracts: `contracts/json-schema/runtime-adapter-request.schema.json` must validate the normalized `memory_context` field; `contracts/json-schema/runtime-adapter-capabilities.schema.json` must continue to represent the capability gate that makes memory context legal or illegal.
- Contract docs: `docs/03-contracts/runtime-adapter-contract.md` must describe the `memory_context` field and cross-link this owner section for read/write semantics.
- Agent JSON docs and schemas: existing `memory_bindings` and node `memory_ids` surfaces must remain provider-neutral; implementation must not add credentials or local provider config to agent files.
- Tests: unit and integration tests must cover request construction, capability rejection, provider resolution failure, read-context population, success-only writes, no writes on non-success outcomes, retry/idempotency metadata, and adapter rendering.
- Examples and proof docs: Stage 2 examples must show a graph-level memory binding that resolves through a registered provider, while proof notes must state whether the path is direct local Mem0 proof, runtime-native Codex graph proof, or still deferred.

## Stage 3 Provider Operations Cleanup

Stage 3 adds a narrow provider-operations surface for Mem0 cleanup. It is not a general memory lifecycle or restore system.

The implemented cleanup contract is:

- cleanup requires a registered Mem0 provider whose local `mem0_config.dennett_namespace_id` is set;
- `memory-cleanup-preview` requires at least one explicit scope selector: `--user-id`, `--agent-id`, or `--run-id`;
- preview returns the namespace, scope, bounded candidate IDs, truncation state, and a confirmation token derived from that preview;
- `memory-cleanup-verified-delete` re-previews the same scope, requires the matching token, deletes only the current preview candidates, and reports post-delete verification;
- a non-truncated cleanup with no remaining candidates may be reported as `verified_empty`;
- cleanup is bounded by the requested limit and must not be represented as provider-wide cleanup when `truncated = true` or when other scopes/namespaces exist.

Stage 3 does not claim:

- true provider backup or restore;
- graph-store cleanup;
- provider-wide delete-all;
- cleanup outside the configured `dennett_namespace_id`;
- durable external provider cleanup beyond the verified scoped records;
- broad Mem0 or provider reliability under load.

TASK-357 live proof used two local Mem0 provider registrations sharing one Chroma storage root and collection but with different `dennett_namespace_id` values. The target namespace preview found two user-scoped candidates, verified delete removed those two IDs and reported `verified_empty`, the target list returned `[]`, and the control namespace record survived. This proves only scoped namespace cleanup and control survival for that disposable local proof.

<a id="russian"></a>
# Memory Bindings

## Контракт Runtime `memory_context` Для Stage 2

Runtime-native memory в Stage 2 использует нормализованное поле запроса `memory_context`. Это поле относится к контракту между Core и runtime adapter, а не к сохраняемому Agent JSON.

Core строит `memory_context`. Runtime adapter может преобразовать его в prompt, context block, MCP/tool surface или другой adapter-private механизм, но adapter не должен сам разрешать providers, читать credentials или обходить внутренний memory layer.

Когда у `runtime_agent` есть effective memory bindings, Core обязан либо разрешить каждую binding через local provider registry и передать нормализованный `memory_context`, либо завершить подготовку ноды ошибкой до запуска runtime, если adapter или provider не удовлетворяет требованиям.

Нормализованный `memory_context` остается provider-neutral и закрытым для Stage 2:

- `bindings`: обязательный массив, по одному элементу на effective memory binding ноды.
- `bindings[].binding_id`: обязательная строка, portable `memory_bindings[].id`.
- `bindings[].codex_ref`: обязательная строка, portable lookup key, разрешенный через local registry.
- `bindings[].intent`: обязательный объект, скопированный из portable binding config.
- `bindings[].required_capabilities`: обязательный массив, скопированный из portable binding config после validation.
- `bindings[].scope`: обязательный объект. Provider-neutral operation scope, выбранный Core для этого invocation ноды.
- `bindings[].read`: необязательный объект, присутствует только после provider retrieval для read-eligible binding.
- `bindings[].read.records`: обязательный массив, если `read` присутствует. Records являются нормализованными memory records из internal memory port.
- `bindings[].write`: обязательный объект.
- `bindings[].write.enabled`: обязательный boolean; `true` только для write-eligible binding.
- `bindings[].write.mode`: обязательная строка при `enabled = true`; Stage 2 допускает только `node_success_output`.
- `bindings[].write.disabled_reason`: необязательная строка при `enabled = false`.

`memory_context` не должен содержать provider credentials, local executable paths, account identifiers, provider SDK clients, raw chat history, hidden runtime traces или полные provider-native response payloads. Provider-native details могут оставаться внутри `records[].provider_data` только когда internal memory port уже exposes them as opaque adapter data и runtime adapter разрешено передавать их дальше.

## Семантика Runtime Memory Read

Binding является read-eligible, если она effective для ноды через `scope` и `memory_ids`, имеет `kind = runtime_memory`, содержит capability `read` или `rag_retrieval`, и выбранный provider adapter проходит capability/transport negotiation.

До запуска runtime node Core выполняет retrieval через internal memory layer. Default query для Stage 2 - resolved node `input_message`, приведенный к stable text: строка используется как есть, JSON objects и arrays сериализуются с детерминированным порядком ключей, numbers и booleans используют JSON literal, `null` становится `null`.

Default limit для Stage 2 - `5` records на read-eligible binding, пока отдельный owner document не задаст portable override. Ошибка resolution, search или list для read-eligible binding завершает ноду до запуска runtime. Core не должен заменять это chat history, resume state или hidden provider.

Runtime adapters могут summarize или format retrieved records для runtime, но обязаны сохранять различие между memory context и chat/history. Retrieved records являются context только для этого invocation; они не являются graph variables, node outputs или canonical local copies of provider state.

## Семантика Runtime Memory Write

Binding является write-eligible, если она effective для ноды через `scope` и `memory_ids`, имеет `kind = runtime_memory`, содержит capability `write`, и выбранный provider adapter проходит capability/transport negotiation.

Core пишет memory только после terminal `success`, когда final output уже удовлетворяет контракту `output`. Core не пишет memory для `invalid_output`, `runtime_error`, `interrupted`, `cancelled`, partial streaming output, comments, user-chat prompts, tool traces или hidden runtime history.

Для каждой write-eligible binding Stage 2 пишет ровно final successful output ноды как durable memory content:

- при `output.mode = text`, `content` равен точному `output_text`;
- при `output.mode = json`, `content` равен deterministic compact JSON serialization от `output_json`.

Write request должен использовать следующий provider-neutral scope:

- `scope.agent_id`: обязателен, текущий `meta.id` агента.
- `scope.run_id`: обязателен, текущий graph run ID.
- `scope.user_id`: необязателен. Присутствует только когда у Core есть явная authenticated или caller-supplied user identity для run. Core не должен выдумывать user ID.

Metadata write request должна включать `dennett_kind`, `agent_id`, `run_id`, `node_id`, `binding_id`, `attempt_id`, `output_mode`, `output_hash` и `dennett_write_key`.

`dennett_write_key` - детерминированный idempotency key Stage 2, построенный из `run_id`, `node_id`, `binding_id` и `output_hash`. Он не должен включать provider credentials, timestamps, random values или user-visible prompt text.

Если binding также требует `infer_extract`, Core может выставить internal memory-port flag `infer` для этого write. Если `infer_extract` отсутствует, Core не должен требовать provider-side extraction behavior.

Post-success memory write является частью completion для write-eligible bindings. Core не должен продвигать downstream nodes или помечать ноду полностью завершенной, пока обязательные provider writes не завершились успехом или видимой ошибкой. Если memory write failed, graph run должен expose memory-write failure вместо того, чтобы делать вид, что нода завершилась нормально.

Stage 2 не заявляет transactional atomicity с external memory providers. Если существует несколько write-eligible bindings и один provider write успел завершиться успехом до failure другого, Core не должен делать вид, что successful external write был rollbacked. Mitigation - visible failure плюс deterministic metadata и `dennett_write_key`, чтобы retries могли обнаруживать или снижать duplicate writes там, где provider поддерживает metadata search, idempotent create или update-by-key semantics.

Retry behavior должен быть явным:

- retry ноды после failed, interrupted или cancelled runtime outcome не должен писать memory для failed attempt;
- retry после runtime success, у которого memory write failed, может писать снова, но обязан использовать тот же `dennett_write_key`, если final output hash тот же;
- если retried runtime производит другой successful output, он производит другой `output_hash` и поэтому другой write key;
- providers без поддержки idempotent write или metadata-query всё еще могут создавать duplicate records, и Stage 2 должен документировать это как provider limitation, а не скрывать.

## Поверхности Реализации Runtime `memory_context`

Stage 2 implementation должна согласованно обновить следующие поверхности:

- Source runtime port: `src/ports/runtime.ts`.
- Graph execution: `src/core/graph-runner.ts`.
- Memory service boundary: `src/core/memory-service.ts` и internal memory port.
- Codex adapter rendering: `src/adapters/codex/codex-app-server-runtime-adapter.ts`.
- Public TypeScript contracts: `contracts/typescript/runtime-adapter.ts`.
- JSON Schema contracts: `contracts/json-schema/runtime-adapter-request.schema.json` и `contracts/json-schema/runtime-adapter-capabilities.schema.json`.
- Contract docs: `docs/03-contracts/runtime-adapter-contract.md`.
- Agent JSON docs and schemas: existing `memory_bindings` и node `memory_ids` должны остаться provider-neutral.
- Tests: request construction, capability rejection, provider resolution failure, read-context population, success-only writes, no writes on non-success outcomes, retry/idempotency metadata и adapter rendering.
- Examples and proof docs: graph-level memory binding должна разрешаться через registered provider, а proof notes должны различать direct local Mem0 proof, runtime-native Codex graph proof и deferred status.

Статус: нормативный владелец расширения memory-binding.

Связанные документы:

- [Каноническая спецификация](../../agent_orchestrator_final_spec_v2.md)
- [Контракт Agent JSON](../03-contracts/agent-json/README.md)
- [Контракт модели memory binding](../03-contracts/agent-json/memory-binding-model-contract.md)
- [Модель интеграции runtime](../02-architecture/runtime-integration-model.md)
- [Состояние](../05-state/README.md)
- [Draft, Live и Deploy](../07-lifecycle/draft-live-deploy.md)
- [ADR-0003: Chat Resume Is Not Memory](../09-adrs/ADR-0003-chat-resume-is-not-memory.md)

## Назначение

Dennett не делает вид, что существует один универсальный внешний memory standard.

Вместо этого Dennett владеет одним vendor-neutral внутренним memory layer. Provider-specific системы подключаются через adapters под этим слоем. Memory bindings являются переносимым объявлением намерения, которое Core использует для экспонирования этой возможности агентам и нодам.

## Что Такое Внутренний Слой

Внутренний memory layer - это Core-owned abstraction для долгоживущего контекста, отдельного от chat history, resume state, параметров и graph variables.

Он отвечает на один стабильный набор вопросов:

- какое memory intent требуется?
- какие capabilities нужны?
- какая локальная provider registration может это удовлетворить?
- какой transport mode разрешён?

Он не задает один общий внешний schema. Для нужных Dennett provider это было бы фикцией.

## Модель Provider Adapter

Provider'ы обрабатываются отдельными adapters под memory layer.

Это означает:

- Core говорит языком portable intent, capability и scope;
- adapters переводят это в provider-native API, SDK или transport-specific поведение;
- provider остается user-owned и локально registered вне portable agent file;
- Core не должен silently install, own или replace provider пользователя.

Portable file может ссылаться на локальный provider registration, но этот registration является локальной конфигурацией, а не portable truth.

## Позиция MCP

MCP не является отдельной top-level memory family.

Для memory MCP моделируется как transport или connection mode под provider-adapter layer. Это один из возможных способов для provider adapter добраться до memory surface provider'а, когда provider exposing MCP, но это не сама memory abstraction.

Это различие важно:

- Mem0: API/SDK - основная поверхность; MCP существует, но более узок, поэтому MCP - только transport variant.
- Zep: API/SDK - основная поверхность; docs MCP surface не является memory, поэтому memory и docs transport остаются отдельными.
- Supermemory: API/SDK - основная поверхность; MCP существует, но более узок, поэтому MCP остаётся transport variant.
- Graphiti: graph engine - центральная абстракция; MCP experimental, поэтому graph и temporal semantics остаются явными.
- Letta: full stateful agent runtime владеет memory нативно; MCP не является memory abstraction.

## Правила Переносимого Binding

Переносимая форма описана в [Контракте модели memory binding](../03-contracts/agent-json/memory-binding-model-contract.md).

В этом расширении важны следующие архитектурные правила:

- `memory_bindings` объявляют intent, а не скрытый provider default;
- `scope` управляет доступностью внутри определения агента, а не выбором provider;
- `required_capabilities` описывают то, что выбранный provider adapter должен реально поддерживать;
- provider-specific поведение допустимо только через явный escape hatch;
- escape hatch никогда не должен переопределять локальные user-owned поля provider registration;
- в текущем Mem0-first slice единственный portable override path - `provider_extension.config.mem0_config.graph_store`;
- внутри этого graph-store override переносимыми являются только provider selection и необязательный empty config;
- вложенные provider-auth и account-bearing поля под `llm`, `embedder` и `vector_store` остаются local-only и не являются portable override inputs;
- `graph_store.provider` обязателен каждый раз, когда присутствует portable `graph_store`;
- вложенные ключи под `graph_store.config` также остаются local-only и не являются portable override inputs;
- `mcp` является preference для transport, а не второй memory family.

Core может принять локальный placeholder binding, но binding становится по-настоящему portable только тогда, когда он несёт стандартизованный memory payload и требования по capabilities.

## Negotiation По Возможностям

Core должен проводить negotiation по memory capabilities до запуска, когда binding требует большего, чем bare local reference.

Порядок negotiation:

1. Разрешить local provider registration, на который ссылается `codex_ref` или provider escape hatch.
2. Проверить, что selected provider adapter поддерживает запрошенные capabilities.
3. Проверить, что запрошенный transport разрешён.
4. Применить provider-specific extension только если он присутствует и поддерживается.
5. Явно завершиться ошибкой, если provider не может выполнить binding.

Core не должен интерпретировать провал negotiation как разрешение откатиться к chat history или к другому скрытому provider.

## Локальная Регистрация Provider

Реальный выбор provider принадлежит пользователю.

Dennett рассматривает provider selection как local configuration, а не как portable file truth. Local registration может содержать:

- family provider'а или adapter name;
- account или credential references;
- transport preferences;
- provider-specific config;
- любые status data, которые adapter хочет кэшировать локально.

Если пользователь не зарегистрировал provider или выбранный provider недоступен, run должен завершиться явной ошибкой. Dennett не должен тихо подменять provider на другой.

## Семантика Отказа

Ошибки memory должны быть явными и немедленными:

- неизвестный local provider reference;
- неподдерживаемая capability;
- неподдерживаемый transport;
- отсутствующая local registration для явно указанного provider;
- provider-specific config, которую adapter не может интерпретировать;
- любая попытка считать MCP другой memory family;
- любая попытка silently reuse chat или resume state как замену отсутствующей memory.

Это ошибки launch-time или validation-time. Это не подсказки продолжить работу с более слабым скрытым поведением.

## Что Core Может И Не Может Предполагать

Core может предполагать:

- memory - это first-class, но optional capability;
- provider'ы materially различаются по capabilities и surface area;
- выбранный provider adapter отвечает за provider-specific translation;
- пользователь владеет реальной provider registration и credentials.

Core не может предполагать:

- один universal memory standard;
- один universal transport model;
- что MCP означает поддержку memory;
- что chat history является memory;
- что Dennett владеет lifecycle provider'а.

## Provider Facts, Которые Нужно Сохранить

Это не абстрактное предпочтение; это продиктовано provider'ами, которые Dennett должен поддерживать.

| Provider | Основная поверхность | Ключевой факт | Правило Dennett |
| --- | --- | --- | --- |
| Mem0 | API/SDK first | entity-scoped memory, infer/extract path, optional graph memory; MCP существует, но более узок | Моделировать MCP только как transport. Оставлять entity memory и optional graph memory явными в provider-specific config. |
| Zep | API/SDK first | memory плюс graph/context engine; session/thread/user/group semantics; docs MCP не является memory | Держать docs MCP отдельно от memory bindings. Не схлопывать session semantics в generic memory family. |
| Supermemory | API/SDK first | memory плюс RAG и user profile synthesis; features в стиле containerTag/customId/versioning; MCP существует, но более узок | Передавать versioning и profile synthesis через escape hatch, когда это нужно. |
| Graphiti | Graph engine/framework | graph и temporal semantics находятся в центре; MCP experimental | Моделировать graph и temporal capabilities напрямую. Treat MCP как optional transport plumbing only. |
| Letta | Full stateful agent platform | memory нативна для runtime; MCP не является memory abstraction | Рассматривать Letta как provider adapter family, а не как вторую top-level memory model. |

## Граница С Chat И Resume

Memory не является другим названием для сохранённого chat.

Эти понятия остаются раздельными:

- chat и resume state сохраняют continuity существующего run или session;
- memory bindings открывают внешний или долговременный context, к которому агент может обращаться во время работы;
- params и vars остаются частью явной модели input и data flow графа.

Ничто в этом расширении не позволяет Core молча превращать chat transcripts в memory или трактовать memory как hidden resume state.

## Межагентные Границы

Memory bindings принадлежат определению того агента, который исполняется в данный момент.

Когда один агент запускает другой через `orchestrator_agent` node, вызываемый агент не наследует memory bindings вызывающего автоматически. Любое будущее правило межагентной передачи потребует собственной явной спецификации.

## Граница Хранения

Core может хранить локальные метаданные о настроенных bindings, проверках доступности или последнем известном статусе adapter.

Core не должен:

- по умолчанию копировать полное внешнее содержимое memory в локальное chat storage;
- трактовать содержимое memory как canonical agent data;
- использовать SQLite или другое локальное хранилище как замену объявленного binding;
- выводить из portable file установку provider или ownership его жизненного цикла.

## Чего Это Расширение Не Требует

Этот документ не требует:

- наличия memory у каждого агента;
- одной shared memory model для всех runtime;
- глубокого анализа содержимого memory силами Core;
- автоматической синхронизации между memory и chat history;
- превращения MCP в top-level memory family.

Расширение намеренно остается optional.

## Stage 3 Provider Operations Cleanup

Stage 3 добавляет узкую provider-operations поверхность для cleanup Mem0. Это не общая система lifecycle для memory и не restore-система.

Реализованный cleanup contract:

- cleanup требует зарегистрированный Mem0 provider, у которого в локальном `mem0_config.dennett_namespace_id` задан namespace;
- `memory-cleanup-preview` требует как минимум один явный scope selector: `--user-id`, `--agent-id` или `--run-id`;
- preview возвращает namespace, scope, bounded candidate IDs, truncation state и confirmation token, производный от этого preview;
- `memory-cleanup-verified-delete` заново выполняет preview того же scope, требует matching token, удаляет только текущие preview candidates и сообщает post-delete verification;
- non-truncated cleanup без remaining candidates может быть представлен как `verified_empty`;
- cleanup ограничен запрошенным limit и не должен представляться как provider-wide cleanup, если `truncated = true` или существуют другие scopes/namespaces.

Stage 3 не заявляет:

- true restore или provider backup/restore;
- graph-store cleanup;
- provider-wide delete-all;
- cleanup вне настроенного `dennett_namespace_id`;
- cleanup вне явно заданного user, agent или run scope;
- broad provider reliability.

TASK-357 live proof использовал две локальные регистрации Mem0 provider с одним Chroma storage root и collection, но с разными значениями `dennett_namespace_id`. Preview target namespace нашел два user-scoped candidates, verified delete удалил эти два ID и сообщил `verified_empty`, target list вернул `[]`, а запись control namespace сохранилась. Это доказывает только scoped namespace cleanup и survival контрольного namespace для этого disposable local proof.

## TASK-333 Stage 2 Runtime Graph Memory Status

English: Stage 2 is implemented only for the narrow registered-provider plus Codex App Server prompt-rendering path. Core resolves memory through the local registry and internal memory layer, passes provider-neutral `memory_context` to the runtime adapter, and writes successful node output through the provider adapter. This is not native App Server memory, not a general provider reliability claim, and not durable provider cleanup.

Russian: Stage 2 реализован только для узкого пути registered provider + prompt rendering в Codex App Server. Core разрешает память через local registry и internal memory layer, передает provider-neutral `memory_context` runtime adapter'у и записывает успешный node output через provider adapter. Это не native App Server memory, не общий claim по reliability provider'ов и не доказательство durable cleanup provider data.
