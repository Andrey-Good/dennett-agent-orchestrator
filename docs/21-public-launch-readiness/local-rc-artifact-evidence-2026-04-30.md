# Local RC Artifact Evidence - 2026-04-30

Status: local-only release-candidate artifact evidence for `dennett-agent-orchestrator@0.1.0-rc.1`. This note records artifacts generated on a local workstation only. It is not public release evidence.

## Source Baseline

- Commit: `c52ad7f97f56a2dd155562af303b176db6ee6db5`
- Branch status at start: `main...origin/main [ahead 2]`
- Package name: `dennett-agent-orchestrator`
- Package version: `0.1.0-rc.1`
- Package private status: `private: true`

## Commands Run

```powershell
git status --short --branch
git rev-parse HEAD
pnpm release-candidate:check
pnpm packlist:check
pnpm build
npm pack --json --ignore-scripts --pack-destination release-artifacts
pnpm supply-chain:local:proof -- --from-tgz release-artifacts\dennett-agent-orchestrator-0.1.0-rc.1.tgz --output release-artifacts\dennett-agent-orchestrator-0.1.0-rc.1.spdx.json
Get-FileHash -Algorithm SHA256 release-artifacts\dennett-agent-orchestrator-0.1.0-rc.1.spdx.json
Get-FileHash -Algorithm SHA256 release-artifacts\dennett-agent-orchestrator-0.1.0-rc.1.tgz
```

## Gate Results

- `pnpm release-candidate:check`: passed.
- `pnpm packlist:check`: passed; validated package inventory with 94 files.
- `pnpm build`: passed.
- `npm pack --json --ignore-scripts --pack-destination release-artifacts`: passed; produced `dennett-agent-orchestrator-0.1.0-rc.1.tgz` with npm `shasum` `c9db79d92166c7cf27fb7af35a098592e30b3a6e` and integrity `sha512-QLRHwXL6qjEjHUHn2/DpTwPMvi54BR+NLQ56wLQh90nYtwZPY2MgsVtAj4eFk2r1XAKu6YN9e9ctQOGd89q+UQ==`.
- `pnpm supply-chain:local:proof`: passed; generated retained SPDX SBOM from the local tarball.

## Retained Local Artifacts

Artifacts are retained under `release-artifacts/`.

| File | Size | SHA256 |
| --- | ---: | --- |
| `dennett-agent-orchestrator-0.1.0-rc.1.tgz` | 134872 bytes | `7c429359b0c25bcac97bfb55411c212ad68202b80e6463f2264f447b13fc8298` |
| `dennett-agent-orchestrator-0.1.0-rc.1.spdx.json` | 9150 bytes | `bdfedb89f24fb33b0f3e4f88efbda9192ce7f2579f811cfcb866e43f3ca43986` |
| `SHA256SUMS` | 222 bytes | `e66066d963166639420c1cbb2090fff5e772f38594abc3178c67bfdc94cdcccb` |

`release-artifacts/SHA256SUMS` records hashes for the tarball and retained SPDX SBOM.

## Limitations

- Local-only evidence; no public release was created.
- Unsigned artifacts; no package signing was performed.
- No npm provenance exists because no npm publish command was run and `package.json` remains `private: true`.
- No public registry install proof exists.
- No tag, push, GitHub release, external registry upload, or publication operation was performed.
- This evidence does not prove reproducible builds, cross-platform installation, public package availability, or general availability readiness.
