[English](#english) | [Русский](#russian)

<a id="english"></a>
# Runtime Adapter Contract

## Purpose and Ownership

This document owns the normalized contract between orchestrator core and a runtime adapter. It is an internal port contract, not a persisted user JSON schema. Concrete class names, method names, and transport choices may differ, but implementations MUST preserve the same fields, capability meaning, terminal outcomes, and prohibitions.

For the Codex path in this repository, this contract is satisfied by an App Server-native adapter implementation behind the boundary. The official App Server material and the generated protocol surface are the Codex-specific source of truth for those semantics. The adapter may launch a long-lived App Server process or binary as its transport endpoint, but a product interface CLI does not satisfy the contract by wrapping one-shot vendor CLI workflows on the adapter's behalf.

This document does not own persisted agent-file fields. Persisted field shapes live in [agent-json/top-level-and-bindings-contract.md](./agent-json/top-level-and-bindings-contract.md), [agent-json/nodes-and-edges-contract.md](./agent-json/nodes-and-edges-contract.md), and [agent-json/interaction-and-chat-contract.md](./agent-json/interaction-and-chat-contract.md).

## Contract Scope

- The contract applies only to `runtime_agent` nodes.
- `orchestrator_agent` execution is owned by orchestrator core and must not be routed through this adapter boundary.
- The adapter receives already-resolved orchestrator intent. It must not parse the agent file by itself in order to discover missing fields or default values.

## Logical Operations

Every adapter implementation MUST expose logical equivalents of the following operations:

- `describeCapabilities()`: returns the adapter capability object.
- `startExecution(request)`: starts a fresh run or a native resume, based on `request.resume.mode`.
- `listModels(request?)`: returns normalized model-catalog data when model discovery is supported.
- `inspectRuntimeEnvironment()`: returns normalized auth, account, rate-limit, and config metadata when runtime-environment introspection is supported.
- `inspectRuntimeSource(source)`: inspects one configured runtime source and returns normalized availability / limit metadata when source introspection is supported.
- `deliverComment(execution, text)`: injects a user comment into the active runtime session when comments are enabled and supported.
- `deliverUserChatResponse(execution, response)`: sends a response to a pending built-in user-chat MCP request.
- `cancelExecution(execution)`: cancels the active runtime session.

An implementation MAY use different concrete method names, but code outside the adapter layer MUST be able to rely on the same logical contract.

## Capability Object

The capability object is closed and MUST contain these booleans:

- `supports_native_resume`: `true` only if the adapter can continue a runtime-native session from an opaque session handle.
- `supports_live_comments`: `true` only if the adapter can inject user comments into an already running runtime session.
- `supports_builtin_user_chat_mcp`: `true` only if the adapter can expose the built-in system MCP `orchestrator.user_chat` to the runtime.
- `supports_memory_bindings`: `true` only if the adapter can consume resolved `memory_bindings` during runtime execution.
- `supports_model_discovery`: `true` only if the adapter can return normalized model-catalog metadata through this contract.
- `supports_runtime_environment_introspection`: `true` only if the adapter can return normalized auth, account, rate-limit, and config metadata through this contract.
- `supports_reasoning_effort`: `true` only if the adapter can honor a normalized runtime-option override for reasoning effort.
- `supports_speed_tiers`: `true` only if the adapter can honor a normalized runtime-option override for speed tier.
- `supports_personality`: `true` only if the adapter can honor a normalized runtime-option override for personality.
- `supports_explicit_runtime_source`: `true` only if the adapter can execute against a Core-selected `runtime_source` exactly as requested, without reopening source selection inside the adapter.
- `supports_runtime_source_introspection`: `true` only if the adapter can inspect a configured `runtime_source` and return normalized availability / limit metadata through this contract.

If a capability is `false`, the adapter MUST reject attempts to use that capability instead of silently degrading to undefined behavior.

## Normalized Execution Request

A runtime adapter MUST receive one normalized execution request per `runtime_agent` node invocation. The in-process representation may differ, but the request MUST carry the following fields and meanings:

- `node_id`: required string. Must equal the ID of the node being executed.
- `runtime_adapter`: required string. Must equal the node's declared adapter ID.
- `prompt`: required string. Must equal the node prompt exactly as defined by the node contract.
- `input_message`: required resolved input payload. It must be derived from the node `input` contract before the adapter call and MUST NOT contain unresolved `ref` tokens.
- `output`: required object. Must equal the node's declared output contract.
- `effective_bindings.skills`: resolved list of skill binding objects available to the node.
- `effective_bindings.mcps`: resolved list of MCP binding objects available to the node.
- `effective_bindings.plugins`: resolved list of plugin binding objects available to the node.
- `effective_bindings.memory_bindings`: resolved list of memory binding objects available to the node, if any.
- `permissions`: effective permission profile after top-level and node-level resolution.
- `runtime_options`: opaque object owned by the node contract and passed through unchanged.
- `runtime_source`: optional selected runtime source object defined below. It MUST be present when Core has narrowed or explicitly selected the effective source because of file-level `runtime_sources`, node policy, or user choice. It MAY be absent only when Core intentionally delegates source choice because no such narrowing applies to the invocation. When absent, the adapter may use its configured default source.
- `interaction.comments_enabled`: boolean telling the adapter whether comment injection is permitted for this node.
- `interaction.user_chat_server_name`: optional string. If present, it is the built-in system MCP name and currently MUST be `orchestrator.user_chat`.
- `resume.mode`: required enum. Allowed values are `fresh` and `native_resume`.
- `resume.native_session_handle`: required opaque value when `resume.mode = native_resume`; forbidden when `resume.mode = fresh`.

The adapter MUST treat `config`, `extra`, `runtime_options`, and opaque session handles as pass-through data unless this document explicitly assigns meaning to them.

### Runtime-Option Subset In The Current Executable Slice

The portable node contract still owns `runtime_options` as an opaque object. The current executable runtime slice assigns normalized meaning only to this subset:

- `model`: adapter-specific model identifier string.
- `reasoning_effort`: normalized reasoning-effort token.
- `speed_tier`: normalized speed-tier token.
- `personality`: normalized personality token.

Any other `runtime_options` key remains outside the current execution slice unless another owner document explicitly widens that surface.

### `output` Object

The normalized request carries the portable output contract as an explicit object:

- `output.mode`: required string. Allowed values are `text` and `json`.
- `output.schema`: required only when `output.mode = json`; forbidden when `output.mode = text`.

`output.schema`, when present, must be a JSON Schema 2020-12 object whose root describes a JSON object. For the Codex/App Server path, the adapter may translate this portable field into App Server-native structured-output parameters below the boundary, but those vendor field names are not the portable contract.

### `runtime_source` Object

When present, `runtime_source` is a closed object with these fields:

- `id`: required string. Selected source identifier. For file-declared sources, this MUST equal the matching `runtime_sources[].id`. For sources surfaced outside the file, this MUST be the local configured-source identifier that Core resolved for the current request.
- `runtime_adapter`: required string. Must equal the request `runtime_adapter`.
- `source_ref`: required string. Opaque adapter-specific handle for the concrete execution source the adapter must resolve.
- `description`: optional string. Human-readable label preserved when Core knows one.

`runtime_source.id` is an identity label for an already resolved source, not an instruction for the adapter to reopen source selection. Outside-the-file IDs are local configured-source identifiers rather than portable agent-file IDs. The adapter MUST accept the same object shape for both file-declared and outside-the-file selections and MUST resolve execution from `source_ref`.

## Request Invariants

- `input_message` must already reflect node-level reference resolution. The adapter must not fetch `params`, `vars`, prior node outputs, or event payload by itself.
- `output.mode = json` means the adapter is expected to return a JSON object that satisfies the declared schema or mark the outcome as `invalid_output`. It must not silently coerce arrays, scalars, or malformed text into an object.
- There is no hidden rule where absent schema means text output. The adapter must rely on the explicit `output` object it receives.
- `permissions` apply to the runtime invocation itself. Plugin-specific permission semantics are out of scope.
- If `interaction.user_chat_server_name` is present, the adapter must expose that MCP server to the runtime exactly as named or reject the request.
- If `resume.mode = native_resume` and `supports_native_resume = false`, the adapter must fail explicitly instead of starting a fresh run.
- If `runtime_source` is present, `runtime_source.runtime_adapter` must equal the request `runtime_adapter`.
- If `runtime_source` is present and `supports_explicit_runtime_source = false`, the adapter must fail explicitly instead of silently ignoring the requested source.
- If `runtime_source` is present, the adapter must treat `runtime_source.source_ref` as the authoritative concrete source handle. It must not require `runtime_source.id` to be a file-declared ID before executing or inspecting the source.
- If `runtime_source` is present, the adapter must execute against that exact source and must not reopen source selection or silently fall back to another source.
- If `runtime_source` is absent, the adapter may use only its configured default source for that invocation. It must not reconstruct file-level, node-level, or user-level source narrowing on its own.

## Runtime Source Inspection

If `supports_runtime_source_introspection = true`, the adapter MUST expose a logical equivalent of `inspectRuntimeSource(source)` for a configured runtime source object, using the same closed `runtime_source` object shape defined above, that Core can lawfully pass through this boundary.

The agent-file extension defines declared `runtime_sources`, but local source catalogs may also exist outside the file. This contract only defines inspection for an explicit configured source object that Core has already resolved. It does not define a portable way to inspect an unnamed adapter default that Core never surfaced as a source object.

The inspection result object is closed and MUST contain these fields:

- `source_id`: required string. Must equal the inspected source's `id`.
- `availability`: required enum. Allowed values are `available`, `unavailable`, and `unknown`.
- `limit_status`: required enum. Allowed values are `ok`, `limited`, `exhausted`, and `unknown`.
- `status_message`: optional string. Human-readable normalized summary for logs or UI.

Inspection result semantics:

- `availability = available` means the runtime reports that the source can currently be used for execution.
- `availability = unavailable` means the runtime reports that the source cannot currently be used for execution.
- `availability = unknown` means the inspection path exists, but the runtime did not return reliable availability status.
- `limit_status = ok` means the runtime did not report a current source-limit condition that blocks or constrains use.
- `limit_status = limited` means the runtime reports a current quota or rate condition that constrains use but does not fully block it.
- `limit_status = exhausted` means the runtime reports that a quota or limit condition currently prevents execution on that source.
- `limit_status = unknown` means the inspection path exists, but the runtime did not return reliable normalized limit status.

If `supports_runtime_source_introspection = false`, the adapter MUST reject inspection calls explicitly. Core must not assume any availability or limit metadata exists through this contract when that capability is absent.

## Model Discovery

If `supports_model_discovery = true`, the adapter MUST expose a logical equivalent of `listModels(request?)`.

The request object is open only to these optional fields:

- `cursor`: opaque pagination cursor.
- `limit`: positive integer hint.
- `include_hidden`: boolean that requests hidden models when the runtime supports surfacing them.

The normalized response object is closed and MUST contain:

- `models`: ordered array of normalized model descriptors.
- `next_cursor`: optional opaque pagination cursor.

Each normalized model descriptor is closed and MUST contain:

- `id`: stable runtime model identifier used for execution.
- `hidden`: boolean.
- `is_default`: boolean.
- `input_modalities`: array of strings.
- `supports_personality`: boolean.
- `supported_reasoning_efforts`: array of normalized reasoning-effort tokens.
- `additional_speed_tiers`: array of normalized speed-tier tokens.

It MAY additionally contain:

- `display_name`
- `description`
- `default_reasoning_effort`
- `upgrade_target`
- `upgrade_info`

This surface is local runtime metadata. It must not be treated as portable agent-file truth.

## Runtime Environment Introspection

If `supports_runtime_environment_introspection = true`, the adapter MUST expose a logical equivalent of `inspectRuntimeEnvironment()`.

The normalized response object is closed and MUST contain:

- `auth`: normalized auth status.
- `account`: normalized account summary.
- `rate_limits`: array of normalized rate-limit summaries.
- `config`: normalized runtime configuration snapshot.
- `config_requirements`: optional normalized configuration-requirements snapshot.

The normalized auth object is closed and MUST contain:

- `authenticated`: boolean.
- `requires_openai_auth`: boolean.
- `auth_method`: optional string.

The normalized account object is closed and MUST contain:

- `status`: `available`, `missing`, or `unknown`.
- optional `account_type`
- optional `email`
- optional `plan_type`

Each normalized rate-limit summary is closed and MUST contain:

- `limit_id`
- optional `limit_name`
- optional `plan_type`
- optional `primary`
- optional `secondary`
- optional `credits`

`primary`, `secondary`, and `credits` remain opaque JSON objects because the verified App Server surface exposes richer nested structures than the current normalized contract needs.

The normalized config snapshot MAY contain:

- `model`
- `review_model`
- `model_provider`
- `approval_policy`
- `sandbox_mode`
- `profile`
- `model_reasoning_effort`
- `service_tier`

The optional config-requirements snapshot MAY contain:

- `allowed_approval_policies`
- `allowed_sandbox_modes`
- `allowed_web_search_modes`
- `enforce_residency`
- `feature_requirements`

This surface is also local runtime metadata. It informs diagnostics and local UX but does not become portable agent-file truth.

## Event Stream

The adapter may emit only normalized event types across this boundary:

- `comment`: a user-visible non-final message from the runtime. The payload MUST contain `text: string`.
- `user_chat_request`: a built-in user-chat MCP request. The payload MUST conform to [orchestrator-user-chat-mcp-contract.md](./orchestrator-user-chat-mcp-contract.md).

Vendor-specific telemetry may exist inside the adapter implementation, but it MUST NOT cross this boundary unless it is first normalized and documented here.

## Terminal Result

Each execution MUST finish with exactly one terminal result object. The result object is closed except for `error.details`, which is opaque.

- `outcome`: required enum. Allowed values are `success`, `invalid_output`, `runtime_error`, `interrupted`, and `cancelled`.
- `output`: required when `outcome = success`. Must be a closed object that repeats the fulfilled output contract.
- `output.mode`: required when `outcome = success`. Must repeat the requested output mode.
- `output_text`: required when `outcome = success` and `output.mode = text`. Must be a string.
- `output_json`: required when `outcome = success` and `output.mode = json`. Must be a JSON object.
- `native_session_handle`: optional opaque value representing the session state needed for native resume.
- `error.code`: required when `outcome` is not `success`.
- `error.message`: required when `outcome` is not `success`.
- `error.details`: optional opaque diagnostic payload.

## Timeout Semantics

Timeouts are controlled runtime failures, not cancellation.

For the Codex App Server adapter, timeout codes are selected from the public operation being performed, not from the internal App Server phase that happened to be waiting when the timer expired:

- Runtime execution through `run`, `run-live`, `resume`, `event-dispatch`, and builder execution uses `CODEX_APP_SERVER_EXECUTION_TIMEOUT`.
- Runtime model catalog inspection uses `CODEX_APP_SERVER_MODEL_CATALOG_TIMEOUT`.
- Runtime environment inspection uses `CODEX_APP_SERVER_ENVIRONMENT_TIMEOUT`.
- Live comment delivery uses `CODEX_APP_SERVER_COMMENT_TIMEOUT`.
- Prompt reply live delivery uses `CODEX_APP_SERVER_REPLY_TIMEOUT`.

Execution timeout MUST map to a terminal `runtime_error`, which Core persists as a failed run with local resume available. It MUST NOT map to `cancelled` or `interrupted`.

The execution timeout measures only an active runtime segment. It starts when the runtime node begins App Server startup, thread/resume startup, turn startup, or terminal waiting. It stops when the adapter returns a terminal result, fails startup, reaches the timeout failure, or the run transitions to durable `waiting_for_user`. Time spent waiting for a human reply is excluded; explicit `reply` plus `resume` starts a fresh active segment timer.

Non-execution timeouts are surface-level failures and do not create or mutate graph terminal state by themselves. A timed-out `comment` must not append a visible comment. A timed-out CLI `reply` must record exactly one visible user reply for explicit resume, leave the run in `waiting_for_user`, keep the pending prompt unresolved, print `Prompt reply recorded for resume.` to stdout, and warn on stderr with `CODEX_APP_SERVER_REPLY_TIMEOUT`.

## Outcome Mapping Rules

- `success` means the runtime completed normally and returned a value that matches the requested `output` contract.
- `invalid_output` means the runtime call completed, but the final value does not satisfy the node output contract.
- `runtime_error` means the runtime could not complete the invocation correctly.
- `interrupted` means execution was stopped by orchestrator shutdown or an external interruption outside an explicit user cancellation.
- `cancelled` means execution was explicitly cancelled by the user.

The adapter must not convert `invalid_output` into `runtime_error` only because validation happens at the adapter boundary.

## Ingress During an Active Run

- `deliverComment` is valid only when `interaction.comments_enabled = true`, the active node is comment-targetable, and `supports_live_comments = true`.
- `deliverUserChatResponse` is valid only when the runtime has an unresolved pending built-in user-chat request and `supports_builtin_user_chat_mcp = true`.
- If the preconditions are not met, the adapter must reject the call explicitly.

## Adapter Prohibitions

The adapter MUST NOT do any of the following:

- Read the agent file as a second source of truth after the orchestrator has already resolved a request.
- Invent hidden defaults for missing persisted fields that are not defined by the canonical contract.
- Rewrite the meaning of `skills`, `mcps`, `plugins`, `memory_bindings`, or `runtime_sources`.
- Reopen runtime-source selection or silently fall back to a different source after Core has already passed `runtime_source`.
- For the Codex path in this repository, satisfy the adapter boundary through one-shot vendor CLI orchestration or by requiring a product interface layer to do so. Launching and managing a long-lived App Server process or binary behind the adapter boundary is allowed.
- Invent source availability or limit metadata that the runtime did not expose through the inspection contract.
- Replace the portable agent file with SQLite or any other local store as the canonical source of agent definition.
- Require domain modules to import vendor SDK types directly.
- Expose chain-of-thought, full tool traces, or internal runtime history as mandatory boundary fields.
- Silently downgrade `native_resume`, user comments, or built-in user-chat MCP into some other behavior.

## Staged App Server-Native Families Outside This Contract

This normalized contract is intentionally narrower than the full verified App Server surface. For the Codex path, the following App Server-native families are real but not yet part of the stable orchestrator port contract:

- thread-oriented primitives such as `thread/fork`, `thread/rollback`, and `thread/injectItems`;
- review-oriented primitives such as `review/start` and related review notifications;
- richer App Server notifications beyond `comment` and `user_chat_request`, such as turn/item, plan, reasoning-summary, command/file-change, token-usage, model-reroute, MCP-progress, and account/config/app/filesystem-status signals.

For the current stage, adapters may consume these families internally or through adapter-private and local-interface APIs when architecture documents allow it, but they MUST NOT smuggle them into the normalized request, event, or terminal-result shapes defined here.

Future contract expansion is allowed only when a leaf owner document assigns stable normalized meaning, failure semantics, and portability boundaries to the added surface.

## Cross-Links

- Adapter boundary rationale lives in [../02-architecture/runtime-integration-model.md](../02-architecture/runtime-integration-model.md).
- Persisted runtime-source and binding definitions live in [agent-json/top-level-and-bindings-contract.md](./agent-json/top-level-and-bindings-contract.md).
- Node-level execution settings live in [agent-json/nodes-and-edges-contract.md](./agent-json/nodes-and-edges-contract.md).
- User-chat payload shape lives in [orchestrator-user-chat-mcp-contract.md](./orchestrator-user-chat-mcp-contract.md).

<a id="russian"></a>
# Контракт Runtime Adapter

## Назначение И Владение

Этот документ владеет нормализованным контрактом между orchestrator core и runtime adapter. Это внутренний port-контракт, а не сохраняемая пользовательская JSON-схема. Конкретные имена классов, методов и транспорта могут отличаться, но реализации обязаны сохранять те же поля, смысл возможностей, терминальные исходы и запреты.

Для Codex path в этом репозитории этот контракт удовлетворяется App Server-native реализацией adapter-а за границей. Официальные материалы App Server и сгенерированная поверхность протокола являются Codex-specific source of truth для этих семантик. Продуктовый interface CLI не удовлетворяет этот контракт путем shell-out в vendor CLI от имени adapter-а.

Этот документ не владеет сохраняемыми полями agent file. Формы сохраняемых полей находятся в [agent-json/top-level-and-bindings-contract.md](./agent-json/top-level-and-bindings-contract.md), [agent-json/nodes-and-edges-contract.md](./agent-json/nodes-and-edges-contract.md) и [agent-json/interaction-and-chat-contract.md](./agent-json/interaction-and-chat-contract.md).

## Граница Контракта

- Контракт применяется только к нодам `runtime_agent`.
- Исполнение `orchestrator_agent` принадлежит orchestrator core и не должно проходить через эту adapter boundary.
- Адаптер получает уже разрешенное намерение оркестратора. Он не должен самостоятельно разбирать agent file, чтобы искать недостающие поля или значения по умолчанию.

## Логические Операции

Каждая реализация адаптера должна предоставлять логические эквиваленты следующих операций:

- `describeCapabilities()`: возвращает объект возможностей адаптера.
- `startExecution(request)`: запускает fresh-run или native resume в зависимости от `request.resume.mode`.
- `inspectRuntimeSource(source)`: инспектирует один настроенный runtime source и возвращает нормализованные metadata доступности и лимитов, если поддерживается source introspection.
- `deliverComment(execution, text)`: внедряет пользовательский комментарий в активную runtime-сессию, если комментарии разрешены и поддерживаются.
- `deliverUserChatResponse(execution, response)`: отправляет ответ на ожидающий встроенный user-chat MCP-запрос.
- `cancelExecution(execution)`: отменяет активную runtime-сессию.

Реализация может использовать другие конкретные имена методов, но код вне adapter-layer должен опираться на тот же логический контракт.

## Объект Возможностей

Объект возможностей является закрытым и обязан содержать следующие булевы поля:

- `supports_native_resume`: `true` только если адаптер умеет продолжать runtime-native session по opaque session handle.
- `supports_live_comments`: `true` только если адаптер умеет внедрять пользовательские комментарии в уже идущую runtime-сессию.
- `supports_builtin_user_chat_mcp`: `true` только если адаптер умеет открывать встроенный системный MCP `orchestrator.user_chat` для runtime.
- `supports_explicit_runtime_source`: `true` только если адаптер умеет выполнять запрос против явно выбранного Core `runtime_source` ровно так, как он передан, не переоткрывая выбор source внутри адаптера.
- `supports_runtime_source_introspection`: `true` только если адаптер умеет инспектировать настроенный `runtime_source` и возвращать через этот контракт нормализованные metadata доступности и лимитов.

Если возможность имеет значение `false`, адаптер обязан явно отклонять попытки использовать эту возможность, а не молча деградировать к неопределенному поведению.

## Нормализованный Execution Request

Runtime adapter обязан получать один нормализованный execution request на каждый вызов `runtime_agent`-ноды. Внутреннее in-process представление может отличаться, но запрос обязан нести следующие поля и значения:

- `node_id`: обязательная строка. Должна совпадать с ID исполняемой ноды.
- `runtime_adapter`: обязательная строка. Должна совпадать с объявленным ID адаптера в ноде.
- `prompt`: обязательная строка. Должна в точности совпадать с prompt ноды.
- `input_message`: обязательный resolved input payload. Он должен быть получен из контракта `input` до вызова адаптера и не должен содержать неразрешенные `ref`.
- `output`: обязательный объект. Должен совпадать с объявленным output-контрактом ноды.
- `effective_bindings.skills`: resolved-список skill binding-объектов, доступных ноде.
- `effective_bindings.mcps`: resolved-список MCP binding-объектов, доступных ноде.
- `effective_bindings.plugins`: resolved-список plugin binding-объектов, доступных ноде.
- `effective_bindings.memory_bindings`: resolved-список memory binding-объектов, доступных ноде, если они есть.
- `permissions`: эффективный permission profile после разрешения top-level и node-level правил.
- `runtime_options`: opaque object, которым владеет контракт ноды и который передается без изменения.
- `runtime_source`: необязательный выбранный объект источника исполнения, определенный ниже. Поле обязано присутствовать, когда Core сузил или явно выбрал эффективный source из-за file-level `runtime_sources`, node policy или user choice. Оно может отсутствовать только когда Core сознательно делегирует выбор source, потому что для этого запуска такое сужение не действует. Если поле отсутствует, адаптер может использовать свой настроенный источник по умолчанию.
- `interaction.comments_enabled`: булево значение, сообщающее адаптеру, разрешено ли внедрение комментариев для этой ноды.
- `interaction.user_chat_server_name`: необязательная строка. Если поле присутствует, это имя встроенного системного MCP, и в текущей версии оно обязано быть `orchestrator.user_chat`.
- `resume.mode`: обязательный enum. Допустимые значения: `fresh` и `native_resume`.
- `resume.native_session_handle`: обязательное opaque значение при `resume.mode = native_resume`; запрещено при `resume.mode = fresh`.

Адаптер обязан рассматривать `config`, `extra`, `runtime_options` и opaque session handles как pass-through-данные, если этот документ явно не задает им смысл.

### Объект `output`

Нормализованный запрос несет переносимый output-контракт как явный объект:

- `output.mode`: обязательная строка. Допустимые значения: `text` и `json`.
- `output.schema`: обязательно только при `output.mode = json`; запрещено при `output.mode = text`.

Если `output.schema` присутствует, оно должно быть JSON Schema 2020-12 object, у которого корень описывает JSON object. Для пути Codex/App Server адаптер может переводить это переносимое поле во внутренние App Server-native параметры structured output ниже границы порта, но эти vendor-поля не являются переносимым контрактом.

### Объект `runtime_source`

Если поле присутствует, `runtime_source` является закрытым объектом со следующими полями:

- `id`: обязательная строка. Идентификатор выбранного source. Для sources, объявленных в файле, он обязан совпадать с соответствующим `runtime_sources[].id`. Для sources, surfaced outside the file, это обязан быть идентификатор локально настроенного source, который Core разрешил для текущего запроса.
- `runtime_adapter`: обязательная строка. Должна совпадать с `runtime_adapter` запроса.
- `source_ref`: обязательная строка. Opaque adapter-specific handle конкретного execution source, который адаптер обязан разрешить.
- `description`: необязательная строка. Человеко-читаемая метка, если она известна Core.

`runtime_source.id` является identity label уже разрешенного source, а не инструкцией для адаптера заново открывать source selection. `id`, пришедшие не из файла, являются идентификаторами локально настроенных sources, а не переносимыми agent-file IDs. Адаптер обязан принимать одинаковую форму объекта и для file-declared, и для outside-the-file selections, и обязан разрешать исполнение по `source_ref`.

## Инварианты Запроса

- `input_message` уже должен отражать результат разрешения node-level references. Адаптер не должен самостоятельно получать `params`, `vars`, outputs предыдущих нод или event payload.
- `output.mode = json` означает, что адаптер обязан вернуть JSON object, удовлетворяющий объявленной schema, или пометить исход как `invalid_output`. Он не должен молча приводить массивы, скаляры или некорректный текст к объекту.
- Скрытого правила «нет schema - значит текстовый output» не существует. Адаптер обязан опираться на явный объект `output`, который он получил.
- `permissions` относятся к самому runtime-вызову. Plugin-specific permission semantics находятся вне области действия этого контракта.
- Если присутствует `interaction.user_chat_server_name`, адаптер обязан открыть runtime ровно этот MCP server по указанному имени или отклонить запрос.
- Если `resume.mode = native_resume`, а `supports_native_resume = false`, адаптер обязан вернуть явную ошибку, а не начинать fresh-run.
- Если присутствует `runtime_source`, `runtime_source.runtime_adapter` обязан совпадать с `runtime_adapter` запроса.
- Если присутствует `runtime_source`, а `supports_explicit_runtime_source = false`, адаптер обязан явно завершить вызов ошибкой, а не молча игнорировать запрошенный source.
- Если присутствует `runtime_source`, адаптер обязан трактовать `runtime_source.source_ref` как авторитетный concrete source handle. Он не должен требовать, чтобы `runtime_source.id` был file-declared ID, прежде чем выполнять или инспектировать source.
- Если присутствует `runtime_source`, адаптер обязан выполнять запрос ровно против этого source и не должен заново открывать выбор source или молча делать fallback на другой source.
- Если `runtime_source` отсутствует, адаптер может использовать только свой настроенный source по умолчанию для этого вызова. Он не должен самостоятельно восстанавливать file-level, node-level или user-level source-сужение.

## Инспекция Runtime Source

Если `supports_runtime_source_introspection = true`, адаптер обязан предоставлять логический эквивалент `inspectRuntimeSource(source)` для настроенного runtime source-объекта, используя ту же закрытую форму объекта `runtime_source`, которая определена выше, и который Core вправе передавать через эту границу.

Расширение agent file определяет объявленные `runtime_sources`, но вне файла также могут существовать локальные каталоги sources. Этот контракт определяет inspection только для явного настроенного source-объекта, который Core уже разрешил. Он не определяет переносимый способ инспектировать безымянный adapter default, который Core никогда не выводил как source-объект.

Inspection result object является закрытым и обязан содержать следующие поля:

- `source_id`: обязательная строка. Должна совпадать с `id` инспектируемого source.
- `availability`: обязательный enum. Допустимые значения: `available`, `unavailable` и `unknown`.
- `limit_status`: обязательный enum. Допустимые значения: `ok`, `limited`, `exhausted` и `unknown`.
- `status_message`: необязательная строка. Человеко-читаемое нормализованное summary для логов или UI.

Семантика результата инспекции:

- `availability = available` означает, что runtime сообщает о текущей возможности использовать source для исполнения.
- `availability = unavailable` означает, что runtime сообщает о текущей невозможности использовать source для исполнения.
- `availability = unknown` означает, что путь инспекции существует, но runtime не вернул надежный статус доступности.
- `limit_status = ok` означает, что runtime не сообщил о текущем source-limit условии, которое блокирует или ограничивает использование.
- `limit_status = limited` означает, что runtime сообщает о текущем quota- или rate-условии, которое ограничивает использование, но не блокирует его полностью.
- `limit_status = exhausted` означает, что runtime сообщает о quota- или limit-условии, которое сейчас не позволяет выполнять запуск на этом source.
- `limit_status = unknown` означает, что путь инспекции существует, но runtime не вернул надежный нормализованный статус лимитов.

Если `supports_runtime_source_introspection = false`, адаптер обязан явно отклонять вызовы инспекции. Когда эта возможность отсутствует, Core не должен предполагать существование каких-либо metadata доступности или лимитов через этот контракт.

## Поток Событий

Через эту границу адаптер может испускать только нормализованные типы событий:

- `comment`: видимое пользователю нефинальное сообщение от runtime. Payload обязан содержать `text: string`.
- `user_chat_request`: запрос встроенного user-chat MCP. Payload обязан соответствовать [orchestrator-user-chat-mcp-contract.md](./orchestrator-user-chat-mcp-contract.md).

Vendor-specific telemetry может существовать внутри реализации адаптера, но не должна пересекать эту границу, пока не будет сначала нормализована и описана здесь.

## Терминальный Результат

Каждое исполнение должно завершаться ровно одним терминальным result object. Result object является закрытым, за исключением `error.details`, которое остается opaque.

- `outcome`: обязательный enum. Допустимые значения: `success`, `invalid_output`, `runtime_error`, `interrupted` и `cancelled`.
- `output`: обязательно при `outcome = success`. Должно быть закрытым объектом, повторяющим выполненный output-контракт.
- `output.mode`: обязательно при `outcome = success`. Должно повторять запрошенный output mode.
- `output_text`: обязательно при `outcome = success` и `output.mode = text`. Должно быть строкой.
- `output_json`: обязательно при `outcome = success` и `output.mode = json`. Должно быть JSON object.
- `native_session_handle`: необязательное opaque значение, представляющее состояние сессии, нужное для native resume.
- `error.code`: обязательно, если `outcome` не равен `success`.
- `error.message`: обязательно, если `outcome` не равен `success`.
- `error.details`: необязательный opaque diagnostic payload.

## Правила Отображения Исходов

- `success` означает, что runtime завершился нормально и вернул значение, соответствующее запрошенному контракту `output`.
- `invalid_output` означает, что runtime-вызов завершился, но финальное значение не удовлетворяет output-контракту ноды.
- `runtime_error` означает, что runtime не смог корректно завершить вызов.
- `interrupted` означает, что исполнение было остановлено из-за shutdown оркестратора или внешнего прерывания вне явной пользовательской отмены.
- `cancelled` означает, что исполнение было явно отменено пользователем.

Адаптер не должен превращать `invalid_output` в `runtime_error` только потому, что валидация выполняется на adapter boundary.

## Входящие Действия Во Время Активного Run

- `deliverComment` допустим только если `interaction.comments_enabled = true`, активная нода допускает комментарии и `supports_live_comments = true`.
- `deliverUserChatResponse` допустим только если у runtime есть неразрешенный ожидающий built-in user-chat request и `supports_builtin_user_chat_mcp = true`.
- Если предусловия не выполнены, адаптер обязан явно отклонить вызов.

## Запреты Для Адаптера

Адаптер НЕ должен делать следующее:

- Читать agent file как второй источник истины после того, как оркестратор уже разрешил запрос.
- Придумывать скрытые defaults для отсутствующих сохраняемых полей, если они не определены каноническим контрактом.
- Переопределять смысл `skills`, `mcps`, `plugins`, `memory_bindings` или `runtime_sources`.
- Заново открывать выбор runtime source или молча делать fallback на другой source после того, как Core уже передал `runtime_source`.
- Для Codex path в этом репозитории удовлетворять adapter boundary через shell-out в vendor CLI или требовать, чтобы это делал product interface layer.
- Придумывать metadata доступности source или лимитов, которые runtime не открыл через inspection contract.
- Подменять переносимый agent file SQLite-базой или любым другим локальным хранилищем как каноническим источником определения агента.
- Требовать, чтобы доменные модули импортировали vendor SDK types напрямую.
- Делать chain-of-thought, полные tool traces или внутреннюю историю runtime обязательными полями этой границы.
- Молча деградировать `native_resume`, пользовательские комментарии или built-in user-chat MCP в какое-то иное поведение.

## Этапные App Server-native семейства вне этого контракта

Этот нормализованный контракт намеренно уже, чем полная проверенная поверхность App Server. Для Codex path следующие App Server-native семейства реальны, но пока не входят в стабильный orchestrator port contract:

- discovery моделей и соседней model metadata из `model/list`;
- локальная inspection auth/account/config/rate limits из `getAccount`, `getAccountRateLimits`, `config/read` и `config/requirements/read`;
- thread-ориентированные primitives вроде `thread/fork`, `thread/rollback` и `thread/injectItems`;
- review-ориентированные primitives вроде `review/start` и связанных review-notifications;
- более богатые App Server notifications сверх `comment` и `user_chat_request`, вроде turn/item-, plan-, reasoning-summary-, command/file-change-, token-usage-, model-reroute-, MCP-progress- и account/config/app/filesystem-status-сигналов.

На current stage adapters могут потреблять эти семейства внутренне или через adapter-private/local-interface APIs, когда это разрешают architecture-документы, но они НЕ ДОЛЖНЫ незаметно протаскивать их в нормализованные формы request, event или terminal result, определенные здесь.

Будущее расширение контракта допустимо только тогда, когда профильный leaf-документ-владелец задаст для добавляемой поверхности стабильный нормализованный смысл, failure semantics и границы переносимости.

## Перекрестные Ссылки

- Архитектурная мотивация adapter boundary находится в [../02-architecture/runtime-integration-model.md](../02-architecture/runtime-integration-model.md).
- Persisted-описание runtime sources и binding-объектов находится в [agent-json/top-level-and-bindings-contract.md](./agent-json/top-level-and-bindings-contract.md).
- Node-level execution settings находятся в [agent-json/nodes-and-edges-contract.md](./agent-json/nodes-and-edges-contract.md).
- Форма payload для user-chat находится в [orchestrator-user-chat-mcp-contract.md](./orchestrator-user-chat-mcp-contract.md).
