# dennett-memory-core

Одна логическая Memory Fabric для embedded и service deployment.

Core владеет семантикой Event/Evidence/Claim/Current State, scopes, correction, deletion, retrieval planning и portable project memory. PostgreSQL, SQLite, object store, pgvector или Qdrant являются adapters.

Ключевое правило: обычная client SQLite — cache/offline log; ПК становится полным Head только с canonical service/replica profile. См. ADR-002.

## M01 project sessions

`session` определяет provider-neutral append-only журнал Project Session. `SessionJournal` проверяет переходы, idempotency и непрерывную revision до вызова физического `SessionEventStore`; snapshot всегда детерминированно пересобирается из канонических событий. Черновики UI в этот журнал не входят.
