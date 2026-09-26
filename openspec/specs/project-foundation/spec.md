# project-foundation Specification

## Purpose

Define portable offline projects, stable identity, precise measurement semantics, and recoverable editing and persistence for woodworking plans.

## Requirements

### Requirement: Projects SHALL work offline and remain portable

The application SHALL create, edit, save, and reopen projects without an account, backend, or network connection. A saved project SHALL preserve its materials, wooden parts, stock, allocations, cutting and pricing assumptions, assemblies, hardware relationships, and project settings. Hardware configurations used by the project SHALL retain their pinned catalog revision and installation reference values so later catalog updates do not silently alter the design. Reopening on another supported computer SHALL not require machine-specific file paths or a catalog download for core editing.

#### Scenario: Move a project to an offline computer
- **WHEN** a user saves a project containing stock allocations and catalog hardware and opens the file on another supported computer without network access
- **THEN** the same design, allocation state, pricing assumptions, and pinned hardware values are available for editing

#### Scenario: Installed catalog changes
- **WHEN** a newer catalog supplies different values for hardware already used in a project
- **THEN** the existing project retains its pinned values until the user explicitly accepts a migration

### Requirement: Project objects SHALL retain stable identities

Project objects referenced by other objects, including parts, assemblies, stock, materials, and hardware, SHALL retain unique stable identifiers through save/load and undo/redo. Labels SHALL not serve as identity. Independent duplicates SHALL receive fresh identifiers, and references SHALL continue to target the intended original or copied object.

#### Scenario: Equal labels do not merge parts
- **WHEN** two independent boards have the same name and dimensions and the project is saved and reopened
- **THEN** they remain distinct parts with their original identifiers and separate allocation references

#### Scenario: Redo restores a created object
- **WHEN** creation of a board is undone and then redone
- **THEN** the restored board and its restored dependent references use the identity assigned by the original creation

### Requirement: Measurements SHALL use explicit project precision

Committed manufacturing dimensions, kerf, trims, and sheet allocation coordinates SHALL have a precision of 0.001 mm, independent of display units or language. Positive dimensions SHALL remain strictly positive at that precision. Input that is not representable at this precision SHALL require confirmation showing its rounded value before commitment; a value rounding to zero for a positive dimension SHALL be rejected. Explicit numeric spatial-position input SHALL use the same input precision, but derived rigid poses SHALL retain sub-micrometre values needed for rotations, reparenting and anchored resizing rather than snapping to a manufacturing grid. Within supported world coordinates of +/-1,000,000 mm per axis, preservation checks SHALL tolerate at most 0.000001 mm positional error and 0.000000001 radians angular error. Operations exceeding supported coordinates SHALL fail atomically with an explanation. Switching display units or display precision SHALL not mutate dimensions, positions, allocations, or project revision. The project SHALL distinguish storage precision, spatial tolerance, and rounded display text.

#### Scenario: Unit switching preserves the design
- **WHEN** a user repeatedly switches between millimetres and inches and back
- **THEN** committed measurements and allocation validity remain unchanged, with no new model revision or undo entry

#### Scenario: Excess precision needs consent
- **WHEN** a user enters a board length of 12.3456 mm
- **THEN** the application offers 12.346 mm for explicit confirmation and retains the previous value if confirmation is cancelled

#### Scenario: Rounding would erase a dimension
- **WHEN** a user enters a thickness of 0.0004 mm
- **THEN** the application rejects the input rather than creating a zero-thickness board

#### Scenario: Half-grid anchor movement
- **WHEN** a board's length grows by 0.001 mm about its centre inside a rotated assembly
- **THEN** its dimension remains exact at manufacturing precision and its derived pose preserves the centre within spatial tolerance without rounding the required half-increment displacement to the manufacturing grid

### Requirement: Projects SHALL use one explicit currency

Each project SHALL declare a single currency for stock purchase prices, cutting charges, and monetary estimates. UI language and export language SHALL not change that currency or the stored amounts. Amounts in another currency SHALL not be silently summed or treated as equivalent. Changing a currency on a project with prices SHALL require an explicit user decision to supply replacement amounts or confirm relabelling without conversion; the application SHALL not imply an exchange-rate conversion occurred.

#### Scenario: Language does not convert prices
- **WHEN** a BRL project is displayed in English
- **THEN** its amounts remain BRL and monetary totals use the same underlying values

#### Scenario: Conflicting currency is entered
- **WHEN** a user attempts to add a USD price to a BRL project without resolving the currency mismatch
- **THEN** the price is rejected and the existing estimate is preserved

### Requirement: Saving SHALL preserve the last successful file on failure

An explicit save SHALL either commit a complete valid project revision or leave the previously saved file intact. Failure SHALL be reported with the unsaved in-memory revision retained for retry or saving elsewhere. Success SHALL only be reported after the complete revision is committed; interrupted writes SHALL not replace the last successful project with a partial file.

#### Scenario: Save cannot complete
- **WHEN** storage exhaustion, permission failure, or interruption prevents a save from completing
- **THEN** the previous successful file remains reopenable and the edited project is not falsely marked as successfully saved

### Requirement: Loading SHALL reject unsupported formats non-destructively

Project files SHALL identify their format version. The application SHALL refuse a newer unsupported version and explain the compatibility issue without rewriting it. Invalid or incomplete files SHALL produce an actionable load error. A failed load SHALL preserve the currently open project and its unsaved edits.

#### Scenario: Newer project is opened
- **WHEN** a user opens a file declaring a newer unsupported format while another project has unsaved edits
- **THEN** the file is refused, neither project is modified, and the user is told that a compatible application version is required

#### Scenario: Damaged file is opened
- **WHEN** project content cannot be validated as a complete supported project
- **THEN** loading fails visibly without replacing the current project or overwriting the source file

### Requirement: Recovery SHALL require a user choice

Recoverable autosaves SHALL be associated with their project and revision and SHALL not silently replace an explicit save. After an interrupted session, the application SHALL offer recovery with enough project and revision information to distinguish the autosave from the saved file. Users SHALL be able to recover, discard the recovery candidate, or defer the decision. Recovering SHALL not overwrite the explicit saved file until the user saves.

#### Scenario: Autosave is newer than the saved file
- **WHEN** the application finds a valid recoverable revision after an interrupted session
- **THEN** it presents the saved and recoverable revisions for user choice rather than automatically overwriting either

#### Scenario: Recovery is declined
- **WHEN** the user chooses to discard the recovery candidate
- **THEN** the explicitly saved project remains unchanged

### Requirement: Undo and redo SHALL restore complete editing transactions

Each committed user edit SHALL be undoable as one coherent transaction, including dependent allocation, conflict, and estimate consequences. A batch edit SHALL be one transaction, not one per affected part. Undo/redo SHALL restore the corresponding model and allocation state together; failed or cancelled previews SHALL not create an edit transaction. A new committed edit after undo SHALL invalidate the superseded redo branch.

#### Scenario: Undo a batch resize that invalidated allocations
- **WHEN** a batch resize changes several boards and their allocation or conflict states and the user invokes undo once
- **THEN** all affected dimensions, placements, allocation states, and resulting estimates return together to their pre-edit state

#### Scenario: Cancelled operation is not undoable
- **WHEN** a user previews a placement or optimization and cancels it
- **THEN** the committed project and its undo history are unchanged
