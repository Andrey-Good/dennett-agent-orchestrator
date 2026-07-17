# dennett-memory-core

Одна логическая Memory Fabric для embedded и service deployment.

Core владеет семантикой Event/Evidence/Claim/Current State, scopes, correction, deletion, retrieval planning и portable project memory. PostgreSQL, SQLite, object store, pgvector или Qdrant являются adapters.

Ключевое правило: обычная client SQLite — cache/offline log; ПК становится полным Head только с canonical service/replica profile. См. ADR-002.
