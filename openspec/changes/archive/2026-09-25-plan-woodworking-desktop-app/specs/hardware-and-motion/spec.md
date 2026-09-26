# Spec Delta

## Purpose

Represent hardware independently of wooden stock and provide documented hinge installation references with honest limits on simulated movement.

## ADDED Requirements

### Requirement: Hardware placeholders
The system SHALL support named dimensioned feet and other reference hardware that can be positioned, duplicated, and nested in assemblies. Hardware SHALL appear in a separate hardware list, never consume sheet stock, and contribute to measured overall assembly bounds when selected for measurement.

#### Scenario: Add feet below a cabinet body
- **WHEN** feet are placed below a 2300 mm-high body
- **THEN** measuring the body still returns 2300 mm while measuring the complete assembly includes the feet, without adding wooden cut-list parts

### Requirement: Curated model-specific catalog
The first version SHALL include at least one real hinge-and-compatible-plate configuration with exact product identifiers, source reference/revision, supported door thickness, installation parameters, opening limit, and explicit motion fidelity. Unsupported variants SHALL NOT inherit another model's claimed dimensions. Catalog entries used in a project SHALL be snapshotted so offline reopening and external catalog changes do not alter the design. Updating a project's entry SHALL be explicit and revalidated.

To count as the delivered baseline, a configuration SHALL have visually reviewed manufacturer evidence for exact hinge/plate pairing, applicable overlay/inset parameters, supported thickness, cup diameter/depth and edge-position reference, mounting-plate positioning references, and opening limit, each traceable to source page/revision. Optional fastener details without verified evidence SHALL be marked unavailable; missing mandatory positioning information SHALL prevent an entry from counting as the supported baseline. Motion fidelity SHALL remain independent of installation-data verification.

#### Scenario: Reopen without catalog connectivity
- **WHEN** a saved project is reopened offline after the external catalog changes
- **THEN** it uses the recorded hinge/plate data and sources without downloading or silently substituting a model

#### Scenario: Incomplete candidate is not a shipped baseline
- **WHEN** a candidate has a product code and opening angle but lacks verified cup or plate positioning references
- **THEN** it remains an incomplete research entry and cannot satisfy the initial supported-configuration requirement

### Requirement: Installation configuration checks
Users SHALL explicitly choose a door board, mounting board, compatible hinge/plate configuration, mounting side, and hinge locations. The system SHALL display manufacturer-based cup and mounting references and allowed parameters, and SHALL flag unsupported thicknesses, missing source data, and mounting references outside the boards. It SHALL NOT fabricate drilling coordinates or infer adequate hinge quantity from visual placement alone. Hole references SHALL be annotations, not solid machining operations in the first version.

#### Scenario: Unsupported door thickness
- **WHEN** a door thickness lies outside the selected hinge's documented range
- **THEN** the installation is flagged unsupported with the documented range and is not represented as validated

#### Scenario: Missing drilling detail
- **WHEN** a catalog entry lacks a verified fastener location
- **THEN** that location is labelled unavailable and omitted from dimensioned installation guidance rather than guessed

### Requirement: Explicit door relationship
A door motion relationship SHALL connect a moving board or nested assembly to a stationary mounting board, separately from ordinary assembly hierarchy. Users SHALL preview and confirm it; attachment SHALL NOT silently resize or reposition unrelated boards. All hinges assigned to one door SHALL use one coherent door motion, not independent competing transforms. Cycles and moving/stationary self-reference SHALL be rejected.

#### Scenario: Door handle follows the door
- **WHEN** the user previews opening a door assembly containing a door board and handle
- **THEN** they move together relative to the mounting board while the cabinet body stays fixed

### Requirement: Approximate motion disclosure
The system SHALL allow an angle-controlled preview bounded by the configured opening limit and clearly label the first-version model as approximate. It SHALL NOT claim accurate concealed-hinge trajectories, guaranteed clearance, door-load suitability, or collision-free construction. Exiting the preview SHALL restore the closed design pose; previewing SHALL NOT change cutting allocations or manufacturing dimensions.

#### Scenario: Inspect a nominal 105 degree opening
- **WHEN** the user scrubs the selected configuration through its allowed opening range
- **THEN** the approximate motion warning remains visible and the user cannot mistake the preview for verified clearance

### Requirement: Hardware changes invalidate dependent guidance
Changes to door dimensions, mounting parts, hinge configuration, or catalog parameters SHALL revalidate installation references and flag affected relationships for review. Deleting a referenced part SHALL identify and remove or explicitly detach affected relationships within the same undoable action, never leave dangling references.

#### Scenario: Door resize moves a reference outside the board
- **WHEN** resizing a door leaves a hinge annotation outside its boundary
- **THEN** that installation is flagged for correction without silently moving the hinge or blocking the board design edit
