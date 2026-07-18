# WP-M01-003 Design QA

## Evidence

- Visual sources: the owner's three private attachments from the second checkpoint. They are intentionally not tracked because they contain private workspace content.
- Owner contract: `docs/design/WP-M01-003/owner-direction-v2.md`.
- Implementation screenshot: `docs/design/WP-M01-003/project-chat-monochrome-v2-1536x971.png`.
- Browser URL during review: `http://127.0.0.1:5173/`.
- Browser viewport and state: 1536 x 971, dark theme, `streaming` fixture, project/chat navigation open, workspace resources open, full plan collapsed.
- Responsive checks: 1100 x 800 and 900 x 800.
- Native check: release Tauri shell built and opened on Windows at its configured 1280 x 800 size with transparent custom chrome and Acrylic window effects.
- Implementation commit: `5665905` on `codex/wp-m01-003-project-chat-screen`.

The three private references and the implementation capture were inspected together in one comparison input. The full-window source establishes the quiet Codex-like density and unified chrome; the project-list crop establishes projects with nested chats followed by standalone recent chats; the results-pane crop establishes the resource grouping and compact rounded surface.

No additional focused crops were needed: two owner attachments are already dedicated, legible crops of the project/chat navigation and resource workspace, while the full implementation capture keeps typography, composer controls and all persistent boundaries readable. Browser DOM inspection supplied exact labels and interaction states that are too small to read reliably in the scaled full-window preview.

## Findings

No actionable P0, P1 or P2 findings remain.

- Typography: Segoe UI/system fallbacks, restrained 10-13 px metadata and 13 px conversation copy preserve the compact desktop hierarchy without clipped controls or broken wrapping at the reviewed sizes.
- Layout and spacing: the title row, 52 px rail and 264 px project/chat sidebar read as one surface. The partial-height divider, centered conversation, anchored composer and 316 px resource workspace remain aligned. At 1100 px the workspace becomes a truthful closable overlay; at 900 px both side panes remain operable overlays with no document-level horizontal or vertical overflow.
- Color and surfaces: CSS palette checks found only equal-channel hexadecimal and RGBA colors. There are no purple, blue or other chromatic state accents. User messages use gray bubbles; agent messages remain unboxed. Rounded borders and surface count were reduced to persistent controls and resource grouping.
- Native material: the web preview stays opaque enough for repeatable screenshots. The native shell uses a transparent Tauri window, Windows Acrylic and more translucent native-only CSS layers; reduced-transparency mode retains an opaque neutral fallback.
- Icons and imagery: visible interface icons use Phosphor Icons. The provisional application icon is a generated monochrome raster source converted by the Tauri icon pipeline into platform assets; it is not treated as approved branding.
- Copy and truthfulness: the UI names Codex SDK as the only available source, labels unavailable features as later work, keeps external effects read-only and does not imply that Git, plugins, voice or provider routing are implemented.
- Accessibility: semantic landmarks and names are present; Command Center traps and restores focus; composer popovers close on Escape, outside interaction or prompt focus; disabled controls are truthful; reduced-motion, reduced-transparency and higher-contrast fallbacks are present. The default state passes the automated accessibility check.

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

## Primary Interactions Tested

- Opened Command Center, verified initial focus, focus wrapping, Escape closure and focus restoration.
- Switched `Full access` to `Auto-approve`.
- Opened the Codex runtime control, switched reasoning from High to Medium and returned focus to the prompt.
- Expanded and collapsed the current plan.
- Collapsed and reopened workspace resources.
- Opened the Browser resource in the central viewer and returned to chat.
- Verified deterministic streaming, restored, cached, stopped, timed-out, stale, resyncing, loading and empty fixtures in component tests.
- Checked 1100 x 800 and 900 x 800 geometry: no document overflow; both compact overlays retain controls.
- Opened a fresh in-app browser tab and found no warning or error console entries.
- Built the Tauri release executable and opened the real Windows window; custom minimize, maximize/restore and close controls were exposed by the native accessibility tree.

## Verification

- Desktop typecheck: passed.
- Desktop tests: 11 passed.
- Desktop production build: passed.
- Automated accessibility check: passed.
- Monochrome CSS token scan: passed.
- Tauri release build without installer bundling: passed.

## Follow-up Polish

- P3: the provisional Dennett icon needs a separate owner branding decision before it becomes final identity.
- P3: tune Acrylic opacity after owner review on the target display; reduced transparency already has a safe opaque fallback.
- P3: resource-pane density can be retuned when real browser, PDF and Git artifacts replace presentation fixtures.

## Owner Gate

Do not merge WP-M01-003 until the owner accepts this second visual checkpoint. Approval covers the visual direction and presentation interactions only; it does not approve real provider, browser, file, PDF or Git effects.

final result: passed
