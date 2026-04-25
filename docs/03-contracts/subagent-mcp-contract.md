[English](#english) | [Русский](#russian)

<a id="english"></a>
# English

# Subagent MCP Contract

Status: normative owner for the managed child-run MCP surface.

Owns: orchestrator-visible `launch`, `wait`, `send`, and `close` semantics for managed subagent work; bounded capability rules; owned value objects and enums for the managed task package; normalized result-shape rules; and invalid boundary cases.

Does not own: the overall orchestration model, graph-node contracts, execution sequencing, persisted state layout, or runtime-vendor internals.

Related documents:

- [Subagent Orchestration Model](../02-architecture/subagent-orchestration-model.md)
- [Contracts README](./README.md)
- [Subagent Task Lifecycle](../04-execution/subagent-task-lifecycle.md)
- [Subagent Context and Memory](../05-state/subagent-context-and-memory.md)
- [Graph Execution](../04-execution/graph-execution.md)
- [Builder Agent](../08-extensions/builder-agent.md)
- [Runtime Integration Model](../02-architecture/runtime-integration-model.md)

This document defines the product-level MCP surface used by Core to manage a bounded child run.
It is vendor-neutral. It defines logical methods, owned value objects, and payload obligations, not transport framing, SDK names, or proprietary session objects.

The base model is interaction-silent at the parent user boundary.
Child runs may exchange internal control messages with their immediate parent orchestrator, but they do not surface live comments or user chat through the parent run.

## 1. Managed Child Run

A managed child run is not a generic tool invocation.

It is a Core-owned orchestration object with:

- a parent run reference;
- a parent task reference;
- a child role;
- a scoped objective;
- an explicit write set;
- an explicit read-context package;
- budget and depth limits;
- an explicit control surface while live;
- a declared terminal return surface.

Managed child runs and `orchestrator_agent` nodes belong to the same child-run family:

- both create a parent/child lineage boundary;
- both are interaction-silent at the parent user boundary in the base model;
- both return only normalized terminal data across that boundary.

They are not the same product surface:

- `orchestrator_agent` is the portable graph child-run primitive. It selects a child agent by `agent_ref`, sends resolved node input, and receives the child run's normalized terminal return.
- The managed subagent MCP surface is the richer Core-owned delegation surface. It adds task-package semantics such as `child_role`, `write_set`, `read_context`, `budgets`, reviewer findings, and explicit control and close behavior.
- An implementation may realize a managed subagent through the same underlying child-run machinery used by `orchestrator_agent`, but that mapping is internal. Callers must not infer managed-task-package fields from a plain `orchestrator_agent` node, and graph execution must not infer graph-node fields from this MCP contract.

## 2. Owned Value Objects

This contract owns the meaning of the following objects and enums.

### 2.1. `write_set`

`write_set` is a closed object with these fields:

| Field | Type / constraints | Meaning |
| --- | --- | --- |
| `mode` | literal `allow_list` | The child may mutate only listed targets. |
| `items` | non-empty array of closed `write_target` objects | Explicit writable resources. |

Each `write_target` has these fields:

| Field | Type / constraints | Meaning |
| --- | --- | --- |
| `resource_kind` | `file`, `directory`, `document`, `structured_record`, or `generic_resource` | Product-level kind of writable resource. |
| `resource_ref` | non-empty string | Stable parent-understood resource identifier. |
| `scope` | `exact` or `descendants` | Whether authority applies only to the named resource or also to its descendants. |
| `access` | `modify_existing`, `create_within`, `create_or_modify`, or `delete` | Allowed mutation class for that target. |

Rules:

- `items` must not contain duplicates.
- Free-form prose, ambiguous wildcards, and vendor-specific handles are not valid replacements for structured targets.
- A sibling conflict exists if two live children hold overlapping writable authority over the same resource or ancestor/descendant chain.

### 2.2. `read_context`

`read_context` is a closed object with these fields:

| Field | Type / constraints | Meaning |
| --- | --- | --- |
| `mode` | `explicit_only` or `explicit_plus_dependencies` | Whether the child receives only listed items or listed items plus owner-approved dependencies of those items. |
| `items` | array of closed `read_context_item` objects | Explicit readable context package. |

Each `read_context_item` has these fields:

| Field | Type / constraints | Meaning |
| --- | --- | --- |
| `context_kind` | `file`, `directory_snapshot`, `document`, `prior_result`, `policy`, `summary`, or `structured_state` | Product-level kind of readable context. |
| `context_ref` | non-empty string | Stable reference understood by the parent and runtime. |
| `inclusion` | `full`, `excerpt`, `summary`, or `reference_only` | How much of the referenced context is passed. |
| `required` | boolean | Whether launch must fail if the item is unavailable. |

Rules:

- The child does not inherit undeclared parent context outside this object.
- If a `required` item is unavailable at launch time, launch must fail before the child starts.

### 2.3. `budgets`

`budgets` is a closed object. Each present field must be a positive integer.

Allowed fields are:

- `max_steps`
- `max_tool_calls`
- `max_wall_clock_seconds`
- `max_spawn_depth`
- `max_children`
- `max_review_loops`

Rules:

- Omitted fields mean "use the stricter parent or Core policy limit".
- `null`, negative values, and zero are invalid.
- A child budget must never widen a stricter parent-tree limit.

### 2.4. `lineage`

`lineage` is a closed object with these fields:

| Field | Type / constraints | Meaning |
| --- | --- | --- |
| `root_run_id` | non-empty string | Root run for the whole task tree. |
| `parent_run_id` | non-empty string | Immediate parent run identity. |
| `parent_task_id` | non-empty string | Immediate parent task or work-item identity. |
| `depth` | positive integer | Nesting depth of this child in the task tree. |

Rules:

- `depth = 1` for a direct child of the root run.
- Each nested launch increments `depth` by exactly one from the immediate parent.

### 2.5. `state`

Allowed `state` values are:

- `queued`
- `running`
- `waiting_on_parent`
- `cancelling`
- `terminal`
- `closed`

Rules:

- `terminal` means execution stopped and a terminal result is available to the parent.
- `closed` means the managed boundary was released and further control messages must not change child behavior.

### 2.6. `reason_code`

Allowed machine-readable `reason_code` values are:

- `invalid_launch_request`
- `write_set_conflict`
- `missing_required_context`
- `budget_exhausted`
- `parent_cancelled`
- `unsupported_interaction_mode`
- `unsupported_nested_spawn`
- `invalid_control_message`
- `child_runtime_error`
- `invalid_child_return`
- `review_findings_raised`
- `closed_boundary`

Rules:

- `reason_code` is required whenever a request is rejected or a terminal outcome is not `accepted`.
- Implementations may map richer internal failures to these product-level codes, but they must not invent undeclared product-level codes.

### 2.7. `findings`

`findings` is an array of closed objects used only by reviewer-like children.

Each finding has these fields:

| Field | Type / constraints | Meaning |
| --- | --- | --- |
| `finding_id` | non-empty string | Stable identifier within the child result. |
| `severity` | `low`, `medium`, `high`, or `critical` | Relative impact of the finding. |
| `category` | `correctness`, `boundary`, `architecture`, `validation`, or `quality` | Product-level finding class. |
| `summary` | non-empty string | Parent-readable description of the issue. |
| `evidence_refs` | non-empty array of strings | References to files, resources, or returned artifacts that support the finding. |
| `recommended_action` | `fix`, `retest`, `replan`, or `investigate` | Suggested next step for the parent. |

Rules:

- Worker children must not emit `findings` unless they were launched in a reviewer-like role.
- Findings are advisory inputs to the parent. They are not self-executing commands.

## 3. Logical Methods

This contract uses four logical methods.

### 3.1. `subagent.launch`

Launch creates one managed child run.

Required request fields:

| Field | Type / constraints | Meaning |
| --- | --- | --- |
| `parent_run_id` | non-empty string | Stable parent run identity. |
| `parent_task_id` | non-empty string | Parent task or work-item identity. |
| `child_role` | `worker`, `reviewer`, or `final_review` | Managed child role. |
| `objective` | non-empty string | Delegated objective. |
| `write_set` | closed `write_set` object | Explicit allowed mutation surface. |
| `read_context` | closed `read_context` object | Explicit allowed context package. |
| `acceptance_criteria` | non-empty array of non-empty strings | What the parent expects to see on return. |
| `prohibitions` | array of non-empty strings | Explicit things the child must not do. |
| `required_validations` | non-empty array of closed validation objects | Validations the child must perform or report as unmet. |
| `dependency_or_sequencing_notes` | array of non-empty strings | Required order, prerequisites, or handoff constraints. |
| `interaction_policy` | literal `silent` | Parent-user-boundary interaction policy in the base model. |
| `budgets` | optional closed `budgets` object | Explicit limits for this child. |

Each validation object has:

- `validation_id`: non-empty string;
- `description`: non-empty string;
- `required`: boolean.

Launch response fields:

| Field | Type / constraints | Meaning |
| --- | --- | --- |
| `subagent_id` | non-empty string | Stable child identifier. |
| `run_id` | non-empty string | Runtime or execution run identifier. |
| `state` | `queued` or `running` | Initial managed child state. |
| `lineage` | closed `lineage` object | Parent/child linkage metadata. |

Launch must fail before the child starts if the scope is ambiguous, the write set overlaps a conflicting sibling, required context is unavailable, the requested interaction policy would expose child interaction through the parent user boundary, or the task package omits required validations.

### 3.2. `subagent.wait`

Wait observes one managed child until a terminal or timeout condition.

Request fields:

| Field | Type / constraints | Meaning |
| --- | --- | --- |
| `subagent_id` | non-empty string | The child to observe. |
| `wait_mode` | `terminal_only` or `terminal_or_update` | Whether the caller wants only terminal data or may also receive a non-terminal update. |
| `timeout_ms` | optional non-negative integer | Maximum wait time for this call. |

Wait response fields:

| Field | Type / constraints | Meaning |
| --- | --- | --- |
| `subagent_id` | non-empty string | Child identifier. |
| `state` | any allowed `state` value except `closed` for active waits | Current child state. |
| `update` | optional closed update object | Non-terminal parent-readable checkpoint when `wait_mode = terminal_or_update`. |
| `outcome` | optional allowed terminal outcome | Present when `state = terminal`. |
| `final_payload` | optional closed result payload object | Present when the child returned a terminal payload. |
| `findings` | optional `findings` array | Present only for reviewer-like children. |
| `reason_code` | optional allowed `reason_code` | Present when the terminal outcome is not `accepted`. |

The optional update object has:

- `update_kind`: `progress` or `needs_parent_input`;
- `summary`: non-empty string.

### 3.3. `subagent.send`

Send delivers bounded orchestration control to a live child.

Request fields:

| Field | Type / constraints | Meaning |
| --- | --- | --- |
| `subagent_id` | non-empty string | The child that receives the control message. |
| `message_id` | non-empty string | Parent-generated id for deduplication and audit. |
| `message_kind` | `clarify_scope`, `narrow_constraints`, `update_budget`, `request_status`, or `cancel` | Kind of allowed control message. |
| `payload` | closed object | Message payload constrained by `message_kind`. |

Payload rules by `message_kind`:

- `clarify_scope`: may contain `summary` and `references`, but must not widen `objective`, `write_set`, or `read_context`.
- `narrow_constraints`: may contain additional `prohibitions`, a narrower `write_set`, or a narrower `read_context`.
- `update_budget`: may contain a replacement `budgets` object, but it may only tighten or restate limits.
- `request_status`: payload must be the empty object.
- `cancel`: may contain an optional human-readable `reason`.

Send response fields:

| Field | Type / constraints | Meaning |
| --- | --- | --- |
| `subagent_id` | non-empty string | Child identifier. |
| `delivery_state` | `accepted`, `rejected`, or `ignored_terminal` | Whether the control message was applied. |
| `state` | any allowed `state` value | Current child state after message handling. |
| `reason_code` | optional allowed `reason_code` | Present when the message was rejected. |

`subagent.send` must not be used to open a user-visible chat channel, to pass hidden reasoning, or to expand the delegated scope.

### 3.4. `subagent.close`

Close releases the managed boundary after parent disposition is known.

Request fields:

| Field | Type / constraints | Meaning |
| --- | --- | --- |
| `subagent_id` | non-empty string | Child identifier. |
| `close_reason` | `accepted_by_parent`, `cancelled_by_parent`, or `abandoned_by_parent` | Parent disposition for this boundary. |

Close response fields:

| Field | Type / constraints | Meaning |
| --- | --- | --- |
| `subagent_id` | non-empty string | Child identifier. |
| `close_status` | `closing`, `closed`, `already_closed`, or `rejected` | Whether the boundary was released. |
| `state` | `cancelling`, `terminal`, or `closed` | Child state after close handling. |
| `outcome` | optional allowed terminal outcome | Present when a terminal result exists. |
| `reason_code` | optional allowed `reason_code` | Present when close is rejected or the terminal outcome is not `accepted`. |

Rules:

- `accepted_by_parent` is valid only when the child is already `terminal` with `outcome = accepted`; success moves the child to `closed`.
- `cancelled_by_parent` is valid only while the child is non-terminal; the response may be `closing` with `state = cancelling`, and the parent must still `wait` for terminal cancellation before the boundary can become `closed`.
- `abandoned_by_parent` is valid only when the child is already `terminal` and the parent is not accepting the result; success moves the child to `closed`.
- `close` is idempotent after `state = closed`.

## 4. Terminal Result Shape

The parent may only rely on normalized child result data.

A terminal child result is a closed object with these fields:

| Field | Type / constraints | Meaning |
| --- | --- | --- |
| `subagent_id` | non-empty string | Child identifier. |
| `child_role` | `worker`, `reviewer`, or `final_review` | Role used for this child. |
| `lineage` | closed `lineage` object | Parent/child lineage metadata. |
| `state` | literal `terminal` | Terminal state marker. |
| `outcome` | `accepted`, `rejected`, `retryable`, `review_required`, `failed`, or `cancelled` | Normalized terminal outcome. |
| `final_payload` | optional closed result payload object | Declared terminal payload. |
| `findings` | optional `findings` array | Reviewer-like findings. |
| `reason_code` | required when `outcome != accepted` | Machine-readable failure or rejection class. |

`final_payload`, when present, is a closed object with:

- `summary`: required non-empty string;
- `artifact_refs`: optional array of strings;
- `validation_results`: optional array of closed objects with `validation_id`, `status` (`passed`, `failed`, or `not_run`), and optional `note`.

Rules:

- `final_payload` is required when `outcome = accepted`.
- `findings` is required when `outcome = review_required`.
- The contract does not expose hidden traces, tool transcripts, internal reasoning, or user-facing chat history as part of the child result.

## 5. Boundary Rules

- Child runs are interaction-silent at the parent user boundary.
- A managed child may receive parent-originated control messages, but it does not gain a user conversation channel by default.
- A child does not inherit the parent's whole working set.
- A child does not inherit write authority outside the declared `write_set`.
- A child does not inherit hidden runtime or memory state unless another owner document explicitly allows that inheritance.
- A plain `orchestrator_agent` node does not implicitly satisfy this contract's managed task-package requirements.
- A managed child surface must reject any request that depends on unspecified vendor session behavior.

## 6. Invalid Cases

The orchestrator must reject these as contract violations:

- missing `parent_run_id` or `parent_task_id` on launch;
- an empty or ambiguous `write_set`;
- a `child_role` value outside the documented set;
- a `read_context` item with no `context_ref`;
- a validation object with missing `validation_id` or `description`;
- an `interaction_policy` other than `silent` in the base model;
- launch requests that try to widen scope through free-form prompt text instead of owned fields;
- send requests that attempt to simulate user chat or hidden reasoning exchange;
- `clarify_scope` messages that widen `objective`, `write_set`, or `read_context`;
- `update_budget` messages that loosen limits;
- `accepted_by_parent` close requests before a terminal accepted result exists;
- `abandoned_by_parent` close requests before the child is terminal;
- results that omit required `state`, `outcome`, or `lineage`;
- accepted results without `final_payload.summary`;
- reviewer-like findings emitted by a non-reviewer child;
- child results that bypass the declared boundary with raw traces or transcript data;
- launch or send requests that would make child interaction visible to the parent user boundary in the base model.

## 7. Cross-Links

- The architecture model lives in [Subagent Orchestration Model](../02-architecture/subagent-orchestration-model.md).
- Graph recursion semantics for `orchestrator_agent` live in [Graph Execution](../04-execution/graph-execution.md).
- Child task sequencing lives in [Subagent Task Lifecycle](../04-execution/subagent-task-lifecycle.md).
- Child lineage and persistence live in [Subagent Context and Memory](../05-state/subagent-context-and-memory.md).
- Builder behavior that consumes this surface lives in [Builder Agent](../08-extensions/builder-agent.md).

<a id="russian"></a>
# Русский

# Контракт MCP для Subagent

Статус: нормативный владелец managed child-run MCP surface.

Владеет: семантикой `launch`, `wait`, `send` и `close` для управляемой subagent-работы; правилами bounded capability; собственными value-objects и enum-ами для managed task package; нормализованной формой результата; и невалидными boundary-cases.

Не владеет: общей моделью оркестрации, графовыми контрактами нод, execution sequencing, layout сохраняемого состояния или внутренностями runtime-вендора.

Связанные документы:

- [Модель оркестрации subagent](../02-architecture/subagent-orchestration-model.md)
- [Contracts README](./README.md)
- [Жизненный цикл subagent-задач](../04-execution/subagent-task-lifecycle.md)
- [Состояние и memory subagent](../05-state/subagent-context-and-memory.md)
- [Исполнение графа](../04-execution/graph-execution.md)
- [Builder Agent](../08-extensions/builder-agent.md)
- [Модель интеграции runtime](../02-architecture/runtime-integration-model.md)

Этот документ определяет product-level MCP surface, который Core использует для управления bounded child run.
Он остается vendor-neutral. Он определяет logical methods, собственные value-objects и обязательства payload, но не transport framing, имена SDK или проприетарные session objects.

Базовая модель является interaction-silent на parent user boundary.
Child run может обмениваться внутренними control messages только со своим непосредственным parent orchestrator, но не должен surface-ить live comments или user chat через parent run.

## 1. Managed Child Run

Managed child run не является generic tool invocation.

Это Core-owned orchestration object со следующими характеристиками:

- ссылка на parent run;
- ссылка на parent task;
- child role;
- scoped objective;
- явный write set;
- явный package read-context;
- budget и depth limits;
- явная control surface во время выполнения;
- объявленная terminal return surface.

Managed child run и нода `orchestrator_agent` принадлежат к одному семейству child-run boundaries:

- обе создают parent/child lineage boundary;
- обе являются interaction-silent на parent user boundary в базовой модели;
- обе возвращают через границу только нормализованные terminal data.

Но это не одна и та же product surface:

- `orchestrator_agent` - переносимый graph child-run primitive. Он выбирает child agent по `agent_ref`, отправляет разрешенный node input и получает нормализованный terminal return child run-а.
- Managed subagent MCP surface - более богатая Core-owned delegation surface. Она добавляет task-package semantics вроде `child_role`, `write_set`, `read_context`, `budgets`, reviewer findings и явного control/close behavior.
- Реализация может порождать managed subagent через тот же базовый child-run machinery, который используется у `orchestrator_agent`, но это внутреннее отображение. Вызывающие стороны не должны выводить managed task-package fields из обычной ноды `orchestrator_agent`, а graph execution не должен выводить graph-node fields из этого MCP-контракта.

## 2. Owned Value Objects

Этот контракт владеет смыслом следующих объектов и enum-ов.

### 2.1. `write_set`

`write_set` - это закрытый объект со следующими полями:

| Поле | Тип / ограничения | Смысл |
| --- | --- | --- |
| `mode` | literal `allow_list` | Child может менять только перечисленные targets. |
| `items` | непустой массив закрытых объектов `write_target` | Явно разрешенные writable resources. |

Каждый `write_target` имеет следующие поля:

| Поле | Тип / ограничения | Смысл |
| --- | --- | --- |
| `resource_kind` | `file`, `directory`, `document`, `structured_record` или `generic_resource` | Product-level тип изменяемого ресурса. |
| `resource_ref` | непустая строка | Стабильный идентификатор ресурса, понятный parent-у. |
| `scope` | `exact` или `descendants` | Применяется ли право только к названному ресурсу или также к потомкам. |
| `access` | `modify_existing`, `create_within`, `create_or_modify` или `delete` | Разрешенный класс изменений для этого target-а. |

Правила:

- `items` не должен содержать дубликатов.
- Свободный текст, неоднозначные wildcard-ы и vendor-specific handles не являются допустимой заменой структурированным target-ам.
- Конфликт sibling-ов существует, если у двух живых children есть пересекающееся writable authority над одним и тем же ресурсом или над одной ancestor/descendant chain.

### 2.2. `read_context`

`read_context` - это закрытый объект со следующими полями:

| Поле | Тип / ограничения | Смысл |
| --- | --- | --- |
| `mode` | `explicit_only` или `explicit_plus_dependencies` | Получает ли child только перечисленные items или перечисленные items плюс owner-approved dependencies этих items. |
| `items` | массив закрытых объектов `read_context_item` | Явный пакет readable context. |

Каждый `read_context_item` имеет следующие поля:

| Поле | Тип / ограничения | Смысл |
| --- | --- | --- |
| `context_kind` | `file`, `directory_snapshot`, `document`, `prior_result`, `policy`, `summary` или `structured_state` | Product-level тип readable context. |
| `context_ref` | непустая строка | Стабильная ссылка, понятная parent-у и runtime. |
| `inclusion` | `full`, `excerpt`, `summary` или `reference_only` | Какой объем контекста реально передается. |
| `required` | boolean | Должен ли launch завершиться ошибкой, если item недоступен. |

Правила:

- Child не наследует неописанный parent context вне этого объекта.
- Если item с `required = true` недоступен в момент launch, launch должен завершиться ошибкой до старта child-а.

### 2.3. `budgets`

`budgets` - это закрытый объект. Каждое присутствующее поле должно быть положительным целым числом.

Допустимые поля:

- `max_steps`
- `max_tool_calls`
- `max_wall_clock_seconds`
- `max_spawn_depth`
- `max_children`
- `max_review_loops`

Правила:

- Отсутствующее поле означает "использовать более строгий лимит parent-а или Core policy".
- `null`, отрицательные значения и ноль невалидны.
- Бюджет child-а никогда не должен расширять более строгий лимит parent task tree.

### 2.4. `lineage`

`lineage` - это закрытый объект со следующими полями:

| Поле | Тип / ограничения | Смысл |
| --- | --- | --- |
| `root_run_id` | непустая строка | Root run для всего task tree. |
| `parent_run_id` | непустая строка | Идентичность непосредственного parent run-а. |
| `parent_task_id` | непустая строка | Идентичность непосредственного parent task или work-item. |
| `depth` | положительное целое число | Глубина вложенности этого child-а в task tree. |

Правила:

- `depth = 1` для прямого child-а root run-а.
- Каждый nested launch увеличивает `depth` ровно на единицу относительно непосредственного parent-а.

### 2.5. `state`

Допустимые значения `state`:

- `queued`
- `running`
- `waiting_on_parent`
- `cancelling`
- `terminal`
- `closed`

Правила:

- `terminal` означает, что выполнение остановлено и terminal result доступен parent-у.
- `closed` означает, что managed boundary освобождена и дальнейшие control messages не должны менять поведение child-а.

### 2.6. `reason_code`

Допустимые machine-readable значения `reason_code`:

- `invalid_launch_request`
- `write_set_conflict`
- `missing_required_context`
- `budget_exhausted`
- `parent_cancelled`
- `unsupported_interaction_mode`
- `unsupported_nested_spawn`
- `invalid_control_message`
- `child_runtime_error`
- `invalid_child_return`
- `review_findings_raised`
- `closed_boundary`

Правила:

- `reason_code` обязателен всякий раз, когда запрос отклонен или terminal outcome не равен `accepted`.
- Реализации могут отображать более богатые внутренние ошибки в эти product-level коды, но не должны изобретать неописанные product-level коды.

### 2.7. `findings`

`findings` - это массив закрытых объектов, который используется только reviewer-like children.

Каждый finding имеет следующие поля:

| Поле | Тип / ограничения | Смысл |
| --- | --- | --- |
| `finding_id` | непустая строка | Стабильный идентификатор внутри child result. |
| `severity` | `low`, `medium`, `high` или `critical` | Относительная серьезность finding-а. |
| `category` | `correctness`, `boundary`, `architecture`, `validation` или `quality` | Product-level класс finding-а. |
| `summary` | непустая строка | Описание проблемы, понятное parent-у. |
| `evidence_refs` | непустой массив строк | Ссылки на files, resources или returned artifacts, подтверждающие finding. |
| `recommended_action` | `fix`, `retest`, `replan` или `investigate` | Предлагаемый следующий шаг для parent-а. |

Правила:

- Worker-child не должен возвращать `findings`, если он не был запущен в reviewer-like role.
- Findings являются advisory inputs для parent-а. Это не self-executing commands.

## 3. Logical Methods

Этот контракт использует четыре logical methods.

### 3.1. `subagent.launch`

Launch создает один managed child run.

Обязательные поля request:

| Поле | Тип / ограничения | Смысл |
| --- | --- | --- |
| `parent_run_id` | непустая строка | Стабильная идентичность parent run-а. |
| `parent_task_id` | непустая строка | Идентичность parent task или work-item. |
| `child_role` | `worker`, `reviewer` или `final_review` | Роль managed child-а. |
| `objective` | непустая строка | Делегированная цель. |
| `write_set` | закрытый объект `write_set` | Явно разрешенная поверхность изменений. |
| `read_context` | закрытый объект `read_context` | Явный разрешенный пакет контекста. |
| `acceptance_criteria` | непустой массив непустых строк | Что parent ожидает увидеть на возврате. |
| `prohibitions` | массив непустых строк | Явные действия, которые child не должен делать. |
| `required_validations` | непустой массив закрытых validation objects | Проверки, которые child должен выполнить или явно пометить как невыполненные. |
| `dependency_or_sequencing_notes` | массив непустых строк | Обязательный порядок, prerequisites или handoff constraints. |
| `interaction_policy` | literal `silent` | Политика взаимодействия на границе parent-user в базовой модели. |
| `budgets` | необязательный закрытый объект `budgets` | Явные лимиты для этого child-а. |

Каждый validation object имеет:

- `validation_id`: непустая строка;
- `description`: непустая строка;
- `required`: boolean.

Поля response на launch:

| Поле | Тип / ограничения | Смысл |
| --- | --- | --- |
| `subagent_id` | непустая строка | Стабильный идентификатор child-а. |
| `run_id` | непустая строка | Идентификатор runtime или execution run-а. |
| `state` | `queued` или `running` | Начальное состояние managed child-а. |
| `lineage` | закрытый объект `lineage` | Metadata связи parent/child. |

Launch должен завершиться ошибкой до старта child-а, если scope неоднозначен, write set пересекается с конфликтующим sibling-ом, обязательный context недоступен, запрошенная interaction policy сделала бы child interaction видимой через parent user boundary или task package не содержит required validations.

### 3.2. `subagent.wait`

Wait наблюдает один managed child до terminal или timeout condition.

Поля request:

| Поле | Тип / ограничения | Смысл |
| --- | --- | --- |
| `subagent_id` | непустая строка | Child, за которым нужно наблюдать. |
| `wait_mode` | `terminal_only` или `terminal_or_update` | Нужны ли вызывающей стороне только terminal data или также допустим non-terminal update. |
| `timeout_ms` | необязательное неотрицательное целое число | Максимальное время ожидания для этого вызова. |

Поля response на wait:

| Поле | Тип / ограничения | Смысл |
| --- | --- | --- |
| `subagent_id` | непустая строка | Идентификатор child-а. |
| `state` | любое допустимое значение `state`, кроме `closed` для активного ожидания | Текущее состояние child-а. |
| `update` | необязательный закрытый update object | Non-terminal checkpoint, понятный parent-у, когда `wait_mode = terminal_or_update`. |
| `outcome` | необязательный допустимый terminal outcome | Присутствует, когда `state = terminal`. |
| `final_payload` | необязательный закрытый объект result payload | Присутствует, когда child вернул terminal payload. |
| `findings` | необязательный массив `findings` | Присутствует только для reviewer-like children. |
| `reason_code` | необязательный допустимый `reason_code` | Присутствует, когда terminal outcome не равен `accepted`. |

Необязательный update object имеет:

- `update_kind`: `progress` или `needs_parent_input`;
- `summary`: непустая строка.

### 3.3. `subagent.send`

Send передает bounded orchestration control живому child-у.

Поля request:

| Поле | Тип / ограничения | Смысл |
| --- | --- | --- |
| `subagent_id` | непустая строка | Child, который получает control message. |
| `message_id` | непустая строка | Parent-generated id для deduplication и audit. |
| `message_kind` | `clarify_scope`, `narrow_constraints`, `update_budget`, `request_status` или `cancel` | Вид допустимого control message. |
| `payload` | закрытый объект | Payload сообщения, ограниченный `message_kind`. |

Правила payload по `message_kind`:

- `clarify_scope`: может содержать `summary` и `references`, но не должен расширять `objective`, `write_set` или `read_context`.
- `narrow_constraints`: может содержать дополнительные `prohibitions`, более узкий `write_set` или более узкий `read_context`.
- `update_budget`: может содержать новый объект `budgets`, но он может только ужесточать лимиты или повторять текущие значения.
- `request_status`: payload обязан быть пустым объектом.
- `cancel`: может содержать необязательный human-readable `reason`.

Поля response на send:

| Поле | Тип / ограничения | Смысл |
| --- | --- | --- |
| `subagent_id` | непустая строка | Идентификатор child-а. |
| `delivery_state` | `accepted`, `rejected` или `ignored_terminal` | Было ли control message применено. |
| `state` | любое допустимое значение `state` | Текущее состояние child-а после обработки сообщения. |
| `reason_code` | необязательный допустимый `reason_code` | Присутствует, когда сообщение отклонено. |

`subagent.send` нельзя использовать для открытия user-visible chat channel, передачи скрытого reasoning или расширения делегированного scope.

### 3.4. `subagent.close`

Close освобождает managed boundary после того, как parent определил disposition.

Поля request:

| Поле | Тип / ограничения | Смысл |
| --- | --- | --- |
| `subagent_id` | непустая строка | Идентификатор child-а. |
| `close_reason` | `accepted_by_parent`, `cancelled_by_parent` или `abandoned_by_parent` | Parent disposition для этой границы. |

Поля response на close:

| Поле | Тип / ограничения | Смысл |
| --- | --- | --- |
| `subagent_id` | непустая строка | Идентификатор child-а. |
| `close_status` | `closing`, `closed`, `already_closed` или `rejected` | Была ли boundary освобождена. |
| `state` | `cancelling`, `terminal` или `closed` | Состояние child-а после обработки close. |
| `outcome` | необязательный допустимый terminal outcome | Присутствует, когда существует terminal result. |
| `reason_code` | необязательный допустимый `reason_code` | Присутствует, когда close отклонен или terminal outcome не равен `accepted`. |

Правила:

- `accepted_by_parent` валиден только когда child уже находится в `terminal` с `outcome = accepted`; при успехе child переходит в `closed`.
- `cancelled_by_parent` валиден только пока child не достиг terminal state; response может быть `closing` с `state = cancelling`, и parent все равно должен вызвать `wait` до terminal cancellation, прежде чем boundary сможет стать `closed`.
- `abandoned_by_parent` валиден только когда child уже находится в `terminal` и parent не принимает результат; при успехе child переходит в `closed`.
- `close` является idempotent после `state = closed`.

## 4. Форма Terminal Result

Parent может полагаться только на normalized child result data.

Terminal child result - это закрытый объект со следующими полями:

| Поле | Тип / ограничения | Смысл |
| --- | --- | --- |
| `subagent_id` | непустая строка | Идентификатор child-а. |
| `child_role` | `worker`, `reviewer` или `final_review` | Роль, использованная для этого child-а. |
| `lineage` | закрытый объект `lineage` | Metadata parent/child lineage. |
| `state` | literal `terminal` | Маркер terminal state. |
| `outcome` | `accepted`, `rejected`, `retryable`, `review_required`, `failed` или `cancelled` | Нормализованный terminal outcome. |
| `final_payload` | необязательный закрытый объект result payload | Объявленный terminal payload. |
| `findings` | необязательный массив `findings` | Reviewer-like findings. |
| `reason_code` | обязателен, когда `outcome != accepted` | Machine-readable класс failure или rejection. |

`final_payload`, когда он присутствует, - это закрытый объект со следующими полями:

- `summary`: обязательная непустая строка;
- `artifact_refs`: необязательный массив строк;
- `validation_results`: необязательный массив закрытых объектов с `validation_id`, `status` (`passed`, `failed` или `not_run`) и необязательным `note`.

Правила:

- `final_payload` обязателен, когда `outcome = accepted`.
- `findings` обязателен, когда `outcome = review_required`.
- Контракт не раскрывает hidden traces, tool transcripts, internal reasoning или user-facing chat history как часть child result.

## 5. Boundary Rules

- Child runs остаются interaction-silent на parent user boundary.
- Managed child может получать parent-originated control messages, но по умолчанию не получает user conversation channel.
- Child не наследует весь working set parent-а.
- Child не наследует write authority вне объявленного `write_set`.
- Child не наследует hidden runtime или memory state, если другой owner document явно не разрешает такое наследование.
- Обычная нода `orchestrator_agent` не удовлетворяет автоматически требованиям этого контракта к managed task package.
- Managed child surface должен отклонять любой запрос, который зависит от неописанного vendor session behavior.

## 6. Невалидные Случаи

Оркестратор обязан отклонять следующие нарушения контракта:

- отсутствие `parent_run_id` или `parent_task_id` на launch;
- пустой или неоднозначный `write_set`;
- значение `child_role` вне документированного набора;
- item `read_context` без `context_ref`;
- validation object без `validation_id` или `description`;
- `interaction_policy`, отличный от `silent`, в базовой модели;
- launch requests, которые пытаются расширить scope через free-form prompt text вместо собственных полей контракта;
- send requests, которые пытаются имитировать user chat или обмен скрытым reasoning;
- сообщения `clarify_scope`, расширяющие `objective`, `write_set` или `read_context`;
- сообщения `update_budget`, ослабляющие лимиты;
- close requests `accepted_by_parent` до появления terminal accepted result;
- close requests `abandoned_by_parent` до того, как child стал terminal;
- results, в которых отсутствуют обязательные `state`, `outcome` или `lineage`;
- accepted results без `final_payload.summary`;
- reviewer-like findings, возвращенные non-reviewer child-ом;
- child results, обходящие объявленную boundary через raw traces или transcript data;
- launch или send requests, которые сделали бы child interaction видимой для parent user boundary в базовой модели.

## 7. Перекрестные Ссылки

- Архитектурная модель находится в [Модели оркестрации subagent](../02-architecture/subagent-orchestration-model.md).
- Семантика graph recursion для `orchestrator_agent` находится в [Исполнении графа](../04-execution/graph-execution.md).
- Sequencing child task находится в [Жизненном цикле subagent-задач](../04-execution/subagent-task-lifecycle.md).
- Lineage child-а и persistence находятся в [Состоянии и memory subagent](../05-state/subagent-context-and-memory.md).
- Builder behavior, который использует эту surface, находится в [Builder Agent](../08-extensions/builder-agent.md).
