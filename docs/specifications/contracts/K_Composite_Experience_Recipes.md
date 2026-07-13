# Модуль K. Composite Experience Recipes

> **Канонический cross-domain supplement · `K`**  
> **Primary owner:** 20 Agentic Control and 41 Capability Fabric.  
> **Происхождение:** выделено из предархитектурного gap-аудита. Документ актуален и нормативен.  
> Маркеры источников вида `[[Sxx]]` раскрыты в [`REFERENCES.md`](REFERENCES.md). Ownership и порядок чтения описаны в [`README.md`](README.md).


## K.1. Назначение

Некоторые важные пользовательские функции Denet не должны становиться отдельными подсистемами. Они являются **recipes** поверх существующих primitives:

```text
prompt/behavior profile
+ skill/procedure
+ memory query
+ optional event/automation
+ artifact template
+ project/context bindings
```

Recipe описывает user-visible behavior и requirements, но не создаёт отдельную canonical database.

## K.2. Recipe Definition

```yaml
experience_recipe:
  recipe_id: id
  name: text
  purpose: text
  activation: manual | context | schedule | event
  required_capabilities: []
  context_query: optional
  prompt_or_skill_refs: []
  output_artifact: optional
  memory_commit_policy: typed
  autonomy_policy: typed
  budget_profile: optional
  user_customizable: boolean
```

Home Assistant blueprints являются полезным precedent reusable automation templates with inputs, import and user customization; Denet recipes расширяют идею AI skills/context, но не превращают любую recipe в strict automation graph. [[S49]]

## K.3. Idea Incubator

### Purpose

Сохранять сырые идеи без немедленного превращения в проект или Task.

### Inputs

- voice note;
- text;
- screenshot/photo;
- link;
- fragment from conversation;
- ambient candidate explicitly committed.

### Behavior

- save original;
- short optional title/tags/project link;
- detect duplicates/related ideas lazily;
- no forced schema;
- periodic review optional;
- statuses are lightweight facets, not mandatory workflow.

### Actions

- develop;
- merge with idea;
- create project;
- research;
- archive;
- ignore;
- remind later.

## K.4. Concept Distiller

Transforms long voice/chat stream into selectable projections:

- cleaned transcript;
- concept map;
- functions;
- principles;
- assumptions;
- contradictions;
- unresolved questions;
- requirements;
- architecture input;
- document draft.

Original stream remains evidence. Distilled result is artifact/projection and user can correct it.

## K.5. Thinking Editor

Behavior profile for collaborative reasoning:

- catches contradiction;
- distinguishes desire vs implementation;
- asks high-value questions;
- identifies hidden assumptions;
- proposes alternative formulation;
- preserves user's ownership of conclusion;
- does not spawn team by default.

Can run in text or voice. No special storage beyond conversation + optional artifact/memory.

## K.6. Daily Briefing

Automation/recipe:

```text
scheduled or user-invoked event
→ retrieve current projects, Inbox, calendar, promises, notifications and overnight outcomes
→ rank by user attention policy
→ generate compact text/voice artifact
→ deliver through chosen channel
→ no action unless separately authorized
```

User controls sections, time, length and delivery. If nothing important, briefing can be empty/silent.

## K.7. Evening Debrief and Monthly Retrospective

Uses history/current outcomes to surface:

- completed work;
- unresolved commitments;
- patterns;
- changed interests/preferences;
- repeated friction;
- potential skill/policy improvements.

Conclusions remain proposals with evidence; no psychological diagnosis.

## K.8. AI News Monitor / Technology Radar

Recipe over World Intelligence Memory:

```text
source subscriptions/search schedule
→ deduplicate articles/posts/papers/releases
→ extract claims and versions
→ assess source quality/freshness
→ match to active projects and interests
→ store evidence
→ notify only high-value delta or include digest
→ allow project experiment/research
```

Important rules:

- tweet is signal, not truth;
- product facts revalidate before use;
- news item does not auto-install capability;
- project-fit ranking uses requirements;
- no separate `News Database` if World Intelligence already holds claims/evidence.

## K.9. Research Dossier

Skill/preset that creates artifact with:

- question;
- decision context;
- sources;
- claims;
- support/contradictions;
- uncertainty;
- gaps;
- recommendation;
- freshness.

Execution remains one strong agent by default; parallel helpers only for independent source branches.

## K.10. Meeting Summary

Voice profile + skill:

- silent capture/diarization;
- transcript evidence;
- decisions/promises/action candidates;
- private vs shareable notes;
- participant review optional;
- create tasks only after policy.

No separate meeting subsystem.

## K.11. Taste Review

Retrieves relevant examples/preferences/negative reactions, then evaluates artifact separately from objective quality.

Output distinguishes:

- objective constraints;
- alignment with user's taste;
- uncertainty;
- references.

Does not make global taste schema mandatory.

## K.12. Red-Team Preset

Behavior profile/skill applied to selected artifact/plan/code:

- failure modes;
- abuse cases;
- assumptions;
- verification gaps;
- rollback.

Does not run after every trivial response. User/project policy determines when.

## K.13. Recipe discovery and creation

Denet can notice repeated user sequence and propose recipe/skill:

- evidence of repetition;
- expected saved effort;
- simplest representation;
- project-local first;
- user can reject;
- no automatic proliferation.

## K.14. Recipe customization

User may edit natural language, source list, schedule, output and autonomy. Advanced implementation details remain hidden unless needed.

## K.15. Evaluation

Each recipe evaluated by its outcome:

- briefing usefulness/dismissal;
- news project relevance;
- concept correction rate;
- research citation quality;
- meeting action accuracy;
- token/attention cost.

Unused/noisy recipe is disabled or simplified.

## K.16. Антиоверинижиниринговые ограничения

- no dedicated microservice/database per recipe;
- no fixed schemas for all ideas/taste/people;
- no mandatory workflow builder;
- no always-on research swarm;
- no notification for every news item;
- no global skill promotion from one success.

## K.17. Карта будущего переноса

- `20 Agentic`: behavior profiles and execution choice.
- `41 Capability`: skills/recipe packages.
- `10 Memory`: World Intelligence, queries, evidence.
- `50 Server`: schedules/events/delivery.
- `40 Voice`: voice recipes.
- `60/61 UI`: presets and customization.


---
