# Часть III. Сквозные нормативные правила

> **Канонический shared supplement.** Primary owner: `01_Dennett_Specification_Index_and_Shared_Contracts.md`.  
> Правила применяются ко всем бизнес- и архитектурным документам. Источники раскрыты в [`REFERENCES.md`](REFERENCES.md).


## 12. Один объект — один канонический владелец

Временный файл не меняет ownership, заданный Specification Index.

| Объект/решение | Канонический владелец |
|---|---|
| Memory Event, Evidence, Claim, retention | Memory Fabric |
| Project Session, Task, Run, Agent | Agentic Control Fabric |
| Permission, grant, identity, consent | Trust Fabric |
| Voice turn, floor, ambient conversation behavior | Voice Fabric |
| Provider/tool/skill/connector availability | Capability Fabric |
| Runtime state, dispatch, sync, update execution | Server Runtime |
| Desktop/mobile interaction | соответствующий UI document |
| Project/Artifact/Communication lifecycle delta из этого файла | позднее распределяется по владельцам согласно карте |

Ни один будущий модуль не создаёт параллельную «истину» только ради удобства реализации.

## 13. Общая цепочка значимого действия

Любое значимое действие проходит концептуальную цепочку:

```text
source signal
→ normalized intent/event/observation
→ identity and trust context
→ current authoritative state
→ context/evidence assembly
→ decision or proposal
→ capability resolution
→ permission/effect validation
→ execution
→ receipt/outcome/artifact
→ memory/provenance update
→ user-visible state
→ recovery path
```

Простая операция может пройти её внутри одного процесса и без LLM. Цепочка является контрактом ответственности, а не требованием строить workflow.

## 14. Events, commands, observations and effects не смешиваются

- **Observation** сообщает, что source что-то увидел/услышал.
- **Event** сообщает, что произошло значимое изменение.
- **Command** просит изменить состояние или выполнить действие.
- **Proposal** предлагает command, но ещё не имеет authority.
- **Effect** изменяет внешний мир.
- **Receipt** подтверждает наблюдаемый результат эффекта.

Внешняя страница, сообщение, голос третьего лица или imported package могут создать observation/event, но не command от владельца.

## 15. Human intent и authority

Пользователь может быстро и явно:

- добавить capability;
- отправить сообщение;
- прикрепить проект;
- сохранить artifact;
- включить trusted scope;
- настроить automation.

Система не должна навязывать utility-review ручному выбору. Но техническая безопасность, exact target и external-effect idempotency остаются.

Память о том, что пользователь «обычно разрешает», помогает предложить bounded policy, но не является current permission.

## 16. Быстрый путь без лишней модели

Без LLM должны выполняться, когда возможно:

- exact permission/grant check;
- path/repository identity;
- update compatibility range;
- checksum/signature verification;
- event deduplication;
- storage thresholds;
- schedule calculation;
- exact search/navigation;
- provider health/quota lookup;
- cancel/stop/mute/privacy controls;
- basic import validation.

Модель подключается для неоднозначного смысла, сравнения, synthesis и адаптации, а не как обязательный посредник каждого действия.

## 17. Неизвестный результат — отдельное состояние

Для внешней отправки, публикации, удаления remote repository, payment, push/release и других consequential effects:

```text
SUCCESS != TIMEOUT
FAILURE != TIMEOUT
TIMEOUT after dispatch → UNKNOWN
```

`UNKNOWN` требует reconciliation. Retry без reconciliation запрещён, если может дублировать эффект.

## 18. Source, trust и permission сохраняются при трансформации

При переходах:

- audio → transcript;
- screen → OCR;
- message → summary;
- research source → claim;
- package → imported objects;
- artifact → export;
- memory → context;

должны сохраняться:

- origin;
- transformation/version;
- trust domain;
- sensitivity;
- owner/scope;
- evidence handle.

Derived text не становится user instruction только потому, что модель его сформулировала.

## 19. Архивирование, удаление, отзыв и отключение различаются

### Shared semantic: Archive

Скрыть/заморозить с сохранением и возможностью возврата.

### Disable/Pause

Остановить активное использование, сохранив объект.

### Shared semantic: Detach

Убрать связь с внешним/физическим ресурсом, не удаляя его.

### Revoke

Запретить дальнейшее использование/доступ или распространение.

### Shared semantic: Delete

Удалить payload/state согласно retention и dependency graph.

### Forget/Hide from context

Не использовать в обычной персонализации, не обязательно удалить bytes.

UI и APIs не используют один глагол для этих разных эффектов.

## 20. User-owned, Dennett-managed и provider-managed

Любой изменяемый package, skill, artifact, draft, project file или setting имеет ownership.

- User-owned: Dennett proposes patch/fork; no silent rewrite.
- Dennett-managed: system may version/update/rollback within policy.
- Provider-managed: native lifecycle preserved.
- Project-shared: repository/version rules apply.
- Imported: separate trust domain until promotion.

## 21. Freshness and authority

Cached state always has observation time. Current-state decision checks live authority when effect/risk requires it.

Examples:

- repository/worktree authority for code;
- provider receipt for sent message;
- Trust registry for active permission;
- Head epoch for coordination;
- Memory ledger for historical event;
- search result is pointer, not authority.

## 22. Progressive disclosure and bounded context

Dennett can know much but show/send little:

- mobile gets summaries and handles;
- voice gets compact answer context;
- project agent gets project-relevant context;
- search reveals details on demand;
- capability descriptions loaded lazily;
- imported package inspected before activation.

This reduces tokens, leaks and cognitive overload.

## 23. User interruption and resumability

Every meaningful long operation specifies:

- can user interrupt;
- what stops immediately;
- what checkpoints;
- what external effect may already have happened;
- what partial artifact remains;
- how to resume;
- what changed while away.

## 24. Resource proportionality

Formal durability, review, provenance and model depth increase only with:

- duration;
- irreversibility;
- external effect;
- data sensitivity;
- concurrency;
- need for reproducibility.

A quick note, exact search or simple project message does not become a Managed Run.

## 25. Platform truth over product fantasy

If OS/provider forbids a background mode:

- feature reports unsupported/restricted;
- alternate mode offered;
- no documentation claim that the feature is guaranteed.

Examples:

- screen capture requiring explicit projection session;
- mobile microphone background restrictions;
- provider lacking message recall;
- local model not fitting hardware.

---
