# Spec Delta

## ADDED Requirements

### Requirement: Sheet materials SHALL say which faces are coated

Materials of type MDF, MDP or HDF SHALL be coated on no face, one face or both faces; other types SHALL have no coating. Coating SHALL be part of the material, so boards of different coatings are never cut from the same sheet. A file from before coatings SHALL get the coating from the material's name.

#### Scenario: Older file
- **WHEN** a schema 5 project with "MDF Branco 1 face", "MDF Cru" and "MDP Branco" opens
- **THEN** they are coated on one side, none and both sides

#### Scenario: Plywood
- **WHEN** a plywood material carries a coating value
- **THEN** its boards are treated as uncoated

### Requirement: One-side boards SHALL be coated on the face that shows unless chosen by hand

On a material coated on one side, each board's coated face SHALL be automatic or chosen by hand. Automatically, a board standing across the cabinet SHALL be coated toward the front, a lying board on its upper face, and a board standing along the cabinet on the face away from the cabinet's middle. Flipping the face and returning to automatic SHALL each be one undo step; boards not coated on one side SHALL be skipped with a message.

#### Scenario: Base cabinet in one-side MDF
- **WHEN** a cabinet's material is coated on one side
- **THEN** its doors and back are coated on the front, its bottom and shelf on top, and its sides outside

#### Scenario: Flip
- **WHEN** the user flips a door's coated face and then returns it to automatic
- **THEN** the first step coats the inside face by hand and the second restores the front, each one undo step
