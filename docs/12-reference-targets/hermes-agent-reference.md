[English](#english) | [Русский](#russian)

<a id="english"></a>
# English

## Hermes Agent Reference

Status: non-normative reference target for Dennett implementation work.

Related documents:

- [Documentation Map](../README.md)
- [Reference Targets](./README.md)
- [Canonical spec](../../agent_orchestrator_final_spec_v2.md)
- [Subagent orchestration model](../02-architecture/subagent-orchestration-model.md)
- [State](../05-state/README.md)
- [Memory bindings](../08-extensions/memory-bindings.md)
- [Runtime sources](../08-extensions/runtime-sources.md)

Hermes is a reference target, not a canonical behavior owner for Dennett. Dennett canon remains authoritative; this document exists so future Dennett work can reproduce Hermes behavior intentionally instead of importing it accidentally into the core contract.

The material below is based on the local Hermes research clone and its docs/code, especially `run_agent.py`, `agent/prompt_builder.py`, `agent/context_compressor.py`, `agent/prompt_caching.py`, `hermes_state.py`, `toolsets.py`, `model_tools.py`, `tools/delegate_tool.py`, `tools/mcp_tool.py`, `hermes_cli/plugins.py`, and the Hermes docs under `website/docs/developer-guide/**`.

### Reproduction Map

Hermes behavior falls into three buckets for Dennett:

- Already visible in the current Dennett code paths: explicit child-run boundaries, deterministic graph-style control flow, separated state ownership, memory as a distinct binding concept, runtime-source narrowing, and approval-aware execution boundaries.
- Needs additional Dennett work: Hermes-style stable prompt prefix caching, Hermes-specific session lineage/compression behavior, the richer plugin/backend loading model, the exact memory-manager/provider split, and the full tool surface composition story.
- Should stay shell/UI/platform-specific: CLI spinners, gateway message routing, platform adapters, prompt cosmetics, and any UX behavior that exists only because Hermes ships as a CLI/gateway product.

### 1. High-Level Architecture

Hermes centers on one `AIAgent` implementation that is reused across CLI, gateway, ACP, batch, and API entry points. The entry-point difference lives around the agent core, not instead of it.

The key moving parts are:

- `run_agent.py` for the conversation loop and subagent delegation;
- `agent/prompt_builder.py` for stable prompt construction;
- `agent/context_compressor.py` and `agent/prompt_caching.py` for context pressure management;
- `model_tools.py` and `toolsets.py` for registry-driven tool discovery and grouping;
- `hermes_state.py` for SQLite session persistence and lineage;
- `agent/memory_manager.py` and `agent/memory_provider.py` for durable memory orchestration;
- `tools/mcp_tool.py`, `tools/delegate_tool.py`, and `tools/approval.py` for runtime-bound capabilities;
- `hermes_cli/plugins.py` for plugin discovery, loading, and hook registration.

The architectural implication for Dennett is simple: Hermes is not a single monolithic prompt loop. It is a core agent plus a set of registries and policy boundaries around it.

### 2. Control Loop

Hermes' control loop is closer to a stable agent kernel than to a one-off command runner:

1. Resolve provider/runtime.
2. Build or reuse the cached system prompt.
3. Load session state and apply compression if needed.
4. Call the model.
5. Dispatch tool calls through the registry.
6. Persist messages, session state, memory, and lineage.
7. Repeat until a terminal answer or a bounded failure occurs.

Two details matter for reproduction:

- The prompt is treated as a stable prefix whenever possible; Hermes tries to avoid rebuilding or mutating the durable prompt state on every turn.
- Tool execution is not purely serial in current code, but the concurrency gate is narrower than the broad human-facing documentation suggests. Interactive tools are forced sequentially, and only a small safe subset is eligible for parallel dispatch.

### 3. Prompt Assembly

Hermes separates cached prompt content from turn-specific additions.

The stable prompt generally combines:

- agent identity;
- tool-behavior guidance;
- memory snapshot content;
- user profile content;
- skills index;
- project context files such as `AGENTS.md`, `.hermes.md`, or compatibility files;
- platform hints and session metadata.

The important implementation idea is not the exact order alone. It is that Hermes tries to keep a stable cached prefix and inject volatile additions outside that prefix when possible.

`agent/prompt_caching.py` is part of that design. Hermes uses cache breakpoints so the prompt prefix can be reused across turns, especially on Anthropic-backed paths.

`skip_context_files` is also relevant: subagent and other isolated flows can intentionally avoid pulling in the full local context-file layer, which keeps the child prompt narrower than the parent prompt.

### 4. Sessions, Storage, and Compression

Hermes persists sessions in SQLite and uses lineage rather than flat overwrite semantics. Compression can produce a child session lineage entry, not just a shortened message list.

The session model is materially tied to:

- full message history;
- session metadata;
- FTS/search support;
- parent/child lineage across compression or session splits;
- durable turn accounting and provider metadata.

In the current local Hermes clone, `hermes_state.py` reports `SCHEMA_VERSION = 8`. Treat that as a snapshot from the clone, not a permanent Hermes-wide fact, and reconcile any older doc text against the code before relying on it.

Compression is two-layered:

- gateway/session hygiene can compress proactively before the agent loop when sessions are already large;
- the agent-side compressor runs inside the loop on real token pressure.

The core behavior is lossy summarization of the middle of the conversation while protecting recent turns and preserving tool-call/tool-result pairing. That is a good structural target for Dennett, but the exact thresholds and helper heuristics are Hermes-specific implementation choices.

### 5. Memory

Hermes treats memory as distinct from chat history and resume state.

The implementation has a real memory-manager layer and memory-provider abstraction rather than a single hard-coded memory file. That matters because memory is a runtime capability, not merely a persisted transcript.

Hermes also keeps memory scoping separate from session scoping. The effective memory visible to a run depends on the selected provider, session/user context, and the current execution boundary.

For Dennett, the key structural lesson is to model memory as an explicit capability binding with selection and isolation rules, not as hidden chat replay.

### 6. Skills, Plugins, MCP, And Toolsets

Hermes assembles its action surface from registries and layered discovery, not from a single fixed tool list.

The main pieces are:

- `toolsets.py` for grouped tool bundles and composite toolset resolution;
- `model_tools.py` and the tool registry for schema collection and dispatch;
- `tools/mcp_tool.py` for MCP client integration;
- `hermes_cli/plugins.py` for plugin loading and hook registration;
- bundled skills plus optional skills for prompt-side capability shaping.

Important nuance: Hermes plugin docs say plugins are opt-in, but the current local clone treats bundled backend plugins differently. In that snapshot, built-in backend plugins can auto-load, while user-installed and project plugins still follow allowlist gating. That distinction should be preserved in any reference model.

Another nuance: MCP is not just a documentation concept. It is part of the runtime action surface and can be registered dynamically through the same registry layer that serves built-in tools.

### 7. Delegation, Subagents, Approvals, And Runtime Isolation

Hermes supports delegation through a dedicated subagent path rather than by pretending every tool call is a child task.

`delegate_tool.py` and the `AIAgent` code track separate child execution, depth, and interruption state. In the current local clone, the child is launched with isolated context and its own execution budget logic. The exact inheritance rules are implementation-specific, so describe them cautiously: the observed code clearly tracks depth and child isolation, but the budget semantics should not be overstated as a universal contract.

Approvals and isolation are also first-class:

- dangerous command paths flow through approval logic;
- tool execution can be interrupted;
- worker threads and child runs have separate cancellation handling;
- environment backends keep runtime execution separated from the host process when needed.

This is one of the clearest places where Dennett should match structure, not UX. The shell-facing approval interaction belongs to the shell. The core boundary around dangerous operations and runtime isolation belongs to the graph/runtime design.

### 8. What A Hermes-Equivalent Dennett Graph Would Need

A Dennett graph that aims to reproduce Hermes needs more than a generic LLM node chain. Structurally, it would need:

- a stable prompt-assembly node that can preserve a cacheable prefix;
- a provider/runtime resolution node that selects the concrete backend before execution;
- a registry-backed tool-surface node that can expose built-in tools, plugin tools, MCP tools, and toolsets;
- a session node that carries lineage, storage, compression state, and replayable history;
- a memory binding node that selects a provider and applies scoped memory visibility;
- a delegation node that creates isolated child runs with explicit budgets and explicit return boundaries;
- an approval/isolation boundary for dangerous or sandboxed tool execution;
- a shell/UI layer above that graph for CLI, gateway, and platform-specific presentation.

If Dennett wants to reproduce Hermes behavior closely, these should be separate structural concerns, not one overloaded prompt function.

### 9. Caveats And Mismatches

Do not flatten the docs into a false certainty. The Hermes repository has a few visible doc/code mismatches or over-broad descriptions:

- session-storage documentation can lag the current local `hermes_state.py` schema snapshot;
- plugin docs present a simple opt-in story, but bundled backend plugins appear to auto-load in the current local code snapshot;
- tool concurrency docs are broader than the gate in code that actually decides whether a batch is parallel-safe;
- child budget and depth handling are visible in the current code snapshot, but the exact contract is not presented as a clean external invariant everywhere.

For Dennett, these are useful signals. They mean the reference target should be treated as "what the local code and docs together imply" for reconstructing Hermes behavior when Hermes docs are stale, while Dennett canon and owner docs remain authoritative for Dennett itself.

<a id="russian"></a>
# Русский

## Справка по Hermes Agent

Статус: ненормативный reference target для реализации Dennett.

Связанные документы:

- [Карта документации](../README.md)
- [Reference Targets](./README.md)
- [Каноническая спецификация](../../agent_orchestrator_final_spec_v2.md)
- [Модель оркестрации subagent](../02-architecture/subagent-orchestration-model.md)
- [Состояние](../05-state/README.md)
- [Memory bindings](../08-extensions/memory-bindings.md)
- [Runtime sources](../08-extensions/runtime-sources.md)

Hermes здесь выступает как reference target, а не как владелец канонического поведения Dennett. Канон Dennett остаётся главным; цель документа — помочь будущей реализации Dennett воспроизводить Hermes сознательно, а не переносить его поведение в core-contract по умолчанию.

Основа этого материала: локальный Hermes research clone и его docs/code, особенно `run_agent.py`, `agent/prompt_builder.py`, `agent/context_compressor.py`, `agent/prompt_caching.py`, `hermes_state.py`, `toolsets.py`, `model_tools.py`, `tools/delegate_tool.py`, `tools/mcp_tool.py`, `hermes_cli/plugins.py`, а также Hermes docs в `website/docs/developer-guide/**`.

### Карта Воспроизведения

Поведение Hermes для Dennett делится на три группы:

- Видно в текущих путях кода Dennett: явные child-run boundary, детерминированный graph-style control flow, отдельное владение состоянием, memory как отдельный binding, narrowing runtime-source и approval-aware boundaries.
- Требует дополнительной работы Dennett: Hermes-style stable prompt prefix caching, поведение lineage/compression для сессий, более богатая модель загрузки plugin/backend, точный split memory-manager/provider и полный story composition tool surface.
- Должно остаться shell/UI/platform-specific: CLI spinner, маршрутизация gateway-сообщений, platform adapters, оформление prompt и любой UX, который существует только потому, что Hermes поставляется как CLI/gateway-продукт.

### 1. Высокоуровневая Архитектура

Hermes построен вокруг одного `AIAgent`, который переиспользуется в CLI, gateway, ACP, batch и API entry points. Различие между entry point находится вокруг ядра агента, а не вместо него.

Ключевые части:

- `run_agent.py` для control loop и delegation subagent;
- `agent/prompt_builder.py` для стабильной сборки prompt;
- `agent/context_compressor.py` и `agent/prompt_caching.py` для управления контекстом;
- `model_tools.py` и `toolsets.py` для registry-driven discovery и группировки tools;
- `hermes_state.py` для SQLite-персистентности и lineage;
- `agent/memory_manager.py` и `agent/memory_provider.py` для durable memory orchestration;
- `tools/mcp_tool.py`, `tools/delegate_tool.py` и `tools/approval.py` для runtime-bound capabilities;
- `hermes_cli/plugins.py` для plugin discovery, loading и hook registration.

### 2. Control Loop

Hermes работает как стабильное agent kernel, а не как разовый command runner:

1. Разрешить provider/runtime.
2. Собрать или переиспользовать cached system prompt.
3. Загрузить session state и при необходимости выполнить compression.
4. Вызвать model.
5. Отправить tool calls через registry.
6. Сохранить messages, session state, memory и lineage.
7. Повторять до terminal answer или bounded failure.

Две детали особенно важны:

- prompt старается оставаться стабильным prefix;
- tool execution не всегда строго последовательный, но gate на concurrency уже, чем широкие текстовые описания в документации: interactive tools идут последовательно, а parallel dispatch разрешён только для небольшого safe subset.

### 3. Prompt Assembly

Hermes разделяет cached prompt content и turn-specific additions.

Обычно стабильный prompt включает:

- agent identity;
- tool behavior guidance;
- memory snapshot;
- user profile;
- skills index;
- project context files вроде `AGENTS.md`, `.hermes.md` и совместимых файлов;
- platform hints и session metadata.

Главная идея не в точном порядке, а в том, что Hermes старается держать стабильный cached prefix и добавлять volatile части вне него, когда это возможно.

`agent/prompt_caching.py` поддерживает это через cache breakpoints, особенно на Anthropic-backed paths.

`skip_context_files` тоже важно: isolated flows, включая subagent path, могут намеренно не подтягивать полный layer локального context-file окружения.

### 4. Sessions, Storage, Compression

Hermes хранит sessions в SQLite и использует lineage вместо простого overwrite.

Модель session включает:

- полную историю сообщений;
- session metadata;
- FTS/search;
- parent/child lineage через compression или session split;
- устойчивый учёт turn count и provider metadata.

В текущем локальном клоне Hermes `hermes_state.py` показывает `SCHEMA_VERSION = 8`. Считайте это снимком состояния клона, а не постоянным фактом для всего Hermes, и сверяйте старый текст документации с кодом перед тем, как на него опираться.

Compression у Hermes двухуровневая:

- gateway/session hygiene может заранее сжать слишком большие сессии;
- agent-side compressor работает внутри loop при реальном давлении на context.

Смысл compression — lossy summarization середины диалога с сохранением последних turns и пар tool call/tool result. Это полезный structural target для Dennett, но точные thresholds и helper heuristics у Hermes свои.

### 5. Memory

Hermes отделяет memory от chat history и resume state.

Есть отдельный memory-manager слой и memory-provider abstraction, а не одна жёстко заданная memory file.

Для Dennett важно моделировать memory как явное capability binding с правилами выбора и изоляции, а не как скрытый replay чата.

### 6. Skills, Plugins, MCP, Toolsets

Hermes строит action surface через registries и layered discovery.

Основные части:

- `toolsets.py` для групп tool bundles и composite resolution;
- `model_tools.py` и tool registry для schema collection и dispatch;
- `tools/mcp_tool.py` для MCP integration;
- `hermes_cli/plugins.py` для loading plugins и hooks;
- bundled skills плюс optional skills для prompt-side shaping.

Важный нюанс: plugin docs говорят об opt-in, но текущий локальный клон отдельно обращается с bundled backend plugins. В этом снимке встроенные backend plugins могут auto-load, а user-installed и project plugins всё ещё проходят allowlist gating.

Ещё один нюанс: MCP — это не только docs-идея, а часть runtime action surface, которая может регистрироваться динамически через тот же registry layer, что и built-in tools.

### 7. Delegation, Subagents, Approvals, Runtime Isolation

Hermes поддерживает delegation через отдельный subagent path, а не делает вид, что любой tool call — это child task.

`delegate_tool.py` и код `AIAgent` ведут отдельное child execution, depth и interrupt state. В текущем локальном клоне child запускается с isolated context и собственной budget logic. Эти правила лучше формулировать осторожно: наблюдаемый код явно показывает depth и isolation, но budget semantics не стоит превращать в якобы полностью универсальный внешний contract.

Approvals и isolation тоже являются first-class:

- dangerous command paths проходят через approval logic;
- tool execution можно interrupt;
- worker threads и child runs имеют отдельную cancellation handling;
- environment backends отделяют runtime execution от host process, когда это нужно.

### 8. Что Нужно Для Hermes-Equivalent Dennett Graph

Чтобы Dennett воспроизвёл Hermes близко, одного линейного LLM node chain недостаточно. Структурно понадобятся:

- node для стабильной prompt assembly с cacheable prefix;
- node для provider/runtime resolution перед исполнением;
- registry-backed tool-surface node для built-in tools, plugin tools, MCP tools и toolsets;
- session node для lineage, storage, compression state и replayable history;
- memory binding node для выбора provider и scoped visibility;
- delegation node для isolated child runs с явными budgets и return boundary;
- approval/isolation boundary для dangerous или sandboxed tool execution;
- shell/UI layer над этим графом для CLI, gateway и platform-specific presentation.

### 9. Ограничения И Несовпадения

Не нужно превращать docs в ложную уверенность. В Hermes есть несколько видимых расхождений между doc и code или слишком широких формулировок:

- session-storage docs могут отставать от текущего локального `hermes_state.py` snapshot по schema version;
- plugin docs упрощают opt-in story, но bundled backend plugins в текущем локальном code snapshot могут auto-load;
- описание tool concurrency шире, чем gate в code, который реально решает, можно ли запускать batch параллельно;
- child budget и depth handling видны в текущем code snapshot, но не всегда изложены как чистый внешний invariant.

Для Dennett это полезные сигналы: reference target нужно читать как "то, что вместе говорят local code и docs", при этом Dennett canon остаётся источником истины, а code имеет приоритет, если docs устарели.
