# Spec Delta

## Purpose

Define free-form assembly placement, predictable nested selection and grouping, and measurements that distinguish wooden bodies from attached hardware.

## ADDED Requirements

### Requirement: Assembly selection SHALL identify physical parts consistently

The editor SHALL support selecting individual parts, assemblies, and multiple objects in the scene and an object list. Selection SHALL visibly identify the selected objects and active editing target. Selecting a wooden part SHALL identify the same part in its stock allocation view, or report it unallocated. Changing stock placement SHALL not move the part in the furniture.

#### Scenario: Select a board from either view
- **WHEN** a user selects an allocated board in the assembly or its stock placement
- **THEN** both views identify the same stable physical part

#### Scenario: Move a cutting-layout placement
- **WHEN** a user changes a board's valid placement on a stock sheet
- **THEN** its assembly position and orientation remain unchanged

### Requirement: Placement SHALL support precise free-form editing

The editor SHALL support dragging, face-based placement, and numeric position and rotation editing without requiring a cabinet preset. Numeric controls SHALL identify their coordinate frame and angular units. A user SHALL be able to select a source face and target face with a visible placement preview showing orientation and offset. Invalid or nonfinite numeric placement inputs SHALL be rejected without changing the committed pose.

#### Scenario: Place a board against a face
- **WHEN** a user chooses source and target faces and enters a placement offset
- **THEN** the editor previews the resulting pose and commits that pose only on acceptance

#### Scenario: Invalid position input
- **WHEN** a user submits a nonfinite position or invalid rotation value
- **THEN** the existing pose is preserved and the invalid input is identified

### Requirement: Snapping SHALL be temporary placement assistance

Snapping SHALL show the candidate target and resulting pose before commitment. Accepted ordinary snapping and face placement SHALL not create persistent constraints or mechanical relationships. Moving a previous target afterward SHALL not move an independently placed board. The user SHALL be able to bypass snapping for free placement.

#### Scenario: Move a former snap target
- **WHEN** a board is snapped to another board and the target is subsequently moved outside any shared grouping or explicit hardware relationship
- **THEN** the placed board remains at its committed pose

#### Scenario: Bypass a snap candidate
- **WHEN** a user disables or bypasses snapping while dragging near a candidate
- **THEN** the board can be placed freely rather than being forced back onto the candidate

### Requirement: Grid snapping SHALL be configurable and optional

The editor SHALL display a world-XY placement grid and let the user set a positive, finite spacing per project, initially 10 mm, using supported metric or imperial input with explicit conversion/rounding confirmation when required. The spacing SHALL persist in the project; opening an older project without a grid setting SHALL use the default without changing its board poses. During Move board dragging, grid snapping SHALL preview the resulting world pose and identify the grid as the snap target, distinct from a visible face candidate. The user SHALL be able to bypass grid and face snapping together with the documented bypass control. An accepted grid snap SHALL store only the resulting pose, not a constraint. Changing spacing or enabling the grid SHALL NOT quantize any existing pose; in particular derived sub-micrometre pose coordinates SHALL remain intact unless that board is explicitly moved.

#### Scenario: Change project grid spacing
- **WHEN** a user sets the grid spacing to 1/2 inch and saves and reopens the project
- **THEN** the grid and subsequent snap previews use 12.7 mm spacing while existing board poses remain unchanged

#### Scenario: Preview and bypass grid snap
- **WHEN** a board is dragged near a grid point while no preferred visible face snap applies
- **THEN** the preview identifies the grid and shows the proposed pose, and holding the bypass control previews free placement instead

#### Scenario: Reject invalid spacing and cancel placement
- **WHEN** a user enters zero, a nonfinite value, or an unconfirmed rounded spacing, or cancels a grid-snapped drag
- **THEN** the prior spacing or board pose respectively remains unchanged and no placement undo entry is added

#### Scenario: Open a project predating grid configuration
- **WHEN** a compatible older saved project has no grid-spacing field
- **THEN** it opens with the 10 mm default without moving parts or changing its saved document merely by opening it

### Requirement: Placement previews SHALL be cancellable transactions

Dragging, snapping, numeric pose previews, and face-placement previews SHALL distinguish tentative from committed geometry. Cancelling SHALL restore the pre-operation pose and selection without changing project revision, allocations, or undo history. Accepting a placement SHALL produce one undoable transaction rather than a transaction per preview update.

#### Scenario: Cancel a multi-step placement
- **WHEN** a user previews several snap targets and offsets and then cancels
- **THEN** every affected object returns to its original pose and the project remains unmodified by that preview

### Requirement: Multi-selection transforms SHALL affect each object once

Multi-selection translation and rotation SHALL apply one shared transform about a displayed pivot while preserving relative poses within the selected set. If an assembly and its descendant are both selected, the descendant SHALL receive the transform only through the selected ancestor, not a second time. Invalid transform input SHALL leave the entire selected set unchanged. Ordinary transforms SHALL not link dimensions.

#### Scenario: Parent and child are selected together
- **WHEN** a selected assembly and one selected descendant are translated by 100 mm
- **THEN** the descendant moves 100 mm with the assembly rather than 200 mm

#### Scenario: Rotate several independent parts
- **WHEN** several parts are rotated around the displayed shared pivot
- **THEN** their relative poses are preserved and none of their local dimensions change

### Requirement: Nested grouping SHALL preserve world poses and reject cycles

Users SHALL be able to group, nest, reparent, and ungroup objects. These hierarchy edits SHALL preserve the affected objects' world positions and orientations at commitment. Moving an assembly SHALL move its descendants together without coupling their dimensions. Attempting to parent an object to itself or one of its descendants SHALL fail without partially changing the hierarchy. Duplicating an assembly SHALL create fresh identities for all copied descendants and preserve their internal relative poses without sharing physical allocations.

#### Scenario: Reparent between rotated assemblies
- **WHEN** a board is moved from one assembly to another with a different orientation
- **THEN** its world pose is unchanged immediately after reparenting

#### Scenario: Reject a grouping cycle
- **WHEN** a user attempts to make an assembly a child of its own descendant
- **THEN** the operation is rejected and all parentage and poses remain unchanged

#### Scenario: Ungroup a nested assembly
- **WHEN** an assembly is ungrouped
- **THEN** its contents retain their world poses and physical identities

### Requirement: Visibility SHALL not remove fabrication obligations

Hiding an object or assembly SHALL affect scene visibility only. Hidden wooden parts SHALL remain in parts lists, allocations, cutting requirements, cost estimates, and shop-ready validation. A hidden object SHALL remain discoverable and selectable through the object list so users can reveal it. Hiding SHALL not mean deletion or exclusion from manufacture.

#### Scenario: Hide an unallocated board
- **WHEN** an unallocated required board is hidden with its parent assembly
- **THEN** it remains an actionable unallocated part and continues to prevent a shop-ready plan

### Requirement: Measurements SHALL distinguish chosen body and overall extents

Users SHALL be able to choose measured objects and a displayed measurement frame to inspect bounding dimensions. Body-only measurements SHALL allow hardware such as feet to be excluded explicitly; overall measurements SHALL include the selected hardware extents. Labels SHALL identify the selected scope and frame rather than imply that body and overall height are equal. Example furniture dimensions SHALL not impose defaults, a cabinet preset, or a fixed construction arrangement.

#### Scenario: Measure a body with feet
- **WHEN** an example body measures 820 mm wide, 600 mm deep, and 2300 mm high in the selected frame, with feet extending below it
- **THEN** selecting the body alone reports its 2300 mm height and selecting the body plus feet reports their combined bounding height with the scope clearly labelled

#### Scenario: Feet do not extend beyond the body bounds
- **WHEN** selected hardware lies entirely inside the selected body's bounds
- **THEN** overall measurement reflects the actual combined bounds rather than blindly adding a nominal foot height
