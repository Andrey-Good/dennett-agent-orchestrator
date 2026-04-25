[English](#english) | [Russian](#russian)

<a id="english"></a>
# Phase 14 Native Runtime Surface Completion

Status: normative owner for the implemented Phase 14 runtime slice.

## Purpose

Phase 14 exists to turn the already-documented App Server-native runtime surface into a narrow executable slice without polluting portable agent JSON with Codex-specific metadata.

This phase owns:

- normalized model discovery;
- normalized runtime-environment introspection for auth, account, rate limits, and config;
- the first executable runtime-option overrides for `reasoning_effort`, `speed_tier`, and `personality`;
- CLI access to that local runtime metadata.
- Codex App Server timeout surfaces for execution, catalog inspection, environment inspection, comments, and prompt replies.

This phase does not claim:

- portable agent-file fields for model catalogs, rate limits, or account state;
- per-source runtime introspection in the current Codex adapter;
- runtime-native memory execution, built-in user chat, or managed subagent orchestration.

## Implemented Slice

The implemented Phase 14 slice is intentionally narrow:

1. `RuntimeAdapterCapabilities` now exposes booleans for:
   - `supports_model_discovery`
   - `supports_runtime_environment_introspection`
   - `supports_reasoning_effort`
   - `supports_speed_tiers`
   - `supports_personality`
2. The normalized runtime port now exposes:
   - `listModels(request?)`
   - `inspectRuntimeEnvironment()`
3. The Codex App Server adapter now reuses native App Server primitives:
   - `model/list`
   - `getAuthStatus`
   - `account/read`
   - `account/rateLimits/read`
   - `config/read`
   - `configRequirements/read`
4. The current execution slice now accepts these `runtime_options` keys:
   - `model`
   - `reasoning_effort`
   - `speed_tier`
   - `personality`
5. The CLI now exposes:
   - `runtime-model-list`
   - `runtime-env-inspect`
6. The Codex App Server adapter and CLI now expose operation-scoped timeout options:
   - `CODEX_APP_SERVER_EXECUTION_TIMEOUT` for runtime execution through `run`, `run-live`, `resume`, `event-dispatch`, and builder execution.
   - `CODEX_APP_SERVER_MODEL_CATALOG_TIMEOUT` for `runtime-model-list`.
   - `CODEX_APP_SERVER_ENVIRONMENT_TIMEOUT` for `runtime-env-inspect`.
   - `CODEX_APP_SERVER_COMMENT_TIMEOUT` for live `comment`.
   - `CODEX_APP_SERVER_REPLY_TIMEOUT` for live prompt `reply` delivery.

## Boundary Rules

The Phase 14 surface is local runtime metadata, not portable agent truth.

That means:

- model catalogs do not become persisted agent-file fields;
- auth/account/config/rate-limit snapshots do not become portable runtime-source fields;
- adapters may expose this metadata through the normalized runtime port and local CLI/UI;
- Core may use the new capability flags and local helper methods, but must not rewrite the portable graph model around them.
- timeout classification is based on the public operation, while internal App Server phases may appear only as safe diagnostic details.
- timeout is not cancellation; execution timeout persists as `runtime_error`, and durable `waiting_for_user` time is excluded from the active runtime timer.

## What Still Remains Out Of Scope

The following remain outside the implemented Phase 14 slice:

- `supports_explicit_runtime_source` and `supports_runtime_source_introspection` for the current Codex adapter remain `false`;
- runtime-source selection and allowlist semantics remain governed by the runtime-source extension;
- top-level and node-level skills/plugins/MCP bindings are still not executed in the current slice;
- built-in user chat remains a later interaction phase;
- per-source limit enforcement remains separate from the new global runtime-environment introspection surface.

## Evidence Level

The implemented slice is backed by:

- focused adapter tests;
- focused CLI/helper tests;
- graph-runner validation for the widened runtime-option allowlist;
- focused timeout tests for execution, model catalog, environment inspection, and live comments;
- full repo test validation;
- `typecheck` and build validation.
- real built-CLI proof for:
  - `runtime-model-list`
  - `runtime-env-inspect`

This document still does not claim broader real-world proof than that evidence supports.
In particular, the current evidence proves live model discovery and runtime-environment inspection on the Codex App Server path, while `reasoning_effort`, `speed_tier`, and `personality` remain validated primarily through focused automated coverage.

<a id="russian"></a>
# Phase 14 Native Runtime Surface Completion

Статус: нормативный owner-документ для реализованного runtime-среза Phase 14.

## Назначение

Phase 14 нужен затем, чтобы превратить уже задокументированную App Server-native runtime surface в узкий исполнимый срез и при этом не загрязнить portable agent JSON Codex-specific metadata.

Этот этап владеет:

- нормализованным discovery моделей;
- нормализованной runtime-environment introspection для auth, account, rate limits и config;
- первым исполнимым набором runtime-option overrides для `reasoning_effort`, `speed_tier` и `personality`;
- CLI-доступом к этим локальным runtime metadata.

Этот этап не заявляет:

- portable agent-file fields для model catalogs, rate limits или account state;
- per-source runtime introspection в текущем Codex adapter;
- runtime-native memory execution, built-in user chat или managed subagent orchestration.

## Реализованный срез

Реализованный срез Phase 14 намеренно узкий:

1. `RuntimeAdapterCapabilities` теперь содержит booleans для:
   - `supports_model_discovery`
   - `supports_runtime_environment_introspection`
   - `supports_reasoning_effort`
   - `supports_speed_tiers`
   - `supports_personality`
2. Нормализованный runtime port теперь открывает:
   - `listModels(request?)`
   - `inspectRuntimeEnvironment()`
3. Codex App Server adapter теперь переиспользует нативные App Server primitives:
   - `model/list`
   - `getAuthStatus`
   - `account/read`
   - `account/rateLimits/read`
   - `config/read`
   - `configRequirements/read`
4. Текущий execution slice теперь принимает такие ключи `runtime_options`:
   - `model`
   - `reasoning_effort`
   - `speed_tier`
   - `personality`
5. CLI теперь открывает:
   - `runtime-model-list`
   - `runtime-env-inspect`

## Правила границы

Поверхность Phase 14 — это локальные runtime metadata, а не portable agent truth.

Это означает:

- catalog моделей не становится persisted agent-file fields;
- auth/account/config/rate-limit snapshots не становятся portable runtime-source fields;
- adapters могут открывать эти metadata через normalized runtime port и локальный CLI/UI;
- Core может использовать новые capability flags и локальные helper methods, но не должен переписывать вокруг них portable graph model.

## Что все еще вне scope

Следующие вещи остаются вне реализованного среза Phase 14:

- `supports_explicit_runtime_source` и `supports_runtime_source_introspection` для текущего Codex adapter остаются `false`;
- source selection и allowlist semantics по-прежнему принадлежат runtime-source extension;
- top-level и node-level skills/plugins/MCP bindings все еще не исполняются в текущем срезе;
- built-in user chat остается более поздней interaction phase;
- per-source limit enforcement остается отдельным слоем относительно новой глобальной runtime-environment introspection surface.

## Уровень доказательств

Реализованный срез подтвержден:

- focused adapter tests;
- focused CLI/helper tests;
- graph-runner validation для расширенного allowlist runtime options;
- `typecheck` и build validation.

Этот документ не утверждает более широкого real-world proof, чем реально подтверждает этот набор доказательств.
