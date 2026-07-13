
# Purpose

This crate is part of Denet's stable core. Keep the public surface small and domain-focused.

# Rules

- Read the root `AGENTS.md` and linked architecture sections.
- Do not import UI frameworks, provider SDKs or physical DB clients unless this crate is explicitly an adapter.
- Preserve one owner for mutable state.
- Add unit/property tests for domain invariants.
- Public contract changes require compatibility review and documentation updates.


# Memory-specific invariants

- This crate defines one logical memory for every deployment role.
- Physical PostgreSQL/SQLite/service adapters must pass the same conformance suite.
- Client cache state must never be promoted to canonical memory without an explicit migration or verified full-replica handoff.
- Indexes are rebuildable; evidence and canonical events are not.

Read specification 10 and architecture volume 81.
