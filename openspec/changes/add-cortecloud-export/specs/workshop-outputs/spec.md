# Spec Delta

## ADDED Requirements

### Requirement: The design SHALL export as a CorteCloud part list

The Handoff workspace SHALL offer CorteCloud as an export format beside the workshop PDF. The file SHALL list identical parts once with a quantity, with length along the grain, finished sizes, the board name, its cabinet, the material and thickness, the band on each side, and face holes measured from their nearest corner on the inner face. Parts without holes SHALL have no machining block, and machining sizes SHALL equal the part's sizes.

#### Scenario: Door with hinges
- **WHEN** a door with two confirmed hinges is exported
- **THEN** its part lists two 35 mm cups on the inner face with their depth, measured from the two corners of the hinge side

#### Scenario: Grain across the width
- **WHEN** a board's grain runs along its width
- **THEN** its `c` is the board width and its holes and bands turn with it

### Requirement: The CorteCloud export SHALL need only a valid design

Exporting SHALL require a structurally valid design with boards and SHALL NOT require sheets, a cut plan, a confirmed kerf or prices. Drilling SHALL be sent only for hardware without issues on doors that need no review, and screw holes SHALL be sent only with a pilot size from the catalog or given for the export. Everything left out SHALL be listed with its reason before exporting.

#### Scenario: No cut plan
- **WHEN** a project has boards but no stock
- **THEN** the CorteCloud export is available

#### Scenario: Unsized screws
- **WHEN** hinge plates have no pilot size
- **THEN** their holes are listed as left out, and giving a pilot size includes them

### Requirement: The user SHALL choose what the CorteCloud file carries

Before exporting, the user SHALL be able to leave out edge banding, hinge holes and drawer slide holes independently; with all three off the file SHALL list only the parts. Content left out by choice SHALL NOT be reported as left out, and kinds the design does not have SHALL be shown as unavailable.

#### Scenario: Only the boards
- **WHEN** the user unticks banding, hinge holes and slide holes
- **THEN** every part has no bands and no machining, and the Left out list is empty

#### Scenario: Bands without hinge holes
- **WHEN** only hinge holes are unticked
- **THEN** parts keep their bands and doors carry no cups

### Requirement: Part-list exports SHALL be written safely and remembered

Writing SHALL be atomic and SHALL replace an existing file only after confirmation. A written file SHALL be recorded with a fingerprint of the part list, without changing the project revision or undo history, and Handoff SHALL say whether the design still matches the last file.

#### Scenario: Design changed
- **WHEN** a door is resized after exporting
- **THEN** the last export is shown as outdated, and undoing the resize shows it as current again
