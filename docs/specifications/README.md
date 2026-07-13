# Продуктовые и бизнес-логические спецификации

| ID | Документ | Каноническая область |
|---:|---|---|
| 00 | [Функциональная концепция](00_Denet_Functional_Concept.md) | Видение продукта и обязательные возможности |
| 01 | [Specification Index and Shared Contracts](01_Denet_Specification_Index_and_Shared_Contracts.md) | Термины, ownership, shared envelopes и конфликты |
| 10 | [Memory Fabric](10_Denet_Memory_Fabric.md) | Долговременная, проектная, персональная и мультимодальная память |
| 20 | [Agentic Control Fabric](20_Denet_Agentic_Control_Fabric.md) | Оркестратор, project agents, Tasks/Runs и delegation |
| 30 | [Trust, Identity, Autonomy and Permissions](30_Denet_Trust_Identity_Autonomy_and_Permissions.md) | Identity, grants, безопасность и recovery |
| 40 | [Voice and Ambient Interaction](40_Denet_Voice_and_Ambient_Interaction_Fabric.md) | Voice Session, turn-taking и ambient interaction |
| 41 | [Capabilities, Providers and Integrations](41_Denet_Capabilities_Providers_and_Integrations.md) | Models, runtimes, skills, MCP, connectors и backends |
| 50 | [Server Runtime, Events, Sync and Portability](50_Denet_Server_Runtime_Events_Sync_and_Portability.md) | Постоянный runtime, devices, events, sync, backup и failover |
| 60 | [Desktop Business Logic](60_Denet_Desktop_Application_Business_Logic.md) | Desktop screens, commands и states |
| 61 | [Mobile Business Logic](61_Denet_Mobile_Application_Business_Logic.md) | Mobile surfaces, быстрые и interruption-safe flows |
| 70 | [End-to-End Validation and Handoff](70_Denet_End_to_End_Validation_and_Architecture_Handoff.md) | Сквозная проверка и architecture gates |

Жизненные циклы, пересекающие несколько областей, вынесены в [`contracts/`](contracts/README.md).
