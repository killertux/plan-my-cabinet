# Spec Delta

## Purpose

Hand off traceable cutting instructions to a woodworking shop while clearly distinguishing unresolved drafts from validated manufacturing layouts.

## ADDED Requirements

### Requirement: Printable cutting handoff
The system SHALL export a PDF containing project/revision identification, material and stock summary, labelled sheet layouts, grain arrows, numbered cut sequence, finished part dimensions and quantities, kerf and trim assumptions, and cut-count and monetary estimates with completeness warnings. Each part label SHALL be consistent across the assembly, layout, and list. Identical parts can be summarized by quantity but SHALL retain individual allocation identities. Hardware SHALL be listed separately from wooden cut parts.

#### Scenario: Locate a shelf in the shop packet
- **WHEN** the user exports a plan containing two dimensionally identical shelves
- **THEN** the parts summary identifies the quantity and each sheet placement retains an unambiguous individual label traceable to the design

### Requirement: Distinguish part dimensions from saw positioning
Output SHALL state finished rectangular blank dimensions independently from kerf. Cut instructions SHALL name the input piece and reference edge, identify the retained side and kerf side, and distinguish cut positions from part-size labels. Layouts SHALL state their scale and warn that printed geometry is not a cutting template. Text and diagrams SHALL remain legible through pagination rather than omitting labels to fit a page.

#### Scenario: Send a kerf-aware diagram
- **WHEN** a 100 mm part is extracted with a 5 mm kerf
- **THEN** the part is labelled 100 mm and the cut diagram separately identifies the 5 mm loss on the waste or adjacent-output side

### Requirement: Draft and shop-ready validation gates
Users SHALL explicitly choose draft or shop-ready output. Shop-ready export SHALL require all wooden parts, including hidden parts, to have compatible valid allocations with established full-span cut sequences under current settings. Invalid or unallocated parts SHALL block shop-ready export and be listed with actionable reasons. Draft export SHALL remain possible with prominent DRAFT/NOT FOR CUTTING markings and issue summaries. Price incompleteness SHALL be disclosed but SHALL NOT invalidate otherwise feasible cuts.

Wood-cut feasibility SHALL be separate from hardware installation validation. Invalid or unverified hardware guidance SHALL not block otherwise valid wood-cut export, but its numeric installation diagrams SHALL be omitted and the hardware section SHALL explicitly identify unresolved guidance. Approximate motion warnings SHALL not be used as a substitute for marking invalid drilling references.

#### Scenario: Hidden unallocated board blocks release
- **WHEN** the user requests shop-ready export while a hidden board is unallocated
- **THEN** export is blocked and the issue list identifies the board with a route to its allocation

#### Scenario: Export an unresolved draft
- **WHEN** the user explicitly chooses draft export for an invalid layout
- **THEN** every layout page is marked as not for cutting and the packet lists unresolved parts and conflicts

#### Scenario: Invalid hinge reference in an otherwise valid cut plan
- **WHEN** all wooden allocations are valid but a hinge reference lies outside the door
- **THEN** the cutting packet can be exported with that hardware issue listed and its dimensioned installation instructions omitted

### Requirement: Immutable exported revisions
Each exported plan SHALL identify the project and manufacturing revision it represents and record its cutting settings. Later manufacturing-relevant edits SHALL mark the last export as out of date without modifying the existing file. Selection, camera, visibility, and motion preview alone SHALL NOT invalidate a cutting revision. Re-export SHALL NOT silently overwrite an existing file without confirmation.

Output freshness SHALL also track every included hardware identity, quantity, label, catalog snapshot and installation reference. A change to included hardware SHALL mark the packet outdated even if wooden cutting feasibility is unchanged.

#### Scenario: Resize after sending a plan
- **WHEN** a part dimension changes after a shop-ready PDF was exported
- **THEN** the app reports that the sent revision is outdated and the original PDF remains unchanged

#### Scenario: Add a foot after export
- **WHEN** a foot is added to the hardware list after exporting a packet
- **THEN** the packet is marked outdated while existing wooden allocations and their cutting validity remain unchanged

### Requirement: Offline localized output
PDF generation SHALL work offline with embedded fonts covering English and Brazilian Portuguese. Users SHALL select the output language independently of UI language and explicitly see output units and currency. Translated labels SHALL NOT replace user-written part names or manufacturer product identifiers. Any included hinge references SHALL repeat the approximation/installation limitations and SHALL NOT imply furniture safety certification.

#### Scenario: English UI and Portuguese packet
- **WHEN** the user exports in Brazilian Portuguese from an English interface
- **THEN** headings and warnings are in Portuguese, user names and hardware codes are unchanged, and dimensions retain their physical values

### Requirement: Export failure is non-destructive
The system SHALL validate before shop-ready generation and report file-write errors without claiming success. Failure SHALL preserve the previous exported file and current project state. A successful export record SHALL be added only after the output is fully written.

#### Scenario: Destination cannot be written
- **WHEN** the output location is not writable
- **THEN** the user receives an actionable error, no successful-export revision is recorded, and an existing PDF is preserved
