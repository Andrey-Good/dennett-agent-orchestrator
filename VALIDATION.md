# Repository Validation Report

**Updated:** 2026-07-21

## Executed on Windows

- `just bootstrap`: passed with pinned mise tools, uv-managed CPython 3.13.5, frozen pnpm install and locked Cargo fetch.
- `just rust`: passed (`cargo fmt --check`, clippy with warnings denied, workspace and doc tests).
- TypeScript workspace install and typecheck: passed for desktop, mobile and Node adapter host.
- Python adapter-host and developer-tool unit tests: passed in the frozen uv environment.
- Repository, documentation, planning, generated-index and metadata checks: run by `just verify`.
- `just check`: passed the complete repository, docs, planning, Rust, Python, TypeScript,
  protocol and generated-artifact gate on M00 qualification commit `ed8f0f8`.
- The owner accepted M00 on 2026-07-17 after reviewing its intended outcome,
  implementation evidence, test strategy and known limitations.
- M00 has 10 `MERGED` Work Packages and 16 catalogued tests. Its lifecycle is now
  `ACCEPTED`; generated views explicitly report that no milestone is current until
  a later milestone is promoted to `ACTIVE` or `QUALIFYING`.
- `just check` passed the complete gate after the owner-accepted lifecycle transition.
- `just demo-fake` passed on the accepted state. Command and result used
  `019f70c5-d63e-78d3-b093-f53ca6845819`; memory event
  `019f70c5-d63e-78d3-b093-f5624f5bcacd` was committed for project
  `019f70c5-d63e-78d3-b093-f546a8f869af` and session
  `019f70c5-d63e-78d3-b093-f55fafe64b43`.
- The milestone schema now exposes the canonical `PROPOSED` → `REFINED` → `ACTIVE` →
  `QUALIFYING` → `ACCEPTED` lifecycle, and generated test views keep at most one
  `ACTIVE` or `QUALIFYING` milestone as current.
- `TEST-MILESTONE-QUALIFYING-001`: passed all 9 focused generator tests, including
  deterministic `QUALIFYING` output and rejection of multiple current milestones.
- `TEST-MILESTONE-CURRENT-LABEL-001`: passed deterministic assertions that generated
  milestone plans say `Current milestone` and expose the actual `ACTIVE` or `QUALIFYING` status.
- `TEST-MILESTONE-ACCEPTED-HANDOFF-001`: passed deterministic `M00=ACCEPTED` and
  `M01=REFINED` handoff assertions; the generated plan explicitly reports no current
  milestone until one `REFINED` milestone is promoted to `ACTIVE`.
- The exact CI sequence `just bootstrap` → clean-worktree probe → `just check` →
  clean-worktree probe passed without tracked or untracked drift.
- GitHub Actions `Fast Gate` run `29572378188` and `Protocol Compatibility` run
  `29572378080` passed for commit `e72403f`.
- WP-M00-008 passed PR `Fast Gate` run `29576019629`; merge commit `a8c5024`
  passed main `Fast Gate` run `29576110530` and `Protocol Compatibility` run `29576110535`.
- WP-M00-009 passed PR `Fast Gate` run `29577110547`; merge commit `a5a1482`
  passed main `Fast Gate` run `29577197354` and `Protocol Compatibility` run `29577197296`.
- WP-M00-010 passed PR `Fast Gate` run `29579284822`; merge commit `e54a16a`
  passed main `Fast Gate` run `29579395509` and `Protocol Compatibility` run `29579395557`.
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

## M01 owner acceptance

- The owner exercised the native Dennett application throughout qualification and explicitly
  accepted WP-M01-007 and milestone M01 on 2026-07-21.
- M01 has 7 `MERGED` Work Packages and 22 catalogued acceptance cases. Its lifecycle is now
  `ACCEPTED`; generated views report no current milestone until the next refined milestone is
  promoted.
- `mise exec -- just check` passed on merge-ready commit `5565e57`, covering repository,
  documentation, planning, Rust, Python, TypeScript, protocol and generated-artifact gates.
- The same complete gate passed again after the lifecycle transition to `ACCEPTED`; the
  generated milestone plan reports that no milestone is currently active or qualifying.
- GitHub Protocol Compatibility run `29840909681`, Fast Gate run `29840909727` and Windows
  Desktop IPC run `29840910310` passed on merge-ready commit `5565e57` before PR #20 merged
  as `29ede78`.
- The qualified desktop boundary includes 59 renderer tests, 15 Tauri shell tests, 13 SQLite
  tests, 14 Head conversation integration tests, 61 adapter-host tests and 4 real-process
  desktop conversation tests.
- Both ignored live Codex tests were run explicitly with the owner's ChatGPT subscription:
  the session continued after Node restart, and a clarification changed the same active Codex
  turn without hidden cancellation or replacement.
- A detached R2 closure review of executable commit `24da00f` returned PASS with no open
  P0-P2 findings and independently repeated the atomic SQLite regression, all Head conversation
  tests and all adapter-host tests.

## Deferred beyond M01

- Project file edits, diff review, project tests and Git operations belong to M02.
- Managed background Tasks, Runs and multi-agent control surfaces belong to M03.
- Project-folder create/import and chat rename or safe deletion need separate bounded Work
  Packages with Node-owned authority and recovery semantics.
- Providers other than Codex, local models, installers, remote sync, voice, ambient interaction,
  nightly matrices, release signing and sustained adversarial campaigns remain later work.

The repository does not claim production readiness.
