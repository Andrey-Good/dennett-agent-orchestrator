# WP-M01-003 Design QA

## Evidence

- Visual sources: the owner's three private attachments from the second checkpoint. They are intentionally not tracked because they contain private workspace content.
- Owner contract: `docs/design/WP-M01-003/owner-direction-v2.md`.
- Implementation screenshot: `docs/design/WP-M01-003/project-chat-monochrome-v2-1536x971.png`.
- Browser URL during review: `http://127.0.0.1:5173/`.
- Browser viewport and state: 1536 x 971, dark theme, `streaming` fixture, project/chat navigation open, workspace resources open, full plan collapsed.
- Responsive checks: 1100 x 800 and 900 x 800.
- Native check: the official Tauri release pipeline embedded the production frontend, and the resulting standalone executable opened on Windows at its configured 1280 x 800 size without a dev server. The transparent custom chrome, Acrylic window material and custom close control were observed in the native window.
- Implementation commits: `542078a` (review corrections), `0d5997e` (reproducible standalone Tauri build) and `eed2138` (resource-contrast closure) on `codex/wp-m01-003-project-chat-screen`.

The three private references and the implementation capture were inspected together in one comparison input. The full-window source establishes the quiet Codex-like density and unified chrome; the project-list crop establishes projects with nested chats followed by standalone recent chats; the results-pane crop establishes the resource grouping and compact rounded surface.

No additional focused crops were needed: two owner attachments are already dedicated, legible crops of the project/chat navigation and resource workspace, while the full implementation capture keeps typography, composer controls and all persistent boundaries readable. Browser DOM inspection supplied exact labels and interaction states that are too small to read reliably in the scaled full-window preview.

## Findings

No actionable visual P0, P1 or P2 findings remain. The required detached R2 closure re-review passed against implementation commit `b48520d`; it independently reran the desktop suite (14/14) and confirmed that all prior findings are closed without introducing a new visual, interaction or accessibility regression.

- Typography: Segoe UI/system fallbacks, restrained 10-13 px metadata and 13 px conversation copy preserve the compact desktop hierarchy without clipped controls or broken wrapping at the reviewed sizes.
- Layout and spacing: the title row, 52 px rail and 264 px project/chat sidebar read as one surface. The partial-height divider, centered conversation, anchored composer and 316 px resource workspace remain aligned. At 1100 px the workspace becomes a truthful closable overlay; at 900 px both side panes remain operable overlays with no document-level horizontal or vertical overflow.
- Color and surfaces: CSS palette checks found only equal-channel hexadecimal and RGBA colors. There are no purple, blue or other chromatic state accents. User messages use gray bubbles; agent messages remain unboxed. Rounded borders and surface count were reduced to persistent controls and resource grouping.
- Native material: the web preview stays opaque enough for repeatable screenshots. The native shell uses a transparent Tauri window, Windows Acrylic and more translucent native-only CSS layers; reduced-transparency mode retains an opaque neutral fallback.
- Icons and imagery: visible interface icons use Phosphor Icons. The provisional application icon is a generated monochrome raster source converted by the Tauri icon pipeline into platform assets; it is not treated as approved branding.
- Copy and truthfulness: the UI names Codex SDK as the only available source, labels unavailable features as later work, keeps external effects read-only and does not imply that Git, plugins, voice or provider routing are implemented.
- Accessibility: semantic landmarks and names are present; Command Center traps and restores focus; access and runtime popovers expose `aria-controls`, focus their first choice and return focus on Escape; disabled controls are truthful; reduced-motion, reduced-transparency and higher-contrast fallbacks are present. Structural checks pass in axe, while a separate deterministic token test verifies WCAG AA text contrast because jsdom cannot calculate rendered CSS contrast reliably.

## Comparison History

### Rejected checkpoint

- The owner rejected chromatic purple tint, browser-like Back/Forward controls, `Local node`, the metrics inspector, the project/session mode switch, redundant composer scope and the detached top-right account controls.
- The owner retained the minimal first visual direction and requested a closer Codex-like project/chat hierarchy, resource pane, embedded viewers, rounded geometry and strictly achromatic glass.

### Monochrome workbench v2

- Removed every chromatic token and introduced a CSS palette invariant check.
- Rebuilt the left side as a narrow activity rail plus projects with nested chats and standalone recent chats, sharing a bottom account/update/voice dock.
- Added the selected chat to breadcrumbs, kept one Command Center and replaced browser navigation with native window controls.
- Replaced the metrics inspector with Results, Subagents, Browser and Sources; the current plan step expands only on hover, focus or activation.
- Connected result, browser and source fixtures to a central read-only viewer and added a compact collapsed resource dock.
- Replaced composer scope clutter with context, plugin, access and truthful Codex runtime controls.
- Added native Acrylic configuration, generated app resources and a standalone desktop-shell Cargo boundary so the release executable can be built and opened.

### Detached R2 findings and corrections

- Raised all faint metadata to an achromatic token that remains at or above 4.5:1 on the lightest supported dark surface, removed the lower-contrast timestamp and heading overrides, and added a deterministic WCAG contrast test.
- Replaced one global local-message list with per-session draft storage. New chat now creates a distinct temporary session instead of reusing the owner-checkpoint session; switching chats cannot leak a locally submitted draft.
- Expanded component coverage from spot checks to explicit assertions for every streaming, restored, cached, stopped, timed-out, stale, resyncing, loading and empty fixture. Loading now shows only its skeleton rather than the empty-session prompt.
- Connected access/runtime triggers to their popovers, moved keyboard focus directly into the first choice and restored it to the trigger on Escape.
- Caught a packaging regression during the native launch check: a raw Cargo build still targeted the development URL and showed `ERR_CONNECTION_REFUSED`. The Tauri hooks now invoke the pinned package manager through Corepack, and the official release build was rerun; the rebuilt executable opened the embedded production UI with no dev server running.
- The first closure re-review found two remaining hard-coded resource-heading grays below AA contrast. All solid text foregrounds now route through contrast-tested semantic tokens, a regression test rejects literal text-color hex values, and the 1536 x 971 implementation was recaptured and compared with all three owner references. The brighter resource headings preserve the reference hierarchy without introducing a chromatic accent or a new visual mismatch.
- The final detached R2 re-review at `b48520d` returned PASS. It verified the semantic-token correction and literal-color guard, reran all 14 desktop tests and found no remaining P0, P1 or P2 issue. This closes independent implementation review; the separate owner visual-approval gate remains open.

## Primary Interactions Tested

- Opened Command Center, verified initial focus, focus wrapping, Escape closure and focus restoration.
- Opened access and runtime controls, verified `aria-controls`, first-choice focus and Escape focus restoration, then switched `Full access` to `Auto-approve` and reasoning from High to Medium.
- Submitted a local preview message, verified that it disappears in another session and returns only with its owning session, then created a distinct empty standalone chat.
- Expanded and collapsed the current plan.
- Collapsed and reopened workspace resources.
- Opened the Browser resource in the central viewer and returned to chat.
- Verified deterministic streaming, restored, cached, stopped, timed-out, stale, resyncing, loading and empty fixtures in component tests.
- Checked 1100 x 800 and 900 x 800 geometry: no document overflow; both compact overlays retain controls.
- Built the standalone Tauri release executable through the official pipeline, opened the real Windows window without a dev server, observed the embedded UI/Acrylic chrome in the native capture and verified the custom Close control terminates the app.

## Verification

- Desktop typecheck: passed.
- Desktop tests: 14 passed, including all nine fixture states, per-session draft isolation, distinct new-chat creation, popover focus, deterministic WCAG contrast and a semantic text-color invariant.
- Desktop production build: passed.
- Automated structural accessibility and deterministic WCAG token-contrast checks: passed.
- Monochrome CSS token scan: passed.
- Official Tauri release build without installer bundling: passed; standalone launch and custom Close behavior passed.

## Follow-up Polish

- P3: the provisional Dennett icon needs a separate owner branding decision before it becomes final identity.
- P3: tune Acrylic opacity after owner review on the target display; reduced transparency already has a safe opaque fallback.
- P3: resource-pane density can be retuned when real browser, PDF and Git artifacts replace presentation fixtures.

## Owner Gate

Do not merge WP-M01-003 until the owner accepts this second visual checkpoint. Approval covers the visual direction and presentation interactions only; it does not approve real provider, browser, file, PDF or Git effects.

final result: passed
