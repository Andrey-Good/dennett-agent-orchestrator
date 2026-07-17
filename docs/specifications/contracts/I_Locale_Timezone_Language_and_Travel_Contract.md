# Модуль I. Locale, Timezone, Language and Travel Contract

> **Канонический cross-domain supplement · `I`**  
> **Primary owner:** 50 Server Runtime.  
> **Происхождение:** выделено из предархитектурного gap-аудита. Документ актуален и нормативен.  
> Маркеры источников вида `[[Sxx]]` раскрыты в [`REFERENCES.md`](REFERENCES.md). Ownership и порядок чтения описаны в [`README.md`](README.md).


## I.1. Назначение

Dennett работает с голосом, schedules, events, messages, projects, memories and multiple devices. Поэтому нельзя использовать:

- implicit server timezone;
- UI language как язык пользователя во всех contexts;
- fixed UTC offset вместо timezone;
- «завтра в 8» без reference context;
- автоматический перевод без сохранения original.

## I.2. Четыре независимых понятия

### Locale

Формат дат, чисел, валюты, plural rules, units.

### Language

Язык текста/речи/content.

### Timezone

IANA zone rule set, например `Europe/Helsinki`, а не только `UTC+03:00`.

### Region/Policy

Региональные provider/legal/storage defaults. Не выводится надёжно только из языка.

CLDR предоставляет locale data для дат, чисел, units и plural rules; BCP 47 используется для language tags; IANA tzdb обновляется при политических изменениях offsets/DST. [[S42]] [[S43]] [[S44]]

## I.3. User and Device Profile

```yaml
locale_profile:
  preferred_ui_locales: []
  preferred_response_languages: []
  default_timezone: iana_zone
  home_timezone: optional
  date_time_style: typed
  number_currency_preferences: {}
  measurement_system: typed
  translation_policy: typed
```

Device reports local zone/locale as signal, not automatically global owner preference.

## I.4. Timestamp storage

Every instant stores:

- UTC/RFC3339-compatible instant;
- source timezone if user-facing/scheduled;
- original local expression where meaningful;
- tzdb version/interpretation where reproducibility matters.

RFC 3339 provides interoperable internet timestamp representation; recurrence and local scheduling still need IANA timezone semantics. [[S45]]

## I.5. Natural language time

For «завтра в восемь» resolve using:

- speaker/session device timezone;
- current date at utterance;
- user travel state;
- project/event timezone;
- conversation context;
- ambiguity policy.

Stored as:

```yaml
temporal_intent:
  original_expression: text
  reference_instant: timestamp
  reference_timezone: iana_zone
  resolved_instant: optional
  recurrence_rule: optional
  ambiguity: []
  resolution_basis: []
```

## I.6. Travel

When device timezone changes:

- update current presence signal;
- do not rewrite home timezone;
- upcoming schedules categorized:
  - anchored to local wall time;
  - anchored to absolute instant;
  - anchored to project/location timezone;
  - ask on ambiguity.

Examples:

- «каждый день в 9 утра» usually follows current/local or chosen home policy;
- flight at 14:00 airport local time belongs to location;
- server backup at 02:00 server zone may stay fixed;
- deadline UTC remains absolute.

## I.7. DST and tzdb updates

- recurrence stores timezone ID, not future precomputed UTC forever;
- next occurrences recalculate using current tzdb;
- already executed historical events retain interpreted instant;
- tzdb update that changes future schedule produces review only if material;
- ambiguous/nonexistent local times follow explicit policy (earlier/later/skip/ask).

## I.8. Multi-language conversation

Voice/text can:

- detect language per turn;
- maintain chosen response language;
- switch when user switches intentionally;
- preserve names/code/quotes;
- avoid changing UI locale automatically.

Low-confidence detection asks or follows session default.

## I.9. Memory and translation

Memory preserves original content.

Derived translations:

- linked to source;
- language/model/version/date;
- not treated as exact quote;
- searchable across languages;
- retranslated if quality improves.

User correction updates translation preference, not original evidence.

## I.10. External communication

Response language chosen from:

- thread language;
- relationship history;
- explicit user command;
- recipient preference;
- current content.

Do not translate sensitive/legal text automatically without indication.

## I.11. Locale-sensitive tools

Tool/action parameters use canonical formats:

- amounts with currency code;
- decimal separator normalized;
- units explicit;
- dates structured;
- addresses retain locale.

Rendered UI can localize, but Action Request exact values remain unambiguous.

## I.12. Scheduling across devices

Head is authority for resolved schedule. Device may create offline temporal intent and sync later with original expression/reference time.

If sync after intended time:

- execute late only if policy;
- notify;
- skip;
- reschedule;
- never silently pretend it ran on time.

## I.13. Failure scenarios

### Incorrect device clock

Use authenticated server time where available; preserve source observed time and confidence.

### Timezone unavailable/offline

Use last known zone and mark; do not infer solely from IP for consequential schedule.

### User moves during voice session

Session timezone fixed unless user says otherwise; future commands can use updated context.

### Mixed-language ASR

Store audio/original hypotheses; avoid translating code/identifiers.

## I.14. UI requirements

UI later must show timezone for:

- scheduled external action;
- deadline with remote participants;
- recurring automation;
- imported event;
- travel ambiguity.

Routine local timestamps can remain concise with timezone on detail.

## I.15. Evaluation

- DST transition tests;
- ambiguous/nonexistent local time;
- travel zone changes;
- offline late sync;
- multilingual turns;
- translated search;
- amount/date parsing;
- user correction;
- provider timestamps.

## I.16. Антиоверинижиниринговые ограничения

Не создавать:

- own timezone database;
- LLM for standard date formatting;
- automatic global language switch from one foreign phrase;
- permanent translation copies for everything;
- legal region inference solely from GPS/IP.

## I.17. Критерии готовности

- IANA timezone stored;
- original temporal expression preserved;
- absolute vs wall-time semantics explicit;
- travel/DST covered;
- BCP47 languages and CLDR locale concept separated;
- original content retained alongside translations;
- structured action parameters locale-safe.

## I.18. Карта будущего переноса

- `50 Server`: schedules/event time/tzdb update.
- `40 Voice`: per-turn language/time interpretation.
- `10 Memory`: original/translation/time provenance.
- `60/61 UI`: locale rendering/travel prompts.
- `B communication`: thread language/scheduled send.


---
