[English](#english) | [Русский](#russian)

<a id="english"></a>
# Subagent Orchestration Model

Status: normative owner for the subagent reference model.

Related documents:

- [`README.md`](./README.md)
- [`core-and-interfaces.md`](./core-and-interfaces.md)
- [`runtime-integration-model.md`](./runtime-integration-model.md)
- [`../04-execution/graph-execution.md`](../04-execution/graph-execution.md)
- [`../04-execution/outputs-outcomes-and-final-response.md`](../04-execution/outputs-outcomes-and-final-response.md)
- [`../05-state/README.md`](../05-state/README.md)
- [`../06-interaction/live-run-interaction.md`](../06-interaction/live-run-interaction.md)
- [`../07-lifecycle/agent-registry.md`](../07-lifecycle/agent-registry.md)
- [`../08-extensions/builder-agent.md`](../08-extensions/builder-agent.md)
- [`../08-extensions/memory-bindings.md`](../08-extensions/memory-bindings.md)
- [`../08-extensions/runtime-sources.md`](../08-extensions/runtime-sources.md)

This document formalizes the current working subagent model used by the orchestrator and turns it into a product-facing architecture reference. It is intentionally close to the observed native working style: a parent agent decomposes a larger task into bounded child tasks, assigns each child a clear write set and acceptance target, waits for the child result, and uses explicit review loops before treating the larger task as complete.

This document is architecture-owned. It defines the orchestration primitive, role model, lifecycle, and responsibility boundaries. It does not define field-level wire contracts for launching subagents or persisting their state.

## 1. Normative Claim

In this product, a subagent is not a generic tool call.

A subagent launch is a Core-owned orchestration action that creates a child run with:

- a delegated objective;
- a bounded authority set;
- an explicit task boundary;
- an expected result and validation target;
- a return path back into the parent orchestration flow.

The implementation must preserve that distinction. If subagents are modeled as ordinary opaque tools, the product loses ownership boundaries, review semantics, and nested-run correctness.

### Relation to `orchestrator_agent`

The product has two child-run surfaces, not one overloaded surface.

- `orchestrator_agent` is the portable graph-defined recursive child-run primitive. It owns `agent_ref` resolution, delivery of resolved node input, and the normalized child-return boundary inside graph execution.
- Managed subagent orchestration is a richer Core-owned delegation model layered on top of the same child-run laws. It adds explicit task-package semantics such as `write_set`, `read_context`, `child_role`, review loops, and budgeted control and close operations.
- Therefore, not every `orchestrator_agent` node is a managed subagent. Managed subagents may be implemented through the same underlying child-run mechanism or through an internal child agent launched by that mechanism, but graph execution must not invent managed-task-package semantics for a plain `orchestrator_agent` node.

## 2. What Must Be Copied Closely From The Current Working Model

The current working model already has a stable shape. The product implementation should preserve these characteristics almost exactly:

- large work is normally decomposed rather than executed monolithically;
- decomposition is done by responsibility, not by arbitrary file slicing;
- overlapping write sets between sibling subagents are forbidden;
- each child receives a task-style brief with scope, prohibitions, validation, and acceptance criteria;
- worker execution and review execution are separate roles;
- valid reviewer findings trigger a fix-and-review loop rather than a silent accept;
- parent agents remain responsible for the overall task and for cross-subtask consistency;
- a final system-level review may be required after local subtasks are complete;
- direct execution without delegation is a fast path only for genuinely small work.

The product must make these behaviors explicit and durable instead of leaving them as informal prompting habits.

## 3. What Must Be Made Stricter Than The Current Native Environment

The native working style is useful but under-specified in places. Product implementation must make the following stricter:

- delegation boundaries must be first-class runtime objects rather than prompt-only conventions;
- write-set exclusivity must be enforced or rejected before conflicting sibling work starts;
- parent and child identities, run boundaries, and task lineage must be visible in Core state;
- budgets and spawn limits must be checked explicitly rather than implied socially;
- child-return handling must use normalized outcomes and final payload rules rather than ad hoc parsing;
- review loops must be representable as explicit workflow states, not just free-form conversation history;
- context inheritance must be selective and rule-driven rather than "whatever happened to be in the thread";
- nested spawning must be bounded by policy and must never default to uncontrolled recursion.

## 4. Roles In The Subagent System

The base model has these roles:

### 4.1. Parent Orchestrator

The parent orchestrator owns:

- deciding whether decomposition is needed;
- choosing the subtask split;
- assigning non-overlapping write sets;
- selecting which subtasks may run in parallel and which require sequencing;
- preparing the child task package;
- deciding when reviewer passes are required;
- evaluating child results and reviewer findings;
- deciding whether to accept, relaunch, repair, escalate, or fail the overall task.

The parent does not stop being responsible after launch. Delegation transfers work, not ownership of the top-level outcome.

### 4.2. Worker Subagent

The worker owns one delegated task package and must operate inside it. The worker is responsible for:

- reading the provided task boundary and allowed context;
- producing the requested artifact or change;
- staying inside the assigned write set;
- performing the validations named in the task package when possible;
- returning a bounded result to the parent.

### 4.3. Reviewer Subagent

The reviewer is a distinct child role, not a post-processing mode of the worker. Its job is to evaluate:

- correctness;
- boundary compliance;
- architecture fit;
- completeness;
- quality risks;
- adequacy of validation.

The reviewer does not automatically override the parent. The parent evaluates the reviewer output and decides whether the finding is valid.

### 4.4. Final System Reviewer

For larger compositions, the system may require one more child pass after local worker/reviewer loops are complete. This role checks for:

- hidden conflicts between subtasks;
- cross-boundary regressions;
- architecture drift;
- locally acceptable changes that fail globally.

## 5. Lifecycle Of A Delegated Child Task

The product should implement the child-task lifecycle close to this sequence:

1. Detect that the current task is large enough or risky enough to justify delegation.
2. Decompose the task into one or more bounded subtasks.
3. Assign ownership and reject overlapping sibling write sets.
4. Build the child task package.
5. Launch the child run.
6. Wait for the child terminal outcome and final return payload.
7. Classify the result as accepted, rejected, retryable, or requiring follow-up review.
8. Optionally launch a reviewer child.
9. If reviewer findings are valid, launch a repair worker and then review again.
10. Merge accepted child outputs into the parent plan and continue orchestration.
11. Optionally launch a final system reviewer before the parent task is considered complete.

This lifecycle is sequential at each single child boundary, even if the overall parent may have multiple children in flight. A single child launch produces one child run and one parent-visible return boundary.

## 6. Decomposition Rules

Subtask decomposition is not arbitrary fan-out. Core must preserve these rules:

- split by responsibility and deliverable, not by random file chunk;
- prefer one child when the work is tightly coupled and splitting would lower quality;
- allow parallel children only when their write sets and acceptance surfaces do not conflict;
- require sequential execution when one child produces context, decisions, or artifacts another child depends on;
- require each child to have a narrow enough mission that it can succeed without guessing hidden intent;
- reject decomposition plans that create ambiguous ownership.

The decomposition phase must therefore produce both a task graph and an ownership map.

## 7. Child Task Package

This document does not define exact wire fields, but the child package must carry these semantic categories:

- child task identifier and role;
- objective and expected result;
- parent relationship and lineage;
- in-scope work;
- out-of-scope work;
- allowed write set;
- read-only neighboring context;
- architectural constraints and prohibited behaviors;
- required validations;
- acceptance criteria;
- dependency or sequencing notes;
- any explicit budget or depth limit relevant to that child.

This package is the formal product equivalent of the current task-document pattern. Future contract docs should encode these categories directly instead of reducing a child launch to "prompt plus files".
It is richer than a portable graph node invocation. A managed child package may be carried by a higher-level orchestration surface or compiled into an internal child agent, but it is not encoded by the portable `orchestrator_agent` node contract itself.

## 8. Context Boundary

Child context must be explicit and selective.

The parent may pass:

- the task package;
- relevant source documents or files;
- summarized prior findings;
- constrained local state required for execution;
- policy and budget information.

The child must not automatically inherit the entire parent working set. In particular:

- the child does not gain write access outside the assigned boundary;
- the child does not gain hidden access to every parent artifact just because the parent saw it;
- the child does not automatically inherit the caller's memory bindings;
- the child does not automatically inherit the caller's runtime-source narrowing;
- the child does not inherit surfaced user interaction from the parent boundary in the base model.

This selective inheritance rule is essential for reproducibility and for future contract enforcement.

## 9. Parent-Child Return Boundary

The parent may observe only normalized child boundary data.

For a child run, the parent may rely on:

- child identity and lineage metadata;
- normalized terminal outcome;
- the child run's final response payload when one exists;
- explicit validation or review artifacts the child chose to return through its declared output surface.

The parent must not rely on:

- hidden chain-of-thought;
- undeclared intermediate notes;
- internal tool traces;
- implicit "last child step" data;
- any secret backchannel that bypasses the child output contract.

This is the shared child-run return law with `orchestrator_agent`: only normalized outcome plus final response payload cross the boundary.

## 10. Nested Spawning

Nested spawning is allowed, but only as bounded recursive orchestration.

The base model is:

- a child may itself act as a parent and launch its own children;
- each nested launch creates a new child boundary with the same ownership rules;
- nested runs remain internal to their immediate parent unless explicitly surfaced by a future specification;
- surfaced live interaction does not automatically pass through nested boundaries;
- if a selected child plan would require unsupported surfaced nested interaction, Core must reject it before launch.

Uncontrolled recursive spawning is forbidden. The implementation must support explicit limits such as:

- maximum nesting depth;
- maximum total descendants for one root task;
- maximum outstanding sibling children;
- maximum repair/review loop count for one subtask;
- maximum cumulative budget consumption for one task tree.

The exact enforcement mechanism belongs in execution and state documents, but the architecture must require these controls.

## 11. Budgets

Subagent execution is budgeted work, not an unbounded planning fantasy.

At minimum, the product model should support these budget dimensions:

- delegation budget: whether the parent may spawn any children at all;
- parallelism budget: how many siblings may run concurrently;
- depth budget: how deep recursive delegation may go;
- retry budget: how many times a failed worker or reviewer loop may be relaunched;
- context budget: how much material may be passed into a child;
- write budget: which files or resources the child may mutate;
- time or step budget: how much execution the child may consume before the parent must re-evaluate.

Budgets are owned by Core policy. Adapters and interfaces may display them, but they must not redefine them.

## 12. Review Loop Model

The review loop is a first-class orchestration pattern:

1. launch worker;
2. inspect result;
3. launch reviewer when the task warrants review;
4. classify reviewer findings as valid or invalid;
5. if valid, launch a bounded repair worker;
6. review again;
7. stop only when the output is accepted or the loop budget is exhausted.

Important consequences:

- a reviewer is not merely another worker prompt with a different label;
- reviewer findings are inputs to parent decision-making, not self-executing commands;
- not every reviewer claim should be followed mechanically;
- the parent must preserve the distinction between "review found a valid defect" and "review expressed an incorrect opinion".

## 13. Interaction Model For Child Runs

In the base model, a child run is interaction-silent from the parent user's point of view.

That means:

- the child run's live comments are not surfaced through the parent run;
- the child run's built-in `orchestrator.user_chat` traffic is not surfaced through the parent run;
- free-form user text in the parent surface does not become child input by accident;
- if a child task would require surfaced nested live interaction that the base model does not support, the launch must be rejected before the child starts.

This keeps one visible interactive boundary at a time and prevents ambiguous routing.

## 14. Failure And Escalation Semantics

The architecture must distinguish at least these classes of child failure:

- boundary failure: invalid task package, invalid permissions, conflicting write set, unsupported nested mode;
- execution failure: runtime or orchestration error during child work;
- invalid return: child completed but produced no acceptable return payload for the parent boundary;
- review rejection: reviewer found a valid defect requiring repair;
- budget exhaustion: retry, depth, or time budget was consumed before acceptance.

These are not all equivalent. Some require repair, some require replanning, and some require parent-level failure of the whole task.

## 15. Relationship To Other Owner Docs

This document owns the subagent system as an orchestration model.

It does not own:

- exact launch and return payload fields; those belong to later contracts docs;
- detailed child-run state storage and resume records; those belong to state docs;
- exact review-loop execution states and transition tables; those belong to execution docs;
- file-level `agent_ref` resolution rules; those belong to lifecycle docs;
- memory-binding and runtime-source semantics beyond the inheritance boundary stated here; those belong to extension docs.

When a later doc adds detail, it must refine this model rather than replace it.

## 16. Architectural Placement

Subagent semantics belong to Core.

Implementation should therefore follow this placement logic:

- child-task planning, ownership checks, budget checks, and launch decisions live in `src/core`;
- child-launch and child-result ports live under `src/ports`;
- runtime-specific ways of executing a child through an external runtime live in `src/adapters/runtime/*`;
- state persistence for child lineage, attempts, reviews, and accepted outputs lives behind Core-owned state ports;
- interfaces may render child progress and review status, but they must not invent delegation rules of their own.

## 17. Compliance Checklist

An implementation conforms to this model only if all of the following remain true:

- subagents are modeled as orchestration primitives rather than generic opaque tools;
- sibling write-set conflicts are prevented or rejected explicitly;
- child runs have explicit lineage back to a parent task;
- a child receives a bounded task package rather than accidental whole-thread inheritance;
- the parent reads only normalized child outcome plus declared return payload;
- nested spawning is bounded by policy and does not default to infinite recursion;
- review loops are explicit and bounded;
- the system preserves the distinction between portable `orchestrator_agent` recursion and richer managed subagent orchestration while keeping their shared boundary laws coherent;
- Core, not interfaces or adapters, owns delegation semantics.

<a id="russian"></a>
# Модель оркестрации субагентов

Статус: нормативный документ-владелец для reference-модели субагентов.

Связанные документы:

- [`README.md`](./README.md)
- [`core-and-interfaces.md`](./core-and-interfaces.md)
- [`runtime-integration-model.md`](./runtime-integration-model.md)
- [`../04-execution/graph-execution.md`](../04-execution/graph-execution.md)
- [`../04-execution/outputs-outcomes-and-final-response.md`](../04-execution/outputs-outcomes-and-final-response.md)
- [`../05-state/README.md`](../05-state/README.md)
- [`../06-interaction/live-run-interaction.md`](../06-interaction/live-run-interaction.md)
- [`../07-lifecycle/agent-registry.md`](../07-lifecycle/agent-registry.md)
- [`../08-extensions/builder-agent.md`](../08-extensions/builder-agent.md)
- [`../08-extensions/memory-bindings.md`](../08-extensions/memory-bindings.md)
- [`../08-extensions/runtime-sources.md`](../08-extensions/runtime-sources.md)

Этот документ формализует текущую рабочую модель субагентов, которую использует оркестратор, и превращает ее в продуктовую архитектурную reference-модель. Он намеренно близок к наблюдаемому нативному стилю работы: родительский агент декомпозирует крупную задачу на ограниченные дочерние задачи, назначает каждой четкий write set и критерий приемки, ждет результат дочернего запуска и использует явные review-циклы, прежде чем считать большую задачу завершенной.

Этот документ относится к архитектуре. Он определяет orchestration primitive, ролевую модель, lifecycle и границы ответственности. Он не определяет field-level wire-контракты для запуска субагентов или хранения их состояния.

## 1. Нормативное утверждение

В этом продукте субагент не является generic tool call.

Запуск субагента - это принадлежащее Core orchestration-действие, которое создает child run с:

- делегированной целью;
- ограниченным набором полномочий;
- явной границей задачи;
- ожидаемым результатом и целью валидации;
- путем возврата обратно в родительский orchestration flow.

Реализация обязана сохранять это различие. Если субагенты будут смоделированы как обычные непрозрачные tools, продукт потеряет границы владения, review-семантику и корректность nested-run поведения.

### Связь с `orchestrator_agent`

В продукте есть две поверхности child-run, а не одна перегруженная поверхность.

- `orchestrator_agent` - это переносимый graph-defined primitive рекурсивного child run. Он владеет разрешением `agent_ref`, доставкой resolved node input и нормализованной child-return boundary внутри graph execution.
- Managed subagent orchestration - это более богатая Core-owned delegation model, построенная поверх тех же child-run laws. Она добавляет явную task-package semantics, такую как `write_set`, `read_context`, `child_role`, review loops и budgeted control/close operations.
- Поэтому не каждая нода `orchestrator_agent` является managed subagent. Managed subagent может быть реализован через тот же базовый child-run mechanism или через внутренний child agent, запускаемый этим mechanism, но graph execution не должен изобретать managed-task-package semantics для обычной ноды `orchestrator_agent`.

## 2. Что нужно копировать близко к текущей рабочей модели

У текущей рабочей модели уже есть устойчивая форма. Продуктовая реализация должна почти дословно сохранить следующие свойства:

- крупная работа обычно декомпозируется, а не исполняется монолитно;
- декомпозиция делается по ответственности, а не по случайным кускам файлов;
- пересекающиеся write sets у sibling-субагентов запрещены;
- каждый child получает task-style brief со scope, запретами, валидацией и acceptance criteria;
- worker-исполнение и review-исполнение являются разными ролями;
- валидные reviewer findings запускают цикл fix-and-review, а не тихое принятие;
- родительские агенты продолжают отвечать за итоговую задачу и за согласованность между подзадачами;
- после локального завершения подзадач может требоваться финальный system-level review;
- прямое исполнение без делегирования является fast path только для действительно маленькой работы.

Продукт должен сделать это поведение явным и долговечным, а не оставлять его на уровне неформальных prompting-привычек.

## 3. Что нужно сделать строже, чем в текущем нативном окружении

Нативный стиль работы полезен, но местами недоспецифицирован. Продуктовая реализация должна сделать строже следующее:

- границы делегирования должны быть first-class runtime-объектами, а не только prompt-конвенцией;
- эксклюзивность write set должна проверяться или отклоняться до запуска конфликтующих sibling-работ;
- родительские и дочерние идентичности, границы run-ов и lineage задач должны быть видимы в Core state;
- бюджеты и лимиты на spawning должны проверяться явно, а не предполагаться социально;
- обработка child-return должна использовать нормализованные outcomes и правила final payload, а не ad hoc parsing;
- review-циклы должны представляться как явные workflow-состояния, а не только как свободная переписка;
- наследование контекста должно быть избирательным и rule-driven, а не "все, что случайно оказалось в треде";
- nested spawning должен быть ограничен политикой и никогда не должен по умолчанию превращаться в неконтролируемую рекурсию.

## 4. Роли в системе субагентов

В базовой модели есть следующие роли:

### 4.1. Родительский оркестратор

Родительский оркестратор владеет:

- решением, нужна ли декомпозиция;
- выбором разбиения на подзадачи;
- назначением непересекающихся write sets;
- определением, какие подзадачи можно выполнять параллельно, а какие требуют последовательности;
- подготовкой child task package;
- решением, когда нужны reviewer-проходы;
- оценкой child-результатов и reviewer findings;
- решением, принять, перезапустить, починить, эскалировать или провалить общую задачу.

После запуска родитель не перестает быть ответственным. Делегирование передает работу, но не владение top-level результатом.

### 4.2. Worker-субагент

Worker владеет одной делегированной task package и обязан работать внутри нее. Worker отвечает за:

- чтение предоставленной границы задачи и разрешенного контекста;
- создание запрошенного артефакта или изменения;
- работу строго внутри назначенного write set;
- выполнение указанных в task package проверок, когда это возможно;
- возврат ограниченного результата родителю.

### 4.3. Reviewer-субагент

Reviewer - это отдельная child-роль, а не post-processing режим worker-а. Его задача оценить:

- correctness;
- соблюдение границ;
- архитектурную совместимость;
- полноту;
- quality risks;
- достаточность validation.

Reviewer не переопределяет родителя автоматически. Родитель оценивает reviewer output и решает, валидна ли находка.

### 4.4. Финальный системный reviewer

Для больших композиций системе может потребоваться еще один child-pass после завершения локальных worker/reviewer циклов. Эта роль проверяет:

- скрытые конфликты между подзадачами;
- cross-boundary regressions;
- architecture drift;
- изменения, которые локально выглядят приемлемо, но глобально ломают систему.

## 5. Lifecycle делегированной дочерней задачи

Продукт должен реализовать lifecycle дочерней задачи примерно по такой последовательности:

1. Выявить, что текущая задача достаточно большая или рискованная и требует делегирования.
2. Декомпозировать задачу на одну или несколько ограниченных подзадач.
3. Назначить владение и отклонить пересекающиеся sibling write sets.
4. Сформировать child task package.
5. Запустить child run.
6. Дождаться terminal outcome дочернего run-а и его final return payload.
7. Классифицировать результат как принятый, отклоненный, retryable или требующий follow-up review.
8. При необходимости запустить reviewer-child.
9. Если reviewer findings валидны, запустить repair worker, а затем снова review.
10. Встроить принятые child outputs обратно в parent plan и продолжить orchestration.
11. При необходимости запустить финального system reviewer до того, как parent task будет считаться завершенной.

Этот lifecycle является последовательным на границе каждого отдельного child. Один child launch создает один child run и одну видимую для parent-а return boundary.

## 6. Правила декомпозиции

Декомпозиция подзадач не является произвольным fan-out. Core обязан сохранять такие правила:

- делить по ответственности и deliverable, а не по случайным фрагментам файлов;
- предпочитать одного child-а, когда работа тесно связана и разбиение ухудшит качество;
- разрешать параллельных children только когда их write sets и acceptance surfaces не конфликтуют;
- требовать последовательного исполнения, когда один child производит контекст, решения или артефакты, от которых зависит другой;
- требовать, чтобы миссия каждого child была достаточно узкой и не заставляла его угадывать скрытый замысел;
- отклонять планы декомпозиции, создающие неоднозначное владение.

Следовательно, фаза декомпозиции должна порождать и task graph, и ownership map.

## 7. Child task package

Этот документ не определяет точные wire-поля, но child package обязана нести следующие семантические категории:

- идентификатор child task и роль;
- цель и ожидаемый результат;
- связь с родителем и lineage;
- in-scope работа;
- out-of-scope работа;
- разрешенный write set;
- read-only соседний контекст;
- архитектурные ограничения и запрещенное поведение;
- обязательные validations;
- acceptance criteria;
- dependency или sequencing notes;
- любые явные бюджеты или лимиты глубины, относящиеся к этому child.

Эта package является формальным продуктовым эквивалентом текущего task-document pattern. Будущие contract docs должны кодировать эти категории напрямую, а не сводить child launch к "prompt плюс files".
Она богаче, чем переносимый вызов graph node. Managed child package может передаваться через более высокий orchestration surface или компилироваться во внутренний child agent, но сам по себе он не закодирован в переносимом контракте ноды `orchestrator_agent`.

## 8. Граница контекста

Контекст child-а должен быть явным и избирательным.

Parent может передавать:

- task package;
- релевантные source documents или files;
- summarized prior findings;
- ограниченное локальное state, необходимое для исполнения;
- policy и budget information.

Child не должен автоматически наследовать весь working set родителя. В частности:

- child не получает write access вне назначенной границы;
- child не получает скрытый доступ ко всем parent artifacts только потому, что parent их видел;
- child не наследует memory bindings вызывающего автоматически;
- child не наследует runtime-source narrowing вызывающего автоматически;
- child не наследует surfaced user interaction через parent boundary в базовой модели.

Это правило выборочного наследования критично для воспроизводимости и для будущего enforcement на уровне контрактов.

## 9. Граница возврата parent-child

Parent может наблюдать только нормализованные данные child boundary.

Для child run-а parent может опираться на:

- metadata идентичности child-а и lineage;
- нормализованный terminal outcome;
- final response payload child run-а, когда он существует;
- явные validation- или review-артефакты, которые child решил вернуть через свою объявленную output surface.

Parent не должен опираться на:

- скрытый chain-of-thought;
- необъявленные промежуточные заметки;
- внутренние tool traces;
- неявные данные уровня "last child step";
- любой секретный backchannel, обходящий child output contract.

Это общий child-run law возврата с `orchestrator_agent`: через границу проходят только нормализованный outcome и final response payload.

## 10. Nested spawning

Nested spawning разрешен, но только как ограниченная рекурсивная оркестрация.

Базовая модель такова:

- child может сам действовать как parent и запускать собственных children;
- каждый nested launch создает новую child boundary с теми же правилами владения;
- nested runs остаются внутренними по отношению к своему непосредственному parent-у, если только будущая спецификация явно не сделает их surfaced;
- surfaced live interaction не проходит через nested boundaries автоматически;
- если выбранный child plan требует неподдерживаемого surfaced nested interaction, Core обязан отклонить его до запуска.

Неконтролируемый рекурсивный spawning запрещен. Реализация обязана поддерживать явные лимиты, такие как:

- максимальная глубина вложенности;
- максимальное число потомков у одной root-задачи;
- максимальное количество одновременно активных sibling-children;
- максимальное число repair/review-циклов для одной подзадачи;
- максимальный совокупный расход бюджета для одного task tree.

Точный механизм enforcement относится к execution и state docs, но архитектура обязана требовать эти контроли.

## 11. Бюджеты

Исполнение субагентов - это budgeted work, а не безграничная planning-фантазия.

Как минимум продуктовая модель должна поддерживать такие измерения бюджета:

- delegation budget: можно ли parent-у вообще порождать children;
- parallelism budget: сколько sibling-children может работать одновременно;
- depth budget: насколько глубоко может идти рекурсивное делегирование;
- retry budget: сколько раз можно перезапускать failed worker или reviewer loop;
- context budget: сколько материала можно передавать child-у;
- write budget: какие files или resources child может менять;
- time или step budget: сколько исполнения child может потребить до того, как parent обязан переоценить ситуацию.

Бюджетами владеет Core policy. Adapters и interfaces могут их показывать, но не имеют права их переопределять.

## 12. Модель review-loop

Review loop является first-class orchestration pattern:

1. запустить worker-а;
2. изучить результат;
3. запустить reviewer-а, когда задача требует review;
4. классифицировать reviewer findings как валидные или невалидные;
5. если finding валиден, запустить ограниченного repair worker-а;
6. снова провести review;
7. остановиться только когда output принят или loop budget исчерпан.

Важные следствия:

- reviewer - это не просто еще один worker prompt с другой меткой;
- reviewer findings являются входом для parent decision-making, а не самовыполняющимися командами;
- не каждое утверждение reviewer-а нужно выполнять механически;
- parent обязан сохранять различие между "review нашел валидный дефект" и "review высказал неверное мнение".

## 13. Модель взаимодействия для child run-ов

В базовой модели child run является interaction-silent с точки зрения пользователя parent run-а.

Это означает:

- live comments child run-а не surface-ятся через parent run;
- трафик built-in `orchestrator.user_chat` child run-а не surface-ится через parent run;
- свободный пользовательский текст в parent surface не превращается в child input случайно;
- если child task потребовала бы surfaced nested live interaction, которое базовая модель не поддерживает, запуск должен быть отклонен до старта child-а.

Это сохраняет одну видимую interactive boundary за раз и предотвращает неоднозначную маршрутизацию.

## 14. Семантика отказов и эскалации

Архитектура обязана различать как минимум следующие классы child-failure:

- boundary failure: невалидная task package, невалидные permissions, конфликтующий write set, неподдерживаемый nested mode;
- execution failure: runtime- или orchestration-ошибка во время работы child-а;
- invalid return: child завершился, но не выдал приемлемый return payload для parent boundary;
- review rejection: reviewer нашел валидный дефект, требующий repair;
- budget exhaustion: retry-, depth- или time-budget исчерпан до приемки.

Это не эквивалентные случаи. Некоторые требуют repair, некоторые - replanning, а некоторые должны приводить к parent-level провалу всей задачи.

## 15. Связь с другими owner docs

Этот документ владеет системой субагентов как orchestration-моделью.

Он не владеет:

- точными launch- и return-payload полями; это относится к будущим contracts docs;
- детальным хранением child-run state и resume records; это относится к state docs;
- точными execution states и transition tables для review loops; это относится к execution docs;
- правилами разрешения `agent_ref` на уровне файлов; это относится к lifecycle docs;
- semantics memory bindings и runtime sources сверх указанной здесь границы наследования; это относится к extension docs.

Когда более поздний документ добавляет детали, он должен уточнять эту модель, а не заменять ее.

## 16. Архитектурное размещение

Семантика субагентов принадлежит Core.

Следовательно, реализация должна следовать такой логике размещения:

- child-task planning, ownership checks, budget checks и launch decisions живут в `src/core`;
- child-launch и child-result ports живут в `src/ports`;
- runtime-specific способы исполнить child-а через внешний runtime живут в `src/adapters/runtime/*`;
- state persistence для child lineage, attempts, reviews и accepted outputs живет за Core-owned state ports;
- interfaces могут отображать child progress и review status, но не имеют права изобретать собственные правила делегирования.

## 17. Чеклист соответствия

Реализация соответствует этой модели только если одновременно сохраняется следующее:

- субагенты смоделированы как orchestration primitives, а не как generic opaque tools;
- конфликты sibling write sets предотвращаются или явно отклоняются;
- у child run-ов есть явный lineage обратно к parent task;
- child получает ограниченную task package, а не случайное наследование всего thread-а;
- parent читает только нормализованный child outcome плюс объявленный return payload;
- nested spawning ограничен политикой и не превращается по умолчанию в бесконечную рекурсию;
- review loops явны и ограничены;
- система сохраняет различие между переносимой рекурсией `orchestrator_agent` и более богатой managed subagent orchestration, при этом их общие boundary laws остаются согласованными;
- semantics делегирования принадлежат Core, а не interfaces или adapters.
