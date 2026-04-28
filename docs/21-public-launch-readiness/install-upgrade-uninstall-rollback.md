# Install, Upgrade, Uninstall, And Rollback

Status: user-facing Stage 11 package lifecycle guide for proven local tarball paths only. This document does not describe a public npm registry install, installer, container, hosted deployment, or managed update channel.

Related documents:

- [Stage 11 Distribution Proof](./distribution-proof.md)
- [Package Identity And Registry](./package-identity-and-registry.md)
- [Supply Chain Attestation](./supply-chain-attestation.md)
- [Security Policy](../../SECURITY.md)

## Prerequisites

Use this guide only for a local `.tgz` package artifact produced from this repository or supplied as an explicit previous/candidate artifact.

Required tools:

- Node.js compatible with `package.json` `engines.node`, currently `>=22.13.0`;
- `pnpm@10.33.0` for repository scripts;
- npm for local package packing, temporary consumer install, uninstall, and SBOM commands.

The package is still private. There is no supported `npm install dennett-agent-orchestrator` public-registry command.

## Current Artifact Install And Uninstall Proof

From the repository root:

```powershell
pnpm package:local-install:proof
```

This command:

- runs `pnpm build`;
- creates a local `.tgz` package with `npm pack --json --ignore-scripts`;
- creates a temporary consumer package;
- installs the tarball with `npm install --ignore-scripts --no-audit --fund=false <artifact.tgz>`;
- runs the installed package bin with `--help`;
- uninstalls `dennett-agent-orchestrator`;
- verifies that the temporary consumer project's installed package directory and package bin are gone.

Use `--keep-temp` only when you need to inspect the proof workspace:

```powershell
pnpm package:local-install:proof -- --keep-temp
```

## Installing A Specific Local Tarball Manually

The proof command does not install the package globally. To inspect a tarball manually in a disposable project, use a temporary consumer directory:

```powershell
mkdir dennett-consumer-proof
cd dennett-consumer-proof
npm init -y
npm install --ignore-scripts --no-audit --fund=false C:\path\to\dennett-agent-orchestrator-0.0.0.tgz
npx dennett-agent-orchestrator --help
npm uninstall --ignore-scripts --no-audit --fund=false dennett-agent-orchestrator
```

This manual flow should be treated as local artifact inspection, not public installation guidance.

## Upgrade And Rollback Proof

Upgrade and rollback proof requires two distinct existing package tarballs:

```powershell
pnpm package:upgrade-rollback:proof -- --from-tgz C:\path\to\old.tgz --to-tgz C:\path\to\new.tgz
```

The harness installs the `from` tarball, smokes the installed CLI help, installs the `to` tarball, smokes help again, reinstalls the `from` tarball, and smokes help a final time.

If there is no previous `.tgz` artifact, rollback proof is unavailable. Stage 11 does not create or retain a prior-version artifact automatically.

## What Uninstall Removes

The proven uninstall scope is narrow:

- the installed package directory under the temporary consumer project's `node_modules`;
- the generated package bin under the temporary consumer project's `node_modules/.bin`.

The proof does not show deletion of:

- repository checkout files;
- generated `dist` output in the source repository;
- local SQLite state;
- agent files, drafts, registry state, sidecar files, or logs;
- local provider registry/config;
- runtime account data or runtime provider records;
- Mem0 or other memory provider data;
- external provider data, telemetry, retention, or account history.

Use provider-specific cleanup commands and provider account controls for provider data. Package uninstall must not be described as full application-data deletion.

## Unsupported Paths

The following paths remain unsupported by Stage 11:

- public `npm install dennett-agent-orchestrator`;
- global install support claims;
- signed binary installers;
- container images;
- automatic update channels;
- hosted rollback or remote disablement;
- rollback without an explicit previous tarball;
- rollback of local app state or provider data;
- public support claims for an OS without recorded green proof for that OS.

