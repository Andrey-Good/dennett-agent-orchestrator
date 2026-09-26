# Проверки перед реализацией и выпуском

2026-09-26. Согласованные архитектурные решения больше не являются открытым выбором: память — отдельный самостоятельный memoryd; canonical DB — PostgreSQL; semantic coordination — текст; готовые harnesses предпочтительны. [Архитектура 2.0](architecture/README.md) задаёт базу, [матрица](architecture/ARCHITECTURE_VALIDATION.md) — проверку.

## Конкретные проверки совместимости

**Серверный резерв:** закрепить compatible PostgreSQL major/build/extensions и упаковку на выбранном ПК/сервере. Проверить native streaming/read-only, данные вне SQL, rewind/reseed и authority fencing. Без доказанного coordinator остаётся управляемый перенос; при partition новый полный Head не самоназначается.

**Native runtimes:** закрепить версии App Server/Claude CLI/SDK и их реально поддержанные approvals/stop/resume/hooks. У полезного host не удалять обязанности ради сокращения процесса. Отдельно сверить условия публичного подписочного подключения; не обойти их извлечением токена.

**Memory:** сравнить выбранные индексы/обработчики в разработке. Полнота истории/current state/provenance не удаляется ради benchmark. Standalone auth/model/index configuration проверяется без Head.

**Клиенты:** SQLite encryption и native keystore, notification/process-death, browser isolation, Windows session capture и mobile background capabilities. Полный mobile Head не обещается без поддержанного runtime.

**Графика:** закрепить AppBridge/A2UI renderer и необязательный AG-UI adapter по совместимости, а не по слову latest. Отказ компонента имеет file/custom-web fallback с явными ограничениями; не создаёт второй state authority.

**Backup и performance:** измерить complete RecoveryCut RPO/RTO, retention-boundary restore, busy ordering key, UI/voice latency, sensory и WAL pressure на целевых устройствах. Архитектурные targets не выдаются за выполненные SLO.

## Оставшиеся продуктовые/организационные решения

Публичная лицензия ещё не установлена. Окончательный workflow разработки, WP/batch правила, состав агентной команды и этапы владельческой приёмки обсуждаются отдельно. Эта страница не разрешает начать реализацию до решения владельца.

## Ограничения текущего документационного прохода

Полный checkout через сеть контейнера недоступен; полноценные repository metadata/checksum regeneration и CI не выполнены. Сгенерированная navigation изменённых архитектурных файлов пересобирается штатным генератором на доступном комплекте. Legacy full-tree manifests/checksums должны быть пересозданы и проверены на полном дереве перед интеграцией в main. Их наличие не определяет нормативный приоритет и не считается успешным CI.
