[English](#english) | [Русский](#russian)

# English

## ADR-0001: Codex-First, Not Codex-Only

Status: Accepted
Date: 2026-04-21

Related normative documents:

- [Technology Stack](../01-foundations/technology-stack.md)
- [Runtime Integration Model](../02-architecture/runtime-integration-model.md)
- [Runtime Sources](../08-extensions/runtime-sources.md)

## Context

The project needed one concrete runtime path for the first serious vertical slice. Codex already offered the strongest immediate fit because it exposed the behaviors the orchestrator wanted to rely on early: resume, live interaction, MCP connectivity, skills, and plugin-aware execution.

The repository also needed that first path to stay inside the runtime-adapter boundary rather than turning a product interface into a vendor-runtime wrapper. For Codex, that means an App Server-native adapter path rather than a product-level CLI subprocess path.

At the same time, there was a clear architectural risk in letting the urgency of MVP delivery collapse the internal model into vendor-specific assumptions. If Core started depending directly on Codex client types or session semantics, the project would trade short-term speed for long-term rigidity.

## Alternatives Considered

### Alternative 1: Codex-only architecture

This option would have treated Codex as both the first runtime and the permanent shape of the system. It offered the fastest route in the short term, but it would also have pushed vendor naming, SDK types, and adapter behavior into the core model much earlier than necessary.

### Alternative 2: Fully generic runtime abstraction before any concrete runtime

This option would have tried to design for many future runtimes before the project had validated one end-to-end path. It reduced vendor coupling in theory, but it also risked producing abstractions with no grounded operational reality.

### Alternative 3: Codex-first, not codex-only

This option used Codex as the first-class runtime path for MVP while preserving a clear adapter boundary and a Core vocabulary that was broader than one vendor.

## Decision Rationale

The project chose the third option.

That choice balanced two pressures:

- it kept the first implementation anchored in a real runtime with strong existing capabilities;
- it preserved the architectural discipline needed to avoid turning Core, storage, and portable contracts into direct wrappers around one SDK.

The phrase "codex-first" captured delivery priority. The phrase "not codex-only" captured the longer-lived boundary discipline.

In this repository that discipline also means the Codex path is App Server-native behind the adapter boundary. The product CLI may be the first user interface, but it is not the Codex runtime mechanism.

## Consequences

Positive consequences:

- the team can build around a real runtime instead of a hypothetical abstraction;
- the orchestrator can reuse mature runtime features instead of re-creating them poorly in Core;
- the adapter boundary stays meaningful from the beginning.

Accepted costs:

- some adapter-facing abstractions appear before a second runtime exists;
- integration work is split between Core and a vendor-specific adapter layer;
- feature requests sometimes need translation from vendor language into orchestrator language before they fit the architecture.

## What This ADR Owns

This ADR owns the rationale for choosing the strategy. It does not own the current rules about the technology stack, SDK boundaries, or runtime-source semantics. Those rules live in the linked normative documents above.

# Russian

## ADR-0001: Codex-First, Not Codex-Only

Статус: Принято
Дата: 2026-04-21

Связанные нормативные документы:

- [Технологический стек](../01-foundations/technology-stack.md)
- [Модель интеграции runtime](../02-architecture/runtime-integration-model.md)
- [Runtime Sources](../08-extensions/runtime-sources.md)

## Контекст

Проекту был нужен один конкретный runtime-путь для первого серьезного вертикального среза. Codex давал лучший немедленный fit, потому что уже открывал те поведения, на которые оркестратор хотел опереться с самого начала: resume, live-взаимодействие, подключение MCP, skills и execution с учетом plugins.

Репозиторию также было нужно, чтобы этот первый путь оставался внутри границы runtime adapter-а, а не превращал продуктовый интерфейс в оболочку над vendor runtime. Для Codex это означает App Server-native путь adapter-а, а не product-level CLI subprocess path.

Одновременно существовал явный архитектурный риск: срочность MVP могла схлопнуть внутреннюю модель в vendor-specific assumptions. Если бы Core начал напрямую зависеть от типов Codex client code или семантики его сессий, проект обменял бы краткосрочную скорость на долгосрочную жесткость.

## Рассмотренные альтернативы

### Альтернатива 1: Codex-only архитектура

Этот вариант трактовал бы Codex и как первый runtime, и как постоянную форму системы. В краткосрочной перспективе это давало самый быстрый путь, но также слишком рано протолкнуло бы vendor naming, SDK types и adapter-поведение в core-модель.

### Альтернатива 2: Полностью generic runtime abstraction до любого конкретного runtime

Этот вариант пытался бы проектировать сразу под множество будущих runtime, еще не проверив ни одного end-to-end пути. Теоретически он снижал vendor coupling, но также рисковал породить абстракции без опоры на реальную операционную картину.

### Альтернатива 3: Codex-first, not codex-only

Этот вариант использовал Codex как приоритетный runtime-путь для MVP, сохраняя при этом явную adapter-границу и словарь Core, который шире одного vendor.

## Мотивация решения

Проект выбрал третий вариант.

Этот выбор уравновешивал два давления:

- он удерживал первую реализацию на реальном runtime с сильными существующими возможностями;
- он сохранял архитектурную дисциплину, необходимую для того, чтобы Core, storage и portable contracts не превратились в прямые обертки вокруг одного SDK.

Фраза "codex-first" фиксировала приоритет поставки. Фраза "not codex-only" фиксировала более долговечную дисциплину архитектурных границ.

В этом репозитории эта дисциплина также означает, что путь Codex остается App Server-native за границей adapter-а. Продуктовый CLI может быть первым пользовательским интерфейсом, но он не является runtime-механизмом Codex.

## Последствия

Положительные последствия:

- команда может строить систему вокруг реального runtime, а не гипотетической абстракции;
- оркестратор может переиспользовать зрелые возможности runtime вместо того, чтобы плохо воссоздавать их в Core;
- adapter-boundary сохраняет смысл с самого начала.

Осознанно принятые издержки:

- часть adapter-facing abstractions появляется раньше, чем второй runtime вообще существует;
- интеграционная работа разделяется между Core и vendor-specific adapter layer;
- feature requests иногда требуют перевода из языка vendor в язык оркестратора, прежде чем впишутся в архитектуру.

## Чем владеет этот ADR

Этот ADR владеет мотивацией выбора стратегии. Он не владеет текущими правилами технологического стека, границ SDK или семантикой runtime sources. Эти правила живут в указанных выше нормативных документах.
