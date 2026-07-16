# Участие в разработке Dennett

Dennett развивается от спецификации к реализации. Изменения должны сохранять связь между намерением продукта, бизнес-логикой, архитектурой, кодом и тестами.

## Перед изменением

1. Прочитайте корневой [`AGENTS.md`](AGENTS.md).
2. Прочитайте ближайший вложенный `AGENTS.md` в изменяемой области.
3. Определите каноническую продуктовую и архитектурную документацию.
4. Найдите источник истины и проверьте влияние на permissions, external effects, migrations и protocol compatibility.
5. Выберите минимальную реализацию, которая выполняет документированный контракт.

## Классы изменений

- **Только реализация:** публичный контракт не меняется; обычно достаточно тестов.
- **Изменение контракта:** обновляются protocol/schema, compatibility tests и связанный раздел архитектуры.
- **Изменение бизнес-логики:** каноническая спецификация обновляется до кода или вместе с ним.
- **Изменение архитектуры:** создаётся или обновляется ADR и соответствующий архитектурный том.
- **Изменение provider/adapter:** сохраняется общий port, добавляются capability probes и conformance tests.

## Что должно быть в pull request

- решаемая пользовательская или системная проблема;
- затронутый authoritative state;
- изменённые модули;
- сохранённые или изменённые инварианты;
- выполненные тесты;
- влияние на migrations и compatibility;
- влияние на observability и recovery;
- обновлённые документы или ADR.

## Принципы кода

- Domain/application code не импортирует provider SDK, UI framework или physical database client.
- UI и model context не являются authoritative store.
- External effect не повторяется без idempotency или reconciliation.
- У каждого mutable state один ясный владелец.
- Dependencies передаются явно; глобальный service locator запрещён.
- Новый процесс появляется только ради измеримой изоляции, lifecycle, privilege, reuse или resource boundary.
- Не создавайте абстракцию «на будущее», если у неё один caller и нет реального варианта замены.

## Документация

```bash
python tools/verify_docs.py
python tools/verify_planning.py
python tools/generate_doc_index.py --check
```

Большие документы остаются каноническими; navigation indexes и contract supplements позволяют быстро перемещаться, не создавая конфликтующие копии норм.
