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
| `private` | `true` | Publication is blocked by metadata and policy. |
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
- no `npm publish` has been run or approved;
- no npm package page is claimed;
- no public registry install path is claimed;
- no ownership transfer, namespace reservation, organization setup, or npm automation token readiness is recorded.
- public-ready metadata is present in `package.json`, but metadata presence is not publication approval and does not prove registry ownership.

## Conditions Before `private: false`

Do not set `private` to `false` unless a later explicit release-approval task records at least:

- registry or namespace ownership proof for the chosen final package name;
- final package metadata review approval, including confirmation that `bugs`, `homepage`, and `keywords` remain correct for the chosen public release identity;
- versioning policy and changelog/release-note requirements;
- publication account, token, 2FA, automation, and access-control policy;
- clean public-registry install proof or an approved equivalent dry-run proof;
- OS-specific install and CLI smoke evidence for every claimed supported OS;
- upgrade, rollback, and uninstall boundaries for public artifacts;
- SBOM retention and attachment policy;
- provenance and signing policy, or an explicit recorded decision for unsigned/unattested artifacts;
- security disclosure and support policy alignment;
- approval to create any tag, push, GitHub release, or public registry artifact.

## Forbidden Claims

Do not claim:

- npm publication has occurred;
- the npm package name or namespace is owned or reserved;
- `npm install dennett-agent-orchestrator` is supported;
- the package is ready for public users because local tarball proof passes;
- public-ready metadata by itself approves publication, proves registry ownership, or establishes a supported public install path;
- package identity implies stable JS API, hosted service readiness, installer readiness, or production support.
