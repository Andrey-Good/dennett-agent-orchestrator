# Release Settings User Checklist

Status: user/admin intervention checklist for the later OSS CLI/package publication gate. This document records external settings that cannot be completed by repository code alone. It is not release approval, publication proof, tag creation approval, or permission to publish to npm.

Related documents:

- [Release Engineering And Supply Chain](./release-engineering-and-supply-chain.md)
- [Supply Chain Attestation](./supply-chain-attestation.md)
- [Package Identity And Registry](./package-identity-and-registry.md)
- [Final Public Launch Gate Decision](./final-public-launch-gate-decision.md)

## Prepared In Repository

The repository currently prepares the following local and CI-side controls:

- `.github/workflows/release.yml` validates release candidates from `v*` tags or manual dispatch.
- The publish job is reachable only from `workflow_dispatch` with `release_mode: publish`.
- The publish job is bound to the GitHub environment named `npm-production`.
- The publish job requests `id-token: write` and uses `npm publish <validated-tarball> --access public`.
- Publish mode requires exact typed confirmations for package name and version.
- Publish mode requires the selected Git ref to be tag `v${package.json.version}`.
- Publish mode still requires the package to be prepared without `private: true`, a matching version tag, exact typed confirmations, trusted publishing, and any required environment approval.
- Release-candidate CI retains the candidate tarball, SPDX SBOM JSON, `npm-pack.json`, and `SHA256SUMS`.

These controls do not prove npm package ownership, npm Trusted Publisher configuration, GitHub environment approval rules, tag protection, public publication, npm provenance, or launch approval.

The GitHub repository is public as of the 2026-04-30 verification recorded in [Final Public Launch Gate Decision](./final-public-launch-gate-decision.md). That closes the repository-visibility prerequisite for `repository-public-preview`, but it does not close any npm/package-publication prerequisite below.

## User/Admin Actions Required

### 1. npm Authentication And Package Ownership

User/admin action:

- Sign in to the npm account or organization that will own `dennett-agent-orchestrator`.
- Confirm whether the unscoped package name `dennett-agent-orchestrator` is available, already owned by the intended maintainer, or must be renamed before public release.
- If the package already exists, ensure the intended release maintainer is listed as an owner or publisher.
- If npm requires an existing package before configuring package settings, complete only the npm-supported package-claim/bootstrap step approved by the release owner; do not treat that bootstrap as OSS launch approval unless the final gate explicitly approves publication.
- Do not add a long-lived `NPM_TOKEN` to this repository for publication. The intended release path is npm Trusted Publisher/OIDC.

Closure evidence after user/admin action:

```powershell
npm view dennett-agent-orchestrator name version repository.url --json
npm owner ls dennett-agent-orchestrator
```

Expected close condition:

- `npm view` resolves the intended public package record, or the release decision explicitly records a package rename before launch.
- `npm owner ls` shows the intended owner/publisher account.
- The evidence record names the npm account or organization that controls publication.

If the package has not yet been publicly created, record the current `npm view` 404 as "name not publicly claimed yet"; this does not close the ownership blocker.

### 2. npm Trusted Publisher

User/admin action:

- In npm package settings, add a Trusted Publisher for GitHub Actions.
- Use exactly these repository values unless a later release decision changes package identity:
- Organization or user: `Andrey-Good`
- Repository: `dennett-agent-orchestrator`
- Workflow filename: `release.yml`
- Environment name: `npm-production`
- Confirm the workflow filename is only the filename, not `.github/workflows/release.yml`.
- Confirm the repository URL in `package.json` continues to match the GitHub repository used in the npm Trusted Publisher settings.
- Prefer disabling token-based publishing after Trusted Publisher is configured if npm account policy and maintainer workflow allow it.

Closure evidence after user/admin action:

```powershell
npm view dennett-agent-orchestrator repository.url --json
gh workflow view release.yml
```

Expected close condition:

- A release evidence record includes a screenshot or exported settings note showing the npm Trusted Publisher values above.
- A later approved publish run reaches `npm publish` through OIDC without `ENEEDAUTH`.
- After approved publication from a public repository and public package, `npm view dennett-agent-orchestrator dist-tags versions --json` resolves the published version.

Until the approved publish run succeeds, npm Trusted Publisher remains configured-but-unproven and npm provenance remains deferred.

Reference:

- [npm Trusted Publishing](https://docs.npmjs.com/trusted-publishers/)

### 3. GitHub `npm-production` Environment

User/admin action:

- In GitHub repository settings, create or verify the environment named `npm-production`.
- Add required reviewers for the publish job.
- Enable prevention of self-approval if the repository plan and policy allow it.
- Restrict environment deployment branches/tags to release tags matching `v*`, or record the exact stricter release-tag pattern selected by maintainers.
- Decide whether administrators may bypass environment protection rules; if bypass remains allowed, record who can bypass and why.
- Do not store npm publish tokens in the environment for the normal release path.

Closure evidence after user/admin action:

```powershell
gh api repos/Andrey-Good/dennett-agent-orchestrator/environments/npm-production
gh api repos/Andrey-Good/dennett-agent-orchestrator/environments/npm-production/deployment-branch-policies
```

Expected close condition:

- The environment exists.
- Required reviewers and branch/tag deployment restrictions are visible in the GitHub environment configuration or evidence record.
- A dry-run release-candidate workflow completes validation without attempting publication:

```powershell
gh workflow run release.yml --ref <approved-ref> -f release_mode=dry-run
```

The dry-run proves validation wiring only. It does not prove environment approval, npm authentication, or publication.

Reference:

- [GitHub deployments and environments](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments)

### 4. Protected Release Tags

User/admin action:

- Create an active GitHub tag ruleset for release tags.
- Target tags matching `v*`, or record the exact stricter release-tag pattern selected by maintainers.
- Restrict creation, update, and deletion of matching release tags to the approved release maintainers or release automation.
- Avoid broad bypass permissions; if bypass is allowed, record every bypass actor and reason.
- Do not create or push a release tag until final release approval names the exact version and source commit.

Closure evidence after user/admin action:

```powershell
gh api repos/Andrey-Good/dennett-agent-orchestrator/rulesets
```

Expected close condition:

- The evidence record identifies an active tag ruleset targeting the release tag pattern.
- The ruleset restricts who can create, update, and delete matching tags.
- The final release decision records the exact tag name, source commit, and approver before anyone creates or pushes the tag.

Reference:

- [GitHub repository rulesets](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/creating-rulesets-for-a-repository)

### 5. Final Release Approval

User/admin action:

- Approve the package version, release notes, changelog entry, support boundary, and rollback owner.
- Approve the exact commit that may be released.
- Approve any actual npm publication only in the same release-preparation scope that records package ownership, final version/tag/release notes, publish path, package metadata/packlist review, minimal supply-chain posture, beta status, and public install documentation/proof plan.
- Approve creation of the matching release tag `v<version>`.
- Approve manual `workflow_dispatch` publish mode only after the release candidate and external settings evidence are attached to the release decision.
- Do not approve hosted, managed, SaaS, production, installer, container, or signed-binary claims unless separate evidence exists.

Closure evidence after user/admin action:

```powershell
git rev-parse HEAD
git tag --list "v<version>"
gh run list --workflow release.yml --branch "v<version>"
npm view dennett-agent-orchestrator@<version> name version dist.tarball repository.url --json
```

Expected close condition:

- A later final gate document changes the decision from blocked to approved and names the exact evidence baseline.
- The release tag exists only after approval and points to the approved commit.
- The release workflow publish run completed from the approved tag with the typed package and version confirmations.
- Public npm package metadata resolves for the approved version.
- Post-publish install, upgrade, uninstall, rollback, SBOM, hash, and provenance/signing evidence is recorded before public claims are expanded.

## Blocker Closure Table

| Blocker | Who must act | Close evidence |
| --- | --- | --- |
| npm auth and ownership | npm owner/admin | `npm view`, `npm owner ls`, and evidence record naming the owning npm account or package rename decision. |
| npm Trusted Publisher | npm package owner/admin | Trusted Publisher settings record plus successful approved OIDC publish run. |
| GitHub environment approval | GitHub repository admin | `npm-production` environment evidence with required reviewers and release tag restriction. |
| Protected release tags | GitHub repository admin | Active tag ruleset evidence for the release tag pattern and restricted mutation. |
| Release approval | release owner/admin | Later final gate document approving exact version, commit, tag, publish run, and public claims. |

## Minimal Operator Path

Use the existing `.github/workflows/release.yml` release workflow when publication becomes appropriate. Do not create a second release process unless a later release decision explicitly replaces this workflow.

The intended low-maintenance posture for the first package release is:

- keep npm Trusted Publisher/OIDC as the preferred publish path;
- retain release tarball, SPDX SBOM, and `SHA256SUMS`;
- prefer npm provenance through trusted publishing;
- either defer signing explicitly or implement it in a separate approved release-hardening task;
- avoid long-lived `NPM_TOKEN`, custom signing infrastructure, and duplicate publication scripts for preview.

## Non-Goals

This checklist does not introduce SaaS, hosted deployment, managed operation, installer distribution, container distribution, signed binaries, production SLA, or general availability requirements. It only covers the external settings needed for a later OSS CLI/package publication gate.
