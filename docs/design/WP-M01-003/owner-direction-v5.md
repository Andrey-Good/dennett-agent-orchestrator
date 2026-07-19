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
- The central workspace uses the same Mica foundation with a translucent `#181818` density layer above it. Static layer inspection found no additional opaque central surface. The current review opacity is `36%` (64% transparency, twice the previous 32%) so the wallpaper-derived Mica remains visibly present.
- The Workspace/resource panel is inset above the central workspace near the right edge. It is not a peer background column.
- The conversation scrollbar remains at the physical right edge. The inset resource panel stays slightly left of it.
- The composer, Workspace panel and sent user messages use a borderless translucent `#2d2d2d` raised-surface tint at `24%` local opacity, without a white veil, glow or edge outline. Because it sits above the `36%` opaque central tint, the combined surface retains about `49%` of the native Mica contribution: denser than the `64%` center while still visibly translucent. Composer and Workspace do not apply a second CSS backdrop blur; native Mica already owns diffusion, and the redundant WebView blur made the material read as an opaque slab.

## Contrast and selection

- Project names, chat names, account text, important resource names and active controls are translucent white rather than gray.
- Section headings, metadata, timestamps and secondary descriptions remain gray so hierarchy is still clear.
- White symbols are normal translucent glyphs with restrained bloom. Mica is a window material and must not be simulated inside icon strokes or text.
- The selected chat uses a light, transparent white highlight. It must be lighter than the surrounding left shell, not an opaque gray pill.
- Conversation text and composer input are one pixel larger than the prior checkpoint. Project/chat navigation and Workspace/resource typography are two pixels larger while preserving white primary labels and gray secondary hierarchy.

## Conversation top edge

- Scrolled text must dissolve before crossing the top edge of the central workspace.
- The transition spans beneath the inset Workspace panel to the right side of the window and stops before the physical scrollbar.
- A large WebView `backdrop-filter` surface is forbidden: the previous implementation produced a displaced empty-panel artifact and could hide the native material.
- Until a native-safe localized blur is proven, use a compact opacity/material fade. Reintroduce stronger optical blur only after a native checkpoint demonstrates that it creates no separate compositing rectangle.

## Owner gate

The owner reviewed this direction in the release Tauri window on 2026-07-19 and accepted the checkpoint. The remaining desire for general polish is deferred and does not reopen this bounded screen approval.

## Future visual workflow

- Before the next substantial screen, material or layout implementation, prepare an editable Figma prototype rather than iterating primarily through code builds.
- The owner tunes colors, material opacity, blur impression and UX in Figma and explicitly approves the selected frame.
- Implement the approved frame in the native application as closely as the platform permits, then use the release application only for native-material and behavior verification that Figma cannot prove.
- Technical composition, accessibility, performance and platform constraints remain engineering responsibilities; the Figma approval owns the intended visual and UX result.
