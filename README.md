# dennett-agent-orchestrator

Status: repository entrypoint, local checkout quickstart, and implementation map.
Owns: repository purpose, first-user path, reading order, directory intent, and top-level contributor guardrails.
Does not own: detailed architecture, formal contracts, runtime adapter protocol, lifecycle rules, or public-launch claim expansion.
Primary sources: [canonical specification](./agent_orchestrator_final_spec_v2.md), [documentation root](./docs/README.md), [foundations](./docs/01-foundations/README.md), [public docs and claims owner](./docs/21-public-launch-readiness/public-docs-onboarding-and-claims.md), and the official [OpenAI App Server material](https://developers.openai.com/codex/app-server) for Codex-specific behavior.

`dennett-agent-orchestrator` is a Codex-first orchestrator for portable agent runs. It stores agents as JSON files, executes their graphs through runtime adapters, preserves only the operational state needed for chats and resume, and keeps the agent definition separate from chats, events, and interfaces. The product CLI is a user-facing interface over Core; Codex execution belongs behind the App Server-native runtime adapter boundary.

The current executable scope is bounded local CLI/repository readiness. Stage 17 explicitly blocks public launch and allows only bounded local/package readiness continuation. The local scope includes local graph execution, local SQLite state, a Codex App Server adapter path for the documented subset, local memory-provider registration, a Mem0-first provider path, support diagnostics, and selected stable CLI commands. It does not claim public npm publication, hosted service operation, production readiness, broad provider support, native App Server memory, full App Server certification, full user interaction readiness, complete managed-subagent orchestration, completed external beta, or complete public Builder 2.0 readiness.

## Clean Checkout Quickstart

Use this path from a local source checkout. Generated `dist` output is build-local and is not tracked source.

```powershell
pnpm install --frozen-lockfile
pnpm build
node .\dist\src\interfaces\cli.js --help
node .\dist\src\interfaces\cli.js support-bundle
```

The default local state database is `.dennett/local-state.sqlite` under the checkout. Most stateful commands accept `--state-db <path>` if you want an isolated or disposable database.

The first example smoke path is documented in [examples/agents/README.md](./examples/agents/README.md). The live Codex examples use `runtime_options.model = "gpt-5.3-codex"`, but that model name is account/model availability dependent. If the authenticated Codex account cannot use that model, live execution can fail even when local schema validation and offline example tests pass.

For local tarball proof, use the Stage 11 path documented in [Stage 11 Distribution Proof](./docs/21-public-launch-readiness/distribution-proof.md):

```powershell
pnpm package:local-install:proof
```

That command proves only a controlled local `.tgz` install/uninstall smoke in a temporary npm consumer project. It is not npm publication, public registry install, signing, provenance, retained SBOM, hosted deployment, or general availability proof.

## Project Status For First-Time Users

This repository is preparing a bounded OSS-facing local CLI path, but public launch is still blocked by the current gate record. The GitHub repository URL has been locally verified as publicly reachable with `git ls-remote https://github.com/Andrey-Good/dennett-agent-orchestrator HEAD`, which returned remote HEAD `716f694819c1e84af8de2dd6de46d913001d1e67` on 2026-04-29. That proves repository URL accessibility only. The local `main` checkout used for the current launch-gate docs was still ahead of `origin/main` by 23 commits at local HEAD `241b4a50e084f15f04163a9dfcce6cededb45c41`, so those local changes are not proven on GitHub until pushed and re-verified. The package is private in `package.json`, so use a source checkout and the commands above rather than `npm install`, `npx`, or public registry instructions.

For contribution and support basics:

- Read [CONTRIBUTING.md](./CONTRIBUTING.md) before opening implementation or documentation changes.
- Use [SECURITY.md](./SECURITY.md) for vulnerability reporting and sensitive disclosure boundaries.
- Check [CHANGELOG.md](./CHANGELOG.md) for public-facing change notes and launch-boundary status.
- Use the GitHub issue templates for bugs, documentation issues, support requests, security redirects, and bounded beta feedback.

## Public Claim Boundary

Before writing README text, examples, release notes, issue templates, or public-facing docs, check [Final Public Launch Gate Decision](./docs/21-public-launch-readiness/final-public-launch-gate-decision.md) and [Public Docs, Onboarding, And Claims](./docs/21-public-launch-readiness/public-docs-onboarding-and-claims.md). Public claims must stay inside the bounded local CLI/repository evidence unless a later owner document records the required proof and explicitly expands the scope.

Allowed high-level claim:

- Dennett currently has a bounded local CLI/repository path that can be built from checkout and exercised through documented local commands.

Forbidden high-level claims:

- public npm package availability, public registry install, installer/container distribution, signed artifacts, provenance, or retained SBOMs;
- hosted, managed, SaaS, uptime, SLA, automatic telemetry, status-page, cloud deployment, production-load, or managed incident-response readiness;
- broad runtime or memory-provider support, native App Server memory, full App Server certification, full user interaction readiness, complete managed-subagent orchestration, or complete public Builder 2.0 readiness.

## What Exists In The Repository

| Path | Purpose | Implementation note |
| --- | --- | --- |
| [`contracts/`](./contracts/) | Formal contracts and schemas | Machine-checkable definitions belong here, not in prose docs. |
| [`docs/`](./docs/README.md) | Human-readable specification tree | Read this before implementing behavior that is not obvious. |
| [`examples/`](./examples/) | Human-oriented sample agents and scenarios | Examples illustrate contracts; they do not define them. |
| [`src/core/`](./src/core/) | Orchestration domain logic | Must stay free of vendor SDK imports. |
| [`src/ports/`](./src/ports/) | Internal boundaries used by core | Ports express orchestrator semantics, not vendor naming. |
| [`src/adapters/`](./src/adapters/) | Integrations with runtimes and storage technologies | Codex-specific code belongs here, behind ports. |
| [`src/interfaces/`](./src/interfaces/) | CLI and future UI-facing entrypoints | Interfaces talk to core; they do not host domain rules. |
| [`src/resources/`](./src/resources/) | Non-code resources reserved for runtime support | Keep generated or bundled resources separate from logic. |
| [`tests/`](./tests/) | Contract, unit, integration, fixture, and golden tests | Test categories are intentionally separated to reduce drift. |
| [`subagent_tasks/`](./subagent_tasks/) | Task ownership documents for delegated work | Large changes should be decomposed here before implementation. |

## Read This Before Coding

1. Start with the [canonical specification](./agent_orchestrator_final_spec_v2.md) for product identity and project laws.
2. Read the [foundations section](./docs/01-foundations/README.md) to lock scope, terminology, system boundaries, truth sources, defaults, and stack decisions.
3. Move to the relevant detailed section under [`docs/`](./docs/README.md) before changing architecture, contracts, execution logic, state, interaction, lifecycle, or extensions.
4. If a needed rule is missing and the change is significant or contested, record it through an ADR in [`docs/09-adrs`](./docs/09-adrs/README.md) instead of inventing behavior silently in code.
5. For implemented-versus-deferred capability claims, check [`docs/13-capability-gap-lock`](./docs/13-capability-gap-lock/README.md) and [`docs/21-public-launch-readiness`](./docs/21-public-launch-readiness/README.md).
6. For support diagnostics, read [`docs/21-public-launch-readiness/observability-support-operations.md`](./docs/21-public-launch-readiness/observability-support-operations.md) before sharing logs or claiming support readiness.

## Repository-Level Guardrails

- The product is an orchestrator of agent runs, not a replacement agent platform.
- The portable `agent JSON` file is the source of truth for the agent definition.
- Local metadata storage may index or support execution, but it must not redefine the agent.
- `skills`, `MCPs`, and `plugins` follow the compatible runtime ecosystem; this repository only references and routes them.
- The runtime adapter boundary is mandatory. Vendor client imports do not belong in core, contracts, or interfaces.
- The repository CLI is a product interface layer, not a Codex execution path. Codex integration belongs behind the App Server-native runtime adapter boundary.
- Chats and resume state are operational state, not the agent definition and not long-term memory by default.
