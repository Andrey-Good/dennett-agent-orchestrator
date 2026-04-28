# Example Agents

These examples are illustrative public fixtures. They demonstrate documented contracts and currently validated shapes; they do not define new contracts and they do not imply hosted, production, broad provider, or public package readiness.

## Validation Commands

Run the public example validation suite:

```powershell
pnpm test -- tests/unit/public-examples.test.ts
```

That targeted suite validates:

- every `examples/agents/valid/*.json` file loads as Agent JSON;
- valid examples declare the top-level `params.*` references they use;
- builder draft wrapper examples validate or fail against `contracts/json-schema/builder-output.schema.json` as documented;
- the Phase 5 example can run through the CLI in an offline mocked Codex adapter smoke with `--param topic` resolved.

If `dist` is missing, build before trying live CLI commands:

```powershell
pnpm build
node .\dist\src\interfaces\cli.js --help
```

## Phase 5 Minimal Codex Example

`examples/agents/valid/phase5-codex-minimal.json` is the minimal vertical-slice example. It requires `params.topic` and pins the runtime node to `runtime_options.model = "gpt-5.3-codex"`.

Offline mocked validation is covered by:

```powershell
pnpm test -- tests/unit/public-examples.test.ts
```

Live-only command snippet:

```powershell
node .\dist\src\interfaces\cli.js run .\examples\agents\valid\phase5-codex-minimal.json --param topic="offline-first public docs"
```

Live execution requires local Codex/App Server authentication and account access to `gpt-5.3-codex`. The model name is account/model availability dependent; schema validation and the offline mocked test can pass even when a local live account cannot run that model.

## Stage 2 Runtime Memory Mem0 Example

`examples/agents/valid/stage2-codex-runtime-memory-mem0.json` is the narrow Stage 2 proof fixture for Codex `runtime_agent` execution with a registered local Mem0 provider exposed as `primary_memory`.

The fixture demonstrates provider-neutral `memory_context` resolution, prompt-rendered Codex memory context, and success-only provider writes. It does not claim native App Server memory, broad provider support, durable provider cleanup, provider-wide cleanup, true restore, or provider reliability.

Live-only local provider registration shape:

```powershell
node .\dist\src\interfaces\cli.js memory-provider-register mem0-local --family mem0 --codex-ref primary_memory --transport sdk --capability read --capability write --capability entity_scoped --config "{}"
```

Live-only run snippet:

```powershell
node .\dist\src\interfaces\cli.js run .\examples\agents\valid\stage2-codex-runtime-memory-mem0.json --param topic="project memory context"
```

The Mem0 example requires local provider registration, local provider configuration that is valid for the user-owned Mem0 setup, and local Codex/App Server authentication with access to `gpt-5.3-codex`. The portable agent file intentionally contains no local Mem0 credentials, paths, provider lifecycle instructions, or account setup.

## Builder Draft Output

`examples/agents/builder-drafts/valid-output-wrapper.json` demonstrates the formal Builder output wrapper shape required by `contracts/json-schema/builder-output.schema.json`.

`examples/agents/builder-drafts/invalid-output-wrapper-extra-diagnostics.json` demonstrates a rejected wrapper pattern: diagnostics belong to host output, not to the wrapper or embedded Agent JSON.

These are draft-authoring examples only. They are not live execution proof, deploy proof, provider registration proof, or proof that a builder-authored draft runs on every runtime.
