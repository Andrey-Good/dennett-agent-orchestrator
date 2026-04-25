[English](#english) | [Русский](#russian)

# English

## ADRs

Status: section index only. Individual ADRs are historical rationale records, not normative source-of-truth documents.

Related documents:

- [Canonical spec](../../agent_orchestrator_final_spec_v2.md)
- [Foundations](../01-foundations/README.md)
- [Architecture](../02-architecture/README.md)
- [Lifecycle](../07-lifecycle/README.md)
- [Extensions](../08-extensions/README.md)

This section captures why long-lived architectural choices were made, what alternatives were on the table, and which tradeoffs the project accepted at the time. The current rules themselves stay in the profile documents that own those boundaries.

Recorded ADRs:

- [ADR-0001: Codex-First, Not Codex-Only](./ADR-0001-codex-first-not-codex-only.md)
- [ADR-0002: Agent File vs Local State](./ADR-0002-agent-file-vs-local-state.md)
- [ADR-0003: Chat Resume Is Not Memory](./ADR-0003-chat-resume-is-not-memory.md)

## When to write an ADR

An ADR is appropriate when:

- there were multiple realistic architectural options;
- the decision affects more than one subsystem;
- the project benefits from remembering the reasoning later;
- the tradeoff is expensive to reverse.

## What ADRs do not own

ADRs do not own:

- the portable file contract;
- lifecycle semantics;
- runtime adapter contracts;
- extension field behavior;
- examples and templates.

When an ADR mentions a current rule, it should point to the normative document that owns that rule now.

# Russian

## ADR

Статус: только индекс раздела. Отдельные ADR являются историческими записями мотивации, а не нормативными документами-источниками истины.

Связанные документы:

- [Каноническая спецификация](../../agent_orchestrator_final_spec_v2.md)
- [Foundations](../01-foundations/README.md)
- [Архитектура](../02-architecture/README.md)
- [Жизненный цикл](../07-lifecycle/README.md)
- [Расширения](../08-extensions/README.md)

Этот раздел фиксирует, почему были выбраны долгоживущие архитектурные решения, какие альтернативы рассматривались и какие компромиссы проект осознанно принял на тот момент. Сами актуальные правила остаются в профильных документах, которые владеют соответствующими границами.

Зафиксированные ADR:

- [ADR-0001: Codex-First, Not Codex-Only](./ADR-0001-codex-first-not-codex-only.md)
- [ADR-0002: Agent File vs Local State](./ADR-0002-agent-file-vs-local-state.md)
- [ADR-0003: Chat Resume Is Not Memory](./ADR-0003-chat-resume-is-not-memory.md)

## Когда нужен ADR

ADR уместен, когда:

- существовало несколько реалистичных архитектурных вариантов;
- решение затрагивает больше одной подсистемы;
- проекту полезно сохранить логику выбора для будущего;
- откат такого решения был бы дорогим.

## Чем ADR не владеют

ADR не владеют:

- переносимым файловым контрактом;
- lifecycle-семантикой;
- контрактами runtime adapter;
- поведением extension-полей;
- примерами и шаблонами.

Если ADR упоминает актуальное правило, он должен ссылаться на нормативный документ, который сейчас владеет этим правилом.
