# Repository Validation Report

**Updated:** 2026-07-17

## Executed on Windows

- `just bootstrap`: passed with pinned mise tools, uv-managed CPython 3.13.5, frozen pnpm install and locked Cargo fetch.
- `just rust`: passed (`cargo fmt --check`, clippy with warnings denied, workspace and doc tests).
- TypeScript workspace install and typecheck: passed for desktop, mobile and Node adapter host.
- Python adapter-host and developer-tool unit tests: passed in the frozen uv environment.
- Repository, documentation, planning, generated-index and metadata checks: run by `just verify`.
- `just check`: passed the complete repository, docs, planning, Rust, Python, TypeScript,
  protocol and generated-artifact gate on M00 qualification commit `ed8f0f8`.
- M00 qualification has 9 `MERGED` Work Packages, 15 catalogued tests and one
  current milestone in `QUALIFYING`; generated views reproduce that exact state.
- The milestone schema now exposes the canonical `PROPOSED` → `REFINED` → `ACTIVE` →
  `QUALIFYING` → `ACCEPTED` lifecycle, and generated test views keep exactly one
  `ACTIVE` or `QUALIFYING` milestone as current.
- `TEST-MILESTONE-QUALIFYING-001`: passed all 9 focused generator tests, including
  deterministic `QUALIFYING` output and rejection of zero or multiple current milestones.
- `TEST-MILESTONE-CURRENT-LABEL-001`: passed deterministic assertions that generated
  milestone plans say `Current milestone` and expose the actual `ACTIVE` or `QUALIFYING` status.
- The exact CI sequence `just bootstrap` → clean-worktree probe → `just check` →
  clean-worktree probe passed without tracked or untracked drift.
- GitHub Actions `Fast Gate` run `29572378188` and `Protocol Compatibility` run
  `29572378080` passed for commit `e72403f`.
- WP-M00-008 passed PR `Fast Gate` run `29576019629`; merge commit `a8c5024`
  passed main `Fast Gate` run `29576110530` and `Protocol Compatibility` run `29576110535`.
- WP-M00-009 passed PR `Fast Gate` run `29577110547`; merge commit `a5a1482`
  passed main `Fast Gate` run `29577197354` and `Protocol Compatibility` run `29577197296`.
- GitHub branch protection rule `80381387` applies to `main`, requires the up-to-date
  `Fast Gate` context from GitHub Actions, applies to administrators, and disallows
  force-pushes and branch deletion.
- `just demo-fake` on M00 qualification commit `ed8f0f8`: passed through the public
  Head use case without credentials. Command and result used
  `019f6fdf-16c6-7c80-8df0-0cb85d23aea5`; memory event
  `019f6fdf-16c6-7c80-8df0-0ce7f78c8867` was committed for project
  `019f6fdf-16c6-7c80-8df0-0cc5f88e4d62` and session
  `019f6fdf-16c6-7c80-8df0-0cd682f7bcbf`.
- `cargo test -p dennett-head`: passed the credential-free fake conversation integration test.
- No provider credential is read or required by bootstrap or these checks.

The exact resolved versions are printed and validated by `just doctor`. On Windows, Rust commands
load the installed Visual Studio Build Tools environment automatically.

## Deferred beyond M00

- Native Tauri/mobile packaging and Docker-backed service integration are later milestones.
- Nightly matrices, release signing and production provider credentials are later milestones.

The repository does not claim production readiness.
