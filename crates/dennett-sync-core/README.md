# dennett-sync-core

Domain operations, revisions, watermarks, epoch checks и merge/revalidation decisions. Не синхронизирует SQLite/PostgreSQL files.

Первые property tests должны покрывать duplicate, reorder, stale epoch, conflicting note edits, monotonic deletion и offline consequential commands.

M01 добавляет generic watch reducer: snapshot устанавливает stream/revision, duplicate delta является no-op, gap или новый Authority Epoch блокирует deltas до нового snapshot, unavailable сохраняет разрешённый cache, а revoke немедленно удаляет его. `DraftCachePort` — отдельный неканонический client-state contract со стабильным `command_id`.
