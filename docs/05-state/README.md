[English](#english) | [Русский](#russian)

<a id="english"></a>
# English

# State

This section owns local state, persistence, and recovery rules for the orchestrator. It defines what may become local truth, what remains file truth, what is stored for resume, and what durability guarantees are mandatory.

## Document Owners

- [`chat-and-resume.md`](./chat-and-resume.md) owns chat state, resume state, resume mode selection, and explicit-resume invariants.
- [`subagent-context-and-memory.md`](./subagent-context-and-memory.md) owns managed child-run lineage, explicit context inheritance records, attempt/review state categories, and persistence prohibitions.
- [`local-storage-model.md`](./local-storage-model.md) owns the local source-of-truth matrix, logical storage records, and derived-metadata boundaries.
- [`secret-markers.md`](./secret-markers.md) owns the optional secret-marker extraction and restore model.
- [`atomic-write-policy.md`](./atomic-write-policy.md) owns crash-safe writes for critical JSON files and file-backed state.

## Section Boundaries

- Execution semantics and node outcomes belong to [`../04-execution`](../04-execution/README.md).
- User-visible chat interaction belongs to [`../06-interaction`](../06-interaction/README.md).
- Draft/live/deploy lifecycle belongs to [`../07-lifecycle`](../07-lifecycle/README.md).
- Runtime adapter architecture belongs to [`../02-architecture/runtime-integration-model.md`](../02-architecture/runtime-integration-model.md).
- Managed child-run lineage and context inheritance belong to [`subagent-context-and-memory.md`](./subagent-context-and-memory.md); that document refines the architecture owner without replacing it.
- The canonical source above this section remains [`agent_orchestrator_final_spec_v2.md`](../../agent_orchestrator_final_spec_v2.md).

## Reading Order

1. Read [`chat-and-resume.md`](./chat-and-resume.md) for what state exists and when resume is allowed.
2. Read [`subagent-context-and-memory.md`](./subagent-context-and-memory.md) for child-run lineage, context inheritance, and persistence boundaries.
3. Read [`local-storage-model.md`](./local-storage-model.md) for what the local store may and may not contain.
4. Read [`secret-markers.md`](./secret-markers.md) if sensitive fragments must be persisted safely.
5. Read [`atomic-write-policy.md`](./atomic-write-policy.md) before implementing any file mutation path.

This README is navigation only. It does not define rules by itself.

<a id="russian"></a>
# Русский

# Состояние

Этот раздел владеет локальным состоянием, персистентностью и правилами восстановления оркестратора. Он определяет, что может становиться локальной истиной, что остается файловой истиной, что сохраняется для resume и какие гарантии durability обязательны.

## Документы-владельцы

- [`chat-and-resume.md`](./chat-and-resume.md) владеет chat state, resume state, выбором режима resume и инвариантами explicit resume.
- [`subagent-context-and-memory.md`](./subagent-context-and-memory.md) владеет managed child-run lineage, явными записями context inheritance, категориями attempt/review state и запретами на хранение.
- [`local-storage-model.md`](./local-storage-model.md) владеет матрицей локальных источников истины, логическими записями хранилища и границами производных метаданных.
- [`secret-markers.md`](./secret-markers.md) владеет optional-моделью извлечения и восстановления secret markers.
- [`atomic-write-policy.md`](./atomic-write-policy.md) владеет crash-safe записью критичных JSON files и file-backed state.

## Границы раздела

- Семантика исполнения и исходы нод относятся к [`../04-execution`](../04-execution/README.md).
- Пользовательское chat-взаимодействие относится к [`../06-interaction`](../06-interaction/README.md).
- Жизненный цикл draft/live/deploy относится к [`../07-lifecycle`](../07-lifecycle/README.md).
- Архитектурная граница runtime adapters относится к [`../02-architecture/runtime-integration-model.md`](../02-architecture/runtime-integration-model.md).
- Managed child-run lineage и context inheritance относятся к [`subagent-context-and-memory.md`](./subagent-context-and-memory.md); этот документ уточняет architecture owner, а не заменяет его.
- Канонический источник над этим разделом остается [`agent_orchestrator_final_spec_v2.md`](../../agent_orchestrator_final_spec_v2.md).

## Порядок чтения

1. Сначала читайте [`chat-and-resume.md`](./chat-and-resume.md) для понимания того, какие состояния существуют и когда разрешен resume.
2. Затем читайте [`subagent-context-and-memory.md`](./subagent-context-and-memory.md) для child-run lineage, context inheritance и границ persistence.
3. Затем читайте [`local-storage-model.md`](./local-storage-model.md) для понимания того, что локальное хранилище может и не может содержать.
4. Затем читайте [`secret-markers.md`](./secret-markers.md), если нужно безопасно сохранять чувствительные фрагменты.
5. Перед реализацией любого пути мутации файлов обязательно читайте [`atomic-write-policy.md`](./atomic-write-policy.md).

Этот README выполняет только навигационную роль и сам по себе норм не задает.
