[English](#english) | [Русский](#russian)

# English

## ADR-0002: Agent File vs Local State

Status: Accepted
Date: 2026-04-21

Related normative documents:

- [Agent JSON contract](../03-contracts/agent-json/README.md)
- [Agent Registry](../07-lifecycle/agent-registry.md)
- [Draft, Live, and Deploy](../07-lifecycle/draft-live-deploy.md)
- [State](../05-state/README.md)

## Context

The orchestrator needed both a portable agent artifact and a local working surface for drafts, chats, resume data, and indexing. The tension was obvious: a database-centric model would simplify some local workflows, but it would also make portability, reviewability, and explicit ownership boundaries harder to keep.

The project also needed to survive ordinary developer practices such as file diffs, manual edits, Git history, file copies, and cross-machine transfer. Those practices strongly favored a file-centered canonical artifact.

## Alternatives Considered

### Alternative 1: Local database as the primary source of truth

This option would have made Core state convenient to query and mutate, but the canonical agent definition would effectively live in a machine-local store. Portability would then depend on export logic, and the JSON file would become secondary or synthetic.

### Alternative 2: Dual truth between files and local state

This option would have shared authority between the agent file and local storage. It looked flexible at first, but it created too many ambiguous failure modes: drift, partial synchronization, unclear merge behavior, and uncertainty over which copy to trust after crashes or manual edits.

### Alternative 3: Agent file as truth, local state as derivative working surface

This option kept the portable file canonical while still allowing a rich local registry, draft lifecycle, and resume-oriented state model.

## Decision Rationale

The project chose the third option.

That choice aligned with the product shape:

- agents are intended to be portable artifacts;
- developers need to inspect and edit them directly;
- local lifecycle state is useful, but it is still local, derivative, and operational.

Treating the file as canonical reduced architectural ambiguity at the cost of requiring a clearer registry and atomic file-write discipline.

## Consequences

Positive consequences:

- agent definitions stay portable and reviewable;
- the local registry can remain an index instead of becoming a competing domain model;
- manual edits and Git workflows remain first-class rather than accidental edge cases.

Accepted costs:

- Core needs indexing and refresh logic;
- deploy flows need careful atomic writes and revalidation;
- local state needs to tolerate files being moved, edited externally, or temporarily invalid.

## What This ADR Owns

This ADR owns the rationale for preferring file truth over local-state truth. It does not own the current registry rules, draft/live semantics, or storage contracts. Those rules live in the linked normative documents above.

# Russian

## ADR-0002: Agent File vs Local State

Статус: Принято
Дата: 2026-04-21

Связанные нормативные документы:

- [Контракт Agent JSON](../03-contracts/agent-json/README.md)
- [Реестр агентов](../07-lifecycle/agent-registry.md)
- [Draft, Live и Deploy](../07-lifecycle/draft-live-deploy.md)
- [Состояние](../05-state/README.md)

## Контекст

Оркестратору требовались и переносимый артефакт агента, и локальная рабочая поверхность для drafts, chats, resume data и indexing. Напряжение было очевидным: database-centric модель упростила бы часть локальных workflow, но одновременно затруднила бы переносимость, ревьюемость и явные границы владения.

Проекту также нужно было переживать обычные практики разработки: file diffs, ручные правки, историю Git, копирование файлов и перенос между машинами. Эти практики сильно тянули в сторону file-centered канонического артефакта.

## Рассмотренные альтернативы

### Альтернатива 1: Локальная база данных как основной источник истины

Этот вариант сделал бы состояние Core удобным для запросов и изменений, но каноническое определение агента фактически жило бы в machine-local store. Тогда переносимость зависела бы от export-логики, а JSON-файл стал бы вторичным или синтетическим.

### Альтернатива 2: Двойная истина между файлами и локальным состоянием

Этот вариант делил бы полномочия между agent file и локальным хранилищем. Сначала он выглядел гибким, но создавал слишком много двусмысленных режимов отказа: drift, частичную синхронизацию, неясное merge-поведение и неопределенность, какой копии верить после сбоев или ручных правок.

### Альтернатива 3: Agent file как истина, local state как производная рабочая поверхность

Этот вариант сохранял переносимый файл каноническим и одновременно позволял богатый локальный реестр, draft-lifecycle и state-модель, ориентированную на resume.

## Мотивация решения

Проект выбрал третий вариант.

Этот выбор соответствовал форме продукта:

- агенты задуманы как переносимые артефакты;
- разработчикам нужно просматривать и редактировать их напрямую;
- локальное lifecycle-state полезно, но все равно остается локальным, производным и операционным.

Трактовка файла как канонического источника снижала архитектурную двусмысленность ценой более четкой дисциплины реестра и атомарной записи файлов.

## Последствия

Положительные последствия:

- определения агентов остаются переносимыми и пригодными для review;
- локальный реестр может оставаться индексом, а не конкурирующей доменной моделью;
- ручные правки и workflow вокруг Git остаются first-class, а не случайными edge cases.

Осознанно принятые издержки:

- Core нужна логика indexing и refresh;
- deploy-flow требует аккуратной атомарной записи и повторной валидации;
- локальное состояние должно терпимо относиться к тому, что файлы переносят, редактируют извне или временно делают невалидными.

## Чем владеет этот ADR

Этот ADR владеет мотивацией выбора файловой истины вместо истины локального состояния. Он не владеет текущими правилами реестра, семантикой draft/live или storage-контрактами. Эти правила живут в указанных выше нормативных документах.
