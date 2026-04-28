[English](#english)

<a id="english"></a>
# Release Engineering And Supply Chain

Status: canonical Stage 4 release-engineering and supply-chain foundation for the `cli-package-first-public-launch` target. This document records the private package foundation, deterministic guards, and publication blockers. Stage 11 owns local tarball distribution proof. This document is not publication proof, a provenance attestation, or a public distribution claim.

Related documents:

- [Public Launch Scope](./public-launch-scope.md)
- [Security, Privacy, And Legal Foundation](./security-privacy-legal-foundation.md)
- [Baseline Gap And Forbidden Claims](./baseline-gap-and-forbidden-claims.md)
- [Release Scope Lock](../20-real-world-proof-and-release/release-scope-lock.md)
- [Release Decision Record](../20-real-world-proof-and-release/release-decision-record.md)
- [Release Gates](../11-hardening/release-gates.md)
- [Stage 11 Distribution Proof](./distribution-proof.md)
- [Supply Chain Attestation](./supply-chain-attestation.md)

## Stage 4 Decision

The selected launch shape remains CLI/package-first, but the package must stay private until a later explicit release-approval task changes that state.

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

These controls are private package foundation checks plus local artifact proof hooks. They do not prove that the package is published, installable from a public registry, signed, reproducible across platforms, or safe for general availability.

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
- provenance, signing, or an explicit public-release decision that names what is not signed and why;
- dependency audit posture and license-review process;
- release notes, changelog, versioning, branch, tag, and rollback process;
- OS-specific install and CLI smoke proof for every claimed supported OS;
- CI parity with the exact public package gates;
- user-facing install documentation that matches the proven artifact and support boundary.

## Forbidden Claims

Do not claim:

- the package has been published;
- npm or another public registry distribution is proven;
- public registry install, upgrade, uninstall, rollback, retained SBOM, signing, provenance, or reproducible-build proof exists;
- package namespace ownership has been verified unless the proving task records evidence;
- hosted, managed, installer, container, or signed-binary launch is in scope;
- Stage 4 alone makes the product generally available or production ready.

## Acceptance Rule For Later Stages

A later stage may remove a blocker only by naming the exact artifact, environment, command, evidence, and resulting user-visible claim. If the evidence is local-only, private-only, OS-specific, registry-specific, or provider-specific, the public claim must keep the same limitation.
