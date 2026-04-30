# Contributing

Thank you for considering a contribution to `dennett-agent-orchestrator`.

This repository is in bounded local CLI/repository readiness. Public launch, public npm publication, hosted service readiness, production readiness, and broad provider support are not approved. Keep pull requests and documentation claims inside the evidence recorded in [README.md](./README.md), [Public Docs, Onboarding, And Claims](./docs/21-public-launch-readiness/public-docs-onboarding-and-claims.md), and [Final Public Launch Gate Decision](./docs/21-public-launch-readiness/final-public-launch-gate-decision.md).

## Local Development

Prerequisites:

- Node.js `>=22.13.0`
- pnpm `10.33.0` or compatible pnpm 10.x

Set up from a local checkout:

```powershell
pnpm install --frozen-lockfile
pnpm build
node .\dist\src\interfaces\cli.js --help
node .\dist\src\interfaces\cli.js support-bundle
```

The package is not published to npm yet. Do not document or depend on public npm install, public registry install, `npx`, hosted deployment, or production service behavior unless a later gate document approves and proves that scope.

## Validation Commands

Run the checks that match your change:

```powershell
pnpm typecheck
pnpm test
pnpm lint
pnpm public-release-foundation:check
```

For packaging mechanics, use only the local proof commands:

```powershell
pnpm package:local-install:proof
pnpm package:upgrade-rollback:proof
pnpm supply-chain:local:proof
```

These commands prove local package mechanics only. They do not prove public npm publication, provenance, signing, retained SBOM publication, or general availability.

## Documentation Claim Boundaries

When editing public docs, examples, release notes, issue templates, or package-facing text:

- Say whether a path is local checkout, controlled local tarball proof, live runtime, or provider-backed.
- Mention `pnpm build`, generated `dist`, local state DBs, Codex authentication, model access, or provider registration near commands that require them.
- Separate schema/offline validation from live runtime execution.
- Do not claim public launch approval, public npm availability, hosted/SaaS readiness, production readiness, completed external beta, full App Server certification, native App Server memory, complete managed-subagent orchestration, or complete public Builder 2.0 readiness.
- Do not paste secrets, tokens, account data, private prompts, memory records, provider config, unredacted logs, or full transcripts into public issues or docs.

## Pull Request Expectations

- Keep changes scoped to one responsibility.
- Add or update tests when behavior changes.
- Prefer existing platform, runtime, framework, and toolchain capabilities over custom reimplementation.
- Keep core logic behind the established boundaries in `src/core`, `src/ports`, `src/adapters`, and `src/interfaces`.
- Record significant invented product logic in specs, ADRs, task documents, or acceptance tests.
- Explain which validation commands you ran and which checks remain unrun.

## Issue Routing

- Use the bug template for reproducible local CLI, contract, or packaging defects.
- Use the documentation template for missing, confusing, or stale docs.
- Use the support template for setup or expected-behavior questions that do not contain sensitive data.
- Follow [SECURITY.md](./SECURITY.md) for vulnerabilities or reports that require private details.
- Use external beta feedback only for bounded workflows that a maintainer explicitly asked you to exercise.

## Code Of Conduct

Participation in this repository is covered by [CODE_OF_CONDUCT.md](./CODE_OF_CONDUCT.md).
