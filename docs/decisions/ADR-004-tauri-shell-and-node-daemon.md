
# ADR-004: Use Tauri as the desktop shell and a separate persistent Node daemon

- **Status:** accepted
- **Date:** 2026-07-13

## Decision

The Tauri/React application owns desktop presentation and OS UI integration. `dennett-node` owns local durable behavior and continues after windows close.

## Consequences

UI crashes and updates do not terminate active work. IPC and service registration add complexity but are isolated and testable.
