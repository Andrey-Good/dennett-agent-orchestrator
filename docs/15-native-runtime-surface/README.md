[English](#english) | [Russian](#russian)

<a id="english"></a>
# Native Runtime Surface

Status: owner section for the first executable native-runtime-surface slice after the capability freeze.

Owns:

- the implemented Phase 14 runtime surface;
- normalized model discovery and runtime-environment introspection;
- the first executable runtime-option overrides for `reasoning_effort`, `speed_tier`, and `personality`;
- the boundary between local runtime metadata and portable agent-file truth.

Does not own:

- the portable node contract beyond the existing `runtime_options` field;
- per-source runtime introspection semantics beyond what the runtime-source extension already owns;
- built-in user-chat MCP, managed subagents, or provider-backed memory execution.

Primary sources:

- [Capability Gap Lock](../13-capability-gap-lock/README.md)
- [Runtime Integration Model](../02-architecture/runtime-integration-model.md)
- [Runtime Adapter Contract](../03-contracts/runtime-adapter-contract.md)
- [Runtime Sources](../08-extensions/runtime-sources.md)

Documents:

- [Phase 14 Native Runtime Surface Completion](./phase-14-native-runtime-surface-completion.md)

<a id="russian"></a>
# Native Runtime Surface

Статус: раздел-владелец для первого исполнимого среза native runtime surface после capability freeze.

Владеет:

- реализованным runtime-срезом Phase 14;
- нормализованным discovery моделей и runtime-environment introspection;
- первым исполнимым набором runtime-option overrides для `reasoning_effort`, `speed_tier` и `personality`;
- границей между локальными runtime metadata и portable agent-file truth.

Не владеет:

- portable node contract сверх уже существующего поля `runtime_options`;
- семантикой per-source runtime introspection сверх того, чем уже владеет runtime-source extension;
- built-in user-chat MCP, managed subagents и provider-backed memory execution.

Основные источники:

- [Capability Gap Lock](../13-capability-gap-lock/README.md)
- [Runtime Integration Model](../02-architecture/runtime-integration-model.md)
- [Runtime Adapter Contract](../03-contracts/runtime-adapter-contract.md)
- [Runtime Sources](../08-extensions/runtime-sources.md)

Документы:

- [Phase 14 Native Runtime Surface Completion](./phase-14-native-runtime-surface-completion.md)
