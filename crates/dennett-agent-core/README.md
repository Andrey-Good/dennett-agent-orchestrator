# dennett-agent-core

Provider-neutral агентная семантика: запрос, результат, runtime port, cancellation и fake runtime.

Развитие по этапам:

1. Context Manifest и Result Envelope;
2. session start/resume/steer/cancel;
3. runtime capability descriptor;
4. event stream и usage;
5. conformance suite для Codex, Claude и generic runtime.

Не реализуйте provider loop здесь, если зрелый native runtime уже его предоставляет.

## M01 runtime contract

The public streaming surface is provider-neutral: every event is scoped to a
session and runtime turn, carries a monotonic sequence and reaches at most one
terminal outcome. Continuations are opaque and adapter-bound. Cancellation is
scoped and idempotently acknowledged; timeout, cancellation and provider
failure remain distinct outcomes.

`ScriptedFakeAgentRuntime` advances a virtual deadline and is safe for
credential-free tests. It and the Node Codex adapter consume the shared
conformance fixture under `tests/contracts`.
