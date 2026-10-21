# Spec Delta

## MODIFIED Requirements

### Requirement: Measure tool SHALL preserve measurement scope and frame

The inspector SHALL expose bounding dimensions for selected boards, assemblies, and supported hardware while the Measure tool is active, using an explicitly selected Body or Overall scope and World or selected-object measurement frame. The inspector readout SHALL identify scope, frame, and units and SHALL preserve those choices when entering or leaving the tool. Measurement SHALL remain read-only, include hidden selected descendants, deduplicate descendants, and report unavailable extents for undimensioned hardware rather than fabricate an overall size. Overall bounds SHALL use actual hardware extents rather than add nominal hardware heights. Bounding dimensions SHALL remain distinct from editable board-local length, width, and thickness.

#### Scenario: Measure a rotated body with feet
- **WHEN** a user measures a rotated assembly first as Body and then as Overall in its object frame
- **THEN** the inspector identifies that frame and scope, excludes feet from Body, includes their actual extents in Overall, and leaves geometry and undo history unchanged

#### Scenario: Resume measurement with unsupported hardware
- **WHEN** a user returns to Measure with Overall selected and selected hardware has no dimensioned extents
- **THEN** the scope and frame are retained and the missing extents are explained instead of showing a complete overall measurement

## ADDED Requirements

### Requirement: Measure tool SHALL measure between snapped points

In the Measure tool, two clicks in the 3D view SHALL measure the straight distance between two points and show it with its X, Y and Z components in the display unit and presentation locale. Points SHALL snap to visible corners, edge midpoints, face centres, edges and faces of boards and dimensioned hardware, and SHALL name the snap while hovering. Features hidden behind nearer objects SHALL NOT be snapped to. Measuring SHALL NOT change the selection, the project or undo history, and dragging SHALL still orbit and pan between the two clicks.

#### Scenario: Gap between two shelves
- **WHEN** a user clicks a corner of a lower shelf, presses Z, and clicks near a corner of the shelf above
- **THEN** the second point is that corner projected onto the vertical line through the first, and the distance equals their vertical gap

#### Scenario: Hidden corner
- **WHEN** the pointer is over a corner hidden behind another board
- **THEN** the point snaps to the nearer board instead

#### Scenario: Axis inference and free placement
- **WHEN** the second point shares a world axis line with the first
- **THEN** the hover names that axis and the line takes the axis colour, and holding Alt places the point on the surface without snapping

### Requirement: Kept measurements SHALL be session-only and follow their objects

Finished measurements SHALL stay visible in the Measure tool until cleared. Esc SHALL cancel a measurement in progress and otherwise clear all kept measurements; Delete SHALL remove the last one. A measured point on a board or hardware item SHALL follow that object when it moves or is resized, and a measurement SHALL disappear when its object is deleted. Kept measurements SHALL NOT be saved with the project.

#### Scenario: Board moved after measuring
- **WHEN** a user measures to a board's corner and then moves the board
- **THEN** the measurement ends at the corner's new position
