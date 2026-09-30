# Spec Delta

## ADDED Requirements

### Requirement: Materials SHALL say whether they take edge banding

Every material SHALL have a type: MDF, MDP, HDF, plywood, solid wood or other. Only MDF and MDP SHALL take edge banding. A file from before material types SHALL get the type from the material's name. A material that takes banding MAY name a default band.

#### Scenario: Older file
- **WHEN** a schema 4 project with "MDF Branco", "Compensado" and "HDF" opens
- **THEN** their types are MDF, plywood and HDF, and no board is banded

#### Scenario: Banding on plywood
- **WHEN** a document has a banded edge on a plywood board
- **THEN** it is refused as invalid

### Requirement: Board edges SHALL be banded automatically unless set by hand

Each board edge SHALL be automatic, banded or unbanded. An automatic edge SHALL get its material's default band when no other board sits flat against at least half of its face within 0.5 mm, and no band otherwise. Boards that move with a door or drawer SHALL only join boards moving with them. The effective banding SHALL follow geometry changes without an edit.

#### Scenario: Base cabinet
- **WHEN** a base cabinet is generated in white MDF with a default band
- **THEN** the sides' front, top and bottom edges are banded, their rear edges against the back are not, and the bottom's ends between the sides are not

#### Scenario: Drawer front
- **WHEN** a drawer front closes against the carcass
- **THEN** the carcass front edges behind it stay banded and the front is banded all round

### Requirement: Banding SHALL be editable per edge, per board and in 3D

The Design inspector SHALL show a board's edges with their bands and let the user flip an edge, return it to automatic, choose the band, and apply Automatic, None, Front or All 4, for one or several boards. The 3D view SHALL draw banded edges and offer a tool that flips the edge under the pointer. Each change SHALL be one undo step, and boards that take no banding SHALL be skipped with a message.

#### Scenario: Flip twice
- **WHEN** the user clicks an automatically banded edge twice
- **THEN** the first click removes the band as a manual override and the second returns the edge to automatic, each in one undo step

#### Scenario: Material change
- **WHEN** a banded board changes to a plywood material
- **THEN** its manual bands are removed in the same undo step and the user is told how many edges lost their band
