# ADR-002: Одна логическая память, самостоятельный memoryd и полный серверный резерв

Статус: пересмотрен и принят 2026-09-26. Заменяет решение 2026-07-13 в части in-process/SQLite canonical deployment. Причины старого решения остаются в Git и архиве томов.

## Решение

Memory Fabric работает отдельным `dennett-memoryd` как в полной установке Dennett, так и самостоятельно для внешнего агента. Head — клиент сервиса; memory-core не встраивается в Head как production-вариант. Общая семантическая библиотека и узкие порты позволяют независимо развивать память без зависимости от Task service, главного агента или UI.

Каноническая memory database — PostgreSQL. Клиентское SQLite/native storage хранит staging, drafts и разрешённые кэши, но не является второй серверной реализацией. Выбранный full standby содержит PostgreSQL-копию и совместимый memoryd плюс необходимые объекты/ключи; он не создаётся из кэша при аварии.

Для полного совместимого серверного резерва используется native PostgreSQL replication. Клиентские domain operations и синхронизация внешних объектов/проектов — отдельные пути. Только один primary/Head получает активную власть; физическая копия сама не выдаёт полномочий.

## Последствия

Память можно устанавливать, тестировать и использовать без Dennett Head. Сохраняется цена отдельного процесса/IPC, принятая ради независимости. Убирается полная embedded/service и PG/SQLite server parity. Отдельный verifier и model/index ports позволяют standalone-use; интеграция не делает каждое чтение зависимым от доступности Head.

Точные обязанности, безопасное ограничение stale auth и replica readiness: [80 §§3, 5–8](../architecture/80_Dennett_System_Architecture_and_Runtime_Topology.md), [81 §§1, 6, 9–10](../architecture/81_Dennett_Data_Memory_Storage_Sync_and_Protocol_Architecture.md). Это проектное решение владельца, не утверждение, что separate process всегда быстрее.
