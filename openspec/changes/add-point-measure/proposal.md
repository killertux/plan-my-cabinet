# Proposal

## Why

The Measure tool only showed the bounding box of the selection, in a card at
the top right of the 3D view that the camera bar and the "Face + grid" snap
chip covered. It could not answer the everyday workshop questions: how big is
the gap between two shelves, how far is this hinge from the top, does this
diagonal match. Picking an exact point in 3D with a mouse is hard, so the
tool has to help the way SketchUp's tape measure does.

## What Changes

- **Point-to-point measuring.** In Measure, two clicks measure between two
  points. Drags still orbit and pan, so the camera can turn between clicks.
  Clicks no longer change the selection.
- **Snapping.** Points snap to corners, edge midpoints, face centres, edges
  and faces of boards, dimensioned hardware, slides and hinges, within a
  small screen radius. Features hidden behind nearer boxes are skipped. A
  marker shape and a tooltip name the snap. Off the scene, a first point
  lands on the floor.
- **Inference and locks.** The second point is tagged when it shares an X,
  Y or Z line with the first, and snaps to that line near it. X / Y / Z lock
  an axis (again to unlock). Shift holds the shown axis or edge direction.
  With a lock, a hovered feature is projected onto the locked line, so a gap
  is measured exactly along an axis. Alt places points freely.
- **Readout.** A pill at each line's middle shows the distance. A card docked
  at the bottom left shows the distance, ΔX / ΔY / ΔZ and the two snap types,
  in the display unit and the UI locale.
- **Kept measurements.** Finished measurements stay on screen until Esc
  (which first cancels one in progress) or Delete (the last one). They are
  attached to the boards they snapped to and follow edits. Session only;
  never saved.
- **Bounding dimensions** move to the inspector only, where their scope and
  frame controls already were.

## Impact

- Library: `render::snap` (features, occlusion, inference); the viewport's
  box hit test moves there and picking shares it.
- Desktop: `viewport::measure_tool` (state, input, overlay); the canvas
  readout card is removed; shortcut list and status hint updated.
- No project data, persistence, export or MCP change.
