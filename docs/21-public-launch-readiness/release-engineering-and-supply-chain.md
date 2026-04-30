[English](#english)

<a id="english"></a>
# Release Engineering And Supply Chain

Status: canonical Stage 4 release-engineering and supply-chain foundation for the `cli-package-first-public-launch` target, extended with a safe OSS release-candidate workflow for later public publication. This document records the private package foundation, deterministic guards, release CI shape, and publication blockers. Stage 11 owns local tarball distribution proof. This document is not publication proof, a completed npm provenance attestation, or a public distribution claim.

Related documents:

- [Public Launch Scope](./public-launch-scope.md)
- [Security, Privacy, And Legal Foundation](./security-privacy-legal-foundation.md)
- [Baseline Gap And Forbidden Claims](./baseline-gap-and-forbidden-claims.md)
- [Release Scope Lock](../20-real-world-proof-and-release/release-scope-lock.md)
- [Release Decision Record](../20-real-world-proof-and-release/release-decision-record.md)
- [Release Gates](../11-hardening/release-gates.md)
- [Stage 11 Distribution Proof](./distribution-proof.md)
- [Supply Chain Attestation](./supply-chain-attestation.md)
- [Release Settings User Checklist](./release-settings-user-checklist.md)

## Stage 4 Decision

The selected launch shape remains CLI/package-first. `repository-public-preview` is achieved, but the npm package must stay private until a later explicit release-approval task changes that state.

Stage 4 establishes deterministic local checks and unambiguous private package metadata. It must not publish to npm, set `private` to `false`, create tags, push commits, claim public package installation proof, or imply public distribution readiness. Local `.tgz` install/uninstall proof, upgrade/rollback harness boundaries, and local SBOM validation are owned by [Stage 11 Distribution Proof](./distribution-proof.md).

## Current Package Foundation

The current foundation is:

- `package.json` keeps `"private": true`;
- `package.json` declares `license: "Apache-2.0"`, matching the root Apache-2.0 `LICENSE`;
- the CLI entry point is declared through `bin.dennett-agent-orchestrator`;
- package contents are constrained by the `files` allowlist;
- `scripts/check-packlist.js` validates the dry-run npm package inventory;
- `scripts/check-distribution.js` validates generated distribution shape and CLI help;
- `scripts/check-distribution.js` also owns Stage 11 local package install/uninstall proof, explicit two-tarball upgrade/rollback proof, and local SBOM validation;
- `scripts/check-release-candidate.js` validates repository hygiene for candidate contents;
- `SECURITY.md` defines the current vulnerability-reporting and supported-surface boundary;
- CI runs typecheck, lint, tests, build, generated distribution checks, package inventory checks, and release-candidate hygiene checks.
- `.github/workflows/release.yml` runs release-candidate validation from `v*` tags or manual dispatch, retains a candidate npm tarball, retained SPDX SBOM JSON, npm pack metadata, and a SHA-256 manifest, and only attempts npm publication from an explicitly confirmed manual dispatch.

These controls are private package foundation checks plus local artifact proof hooks. They do not prove that the package is published, installable from a public registry, signed, reproducible across platforms, or safe for general availability.

`package.json` `"private": true` is an npm publication guard, not GitHub repository visibility. The GitHub repository can be public while package publication remains intentionally blocked.

## Release Candidate Workflow

The release workflow is intentionally split into validation and publication gates:

- tag pushes matching `v*` run validation only;
- manual `workflow_dispatch` defaults to `dry-run` and also runs validation only;
- manual `workflow_dispatch` with `release_mode: publish` is the only path that can reach `npm publish`;
- publish mode must run from a selected Git tag whose name equals `v${package.json.version}`;
- publish mode requires exact typed confirmations for `package.json` name and version;
- publish mode fails while `package.json` has `"private": true`;
- publish mode uses `npm publish <validated-tarball> --access public` with GitHub OIDC/trusted publishing instead of a long-lived npm token;
- publication is attached to the `npm-production` GitHub environment so repository maintainers can add deployment approvals before enabling release.

The validation job runs the same local quality/package gates as CI, packs the candidate with `npm pack --ignore-scripts`, generates a retained SPDX SBOM through `pnpm supply-chain:local:proof -- --from-tgz <tarball> --output <sbom>`, records SHA-256 hashes, and uploads those files as GitHub Actions artifacts. When the repository is public, the workflow also requests GitHub artifact attestations for the tarball and the SBOM.

The workflow requires npm trusted publishing support rather than an `NPM_TOKEN`. Per npm's trusted publishing documentation, GitHub Actions publication needs `id-token: write`, a trusted publisher entry whose workflow filename matches `release.yml`, and an npm CLI/new enough Node runtime that supports OIDC. npm trusted publishing automatically publishes npm provenance for public packages from public repositories, so the workflow does not add a separate `--provenance` flag.

Reference documentation:

- [npm Trusted Publishing](https://docs.npmjs.com/trusted-publishers/)
- [GitHub Artifact Attestations](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations)

## Manual External Prerequisites For Publication

Before the publish gate may be used, maintainers must complete and record evidence for all of the following:

- transfer the package from private package foundation to an approved public package state, including coordinated `package.json` changes such as removing `"private": true`;
- create or verify the npm package namespace and package ownership;
- configure npm Trusted Publisher for owner `Andrey-Good`, repository `dennett-agent-orchestrator`, workflow filename `release.yml`, and the chosen GitHub environment if one is required by npm settings;
- configure the GitHub `npm-production` environment with required reviewers and any branch/tag deployment restrictions;
- protect release tags or otherwise restrict who can create `v*` tags;
- ensure the selected GitHub-hosted runner provides Node `>=22.14.0` and npm CLI `>=11.5.1`;
- keep the repository public before relying on public npm provenance or public GitHub artifact attestations; this repository-visibility prerequisite is currently satisfied by the 2026-04-30 public-preview evidence;
- define release-note, changelog, rollback, and post-publish verification ownership.

The exact user/admin actions and close-out evidence for these external settings are tracked in [Release Settings User Checklist](./release-settings-user-checklist.md). Those actions are not satisfied by repository code changes alone.

## Deterministic Foundation Guard

`pnpm public-release-foundation:check` validates Stage 4 invariants without requiring publication:

- package license is `Apache-2.0`;
- package remains private;
- package `files` allowlist is present and non-empty;
- packlist and release-candidate guard scripts exist;
- required release-foundation package scripts exist;
- `SECURITY.md` exists;
- Stage 1 through Stage 4 public-launch documents exist and the section README links the Stage 4 document.

The guard may report future publication blockers as non-failing output when they are not yet required for the private Stage 4 foundation.

## Future Publication Blockers

The CLI/package-first public launch remains blocked until later stages produce evidence for:

- package namespace ownership and final package metadata such as repository, bugs, homepage, and keywords;
- public registry install, upgrade, uninstall, and rollback proof from the selected public artifact;
- public registry publication dry run or actual approved publication evidence;
- retained SBOM artifact path, publication attachment, and retention policy;
- provenance, signing, or an explicit public-release decision that names what is not signed and why; release CI can generate candidate artifacts and public-repository GitHub attestations, but npm provenance exists only after an approved trusted-publishing run publishes a public package from a public repository;
- dependency audit posture and license-review process;
- release notes, changelog, versioning, branch, tag, and rollback process;
- OS-specific install and CLI smoke proof for every claimed supported OS;
- CI parity with the exact public package gates;
- user-facing install documentation that matches the proven artifact and support boundary.

Before removing `"private": true`, the release-preparation task must record npm ownership, final version/tag/release notes, approved publish path, package metadata/packlist review, retained SBOM plus `SHA256SUMS` posture, npm provenance preference, signing defer-or-implement decision, and a post-publish public install/CLI smoke/uninstall proof plan.

## Forbidden Claims

Do not claim:

- the package has been published;
- npm or another public registry distribution is proven;
- public registry install, upgrade, uninstall, rollback, retained SBOM, signing, provenance, or reproducible-build proof exists;
- release-candidate artifacts are equivalent to published npm package evidence;
- package namespace ownership has been verified unless the proving task records evidence;
- hosted, managed, installer, container, or signed-binary launch is in scope;
- Stage 4 alone makes the product generally available or production ready.

## Acceptance Rule For Later Stages

A later stage may remove a blocker only by naming the exact artifact, environment, command, evidence, and resulting user-visible claim. If the evidence is local-only, private-only, OS-specific, registry-specific, or provider-specific, the public claim must keep the same limitation.
