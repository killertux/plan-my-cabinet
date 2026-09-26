# boards-and-materials Specification

## Purpose

Define precise rectangular wooden parts, deliberate material and grain changes, and predictable independent and batch dimension editing.

## Requirements

### Requirement: Boards SHALL have explicit local dimensions

New board SHALL create a rectangular wooden part with user-entered length and width and material-derived thickness. Length, width, and thickness SHALL name the board's own local axes and SHALL not be reassigned by world orientation, camera orientation, or which face is visible. The initial rectangular stock blank SHALL equal these dimensions. Creation SHALL show the effective material and thickness before confirmation.

#### Scenario: Create a rotated side panel
- **WHEN** a user creates a 2300 mm long, 600 mm wide board from an 18 mm material and then rotates it upright
- **THEN** its local dimensions remain length 2300 mm, width 600 mm, and thickness 18 mm even though its world-axis extents change

### Requirement: Board dimension inputs SHALL be valid before commitment

Board length, width, and thickness SHALL be finite and strictly positive at project precision. Missing, nonnumeric, nonfinite, zero, negative, or unrepresentable positive inputs SHALL prevent commitment with field-specific feedback. Cancelling or failing creation SHALL leave no partial part or allocation. A failed edit SHALL retain all previously committed dimensions.

#### Scenario: Invalid creation is rejected
- **WHEN** the New board dialog contains a zero width or a missing material thickness
- **THEN** no board is created and the invalid field is identified

#### Scenario: Invalid edit preserves the board
- **WHEN** a user submits a negative length for an existing allocated board
- **THEN** the previous dimensions and allocation state remain unchanged

### Requirement: Material changes SHALL be explicit about affected parts

Materials SHALL expose their identifying name, thickness, and relevant grain default. Assigning another material to a board SHALL show its resulting thickness before commitment. Editing a material used by existing boards SHALL require an explicit choice of whether to apply the changed properties to existing dependants or preserve their previous effective properties. The application SHALL identify affected boards and resulting allocation conflicts; it SHALL not silently resize or silently reallocate them. Thickness edits SHALL use the same local anchor semantics as other dimension edits.

Material identity SHALL denote the user-declared composition/grade, with thickness as a default for new assignments. Each board and physical stock piece SHALL retain its own effective thickness. Allocation compatibility SHALL require equal material identity and equal effective thickness, not equal current material defaults. Updating defaults SHALL never update measured stock thickness implicitly. Different material identities SHALL remain incompatible until explicitly reassigned by the user.

#### Scenario: Shared material thickness is revised
- **WHEN** a user changes a material from 18 mm to 15 mm while several boards use it
- **THEN** the application identifies the affected boards and requires an explicit decision before their effective thickness changes

#### Scenario: Existing board properties are retained
- **WHEN** the user chooses to preserve existing effective properties during a material edit
- **THEN** those boards keep their previous thickness and grain state across save/load while new assignments use the revised properties

#### Scenario: Preserved thickness retains stock compatibility
- **WHEN** material M's default changes from 18 to 15 mm while an existing 18 mm board and stock piece preserve their effective values
- **THEN** their allocation remains compatible across save/load; directly changing only the board to 15 mm makes it incompatible with that 18 mm stock without changing the stock itself

### Requirement: Grain SHALL support a per-board override

Plywood boards SHALL default to grain along local length. Each board SHALL expose whether grain follows its material default or a per-board override, with choices for local length, local width, or no grain restriction. Grain SHALL remain attached to local board axes through assembly rotation. Allocation SHALL use the effective grain setting, and a change that invalidates an allocation SHALL surface the conflict without silently moving the board or its allocation.

#### Scenario: Override survives rotation and default changes
- **WHEN** a board with an explicit local-width grain override is rotated and its material grain default changes
- **THEN** the board retains its local-width override

#### Scenario: Grain edit conflicts with stock orientation
- **WHEN** a confirmed grain change makes the existing stock placement incompatible
- **THEN** the placement is visibly marked conflicted rather than silently rotated or discarded

### Requirement: Resizing SHALL preserve the chosen local anchor

Each dimension edit SHALL allow start, center, or end anchoring on the edited local axis and SHALL show the active anchor before commitment. Start SHALL preserve the minimum-coordinate face, end the maximum-coordinate face, and center the midpoint on that axis in the board's current pose. Resizing SHALL preserve orientation and the unedited dimensions. Anchors SHALL apply in the board's local frame even inside rotated assemblies.

#### Scenario: Resize a rotated board from its end
- **WHEN** a board in a rotated assembly changes local length using the end anchor
- **THEN** its local-length end face stays at the same world position and the opposite face moves along the board's local-length axis

#### Scenario: Centered thickness change
- **WHEN** thickness changes with the center anchor
- **THEN** the two thickness faces move equally in opposite local directions and the board's orientation, length, and width remain unchanged

### Requirement: Duplicates SHALL be independent physical parts

Duplicating a board SHALL create a new stable part identifier with copied dimensions, material assignment, effective grain settings, and the requested pose. Later dimension or pose edits SHALL not propagate between the original and copy. A duplicate SHALL require its own stock allocation; it SHALL not share the original's physical allocation. Failure to allocate SHALL leave the duplicate present and visibly unallocated.

#### Scenario: Edit an independent copy
- **WHEN** a user duplicates a board and changes only the copy's length
- **THEN** the original remains unchanged and both boards are separately identifiable in the parts list and allocation view

#### Scenario: No stock fits the duplicate
- **WHEN** the original is allocated but no available stock can accept its duplicate
- **THEN** the duplicate remains in the design as unallocated and the original retains its allocation

### Requirement: Multi-selection dimension edits SHALL commit atomically

The application SHALL allow the same named local dimension to be edited across multiple selected boards, including setting a common value when their current values differ. It SHALL show mixed values, the affected boards, and each effective start, center, or end anchor before commitment. Each distinct board SHALL be edited once, including when selection includes both an assembly and a descendant. All targeted board dimensions SHALL commit together or none SHALL change if an input or target is invalid. A valid edit that causes allocation conflicts SHALL retain the design edit and expose those conflicts as part of the same undoable transaction.

#### Scenario: Apply local width to differently oriented boards
- **WHEN** a user sets width to 600 mm for several differently rotated boards using a center anchor
- **THEN** each board's local width becomes 600 mm around its own local midpoint in one transaction

#### Scenario: One target makes the batch invalid
- **WHEN** any targeted board cannot accept the requested dimension edit
- **THEN** no targeted board or dependent allocation is changed and the failing target is identified
