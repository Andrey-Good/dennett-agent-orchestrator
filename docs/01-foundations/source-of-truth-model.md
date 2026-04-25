[English](#english) | [Русский](#russian)

## English

# Source Of Truth Model

Status: approved foundational specification.  
Owns: authoritative artifact rules for each major class of information in the system and conflict-resolution rules when representations disagree.  
Does not own: detailed storage schema, draft implementation workflow, or contract field syntax.  
Primary sources: [canonical specification](../../agent_orchestrator_final_spec_v2.md), [system boundaries](./system-boundaries.md), [glossary](./glossary.md), [lifecycle section](../07-lifecycle/README.md).

## Core Rule

Every major class of information must have one authoritative home. Derived indexes, caches, vendor sessions, and examples are allowed, but they must not quietly become competing truths.

The source-of-truth model is what prevents the project from drifting into "whatever copy was easiest to query at the moment."

## Authoritative Artifacts By Information Class

| Information class | Authoritative artifact | What derived systems may do | What they may not do |
| --- | --- | --- | --- |
| Agent definition | Portable `agent JSON` file | Index it, cache it, reference it, validate it | Replace it with a database-first model |
| Graph compatibility requirement | `graph_contract_version` inside the agent file | Check compatibility and reject unsupported versions | Guess or partially interpret an unsupported version |
| Agent logical version | `meta.agent_version` inside the agent file | Display it, compare it, include it in lifecycle UX | Confuse it with live revision or tool version |
| Local working activation state | Registry/lifecycle local metadata | Track which draft or live revision is active locally | Redefine the agent's canonical graph semantics |
| Chat and resume state | Local core storage | Store visible messages, continuation metadata, and operational resume data | Become the canonical source of the agent definition |
| Events and triggers | Separate event/trigger storage or configuration | Link triggers to agents and payloads | Move trigger definitions into the portable agent file |
| Runtime session internals | The runtime itself, for its own internal lifecycle | Expose normalized capability through adapters | Leak vendor-internal structures into the public agent model |
| Skill, MCP, and plugin contracts | The compatible runtime ecosystem | Store references and pass-through configuration | Invent a second universal contract owned by this project |
| Documentation rules | The owner document for the topic, under the canonical spec | Summarize or cross-link from README files and examples | Let examples or code comments silently replace normative docs |

## Conflict Resolution Rules

When two representations disagree, resolve them as follows:

1. If a local registry entry conflicts with the agent file, the agent file wins.
2. If local operational state conflicts with the portable agent definition, the operational state may control only chat/resume behavior, never the meaning of the agent graph.
3. If runtime-native session data conflicts with local orchestration assumptions, the orchestrator must normalize or invalidate the session rather than mutate the canonical agent model.
4. If an example conflicts with a spec document, the owner spec document wins and the example must be fixed.
5. If a focused document conflicts with the top-level canonical spec, fix the focused document; do not treat the contradiction as dual truth.

## Derived Representations That Are Allowed

The following derived representations are expected and healthy as long as they stay derived:

- Agent registries and indexes.
- Draft working copies and live revision metadata.
- Resume snapshots and chat metadata.
- Validation artifacts and cached parse results.
- Search indexes and UI-oriented summaries.

Each of these may improve usability and performance. None of them may silently become the new canonical agent model.

## Documentation Ownership As Source Of Truth

The documentation set itself follows the same rule:

- The [canonical specification](../../agent_orchestrator_final_spec_v2.md) is the top-level anti-contradiction anchor.
- A focused document owns one narrower topic once the topic is split out.
- README files summarize scope and route readers to owner documents.
- ADRs record why a contested decision was accepted, but the operational rule should still live in the owner spec for that topic.

## Implementation Consequences

- Design storage schemas as supporting structures around the file, not as replacements for it.
- Keep lifecycle metadata separate from portable artifact semantics.
- Keep vendor-native state behind adapters and normalization layers.
- Treat conflict handling as explicit logic, not as an invitation to "pick whichever copy looks newer."

## Russian

# Модель Source Of Truth

Статус: утвержденная foundational-спецификация.  
Владеет: правилами авторитетного артефакта для каждого крупного класса информации в системе и правилами разрешения конфликтов между представлениями.  
Не владеет: детальными storage schemas, workflow реализации drafts и синтаксисом contract fields.  
Основные источники: [каноническая спецификация](../../agent_orchestrator_final_spec_v2.md), [system boundaries](./system-boundaries.md), [glossary](./glossary.md), [раздел lifecycle](../07-lifecycle/README.md).

## Главное Правило

У каждого крупного класса информации должен быть один авторитетный дом. Производные индексы, кэши, vendor sessions и примеры допустимы, но они не должны тихо становиться конкурирующими источниками истины.

Именно модель source of truth не дает проекту скатиться в режим "истиной считается та копия, которую в этот момент было проще прочитать".

## Авторитетные Артефакты По Классам Информации

| Класс информации | Авторитетный артефакт | Что могут делать производные системы | Чего они не могут делать |
| --- | --- | --- | --- |
| Определение агента | Переносимый `agent JSON` file | Индексировать его, кэшировать, ссылаться на него, валидировать его | Заменять его database-first моделью |
| Требование совместимости графа | `graph_contract_version` внутри agent file | Проверять совместимость и отклонять неподдерживаемые версии | Догадываться, как интерпретировать неподдерживаемую версию |
| Логическая версия агента | `meta.agent_version` внутри agent file | Показывать ее, сравнивать, использовать в lifecycle UX | Смешивать ее с live revision или версией утилиты |
| Локальное состояние активации | Локальная registry/lifecycle metadata | Отслеживать, какой draft или live revision активен локально | Переопределять каноническую семантику графа агента |
| Chat и resume state | Локальное хранилище core | Хранить видимые сообщения, metadata продолжения и операционные resume-данные | Становиться каноническим source of truth для определения агента |
| Events и triggers | Отдельное storage/configuration для events и triggers | Связывать triggers с агентами и payloads | Переносить определения triggers внутрь переносимого agent file |
| Runtime session internals | Сам runtime для своего внутреннего lifecycle | Отдавать нормализованную capability через adapters | Утекать vendor-internal structures в публичную модель агента |
| Контракты skill, MCP и plugin | Экосистема совместимого runtime | Хранить ссылки и pass-through configuration | Изобретать второй универсальный контракт, которым владеет этот проект |
| Правила документации | Документ-владелец соответствующей темы под канонической спецификацией | Суммировать и ссылаться из README и examples | Позволять examples или code comments молча заменять нормативные docs |

## Правила Разрешения Конфликтов

Когда два представления расходятся, конфликт решается так:

1. Если локальная registry entry расходится с agent file, побеждает agent file.
2. Если локальное операционное состояние расходится с переносимым определением агента, операционное состояние может управлять только поведением chat/resume, но не смыслом графа агента.
3. Если runtime-native session data расходится с локальными предположениями оркестратора, оркестратор должен нормализовать или инвалидировать сессию, а не менять каноническую модель агента.
4. Если example расходится со spec-документом, побеждает документ-владелец, а example нужно исправить.
5. Если профильный документ расходится с верхнеуровневым каноном, исправлять нужно профильный документ; противоречие не создает две равноправные истины.

## Какие Производные Представления Разрешены

Следующие производные представления ожидаемы и полезны, пока они остаются производными:

- Agent registries и индексы.
- Рабочие drafts и metadata live revisions.
- Resume snapshots и chat metadata.
- Артефакты валидации и кэшированные результаты разбора.
- Search indexes и summaries для UI.

Каждый из этих слоев может улучшать удобство и производительность. Ни один из них не может незаметно стать новой канонической моделью агента.

## Владение Документами Как Source Of Truth

Сам набор документации подчиняется тому же правилу:

- [Каноническая спецификация](../../agent_orchestrator_final_spec_v2.md) — это верхнеуровневый анти-противоречивый якорь.
- Профильный документ начинает владеть более узкой темой после ее выделения из общего канона.
- README-файлы суммируют scope и направляют читателя к документу-владельцу.
- ADR фиксирует, почему спорное решение было принято, но операционное правило все равно должно жить в профильной спецификации этой темы.

## Последствия Для Реализации

- Проектируйте storage schemas как поддерживающие структуры вокруг файла, а не как его замену.
- Держите lifecycle metadata отдельно от семантики переносимого артефакта.
- Держите vendor-native state за adapters и normalization layers.
- Рассматривайте обработку конфликтов как явную логику, а не как приглашение "выбрать ту копию, которая выглядит новее".
