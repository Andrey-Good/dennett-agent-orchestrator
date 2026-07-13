
# ADR-005: Separate canonical, local and object storage roles

- **Status:** accepted
- **Date:** 2026-07-13

## Decision

Use PostgreSQL for multi-device canonical operational/memory state, SQLite for client-local state and single-device embedded mode, immutable object storage for large bytes, and Git/filesystem for project files. Search indexes are rebuildable.

## Consequences

No one storage engine is forced to solve every problem. Domain repositories and conformance tests preserve semantic parity.
