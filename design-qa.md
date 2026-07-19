# WP-M01-003 Design QA

## Current decision source

- Current owner contract: `docs/design/WP-M01-003/owner-direction-v5.md`.
- Earlier owner contracts remain historical context: `owner-direction-v2.md`, `owner-direction-v3.md` and `owner-direction-v4.md`.
- Product semantics remain owned by specification 60; the visual notes do not authorize M02 Git, file or browser effects.

## Current implementation evidence

- `apps/desktop/src-tauri/tauri.conf.json` uses the native Tauri Windows boundary: `transparent: true` and `windowEffects.effects: ["mica"]`. It does not declare Acrylic or an explicit transparent `backgroundColor`.
- The native bridge checks the real Windows build before claiming Mica support. Unsupported systems and reduced-transparency preferences receive an opaque neutral fallback instead of an unreadable transparent WebView.
- `apps/desktop/src/styles.css` leaves the native roots and unified top/left shell transparent, applies a `36%` opaque / `64%` transparent `#181818` density layer to the full central workspace, keeps the resource panel inset above that workspace and reserves the physical right edge for the conversation scrollbar. Static inspection confirms no second full-size opaque central layer.
- The composer, sent user messages and Workspace panel use a `24%` local-opacity `#2d2d2d` tint above the central layer. Sequential compositing leaves about `49%` of native Mica visible, versus `64%` in the center, so these surfaces remain denser without reading as solid slabs. The redundant composer/resource WebView backdrop blur is disabled in the native shell because it flattened the already-diffused Mica.
- Conversation and composer text are one pixel larger; left navigation and Workspace/resource text are two pixels larger while preserving the primary/secondary contrast hierarchy.
- Important project, chat, account and resource labels use translucent white semantic tokens; section labels and metadata retain gray hierarchy.
- The selected chat uses a translucent white fill. It no longer copies the dark central material.
- The previous large top-edge `backdrop-filter` was removed. The compact transition extends beneath the resource panel to the right edge while stopping before the scrollbar, without creating a separate GPU blur rectangle.
- `apps/desktop/src/App.test.tsx` guards the native Mica configuration and the absence of the retired internal wallpaper layer.
- Fixture state, Stop state, sent local messages and unsent drafts are scoped by session; keyboard switching A→B→A is covered so one chat cannot change another.
- Context and plugin popovers expose their dialog relationship, move focus on open, restore focus on Escape and pass automated accessibility checks in their open states.

## Visual evidence and history

- `project-chat-owner-v3-2048x1280.png` and `project-chat-owner-v3-empty-2048x1280.png` remain the accepted structure and interaction-density references.
- `project-chat-native-v6-owner-tuning-1280x800.png` records the owner's preferred earlier glass character, but Acrylic is now rejected because it can include windows behind Dennett.
- `project-chat-native-v7-mica-wallpaper-1280x800.jpg` through `project-chat-native-v10-projection-retry-1280x800.jpg` record the rejected attempts to imitate wallpaper material inside React.
- `project-chat-native-v11-mica-1280x800.jpg` records the first Tauri Mica checkpoint. A later live run exposed two unresolved defects in that revision: the Mica was obscured by dense overlays and a large top-edge `backdrop-filter` could appear as a displaced empty rectangle.
- The corrected release executable was reviewed live in the native Tauri window on 2026-07-19. The owner confirmed that the material hierarchy was substantially improved and explicitly accepted the screen; no persisted screenshot is claimed for that final live review.

## Automated verification

- Desktop component suite: 20 tests passed.
- Coverage includes all material fixture states, per-session draft isolation, project-scoped creation, focus restoration, Command Center keyboard behavior, access/runtime controls, resource opening, structural accessibility and native Mica configuration.
- Desktop typecheck: passed.
- Desktop production build: passed.
- Rust workspace tests: passed, including runtime conformance, persistence/restart, journal integrity, watch-gap/resync, draft recovery and Head eligibility coverage.
- Official Tauri release build without installer bundling: passed. The executable is `apps/desktop/src-tauri/target/release/dennett-desktop-shell.exe`.
- Native shell tests cover the Windows 11 Mica build boundary plus pending, unavailable, rejected and available capability-probe states; the opaque fallback is present before the asynchronous probe begins.
- `git diff --check`: passed before the acceptance-record update and is rerun as part of package closure.

## Current findings

- No automated functional or structural accessibility regression is known.
- The custom wallpaper projection path has been removed, so monitor cropping, startup projection races and accidental live-window sampling are no longer application responsibilities.
- The owner inspected the unlocked native window and did not report recurrence of the displaced empty-surface artifact.
- The owner accepted the visible Mica hierarchy, `#181818` center, lighter selected-chat treatment, translucent `#2d2d2d` raised surfaces and the full-width top transition on 2026-07-19.
- Additional visual polish remains desirable but is explicitly deferred and does not block this bounded screen checkpoint.

## Completed native comparison

The release Tauri window was reviewed live against the established structure and native-material evidence. The accepted checks were:

1. the top/left shell visibly receives native Mica;
2. the central workspace reads as `#181818` while retaining a light Mica impression and never becoming a shifted rectangle;
3. the Workspace panel is inset above the center and no peer gray column appears behind it;
4. the top transition continues beneath the Workspace panel and stops before the scrollbar;
5. important labels are white while section headings remain gray;
6. the selected chat is subtly lighter than its surrounding shell;
7. the composer and Workspace panel read as translucent `#2d2d2d` without white illumination and match the sent-user-message material family while remaining clearly separated from the center.

## Owner gate

Resolved on 2026-07-19: the owner accepted this presentation and its fixture interactions. The approval does not authorize M02 Git, file, terminal or browser effects. Future substantial visual revisions will be tuned in an editable Figma prototype before they are transferred to code.

final result: passed by owner live native review
