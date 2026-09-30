# Design

## Selection and inspection

`InspectorTarget` now covers every kind of fitting: `Hardware`, `Slide`, `Installation`, `Door` and `Catalog`. `request_inspect(target)` is the single entry point, shared by panel rows, links, viewport picks and actions. It routes through the pending-navigation guard, so an unsaved draft prompts Apply / Discard / Stay. Inspecting keeps the current workspace. Inspecting `Hardware` also selects the object in the scene.

## Drafts

`FittingDraft` holds the text fields of one fitting, by target:

| Target | Fields |
|---|---|
| Hardware | World X/Y/Z, rotation, and placeholder dimensions |
| Slide | Height and setback |
| Hinge | Door Y and mount Y |

The draft previews on a scratch editor. It commits in one transaction through `edit_placeholder`, `move_hardware`, `slide_installation::update` or `hinge_installation::set_positions`. Combos and buttons commit immediately.

## Positions

Hardware positions are shown and edited in world coordinates:

- `move_hardware` converts world coordinates back to the parent's frame.
- A new foot sits under the parent's body bounds, 20 mm in from the minimum corner, with its top on the bottom face.
- When that leaves it below z = 0, the inspector offers to raise the top assembly by the overhang.

## Adding

Library commands keep each add to one undo step:

- `door_joint::create_with_hinges`, `add_hinge` and `reconfigure`
- `hinge_installation::standard_set`
- `slide_installation::propose` and `update_with_catalog`
- `hardware_catalog::remove`

## Hinge solids

- A hinge is drawn only when its references resolve, so a hinge with issues and no references stays reachable from the panel only.
- The cup is a frustum with a 1.5 mm lip and is posed with the door, so it follows motion. The plate is a box around its holes and is posed with the mount.
- Colour is nickel, or amber when the hinge has issues.
- The scene description leaves hinges out on purpose, because a cup sits inside the door and would read as an overlap.

## Picking and Move

`hardware_mesh::pick_boxes` gives cheap boxes for slides and hinges. Picking one inspects it; other picks select as before. In Hardware, `MoveTool.hardware_only` limits dragging to feet and placeholders, and `drag_enabled` is false while a fitting draft is dirty.
