# Изолированный тестовый профиль PostgreSQL (`ops/test-env`)

Этот каталог задаёт одноразовую тестовую среду PostgreSQL для интеграционных и отказных проверок Dennett (D0+).

## Что внутри

- Контейнер `postgres:16-alpine` привязан **только к loopback** (`127.0.0.1:55432`).
- Хранилище данных смонтировано в оперативную память (`tmpfs` на `/var/lib/postgresql/data`): после остановки контейнера тестовые данные исчезают полностью и не затрагивают никакие пользовательские базы.
- Скрипт инициализации [`init/01_init_roles_and_dbs.sql`](init/01_init_roles_and_dbs.sql) (монтируется read-only) создаёт две раздельные непривилегированные роли и две отдельные базы с суффиксом `_test`:
  - `dennett_memory_role` → только `dennett_memory_test`
  - `dennett_control_role` → только `dennett_control_test`
- Права `CONNECT` у `PUBLIC` отозваны на `dennett_memory_test`, `dennett_control_test`, а также на системных базах `postgres` и `template1`: попытка роли `dennett_memory_role` подключиться к `dennett_control_test` или `postgres` (и аналогично для `dennett_control_role`) завершается отказом PostgreSQL с `SQLSTATE 42501` (`insufficient_privilege`). Обе роли созданы с `NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOREPLICATION NOBYPASSRLS`.

## Правила безопасности тестового профиля

1. Все деструктивные пробы и fault-injection проверки в `dennett-testkit` защищены `FaultInjectionGuard` и отказываются запускаться без:
   - `DENNETT_TEST_PROFILE=1`,
   - режима `ProfileMode::TestIsolated`,
   - `allow_live_external_effects = false`,
   - имён баз с суффиксом `_test`,
   - валидированных путей (`root_dir`, `data_dir`, `workspace_dir`) строго внутри системной временной папки, вне репозитория/`cwd`/`HOME` и без symlink-компонентов.
2. Учётные данные в `docker-compose.test.yml` предназначены **исключительно** для локального одноразового контейнера и никогда не используются в production-профиле.

## Запуск вручную или через runner

Через единый runner (сам поднимает и после прогона полностью удаляет контейнер):

```bash
uv run --project ops/runner dennett-check integration --manage-pg
```

Ручной цикл Docker Compose:

```bash
docker compose -f ops/test-env/docker-compose.test.yml up -d --wait
# ... прогон проверок с DENNETT_TEST_PROFILE=1 ...
docker compose -f ops/test-env/docker-compose.test.yml down -v
```
