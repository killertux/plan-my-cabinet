## ADDED Requirements

### Requirement: Stock workspace inventory and inspector
The Stock workspace SHALL present material filters and summaries, a material-grouped stock table, and an inspector for the selected physical piece. Material rows SHALL show display swatches, thickness, board count, stock-piece count, and a warning when boards have no stock; All stock SHALL clear the filter. The table SHALL show actual global priority rank separately from the stable stock label, measured length/width/thickness, grain, trims, ownership, price, and allocation usage. The inspector SHALL expose measured size, all four grain choices, four edge trims around a derived usable rectangle, ownership, per-piece price, and assigned parts with an Open in cut plan action. Material editing SHALL retain the existing preserve/apply decision. Stock creation SHALL retain names, material selection and quantity expansion into individually identifiable pieces; editing SHALL retain validation, exact values and explicit rounding consent.

#### Scenario: Inspect filtered stock
- **WHEN** a user selects a material and then one of its stock pieces
- **THEN** the table filters to that material, the inspector shows that piece's measured and usable dimensions and allocations, and Open in cut plan focuses that same piece

#### Scenario: Add stock for a missing material
- **WHEN** a material used by boards has no stock and the user activates its Add sheet action
- **THEN** stock creation opens with that material and required thickness prefilled and commits only after valid explicit confirmation

#### Scenario: Preserve advanced stock inputs
- **WHEN** a user edits a piece with Unknown grain and separately measured trims requiring rounding
- **THEN** all four grain choices and trims remain editable, each rounded field requires consent, and cancelling preserves the previous piece and allocations

### Requirement: Stable stock aliases independent of priority
The system SHALL assign persistent, project-local, unique human-readable stock aliases using monotonic S labels for initially to-purchase pieces and O labels for initially owned pieces. Once assigned, aliases SHALL be separate from UUID identity, current ownership, and priority; changing ownership, dimensions, name, filter or priority SHALL NOT renumber a piece. Ownership SHALL be communicated by its explicit status rather than inferred from an alias prefix. Deleted aliases SHALL NOT be recycled for different pieces; undo/redo SHALL restore a piece's original alias. Quantity-created pieces SHALL receive distinct aliases. Existing projects without aliases SHALL receive deterministic labels without rewriting the source file on open, and labels SHALL persist on explicit save. All stock references across workspaces and newly generated output SHALL use consistent aliases while retaining access to underlying identities.

#### Scenario: Reorder and reopen a labelled piece
- **WHEN** S3 moves to priority rank 1 and the project is saved and reopened
- **THEN** it remains S3 with the same UUID and allocations, and its separately displayed priority rank is 1

#### Scenario: Restore a removed offcut
- **WHEN** O2 is deleted, another owned piece is created, and the user undoes that creation and then the deletion
- **THEN** the other offcut does not reuse O2 and the restored piece retains O2

### Requirement: Truthful grouped and filtered priority dragging
The stock table SHALL support undoable drag reordering of first-fit priority and expose the actual global rank on every row even when material grouping or filtering changes visual order. A drag within a displayed group or filtered subset SHALL permute only the global slots occupied by that subset, preserving hidden or other-group pieces in their existing slots. The drag destination SHALL explain its scope and resulting global rank. Users SHALL retain an accessible way to move a selected piece to any global priority position. Reordering SHALL NOT relocate existing allocations or silently optimize the plan.

#### Scenario: Reorder a filtered material without moving hidden slots
- **WHEN** global order is S1, O1, S2, O2, S3 and a filter showing S1, S2, S3 moves S3 before S1
- **THEN** global order becomes S3, O1, S1, O2, S2, visible ranks are 1, 3, 5, and existing part placements remain unchanged

#### Scenario: Grouping is not priority renumbering
- **WHEN** material grouping brings pieces with global ranks 2 and 6 next to each other
- **THEN** their rows still show ranks 2 and 6 and a global-position action can explicitly move either piece outside those group slots

### Requirement: Sheet navigation and actionable allocation diagnostics
The Cut plan workspace SHALL provide stock cards in actual priority order with aliases, material, dimensions, part count, utilization status, ownership and unused-state thumbnails, plus a sheet switcher and Add sheet or offcut action. Its Needs stock list SHALL include unallocated and conflicted boards, including hidden boards, with dimensions, all relevant diagnostic reasons, Reveal, and contextual stock creation where appropriate. Reveal and linked stock/part actions SHALL route to the identified object without changing its pose or allocation. Multiple reasons for one board SHALL remain discoverable without presenting duplicate board issue rows. Diagnostics SHALL retain material, thickness, grain, bounds, overlap, kerf, trim, missing or duplicate allocation, cut-model limitation and search-exhaustion distinctions.

#### Scenario: Reveal a hidden conflicted board
- **WHEN** a hidden board has overlapping and incompatible allocations and its Needs stock card is activated
- **THEN** one board card exposes its applicable reasons and Reveal selects and reveals that board with a route to its allocation repair

#### Scenario: Browse an unused piece
- **WHEN** an unused piece is selected from the sheet cards or stock inspector
- **THEN** the canvas focuses that piece and clearly indicates no placements rather than displaying another sheet's verified statistics

### Requirement: Complete repair workflow in the Cut plan workspace
The View/Repair control SHALL expose the existing atomic multi-placement repair workflow, including unlocked drag with live feasibility ghost, numeric X/Y entry with rounding consent, allowed quarter-turn, target-sheet transfer, explicit unallocation, lock/unlock, affected-sheet diagnostics, Accept repair, and Cancel repair including Escape. Intermediate invalid states SHALL remain stageable; acceptance SHALL require the existing feasible-witness checks for remaining placements on affected sheets. Invalid recorded positions SHALL remain visible as conflicts, not silently repacked. Workspace navigation during dirty stock drafts or active repair SHALL require an explicit valid commit/accept, discard/cancel, or Stay decision; Stay SHALL preserve the draft and active workspace, and invalid acceptance SHALL NOT navigate away.

#### Scenario: Repair requires multiple actions
- **WHEN** a user unlocks a part, transfers it numerically, and explicitly unallocates another while a conflict persists
- **THEN** the staged session retains all actions and diagnostics, acceptance checks all affected sheets, and Cancel restores the full pre-repair state

#### Scenario: Stay in unfinished repair
- **WHEN** a user requests another workspace during repair and chooses Stay
- **THEN** the workspace, numeric drafts, staged placements and diagnostic context remain available without a commit or cancellation

### Requirement: Stock spending summary with honest completeness
The Stock workspace SHALL expose the editable cut-fee chip and summary cards for used to-purchase stock, physical cutting charges and owned pieces consumed. Unknown prices and fees SHALL remain distinct from known zero, incomplete or unverified estimates SHALL be labelled, and exclusions SHALL remain visible. Currency changes SHALL retain the existing explicit price-replacement or relabelling flow without implied conversion.

#### Scenario: Unknown cutting fee
- **WHEN** used purchased stock has known prices but the cut fee is blank
- **THEN** the material subtotal and owned consumption remain visible, cutting and total completeness are explained, and Set opens the fee editor with Free (0) available as an explicit known-zero choice
