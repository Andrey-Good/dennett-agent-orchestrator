# Трассируемость: спецификация → архитектура → код → тест

| Область | Спецификация | Архитектура | Основные code roots | Основные тесты |
|---|---|---|---|---|
| Projects, agents, Tasks/Runs | 20, contract C | 80, 82, 83 | `crates/dennett-agent-core`, `services/head` | domain, fake-runtime, E2E project chat |
| Memory | 10, contracts A/H/J | 81, 82 | `crates/dennett-memory-core`, `services/memoryd` | memory conformance, retrieval, deletion, parity |
| Trust и permissions | 30, contract F | 80, 82, 83 | `crates/dennett-trust-core` | policy/state-machine, head-eligibility, effect tests |
| Voice и ambient | 40, contract A | 80, 82, 83 | sensor worker, client apps, voice adapters | audio fixtures, interruption, sensor scenarios |
| Capabilities/providers | 41, contracts E/G/J/K | 82, 83 | `adapters/`, adapter hosts | adapter conformance и live canaries |
| Events, sync и failover | 50, contracts E/G/I | 80, 81, 83 | `crates/dennett-sync-core`, Head/Node | deterministic simulation, reconnect, epoch fencing |
| External communication | contract B | 81, 82, 83 | connectors + `dennett-effect-core` | unknown-effect и no-duplicate scenarios |
| Desktop | 60 | 83 | `apps/desktop` | component, Tauri E2E, accessibility |
| Mobile | 61 | 83 | `apps/mobile` | component, native integration, Maestro E2E |
| Updates и migrations | contract E | 81, 83 | update/migration modules | mixed-version и rollback tests |
| Artifacts | contract D | 81, 83 | object/artifact repositories и viewers | version/publication/revocation scenarios |
