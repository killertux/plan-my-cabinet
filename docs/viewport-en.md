# 3D view navigation

The view shows project boards at their assembly positions. X (length) is red,
Y (width) green, and Z (thickness) blue. Click a visible board face or its
name in the left object list to select it; hold Shift or Command (macOS) / Ctrl
(Linux) while clicking to add or remove boards. Click empty scene space to
clear selection. The active board is amber and prefixed with `*` in the list;
other selected boards are cyan. Hidden faces behind closer boards are not
selectable by clicking through the closer board; use the list instead.
**Frame selection/scene** fits the selected board(s), or all
boards when nothing is selected. With an empty project, it returns to the
origin. Choose Isometric, Front, Right, or Top and Perspective or Orthographic
from the controls above the view. Camera navigation does not change the project.
The Orbit/Pan Left/Right buttons and Zoom +/- buttons offer pointer alternatives
to dragging and gestures.

- Drag with the primary button (one-finger click and drag on a trackpad) to orbit.
- Drag with the secondary button (two-finger click and drag on a trackpad), or
  hold Shift while dragging with the primary button, to pan.
- Scroll vertically or pinch on the view to zoom. Zoom works in both projections.
- Click the view to focus it; arrow keys orbit, Shift+arrow keys pan, and `+`/`-`
  zoom. Tab can also focus the view. Scene shortcuts do not run when a dialog
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

From a board's row, **Numeric pose** opens a preview with an explicit **Local
parent** or **World** coordinate frame. Position is in mm (unit suffixes are
accepted); rotation is in degrees about X, Y, then Z. Untouched derived
sub-micrometre coordinates stay exact instead of committing rounded display
text. **Place face to face** chooses source and target faces, in-plane
start/centre/end alignment, offsets and an outward gap. Cyan and pink
highlight the proposed faces. **Confirm** commits one pose without creating
a permanent snap constraint; **Cancel** or Escape restores the starting pose.
Use World for absolute positions and Local parent for coordinates within an
assembly; rotations are XYZ degrees. Alignment selects start/centre/end on
each in-plane axis, offsets run along the target face's in-plane axes, and the
outward gap runs along its normal. For exact board-by-board steps and a
body-only versus overall measurement example, see [Assemblies and hierarchy](assembly-en.md).
# Project XY grid

The viewport grid lies in world XY at Z = 0, anchored at the world origin. Its project-local spacing starts at 10 mm. Use **Edit grid spacing** in the sidebar to enter a positive value up to 1,000,000 mm; choose mm or in for unsuffixed input, or type mm, cm, m, in, or ft explicitly (including fractional inches). Values beyond 0.001 mm precision require confirmation of the rounded result. Cancel, invalid input, and merely focusing the field leave the project unchanged. The visible grid thins at distant zoom levels for readability, while snapping always uses the exact saved spacing.

In **Move board**, drag a selected board near an XY grid intersection to preview its world-origin X/Y on that intersection; its Z and rotation remain intact, even under a rotated assembly. A visible face snap takes priority over a grid snap. The drag status names the face target or **XY grid**; hold Alt to bypass both. A grid snap engages only after the drag has moved and the candidate is close on screen, so starting a drag does not jump an existing pose. Release to accept one undoable pose edit, or press Esc to cancel. Snap placement creates no permanent constraint; changing spacing never repositions existing boards. Spacing is saved with the project and restored when reopened.
