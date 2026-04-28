# Supply Chain Attestation

Status: Stage 11 supply-chain attestation record for local package proof. This document distinguishes local SBOM validation from retained SBOMs, signatures, provenance, and public release attestations.

Related documents:

- [Stage 11 Distribution Proof](./distribution-proof.md)
- [Package Identity And Registry](./package-identity-and-registry.md)
- [Install, Upgrade, Uninstall, And Rollback](./install-upgrade-uninstall-rollback.md)
- [Release Engineering And Supply Chain](./release-engineering-and-supply-chain.md)

## Local SBOM Status

Stage 11 implements local SBOM validation, not SBOM publication.

Default command:

```powershell
pnpm supply-chain:local:proof
```

Specific tarball command:

```powershell
pnpm supply-chain:local:proof -- --from-tgz C:\path\to\candidate.tgz
```

The proof installs a local tarball into a temporary consumer project and runs:

```powershell
npm sbom --sbom-format=spdx --sbom-type=application
```

The resulting JSON must include an SPDX version and a package entry named `dennett-agent-orchestrator`.

There is no canonical SBOM file path in the repository or release artifacts. The script validates the generated SBOM in memory and removes the temporary workspace unless `--keep-temp` is supplied. SBOM retention, release attachment, and long-term evidence storage remain deferred.

## Provenance And Signing Status

Current status:

| Control | Status | Reason |
| --- | --- | --- |
| npm provenance | Deferred. | Package publication is blocked by `private: true`, and Stage 11 must not run `npm publish`. |
| Package signing | Deferred. | No local signing identity or publication signing infrastructure is configured. |
| Signed SBOM | Deferred. | No retained canonical SBOM artifact exists. |
| Artifact hash manifest | Deferred. | The local proof creates temporary tarballs but does not record a canonical hash manifest. |
| Git tag or GitHub release attestation | Deferred. | Stage 11 must not create tags, push commits, or create releases. |

The proof script prints the provenance and signing deferrals as explicit output during local SBOM proof.

## Artifact Hash And Evidence Expectations

When a later task promotes a package artifact beyond local proof, it must record:

- exact artifact file name and version;
- SHA-256 or stronger artifact hash;
- source commit and build environment;
- package inventory evidence;
- install/uninstall proof logs;
- upgrade/rollback proof logs when a previous artifact exists;
- retained SBOM path and hash;
- provenance and signing status;
- OS and package-manager evidence for every public support claim.

Until that evidence exists, Stage 11 local tarballs are temporary proof artifacts, not release artifacts.

## What Remains Unsigned Or Unattested

The following remain unsigned or unattested:

- locally packed `.tgz` files;
- generated `dist` contents;
- local SBOM output;
- package inventory dry-run output;
- CI logs unless a later release process archives them;
- docs and README updates;
- any manual tarballs supplied to the upgrade/rollback harness.

## Forbidden Claims

Do not claim:

- supply-chain attestation is complete;
- artifacts are signed;
- npm provenance exists;
- SBOMs are retained, published, or attached to releases;
- local SBOM validation is equivalent to public release attestation;
- reproducible builds are proven;
- package hashes are recorded unless a later evidence document records them.

