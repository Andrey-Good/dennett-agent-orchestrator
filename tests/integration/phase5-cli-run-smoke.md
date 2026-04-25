# Phase 5 Codex Run Smoke

This is the smallest runtime-facing smoke scenario for the vertical slice.

## Fixture

- `examples/agents/valid/phase5-codex-minimal.json`

## Goal

- Confirm the CLI run flow can launch a single `runtime_agent` node through the App Server-native Codex adapter.
- Confirm the runtime node is pinned to `runtime_options.model = "gpt-5.3-codex"`.
- Confirm the run stays minimal: one required parameter, one short prompt, one text output.

## Runtime-Dependent Notes

- This path requires a real Codex runtime and authenticated environment.
- Treat it as a smoke test, not an offline unit test.
- If the runtime is unavailable, skip this scenario explicitly rather than failing the structural test suite.
- If the account does not support `gpt-5.3-codex`, the App Server path should fail explicitly with an unsupported-model error rather than silently swapping models.

## Offline Distribution Smoke

- Run `pnpm build` before using the generated CLI artifact.
- Run `node dist/src/interfaces/cli.js --help` after the build; this help smoke must not require provider credentials or emit avoidable eager SQLite warnings.
- Run `pnpm dist:check`, `pnpm packlist:check`, and `pnpm package:check` to prove the build-local distribution and npm dry-run inventory.
- Do not treat generated `dist` as tracked source or as already present in a clean checkout.

## Expected Checks

- The agent file loads successfully.
- The CLI run flow resolves the `topic` parameter.
- The runtime request uses the `gpt-5.3-codex` model marker from the node fixture.
- The run returns a short text result without requiring extra graph branches.
