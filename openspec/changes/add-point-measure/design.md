# Design

## Geometry

Everything pickable is a posed box: boards (`blank_dimensions`), dimensioned
hardware, and slide and hinge parts (`hardware_mesh::pick_boxes`).
`snap::scene_boxes` lists them; `snap::nearest_hit` is the shared ray test,
also used by the viewport's object picking. No GPU readback is needed.

Per box the candidates are its 8 corners, 12 edge midpoints, 6 face centres,
the point on each edge closest to the pointer ray, and the surface hit.
Corners, midpoints and centres attract within 12 px, ranked by pixel
distance plus a small bias (0 / 3 / 5 px) so a corner wins unless the
pointer is clearly on something else. Edges attract within 8 px. A candidate
counts only if the ray from the eye to it meets nothing nearer (0.05 mm
tolerance). Fallbacks: the surface under the pointer; else the floor (Z = 0)
for a first point, or the camera-facing plane through the first point.

## Second point

- Locked (X/Y/Z, or Shift on an inferred axis or edge): the point lies on
  `from + s·direction`. A hovered feature gives its perpendicular foot on
  that line and a dotted guide back to the feature; otherwise the line's
  point closest to the pointer ray.
- Unlocked: a feature already on a world axis through the first point is
  tagged with that axis. Away from corners, midpoints and centres, an axis
  line within 10 px on screen pulls the point onto it. An axis pointing at
  the viewer is skipped.
- Alt: only the surface under the pointer, no inference.

## Kept points

A measured end is a world point or an anchor (object id plus a point in its
local frame) resolved every frame, so measurements follow moves and resizes.
Locked feet and points on slide and hinge parts stay in world coordinates. A
measurement whose object is deleted is dropped.

## Presentation

Lines and markers are 2D overlays drawn on top, like SketchUp. A line along
a world axis takes the axis colour; otherwise the accent. Marker shapes:
filled dot (corner), ring (midpoint), ring with dot (face centre), square
(edge), diamond (face or axis), cross (floor or free). The readout card is
an `egui::Area` docked at the bottom left, above the selected-board strip
and right of the tool strip. Lengths use `format_length` with trailing zeros
trimmed.

## Phase 3: real shapes, object axes, perpendicular, copy

### Fitting geometry

`hardware_mesh::pick_parts` returns, for each pickable fitting part, its
envelope box, its exact world-space edges and triangles when the envelope is
not the real shape, and the id of the object that moves it. Feet use the
edges and triangles already built by `foot_solid`; a hinge cup and its plate
are separate parts anchored to the door and the mounting board; each slide
member is an exact box anchored to its drawer side.

`snap::SceneBox` gains `shape` (exact edges and triangles) and `anchor`
(anchor object id, its world pose, and the fitting id to require). For a
shaped part the candidates are the real edge endpoints, edge midpoints and
the closest point on each edge; triangle vertices add cap centres. Surface
hits and hidden-feature checks use ray-triangle intersection against the same
triangles, so a tapered foot's actual bottom or a hinge cup's rim is
measurable and its real surface is used for occlusion. Unshaped boxes keep
the existing corner/edge/face-centre candidates. An anchored point stores the
object id plus the point in that object's local frame, so it follows moves
and resizes; a fitting-anchored point also stores the fitting id and is
dropped when that fitting is deleted.

### Locks

The first point remembers the visible face normal from the nearest hit.
X / Y / Z cycle per key: world axis → the object axis through the first
point → off. Object axes come from the anchor object's world rotation; a
world or free first point has no object axes, so the second press releases.
P toggles perpendicular-to-face and clears an axis lock; an axis key clears
perpendicular. Shift still captures an inferred world axis or an edge
direction when no explicit lock is active. A locked line projects a hovered
feature onto itself and shows the dotted guide back to the feature.

The lock carries its named axis and whether that axis is local, plus a
perpendicular flag, so the readout can say **X · local · locked** or
**Perpendicular · locked** without falsely colouring a rotated local axis as
a world axis.

### Copy

Cmd+C (Ctrl+C on Linux) copies the distance currently shown: the in-progress
line if the pointer is hovering a second point, otherwise the last kept
measurement. It uses `length_text` with the project display unit and the UI
language, including the unit suffix, and does nothing when there is no
measurement. The clipboard is the only side effect; no project or session
state is saved. A short "Copied" line in the readout confirms the action.
