
# Denet Memory Service

Wraps the same `denet-memory-core` semantics used by embedded mode.

- Never become a second Head.
- Canonical ingest must continue even if derived indexers fail.
- Embedded and service adapters must pass identical conformance fixtures.
- No agent/provider/UI direct database access.
