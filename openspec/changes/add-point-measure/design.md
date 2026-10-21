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
