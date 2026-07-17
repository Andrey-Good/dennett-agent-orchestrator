# Модуль H. Federated Global Search Contract

> **Канонический cross-domain supplement · `H`**  
> **Primary owner:** 50 Server Runtime with Memory source adapters.  
> **Происхождение:** выделено из предархитектурного gap-аудита. Документ актуален и нормативен.  
> Маркеры источников вида `[[Sxx]]` раскрыты в [`REFERENCES.md`](REFERENCES.md). Ownership и порядок чтения описаны в [`README.md`](README.md).


## H.1. Назначение

Пользователь должен иметь один быстрый поиск по Dennett, но данные остаются распределены по разным authoritative domains:

- проекты и файлы;
- project sessions/messages;
- Memory Fabric;
- artifacts;
- visual/audio captures;
- Tasks/Runs;
- Action Inbox;
- capabilities/skills/MCP;
- commands/settings;
- contacts/communication threads;
- events/automations;
- remote/offline devices.

Global Search не должен копировать всё в одну бесконтрольную vector database и не должен выдавать stale cached result как authority.

## H.2. Главная модель

> Global Search — query federation и result fusion над domain-specific indexes. Каждый результат сохраняет source, authority, scope, freshness и способ открыть канонический объект.

## H.3. Searchable Source Contract

Каждый domain adapter объявляет:

```yaml
search_source:
  source_id: id
  domain: typed
  scopes: []
  query_modes: [exact, lexical, semantic, structured, temporal]
  freshness_model: typed
  offline_behavior: typed
  result_schema: ref
  open_resolver: command_or_uri
  authority_description: text
  health: typed
```

## H.4. Query Intent

Search query может содержать:

- free text;
- exact quoted term;
- type filters;
- project/person/device;
- time range;
- current vs historical;
- source scope;
- privacy/local-only;
- desired action: navigate, answer, compare, command.

Query planner starts cheap:

1. command/entity exact search;
2. lexical indexes;
3. structured filters;
4. semantic lanes only if needed;
5. remote/cold expansion on demand.

## H.5. Query classes

### Navigation

«Открой проект Dennett», «найди run X».

### Exact lookup

File path, commit, ID, contact, error string.

### Semantic memory

«Где я говорил, что мне не нравится этот стиль?»

### Current state

«Какая версия модели сейчас выбрана?»

### Historical

«Что мы решили в июне?»

### Cross-domain

«Покажи статью, после которой мы изменили архитектуру, и связанный commit».

### Action/command

«Создать проект» should resolve to command, not a random memory note.

## H.6. Result Envelope

```yaml
federated_search_result:
  result_id: id
  source_ref: ref
  object_ref: ref
  object_type: typed
  title: text
  snippet_or_preview: text
  match_reasons: []
  score_components: {}
  scope: ref
  freshness: typed
  authority: typed
  observed_at: optional
  sensitivity: typed
  open_command: ref
  availability: local | remote | offline_cached | unavailable
```

## H.7. Fusion and ranking

Different indexes have incomparable scores. Hybrid-search systems обычно объединяют lexical и semantic retrieval как разные источники кандидатов; Dennett принимает это как baseline, не как единственный engine. [[S41]] Reciprocal Rank Fusion combines ranked result sets without requiring calibrated scores and is a useful baseline. [[S40]]

Dennett ranking considers:

- exactness;
- intent/type match;
- project/current context;
- semantic similarity;
- recency/freshness where relevant;
- authority;
- user history/pins;
- source availability;
- sensitivity and permission;
- dedup/relationship diversity.

No single global score becomes permanent truth.

## H.8. Deduplication and grouping

Same underlying object may appear as:

- project file;
- artifact snapshot;
- memory evidence;
- chat attachment;
- screenshot OCR.

Search groups related representations and offers:

- canonical object;
- relevant version;
- evidence/source;
- derived views.

It must not erase meaningful distinct versions.

## H.9. Freshness and authority

Result displays:

- current/fresh;
- observed as of time;
- cached/offline;
- historical;
- possibly stale;
- deleted/revoked;
- unavailable source.

For current-state questions, search may retrieve candidate but final answer checks authoritative live source when required.

## H.10. Permissions and private results

Search applies Trust/Memory scopes before ranking/rendering.

- result count should not leak existence of hidden vault/project where policy forbids;
- snippets are redacted;
- local-only source may return only on corresponding device or secure peer channel;
- external untrusted content labelled;
- voice/public mode suppresses sensitive spoken results.

## H.11. Offline and partial search

Global search may return:

```text
12 local results
3 cached remote results
2 sources unavailable
```

User can:

- search available now;
- request remote fetch;
- queue search for reconnect;
- handoff to device holding source.

Partial failure is visible and does not invalidate available results.

## H.12. Search and answer

Search itself returns objects. «Ask Dennett» can build answer from selected results with evidence.

Separation prevents:

- model hallucination hidden behind search UI;
- inability to open source;
- expensive model call for simple navigation.

## H.13. Index lifecycle

Each source maintains:

- watermark/version;
- last indexed event;
- model/index version;
- rebuild state;
- lag;
- deletion obligations.

Derived global index can store routing metadata, but domain-specific content remains rebuildable and governed by source.

## H.14. Commands and settings search

Commands have:

- stable command ID;
- title/synonyms;
- context availability;
- permission/effect preview.

Search distinguishes `Run command` from `Open documentation/result`.

Dangerous commands never execute on Enter without exact configured UX/confirmation.

## H.15. Personalization

Allowed:

- recent projects;
- pinned items;
- frequent commands;
- current context.

Not allowed:

- burying exact result because model predicts another intent;
- exposing sensitive personal result without query relevance;
- making ranking unexplainable.

`Why this result` shows major reasons on demand.

## H.16. Failure modes

### Index stale

Show stale and offer refresh; direct open may validate current object.

### Index corrupted

Source remains usable; rebuild; search partial.

### Embedding model changed

Parallel rebuild/dual index; no all-or-nothing outage.

### Source deleted

Remove active index entries; preserve tombstone where allowed.

### Duplicate object IDs

Stable namespace/source IDs prevent collision.

### Query too broad

Progressive refinement/type chips; do not dump thousands of results into LLM.

## H.17. Evaluation

Benchmark queries across:

- exact names;
- typos;
- semantic memory;
- temporal current/history;
- cross-domain chains;
- privacy exclusions;
- offline partial;
- deleted items;
- commands.

Metrics:

- MRR/nDCG/recall;
- exact top-1;
- stale-current error;
- open success;
- latency;
- model calls;
- sensitive leak rate;
- duplicate rate;
- user reformulations.

## H.18. Антиоверинижиниринговые ограничения

Не создавать:

- one giant vector store as authority;
- graph traversal for every query;
- LLM router for exact command/file lookup;
- mandatory global reindex before app usable;
- hidden remote fetch that leaks data;
- independent copy of full object in search index;
- universal score calibration before RRF baseline measured.

## H.19. Критерии готовности

- source adapters defined;
- exact/lexical/semantic lanes coexist;
- authority/freshness visible;
- permission applied before result disclosure;
- partial/offline works;
- command search separate from knowledge answer;
- result opens canonical object;
- indexes rebuildable/deletion-aware.

## H.20. Карта будущего переноса

- `10 Memory`: memory search lanes/evidence.
- `50 Server`: federation, source health, indexing jobs.
- `60/61 UI`: command/global search UX.
- `41 Capability`: external search/connectors.
- architecture data volume: physical indexes/query API.

---
