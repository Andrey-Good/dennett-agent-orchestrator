# dennett-sync-core

Domain operations, revisions, watermarks, epoch checks и merge/revalidation decisions. Не синхронизирует SQLite/PostgreSQL files.

Первые property tests должны покрывать duplicate, reorder, stale epoch, conflicting note edits, monotonic deletion и offline consequential commands.
