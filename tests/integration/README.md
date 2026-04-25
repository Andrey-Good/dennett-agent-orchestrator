# Phase 5 Integration Surface

This directory separates structural checks from runtime-dependent smoke coverage.

## Offline Or Structural

- `tests/fixtures/agents/valid/phase5-codex-minimal.json` is a parseable contract fixture for the minimal vertical slice and uses `gpt-5.3-codex`.
- The fixture can be validated without a live Codex runtime.
- Distribution artifact smoke is offline but build-dependent: run `pnpm build` first, then run `node dist/src/interfaces/cli.js --help`, `pnpm dist:check`, `pnpm packlist:check`, and `pnpm package:check`.
- Generated `dist` is a local build artifact and is not assumed to be tracked or already present in a clean checkout.

## Runtime-Dependent

- `tests/integration/phase5-cli-run-smoke.md` describes the CLI run smoke path.
- That smoke path requires an authenticated Codex App Server runtime and should be skipped or marked separately when the runtime is unavailable.

## Phase 18 Integrated Product Flows

- `tests/golden/phase18-integrated-product-flows.md` defines the golden acceptance matrix for local/offline Phase 18 integration evidence.
- Phase 18 evidence remains separate from Phase 19 real-world proof, which requires live runtimes, providers, accounts, operational evidence, and the Phase 19 [`release decision record`](../../docs/20-real-world-proof-and-release/release-decision-record.md).
- Phase 19 proof evidence should be recorded in the Phase 19 [`evidence log`](../../docs/20-real-world-proof-and-release/evidence-log.md), not inferred from local integration tests.
