# WP-M01-003 Design QA

## Current decision source

- Current owner contract: `docs/design/WP-M01-003/owner-direction-v5.md`.
- Earlier owner contracts remain historical context: `owner-direction-v2.md`, `owner-direction-v3.md` and `owner-direction-v4.md`.
- Product semantics remain owned by specification 60; the visual notes do not authorize M02 Git, file or browser effects.

## Current implementation evidence

- `apps/desktop/src-tauri/tauri.conf.json` uses the native Tauri Windows boundary: `transparent: true` and `windowEffects.effects: ["mica"]`. It does not declare Acrylic or an explicit transparent `backgroundColor`.
- `apps/desktop/src/styles.css` leaves the native roots and unified top/left shell transparent, applies one neutral gray density layer to the full central workspace, keeps the resource panel inset above that workspace and reserves the physical right edge for the conversation scrollbar.
- Important project, chat, account and resource labels use translucent white semantic tokens; section labels and metadata retain gray hierarchy.
- The selected chat uses a translucent white fill. It no longer copies the dark central material.
- The previous large top-edge `backdrop-filter` was removed. The compact transition extends beneath the resource panel to the right edge while stopping before the scrollbar, without creating a separate GPU blur rectangle.
- `apps/desktop/src/App.test.tsx` guards the native Mica configuration and the absence of the retired internal wallpaper layer.

## Visual evidence and history

- `project-chat-owner-v3-2048x1280.png` and `project-chat-owner-v3-empty-2048x1280.png` remain the accepted structure and interaction-density references.
- `project-chat-native-v6-owner-tuning-1280x800.png` records the owner's preferred earlier glass character, but Acrylic is now rejected because it can include windows behind Dennett.
- `project-chat-native-v7-mica-wallpaper-1280x800.jpg` through `project-chat-native-v10-projection-retry-1280x800.jpg` record the rejected attempts to imitate wallpaper material inside React.
- `project-chat-native-v11-mica-1280x800.jpg` records the first Tauri Mica checkpoint. A later live run exposed two unresolved defects in that revision: the Mica was obscured by dense overlays and a large top-edge `backdrop-filter` could appear as a displaced empty rectangle.
- The current correction has been built as a release executable, but its final native screenshot is intentionally not claimed yet. The first verification attempt reached the Windows lock screen; Computer Use was stopped immediately. A fresh 1280 x 800 native capture and same-input comparison are still required after unlock.

## Automated verification

- Desktop component suite: 17 tests passed.
- Coverage includes all material fixture states, per-session draft isolation, project-scoped creation, focus restoration, Command Center keyboard behavior, access/runtime controls, resource opening, structural accessibility and native Mica configuration.
- Desktop typecheck: passed.
- Desktop production build: passed.
- Rust workspace tests: passed, including runtime conformance, persistence/restart, journal integrity, watch-gap/resync, draft recovery and Head eligibility coverage.
- Official Tauri release build without installer bundling: passed. The executable is `apps/desktop/src-tauri/target/release/dennett-desktop-shell.exe`.
- `git diff --check`: passed before the documentation update.

## Current findings

- No automated functional or structural accessibility regression is known.
- The custom wallpaper projection path has been removed, so monitor cropping, startup projection races and accidental live-window sampling are no longer application responsibilities.
- The displaced empty-surface mechanism has been removed from CSS, but absence of the visible artifact is not proven until the unlocked native window is inspected.
- Mica visibility, central density, selected-chat brightness, composer/resource lightness and the full-width top transition remain owner-review items.

## Required native comparison

At 1280 x 800, capture the release Tauri window in the same fixture state as `project-chat-native-v11-mica-1280x800.jpg`. Compare both images together and check:

1. the top/left shell visibly receives native Mica;
2. the central workspace is denser and gray without becoming an opaque shifted rectangle;
3. the Workspace panel is inset above the center and no peer gray column appears behind it;
4. the top transition continues beneath the Workspace panel and stops before the scrollbar;
5. important labels are white while section headings remain gray;
6. the selected chat is subtly lighter than its surrounding shell;
7. the composer and Workspace panel match the lighter user-message material family.

## Owner gate

Do not merge WP-M01-003 until the unlocked native comparison is complete and the owner accepts the checkpoint. Approval covers this presentation and its fixture interactions only.

final result: blocked on native visual review
