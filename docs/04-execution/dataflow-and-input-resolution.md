[English](#english) | [Русский](#russian)

<a id="english"></a>
# English

# Dataflow And Input Resolution

Status: normative.

Related documents:

- [`README.md`](./README.md)
- [`graph-execution.md`](./graph-execution.md)
- [`outputs-outcomes-and-final-response.md`](./outputs-outcomes-and-final-response.md)
- [`../05-state/local-storage-model.md`](../05-state/local-storage-model.md)
- [`../../agent_orchestrator_final_spec_v2.md`](../../agent_orchestrator_final_spec_v2.md)

## 1. Scope

This document defines what data exists during one run, how that data may be referenced, and how a node input is resolved before invocation.

## 2. The Four Allowed Data Spaces

Only the following data spaces may feed graph execution:

- `params`: immutable run parameters supplied for this agent invocation;
- `vars`: mutable graph-global variables for the current run;
- `node outputs`: committed final outputs of previously completed node attempts;
- `event`: immutable graph-visible event envelope for event-triggered runs.

Nothing else may be read implicitly by graph rules.

In particular, the following are not hidden data spaces:

- edge definitions;
- runtime-private traces;
- chat storage internals;
- local database rows that are not surfaced through one of the four allowed spaces;
- memory sources, unless a node reaches them through an explicit runtime capability defined elsewhere.

## 3. Data Ownership Rules

Mandatory ownership boundaries:

- `params` are provided to the run and do not mutate during the run.
- `vars` start from `initial_vars` and may change only through explicit execution rules.
- `node outputs` contain only final node results, never intermediate reasoning.
- `event` is read-only for the lifetime of the run.

These spaces must not be collapsed into one merged dictionary.

## 4. Canonical Input Shape

Node input is declared as structured JSON, not as an implicit string template.

The canonical contract is:

```json
{
  "parts": [
    {"type": "text", "text": "Task:\n"},
    {"type": "ref", "ref": "params.task"}
  ]
}
```

The semantics are an ordered sequence of parts. Core must preserve that order exactly.

## 5. Allowed Part Types

Only two part types are allowed:

- `text`
- `ref`

`text` contributes the declared literal text.

`ref` contributes a value resolved from one of the allowed reference forms.

Core must not add synthetic parts, hidden prefixes, or undeclared data fragments.

## 6. Allowed Reference Forms

The only legal reference forms are:

- `params.<name>`
- `vars.<name>`
- `node.<node_id>.text`
- `node.<node_id>.json.<path>`
- `event.<path>`

There is no fallback lookup in any other namespace.

A reference outside this set is an error. It must never be interpreted heuristically.

## 6.1. Graph-Visible Event Envelope

When a run starts from an event, the `event` data space exposed to the graph is one immutable normalized envelope with reserved top-level keys:

- `payload`: the trigger payload exactly as accepted for that event, when present;
- `launch_note`: the optional start prompt or launch note string, when present.

Therefore:

- trigger payload is addressed through `event.payload` and `event.payload.<path>`;
- the event launch note is addressed through `event.launch_note`;
- payload keys are never flattened directly under `event`;
- trigger references, logical agent references, timestamps, dispatch outcome, storage ids, and other local lifecycle metadata are not injected into `event` unless a future specification adds them explicitly.

## 7. Resolution Snapshot

All references for one node invocation must be resolved against one committed pre-dispatch snapshot.

This means:

- the node sees the latest committed `vars` values available before its call starts;
- the node sees only previously committed node outputs;
- the node does not see its own in-flight output;
- later writes from future nodes cannot affect the already-resolved input of the current node.

Because the base execution model is sequential, this snapshot rule is deterministic.

## 8. Resolution Rules By Namespace

### 8.1. `params.<name>`

The lookup reads the immutable run parameter named `<name>`.

If the parameter is absent, resolution fails before runtime invocation.

### 8.2. `vars.<name>`

The lookup reads the current committed graph variable named `<name>`.

If the variable is absent, resolution fails before runtime invocation.

### 8.3. `node.<node_id>.text`

The lookup reads the latest committed successful text output of `<node_id>` in the current run.

It is valid only if:

- the referenced node has already completed successfully in the current run;
- the latest committed output of that node is in `text` mode.

Otherwise, resolution fails before runtime invocation.

### 8.4. `node.<node_id>.json.<path>`

The lookup reads the latest committed successful JSON output of `<node_id>` and resolves `<path>` inside that object.

It is valid only if:

- the referenced node has already completed successfully in the current run;
- the latest committed output of that node was validated through `output.mode = json`;
- the requested JSON path exists.

Otherwise, resolution fails before runtime invocation.

### 8.5. `event.<path>`

The lookup reads the graph-visible event envelope supplied by event dispatch.

In the base model, the stable graph-visible paths are `event.payload...` and `event.launch_note`.

It is valid only for event-triggered runs when the requested path exists in that envelope. Otherwise, resolution fails before runtime invocation.

## 9. Revisited Nodes

The canonical contract does not ban revisiting a node id within one run, so local execution data must remain well-defined if it happens.

This document fixes the rule:

- `node.<node_id>.*` always resolves to the latest committed successful output of that node in the current run.

Implementations may store a full per-attempt journal, but the reference language exposed to graph authors addresses only the latest committed successful output for a given `node_id`.

## 10. From Output To `vars`

Only successful node outputs validated through `output.mode = json` may mutate `vars` automatically.

Mandatory rules:

- The output must be a JSON object at the top level and satisfy the node's declared `output.schema`.
- Each top-level field of that object is copied into `vars`.
- If multiple successful JSON outputs write the same variable name, the last committed write wins.
- Outputs declared as `output.mode = text` do not mutate `vars` automatically.
- `invalid_output`, `runtime_error`, `cancelled`, and `interrupted` do not mutate `vars`.

The orchestrator must not create hidden variable writes outside these rules.

## 11. Relationship Between Control Flow And Dataflow

Control flow and dataflow must stay separate.

Therefore:

- edges decide where execution goes;
- input references decide what data a node reads;
- schema-valid JSON outputs decide which `vars` keys may change;
- node outputs remain readable as outputs, not as edge payloads.

An implementation that uses edges to smuggle data between nodes is non-conformant.

## 12. `runtime_agent` vs `orchestrator_agent`

Both node kinds use the same input-resolution rules.

The difference is the call target:

- `runtime_agent` sends the resolved input plus node-scoped runtime configuration to a runtime adapter;
- `orchestrator_agent` sends the resolved input to another agent selected by `agent_ref` and resolved by the lifecycle live-resolution rules.

For a direct child launch through `orchestrator_agent`, Core passes that resolved string through the child run's standard invocation params as `params.input`.
This is not an event launch: `event.launch_note` remains reserved for true event-triggered runs only.

For dataflow purposes, an `orchestrator_agent` node still contributes only one final node output back to the parent run: the child run's final response payload when one exists. If the child run provides no final response payload, there is no node output and the parent node's non-success outcome is classified by [`outputs-outcomes-and-final-response.md`](./outputs-outcomes-and-final-response.md).

## 13. What Data Is Never Injected Automatically

Core must never auto-inject any of the following into a node input:

- chain-of-thought;
- hidden storage metadata;
- hidden copies of an event launch note outside the explicit `event.launch_note` path;
- implicit copies of prior prompts;
- full runtime transcripts;
- secret-marker decrypted content;
- memory contents not explicitly reached through the runtime boundary.

If a graph author wants a node to see data, the graph must reference that data explicitly through allowed mechanisms.

## 14. Acceptance Criteria For An Implementation

An implementation conforms to this document only if:

- every node input comes entirely from ordered declared `parts`;
- every `ref` resolves only through the allowed namespaces;
- failed reference resolution stops the node before runtime invocation;
- automatic `vars` mutation occurs only from successful schema-valid JSON-object outputs;
- `node.<node_id>.*` resolution is deterministic even when a node is revisited.

<a id="russian"></a>
# Русский

# Поток данных и разрешение входов

Статус: нормативный.

Связанные документы:

- [`README.md`](./README.md)
- [`graph-execution.md`](./graph-execution.md)
- [`outputs-outcomes-and-final-response.md`](./outputs-outcomes-and-final-response.md)
- [`../05-state/local-storage-model.md`](../05-state/local-storage-model.md)
- [`../../agent_orchestrator_final_spec_v2.md`](../../agent_orchestrator_final_spec_v2.md)

## 1. Область действия

Этот документ определяет, какие данные существуют во время одного run, как на них можно ссылаться и как разрешается вход ноды перед вызовом.

## 2. Четыре разрешенных пространства данных

Только следующие пространства данных могут питать исполнение графа:

- `params`: неизменяемые параметры run, переданные в этот вызов агента;
- `vars`: изменяемые глобальные переменные графа для текущего run;
- `node outputs`: зафиксированные финальные outputs ранее завершенных попыток нод;
- `event`: неизменяемый graph-visible envelope события для event-triggered runs.

Никакие другие данные не могут читаться неявно правилами графа.

В частности, скрытыми пространствами данных не являются:

- определения edges;
- приватные runtime traces;
- внутренности chat storage;
- строки локальной базы, не выведенные в одно из четырех разрешенных пространств;
- memory sources, если нода не достигает их через явную runtime capability, определенную в другом месте.

## 3. Правила владения данными

Обязательные границы ответственности:

- `params` задаются при запуске run и не меняются в его ходе.
- `vars` стартуют из `initial_vars` и могут меняться только по явным правилам исполнения.
- `node outputs` содержат только финальные результаты нод, а не промежуточные рассуждения.
- `event` является read-only на протяжении всего run.

Эти пространства нельзя схлопывать в один общий словарь.

## 4. Каноническая форма входа

Вход ноды задается структурированным JSON, а не неявным строковым шаблоном.

Канонический контракт:

```json
{
  "parts": [
    {"type": "text", "text": "Task:\n"},
    {"type": "ref", "ref": "params.task"}
  ]
}
```

Семантически это упорядоченная последовательность частей. Core обязан сохранять этот порядок без изменений.

## 5. Разрешенные типы частей

Разрешены только два типа частей:

- `text`
- `ref`

`text` добавляет объявленный literal text.

`ref` добавляет значение, разрешенное из одной из разрешенных форм ссылок.

Core не имеет права добавлять synthetic parts, скрытые префиксы или неописанные фрагменты данных.

## 6. Разрешенные формы ссылок

Единственные легальные формы ссылок:

- `params.<name>`
- `vars.<name>`
- `node.<node_id>.text`
- `node.<node_id>.json.<path>`
- `event.<path>`

Fallback lookup в другие пространства имен отсутствует.

Ссылка вне этого набора является ошибкой. Ее нельзя интерпретировать эвристически.

## 6.1. Graph-visible envelope события

Когда run стартует из события, пространство данных `event`, видимое графу, представляет собой один нормализованный неизменяемый envelope с зарезервированными top-level ключами:

- `payload`: payload trigger-а в точности в том виде, в каком он был принят для этого события, если он присутствует;
- `launch_note`: optional строка start prompt или launch note, если она присутствует.

Следовательно:

- к payload trigger-а обращаются через `event.payload` и `event.payload.<path>`;
- к launch note события обращаются через `event.launch_note`;
- ключи payload никогда не схлопываются напрямую под `event`;
- ссылки на trigger, ссылки на логического агента, timestamps, исход dispatch, storage ids и другие локальные lifecycle-метаданные не внедряются в `event`, если будущая спецификация не добавит их явно.

## 7. Snapshot разрешения

Все ссылки для одного вызова ноды должны разрешаться относительно одного committed pre-dispatch snapshot.

Это означает:

- нода видит последние committed значения `vars`, доступные до старта ее вызова;
- нода видит только ранее committed node outputs;
- нода не видит собственный in-flight output;
- более поздние записи будущих нод не могут влиять на уже разрешенный вход текущей ноды.

Поскольку базовая модель исполнения последовательна, это правило snapshot остается детерминированным.

## 8. Правила разрешения по namespace

### 8.1. `params.<name>`

Lookup читает неизменяемый параметр run с именем `<name>`.

Если параметр отсутствует, разрешение завершается ошибкой до runtime invocation.

### 8.2. `vars.<name>`

Lookup читает текущую committed graph variable с именем `<name>`.

Если переменная отсутствует, разрешение завершается ошибкой до runtime invocation.

### 8.3. `node.<node_id>.text`

Lookup читает последний committed successful text output ноды `<node_id>` в текущем run.

Он валиден только если:

- указанная нода уже успешно завершалась в текущем run;
- последний committed output этой ноды находится в режиме `text`.

Иначе разрешение завершается ошибкой до runtime invocation.

### 8.4. `node.<node_id>.json.<path>`

Lookup читает последний committed successful JSON output ноды `<node_id>` и разрешает `<path>` внутри этого объекта.

Он валиден только если:

- указанная нода уже успешно завершалась в текущем run;
- последний committed output этой ноды находится в режиме `json`;
- запрошенный JSON path существует.

Иначе разрешение завершается ошибкой до runtime invocation.

### 8.5. `event.<path>`

Lookup читает graph-visible envelope события, переданный event dispatch.

В базовой модели стабильными graph-visible путями являются `event.payload...` и `event.launch_note`.

Он валиден только для event-triggered runs, когда запрошенный путь существует внутри этого envelope. Иначе разрешение завершается ошибкой до runtime invocation.

## 9. Повторные посещения нод

Канонический контракт не запрещает повторно заходить в одну и ту же `node_id` в рамках одного run, поэтому локальные execution data должны оставаться определенными и в этом случае.

Этот документ фиксирует правило:

- `node.<node_id>.*` всегда разрешается в последний committed successful output этой ноды в текущем run.

Реализации могут хранить полный журнал попыток, но язык ссылок, видимый автору графа, адресует только последний committed successful output для данного `node_id`.

## 10. Переход от output к `vars`

Только успешные outputs нод, валидированные через `output.mode = json`, могут автоматически мутировать `vars`.

Обязательные правила:

- Output обязан быть JSON object на верхнем уровне и удовлетворять объявленному `output.schema`.
- Каждое top-level поле этого object копируется в `vars`.
- Если несколько успешных JSON-outputs записывают одну и ту же переменную, побеждает последняя committed запись.
- Outputs, объявленные как `output.mode = text`, не мутируют `vars` автоматически.
- `invalid_output`, `runtime_error`, `cancelled` и `interrupted` не мутируют `vars`.

Оркестратор не может создавать скрытые записи переменных вне этих правил.

## 11. Связь control flow и data flow

Control flow и dataflow должны оставаться разделенными.

Поэтому:

- edges решают, куда идет исполнение;
- input references решают, какие данные читает нода;
- Schema-valid JSON-outputs решают, какие ключи `vars` могут измениться;
- node outputs остаются читаемыми как outputs, а не как edge payload.

Реализация, использующая edges для скрытой передачи данных между нодами, не соответствует этому документу.

## 12. `runtime_agent` и `orchestrator_agent`

Оба типа нод используют одни и те же правила разрешения входа.

Разница только в целевом вызове:

- `runtime_agent` отправляет разрешенный вход и node-scoped runtime configuration в runtime adapter;
- `orchestrator_agent` отправляет разрешенный вход другому агенту, выбранному по `agent_ref` и разрешенному по lifecycle-правилам live-resolution.

Для прямого child launch-а через `orchestrator_agent` Core передает эту разрешенную строку через стандартные invocation params child run-а как `params.input`.
Это не event launch: `event.launch_note` по-прежнему зарезервирован только для настоящих event-triggered run-ов.

С точки зрения dataflow нода `orchestrator_agent` все равно возвращает в parent run только один финальный node output: payload финального ответа child run-а, когда такой payload существует. Если child run не предоставляет payload финального ответа, node output не возникает, а non-success outcome родительской ноды классифицируется по [`outputs-outcomes-and-final-response.md`](./outputs-outcomes-and-final-response.md).

## 13. Какие данные никогда не инжектятся автоматически

Core никогда не должен автоматически подмешивать во вход ноды:

- chain-of-thought;
- скрытые storage metadata;
- скрытые копии event launch note вне явного пути `event.launch_note`;
- неявные копии предыдущих prompts;
- полные runtime transcripts;
- расшифрованное содержимое secret markers;
- содержимое memory, если оно явно не достигнуто через runtime boundary.

Если автор графа хочет, чтобы нода увидела данные, граф обязан сослаться на эти данные явно через разрешенные механизмы.

## 14. Критерии приемки реализации

Реализация соответствует этому документу только если:

- каждый вход ноды полностью формируется из упорядоченных объявленных `parts`;
- каждый `ref` разрешается только через разрешенные namespaces;
- ошибка разрешения ссылки останавливает ноду до runtime invocation;
- автоматическая мутация `vars` происходит только из успешных schema-valid JSON-object outputs;
- разрешение `node.<node_id>.*` остается детерминированным даже при повторном посещении ноды.
