# WP-M01-003 Design QA

## Evidence

- Visual sources: the owner's three private attachments from the second checkpoint and the annotated full-window correction from the third checkpoint. They are intentionally not tracked because they contain private workspace content.
- Owner contracts: `docs/design/WP-M01-003/owner-direction-v2.md` and `docs/design/WP-M01-003/owner-direction-v3.md`.
- Implementation screenshots: `docs/design/WP-M01-003/project-chat-owner-v3-2048x1280.png` (`streaming`), `docs/design/WP-M01-003/project-chat-owner-v3-empty-2048x1280.png` (`empty`) and `docs/design/WP-M01-003/shell-layer-after-v1.png` (unified shell correction).
- Browser URL during review: `http://127.0.0.1:5173/`.
- Browser viewport and state: 1536 x 971 and 2048 x 1280, dark theme, `streaming` and `empty` fixtures, project/chat navigation open, workspace resources open, full plan collapsed.
- Responsive checks: 1100 x 800 and 900 x 700.
- Native check: the official Tauri release pipeline embedded the production frontend, and the resulting standalone executable opened on Windows at its configured 1280 x 800 size. The transparent custom chrome, Acrylic material, system window controls, hover-only creation actions and open/collapsed workspace-panel icons were observed in the native window.
- Previous implementation closure: `eed2138` (resource-contrast correction) and `b48520d` (detached R2 closure) on `codex/wp-m01-003-project-chat-screen`. The third-checkpoint commit and detached closure are recorded after review below.

The annotated third-checkpoint reference, the selected Codex-like full-window source and the 1536 x 971 unified-shell implementation capture were inspected together in one comparison input. The full-window source establishes the quiet Codex-like density and unified chrome; the project-list crop establishes projects with nested chats followed by standalone recent chats; the results-pane crop establishes the resource grouping and compact rounded surface; the owner's shell-corner crop establishes that top and left chrome must form one lower layer beneath the conversation and resource surfaces.

No additional focused crops were needed: two owner attachments are already dedicated, legible crops of the project/chat navigation and resource workspace, while the full implementation capture keeps typography, composer controls and all persistent boundaries readable. Browser DOM inspection supplied exact labels and interaction states that are too small to read reliably in the scaled full-window preview.

## Findings

No actionable visual P0, P1 or P2 findings remain in the current comparison. Detached review of owner-correction commit `49f6ca2` found two behavioral P2 findings: the New Project trigger did not own its focus-restoration ref, and the Command Center created a standalone chat while a project was selected. Both were corrected with regression coverage, and detached closure review of exact commit `297eb20` returned PASS after rerunning desktop typecheck and all 16 desktop tests.

- Typography: Segoe UI/system fallbacks, restrained 10-13 px metadata and 13 px conversation copy preserve the compact desktop hierarchy without clipped controls or broken wrapping at the reviewed sizes.
- Layout and spacing: the title row, 52 px rail and 264 px project/chat sidebar read as one surface. The partial-height divider, centered conversation, anchored composer and 316 px resource workspace remain aligned. At 1100 px the workspace becomes a truthful closable overlay; at 900 px both side panes remain operable overlays with no document-level horizontal or vertical overflow.
- Color and surfaces: CSS palette checks found only equal-channel hexadecimal and RGBA colors. There are no purple, blue or other chromatic state accents. User messages use gray bubbles; agent messages remain unboxed. Rounded borders and surface count were reduced to persistent controls and resource grouping.
- Native material: the web preview stays opaque enough for repeatable screenshots. The native shell uses a transparent Tauri window, Windows Acrylic and more translucent native-only CSS layers; reduced-transparency mode retains an opaque neutral fallback.
- Icons and imagery: visible interface icons use Phosphor Icons. The provisional application icon is a generated monochrome raster source converted by the Tauri icon pipeline into platform assets; it is not treated as approved branding.
- Copy and truthfulness: the UI names Codex SDK as the only available source, labels unavailable features as later work, keeps external effects read-only and does not imply that Git, plugins, voice or provider routing are implemented.
- Accessibility: semantic landmarks and names are present; Command Center traps and restores focus; access and runtime popovers expose `aria-controls`, focus their first choice and return focus on Escape; disabled controls are truthful; reduced-motion, reduced-transparency and higher-contrast fallbacks are present. Structural checks pass in axe, while a separate deterministic token test verifies WCAG AA text contrast because jsdom cannot calculate rendered CSS contrast reliably.
- Runtime console: one initial Vite HMR WebSocket connection error was recorded before the server stabilized; later reloads connected successfully and no application exception or persistent console error remained.

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

### Owner-corrected workbench v3

- Integrated account, optional update and voice controls into the full left material with only a quiet separator; the update action is absent when no update exists.
- Made the Projects action hover/focus-only and connected it to a compact truthful menu for an empty preview project or an existing-folder preview. Real directory creation and the system folder picker remain M02 effects.
- Added hover/focus New Chat actions to each project and Recent, removed New Chat from the activity rail and renamed the rail destination to Chats.
- Reduced selected-chat height, joined the top and left materials without a full-width seam and added the rounded inner transition into the conversation canvas.
- Enlarged the composer access/runtime labels, narrowed both popovers, left-aligned Auto-approve and replaced generic empty prompts with the owner-requested project-idea and repository-inspection starters.
- Removed the duplicate titlebar resource toggle. The workspace header now owns a window-panel icon and the collapsed rail exposes the corresponding reopen icon.
- Lowered blur and opacity on the titlebar and resource workspace relative to the central canvas while preserving the achromatic palette and native Acrylic fallback.

### Detached R2 findings and corrections

- Raised all faint metadata to an achromatic token that remains at or above 4.5:1 on the lightest supported dark surface, removed the lower-contrast timestamp and heading overrides, and added a deterministic WCAG contrast test.
- Replaced one global local-message list with per-session draft storage. New chat now creates a distinct temporary session instead of reusing the owner-checkpoint session; switching chats cannot leak a locally submitted draft.
- Expanded component coverage from spot checks to explicit assertions for every streaming, restored, cached, stopped, timed-out, stale, resyncing, loading and empty fixture. Loading now shows only its skeleton rather than the empty-session prompt.
- Connected access/runtime triggers to their popovers, moved keyboard focus directly into the first choice and restored it to the trigger on Escape.
- Caught a packaging regression during the native launch check: a raw Cargo build still targeted the development URL and showed `ERR_CONNECTION_REFUSED`. The Tauri hooks now invoke the pinned package manager through Corepack, and the official release build was rerun; the rebuilt executable opened the embedded production UI with no dev server running.
- The first closure re-review found two remaining hard-coded resource-heading grays below AA contrast. All solid text foregrounds now route through contrast-tested semantic tokens, a regression test rejects literal text-color hex values, and the 1536 x 971 implementation was recaptured and compared with all three owner references. The brighter resource headings preserve the reference hierarchy without introducing a chromatic accent or a new visual mismatch.
- The final detached R2 re-review at `b48520d` returned PASS. It verified the semantic-token correction and literal-color guard, reran all 14 desktop tests and found no remaining P0, P1 or P2 issue. This closes independent implementation review; the separate owner visual-approval gate remains open.

### Owner-correction detached findings

- Detached review at `49f6ca2` found that closing the Projects menu could not reliably restore keyboard focus because the trigger ref was not attached to the rendered button.
- The same review found that the Command Center's chat-creation action ignored the currently selected project and always created a standalone Recent chat.
- The project trigger now receives the focus-restoration ref, with tests covering Escape and action-completion return paths. The Command Center now labels and creates its chat in the selected project, or explicitly creates a standalone chat when a Recent session is selected.
- Detached closure re-review of exact commit `297eb20` returned PASS. It verified both focus-restoration paths and selected-scope chat creation, reran desktop typecheck and all 16 desktop tests, and reported no remaining P0, P1 or P2 finding.

### Unified shell-layer correction

- Earlier evidence: `docs/design/WP-M01-003/shell-layer-before.png` showed a competing rounded top-right corner on the project sidebar and a separate titlebar seam. This made the top and left chrome look like adjacent cards instead of one continuous base surface.
- Fix: `apps/desktop/src/styles.css` now gives the workbench one achromatic glass base layer. The titlebar, activity rail and project sidebar are transparent children on that layer; the central workspace is a higher layer with a natural 22 px top-left radius; the resource panel remains its own quieter rounded layer. The responsive project drawer regains an independent surface only below 960 px, where it becomes an overlay.
- Post-fix evidence: `docs/design/WP-M01-003/shell-layer-after-v1.png` shows the titlebar and both left regions reading as one continuous background, with the central workspace and resource panel visibly laid over it. Combined visual comparison found no remaining P0, P1 or P2 mismatch in the corrected corner, separators, radii or surface hierarchy.
- Focused comparison: the owner's cropped shell-corner screenshot and the same top-left region of `shell-layer-after-v1.png` were compared in the same visual input. No additional crop was needed because both regions remain legible at original resolution.

## Primary Interactions Tested

- Opened Command Center, verified initial focus, focus wrapping, Escape closure, focus restoration and project-versus-standalone chat creation according to the selected scope.
- Opened access and runtime controls, verified `aria-controls`, first-choice focus and Escape focus restoration, then switched `Full access` to `Auto-approve` and reasoning from High to Medium.
- Opened the hover/focus-only Projects menu, exercised both creation choices as effect-free previews, verified focus restoration after Escape and action completion, and verified per-project and Recent New Chat placement in component and native interaction checks.
- Submitted a local preview message, verified that it disappears in another session and returns only with its owning session, then created a distinct empty standalone chat.
- Expanded and collapsed the current plan.
- Collapsed and reopened workspace resources.
- Opened the Browser resource in the central viewer and returned to chat.
- Verified deterministic streaming, restored, cached, stopped, timed-out, stale, resyncing, loading and empty fixtures in component tests.
- Checked 1100 x 800 and 900 x 700 geometry: no document overflow; both compact overlays retain controls.
- Collapsed and reopened the project/chat navigation and workspace resources in the browser after the shell-layer change, verifying truthful control labels and restoration of the reviewed state.
- Built the standalone Tauri release executable through the official pipeline, opened the real Windows window without a dev server, observed the embedded UI/Acrylic chrome in the native capture and verified the custom Close control terminates the app.

## Verification

- Desktop typecheck: passed.
- Desktop tests: 16 passed, including all nine fixture states, per-session draft isolation, collection-scoped chat creation, project-menu focus restoration, popover focus, deterministic WCAG contrast and a semantic text-color invariant.
- Desktop production build: passed.
- Automated structural accessibility and deterministic WCAG token-contrast checks: passed.
- Monochrome CSS token scan: passed.
- Official Tauri release build without installer bundling: passed; standalone launch and custom Close behavior passed.

## Follow-up Polish

- P3: the provisional Dennett icon needs a separate owner branding decision before it becomes final identity.
- P3: tune Acrylic opacity after owner review on the target display; reduced transparency already has a safe opaque fallback.
- P3: resource-pane density can be retuned when real browser, PDF and Git artifacts replace presentation fixtures.

## Owner Gate

Do not merge WP-M01-003 until the owner accepts this updated visual checkpoint. Approval covers the visual direction and presentation interactions only; it does not approve real provider, browser, file, PDF, project-folder or Git effects.

final result: passed
