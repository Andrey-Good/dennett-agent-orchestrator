[English](#english)

<a id="english"></a>
# Memory Productization

Status: canonical Stage 6 memory productization record for the `cli-package-first-public-launch` target. This document productizes only the bounded local Mem0-first path described here. It is not a broad provider ecosystem, hosted memory, managed backup, provider-wide cleanup, or native App Server memory certification.

Related documents:

- [Public Launch Scope](./public-launch-scope.md)
- [Security, Privacy, And Legal Foundation](./security-privacy-legal-foundation.md)
- [Runtime App Server Certification](./runtime-app-server-certification.md)
- [Memory Bindings](../08-extensions/memory-bindings.md)
- [Memory Binding Model Contract](../03-contracts/agent-json/memory-binding-model-contract.md)
- [Phase 12 Capability Gap Lock](../13-capability-gap-lock/phase-12-capability-gap-lock.md)

## Product Boundary

Stage 6 covers this memory subset only:

- local, user-owned memory provider registration in the Dennett local SQLite state database;
- Mem0 as the first implemented provider family;
- CLI registration, list, show, write, read, search, update, delete, cleanup-preview, and cleanup-verified-delete flows where supported by the current Mem0 adapter;
- portable `memory_bindings[]` that resolve by `codex_ref` to a local provider registration;
- capability and transport checks before a binding can use a registered provider;
- prompt-rendered memory context and success-only memory writes through graph execution.

Stage 6 does not cover:

- hosted, managed, SaaS, multi-tenant, or Dennett-owned memory service operation;
- broad provider availability beyond the current Mem0 family;
- automatic provider installation, account creation, credential management, or provider lifecycle ownership by Dennett;
- native Codex App Server memory;
- provider-wide deletion, full provider cleanup, graph-store cleanup, backup, restore, export, or disaster recovery;
- production reliability, uptime, retention, training, residency, or confidentiality guarantees for Mem0 or any other provider.

## Supported Setup UX

The user registers memory providers locally. Portable agent JSON must not contain credentials, local executable paths, account-bearing provider config, or private provider URLs.

Current setup shape:

```powershell
dennett-agent-orchestrator memory-provider-register mem0-local `
  --family mem0 `
  --codex-ref primary_memory `
  --display-name "Primary Mem0" `
  --config "{""python_executable"":""C:/path/to/python.exe"",""mem0_config"":{...}}"
```

Rules:

- `provider-id` is the local stable registry id.
- `codex_ref` is the portable name used by `memory_bindings[]`.
- `family` is currently limited to `mem0` in the productized slice.
- omitted Mem0 transport defaults to `sdk`.
- `--config` is local operational config and may include paths, provider config, and credential references needed by the adapter.
- setup must fail explicitly for unsupported families, unknown providers, missing capabilities, forbidden transports, and unsupported provider-specific overrides.

## Secrets And Local Config

Provider config is sensitive by default because it can contain:

- API keys, tokens, account references, endpoints, and provider-specific auth material;
- local executable paths, working directories, vector-store paths, history database paths, and private workspace paths;
- model, embedder, vector-store, or graph-store provider settings that can reveal local infrastructure.

Default public CLI behavior:

- `memory-provider-register`, `memory-provider-list`, and `memory-provider-show` redact `config` in printed JSON output;
- raw provider config remains in local state for execution and is not printed by default;
- there is currently no public safe raw-config display command;
- docs, examples, tasks, and evidence must use placeholders and must not publish real provider config.

The local SQLite state database configured by `--state-db` can contain provider config and should be treated as private operational state.

## Capability Status Matrix

| Capability | Stage 6 status | Evidence and boundary |
| --- | --- | --- |
| Local provider registry | Productized for Mem0-first local CLI/package scope | Register, list, show, local resolution, status, family, transport, capability metadata, and config persistence exist in local SQLite state. |
| Broad provider registry | Unsupported/deferred | The registry is shaped for future providers, but public support is limited to `mem0`. |
| Provider config display | Productized with default redaction | Register/list/show CLI output redacts provider `config`; raw config remains local and internal for execution. |
| Mem0 provider adapter | Productized only for local user-owned setup | The adapter invokes the configured local Python/Mem0 bridge with user-provided local config. Dennett does not install or own the provider. |
| Capability negotiation | Productized for current registry/binding path | Required memory capabilities and transport preferences fail explicitly when unsupported. |
| Memory write/read/search/list/update/delete | Productized for supported Mem0 local path | CLI and service paths exist; live provider behavior depends on the user's local Mem0 setup. |
| Namespace isolation | Productized only at the documented Dennett namespace/scope layer | Mem0 calls include Dennett metadata and namespace derivation for user, agent, and run scope. This is not tenant isolation or provider-wide partitioning. |
| Scoped cleanup preview and verified delete | Productized for explicit scope only | Cleanup requires at least one of user, agent, or run scope, previews candidate ids, requires a confirmation token, and verifies remaining records in the same namespace. |
| Provider-wide cleanup | Unsupported/deferred | No delete-all, account-wide purge, graph-store cleanup, or multi-provider cleanup claim is allowed. |
| Restart persistence | Deterministic/local persistence only | Provider registration and local state persist in SQLite across process restarts. Provider memory persistence depends on Mem0/vector-store config chosen by the user. |
| Backup and restore | Unsupported/deferred | There is no productized backup, restore, export, migration, or disaster-recovery flow for local state plus provider data. |
| Native App Server memory | Unsupported/deferred | Runtime memory is prompt-rendered context plus provider writes through the memory port, not native App Server memory. |

## Namespace Isolation And Cleanup Limits

The current Mem0 adapter scopes records through Dennett-controlled metadata and namespace derivation. The namespace can include user, agent, and run scope.

Allowed cleanup claim:

"Dennett supports scoped Mem0 cleanup preview and verified delete for an explicit user, agent, or run scope in the current local Mem0 path."

Forbidden cleanup claims:

- "Dennett can delete all provider data."
- "Dennett cleanup covers every Mem0 collection, graph store, account, or tenant."
- "Uninstalling Dennett deletes memory."
- "Cleanup works identically for all memory providers."
- "A cleanup preview is proof that no data remains anywhere in the provider."

If a user needs provider-wide deletion, graph-store deletion, account deletion, or legal/compliance erasure, they must use provider-owned tools and provider account settings unless a later stage proves a narrower Dennett-owned flow.

## Restart And Persistence Evidence

Stage 6 can claim only local deterministic persistence:

- memory provider registrations are persisted in the configured local SQLite database;
- a fresh CLI process can read the same provider registration when pointed at the same `--state-db`;
- graph execution resolves `codex_ref` through the persisted registry;
- Mem0 record persistence depends on the user's configured Mem0/vector-store/history paths or external provider settings.

Stage 6 cannot claim:

- that every Mem0 backend persists across restart;
- that transient or misconfigured provider stores retain records;
- that provider data survives machine loss, account deletion, package uninstall, or vector-store corruption;
- that Dennett can reconstruct memory data from SQLite registry state alone.

## Backup And Restore Status

Backup/restore is blocked for public claims.

Reasons:

- local Dennett SQLite state and provider-owned memory data are separate state domains;
- Mem0 may use provider-specific vector stores, graph stores, history databases, remote accounts, or local files outside Dennett's SQLite database;
- no end-to-end restore test proves recovery of registry state plus provider records plus graph memory plus runtime references;
- no export format, versioning, restore conflict policy, or integrity check is productized.

Allowed wording:

"Memory backup and restore are not currently productized. Users are responsible for backing up their configured local state database and provider storage using provider-appropriate tools."

## Data Handling

Memory records may contain prompts, user data, node outputs, derived facts, and metadata. Treat memory content as sensitive by default.

Data movement:

- writes send memory text, scope, metadata, and optional inference flags to the configured memory provider;
- search/list/read may retrieve provider records into Dennett local execution;
- memory context may be rendered into runtime prompts when a graph uses memory;
- successful node output may be written back to memory when the binding and implementation allow it.

Dennett does not promise provider retention, training, telemetry, confidentiality, deletion, residency, availability, or abuse-policy behavior. Those are provider/account responsibilities.

## Future Provider Integration Path

Future providers must be added behind the existing memory registry, memory service, and memory port boundaries. A new provider must not be documented as public-ready until it has:

- explicit family support and adapter implementation;
- setup docs with user-owned config and no secrets in agent JSON;
- capability and transport mapping;
- deterministic tests for registration, resolution, capability failures, and redaction;
- provider-specific data handling, cleanup, retention, and unsupported-case documentation;
- live proof if the public claim depends on live provider behavior.

Future adapters may expose provider-specific config through `provider_extension`, but only through explicitly documented, capability-gated subtrees. They must not silently accept credentials or local launch plumbing from portable agent JSON.

## Troubleshooting

| Symptom | Likely cause | Action |
| --- | --- | --- |
| `UNSUPPORTED_MEMORY_PROVIDER_FAMILY` | The provider family is not in the current productized slice. | Use `mem0` or wait for a documented provider adapter. |
| `MEMORY_PROVIDER_NOT_FOUND` | The requested `provider-id` or `codex_ref` is not registered in the selected `--state-db`. | Register the provider or point the CLI at the correct local state database. |
| `MEMORY_PROVIDER_CAPABILITY_MISSING` | The agent binding requires a capability that the provider registration does not advertise. | Update the local registration only if the provider really supports the capability. |
| `MEMORY_PROVIDER_TRANSPORT_FORBIDDEN` or `MEMORY_PROVIDER_TRANSPORT_MISMATCH` | The binding transport requirements conflict with the registration. | Register the provider with a supported transport or change the binding requirements. |
| Empty search/list results | Scope mismatch, wrong `codex_ref`, provider storage reset, or provider backend not persisted. | Check `--user-id`, `--agent-id`, `--run-id`, `codex_ref`, local paths, and provider storage settings. |
| Cleanup refuses to run | No explicit cleanup scope was provided. | Use at least one of `--user-id`, `--agent-id`, or `--run-id`, run preview, then pass the confirmation token to verified delete. |
| Provider config is hidden in CLI output | Default redaction is working. | Inspect local config through user-owned local files or state tools only when safe; do not publish raw config. |

## Forbidden Public Claims

Do not claim:

- "Dennett supports memory providers" without saying the public slice is Mem0-first and local/user-owned.
- "Dennett manages memory for users."
- "Dennett stores memory securely" without naming the local/provider boundary and evidence.
- "Dennett backs up or restores memory."
- "Dennett can delete all memory."
- "Mem0 integration is production-ready for all deployments."
- "Memory is isolated for tenants."
- "Native App Server memory is supported."
- "Provider config is safe to share."
- "Agent JSON contains everything needed to use memory."

## Allowed Public Claim

After Stage 6, the allowed public claim is:

"Dennett's CLI/package-first launch target includes a limited local Mem0-first memory path. Users register their own local provider config, portable agents reference it by `codex_ref`, provider capabilities are checked before use, default provider CLI output redacts config, and scoped cleanup is limited to explicit user/agent/run namespaces. Broad providers, hosted memory, native App Server memory, provider-wide cleanup, and backup/restore remain unsupported."

No shorter claim may remove "limited", "local", "Mem0-first", or "user-owned" unless a later stage records stronger evidence and updates this document.
