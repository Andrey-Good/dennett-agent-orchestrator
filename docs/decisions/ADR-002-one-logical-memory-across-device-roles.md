# ADR-002: Одна логическая Memory Fabric для всех ролей устройств

- **Статус:** принят
- **Дата:** 2026-07-13

## Контекст

ПК может быть обычным клиентом, основным Head или заранее разрешённым кандидатом на failover. Если считать «память ПК» и «память сервера» разными системами, появятся конкурирующие источники истины, сложный sync и непредсказуемые конфликты.

## Решение

`denet-memory-core` определяет одну каноническую семантику памяти. Физический adapter зависит от deployment role:

- обычный клиент — SQLite cache, drafts и offline operation log;
- single-device local-only profile — явный embedded canonical store;
- multi-device Head, включая ПК, намеренно назначенный сервером — server-grade canonical Memory Service и object-store role;
- full failover candidate — полная реплика либо проверенный restore path;
- emergency candidate — ограниченный subset, который никогда не выдаётся за полную каноническую память.

Переход из single-device embedded mode в multi-device mode выполняется как плановая migration, а не как неявное появление второй authority.

## Последствия

Agents и clients всегда используют `MemoryPort`; им не важно, встроена память или доступна через service. Sync связывает каноническую authority с device operation logs, а не две независимые «памяти».
