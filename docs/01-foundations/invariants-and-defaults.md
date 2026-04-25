[English](#english) | [Русский](#russian)

## English

# Invariants And Defaults

Status: approved foundational specification.  
Owns: non-negotiable project laws, base-model behavioral constraints, and defaults that apply when configuration is silent.  
Does not own: exact field syntax or validation schema text.  
Primary sources: [canonical specification](../../agent_orchestrator_final_spec_v2.md), [source-of-truth model](./source-of-truth-model.md), [project scope](./project-scope-and-non-goals.md).

## How To Read This Document

This document distinguishes three things:

- Invariant: a rule that implementation must preserve.
- Default: a behavior used only when no explicit override exists.
- Explicit configuration: user- or file-provided intent that overrides a default but may not break an invariant.

When defaults and invariants seem to compete, the invariant wins.

## Project Invariants

### Product Identity

- The product is an orchestrator of agent runs, not a new agent runtime.
- The portable agent definition lives in a JSON file and remains a first-class artifact.
- The orchestrator owns the execution boundary of the agent call, not the runtime's internal reasoning.

### Integration Model

- `skills`, `MCPs`, and `plugins` follow the compatible runtime ecosystem rather than a new custom contract owned by this project.
- `MCP` is the correct model term for external capabilities; `SDK` is an implementation detail, not a domain concept.
- Runtime integration must go through an adapter boundary rather than direct vendor dependencies in core.

### Execution Model

- The base graph model is sequential: one active node at a time.
- Edges control execution flow only; data travels through params, vars, node outputs, and event payload.
- A node returns only its final output, not hidden internal runtime history.
- If a node expects JSON output, the author must obtain it through explicit prompting or tooling; core does not inject hidden coercion.
- If multiple nodes write the same var name, the latest value wins.

### State Model

- Chats store only visible history and the data required for resume.
- Resume is explicit. The system must not pretend unsupported state can always be reconstructed implicitly.
- Triggers live outside the agent file and cause one launch without automatic retry in the base model.

### Versioning Model

- `graph_contract_version` is mandatory for the agent file.
- `graph_contract_version`, logical agent version, live revision, and tool version are independent axes and must not be collapsed into one number.
- If a graph contract version is unsupported, core must fail clearly instead of guessing.

## Canonical Defaults

These defaults apply when no explicit configuration says otherwise:

| Setting | Default | Consequence |
| --- | --- | --- |
| `final_output.mode` | `last_node_output` | The final answer is the output of the last successful terminal node. |
| `chat.prefer_native_resume` | `true` | Prefer a runtime-native resume path when available, without changing the source-of-truth model for chats and resume. |
| `chat.store_visible_messages` | `true` | Visible conversation should be persisted unless explicitly disabled. |
| `chat.store_context_window` | `true` | Store the context needed for continuation unless explicitly disabled. |
| `chat.allow_fresh_start` | `true` | A new conversation may start without forcing resume. |
| `interaction.comments.enabled` | `false` | User comments during execution are opt-in. |
| `interaction.user_mcp.enabled` | `false` | The built-in user communication MCP is opt-in. |
| `chat.secret_markers.enabled` | `false` | Secret-marker behavior is disabled unless explicitly enabled. |
| Node-level `permissions` fallback | Inherit top-level `permissions` | Node configuration does not need to repeat shared permissions. |
| Missing top-level and node-level `permissions` | Runtime default | Absence of explicit permissions does not force a project-defined synthetic permission set. |
| Interface close policy | `keep_core_running` | Closing the interface does not stop core unless explicitly configured otherwise. |

## Required Explicitness

Some behavior is intentionally not defaulted:

- Node `output` contract must be explicit; there is no hidden default.
- Unsupported graph versions must fail explicitly.
- Any behavior that broadens the base model, such as extension-specific capability, must be introduced by explicit spec and configuration rather than assumption.

## Implementation Consequences

- Defaults should be encoded in one clear place, then surfaced consistently through validation, runtime behavior, and tests.
- Invariants should be backed by negative tests where possible so later refactors cannot accidentally weaken them.
- Avoid "helpful" hidden behavior that makes the system seem convenient while silently violating the model above.

## Russian

# Инварианты И Дефолты

Статус: утвержденная foundational-спецификация.  
Владеет: нерушимыми законами проекта, ограничениями поведения базовой модели и дефолтами, которые применяются при отсутствии явной конфигурации.  
Не владеет: точным field syntax или текстом validation schema.  
Основные источники: [каноническая спецификация](../../agent_orchestrator_final_spec_v2.md), [модель source of truth](./source-of-truth-model.md), [scope проекта](./project-scope-and-non-goals.md).

## Как Читать Этот Документ

Этот документ различает три вещи:

- Invariant: правило, которое реализация обязана сохранять.
- Default: поведение, которое используется только при отсутствии явного override.
- Explicit configuration: пользовательское или файловое намерение, которое переопределяет default, но не может ломать invariant.

Если кажется, что default и invariant конфликтуют, побеждает invariant.

## Инварианты Проекта

### Идентичность Продукта

- Продукт — это оркестратор запусков агентов, а не новая agent runtime-система.
- Переносимое определение агента живет в JSON-файле и остается first-class артефактом.
- Оркестратор владеет границей исполнения вызова агента, а не внутренним reasoning runtime.

### Модель Интеграции

- `skills`, `MCPs` и `plugins` следуют экосистеме совместимого runtime, а не новому кастомному контракту этого проекта.
- `MCP` — корректный модельный термин для внешних возможностей; `SDK` — деталь реализации, а не доменная сущность.
- Runtime-интеграция обязана идти через adapter boundary, а не через прямые vendor dependencies в core.

### Модель Исполнения

- Базовая модель графа последовательна: в один момент времени активна только одна нода.
- Edges управляют только execution flow; данные идут через params, vars, node outputs и event payload.
- Нода возвращает только свой final output, а не скрытую внутреннюю историю runtime.
- Если нода ожидает JSON output, автор должен получить его через явный prompt или tooling; core не добавляет скрытую coercion-логику.
- Если несколько нод записывают одно и то же имя var, побеждает последнее значение.

### Модель Состояния

- Chats хранят только видимую историю и данные, нужные для resume.
- Resume является явным. Система не должна притворяться, что неподдерживаемое состояние всегда можно восстановить неявно.
- Triggers живут вне agent file и в базовой модели инициируют один запуск без automatic retry.

### Модель Версионирования

- `graph_contract_version` обязателен для agent file.
- `graph_contract_version`, логическая версия агента, live revision и версия утилиты — независимые оси, и их нельзя схлопывать в одно число.
- Если версия graph contract не поддерживается, core обязан падать явно, а не догадываться.

## Канонические Дефолты

Эти дефолты действуют, если явная конфигурация не говорит иного:

| Настройка | Дефолт | Последствие |
| --- | --- | --- |
| `final_output.mode` | `last_node_output` | Финальный ответ равен output последней успешной терминальной ноды. |
| `chat.prefer_native_resume` | `true` | При наличии native resume путь runtime предпочитается, но это не меняет модель source of truth для chat и resume. |
| `chat.store_visible_messages` | `true` | Видимая история разговора сохраняется, если это явно не отключено. |
| `chat.store_context_window` | `true` | Контекст, нужный для продолжения, сохраняется, если это явно не отключено. |
| `chat.allow_fresh_start` | `true` | Можно начать новый разговор без обязательного resume. |
| `interaction.comments.enabled` | `false` | Комментарии пользователя во время исполнения включаются только явно. |
| `interaction.user_mcp.enabled` | `false` | Встроенный пользовательский communication MCP включается только явно. |
| `chat.secret_markers.enabled` | `false` | Поведение secret markers отключено, пока не включено явно. |
| Fallback для node-level `permissions` | Наследование top-level `permissions` | Ноде не нужно дублировать общие права. |
| Отсутствие и top-level, и node-level `permissions` | Runtime default | Отсутствие явных прав не заставляет проект изобретать синтетический permission set. |
| Политика закрытия интерфейса | `keep_core_running` | Закрытие интерфейса не останавливает core, пока это явно не переопределено. |

## Где Требуется Явность

Некоторые вещи специально не имеют дефолта:

- Контракт `output` у ноды должен быть явным; скрытого значения по умолчанию нет.
- Неподдерживаемые версии графа должны приводить к явной ошибке.
- Любое поведение, расширяющее базовую модель, например extension-specific capability, должно вводиться явной спецификацией и конфигурацией, а не предположением.

## Последствия Для Реализации

- Дефолты должны кодироваться в одном ясном месте и затем одинаково проявляться в validation, runtime behavior и tests.
- Инварианты по возможности должны быть подкреплены negative tests, чтобы последующие refactor не смогли их незаметно ослабить.
- Избегайте "полезного" скрытого поведения, которое делает систему удобнее на вид, но тайно нарушает модель выше.
