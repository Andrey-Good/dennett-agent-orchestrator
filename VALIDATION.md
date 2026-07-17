# Repository Validation Report

**Updated:** 2026-07-17

## Executed on Windows

- `just bootstrap`: passed with pinned mise tools, uv-managed CPython 3.13.5, frozen pnpm install and locked Cargo fetch.
- `just rust`: passed (`cargo fmt --check`, clippy with warnings denied, workspace and doc tests).
- TypeScript workspace install and typecheck: passed for desktop, mobile and Node adapter host.
- Python adapter-host and developer-tool unit tests: passed in the frozen uv environment.
- Repository, documentation, planning, generated-index and metadata checks: run by `just verify`.
- `just demo-fake`: passed through the public Head application use case and emitted correlated command, result and memory-event identifiers.
- `cargo test -p dennett-head`: passed the credential-free fake conversation integration test.
- No provider credential is read or required by bootstrap or these checks.

The exact resolved versions are printed and validated by `just doctor`. On Windows, Rust commands
load the installed Visual Studio Build Tools environment automatically.

## Deferred beyond WP-M00-001

- Buf lint, generation and breaking-change checks are completed by WP-M00-002.
- Final pull-request gate composition and a clean GitHub Actions run are completed by WP-M00-003.
- Native Tauri/mobile packaging and Docker-backed service integration are later milestones.

The repository does not claim production readiness.
