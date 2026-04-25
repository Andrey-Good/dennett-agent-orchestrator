[English](#english) | [Русский](#russian)

<a id="english"></a>
# English

# Subagent Task Lifecycle

Status: normative owner for delegated child-run sequencing.

Owns: parent/worker/reviewer/final-review progression; retry and escalation rules; and parallel-vs-sequential semantics for managed child runs.

Does not own: launch payload shape, state storage layout, or the overall orchestration model.

Related documents:

- [Subagent Orchestration Model](../02-architecture/subagent-orchestration-model.md)
- [Subagent MCP Contract](../03-contracts/subagent-mcp-contract.md)
- [Execution README](./README.md)
- [Subagent Context and Memory](../05-state/subagent-context-and-memory.md)
- [Graph Execution](./graph-execution.md)
- [Outputs, Outcomes, and Final Response](./outputs-outcomes-and-final-response.md)

This document refines the architecture owner by naming the execution sequence once work has been delegated into managed children.
It does not redefine the subagent model, the MCP contract, or the persisted state model.

## 1. Roles

The delegated-task lifecycle uses four roles:

- parent orchestrator;
- worker child;
- reviewer child;
- final-review child.

The parent remains responsible for the overall task at every stage. Delegation transfers work, not ownership of the final outcome.

## 2. Lifecycle Summary

The normal delegated flow is:

1. Parent decides that decomposition is warranted.
2. Parent assigns bounded subtasks with non-overlapping write sets.
3. Parent launches one or more worker children.
4. Each worker runs to a terminal boundary.
5. Parent classifies the worker result.
6. Parent decides whether the result needs review.
7. If review is needed, parent launches a reviewer child.
8. Parent evaluates reviewer findings.
9. Valid findings trigger a bounded repair worker.
10. The repaired result is reviewed again when review remains required.
11. Parent may run a final-review child after local worker/reviewer loops settle.
12. Parent accepts, replans, escalates, or fails the overall task.

This lifecycle is sequential around each child boundary, even when several unrelated children are active in parallel.

## 3. Worker Pass

A worker child owns one delegated task package.

The worker pass exists to produce the requested artifact or change inside the declared boundary.

Worker rules:

- the worker must stay inside the assigned write set;
- the worker must use the provided objective and prohibitions;
- the worker should perform the validations named in the task package when possible;
- the worker must return a bounded result to the parent;
- the worker does not decide whether its own output is accepted.

## 4. Reviewer Pass

The reviewer is a distinct child role, not a special mode of the worker.

Reviewer duties:

- assess correctness;
- assess boundary compliance;
- assess architecture fit;
- assess completeness;
- assess quality risks;
- assess validation adequacy.

Reviewer findings are inputs to parent decision-making. They are not self-executing commands.

The parent decides whether a reviewer finding is valid. If the finding is valid, the parent launches repair work. If the finding is not valid, the parent may reject it and continue.

## 5. Repair Loop

Valid reviewer findings trigger a bounded repair loop.

Required sequence:

1. parent accepts the finding as valid;
2. parent launches a repair worker with the narrow fix scope;
3. repair worker returns a bounded result;
4. parent launches the reviewer again when the review requirement still stands;
5. parent stops only when the result is accepted or the loop budget is exhausted.

The repair loop must not become uncontrolled recursion. The retry budget is explicit and bounded.

## 6. Parallel vs Sequential

Parallel execution is allowed only when the sibling children have disjoint write sets and independent acceptance surfaces.

Sequential execution is required when:

- one child produces context that another child needs;
- one child's decision constrains another child;
- a reviewer pass depends on the exact artifact produced by the prior worker;
- a repair pass depends on a valid reviewer finding;
- a final-review pass depends on the settled outputs of multiple children.

One child boundary is always sequential from the parent's point of view, even if the parent manages several children concurrently.

Nested spawning is allowed only while Core-enforced caps remain within policy: maximum nesting depth, maximum total descendants for one root task, maximum outstanding sibling children, maximum repair/review loops per subtask, and maximum cumulative task-tree budget. A launch that would exceed any of those caps must be rejected before the child starts.

## 7. Final Review

The final-review child is a separate gate for larger compositions.

Use a final-review child when the parent needs one more pass over the assembled result to catch:

- hidden conflicts between subtasks;
- cross-boundary regressions;
- architecture drift;
- locally acceptable changes that fail globally.

A final-review finding follows the same parent-evaluated repair logic as any other reviewer finding. It does not bypass the parent.

## 8. Escalation And Stop Conditions

The parent must distinguish at least these outcomes:

- accepted;
- retryable;
- review-rejected;
- escalated;
- failed;
- budget-exhausted.

Escalation is appropriate when the child result is outside the retry envelope, the task package is invalid, the write set conflicts with another child, or the review loop no longer improves confidence.

The parent may fail the overall task when the child result cannot be repaired within the explicit budget or when the task would require unsupported nested interaction.

## 9. Interaction Silence

Child runs are interaction-silent at the parent user boundary.

That means:

- child live comments are not surfaced through the parent run;
- child built-in user-chat traffic is not surfaced through the parent run;
- parent user text does not become child input by accident.

If a task would require surfaced child interaction that the base model does not support, the parent must reject that plan before launch.

## 10. Codex App Server Review And Thread Primitives

This lifecycle remains Core-owned even if the Codex adapter later uses App Server-native helpers underneath it.

Useful later-scope helpers already present in the verified App Server surface include:

- `review/start`, a review-oriented primitive that could later support Codex-specific review helpers once a higher-level owner document assigns stable lifecycle meaning to it;
- `thread/fork`, a thread-oriented primitive that could later support Codex-specific execution helpers once higher-level policy defines when and why to use it;
- `thread/rollback`, a thread-oriented primitive whose lifecycle meaning would need explicit future policy before the parent can rely on it;
- `thread/injectItems`, a thread-oriented primitive whose lifecycle meaning would need explicit future policy before the parent can rely on it.

Staging rules:

- the current base lifecycle does not require these primitives for worker, reviewer, repair, or final-review passes;
- if the Codex path adopts them later, they remain implementation helpers behind the adapter boundary rather than new portable lifecycle rights;
- parent acceptance, reviewer validity, repair budgeting, and write-set isolation remain Core-owned and cannot be delegated to App Server-native helper flows;
- any lifecycle meaning for `review/start`, `thread/fork`, `thread/rollback`, or `thread/injectItems` requires an explicit future owner document and policy before the parent may rely on it.

## 11. Cross-Links

- Child launch payload rules live in [Subagent MCP Contract](../03-contracts/subagent-mcp-contract.md).
- Child lineage and persistence live in [Subagent Context and Memory](../05-state/subagent-context-and-memory.md).
- Graph node dispatch remains in [Graph Execution](./graph-execution.md).
- Final output policy remains in [Outputs, Outcomes, and Final Response](./outputs-outcomes-and-final-response.md).

<a id="russian"></a>
# Русский

# Жизненный Цикл Subagent-задачи

Статус: нормативный владелец sequencing делегированных child-run.

Владеет: progression parent/worker/reviewer/final-review; правилами retry и escalation; а также parallel-vs-sequential семантикой для managed child run.

Не владеет: формой launch payload, layout хранилища состояния или общей моделью оркестрации.

Связанные документы:

- [Модель оркестрации subagent](../02-architecture/subagent-orchestration-model.md)
- [Контракт MCP для Subagent](../03-contracts/subagent-mcp-contract.md)
- [Execution README](./README.md)
- [Состояние и memory subagent](../05-state/subagent-context-and-memory.md)
- [Graph Execution](./graph-execution.md)
- [Outputs, Outcomes, and Final Response](./outputs-outcomes-and-final-response.md)

Этот документ уточняет architecture owner, задавая execution sequence после делегирования работы в managed children.
Он не переопределяет модель subagent, MCP contract или persisted state model.

## 1. Роли

В lifecycle делегированной задачи используются четыре роли:

- parent orchestrator;
- worker child;
- reviewer child;
- final-review child.

Parent остается ответственным за общую задачу на каждом этапе. Делегирование передает работу, но не ownership финального результата.

## 2. Краткий Lifecycle

Нормальный делегированный flow:

1. Parent решает, что нужна decomposition.
2. Parent назначает bounded subtasks с непересекающимися write set.
3. Parent запускает одного или нескольких worker children.
4. Каждый worker доходит до terminal boundary.
5. Parent классифицирует результат worker.
6. Parent решает, нужен ли review.
7. Если review нужен, parent запускает reviewer child.
8. Parent оценивает reviewer findings.
9. Валидные findings запускают bounded repair worker.
10. Исправленный результат снова проходит review, если требование review сохраняется.
11. Parent может запустить final-review child после того, как локальные worker/reviewer loops стабилизировались.
12. Parent принимает, replans, escalates или fail-ит общую задачу.

Этот lifecycle является последовательным вокруг каждого child boundary, даже если несколько независимых children активны параллельно.

## 3. Worker Pass

Worker child владеет одним delegated task package.

Worker pass нужен для создания требуемого artifact или изменения внутри объявленной boundary.

Правила worker:

- worker должен оставаться внутри назначенного write set;
- worker должен использовать предоставленные objective и prohibitions;
- worker должен по возможности выполнять validations, указанные в task package;
- worker должен возвращать bounded result родителю;
- worker не решает самостоятельно, принимается ли его output.

## 4. Reviewer Pass

Reviewer — это отдельная child role, а не специальный режим worker.

Обязанности reviewer:

- оценка correctness;
- оценка boundary compliance;
- оценка architecture fit;
- оценка completeness;
- оценка quality risks;
- оценка adequacy validation.

Reviewer findings являются входом для parent decision-making. Они не self-executing commands.

Parent сам решает, является ли reviewer finding valid. Если finding valid, parent запускает repair work. Если finding невалиден, parent может отклонить его и продолжить.

## 5. Repair Loop

Валидные reviewer findings запускают bounded repair loop.

Обязательная последовательность:

1. parent принимает finding как valid;
2. parent запускает repair worker с narrow fix scope;
3. repair worker возвращает bounded result;
4. parent снова запускает reviewer, если review requirement все еще действует;
5. parent останавливается только когда результат принят или loop budget исчерпан.

Repair loop не должен превращаться в uncontrolled recursion. Retry budget является явным и bounded.

## 6. Parallel и Sequential

Параллельное исполнение допускается только тогда, когда sibling children имеют непересекающиеся write set и независимые acceptance surfaces.

Последовательное исполнение требуется, когда:

- один child производит context, нужный другому child;
- решение одного child ограничивает другого child;
- reviewer pass зависит от точного artifact, созданного предыдущим worker;
- repair pass зависит от валидного reviewer finding;
- final-review pass зависит от стабилизированных outputs нескольких children.

Одна child boundary всегда последовательна с точки зрения parent, даже если parent управляет несколькими children одновременно.

Nested spawning разрешен только пока Core-enforced caps остаются в пределах policy: maximum nesting depth, maximum total descendants для одной root task, maximum outstanding sibling children, maximum repair/review loops для одной подзадачи и maximum cumulative task-tree budget. Launch, который превысил бы любой из этих caps, должен быть отклонен до старта child.

## 7. Final Review

Final-review child — это отдельный gate для более крупных композиций.

Используйте final-review child, когда parent нужен еще один проход по собранному результату, чтобы поймать:

- скрытые конфликты между subtasks;
- cross-boundary regressions;
- architecture drift;
- локально допустимые изменения, которые глобально ошибочны.

Finding от final-review проходит ту же parent-evaluated repair logic, что и любой другой reviewer finding. Он не обходит parent.

## 8. Escalation и Stop Conditions

Parent должен различать как минимум следующие outcomes:

- accepted;
- retryable;
- review-rejected;
- escalated;
- failed;
- budget-exhausted.

Escalation уместна, когда результат child выходит за retry envelope, task package невалиден, write set конфликтует с другим child или review loop больше не повышает уверенность.

Parent может fail-ить общую задачу, если результат child нельзя исправить в пределах явного budget или если задача потребовала бы неподдерживаемого nested interaction.

## 9. Interaction Silence

Child run остаются interaction-silent на parent user boundary.

Это означает:

- live comments child не surface-ятся через parent run;
- built-in user-chat traffic child не surface-ится через parent run;
- parent user text не становится child input случайно.

Если задача потребовала бы surfaced child interaction, которую базовая модель не поддерживает, parent обязан отклонить такой plan до launch.

## 10. Codex App Server review- и thread-примитивы

Этот lifecycle остается Core-owned даже тогда, когда Codex adapter позже использует под ним App Server-native helpers.

Полезные later-stage helpers, уже присутствующие в проверенной поверхности App Server:

- `review/start`, review-ориентированный primitive, который позже может поддержать Codex-specific review-helpers только после того, как более высокий owner-документ задаст ему стабильный lifecycle-смысл;
- `thread/fork`, thread-ориентированный primitive, который позже может поддержать Codex-specific execution-helpers только после явной higher-level policy о том, когда и зачем его использовать;
- `thread/rollback`, thread-ориентированный primitive, чей lifecycle-смысл потребует явной будущей policy до того, как parent сможет на него опираться;
- `thread/injectItems`, thread-ориентированный primitive, чей lifecycle-смысл потребует явной будущей policy до того, как parent сможет на него опираться.

Правила этапности:

- текущий базовый lifecycle не требует этих примитивов для worker-, reviewer-, repair- или final-review-pass;
- если Codex path позже начнет их использовать, они остаются implementation helpers за границей adapter-а, а не новыми portable lifecycle-rights;
- принятие parent-ом, валидность reviewer finding, budgeting repair и изоляция write set остаются Core-owned и не могут быть делегированы App Server-native helper-flows;
- любой lifecycle-смысл для `review/start`, `thread/fork`, `thread/rollback` или `thread/injectItems` требует явного будущего owner-документа и policy до того, как parent сможет на него опираться.

## 11. Перекрестные Ссылки

- Правила launch payload child находятся в [Контракте MCP для Subagent](../03-contracts/subagent-mcp-contract.md).
- Lineage child и persistence находятся в [Состоянии и memory subagent](../05-state/subagent-context-and-memory.md).
- Dispatch graph node остается в [Graph Execution](./graph-execution.md).
- Политика final output остается в [Outputs, Outcomes, and Final Response](./outputs-outcomes-and-final-response.md).
