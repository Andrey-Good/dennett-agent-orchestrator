[English](#english) | [Russian](#russian)

<a id="english"></a>
# Phase 13 Mem0-First Native Memory Integration

Status: normative owner for the implemented Phase 13 memory slice.

Related documents:

- [Capability Gap Lock](../13-capability-gap-lock/phase-12-capability-gap-lock.md)
- [Memory Bindings](../08-extensions/memory-bindings.md)
- [Memory Binding Model Contract](../03-contracts/agent-json/memory-binding-model-contract.md)
- [Runtime Integration Model](../02-architecture/runtime-integration-model.md)

## Purpose

Phase 13 closes the first real gap between the portable memory-binding documents and the executable product slice.

It does that narrowly and honestly:

- by adding a local memory-provider registry;
- by adding a vendor-neutral internal memory port;
- by implementing a first provider adapter for Mem0;
- by exposing direct provider-backed memory operations through Core and CLI;
- by proving the core CLI CRUD path with real local Mem0 round-trips.

It does not claim that Codex App Server provides a native memory primitive or owns provider memory. TASK-333 Stage 2 adds a later narrow runtime graph path where Core resolves registered memory bindings, renders provider-neutral context into Codex developer instructions, and writes successful node output through the internal memory layer.

## Implemented Slice

The implemented Phase 13 slice consists of:

1. `SQLite`-backed local provider registration.
2. A Core-owned `MemoryService` that resolves portable `codex_ref` references through the local registry.
3. Capability and transport negotiation before provider use.
4. A `Mem0MemoryAdapter` behind the internal memory port.
5. Direct CRUD and search operations through the CLI:
   - `memory-provider-register`
   - `memory-provider-list`
   - `memory-provider-show`
   - `memory-write`
   - `memory-read`
   - `memory-search`
   - `memory-list`
   - `memory-update`
   - `memory-delete`
6. Real local Mem0 proof for register -> write -> read -> update -> list -> delete -> read-after-delete.
7. Automated search coverage for the Mem0 adapter, `MemoryService`, and CLI path.
8. Stage 3 provider-operations cleanup:
   - `memory-cleanup-preview`
   - `memory-cleanup-verified-delete`
   - namespace isolation through local `mem0_config.dennett_namespace_id`
   - explicit user, agent, or run scope requirements
   - bounded candidate lists and post-delete verification.

## Local Provider Registry

Phase 13 introduces a local registry record for memory providers.

The registry owns:

- stable local `provider_id`;
- portable lookup key `codex_ref`;
- `provider_family`;
- transport metadata;
- advertised capability set;
- local provider config;
- local availability and error status.

This registry is local operational state. It is not portable agent truth.

## Mem0 Local Config Shape

In the current Phase 13 slice, a registered Mem0 provider must supply local config in this shape:

```json
{
  "python_executable": "C:/.../python.exe",
  "working_directory": "C:/.../dennett-agent-orchestrator",
  "mem0_config": {
    "vector_store": {
      "provider": "chroma",
      "config": {
        "path": "C:/.../chroma",
        "collection_name": "phase13-example"
      }
    },
    "embedder": {
      "provider": "fastembed",
      "config": {
        "model": "BAAI/bge-small-en-v1.5"
      }
    },
    "llm": {
      "provider": "ollama",
      "config": {
        "model": "qwen2.5:0.5b-instruct"
      }
    },
    "history_db_path": "C:/.../history.db",
    "version": "v1.1"
  }
}
```

This is local user-owned config, not portable agent JSON.

## Portable Binding Resolution

Portable `memory_bindings[]` still declare intent and requirements.

In Phase 13, Core can now resolve:

- `binding.codex_ref`
- `config.required_capabilities`
- `config.transport_preferences`
- `config.provider_extension.provider`

against the local registry and the selected adapter.

That means the portable binding is no longer documentation-only. It can drive real provider resolution in product code.

## Portable Override Boundary

Phase 13 intentionally keeps the portable Mem0 escape hatch narrow.

Portable bindings may influence Mem0-native memory semantics only through:

- `config.provider_extension.config.mem0_config.graph_store`

Portable bindings may not override local user-owned registration fields such as:

- `python_executable`
- `working_directory`
- `bridge_timeout_ms`
- credentials or account references

Portable bindings may also not override nested auth/account-bearing fields under:

- `mem0_config.llm`
- `mem0_config.embedder`
- `mem0_config.vector_store`
- `mem0_config.graph_store.config`

In the current slice, portable `graph_store` overrides are limited to:

- required `graph_store.provider`
- optional empty `graph_store.config`

This boundary is enforced by both schema validation and runtime validation. The portable agent file may shape only Mem0 graph-store provider selection in the current slice, but it may not replace local launch plumbing or nested provider-auth configuration owned by the user's registered provider.

## What Phase 13 Explicitly Does Not Claim

Phase 13 does not claim:

- runtime-attached memory support inside current Codex `runtime_agent` execution;
- automatic installation or lifecycle ownership of the provider;
- support for providers other than Mem0;
- provider-specific escape-hatch behavior beyond the first Mem0 path;
- MCP-backed memory execution as a separate finished product lane.

Those remain later-phase work.

## Verification Bar Reached

Phase 13 is considered complete only because all of the following are true:

- the code exists in the executable slice;
- targeted tests cover registry, port negotiation, Mem0 adapter behavior, and MemoryService integration;
- full repo tests and build pass in the current workspace;
- a real user-facing CLI CRUD proof path succeeded against a locally registered Mem0 provider;
- TASK-357 proved a disposable local Mem0 namespace cleanup path where target records were deleted and a control namespace survived;
- live semantic `memory-search` remains implemented and covered by automated tests, but this owner doc does not claim a deterministic live search-hit proof on every local machine.

## Architectural Consequence

Dennett now has a real memory subsystem entrypoint.

The system still does not pretend every runtime can consume memory bindings natively during model execution. Instead:

- Core owns provider registration and resolution;
- the memory layer is provider-backed and real;
- runtime-attached memory remains capability-gated and future-facing.

That split is intentional and correct.

## Stage 2 Runtime-Memory Handoff

The first runtime-memory implementation uses the Stage 2 contract in [Memory Bindings](../08-extensions/memory-bindings.md#stage-2-runtime-memory-context-contract).

Phase 13 supplies the first executable provider path for that work, but it does not redefine the product semantics:

- Core must still resolve memory through the local provider registry and internal memory port.
- The first live proof may use Codex plus a locally registered Mem0 provider.
- The normalized `memory_context` contract remains provider-neutral.
- Mem0-specific behavior stays behind the Mem0 adapter or the explicitly allowed Mem0 provider extension.
- Runtime node writes must follow the success-only `node_success_output` write semantics owned by Memory Bindings.
- Direct CLI memory proof remains distinct from runtime graph proof. TASK-333 proof must record whether a Codex graph run actually consumed prompt-rendered `memory_context` and performed provider-backed read/write through Core.

## TASK-333 Stage 2 Proof Boundary

The Stage 2 proof fixture is `examples/agents/valid/stage2-codex-runtime-memory-mem0.json`. It is valid portable agent JSON, but it is executable only after a local Mem0 provider is registered with `codex_ref = primary_memory`.

The proof may show:

- registered Mem0 provider resolution through Core;
- read context rendered into the Codex prompt path;
- success-only node output written back through the Mem0 adapter;
- generated `dist` and local validation gates.

The proof must not be used to claim native App Server memory, provider reliability under load, durable external cleanup, multi-provider support, or release readiness.

## TASK-357 Stage 3 Provider Operations Cleanup Boundary

Stage 3 adds provider-operations cleanup only for the Mem0 registered-provider path. It is implemented as preview plus verified delete, not as backup/restore.

The supported operational claim is narrow:

- a Mem0 provider registration may include `mem0_config.dennett_namespace_id`;
- all adapter read/list/search/update/delete and cleanup paths are constrained by that namespace;
- cleanup preview requires explicit `user_id`, `agent_id`, or `run_id` scope;
- verified delete requires the preview-derived confirmation token;
- verification reports whether the same scoped namespace is empty after deletion.

TASK-357 live proof used disposable local state and storage under `%TEMP%`, two registered Mem0 providers, one shared Chroma path and collection, separate local history DB files under the same storage root, and two namespace IDs. The target namespace contained two records for `task357-cleanup-user`; the control namespace contained one record for the same user. Target preview returned two candidate IDs and token `cleanup:554d8d30df67c0ed204efdfb`; verified delete removed both requested IDs, reported `verified_empty: true`, target list returned `[]`, and control list still returned the control record.

This proves only scoped namespace cleanup for disposable local Mem0 proof data. It does not prove true restore, graph-store cleanup, provider-wide cleanup, cleanup for records outside the configured namespace and explicit scope, multi-provider cleanup, or broad Mem0 reliability.

<a id="russian"></a>
# Phase 13 Mem0-First Native Memory Integration

## Handoff Runtime-Memory Для Stage 2

Следующая runtime-native memory implementation должна использовать Stage 2 contract в [Memory Bindings](../08-extensions/memory-bindings.md#stage-2-runtime-memory-context-contract).

Phase 13 дает первый executable provider path для этой работы, но не переопределяет product semantics:

- Core по-прежнему должен разрешать memory через local provider registry и internal memory port.
- Первый live proof может использовать Codex и локально зарегистрированный Mem0 provider.
- Нормализованный contract `memory_context` остается provider-neutral.
- Mem0-specific behavior остается за Mem0 adapter или явно разрешенным Mem0 provider extension.
- Runtime node writes должны следовать success-only `node_success_output` semantics из Memory Bindings.
- Direct CLI memory proof остается отдельным от runtime-native graph proof, пока Codex graph run реально не использует `memory_context` и provider-backed read/write через Core.

Статус: нормативный owner-doc для реализованного memory-среза Phase 13.

Связанные документы:

- [Capability Gap Lock](../13-capability-gap-lock/phase-12-capability-gap-lock.md)
- [Memory Bindings](../08-extensions/memory-bindings.md)
- [Memory Binding Model Contract](../03-contracts/agent-json/memory-binding-model-contract.md)
- [Runtime Integration Model](../02-architecture/runtime-integration-model.md)

## Назначение

Phase 13 закрывает первый реальный разрыв между portable memory-binding документацией и исполнимым product slice.

Он делает это узко и честно:

- добавляет локальный memory-provider registry;
- добавляет vendor-neutral internal memory port;
- реализует первый provider adapter для Mem0;
- открывает прямые provider-backed memory operations через Core и CLI;
- подтверждает core CLI CRUD path реальными локальными Mem0 round-trips.

Phase 13 не заявляет, что Codex runtime execution уже нативно потребляет memory bindings внутри `runtime_agent`-нод. Это остаётся отдельной более поздней возможностью.

## Что Реализовано

Реализованный memory-срез Phase 13 состоит из:

1. локальной регистрации providers на `SQLite`;
2. Core-owned `MemoryService`, который разрешает portable `codex_ref` через local registry;
3. capability и transport negotiation до использования provider;
4. `Mem0MemoryAdapter` за внутренним memory port;
5. прямых CLI-команд CRUD и search:
   - `memory-provider-register`
   - `memory-provider-list`
   - `memory-provider-show`
   - `memory-write`
   - `memory-read`
   - `memory-search`
   - `memory-list`
   - `memory-update`
   - `memory-delete`
6. реального локального Mem0 proof для register -> write -> read -> update -> list -> delete -> read-after-delete.
7. automated search coverage для Mem0 adapter, `MemoryService` и CLI path.
8. Stage 3 provider-operations cleanup:
   - `memory-cleanup-preview`
   - `memory-cleanup-verified-delete`
   - namespace isolation через локальный `mem0_config.dennett_namespace_id`
   - явные требования user, agent или run scope
   - bounded candidate lists и post-delete verification.

## Локальный Provider Registry

Phase 13 вводит локальную registry-запись для memory providers.

Registry владеет:

- стабильным локальным `provider_id`;
- portable lookup key `codex_ref`;
- `provider_family`;
- transport metadata;
- advertised capability set;
- локальным provider config;
- локальным availability/error status.

Этот registry является локальным operational state. Это не portable truth агента.

## Форма Локального Mem0 Config

В текущем срезе Phase 13 зарегистрированный Mem0 provider должен передавать локальный config в форме:

```json
{
  "python_executable": "C:/.../python.exe",
  "working_directory": "C:/.../dennett-agent-orchestrator",
  "mem0_config": {
    "vector_store": {
      "provider": "chroma",
      "config": {
        "path": "C:/.../chroma",
        "collection_name": "phase13-example"
      }
    },
    "embedder": {
      "provider": "fastembed",
      "config": {
        "model": "BAAI/bge-small-en-v1.5"
      }
    },
    "llm": {
      "provider": "ollama",
      "config": {
        "model": "qwen2.5:0.5b-instruct"
      }
    },
    "history_db_path": "C:/.../history.db",
    "version": "v1.1"
  }
}
```

Это локальный user-owned config, а не portable agent JSON.

## Разрешение Portable Binding

Portable `memory_bindings[]` по-прежнему объявляют intent и требования.

В Phase 13 Core теперь умеет реально разрешать:

- `binding.codex_ref`
- `config.required_capabilities`
- `config.transport_preferences`
- `config.provider_extension.provider`

через local registry и выбранный adapter.

Это означает, что portable binding больше не является только документацией. Он теперь может управлять реальным provider resolution в product code.

## Граница Portable Override

Phase 13 намеренно сохраняет portable Mem0 escape hatch узким.

Portable bindings могут влиять на Mem0-native memory semantics только через:

- `config.provider_extension.config.mem0_config.graph_store`

Portable bindings не могут override local user-owned registration fields, такие как:

- `python_executable`
- `working_directory`
- `bridge_timeout_ms`
- credentials или account references

Portable bindings также не могут override nested auth/account-bearing fields внутри:

- `mem0_config.llm`
- `mem0_config.embedder`
- `mem0_config.vector_store`
- `mem0_config.graph_store.config`

В текущем срезе portable `graph_store` overrides ограничены:

- обязательным `graph_store.provider`
- optional empty `graph_store.config`

Эта boundary enforced и schema validation, и runtime validation. Portable agent file в текущем срезе может формировать только выбор Mem0 graph-store provider, но не может заменять local launch plumbing или nested provider-auth configuration, которыми владеет user registered provider.

## Что Phase 13 Явно Не Заявляет

Phase 13 не заявляет:

- runtime-attached memory support внутри текущего Codex `runtime_agent` execution;
- автоматическую установку provider или ownership его жизненного цикла;
- поддержку providers кроме Mem0;
- provider-specific escape hatch behavior сверх первого Mem0 path;
- завершённую отдельную продуктовую ветку MCP-backed memory execution.

Это остаётся работой следующих фаз.

## Достигнутый Уровень Проверки

Phase 13 считается завершённой только потому, что одновременно выполнено всё следующее:

- код существует в executable slice;
- targeted tests покрывают registry, port negotiation, Mem0 adapter behavior и MemoryService integration;
- полный repo test и build проходят;
- реальный user-facing CLI CRUD proof path, включая read-after-delete, успешно прошёл против локально зарегистрированного Mem0 provider;
- TASK-357 доказал disposable local Mem0 namespace cleanup path, где target records были удалены, а control namespace сохранился;
- live semantic `memory-search` остается implemented и covered by automated tests, но этот owner doc не заявляет deterministic live search-hit proof на каждой local machine.

## Архитектурное Следствие

У Dennett теперь есть реальная входная точка memory subsystem.

Система всё ещё не делает вид, что любой runtime уже может нативно потреблять memory bindings во время model execution. Вместо этого:

- Core владеет provider registration и resolution;
- memory layer реально существует и опирается на provider;
- runtime-attached memory остаётся capability-gated и future-facing.

Это разделение сделано намеренно и является правильным.

## TASK-333 Граница proof для Stage 2

Stage 2 proof fixture: `examples/agents/valid/stage2-codex-runtime-memory-mem0.json`. Этот пример является валидным portable Agent JSON, но запускается только при наличии локальной регистрации Mem0 provider с `codex_ref = primary_memory`.

Proof может показать registered Mem0 provider resolution через Core, prompt-rendered `memory_context` в Codex App Server path и success-only запись node output через Mem0 adapter. Proof не должен утверждать native App Server memory, reliability provider'а под нагрузкой, durable cleanup внешних данных, multi-provider support или release readiness.

## TASK-357 Граница Stage 3 Provider Operations Cleanup

Stage 3 добавляет provider-operations cleanup только для пути Mem0 registered-provider. Он реализован как preview плюс verified delete, а не как backup/restore.

Поддерживаемый operational claim узкий:

- регистрация Mem0 provider может включать `mem0_config.dennett_namespace_id`;
- все adapter read/list/search/update/delete и cleanup paths ограничены этим namespace;
- cleanup preview требует явный `user_id`, `agent_id` или `run_id` scope;
- verified delete требует confirmation token, полученный из preview;
- verification сообщает, пуст ли тот же scoped namespace после deletion.

TASK-357 live proof использовал disposable local state и storage под `%TEMP%`, две зарегистрированные Mem0 providers, один общий Chroma path и collection, отдельные local history DB files под тем же storage root и два namespace IDs. Target namespace содержал две records для `task357-cleanup-user`; control namespace содержал одну record для того же user. Target preview вернул два candidate IDs и token `cleanup:554d8d30df67c0ed204efdfb`; verified delete удалил оба requested IDs, сообщил `verified_empty: true`, target list вернул `[]`, а control list все еще вернул control record.

Это доказывает только scoped namespace cleanup для disposable local Mem0 proof data. Это не доказывает true restore, graph-store cleanup, provider-wide cleanup, cleanup для records вне configured namespace и explicit scope, multi-provider cleanup или broad Mem0 reliability.
