[English](#english) | [Russian](#russian)

<a id="english"></a>
# Security, Privacy, And Legal Foundation

Status: canonical Stage 3 foundation for the `cli-package-first-public-launch` target. This document records security, privacy, legal, and trust boundaries that later public-launch stages must satisfy. It is a foundation and blocker definition, not a release approval, external audit result, or operational certification.

Related documents:

- [Public Launch Scope](./public-launch-scope.md)
- [Baseline Gap And Forbidden Claims](./baseline-gap-and-forbidden-claims.md)
- [Local Storage Model](../05-state/local-storage-model.md)
- [Secret Markers](../05-state/secret-markers.md)
- [Memory Bindings](../08-extensions/memory-bindings.md)
- [Runtime Adapter Contract](../03-contracts/runtime-adapter-contract.md)
- [Builder Agent](../08-extensions/builder-agent.md)
- [Security Policy](../../SECURITY.md)

## Stage 3 Decision

The first public-launch target remains CLI/package-first. It runs on a user-controlled machine, uses user-provided runtime and memory-provider accounts, and stores operational state locally unless a configured runtime or provider receives data as part of execution.

Hosted and managed service launch remains deferred. No Stage 3 text may be used to infer hosted operation, multi-tenant isolation, uptime, managed incident response, hosted rollback, or SaaS data-processing readiness.

## Threat Model For CLI/Package-First

Protected assets:

- user prompts, agent inputs, node outputs, chat transcripts, resume state, and run metadata;
- local agent JSON files, drafts, registry state, SQLite state, and sidecar files;
- provider registration config, runtime source config, account metadata, and credential references;
- external memory records stored through configured providers such as Mem0;
- local workspace files reachable by a runtime, tool, MCP, plugin, or generated agent;
- package artifacts and dependencies used to install or run the CLI.

Trust boundaries:

- the local OS account is trusted to run the CLI, but agent files, prompts, memory records, package dependencies, MCP servers, plugins, and runtime outputs must be treated as untrusted inputs;
- the portable agent JSON file is configuration and execution intent, not proof that an agent is safe;
- local provider registration is user-owned operational state, not portable agent truth;
- Codex App Server and any other runtime provider are external processing surfaces with their own account, retention, telemetry, and abuse policies;
- memory providers are external or local third-party systems owned by the user configuration, not by the portable agent file.

Primary threat categories:

| Threat | CLI/package-first boundary | Required public-launch control |
| --- | --- | --- |
| Malicious agent JSON | A file may request dangerous prompts, bindings, runtimes, memory use, or future tools. | Validate schema, surface capabilities before execution, document review expectations, and reject unsupported fields instead of silently interpreting them. |
| Prompt injection and data exfiltration | Runtime output, retrieved memory, and user-provided context may instruct the agent to leak files, secrets, or config. | Keep explicit permission boundaries, avoid ambient workspace access, warn that prompts and memory are untrusted, and document safe handling of sensitive inputs. |
| Secret leakage | Secrets may appear in prompts, agent files, logs, local config, runtime metadata, memory records, or transcripts. | Keep secrets out of portable agent JSON and examples, prefer environment or local secret stores, redact logs/docs, and treat secret markers as optional defense-in-depth only. |
| Unsafe filesystem, process, or network access | CLI execution runs with the user's local OS privileges and may invoke runtimes or tools that can reach local resources. | Do not imply sandboxing beyond verified runtime capability. Capability-gate any filesystem/process/network surface and document the effective runtime policy. |
| Memory provider data leakage | Memory writes may persist sensitive content in Mem0 or another provider. Prompt-rendered memory may be sent to the runtime. | Require explicit memory bindings, user-owned provider registration, visible provider boundaries, scoped cleanup language, and no broad deletion or restore claims. |
| Runtime/provider data exposure | Prompts, inputs, memory context, outputs, and account metadata may flow to a runtime provider. | Document exactly what the adapter sends, distinguish local diagnostics from provider processing, and require provider-specific user notice. |
| Dependency and supply-chain risk | Package install executes code and depends on the JavaScript and Python ecosystems plus runtime/provider tooling. | Stage 4 must keep the private package foundation and inventory guards; Stage 11 must own local tarball install/uninstall proof, upgrade/rollback harness limits, local SBOM validation, and provenance/signing deferrals. |
| Dangerous Builder output | Builder-authored drafts may include unsafe bindings, unsupported fields, or risky instructions. | Treat Builder output as untrusted draft JSON until validation and human review; Builder must not register providers, store secrets, deploy silently, or bypass contracts. |
| MCP/plugin/skill risk | MCP servers, plugins, and skills can expand tool access and data movement. | Treat them as capability grants, not passive metadata; require explicit binding, runtime support, and permission documentation before public claims. |
| Hosted-future isolation risk | A later hosted service would add tenants, shared infrastructure, server-side secrets, support access, observability, and incident obligations. | Keep hosted launch blocked until tenant isolation, data-processing, abuse, incident, observability, and deletion controls are designed and proven. |

## Security Principles

- Local-first does not mean risk-free. Local execution still has access to the user's files, accounts, provider configuration, and runtime sessions.
- Explicit capability is required. Unsupported runtime, memory, MCP, plugin, Builder, or interaction features must fail visibly instead of degrading silently.
- Portable files must not carry secrets. Agent JSON may reference local config by stable identifiers, but it must not embed credentials or account-bearing provider config.
- Reused provider primitives should stay behind stable internal contracts. Do not rebuild runtime or provider behavior when a real primitive exists, but do not expose vendor internals as portable truth.
- Data minimization is mandatory for public docs and examples. Examples must use placeholders and synthetic data.
- Runtime and memory outputs are untrusted until validated against the requested output contract and the current permission boundary.
- Deletion, cleanup, restore, rollback, and support claims require evidence. If only a scoped path is proven, only that scoped path may be described.

## Secrets And Local Config Rules

For the CLI/package-first launch target:

- secrets must not be committed to the repository, examples, docs, screenshots, task files, package metadata, or agent JSON;
- credentials should be supplied through environment variables, OS/user secret stores, or local config files excluded from version control;
- portable memory bindings may reference a local provider by `codex_ref`, but the local provider registration owns credentials, paths, account-specific config, and provider lifecycle;
- runtime source and account metadata are local diagnostics and must not become portable agent-file fields;
- logs and error reports should redact tokens, account identifiers, provider URLs that embed credentials, local private paths when not needed, and raw prompt/output data unless the user explicitly includes them;
- secret markers are off by default and are not a substitute for avoiding sensitive prompt/output content.

Before public package launch, user-facing docs must name the default local config paths, which files are safe to share, and which files may contain secrets or sensitive operational data.

## Agent JSON, Builder, MCP, Plugin, And Skill Boundaries

Agent JSON:

- is the portable source of truth for agent definition;
- may request bindings and behavior, but does not prove those bindings are available, safe, or supported;
- must be schema-validated and capability-checked before execution;
- must not carry hidden provider credentials, hidden runtime selection, or undeclared tool permissions.

Builder:

- produces draft candidate agent JSON only;
- cannot be treated as a safety reviewer for its own output;
- must not silently deploy, register providers, store secrets, create hidden managed subagents, or bypass lifecycle validation;
- should emit reviewable, diffable files that users can inspect before use.

MCPs, plugins, and skills:

- are tool and context expansion surfaces;
- may move data outside the local process or access local resources depending on the implementation;
- require explicit binding, user-visible permission semantics, and runtime support before public launch claims;
- must not be presented as harmless metadata.

## Memory Data Handling

The current memory path is user-owned and provider-backed:

- local provider registration owns the concrete provider, account, config, transport, and credentials;
- portable `memory_bindings[]` express intent and capability requirements, not provider ownership;
- Mem0 is the first real provider path, with direct CLI operations and a narrow runtime-memory path;
- Codex runtime memory is prompt-rendered context plus success-only provider writes, not native App Server memory;
- memory records may contain user data, prompt context, agent outputs, or derived facts and should be treated as sensitive by default;
- retrieved memory may be sent to the runtime when a run uses memory context;
- successful node output may be written back through the provider when the binding and current implementation allow it;
- scoped Mem0 namespace cleanup exists only within the documented namespace and explicit user, agent, or run scope.

Current memory limits that must stay visible:

- no broad provider reliability claim;
- no provider-wide delete-all claim;
- no true restore or backup claim;
- no graph-store cleanup claim;
- no multi-provider cleanup claim;
- no native App Server memory claim.

Before public package launch, Stage 6 must publish the provider-specific data notice for supported memory paths, including provider setup, data sent, data retained locally, deletion scope, unsupported cleanup cases, and user responsibility for provider account settings.

## Runtime And Provider Data Handling

Runtime execution can send the following data to the selected runtime provider:

- node prompts and resolved input payloads;
- selected runtime options and source handles;
- effective skills, MCP, plugin, and memory binding metadata when supported by the current runtime path;
- prompt-rendered memory context for supported memory runs;
- user chat replies or live comments for supported interaction paths;
- structured-output schemas and final output validation context where applicable.

Runtime introspection can expose local account and config metadata such as auth status, account status, plan type, rate-limit summaries, selected model, approval policy, sandbox mode, and service tier. This metadata is for local diagnostics and must not be promoted to portable agent truth.

Public docs must state that third-party runtime providers process data under their own terms and settings. Dennett documentation can describe what the adapter sends, but it cannot promise provider retention, training, audit, deletion, residency, or confidentiality behavior unless separately evidenced for that provider and launch scope.

## Local State Retention And Deletion

Local state may include:

- SQLite operational state for runs, chats, resume metadata, provider registry entries, indexes, and summaries;
- agent JSON files, drafts, generated artifacts, and sidecar files;
- local memory-provider config and adapter status;
- local runtime metadata caches or diagnostics where implemented;
- logs, terminal output, and task/evidence documents created by contributors.

Retention baseline:

- local state remains on the user's machine until the user deletes the relevant workspace, database, config, provider storage, or generated artifacts;
- external memory/provider state remains with that provider until deleted through the provider or a proven supported cleanup path;
- package uninstall alone must not be described as deleting all local or provider data. Stage 11 proves only removal of the installed package directory and bin from a temporary npm consumer project.

Before public package launch, documentation must provide a deletion map for the selected CLI/package artifact: local state locations, generated artifact locations, provider registry/config locations, what uninstall removes, what it leaves behind, and how to remove Mem0/provider data within the supported scope. Stage 11 provides only the local package uninstall boundary, not a full application-data deletion map.

## Telemetry Policy

For the CLI/package-first launch target:

- Dennett must not introduce automatic product telemetry without explicit documentation, user notice, and an opt-in or equivalent user-controlled mechanism approved by a later stage;
- local logs and diagnostics are not telemetry by themselves, but they may contain sensitive prompts, outputs, paths, and provider metadata;
- third-party runtimes, memory providers, package registries, MCP servers, plugins, and dependency tools may collect their own telemetry or logs under their own policies;
- hosted/managed observability, analytics, audit logs, support tooling, and incident monitoring remain out of scope until a later hosted scope decision.

Before public package launch, docs must state whether the CLI sends any Dennett-owned telemetry. If none exists, the claim must be limited to Dennett-owned telemetry and must not cover third-party providers.

## Legal And Trust Boundaries

This document is product documentation, not legal advice.

Current legal/trust posture:

- the repository includes an Apache License 2.0 file;
- dependency license and package inventory review are Stage 4 responsibilities before package publication;
- public docs must avoid unsupported privacy, compliance, safety, or production-readiness claims;
- third-party providers remain responsible for their own terms, data processing, availability, security controls, and account management;
- users are responsible for choosing what data they send to runtimes and memory providers through local configuration and agent execution.

Before public package launch, user-facing docs must include:

- license summary and link to the repository license;
- dependency/package inventory posture;
- vulnerability disclosure path;
- data-handling notice for local state, runtime providers, and memory providers;
- unsupported hosted/managed claims and deferred support surfaces.

## Vulnerability Disclosure And Supported Versions

The root [Security Policy](../../SECURITY.md) owns public vulnerability reporting instructions.

Stage 3 policy boundary:

- reports should cover the current public repository state and documented CLI/package-first scope;
- no hosted service, managed deployment, SLA, or long-term support version promise exists in this stage;
- sensitive reports should not include exploit details in public issues;
- security reports about third-party runtimes, providers, MCP servers, plugins, or dependencies may need to be reported to the upstream owner as well as to this project when Dennett integration behavior is involved.

## Hosted-Future Blockers

Hosted or managed launch cannot proceed until a later scope decision and evidence cover at least:

- tenant isolation for local state equivalents, memory providers, runtime sessions, logs, artifacts, and Builder drafts;
- server-side secret storage, rotation, access control, audit, and break-glass rules;
- hosted runtime/provider account ownership and per-tenant provider configuration;
- network egress, MCP/plugin approval, sandbox, and filesystem isolation policy;
- abuse prevention and unsafe-agent handling;
- support access to user data, support redaction rules, and incident-response procedures;
- data retention, deletion, export, backup, restore, and legal hold behavior;
- provider data-processing terms, subprocessors, residency, and user notice;
- telemetry, monitoring, audit logging, and alerting policy;
- public operational status, rollback, disablement, and security advisory process.

## Stage 3 Launch Blockers For Later Stages

Later public-launch stages must not move the CLI/package-first target forward unless the following are satisfied or explicitly kept deferred:

- Stage 4 records the selected private package foundation and inventory controls; Stage 11 records local tarball install/uninstall proof, explicit two-tarball upgrade/rollback smoke, local SBOM validation, and unsigned/unattested deferrals.
- Stage 5 publishes the exact supported Codex App Server subset and runtime data-handling boundaries.
- Stage 6 publishes provider-specific memory data handling, cleanup, retention, and unsupported cases.
- Stage 7 publishes user-visible interaction data handling for prompts, replies, blocked waits, and resume.
- Stage 8 either keeps managed subagents deferred or proves operator-facing orchestration security boundaries.
- Stage 9 either keeps Builder 2.0 deferred or proves Builder output validation, review, and unsafe-generation handling.
- Stage 10 freezes the CLI/API contract, compatibility policy, support boundary, and user-facing security/trust language.

<a id="russian"></a>
# Основа безопасности, приватности и правовых границ

Статус: каноническая основа Stage 3 для цели `cli-package-first-public-launch`. Этот документ фиксирует границы безопасности, приватности, правовых и доверительных обязательств, которые должны учитывать последующие этапы. Это не внешний аудит, не сертификация и не решение о выпуске.

Связанные документы:

- [Public Launch Scope](./public-launch-scope.md)
- [Baseline Gap And Forbidden Claims](./baseline-gap-and-forbidden-claims.md)
- [Local Storage Model](../05-state/local-storage-model.md)
- [Secret Markers](../05-state/secret-markers.md)
- [Memory Bindings](../08-extensions/memory-bindings.md)
- [Runtime Adapter Contract](../03-contracts/runtime-adapter-contract.md)
- [Builder Agent](../08-extensions/builder-agent.md)
- [Security Policy](../../SECURITY.md)

## Решение Stage 3

Первая публичная цель остается CLI/package-first. Она выполняется на машине пользователя, использует учетные записи runtime и memory providers, которые настраивает пользователь, и хранит операционное состояние локально, если настроенный runtime или provider не получает данные во время выполнения.

Hosted и managed запуск остаются отложенными. Текст Stage 3 не дает оснований заявлять hosted operation, multi-tenant isolation, uptime, managed incident response, hosted rollback или готовность SaaS data processing.

## Модель угроз для CLI/package-first

Защищаемые активы:

- prompts, inputs, outputs, transcripts, resume state и run metadata;
- local agent JSON files, drafts, registry state, SQLite state и sidecar files;
- provider registration config, runtime source config, account metadata и references на credentials;
- external memory records, сохраненные через providers, такие как Mem0;
- local workspace files, доступные runtime, tool, MCP, plugin или generated agent;
- package artifacts и dependencies, используемые для установки или запуска CLI.

Границы доверия:

- локальная учетная запись ОС доверена для запуска CLI, но agent files, prompts, memory records, package dependencies, MCP servers, plugins и runtime outputs считаются недоверенными inputs;
- portable agent JSON является configuration и execution intent, а не доказательством безопасности agent;
- local provider registration является user-owned operational state, а не portable agent truth;
- Codex App Server и другие runtime providers являются внешними processing surfaces со своими правилами account, retention, telemetry и abuse;
- memory providers являются внешними или локальными third-party systems, заданными пользовательской конфигурацией, а не portable agent file.

Основные угрозы:

| Угроза | Граница CLI/package-first | Требуемый контроль для public launch |
| --- | --- | --- |
| Malicious agent JSON | Файл может запрашивать опасные prompts, bindings, runtimes, memory use или будущие tools. | Проверять schema, показывать capabilities перед execution, документировать review expectations и reject unsupported fields без silent interpretation. |
| Prompt injection и data exfiltration | Runtime output, retrieved memory и user context могут просить agent раскрыть files, secrets или config. | Сохранять explicit permission boundaries, избегать ambient workspace access, считать prompts и memory недоверенными, документировать safe handling sensitive inputs. |
| Secret leakage | Secrets могут попасть в prompts, agent files, logs, local config, runtime metadata, memory records или transcripts. | Не хранить secrets в portable agent JSON и examples, использовать environment или local secret stores, redact logs/docs, считать secret markers только дополнительной защитой. |
| Unsafe filesystem, process или network access | CLI запускается с локальными правами пользователя и может вызывать runtimes или tools с доступом к local resources. | Не обещать sandboxing сверх проверенной runtime capability. Capability-gate любые filesystem/process/network surfaces и документировать effective runtime policy. |
| Memory provider data leakage | Memory writes могут сохранять sensitive content в Mem0 или другом provider. Prompt-rendered memory может отправляться runtime. | Требовать explicit memory bindings, user-owned provider registration, видимые provider boundaries, scoped cleanup language и отсутствие broad deletion/restore claims. |
| Runtime/provider data exposure | Prompts, inputs, memory context, outputs и account metadata могут передаваться runtime provider. | Документировать, что отправляет adapter, отделять local diagnostics от provider processing и требовать provider-specific user notice. |
| Dependency and supply-chain risk | Package install выполняет code и зависит от JavaScript/Python ecosystems и runtime/provider tooling. | Stage 4 keeps private package foundation and inventory guards; Stage 11 owns local tarball install/uninstall proof, upgrade/rollback harness limits, local SBOM validation и provenance/signing deferrals. |
| Dangerous Builder output | Builder drafts могут содержать unsafe bindings, unsupported fields или risky instructions. | Считать Builder output недоверенным draft JSON до validation и human review; Builder не должен register providers, store secrets, silently deploy или bypass contracts. |
| MCP/plugin/skill risk | MCP servers, plugins и skills расширяют tool access и data movement. | Считать их capability grants, требовать explicit binding, runtime support и permission documentation. |
| Hosted-future isolation risk | Hosted service добавит tenants, shared infrastructure, server-side secrets, support access, observability и incident obligations. | Блокировать hosted launch до design и proof для tenant isolation, data processing, abuse, incident, observability и deletion controls. |

## Принципы безопасности

- Local-first не означает отсутствие риска: локальный запуск имеет доступ к files, accounts, provider configuration и runtime sessions пользователя.
- Нужна явная capability. Unsupported runtime, memory, MCP, plugin, Builder или interaction features должны завершаться явной ошибкой.
- Portable files не должны хранить secrets. Agent JSON может ссылаться на local config через stable identifiers, но не должен включать credentials или account-bearing provider config.
- Provider primitives должны оставаться за stable internal contracts. Vendor internals не становятся portable truth.
- Public docs и examples должны использовать минимальные данные, placeholders и synthetic data.
- Runtime и memory outputs недоверены, пока они не проверены against output contract и текущей permission boundary.
- Claims о deletion, cleanup, restore, rollback и support требуют evidence. Если доказан только scoped path, описывать можно только его.

## Secrets и local config

Для CLI/package-first:

- secrets нельзя commit-ить в repository, examples, docs, screenshots, task files, package metadata или agent JSON;
- credentials следует передавать через environment variables, OS/user secret stores или local config files вне version control;
- portable memory bindings могут ссылаться на local provider через `codex_ref`, но credentials, paths, account config и provider lifecycle принадлежат local provider registration;
- runtime source и account metadata являются local diagnostics и не становятся portable agent-file fields;
- logs и error reports должны redact tokens, account identifiers, provider URLs с credentials, private paths и raw prompt/output data, если пользователь явно не включил их;
- secret markers выключены по умолчанию и не заменяют аккуратное обращение с sensitive content.

До public package launch пользовательская документация должна назвать default local config paths, какие files можно безопасно передавать, и какие files могут содержать secrets или sensitive operational data.

## Границы Agent JSON, Builder, MCP, plugin и skill

Agent JSON:

- является portable source of truth для agent definition;
- может запрашивать bindings и behavior, но не доказывает, что они доступны, безопасны или supported;
- должен проходить schema validation и capability checks перед execution;
- не должен хранить hidden provider credentials, hidden runtime selection или undeclared tool permissions.

Builder:

- создает только draft candidate agent JSON;
- не является safety reviewer собственного output;
- не должен silently deploy, register providers, store secrets, create hidden managed subagents или bypass lifecycle validation;
- должен создавать reviewable и diffable files.

MCPs, plugins и skills:

- являются surfaces для расширения tools и context;
- могут передавать data за пределы local process или получать доступ к local resources;
- требуют explicit binding, user-visible permission semantics и runtime support до public launch claims;
- не должны описываться как harmless metadata.

## Обработка memory data

Текущий memory path является user-owned и provider-backed:

- local provider registration владеет provider, account, config, transport и credentials;
- portable `memory_bindings[]` выражают intent и capability requirements, а не ownership provider;
- Mem0 является первым real provider path, с direct CLI operations и узким runtime-memory path;
- Codex runtime memory является prompt-rendered context плюс success-only provider writes, а не native App Server memory;
- memory records могут содержать user data, prompt context, agent outputs или derived facts и считаются sensitive by default;
- retrieved memory может отправляться runtime, когда run использует memory context;
- successful node output может записываться обратно через provider, если binding и implementation это позволяют;
- scoped Mem0 namespace cleanup существует только в documented namespace и explicit user, agent или run scope.

Ограничения memory должны оставаться видимыми:

- нет broad provider reliability claim;
- нет provider-wide delete-all claim;
- нет true restore или backup claim;
- нет graph-store cleanup claim;
- нет multi-provider cleanup claim;
- нет native App Server memory claim.

До public package launch Stage 6 должен опубликовать provider-specific data notice для supported memory paths: provider setup, data sent, data retained locally, deletion scope, unsupported cleanup cases и responsibility пользователя за provider account settings.

## Runtime и provider data handling

Runtime execution может отправлять выбранному runtime provider:

- node prompts и resolved input payloads;
- selected runtime options и source handles;
- effective skills, MCP, plugin и memory binding metadata, если это supported текущим runtime path;
- prompt-rendered memory context для supported memory runs;
- user chat replies или live comments для supported interaction paths;
- structured-output schemas и final output validation context, если применимо.

Runtime introspection может показывать local account и config metadata: auth status, account status, plan type, rate-limit summaries, selected model, approval policy, sandbox mode и service tier. Эти metadata нужны для local diagnostics и не становятся portable agent truth.

Public docs должны говорить, что third-party runtime providers обрабатывают data по своим terms и settings. Dennett docs могут описывать, что adapter отправляет, но не могут обещать provider retention, training, audit, deletion, residency или confidentiality behavior без отдельного evidence для provider и launch scope.

## Retention и deletion локального состояния

Local state может включать:

- SQLite operational state для runs, chats, resume metadata, provider registry entries, indexes и summaries;
- agent JSON files, drafts, generated artifacts и sidecar files;
- local memory-provider config и adapter status;
- local runtime metadata caches или diagnostics;
- logs, terminal output и task/evidence documents.

Baseline retention:

- local state остается на машине пользователя, пока пользователь не удалит workspace, database, config, provider storage или generated artifacts;
- external memory/provider state остается у provider, пока не удалено через provider или доказанный supported cleanup path;
- package uninstall нельзя описывать как удаление всех local/provider data. Stage 11 доказывает только removal of the installed package directory and bin from a temporary npm consumer project.

До public package launch документация должна дать deletion map для selected CLI/package artifact: local state locations, generated artifact locations, provider registry/config locations, что uninstall удаляет, что остается, и как удалить Mem0/provider data в supported scope.

## Telemetry policy

Для CLI/package-first:

- Dennett не должен вводить automatic product telemetry без explicit documentation, user notice и opt-in или другого user-controlled mechanism, утвержденного более поздним stage;
- local logs и diagnostics сами по себе не telemetry, но могут содержать sensitive prompts, outputs, paths и provider metadata;
- third-party runtimes, memory providers, package registries, MCP servers, plugins и dependency tools могут собирать собственные telemetry/logs по своим policies;
- hosted/managed observability, analytics, audit logs, support tooling и incident monitoring остаются out of scope.

До public package launch docs должны сказать, отправляет ли CLI какую-либо Dennett-owned telemetry. Если нет, claim должен ограничиваться Dennett-owned telemetry и не распространяться на third-party providers.

## Legal и trust boundaries

Этот документ является product documentation, а не legal advice.

Текущая legal/trust posture:

- repository содержит Apache License 2.0;
- dependency license и package inventory review являются responsibility Stage 4 перед package publication;
- public docs не должны содержать unsupported privacy, compliance, safety или production-readiness claims;
- third-party providers отвечают за свои terms, data processing, availability, security controls и account management;
- users отвечают за выбор data, которые они отправляют runtimes и memory providers через local configuration и agent execution.

До public package launch user-facing docs должны включить:

- license summary и link на repository license;
- dependency/package inventory posture;
- vulnerability disclosure path;
- data-handling notice для local state, runtime providers и memory providers;
- unsupported hosted/managed claims и deferred support surfaces.

## Vulnerability disclosure и supported versions

Root [Security Policy](../../SECURITY.md) владеет public vulnerability reporting instructions.

Граница Stage 3:

- reports должны относиться к current public repository state и documented CLI/package-first scope;
- на этом stage нет hosted service, managed deployment, SLA или long-term support version promise;
- sensitive reports не должны публиковать exploit details в public issues;
- security reports о third-party runtimes, providers, MCP servers, plugins или dependencies могут требовать report upstream owner, а также этому project, если затронута Dennett integration behavior.

## Hosted-future blockers

Hosted или managed launch невозможен, пока later scope decision и evidence не покроют:

- tenant isolation для equivalents local state, memory providers, runtime sessions, logs, artifacts и Builder drafts;
- server-side secret storage, rotation, access control, audit и break-glass rules;
- hosted runtime/provider account ownership и per-tenant provider configuration;
- network egress, MCP/plugin approval, sandbox и filesystem isolation policy;
- abuse prevention и unsafe-agent handling;
- support access к user data, support redaction rules и incident-response procedures;
- data retention, deletion, export, backup, restore и legal hold behavior;
- provider data-processing terms, subprocessors, residency и user notice;
- telemetry, monitoring, audit logging и alerting policy;
- public operational status, rollback, disablement и security advisory process.

## Blockers Stage 3 для later stages

Следующие public-launch stages не должны продвигать CLI/package-first target, пока эти пункты не выполнены или явно не оставлены deferred:

- Stage 4 records selected private package foundation and inventory controls; Stage 11 records local tarball install/uninstall proof, explicit two-tarball upgrade/rollback smoke, local SBOM validation и unsigned/unattested deferrals.
- Stage 5 публикует exact supported Codex App Server subset и runtime data-handling boundaries.
- Stage 6 публикует provider-specific memory data handling, cleanup, retention и unsupported cases.
- Stage 7 публикует user-visible interaction data handling для prompts, replies, blocked waits и resume.
- Stage 8 либо оставляет managed subagents deferred, либо доказывает operator-facing orchestration security boundaries.
- Stage 9 либо оставляет Builder 2.0 deferred, либо доказывает Builder output validation, review и unsafe-generation handling.
- Stage 10 freezes CLI/API contract, compatibility policy, support boundary и user-facing security/trust language.
