# Package Identity And Registry

Status: Stage 11 package identity and registry boundary record, updated for the prepared private `0.1.0-rc.1` package metadata. This document records the current private package identity and what must change before public publication. It is not a registry ownership proof or npm launch approval.

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
| `version` | `0.1.0-rc.1` | Prepared private release-candidate metadata; not a public prerelease artifact, public release version claim, or npm publication proof. |
| `private` | `true` | npm publication is blocked by metadata and policy; this does not mean the GitHub repository is private. |
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
- the package remains private;
- `private: true` is an npm/package-publication guard, not repository visibility; the GitHub repository can be public while npm publication remains blocked;
- no `npm publish` has been run or approved;
- no npm package page is claimed;
- no public registry install path is claimed;
- no ownership transfer, namespace reservation, organization setup, or npm automation token readiness is recorded.
- public-ready metadata is present in `package.json`, but metadata presence is not publication approval and does not prove registry ownership.

## Conditions Before `private: false`

Do not set `private` to `false` unless a later explicit release-preparation or release-approval task records all of the following:

- npm package name and ownership proof for `dennett-agent-orchestrator`, or an approved rename before publication;
- final prerelease or release version approval;
- exact source commit, release tag, and release notes approval;
- publication path configured or explicitly approved, preferably npm Trusted Publisher/OIDC rather than a long-lived `NPM_TOKEN`;
- final package metadata review approval, including `name`, `version`, `bin`, `files`, `exports`, `repository`, `bugs`, `homepage`, `keywords`, README, and packlist;
- minimal supply-chain posture: retained SBOM plus `SHA256SUMS`, npm provenance preferred, and signing either explicitly deferred or implemented;
- post-publish public install, CLI smoke, uninstall, and later upgrade/rollback proof plan;
- security disclosure and support policy alignment;
- approval to create any tag, push, GitHub release, or public registry artifact.

Public-registry install proof cannot fully close before the first approved publication. Before removing `private: true`, the project must at least have the proof plan, ownership, version, publish path, metadata/packlist review, and supply-chain posture ready. After publication, the proof must be executed before public install claims expand.

## Forbidden Claims

Do not claim:

- npm publication has occurred;
- the npm package name or namespace is owned or reserved;
- `npm install dennett-agent-orchestrator` is supported;
- the package is ready for public users because local tarball proof passes;
- public-ready metadata by itself approves publication, proves registry ownership, or establishes a supported public install path;
- package identity implies stable JS API, hosted service readiness, installer readiness, or production support.
