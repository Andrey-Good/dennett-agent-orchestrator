[English](#english) | [Russian](#russian)

<a id="english"></a>
# Builder 2.0 Productization

Status: canonical Stage 9 public-launch readiness owner for Builder 2.0. Stage 9 records the bounded, audited, draft-first Builder authoring surface. It does not claim a complete public authoring product or prove that every builder-authored draft executes everywhere.

Related documents:

- [Public Launch Scope](./public-launch-scope.md)
- [Baseline Gap And Forbidden Claims](./baseline-gap-and-forbidden-claims.md)
- [Builder Agent](../08-extensions/builder-agent.md)
- [Phase 17 Builder 2.0](../18-builder-2-0/phase-17-builder-2-0.md)
- [Builder Output Schema](../../contracts/json-schema/builder-output.schema.json)
- [Memory Binding Model Contract](../03-contracts/agent-json/memory-binding-model-contract.md)
- [Runtime Adapter Contract](../03-contracts/runtime-adapter-contract.md)
- [Interaction And Chat Contract](../03-contracts/agent-json/interaction-and-chat-contract.md)
- [Managed Subagent Productization](./managed-subagent-productization.md)

## Stage 9 Decision

Builder 2.0 is productized only as an audited draft-authoring surface for the current public portable Agent JSON contract.

The supported public shape is:

- users invoke the builder flow through the documented interface-backed path;
- the builder system agent returns a formal wrapper payload with the exact top-level shape `{"agent_file": <portable-agent-json>}`;
- Core extracts and validates the embedded `agent_file`;
- Core runs a deterministic candidate audit before persistence;
- accepted candidates are saved as draft revisions only;
- CLI output includes candidate diagnostics outside Agent JSON;
- deploy, runtime/provider registration, live execution proof, and integrated product-flow proof remain separate steps.

## Formal Builder Output Wrapper

`contracts/json-schema/builder-output.schema.json` is the formal builder output wrapper contract.

The wrapper is intentionally small:

```json
{
  "agent_file": {
    "graph_contract_version": "1.0",
    "meta": {
      "id": "agent.example",
      "name": "Example Agent"
    },
    "entry_node_id": "start",
    "nodes": [
      {
        "id": "start",
        "kind": "runtime_agent",
        "runtime_adapter": "codex",
        "prompt": "Return a concise answer.",
        "input": {
          "parts": [
            {
              "type": "text",
              "text": "Hello."
            }
          ]
        },
        "output": {
          "mode": "text"
        }
      }
    ]
  }
}
```

The schema requires only `agent_file` at the wrapper level and rejects additional wrapper properties. Diagnostics, audit findings, capabilities, builder run IDs, and lifecycle draft metadata are host output, not portable Agent JSON fields.

## Deterministic Candidate Audit

TASK-557 added a deterministic audit after schema validation and before draft persistence. A rejected candidate must not be persisted.

The audit currently checks:

- runtime option validation for `model`, `reasoning_effort`, `speed_tier`, and `personality`;
- runtime capability gates for explicit runtime sources, memory bindings, live comments, built-in user chat MCP, reasoning effort, speed tiers, and personality;
- JSON output schema compilation for nodes whose output mode is `json`;
- hidden managed-subagent field rejection, including task packages, write sets, lineage, budgets, and control payloads;
- local provider, account, rate-limit, credential, secret, executable, package path, and provider-registration data rejection;
- memory provider-extension smuggling, including non-Mem0 provider config and forbidden local or secret-like Mem0 fields.

Candidate diagnostics are exposed outside the Agent JSON as `candidate_diagnostics`. They include audit status, issues, and selected runtime adapter capabilities. They are not persisted into the portable agent file.

## Product Boundary

Builder 2.0 may author intent for public surfaces, but those surfaces remain owned by their subsystem contracts after draft creation.

| Surface | Builder may draft | Owner after draft creation |
| --- | --- | --- |
| Runtime controls | Portable runtime source references and supported runtime options. | Runtime adapter contract and selected runtime capability metadata. |
| Memory bindings | Portable memory intent, required capabilities, transport preferences, and allowed provider-extension data. | Memory binding contract, local provider registry, and provider adapter. |
| Interaction | Documented comments and `orchestrator.user_chat` settings. | Interaction and chat contracts plus runtime support. |
| Managed subagents | Public `orchestrator_agent` graph nodes and handoff prompts. | Managed Subagent MCP/product surface, not Agent JSON hidden fields. |
| Lifecycle | Draft creation or revision output. | Registry, draft/live/deploy lifecycle services. |

Builder 2.0 does not:

- deploy or mark drafts live;
- register memory providers or runtime accounts;
- own local secrets, credentials, package paths, provider registrations, account metadata, or rate limits;
- perform hidden live managed-subagent orchestration;
- prove that every draft can execute on every runtime;
- bypass subsystem capability gates;
- make diagnostics part of Agent JSON.

## CLI/Public Output Boundary

The Stage 9 CLI builder command remains draft-first. It does not expose inline deploy and does not expose builder-time runtime-source narrowing as a supported option.

Successful output may include:

- `operation`;
- `builder_run_id`;
- `candidate_diagnostics`;
- `base_revision`;
- `draft_revision`;
- `draft_status`.

This output proves that a candidate passed local schema validation, deterministic audit, and draft persistence. It is not live execution proof.

## Examples

Builder-authored examples are draft examples unless an owner document records separate live execution evidence.

Valid draft examples may demonstrate:

- the wrapper shape required by `builder-output.schema.json`;
- portable memory bindings with no local provider data;
- runtime options limited to the supported keys and adapter capabilities;
- `orchestrator_agent` nodes without hidden managed-subagent task-package internals.

Invalid examples should demonstrate rejection of:

- wrapper-level `diagnostics` or other extra fields;
- `runtime_options.speed_tier = "standard"`;
- unrecognized runtime option keys such as `temperature`;
- invalid JSON output schemas;
- local provider secrets, runtime account data, rate limits, executable paths, or hidden managed-subagent fields.

## Remaining Public-Launch Gaps

Stage 9 does not unlock a full public Builder 2.0 readiness claim. Remaining gaps include:

- real-runtime and real-provider execution proof for representative builder-authored drafts;
- integrated product-flow evidence combining builder, lifecycle, runtime features, memory, interaction, and managed subagents;
- user-facing failure-mode documentation for rejected candidates;
- stable CLI/API output compatibility owned by Stage 10;
- broader packaged/public distribution proof owned by release engineering.

<a id="russian"></a>
# Productization Builder 2.0

Статус: канонический документ-владелец Stage 9 для готовности Builder 2.0 к публичному запуску. Stage 9 фиксирует только ограниченную, проверяемую и draft-first поверхность авторинга. Документ не заявляет, что Builder 2.0 является полноценным публичным authoring-продуктом, и не доказывает, что каждый созданный draft выполняется везде.

## Решение Stage 9

Builder 2.0 productized только как audited draft-authoring surface для текущего публичного portable Agent JSON contract.

Поддерживаемая форма:

- builder возвращает wrapper `{"agent_file": <portable-agent-json>}`;
- Core извлекает и валидирует `agent_file`;
- Core выполняет deterministic candidate audit до сохранения;
- принятый candidate сохраняется только как draft revision;
- diagnostics выводятся вне Agent JSON;
- deploy, регистрация provider/runtime, live execution proof и integrated proof остаются отдельными шагами.

## Граница продукта

Builder может создавать intent для public runtime, memory, interaction и `orchestrator_agent` surfaces, но не становится владельцем этих подсистем.

Он не должен:

- выполнять deploy или делать draft live;
- регистрировать memory providers или runtime accounts;
- сохранять secrets, credentials, local package paths, provider registrations, account metadata или rate limits;
- использовать hidden managed-subagent internals;
- доказывать выполнение draft на всех runtimes;
- обходить capability gates;
- записывать diagnostics внутрь Agent JSON.

## Оставшиеся риски

Stage 9 не разблокирует полный claim о public Builder 2.0 readiness. Для этого все еще нужны live/integration evidence, failure-mode docs, Stage 10 compatibility freeze и release-engineering proof.
