[English](#english) | [Russian](#russian)

<a id="english"></a>
# Capability Gap Lock

Status: owner section for the post-hardening gap lock and the first post-11 roadmap freeze.

Owns:

- the Phase 12 capability-status model;
- the canonical matrix that maps owner docs to code, tests, and live proof;
- the Mem0-first readiness note used to prevent fictional memory-progress claims;
- the frozen handoff from the completed 1-11 roadmap to later product stages.

Does not own:

- the lower-level behavior of graph execution, state, interaction, lifecycle, runtime adapters, or memory contracts;
- provider-specific implementation details beyond the gap-lock and readiness framing.

Primary sources:

- [AGENTS.md](../../AGENTS.md)
- [Documentation Root](../README.md)
- [Hardening](../11-hardening/README.md)
- [Memory Bindings](../08-extensions/memory-bindings.md)
- [Runtime Integration Model](../02-architecture/runtime-integration-model.md)

Read this section when:

- a later phase needs to know whether something is already implemented or only documented;
- a contributor wants to claim a capability is 'done' and needs the current evidence bar;
- external-provider work begins and the project must separate 'package downloaded' from 'feature really works'.

Documents:

- [Phase 12 Capability Gap Lock](./phase-12-capability-gap-lock.md)
- [Phase 13 Mem0-First Native Memory Integration](../14-native-memory-integration/phase-13-mem0-first-native-memory-integration.md)

Current reconciliation note:

- [2026-04-29 App Facade And Architecture Gate Reconciliation](./phase-12-capability-gap-lock.md#2026-04-29-app-facade-and-architecture-gate-reconciliation)
- [2026-04-29 Stage 14 Runtime Surface Certification](./phase-12-capability-gap-lock.md#2026-04-29-stage-14-runtime-surface-certification)

<a id="russian"></a>
# Capability Gap Lock

Статус: раздел-владелец для post-hardening gap lock и первой заморозки roadmap после этапа 11.

Владеет:

- моделью статусов возможностей для Phase 12;
- канонической матрицей, которая связывает owner-docs с кодом, тестами и live proof;
- заметкой о Mem0-first readiness, которая не дает выдавать желаемое за реализованное по памяти;
- зафиксированной передачей проекта от завершенного roadmap 1-11 к следующим стадиям.

Не владеет:

- нижнеуровневым поведением graph execution, state, interaction, lifecycle, runtime adapters или memory contracts;
- provider-specific деталями реализации сверх рамки gap-lock и readiness.

Основные источники:

- [AGENTS.md](../../AGENTS.md)
- [Корень документации](../README.md)
- [Hardening](../11-hardening/README.md)
- [Memory Bindings](../08-extensions/memory-bindings.md)
- [Модель интеграции runtime](../02-architecture/runtime-integration-model.md)

Читайте этот раздел, когда:

- следующему этапу нужно понять, что уже реализовано, а что только описано;
- участник хочет заявить, что какая-то возможность 'готова', и нужно понять текущую планку доказательства;
- начинается работа с внешним provider и проект должен различать 'пакет скачан' и 'функция реально работает'.

Документы:

- [Phase 12 Capability Gap Lock](./phase-12-capability-gap-lock.md)
