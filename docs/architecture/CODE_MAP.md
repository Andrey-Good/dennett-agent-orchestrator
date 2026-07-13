# Карта архитектуры и кода

## Исполняемые процессы

| Process | Code root | Владеет | Главные документы |
|---|---|---|---|
| Desktop shell | `apps/desktop` | Окна, workbench presentation, OS UI bridge | 60, 83 |
| Mobile client | `apps/mobile` | Mobile presentation и native remote surfaces | 61, 83 |
| Head | `services/head` | Глобальная runtime coordination | 20, 50, 80 |
| Node | `services/node` | Локальное долговечное device execution | 50, 80, 83 |
| Memory service | `services/memoryd` | Service boundary Memory Fabric | 10, 81 |
| Adapter hosts | `services/adapter-host-*` | Изоляция внешних SDK | 41, 82, 83 |
| Sensor worker | `services/sensor-worker` | Ambient source runtime | 40, contract A, 82 |

## Stable core

| Crate | Назначение | Запрещённые зависимости |
|---|---|---|
| `denet-contracts` | Stable cross-boundary identifiers/envelopes | UI, provider SDK, SQL |
| `denet-kernel` | Минимальные use-case interfaces | Concrete adapters |
| `denet-agent-core` | Provider-neutral agent runtime semantics | OpenAI/Claude types |
| `denet-memory-core` | Memory semantic core | Provider SDK, UI, physical DB |
| `denet-trust-core` | Identity/authorization decisions | Model calls as authority |
| `denet-effect-core` | External effect lifecycle | Connector SDK direct ownership |
| `denet-sync-core` | Operation-log and merge semantics | DB file replication |
| `denet-observability` | Privacy-aware telemetry bootstrap | Canonical audit ownership |

## Поток первого vertical slice

```text
React project chat
→ Tauri command
→ local Node API
→ Head application
→ Fake/real AgentRuntimePort
→ ResultEnvelope
→ MemoryPort append
→ WatchDelta
→ React cache
```

Каждый следующий vertical slice должен расширять этот путь, а не создавать параллельный runtime.
