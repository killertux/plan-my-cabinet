## ADDED Requirements

### Requirement: Hardware workspace tree and catalog access
The Hardware workspace SHALL provide a pinned catalog card with exact kit and plate identifiers, supported thickness and applicable parameters, snapshot revision and Browse access to catalog snapshots and source review. It SHALL show door relationships with stationary-board context and child hinges with positions and warning indicators, including access to unassigned installations and reference hardware rather than hiding them. Selecting an installation SHALL link the tree, viewport annotation and inspector by identity. Add hinge, Add door, edit and removal actions SHALL remain available, together with dimensioned reference-hardware creation, positioning, duplication and assembly nesting. Catalog refresh SHALL remain explicit and revalidated, and missing source data SHALL remain labelled.

#### Scenario: Select a warned hinge
- **WHEN** the user selects a hinge with unsupported thickness in a door's tree
- **THEN** the inspector and annotation identify that installation and expose its specific warning and source context

#### Scenario: Inspect standalone hardware offline
- **WHEN** an offline project has an unassigned hinge and reference feet outside any door relationship
- **THEN** both remain accessible for their supported editing actions and Browse shows the saved catalog snapshots without silently refreshing them

### Requirement: Complete mounting inspector and derived references
The installation inspector SHALL expose the door board and reference edge and face, cabinet board and front edge and face, independent door and mounting-board Y positions, catalog pairing, setback K and overlay R. Quick preset pairs SHALL reflect the selected documented configuration and SHALL NOT replace access to independent supported inputs or assume every model uses the pictured values. The reference card SHALL derive cup dimensions and position, plate spacing and positioning references, and thickness validity from the selected snapshot and installation. It SHALL expose source attribution, page/revision and Source review, and mark unavailable fastener drilling and unsupported configurations explicitly. Invalid numeric drafts SHALL show actionable validation without mutating the committed installation.

#### Scenario: Independent mounting positions
- **WHEN** an installation requires different door and cabinet Y positions and opposite supported mounting faces
- **THEN** the inspector permits those explicit values, previews the derived references and validation, and commits them only through a valid confirmed edit

#### Scenario: Unsupported preset or missing evidence
- **WHEN** the selected configuration lacks evidence for a displayed mounting value
- **THEN** the inspector does not apply that value as a verified preset or fabricate drilling references and identifies the unavailable evidence

### Requirement: Derived hardware viewport annotations
The Hardware viewport SHALL show the selected relationship's derived hinge axis as a dashed line, cup and plate reference markers and their connectors, installation labels and active-selection emphasis, with Iso, Front and Top camera controls. Annotations SHALL follow board-local references and current display pose, remain distinct from solid machining geometry, and communicate invalid or unavailable references rather than implying validation. Diagram measurements SHALL use verified installation data and preserve the approximation and installation limitations.

#### Scenario: Display a valid installation
- **WHEN** the user inspects a verified installation in Front view
- **THEN** cup and plate markers and the labelled hinge axis match its derived board-local references and selecting the installation highlights its own markers

#### Scenario: Board edit invalidates an annotation
- **WHEN** a board edit moves a reference outside the board
- **THEN** the annotation and inspector flag the affected installation rather than silently relocating the reference or showing a validated drilling diagram

### Requirement: Workspace-scoped door display preview
Motion preview and Closed controls SHALL operate on the selected valid door relationship, with a live angle readout and slider bounded by its configured opening limit, appropriate tick labels, and persistent display-only and approximate-motion disclosures. All members of the moving board or assembly SHALL follow the coherent relationship while stationary members remain fixed. Choosing Closed or completing navigation out of Hardware SHALL reset display motion to the closed design pose; returning SHALL NOT silently resume an open preview. Display motion SHALL NOT save pose edits, alter allocations or dimensions, create undo entries, or stale manufacturing output. Invalid relationships SHALL explain why motion is unavailable.

#### Scenario: Leave an open door preview
- **WHEN** a user previews 60 degrees and completes navigation from Hardware to Design
- **THEN** the displayed door returns closed and the saved pose, undo history, allocations and manufacturing freshness remain unchanged

#### Scenario: Configured limit differs from the reference screen
- **WHEN** the selected relationship has a supported opening limit other than 105 degrees
- **THEN** the slider and tick endpoint use that actual limit and the approximate-motion disclosure remains visible throughout scrubbing

### Requirement: Preserve door relationship transactions during navigation
The Hardware workspace SHALL retain explicit moving-root and stationary-board selection, hinge membership, moving-member and derived-axis previews, relationship confirmation, cycle/self-reference rejection, and explicit detach/removal behavior. Deletion of referenced objects SHALL show affected relationships before the existing atomic undoable action. Leaving during dirty installation drafts, relationship drafts or pose-edit previews SHALL require explicit valid commit, discard/cancel, or Stay; starting display motion SHALL NOT silently discard those edits. Stay SHALL retain the draft and workspace, and invalid confirmation SHALL NOT complete navigation.

#### Scenario: Retain an unfinished relationship
- **WHEN** the user attempts workspace navigation while a door relationship draft is incomplete and chooses Stay
- **THEN** the draft's moving root, mounting board and hinge choices remain intact and no relationship is created or discarded

#### Scenario: Remove a referenced object
- **WHEN** the user confirms deletion of a board referenced by a door relationship
- **THEN** affected relationships are identified and removed or explicitly detached in the same undoable action without dangling references

### Requirement: Outward handedness with explicit legacy review
Positive door-preview angles SHALL move the free edge away from the selected cup face, with direction derived from both the hinge edge and cup face rather than one global rotation sign. The rule SHALL work for mirrored doors and rotated or nested moving roots without changing closed poses or stationary members. Existing persisted axes SHALL remain unchanged on load. A stored relationship differing from the corrected proposal SHALL be marked for review and SHALL NOT start motion until explicit reconfirmation through the existing undoable relationship transaction. Unchanged valid axes SHALL remain usable. This correction SHALL NOT imply verified concealed-hinge trajectories, clearance or collision safety.

#### Scenario: Mirrored edges and faces open outward
- **WHEN** otherwise valid relationships use either X hinge edge and either Z cup face and preview a positive angle
- **THEN** each free edge moves away from its selected cup face, all moving members follow coherently, and a zero angle retains the exact closed poses

#### Scenario: A legacy axis requires reconfirmation
- **WHEN** an existing project is opened with a stored axis opposite to the corrected edge/face-derived direction
- **THEN** source bytes and the stored relationship remain unchanged, motion is blocked with a review warning, explicit reconfirmation updates the relationship as one edit, and undo restores the prior axis and review requirement
