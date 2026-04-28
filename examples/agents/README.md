# Example Agents

These examples are illustrative only and mirror the test fixtures where practical.

## Phase 5

- `examples/agents/valid/phase5-codex-minimal.json` is the minimal vertical-slice example.
- The runtime node is pinned to `runtime_options.model = "gpt-5.3-codex"` and keeps the prompt intentionally small.

## Stage 2 Runtime Memory

- `examples/agents/valid/stage2-codex-runtime-memory-mem0.json` is the narrow Stage 2 proof fixture for Codex `runtime_agent` execution with a registered local Mem0 provider exposed as `primary_memory`.
- The fixture demonstrates provider-neutral `memory_context` resolution, prompt-rendered Codex memory context, and success-only provider writes. It does not claim native App Server memory, broad provider support, durable provider cleanup, or provider reliability.

## Builder Draft Output

- `examples/agents/builder-drafts/valid-output-wrapper.json` demonstrates the formal Builder output wrapper shape required by `contracts/json-schema/builder-output.schema.json`.
- `examples/agents/builder-drafts/invalid-output-wrapper-extra-diagnostics.json` demonstrates a rejected wrapper pattern: diagnostics belong to host output, not to the wrapper or embedded Agent JSON.
- These are draft-authoring examples only. They are not live execution proof, deploy proof, provider registration proof, or proof that a builder-authored draft runs on every runtime.
