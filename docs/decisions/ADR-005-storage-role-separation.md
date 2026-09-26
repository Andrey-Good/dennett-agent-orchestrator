# ADR-005: PostgreSQL как единственный canonical server store

Статус: пересмотрен и принят 2026-09-26. Решение 2026-07-13 о canonical SQLite local-only отменено; логическое разделение хранилищ сохранено.

## Решение

Один PostgreSQL cluster стандартной установки содержит control и memory domains с независимыми владельцами, ролями и migrations. Canonical schema/queries не реализуются повторно на SQLite. На ПК, выбранном Head или полноценным резервом, используется тот же серверный профиль.

SQLite/нативное embedded storage остаётся для клиентских offline drafts, capture staging, operation log и cache. Большие immutable bytes — Object Store, проекты — filesystem/Git, секреты — keystore/vault. Индексы являются rebuildable projections.

Полные совместимые резервные узлы получают native base backup/WAL streaming PostgreSQL. Это не синхронизация живых DB-файлов, не клиентская read replication и не multi-master merge. Object/file replication, scoped keys и runtime readiness проверяются дополнительно. Replica не заменяет historic backup.

## Последствия

Одна полная серверная data implementation и одна система её migrations вместо двух. Локальному полноценному Head нужен PostgreSQL; мобильному клиенту не требуется сервер БД. Переход управления не конвертирует SQLite в PostgreSQL. Окно восстановления проверяется целиком по RecoveryCut и корректному pgBackRest retention.

Нормативные детали: [81](../architecture/81_Dennett_Data_Memory_Storage_Sync_and_Protocol_Architecture.md). Источники: [AR04–AR12](../research/2026-09-26_architecture_evidence.md). Ограничения совместимости и measured capacity не отменяются ради формулировки «одна база».
