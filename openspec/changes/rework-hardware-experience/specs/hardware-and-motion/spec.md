# Spec Delta

## ADDED Requirements

### Requirement: The Hardware panel SHALL group fittings by kind

The Hardware workspace SHALL show an Add hardware menu followed by five always-visible sections: Doors & hinges, Drawers & slides, Feet & legs, Other hardware, and Catalog models. Each section SHALL have its own add control and an empty state that says what to select first. Catalog models SHALL list every pinned model of any kind with its kind, code and usage. Selecting any row SHALL open that item in the inspector through the unsaved-draft guard.

#### Scenario: Empty project
- **WHEN** a project has no hardware
- **THEN** all five sections are shown with their empty states, and none of them describes hinges except Doors & hinges

#### Scenario: Pinned slide model
- **WHEN** a chest uses one slide model on two drawers
- **THEN** Catalog models lists it as a slide used twice and never shows it as a hinge card

### Requirement: Adding hardware SHALL create the item and open it for editing

Each add action SHALL create its item in one undo step from the selection and defaults, and SHALL open it in the inspector. A foot goes under the selected cabinet using the last foot model. Other hardware is a 100 mm box. Slides go on the selected drawer using the last slide family. A hinge joins the selected door. A door hangs the selected board with its loose hinges or a standard set. A picker SHALL open only when a required choice is missing, and adding slides to a drawer that already has them SHALL open the existing pair.

#### Scenario: Foot under a chest
- **WHEN** the user selects a chest board and adds a foot
- **THEN** one undo step creates the foot under the chest's top assembly, and its inspector opens and offers to raise the chest by the foot's height

#### Scenario: Drawer without a selection
- **WHEN** the user adds slides with nothing selected
- **THEN** the slide picker opens and the project is unchanged

### Requirement: Every hardware item SHALL be editable in the inspector in both workspaces

Feet, other hardware, slides, hinges, doors and catalog models SHALL have editable inspectors in both the Hardware and Design workspaces. Text fields SHALL commit only on Enter or Apply, revert on Escape, never commit on focus loss, and prompt before navigation discards them. Choices SHALL commit immediately in one undo step. A catalog model SHALL be removable only while nothing uses it.

#### Scenario: Move a foot by typing
- **WHEN** the user types a new X for a foot and moves focus elsewhere
- **THEN** the project is unchanged until Apply or Enter, which commits one undo step

#### Scenario: Remove a model in use
- **WHEN** a slide model is used by a drawer
- **THEN** Remove is unavailable with a reason, and it becomes available once nothing uses the model

### Requirement: Hardware SHALL be pickable and movable in the viewport

Clicking a slide or hinge in the 3D view SHALL inspect it without leaving the current workspace. Hinges with resolved references SHALL be drawn as a representational cup that moves with the door and a plate on the mount, tinted when the hinge has issues, and never presented as drilling geometry. Move SHALL drag feet and other hardware in one undo step; in the Hardware workspace it SHALL NOT drag boards, and it SHALL be unavailable while a position draft is unsaved.

#### Scenario: Click a hinge
- **WHEN** the user clicks the cup of a hinge on the inside of a door in Design
- **THEN** the hinge inspector opens and Design stays active

#### Scenario: Drag in Hardware
- **WHEN** a board is selected in the Hardware workspace and the user drags it with Move
- **THEN** the board does not move, while dragging a selected foot moves it in one undo step
