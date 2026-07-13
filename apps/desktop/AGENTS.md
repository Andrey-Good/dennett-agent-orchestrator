
# Desktop Application Instructions

## Purpose

Implement the Tauri/React Adaptive Agent Workbench from specification 60 and architecture volume 83.

## Boundaries

- React owns presentation state only.
- Tauri owns OS UI integration and a narrow command/channel bridge.
- `denet-node` owns durable local state and commands.
- Never read SQLite/PostgreSQL directly from frontend or Tauri commands.
- Every visible action invokes a stable `command_id`.
- Streaming updates must not steal focus or claim completion before authoritative acknowledgement.

## Required tests

Component/accessibility tests, bridge contract tests, Tauri E2E and restart/resume scenarios.
