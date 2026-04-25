[English](#english) | [Р СѓСЃСЃРєРёР№](#russian)

<a id="english"></a>
# Valid Patterns

Status: non-normative illustrative patterns.
Owns: nothing. Each pattern below is subordinate to its linked owner docs.

Section map:

- [Examples index](./README.md)
- [Canonical agent JSON example](./canonical-agent-json-example.md)
- [Invalid patterns](./invalid-patterns.md)
- [Interaction sequences](./interaction-sequences.md)

## 1. Compose Input From Ordered Parts

```json
{
  "input": {
    "parts": [
      { "type": "text", "text": "Task:\n" },
      { "type": "ref", "ref": "params.task" },
      { "type": "text", "text": "\nPrior output:\n" },
      { "type": "ref", "ref": "node.prepare.text" }
    ]
  }
}
```

Why this is valid:

- It uses only the two allowed part types: `text` and `ref`.
- It reads only from allowed namespaces.
- It keeps the data flow explicit instead of hiding it in prompt templates.

Owner docs: [nodes-and-edges-contract.md](../03-contracts/agent-json/nodes-and-edges-contract.md), [dataflow-and-input-resolution.md](../04-execution/dataflow-and-input-resolution.md)

## 2. Let Successful JSON Output Update `vars`, Then Use Edges Only For Control Flow

```json
{
  "output": {
    "mode": "json",
    "schema": {
      "type": "object",
      "properties": {
        "status": { "type": "string" }
      }
    }
  },
  "edges": [
    {
      "from": "triage",
      "to": "handoff",
      "condition": { "code": "vars.status == 'ready'" }
    }
  ]
}
```

Why this is valid:

- A successful top-level JSON object that satisfies the declared schema may update `vars`.
- The edge reads `vars.status` using documented field access, but does not transport payload data itself.
- If the JSON output is invalid or the node fails, no hidden `vars` write happens.

Owner docs: [nodes-and-edges-contract.md](../03-contracts/agent-json/nodes-and-edges-contract.md), [graph-execution.md](../04-execution/graph-execution.md), [outputs-outcomes-and-final-response.md](../04-execution/outputs-outcomes-and-final-response.md)

## 3. Keep Comments And Built-In User Chat As Two Explicit Channels

```json
{
  "interaction": {
    "comments": {
      "enabled": true,
      "target_node_ids": ["triage"]
    },
    "user_mcp": {
      "enabled": true,
      "server_name": "orchestrator.user_chat"
    }
  }
}
```

Why this is valid:

- Comments remain free-form run-time input for the active target node.
- Built-in user chat remains a distinct MCP channel for explicit prompt/reply flows.
- A pending prompt does not erase the generic comment channel by itself.

Owner docs: [interaction-and-chat-contract.md](../03-contracts/agent-json/interaction-and-chat-contract.md), [orchestrator-user-chat-mcp-contract.md](../03-contracts/orchestrator-user-chat-mcp-contract.md), [live-run-interaction.md](../06-interaction/live-run-interaction.md), [presentation-rules.md](../06-interaction/presentation-rules.md)

## 4. Narrow Runtime Sources Without Embedding Secrets

```json
{
  "runtime_sources": [
    { "id": "primary", "runtime_adapter": "codex", "source_ref": "account:main" },
    { "id": "backup", "runtime_adapter": "codex", "source_ref": "account:backup" }
  ],
  "nodes": [
    {
      "id": "triage",
      "kind": "runtime_agent",
      "runtime_adapter": "codex",
      "runtime_source_policy": "prefer_first",
      "runtime_source_ids": ["primary", "backup"]
    }
  ]
}
```

Why this is valid:

- `source_ref` stays an opaque handle, not a raw credential.
- The node narrows launch choice inside one adapter family.
- Source selection remains a Core concern before the adapter call.

Owner docs: [top-level-and-bindings-contract.md](../03-contracts/agent-json/top-level-and-bindings-contract.md), [nodes-and-edges-contract.md](../03-contracts/agent-json/nodes-and-edges-contract.md), [runtime-sources.md](../08-extensions/runtime-sources.md), [runtime-adapter-contract.md](../03-contracts/runtime-adapter-contract.md)

## 5. Make Memory Availability Explicit Instead Of Smuggling It Through Chat

```json
{
  "memory_bindings": [
    {
      "id": "project_memory",
      "kind": "runtime_memory",
      "codex_ref": "memory://team/project",
      "scope": "node",
      "config": {
        "intent": {
          "summary": "Project memory for investigation work.",
          "labels": ["project", "retrieval"]
        },
        "required_capabilities": ["read", "write"]
      }
    }
  ],
  "nodes": [
    {
      "id": "investigate",
      "kind": "runtime_agent",
      "memory_ids": ["project_memory"]
    }
  ]
}
```

Why this is valid:

- The binding is declared explicitly at the top level.
- The binding carries the portable memory payload in `config` instead of relying on an implicit provider default.
- The node opts into a `scope = node` binding explicitly with `memory_ids`.
- Chat and resume remain separate from long-term memory.

Owner docs: [top-level-and-bindings-contract.md](../03-contracts/agent-json/top-level-and-bindings-contract.md), [memory-bindings.md](../08-extensions/memory-bindings.md), [chat-and-resume.md](../05-state/chat-and-resume.md)

## 6. Keep Drafts, Live Revisions, And Existing Chats On Separate Axes

Example flow:

1. Chat `C1` starts from resolved live revision `R7`.
2. A user creates draft `R8` and later deploys it.
3. Future opens and future event dispatches may resolve to `R8`.
4. Chat `C1` still resumes against `R7`, not against the newer live revision.

Why this is valid:

- Drafts do not silently replace the current live revision.
- Existing chats and resumable runs stay bound to the revision they started with.
- Future events resolve the current live revision at dispatch time.

Owner docs: [agent-registry.md](../07-lifecycle/agent-registry.md), [draft-live-deploy.md](../07-lifecycle/draft-live-deploy.md), [events-and-triggers.md](../07-lifecycle/events-and-triggers.md), [versioning-axes.md](../07-lifecycle/versioning-axes.md), [chat-and-resume.md](../05-state/chat-and-resume.md)

## 7. Let The Builder Work Through Drafts Rather Than Bypassing Lifecycle

Example flow:

1. The builder inspects permitted files and produces a candidate revision.
2. The candidate is stored as a draft and validated.
3. The draft is run or compared.
4. Deploy happens only as an explicit later action.

Why this is valid:

- The builder remains an agent inside the same architecture.
- File truth, registry truth, and deploy semantics stay intact.
- Manual editing and review remain first-class.

Owner docs: [builder-agent.md](../08-extensions/builder-agent.md), [draft-live-deploy.md](../07-lifecycle/draft-live-deploy.md), [agent-registry.md](../07-lifecycle/agent-registry.md)

<a id="russian"></a>
# Р’Р°Р»РёРґРЅС‹Рµ РџР°С‚С‚РµСЂРЅС‹

РЎС‚Р°С‚СѓСЃ: РЅРµРЅРѕСЂРјР°С‚РёРІРЅС‹Рµ РёР»Р»СЋСЃС‚СЂР°С‚РёРІРЅС‹Рµ РїР°С‚С‚РµСЂРЅС‹.
Р’Р»Р°РґРµРЅРёРµ: РЅРёС‡РµРј. РљР°Р¶РґС‹Р№ РїР°С‚С‚РµСЂРЅ РЅРёР¶Рµ РїРѕРґС‡РёРЅРµРЅ СЃРІРѕРёРј РїСЂРѕС„РёР»СЊРЅС‹Рј РґРѕРєСѓРјРµРЅС‚Р°Рј-РІР»Р°РґРµР»СЊС†Р°Рј.

РљР°СЂС‚Р° СЂР°Р·РґРµР»Р°:

- [РРЅРґРµРєСЃ РїСЂРёРјРµСЂРѕРІ](./README.md)
- [РљР°РЅРѕРЅРёС‡РµСЃРєРёР№ РїСЂРёРјРµСЂ agent JSON](./canonical-agent-json-example.md)
- [РќРµРІР°Р»РёРґРЅС‹Рµ РїР°С‚С‚РµСЂРЅС‹](./invalid-patterns.md)
- [РЎС†РµРЅР°СЂРёРё РІР·Р°РёРјРѕРґРµР№СЃС‚РІРёСЏ](./interaction-sequences.md)

## 1. РЎРѕР±РёСЂР°С‚СЊ Р’С…РѕРґ РР· РЈРїРѕСЂСЏРґРѕС‡РµРЅРЅС‹С… Р§Р°СЃС‚РµР№

```json
{
  "input": {
    "parts": [
      { "type": "text", "text": "Task:\n" },
      { "type": "ref", "ref": "params.task" },
      { "type": "text", "text": "\nPrior output:\n" },
      { "type": "ref", "ref": "node.prepare.text" }
    ]
  }
}
```

РџРѕС‡РµРјСѓ СЌС‚Рѕ РІР°Р»РёРґРЅРѕ:

- Р—РґРµСЃСЊ РёСЃРїРѕР»СЊР·СѓСЋС‚СЃСЏ С‚РѕР»СЊРєРѕ РґРІР° СЂР°Р·СЂРµС€РµРЅРЅС‹С… С‚РёРїР° С‡Р°СЃС‚РµР№: `text` Рё `ref`.
- Р§С‚РµРЅРёРµ РёРґРµС‚ С‚РѕР»СЊРєРѕ РёР· СЂР°Р·СЂРµС€РµРЅРЅС‹С… namespaces.
- РџРѕС‚РѕРє РґР°РЅРЅС‹С… РѕСЃС‚Р°РµС‚СЃСЏ СЏРІРЅС‹Рј, Р° РЅРµ СЃРєСЂС‹С‚С‹Рј РІРЅСѓС‚СЂРё prompt templates.

Р”РѕРєСѓРјРµРЅС‚С‹-РІР»Р°РґРµР»СЊС†С‹: [nodes-and-edges-contract.md](../03-contracts/agent-json/nodes-and-edges-contract.md), [dataflow-and-input-resolution.md](../04-execution/dataflow-and-input-resolution.md)

## 2. Р”Р°РІР°С‚СЊ РЈСЃРїРµС€РЅРѕРјСѓ JSON-Output РћР±РЅРѕРІР»СЏС‚СЊ `vars`, Рђ Edges РСЃРїРѕР»СЊР·РѕРІР°С‚СЊ РўРѕР»СЊРєРѕ Р”Р»СЏ Control Flow

```json
{
  "output": {
    "mode": "json",
    "schema": {
      "type": "object",
      "properties": {
        "status": { "type": "string" }
      }
    }
  },
  "edges": [
    {
      "from": "triage",
      "to": "handoff",
      "condition": { "code": "vars.status == 'ready'" }
    }
  ]
}
```

РџРѕС‡РµРјСѓ СЌС‚Рѕ РІР°Р»РёРґРЅРѕ:

- РЈСЃРїРµС€РЅС‹Р№ top-level JSON object, РєРѕС‚РѕСЂС‹Р№ СЃРѕРѕС‚РІРµС‚СЃС‚РІСѓРµС‚ РѕР±СЉСЏРІР»РµРЅРЅРѕР№ schema, РјРѕР¶РµС‚ РѕР±РЅРѕРІР»СЏС‚СЊ `vars`.
- Edge С‡РёС‚Р°РµС‚ `vars`, РЅРѕ СЃР°Рј РЅРµ РїРµСЂРµРЅРѕСЃРёС‚ payload.
- Р•СЃР»Рё JSON-output РЅРµРІР°Р»РёРґРµРЅ РёР»Рё РЅРѕРґР° СѓРїР°Р»Р°, СЃРєСЂС‹С‚РѕР№ Р·Р°РїРёСЃРё РІ `vars` РЅРµ РїСЂРѕРёСЃС…РѕРґРёС‚.

Р”РѕРєСѓРјРµРЅС‚С‹-РІР»Р°РґРµР»СЊС†С‹: [nodes-and-edges-contract.md](../03-contracts/agent-json/nodes-and-edges-contract.md), [graph-execution.md](../04-execution/graph-execution.md), [outputs-outcomes-and-final-response.md](../04-execution/outputs-outcomes-and-final-response.md)

## 3. Р”РµСЂР¶Р°С‚СЊ РљРѕРјРјРµРЅС‚Р°СЂРёРё Р Built-In User Chat РљР°Рє Р”РІР° РЇРІРЅС‹С… РљР°РЅР°Р»Р°

```json
{
  "interaction": {
    "comments": {
      "enabled": true,
      "target_node_ids": ["triage"]
    },
    "user_mcp": {
      "enabled": true,
      "server_name": "orchestrator.user_chat"
    }
  }
}
```

РџРѕС‡РµРјСѓ СЌС‚Рѕ РІР°Р»РёРґРЅРѕ:

- РљРѕРјРјРµРЅС‚Р°СЂРёРё РѕСЃС‚Р°СЋС‚СЃСЏ СЃРІРѕР±РѕРґРЅС‹Рј run-time РІРІРѕРґРѕРј РґР»СЏ Р°РєС‚РёРІРЅРѕР№ С†РµР»РµРІРѕР№ РЅРѕРґС‹.
- Built-in user chat РѕСЃС‚Р°РµС‚СЃСЏ РѕС‚РґРµР»СЊРЅС‹Рј MCP-РєР°РЅР°Р»РѕРј РґР»СЏ СЏРІРЅС‹С… prompt/reply-РїРѕС‚РѕРєРѕРІ.
- РЎР°Рј С„Р°РєС‚ РѕР¶РёРґР°СЋС‰РµРіРѕ prompt РЅРµ СѓР±РёСЂР°РµС‚ generic comment channel.

Р”РѕРєСѓРјРµРЅС‚С‹-РІР»Р°РґРµР»СЊС†С‹: [interaction-and-chat-contract.md](../03-contracts/agent-json/interaction-and-chat-contract.md), [orchestrator-user-chat-mcp-contract.md](../03-contracts/orchestrator-user-chat-mcp-contract.md), [live-run-interaction.md](../06-interaction/live-run-interaction.md), [presentation-rules.md](../06-interaction/presentation-rules.md)

## 4. РЎСѓР¶Р°С‚СЊ Runtime Sources Р‘РµР· Р’СЃС‚СЂР°РёРІР°РЅРёСЏ РЎРµРєСЂРµС‚РѕРІ

```json
{
  "runtime_sources": [
    { "id": "primary", "runtime_adapter": "codex", "source_ref": "account:main" },
    { "id": "backup", "runtime_adapter": "codex", "source_ref": "account:backup" }
  ],
  "nodes": [
    {
      "id": "triage",
      "kind": "runtime_agent",
      "runtime_adapter": "codex",
      "runtime_source_policy": "prefer_first",
      "runtime_source_ids": ["primary", "backup"]
    }
  ]
}
```

РџРѕС‡РµРјСѓ СЌС‚Рѕ РІР°Р»РёРґРЅРѕ:

- `source_ref` РѕСЃС‚Р°РµС‚СЃСЏ opaque handle, Р° РЅРµ СЃС‹СЂС‹Рј credential.
- РќРѕРґР° СЃСѓР¶Р°РµС‚ РІС‹Р±РѕСЂ Р·Р°РїСѓСЃРєР° РІРЅСѓС‚СЂРё РѕРґРЅРѕРіРѕ adapter-family.
- Р’С‹Р±РѕСЂ РёСЃС‚РѕС‡РЅРёРєР° РѕСЃС‚Р°РµС‚СЃСЏ concern-РѕРј Core РґРѕ РІС‹Р·РѕРІР° adapter.

Р”РѕРєСѓРјРµРЅС‚С‹-РІР»Р°РґРµР»СЊС†С‹: [top-level-and-bindings-contract.md](../03-contracts/agent-json/top-level-and-bindings-contract.md), [nodes-and-edges-contract.md](../03-contracts/agent-json/nodes-and-edges-contract.md), [runtime-sources.md](../08-extensions/runtime-sources.md), [runtime-adapter-contract.md](../03-contracts/runtime-adapter-contract.md)

## 5. Р”РµР»Р°С‚СЊ Р”РѕСЃС‚СѓРїРЅРѕСЃС‚СЊ Memory РЇРІРЅРѕР№, Рђ РќРµ РџСЂРѕС‚Р°С‰РµРЅРЅРѕР№ Р§РµСЂРµР· Chat

```json
{
  "memory_bindings": [
    {
      "id": "project_memory",
      "kind": "runtime_memory",
      "codex_ref": "memory://team/project",
      "scope": "node",
      "config": {
        "intent": {
          "summary": "Project memory for investigation work.",
          "labels": ["project", "retrieval"]
        },
        "required_capabilities": ["read", "write"]
      }
    }
  ],
  "nodes": [
    {
      "id": "investigate",
      "kind": "runtime_agent",
      "memory_ids": ["project_memory"]
    }
  ]
}
```

РџРѕС‡РµРјСѓ СЌС‚Рѕ РІР°Р»РёРґРЅРѕ:

- Binding СЏРІРЅРѕ РѕР±СЉСЏРІР»РµРЅ РЅР° top-level.
- Binding РЅРµСЃРµС‚ РїРµСЂРµРЅРѕСЃРёРјС‹Р№ memory payload РІ `config`, Р° РЅРµ РѕРїРёСЂР°РµС‚СЃСЏ РЅР° РЅРµСЏРІРЅС‹Р№ provider default.
- РќРѕРґР° СЏРІРЅРѕ РїРѕРґРєР»СЋС‡Р°РµС‚ `scope = node` binding С‡РµСЂРµР· `memory_ids`.
- Chat Рё resume РѕСЃС‚Р°СЋС‚СЃСЏ РѕС‚РґРµР»РµРЅРЅС‹РјРё РѕС‚ long-term memory.

Р”РѕРєСѓРјРµРЅС‚С‹-РІР»Р°РґРµР»СЊС†С‹: [top-level-and-bindings-contract.md](../03-contracts/agent-json/top-level-and-bindings-contract.md), [memory-bindings.md](../08-extensions/memory-bindings.md), [chat-and-resume.md](../05-state/chat-and-resume.md)
## 6. Р”РµСЂР¶Р°С‚СЊ Drafts, Live-Р РµРІРёР·РёРё Р РЎСѓС‰РµСЃС‚РІСѓСЋС‰РёРµ Chats РќР° Р Р°Р·РЅС‹С… РћСЃСЏС…

РџСЂРёРјРµСЂРЅС‹Р№ РїРѕС‚РѕРє:

1. Chat `C1` СЃС‚Р°СЂС‚СѓРµС‚ РёР· СЂР°Р·СЂРµС€РµРЅРЅРѕР№ live-СЂРµРІРёР·РёРё `R7`.
2. РџРѕР»СЊР·РѕРІР°С‚РµР»СЊ СЃРѕР·РґР°РµС‚ draft `R8` Рё РїРѕР·Р¶Рµ deploy-РёС‚ РµРіРѕ.
3. Р‘СѓРґСѓС‰РёРµ РѕС‚РєСЂС‹С‚РёСЏ Рё Р±СѓРґСѓС‰РёРµ event-dispatch РјРѕРіСѓС‚ СѓР¶Рµ СЂР°Р·СЂРµС€Р°С‚СЊСЃСЏ РІ `R8`.
4. Chat `C1` РІСЃРµ СЂР°РІРЅРѕ resume-РёС‚СЃСЏ РїСЂРѕС‚РёРІ `R7`, Р° РЅРµ РїСЂРѕС‚РёРІ Р±РѕР»РµРµ РЅРѕРІРѕР№ live-СЂРµРІРёР·РёРё.

РџРѕС‡РµРјСѓ СЌС‚Рѕ РІР°Р»РёРґРЅРѕ:

- Drafts РЅРµ РїРѕРґРјРµРЅСЏСЋС‚ С‚РµРєСѓС‰СѓСЋ live-СЂРµРІРёР·РёСЋ РјРѕР»С‡Р°.
- РЎСѓС‰РµСЃС‚РІСѓСЋС‰РёРµ chats Рё resumable runs РѕСЃС‚Р°СЋС‚СЃСЏ РїСЂРёРІСЏР·Р°РЅРЅС‹РјРё Рє СЂРµРІРёР·РёРё, РёР· РєРѕС‚РѕСЂРѕР№ СЃС‚Р°СЂС‚РѕРІР°Р»Рё.
- Р‘СѓРґСѓС‰РёРµ events СЂР°Р·СЂРµС€Р°СЋС‚ С‚РµРєСѓС‰СѓСЋ live-СЂРµРІРёР·РёСЋ С‚РѕР»СЊРєРѕ РІ РјРѕРјРµРЅС‚ dispatch.

Р”РѕРєСѓРјРµРЅС‚С‹-РІР»Р°РґРµР»СЊС†С‹: [agent-registry.md](../07-lifecycle/agent-registry.md), [draft-live-deploy.md](../07-lifecycle/draft-live-deploy.md), [events-and-triggers.md](../07-lifecycle/events-and-triggers.md), [versioning-axes.md](../07-lifecycle/versioning-axes.md), [chat-and-resume.md](../05-state/chat-and-resume.md)

## 7. Р”Р°РІР°С‚СЊ Builder Р Р°Р±РѕС‚Р°С‚СЊ Р§РµСЂРµР· Drafts, Рђ РќРµ Р’ РћР±С…РѕРґ Lifecycle

РџСЂРёРјРµСЂРЅС‹Р№ РїРѕС‚РѕРє:

1. Builder РёСЃСЃР»РµРґСѓРµС‚ СЂР°Р·СЂРµС€РµРЅРЅС‹Рµ С„Р°Р№Р»С‹ Рё СЃРѕР·РґР°РµС‚ РєР°РЅРґРёРґР°С‚РЅСѓСЋ СЂРµРІРёР·РёСЋ.
2. РљР°РЅРґРёРґР°С‚ СЃРѕС…СЂР°РЅСЏРµС‚СЃСЏ РєР°Рє draft Рё РІР°Р»РёРґРёСЂСѓРµС‚СЃСЏ.
3. Draft Р·Р°РїСѓСЃРєР°РµС‚СЃСЏ РёР»Рё СЃСЂР°РІРЅРёРІР°РµС‚СЃСЏ.
4. Deploy РїСЂРѕРёСЃС…РѕРґРёС‚ С‚РѕР»СЊРєРѕ РєР°Рє РѕС‚РґРµР»СЊРЅРѕРµ СЏРІРЅРѕРµ РґРµР№СЃС‚РІРёРµ РїРѕР·Р¶Рµ.

РџРѕС‡РµРјСѓ СЌС‚Рѕ РІР°Р»РёРґРЅРѕ:

- Builder РѕСЃС‚Р°РµС‚СЃСЏ Р°РіРµРЅС‚РѕРј РІРЅСѓС‚СЂРё С‚РѕР№ Р¶Рµ Р°СЂС…РёС‚РµРєС‚СѓСЂС‹.
- File truth, registry truth Рё deploy semantics РѕСЃС‚Р°СЋС‚СЃСЏ С†РµР»С‹РјРё.
- Р СѓС‡РЅРѕРµ СЂРµРґР°РєС‚РёСЂРѕРІР°РЅРёРµ Рё review РѕСЃС‚Р°СЋС‚СЃСЏ first-class.

Р”РѕРєСѓРјРµРЅС‚С‹-РІР»Р°РґРµР»СЊС†С‹: [builder-agent.md](../08-extensions/builder-agent.md), [draft-live-deploy.md](../07-lifecycle/draft-live-deploy.md), [agent-registry.md](../07-lifecycle/agent-registry.md)
