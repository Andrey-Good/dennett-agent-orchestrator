# WP-M01-003 owner direction v4

This note records the owner's fourth annotated correction pass for the Project Chat checkpoint. It extends `owner-direction-v3.md` and owns visual and presentation decisions for this work package only. Specifications 60 and 83 still own durable product semantics.

## Central workspace boundary

- Do not draw a separate vertical divider at the right edge of the project sidebar.
- The only boundary between the left shell and the work area belongs to the central workspace itself and follows its rounded top-left corner.
- The central workspace surface extends continuously to the bottom and right edges of the window.
- The right Workspace panel is an inset glass surface inside that central workspace, not a peer surface with the base shell visible behind it.
- The central conversation owns the full-width scroll container, so its scrollbar sits at the physical right edge of the window. Conversation content and the composer reserve room for the overlaid Workspace panel, while the panel itself remains slightly left of that scrollbar.

## Material and contrast

- Dividers should read as soft white glass highlights rather than dark gray rules.
- Left-shell labels and icons may use a restrained light bloom, but text must remain sharp and readable; do not apply destructive blur to glyphs.
- In the native shell, the base material must always derive from the Windows desktop wallpaper, not whichever application happens to sit behind Dennett. The shell therefore loads the current Windows wallpaper into an internal, non-transparent material layer and never uses the live area behind the window as its visual input. Browser preview remains an engineering fallback and is not the owner-review surface for material fidelity.
- The wallpaper-backed material is rendered achromatically so colored wallpaper cannot reintroduce purple, blue or other interface tint.
- Preserve the approved depth hierarchy above that base: the top and left shell are lighter and less obscuring, while the central workspace is darker and more strongly diffused.
- Project the wallpaper at the monitor's real scale and offset it by the native window position. Do not recrop the wallpaper independently for the Dennett window; the visible material must correspond to the same desktop region beneath it.
- The composer and Workspace panel use the same light gray material family as sent user messages. They may remain translucent, but must not read as darker than the user-message surface.

## Conversation edge and selection tuning

- Keep the scrolled-content fade at its current compact height, but make the blur materially stronger so text dissolves before it crosses the workspace edge.
- The selected chat uses a light transparent white-glass highlight. It must not become an opaque gray or a dark copy of the central workspace.
- These material details are judged in the native Tauri window, where the internal wallpaper-backed material is present.

## Owner gate

Show the next visual checkpoint in the native Tauri window with the internal wallpaper material enabled. Browser screenshots may supplement interaction and responsive checks, but they do not substitute for native material review.
