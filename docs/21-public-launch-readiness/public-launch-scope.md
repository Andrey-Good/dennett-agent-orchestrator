[English](#english) | [Russian](#russian)

<a id="english"></a>
# Public Launch Scope

Status: canonical Stage 2 public-launch scope decision for Part 1. This document selects the public-launch target shape only; it does not claim the target is ready, published, hosted, or generally available.

Related documents:

- [Baseline Gap And Forbidden Claims](./baseline-gap-and-forbidden-claims.md)
- [Release Scope Lock](../20-real-world-proof-and-release/release-scope-lock.md)
- [Release Decision Record](../20-real-world-proof-and-release/release-decision-record.md)
- [Phase 12 Capability Gap Lock](../13-capability-gap-lock/phase-12-capability-gap-lock.md)
- [Release Gates](../11-hardening/release-gates.md)
- [Managed Subagent Productization](./managed-subagent-productization.md)
- [Builder 2.0 Productization](./builder-2-0-productization.md)
- [Stable CLI/API Contract Freeze](./stable-cli-api-contract-freeze.md)
- [Stage 11 Distribution Proof](./distribution-proof.md)
- [Install, Upgrade, Uninstall, And Rollback](./install-upgrade-uninstall-rollback.md)
- [Package Identity And Registry](./package-identity-and-registry.md)
- [Supply Chain Attestation](./supply-chain-attestation.md)

## Decision

The selected Part 1 public-launch target is:

`cli-package-first-public-launch`

This means the first public launch should be a user-installed CLI/package distribution, with repository checkout support preserved for contributors and early technical users.

The launch target is selected because current evidence supports a local CLI/repository product shape and controlled local package proof better than it supports any hosted or managed service. Stage 4 owns the private package and supply-chain foundation. Stage 11 owns local `.tgz` distribution proof. Public registry publication remains blocked until a later release-approval task records registry ownership, publication controls, retained evidence, and public install proof.

## Hosted And Managed Status

Hosted and managed product launch is explicitly deferred.

Deferred hosted/managed scope includes:

- hosted SaaS operation;
- managed deployments;
- uptime, availability, or service-level promises;
- multi-tenant isolation claims;
- hosted rollback or disablement;
- hosted observability, incident response, and support operations.

Hosted or managed launch may enter scope only through a later scope decision that names the deployment artifact, runtime environment, rollback path, operational owner, security/privacy/legal posture, and live evidence requirements.

## Supported Matrix For The Selected Launch Target

| Area | Stage 2 launch-scope status | Public-launch boundary |
| --- | --- | --- |
| Launch form | Selected target: CLI/package-first. | Not ready or published until security/legal, release-engineering, Stage 11 distribution proof, and later release-approval gates pass for the chosen public artifact. |
| Repository checkout | Supported as contributor and local-user path by the bounded `local-cli-repository-readiness` evidence. | Users build from checkout with `pnpm build`; generated `dist` is not promised in a clean checkout. |
| Package distribution | Local tarball proof exists; public registry publication is not proven. | Stage 11 proves controlled local `.tgz` install/uninstall, explicit two-tarball upgrade/rollback smoke, local SBOM validation, and CI package-proof job configuration. It does not prove public npm publication, signing, provenance, retained SBOMs, or public registry install. |
| Hosted/managed service | Deferred. | No hosted, managed, SaaS, uptime, multi-tenant, or hosted rollback claim. |
| Primary OS evidence | Windows local evidence. | Windows is the only evidenced OS baseline for current local release proof. |
| Linux and macOS | CI package-proof jobs are configured as evidence candidates. | No public support claim until green package proof, gates, CLI smoke, and runtime/provider proof are recorded for each claimed OS. |
| Node.js | `package.json` requires `>=22.13.0`; evidence records Node v22.17.1 and `node:sqlite` import proof. | Public launch must require Node.js `>=22.13.0` unless Stage 4 changes package metadata through its own approved task. |
| Package manager | Canonical workflow is `pnpm`; package metadata records `pnpm@10.33.0`. | Contributors and source-checkout users should use `pnpm 10.33.0` or a compatible pnpm 10.x path proven by later evidence. `npm` may be used only where the package/install path explicitly requires it. |
| npm package manager | Consumer-package tool for local tarball proof. | Do not claim npm is the canonical repository workflow. Do not claim npm publication or public `npm install dennett-agent-orchestrator` support before later registry proof and release approval. |
| Runtime provider | Codex App Server adapter path only, with narrow local proof for runtime discovery, environment inspection, and minimal graph execution. | Do not claim full App Server certification, all model/options support, or broad runtime-provider reliability. |
| Memory provider | Direct local Mem0 provider path, plus narrow prompt-rendered Codex memory context and success-only provider writes. | Do not claim native App Server memory, broad memory-provider support, durable cleanup beyond verified scoped Mem0 namespace cleanup, true restore, provider-wide cleanup, or provider reliability. |
| Local state | SQLite local metadata and run state. | SQLite remains local and derivative, not hosted storage or a distributed operational backend. |

## Capability Scope

| Capability | Public-launch classification | Boundary |
| --- | --- | --- |
| CLI execution from installed package or local checkout | Included target for repository checkout and local `.tgz` proof. | Only commands and outputs frozen by Stage 10 may be called stable. Public registry install remains unproven. |
| Agent JSON validation and contract examples | Included target. | Must stay within documented schemas and examples; no hidden builder-only or hosted-only behavior. |
| Local graph execution | Included target for proven local CLI paths. | Does not imply hosted execution, production load, or automatic live crash recovery. |
| Codex App Server runtime | Limited/beta for the certified subset. | Stage 5 must name supported models/options and unsupported cases before public claims expand. |
| Runtime discovery and environment inspection | Limited/beta for local authenticated Codex path. | Redact account data and avoid account/rate-limit promises. |
| Direct local Mem0 memory operations | Limited/beta for registered local Mem0 provider path. | Stage 6 must lock provider limits, cleanup guarantees, reliability boundaries, and unsupported cases. |
| Runtime memory with Codex plus Mem0 | Limited/beta for prompt-rendered context and success-only writes. | Not native App Server memory and not broad provider support. |
| User prompt wait/reply/resume | Limited/beta for currently tested prompt shapes. | Stage 7 must prove full user-visible interaction semantics before broader claims. |
| Managed subagent primitives | Limited/beta for the bounded local CLI operator surface. | Stage 8 supports `subagent-launch`, `subagent-list`, `subagent-show`, `subagent-wait`, `subagent-record-control`, and `subagent-close` only within the limits in [Managed Subagent Productization](./managed-subagent-productization.md). Launch is launch-and-wait only; control and cancellation are state-recorded, not live-delivered. |
| Builder 2.0 authoring | Limited/beta for audited draft-first authoring only. | Stage 9 supports formal builder output wrapper validation, deterministic candidate audit, diagnostics outside Agent JSON, and draft-only persistence as documented in [Builder 2.0 Productization](./builder-2-0-productization.md). It does not prove full public authoring readiness, deploy, provider registration, live managed orchestration, or execution of every draft. |
| Stable CLI/API compatibility | Frozen only for the bounded Stage 10 surface. | Only commands labeled `[stable]`, the `[stable/safety-protocol]` cleanup flow, exported JSON schema artifacts, and the no-stable-JS-API package boundary are stable under [Stable CLI/API Contract Freeze](./stable-cli-api-contract-freeze.md). Experimental commands remain unstable. |
| Containers, installers, signed binaries, hosted deployments | Deferred. | Each requires a separate artifact, proof, rollback/uninstall path, and security/release decision. |

## Public Claims Allowed After Required Gates

These claims are allowed only after the later Part 1 stages produce the required evidence for the selected CLI/package-first target:

- Dennett provides a public CLI/package-first launch for the documented supported environment.
- The documented CLI package or install path has passed clean-environment install and smoke validation.
- The documented command set is supported within the Stage 10 compatibility policy.
- Codex App Server integration is supported only for the certified subset named by Stage 5.
- Mem0 memory integration is supported only for the provider behavior named by Stage 6.
- Limitations, unsupported providers, unsupported OSes, and beta/limited features are public and visible.

## Forbidden Claims

Do not claim:

- Dennett is already publicly launched, generally available, fully released, or production ready because Stage 2 selected a launch target.
- Hosted service operation, managed deployment, SaaS readiness, uptime, multi-tenancy, hosted rollback, or hosted support operations are in scope.
- npm publication, installer distribution, container distribution, signed artifacts, provenance, retained SBOMs, or public package rollback are proven by Stage 11 local tarball proof.
- Linux or macOS are publicly supported before OS-specific evidence exists.
- Full Codex App Server certification is complete.
- Any non-Codex runtime provider is publicly supported.
- Native App Server memory is implemented or Mem0 is consumed through a native App Server memory primitive.
- Memory behavior is broader than registered local provider resolution, prompt-rendered Codex context, and success-only provider writes.
- Durable external provider cleanup, true restore, graph-store cleanup, provider-wide cleanup, delete-all, throttling behavior, or volume reliability is proven.
- Full user interaction readiness, complete managed-subagent orchestration, durable background subagent execution, live subagent cancellation delivery, complete public Builder 2.0 readiness beyond audited draft-first authoring, stable compatibility for experimental CLI commands, or any stable JS/TS API is complete.

## Launch Blockers

The CLI/package-first public launch remains blocked until all selected launch-scope gates pass:

- Stage 3 security, privacy, legal, secret-handling, license/package, disclosure, and data-retention decisions are complete.
- Stage 4 records the private package and supply-chain foundation.
- Stage 11 records controlled local `.tgz` install/uninstall proof, explicit two-tarball upgrade/rollback smoke, local SBOM validation, and CI package-proof job configuration.
- A later release-approval task records registry ownership, public publication controls, retained evidence, signing/provenance decisions, and public install proof.
- Stage 5 certifies the exact Codex App Server runtime subset and unsupported model/option cases.
- Stage 6 productizes the exact memory provider boundary and cleanup/reliability limits.
- Stage 7 records user-visible interaction semantics and unsupported prompt/reply cases.
- Stage 8 records the bounded local CLI managed-subagent operator surface and keeps broader orchestration explicitly deferred.
- Stage 9 records the bounded audited draft-first Builder 2.0 authoring surface and keeps complete public Builder readiness explicitly deferred.
- Stage 10 freezes only the bounded public CLI/API contract and compatibility policy documented in [Stable CLI/API Contract Freeze](./stable-cli-api-contract-freeze.md).
- README and user-facing docs use the same scope language as this document.

## Decision Criteria For Later Scope Changes

A later stage may expand the scope only when:

1. the expansion names exact user-visible behavior and non-goals;
2. implementation, docs, and tests match the same boundary;
3. artifact, runtime, provider, or hosted claims have live or clean-environment evidence;
4. rollback, uninstall, cleanup, or recovery is proven where the claim implies operational responsibility;
5. failed, blocked, inconclusive, and superseded evidence remains visible;
6. public docs include limitations and forbidden claims with no contradictory marketing language.

<a id="russian"></a>
# Область публичного запуска

Статус: каноническое Stage 2 решение об области публичного запуска для Part 1. Этот документ выбирает только форму цели публичного запуска; он не утверждает, что цель готова, опубликована, размещена как сервис или общедоступна.

Связанные документы:

- [Baseline Gap And Forbidden Claims](./baseline-gap-and-forbidden-claims.md)
- [Release Scope Lock](../20-real-world-proof-and-release/release-scope-lock.md)
- [Release Decision Record](../20-real-world-proof-and-release/release-decision-record.md)
- [Phase 12 Capability Gap Lock](../13-capability-gap-lock/phase-12-capability-gap-lock.md)
- [Release Gates](../11-hardening/release-gates.md)
- [Managed Subagent Productization](./managed-subagent-productization.md)
- [Builder 2.0 Productization](./builder-2-0-productization.md)
- [Stable CLI/API Contract Freeze](./stable-cli-api-contract-freeze.md)

## Решение

Выбранная цель публичного запуска Part 1:

`cli-package-first-public-launch`

Это означает, что первый публичный запуск должен быть CLI/package distribution, устанавливаемой пользователем, при сохранении repository checkout как пути для контрибьюторов и ранних технических пользователей.

Цель выбрана потому, что текущие доказательства лучше поддерживают локальный CLI/repository product shape и controlled local package proof, чем hosted или managed service. Stage 4 владеет private package and supply-chain foundation. Stage 11 владеет local `.tgz` distribution proof. Public registry publication остается blocked до отдельной release-approval task с registry ownership, publication controls, retained evidence и public install proof.

## Статус Hosted И Managed

Hosted и managed product launch явно отложены.

Отложенная hosted/managed область включает:

- hosted SaaS operation;
- managed deployments;
- uptime, availability или service-level promises;
- multi-tenant isolation claims;
- hosted rollback или disablement;
- hosted observability, incident response и support operations.

Hosted или managed launch может войти в scope только через более позднее scope decision, которое называет deployment artifact, runtime environment, rollback path, operational owner, security/privacy/legal posture и live evidence requirements.

## Матрица Поддержки Для Выбранной Цели Запуска

| Область | Stage 2 статус launch-scope | Граница публичного запуска |
| --- | --- | --- |
| Launch form | Выбранная цель: CLI/package-first. | Не готово и не опубликовано, пока security/legal, release-engineering, Stage 11 distribution proof и later release-approval gates не пройдут для выбранного public artifact. |
| Repository checkout | Поддерживается как путь для контрибьюторов и локальных пользователей доказательствами bounded `local-cli-repository-readiness`. | Пользователи собирают из checkout через `pnpm build`; generated `dist` не обещается в clean checkout. |
| Package distribution | Local tarball proof существует; public registry publication не доказана. | Stage 11 доказывает controlled local `.tgz` install/uninstall, explicit two-tarball upgrade/rollback smoke, local SBOM validation и CI package-proof job configuration. Это не доказывает public npm publication, signing, provenance, retained SBOMs или public registry install. |
| Hosted/managed service | Отложено. | Нет hosted, managed, SaaS, uptime, multi-tenant или hosted rollback claim. |
| Primary OS evidence | Windows local evidence. | Windows - единственный evidenced OS baseline для текущего local release proof. |
| Linux и macOS | CI package-proof jobs are configured as evidence candidates. | Нет public support claim до green package proof, gates, CLI smoke и runtime/provider proof на каждой claimed OS. |
| Node.js | `package.json` требует `>=22.13.0`; evidence фиксирует Node v22.17.1 и `node:sqlite` import proof. | Public launch должен требовать Node.js `>=22.13.0`, если Stage 4 не изменит package metadata в своей approved task. |
| Package manager | Канонический workflow - `pnpm`; package metadata фиксирует `pnpm@10.33.0`. | Контрибьюторы и source-checkout users должны использовать `pnpm 10.33.0` или compatible pnpm 10.x path, доказанный later evidence. `npm` может использоваться только там, где package/install path явно этого требует. |
| npm package manager | Consumer-package tool для local tarball proof. | Не заявлять, что npm является canonical repository workflow. Не заявлять npm publication или public `npm install dennett-agent-orchestrator` support до later registry proof and release approval. |
| Runtime provider | Только Codex App Server adapter path, с узким local proof для runtime discovery, environment inspection и minimal graph execution. | Не заявлять full App Server certification, all model/options support или broad runtime-provider reliability. |
| Memory provider | Direct local Mem0 provider path плюс narrow prompt-rendered Codex memory context и success-only provider writes. | Не заявлять native App Server memory, broad memory-provider support, durable cleanup beyond verified scoped Mem0 namespace cleanup, true restore, provider-wide cleanup или provider reliability. |
| Local state | SQLite local metadata and run state. | SQLite остается local and derivative, а не hosted storage или distributed operational backend. |

## Область Возможностей

| Capability | Public-launch classification | Boundary |
| --- | --- | --- |
| CLI execution from installed package or local checkout | Included target для repository checkout и local `.tgz` proof. | Только commands and outputs, frozen by Stage 10, могут называться stable. Public registry install remains unproven. |
| Agent JSON validation and contract examples | Included target. | Должны оставаться внутри documented schemas and examples; без hidden builder-only или hosted-only behavior. |
| Local graph execution | Included target для proven local CLI paths. | Не означает hosted execution, production load или automatic live crash recovery. |
| Codex App Server runtime | Limited/beta для certified subset. | Stage 5 должен назвать supported models/options и unsupported cases до расширения public claims. |
| Runtime discovery and environment inspection | Limited/beta для local authenticated Codex path. | Account data редактировать; не давать account/rate-limit promises. |
| Direct local Mem0 memory operations | Limited/beta для registered local Mem0 provider path. | Stage 6 должен зафиксировать provider limits, cleanup guarantees, reliability boundaries и unsupported cases. |
| Runtime memory with Codex plus Mem0 | Limited/beta для prompt-rendered context и success-only writes. | Не native App Server memory и не broad provider support. |
| User prompt wait/reply/resume | Limited/beta для currently tested prompt shapes. | Stage 7 должен доказать full user-visible interaction semantics до broad claims. |
| Managed subagent primitives | Limited/beta для bounded local CLI operator surface. | Stage 8 поддерживает только `subagent-launch`, `subagent-list`, `subagent-show`, `subagent-wait`, `subagent-record-control` и `subagent-close` в пределах [Managed Subagent Productization](./managed-subagent-productization.md). Launch является только launch-and-wait; control и cancellation записываются в state, а не live-deliver-ятся. |
| Builder 2.0 authoring | Limited/beta только для audited draft-first authoring. | Stage 9 поддерживает formal builder output wrapper validation, deterministic candidate audit, diagnostics вне Agent JSON и draft-only persistence по [Builder 2.0 Productization](./builder-2-0-productization.md). Это не доказывает full public authoring readiness, deploy, provider registration, live managed orchestration или execution каждого draft. |
| Stable CLI/API compatibility | Frozen only for the bounded Stage 10 surface. | Только commands с label `[stable]`, `[stable/safety-protocol]` cleanup flow, exported JSON schema artifacts и no-stable-JS-API package boundary являются stable по [Stable CLI/API Contract Freeze](./stable-cli-api-contract-freeze.md). Experimental commands остаются unstable. |
| Containers, installers, signed binaries, hosted deployments | Deferred. | Каждый требует separate artifact, proof, rollback/uninstall path и security/release decision. |

## Разрешенные Public Claims После Нужных Gates

Эти claims разрешены только после того, как более поздние Part 1 stages дадут required evidence для выбранного CLI/package-first target:

- Dennett предоставляет public CLI/package-first launch для documented supported environment.
- Documented CLI package или install path прошел clean-environment install and smoke validation.
- Documented command set поддерживается внутри Stage 10 compatibility policy.
- Codex App Server integration поддерживается только для certified subset, названного Stage 5.
- Mem0 memory integration поддерживается только для provider behavior, названного Stage 6.
- Limitations, unsupported providers, unsupported OSes и beta/limited features публичны и видимы.

## Запрещенные Claims

Не заявлять:

- Dennett уже publicly launched, generally available, fully released или production ready из-за того, что Stage 2 выбрал launch target.
- Hosted service operation, managed deployment, SaaS readiness, uptime, multi-tenancy, hosted rollback или hosted support operations входят в scope.
- npm publication, installer distribution, container distribution, signed artifacts, provenance, retained SBOMs или public package rollback доказаны Stage 11 local tarball proof.
- Linux или macOS публично поддерживаются до появления OS-specific evidence.
- Full Codex App Server certification завершена.
- Любой non-Codex runtime provider публично поддерживается.
- Native App Server memory реализована или Mem0 потребляется через native App Server memory primitive.
- Memory behavior шире registered local provider resolution, prompt-rendered Codex context и success-only provider writes.
- Durable external provider cleanup, true restore, graph-store cleanup, provider-wide cleanup, delete-all, throttling behavior или volume reliability доказаны.
- Full user interaction readiness, complete managed-subagent orchestration, durable background subagent execution, live subagent cancellation delivery, complete public Builder 2.0 readiness beyond audited draft-first authoring, stable compatibility for experimental CLI commands или any stable JS/TS API завершены.

## Блокеры Запуска

CLI/package-first public launch остается blocked, пока все selected launch-scope gates не пройдут:

- Stage 3 security, privacy, legal, secret-handling, license/package, disclosure и data-retention decisions завершены.
- Stage 4 records the private package and supply-chain foundation.
- Stage 11 records controlled local `.tgz` install/uninstall proof, explicit two-tarball upgrade/rollback smoke, local SBOM validation и CI package-proof job configuration.
- Later release-approval task records registry ownership, public publication controls, retained evidence, signing/provenance decisions и public install proof.
- Stage 5 сертифицирует exact Codex App Server runtime subset и unsupported model/option cases.
- Stage 6 productizes exact memory provider boundary и cleanup/reliability limits.
- Stage 7 фиксирует user-visible interaction semantics и unsupported prompt/reply cases.
- Stage 8 фиксирует bounded local CLI managed-subagent operator surface и явно оставляет broader orchestration deferred.
- Stage 9 фиксирует bounded audited draft-first Builder 2.0 authoring surface и явно оставляет complete public Builder readiness deferred.
- Stage 10 freezes only bounded public CLI/API contract и compatibility policy, documented in [Stable CLI/API Contract Freeze](./stable-cli-api-contract-freeze.md).
- README и user-facing docs используют тот же scope language, что и этот документ.

## Критерии Решения Для Поздних Изменений Scope

Более поздний stage может расширить scope только когда:

1. expansion называет exact user-visible behavior и non-goals;
2. implementation, docs и tests соответствуют той же boundary;
3. artifact, runtime, provider или hosted claims имеют live или clean-environment evidence;
4. rollback, uninstall, cleanup или recovery доказаны там, где claim подразумевает operational responsibility;
5. failed, blocked, inconclusive и superseded evidence остаются видимыми;
6. public docs включают limitations и forbidden claims без contradictory marketing language.
