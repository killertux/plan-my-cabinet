## ADDED Requirements

### Requirement: Design SHALL expose the outliner inspector and selection HUD

The Design workspace SHALL provide the chosen handoff 2a outliner, material and stock summaries, live 3D viewport, selection inspector, and floating tools. The outliner SHALL distinguish the active target, other selected objects, nested assemblies, visibility, and allocation issues without replacing names with selection prefixes. Hidden objects SHALL remain discoverable and revealable. The inspector SHALL identify the actual part, parent, allocation state, material, grain, local dimensions and anchor, frame-labelled position and rotation, stock preview, and bounding measurement. A single selected board SHALL expose a bottom HUD with its name, shared inline length and width edits, effective-thickness summary, and functional face-placement, duplicate, hide, and delete actions. Multiple or empty selection SHALL not display a misleading single-board HUD. Existing grouping, reparenting, ungrouping, assembly duplication, multi-selection transforms, and batch resizing SHALL remain accessible.

#### Scenario: Inspect an allocated nested board
- **WHEN** a board inside a rotated assembly is the active selection
- **THEN** the outliner, inspector, HUD, dimension label, and stock preview identify that board and show its real local dimensions, parent, and allocation rather than example values

#### Scenario: Hidden unallocated descendant
- **WHEN** a user hides an assembly containing an unallocated board
- **THEN** the outliner retains the descendant and its allocation issue, allows it to be selected and revealed, and does not remove its manufacturing obligations

### Requirement: Viewport controls SHALL operate on the actual scene camera

The viewport SHALL render current project geometry and tentative placement geometry through the native scene, with Navigate, Move board, Measure, and Frame selection controls and Iso, Front, Right, Top, Perspective, and Orthographic camera choices. Projection choices SHALL change actual rendering and picking consistently, and presets SHALL orient the camera without transforming project objects. Framing SHALL use the selected geometry's actual bounds. View overlays SHALL reflect current geometry and selection rather than reproduce a fixed HTML illustration. Camera operations SHALL not create project edits, allocation changes, or undo entries.

#### Scenario: Switch projection and select a board
- **WHEN** a user changes from Perspective to Orthographic and selects a visible board
- **THEN** the rendered projection changes and picking selects the board under the pointer in that projection while all committed poses remain unchanged

#### Scenario: Frame a different assembly
- **WHEN** a user selects an assembly with dimensions unlike the handoff example and chooses Top then Frame selection
- **THEN** the real selected assembly fits the viewport in the requested camera view and its displayed dimensions come from that assembly

### Requirement: Measure tool SHALL preserve measurement scope and frame

The Measure tool SHALL expose bounding dimensions for selected boards, assemblies, and supported hardware using an explicitly selected Body or Overall scope and World or selected-object measurement frame. The viewport readout and inspector SHALL identify scope, frame, and units and SHALL preserve those choices when entering or leaving the tool. Measurement SHALL remain read-only, include hidden selected descendants, deduplicate descendants, and report unavailable extents for undimensioned hardware rather than fabricate an overall size. Overall bounds SHALL use actual hardware extents rather than add nominal hardware heights. Bounding dimensions SHALL remain distinct from editable board-local length, width, and thickness.

#### Scenario: Measure a rotated body with feet
- **WHEN** a user measures a rotated assembly first as Body and then as Overall in its object frame
- **THEN** the readout identifies that frame and scope, excludes feet from Body, includes their actual extents in Overall, and leaves geometry and undo history unchanged

#### Scenario: Resume measurement with unsupported hardware
- **WHEN** a user returns to Measure with Overall selected and selected hardware has no dimensioned extents
- **THEN** the scope and frame are retained and the missing extents are explained instead of showing a complete overall measurement

### Requirement: Snap popover SHALL control face and grid assistance independently

The snap chip SHALL truthfully summarize enabled assistance and current grid spacing. Its popover SHALL provide independent face-snap and grid-snap toggles and access to validated project grid spacing. During Move board dragging, only enabled assistance SHALL generate candidates; visible face candidates SHALL take precedence over grid candidates when both apply. The documented Alt bypass SHALL temporarily bypass both forms of assistance without changing their configured toggles. Candidate highlighting SHALL distinguish face source and target from a grid target. Toggling assistance or changing spacing SHALL not move existing parts; accepted placement SHALL store only the resulting pose.

#### Scenario: Use either snap mode alone
- **WHEN** a user enables grid snapping but disables face snapping and drags near both a face and a grid point
- **THEN** only grid assistance is offered, and reversing the toggles offers only eligible face assistance

#### Scenario: Temporarily bypass both modes
- **WHEN** both modes are enabled and the user holds Alt during a drag then releases it
- **THEN** the preview is free while Alt is held and eligible snapping resumes after release without changing the toggles or committing a pose until placement acceptance

### Requirement: Pose quick presets SHALL be explicit cancellable previews

Numeric pose editing SHALL provide Stand up, Lay flat, and Turn 90° Z presets alongside editable position and degree-labelled rotation, an explicit World or Local-parent frame, a visible pivot, and derived extents. Each preset SHALL disclose its resulting orientation and rotation axis in the selected frame; Stand up and Lay flat SHALL identify which board-local face or axis is aligned. Invoking a preset SHALL update a tentative preview and numeric fields, not silently accept a transform. Apply SHALL commit one validated undoable placement transaction; Cancel SHALL restore the pre-operation pose and selection without modifying project revision, allocations, or undo history. Multi-selection transforms SHALL retain shared-pivot and descendant-deduplication behavior.

The board's local X/Y plane is its broad face (X length, Y width, Z thickness). Lay flat SHALL align local X/Y/Z with the selected frame's X/Y/Z axes. Stand up SHALL align local X with frame +Z, local Y with frame +Y and local Z with frame −X. Turn 90° Z SHALL apply a relative +90° rotation around the selected frame's +Z from the current tentative pose. These presets SHALL pivot at the board pose origin in the selected frame rather than its centre; the dialog SHALL disclose that origin pivot. Switching the displayed coordinate frame SHALL not silently accept or reinterpret a tentative pose.

#### Scenario: Preview a preset inside a rotated parent
- **WHEN** a user chooses Local-parent frame and Turn 90° Z for a board inside a rotated assembly
- **THEN** the preview and fields show a 90-degree turn about that frame's Z axis and the displayed pivot, while committed geometry remains unchanged until Apply

#### Scenario: Cancel several preset previews
- **WHEN** a user previews Stand up, Lay flat, and numeric adjustments and then cancels
- **THEN** the original pose and selection are restored and no placement transaction is recorded

#### Scenario: Accept a valid preset
- **WHEN** a user accepts a valid preset preview after inspecting its frame, pivot, and extents
- **THEN** exactly one placement transaction is committed and one undo restores the original pose
