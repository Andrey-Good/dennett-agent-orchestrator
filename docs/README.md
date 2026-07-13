# Карта документации Denet

Документация разделена по назначению. Большие канонические документы объясняют *зачем* и *что*. Небольшие cross-domain contracts описывают жизненные циклы, пересекающие несколько областей. Архитектурные тома объясняют *где* и *как*. Локальные `AGENTS.md` дают coding-agent-у практические правила для конкретной части кода.

## Быстрые маршруты чтения

### Обзор продукта за 15 минут

1. [`00_Denet_Functional_Concept.md`](specifications/00_Denet_Functional_Concept.md)
2. [`Карта архитектуры`](architecture/README.md)
3. [`Трассируемость требований`](TRACEABILITY.md)

### Продукт и бизнес-логика

Читайте спецификации по номерам: 00, 01, 10, 20, 30, 40, 41, 50, 60, 61, 70. Cross-domain contract открывайте, когда на него ссылается изменяемая область.

### Реализация

1. [`implementation/README.md`](implementation/README.md).
2. Work Package или Autonomous Batch из [`../planning/`](../planning/).
3. Корневой [`AGENTS.md`](../AGENTS.md).
4. [`Карта архитектуры`](architecture/README.md).
5. Ближайший вложенный `AGENTS.md`.
6. Public trait/schema.
7. Тесты и похожий реализованный модуль.

### Память

`10` → contracts A/H/J → архитектура `81` → `crates/denet-memory-core/AGENTS.md`.

### Provider/tool adapter

`41` → архитектура `82` → `adapters/AGENTS.md`.

### Desktop или mobile

`60` или `61` → архитектура `83` → `apps/*/AGENTS.md`.

## Каноничность

- `docs/specifications/` и `docs/architecture/` — актуальные нормы.
- `docs/decisions/` объясняет труднообратимые решения.
- `docs/runbooks/` содержит эксплуатационные процедуры.
- `docs/research/` поддерживает решения evidence, но не становится нормой автоматически.
- `docs/archive/` не является источником истины.

## Навигация

Сгенерированные индексы заголовков находятся в [`navigation/`](navigation/README.md). Они позволяют прыгать по большим файлам без искусственного разрезания канонического текста.

## Дополнительные карты

- [`implementation/README.md`](implementation/README.md) — стратегия реализации, агентный протокол и роль владельца.
- [`testing/TEST_CATALOGUE_AND_QUALITY_GATES.md`](testing/TEST_CATALOGUE_AND_QUALITY_GATES.md) — структурированный каталог тестовых обязательств.

- [`TRACEABILITY.md`](TRACEABILITY.md) — спецификация → архитектура → код → тест.
- [`architecture/CODE_MAP.md`](architecture/CODE_MAP.md) — процессы и code roots.
- [`OPEN_QUESTIONS.md`](OPEN_QUESTIONS.md) — решения, требующие risk spike или измерения.
