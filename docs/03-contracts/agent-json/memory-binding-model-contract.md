[English](#english) | [Russian](#russian)

<a id="english"></a>
# Memory Binding Model Contract

## Purpose And Ownership

This document owns the portable memory-binding payload used by `memory_bindings` when `kind = runtime_memory`.

It defines:

- the portable intent fields for a memory binding;
- the required capability negotiation model;
- the provider-specific escape hatch;
- the local, user-owned provider-registration boundary;
- the failure semantics when a provider, capability, or transport is unavailable.

This document does not own:

- the provider's native API schema;
- the provider's storage format, graph model, or session model;
- the installation, billing, or lifecycle of the provider itself;
- chat history, resume state, or runtime source selection.

## Core Model

Dennett has one vendor-neutral internal memory layer. A portable memory binding is a declaration of intent against that layer, not a promise that every provider exposes the same API, graph semantics, or memory shape.

The binding must stay provider-neutral by default. A provider name is only allowed through the explicit escape hatch defined below.

## Portable Binding Shape

The top-level `memory_bindings[]` object remains the owner of identity and availability scope. For `kind = runtime_memory`, the portable payload lives in `config` and uses this shape:

- `intent`: required object.
- `required_capabilities`: required array of strings.
- `transport_preferences`: optional object.
- `provider_extension`: optional object.
- `local_notes`: optional string.

### `intent`

- Type: `object`
- Required: yes
- Closed: yes

Allowed fields:

- `summary`: required string. Human-readable description of what the binding is for.
- `labels`: optional array of strings. Portable, low-cardinality tags such as `team`, `project`, `entity`, `profile`, `graph`, `session`, or `retrieval`.

Rules:

- `intent.summary` must describe the binding without naming a provider by default.
- `labels` are descriptive hints, not provider contracts.

### `required_capabilities`

- Type: `array`
- Required: yes

Each item is a portable capability token chosen from the table below.

Rules:

- the array may be empty only when the binding is purely descriptive;
- tokens must be unique within the array;
- if a token is unknown to Core, the binding is not portable by default;
- an unknown token may still be used only when `provider_extension.provider` is present and the selected local provider adapter explicitly documents support for that provider-specific token through its own provider-specific configuration path.

Portable capability tokens:

- `read`
- `write`
- `entity_scoped`
- `user_scoped`
- `group_scoped`
- `session_scoped`
- `graph_context`
- `temporal_index`
- `profile_synthesis`
- `rag_retrieval`
- `infer_extract`
- `versioned_write`
- `mcp_transport`

These tokens describe capability shape, not vendor identity.

### `transport_preferences`

- Type: `object`
- Required: no
- Closed: yes

Allowed fields:

- `preferred`: optional array of strings. Allowed values are `api`, `sdk`, and `mcp`.
- `forbid`: optional array of strings. Allowed values are `api`, `sdk`, and `mcp`.

Rules:

- `mcp` is a transport or connection mode only.
- `mcp` is not a separate top-level memory family.
- `preferred` and `forbid` must not contain duplicates.
- if both `preferred` and `forbid` are present, they must not conflict.

### `provider_extension`

- Type: `object`
- Required: no
- Closed: yes

Allowed fields:

- `provider`: required string when `provider_extension` is present. Local provider family or adapter name such as `mem0`, `zep`, `supermemory`, `graphiti`, or `letta`.
- `transport`: optional string. Allowed values are `api`, `sdk`, and `mcp`.
- `config`: optional object. Provider-specific configuration constrained by the selected provider family.

Rules:

- `provider_extension` is the explicit escape hatch for provider-specific features.
- If `provider_extension` is present, Core may use the named provider only when that provider is registered locally by the user.
- If `provider_extension.transport = mcp`, that only means the chosen provider adapter may use MCP as a connection mode.
- `provider_extension.config` must not override local user-owned registration fields such as local executables, working directories, bridge timeouts, credentials, or account references.
- In the current Mem0-first executable slice, the only portable Mem0 override path is `provider_extension.config.mem0_config.graph_store`.
- That means Mem0 bindings may influence only the graph-memory provider selection in the current slice; they may not replace local launch plumbing such as `python_executable`, `working_directory`, or `bridge_timeout_ms`, and they may not override nested provider-auth or provider-account fields under `llm`, `embedder`, `vector_store`, or `graph_store.config`.
- Future providers may document their own allowed provider-specific config subtree, but each such subtree must be explicit and capability-gated.

### Current Mem0 Override Boundary

For `provider_extension.provider = mem0`, the portable contract currently allows only this shape:

```json
{
  "provider": "mem0",
  "transport": "sdk",
  "config": {
    "mem0_config": {
      "graph_store": {
        "provider": "networkx",
        "config": {}
      }
    }
  }
}
```

Rules:

- `mem0_config.graph_store` is merged into the registered local `mem0_config.graph_store` subtree only.
- No other `mem0_config` key is portable in the current slice.
- Inside `graph_store`, `provider` is required and optional empty `config` is portable in the current slice.
- No top-level local registration field may be overridden from the portable binding.
- If a binding attempts to set `python_executable`, `working_directory`, `bridge_timeout_ms`, credentials, or any other local registration field, Core must reject the binding.
- If a binding attempts to override nested fields under `llm`, `embedder`, or `vector_store`, Core must reject the binding.
- If a binding attempts to add nested keys under `graph_store.config`, Core must reject the binding.

## Local Provider Registry

The actual memory provider is chosen outside the portable agent file.

Core may resolve `codex_ref` and `provider_extension.provider` only against a local, user-owned memory-provider registry. That registry is not portable file truth. It is local configuration owned by the user or workspace, and it may point to:

- a provider adapter;
- credentials or account references;
- transport preferences;
- provider-specific config stored locally.

Dennett must not silently install, own, or replace the user's provider.

If the user has not registered a provider, or if a binding names a provider that is not present locally, the run must fail explicitly.

## Provider Facts And Architectural Consequences

The known providers do not share one universal memory standard. Dennett must adapt to their real shape instead of flattening them into one fiction.

| Provider | Primary surface | MCP role | Dennett consequence |
| --- | --- | --- | --- |
| Mem0 | API/SDK first; entity-scoped memory, infer/extract path, optional graph memory | Narrower connection mode, not the main memory abstraction | Treat MCP as a transport variant under the provider adapter. Do not model Mem0 MCP as a separate memory family. |
| Zep | API/SDK first; memory plus graph/context engine; session, thread, user, and group semantics | Docs MCP is not memory | Keep docs MCP separate from memory bindings. The provider adapter owns the actual memory behavior. |
| Supermemory | API/SDK first; memory, RAG, user profile synthesis; containerTag, customId, and versioning style features | Narrower connection mode | Surface provider-specific config through the escape hatch when these features matter. |
| Graphiti | Graph engine/framework; graph and temporal semantics are central | MCP exists but is experimental | Model graph and temporal capabilities directly in required capabilities and provider extension data. |
| Letta | Full stateful agent platform; memory is native to the runtime | MCP is not the memory abstraction | Treat Letta as a provider adapter family, not as a new top-level memory system. |

## Capability Negotiation

Core must negotiate memory capabilities before launch whenever the binding asks for anything beyond a plain local reference.

Negotiation order:

1. Resolve the local provider registration.
2. Check that the provider adapter supports every required capability.
3. Check that the requested transport is allowed.
4. Apply the provider-specific escape hatch only if it is present and supported.
5. Fail explicitly if any required capability is missing.

Rules:

- Core must not reinterpret a failed capability check as a silent fallback to chat history.
- Core must not invent a substitute provider when the requested one is unavailable, unless a separate user-owned policy explicitly says to do so.
- Core must not treat MCP support as proof of memory support.

## Failure Semantics

Memory binding failures are configuration or launch-time failures, not soft hints.

Required failure cases:

- unknown `codex_ref`;
- unresolved `provider_extension.provider`;
- unsupported required capability;
- unsupported requested transport;
- provider-specific config that the local provider adapter cannot interpret;
- any attempt to use a provider that the user has not registered locally;
- any attempt to rely on MCP as a different memory family.

In all of those cases, Core must fail clearly rather than degrade into hidden chat replay or hidden provider substitution.

## What Core May And May Not Assume

Core may assume:

- one portable binding can resolve to one local provider registration at launch time;
- the provider adapter can expose different capabilities than another provider adapter;
- MCP may be used as a connection mode when the provider adapter allows it;
- a missing capability is a real failure, not an implicit pass.

Core may not assume:

- one universal external memory standard;
- that all providers have the same API shape or memory semantics;
- that MCP is a separate top-level memory family;
- that the user's provider is installed or managed by Dennett;
- that chat history or resume state can be substituted for missing memory.

<a id="russian"></a>
# РљРѕРЅС‚СЂР°РєС‚ РњРѕРґРµР»Рё Memory Binding

## РќР°Р·РЅР°С‡РµРЅРёРµ Р Р’Р»Р°РґРµРЅРёРµ

Р­С‚РѕС‚ РґРѕРєСѓРјРµРЅС‚ РІР»Р°РґРµРµС‚ РїРµСЂРµРЅРѕСЃРёРјС‹Рј payload РґР»СЏ memory-binding, РєРѕРіРґР° `kind = runtime_memory`.

РћРЅ РѕРїСЂРµРґРµР»СЏРµС‚:

- РїРµСЂРµРЅРѕСЃРёРјС‹Рµ РїРѕР»СЏ intent РґР»СЏ memory binding;
- РјРѕРґРµР»СЊ negotiation РїРѕ РІРѕР·РјРѕР¶РЅРѕСЃС‚СЏРј;
- escape hatch РґР»СЏ provider-specific РѕСЃРѕР±РµРЅРЅРѕСЃС‚РµР№;
- РіСЂР°РЅРёС†Сѓ Р»РѕРєР°Р»СЊРЅРѕРіРѕ, РїСЂРёРЅР°РґР»РµР¶Р°С‰РµРіРѕ РїРѕР»СЊР·РѕРІР°С‚РµР»СЋ provider registry;
- СЃРµРјР°РЅС‚РёРєСѓ РѕС‚РєР°Р·Р°, РєРѕРіРґР° provider, capability РёР»Рё transport РЅРµРґРѕСЃС‚СѓРїРЅС‹.

Р­С‚РѕС‚ РґРѕРєСѓРјРµРЅС‚ РЅРµ РІР»Р°РґРµРµС‚:

- РЅР°С‚РёРІРЅРѕР№ API-СЃС…РµРјРѕР№ provider;
- С„РѕСЂРјР°С‚РѕРј С…СЂР°РЅРµРЅРёСЏ provider, graph-РјРѕРґРµР»СЊСЋ РёР»Рё session-РјРѕРґРµР»СЊСЋ;
- СѓСЃС‚Р°РЅРѕРІРєРѕР№, Р±РёР»Р»РёРЅРіРѕРј РёР»Рё Р¶РёР·РЅРµРЅРЅС‹Рј С†РёРєР»РѕРј СЃР°РјРѕРіРѕ provider;
- chat history, resume state РёР»Рё РІС‹Р±РѕСЂРѕРј runtime source.

## Р‘Р°Р·РѕРІР°СЏ РњРѕРґРµР»СЊ Core

РЈ Dennett РµСЃС‚СЊ РѕРґРёРЅ vendor-neutral РІРЅСѓС‚СЂРµРЅРЅРёР№ memory layer. РџРµСЂРµРЅРѕСЃРёРјС‹Р№ memory binding РѕРїРёСЃС‹РІР°РµС‚ РЅР°РјРµСЂРµРЅРёРµ РґР»СЏ СЌС‚РѕРіРѕ СЃР»РѕСЏ, Р° РЅРµ РѕР±РµС‰Р°РµС‚, С‡С‚Рѕ РєР°Р¶РґС‹Р№ provider РёРјРµРµС‚ РѕРґРёРЅР°РєРѕРІС‹Р№ API, graph semantics РёР»Рё С„РѕСЂРјСѓ РїР°РјСЏС‚Рё.

РџРѕ СѓРјРѕР»С‡Р°РЅРёСЋ binding РґРѕР»Р¶РµРЅ РѕСЃС‚Р°РІР°С‚СЊСЃСЏ provider-neutral. РРјСЏ provider РґРѕРїСѓСЃРєР°РµС‚СЃСЏ С‚РѕР»СЊРєРѕ С‡РµСЂРµР· СЏРІРЅС‹Р№ escape hatch, РѕРїРёСЃР°РЅРЅС‹Р№ РЅРёР¶Рµ.

## РџРµСЂРµРЅРѕСЃРёРјР°СЏ Р¤РѕСЂРјР° Binding

Top-level РѕР±СЉРµРєС‚ `memory_bindings[]` РїРѕ-РїСЂРµР¶РЅРµРјСѓ РѕС‚РІРµС‡Р°РµС‚ Р·Р° РёРґРµРЅС‚РёС‡РЅРѕСЃС‚СЊ Рё scope РґРѕСЃС‚СѓРїРЅРѕСЃС‚Рё. Р”Р»СЏ `kind = runtime_memory` РїРµСЂРµРЅРѕСЃРёРјС‹Р№ payload Р¶РёРІРµС‚ РІ `config` Рё РёСЃРїРѕР»СЊР·СѓРµС‚ С‚Р°РєСѓСЋ С„РѕСЂРјСѓ:

- `intent`: РѕР±СЏР·Р°С‚РµР»СЊРЅС‹Р№ РѕР±СЉРµРєС‚.
- `required_capabilities`: РѕР±СЏР·Р°С‚РµР»СЊРЅС‹Р№ РјР°СЃСЃРёРІ СЃС‚СЂРѕРє.
- `transport_preferences`: РЅРµРѕР±СЏР·Р°С‚РµР»СЊРЅС‹Р№ РѕР±СЉРµРєС‚.
- `provider_extension`: РЅРµРѕР±СЏР·Р°С‚РµР»СЊРЅС‹Р№ РѕР±СЉРµРєС‚.
- `local_notes`: РЅРµРѕР±СЏР·Р°С‚РµР»СЊРЅР°СЏ СЃС‚СЂРѕРєР°.

### `intent`

- РўРёРї: `object`
- РћР±СЏР·Р°С‚РµР»СЊРЅРѕРµ: РґР°
- Р—Р°РєСЂС‹С‚С‹Р№ РѕР±СЉРµРєС‚: РґР°

Р”РѕРїСѓСЃС‚РёРјС‹Рµ РїРѕР»СЏ:

- `summary`: РѕР±СЏР·Р°С‚РµР»СЊРЅР°СЏ СЃС‚СЂРѕРєР°. Р§РµР»РѕРІРµРєРѕ-С‡РёС‚Р°РµРјРѕРµ РѕРїРёСЃР°РЅРёРµ С‚РѕРіРѕ, РґР»СЏ С‡РµРіРѕ РЅСѓР¶РµРЅ binding.
- `labels`: РЅРµРѕР±СЏР·Р°С‚РµР»СЊРЅС‹Р№ РјР°СЃСЃРёРІ СЃС‚СЂРѕРє. РџРµСЂРµРЅРѕСЃРёРјС‹Рµ, РЅРёР·РєРѕРєР°СЂРґРёРЅР°Р»СЊРЅС‹Рµ С‚РµРіРё РІСЂРѕРґРµ `team`, `project`, `entity`, `profile`, `graph`, `session` РёР»Рё `retrieval`.

РџСЂР°РІРёР»Р°:

- `intent.summary` РґРѕР»Р¶РµРЅ РѕРїРёСЃС‹РІР°С‚СЊ binding Р±РµР· РёРјРµРЅРё provider РїРѕ СѓРјРѕР»С‡Р°РЅРёСЋ.
- `labels` СЏРІР»СЏСЋС‚СЃСЏ РѕРїРёСЃР°С‚РµР»СЊРЅС‹РјРё РїРѕРґСЃРєР°Р·РєР°РјРё, Р° РЅРµ provider contract.

### `required_capabilities`

- РўРёРї: `array`
- РћР±СЏР·Р°С‚РµР»СЊРЅРѕРµ: РґР°

РљР°Р¶РґС‹Р№ СЌР»РµРјРµРЅС‚ РґРѕР»Р¶РµРЅ Р±С‹С‚СЊ РїРµСЂРµРЅРѕСЃРёРјС‹Рј token capability РёР· С‚Р°Р±Р»РёС†С‹ РЅРёР¶Рµ.

РџСЂР°РІРёР»Р°:

- РјР°СЃСЃРёРІ РјРѕР¶РµС‚ Р±С‹С‚СЊ РїСѓСЃС‚С‹Рј С‚РѕР»СЊРєРѕ РµСЃР»Рё binding РЅРѕСЃРёС‚ СЃСѓРіСѓР±Рѕ РѕРїРёСЃР°С‚РµР»СЊРЅС‹Р№ С…Р°СЂР°РєС‚РµСЂ;
- С‚РѕРєРµРЅС‹ РґРѕР»Р¶РЅС‹ Р±С‹С‚СЊ СѓРЅРёРєР°Р»СЊРЅС‹РјРё;
- РµСЃР»Рё token РЅРµРёР·РІРµСЃС‚РµРЅ Core, binding РїРѕ СѓРјРѕР»С‡Р°РЅРёСЋ РЅРµ СЏРІР»СЏРµС‚СЃСЏ РїРµСЂРµРЅРѕСЃРёРјС‹Рј;
- РЅРµРёР·РІРµСЃС‚РЅС‹Р№ token РјРѕР¶РЅРѕ РёСЃРїРѕР»СЊР·РѕРІР°С‚СЊ С‚РѕР»СЊРєРѕ РµСЃР»Рё РїСЂРёСЃСѓС‚СЃС‚РІСѓРµС‚ `provider_extension.provider` Рё РІС‹Р±СЂР°РЅРЅС‹Р№ Р»РѕРєР°Р»СЊРЅС‹Р№ provider adapter СЏРІРЅРѕ РґРѕРєСѓРјРµРЅС‚РёСЂСѓРµС‚ РїРѕРґРґРµСЂР¶РєСѓ СЌС‚РѕРіРѕ provider-specific token С‡РµСЂРµР· СЃРІРѕР№ provider-specific configuration path.

РџРµСЂРµРЅРѕСЃРёРјС‹Рµ capability tokens:

- `read`
- `write`
- `entity_scoped`
- `user_scoped`
- `group_scoped`
- `session_scoped`
- `graph_context`
- `temporal_index`
- `profile_synthesis`
- `rag_retrieval`
- `infer_extract`
- `versioned_write`
- `mcp_transport`

Р­С‚Рё С‚РѕРєРµРЅС‹ РѕРїРёСЃС‹РІР°СЋС‚ С„РѕСЂРјСѓ capability, Р° РЅРµ identity vendor.

### `transport_preferences`

- РўРёРї: `object`
- РћР±СЏР·Р°С‚РµР»СЊРЅРѕРµ: РЅРµС‚
- Р—Р°РєСЂС‹С‚С‹Р№ РѕР±СЉРµРєС‚: РґР°

Р”РѕРїСѓСЃС‚РёРјС‹Рµ РїРѕР»СЏ:

- `preferred`: РЅРµРѕР±СЏР·Р°С‚РµР»СЊРЅС‹Р№ РјР°СЃСЃРёРІ СЃС‚СЂРѕРє. Р”РѕРїСѓСЃС‚РёРјС‹Рµ Р·РЅР°С‡РµРЅРёСЏ: `api`, `sdk`, `mcp`.
- `forbid`: РЅРµРѕР±СЏР·Р°С‚РµР»СЊРЅС‹Р№ РјР°СЃСЃРёРІ СЃС‚СЂРѕРє. Р”РѕРїСѓСЃС‚РёРјС‹Рµ Р·РЅР°С‡РµРЅРёСЏ: `api`, `sdk`, `mcp`.

РџСЂР°РІРёР»Р°:

- `mcp` СЏРІР»СЏРµС‚СЃСЏ С‚РѕР»СЊРєРѕ transport/connection mode.
- `mcp` РЅРµ СЏРІР»СЏРµС‚СЃСЏ РѕС‚РґРµР»СЊРЅРѕР№ РІРµСЂС…РЅРµСѓСЂРѕРІРЅРµРІРѕР№ memory family.
- `preferred` Рё `forbid` РЅРµ РґРѕР»Р¶РЅС‹ СЃРѕРґРµСЂР¶Р°С‚СЊ РґСѓР±Р»РёРєР°С‚С‹.
- РµСЃР»Рё РїСЂРёСЃСѓС‚СЃС‚РІСѓСЋС‚ Рё `preferred`, Рё `forbid`, РѕРЅРё РЅРµ РґРѕР»Р¶РЅС‹ РєРѕРЅС„Р»РёРєС‚РѕРІР°С‚СЊ.

### `provider_extension`

- РўРёРї: `object`
- РћР±СЏР·Р°С‚РµР»СЊРЅРѕРµ: РЅРµС‚
- Р—Р°РєСЂС‹С‚С‹Р№ РѕР±СЉРµРєС‚: РґР°

Р”РѕРїСѓСЃС‚РёРјС‹Рµ РїРѕР»СЏ:

- `provider`: РѕР±СЏР·Р°С‚РµР»СЊРЅР°СЏ СЃС‚СЂРѕРєР°, РµСЃР»Рё `provider_extension` РїСЂРёСЃСѓС‚СЃС‚РІСѓРµС‚. Р›РѕРєР°Р»СЊРЅРѕРµ РёРјСЏ family provider РёР»Рё adapter, РЅР°РїСЂРёРјРµСЂ `mem0`, `zep`, `supermemory`, `graphiti` РёР»Рё `letta`.
- `transport`: РЅРµРѕР±СЏР·Р°С‚РµР»СЊРЅР°СЏ СЃС‚СЂРѕРєР°. Р”РѕРїСѓСЃС‚РёРјС‹Рµ Р·РЅР°С‡РµРЅРёСЏ: `api`, `sdk`, `mcp`.
- `config`: РЅРµРѕР±СЏР·Р°С‚РµР»СЊРЅС‹Р№ object. Opaque provider-specific configuration.

РџСЂР°РІРёР»Р°:

- `provider_extension` СЏРІР»СЏРµС‚СЃСЏ СЏРІРЅС‹Рј escape hatch РґР»СЏ provider-specific РѕСЃРѕР±РµРЅРЅРѕСЃС‚РµР№.
- Р•СЃР»Рё `provider_extension` РїСЂРёСЃСѓС‚СЃС‚РІСѓРµС‚, Core РјРѕР¶РµС‚ РёСЃРїРѕР»СЊР·РѕРІР°С‚СЊ СѓРєР°Р·Р°РЅРЅС‹Р№ provider С‚РѕР»СЊРєРѕ С‚РѕРіРґР°, РєРѕРіРґР° СЌС‚РѕС‚ provider Р·Р°СЂРµРіРёСЃС‚СЂРёСЂРѕРІР°РЅ Р»РѕРєР°Р»СЊРЅРѕ РїРѕР»СЊР·РѕРІР°С‚РµР»РµРј.
- Р•СЃР»Рё `provider_extension.transport = mcp`, СЌС‚Рѕ РѕР·РЅР°С‡Р°РµС‚ С‚РѕР»СЊРєРѕ С‚Рѕ, С‡С‚Рѕ РІС‹Р±СЂР°РЅРЅС‹Р№ provider adapter РјРѕР¶РµС‚ РёСЃРїРѕР»СЊР·РѕРІР°С‚СЊ MCP РєР°Рє connection mode.
- Provider-specific config РјРѕР¶РµС‚ РІРєР»СЋС‡Р°С‚СЊ provider-only РїРѕР»СЏ РІСЂРѕРґРµ entity-scoped memory controls, graph toggles, session semantics, container tags, custom ids РёР»Рё versioning rules.

## Р›РѕРєР°Р»СЊРЅС‹Р№ Provider Registry

Р РµР°Р»СЊРЅС‹Р№ memory provider РІС‹Р±РёСЂР°РµС‚СЃСЏ РІРЅРµ portable agent file.

Core РјРѕР¶РµС‚ СЂР°Р·СЂРµС€Р°С‚СЊ `codex_ref` Рё `provider_extension.provider` С‚РѕР»СЊРєРѕ С‡РµСЂРµР· Р»РѕРєР°Р»СЊРЅС‹Р№ provider registry, РїСЂРёРЅР°РґР»РµР¶Р°С‰РёР№ РїРѕР»СЊР·РѕРІР°С‚РµР»СЋ. Р­С‚РѕС‚ registry РЅРµ СЏРІР»СЏРµС‚СЃСЏ РїРµСЂРµРЅРѕСЃРёРјРѕР№ file truth. Р­С‚Рѕ Р»РѕРєР°Р»СЊРЅР°СЏ РєРѕРЅС„РёРіСѓСЂР°С†РёСЏ, РєРѕС‚РѕСЂРѕР№ РІР»Р°РґРµРµС‚ РїРѕР»СЊР·РѕРІР°С‚РµР»СЊ РёР»Рё workspace, Рё РѕРЅР° РјРѕР¶РµС‚ СѓРєР°Р·С‹РІР°С‚СЊ РЅР°:

- provider adapter;
- credentials РёР»Рё account references;
- transport preferences;
- provider-specific config, С…СЂР°РЅРёРјС‹Р№ Р»РѕРєР°Р»СЊРЅРѕ.

Dennett РЅРµ РґРѕР»Р¶РµРЅ silently install, own РёР»Рё replace user's provider.

Р•СЃР»Рё РїРѕР»СЊР·РѕРІР°С‚РµР»СЊ РЅРµ Р·Р°СЂРµРіРёСЃС‚СЂРёСЂРѕРІР°Р» provider РёР»Рё binding СЃСЃС‹Р»Р°РµС‚СЃСЏ РЅР° provider, РєРѕС‚РѕСЂРѕРіРѕ РЅРµС‚ Р»РѕРєР°Р»СЊРЅРѕ, run РґРѕР»Р¶РµРЅ Р·Р°РІРµСЂС€РёС‚СЊСЃСЏ СЏРІРЅРѕР№ РѕС€РёР±РєРѕР№.

## Provider Facts Р РђСЂС…РёС‚РµРєС‚СѓСЂРЅС‹Рµ РЎР»РµРґСЃС‚РІРёСЏ

РР·РІРµСЃС‚РЅС‹Рµ provider РЅРµ СЂР°Р·РґРµР»СЏСЋС‚ РѕРґРёРЅ СѓРЅРёРІРµСЂСЃР°Р»СЊРЅС‹Р№ memory standard. Dennett РґРѕР»Р¶РµРЅ Р°РґР°РїС‚РёСЂРѕРІР°С‚СЊСЃСЏ Рє РёС… СЂРµР°Р»СЊРЅРѕР№ С„РѕСЂРјРµ, Р° РЅРµ СЃС…Р»РѕРїС‹РІР°С‚СЊ РёС… РІ РѕРґРЅСѓ С„РёРєС†РёСЋ.

| Provider | РћСЃРЅРѕРІРЅР°СЏ РїРѕРІРµСЂС…РЅРѕСЃС‚СЊ | Р РѕР»СЊ MCP | РЎР»РµРґСЃС‚РІРёРµ РґР»СЏ Dennett |
| --- | --- | --- | --- |
| Mem0 | API/SDK first; entity-scoped memory, infer/extract path, optional graph memory | РЈР·РєРёР№ connection mode, РЅРµ РѕСЃРЅРѕРІРЅР°СЏ memory abstraction | Р Р°СЃСЃРјР°С‚СЂРёРІР°С‚СЊ MCP РєР°Рє transport variant РІРЅСѓС‚СЂРё provider adapter. РќРµ РјРѕРґРµР»РёСЂРѕРІР°С‚СЊ Mem0 MCP РєР°Рє РѕС‚РґРµР»СЊРЅСѓСЋ memory family. |
| Zep | API/SDK first; memory РїР»СЋСЃ graph/context engine; session, thread, user Рё group semantics | Docs MCP РЅРµ СЏРІР»СЏРµС‚СЃСЏ memory | Р”РµСЂР¶Р°С‚СЊ docs MCP РѕС‚РґРµР»СЊРЅРѕ РѕС‚ memory bindings. Р РµР°Р»СЊРЅРѕРµ memory behavior РїСЂРёРЅР°РґР»РµР¶РёС‚ provider adapter. |
| Supermemory | API/SDK first; memory, RAG, user profile synthesis; features РІ СЃС‚РёР»Рµ containerTag, customId Рё versioning | РЈР·РєРёР№ connection mode | РџРµСЂРµРґР°РІР°С‚СЊ provider-specific config С‡РµСЂРµР· escape hatch, РєРѕРіРґР° СЌС‚Рё features РІР°Р¶РЅС‹. |
| Graphiti | Graph engine/framework; graph Рё temporal semantics РЅР°С…РѕРґСЏС‚СЃСЏ РІ С†РµРЅС‚СЂРµ | MCP СЃСѓС‰РµСЃС‚РІСѓРµС‚, РЅРѕ experimental | РњРѕРґРµР»РёСЂРѕРІР°С‚СЊ graph Рё temporal capabilities РЅР°РїСЂСЏРјСѓСЋ С‡РµСЂРµР· required capabilities Рё provider extension data. |
| Letta | Full stateful agent platform; memory native to runtime | MCP РЅРµ СЏРІР»СЏРµС‚СЃСЏ memory abstraction | Р Р°СЃСЃРјР°С‚СЂРёРІР°С‚СЊ Letta РєР°Рє family provider adapter, Р° РЅРµ РєР°Рє РЅРѕРІСѓСЋ РІРµСЂС…РЅРµСѓСЂРѕРІРЅРµРІСѓСЋ memory system. |

## Negotiation РџРѕ Р’РѕР·РјРѕР¶РЅРѕСЃС‚СЏРј

Core РґРѕР»Р¶РµРЅ РІС‹РїРѕР»РЅСЏС‚СЊ negotiation РїРѕ memory capabilities РґРѕ Р·Р°РїСѓСЃРєР°, РєРѕРіРґР° binding С‚СЂРµР±СѓРµС‚ С‡РµРіРѕ-С‚Рѕ Р±РѕР»СЊС€РµРіРѕ, С‡РµРј РїСЂРѕСЃС‚РѕР№ Р»РѕРєР°Р»СЊРЅС‹Р№ reference.

РџРѕСЂСЏРґРѕРє negotiation:

1. Р Р°Р·СЂРµС€РёС‚СЊ Р»РѕРєР°Р»СЊРЅС‹Р№ provider registration.
2. РџСЂРѕРІРµСЂРёС‚СЊ, С‡С‚Рѕ provider adapter РїРѕРґРґРµСЂР¶РёРІР°РµС‚ РєР°Р¶РґСѓСЋ required capability.
3. РџСЂРѕРІРµСЂРёС‚СЊ, С‡С‚Рѕ Р·Р°РїСЂРѕС€РµРЅРЅС‹Р№ transport РґРѕРїСѓСЃС‚РёРј.
4. РџСЂРёРјРµРЅРёС‚СЊ provider-specific escape hatch С‚РѕР»СЊРєРѕ РµСЃР»Рё РѕРЅ РїСЂРёСЃСѓС‚СЃС‚РІСѓРµС‚ Рё РїРѕРґРґРµСЂР¶РёРІР°РµС‚СЃСЏ.
5. РЇРІРЅРѕ Р·Р°РІРµСЂС€РёС‚СЊСЃСЏ РѕС€РёР±РєРѕР№, РµСЃР»Рё С…РѕС‚СЏ Р±С‹ РѕРґРЅР° required capability РѕС‚СЃСѓС‚СЃС‚РІСѓРµС‚.

РџСЂР°РІРёР»Р°:

- Core РЅРµ РґРѕР»Р¶РµРЅ РїСЂРµРІСЂР°С‰Р°С‚СЊ РїСЂРѕРІР°Р» capability check РІ silent fallback РЅР° chat history.
- Core РЅРµ РґРѕР»Р¶РµРЅ invent substitute provider, РєРѕРіРґР° Р·Р°РїСЂРѕС€РµРЅРЅС‹Р№ provider РЅРµРґРѕСЃС‚СѓРїРµРЅ, РµСЃР»Рё С‚РѕР»СЊРєРѕ РѕС‚РґРµР»СЊРЅР°СЏ РїРѕР»СЊР·РѕРІР°С‚РµР»СЊСЃРєР°СЏ policy РЅРµ СЂР°Р·СЂРµС€Р°РµС‚ СЌС‚Рѕ СЏРІРЅРѕ.
- Core РЅРµ РґРѕР»Р¶РµРЅ СЃС‡РёС‚Р°С‚СЊ РїРѕРґРґРµСЂР¶РєСѓ MCP РґРѕРєР°Р·Р°С‚РµР»СЊСЃС‚РІРѕРј РїРѕРґРґРµСЂР¶РєРё memory.

## РЎРµРјР°РЅС‚РёРєР° РћС‚РєР°Р·Р°

РћС€РёР±РєРё memory binding СЏРІР»СЏСЋС‚СЃСЏ configuration РёР»Рё launch-time РѕС€РёР±РєР°РјРё, Р° РЅРµ РјСЏРіРєРёРјРё РїРѕРґСЃРєР°Р·РєР°РјРё.

РћР±СЏР·Р°С‚РµР»СЊРЅС‹Рµ СЃР»СѓС‡Р°Рё РѕС€РёР±РєРё:

- РЅРµРёР·РІРµСЃС‚РЅС‹Р№ `codex_ref`;
- РЅРµСЂР°Р·СЂРµС€С‘РЅРЅС‹Р№ `provider_extension.provider`;
- РЅРµРїРѕРґРґРµСЂР¶РёРІР°РµРјР°СЏ required capability;
- РЅРµРїРѕРґРґРµСЂР¶РёРІР°РµРјС‹Р№ requested transport;
- provider-specific config, РєРѕС‚РѕСЂС‹Р№ Р»РѕРєР°Р»СЊРЅС‹Р№ provider adapter РЅРµ РјРѕР¶РµС‚ РёРЅС‚РµСЂРїСЂРµС‚РёСЂРѕРІР°С‚СЊ;
- Р»СЋР±Р°СЏ РїРѕРїС‹С‚РєР° РёСЃРїРѕР»СЊР·РѕРІР°С‚СЊ provider, РєРѕС‚РѕСЂС‹Р№ РїРѕР»СЊР·РѕРІР°С‚РµР»СЊ РЅРµ Р·Р°СЂРµРіРёСЃС‚СЂРёСЂРѕРІР°Р» Р»РѕРєР°Р»СЊРЅРѕ;
- Р»СЋР±Р°СЏ РїРѕРїС‹С‚РєР° РёСЃРїРѕР»СЊР·РѕРІР°С‚СЊ MCP РєР°Рє РґСЂСѓРіСѓСЋ memory family.

Р’Рѕ РІСЃРµС… СЌС‚РёС… СЃР»СѓС‡Р°СЏС… Core РґРѕР»Р¶РµРЅ Р·Р°РІРµСЂС€Р°С‚СЊСЃСЏ СЏРІРЅРѕ, Р° РЅРµ РґРµРіСЂР°РґРёСЂРѕРІР°С‚СЊ РІ hidden chat replay РёР»Рё hidden provider substitution.

## Р§С‚Рѕ Core РњРѕР¶РµС‚ Р РќРµ РњРѕР¶РµС‚ РџСЂРµРґРїРѕР»Р°РіР°МЃС‚СЊ

Core РјРѕР¶РµС‚ РїСЂРµРґРїРѕР»Р°РіР°С‚СЊ:

- РѕРґРёРЅ РїРµСЂРµРЅРѕСЃРёРјС‹Р№ binding РјРѕР¶РµС‚ СЂР°Р·СЂРµС€Р°С‚СЊСЃСЏ РІ РѕРґРЅСѓ Р»РѕРєР°Р»СЊРЅСѓСЋ provider registration РЅР° СЃС‚Р°СЂС‚Рµ;
- СЂР°Р·РЅС‹Рµ provider adapter РјРѕРіСѓС‚ РѕС‚РєСЂС‹РІР°С‚СЊ СЂР°Р·РЅС‹Рµ capabilities;
- MCP РјРѕР¶РµС‚ РёСЃРїРѕР»СЊР·РѕРІР°С‚СЊСЃСЏ РєР°Рє connection mode, РµСЃР»Рё СЌС‚Рѕ СЂР°Р·СЂРµС€Р°РµС‚ provider adapter;
- РѕС‚СЃСѓС‚СЃС‚РІРёРµ capability СЏРІР»СЏРµС‚СЃСЏ СЂРµР°Р»СЊРЅРѕР№ РѕС€РёР±РєРѕР№, Р° РЅРµ РЅРµСЏРІРЅС‹Рј pass.

Core РЅРµ РјРѕР¶РµС‚ РїСЂРµРґРїРѕР»Р°РіР°С‚СЊ:

- РѕРґРёРЅ СѓРЅРёРІРµСЂСЃР°Р»СЊРЅС‹Р№ РІРЅРµС€РЅРёР№ memory standard;
- С‡С‚Рѕ Сѓ РІСЃРµС… provider РѕРґРёРЅР°РєРѕРІР°СЏ API shape РёР»Рё memory semantics;
- С‡С‚Рѕ MCP СЏРІР»СЏРµС‚СЃСЏ РѕС‚РґРµР»СЊРЅРѕР№ top-level memory family;
- С‡С‚Рѕ provider РїРѕР»СЊР·РѕРІР°С‚РµР»СЏ СѓР¶Рµ СѓСЃС‚Р°РЅРѕРІР»РµРЅ РёР»Рё СѓРїСЂР°РІР»СЏРµС‚СЃСЏ Dennett;
- С‡С‚Рѕ chat history РёР»Рё resume state РјРѕР¶РЅРѕ РїРѕРґСЃС‚Р°РІРёС‚СЊ РІРјРµСЃС‚Рѕ РѕС‚СЃСѓС‚СЃС‚РІСѓСЋС‰РµР№ memory.
