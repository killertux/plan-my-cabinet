## ADDED Requirements

### Requirement: Inline board dimensions SHALL share one validated draft

The single-board inspector and selection HUD SHALL expose the same pending length and width edit for the same physical board, including entered text, captured entry unit and input locale, active local anchor, validation feedback, and rounding consent. Editing either surface SHALL update the other without creating competing drafts or duplicate commits. Acceptance SHALL validate the complete edit and create one undoable transaction using existing local-axis and anchor semantics; cancellation SHALL preserve committed dimensions and allocations. Focus changes alone SHALL not commit rounded display text. Rounding beyond 0.001 mm precision SHALL show entered and rounded quantities and require explicit consent; changing the proposed value SHALL clear prior consent. Multi-board resizing SHALL retain mixed-value presentation, before/after previews, per-board anchors, and atomic commitment.

#### Scenario: Move an unfinished edit between surfaces
- **WHEN** a user enters an invalid length in the HUD and then focuses the inspector length field
- **THEN** the inspector shows the same pending text and error, the committed board remains unchanged, and correcting and accepting it creates only one edit

#### Scenario: Rounding consent belongs to the current proposal
- **WHEN** a user enters `1/64 in`, confirms the displayed 0.397 mm rounding, and then changes the value in the other editing surface
- **THEN** the old consent is cleared and any new inexact conversion requires fresh explicit consent before commitment

#### Scenario: Rounded display is not an edit
- **WHEN** a 12.345 mm dimension is displayed as 12.35 mm and the user focuses and leaves either inline field without editing
- **THEN** the committed value stays 12.345 mm and no undo entry is created

### Requirement: Effective thickness summaries SHALL remain truthful and editable through advanced controls

The inspector and HUD SHALL display the board's actual effective thickness as a read-only summary, not substitute the material's current default. A material-derived label SHALL only assert the relationship that is true; preserved or explicitly overridden thickness SHALL be distinguished when it differs from the default. The existing advanced thickness editor SHALL remain discoverable from the inspector and SHALL support local anchors, validation, rounding confirmation, undo, and allocation-conflict feedback. Material reassignment and preserve/apply decisions SHALL continue to show affected boards and proposed thickness without silently changing stock thickness or allocations.

#### Scenario: Preserved thickness differs from the default
- **WHEN** material M now defaults to 15 mm but an existing board preserved its 18 mm effective thickness
- **THEN** its HUD and inspector show 18 mm with truthful provenance and expose advanced thickness editing rather than falsely reporting a material-derived 15 mm

#### Scenario: Advanced thickness change conflicts with stock
- **WHEN** a user accepts an anchored change from 18 mm to 15 mm for a board allocated to 18 mm stock
- **THEN** the board changes in one undoable edit, its stock retains its measured thickness, and the incompatibility is shown without silent reallocation

### Requirement: New board SHALL preview dimensions and first fit without mutation

New board SHALL expose name, material, local length and width, effective material-derived thickness, and material-default or explicit grain choice, with live parsing and conversion feedback. Valid drafts SHALL show their resulting dimensions and first-fit outcome against current stock order and constraints, including proposed stock and position or a reason no fit is available. Computing or refreshing this preview SHALL not create a board, reserve stock, change allocations, mark the project dirty, advance revision, or add undo history. Invalid or unconfirmed rounded input SHALL not be presented as an accepted fit. Confirmation SHALL revalidate against current project state and create the board and its actual fit outcome in one transaction; no fit SHALL leave a valid created board visibly unallocated.

#### Scenario: Cancel a fitting draft
- **WHEN** a user enters a valid board that previews a fit on S1, changes its size several times, and cancels
- **THEN** no board or allocation is created, stock availability is unchanged, and dirty state, revision, and undo history are unchanged

#### Scenario: Preview cannot promise obsolete space
- **WHEN** stock availability or constraints change after a fit preview and before confirmation
- **THEN** confirmation uses the current state and reports the actual fit or unallocated outcome rather than committing the obsolete preview placement

### Requirement: Material display colors SHALL be saved manufacturing-neutral metadata

Material creation and editing SHALL offer display-color swatches whose chosen color is saved with the project and restored on load. Accepted color edits SHALL participate in project dirty state and undo/redo. Color-only edits SHALL not change physical material identity, thickness, grain, stock compatibility, allocations, cut validity, optimization manufacturing freshness, or manufacturing freshness of export receipts. Old projects without a color SHALL use a deterministic neutral fallback without rewriting the source file or becoming dirty merely on open. Material-tinted scene faces and material swatches SHALL use the saved color while retaining distinguishable active selection, multi-selection, face-placement, and warning highlights. Disabling material tint in app preferences SHALL use neutral scene shading without deleting saved colors.

#### Scenario: Change only a material color
- **WHEN** a user changes a material's display color in a saved project with a current cut plan and export receipt
- **THEN** the project becomes dirty and the color can be undone or saved and reopened, while allocations, cut validity, and manufacturing freshness remain unchanged

#### Scenario: Open an older project and toggle tint
- **WHEN** a project without display colors opens and the user toggles material tint
- **THEN** boards use the neutral fallback, the source file is not rewritten, and the presentation toggle does not create a project edit
