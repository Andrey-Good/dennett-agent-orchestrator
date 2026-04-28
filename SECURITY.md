# Security Policy

## Scope

This policy covers vulnerabilities in this repository and in the documented CLI/package-first launch path.

Current public-launch readiness does not include a hosted service, managed deployment, installer, container, SLA, or long-term support promise. Reports about those deferred surfaces are useful as design input, but they are not supported deployed surfaces in this stage.

## Supported Versions

After the bounded Stage 10 CLI/API freeze, security reports are triaged for:

- the current public repository state;
- the latest documented bounded local CLI/repository release candidate;
- the commands labeled `[stable]` by CLI help;
- the `[stable/safety-protocol]` memory cleanup flow;
- exported JSON schema artifacts under `contracts/json-schema/*.schema.json`;
- the selected CLI/package-first path once a later stage proves and publishes the package artifact.

Commands labeled `[experimental]`, deep imports from `dist` or `src`, older commits, forks, private modifications, generated local artifacts, and third-party provider deployments are outside this project's direct support boundary.

## Reporting A Vulnerability

Do not include exploit details, secrets, tokens, private prompts, private memory records, or provider account data in a public issue.

Preferred reporting path:

1. Use GitHub private vulnerability reporting if it is enabled for this repository.
2. If private reporting is not available, open a minimal public issue that says a security report exists and request a private contact path. Do not include reproduction details publicly.

Please include, when safe:

- affected commit, branch, package artifact, or documentation path;
- whether the issue affects CLI execution, package installation, runtime adapter behavior, memory provider integration, Builder output, MCP/plugin/skill handling, or local state;
- minimal reproduction steps using synthetic data;
- expected impact and any known workaround.

## Third-Party Providers And Dependencies

Dennett integrates with runtimes, memory providers, MCP servers, plugins, skills, package registries, and language ecosystems. If the vulnerability is in a third-party service or dependency, report it to the upstream owner as well. Also report it here when Dennett's integration, documentation, configuration, or packaging makes the issue exploitable for Dennett users.

## Handling Expectations

Maintainers should acknowledge security reports when practical, avoid requesting unnecessary sensitive data, and keep public discussion limited until a fix or documented mitigation is available.

This policy is a disclosure and triage boundary. It is not a hosted incident-response promise, external audit result, or certification.
