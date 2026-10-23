# Spec Delta

## MODIFIED Requirements

### Requirement: Measurements SHALL distinguish chosen body and overall extents

The inspector SHALL expose bounding dimensions for selected boards, assemblies, and supported hardware while the Measure tool is active, using an explicitly selected Body or Overall scope and World or selected-object measurement frame. The inspector readout SHALL identify scope, frame, and units and SHALL preserve those choices when entering or leaving the tool. Measurement SHALL remain read-only, include hidden selected descendants, deduplicate descendants, and report unavailable extents for undimensioned hardware rather than fabricate an overall size. Overall bounds SHALL use actual hardware extents rather than add nominal hardware heights. Bounding dimensions SHALL remain distinct from editable board-local length, width, and thickness.

#### Scenario: Measure a body with feet
- **WHEN** an example body measures 820 mm wide, 600 mm deep, and 2300 mm high in the selected frame, with feet extending below it
- **THEN** selecting the body alone reports its 2300 mm height and selecting the body plus feet reports their combined bounding height with the scope clearly labelled

#### Scenario: Feet do not extend beyond the body bounds
- **WHEN** selected hardware lies entirely inside the selected body's bounds
- **THEN** overall measurement reflects the actual combined bounds rather than blindly adding a nominal foot height

#### Scenario: Measure a rotated body with feet
- **WHEN** a user measures a rotated assembly first as Body and then as Overall in its object frame
- **THEN** the inspector identifies that frame and scope, excludes feet from Body, includes their actual extents in Overall, and leaves geometry and undo history unchanged

#### Scenario: Resume measurement with unsupported hardware
- **WHEN** a user returns to Measure with Overall selected and selected hardware has no dimensioned extents
- **THEN** the scope and frame are retained and the missing extents are explained instead of showing a complete overall measurement

## ADDED Requirements

### Requirement: Measure tool SHALL measure between snapped points

In the Measure tool, two clicks in the 3D view SHALL measure the straight distance between two points and show it with its X, Y and Z components in the display unit and presentation locale. Points SHALL snap to visible corners, edge midpoints, face centres, edges and faces of boards, dimensioned hardware, and the drawn shapes of feet, hinges and drawer slides, and SHALL name the snap while hovering. Shaped fittings SHALL offer the corners and edges of their actual geometry, and face snaps SHALL use their actual surface, rather than only the bounding box around each part. A point on a fitting SHALL be anchored to the object that moves that fitting and SHALL follow its edits. Features hidden behind nearer objects SHALL NOT be snapped to. Measuring SHALL NOT change the selection, the project or undo history, and dragging SHALL still orbit and pan between the two clicks.

#### Scenario: Gap between two shelves
- **WHEN** a user clicks a corner of a lower shelf, presses Z, and clicks near a corner of the shelf above
- **THEN** the second point is that corner projected onto the vertical line through the first, and the distance equals their vertical gap

#### Scenario: Hidden corner
- **WHEN** the pointer is over a corner hidden behind another board
- **THEN** the point snaps to the nearer board instead

#### Scenario: Axis inference and free placement
- **WHEN** the second point shares a world axis line with the first
- **THEN** the hover names that axis and the line takes the axis colour, and holding Alt places the point on the surface without snapping

#### Scenario: Real fitting shape
- **WHEN** the pointer is over the rim of a tapered foot or the edge of a hinge cup
- **THEN** the snap is to that rim or edge of the drawn part, not to a corner of its envelope, and a face snap lands on the drawn surface

#### Scenario: Fitting moved after measuring
- **WHEN** a user measures from a point on a foot, hinge or slide and then moves the board or hardware that carries it
- **THEN** the measured end follows the fitting, and it disappears if the fitting is deleted

### Requirement: Measure tool SHALL lock to object axes and face normals

X, Y and Z SHALL lock the second point to the corresponding world axis through the first point on the first press, to the corresponding axis of the object under the first point on the second press, and release on the third. Pressing P SHALL lock the line perpendicular to the face under the first point and SHALL clear any axis lock; pressing an axis key SHALL clear the perpendicular lock. A lock SHALL project a hovered feature onto the locked line and SHALL show the guide back to that feature. When the first point has no anchored object, the second press of an axis key SHALL not invent a local axis.

#### Scenario: Rotated shelf
- **WHEN** a user starts on a rotated shelf, presses X twice, and clicks a point off the shelf along that shelf's own length
- **THEN** the second point is projected onto the shelf's local X line, not the world X line

#### Scenario: True gap between rotated panels
- **WHEN** a user starts on a face and presses P
- **THEN** the measured line runs normal to that face, so a hovered point on a parallel panel gives the perpendicular gap between the panels

### Requirement: Measure tool SHALL copy the displayed distance

Cmd+C on macOS or Ctrl+C on Linux SHALL copy the distance currently shown by the Measure tool, using the project display unit and the UI language and including the unit suffix. It SHALL copy the in-progress distance when the pointer is hovering a second point, otherwise the last kept measurement, and SHALL do nothing when there is no measurement. Copying SHALL NOT edit the project, the selection, the undo history or any saved session state.

#### Scenario: Copy a measured gap
- **WHEN** a user finishes a measurement and presses Cmd+C
- **THEN** the clipboard holds the same formatted length that the readout shows, ready to paste into a dimension field

### Requirement: Kept measurements SHALL be session-only and follow their objects

Finished measurements SHALL stay visible in the Measure tool until cleared. Esc SHALL cancel a measurement in progress and otherwise clear all kept measurements; Delete SHALL remove the last one. A measured point on a board, hardware item or fitting SHALL follow that object when it moves or is resized, and a measurement SHALL disappear when its object or fitting is deleted. Kept measurements SHALL NOT be saved with the project.

#### Scenario: Board moved after measuring
- **WHEN** a user measures to a board's corner and then moves the board
- **THEN** the measurement ends at the corner's new position
