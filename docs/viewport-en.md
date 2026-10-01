# 3D view navigation

Settings → General controls navigation hints, inverse scroll zoom, material
tint and the interface scale (90/100/115/130%). Settings → Grid & units changes
the display unit without moving boards or reinterpreting an already dirty,
unsuffixed field: it retains the unit and locale from its first edit. Explicit
unit suffixes override that captured unit. A pristine field reformats from the
exact saved quantity; a rounded draft still needs its original consent.

Open **Design** on the rail. The camera/tool row is above its central canvas;
at compact widths **View options** contains **Frame selection**, projection
and camera presets; at very narrow widths it also contains the tools. The
snap menu controls face and grid assistance independently, reports the saved
spacing and explains when distant zoom levels display a coarser grid. The
on-canvas help line is shortened at narrow widths; hover it for the full
keyboard/trackpad instructions. Camera settings are session view state and do
not edit the project. When the side panels collapse, their header drawers
remain reachable and do not discard an inspector or HUD dimension draft.

The view shows project boards at their assembly positions. X (length) is red,
Y (width) green, and Z (thickness) blue. Click a visible board face or its
name in the left object list to select it; hold Shift or Command (macOS) / Ctrl
(Linux) while clicking to add or remove boards. Click empty scene space to
clear selection. The active board is amber and its Outliner row is highlighted;
other selected boards are cyan. Hidden faces behind closer boards are not
selectable by clicking through the closer board; use the list instead.
**Frame selection** fits the visible selected geometry, or the visible scene
when nothing is selected. A selection with no visible frameable geometry
disables this action; clear the selection to frame the scene. With no selection
and an empty scene, it returns to the origin. Choose **Iso**, **Front**, **Right**,
or **Top** and **Perspective** or **Orthographic** from the controls above the
view or **View options**. Camera navigation does not change the project.

- Drag with the primary button (one-finger click and drag on a trackpad) to orbit.
- Drag with the secondary button (two-finger click and drag on a trackpad), or
  hold Shift while dragging with the primary button, to pan.
- Scroll vertically or pinch on the view to zoom. Zoom works in both projections.
- Click the view to focus it; arrow keys orbit, Shift+arrow keys pan, `+`/`-`
  zoom, and unmodified `F` runs **Frame selection** when available. Tab can also
  focus the view. Scene shortcuts do not run when a dialog
  or menu is open or focus is in another control or text field.

The default **Navigate** mode keeps primary dragging for orbit. Choose **Move
board** above the view to drag the active selected board (start on its visible
surface). Its pose previews on a camera-facing plane without changing the
project. A nearby visible face may snap: the highlighted cyan source and pink
target faces and status label show the proposed placement. Hold **Alt** while
dragging to bypass snapping and place freely. Release to accept one undoable
move; press **Escape** during the drag to cancel and restore the original pose
and selection. Secondary dragging still pans, and switching back to Navigate
restores primary orbit. Dragging empty space in Move mode does not move a board.
These snaps are temporary placement aids: the accepted pose is independent of
the target. Moving the target later does not carry the board along unless both
belong to a moved assembly. No snap creates a hinge or other mechanical joint.

Hardware is part of the scene too. Click a foot or other hardware to select it
like a board, and drag it with **Move board** (one undoable move, grid
snapping only). Click a drawer slide or a hinge to open it in the inspector
without leaving the workspace. Hinges are drawn as a cup in the door and a
plate on the cabinet side, amber when they have a problem. They show where the
hinge is, not where to drill. In **Hardware**, Move drags only hardware, never
boards. Move is paused while the inspector has unsaved position values.

From a board's row, **Numeric pose** opens a preview with an explicit **Local
parent** or **World** coordinate frame. Position is in mm (unit suffixes are
accepted); rotation is in degrees about X, Y, then Z. Untouched derived
sub-micrometre coordinates stay exact instead of committing rounded display
text. **Place face to face** chooses source and target faces, in-plane
start/centre/end alignment, offsets and an outward gap. Cyan and pink
highlight the proposed faces. **Apply** (numeric pose) or **Place** (face to
face) commits one pose without creating a permanent snap constraint;
**Cancel** or Escape restores the starting pose.
Use World for absolute positions and Local parent for coordinates within an
assembly; rotations are XYZ degrees. Alignment selects start/centre/end on
each in-plane axis, offsets run along the target face's in-plane axes, and the
outward gap runs along its normal. The centered modal shows the tentative
result and does not commit the preview until its named footer action is used.
For exact board-by-board steps and a
body-only versus overall measurement example, see [Assemblies and hierarchy](assembly-en.md).
## Measure and projected board dimensions

Select a single visible board to see its local **Length** and **Width** as labels projected onto its actual X and Y edges. The labels follow camera projection and the board's pose; they describe the physical blank along board-local axes, not the screen-space pixel distance or the assembly's bounding box. Hidden boards and multi-selection do not show a misleading single-board pair. The inspector and selection HUD offer the same pending Length/Width draft; the labels themselves are read-only.

Choose **Measure** above the view to show read-only X × Y × Z bounding dimensions for selected boards or assemblies (and supported hardware in Overall). Choose **Body only · wooden boards** or **Overall · boards + selected hardware** and a **Frame** of World or a board/assembly in the measurement controls. The overlay identifies scope, frame and display units; choices persist when changing tools. Hidden selected descendants still count; hardware lacking dimensions makes Overall unavailable rather than implying a complete result. Measure does not alter board poses, manufacturing dimensions, allocations or undo history. The inspector also exposes the measurement. For the frame/preset convention and a worked example, see [Assemblies and hierarchy](assembly-en.md).

## Project XY grid

The viewport grid lies in world XY at Z = 0, anchored at the world origin. Its project-local spacing starts at 10 mm. Use **Edit grid spacing** in Settings → Grid & units or the Design controls to enter a positive value up to 1,000,000 mm; choose mm or in for unsuffixed input, or type mm, cm, m, in, or ft explicitly (including fractional inches). Values beyond 0.001 mm precision require confirmation of the rounded result. Cancel, invalid input, and merely focusing the field leave the project unchanged. The visible grid thins at distant zoom levels for readability, while snapping always uses the exact saved spacing.

In **Move board**, drag a selected board near an XY grid intersection to preview its world-origin X/Y on that intersection; its Z and rotation remain intact, even under a rotated assembly. A visible face snap takes priority over a grid snap. The drag status names the face target or **XY grid**; hold Alt to bypass both. A grid snap engages only after the drag has moved and the candidate is close on screen, so starting a drag does not jump an existing pose. Release to accept one undoable pose edit, or press Esc to cancel. Snap placement creates no permanent constraint; changing spacing never repositions existing boards. Spacing is saved with the project and restored when reopened.

## Lighting

Faces are shaded by the way they face, so a part looks the same however it
was modelled. The light button at the bottom of the tool strip (a sun) and
**Settings → General → Lighting** choose how the view is lit:

- **Follow camera** (default): the light sits just above and beside your
  eye, so whatever you look at is lit.
- **Fixed**: the light stays put while you orbit. Set its direction with the
  *Around* and *Height* sliders, or orbit to where you want the light and
  choose **Fix the light here**.
- **Off**: even, soft shading without a directional light.

The choice is kept on this computer, not in the project. Pictures (PDF,
thumbnails, agent pictures) always use the same fixed studio light.
