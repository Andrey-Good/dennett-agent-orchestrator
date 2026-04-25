[English](#english) | [Russian](#russian)

<a id="english"></a>
# Native Memory Integration

Status: owner section for the first executable native-memory slice after the capability freeze.

Owns:

- the implemented Phase 13 memory slice;
- the Mem0-first local provider path that now exists in code, tests, and live proof;
- the boundary between direct provider-backed memory operations and future runtime-attached memory execution.

Does not own:

- the portable memory-binding contract itself;
- the long-term multi-provider roadmap;
- future runtime-native memory support inside Codex execution;
- subagent or builder behavior.

Primary sources:

- [Capability Gap Lock](../13-capability-gap-lock/README.md)
- [Memory Bindings](../08-extensions/memory-bindings.md)
- [Memory Binding Model Contract](../03-contracts/agent-json/memory-binding-model-contract.md)

Documents:

- [Phase 13 Mem0-First Native Memory Integration](./phase-13-mem0-first-native-memory-integration.md)

<a id="russian"></a>
# Native Memory Integration

Статус: раздел-владелец для первого исполнимого native-memory среза после capability freeze.

Владеет:

- реализованным memory-срезом Phase 13;
- локальным Mem0-first путём provider integration, который теперь подтверждён кодом, тестами и live proof;
- границей между прямыми provider-backed memory operations и будущей runtime-attached memory execution.

Не владеет:

- самим portable memory-binding contract;
- долгосрочной multi-provider roadmap;
- будущей runtime-native поддержкой памяти внутри Codex execution;
- поведением subagents или builder.

Основные источники:

- [Capability Gap Lock](../13-capability-gap-lock/README.md)
- [Memory Bindings](../08-extensions/memory-bindings.md)
- [Memory Binding Model Contract](../03-contracts/agent-json/memory-binding-model-contract.md)

Документы:

- [Phase 13 Mem0-First Native Memory Integration](./phase-13-mem0-first-native-memory-integration.md)
