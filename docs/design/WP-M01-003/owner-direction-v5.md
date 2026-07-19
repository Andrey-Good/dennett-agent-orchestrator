# WP-M01-003 owner direction v5

This note records the owner's current native-material and contrast direction for the Project Chat checkpoint. It supersedes the material implementation in `owner-direction-v4.md`; the earlier notes remain historical evidence. Specifications 60 and 83 continue to own durable product behavior.

## Native material

- Use the Windows 11 Mica material supplied by Tauri. The native window is transparent and declares `windowEffects.effects = ["mica"]`; React does not load, crop or project the wallpaper itself.
- Mica is the single native foundation for the titlebar, activity rail and project/chat sidebar. Those regions visually merge into one continuous lower layer.
- Do not use Acrylic or a live CSS backdrop as a substitute. Other windows behind Dennett must not become the material source.
- Do not set a second explicit transparent WebView background color when Tauri's transparent window already owns that boundary. The native CSS roots remain transparent.
- Mica is allowed to tint and strongly diffuse the wallpaper as Windows intends. Exact wallpaper pixels are not a requirement for this checkpoint.

## Surface hierarchy

- The central workspace starts at the rounded top-left boundary and extends continuously to the physical bottom and right edges of the window.
- The central workspace uses the same Mica foundation with a neutral light-gray density layer above it. The layer should make the center less transparent without making it look darker than the prior approved center.
- The Workspace/resource panel is inset above the central workspace near the right edge. It is not a peer background column.
- The conversation scrollbar remains at the physical right edge. The inset resource panel stays slightly left of it.
- The composer and Workspace panel use the lighter gray material family of sent user messages and remain visibly distinct from the central surface.

## Contrast and selection

- Project names, chat names, account text, important resource names and active controls are translucent white rather than gray.
- Section headings, metadata, timestamps and secondary descriptions remain gray so hierarchy is still clear.
- White symbols are normal translucent glyphs with restrained bloom. Mica is a window material and must not be simulated inside icon strokes or text.
- The selected chat uses a light, transparent white highlight. It must be lighter than the surrounding left shell, not an opaque gray pill.

## Conversation top edge

- Scrolled text must dissolve before crossing the top edge of the central workspace.
- The transition spans beneath the inset Workspace panel to the right side of the window and stops before the physical scrollbar.
- A large WebView `backdrop-filter` surface is forbidden: the previous implementation produced a displaced empty-panel artifact and could hide the native material.
- Until a native-safe localized blur is proven, use a compact opacity/material fade. Reintroduce stronger optical blur only after a native checkpoint demonstrates that it creates no separate compositing rectangle.

## Owner gate

Review this direction in the release Tauri window, not only in the browser renderer. The checkpoint is not accepted until the owner confirms visible Mica, the absence of the displaced empty layer, the lighter text hierarchy and the full-width top transition.
