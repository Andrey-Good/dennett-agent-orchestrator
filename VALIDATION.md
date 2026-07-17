# Repository Validation Report

**Updated:** 2026-07-17

## Executed on Windows

- `just bootstrap`: passed with pinned mise tools, uv-managed CPython 3.13.5, frozen pnpm install and locked Cargo fetch.
- `just rust`: passed (`cargo fmt --check`, clippy with warnings denied, workspace and doc tests).
- TypeScript workspace install and typecheck: passed for desktop, mobile and Node adapter host.
- Python adapter-host and developer-tool unit tests: passed in the frozen uv environment.
- Repository, documentation, planning, generated-index and metadata checks: run by `just verify`.
- `just check`: passed the complete repository, docs, planning, Rust, Python, TypeScript,
  protocol and generated-artifact gate on WP-M00-008 implementation commit `ac99f50`.
- The milestone schema now exposes the canonical `PROPOSED` → `REFINED` → `ACTIVE` →
  `QUALIFYING` → `ACCEPTED` lifecycle, and generated test views keep exactly one
  `ACTIVE` or `QUALIFYING` milestone as current.
- `TEST-MILESTONE-QUALIFYING-001`: passed all 9 focused generator tests, including
  deterministic `QUALIFYING` output and rejection of zero or multiple current milestones.
- The exact CI sequence `just bootstrap` → clean-worktree probe → `just check` →
  clean-worktree probe passed without tracked or untracked drift.
- GitHub Actions `Fast Gate` run `29572378188` and `Protocol Compatibility` run
  `29572378080` passed for commit `e72403f`.
- GitHub branch protection rule `80381387` applies to `main`, requires the up-to-date
  `Fast Gate` context from GitHub Actions, applies to administrators, and disallows
  force-pushes and branch deletion.
- `just demo-fake`: passed through the public Head application use case and emitted correlated command, result and memory-event identifiers.
- `cargo test -p dennett-head`: passed the credential-free fake conversation integration test.
- No provider credential is read or required by bootstrap or these checks.

The exact resolved versions are printed and validated by `just doctor`. On Windows, Rust commands
load the installed Visual Studio Build Tools environment automatically.

## Deferred beyond M00

- Native Tauri/mobile packaging and Docker-backed service integration are later milestones.
- Nightly matrices, release signing and production provider credentials are later milestones.

The repository does not claim production readiness.
