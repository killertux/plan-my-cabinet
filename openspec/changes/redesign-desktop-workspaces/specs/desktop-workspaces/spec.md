# Spec Delta

## Purpose

Provide a coherent native workspace interface that faithfully presents the design handoff while preserving shared project identity, safe editing, and discoverable access to all supported actions.

## ADDED Requirements

### Requirement: Five workspaces SHALL share a truthful project shell

The application SHALL provide Design, Stock, Cut plan, Hardware, and Handoff workspaces through one consistent rail and platform workspace shortcuts. The shared header SHALL expose project navigation, actual unsaved state, command search, available undo/redo, Save, and Export. Export SHALL route to Handoff output preparation rather than bypass its output choices or validation. The status bar SHALL derive kerf confirmation, workspace facts, issues, and spending completeness from the current project. Rail issue indicators SHALL lead to the corresponding issues. Workspace navigation SHALL NOT itself edit the project.

#### Scenario: Navigate to allocation problems
- **WHEN** unallocated boards cause an issue indicator on Cut plan and the user activates it
- **THEN** Cut plan exposes the actual affected boards and actionable diagnostics without changing allocations

#### Scenario: Display an incomplete estimate
- **WHEN** the cut fee or a required purchase price is unknown
- **THEN** the shell labels the estimate incomplete and does not display an unqualified complete spending total

### Requirement: Workspace navigation SHALL preserve identified context

Switching workspaces SHALL preserve the shared scene selection and each workspace's view state, including scroll, filter, camera, and panel state for the current project session. Board, sheet, material, and installation destinations SHALL identify their actual target rather than reuse an unrelated selection. Selecting a sheet SHALL not implicitly select all its boards. A selected board's sheet preview SHALL open Cut plan focused on its allocation; an unallocated board SHALL instead reveal its issue. Handoff Fix actions and palette results SHALL use these same navigation semantics. Hidden objects SHALL remain discoverable without silently changing their visibility.

#### Scenario: Follow a selected shelf to its sheet
- **WHEN** the user opens the inspector's sheet preview for an allocated shelf
- **THEN** Cut plan opens the correct stock piece and highlights the same shelf identity while its furniture pose remains unchanged

#### Scenario: Return to a workspace
- **WHEN** the user leaves Stock and returns without replacing the project
- **THEN** its material filter and scroll position are restored and current project data is displayed

### Requirement: Navigation SHALL explicitly resolve unfinished edits

Before leaving an unfinished inspector/HUD edit, changing its target, or starting an incompatible action, the application SHALL offer Apply, Discard, or Stay. Apply SHALL be unavailable for invalid or unconfirmed rounded input. Active sheet repair and pose-edit sessions SHALL similarly require Accept, Cancel, or Stay, with acceptance subject to existing validation. Cancelling a preview SHALL restore its pre-operation state; staying SHALL retain the draft and location. Navigation SHALL never implicitly commit or discard these edits. Merely collapsing a panel SHALL preserve its draft. Leaving Hardware SHALL reset display-only motion preview to the saved closed pose without a manufacturing edit. Active workers SHALL remain revision-aware and SHALL never be implicitly accepted by navigation.

#### Scenario: Switch with an invalid dimension draft
- **WHEN** a user requests another workspace while the inspector contains an invalid dimension
- **THEN** Apply is disabled and the user can discard the draft and navigate or stay with the draft intact

#### Scenario: Leave an incomplete repair
- **WHEN** a sheet repair still contains conflicts and the user requests Design
- **THEN** acceptance remains unavailable and navigation requires explicit cancellation or staying in the repair

#### Scenario: Collapse an editing inspector
- **WHEN** a window resize collapses the inspector containing a draft
- **THEN** reopening the inspector restores the draft and its validation without committing or discarding it

### Requirement: Command search SHALL invoke the same contextual actions as visible controls

The command palette SHALL search supported actions and named or identified boards, materials, and stock pieces in grouped results. Platform command-K SHALL open it; arrow keys SHALL change the active result, Enter SHALL activate it, and Escape SHALL close it and restore focus. Actions SHALL share availability, validation, modal isolation, and edit-resolution behavior with their visible equivalents. Unavailable actions SHALL expose a reason and SHALL NOT mutate the model. Results SHALL distinguish equal names by identity/context and SHALL revalidate targets before activation. Empty queries and no matches SHALL have usable non-error states. Every existing user action SHALL remain reachable through its workspace, contextual controls, Settings, or the palette; removing old sidebar buttons SHALL not remove capabilities.

#### Scenario: Search duplicate names
- **WHEN** two boards share a name and match the query
- **THEN** separate results identify them unambiguously and activation selects only the chosen board

#### Scenario: Run a blocked action
- **WHEN** an action is unavailable because of an active edit or modal state
- **THEN** palette activation respects that same restriction rather than bypassing the visible control's guard

### Requirement: Visual fidelity SHALL include complete functional states

At 1440 x 900 logical points and 100 percent interface scale, the application SHALL follow Main Window 2a and the remaining approved handoff screens for composition, tokens, type hierarchy, iconography, spacing, and control placement. Welcome SHALL use its 1100 x 700 reference and Settings its 780 x 560 reference. All five rail entries SHALL appear consistently, including Stock where the older Design illustration omits it. Displayed counts, geometry, verification, dates, and amounts SHALL be computed or honestly unavailable rather than copied from examples. Empty, unselected, multi-selected, invalid, stale, busy, long-content, and disabled states SHALL use the same visual system and remain functional. Native rasterization differences SHALL be recorded during visual review; they SHALL NOT excuse missing controls or materially different layout. Existing capabilities omitted from illustrations SHALL remain accessible through cohesive additional states.

#### Scenario: Compare reference screens
- **WHEN** the deterministic reference project is displayed at each reference size
- **THEN** each approved screen is reviewed against the supplied reference with deviations documented, and all visible controls operate on actual state

#### Scenario: Open an empty project
- **WHEN** a new project has no boards, materials, stock, or receipts
- **THEN** all workspaces display useful empty states and creation routes rather than fabricated sample content
