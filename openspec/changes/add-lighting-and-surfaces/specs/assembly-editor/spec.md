# Spec Delta

## ADDED Requirements

### Requirement: The 3D view SHALL light faces by the way they face

Faces SHALL be shaded from their direction in the room by one directional light and a soft sky term, never by which side of the board they are, in the 3D view and in pictures alike. Identical parts SHALL look identical however they were modelled.

#### Scenario: Upside-down twin
- **WHEN** two identical boards fill the same kind of box, one modelled upside down
- **THEN** their visible faces render in the same colours

### Requirement: Board faces SHALL show coating, core or band

Coated faces SHALL show the material colour. Uncoated faces and unbanded edges SHALL show the sheet's core with a texture for its type (MDF fibre, MDP chips, HDF fibre, plywood plies, wood grain). Banded edges SHALL show the band colour. Without material tint every face SHALL be neutral.

#### Scenario: Door in one-side MDF without a band
- **WHEN** a door of one-side MDF has no banding
- **THEN** its front shows the material colour, its back and edges the MDF core
