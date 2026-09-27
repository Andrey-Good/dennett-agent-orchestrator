# Архитектура и места будущей реализации

Архитектура 2.0; статус обновлён 2026-09-27. Ниже только целевые места и обязанности новой реализации. В текущем main нет исходников, старых тестов и каркаса сборки. Каталоги создаются по мере выполнения [плана](../implementation/04_MILESTONE_DEPENDENCY_MAP.md); код из истории не возвращается без нового решения владельца.

## Исполняемые корни

`services/head` — прикладные агенты/сессии/поручения, политика и budgets, events/Inbox/effects. Использует клиент Memory API, не включает memory-core в production-процесс.

`services/node` — постоянная локальная работа, workspace/OS/process management, native runtime supervision, offline staging. Это не Node.js SDK-посредник и не неявно самостоятельный глобальный Head.

`services/memoryd` — **отдельный независимо запускаемый Memory service** со своим PostgreSQL/object/auth/model configuration. `crates/dennett-memory-core` содержит его семантическую библиотеку; production Head не встраивает её. Клиентские bindings не тянут реализацию хранилища.

`apps/desktop` — Tauri bridge и React UI; `apps/mobile` — mobile UI и нативный lifecycle/local persistence. Клиенты не владеют permissions/tasks/server memory и не соединяются напрямую с provider из кнопки.

`adapters/agent-runtimes` — прямой Codex App Server adapter, Claude CLI/SDK и другие поддержанные harnesses. Полезные Python/Node SDK hosts создаются при необходимости; пустой forwarding host не обязателен. Homegrown generic loop не baseline.

`adapters/mcp`, `tools/dennettctl` и подходящие client packages — фасады одних прикладных операций. Имена команд в архитектуре являются целевыми примерами; готового CLI в документационной отправной точке нет.

`adapters/browser`, `computer-use`, `connectors`, `local-models`, `sensors` и media workers — реальные границы внешних систем и ресурсов, по наличию соответствующего кода. Каталог не означает обязанность добавить отдельный процесс на каждую категорию.

## Общие границы

`crates/dennett-contracts` — межпроцессные identity/envelopes; `dennett-kernel` — узкие порты; `dennett-agent-core` — участник и явное состояние без собственного обязательного harness; `dennett-trust-core`/`dennett-effect-core` — actual authority/effect semantics; `dennett-sync-core` — клиентские операции/возобновление, **не замена PostgreSQL physical replication**; `dennett-observability` — безопасная телеметрия.

Серверные repositories/migrations реализуются только для PostgreSQL. SQLite нужна лишь в клиентских компонентах, где есть локальные данные. Head↔memoryd использует MemoryPort/client; memoryd не импортирует Head application/Task database. Связи кооперации хранятся текстом; данные аудита, источников и delivery IDs не используются для автоматического графа потребителей.

## Новые обязанности без новых обязательных сервисов

Shared WebSurfaceHost и готовые AppBridge/A2UI/AG-UI adapters размещаются в клиентской области представления. Evaluation worker переиспользует runtime adapters и isolated fixtures по E01–E10. Native PG replication и pgBackRest/restic вызываются operational tooling; release/replica manifests не становятся самостоятельной платформой управления всеми мыслями агента.

Расположение конкретных новых source-файлов здесь не предписано: сначала ограниченный реализуемый сценарий, затем необходимые файлы. Пустые каркасы всех будущих подсистем не являются целью.
