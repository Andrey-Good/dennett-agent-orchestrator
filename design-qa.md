# WP-M01-003 Design QA

## Evidence

- Source visual truth path: private owner attachment `image-1.png`; the original is intentionally not tracked because it contains private workspace content.
- Implementation screenshot path: `docs/design/WP-M01-003/project-chat-streaming-1280x720.png`.
- Implementation URL: `http://127.0.0.1:1420/` during the review session.
- Implementation commit: `8d6cbf6`.
- Viewport: 1600 × 900 CSS viewport request with a 1280 × 720 browser capture; both it and the private 2558 × 1438 source use the same effective 16:9 content ratio.
- State: dark theme, Project Chat, `streaming` fixture, inspector open, Sessions sidebar selected.
- Browser-rendered evidence: captured in the Codex in-app browser from the Vite renderer.

The source and implementation were opened together in one comparison input. The implementation preserves the source's dark, quiet workbench composition, narrow left navigation, restrained centered conversation, gray user surfaces, unboxed agent responses and bottom composer. The fixed right inspector and muted periwinkle state accent are intentional Dennett product adaptations requested by the owner rather than source drift.

Focused crops were not required: both full-resolution captures made the title/rail/sidebar relationship, chat typography, message surfaces, composer controls, inspector icons and all important spacing boundaries readable. The browser DOM and interaction checks were used for labels and states that are not visually legible at reduced preview scale.

## Findings

No actionable P0, P1 or P2 findings remain.

- Fonts and typography: the system UI stack matches the reference's neutral desktop character. Body copy is 13 px with a 1.65 line height; small persistent metadata was raised to 10 px or above after the first comparison.
- Spacing and layout rhythm: the rail and sidebar share the title-row surface; chat content remains centered and the composer stays anchored without covering the latest turn. The inspector is a fixed work pane by product requirement, unlike the source's floating result card.
- Colors and visual tokens: the near-black canvas, gray user messages, unboxed agent output, low-contrast dividers and translucent chrome match the selected direction. State is never conveyed by color alone.
- Image quality and assets: the screen requires no photo or illustration assets. All visible interface icons come from Phosphor Icons; no handcrafted SVG, CSS icon art or emoji substitutes are used.
- Copy and content: fixture copy explains the M01 boundary without pretending Git, file mutation or provider completion is live.
- Accessibility: visible focus styles, semantic landmarks, named controls, bounded live announcements, reduced motion/transparency modes and target sizes are present. Automated axe checks exclude only color contrast because jsdom cannot compute rendered pixels; contrast was inspected in the browser capture.

## Comparison History

### Iteration 1

- Earlier P2 finding: persistent metadata and conversation copy were visibly smaller and lighter than the source at the review viewport.
- Earlier P2 finding: increasing the type scale exposed that the initial conversation opened at the oldest visible turn, leaving the newest turn clipped above the composer.
- Fixes: raised the main conversation, session, inspector and composer text scale; preserved compact hierarchy for nonessential metadata; followed the latest turn on snapshot changes; kept the runtime notice sticky without moving keyboard focus.
- Post-fix evidence: `docs/design/WP-M01-003/project-chat-streaming-1280x720.png`, commit `303523c`.

### Iteration 2

- The source and post-fix screenshot were compared together again.
- No P0, P1 or P2 mismatch remained. The latest turn and Stop control are visible, type is readable, and no persistent control overlaps or clips.

### Iteration 3 — detached R2 closure

- Earlier P2 finding: Command Center declared modal semantics without containing or restoring focus.
- Earlier P2 finding: enabled preview controls without behavior created false affordances.
- Earlier P2 finding: the transport-neutral client request accepted a fixture-specific identifier.
- Earlier P2 finding: the inspector was hidden only by CSS below 1220 px, leaving its visible-state control inconsistent.
- Fixes: added modal focus wrap and return-to-invoker behavior; disabled or converted unavailable controls to read-only labels; moved fixture choice into the fake-client factory while keeping `DennettClient` requests project/session-only; converted narrow side panes to truthful closable overlays; added focused regression tests.
- Post-fix evidence: `docs/design/WP-M01-003/project-chat-streaming-1280x720.png`, commit `8d6cbf6`; browser check at 1100 px reported a visible fixed 322 px inspector, a truthful Hide label, and successful close/reopen behavior.

## Primary Interactions Tested

- Switched deterministic fixtures through the native state selector and verified stale-state copy.
- Opened the Context inspector tab.
- Entered and locally submitted a draft without invoking a runtime effect.
- Opened Command Center and closed it with Escape.
- Verified Command Center wraps Shift+Tab and restores focus to its invoker.
- Verified the inspector remains visible and closable as an overlay at a 1100 px viewport.
- Verified the streaming Stop control is scoped to the visible session through component tests.
- Checked browser console warnings and errors: none.

## Open Questions

- Native Windows title-bar integration is outside this renderer package. The owner can choose native decorations, overlay chrome or custom controls after approving this visual direction.
- Product localization is not fixed by this checkpoint; fixture text is English while the shell remains localization-ready.

## Follow-up Polish

- P3: consider increasing the three tiny metric labels in the inspector if owner testing is primarily on high-DPI displays.
- P3: tune the right-pane width after real artifacts and code land in M02.

## Implementation Checklist

- Obtain owner visual acceptance before merging WP-M01-003.
- Preserve the provider-neutral `DennettClient` boundary when real IPC replaces fixtures.
- Re-run visual QA after native title-bar integration or material M02 inspector changes.

final result: passed
