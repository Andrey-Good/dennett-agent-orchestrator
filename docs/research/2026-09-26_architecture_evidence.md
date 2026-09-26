# Источники архитектурной редакции 2.0

Проверены 2026-09-26. Это первичная документация и инженерные разборы для согласованных решений, не доказательство оптимальности всей архитектуры. Полные приложения, SDK, репликация и backup в этом проходе не запускались. Номера версий ниже идентифицируют проверенный текст, не заменяют release manifest будущей реализации.

Нормативные требования пользователя — самостоятельный memoryd, текстовая кооперация, одна серверная PostgreSQL и полный резерв — не приписываются научным статьям. Исследования самоулучшения и интерфейсов уже связаны с E01–E10 и L; их результаты не объявляются повторно воспроизведёнными. Архивные библиографии не перепроверены целиком.

## AR01
**OpenAI, Codex App Server.** [Документация](https://developers.openai.com/codex/app-server/), действующий переход на [App Server](https://learn.chatgpt.com/docs/app-server). Проверены lifecycle threads/turns, stdio JSONL, schema generation, approvals и динамические инструменты. Поддерживает прямой адаптер к процессу без обязательного Node.js-посредника. Ответ на approval продолжает выполнение самим App Server. Experimental endpoints/режимы не считаются production-гарантией; pinned binary и conformance обязательны. Следствие: 82 §§3–4, 7–8.

## AR02
**OpenAI, Celia Chen, Unlocking the Codex harness, 4 февраля 2026.** [Инженерная статья](https://openai.com/index/unlocking-the-codex-harness/). Разобран опубликованный подход к встраиванию готового harness. Поддерживает сохранение нативного исполнения за устойчивым интерфейсом, но не доказывает снижение затрат Dennett и не оправдывает обход его памяти/полномочий. Следствие: 82 §§1–4.

## AR03
**Anthropic, Run Claude Code programmatically.** [Документация CLI](https://code.claude.com/docs/en/headless). Проверены программный запуск, форматы вывода и продолжение работы. CLI и SDK дают разные интерфейсы к готовой функциональности; CLI не является универсальным «новым поколением». Терминальный вывод не служит стабильным протоколом. Следствие: 82 §§5, 7.

## AR04
**PostgreSQL 18, Log-Shipping Standby Servers.** [Документация](https://www.postgresql.org/docs/18/warm-standby.html). Проверены base backup/WAL streaming, требования совместимости и async/sync различия. SQL-реплика не переносит внешние файлы, процессы и устройство авторизации. Поддерживает полные совместимые резервные узлы, не multi-master-синхронизацию клиентов. Следствие: 80 §§5, 7–8; 81 §10.

## AR05
**PostgreSQL 18, Failover.** [Документация](https://www.postgresql.org/docs/18/warm-standby-failover.html). Проверена необходимость внешнего определения отказа и предотвращения возврата старого primary в активную роль. Сам факт promotion/новой строки эпохи не блокирует другой изолированный сервер. Следствие: 80 §§7–8; 83 §10. Это ограничение нельзя устранить дополнительным промптом агенту.

## AR06
**PostgreSQL 18, pg_rewind.** [Документация](https://www.postgresql.org/docs/18/app-pgrewind.html). Проверены назначение и предпосылки возврата разошедшейся копии к выбранному primary. Это не слияние двух независимых writable-историй. При непригодности — re-seed. Следствие: 80 §8; 81 §13.

## AR07
**PostgreSQL 18, Replication configuration.** [Документация](https://www.postgresql.org/docs/18/runtime-config-replication.html). Проверены WAL retention/slots и параметры synchronous standby. Поддерживает явное ограничение отстающих резервов; безлимитный slot способен удерживать WAL. Конкретные лимиты определяются ёмкостью, а не заимствуются как универсальные. Следствие: 80 §11; 81 §10.

## AR08
**PostgreSQL 18, Hot Standby.** [Документация](https://www.postgresql.org/docs/18/hot-standby.html). Проверены read-only ограничения и влияние WAL replay на запросы. Standby не является местом самостоятельной записи новых заметок, processing jobs или read-audit в серверные таблицы. Для непринятых локальных данных остаётся клиентский staging. Следствие: 81 §§5–6, 9–10.

## AR09
**PostgreSQL 18, Constraints.** [Документация](https://www.postgresql.org/docs/18/ddl-constraints.html). Поля первичного ключа обязательны. Это подтверждает ошибку nullable ordinal в прежнем ключе; выбор отдельного link_id — проектное решение, сохраняющее независимость позиции и идентичности. Пример SQL новой архитектуры не является применённой миграцией. Следствие: 81 §3.3.

## AR10
**Sequin, Postgres sequences can commit out-of-order.** [Инженерный разбор](https://blog.sequinstream.com/postgres-sequences-can-commit-out-of-order/). Проверен сценарий выделения меньшего номера до задержанного commit и потери при чтении «после max». Используется как контрпример, не универсальное сравнение всех способов CDC. Следствие: обычный транзакционный счётчик на узком потоке в 81 §8, без network/LLM под блокировкой.

## AR11
**PostgreSQL 18, SELECT / locking clauses.** [Документация](https://www.postgresql.org/docs/18/sql-select.html). SKIP LOCKED допускает пропуск занятых записей; сам по себе не сохраняет бизнес-порядок одного ключа. Следствие: eligibility самого раннего unacknowledged элемента, lease generation и idempotent consumers в 81 §8. Не утверждается exactly-once сеть.

## AR12
**pgBackRest, Configuration.** [Документация](https://pgbackrest.org/configuration.html). Проверены time-based full retention и связь WAL с сохраняемой базовой копией. Для 30-дневного восстановления иногда нужна полная копия старше 30 дней. Это устраняет конфликт «две недельные цепочки + 30 дней», но не гарантирует наличие внешних артефактов. Следствие: 81 §§11–12; 83 §10.

## AR13
**Anthropic, Claude Code legal and compliance.** [Условия интеграции](https://code.claude.com/docs/en/legal-and-compliance). Проверенный текст допускает размещение неизменённого бинарника при сохранении штатной авторизации, собственных учётных данных пользователя, прямой оплаты поставщику и прочих перечисленных условий. Это не безусловное разрешение любой сторонней подписочной схемы. Следствие: 82 §5; никаких извлечения токенов, impersonation или перепродажи квоты. Перед публичным выпуском условия перепроверяются.

## AR14
**Anthropic, Claude Agent SDK overview.** [Документация](https://code.claude.com/docs/en/agent-sdk/overview). Проверены готовый agent loop и отдельное ограничение предложения сторонним продуктом claude.ai login/rate limits без одобрения. SDK/API-режим и нативный Claude Code описаны отдельно. Фактическое списание из подписки не объявляется правом на любую интеграцию. Следствие: 82 §5.

## AR15
**Anthropic, Agent SDK permissions.** [Документация](https://code.claude.com/docs/en/agent-sdk/permissions). Проверены различие PreToolUse и canUseTool и граница нативных разрешений. Не каждый путь проходит через один callback. Следствие: adapter conformance и реальное enforcement без повторного выполнения инструмента в 82 §§5, 8.

## AR16
**MCP Apps SDK, AppBridge.** [API reference](https://apps.extensions.modelcontextprotocol.io/api/classes/app-bridge.AppBridge.html). Проверен готовый компонент host↔app обмена. Он сокращает протокольный код, но не даёт готовый безопасный host целиком и не делает DOM автоматически видимым модели. Следствие: 83 §§4–7, совместно с контрактом L.

## AR17
**Android Developers, Persistent background work.** [Документация](https://developer.android.com/develop/background-work/background-tasks/persistent). Проверены системное управление фоновой работой и ограничения lifecycle. Поддерживает native Mobile Node/staging, а не обещание постоянного полного сервера на любом телефоне. Следствие: 80 §§5, 12; 83 §3. Конкретные iOS-возможности проверяются отдельно при реализации.

## AR18
**Google A2UI Team и авторы MCP Apps, A2UI and MCP Apps, 17 июня 2026.** [Инженерная демонстрация](https://developers.googleblog.com/a2ui-and-mcp-apps/). Проверены варианты композиции A2UI/MCP Apps и различие каталога компонентов и произвольного веб-интерфейса. Примеры не доказывают production-совместимость любой комбинации. Следствие: общий WebSurfaceHost с готовыми адаптерами в 83 §§4–5, без обязательной цепочки всех протоколов.

## AR19
**AG-UI, State Management.** [Документация](https://docs.ag-ui.com/concepts/state). Проверены snapshots/deltas и двустороннее состояние. Протокол не выбирает владельца данных за приложение. Следствие: AG-UI-адаптер поверх существующего command/message path; не новая память, план задач или источник полномочий. 83 §§5–6.

## AR20
**PostgreSQL 18, System Administration Functions.** [Backup control](https://www.postgresql.org/docs/18/functions-admin.html#FUNCTIONS-ADMIN-BACKUP). Проверены pg_create_restore_point и связанные WAL-функции. Именованная точка — marker для восстановления при наличии нужной базы/WAL, не самостоятельный backup. Следствие: согласованный RecoveryCut с immutable manifests в 81 §11; coordinator не выдаёт агенту database superuser. Междоменная/файловая полнота остаётся обязанностью Dennett.

## Сила вывода

Техническая документация подтверждает допустимые операции и ограничения конкретных интерфейсов. Небольшой контрпример позволяет отвергнуть неверную гарантию, но не измеряет latency/масштаб будущего Dennett. Архитектурная целостность проверена сценарием и трассировкой; реальную совместимость версий, security enforcement, restore и performance необходимо испытать до соответствующего заявления о готовности.
