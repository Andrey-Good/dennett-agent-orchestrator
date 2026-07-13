# Архитектура Denet

Архитектура собрана в четыре цельных тома вместо десятков разрозненных заметок.

| Том | Главный вопрос |
|---:|---|
| [80](80_Denet_System_Architecture_and_Runtime_Topology.md) | Какие исполняемые части существуют, где живут и как безопасно переживают отказы? |
| [81](81_Denet_Data_Memory_Storage_Sync_and_Protocol_Architecture.md) | Где живёт состояние, как оно синхронизируется, ищется, мигрирует и восстанавливается? |
| [82](82_Denet_Agent_Voice_Capability_and_Integration_Architecture.md) | Как исполняются agents, models, voice, tools, MCP, connectors и computer-use? |
| [83](83_Denet_Client_Operations_Testing_and_Implementation_Blueprint.md) | Как устроены clients, packaging, tests, CI/CD, repository structure и implementation plan? |

## Стабильные принципы

- Process-selective modular monolith вместо обязательных microservices.
- Один логический Head и локально способные Nodes.
- Роль устройства задаётся конфигурацией; Head eligibility требует явного разрешения.
- Одна логическая Memory Fabric независимо от того, является Head выделенным сервером или ПК.
- Для multi-device Head PostgreSQL/server storage каноничен; SQLite клиента — cache/offline state.
- Provider-native runtimes остаются adapters; Denet владеет Tasks, context, permissions, effects и history.
- External effects idempotent или reconcilable.
- Derived indexes заменяемы и пересобираемы.
- У каждой значимой границы есть fake и conformance implementation.

Краткие причины решений находятся в [`docs/decisions/`](../decisions/README.md).

Для быстрой ориентации в реализации используйте [`CODE_MAP.md`](CODE_MAP.md), а нерешённые технологические вопросы перечислены в [`docs/OPEN_QUESTIONS.md`](../OPEN_QUESTIONS.md).
