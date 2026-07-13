# Feature Audit

This checklist was produced after assembling the final repository edition. `Covered` means the behavior and architecture have an explicit owner; it does not mean the feature is already implemented.

| Feature | Status | Canonical locations |
|---|---|---|
| Direct project chat in a real folder/repository | Covered | spec 00/20/60/61; arch 80/82/83 |
| Persistent main orchestrator | Covered | spec 20/50; arch 80/82 |
| Single-agent-first execution and bounded subagents | Covered | spec 20; arch 82 |
| Managed Runs, cancellation and recovery | Covered | spec 20/50; arch 80/81/83 |
| One logical Memory Fabric | Covered | spec 10; ADR-002; arch 80/81 |
| Project-local portable memory | Covered | spec 10, contract J; arch 81 |
| Always-on smart microphone on phone/PC | Covered | spec 40, contract A; arch 80/82/83 |
| Event-driven PC screen capture | Covered | spec 10/40, contract A; arch 80/82/83 |
| Photos, clipboard, selection and camera capture | Covered | contract A; spec 60/61; arch 82/83 |
| Voice interruption, self-correction and actually-heard history | Covered | spec 40; arch 82/83 |
| Fast voice agent with stronger-model sidecar | Covered | spec 40; arch 82 |
| Voice control of projects and orchestrator | Covered | spec 40/20; arch 82/83 |
| Meeting transcription/summary/action candidates | Covered as recipe/profile | spec 40, contract K; arch 82/83 |
| Action Inbox | Covered | spec 50/60/61; arch 80/81/83 |
| Agent Radar and optional Office projection | Covered | spec 50/60/61; arch 80/83 |
| Skills discovery, comparison, delta extraction and project skills | Covered | spec 41; arch 82 |
| MCP, plugins and provider-native extensions | Covered | spec 41; arch 82 |
| Local models and hardware-aware routing | Covered | spec 41; arch 80/82/83 |
| Provider subscriptions, API connections, health and fallback | Covered | spec 41; arch 82/83 |
| Computer-use backends and takeover | Covered | spec 30/41; arch 82 |
| Telegram/email and exact-recipient communication | Covered | contract B; spec 30/41; arch 81/82 |
| Unknown external effect and no duplicate retry | Covered | spec 30/50; arch 80/81/82; runbook |
| Events, semantic triggers and do-nothing option | Covered | spec 20/50; arch 80/81 |
| Offline device operation and reconnect | Covered | spec 50/61; arch 80/81/83 |
| Device sync and conflict preservation | Covered | spec 10/50; arch 81 |
| Head handoff/failover | Covered | spec 50, contract F; arch 80/81/83 |
| Head eligibility only after owner opt-in | Covered | ADR-003; Trust/Server; code/test scenario |
| Backup, restore and recovery drills | Covered | spec 50, contract F; arch 81/83 |
| Signed updates, migrations and version skew | Covered | contract E; arch 80/81/83 |
| Resource pressure, storage and usage budgets | Covered | contract G; arch 80/81/83 |
| Federated global search | Covered | contract H; arch 81/83 |
| Locale, timezone, language and travel | Covered | contract I; arch 81/83 |
| Artifacts, versions, publication and revocation | Covered | contract D; arch 81/83 |
| Project archive/detach/delete/rebind/transfer | Covered | contract C; arch 80/81/83 |
| Portable client / emergency access | Covered, later milestone | spec 50/61; arch 80/83 |
| Daily briefing, retrospectives, AI-news radar and idea distillation | Covered as recipes | contract K; spec 20/41/50 |
| 2D Office / VR / advanced wearables | Deliberately deferred | spec 00/60/61; architecture extension points |
| Automatic emergency-service calls | Deliberately not baseline | spec 00/30 |

## Audit rule

A feature is not considered implementation-ready merely because its name appears in a document. It must have:

1. an authoritative owner;
2. a normal path and failure/recovery path;
3. Trust/effect semantics where relevant;
4. architecture boundaries;
5. a test or risk-spike path;
6. a user-visible state where relevant.
