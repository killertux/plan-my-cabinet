## ADDED Requirements

### Requirement: Witness-backed cut-plan canvas
The Cut plan canvas SHALL provide Fit, sheet rulers and a legend, and independent Cuts, Offcuts, Grain and IDs display toggles. It SHALL show part names and finished dimensions, shared active selection, grain glyphs, hatched reusable rectangular offcuts, kerf bands scaled from the actual configured width with a visible minimum stroke, numbered cut markers and recorded conflict positions. Small parts SHALL retain readable labels through compact labels or associated callouts. Display toggles and fitting SHALL affect presentation only. Cut markers, reusable offcuts and verified status SHALL derive from the current established cutting witness; an absent or stale witness SHALL NOT be represented as verified. The redesign SHALL preserve existing full-span, full-in-stock kerf, trimming, grain, paid-pass and material-accounting mathematics.

#### Scenario: Show a verified sheet
- **WHEN** a sheet with an established current full-span sequence is viewed with all overlays enabled
- **THEN** its numbered cuts, kerf bands, grain, part identities and offcut dimensions correspond to that witness and selected parts match the shared selection

#### Scenario: Hide overlays on an invalid sheet
- **WHEN** a user hides Cuts and IDs on a sheet whose retained placements violate kerf constraints
- **THEN** the conflicts and invalid status remain discoverable and hiding overlays does not establish feasibility or change any manufacturing data

### Requirement: Linked physical sequence and truthful sheet statistics
The sheet inspector SHALL show ownership, material and thickness, grain, trim and price alongside utilization, physical-cut count, recoverable offcut area and kerf loss. Trimming loss SHALL remain discoverable and separate from kerf loss or explicitly included in a clearly labelled combined loss, preserving complete accounting. Cut-sequence rows SHALL expose cut number, input piece, reference edge, retained-edge distance, kerf side and resulting piece identities and dimensions. Hovering a row SHALL highlight the corresponding physical cut on the canvas without committing selection or edits. Verified full-span status SHALL appear only for a current established witness; rule violations and exhausted proof search SHALL remain distinct, and unavailable metrics SHALL be labelled unverified rather than zero.

#### Scenario: Follow a crosscut
- **WHEN** a user hovers the row cutting a strip produced by an earlier rip
- **THEN** the strip's matching cut is highlighted and the row identifies the already-produced input, reference edge, kerf side and outputs

#### Scenario: Proof budget ends
- **WHEN** reconstruction ends without a witness because its search budget was exhausted
- **THEN** the inspector reports feasibility unknown within the budget, does not claim impossibility, and does not display a verified physical-cut count or derived loss as fact

### Requirement: Optimization comparison in the Cut plan inspector
The Cut plan workspace SHALL expose all three existing objectives, Search/Cancel search, progress and source revision, and a current-versus-best-found comparison before acceptance. The comparison SHALL show sheets used, physical cuts, offcut rectangles and recoverable area, least-unused-stock area, irreversible loss, cost completeness, and per-part old/new sheet, location and orientation. It SHALL retain lock preservation, heuristic and budget-limit disclosures, no-complete-result and unchanged-current outcomes, and prohibit lowest-spending claims for incomplete costs. Relevant manufacturing changes SHALL visibly stale results and disable acceptance until a new search; uncommitted repair or editing previews SHALL prevent conflicting acceptance. Cancel SHALL preserve the current plan, and acceptance SHALL be one undoable action.

#### Scenario: Compare an unverified current plan
- **WHEN** a search finds a complete candidate while the current placements lack a witness
- **THEN** current allocations remain visible, witness-dependent current metrics are marked unverified, candidate metrics are shown, and no placement changes until explicit acceptance

#### Scenario: Result becomes stale
- **WHEN** kerf or another relevant manufacturing input changes after search completion
- **THEN** the comparison shows stale — search again and cannot accept the old candidate

#### Scenario: Cancel a running search
- **WHEN** the user selects Cancel search while optimization is running
- **THEN** the current allocations remain unchanged and no late result is silently accepted

### Requirement: Kerf confirmation provenance and cutting settings
Cutting settings and workspace status SHALL expose kerf value and confirmation state, a confirmation explanation and explicit confirmation control, the supported cutting-model limitations, and access to worked examples. A new shop confirmation SHALL record and display its actual confirmation date associated with the confirmed kerf. Changing kerf SHALL clear current confirmation and its applicable date and revalidate placements without silently resizing parts. Undo/redo SHALL restore the corresponding confirmation state and provenance. Legacy confirmed projects without recorded dates SHALL show confirmation with date unavailable rather than inventing a date.

#### Scenario: Change a confirmed kerf
- **WHEN** a user changes a dated confirmed 5 mm kerf to 6 mm
- **THEN** the confirmation becomes unconfirmed, its previous date is not shown as confirmation of 6 mm, and affected allocations are revalidated

#### Scenario: Load legacy confirmation
- **WHEN** a project records a confirmed kerf but no confirmation timestamp
- **THEN** the UI preserves the known confirmation state and identifies the date as unavailable until a new explicit confirmation is made
