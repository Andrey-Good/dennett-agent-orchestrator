# WP-M01-003 owner direction v3

This note records the owner's annotated corrections to the third Project Chat checkpoint. It extends `owner-direction-v2.md` and owns visual and presentation decisions for this work package only. Specifications 60 and 83 still own durable product semantics.

## Unified left shell

- Account identity and future microphone access belong inside the combined left shell, separated from navigation by the same restrained divider language as the rail/sidebar split.
- The account area must not read as a floating card: no detached rectangle, heavy shadow or separate elevation.
- The update control is absent when no update exists. It appears only for a real available update.
- Remove the standalone New Chat action from the narrow activity rail.
- Rename the rail destination from `Projects` to `Chats` and use a chat icon.
- Remove the visible seam between the title row and the left shell. The transition into the central workspace has a rounded inner corner rather than a square intersection.

## Project and chat creation

- The action beside `Projects` creates or adds a project; it does not create a chat.
- Reveal the project action only on hover or keyboard focus of the `Projects` row.
- Its menu has two choices: `Create empty project` and `Add existing folder`.
- Real folder creation and the OS folder picker remain M02 effects. The M01 checkpoint implements the full presentation flow with truthful local preview state and must not claim that the filesystem changed.
- Reveal a New Chat action beside each project on hover or keyboard focus; it creates a chat scoped to that project.
- Reveal a New Chat action beside `Recent` on hover or keyboard focus; it creates a standalone chat.
- Reduce the selected-chat row height and padding so the active state is compact.

## Composer

- Increase the runtime/access control type size for comfortable reading.
- Keep access choices left-aligned and reduce the access/runtime popup width.
- Empty-chat suggestions should be useful starting prompts: ask the agent to question the user's project idea, or ask it to inspect and explain the repository.

## Right workspace and material

- Remove the duplicate top-row button for opening or closing workspace resources.
- The workspace header owns collapse. Replace its close `X` with a right-panel/window state icon; the collapsed dock shows the corresponding closed state and reopens the workspace.
- Fixture rows in the right workspace remain acceptable as explicit demonstration data, not hard-coded claims of implemented integrations.
- The top row and right workspace use less darkening and less blur than the central canvas. The left shell and title row still read as one continuous translucent material.

## Owner gate

Show the revised hover, menu, compact-row, open/closed workspace and native Acrylic states before merge. Acceptance remains limited to visual direction and presentation interactions; it does not approve real filesystem, Git, browser, PDF, provider or update effects.
