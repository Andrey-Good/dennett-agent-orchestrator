[English](#english) | [Русский](#russian)

# English

## Lifecycle

Status: section index only. The normative rules in this section live in the leaf documents.

Related documents:

- [Canonical spec](../../agent_orchestrator_final_spec_v2.md)
- [Agent JSON contract](../03-contracts/agent-json/README.md)
- [Execution](../04-execution/README.md)
- [State](../05-state/README.md)
- [Extensions](../08-extensions/README.md)
- [ADRs](../09-adrs/README.md)

This section owns the working-surface lifecycle of agents inside Core. It explains how portable agent files become known to the local system, how editable drafts relate to the current live version, how external events launch runs, and how version axes stay separate over time.

Normative documents in this section:

- [Agent Registry](./agent-registry.md): the local index and working-surface boundary for known agents.
- [Draft, Live, and Deploy](./draft-live-deploy.md): the edit and publication lifecycle for agent revisions.
- [Versioning Axes](./versioning-axes.md): the independent meanings of format version, agent version, local live revision, and product version.
- [Events and Triggers](./events-and-triggers.md): how external triggers materialize events and launch runs.

Section boundary:

- Portable graph structure and file fields stay owned by the [Agent JSON contract](../03-contracts/agent-json/README.md).
- Runtime launch behavior and resume mechanics stay owned by [Execution](../04-execution/README.md) and [State](../05-state/README.md).
- Optional model extensions such as builder behavior, memory bindings, and runtime source bindings stay owned by [Extensions](../08-extensions/README.md).
- Historical rationale belongs in [ADRs](../09-adrs/README.md), not here.

## How to use this section

Use this section when the question is about local lifecycle semantics rather than portable contract syntax:

- Which file or revision is the current live agent?
- What may the registry cache, and what may it never replace?
- What happens when an event fires while drafts exist?
- How do version fields interact without collapsing into one number?

If the question is about the shape of JSON itself, start from the contract section. If the question is why a boundary exists, start from the ADR section.

# Russian

## Жизненный цикл

Статус: только индекс раздела. Нормативные правила этого раздела живут в профильных документах.

Связанные документы:

- [Каноническая спецификация](../../agent_orchestrator_final_spec_v2.md)
- [Контракт Agent JSON](../03-contracts/agent-json/README.md)
- [Исполнение](../04-execution/README.md)
- [Состояние](../05-state/README.md)
- [Расширения](../08-extensions/README.md)
- [ADR](../09-adrs/README.md)

Этот раздел владеет семантикой рабочего жизненного цикла агента внутри Core. Здесь описано, как переносимые agent files становятся известны локальной системе, как редактируемые drafts соотносятся с текущей live-версией, как внешние события запускают run-ы и как разные оси версионирования не смешиваются со временем.

Нормативные документы раздела:

- [Реестр агентов](./agent-registry.md): локальный индекс и граница рабочей поверхности для известных агентов.
- [Draft, Live и Deploy](./draft-live-deploy.md): жизненный цикл редактирования и публикации ревизий агента.
- [Оси версионирования](./versioning-axes.md): независимые значения версии формата, версии агента, локальной live-ревизии и версии продукта.
- [События и триггеры](./events-and-triggers.md): как внешние триггеры материализуют события и запускают run-ы.

Граница раздела:

- Переносимая структура графа и поля файла остаются во владении [контракта Agent JSON](../03-contracts/agent-json/README.md).
- Поведение запуска runtime и механика resume остаются во владении [исполнения](../04-execution/README.md) и [состояния](../05-state/README.md).
- Опциональные расширения модели, такие как builder, memory bindings и runtime sources, остаются во владении [расширений](../08-extensions/README.md).
- Историческая мотивация живет в [ADR](../09-adrs/README.md), а не здесь.

## Как читать этот раздел

Этот раздел нужен, когда вопрос относится к локальной lifecycle-семантике, а не к синтаксису переносимого контракта:

- Какая именно ревизия или какой файл считается текущим live-агентом?
- Что реестр вправе кэшировать, а что никогда не должен подменять?
- Что происходит, когда срабатывает событие при наличии drafts?
- Как взаимодействуют версии, не схлопываясь в одно число?

Если вопрос касается формы JSON как таковой, начинать нужно с contract-раздела. Если вопрос о том, почему эта граница вообще существует, начинать нужно с раздела ADR.
