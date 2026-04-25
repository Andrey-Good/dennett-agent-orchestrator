[English](#english) | [Russian](#russian)

<a id="english"></a>
# Cross-Subsystem Conflict Rules

Status: Phase 18 integration rules for conflicts between existing subsystem contracts.

## Rule Model

These rules do not redefine subsystem behavior. They define routing and precedence when an integrated flow touches multiple owners.

When a rule detects a conflict, the product should fail closed, preserve durable state that already belongs to completed steps, and return an error that names the owning subsystem and violated contract.

## Precedence Rules

1. Schema and portable contract validation runs before lifecycle persistence, deploy, runtime selection, memory attachment, interaction waits, or subagent launch.
2. Lifecycle owns draft/live/deploy identity. Builder output is only a candidate until lifecycle accepts and persists it.
3. Capability gates run before execution. Runtime capability checks own runtime options; memory capability checks own provider-backed memory behavior.
4. Interaction wait state owns user-visible blocking. Runtime, memory, and subagent work must not silently bypass a blocked prompt or unresolved required reply.
5. Managed subagent ownership rules run before sibling work starts. Write-scope conflicts, budget caps, cancellation, and close semantics remain owned by the managed subagent surface.
6. Durable state owners win over transient UI or adapter observations. If a UI, adapter, or child result disagrees with durable state, the flow must reconcile through the durable owner.

## Builder Conflicts

Builder output conflicts with other subsystems when it:

- introduces fields outside the public portable contract;
- embeds provider registration, credentials, runtime-local metadata, or managed-subagent task state inside portable Agent JSON;
- attempts to deploy, launch, or mark live as a hidden side effect of authoring;
- treats self-review or confidence as equivalent to schema, lifecycle, or acceptance evidence.

Resolution: reject or revise the draft through builder and validation feedback. Do not let builder-specific state override lifecycle, runtime, memory, interaction, or managed-subagent owners.

## Lifecycle Conflicts

Lifecycle conflicts exist when a flow:

- runs a draft as if it were live without the documented lifecycle path;
- changes live revision identity during an active run without an owner-approved transition;
- deploys output that failed schema, owner-doc, or capability validation;
- lets runtime or builder metadata become the source of truth for revision identity.

Resolution: lifecycle blocks persistence or deploy until the owning validation and identity rules pass.

## Runtime Conflicts

Runtime conflicts exist when a flow:

- requests `reasoning_effort`, `speed_tier`, model metadata, or account behavior that the selected runtime does not advertise;
- treats runtime-local discovery as portable agent-file truth;
- assumes runtime-native behavior for memory, interaction, or subagents that is not declared by the runtime adapter contract;
- changes runtime configuration while an interaction owner has a blocked prompt whose policy forbids that change.

Resolution: runtime selection must downgrade only through documented fallback policy, ask the user where required, or fail with a runtime-owned capability error.

## Interaction Conflicts

Interaction conflicts exist when a flow:

- receives a user reply after the associated prompt is closed, cancelled, or superseded;
- attempts to resume execution without the required durable reply;
- lets child-agent interaction appear directly on the parent boundary without an owner-defined routing rule;
- changes risky parameters mid-run without the interaction policy allowing it.

Resolution: interaction state decides whether the message is accepted, rejected, queued, or routed to a specific child boundary.

## Memory Conflicts

Memory conflicts exist when a flow:

- treats a portable `memory_bindings` entry as proof that a provider is registered and ready;
- writes memory without provider capability negotiation;
- reads memory from a provider or scope that the binding does not authorize;
- mixes runtime-native memory assumptions with provider-backed memory evidence.

Resolution: memory binding validation and provider capability negotiation decide whether the flow continues. Missing provider readiness is an integration failure for Phase 18 unless the scenario explicitly uses a documented fake provider.

## Managed Subagent Conflicts

Managed subagent conflicts exist when a flow:

- launches siblings with overlapping write scopes;
- exceeds persisted budgets or ignores cancellation;
- treats a child terminal result as closed before the parent explicitly closes the boundary;
- serializes managed task packages, findings, or review-loop state into portable Agent JSON;
- lets builder or runtime logic weaken reviewer/fix-loop requirements.

Resolution: the managed subagent service rejects the launch or control action, records the conflict where durable state exists, and keeps review/fix-loop semantics outside portable Agent JSON.

## Combined Failure Handling

When multiple conflicts appear, report the earliest violated gate in this order:

1. schema and portable contract;
2. lifecycle identity and deploy;
3. capability gates for runtime and memory;
4. interaction wait-state policy;
5. managed subagent write-scope, budget, cancellation, and close policy;
6. adapter execution result.

This order is for integrated-flow diagnostics only. It does not change the subsystem contracts themselves.

<a id="russian"></a>
# Правила конфликтов между подсистемами

Статус: интеграционные правила Phase 18 для конфликтов между существующими контрактами подсистем.

## Модель правил

Эти правила не переопределяют поведение подсистем. Они определяют маршрутизацию и приоритеты, когда интегрированный поток затрагивает нескольких владельцев.

Когда правило обнаруживает конфликт, продукт должен fail closed, сохранить durable state, которая уже принадлежит завершенным шагам, и вернуть ошибку, называющую owning subsystem и нарушенный contract.

## Правила приоритета

1. Schema and portable contract validation выполняется до lifecycle persistence, deploy, runtime selection, memory attachment, interaction waits или subagent launch.
2. Lifecycle владеет draft/live/deploy identity. Builder output остается только candidate, пока lifecycle не примет и не сохранит его.
3. Capability gates выполняются до execution. Runtime capability checks владеют runtime options; memory capability checks владеют provider-backed memory behavior.
4. Interaction wait state владеет user-visible blocking. Runtime, memory и subagent work не должны молча обходить blocked prompt или unresolved required reply.
5. Правила владения managed subagent выполняются до начала sibling work. Write-scope conflicts, budget caps, cancellation и close semantics остаются во владении managed subagent surface.
6. Владельцы durable state имеют приоритет над transient UI или adapter observations. Если UI, adapter или child result расходится с durable state, поток должен согласоваться через durable owner.

## Конфликты Builder

Builder output конфликтует с другими подсистемами, когда он:

- вводит поля вне public portable contract;
- встраивает provider registration, credentials, runtime-local metadata или managed-subagent task state внутрь portable Agent JSON;
- пытается выполнить deploy, launch или mark live как скрытый side effect authoring;
- считает self-review или confidence эквивалентом schema, lifecycle или acceptance evidence.

Разрешение: отклонить или пересмотреть draft через builder and validation feedback. Не позволяйте builder-specific state переопределять владельцев lifecycle, runtime, memory, interaction или managed-subagent.

## Конфликты Lifecycle

Lifecycle conflicts существуют, когда поток:

- запускает draft как live без документированного lifecycle path;
- меняет live revision identity во время active run без owner-approved transition;
- deploys output, который не прошел schema, owner-doc или capability validation;
- позволяет runtime или builder metadata стать source of truth для revision identity.

Разрешение: lifecycle блокирует persistence или deploy, пока owning validation и identity rules не пройдут.

## Конфликты Runtime

Runtime conflicts существуют, когда поток:

- запрашивает `reasoning_effort`, `speed_tier`, model metadata или account behavior, которые выбранный runtime не объявляет;
- считает runtime-local discovery переносимой agent-file truth;
- предполагает runtime-native behavior для memory, interaction или subagents, который не объявлен runtime adapter contract;
- меняет runtime configuration, пока у interaction owner есть blocked prompt, политика которого запрещает такое изменение.

Разрешение: runtime selection должен выполнять downgrade только через documented fallback policy, спрашивать пользователя там, где это требуется, или завершаться runtime-owned capability error.

## Конфликты Interaction

Interaction conflicts существуют, когда поток:

- получает user reply после того, как связанный prompt closed, cancelled или superseded;
- пытается resume execution без required durable reply;
- позволяет child-agent interaction появиться напрямую на parent boundary без owner-defined routing rule;
- меняет risky parameters mid-run без разрешения interaction policy.

Разрешение: interaction state решает, будет ли message accepted, rejected, queued или routed к specific child boundary.

## Конфликты Memory

Memory conflicts существуют, когда поток:

- считает portable `memory_bindings` entry доказательством того, что provider registered and ready;
- пишет memory без provider capability negotiation;
- читает memory из provider или scope, который binding не авторизует;
- смешивает runtime-native memory assumptions с provider-backed memory evidence.

Разрешение: memory binding validation и provider capability negotiation решают, продолжается ли поток. Отсутствие provider readiness является integration failure для Phase 18, если сценарий явно не использует documented fake provider.

## Конфликты Managed Subagent

Managed subagent conflicts существуют, когда поток:

- запускает siblings с overlapping write scopes;
- превышает persisted budgets или игнорирует cancellation;
- считает child terminal result closed до того, как parent явно close boundary;
- сериализует managed task packages, findings или review-loop state в portable Agent JSON;
- позволяет builder или runtime logic ослаблять reviewer/fix-loop requirements.

Разрешение: managed subagent service отклоняет launch или control action, записывает conflict там, где существует durable state, и держит review/fix-loop semantics вне portable Agent JSON.

## Обработка комбинированных ошибок

Когда появляется несколько conflicts, сообщайте самый ранний нарушенный gate в таком порядке:

1. schema and portable contract;
2. lifecycle identity and deploy;
3. capability gates for runtime and memory;
4. interaction wait-state policy;
5. managed subagent write-scope, budget, cancellation, and close policy;
6. adapter execution result.

Этот порядок нужен только для диагностики integrated-flow. Он не изменяет сами subsystem contracts.
