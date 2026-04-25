[English](#english) | [Р СѓСЃСЃРєРёР№](#russian)

<a id="english"></a>
# Canonical Agent JSON Example

Status: non-normative illustrative example.
Owns: nothing. The normative owners are still [top-level-and-bindings-contract.md](../03-contracts/agent-json/top-level-and-bindings-contract.md), [nodes-and-edges-contract.md](../03-contracts/agent-json/nodes-and-edges-contract.md), and [interaction-and-chat-contract.md](../03-contracts/agent-json/interaction-and-chat-contract.md).

Section map:

- [Examples index](./README.md)
- [Valid patterns](./valid-patterns.md)
- [Invalid patterns](./invalid-patterns.md)
- [Interaction sequences](./interaction-sequences.md)

This sample is designed to be contract-shaped and behaviorally consistent with the owner docs. Opaque values such as `codex_ref`, `source_ref`, permission tokens, and `runtime_options` remain illustrative only. The `graph_contract_version` string below reuses the example value shown in the canonical specification; this file does not define the supported version set.

## Shared JSON Sample

The JSON block below is language-neutral and is referenced by both the English and Russian notes in this file.

```json
{
  "graph_contract_version": "1.0",
  "meta": {
    "id": "review-assistant",
    "name": "Review Assistant",
    "description": "Triage a repository task, call a specialist agent, and publish a user-facing handoff.",
    "agent_version": "2.3.0"
  },
  "entry_node_id": "triage",
  "params": {
    "task": {
      "type": "string",
      "required": true,
      "description": "The task the operator wants handled."
    },
    "priority": {
      "type": "string",
      "required": false,
      "default": "normal",
      "description": "Operator-selected priority for this run."
    }
  },
  "initial_vars": {
    "status": "new"
  },
  "skills": [
    {
      "id": "triage_skill",
      "codex_ref": "skills/review-triage.md"
    },
    {
      "id": "handoff_style",
      "inline_text": "Write concise operator-facing summaries. Do not present intermediate reasoning as final output.",
      "frozen": true
    }
  ],
  "mcps": [
    {
      "id": "repo_search",
      "codex_ref": "workspace.search"
    }
  ],
  "plugins": [
    {
      "id": "github",
      "codex_ref": "github"
    }
  ],
  "permissions": {
    "profile": "workspace-write",
    "allow": [
      "read_repo",
      "write_owned_files"
    ]
  },
  "interaction": {
    "comments": {
      "enabled": true,
      "target_node_ids": [
        "triage",
        "handoff"
      ]
    },
    "user_mcp": {
      "enabled": true,
      "server_name": "orchestrator.user_chat"
    }
  },
  "chat": {
    "prefer_native_resume": true,
    "store_visible_messages": true,
    "store_context_window": true,
    "allow_fresh_start": true,
    "secret_markers": {
      "enabled": true
    }
  },
  "final_output": {
    "mode": "last_node_output"
  },
  "memory_bindings": [
    {
      "id": "team_memory",
      "kind": "runtime_memory",
      "codex_ref": "memory://team/review-assistant",
      "scope": "agent",
      "config": {
        "intent": {
          "summary": "Shared review knowledge for the review-assistant team.",
          "labels": [
            "team",
            "project",
            "retrieval"
          ]
        },
        "required_capabilities": [
          "read",
          "write",
          "entity_scoped"
        ],
        "transport_preferences": {
          "preferred": [
            "api"
          ]
        }
      }
    },
    {
      "id": "incident_memory",
      "kind": "runtime_memory",
      "codex_ref": "memory://team/incidents",
      "scope": "node",
      "config": {
        "intent": {
          "summary": "Incident timeline and remediation memory for investigation nodes.",
          "labels": [
            "graph",
            "session",
            "retrieval"
          ]
        },
        "required_capabilities": [
          "read",
          "graph_context",
          "temporal_index"
        ],
        "provider_extension": {
          "provider": "graphiti",
          "config": {
            "group_id": "incidents",
            "search_profile": "timeline"
          }
        }
      }
    }
  ],
  "runtime_sources": [
    {
      "id": "primary_codex",
      "runtime_adapter": "codex",
      "source_ref": "account:main",
      "description": "Primary configured Codex account."
    },
    {
      "id": "backup_codex",
      "runtime_adapter": "codex",
      "source_ref": "account:backup",
      "description": "Fallback configured Codex account."
    }
  ],
  "nodes": [
    {
      "id": "triage",
      "title": "Triage the request",
      "kind": "runtime_agent",
      "runtime_adapter": "codex",
      "prompt": "Review the task. Decide the current status, produce a short plan summary, and say whether a user reply is needed.",
      "input": {
        "parts": [
          {
            "type": "text",
            "text": "Task:\\n"
          },
          {
            "type": "ref",
            "ref": "params.task"
          },
          {
            "type": "text",
            "text": "\\nPriority:\\n"
          },
          {
            "type": "ref",
            "ref": "params.priority"
          },
          {
            "type": "text",
            "text": "\\nCurrent status:\\n"
          },
          {
            "type": "ref",
            "ref": "vars.status"
          }
        ]
      },
      "output": {
        "mode": "json",
        "schema": {
          "type": "object",
          "required": [
            "status",
            "plan",
            "needs_user_reply"
          ],
          "properties": {
            "status": {
              "type": "string"
            },
            "plan": {
              "type": "object",
              "required": [
                "summary"
              ],
              "properties": {
                "summary": {
                  "type": "string"
                }
              },
              "additionalProperties": false
            },
            "needs_user_reply": {
              "type": "boolean"
            }
          },
          "additionalProperties": false
        }
      },
      "skill_ids": [
        "triage_skill",
        "handoff_style"
      ],
      "mcp_ids": [
        "repo_search"
      ],
      "plugin_ids": [
        "github"
      ],
      "memory_ids": [
        "team_memory",
        "incident_memory"
      ],
      "runtime_source_policy": "prefer_first",
      "runtime_source_ids": [
        "primary_codex",
        "backup_codex"
      ],
      "runtime_options": {
        "temperature": 0
      }
    },
    {
      "id": "specialist_review",
      "title": "Call a specialist child agent",
      "kind": "orchestrator_agent",
      "agent_ref": "specialist-review",
      "input": {
        "parts": [
          {
            "type": "text",
            "text": "Review this plan and return a short recommendation.\\nPlan:\\n"
          },
          {
            "type": "ref",
            "ref": "node.triage.json.plan"
          },
          {
            "type": "text",
            "text": "\\nPriority:\\n"
          },
          {
            "type": "ref",
            "ref": "params.priority"
          }
        ]
      },
      "output": {
        "mode": "text"
      }
    },
    {
      "id": "handoff",
      "title": "Produce the operator-facing handoff",
      "kind": "runtime_agent",
      "runtime_adapter": "codex",
      "prompt": "Write the final operator-facing handoff. Keep it plain, concise, and non-speculative.",
      "input": {
        "parts": [
          {
            "type": "text",
            "text": "Specialist recommendation:\\n"
          },
          {
            "type": "ref",
            "ref": "node.specialist_review.text"
          },
          {
            "type": "text",
            "text": "\\nNeeds user reply:\\n"
          },
          {
            "type": "ref",
            "ref": "node.triage.json.needs_user_reply"
          }
        ]
      },
      "output": {
        "mode": "text"
      },
      "skill_ids": [
        "handoff_style"
      ],
      "mcp_ids": [
        "repo_search"
      ],
      "plugin_ids": [
        "github"
      ],
      "permissions": {
        "profile": "workspace-read",
        "allow": [
          "read_repo"
        ]
      },
      "runtime_source_policy": "restrict",
      "runtime_source_ids": [
        "primary_codex"
      ]
    }
  ],
  "edges": [
    {
      "from": "triage",
      "to": "specialist_review",
      "condition": {
        "code": "vars.status == 'ready'"
      }
    },
    {
      "from": "specialist_review",
      "to": "handoff"
    }
  ]
}
```

## What This Example Illustrates

- The root object uses only allowed top-level fields from the closed portable contract.
Owner docs: [top-level-and-bindings-contract.md](../03-contracts/agent-json/top-level-and-bindings-contract.md), [source-of-truth-model.md](../01-foundations/source-of-truth-model.md)

- `triage` and `handoff` are `runtime_agent` nodes, while `specialist_review` is an `orchestrator_agent` node. The runtime-facing fields appear only on the runtime nodes, and `agent_ref` is shown as a logical agent id rather than a file path.
Owner docs: [nodes-and-edges-contract.md](../03-contracts/agent-json/nodes-and-edges-contract.md), [agent-registry.md](../07-lifecycle/agent-registry.md), [runtime-integration-model.md](../02-architecture/runtime-integration-model.md)

- `input.parts` stays explicit and ordered. Later nodes read only from allowed spaces such as `params`, `vars`, and prior node outputs.
Owner docs: [nodes-and-edges-contract.md](../03-contracts/agent-json/nodes-and-edges-contract.md), [dataflow-and-input-resolution.md](../04-execution/dataflow-and-input-resolution.md)

- The `triage` node declares `output.mode = json` with an explicit schema, so a successful top-level object may both remain addressable through `node.triage.json.*` and copy its top-level fields into `vars`.
Owner docs: [nodes-and-edges-contract.md](../03-contracts/agent-json/nodes-and-edges-contract.md), [outputs-outcomes-and-final-response.md](../04-execution/outputs-outcomes-and-final-response.md)

- The edge after `triage` checks `vars.status` using the documented condition-context field surface and only controls flow. It does not transport payload data by itself.
Owner docs: [nodes-and-edges-contract.md](../03-contracts/agent-json/nodes-and-edges-contract.md), [graph-execution.md](../04-execution/graph-execution.md)

- Comments and built-in user chat are configured separately. Free-form comments target only active runtime nodes listed in `target_node_ids`, while prompt replies use `orchestrator.user_chat`.
Owner docs: [interaction-and-chat-contract.md](../03-contracts/agent-json/interaction-and-chat-contract.md), [orchestrator-user-chat-mcp-contract.md](../03-contracts/orchestrator-user-chat-mcp-contract.md), [live-run-interaction.md](../06-interaction/live-run-interaction.md)

- The chat policy enables explicit resume features without turning chat state into hidden long-term memory. Secret markers remain optional and separate from the base output contract.
Owner docs: [interaction-and-chat-contract.md](../03-contracts/agent-json/interaction-and-chat-contract.md), [chat-and-resume.md](../05-state/chat-and-resume.md), [secret-markers.md](../05-state/secret-markers.md)

- `memory_bindings` and `memory_ids` are explicit extension points. Portable memory bindings now carry intent, capability requirements, and an optional provider-specific escape hatch inside `config`; they still do not permit hidden promotion of chat transcript into memory.
Owner docs: [top-level-and-bindings-contract.md](../03-contracts/agent-json/top-level-and-bindings-contract.md), [memory-bindings.md](../08-extensions/memory-bindings.md)

- `runtime_sources` narrow launch choice without embedding raw credentials. `prefer_first` and `restrict` are Core-side selection policies, not new adapter semantics invented by this example.
Owner docs: [nodes-and-edges-contract.md](../03-contracts/agent-json/nodes-and-edges-contract.md), [runtime-sources.md](../08-extensions/runtime-sources.md), [runtime-adapter-contract.md](../03-contracts/runtime-adapter-contract.md)

## Illustrative Successful `triage` Result

If `triage` succeeds, one compatible result could look like this:

```json
{
  "status": "ready",
  "plan": {
    "summary": "Inspect open review threads and unresolved CI failures."
  },
  "needs_user_reply": false
}
```

That illustrative result would make these example consequences visible:

- `vars.status` becomes `ready`, so the first edge may match.
- `node.triage.json.plan` becomes available to `specialist_review`.
- `node.triage.json.needs_user_reply` becomes available to `handoff`.
- The run still has no final user-facing answer until the graph reaches its last successful terminal node.

Owner docs: [dataflow-and-input-resolution.md](../04-execution/dataflow-and-input-resolution.md), [graph-execution.md](../04-execution/graph-execution.md), [outputs-outcomes-and-final-response.md](../04-execution/outputs-outcomes-and-final-response.md)

## What Is Intentionally Missing

- No `live_revision`, draft marker, registry status, trigger, or chat record appears inside the file. Those are local lifecycle or state concerns, not portable file fields.
Owner docs: [agent-registry.md](../07-lifecycle/agent-registry.md), [draft-live-deploy.md](../07-lifecycle/draft-live-deploy.md), [events-and-triggers.md](../07-lifecycle/events-and-triggers.md)

- No SDK types, session objects, or UI-only fields appear in the file. Those belong behind the adapter or interface boundaries.
Owner docs: [core-and-interfaces.md](../02-architecture/core-and-interfaces.md), [runtime-integration-model.md](../02-architecture/runtime-integration-model.md)

<a id="russian"></a>
# РљР°РЅРѕРЅРёС‡РµСЃРєРёР№ РџСЂРёРјРµСЂ Agent JSON

РЎС‚Р°С‚СѓСЃ: РЅРµРЅРѕСЂРјР°С‚РёРІРЅС‹Р№ РёР»Р»СЋСЃС‚СЂР°С‚РёРІРЅС‹Р№ РїСЂРёРјРµСЂ.
Р’Р»Р°РґРµРЅРёРµ: РЅРёС‡РµРј. РќРѕСЂРјР°С‚РёРІРЅС‹РјРё РІР»Р°РґРµР»СЊС†Р°РјРё РїРѕ-РїСЂРµР¶РЅРµРјСѓ РѕСЃС‚Р°СЋС‚СЃСЏ [top-level-and-bindings-contract.md](../03-contracts/agent-json/top-level-and-bindings-contract.md), [nodes-and-edges-contract.md](../03-contracts/agent-json/nodes-and-edges-contract.md) Рё [interaction-and-chat-contract.md](../03-contracts/agent-json/interaction-and-chat-contract.md).

РљР°СЂС‚Р° СЂР°Р·РґРµР»Р°:

- [РРЅРґРµРєСЃ РїСЂРёРјРµСЂРѕРІ](./README.md)
- [Р’Р°Р»РёРґРЅС‹Рµ РїР°С‚С‚РµСЂРЅС‹](./valid-patterns.md)
- [РќРµРІР°Р»РёРґРЅС‹Рµ РїР°С‚С‚РµСЂРЅС‹](./invalid-patterns.md)
- [РЎС†РµРЅР°СЂРёРё РІР·Р°РёРјРѕРґРµР№СЃС‚РІРёСЏ](./interaction-sequences.md)

JSON-Р±Р»РѕРє РІС‹С€Рµ СЏРІР»СЏРµС‚СЃСЏ РѕР±С‰РёРј РґР»СЏ РѕР±РµРёС… СЏР·С‹РєРѕРІС‹С… СЃРµРєС†РёР№. РћРЅ Р·Р°РґСѓРјР°РЅ РєР°Рє РїСЂРёРјРµСЂ С„РѕСЂРјС‹ РєРѕРЅС‚СЂР°РєС‚Р° Рё СЃРѕРіР»Р°СЃРѕРІР°РЅРЅРѕРіРѕ РїРѕРІРµРґРµРЅРёСЏ, РЅРѕ РЅРµ РєР°Рє РІС‚РѕСЂРѕР№ РёСЃС‚РѕС‡РЅРёРє РёСЃС‚РёРЅС‹. РќРµРїСЂРѕР·СЂР°С‡РЅС‹Рµ Р·РЅР°С‡РµРЅРёСЏ РІСЂРѕРґРµ `codex_ref`, `source_ref`, permission-С‚РѕРєРµРЅРѕРІ Рё `runtime_options` Р·РґРµСЃСЊ РѕСЃС‚Р°СЋС‚СЃСЏ С‚РѕР»СЊРєРѕ РёР»Р»СЋСЃС‚СЂР°С‚РёРІРЅС‹РјРё. РЎС‚СЂРѕРєР° `graph_contract_version` РїРѕРІС‚РѕСЂСЏРµС‚ РїСЂРёРјРµСЂ РёР· РєР°РЅРѕРЅРёС‡РµСЃРєРѕР№ СЃРїРµС†РёС„РёРєР°С†РёРё; СЌС‚РѕС‚ С„Р°Р№Р» РЅРµ РѕРїСЂРµРґРµР»СЏРµС‚ РЅР°Р±РѕСЂ РїРѕРґРґРµСЂР¶РёРІР°РµРјС‹С… РІРµСЂСЃРёР№.

## Р§С‚Рѕ РР»Р»СЋСЃС‚СЂРёСЂСѓРµС‚ Р­С‚РѕС‚ РџСЂРёРјРµСЂ

- РљРѕСЂРЅРµРІРѕР№ РѕР±СЉРµРєС‚ РёСЃРїРѕР»СЊР·СѓРµС‚ С‚РѕР»СЊРєРѕ С‚Рµ top-level РїРѕР»СЏ, РєРѕС‚РѕСЂС‹Рµ СЂР°Р·СЂРµС€РµРЅС‹ Р·Р°РєСЂС‹С‚С‹Рј РїРµСЂРµРЅРѕСЃРёРјС‹Рј РєРѕРЅС‚СЂР°РєС‚РѕРј.
Р”РѕРєСѓРјРµРЅС‚С‹-РІР»Р°РґРµР»СЊС†С‹: [top-level-and-bindings-contract.md](../03-contracts/agent-json/top-level-and-bindings-contract.md), [source-of-truth-model.md](../01-foundations/source-of-truth-model.md)

- `triage` Рё `handoff` СЏРІР»СЏСЋС‚СЃСЏ РЅРѕРґР°РјРё `runtime_agent`, Р° `specialist_review` СЏРІР»СЏРµС‚СЃСЏ РЅРѕРґРѕР№ `orchestrator_agent`. Runtime-facing РїРѕР»СЏ РїСЂРёСЃСѓС‚СЃС‚РІСѓСЋС‚ С‚РѕР»СЊРєРѕ Сѓ runtime-РЅРѕРґ, Р° `agent_ref` РїРѕРєР°Р·Р°РЅ РєР°Рє Р»РѕРіРёС‡РµСЃРєРёР№ РёРґРµРЅС‚РёС„РёРєР°С‚РѕСЂ Р°РіРµРЅС‚Р°, Р° РЅРµ РєР°Рє С„Р°Р№Р»РѕРІС‹Р№ РїСѓС‚СЊ.
Р”РѕРєСѓРјРµРЅС‚С‹-РІР»Р°РґРµР»СЊС†С‹: [nodes-and-edges-contract.md](../03-contracts/agent-json/nodes-and-edges-contract.md), [agent-registry.md](../07-lifecycle/agent-registry.md), [runtime-integration-model.md](../02-architecture/runtime-integration-model.md)

- `input.parts` РѕСЃС‚Р°РµС‚СЃСЏ СЏРІРЅС‹Рј Рё СѓРїРѕСЂСЏРґРѕС‡РµРЅРЅС‹Рј. РџРѕР·РґРЅРёРµ РЅРѕРґС‹ С‡РёС‚Р°СЋС‚ С‚РѕР»СЊРєРѕ РёР· СЂР°Р·СЂРµС€РµРЅРЅС‹С… РїСЂРѕСЃС‚СЂР°РЅСЃС‚РІ, С‚Р°РєРёС… РєР°Рє `params`, `vars` Рё outputs РїСЂРµРґС‹РґСѓС‰РёС… РЅРѕРґ.
Р”РѕРєСѓРјРµРЅС‚С‹-РІР»Р°РґРµР»СЊС†С‹: [nodes-and-edges-contract.md](../03-contracts/agent-json/nodes-and-edges-contract.md), [dataflow-and-input-resolution.md](../04-execution/dataflow-and-input-resolution.md)

- РќРѕРґР° `triage` РѕР±СЉСЏРІР»СЏРµС‚ `output.mode = json` СЃ СЏРІРЅРѕР№ schema, РїРѕСЌС‚РѕРјСѓ СѓСЃРїРµС€РЅС‹Р№ top-level object РѕРґРЅРѕРІСЂРµРјРµРЅРЅРѕ РѕСЃС‚Р°РµС‚СЃСЏ РґРѕСЃС‚СѓРїРЅС‹Рј С‡РµСЂРµР· `node.triage.json.*` Рё РєРѕРїРёСЂСѓРµС‚ СЃРІРѕРё top-level РїРѕР»СЏ РІ `vars`.
Р”РѕРєСѓРјРµРЅС‚С‹-РІР»Р°РґРµР»СЊС†С‹: [nodes-and-edges-contract.md](../03-contracts/agent-json/nodes-and-edges-contract.md), [outputs-outcomes-and-final-response.md](../04-execution/outputs-outcomes-and-final-response.md)

- Edge РїРѕСЃР»Рµ `triage` РїСЂРѕРІРµСЂСЏРµС‚ `vars` Рё СѓРїСЂР°РІР»СЏРµС‚ С‚РѕР»СЊРєРѕ control flow. РћРЅ СЃР°Рј РїРѕ СЃРµР±Рµ РЅРµ РїРµСЂРµРЅРѕСЃРёС‚ payload.
Р”РѕРєСѓРјРµРЅС‚С‹-РІР»Р°РґРµР»СЊС†С‹: [nodes-and-edges-contract.md](../03-contracts/agent-json/nodes-and-edges-contract.md), [graph-execution.md](../04-execution/graph-execution.md)

- РљРѕРјРјРµРЅС‚Р°СЂРёРё Рё РІСЃС‚СЂРѕРµРЅРЅС‹Р№ user chat РєРѕРЅС„РёРіСѓСЂРёСЂСѓСЋС‚СЃСЏ СЂР°Р·РґРµР»СЊРЅРѕ. РЎРІРѕР±РѕРґРЅС‹Рµ РєРѕРјРјРµРЅС‚Р°СЂРёРё Р°РґСЂРµСЃСѓСЋС‚СЃСЏ С‚РѕР»СЊРєРѕ Р°РєС‚РёРІРЅС‹Рј runtime-РЅРѕРґР°Рј РёР· `target_node_ids`, Р° РѕС‚РІРµС‚С‹ РЅР° prompt РёРґСѓС‚ С‡РµСЂРµР· `orchestrator.user_chat`.
Р”РѕРєСѓРјРµРЅС‚С‹-РІР»Р°РґРµР»СЊС†С‹: [interaction-and-chat-contract.md](../03-contracts/agent-json/interaction-and-chat-contract.md), [orchestrator-user-chat-mcp-contract.md](../03-contracts/orchestrator-user-chat-mcp-contract.md), [live-run-interaction.md](../06-interaction/live-run-interaction.md)

- Chat policy РІРєР»СЋС‡Р°РµС‚ РІРѕР·РјРѕР¶РЅРѕСЃС‚Рё explicit resume, РЅРµ РїСЂРµРІСЂР°С‰Р°СЏ chat state РІ СЃРєСЂС‹С‚СѓСЋ РґРѕР»РіРѕРІСЂРµРјРµРЅРЅСѓСЋ РїР°РјСЏС‚СЊ. Secret markers РѕСЃС‚Р°СЋС‚СЃСЏ РѕРїС†РёРѕРЅР°Р»СЊРЅС‹РјРё Рё РѕС‚РґРµР»СЊРЅС‹РјРё РѕС‚ Р±Р°Р·РѕРІРѕРіРѕ output-РєРѕРЅС‚СЂР°РєС‚Р°.
Р”РѕРєСѓРјРµРЅС‚С‹-РІР»Р°РґРµР»СЊС†С‹: [interaction-and-chat-contract.md](../03-contracts/agent-json/interaction-and-chat-contract.md), [chat-and-resume.md](../05-state/chat-and-resume.md), [secret-markers.md](../05-state/secret-markers.md)

- `memory_bindings` Рё `memory_ids` СЏРІР»СЏСЋС‚СЃСЏ СЏРІРЅС‹РјРё extension points. Portable memory bindings С‚РµРїРµСЂСЊ РЅРµСЃСѓС‚ intent, С‚СЂРµР±РѕРІР°РЅРёСЏ Рє capabilities Рё РѕРїС†РёРѕРЅР°Р»СЊРЅС‹Р№ provider-specific escape hatch РІРЅСѓС‚СЂРё `config`; РѕРЅРё РїРѕ-РїСЂРµР¶РЅРµРјСѓ РЅРµ СЂР°Р·СЂРµС€Р°СЋС‚ СЃРєСЂС‹С‚РѕРµ РїСЂРµРІСЂР°С‰РµРЅРёРµ transcript С‡Р°С‚Р° РІ memory.
Р”РѕРєСѓРјРµРЅС‚С‹-РІР»Р°РґРµР»СЊС†С‹: [top-level-and-bindings-contract.md](../03-contracts/agent-json/top-level-and-bindings-contract.md), [memory-bindings.md](../08-extensions/memory-bindings.md)

- `runtime_sources` СЃСѓР¶Р°СЋС‚ РІС‹Р±РѕСЂ Р·Р°РїСѓСЃРєР° Р±РµР· РІСЃС‚СЂР°РёРІР°РЅРёСЏ СЃС‹СЂС‹С… credentials. `prefer_first` Рё `restrict` СЏРІР»СЏСЋС‚СЃСЏ Core-side РїРѕР»РёС‚РёРєР°РјРё РІС‹Р±РѕСЂР°, Р° РЅРµ РЅРѕРІРѕР№ СЃРµРјР°РЅС‚РёРєРѕР№ Р°РґР°РїС‚РµСЂР°, РїСЂРёРґСѓРјР°РЅРЅРѕР№ СЌС‚РёРј РїСЂРёРјРµСЂРѕРј.
Р”РѕРєСѓРјРµРЅС‚С‹-РІР»Р°РґРµР»СЊС†С‹: [nodes-and-edges-contract.md](../03-contracts/agent-json/nodes-and-edges-contract.md), [runtime-sources.md](../08-extensions/runtime-sources.md), [runtime-adapter-contract.md](../03-contracts/runtime-adapter-contract.md)

## РР»Р»СЋСЃС‚СЂР°С‚РёРІРЅС‹Р№ РЈСЃРїРµС€РЅС‹Р№ Р РµР·СѓР»СЊС‚Р°С‚ `triage`

Р•СЃР»Рё `triage` Р·Р°РІРµСЂС€Р°РµС‚СЃСЏ СѓСЃРїРµС€РЅРѕ, РѕРґРёРЅ СЃРѕРІРјРµСЃС‚РёРјС‹Р№ СЂРµР·СѓР»СЊС‚Р°С‚ РјРѕР¶РµС‚ РІС‹РіР»СЏРґРµС‚СЊ С‚Р°Рє:

```json
{
  "status": "ready",
  "plan": {
    "summary": "Inspect open review threads and unresolved CI failures."
  },
  "needs_user_reply": false
}
```

РЈ С‚Р°РєРѕРіРѕ РёР»Р»СЋСЃС‚СЂР°С‚РёРІРЅРѕРіРѕ СЂРµР·СѓР»СЊС‚Р°С‚Р° Р±С‹Р»Рё Р±С‹ СЃР»РµРґСѓСЋС‰РёРµ РїРѕСЃР»РµРґСЃС‚РІРёСЏ:

- `vars.status` СЃС‚Р°РЅРѕРІРёС‚СЃСЏ СЂР°РІРЅС‹Рј `ready`, РїРѕСЌС‚РѕРјСѓ РїРµСЂРІС‹Р№ edge РјРѕР¶РµС‚ СЃРѕРІРїР°СЃС‚СЊ.
- `node.triage.json.plan` СЃС‚Р°РЅРѕРІРёС‚СЃСЏ РґРѕСЃС‚СѓРїРЅС‹Рј РґР»СЏ `specialist_review`.
- `node.triage.json.needs_user_reply` СЃС‚Р°РЅРѕРІРёС‚СЃСЏ РґРѕСЃС‚СѓРїРЅС‹Рј РґР»СЏ `handoff`.
- РЈ run РїРѕ-РїСЂРµР¶РЅРµРјСѓ РЅРµС‚ С„РёРЅР°Р»СЊРЅРѕРіРѕ РїРѕР»СЊР·РѕРІР°С‚РµР»СЊСЃРєРѕРіРѕ РѕС‚РІРµС‚Р°, РїРѕРєР° РіСЂР°С„ РЅРµ РґРѕСЃС‚РёРіРЅРµС‚ СЃРІРѕРµР№ РїРѕСЃР»РµРґРЅРµР№ СѓСЃРїРµС€РЅРѕ Р·Р°РІРµСЂС€РёРІС€РµР№СЃСЏ С‚РµСЂРјРёРЅР°Р»СЊРЅРѕР№ РЅРѕРґС‹.

Р”РѕРєСѓРјРµРЅС‚С‹-РІР»Р°РґРµР»СЊС†С‹: [dataflow-and-input-resolution.md](../04-execution/dataflow-and-input-resolution.md), [graph-execution.md](../04-execution/graph-execution.md), [outputs-outcomes-and-final-response.md](../04-execution/outputs-outcomes-and-final-response.md)

## Р§РµРіРѕ Р—РґРµСЃСЊ РќР°РјРµСЂРµРЅРЅРѕ РќРµС‚

- Р’РЅСѓС‚СЂРё С„Р°Р№Р»Р° РЅРµС‚ `live_revision`, draft-РјР°СЂРєРµСЂР°, registry-status, trigger РёР»Рё chat-record. Р­С‚Рѕ Р»РѕРєР°Р»СЊРЅС‹Рµ concerns Р¶РёР·РЅРµРЅРЅРѕРіРѕ С†РёРєР»Р° РёР»Рё СЃРѕСЃС‚РѕСЏРЅРёСЏ, Р° РЅРµ РїРµСЂРµРЅРѕСЃРёРјС‹Рµ РїРѕР»СЏ С„Р°Р№Р»Р°.
Р”РѕРєСѓРјРµРЅС‚С‹-РІР»Р°РґРµР»СЊС†С‹: [agent-registry.md](../07-lifecycle/agent-registry.md), [draft-live-deploy.md](../07-lifecycle/draft-live-deploy.md), [events-and-triggers.md](../07-lifecycle/events-and-triggers.md)

- Р’РЅСѓС‚СЂРё С„Р°Р№Р»Р° РЅРµС‚ SDK-types, session objects РёР»Рё UI-only РїРѕР»РµР№. РћРЅРё РїСЂРёРЅР°РґР»РµР¶Р°С‚ РіСЂР°РЅРёС†Р°Рј adapter РёР»Рё interface.
Р”РѕРєСѓРјРµРЅС‚С‹-РІР»Р°РґРµР»СЊС†С‹: [core-and-interfaces.md](../02-architecture/core-and-interfaces.md), [runtime-integration-model.md](../02-architecture/runtime-integration-model.md)
