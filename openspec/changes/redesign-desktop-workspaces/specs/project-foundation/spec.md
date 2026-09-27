# Spec Delta

## ADDED Requirements

### Requirement: Welcome SHALL manage recent projects without replacing work silently

The application SHALL show Welcome at launch or when no project is open, with New, Open, Base/Wall/Drawers templates, language, preferences, and a filterable recent-project list. Recent entries SHALL show available project name, path, counts, last-use information, and qualified recovery/export status from actual metadata. Missing or unavailable metadata SHALL not be fabricated. Entries SHALL be registered only after successful open/save operations. Missing files SHALL offer Locate and Remove; locating SHALL validate the candidate before replacing the entry, and removing SHALL remove only the recent-list entry, not the project, its recovery, or exports. Returning to Welcome or choosing a recent project SHALL retain existing unsaved-work and prepared-load protections.

#### Scenario: Locate a missing recent project
- **WHEN** the user chooses a replacement file and that file fails validation
- **THEN** the recent entry and currently open project remain unchanged and an actionable error is shown

#### Scenario: Remove a recent entry
- **WHEN** the user removes a missing project from the list
- **THEN** only its recent-list entry is removed and no project or recovery file is deleted

### Requirement: Project thumbnails SHALL be optional local derived data

After successful explicit save, the application SHALL generate a small viewport thumbnail for that saved state and associate it with the project's local recent entry. A missing, failed, or not-yet-generated thumbnail SHALL use a placeholder. Thumbnail generation SHALL not alter project geometry, the saved camera/selection, or save success; its failure SHALL not undo a successful save. Thumbnails and recent paths SHALL not be required to open a portable project on another machine.

#### Scenario: Save succeeds but thumbnail capture fails
- **WHEN** the project is saved successfully but no thumbnail can be generated
- **THEN** the project remains successfully saved and Welcome uses a placeholder

### Requirement: Recovery discovery and cleanup SHALL preserve explicit user control

Welcome SHALL surface validated recoverable work with project/path identity and saved-versus-recovery revision information, offering Recover edits, Decide later, and Discard snapshot. Discovery SHALL preserve the existing project-identity and canonical-path association; copied or relocated files SHALL not silently adopt another path's recovery. Recovering SHALL leave work unsaved until explicit save; deciding later SHALL leave the candidate intact. Settings SHALL offer opening the recovery folder and a cleanup review listing candidate identity, status, and available timestamps. Cleanup SHALL have no preselected candidates and SHALL delete only explicitly selected snapshots after confirmation, never saved project files. Invalid records SHALL remain available for diagnosis unless explicitly selected for deletion. No automatic age-based retention deletion SHALL be introduced.

#### Scenario: Defer from Welcome
- **WHEN** the user chooses Decide later for a newer valid snapshot
- **THEN** both the saved file and snapshot remain intact and the candidate can be offered again

#### Scenario: Cancel cleanup
- **WHEN** snapshots have been selected for cleanup and the confirmation is cancelled
- **THEN** no snapshot or project file is removed

#### Scenario: Find a copied project
- **WHEN** a recent entry points to a copy at a different canonical path
- **THEN** discovery does not attach the original path's recovery merely because project identifiers match

#### Scenario: Recover an untitled project
- **WHEN** a valid recovery candidate belongs to a project that has never been explicitly saved
- **THEN** Welcome identifies it as unsaved work without inventing a saved path or saved revision, and recovery requires an explicit choice followed by Save As to create a project file

### Requirement: Templates SHALL generate independent editable assemblies once

Base, Wall, and Drawers SHALL accept explicit overall dimensions and material choices and preview their construction assumptions, clearances, generated boards, and effective thicknesses before creation. Base and Wall SHALL generate their depicted carcass arrangements as ordinary boards. Drawers SHALL generate the carcass plus drawer boxes and fronts, not merely empty drawer bays; its inputs SHALL expose drawer count, box depth, and relevant side, rear, vertical, and front clearances. Differently thick components SHALL use explicitly chosen materials. Invalid or nonpositive derived dimensions SHALL prevent creation atomically. Confirmation SHALL create one undoable assembly transaction with fresh identities and normal first-fit allocation attempts; insufficient stock SHALL leave generated boards visibly unallocated rather than creating purchased stock. Subsequent board edits SHALL not propagate through a template constraint system. No automatic slide selection, machining instructions, load suitability, or validated mechanical fit SHALL be claimed.

#### Scenario: Generate complete drawers
- **WHEN** the user confirms a valid Drawers preview for three drawers
- **THEN** the assembly contains the carcass and three sets of drawer-box parts and fronts, all independently editable and individually represented in allocation and part accounting

#### Scenario: Clearance consumes the box width
- **WHEN** the chosen clearances and material thicknesses leave a nonpositive drawer dimension
- **THEN** generation is blocked with the affected input identified and no partial assembly is created

#### Scenario: Edit and undo a generated assembly
- **WHEN** a user generates a template and then changes one board
- **THEN** no other board is parametrically resized, undo reverses that edit, and a further undo removes the complete generation transaction

### Requirement: Welcome templates SHALL bootstrap projects and materials safely

Launching a Welcome template SHALL start a staged new-project setup usable without any existing project or material library. It SHALL expose project name, currency and input units and creation of draft materials with name, valid thickness, grain and optional display color, with explicit assignments to each component role. Nested material creation SHALL retain the template draft. Setup SHALL not mutate any existing document or add a recent-project entry. Generate SHALL validate the complete candidate and resolve existing unfinished edits and unsaved-document replacement before opening the new project. Successful generation SHALL commit the staged materials and assembly together as one undoable transaction in that new project, open Design with the generated assembly selected, and leave it unsaved. Without declared stock, generated parts SHALL be visibly unallocated and the user SHALL have a route to Stock. Cancelling setup SHALL discard only staged setup data and leave any existing document unchanged. Cancellation or failure of a replacement Save SHALL retain both the existing document and the setup for retry.

#### Scenario: Generate Drawers from first-run Welcome
- **WHEN** no project or materials exist and the user launches Drawers, defines the required materials and confirms a valid setup
- **THEN** a new unsaved project opens with its materials, carcass, drawer boxes and fronts, unallocated boards are identified honestly, and a single undo removes the generated assembly and its newly created materials

#### Scenario: Cancel nested material setup
- **WHEN** the user cancels material creation inside template setup and later cancels the setup itself
- **THEN** material cancellation restores the intact template draft and setup cancellation returns to Welcome without creating a project, materials, recent entry or edits to a previously open document

#### Scenario: Replacement save fails
- **WHEN** a valid template setup would replace an unsaved document and saving that document fails or its picker is cancelled
- **THEN** the existing document is not replaced, the staged template setup remains available, and no generated project is reported as created or saved

### Requirement: Redesign metadata SHALL preserve legacy projects and provenance

The application SHALL load existing schema-version-1 projects through a validated compatibility path while preserving identities, exact dimensions, poses, allocations, pricing, hardware snapshots, and historical receipts. New persisted metadata SHALL have explicit defaults or unknown states: absent material colors use a neutral appearance, absent confirmation dates and historical packet modes remain unknown, and absent receipt baselines SHALL not yield invented detailed diffs. Opening or migrating in memory SHALL not rewrite the source file or alone mark it edited. Explicit save SHALL use the supported current format and retain atomic-write and overwrite protections. Unsupported newer versions SHALL still fail non-destructively. Machine-local preferences, recent paths, thumbnails, camera/tool state, and workspace drafts SHALL not become required portable project data.

#### Scenario: Open a legacy project
- **WHEN** a valid version-1 project without redesign metadata is opened
- **THEN** its manufacturing data is preserved, missing presentation/provenance fields have honest defaults, and the source file remains untouched until explicit save

#### Scenario: Open a future project version
- **WHEN** the file declares a newer unsupported schema version
- **THEN** loading is rejected without replacing the current project or rewriting either file
