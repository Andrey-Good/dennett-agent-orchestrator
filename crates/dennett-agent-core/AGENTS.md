
# Purpose

This crate is part of Dennett's stable core. Keep the public surface small and domain-focused.

# Rules

- Read the root `AGENTS.md` and linked architecture sections.
- Do not import UI frameworks, provider SDKs or physical DB clients unless this crate is explicitly an adapter.
- Preserve one owner for mutable state.
- Add unit/property tests for domain invariants.
- Public contract changes require compatibility review and documentation updates.

Canonical references: specification 20 and architecture volume 82.
