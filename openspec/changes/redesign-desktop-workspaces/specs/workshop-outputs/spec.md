## MODIFIED Requirements

### Requirement: Printable cutting handoff
The system SHALL export a PDF containing mandatory project/revision identification, packet mode, output units and language, applicable cutting assumptions, safety markings, issue summaries and completeness/limitation disclosures. Users SHALL control inclusion of Parts list & costs, Sheet diagrams + cut steps, and Hinge references; all three SHALL initially be included. With these sections included, the packet SHALL contain material and stock summary, labelled sheet layouts, grain arrows, numbered cut sequence, finished part dimensions and quantities, kerf and trim assumptions, and cut-count and monetary estimates with completeness warnings. Each part label SHALL be consistent across the assembly, layout, and list. Identical parts can be summarized by quantity but SHALL retain individual allocation identities. Hardware SHALL be listed separately from wooden cut parts.

Section controls SHALL omit only their optional detail, never change draft/shop-ready eligibility, hide unresolved wood or hardware issues, remove applicable warnings, or authorize invalid numeric installation guidance. A concise mandatory summary SHALL identify omitted sections and retain material/stock context, unresolved hardware guidance, and incomplete-estimate disclosures even when corresponding detail is omitted. Sheet diagrams and their cut steps SHALL be included or excluded together so diagrams are not detached from their instructions. Disabling all optional sections SHALL still produce the mandatory identification, scope, assumptions, issues and warnings.

#### Scenario: Locate a shelf in the shop packet
- **WHEN** the user exports a plan containing two dimensionally identical shelves
- **THEN** the parts summary identifies the quantity and each sheet placement retains an unambiguous individual label traceable to the design

#### Scenario: Exclude optional details without hiding safety content
- **WHEN** a user disables Parts list & costs and Hinge references for a draft with an unallocated board, unknown prices and invalid hinge guidance
- **THEN** those detailed sections are omitted but draft markings, the board issue, incomplete-estimate disclosure and unresolved hardware warning remain in the mandatory summary and shop-ready remains blocked by the unallocated board

#### Scenario: Include sheet instructions as a unit
- **WHEN** the user enables Sheet diagrams + cut steps
- **THEN** labelled layouts and their associated physical cutting instructions appear together with kerf, trim, scale and not-a-cutting-template warnings

## ADDED Requirements

### Requirement: Handoff workspace readiness and output controls
The Handoff workspace SHALL provide explicit Draft and Shop-ready packet cards, a current readiness checklist with actionable reasons and Fix routes to the relevant Cut plan issue or cutting settings, output language and unit controls, section controls, a mode-specific Export PDF action, preview and export history. Shop-ready SHALL remain unavailable until current wood-cut validation and shop kerf confirmation pass; hiding sections SHALL NOT change these checks. Hardware validation SHALL remain separate and price incompleteness SHALL remain disclosed without blocking otherwise valid wood cutting. Output language SHALL be independent of UI language and output units SHALL retain mm, cm, m, in and ft, including choices not pictured in the compact reference control. Navigation through Fix SHALL respect dirty-draft, repair and pose-edit resolution, including Stay.

#### Scenario: Fix a hidden unallocated board
- **WHEN** the user activates Fix beside a hidden unallocated board in the readiness checklist
- **THEN** navigation targets that board's allocation issue after any required edit-resolution decision and shop-ready remains unavailable until current validation succeeds

#### Scenario: Export in a less-prominent unit
- **WHEN** a user selects metres or feet and Portuguese output from an English UI
- **THEN** both preview and export use the chosen units and language without changing physical dimensions or interface language

### Requirement: Shared paginated preview and export layout
Document preview and PDF export SHALL use the same prepared document content, page layout and pagination for a given project snapshot, packet mode, language, units and section selection. The preview SHALL provide A4 page presentation, accurate page count, selectable page thumbnails, current-page navigation and zoom controls; zoom SHALL NOT repaginate the document. Text, diagrams, labels, tables, callouts, repeated context, warnings and draft stamps SHALL occupy corresponding pages in preview and exported PDF. Changes to relevant content or output controls SHALL refresh preview and pagination before a subsequent export; an export SHALL correspond to the displayed prepared snapshot or require the user to review a refreshed preview. Previewing SHALL NOT write a PDF receipt or mark a project as sent.

#### Scenario: Paginate a dense packet
- **WHEN** a packet with many parts, cuts and Portuguese labels spans multiple pages
- **THEN** preview thumbnails and page count match the PDF, all labels and warnings remain legible on corresponding pages, and zooming changes only preview magnification

#### Scenario: Change sections before export
- **WHEN** the user disables Hinge references or changes units in the Handoff workspace
- **THEN** the preview regenerates the shared layout and page count and the next export uses those same settings and mandatory safety content

### Requirement: Truthful historical export receipts
Successful export receipts SHALL preserve filename/path, project and manufacturing revision, actual completion time, output language and units, packet mode, included sections, recorded hashes and enough historical comparison evidence to explain subsequent relevant changes. History cards SHALL display those recorded facts, current/out-of-date state, supported Since then summaries, and older-receipt supersession distinctly from freshness. A historical receipt SHALL never establish current shop-ready eligibility. Receipts SHALL be added only after a complete successful write; preview, cancellation, overwrite refusal or failure SHALL NOT add one. The workspace SHALL retain overwrite confirmation, actionable write errors, before-you-send guidance and the reminder to save the project to persist receipts.

Legacy receipts SHALL preserve recorded values and display missing packet mode as unknown, missing dates as unavailable, and unavailable comparison details explicitly. The system SHALL NOT infer an old mode from current validity, substitute load/save time for an absent export date, or fabricate a detailed diff from hashes alone. Missing inclusion metadata SHALL NOT be presented as a known historical section choice.

#### Scenario: Reopen an older receipt
- **WHEN** a legacy receipt has a filename, hashes, units and actual completion time but no packet mode or comparison evidence
- **THEN** the card shows its recorded time and settings, mode unknown and comparison details unavailable, without inferring shop-ready status or inventing changed-part names

#### Scenario: Export another revision
- **WHEN** a newly prepared packet is successfully written to a new filename
- **THEN** a receipt records the actual completed export's settings and hashes, earlier files and receipts remain immutable, older cards show supersession separately from freshness, and the user is reminded to save receipt metadata

### Requirement: Manufacturing-neutral presentation and independent output freshness
The Handoff workspace SHALL distinguish unsaved project state, current wood-cut readiness, included-hardware guidance freshness and historical packet freshness. Material display-color edits and display-only changes SHALL NOT stale manufacturing output even when persisted presentation edits make the project unsaved. Relevant manufacturing or included-hardware edits SHALL continue to mark dependent output outdated without modifying exported files. Since then summaries SHALL be based on recorded historical evidence and identify unavailable detail honestly; changes to prospective output language, units or section controls SHALL NOT rewrite historical receipt settings or imply that the old file was regenerated.

#### Scenario: Change only material color
- **WHEN** the user changes a material display color after exporting a current packet
- **THEN** project dirty state reflects the unsaved presentation edit while wood-cut readiness and manufacturing-output freshness remain unchanged

#### Scenario: Hardware-only edit after export
- **WHEN** an included hardware identity, quantity or installation reference changes after export
- **THEN** affected packet guidance is marked outdated separately from unchanged wooden cutting validity, and any displayed change summary is supported by recorded evidence
