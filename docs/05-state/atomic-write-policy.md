[English](#english) | [Русский](#russian)

<a id="english"></a>
# English

# Atomic Write Policy

Status: normative.

Related documents:

- [`README.md`](./README.md)
- [`local-storage-model.md`](./local-storage-model.md)
- [`../07-lifecycle/README.md`](../07-lifecycle/README.md)
- [`../../agent_orchestrator_final_spec_v2.md`](../../agent_orchestrator_final_spec_v2.md)

## 1. Scope

This document defines the crash-safe file-write policy for critical JSON files in the project.

## 2. Files Covered By This Policy

At minimum, this policy applies to:

- live agent files;
- draft agent files;
- any other critical JSON file whose corruption would change orchestrator-visible behavior or recovery semantics.

If an implementation introduces another critical JSON artifact, it inherits this policy automatically unless a stricter policy is defined.

## 3. Forbidden Write Pattern

Direct in-place overwrite of a critical target file is forbidden.

The implementation must never rely on:

- partial overwrite of the target path;
- buffered write success without flush;
- rename across directories or filesystems;
- best-effort cleanup as a substitute for durability.

## 4. Required Write Sequence

The minimum required sequence is:

1. Serialize the full new content before touching the target.
2. Validate that the new content is complete and structurally acceptable.
3. Write it to a temporary file in the same directory as the target.
4. Flush process buffers for the temporary file.
5. `fsync` or the platform-equivalent durability call for the temporary file.
6. Atomically replace the target path with the temporary file.
7. `fsync` the containing directory when the platform allows it.

The temporary file must live on the same filesystem as the target so that the replacement step is actually atomic.

## 5. Visibility Guarantee

Readers of the target path must observe one of only two states:

- the complete old file;
- the complete new file.

Readers must never observe:

- half-written JSON;
- truncated JSON;
- a target path deleted between write steps as part of the normal success path.

## 6. Validation Before Replace

The target path must not be replaced unless the new serialized content has already passed all validations required for that file type.

If validation fails:

- the target file remains untouched;
- the failed temporary artifact is not promoted;
- the write operation is a failure, not a degraded success.

## 7. Coordination With Local Metadata

When a local metadata transaction points to a newly written critical file, ordering matters.

Mandatory rule:

- metadata must not advertise the new file state before the atomic file replace has completed successfully.

If both file write and local metadata update are part of one higher-level operation, the implementation must order them so that readers cannot observe metadata referencing a file version that is not yet durable.

## 8. Crash Recovery And Orphaned Temps

Temporary files are never canonical state by themselves.

After a crash:

- the target path remains authoritative if it exists and validates;
- orphaned temporary files may be inspected or cleaned up, but they must not automatically replace a valid target;
- automatic promotion of an orphaned temporary file is allowed only if the target is absent and the implementation can prove the temporary file is the intended fully written replacement.

This keeps cleanup logic from inventing new truth after a crash.

## 9. Permissions And Metadata

If the platform requires explicit preservation of file permissions or metadata relevant to correct operation, that preservation must happen as part of the atomic-write path, not as a later best-effort step that could be skipped by a crash.

## 10. Relationship To Database Transactions

This policy is about file-backed critical artifacts.

It does not replace transactional discipline for SQLite or any other local metadata store. Instead:

- file durability is governed here;
- database durability is governed by the database transaction model;
- cross-boundary operations must respect both.

## 11. Acceptance Criteria For An Implementation

An implementation conforms to this document only if:

- critical JSON files are written through same-directory temporary files and atomic replace;
- the write path flushes and syncs before replace;
- readers cannot observe partial target content;
- validation failure leaves the target untouched;
- post-crash cleanup does not promote orphaned files into truth without proof.

<a id="russian"></a>
# Русский

# Политика атомарной записи

Статус: нормативный.

Связанные документы:

- [`README.md`](./README.md)
- [`local-storage-model.md`](./local-storage-model.md)
- [`../07-lifecycle/README.md`](../07-lifecycle/README.md)
- [`../../agent_orchestrator_final_spec_v2.md`](../../agent_orchestrator_final_spec_v2.md)

## 1. Область действия

Этот документ определяет crash-safe policy записи критичных JSON files проекта.

## 2. Файлы, на которые распространяется policy

Как минимум эта policy распространяется на:

- live agent files;
- draft agent files;
- любые другие критичные JSON files, повреждение которых изменило бы видимое оркестратору поведение или семантику восстановления.

Если реализация вводит еще один критичный JSON artifact, он автоматически наследует эту policy, если только для него не определена более строгая policy.

## 3. Запрещенный паттерн записи

Прямая in-place перезапись критичного target file запрещена.

Реализация никогда не должна полагаться на:

- частичную перезапись target path;
- успех buffered write без flush;
- rename между разными директориями или файловыми системами;
- best-effort cleanup как замену durability.

## 4. Обязательная последовательность записи

Минимально обязательная последовательность такова:

1. Полностью сериализовать новое содержимое до касания target.
2. Провалидировать, что новое содержимое полно и структурно допустимо.
3. Записать его во временный файл в той же директории, что и target.
4. Выполнить flush process buffers для временного файла.
5. Выполнить `fsync` или эквивалентный платформенный durability call для временного файла.
6. Атомарно заменить target path временным файлом.
7. Выполнить `fsync` директории-контейнера, если платформа это позволяет.

Временный файл обязан жить на той же файловой системе, что и target, чтобы шаг замены действительно был атомарным.

## 5. Гарантия видимости

Читатели target path должны наблюдать только одно из двух состояний:

- полный старый файл;
- полный новый файл.

Читатели никогда не должны наблюдать:

- half-written JSON;
- truncated JSON;
- target path, удаленный между шагами записи как часть нормального success path.

## 6. Валидация до замены

Target path нельзя заменять, пока новое сериализованное содержимое не прошло все обязательные валидации для данного типа файла.

Если валидация не пройдена:

- target file остается нетронутым;
- неуспешный temporary artifact не продвигается;
- операция записи считается ошибкой, а не degraded success.

## 7. Координация с локальными metadata

Когда локальная metadata transaction ссылается на только что записанный критичный файл, порядок операций важен.

Обязательное правило:

- metadata не имеют права рекламировать новое файловое состояние до успешного завершения atomic file replace.

Если file write и local metadata update входят в одну операцию более высокого уровня, реализация обязана упорядочить их так, чтобы читатели не могли увидеть metadata, ссылающиеся на версию файла, которая еще не стала durable.

## 8. Восстановление после сбоев и orphaned temps

Temporary files сами по себе никогда не являются каноническим состоянием.

После сбоя:

- target path остается authoritative, если он существует и валиден;
- orphaned temporary files можно inspect-ить или удалять, но они не могут автоматически заменять валидный target;
- автоматическое продвижение orphaned temporary file допустимо только если target отсутствует и реализация может доказать, что temporary file является именно intended fully written replacement.

Это не позволяет cleanup-логике придумывать новую truth после сбоя.

## 9. Права доступа и metadata

Если платформа требует явного сохранения file permissions или metadata, критичных для корректной работы, это сохранение должно происходить как часть atomic-write path, а не как поздний best-effort step, который может быть пропущен из-за сбоя.

## 10. Связь с транзакциями базы данных

Эта policy относится к file-backed critical artifacts.

Она не заменяет транзакционную дисциплину SQLite или любого другого local metadata store. Вместо этого:

- файловая durability регулируется здесь;
- database durability регулируется транзакционной моделью базы;
- cross-boundary operations обязаны уважать обе стороны.

## 11. Критерии приемки реализации

Реализация соответствует этому документу только если:

- критичные JSON files пишутся через temporary files в той же директории и atomic replace;
- путь записи выполняет flush и sync до замены;
- читатели не могут наблюдать partial target content;
- ошибка валидации оставляет target нетронутым;
- post-crash cleanup не повышает orphaned files до truth без доказательства.
