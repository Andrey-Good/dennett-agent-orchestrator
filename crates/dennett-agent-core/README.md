# dennett-agent-core

Provider-neutral агентная семантика: запрос, результат, runtime port, cancellation и fake runtime.

Развитие по этапам:

1. Context Manifest и Result Envelope;
2. session start/resume/steer/cancel;
3. runtime capability descriptor;
4. event stream и usage;
5. conformance suite для Codex, Claude и generic runtime.

Не реализуйте provider loop здесь, если зрелый native runtime уже его предоставляет.
