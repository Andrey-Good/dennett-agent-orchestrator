[English](#english) | [Russian](#russian)

<a id="english"></a>
# Phase 15 Full User Interaction Layer

Status: normative owner for the implemented Phase 15 interaction slice.

## Purpose

Phase 15 exists to close the smallest honest gap left after Phase 14 in the user-interaction model without claiming end-to-end behavior that the current executable slice still cannot safely expose.

## Implemented Slice

The current Phase 15 slice is intentionally narrow but now end-to-end on the supported Codex adapter path:

1. The Codex App Server adapter now understands App Server `item/tool/requestUserInput` requests when they can be mapped to the existing built-in `orchestrator.user_chat` contract.
2. The adapter now exposes blocking built-in prompts as runtime events so Core can persist a durable `waiting_for_user` / `pending_prompt` boundary.
3. The CLI reply path records the reply into run state and resumes the same run from stored prompt state.
4. The supported request subset is:
   - exactly one question;
   - non-secret text prompts; or
   - closed options prompts with no mixed free-form `isOther` path.
5. Closed App Server options are normalized to the current explicit option-reply contract by using stable local option IDs and label-string values.
6. Same-session live reply delivery remains best-effort for the original App Server turn, but it is no longer required for the supported product path to complete honestly.

## What This Slice Does Not Claim

This slice does not claim broader user-interaction completion than the supported Codex adapter path actually proves.

Specifically:

- `supports_builtin_user_chat_mcp` is only enabled on the supported Codex adapter path, not as a generic runtime guarantee;
- the supported path is still intentionally narrow and only covers the prompt shapes listed above;
- live same-session prompt delivery is opportunistic, not the source of truth for resume;
- the slice does not claim broader interaction features beyond durable prompt wait/reply/resume for built-in user chat.

## Boundary Rules

- The implemented support is durable and state-backed through Core.
- Unsupported App Server prompt shapes must fail explicitly rather than being silently degraded.
- Secret prompts and mixed options-plus-freeform prompts remain unsupported in the current slice.
- Docs, tests, and capability reporting must stay aligned with that boundary.

## Evidence Level

The current slice is backed by:

- focused adapter tests for supported text and options prompts;
- focused adapter tests for unsupported prompt shapes;
- focused adapter tests for blocking prompt events and same-session reply delivery;
- focused graph-runner tests for durable `waiting_for_user` state and reply/resume;
- focused CLI-path coverage for the reply/resume contract;
- `typecheck` and the repo test suite.

This document does not claim broader live proof than that evidence supports.

<a id="russian"></a>
# Phase 15 Full User Interaction Layer

Статус: нормативный owner-документ для реализованного interaction-среза Phase 15.

## Назначение

Phase 15 нужен затем, чтобы закрыть самый узкий честный разрыв, оставшийся после Phase 14, в пользовательской interaction-модели, не заявляя end-to-end поведение, которое текущий executable slice пока не может безопасно открыть.

## Реализованный Срез

Текущий срез Phase 15 намеренно узкий, но теперь end-to-end на поддерживаемом Codex adapter path:

1. Codex App Server adapter теперь понимает App Server-запросы `item/tool/requestUserInput`, когда их можно отобразить в существующий built-in контракт `orchestrator.user_chat`.
2. Adapter теперь surface-ит blocking built-in prompts как runtime events, чтобы Core мог persist-ить durable `waiting_for_user` / `pending_prompt` boundary.
3. CLI reply path записывает reply в run state и возобновляет тот же run из сохраненного prompt state.
4. Поддерживаемое подмножество запросов:
   - ровно один вопрос;
   - не-secret text prompt; или
   - closed options prompt без смешанного free-form пути `isOther`.
5. Closed App Server options нормализуются в текущий контракт explicit option-reply через стабильные локальные option ID и label-string values.
6. Same-session live reply delivery остается best-effort для исходного App Server turn, но больше не требуется, чтобы поддерживаемый product path честно завершался.

## Чего Этот Срез Не Заявляет

Этот срез не заявляет более широкое завершение user-interaction, чем реально доказывает поддерживаемый Codex adapter path.

Конкретно:

- `supports_builtin_user_chat_mcp` включен только на поддерживаемом Codex adapter path, а не как generic runtime guarantee;
- поддерживаемый path остается намеренно узким и покрывает только перечисленные выше prompt shapes;
- live same-session prompt delivery является opportunistic, а не source of truth для resume;
- срез не заявляет более широкие interaction features сверх durable prompt wait/reply/resume для built-in user chat.

## Правила Границы

- Реализованная поддержка является durable и state-backed through Core.
- Неподдерживаемые формы App Server prompt-ов должны завершаться явной ошибкой, а не тихой деградацией.
- Secret prompt-ы и смешанные options-plus-freeform prompt-ы остаются unsupported в текущем срезе.
- Документация, тесты и capability reporting обязаны оставаться согласованными с этой границей.

## Уровень Доказательств

Текущий срез подтвержден:

- focused adapter tests для поддерживаемых text- и options-prompt-ов;
- focused adapter tests для неподдерживаемых форм prompt-ов;
- focused adapter tests для blocking prompt events и same-session reply delivery;
- focused graph-runner tests для durable `waiting_for_user` state и reply/resume;
- focused CLI-path coverage для reply/resume contract;
- `typecheck` и repo test suite.

Этот документ не заявляет более широкого live proof, чем реально подтверждает этот набор доказательств.
