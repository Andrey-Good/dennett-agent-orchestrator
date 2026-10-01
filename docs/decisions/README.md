# Архитектурные решения

Текущая база — [архитектура 2.0](../architecture/README.md), статус — [реестр пересмотра](../restart/README.md). ADR фиксируют причины и последствия; не создают конкурирующие правила поверх томов.

- [ADR-001: modular monolith](ADR-001-process-selective-modular-monolith.md) — сохраняется; независимая память является обоснованной процессной границей.
- [ADR-002: самостоятельная единая память и полный резерв](ADR-002-one-logical-memory-across-device-roles.md) — пересмотрен 2026-09-26; embedding в Head отменён.
- [ADR-003: Head eligibility opt-in](ADR-003-head-eligibility-opt-in.md) — сохраняется; наличие PG replica само не даёт роли.
- [ADR-004: Tauri shell и постоянный Node](ADR-004-tauri-shell-and-node-daemon.md) — сохраняется.
- [ADR-005: canonical PostgreSQL и клиентское локальное хранение](ADR-005-storage-role-separation.md) — пересмотрен 2026-09-26; второй server store SQLite не поддерживается.
- [ADR-006: прежняя смена wire epoch M01](ADR-006-m01-protocol-epoch.md) — историческое решение существующего кода; этот коммит не выполняет новую смену протокола. Совместимые идеи snapshot/delta и защиты peer сохраняются, полная готовность старых schemas новой архитектуре не заявляется.
- [Шаблон ADR](ADR-000-template.md).

Прямой native runtime adapter, исправленные доставка/backup/evidence key и общий UI host описаны непосредственно в томах 81–83 и матрице проверки. Не нужен новый ADR на каждую редакционную фразу.
