# Dennett Pragmatic Agentic Control Fabric

> **Repository edition · 2026-07-13 · `20`**  
> Это самостоятельный канонический документ репозитория Dennett. Начните с [карты документации](../README.md).  
> Related: [10_Dennett_Memory_Fabric.md](./10_Dennett_Memory_Fabric.md).

## Интегрированные contract supplements

Следующие небольшие нормативные документы выделены из предархитектурного gap-аудита. Они являются частью текущего набора и обязательны для изменений, пересекающих указанные границы:

- [`B_External_Communication_Operation.md`](contracts/B_External_Communication_Operation.md)
- [`C_Project_Lifecycle_Contract.md`](contracts/C_Project_Lifecycle_Contract.md)
- [`D_Artifact_Lifecycle_Contract.md`](contracts/D_Artifact_Lifecycle_Contract.md)
- [`G_Resource_Pressure_and_Usage_Accounting_Contract.md`](contracts/G_Resource_Pressure_and_Usage_Accounting_Contract.md)
- [`K_Composite_Experience_Recipes.md`](contracts/K_Composite_Experience_Recipes.md)

Эти supplements не дают одному lifecycle разойтись по нескольким большим файлам; canonical owner указан в заголовке каждого документа.


## Полная бизнес-логика главного оркестратора, проектных агентов, задач, делегирования, команд, процедур и управляемых автоматизаций

**Версия:** 1.1  
**Дата исследования:** 11 июля 2026 года  
**Статус:** текущий исследовательский baseline бизнес-логики агентов. Версия 1.1 заменяет `Dennett_agent_orchestration_logic_v1.0_agentic_control_fabric_2026.md` как рекомендуемую основу, сохраняя его сильные решения и отменяя избыточную обязательную формализацию.

Документ продолжает:

- `00_Dennett_Functional_Concept.md`;
- `10_Dennett_Memory_Fabric.md`.

Он не выбирает конкретный agent SDK, workflow engine, язык программирования, очередь или СУБД. Он определяет, **когда Dennett должен просто дать сильному агенту работать, когда требуется лёгкий управляющий слой, когда оправдан subagent или reviewer и когда действительно нужна долговечная автоматизация**.

---

# 0. Итоговый вердикт

## 0.1. Что было ошибочно в версии 1.0

Повторное исследование подтвердило большую часть критики пользователя.

### Ошибка 1: workflow был превращён в обязательный мини-Temporal

Версия 1.0 правильно защищала долгие процессы от падений, повторных внешних эффектов и потери состояния, но слишком быстро переносила эти требования на любой workflow. Обязательная цепочка `IR → compiler → type checking → static analysis → simulation → pilot → run` была оправдана только для малой части задач: долгих автоматизаций, внешних side effects, повторяемых бизнес-процессов, дорогих массовых операций и процессов с ожиданием событий.

Для обычного проектного агента такая схема действительно стала бы бюрократией и ограничила бы адаптивность сильной модели.

### Ошибка 2: Task Registry охватывал слишком мелкую работу

First-class Task нужна только тогда, когда работа должна пережить turn или restart, стать видимой пользователю, ждать события, иметь отдельного owner, бюджет, внешний effect, независимый artifact либо возможность отмены и продолжения.

Внутренний шаг рассуждения, чтение файла, короткий вызов subagent или локальная проверка не должны превращаться в Jira-карточку и проходить десяток состояний.

### Ошибка 3: межагентный протокол был слишком жёстким

Canonical state действительно нельзя хранить только в чате, однако это не означает, что каждое сообщение должно быть классифицировано как `assignment | clarification | challenge | correction`. Сильным агентам полезен свободный естественный контекст. Структура нужна в транспортном envelope и для машинно значимых команд, а не вместо человеческого языка.

### Ошибка 4: multi-agent и review были недостаточно жёстко ограничены стоимостью

Версия 1.0 называла single-agent default, но далее описывала достаточно много topology и reviewers, что могло подтолкнуть реализацию к чрезмерному их использованию. При равном reasoning-token budget один сильный агент способен превосходить multi-agent системы на задачах, требующих единого контекста; coordination, repeated context и synthesis создают значительную цену. [[S04]]

### Ошибка 5: декомпозиция могла разрушить цельность задачи

Подзадача полезна только когда имеет самостоятельный результат и границу. Если каждый worker нуждается почти во всей картине, меняет общие файлы или должен понимать единую эстетическую/архитектурную идею, разделение создаёт context fragmentation и заставляет lead повторять всю работу.

### Ошибка 6: качество рассматривалось отдельно от практической цены

Несколько процентов выигрыша не являются улучшением, если они требуют кратно больше времени, токенов, provider limits и пользовательского ожидания. Dennett должен оптимизировать не максимальный benchmark score, а **стоимость успешного и пригодного результата**.

## 0.2. Что из версии 1.0 сохраняется

Сохраняются важные инварианты:

- пользовательский project chat остаётся базовым режимом;
- один сильный агент является default;
- runtime, а не prompt, хранит значимое долговременное состояние;
- внешние side effects требуют idempotency или reconciliation;
- permissions не передаются текстом;
- mutable resource имеет owner;
- результат проверяется по среде, а не только по словам агента;
- память входит через Context Manifest и stable handles;
- процессы можно остановить;
- provider-native agent loops и tools используются вместо бессмысленного самописания;
- сложность принимается только после eval;
- workflow и multi-agent остаются доступными, но перестают быть обязательной формой серьёзной работы.

## 0.3. Новая центральная модель

Dennett использует лестницу минимально достаточной оркестрации.

### Уровень 0 — Direct Turn

Один ответ или короткая операция в текущей project/voice/orchestrator session. Нет отдельной Task, workflow или subagent.

### Уровень 1 — Adaptive Agent Session

Один сильный агент сам планирует, использует tools, меняет стратегию и ведёт bounded working state. Это основной режим разработки, исследования, дизайна и общего решения задач.

### Уровень 2 — Managed Run

Вокруг агента появляется лёгкий durable envelope: Task ID, статус, бюджет, отмена, checkpoint, external-effect ledger и Result Envelope. Стратегия по-прежнему принадлежит агенту.

### Уровень 3 — Structured Automation

Появляется явная последовательность этапов, durable waits, параллельные shards, gates или повторяемый procedure. Строгость пропорциональна риску. Только этот уровень может требовать static validation, simulation или pilot.

Главное правило:

> **Dennett всегда начинает с самого простого уровня, способного надёжно решить задачу, и повышает уровень только при наблюдаемой необходимости.**

Anthropic рекомендует начинать с простейшего решения, учитывать обмен качества на latency/cost и усложнять систему только при доказанном выигрыше. [[S03]]

## 0.4. Новая формула

> **Dennett Pragmatic Agentic Control Fabric = strong-agent-first execution + lightweight durable control when needed + selective multi-agent delegation + optional structured automation + user-configurable oversight + evidence-based completion + cost-aware evaluation.**

---

# 1. Область документа

## 1.1. Что описывается

Документ определяет:

- источник и конфигурацию агентов;
- главный оркестратор;
- project sessions;
- критерии появления Task;
- Agent Run и Managed Run;
- Strategy Selector;
- декомпозицию и сохранение цельности;
- subagents и teams;
- минимальный протокол сообщений;
- review;
- procedures, skills и structured automations;
- права, ресурсы и внешние эффекты в контексте agents;
- связь с Memory Fabric;
- пользовательские профили свободы и контроля;
- observability, eval и самоулучшение.

## 1.2. Что вынесено в другие документы

- долговременная память определяется Memory Fabric 1.2;
- глобальные identity, security и authorization будут определены отдельным документом;
- голосовой UX и мыслительный слой — будущим Voice document;
- providers, MCP, skills и computer-use как каталог возможностей — будущим Capability document;
- физический server runtime и sync — будущим Server document;
- кнопки desktop/mobile — соответствующими UI-документами.

## 1.3. Нормативное отношение к версии 1.0

Если версия 1.0 требует formal Task, typed message или compiled workflow там, где 1.1 допускает Direct Turn, Adaptive Agent Session или свободное сообщение, действует версия 1.1.

Durability, authorization и external-effect guarantees 1.0 сохраняются для Managed Runs и Structured Automations.

---

# 2. Исследовательский протокол

## 2.1. Гипотезы, проверенные повторно

1. Сильный агент часто лучше фиксированного workflow.
2. Multi-agent часто выигрывает за счёт большего compute, а не архитектуры.
3. Чрезмерная декомпозиция теряет глобальный замысел.
4. Typed messages полезны runtime, но могут мешать смысловой коммуникации.
5. Полная state machine нужна только durable work.
6. Пользователь должен регулировать свободу, review и стоимость.
7. Небольшой прирост качества может не окупить coordination tax.
8. Workflow Studio может оказаться дорогой функцией с низкой реальной частотой использования.

## 2.2. Базовые сравнения

Для каждого класса задач сравниваются:

- одна model call;
- один augmented agent;
- agent + skill;
- agent + lightweight run envelope;
- agent + bounded subagent;
- agent + independent reviewer;
- managed procedure;
- structured automation;
- multi-agent team.

Новый механизм принимается только если он улучшает хотя бы одно из следующего без непропорционального ущерба остальным:

- task success;
- correctness;
- latency;
- token/provider consumption;
- user effort;
- recoverability;
- risk reduction;
- repeatability;
- maintainability.

## 2.3. Cost-of-success

Основная эксплуатационная метрика:

```text
cost_of_success =
    total_model_and_tool_cost
  + latency_penalty
  + coordination_overhead
  + user_interruption_cost
  + failure_and_rework_cost
```

Система не обязана выбирать самый дешёвый вариант, но должна оставаться на Pareto frontier. Efficient Agents показывает, что близкое качество может достигаться значительно меньшей стоимостью при согласовании сложности с задачей. [[S06]]

## 2.4. Критерий принятия multi-agent

Team принимается, только если:

- существует несколько действительно независимых единиц результата;
- workers не нуждаются в полном общем mutable context;
- параллельность или разнообразие даёт измеримый выигрыш;
- lead способен принять результаты без полного повторения работы;
- coordination tax помещается в budget;
- есть объективный merge/evaluation mechanism.

## 2.5. Критерий отказа от декомпозиции

Работа остаётся у одного агента, если:

- подзадачи разделяют большую часть контекста;
- нужен единый стиль, архитектурный замысел или причинная линия;
- изменения затрагивают одну и ту же mutable область;
- output одной части невозможно оценить отдельно;
- синтез требует повторного чтения всей исходной информации;
- coordination дороже последовательного выполнения;
- задача ещё недостаточно понята для корректного разбиения.

## 2.6. Критерий принятия structured automation

Structured Automation оправдана, если есть хотя бы один сильный фактор:

- длительное ожидание или resume после restart;
- повторяемый процесс;
- внешние side effects;
- независимые массовые shards;
- жёсткий compliance/order invariant;
- дорогой run, который стоит предварительно проверить;
- многократное использование одной и той же последовательности;
- человеческое требование наблюдать конкретные этапы.

Если задача является открытым исследованием, разработкой или творческой работой с непредсказуемой стратегией, default остаётся agent-controlled.

## 2.7. Ограничение доказательности

Одна свежая работа 2026 года показала превосходство in-context procedure над внешним LangGraph orchestration в трёх procedural domains. Это сильный сигнал против безусловной оркестрации, но не доказательство ненужности durable workflows вообще. [[S07]]

Решения Dennett принимаются по собственным ablations и реальным project trajectories.

---

# 3. Неподвижные инварианты

1. Project agent остаётся прямым собеседником пользователя.
2. Один сильный агент является baseline.
3. Микрошаг не обязан становиться Task.
4. Task создаётся только при наличии lifecycle value.
5. Agent может свободно менять план внутри выданных границ.
6. Runtime не диктует смысловую декомпозицию без причины.
7. Canonical state не хранится только в model context.
8. Free-form agent communication разрешена.
9. Permissions не передаются сообщением.
10. External effect idempotent, staged или reconcilable.
11. Completion опирается на environment evidence.
12. Multi-agent complexity имеет бюджет и критерий остановки.
13. Decomposition сохраняет global goal и invariants.
14. Workflow strictness пропорциональна риску и повторяемости.
15. User может выбирать уровень свободы и контроля.
16. Non-negotiable safety не отключается ползунком свободы.
17. Любой дополнительный agent, reviewer и gate должен оправдывать стоимость.
18. Provider-native session и tools используются, если они достаточны.
19. Hidden chain-of-thought не является контрактом.
20. Любой процесс имеет stop path и понятный owner.

---

# 4. Модель выполнения

## 4.1. Direct Turn

Подходит, когда работа:

- завершается в текущем turn;
- не требует отдельного ожидания;
- не должна пережить restart;
- не имеет значимого внешнего effect;
- не требует отдельного owner или Radar item.

Примеры:

- прочитать статус проекта;
- объяснить файл;
- выполнить маленькую правку в текущем project chat;
- классифицировать событие;
- сформировать короткий ответ.

Runtime сохраняет обычные model/tool events, но не создаёт полноценную Task.

## 4.2. Adaptive Agent Session

Это основной рабочий режим.

Agent получает:

- цель текущего turn или session;
- Context Manifest;
- tools;
- права;
- рабочую директорию;
- budget/stop conditions;
- возможность запросить память и создать bounded subagent.

Agent сам:

- исследует среду;
- строит и меняет план;
- выбирает tools;
- выполняет несколько действий;
- проверяет промежуточный результат;
- возвращается к пользователю.

Внутренний план может быть свободным текстом или checklist и не обязан становиться workflow graph.

## 4.3. Managed Run

Создаётся, если работа должна стать устойчивой и наблюдаемой.

Managed Run добавляет только необходимое:

- stable run/task ID;
- owner;
- цель;
- status;
- budget;
- cancel/pause;
- checkpoint summary;
- external-effect ledger;
- Result Envelope;
- связь с памятью и Radar.

Agent по-прежнему контролирует стратегию.

## 4.4. Structured Automation

Это опциональная надстройка для процессов, где последовательность и границы сами являются продуктом.

Structured Automation может содержать:

- этапы;
- waits;
- approvals;
- deterministic activities;
- agent phases;
- parallel shards;
- merge point;
- compensation;
- completion rules.

Но внутри agent phase модель остаётся адаптивной и может менять локальный план.

---

# 5. Основные сущности

## 5.1. Project

Постоянная рабочая область вокруг папки, репозитория или набора материалов.

## 5.2. Project Session

Прямой чат пользователя с выбранным project agent. Это не Task Registry и не workflow. Session сохраняет рабочий контекст, но долгосрочная continuity обеспечивается Memory Fabric.

## 5.3. Work Item

Лёгкое обозначение локальной единицы работы внутри agent session. Может существовать только в working state агента. Не получает глобальный ID и state machine по умолчанию.

## 5.4. Task

Durable обязательство получить результат. Создаётся только по promotion criteria.

## 5.5. Run

Конкретное выполнение Task или background operation.

## 5.6. Agent Definition

Переиспользуемая роль, инструкции, model/provider policy и capabilities.

## 5.7. Agent Instance

Конкретный логический участник Run или Session.

## 5.8. Provider Session

Нативная сессия Codex, Claude Code, Agents SDK, локальной модели или другого runtime.

## 5.9. Procedure / Skill

Человекочитаемая повторно используемая guidance. Agent применяет её адаптивно, а не воспроизводит как жёсткий граф.

## 5.10. Automation Definition

Опциональное долговечное описание этапов для Structured Automation.

## 5.11. Artifact

Результат, которым можно независимо оперировать: patch, документ, исследование, dataset, report, image, workflow definition.

## 5.12. Result Envelope

Краткий итог Run:

- outcome;
- summary;
- artifacts;
- evidence;
- verification;
- unresolved;
- next options;
- cost/latency summary при необходимости.

---

# 6. Когда Work Item становится Task

## 6.1. Promotion criteria

Work Item повышается до Task, если истинно хотя бы одно:

- выполнение выходит за текущий turn/session;
- процесс должен пережить restart;
- появляется независимый agent owner;
- пользователь должен видеть его в Radar;
- нужно ждать пользователя, событие, время или dependency;
- имеется внешний side effect;
- нужен отдельный budget/deadline;
- результат должен быть independently reviewed;
- требуется pause/resume/cancel;
- работа создаёт самостоятельный artifact;
- оркестратор должен продолжить её без project agent.

## 6.2. Что не становится Task

- каждый tool call;
- каждый пункт внутреннего плана;
- чтение файла;
- короткий поиск;
- локальный reviewer call внутри одного turn;
- внутреннее уточнение между parent и child;
- промежуточный draft;
- одна итерация рассуждения.

## 6.3. Минимальная state model

```text
QUEUED
→ RUNNING
↔ WAITING
→ COMPLETED | PARTIAL | FAILED | CANCELLED
```

`WAITING` имеет reason:

- user_input;
- authorization;
- dependency;
- time/event;
- resource;
- provider;
- external_callback.

`planning`, `executing`, `verifying`, `reviewing`, `recovering` являются phase/telemetry, а не обязательными top-level states.

## 6.4. Расширенная state model

Дополнительные состояния допускаются только для конкретного runtime или внешнего protocol adapter. Например, A2A использует submitted, working, input-required, auth-required и terminal states. На внутренней модели Dennett они отображаются без необходимости копировать весь protocol во все микрозадачи. [[S10]]

## 6.5. Terminal history

Terminal outcome не переписывается. Продолжение создаёт новый Run или Task relation, но UI может показывать их как одну цепочку.

---

# 7. Главный оркестратор

## 7.1. Логическая роль

Главный оркестратор — единственный глобальный control authority, но не одна бессмертная модель и не участник каждого действия.

Он отвечает за:

- intake намерений и событий;
- выбор уровня выполнения;
- создание Task/Run/Automation при необходимости;
- межпроектные решения;
- provider и resource budgets;
- permission escalation;
- proactive behavior;
- исключения и аварийные остановки;
- обработку результатов;
- связь с Action Inbox, Radar и Memory Fabric.

## 7.2. Что он не делает

Он не обязан:

- читать каждый файл проекта;
- участвовать в каждом project turn;
- подтверждать каждый tool call;
- перепланировать работу сильного project agent;
- хранить всю историю в prompt;
- создавать team ради видимости деятельности.

## 7.3. Intake

Вход может быть:

- пользовательский текст/голос;
- событие;
- сообщение;
- request project agent;
- completion другого Run;
- memory opportunity;
- system health signal.

Оркестратор формирует Intent Record и сначала рассматривает `no-op`, `reply`, `delegate to existing session`, `managed run`, `automation`.

## 7.4. No-op как нормальный результат

Система может:

- ничего не делать;
- только записать;
- отложить;
- продолжить наблюдение;
- подготовить черновик;
- уведомить позже.

Наличие события не означает необходимость нового агента.

## 7.5. Lightweight exoskeleton

Для повторяющихся инвариантов вокруг оркестратора допустим deterministic harness:

1. собрать identity, permissions, active project и current state;
2. выполнить дешёвые preflight checks;
3. дать модели принять смысловое решение;
4. проверить external effects и собрать evidence/result.

Практический Exoskeleton-кейс показывает ценность переноса повторяющихся ошибок из prompt в code, но не требует превращать всю работу в детерминированный граф. [[S08]]

---

# 8. Project agent и обычный чат

## 8.1. Базовая логика

Project agent работает как Codex App или Claude Code:

```text
пользователь ↔ project agent ↔ project directory/tools
```

Он может:

- читать и менять файлы;
- запускать команды;
- исследовать;
- обсуждать решение;
- сохранять project memory;
- вызывать bounded tools/subagents;
- возвращать artifacts;
- просить оркестратор о внешнем доступе.

## 8.2. Никакого обязательного workflow после каждого prompt

Обычная правка или обсуждение остаётся внутри project session. Run создаётся только по promotion criteria.

## 8.3. Переход в фон

Agent предлагает Managed Run, когда:

- работа долгая;
- пользователь хочет уйти;
- нужен background execution;
- есть ожидание;
- требуется отдельный budget;
- полезно наблюдение в Radar.

## 8.4. Возврат в чат

Background result возвращается как Result Envelope и artifacts, а не как весь transcript. Project session получает актуальное состояние и может продолжить разговор.

## 8.5. Оркестратор и project agent

Оркестратор может:

- запросить статус;
- передать пользовательскую directive;
- создать отдельный Run;
- выдать разрешение;
- поставить событие;
- остановить процесс.

Он не переписывает локальный план project agent без причины. Если директива конфликтует с текущей работой, project agent или оркестратор решает: применить сейчас, поставить после checkpoint, создать отдельный branch или спросить пользователя.

## 8.6. Протокол директив

Directive от пользователя или оркестратора имеет смысловое содержание, urgency и desired timing, но project agent сохраняет право сообщить о конфликте с текущим состоянием.

Варианты обработки:

- выполнить немедленно;
- включить в текущий plan;
- выполнить после checkpoint;
- создать parallel branch, если ресурсы независимы;
- превратить в Managed Run;
- запросить уточнение;
- отклонить как противоречащую более новой explicit user instruction.

## 8.7. Статус проекта без отвлечения агента

Оркестратор сначала читает runtime, artifacts, commits, tests и memory projections. Он обращается к project agent только если наблюдаемых данных недостаточно. Это снижает лишние model calls.

---

# 9. Provider Adapter Layer

## 9.1. Использовать готовых агентов

Dennett предпочитает provider-native runtime, если он уже даёт:

- tool loop;
- sessions;
- subagents;
- checkpoints;
- permissions;
- hooks;
- cost/usage;
- computer-use;
- worktrees;
- native continuation.

## 9.2. Adapter contract

Adapter должен нормализовать минимум:

- start/resume/stop session;
- send user/system/context;
- tool and permission events;
- usage;
- artifacts;
- checkpoint/continuation handle;
- error/unknown state;
- child-agent visibility, если доступна.

## 9.3. Не flatten provider advantages

Если Claude Code умеет нативно вести session или Codex эффективно управляет tool calls, Dennett не заставляет его имитировать универсальный `NextStep` JSON.

## 9.4. Provider session не является Task

Сессию можно перезапустить или сменить, сохранив Task/Run и Memory Context.

## 9.5. Минимальный lifecycle значимого Agent Instance

Только Agent Instance, который имеет самостоятельный Run, отдельный ресурсный scope или видим пользователю, получает lifecycle:

```text
STARTING → ACTIVE ↔ WAITING → COMPLETED | FAILED | CANCELLED
```

`WAITING` хранит reason. `RECOVERING` и `VERIFYING` являются phases. Микро-вызов модели внутри parent session не получает отдельную state machine.

При старте Dennett фиксирует assignment, context manifest, provider session, budget и owner. При завершении сохраняет result, artifacts, evidence и stop reason.

## 9.6. Выбор модели

Пользовательская модель project chat не меняется самовольно. Для внутренних agents Selector может выбрать другую модель, если:

- пользователь не запретил;
- роль действительно ограничена;
- provider/model имеет нужные tools;
- ожидаемый выигрыш оправдывает переключение;
- privacy и memory scope допускают provider;
- лимиты подписки учтены.

Дешёвая модель используется для bounded classification/formatting только при наличии fallback: её ошибка не должна уничтожить основной Run.

## 9.7. Delegation и handoff

**Delegation** означает: parent сохраняет ownership, child возвращает bounded result.

**Handoff** означает: другой agent становится owner дальнейшего диалога или Run. Handoff применяется редко — когда specialist должен самостоятельно продолжать работу, а не просто дать материал.

Handoff передаёт:

- global intent capsule;
- current state summary;
- artifacts/evidence;
- unresolved questions;
- permission envelope;
- условие возврата.

Он не передаёт скрытые permissions и не требует копирования всего transcript.

---

# 10. Context и Memory Fabric

## 10.1. Context Manifest

Agent получает manifest, а не случайный dump:

```text
[RUNTIME / SAFETY]
[GLOBAL GOAL AND USER INTENT]
[LOCAL ASSIGNMENT]
[EFFECTIVE PROJECT INSTRUCTIONS]
[CURRENT PROJECT / ENVIRONMENT STATE]
[RELEVANT MEMORY AND EVIDENCE]
[AVAILABLE TOOLS / PERMISSIONS]
[OUTPUT OR COMPLETION EXPECTATION]
[UNCERTAINTY / CONFLICTS]
[STABLE HANDLES]
```

## 10.2. Global Intent Capsule

Каждый значимый child agent получает короткий неизменяемый capsule:

- общая цель;
- почему его часть нужна;
- project invariants;
- non-goals;
- ключевые ограничения;
- кто интегрирует результат.

Это защищает от локально правильного, но глобально бессмысленного результата.

## 10.3. Local Context

Дополнительно child получает только свою область:

- source handles;
- files/resources;
- expected artifact;
- relevant decisions;
- evidence requirements.

## 10.4. Read-more вместо полного дублирования

Если child обнаруживает, что context недостаточен, он вызывает memory/project search или просит parent, а не получает всю историю заранее.

## 10.5. Memory commit

После значимой границы записываются:

- intent;
- decisions;
- actions;
- artifacts;
- verification;
- outcome;
- feedback;
- reusable lesson candidates.

Не сохраняется весь скрытый scratchpad.

---

# 11. Strategy Selector

## 11.1. Начинать с baseline

Порядок кандидатов:

1. direct answer/tool call;
2. one augmented agent;
3. agent + skill/procedure;
4. managed run;
5. one bounded subagent;
6. independent reviewer;
7. parallel agents;
8. structured automation.

Selector не перепрыгивает на более сложный вариант без причины.

## 11.2. Основные признаки

- uncertainty;
- duration;
- external effects;
- need for resume;
- context coupling;
- decomposability;
- shared mutable state;
- verification availability;
- diversity value;
- latency budget;
- token/provider budget;
- user control profile;
- repetition frequency.

## 11.3. Coordination tax

Перед spawn оценивается:

- context preparation;
- repeated input tokens;
- communication;
- waiting;
- synthesis;
- duplicate work;
- merge/review;
- failure attribution.

## 11.4. Marginal agent utility

Новый agent добавляется только если ожидаемая дополнительная ценность выше coordination tax.

```text
marginal_utility =
    expected_quality_gain
  + expected_latency_gain
  + risk_reduction
  - token_cost
  - coordination_cost
  - coherence_risk
  - merge_cost
```

## 11.5. Ограничения количества

По умолчанию:

- `1` main agent;
- `0–2` bounded helpers;
- независимый reviewer только по триггеру риска/неопределённости;
- большие teams требуют явного budget или user profile.

Числа являются default policy, а не фундаментальным лимитом.

---

# 12. Декомпозиция и цельность

## 12.1. Minimum Coherent Unit

Подзадача должна быть минимальной **самостоятельно осмысленной**, а не минимальной технически возможной.

Хорошая подзадача имеет:

- собственный вопрос;
- самостоятельный artifact/evidence;
- ясную границу;
- независимую проверку;
- понятного consumer.

## 12.2. Context-coupling test

Не разделять, если:

- child нужен почти весь контекст parent;
- решения одной части постоянно меняют другую;
- существует единый style/design/architecture voice;
- результат нельзя оценить без всей системы;
- интегратор должен заново пройти исходные рассуждения.

## 12.3. Сохранение общей идеи

Lead остаётся owner глобального замысла. Workers не принимают project-wide решения без возврата к lead.

## 12.4. Подходы вместо fragments

Для творческих и исследовательских задач лучше параллелить независимые **варианты целостного решения**, а не разрезать один замысел на несвязанные куски.

Например, три agents могут предложить три архитектуры целиком. Это часто лучше, чем поручить одному API, второму данные, третьему UI до появления общей архитектуры.

## 12.5. Dynamic decomposition

Agent может начать один, исследовать задачу и только после появления естественных границ создать children. Не требуется заранее строить полный DAG.

## 12.6. Skill-aware decomposition

Если нужны конкретные skills/tools, разбиение учитывает реальные возможности. Исследования compositional skill routing показывают, что неправильная granularity является главным bottleneck декомпозиции. [[S18]]

---

# 13. Subagents

## 13.1. Когда subagent полезен

- независимый поиск источников;
- isolated experiment;
- ограниченная проверка;
- tool specialization;
- сбор данных;
- независимый вариант;
- adversarial review важного diff;
- работа в отдельном resource shard.

## 13.2. Когда subagent вреден

- parent должен передать всю историю;
- output не имеет самостоятельной ценности;
- child меняет тот же mutable state;
- parent всё равно повторит работу;
- работа занимает меньше времени, чем подготовка assignment;
- agent создаётся только потому, что существует роль.

## 13.3. Assignment

Assignment может быть естественным текстом, но обязательно содержит по смыслу:

- objective;
- global intent capsule;
- boundaries;
- inputs/handles;
- expected result;
- stop condition;
- consumer;
- permission scope.

Не требуется длинный YAML для каждого микроагента.

## 13.4. Возврат результата

Child возвращает:

- короткий summary;
- artifacts/handles;
- evidence;
- unresolved/limits.

Свободное объяснение разрешено. Structured fields обязательны только для того, что runtime должен обработать автоматически.

## 13.5. Spawn depth

Default — один уровень children. Более глубокая иерархия допускается только в большой независимой структуре работы и при явном budget.

---

# 14. Multi-agent teams

## 14.1. Team не является свойством проекта

Team создаётся на конкретную phase/Run и распускается после неё.

## 14.2. Полезные topology

- lead + independent researchers;
- independent attempts + selector;
- implementer + independent reviewer;
- bounded specialists;
- sharded workers + one integrator;
- debate/council для вариантов, если есть judge criteria.

## 14.3. Не использовать team для

- последовательной работы с одним context;
- одной небольшой code change;
- project-wide design без отдельного lead;
- рутинной проверки каждого ответа;
- создания видимости автономности.

## 14.4. Single-agent evidence

При равном token budget single-agent системы последовательно соответствовали или превосходили MAS на multi-hop reasoning; выгоды MAS часто отражают больше test-time compute или компенсацию плохого использования единого контекста. [[S04]]

## 14.5. Failure evidence

Анализ пяти MAS frameworks выявил specification failures, inter-agent misalignment и verification/termination failures. Улучшение одних role prompts не устраняло проблему полностью. [[S05]]

## 14.6. Budget-aware teams

Team имеет:

- max agents;
- max parallelism;
- total token/provider budget;
- merge capacity;
- stop rule;
- duplication monitor.

## 14.7. Сокращение team

Если workers начинают дублировать находки, конфликтовать или давать diminishing returns, lead прекращает spawn и завершает текущие branches.

---

# 15. Межагентное общение

## 15.1. Свободный смысл, минимальный transport envelope

```yaml
message:
  id: id
  sender: ref
  recipient: ref_or_group
  context_id: id
  task_or_run_id: optional
  content: freeform_text_or_parts
  artifact_handles: []
  reply_to: optional
  urgency: optional
  expires_at: optional
  created_at: time
```

## 15.2. Optional machine intent

Для cancel, permission request, artifact ready, handoff или heartbeat может добавляться machine-readable `intent`. Обычное обсуждение не обязано иметь enum.

## 15.3. Canonical state отдельно

Сообщение не является:

- разрешением;
- владельцем ресурса;
- фактом завершения;
- единственным местом хранения artifact;
- заменой Task/Run state.

## 15.4. Peer-to-peer

Разрешено, если agents действительно должны уточнять общий предмет. Hub-and-spoke остаётся default не из-за слабости моделей, а чтобы lead сохранял цельность и budget.

## 15.5. Artifact-first для больших данных

Большие reports/diffs/datasets передаются handles. Но child может приложить свободное объяснение, чтобы не потерять смысл.

---

# 16. Review и контроль качества

## 16.1. Review не является default после каждого turn

Project agent сам проверяет обычные изменения через tests/tools/environment.

## 16.2. Независимый reviewer нужен, когда

- цена ошибки высока;
- author bias важен;
- результат проверяем, но не полностью покрыт автоматическими tests;
- большой diff;
- безопасность;
- архитектурное решение;
- user profile `Rigorous`;
- прошлые failure traces показывают пользу.

## 16.3. Reviewer должен быть ограничен

Он получает artifact, task intent, invariants и rubric, но не обязан видеть весь reasoning автора. Bun использовал отдельные context windows для adversarial review и обнаружил правдоподобные компилирующиеся ошибки; этот паттерн полезен для больших изменений, но слишком дорог для каждой мелочи. [[S09]]

## 16.4. Reviewer не управляет бесконечным циклом

Default:

- одна review pass;
- одна rework pass;
- повтор только при blocking finding.

## 16.5. Code/automatic verification first

Если compiler, tests, schema validator или authoritative source могут проверить результат, они применяются раньше дополнительной модели.

---

# 17. Procedures, skills и automations

## 17.1. Procedure/Skill — основной способ повторяемой работы

Большинство «workflow» следует начинать как Markdown skill или prompt recipe:

- цель;
- рекомендации;
- примеры;
- tools;
- критерии результата;
- common failures.

Сильный agent адаптирует шаги к ситуации.

## 17.2. Adaptive Run Plan

Agent может создать временный план:

```text
- выяснить состояние
- проверить два подхода
- выбрать
- реализовать
- проверить
```

План можно показать пользователю и редактировать, но он не обязан быть executable graph.

## 17.3. Managed Procedure

Если procedure запускается в фоне, Dennett оборачивает её в Managed Run с checkpoints, budget и cancel. Стратегия остаётся у агента.

## 17.4. Structured Automation

Только для процессов с реальными operational constraints.

Минимальный набор примитивов:

- agent phase;
- deterministic action;
- wait/event;
- approval;
- parallel shard;
- join/integrate;
- checkpoint;
- terminal outcome.

Не требуется заранее создавать десятки node types.

## 17.5. Proportional validation

### Низкий риск

- проверить, что tools существуют;
- показать краткий план;
- запустить.

### Средний риск/дорогой run

- оценить budget;
- preview external actions;
- small sample/pilot при необходимости.

### Высокий риск или массовое изменение

- static effect analysis;
- simulation/dry run;
- representative pilot;
- explicit approval;
- rollback/compensation plan.

Ни simulation, ни pilot не являются обязательными для обычной procedure.

## 17.6. Visual builder

Визуальный редактор является optional поздней функцией. Он должен:

- показывать Managed Runs и Structured Automations;
- редактировать простую последовательность;
- генерировать её из естественного языка;
- позволять перейти обратно к agent chat;
- отображать actual trace.

Он не должен требовать создания compiler platform до появления реального пользовательского спроса.

## 17.7. Trace-to-procedure

Повторяющийся успешный pattern может стать skill. Только если нужна durability/operational order, он становится Structured Automation.

## 17.8. Evidence against universal orchestration

Контролируемое исследование procedural tasks показало, что полный procedure в system prompt с self-orchestration мог превосходить LangGraph при той же модели. Это подтверждает необходимость skill-first baseline. [[S07]]

## 17.9. Durable engines остаются полезны

Temporal-подобный runtime нужен, когда process должен переживать outages и безопасно повторять activities. Эти свойства относятся к durability, а не к смысловой стратегии агента. [[S11]]

---

# 18. Resource ownership и параллельность

## 18.1. Ownership включается только при конкуренции

Один agent в project session не нуждается в lease на каждый файл.

Lease/partition нужен, когда несколько actors действительно могут менять один ресурс одновременно.

## 18.2. Кодовые shards

Для parallel code work:

- separate worktree/branch или непересекающиеся file sets;
- один integration owner;
- запрет destructive shared Git operations;
- tests после merge.

Bun столкнулся с конфликтами `stash`, `stash pop` и `reset`, после чего ограничил команды и использовал четыре worktree-shard; это практическое доказательство необходимости ownership именно при массовом parallelism. [[S09]]

## 18.3. Research

Researchers обычно владеют source/query partitions, а не общими mutable files. Lead агрегирует evidence.

## 18.4. Effect ownership

Платёж, отправка, публикация или удаление имеют stable logical action ID и одного effect owner.

---

# 19. Надёжность

## 19.1. Надёжность proportional

Direct Turn не требует event-sourced workflow history. Managed Run требует enough state для resume. Structured Automation может требовать полную durable event history.

## 19.2. Checkpoint

Managed Run checkpoint содержит:

- goal;
- current state summary;
- completed logical actions;
- artifacts/evidence;
- pending actions;
- external effects;
- continuation handle/session summary.

## 19.3. Retry

Различаются:

- transport retry;
- tool retry;
- provider session restart;
- strategy retry;
- new task after goal change.

## 19.4. External effect

Если ответ потерян после внешнего действия, состояние считается `unknown` до reconciliation. Повтор не выполняется вслепую.

Практические issue reports CrewAI показывают реальную опасность повторного запуска tools при retry, включая duplicate payments/emails. [[S20]]

## 19.5. Cancellation

- запрет новых effects;
- signal agent/provider;
- сохранить checkpoint;
- cleanup;
- проверить фактическое состояние;
- показать пользователю итог.

## 19.6. No-progress

Сигналы:

- повтор одних tools;
- нет новых evidence/artifact;
- oscillating plan;
- budget burn;
- повторяющиеся ошибки.

Recovery сначала простое: дать недостающий контекст, сузить задачу, сменить tool или спросить. Reviewer/team — не первая реакция.

---

# 20. Пользовательское управление свободой и контролем

## 20.1. Нельзя использовать один глобальный ползунок для всего

Свобода агента, риск внешнего действия, глубина review и число agents — разные измерения. UI может показывать presets, но внутри они настраиваются отдельно.

## 20.2. Presets

### Direct

- один agent;
- минимум planning UI;
- без automatic reviewer;
- низкий interruption rate;
- пользователь ведёт процесс сам.

### Balanced — default

- solo-first;
- bounded subagents по необходимости;
- обычные environment checks;
- Managed Run для фоновой работы;
- targeted review.

### Independent

- agent может сам уходить в фон, создавать bounded helpers и менять план;
- меньше вопросов;
- установлен budget и hard safety limits.

### Rigorous

- больше checkpoints;
- independent review важных artifacts;
- preview дорогих/массовых действий;
- более строгие completion requirements.

### Exploratory

- допускает несколько независимых hypotheses/attempts;
- повышенный budget;
- не используется для irreversible actions без отдельного gate.

## 20.3. Отдельные параметры

- autonomy inside scope;
- max agents;
- max parallelism;
- review depth;
- automation strictness;
- token/provider budget;
- latency budget;
- user interruption threshold;
- plan visibility;
- memory breadth;
- external-action confirmation.

## 20.4. Scope настроек

Настройка может применяться к:

- системе;
- проекту;
- конкретному чату;
- Task/Run;
- workflow/procedure;
- типу действия.

## 20.5. Safety floor

Preset не может отключить:

- ownership/identity checks;
- запрет чужих dangerous commands;
- payment confirmation policy;
- secret disclosure limits;
- reconciliation неизвестного effect;
- emergency stop.

## 20.6. Почему adjustable control важен

Исследования автономности показывают, что intermediate и context-sensitive autonomy может лучше сохранять доверие и чувство контроля, чем как полное отсутствие, так и полная автономия; high-risk tasks требуют большего user control. [[S14]] [[S15]] [[S16]]

---

# 21. Практические patterns

## 21.1. Обычная code change

```text
project chat
→ один agent исследует
→ меняет
→ запускает focused tests
→ показывает diff/result
```

Subagent и workflow не нужны.

## 21.2. Долгая code change в фоне

```text
project agent формирует Managed Run
→ продолжает один
→ checkpoints
→ при естественной независимой проверке вызывает reviewer
→ Result Envelope в чат
```

## 21.3. Большая migration

```text
один lead создаёт общий guide и pilot
→ representative sample
→ только после успеха shards
→ workers в отдельных worktrees
→ independent review пропорционально риску
→ один integrator
→ full tests
```

Bun сначала подготовил porting guide и trial на трёх файлах, затем масштабировал; однако десятки reviewers и workers оправданы размером million-line migration, а не являются default Dennett. [[S09]]

## 21.4. Исследование

Исследование — не отдельная жёсткая подсистема. Это adaptive project/skill/preset.

Default:

```text
один research agent
→ формулирует вопрос
→ ищет и проверяет источники
→ ведёт evidence в Research Memory Space
→ синтезирует
```

Parallel researchers добавляются только если тема имеет независимые направления и breadth выигрывает от параллелизма.

Структура research result задаётся skill/prompt и может меняться под задачу.

## 21.5. Несколько вариантов решения

Лучше запустить 2–3 независимых цельных attempts, чем дробить один дизайн на мелкие части. Selector сравнивает artifacts по rubric и evidence.

## 21.6. Monitoring

Repeated event-driven process может сначала быть Procedure + schedule. Structured Automation нужна, если появляются waits, side effects, multiple stages или recovery requirements.

---

# 22. Outcome, evidence и completion

## 22.1. Agent report не равен завершению

Agent предлагает result. Runtime проверяет только то, что действительно проверяемо и нужно по profile.

## 22.2. Proportional completion

### Direct Turn

Достаточно tool result или обычного ответа.

### Managed Run

Проверяются expected artifacts, существенные external effects и базовые success conditions.

### Rigorous/Structured

Проверяются tests, evidence, gates, unresolved blockers и required review.

## 22.3. Не создавать фиктивную точность

Если качество нельзя объективно проверить, Dennett не должен строить огромный completion contract. Он возвращает ограничения, evidence и uncertainty.

## 22.4. Result Envelope

```yaml
result:
  outcome: completed | partial | failed | cancelled
  summary: text
  artifacts: []
  evidence: []
  verification: []
  unresolved: []
  next_options: []
```

---

# 23. Observability и Radar

## 23.1. Показывать только значимые процессы

Radar не отображает каждый внутренний Work Item. Он показывает:

- project sessions с активной background work;
- Managed Runs;
- Structured Automations;
- significant agents;
- waits;
- approvals;
- risks;
- budgets;
- last meaningful progress.

## 23.2. Agent count как эксплуатационная метрика

Dashboard показывает:

- число активных agents;
- repeated context cost;
- coordination tokens;
- duplicate work;
- merge delay;
- task success.

## 23.3. Query trace

Для проблемного Run можно восстановить:

- почему выбран этот execution level;
- почему создан child;
- какой context он получил;
- что он вернул;
- почему был review;
- что стоило больше всего.

## 23.4. Не наблюдать ради наблюдения

Для Direct Turns достаточно обычных logs. Полная трассировка включается для development, important runs или diagnosis.

---

# 24. Evaluation

## 24.1. Главный baseline

Всегда сравнивать с одним сильным agent + tools + memory.

## 24.2. Метрики

- end-task success;
- correctness;
- cost-of-success;
- wall-clock latency;
- user interventions;
- tokens/provider quota;
- number of agent calls;
- duplication;
- context overlap;
- recovery success;
- false completion;
- user satisfaction.

## 24.3. Multi-agent ablation

Сравнить:

- solo;
- solo + skill;
- solo + one reviewer;
- 2 independent attempts;
- manager-workers;
- full team.

## 24.4. Workflow ablation

Сравнить:

- procedure in context;
- adaptive agent plan;
- Managed Run;
- Structured Automation.

## 24.5. Acceptance rule

Сложность принимается, если она:

- заметно повышает success или снижает существенный риск;
- либо сохраняет качество при ощутимом снижении cost/latency;
- либо даёт обязательную durability/observability, невозможную проще.

Небольшой рост score при кратном росте затрат отклоняется, если пользователь явно не выбрал Rigorous/Exploratory profile.

---

# 25. Самоулучшение

## 25.1. Что можно улучшать автоматически

- skill wording;
- tool description;
- context selection;
- default agent count;
- review trigger;
- model routing;
- retry policy;
- procedure hints.

## 25.2. Что нельзя менять сразу

- safety floor;
- permission model;
- Task promotion criteria;
- external-effect semantics;
- global topology defaults;
- Structured Automation contracts.

## 25.3. Цикл

```text
incident / complaint / high cost
→ attribution
→ simplest candidate fix
→ replay/shadow
→ canary
→ accept or rollback
```

## 25.4. Prefer simplification

Самоулучшение должно уметь удалить лишний reviewer, объединить две подзадачи и заменить workflow обычным skill, а не только добавлять новые layers.

---

# 26. Сквозные сценарии

## 26.1. Пользователь просит маленькую правку

Dennett оставляет её в project session. Task и workflow не создаются.

## 26.2. Пользователь просит сделать большую функцию и уходит

Project agent предлагает Managed Run. Один agent работает с checkpoints. Subagent появляется только для независимого research/test. Результат возвращается в чат.

## 26.3. Агент видит три архитектурных варианта

Он может создать 2–3 независимых attempts с общей задачей и разными hypotheses. Это лучше дробления архитектуры на несвязанные компоненты.

## 26.4. Исследование AI-инструмента

Один research agent использует World Intelligence и web. Если тема широка, создаёт два bounded researchers по независимым направлениям. Итог сохраняется в Research Memory Space. Никакого обязательного research engine.

## 26.5. Еженедельный мониторинг

Сначала schedule + skill. Если позже появляются многоэтапная фильтрация, approvals и внешние действия, процесс повышается до Structured Automation.

## 26.6. Массовая миграция

После guide и small pilot создаётся Structured Automation/sharded Managed Runs. Здесь строгий control оправдан масштабом и merge risk.

## 26.7. User выбирает Direct profile

Dennett минимизирует subagents и review, но сохраняет hard safety и external-effect confirmation.

## 26.8. User выбирает Rigorous profile

Для важного проекта включаются checkpoints, independent review и более строгий completion, но микрошаги всё равно не становятся глобальными Tasks.

---

# 27. Порядок реализации

## Phase 1 — strong-agent core

- project session;
- provider adapters;
- Context Manifest;
- tools/permissions integration;
- Direct Turn;
- basic result/evidence;
- memory read/write.

## Phase 2 — lightweight management

- Task promotion;
- Managed Run;
- pause/cancel;
- checkpoint;
- Radar;
- external-effect ledger;
- Result Envelope.

## Phase 3 — selective collaboration

- bounded subagents;
- independent attempts;
- targeted reviewer;
- context capsules;
- coordination budgets.

## Phase 4 — procedures

- skills/presets;
- natural-language procedure creation;
- trace-to-skill;
- schedules/events.

## Phase 5 — structured automation only after demand

- waits/checkpoints;
- shards/join;
- approvals;
- proportional validation;
- optional visual editor.

Workflow compiler и сложный Studio не являются MVP.

---

# 28. Что намеренно отвергнуто

- Task на каждый внутренний шаг;
- обязательная десятиричная state machine;
- enum для каждого агентного сообщения;
- team как постоянная структура проекта;
- reviewer после каждого ответа;
- максимальное число agents как показатель качества;
- decomposition до атомарных действий;
- fixed YAML graph как default для сильного агента;
- mandatory compile/simulate/pilot для обычной procedure;
- визуальный workflow builder до доказанного спроса;
- единый slider, отключающий безопасность;
- несколько процентов качества любой ценой;
- автоматическое усложнение без удаления старой сложности.

---

# 29. Контракт для следующих документов

Следующие документы должны считать истинным:

- project chat работает agent-first;
- computer-use является capability, а не отдельным agent architecture;
- research задаётся skill/prompt/procedure и при необходимости Managed Run;
- server хранит только значимые Tasks/Runs, а не каждый Work Item;
- UI показывает execution profiles и cost/control параметры;
- voice может инициировать Direct Turn, Managed Run или Automation, но не обязана строить workflow;
- security floor отделён от пользовательского quality-control profile;
- events сначала рассматривают no-op;
- любой новый agent проходит marginal-utility gate.

---

# 30. Каталог источников

**[S01] Dennett functional concept.** Пользовательский project chat, оркестратор, агенты, workflow, память и регулируемая автономность.  
`00_Dennett_Functional_Concept.md`

**[S02] Dennett Memory Fabric 1.2.** Context Manifest, Working Memory, evidence, Causal Trace и federated spaces.  
`10_Dennett_Memory_Fabric.md`

**[S03] Anthropic — Building Effective Agents.** Самое простое решение, workflows для предсказуемости, agents для гибкости, усложнение только после измеримого выигрыша.  
https://www.anthropic.com/engineering/building-effective-agents

**[S04] Single-Agent LLMs Outperform Multi-Agent Systems on Multi-Hop Reasoning Under Equal Thinking Token Budgets.** 2026.  
https://arxiv.org/abs/2604.02460

**[S05] Why Do Multi-Agent LLM Systems Fail?** Failure taxonomy: specification, inter-agent misalignment, verification and termination. 2025.  
https://arxiv.org/abs/2503.13657

**[S06] Efficient Agents: Building Effective Agents While Reducing Cost.** Complexity-to-task fit и cost-of-pass. 2025.  
https://arxiv.org/abs/2508.02694

**[S07] In-Context Prompting Obsoletes Agent Orchestration for Procedural Tasks.** Controlled evidence in favour of self-orchestration for several procedural domains; treated as a bounded result, not universal law. 2026.  
https://arxiv.org/abs/2604.27891

**[S08] Exoskeleton: a lightweight model-dispatcher in a deterministic harness.** Dispatcher model, deterministic invariants, evidence ledger, observability and moving recurring failures out of prompts. 2026.  
https://github.com/muxx/bitgn-ecom1-exoskeleton/blob/main/articles/ARCHITECTURE.md

**[S09] Rewriting Bun in Rust.** Dynamic agent loops, pilot before scale, adversarial review, worktree conflicts, throughput vs integrated correctness. 2026.  
https://bun.com/blog/bun-in-rust

**[S10] Agent2Agent Protocol v1.0.** Minimal external task states, messages, artifacts and interrupted states.  
https://a2a-protocol.org/latest/specification/

**[S11] Temporal Workflow Definition.** Durable replay, deterministic workflow code and external activities; used only for processes that need these guarantees.  
https://docs.temporal.io/workflow-definition

**[S12] OpenAI Agents SDK orchestration.** Agents-as-tools, handoffs и code-driven orchestration.  
https://openai.github.io/openai-agents-python/multi_agent/

**[S13] Interpretable Context Methodology / Model Workspace Protocol.** Filesystem and Markdown can replace heavy orchestration for sequential human-reviewed work. 2026.  
https://arxiv.org/abs/2603.16021

**[S14] Autonomy and Agency in Agentic AI.** Separate dimensions of agency and autonomy, checkpoints, escalation and staging. 2026.  
https://arxiv.org/abs/2605.12105

**[S15] Autonomy Matters.** Intermediate autonomy and user control in personalization/privacy. 2025.  
https://arxiv.org/abs/2510.04465

**[S16] Preserving Sense of Agency.** User control preferences vary by risk; end-user configured autonomy preserves agency. 2025.  
https://arxiv.org/abs/2506.19202

**[S17] AgentBalance.** Budget-aware backbone/topology selection and performance-cost Pareto analysis. 2025.  
https://arxiv.org/abs/2512.11426

**[S18] Compositional Skill Routing / SkillWeaver.** Decomposition granularity as a bottleneck for skill routing. 2026.  
https://arxiv.org/abs/2606.18051

**[S19] Agent Memory: Characterization and System Implications of Stateful Long-Horizon Workloads.** Cost transfer between construction, retrieval and generation; relevant to context/agent budget. 2026.  
https://arxiv.org/abs/2606.06448

**[S20] Production framework issue trackers.** Used as anecdotal evidence of duplicate tool side effects, context pollution, state contention and governance gaps, not as scientific proof.  
https://github.com/crewAIInc/crewAI/issues/5802  
https://github.com/crewAIInc/crewAI/issues/4415  
https://github.com/microsoft/autogen/issues/7487

---

# 31. Финальный чек-лист

- single-agent baseline обязателен;
- complexity ladder начинается с Direct Turn;
- микрошаг не становится Task;
- Task promotion основан на lifecycle value;
- Task state минимальна, waiting reason отдельный;
- project agent сохраняет цельность проекта;
- subagent имеет independent output и consumer;
- decomposition проходит context-coupling test;
- global intent capsule передаётся children;
- свободная межагентная речь разрешена;
- message envelope минимален;
- canonical state хранится отдельно;
- team temporary и budgeted;
- agent count ограничивается marginal utility;
- review targeted и proportional;
- skill/procedure предшествует structured workflow;
- agent-controlled plan предшествует DAG;
- simulation/pilot только по риску и стоимости;
- visual Workflow Studio не является MVP;
- resource leases только при реальной конкуренции;
- external effects idempotent/reconcilable;
- user выбирает execution/control profile;
- safety floor не отключается;
- cost-of-success является основной метрикой;
- несколько процентов качества не оправдывают кратный overhead без явного выбора пользователя;
- самоулучшение умеет упрощать систему;
- Memory Fabric входит через Context Manifest и evidence handles;
- research остаётся адаптивным skill/preset, а не отдельным жёстким engine.

Конец документа.
