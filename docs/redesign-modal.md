# Native modal chrome integration

`src/modal_chrome.rs` provides `ModalChrome` over egui 0.36.2's `Modal`.
Creation, placement, batch resizing, stock, hardware, relationship, material,
project-file, overwrite, recovery and grid/kerf dialogs use the shared
presentation. Keep one controller and one draft per dialog identity, then
render a pending two-action dialog with:

```rust
let result = chrome.show(
    ctx,
    &localizer.text("board-new"),
    ModalActions {
        cancel: &localizer.text("cancel"),
        confirm: &localizer.text("board-create"),
    },
    |ui| {
        ui.add(egui::TextEdit::singleline(&mut draft.name).id(name_field_id));
        ((), draft.is_valid())
    },
);
match result.action {
    ModalAction::Cancel => { chrome.close(ctx); /* discard this draft */ }
    ModalAction::Confirm => {
        if commit(&draft).is_ok() { chrome.close(ctx); }
        // On failure, retain the draft and render its error next frame.
    }
    ModalAction::None => {}
}
```

Construct with `.first_focus(name_field_id)` when the first applicable field has an explicit widget ID, and optionally `.width(points)` and `.icon(Icon::Board)` for the dialog. A dynamically identified first chooser can request focus after the opening render. Without a field ID, focus initially goes to Cancel. Enter must activate that visibly focused Cancel, not an invisible affirmative default. Never reinterpret `ModalAction::Cancel` based on raw Enter. `ModalResult::body` returns the body's result, while its validity boolean controls the disabled confirmation and unconsumed Enter. Pass the actual localized action verb for confirmation, especially destructive actions. The title and footer stay outside the independently scrolling body; footer actions wrap in a narrow window.

Pending-navigation and unsaved-work prompts need **three** distinct decisions;
never disguise Discard as body text. Use `show_three(ctx, title,
ModalThreeActions { primary: apply_or_save, secondary: discard_or_cancel,
stay }, body)`. Its `ModalThreeAction::{Primary, Secondary, Stay, None}` is
resolved by the owning workflow. An invalid edit disables Primary; Enter
activates a valid primary only when no field or popup consumes it, and Escape
chooses Stay. Close the chrome only after successful decision resolution.
Settings deliberately keeps its larger section-list `egui::Modal` and **Done**
footer while applying the same focus, popup, scroll and background boundary.

For a nested material dialog, retain the board controller **and board draft** while rendering only the child. The child captures the focused board field; `child.close(ctx)` restores it, and the parent controller re-establishes focus on its next render even if egui dropped the suspended widget's focus. Use distinct modal IDs and stable field IDs. Calling `close` only after the action completes also preserves drafts when validation/commit fails.

The underlying `egui::Modal` handles foreground focus traversal, backdrop pointer capture and widget interaction isolation. The controller checks popup state both before and after drawing the body, so Enter/Escape from a popup cannot submit or cancel the parent. Escape cancellation uses `ModalResponse::should_close` for topmost-modal handling. If a field/edit session handles Enter or Escape first, consume that key in the body before the chrome checks it; the parent modal remains open. Keep application-level raw-input listeners (`ctx.input` scene drag/camera/scroll, Delete, undo/redo and project shortcuts) behind the existing dialog-active guard, including the frame a dialog first opens: egui modal layers do not erase raw input events. `chrome.is_active()` exposes the rendered controller state; a pending dialog in app state is the earlier guard if the scene is drawn before the modal in that frame. Draw any modal above underlying scene controls.

The headless tests in `modal_chrome.rs` exercise forward/reverse focus traversal,
popup Enter/Escape, invalid confirmation, child focus return, bounded long-body
layout, three-decision footer semantics and a background button/scroll area
plus modal layer. Focused owner tests additionally cover nested material
return, project-file guards, and the navigation prompt. The expanded owner
inventory traverses 40 forward and 40 reverse Tab steps in stock, currency,
fee, grid/kerf, hardware, hinge, relationship and Transform forms as well as the
original board/material/placement forms. Real hinge and relationship chooser
tests open the popup and select its already-selected option with Enter twice,
checking that the popup closes without submitting or editing the project.
Recovery cleanup, overwrite and recovery prompts test focused Cancel/Defer
against actual temporary files; separate regressions cover reader Back/return
focus and camera-wheel blocking on a project prompt's first mounted frame.

The six native Metal captures were generated independently using
`--capture-dialog board|position|face|resize|material|unsaved` under
`…/opencode/modal-reference-0927-*/`. They mount real drafts over the isolated
reference project and record the selector in the manifest; other surface
selectors are rejected. These captures revealed remaining reference differences,
not a visual pass. Subsequent header/padding/width/footer styling needs recapture.

## Remaining runtime inventory for 7.4

| Owner | Boundary decision / outstanding verification |
| --- | --- |
| `assembly_ui.rs` Transform dialog | Now uses its owned `ModalChrome`, stable pivot-X initial focus and the existing atomic transaction path. Included in owner focus traversal; retain native compact/cancel/undo checks. |
| `main.rs` Settings Worked Examples | Uses shared `show_reader` with a single Back footer, bounded body and initial/return focus; no affirmative edit action. |
| `recovery_cleanup_ui.rs` review/confirmation | Both use shared chrome without raw Enter overrides. The parent remains suspended while its child is open. Explicit Cancel never starts or executes deletion. |
| `project_ui.rs` overwrite/recovery | Removed hidden default-Enter overrides; focused Cancel/Defer is honored and confirmation requires moving to the affirmative control. |
| Kerf | Value editing uses chrome. Shop confirmation now opens a revision-bound shared modal from Settings, Handoff or the palette, requiring explicit checkbox acknowledgement before Confirm. Opening, Cancel and stale drafts do not change provenance; native visual review remains pending. |
| `command_palette.rs` search overlay | Intentional palette exception rather than an editing dialog. It already owns query focus, invoking-focus return, result navigation and modal guards; retain its distinct search composition and verify background isolation. |
| `settings_ui.rs` main Settings | Intentional larger section-list/Done exception required by the handoff; it must still pass the same input/focus boundary tests. |

The compact Controls/Inspector `egui::Window` drawers are nonmodal workspace
panes, not substitutes for a dialog. They should remain blocked beneath an
active modal and preserve their own drafts and scroll state. Reference-modal
acceptance remains open: compare all six separately after styling corrections,
including compact pt-BR native interaction. The handoff montage is not one
application screen and static captures do not prove interaction isolation.
