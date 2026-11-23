# D2 Architecture, Contracts, Schema & UX Blueprint

Статус: Утверждённая спецификация реализации для этапа **D2 («Поручения, автономное ожидание и предсказуемая остановка»)** · 2026-11-24 · Ветка `d2/durable-runs-and-stop` (от принятой кодовой базы D1 `7496ee2`).

Настоящий документ переводит продуктовые и архитектурные обязательства этапа D2 ([04_MILESTONE_DEPENDENCY_MAP.md](../implementation/04_MILESTONE_DEPENDENCY_MAP.md), [AGENTS.md](../../AGENTS.md), [UX_AND_PRODUCT_BACKLOG.md](UX_AND_PRODUCT_BACKLOG.md), [80](../architecture/80_Dennett_System_Architecture_and_Runtime_Topology.md)–[83](../architecture/83_Dennett_Client_Operations_Testing_and_Implementation_Blueprint.md), [20](../specifications/20_Dennett_Agentic_Control_Fabric.md), [30](../specifications/30_Dennett_Trust_Identity_Autonomy_and_Permissions.md), [50](../specifications/50_Dennett_Server_Runtime_Events_Sync_and_Portability.md), [60](../specifications/60_Dennett_Desktop_Application_Business_Logic.md) и контракты [00](../specifications/contracts/00_Shared_Cross_Domain_Rules.md), [B](../specifications/contracts/B_External_Communication_Operation.md), [C](../specifications/contracts/C_Project_Lifecycle_Contract.md), [D](../specifications/contracts/D_Artifact_Lifecycle_Contract.md), [G](../specifications/contracts/G_Resource_Pressure_and_Usage_Accounting_Contract.md), [I](../specifications/contracts/I_Locale_Timezone_Language_and_Travel_Contract.md), [M](../specifications/contracts/M_Runtime_Rollout_Contract.md), [90](../specifications/contracts/90_Integrated_End_to_End_Scenarios.md)) в точные схемы PostgreSQL, контракты Rust/TypeScript, инварианты конкурентности, UX-компоненты в визуальном эталоне [owner-direction-v5.md](WP-M01-003/owner-direction-v5.md) ([D1_UI_REWORK_BLUEPRINT.md](D1_UI_REWORK_BLUEPRINT.md)) и матрицу приёмочных проверок.

---

## 1. Архитектурная граница D2 и трассировка обязательных сценариев

### 1.1. Жёсткая граница против оверинжиниринга
> **«Не строить универсальный workflow-язык и собственный планировщик мышления. Семантическое решение остаётся за агентом; инфраструктура сохраняет и исполняет обязательства.»** ([04_MILESTONE_DEPENDENCY_MAP.md](../implementation/04_MILESTONE_DEPENDENCY_MAP.md) §D2, [80](../architecture/80_Dennett_System_Architecture_and_Runtime_Topology.md) §§1, 2, 10, [82](../architecture/82_Dennett_Agent_Voice_Capability_and_Integration_Architecture.md) §§1, 10–11)

Запрещено вводить в D2:
- собственный интерпретатор DAG/BPMN/YAML-воркфлоу или обязательные JSON-анкеты `NextStep` перед каждым ходом модели;
- граф смысловых потребителей (`depends_on` / `consumers` registry) для каскадного авто-убийства соседних задач при отмене одной из них;
- внешние брокеры (Kafka, Redis, Temporal) или встраивание `dennett-memory-core` внутрь `dennett-head`;
- любые зависимости `Claude Code` (`R-CLAUDE`) / `Codex` (`R-CODEX`) или тихий fallback с `oauth-personal` на платный `GEMINI_API_KEY`;
- прямые зависимости в `dennett-contracts` сверх разрешённого набора `{"serde", "thiserror", "uuid"}` (проверяется `dennett-check deps`).

Простые реплики в чате остаются `Direct Turn` / `Adaptive Session` в существующих таблицах `sessions` + `runs` + `messages` без принудительного создания `TaskRecord`. Долговечное `TaskRecord` создаётся тогда, когда обязательство должно пережить закрытие окна/перезапуск `Head`, ждать время/сигнал/ответ/разрешение, исполняться с бюджетом или выполнять отслеживаемый внешний эффект.

### 1.2. Обязательные сценарии проверки этапа D2
1. **Архитектурные сценарии ([ARCHITECTURE_VALIDATION.md](../architecture/ARCHITECTURE_VALIDATION.md) и [REQUIREMENTS_COVERAGE.md](../testing/REQUIREMENTS_COVERAGE.md))**:
   - **V07** — изменение цели поручения и отмена общей работы через текстовую координацию между агентами без кастомного графа потребителей, плюс защита от перезаписи новой цели поздним результатом старого поколения (`generation` fencing).
   - **V15** — меньший номер задержал commit: курсор потока выделяется только через транзакционный счётчик `control_stream_counters` на короткой секции фиксации (никогда не `BIGSERIAL` / `MAX(id)`), исключая пропуск задержанных событий подписчиком.
   - **V16** — два параллельных обработчика одного `ordering_key`: `FOR UPDATE SKIP LOCKED` применяется строго к головному неподтверждённому элементу ключа (`ordered_outbox_heads`); следующий элемент того же ключа не обгоняет занятый/упавший первый, разные ключи идут параллельно, а истёкший lease увеличивает `lease_generation` и отвергает поздний `ack` старого воркера.
   - **V17** — сбой процесса после доставки до `ack`: повторная доставка по истёкшему lease идемпотентно поглощается получателем/реестром эффектов без дублирования логического действия.
   - **V18** — согласованный `REPEATABLE READ` снимок состояния + `snapshot_cursor`, безопасное продвижение через скрытые по scope записи (`scanned_through_seq`) без раскрытия чужого payload и без ложного gap, а также явный `ResyncRequired` при устаревшем курсоре или смене `authority_epoch`.
   - **Сценарий 90 §30** ([90_Integrated_End_to_End_Scenarios.md](../specifications/contracts/90_Integrated_End_to_End_Scenarios.md)) — обратимое архивирование проекта с активной работой/таймерами без удаления локальных файлов и с приостановкой будущих запусков.
   - **Сохранение регрессий D0/D1**: `V01`–`V06`, `V19`, `V29` обязаны оставаться зелёными.
2. **Приёмка первого runtime `agy acp` ([ANTIGRAVITY_RUNTIME_ACCEPTANCE.md](../testing/ANTIGRAVITY_RUNTIME_ACCEPTANCE.md))**:
   - **A07 (срез D2)** — делегирование помощнику свободным текстом без JSON-анкеты, наследование ограниченных прав (`child_scope ⊆ parent_scope`), общий/индивидуальный бюджет, межагентные текстовые сообщения и отсутствие поломки соседней работы при отмене или позднем ответе.
   - **A10 (срез D2)** — закрытие окна UI не завершает `dennett-head` и `agy acp`; таймеры и расписания хранит `Head` в PostgreSQL (переживают полный перезапуск `Head`), а не дочерний процесс `agy`; фоновый профиль `gemini-api-key` изолирован от `oauth-personal` (при наличии тестового `GEMINI_API_KEY` проходит живой фоновый canary, при отсутствии — fail-closed `AUTH_REQUIRED` без тихого переключения); при `LostAck` / `Stop` / рестарте неизвестный внешний исход фиксируется как `unknown_external` и никогда не повторяется вслепую.
   - **A05 / A06 (интерактивное расширение D2)** — живой цикл `session/request_permission` в режиме `ask_before_write` (`Спрашивать запись`) с интерактивным подтверждением/отклонением пользователем, отзываемыми `capability_grants` и мгновенным сбросом ожидающих разрешений при `Stop`.

---

## 2. Разделение реализации на два непересекающихся среза (Slice A / Slice B)

Чтобы исключить конфликты владения файлами между Neon-исполнителями, этап D2 делится на две чёткие границы:

- **Slice A (`Backend + ACP + CLI`, автор `@d2-backend-author`)**:
  - Владеет:
    - `crates/dennett-contracts/**` (новые доменные типы D2; **0 новых зависимостей** в `crates/dennett-contracts/Cargo.toml`);
    - `services/head/migrations/0004_d2_durable_tasks_and_lifecycle.sql` (`LATEST_CONTROL_SCHEMA_VERSION = 4`) и `services/head/src/**`;
    - `adapters/antigravity-acp/src/**` (асинхронный `PolicyEvaluator` для живых интерактивных подтверждений, учёт токенов `usage`, поддержка elicitation/clarification и новых директив в `SimulatedRuntime`);
    - `crates/dennett-head-client/src/**` (`HeadClient` + полный CLI-паритет `dennettctl`);
    - `crates/dennett-testkit/**` (unit, `proptest` и реальные PostgreSQL + многопроцессные интеграционные тесты всех гарантий D2).
  - **Не редактирует**: `apps/desktop/**` и `ops/runner/**` (кроме проверки, что существующий `apps/desktop/src-tauri` компилируется без регрессий за счёт `#[serde(default)]` и обратной совместимости существующих методов `HeadClient`).

- **Slice B (`Desktop UI + Tauri Bridge + Runner`, автор `@d2-ui-author` после готовности контрактов Slice A)**:
  - Владеет:
    - `apps/desktop/src-tauri/**` (новые IPC-команды Tauri 2 поверх `HeadClient`, без расширения `capabilities/default.json` и CSP);
    - `apps/desktop/src/**` (`contracts.ts`, `bridge.ts`, `devHttpBridge.ts`, `mockBridge.ts`, `WorkspaceProvider.tsx`, подписка по `committed_cursor`, локальная офлайн-очередь черновиков, `ActivityRail`, `InboxRadarCard`, `TasksCard`, живые карточки подтверждения в чате и `Inbox`, возврат `Спрашивать запись` в `Composer`, контекстные меню `…` проектов и артефактов, переключатель версий `v1`/`v2` в `WorkspacePanel`, тесты Vitest);
    - `ops/runner/src/dennett_runner/cli.py` (команда `dennett-check demo-d2` и повышение порогов `MIN_INTEGRATION_TESTS` / `MIN_DESKTOP_FRONTEND_TESTS`);
    - обновление `docs/implementation/STATUS.md` и `docs/testing/REQUIREMENTS_COVERAGE.md` перед H2.

---

## 3. Схема PostgreSQL `0004_d2_durable_tasks_and_lifecycle.sql` (`v4`)

Миграция `services/head/migrations/0004_d2_durable_tasks_and_lifecycle.sql` поднимает `LATEST_CONTROL_SCHEMA_VERSION` с `3` до `4`. Все счётчики последовательностей используют исключительно `control_stream_counters` (никаких `SERIAL` / `BIGSERIAL` / `GENERATED AS IDENTITY`).

### 3.1. Расширение существующих таблиц (`projects`, `sessions`, `runs`, `messages`, `artifacts`, `agent_config_versions`, `control_stream_counters`)

```sql
-- 1. Retention & epoch metadata on stream counters (V15, V18)
ALTER TABLE control_stream_counters
    ADD COLUMN IF NOT EXISTS authority_epoch BIGINT NOT NULL DEFAULT 1 CHECK (authority_epoch > 0),
    ADD COLUMN IF NOT EXISTS retained_from_seq BIGINT NOT NULL DEFAULT 1 CHECK (retained_from_seq >= 1);

-- 2. Agent default resource budget & attached skills (A07, Contract G)
ALTER TABLE agent_config_versions
    ADD COLUMN IF NOT EXISTS default_budget JSONB NULL,
    ADD COLUMN IF NOT EXISTS attached_skills JSONB NOT NULL DEFAULT '[]'::jsonb;

-- 3. Project lifecycle separation (Contract C: active | archived | detached | deleted_record)
ALTER TABLE projects
    ADD COLUMN IF NOT EXISTS lifecycle_state TEXT NOT NULL DEFAULT 'active'
        CHECK (lifecycle_state IN ('active', 'archived', 'detached', 'deleted_record')),
    ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ NULL,
    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ NULL,
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now();

CREATE INDEX IF NOT EXISTS projects_by_lifecycle_created
    ON projects(lifecycle_state, created_at DESC);

-- 4. Client message idempotency key on `messages` (offline reconnect dedup)
ALTER TABLE messages
    ADD COLUMN IF NOT EXISTS client_message_id TEXT NULL
        CHECK (client_message_id IS NULL OR (octet_length(client_message_id) > 0 AND octet_length(client_message_id) <= 128));

CREATE UNIQUE INDEX IF NOT EXISTS messages_session_client_msg_uniq
    ON messages(session_id, client_message_id)
    WHERE client_message_id IS NOT NULL;

-- 5. Artifact completeness (partial | final) and lifecycle separation (Contract D)
ALTER TABLE artifacts
    ADD COLUMN IF NOT EXISTS completeness_status TEXT NOT NULL DEFAULT 'final'
        CHECK (completeness_status IN ('partial', 'final')),
    ADD COLUMN IF NOT EXISTS lifecycle_state TEXT NOT NULL DEFAULT 'active'
        CHECK (lifecycle_state IN ('active', 'archived', 'removed', 'deleted')),
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now();

-- Allow content-free tombstones when `lifecycle_state = 'deleted'` (Contract D §D.5 / §D.13)
ALTER TABLE artifacts DROP CONSTRAINT IF EXISTS artifacts_payload_or_path_chk;
ALTER TABLE artifacts ADD CONSTRAINT artifacts_payload_or_path_chk
    CHECK (lifecycle_state = 'deleted' OR content IS NOT NULL OR relative_path IS NOT NULL);
```

*(Примечание к триггеру `trg_agent_config_versions_append_only`: `ALTER TABLE ... ADD COLUMN` в PostgreSQL не вызывает `BEFORE UPDATE` row-level триггер при константном `DEFAULT`, поэтому существующие строки `agent_config_versions` мигрируют безопасно и остаются append-only.)*

### 3.2. Таблицы поручений, таймеров, идемпотентных сигналов и связи с `runs`

```sql
CREATE TABLE IF NOT EXISTS tasks (
    task_id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(project_id),
    session_id UUID NOT NULL REFERENCES sessions(session_id),
    agent_id UUID NOT NULL REFERENCES agents(agent_id),
    parent_task_id UUID NULL REFERENCES tasks(task_id),
    title TEXT NOT NULL CHECK (octet_length(title) > 0 AND octet_length(title) <= 256),
    goal TEXT NOT NULL CHECK (octet_length(goal) > 0 AND octet_length(goal) <= 32768),
    generation BIGINT NOT NULL DEFAULT 1 CHECK (generation > 0),
    status TEXT NOT NULL CHECK (status IN (
        'queued',
        'running',
        'waiting_timer',
        'waiting_signal',
        'waiting_clarification',
        'waiting_approval',
        'succeeded',
        'failed',
        'cancelled',
        'unknown_external'
    )),
    wait_detail TEXT NULL CHECK (wait_detail IS NULL OR octet_length(wait_detail) <= 2048),
    partial_result_summary TEXT NULL CHECK (partial_result_summary IS NULL OR octet_length(partial_result_summary) <= 16384),
    error_code TEXT NULL CHECK (error_code IS NULL OR octet_length(error_code) <= 64),
    error_summary TEXT NULL CHECK (error_summary IS NULL OR octet_length(error_summary) <= 2048),
    active_run_id UUID NULL REFERENCES runs(run_id),
    schedule_json JSONB NULL,
    budget_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    turns_used INTEGER NOT NULL DEFAULT 0 CHECK (turns_used >= 0),
    tokens_used BIGINT NOT NULL DEFAULT 0 CHECK (tokens_used >= 0),
    wall_ms_used BIGINT NOT NULL DEFAULT 0 CHECK (wall_ms_used >= 0),
    usage_confidence TEXT NOT NULL DEFAULT 'unknown'
        CHECK (usage_confidence IN ('provider_reported', 'estimated', 'unknown')),
    attached_skills JSONB NOT NULL DEFAULT '[]'::jsonb,
    auth_profile_override TEXT NULL
        CHECK (auth_profile_override IS NULL OR auth_profile_override IN ('oauth-personal', 'gemini-api-key')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ NULL
);

CREATE INDEX IF NOT EXISTS tasks_by_project_updated
    ON tasks(project_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS tasks_by_status_updated
    ON tasks(status, updated_at ASC);

-- Link `runs` to `tasks` + generation fencing & expanded run statuses
ALTER TABLE runs
    ADD COLUMN IF NOT EXISTS task_id UUID NULL REFERENCES tasks(task_id),
    ADD COLUMN IF NOT EXISTS task_generation BIGINT NULL
        CHECK (task_generation IS NULL OR task_generation > 0),
    ADD COLUMN IF NOT EXISTS tokens_used BIGINT NULL CHECK (tokens_used IS NULL OR tokens_used >= 0),
    ADD COLUMN IF NOT EXISTS wall_ms BIGINT NULL CHECK (wall_ms IS NULL OR wall_ms >= 0);

ALTER TABLE runs DROP CONSTRAINT IF EXISTS runs_status_check;
ALTER TABLE runs ADD CONSTRAINT runs_status_check
    CHECK (status IN (
        'running',
        'succeeded',
        'cancelled',
        'failed',
        'degraded',
        'superseded',
        'unknown_external'
    ));

CREATE INDEX IF NOT EXISTS runs_by_task_created
    ON runs(task_id, created_at ASC)
    WHERE task_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS task_timers (
    timer_id UUID PRIMARY KEY,
    task_id UUID NOT NULL REFERENCES tasks(task_id),
    task_generation BIGINT NOT NULL CHECK (task_generation > 0),
    idempotency_key TEXT NOT NULL CHECK (octet_length(idempotency_key) > 0 AND octet_length(idempotency_key) <= 128),
    fire_at TIMESTAMPTZ NOT NULL,
    timezone TEXT NOT NULL DEFAULT 'UTC' CHECK (octet_length(timezone) > 0 AND octet_length(timezone) <= 128),
    original_expression TEXT NULL CHECK (original_expression IS NULL OR octet_length(original_expression) <= 256),
    recurrence_kind TEXT NOT NULL CHECK (recurrence_kind IN ('once', 'interval', 'daily_wall')),
    recurrence_spec JSONB NULL,
    missed_run_policy TEXT NOT NULL DEFAULT 'execute_once'
        CHECK (missed_run_policy IN ('execute_once', 'skip', 'ask')),
    status TEXT NOT NULL CHECK (status IN ('pending', 'claimed', 'fired', 'paused', 'skipped', 'cancelled')),
    lease_owner TEXT NULL CHECK (lease_owner IS NULL OR octet_length(lease_owner) <= 128),
    lease_generation BIGINT NOT NULL DEFAULT 0 CHECK (lease_generation >= 0),
    lease_expires_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    fired_at TIMESTAMPTZ NULL,
    CONSTRAINT task_timers_task_idem_uniq UNIQUE (task_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS task_timers_due_idx
    ON task_timers(status, fire_at ASC)
    WHERE status IN ('pending', 'claimed');

CREATE TABLE IF NOT EXISTS task_signals (
    signal_id UUID PRIMARY KEY,
    task_id UUID NOT NULL REFERENCES tasks(task_id),
    idempotency_key TEXT NOT NULL CHECK (octet_length(idempotency_key) > 0 AND octet_length(idempotency_key) <= 128),
    signal_name TEXT NOT NULL CHECK (octet_length(signal_name) > 0 AND octet_length(signal_name) <= 128),
    payload_text TEXT NOT NULL DEFAULT '' CHECK (octet_length(payload_text) <= 32768),
    payload_sha256 TEXT NOT NULL CHECK (payload_sha256 ~ '^[0-9a-f]{64}$'),
    applied_generation BIGINT NULL,
    outcome_status TEXT NOT NULL CHECK (outcome_status IN ('woke_task', 'queued_on_task', 'rejected_stopped')),
    correlation_id TEXT NOT NULL CHECK (octet_length(correlation_id) > 0 AND octet_length(correlation_id) <= 128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT task_signals_task_idem_uniq UNIQUE (task_id, idempotency_key)
);
```

### 3.3. Интерактивные разрешения, отзываемые `CapabilityGrants`, текстовые сообщения агентов, `Action Inbox` и `External Effects`

```sql
CREATE TABLE IF NOT EXISTS capability_grants (
    grant_id UUID PRIMARY KEY,
    scope TEXT NOT NULL CHECK (octet_length(scope) > 0 AND octet_length(scope) <= 128),
    project_id UUID NOT NULL REFERENCES projects(project_id),
    agent_id UUID NULL REFERENCES agents(agent_id),
    session_id UUID NULL REFERENCES sessions(session_id),
    task_id UUID NULL REFERENCES tasks(task_id),
    tool_pattern TEXT NOT NULL CHECK (octet_length(tool_pattern) > 0 AND octet_length(tool_pattern) <= 256),
    target_prefixes JSONB NOT NULL DEFAULT '[]'::jsonb,
    max_uses INTEGER NULL CHECK (max_uses IS NULL OR max_uses > 0),
    uses_consumed INTEGER NOT NULL DEFAULT 0 CHECK (uses_consumed >= 0),
    reason TEXT NOT NULL CHECK (octet_length(reason) > 0 AND octet_length(reason) <= 1024),
    expires_at TIMESTAMPTZ NULL,
    revoked_at TIMESTAMPTZ NULL,
    revoked_reason TEXT NULL CHECK (revoked_reason IS NULL OR octet_length(revoked_reason) <= 512),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS capability_grants_active_by_project
    ON capability_grants(project_id, created_at DESC)
    WHERE revoked_at IS NULL;

CREATE TABLE IF NOT EXISTS permission_requests (
    request_id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(project_id),
    session_id UUID NOT NULL REFERENCES sessions(session_id),
    run_id UUID NOT NULL REFERENCES runs(run_id),
    task_id UUID NULL REFERENCES tasks(task_id),
    agent_id UUID NOT NULL REFERENCES agents(agent_id),
    tool_call_id TEXT NOT NULL CHECK (octet_length(tool_call_id) <= 256),
    tool_name TEXT NOT NULL CHECK (octet_length(tool_name) > 0 AND octet_length(tool_name) <= 256),
    tool_kind TEXT NOT NULL CHECK (octet_length(tool_kind) <= 64),
    target_summary TEXT NOT NULL CHECK (octet_length(target_summary) <= 1024),
    normalized_targets JSONB NOT NULL DEFAULT '[]'::jsonb,
    parameters_sha256 TEXT NOT NULL CHECK (parameters_sha256 ~ '^[0-9a-f]{64}$'),
    reason TEXT NOT NULL CHECK (octet_length(reason) > 0 AND octet_length(reason) <= 1024),
    status TEXT NOT NULL CHECK (status IN (
        'pending',
        'approved_once',
        'approved_scope',
        'denied',
        'expired'
    )),
    issued_grant_id UUID NULL REFERENCES capability_grants(grant_id),
    decision_id UUID NULL REFERENCES permission_decisions(decision_id),
    revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0),
    expires_at TIMESTAMPTZ NOT NULL,
    resolved_at TIMESTAMPTZ NULL,
    correlation_id TEXT NOT NULL CHECK (octet_length(correlation_id) > 0 AND octet_length(correlation_id) <= 128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS permission_requests_pending_idx
    ON permission_requests(status, created_at ASC)
    WHERE status = 'pending';

CREATE TABLE IF NOT EXISTS effect_claims (
    effect_id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(project_id),
    session_id UUID NOT NULL REFERENCES sessions(session_id),
    task_id UUID NULL REFERENCES tasks(task_id),
    run_id UUID NULL REFERENCES runs(run_id),
    agent_id UUID NOT NULL REFERENCES agents(agent_id),
    effect_idempotency_key TEXT NOT NULL CHECK (octet_length(effect_idempotency_key) > 0 AND octet_length(effect_idempotency_key) <= 128),
    connector_id TEXT NOT NULL CHECK (octet_length(connector_id) > 0 AND octet_length(connector_id) <= 64),
    operation TEXT NOT NULL CHECK (octet_length(operation) > 0 AND octet_length(operation) <= 128),
    target_recipient TEXT NOT NULL CHECK (octet_length(target_recipient) > 0 AND octet_length(target_recipient) <= 512),
    parameters_json JSONB NOT NULL,
    parameters_sha256 TEXT NOT NULL CHECK (parameters_sha256 ~ '^[0-9a-f]{64}$'),
    status TEXT NOT NULL CHECK (status IN (
        'prepared',
        'dispatching',
        'confirmed',
        'denied',
        'failed',
        'unknown'
    )),
    provider_operation_id TEXT NULL CHECK (provider_operation_id IS NULL OR octet_length(provider_operation_id) <= 256),
    receipt_json JSONB NULL,
    error_summary TEXT NULL CHECK (error_summary IS NULL OR octet_length(error_summary) <= 2048),
    reconciled_at TIMESTAMPTZ NULL,
    correlation_id TEXT NOT NULL CHECK (octet_length(correlation_id) > 0 AND octet_length(correlation_id) <= 128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT effect_claims_idem_uniq UNIQUE (effect_idempotency_key)
);

CREATE INDEX IF NOT EXISTS effect_claims_by_task_status
    ON effect_claims(task_id, status)
    WHERE task_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS inbox_cards (
    card_id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(project_id),
    session_id UUID NULL REFERENCES sessions(session_id),
    task_id UUID NULL REFERENCES tasks(task_id),
    run_id UUID NULL REFERENCES runs(run_id),
    agent_id UUID NULL REFERENCES agents(agent_id),
    kind TEXT NOT NULL CHECK (kind IN (
        'permission_approval',
        'clarification',
        'unknown_effect',
        'budget_exceeded',
        'missed_schedule'
    )),
    status TEXT NOT NULL CHECK (status IN ('open', 'resolved', 'expired', 'superseded', 'withdrawn')),
    urgency TEXT NOT NULL DEFAULT 'normal' CHECK (urgency IN ('low', 'normal', 'high', 'critical')),
    subject TEXT NOT NULL CHECK (octet_length(subject) > 0 AND octet_length(subject) <= 512),
    detail TEXT NOT NULL CHECK (octet_length(detail) > 0 AND octet_length(detail) <= 4096),
    permission_request_id UUID NULL REFERENCES permission_requests(request_id),
    effect_id UUID NULL REFERENCES effect_claims(effect_id),
    resolution_summary TEXT NULL CHECK (resolution_summary IS NULL OR octet_length(resolution_summary) <= 2048),
    revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at TIMESTAMPTZ NULL
);

CREATE INDEX IF NOT EXISTS inbox_cards_open_idx
    ON inbox_cards(status, created_at DESC)
    WHERE status = 'open';

CREATE TABLE IF NOT EXISTS agent_messages (
    agent_message_id UUID PRIMARY KEY,
    sender_agent_id UUID NULL REFERENCES agents(agent_id),
    recipient_agent_id UUID NOT NULL REFERENCES agents(agent_id),
    project_id UUID NOT NULL REFERENCES projects(project_id),
    related_task_id UUID NULL REFERENCES tasks(task_id),
    idempotency_key TEXT NOT NULL CHECK (octet_length(idempotency_key) > 0 AND octet_length(idempotency_key) <= 128),
    body_text TEXT NOT NULL CHECK (octet_length(body_text) > 0 AND octet_length(body_text) <= 65536),
    delivered_in_run_id UUID NULL REFERENCES runs(run_id),
    correlation_id TEXT NOT NULL CHECK (octet_length(correlation_id) > 0 AND octet_length(correlation_id) <= 128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    delivered_at TIMESTAMPTZ NULL,
    CONSTRAINT agent_messages_recipient_idem_uniq UNIQUE (recipient_agent_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS agent_messages_undelivered_idx
    ON agent_messages(recipient_agent_id, project_id, created_at ASC)
    WHERE delivered_in_run_id IS NULL;
```

### 3.4. Журнал упорядоченной доставки (`control_events` + `ordered_outbox` / `ordered_outbox_heads`, V15–V18)

```sql
CREATE TABLE IF NOT EXISTS control_events (
    cursor_seq BIGINT PRIMARY KEY CHECK (cursor_seq > 0),
    event_id UUID NOT NULL UNIQUE,
    authority_epoch BIGINT NOT NULL DEFAULT 1 CHECK (authority_epoch > 0),
    entity_key TEXT NOT NULL CHECK (octet_length(entity_key) > 0 AND octet_length(entity_key) <= 160),
    entity_seq BIGINT NOT NULL CHECK (entity_seq > 0),
    event_type TEXT NOT NULL CHECK (octet_length(event_type) > 0 AND octet_length(event_type) <= 128),
    project_id UUID NULL REFERENCES projects(project_id),
    session_id UUID NULL REFERENCES sessions(session_id),
    task_id UUID NULL REFERENCES tasks(task_id),
    correlation_id TEXT NOT NULL CHECK (octet_length(correlation_id) > 0 AND octet_length(correlation_id) <= 128),
    payload_json JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT control_events_entity_seq_uniq UNIQUE (entity_key, entity_seq)
);

CREATE INDEX IF NOT EXISTS control_events_by_project_cursor
    ON control_events(project_id, cursor_seq ASC)
    WHERE project_id IS NOT NULL;

DROP TRIGGER IF EXISTS trg_control_events_append_only ON control_events;
CREATE TRIGGER trg_control_events_append_only
    BEFORE UPDATE OR DELETE ON control_events
    FOR EACH ROW
    EXECUTE FUNCTION dennett_control_reject_append_only_mutation();

-- Per-key head pointer for V16/V17 parallel ordered outbox delivery without key overtaking
CREATE TABLE IF NOT EXISTS ordered_outbox_heads (
    destination TEXT NOT NULL CHECK (octet_length(destination) > 0 AND octet_length(destination) <= 64),
    ordering_key TEXT NOT NULL CHECK (octet_length(ordering_key) > 0 AND octet_length(ordering_key) <= 160),
    next_deliver_seq BIGINT NOT NULL DEFAULT 1 CHECK (next_deliver_seq > 0),
    PRIMARY KEY (destination, ordering_key)
);

CREATE TABLE IF NOT EXISTS ordered_outbox (
    item_id UUID PRIMARY KEY,
    destination TEXT NOT NULL CHECK (octet_length(destination) > 0 AND octet_length(destination) <= 64),
    ordering_key TEXT NOT NULL CHECK (octet_length(ordering_key) > 0 AND octet_length(ordering_key) <= 160),
    key_seq BIGINT NOT NULL CHECK (key_seq > 0),
    cursor_seq BIGINT NOT NULL CHECK (cursor_seq > 0),
    idempotency_key TEXT NOT NULL CHECK (octet_length(idempotency_key) > 0 AND octet_length(idempotency_key) <= 128),
    payload_json JSONB NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'leased', 'delivered', 'parked')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    lease_owner TEXT NULL CHECK (lease_owner IS NULL OR octet_length(lease_owner) <= 128),
    lease_generation BIGINT NOT NULL DEFAULT 0 CHECK (lease_generation >= 0),
    lease_expires_at TIMESTAMPTZ NULL,
    last_error TEXT NULL CHECK (last_error IS NULL OR octet_length(last_error) <= 2048),
    correlation_id TEXT NOT NULL CHECK (octet_length(correlation_id) > 0 AND octet_length(correlation_id) <= 128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ordered_outbox_dest_key_seq_uniq UNIQUE (destination, ordering_key, key_seq),
    CONSTRAINT ordered_outbox_dest_idem_uniq UNIQUE (destination, idempotency_key)
);

CREATE INDEX IF NOT EXISTS ordered_outbox_claim_idx
    ON ordered_outbox(destination, ordering_key, key_seq)
    WHERE status IN ('pending', 'leased');
```

---

## 4. Точные инварианты конкурентности, порядка и отказоустойчивости

### 4.1. Строгий порядок блокировок и `committed_cursor` без дыр (V15, V18)
1. **Запрет `BIGSERIAL` как курсора**: `cursor_seq` выделяется **только** в конце короткой транзакции записи через `control_stream_counters` (`stream_id = 'control:journal'`).
2. **Детерминированная иерархия блокировок (0 дедлоков)**:
   - Сначала блокируется доменная строка (`tasks` / `sessions` / `projects` / `inbox_cards` `FOR UPDATE`),
   - затем счётчик сущности `control_stream_counters` (`stream_id = 'entity:<entity_key>'`), дающий `entity_seq`,
   - и **самым последним шагом перед `COMMIT`** — глобальный счётчик `control_stream_counters` (`stream_id = 'control:journal'`), дающий `cursor_seq` и вставляющий строку в `control_events`.
   - Так как PostgreSQL держит row-level `XMAX`-блокировку строки `'control:journal'` до завершения `COMMIT` и появления транзакции в MVCC, ни одна параллельная транзакция с `cursor_seq = N + 1` не может завершиться раньше `cursor_seq = N`. Задержанная транзакция не создаёт «невидимой дыры», которую перепрыгнул бы подписчик.
   - При `ROLLBACK` откатываются одновременно изменение сущности, `entity_seq` и `cursor_seq`.
3. **Согласованный `Snapshot + Subscription` (V18)**:
   - `GET /v1/state/snapshot` открывает транзакцию `ISOLATION LEVEL REPEATABLE READ READ ONLY`, читает в одном MVCC-снимке `(authority_epoch, committed_seq, retained_from_seq)` из `control_stream_counters` (`'control:journal'`) и актуальные коллекции (`projects`, `agents`, `sessions`, `tasks`, `inbox_cards`, `active_grants`).
   - `GET /v1/events?after_cursor=<seq>&epoch=<epoch>&project_id=<opt>&limit=<n>`:
     - если `epoch != current_epoch` -> возвращает `409 Conflict` (`DennettError::Conflict` с `resource = "stream_epoch"`), требуя переснять snapshot;
     - если `after_cursor + 1 < retained_from_seq` -> возвращает `409 Conflict` (`resource = "stream_cursor_expired"`), требуя переснять snapshot (для теста V18 предусмотреть метод `HeadService::advance_retention_watermark(retained_from_seq)` в `TestIsolated` без удаления строк через триггер — с проверкой фильтрации чтения `cursor_seq >= retained_from_seq`);
     - при фильтрации по `project_id`: выбирает диапазон до `limit` сканируемых записей `WHERE cursor_seq > $after_cursor ORDER BY cursor_seq ASC LIMIT $limit`, отбирает в `events` только строки разрешённого `project_id`, но в `next_cursor` (`scanned_through_seq`) возвращает максимальный просканированный `cursor_seq`! Чужие данные не раскрываются, а клиент продвигает курсор без ложного ощущения потерянных событий.

### 4.2. Параллельный `ordered_outbox` с защитой порядка внутри ключа и `lease_generation` (V16, V17)
Захват элемента воркером выполняется строго через соединение с `ordered_outbox_heads`:
```sql
WITH claimable_head AS (
    SELECT h.destination, h.ordering_key, h.next_deliver_seq, o.item_id
    FROM ordered_outbox_heads h
    JOIN ordered_outbox o
      ON o.destination = h.destination
     AND o.ordering_key = h.ordering_key
     AND o.key_seq = h.next_deliver_seq
    WHERE o.available_at <= $1
      AND (
          o.status = 'pending'
          OR (o.status = 'leased' AND o.lease_expires_at <= $1)
      )
    ORDER BY o.cursor_seq ASC
    LIMIT $2
    FOR UPDATE OF h, o SKIP LOCKED
)
UPDATE ordered_outbox o
SET status = 'leased',
    lease_owner = $3,
    lease_generation = o.lease_generation + 1,
    lease_expires_at = $1 + ($4 * INTERVAL '1 millisecond'),
    attempts = o.attempts + 1,
    updated_at = $1
FROM claimable_head ch
WHERE o.item_id = ch.item_id
RETURNING o.*;
```
- Пока `(destination, "task:A", key_seq = 1)` находится в `status = 'leased'` с живым `lease_expires_at`, второй воркер **физически не может** захватить `key_seq = 2` для `"task:A"`, но параллельно захватывает `key_seq = 1` для `"task:B"`.
- Подтверждение (`ack_outbox_item(item_id, expected_lease_generation)`) обновляет строку **только при `WHERE item_id = $1 AND status = 'leased' AND lease_generation = $2`** и в той же транзакции двигает `ordered_outbox_heads.next_deliver_seq = next_deliver_seq + 1`. Опоздавший воркер со старым `lease_generation` получает `Conflict` и не ломает очередь.
- При исчерпании попыток (`attempts >= max_attempts`) элемент переходит в `status = 'parked'`: паркуется **только его `ordering_key`**, остальные ключи продолжают движение.

### 4.3. Защита поколения цели (`generation` fencing) и позднего воркера
1. Любое изменение `goal` или `schedule` в `PATCH /v1/tasks/{task_id}` берёт `SELECT ... FROM tasks WHERE task_id = $1 FOR UPDATE`, атомарно увеличивает `generation = generation + 1`, отменяет старые `pending` таймеры прежнего поколения и (если задача не остановлена и не в `unknown_external`) ставит задачу на выполнение новой цели (`queued` или `waiting_timer` по новому расписанию). Если в этот момент шёл ход старого поколения `g_old`, ему посылается неблокирующий сигнал прерывания старой попытки, **но корректность не зависит от того, успеет ли сигнал до ответа модели**.
2. В `complete_task_run(task_id, run_id, run_generation, outcome)`:
   - транзакция блокирует `tasks WHERE task_id = $1 FOR UPDATE`;
   - **если `task.generation != run_generation` или `task.status == 'cancelled'`**:
     - запись в `runs` сохраняется со статусом `RunStatus::Superseded` (`stop_reason = "superseded_by_generation"`), а поздний текст сохраняется в истории с явной пометкой принадлежности к устаревшему запуску `run_id`;
     - поля актуального `tasks` (`goal`, `status`, `partial_result_summary`, `finished_at`) **НЕ ПЕРЕЗАПИСЫВАЮТСЯ**;
     - `active_run_id` очищается только если `active_run_id == Some(run_id)`.

### 4.4. Таймеры и IANA-расписания через PostgreSQL 16 (Contract I + A10)
- `dennett-contracts` валидирует синтаксис IANA-зоны (`UTC` или `Area/Location`, ASCII без пробелов и управляющих символов — **0 новых крейтов в `dennett-contracts`**).
- `dennett-head` проверяет существование зоны через системный каталог PostgreSQL 16 (`SELECT EXISTS(SELECT 1 FROM pg_timezone_names WHERE name = $1)`) и вычисляет следующее срабатывание `DailyWallTime { hour, minute }` через нативный движок часовых поясов PostgreSQL (`AT TIME ZONE $tz`), автоматически учитывая переходы DST без добавления сторонних tz-зависимостей в Cargo-граф.
- `HeadScheduler` запускается фоновой задачей внутри `HeadService` (плюс публичный детерминированный метод `HeadService::tick_scheduler_at(now: TimestampMs)` для мгновенных тестов без `sleep`!). При перезапуске `Head`:
  - все невыполненные `task_timers` в PostgreSQL сохраняются;
  - просроченные за время простоя таймеры обрабатываются по `missed_run_policy`:
    - `execute_once` (дефолт): запускается **ровно один раз** (даже если за время выключения прошло 20 интервалов), а следующий `fire_at` пересчитывается строго `> now`;
    - `skip`: пропущенное окно помечается `skipped`, создаётся следующий будущий таймер;
    - `ask`: задача переводится в `waiting_approval` и создаётся карточка `inbox_cards` (`kind = 'missed_schedule'`).

### 4.5. Живые интерактивные разрешения (`ask_before_write`) и отзываемые `CapabilityGrants` (A05, A06)
1. **Трейт `PolicyEvaluator` в `adapters/antigravity-acp/src/port.rs` переводится на `BoxFuture`**:
   ```rust
   pub trait PolicyEvaluator: Send + Sync {
       /// Evaluates a pending tool call strictly outside the model (supporting interactive owner approval).
       fn evaluate<'a>(
           &'a self,
           req: &'a PolicyEvaluationRequest,
       ) -> BoxFuture<'a, PolicyEvaluationOutcome>;
   }
   ```
2. **Порядок проверки в `PolicyEngine` (строго вне модели, без обхода границ)**:
   - **Шаг 1 (Жёсткий контур рабочей папки)**: `resolve_within_workspace` для всех чтений/записей. Попытка выйти за `workspace_root`, `..`, UNC или симлинк/NTFS-junction за пределы папки — **немедленный `Deny` без показа карточки подтверждения пользователю**.
   - **Шаг 2 (Явные `Deny`-правила `policy_rules`)**: **немедленный `Deny`**.
   - **Шаг 3 (`WorkspaceWritePolicy`)**:
     - `DenyAll` -> `Deny`;
     - `AllowWorkspace` / `AllowPaths` -> как в D1;
     - **`AskBeforeWrite`**:
       1. `InteractivePolicyEvaluator` делает живой запрос в `dennett_control` к таблице `capability_grants` (`WHERE project_id = $1 AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now()) AND (max_uses IS NULL OR uses_consumed < max_uses)`). Если найден подходящий активный grant (например, выданный кнопкой «Для проекта» или «Для поручения»), атомарно увеличивает `uses_consumed`, повторно проверяет отсутствие `Deny`-правил и возвращает `Allow`! (Если пользователь отозвал grant через `POST /v1/grants/{id}/revoke` секунду назад, запрос в БД не найдёт активный grant и снова потребует подтверждения!)
       2. Если подходящего grant нет — создаёт в одной транзакции запись `permission_requests` (`status = 'pending'`), карточку `inbox_cards` (`kind = 'permission_approval'`) и событие `permission.requested` в `control_events` (при работе в рамках `Task` переводит `tasks.status = 'waiting_approval'`), затем ждёт в `tokio::select!` сигнал разрешения (`POST /v1/permissions/requests/{id}/resolve`), отмену хода (`cancel_rx`) или тайм-аут ожидания.
       3. При вызове `POST /v1/permissions/requests/{id}/resolve` (`ApproveOnce` | `ApproveProject` | `ApproveTask` | `Deny`):
          - проверяется `expected_revision` (второе устройство/повторный клик с устаревшей ревизией получает честный `Conflict` `already_resolved`);
          - проверяется, что запуск не отменён (`Stop`);
          - при `ApproveProject` / `ApproveTask` создаётся отзываемый `capability_grants`;
          - **повторно вызывается `PolicyEngine`** (чтобы убедиться, что пока карточка висела, папку проекта не отвязали и не добавили `Deny`-правило!);
          - записывается `PermissionDecisionRecord` (`evaluated_outside_model = true`), закрывается карточка `inbox_cards` и пробуждается ожидающий `oneshot`, отправляя `allow_once` (или `reject_once` при отказе) в `agy acp`!
          - В сам `agy acp` всегда возвращается **только `allow_once`** (никогда не `allow_always` на стороне подпроцесса!), чтобы каждый последующий вызов инструмента снова проходил через референс-монитор `Head` и мгновенно учитывал отзыв `capability_grants`!

### 4.6. Симулируемый внешний сервис, потерянный `ack`, состояние `UNKNOWN` и честный `Stop`
1. **Порт `ExternalEffectPort` и `SimulatedExternalConnector` (`services/head/src/effects.rs`)**:
   - Исполняет внешние эффекты строго вне открытой SQL-транзакции по протоколу `EffectClaim` ([50](../specifications/50_Dennett_Server_Runtime_Events_Sync_and_Portability.md) §11.3):
     1. Проверка прав и отсутствия `cancelled_flag`.
     2. Транзакция 1: проверка `effect_claims` по `effect_idempotency_key`:
        - чужой `parameters_sha256` -> `409 IdempotencyMismatch`;
        - уже `confirmed` -> возврат сохранённого `receipt` без повторного вызова коннектора;
        - уже `unknown` или `dispatching` -> **отказ от слепого повтора** (`DennettError::Conflict` / возврат статуса `Unknown`, требующего `reconcile`!).
        - новая запись вставляется со `status = 'dispatching'`.
     3. Вызов `ExternalEffectPort::dispatch`:
        - `SimulatedExternalConnector` хранит журнал реально принятых сервисом действий (в памяти + опциональный JSON-файл в изолированном каталоге для проверки рестарта процесса `Head`) и поддерживает детерминированные режимы на запрос/цель:
          - `Success` -> записывает эффект у провайдера (`provider_count = 1`), возвращает `ProviderReceipt`;
          - `Denied` -> `provider_count = 0`, возвращает отказ провайдера (`status = 'denied'`);
          - `FailedBeforeAccept` -> `provider_count = 0`, сбой до принятия (`status = 'failed'`);
          - **`LostAck`** -> **фиксирует эффект на стороне симулируемого сервиса (`provider_count = 1`, выдаёт `provider_operation_id`), но обрывает ответ с ошибкой потери подтверждения!**
     4. Транзакция 2 при `LostAck` (а также при стартовом восстановлении `Head` для любых строк, застрявших в `dispatching` из-за падения демона!):
        - переводит `effect_claims.status = 'unknown'`;
        - переводит связанный `Task` в `TaskStatus::UnknownExternal` (и `RunRecord` в `RunStatus::UnknownExternal`);
        - открывает карточку `inbox_cards` (`kind = 'unknown_effect'`) и публикует событие `effect.unknown`.
2. **Поведение `Stop` при `UNKNOWN` и уже выполненных эффектах**:
   - `POST /v1/tasks/{id}/cancel` и `POST /v1/sessions/{id}/cancel` немедленно выставляют атомарный флаг отмены (блокируя любые **новые** эффекты и ходы), но:
     - если есть эффекты в `dispatching` / `unknown`, статус поручения остаётся/становится `unknown_external`, поле ответа `external_outcome_unknown = true`, и система прямо сообщает, что внешний исход неизвестен (никогда не выдавая `Stop` за гарантированную отмену отправленного действия!);
     - если есть уже подтверждённые эффекты (`confirmed`), они возвращаются в `committed_effects` ответа отмены.
3. **Сверка (`POST /v1/effects/{effect_id}/reconcile` и ручное разрешение)**:
   - Опрашивает `ExternalEffectPort::reconcile(effect_idempotency_key)`: находит у симулированного провайдера уже принятый `provider_operation_id` (при `provider_count == 1`, **0 повторных отправок!**), переводит `effect_claims` в `confirmed`, закрывает карточку `Inbox` и переводит поручение из `unknown_external` в `succeeded` (либо `cancelled`, если пользователь ранее нажимал `Stop`). Также поддерживает явное ручное решение владельца (`mark_confirmed` / `mark_not_executed`), когда внешний провайдер не поддерживает автоматическую сверку.

---

## 5. Контракты API (`dennett-contracts`, HTTP `/v1/*`, `dennettctl`, Tauri IPC)

### 5.1. Новые идентификаторы и структуры в `crates/dennett-contracts/src/control.rs`
Все существующие структуры D1 получают новые поля с `#[serde(default)]`, сохраняя обратную совместимость JSON:
- Новые UUID-типы (`define_control_uuid_id!`): `TaskId`, `TaskTimerId`, `TaskSignalId`, `PermissionRequestId`, `CapabilityGrantId`, `InboxCardId`, `EffectClaimId`, `AgentMessageId`.
- `ProjectLifecycleState`: `Active | Archived | Detached | DeletedRecord` (добавляется `#[serde(default)] pub lifecycle_state: ProjectLifecycleState` и `pub archived_at: Option<TimestampMs>` в `ProjectRecord`).
- `ArtifactCompleteness`: `Partial | Final`; `ArtifactLifecycleState`: `Active | Archived | Removed | Deleted` (добавляются с `#[serde(default)]` в `ArtifactRecord`).
- `TaskStatus`: `Queued | Running | WaitingTimer | WaitingSignal | WaitingClarification | WaitingApproval | Succeeded | Failed | Cancelled | UnknownExternal`.
- `RunStatus`: дополняется вариантами `Superseded` и `UnknownExternal`.
- `ScheduleKind`:
  - `OnceAt { fire_at: TimestampMs }`
  - `Interval { interval_secs: u64 }`
  - `DailyWallTime { hour: u8, minute: u8 }`
- `MissedRunPolicy`: `ExecuteOnce` (default) | `Skip` | `Ask`.
- `TaskSchedule { timezone: String, original_expression: Option<String>, kind: ScheduleKind, missed_run_policy: MissedRunPolicy, next_fire_at: Option<TimestampMs> }`.
- `BudgetExceedPolicy`: `AskApproval` (default) | `Fail`.
- `TaskBudget { max_turns: Option<u32>, max_tokens: Option<u64>, max_wall_secs: Option<u64>, exceed_policy: BudgetExceedPolicy }`.
- `UsageConfidence`: `ProviderReported | Estimated | Unknown`.
- `TaskUsage { turns_used: u32, tokens_used: u64, wall_ms_used: u64, confidence: UsageConfidence }`.
- `TaskRecord`, `TaskTimerRecord`, `TaskSignalRecord`, `PermissionRequestStatus`, `PermissionRequestRecord`, `CapabilityGrantRecord`, `ExternalEffectStatus` (`Prepared | Dispatching | Confirmed | Denied | Failed | Unknown`), `SimEffectBehavior` (`Success | Denied | FailedBeforeAccept | LostAck`), `EffectClaimRecord`, `InboxCardKind`, `InboxCardStatus`, `InboxCardRecord`, `AgentMessageRecord`, `StreamCursor`, `ControlEventRecord`, `StateSnapshotResponse`, `EventStreamResponse`.
- В `SendMessageRequest`: добавляется `#[serde(default, skip_serializing_if = "Option::is_none")] pub client_message_id: Option<String>`.

### 5.2. Маршруты `dennett-head` (`services/head/src/http.rs`)
Поверх всех маршрутов D1 (с той же защитой loopback `Host` / `Origin` / `Sec-Fetch-Site`):

| Метод и путь | Назначение |
|---|---|
| `GET /v1/state/snapshot` | Согласованный `REPEATABLE READ` снимок (`snapshot_cursor` + проекты, сессии, поручения, открытые карточки `Inbox`, активные `grants`; `?project_id=...`, `?include_archived=true`) |
| `GET /v1/events` | Подписка/догонка событий по курсору (`?after_cursor=<u64>&epoch=<u64>&project_id=<uuid>&limit=<u32>`) с `scanned_through_seq` (V15, V18) |
| `POST /v1/tasks` | Создать поручение/обязательство (немедленное, отложенное `OnceAt`, периодическое `Interval`/`DailyWallTime`, либо ожидающее сигнала; опциональный `parent_task_id`, `budget`, `attached_skills`, `auth_profile_override`) |
| `GET /v1/tasks` | Список поручений (`?project_id=...&status=...`) |
| `GET /v1/tasks/{task_id}` | Полная карточка поручения (`TaskDetailResponse`: `task`, `runs`, `timers`, `signals`, `effects`, `open_inbox_cards`) |
| `PATCH /v1/tasks/{task_id}` | Изменить `goal`, `title`, `schedule` или `budget` с атомарным инкрементом `generation` (generation fencing) |
| `POST /v1/tasks/{task_id}/signals` | Доставить идемпотентный сигнал (`idempotency_key`, `signal_name`, `payload_text`; `UNIQUE (task_id, idempotency_key)`) |
| `POST /v1/tasks/{task_id}/reply` | Ответить на уточняющий вопрос агента (`waiting_clarification` -> `queued`) |
| `POST /v1/tasks/{task_id}/cancel` | Адресный `Stop` одного поручения без задевания соседних задач и с честной проверкой `UNKNOWN`/уже выполненных внешних эффектов |
| `GET /v1/inbox` | Список карточек решений `Action Inbox` (`?status=open|resolved|all&project_id=...`) + счётчик `unresolved_count` |
| `POST /v1/inbox/{card_id}/resolve` | Разрешить карточку `Inbox` с проверкой `expected_revision` (подтверждение записи, ответ на вопрос, увеличение бюджета, решение по пропущенному расписанию) |
| `GET /v1/permissions/requests` | Список запросов разрешений (`?status=pending&session_id=...`) |
| `POST /v1/permissions/requests/{request_id}/resolve` | Интерактивное решение (`approve_once`, `approve_project`, `approve_task`, `deny`) с повторной валидацией через `PolicyEngine` |
| `GET /v1/grants` | Список действующих `capability_grants` (`?project_id=...&include_revoked=false`) |
| `POST /v1/grants/{grant_id}/revoke` | Мгновенный отзыв действующего разрешения |
| `POST /v1/agents/{agent_id}/messages` | Отправить текстовое межагентное сообщение (`body_text`, `idempotency_key`, проверка границ scope без повышения прав) |
| `GET /v1/agents/{agent_id}/messages` | Входящие текстовые сообщения агента (`?undelivered_only=true`) |
| `POST /v1/effects/dispatch` | Выполнить внешнее действие через `ExternalEffectPort` (в D2 — `SimulatedExternalConnector` с поддержкой `Success | Denied | FailedBeforeAccept | LostAck`) |
| `GET /v1/effects` | Список `effect_claims` (`?task_id=...&session_id=...&status=...`) |
| `POST /v1/effects/{effect_id}/reconcile` | Сверка состояния `UNKNOWN` с внешним сервисом (или явное ручное урегулирование владельцем) без слепого повтора |
| `POST /v1/projects/{project_id}/rebind` | Перепривязать папку проекта с канонической проверкой и записью provenance |
| `POST /v1/projects/{project_id}/detach-workspace` | Отвязать рабочую папку от проекта без удаления файлов на диске |
| `POST /v1/projects/{project_id}/archive` | Обратимо архивировать проект с приостановкой таймеров (Сценарий 90 §30) |
| `POST /v1/projects/{project_id}/unarchive` | Вернуть проект из архива |
| `POST /v1/projects/{project_id}/remove` | Убрать запись проекта из активного списка Dennett (0 удалений пользовательских файлов) |
| `DELETE /v1/projects/{project_id}` | Удалить служебные scratch-данные и запись проекта в Dennett (`?confirm=true`, 0 удалений внешних папок пользователя!) |
| `GET /v1/sessions/{session_id}/artifacts/{artifact_id}/revisions` | Все версии (`v1`, `v2`, …) выбранного артефакта сессии |
| `POST /v1/sessions/{session_id}/artifacts/{artifact_id}/lifecycle` | Управление жизнью артефакта (`finalize`, `archive`, `unarchive`, `remove`, `delete` — без удаления файлов рабочей папки) |

*(В режиме `ProfileMode::TestIsolated` дополнительно доступен `POST /v1/tasks/scheduler/tick` для мгновенного детерминированного продвижения таймеров и очередей в интеграционных тестах без `sleep`.)*

### 5.3. Паритет `HeadClient`, `dennettctl` и Tauri IPC
1. **Единый прикладной вход (V06)**: все новые операции добавляются в `HeadClient` (`crates/dennett-head-client/src/lib.rs`), затем экспортируются:
   - в `dennettctl` (`task create|list|show|update|signal|reply|cancel`, `inbox list|resolve`, `permission list|resolve`, `grant list|revoke`, `agent send-message|inbox`, `effect dispatch|list|reconcile`, `project list --include-archived|rebind|detach|archive|unarchive|remove|delete`, `artifact revisions|finalize|archive|remove|delete`, `events snapshot|tail`, плюс флаг `--client-message-id` у `dennettctl send`);
   - в Tauri 2 (`apps/desktop/src-tauri/src/commands.rs` и `lib.rs`), где каждая команда клонирует `Arc`-мост `HeadBridge` и освобождает `RwLock` до сетевого вызова, чтобы `resolve_permission_request`, `cancel_task`, `cancel_run` и `get_events` **никогда не блокировались** за выполняющимся `send_message`!

### 5.4. Расширение `SimulatedRuntime` для детерминированных CI-тестов D2
В `adapters/antigravity-acp/src/simulated.rs` добавить детерминированные директивы:
- `[SIM:CLARIFY:<question>]` — испускает `RuntimeEvent::ClarificationRequested { question }` и завершает ход запросом уточнения (`stop_reason = "clarification_needed"`).
- `[SIM:TOKENS:<count>]` — возвращает `usage: { "totalTokens": <count> }` для точной проверки бюджетов `max_tokens`.
- `[SIM:PARTIAL_ARTIFACT:<name>|<content>]` — проверяет сохранение частичного результата (`completeness_status = Partial`) при прерывании/чекпоинте.
- `[SIM:EFFECT:<recipient>|<operation>|<behavior>|<idem_key>]` — позволяет проверить сквозной вызов `ExternalEffectPort` прямо внутри хода поручения (включая `LostAck -> unknown_external`).

---

## 6. Спецификация пользовательского интерфейса Desktop (`WP-M01-003` Fidelity)

### 6.1. Сохранение визуального эталона и жёсткие UX-правила
1. **Материальная среда**: 1-в-1 сохраняется геометрия и эстетика [owner-direction-v5.md](WP-M01-003/owner-direction-v5.md) и [D1_UI_REWORK_BLUEPRINT.md](D1_UI_REWORK_BLUEPRINT.md):
   - трёхчастная левая сетка (`52px` `.activity-rail` + `264px` `.project-sidebar` над бесшовной нижней полоской `.user-settings-bar` высотой `46px` без рамок, и правая сцена `.workspace-stage` со скруглёнными карточками `20px`);
   - **строго 0 элементов `<select>` во всём DOM** (`document.querySelectorAll("select").length === 0`); все меню времени, таймзон, бюджетов, версий `v1/v2` и контекстных действий используют `.floating-popover`;
   - **строго 0 `backdrop-filter`** внутри `html.native-shell`; реалистичный тёмный Mica-фон в `html.browser-mica-preview` для живой проверки в браузере;
   - **строго 0 `dangerouslySetInnerHTML`**.

### 6.2. Навигация в `ActivityRail` и живые индикаторы
В `.activity-rail` активируются рабочие разделы этапа D2 (без превращения интерфейса в перегруженный комбайн):
1. **Чаты и проекты** (`ChatsCircle`, `data-testid="rail-nav-chat"`, хоткей `Alt+1` / клик по любому чату в сайдбаре; повторный клик на активном разделе чата сворачивает/разворачивает `.project-sidebar`, сохраняя `Ctrl+B`).
2. **Входящие и Радар (`Inbox & Radar`)** (`Tray`, `data-testid="rail-nav-inbox"`, хоткей `Alt+2`):
   - Показывает компактный числовой бейдж `unresolvedInboxCount` (`data-testid="rail-inbox-badge"`) **только когда `unresolvedInboxCount > 0`** (при `0` бейдж полностью скрыт!).
3. **Поручения и расписания (`Tasks & Timers`)** (`ListChecks`, `data-testid="rail-nav-tasks"`, хоткей `Alt+3` — активируется вместо заглушки D1!):
   - Показывает аккуратную живую точку/счётчик активных или ожидающих поручений, если они есть.
4. **Рабочее пространство (`Workspace`)** (`Browsers`, `data-testid="rail-toggle-workspace"`, `Ctrl+\`) — открывает/скрывает правую соседнюю карточку `.workspace-panel`.
5. **Интеграции и MCP** (`Plug`, `rail-future`, disabled, честный тултип `Интеграции и MCP · этап D3`).
6. Внизу рейла над `.user-settings-bar` остаётся `<QuotaRings />`.

**Пространственная стабильность**: левая колонка `.project-sidebar` остаётся видимой во всех трёх режимах (`chat`, `inbox`, `tasks`), позволяя в 1 клик вернуться в любой разговор или отфильтровать `Inbox`/`Поручения` по выбранному проекту.

### 6.3. Режим `Входящие и Радар` (`InboxRadarCard.tsx`) и живые подтверждения в чате
1. **Двойная подача срочного подтверждения (`30 §25.6`)**:
   - Если пользователь прямо сейчас находится в открытом чате, где агент с политикой `Спрашивать запись` (`ask_before_write`) запросил изменение файла, карточка подтверждения появляется **мгновенно прямо над композером активного чата** (`InlineApprovalBanner`, получаемый по потоку `/v1/events`) **И** одновременно отражается во `Входящих` (`InboxRadarCard`)! Пользователю не нужно уходить из чата, чтобы нажать `Разрешить`.
2. **Карточки `Action Inbox` (никаких пустых «Разрешить продолжить?», всегда точные параметры)**:
   - **Подтверждение записи / инструмента (`permission_approval`)**:
     - Показывает имя агента, проект, точный инструмент (`write_file` / `edit_file` / `run_command`) и точный список относительных путей внутри папки проекта.
     - Три чёткие кнопки в 1 клик:
       1. **`Разрешить`** (`ApproveOnce` — только этот вызов);
       2. **`Для проекта`** (`ApproveProject` — создаёт видимое отзываемое разрешение `capability_grant` на запись в папку этого проекта);
       3. **`Отклонить`** (`Deny`).
   - **Вопрос агента (`clarification`)**:
     - Показывает вопрос агента, название поручения/проекта и **встроенное поле быстрого ответа** (`input` + кнопка `Ответить`, отправка по `Enter`) + вторичную кнопку `Остановить поручение`.
   - **Неизвестный внешний исход (`unknown_effect` — `Неизвестно, выполнено ли`)**:
     - Выделен явным предупреждающим бейджем **`Неизвестно, выполнено ли`** (никогда не смешивается с обычной ошибкой или ожиданием!).
     - Показывает получателя, операцию, ключ идемпотентности и честное объяснение: *«Запрос отправлен во внешний сервис, но подтверждение потеряно. Слепой повтор заблокирован, чтобы не создать дубль.»*
     - Действия: **`Проверить у сервиса`** (`reconcile`), **`Подтвердить вручную (выполнено)`**, **`Пометить не выполненным`**. Кнопки слепого `Повторить` нет!
   - **Исчерпан бюджет (`budget_exceeded`)**:
     - Показывает фактический расход против лимита (`ходов`, `токенов` или `секунд`) и кнопки **`Добавить бюджет и продолжить`** / **`Завершить`**.
3. **Полоса `Действующие разрешения` и `Радар активности`**:
   - В `InboxRadarCard` отображаются активные отзываемые `capability_grants` текущего проекта с кнопкой **`Отозвать`** в 1 клик (`POST /v1/grants/{id}/revoke`), а ниже — компактный `Agent Radar` активных/ожидающих/проблемных запусков со временем последнего прогресса и адресным `Stop`.

### 6.4. Режим `Поручения` (`TasksCard.tsx`) — нулевая бюрократия и различимые статусы
1. **Быстрое создание обязательства в одну строку (без «админских анкет»)**:
   - Поле ввода цели (`Напишите поручение или напоминание для агента…`, отправка по `Enter`) и компактные полупрозрачные пиллы-триггеры в нижней плашке (аналогично композеру чата, **0 `<select>`**):
     - **Когда (`SchedulePopover`)**:
       - `Сразу` (фоновый запуск сейчас),
       - `Через 10 сек` (удобно для быстрой живой проверки владельцем!), `Через 5 мин`, `Через 1 час`,
       - `Ежедневно в 09:00` (с выбором часа/минут в кастомном поповере и явным отображением IANA часового пояса `Intl.DateTimeFormat().resolvedOptions().timeZone`, например `Asia/Yekaterinburg`),
       - `Ждать сигнал` (ожидание внешнего/межагентного сигнала по имени).
     - **Бюджет (`BudgetPopover`)**:
       - `Стандартный` (без жёсткого отсечения), `До 3 ходов`, `До 10 ходов`, `До 60 сек` (плюс переключатель поведения при исчерпании: `Спросить меня` / `Остановить с ошибкой`).
     - Кнопка **`Поручить`**.
2. **Визуально безошибочное различие состояний (обязательный пункт личного контроля H2)**:
   Каждая карточка поручения имеет контрастный семантический пилл состояния (`data-testid="task-status-pill-<status>"`):
   - `Ожидает · 00:08` (`waiting_timer` с живым тикающим обратным отсчётом до `fire_at`!) / `Ожидает сигнала` / `Ожидает ответа` / `Ожидает разрешения` — спокойный холодный/янтарный контур;
   - `Выполняется` (`running`) — живой индикатор и кнопка **`Stop`** (`data-testid="task-stop-btn-<id>"`), останавливающая **только это поручение**;
   - **`Неизвестно, выполнено ли`** (`unknown_external`) — контрастный предупреждающий бейдж с иконкой вопроса/щита и переходом к сверке (`Сверить у сервиса`);
   - `Ошибка` (`failed`) — коралловый бейдж + понятная причина (включая `Лимит бюджета исчерпан`);
   - `Остановлено` (`cancelled`) — спокойный нейтральный бейдж с сохранением `partial_result_summary` и отметкой об уже выполненных до остановки действиях;
   - `Выполнено` (`succeeded`) — аккуратный завершённый бейдж с кратким итогом.
3. **Inline-редактирование цели и быстрые переходы**:
   - Клик по карандашу / двойной клик по тексту цели поручения открывает inline-редактор (`Enter` -> `PATCH /v1/tasks/{id}`): на карточке аккуратно отображается обновлённое поколение (`цель v2`), если цель менялась!
   - Кнопка `Открыть чат` мгновенно открывает связанную сессию поручения; в тестовом/демо-режиме у поручения в `waiting_signal` есть компактная кнопка `Подать сигнал…` (с ключом идемпотентности) для наглядной демонстрации.

### 6.5. Возврат `Спрашивать запись` в `Composer`, офлайн-черновики и контекстные меню `…`
1. **Полный выбор доступа в `Composer.tsx`**:
   - В поповере `ShieldCheck` теперь доступны все 3 полноценных рабочих режима:
     - **`Спрашивать запись`** (`ask_before_write`) — *«Запрашивать подтверждение в чате и Входящих перед изменением каждого файла»*;
     - **`Запись в папку`** (`allow_workspace`) — *«Разрешить запись строго внутри корневой папки проекта»*;
     - **`Только чтение`** (`deny_all`) — *«Запретить любые файловые изменения в этом проекте»*.
2. **Подписка по `committed_cursor` и устойчивость черновиков при обрыве связи с `Head`**:
   - `WorkspaceProvider` при подключении загружает `/v1/state/snapshot` (запоминая `snapshot_cursor`), затем опрашивает `/v1/events?after_cursor=...` каждые `800ms` при активной работе / `2500ms` в покое (и немедленно при старте хода!), обновляя карточки разрешений, статусы поручений и артефакты без полной перезагрузки экрана; при `409 stream_epoch / stream_cursor_expired` автоматически делает чистый resync через `/v1/state/snapshot`.
   - Черновики `draftsBySession` и локальная очередь неотправленных сообщений `queuedSubmissions` (каждое со стабильным `client_message_id`) синхронно сохраняются в `localStorage` (`dennett.desktop.d2.drafts.v1`).
   - При падении/недоступности `dennett-head`:
     - открытый разговор и его поле ввода **не блокируются намертво**: вверху карточки чата показывается честная плашка офлайн-режима (*«Связь с dennett-head потеряна — вы можете продолжать писать черновик или поставить сообщение в очередь проверки»*), а набранный текст переживает перезагрузку окна;
     - при нажатии `Сохранить к отправке` офлайн-сообщение помещается в видимую локальную очередь со статусом *«Ждёт восстановления связи»* (не притворяясь отправленным!);
     - после возвращения `Head` пользователь нажимает `Отправить сохранённое` (либо подтверждает перепроверку очереди): если проект не был архивирован/удалён, сообщение отправляется со своим `client_message_id`, гарантируя **ровно 1 логический ход** даже при повторном разрыве ответа.
3. **Контекстное меню `…` (`DotsThree`) у проектов в `Sidebar.tsx` и секция `Архив`**:
   - У каждого проекта кнопка `…` открывает полупрозрачный `.floating-popover` с чётким разделением операций контракта C:
     - `Переименовать` (`F2`);
     - `Привязать папку…` / `Перепривязать папку…` (`rebind`);
     - `Отвязать папку` (только связь, файлы на диске не трогаются);
     - `Архивировать в Dennett` (или `Вернуть из архива` для архивных);
     - `Убрать из списка Dennett` (`remove` — без удаления файлов);
     - `Удалить данные проекта в Dennett…` (`delete` — открывает компактный диалог подтверждения с точным однострочным объяснением последствий: *«Служебные данные проекта в Dennett будут удалены; папка с вашими файлами на диске останется нетронутой»*).
   - Если есть архивированные проекты (`lifecycle_state === "archived"`), внизу списка проектов появляется аккуратная сворачиваемая строка `Архив (N)` для просмотра и разархивации в 1 клик.
4. **Версии артефактов (`v1`, `v2`, …) и контекстное меню `…` в `WorkspacePanel.tsx`**:
   - Артефакты в правой панели группируются по логическому имени внутри сессии, показывая бейдж полноты (`Частичный` для `partial` vs `Финальный` для `final`) и номер последней версии (`v2`).
   - В шапке предпросмотра выбранного артефакта отображаются интерактивные пиллы версий (`v1`, `v2`, `v3`… — `data-testid="artifact-revision-pill-<rev>"`): клик по любой версии мгновенно показывает её содержимое и SHA-256 без потери новых версий!
   - Кнопка `…` у артефакта даёт раздельные действия: `Отметить финальным` (если `partial`), `Архивировать`, `Убрать из списка`, `Удалить содержимое из Dennett…` (никогда не удаляя файлы в папке проекта).

### 6.6. Демо-стенд `dennett-check demo-d2` (`ops/runner/src/dennett_runner/cli.py`)
Добавить команду `uv run --project ops/runner dennett-check demo-d2` (с флагами `--runtime antigravity|simulated`, `--auth-profile oauth-personal|gemini-api-key`, `--no-ui`, `--dry-run`, при сохранении `demo-d1` для регрессионной совместимости):
- поднимает изолированный PostgreSQL 16 + самостоятельный `dennett-memoryd` (с HMAC `dnt1.`) + `dennett-head` (схема `v4`, запущенный `HeadScheduler` и `SimulatedExternalConnector` в изолированном каталоге);
- предсоздаёт чистое рабочее пространство «Рабочее пространство» с агентом в режиме `Спрашивать запись` (`ask_before_write`) или `Запись в папку`, пустым чатом «Новый чат» и готовым окружением для личной проверки владельцем таймеров, смены цели, адресного `Stop` и состояния `Неизвестно, выполнено ли`.

---

## 7. Обязательная матрица тестов и критерии приёмки H2

Ни один тест не может быть `skipped`, пустым или проверять только собственный mock ([TEST_CATALOGUE_AND_QUALITY_GATES.md](../testing/TEST_CATALOGUE_AND_QUALITY_GATES.md)).

### 7.1. Быстрые unit и `proptest` проверки (`dennett-check fast`)
1. **`proptest` инвариантов D2**:
   - монотонность и идемпотентность переходов `TaskStatus` и `generation` fencing (любая последовательность `create -> run(g1) -> patch_goal(g2) -> late_complete(g1) -> complete(g2)` никогда не затирает состояние `g2`);
   - идемпотентность `task_signals` и `effect_claims` (повтор того же ключа с тем же хешем — no-op, с иным хешем — `IdempotencyMismatch`);
   - проверка бюджетов (`max_turns`, `max_tokens`, `max_wall_secs`) при граничных значениях `limit - 1`, `limit`, `limit + 1`;
   - ослабление полномочий при делегировании помощнику (`child_scope ⊆ parent_scope` и монотонность `WorkspaceWritePolicy`).
2. **Unit-тесты `PolicyEngine` + `CapabilityGrants`**:
   - `AskBeforeWrite` без grant требует интерактивного подтверждения; с активным grant в пределах пути — разрешает; с истёкшим или отозванным (`revoked_at IS NOT NULL`) grant — снова блокирует/запрашивает подтверждение; попытка выхода за `workspace_root` отклоняется **до** интерактивного запроса даже при наличии широкого grant.

### 7.2. Реальные PostgreSQL 16 + мультипроцессные интеграционные тесты (`dennett-check integration --manage-pg`)
1. **Миграция `v1 -> v2 -> v3 -> v4`**: чистый старт, переход с базы `v3` с существующими данными D1, повторный идемпотентный прогон, проверка контрольных сумм и append-only триггеров `control_events`.
2. **Переживание перезапуска таймером и сигналы (`A10`)**: создать поручение с `OnceAt` / `Interval`, остановить `HeadService` / демон `dennett-head`, поднять заново на той же БД — таймер не потерян, `execute_once` срабатывает ровно 1 раз без лавины дублей; повторная отправка `POST /v1/tasks/{id}/signals` с тем же `idempotency_key` (в том числе параллельно из двух клиентов!) вызывает ровно 1 логическое пробуждение/ход, а с изменённым текстом возвращает `409 IDEMPOTENCY_MISMATCH`.
3. **Гонка изменения цели и позднего воркера (`V07` + `generation` fencing)**: запустить медленный ход `generation = 1` (`[SIM:SLOW:...]`), параллельно выполнить `PATCH /v1/tasks/{id}` с новой целью (`generation = 2`), дождаться завершения старого воркера — старый `RunRecord` получает статус `superseded`, а `TaskRecord` сохраняет цель `generation = 2` и выполняет её без перезаписи старым ответом.
4. **Адресный `Stop` и текстовая кооперация без графа потребителей (`V07`, `A07`)**: запустить два параллельных поручения `Task A` и `Task B` (а также родительское поручение и делегированного помощника); вызвать `POST /v1/tasks/{A}/cancel` — `Task A` мгновенно останавливается и не начинает новых инструментов/эффектов после `Stop`, тогда как `Task B` и помощник штатно завершают работу; проверить доставку текстового сообщения через `POST /v1/agents/{id}/messages` в контекст следующего хода получателя.
5. **Интерактивный цикл `ask_before_write` и отзыв `CapabilityGrant` (`A05`, `A06`)**: запустить ход с записью файла при `AskBeforeWrite` -> убедиться по `/v1/events`, что появилась `pending` карточка в `permission_requests` и `inbox_cards`, а файл на диске **ещё не создан** -> разрешить (`approve_project`) -> ход завершается и файл появляется ровно 1 раз -> отозвать grant (`POST /v1/grants/{id}/revoke`) -> следующий ход записи снова ждёт подтверждения; отдельно проверить, что `Stop` во время ожидания подтверждения немедленно отменяет запрос и поздний `resolve` отвергается.
6. **Журнал доставки `V15`, `V16`, `V17`, `V18` на реальном PostgreSQL**:
   - **V15**: с помощью управляемого барьера задержать фиксацию транзакции A и параллельно зафиксировать транзакцию B — доказать, что подписчик `/v1/events?after_cursor=...` никогда не пропускает событие A;
   - **V16**: два конкурентных воркера `ordered_outbox` на одном `ordering_key` + втором независимом ключе — доказать строгий порядок `key_seq = 1, 2, 3` внутри одного ключа, параллелизм разных ключей и отказ позднему `ack` со старым `lease_generation` после истечения lease;
   - **V17**: имитировать падение воркера после доставки до `ack` — повторная доставка после истечения lease не создаёт дубликата у идемпотентного приёмника;
   - **V18**: конкурентная запись между `/v1/state/snapshot` и `/v1/events?after_cursor=snapshot_cursor` не теряет и не дублирует ни одного события; скрытые чужим `project_id` записи продвигают `scanned_through_seq` без утечки payload; устаревший курсор и смена `epoch` возвращают детерминированный сигнал resync.
7. **Потерянный `ack`, состояние `UNKNOWN` и запрет слепого повтора (`Contract B / 50 §11.5`)**: выполнить `SimulatedExternalConnector` в режиме `LostAck` -> доказать, что во внешнем сервисе `execution_count == 1`, в `Head` записано `status = unknown` (`TaskStatus::UnknownExternal`), открыта карточка `Inbox`, повторный вызов с тем же ключом и перезапуск `Head` **не делают второй отправки** (`execution_count` остаётся `1`!), `Stop` возвращает `external_outcome_unknown = true` (не обещая отмену отправленного эффекта), а `POST /v1/effects/{id}/reconcile` переводит эффект в `confirmed` при `execution_count == 1`.
8. **Идемпотентность `client_message_id`, жизненный цикл `archive/rebind/detach/remove/delete` и версии артефактов (`Contract C / D`, `90 §30`)**:
   - повтор `POST /v1/sessions/{id}/messages` с тем же `client_message_id` возвращает тот же `RunRecord` без второго запуска модели;
   - `archive` приостанавливает таймеры проекта и обратим через `unarchive`; `rebind` канонизирует новую папку и вызывает честный rehydrate сессии; `detach`, `remove` и `delete?confirm=true` оставляют физическую пользовательскую папку и все её файлы **100% нетронутыми на диске**;
   - артефакт с `v1 (partial)` -> `v2 (final)` отдаёт полную историю через `/revisions`, а `archive` / `remove` / `delete` артефакта не затрагивают файлы рабочей папки.

### 7.3. Контрольные мутации для `@d2-code-verifier` (Carbon)
Независимый верификатор обязан проверить чувствительность тестов минимум на 4 локальных мутациях (с полным откатом до байта после проверки):
1. Убрать проверку `task.generation == run_generation` при завершении хода -> тест позднего воркера обязан упасть.
2. Убрать соединение с `ordered_outbox_heads` в `SKIP LOCKED` запросе -> тест V16 обязан поймать нарушение порядка одного ключа.
3. Разрешить автоматический повтор при `effect_claims.status == 'unknown'` -> тест `LostAck / UNKNOWN` обязан поймать `execution_count == 2`.
4. Пропустить проверку `revoked_at IS NULL` у `capability_grants` -> тест отзыва разрешения обязан упасть.

### 7.4. Компонентные тесты Vitest и живой браузерный UX-аудит (`@d2-ui-reviewer` на Carbon)
1. **Vitest (`apps/desktop`)**:
   - `0` элементов `<select>` во всех новых экранах и поповерах (`Inbox`, `Поручения`, `SchedulePopover`, `BudgetPopover`, меню `…` проектов и артефактов, переключатель `v1/v2`);
   - живая карточка `Спрашивать запись` (`Разрешить` / `Для проекта` / `Отклонить`) и бейдж `Inbox` в `ActivityRail`;
   - визуальное и текстовое различие пиллов `Ожидает`, `Выполняется`, `Ошибка`, `Неизвестно, выполнено ли` и `Остановлено`, inline-правка цели и адресный `Stop`;
   - переключение версий `v1` / `v2` в `WorkspacePanel` и сохранность локального черновика / офлайн-очереди с `client_message_id` при отключении `Head` и последующем переподключении.
2. **Живой проход `@d2-ui-reviewer` во встроенном браузере на реальном стеке (`demo-d2 --no-ui` + Vite dev proxy на `1280×800` и `1024×720`)**:
   - физически прокликать переключение `Чаты` ↔ `Входящие` ↔ `Поручения` ↔ правая панель `Workspace`;
   - включить `Спрашивать запись` в композере, попросить живого агента `agy acp` создать файл, увидеть живую карточку подтверждения, одобрить её в 1 клик и увидеть реальный файл в `Workspace`;
   - создать отложенное поручение, изменить его цель inline, остановить одно поручение без остановки соседнего, проверить карточку `Неизвестно, выполнено ли` и сверку, переключить `v1`/`v2` артефакта и проверить меню `…` проекта (архивация и возврат из архива без потери данных);
   - убедиться в отсутствии наездов текста, обрезанных кнопок, системных `<select>`, сломанного контраста и визуального шума.
