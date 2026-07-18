# WP-M01-003 owner direction v2

This note records the owner corrections for the second Project Chat checkpoint. It owns visual and presentation decisions for this work package only; specifications 60 and 83 still own durable product semantics.

## Visual foundation

- Use a strictly achromatic palette: white, black and neutral gray only. No purple, blue, green, amber or red accents.
- The title row, activity rail and project/chat sidebar read as one translucent surface.
- Separate the narrow rail from the wider sidebar with a very light partial-height divider that does not reach the top or bottom edge.
- Use rounded persistent surfaces and restrained borders. Remove decorative cards, labels and metrics that do not help the current task.
- Keep user messages in a soft gray bubble. Agent messages remain unboxed.
- The browser checkpoint may only approximate glass over its own neutral substrate. The native Tauri shell must use a transparent window plus an OS compositor effect; reduced-transparency mode needs an opaque gray fallback.

## Left navigation

- Keep the narrow icon-only activity rail.
- The wider sidebar shows projects with their chats nested below each project, followed by recent standalone chats that belong to no project.
- Remove the M01 `Sessions / Project` segmented switch. Project-mode sections return only when they have real content; an empty or invented mode must not be exposed.
- Remove the duplicate project search field because the global Command Center already owns search.
- Add a small shared bottom dock under both left navigation columns with account identity, updates and future microphone access.

## Title row

- Remove browser-like Back and Forward controls.
- Breadcrumbs show the full current location, including the selected chat title.
- Retain one central search/command entry.
- Remove `Local node`; normal infrastructure health is not persistent chrome.
- Reserve the right side for contextual indicators only when a real need exists.
- Use a custom Windows control group (minimize, maximize/restore, close) in the native desktop shell.

## Main conversation

- Keep the conversation calm and centered with no redundant second header.
- Present runtime state as a compact, readable line rather than a prominent dashboard card.
- Preserve truthful cached, streaming, stopped, timed-out, stale, resyncing, loading and empty fixtures.
- Switching a resource or artifact may replace the central conversation with an embedded viewer; returning to chat must be obvious.

## Composer

- Remove the redundant `Project` scope control.
- Replace the paperclip with a plus control for context and a plug icon for plugins.
- Keep access policy beside the prompt with two owner-approved choices: `Full access` and `Auto-approve`.
- Use a compound runtime control that can eventually expose provider/source, model, reasoning level and provider-specific speed. Under the current runtime constraint it shows Codex SDK, provider default model and a configurable reasoning level without pretending other providers are available.
- Keep voice and send/stop controls at the right edge.

## Right workspace

- Replace the M01 metrics inspector with a collapsible workspace drawer inspired by the owner's result-pane reference.
- Show the current plan step above resources. Reveal the full plan on hover/focus or explicit activation.
- Group truthful presentation fixtures under Results, Subagents, Browser and Sources.
- When collapsed, retain a narrow icon dock with labels available through tooltips and accessible names.
- Clicking the browser/result/source fixture opens a real presentation-state viewer in the center. Git mutation and external browsing remain disabled by the M01 boundary.
- Put compact branch/worktree state at the bottom of this workspace rather than beneath the composer.

## Owner gate

The second checkpoint must be shown live before merge. Owner acceptance applies to the overall visual direction, navigation hierarchy, composer density and right-workspace behavior. It does not approve real provider, Git, browser, PDF or file effects, which remain later packages.
