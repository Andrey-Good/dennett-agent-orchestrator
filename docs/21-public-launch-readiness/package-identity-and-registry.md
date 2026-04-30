# Package Identity And Registry

Status: Stage 11 package identity and registry boundary record, updated for the `0.1.0-rc.1` release-prep package metadata. This document records the current package identity and what must still change before public publication. It is not a registry ownership proof, npm launch approval, or public install proof.

Related documents:

- [Stage 11 Distribution Proof](./distribution-proof.md)
- [Install, Upgrade, Uninstall, And Rollback](./install-upgrade-uninstall-rollback.md)
- [Supply Chain Attestation](./supply-chain-attestation.md)
- [Release Engineering And Supply Chain](./release-engineering-and-supply-chain.md)

## Current Package Identity

Current package metadata:

| Field | Current value | Status |
| --- | --- | --- |
| `name` | `dennett-agent-orchestrator` | Local package identity used by the proof scripts. |
| `version` | `0.1.0-rc.1` | Prepared release-candidate metadata for preview publication planning; not a public prerelease artifact, public release version claim, or npm publication proof. |
| `private` | absent | The package-level npm publish guard has been removed for release preparation; publication remains blocked by ownership, approval, registry, and evidence gates. |
| `description` | `Codex-first orchestrator for portable agent runs.` | Present. |
| `license` | `Apache-2.0` | Matches root `LICENSE`. |
| `repository` | `https://github.com/Andrey-Good/dennett-agent-orchestrator` | Verified repository metadata only. |
| `bugs` | `https://github.com/Andrey-Good/dennett-agent-orchestrator/issues` | Public issue-routing metadata is present. |
| `homepage` | `https://github.com/Andrey-Good/dennett-agent-orchestrator#readme` | Public homepage metadata is present. |
| `keywords` | `agent-orchestration`, `agent-runtime`, `codex`, `cli`, `workflow` | Public discovery metadata is present. |

The package exposes only:

- the package `bin` entry `dennett-agent-orchestrator`;
- `./package.json`;
- JSON schema exports under `./contracts/json-schema/*.schema.json`.

No stable JS or TS API is exported.

## Registry Ownership Status

No public registry ownership proof is recorded for the `dennett-agent-orchestrator` npm name or any namespace.

Current truthful state:

- the package name is used for local tarball proof only;
- an npm `E404` or name-lookup miss means no visible public package was found at the time of checking; it does not prove name ownership, namespace reservation, publication rights, or maintainer access;
- the package is not published to npm;
- the removed `private: true` field was an npm/package-publication guard, not repository visibility; the GitHub repository can be public while npm publication remains blocked;
- no `npm publish` has been run or approved;
- no npm package page is claimed;
- no public registry install path is claimed;
- no ownership transfer, namespace reservation, organization setup, or npm automation token readiness is recorded.
- public-ready metadata is present in `package.json`, but metadata presence is not publication approval and does not prove registry ownership.

## Release-Prep Status For Package Publication

The release-preparation gate for removing `"private": true` is now recorded for the current preview package state:

- package version: `0.1.0-rc.1`;
- intended eventual tag: `v0.1.0-rc.1`, not yet created;
- release notes boundary: preview/RC source-checkout and later package candidate only, no GA, production, SaaS, completed external beta, or broad provider claims;
- npm registry lookup: `npm view dennett-agent-orchestrator` returned `E404`, which means no visible public package was found but does not prove ownership;
- local npm auth: `npm whoami` returned `ENEEDAUTH`, so this machine has no local npm publisher proof;
- package metadata and packlist review are prepared locally through `pnpm packlist:check`;
- minimal supply-chain posture for the future publish path is retained SBOM plus `SHA256SUMS`, npm Trusted Publisher/OIDC preferred for provenance, and package signing deferred unless a later release decision implements it;
- post-publish proof plan remains public install, CLI smoke, uninstall, and later upgrade/rollback when at least two public versions exist.

This release-prep status does not approve publication. Before any actual npm publish, maintainers must still record npm package ownership or an approved rename, the exact release-prep commit, final tag and release notes approval, the configured or approved publish path, and any required GitHub `npm-production` environment settings. After publication, the public install proof must be executed before public install claims expand.

## Forbidden Claims

Do not claim:

- npm publication has occurred;
- the npm package name or namespace is owned or reserved;
- `npm install dennett-agent-orchestrator` is supported;
- the package is ready for public users because local tarball proof passes;
- public-ready metadata by itself approves publication, proves registry ownership, or establishes a supported public install path;
- package identity implies stable JS API, hosted service readiness, installer readiness, or production support.
