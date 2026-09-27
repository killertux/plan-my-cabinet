## ADDED Requirements

### Requirement: All dialogs SHALL use the unified modal presentation and input boundary

The application SHALL provide the handoff's centered, dimmed-backdrop modal presentation with contextual header, grouped body fields, inline validation, and action-named footer buttons for New board, Position, Place face to face, Resize N boards, New material, and Unsaved changes. The same presentation and interaction contract SHALL cover existing grid-spacing, kerf confirmation, cut-fee, stock-piece, material preserve/apply, currency-change, grouping/reparenting, duplicate-assembly, hardware, door-relationship, delete, overwrite, and recovery decision dialogs. Settings SHALL use its larger section-list presentation and Done footer. Modals SHALL isolate pointer, scroll, keyboard, scene navigation, and project shortcuts from underlying content. Focus SHALL start in the first applicable field or action, remain visibly within the modal during Tab/Shift-Tab traversal, and return to the invoking context on close. Enter SHALL confirm only a valid form when no child popup or active control consumes it. Escape SHALL close the active popup or cancel the active editing layer before cancelling its containing modal. A single key event SHALL not both select a popup option and submit or cancel the parent form.

#### Scenario: All six reference dialogs retain their actions
- **WHEN** a user opens each of New board, Position, Place face to face, Resize N boards, New material, and Unsaved changes
- **THEN** each uses the unified presentation and exposes its real fields, previews or decisions, with invalid confirmation disabled and cancellation preserving committed state

#### Scenario: Popup consumes Enter and Escape
- **WHEN** a dropdown is open inside a modal and the user selects an option with Enter or closes it with Escape
- **THEN** only the popup handles that key and the containing modal remains open with its draft intact

#### Scenario: Background scene cannot receive modal input
- **WHEN** a modal is open and a user scrolls, drags, presses Delete, invokes project undo, or uses scene camera shortcuts outside its fields
- **THEN** no underlying scene, selection, project history, or viewport camera is changed by those inputs

#### Scenario: Nested material dialog restores board editing
- **WHEN** a user opens New material from New board and cancels the material dialog
- **THEN** the pending board fields remain intact and focus returns to board editing rather than the background workspace

### Requirement: Adaptive layouts SHALL preserve access and unfinished edits

The primary workspace reference size SHALL be 1440 by 900, with Design's 256-point outliner and 292-point inspector around a fluid viewport and the corresponding handoff proportions for other workspaces. Outside the reference size and at supported interface scales, panels and modal bodies SHALL adapt through bounded resizing, scrolling, or explicit side-panel collapse so primary actions, validation, and navigation remain reachable. Collapsing and reopening a side panel SHALL preserve its selection, pending text, validation, rounding consent, and scroll context without committing or discarding edits. Viewport overlays SHALL remain within the available viewport and SHALL not permanently conceal one another's interactive controls. Welcome SHALL support its 1100 by 700 reference and Settings its 780 by 560 reference; smaller available areas SHALL provide reachable content and completion controls through adaptation rather than clipping them off-screen. Layout acceptance SHALL cover English and pt-BR at 90, 100, 115, and 130 percent interface scale.

#### Scenario: Collapse an inspector with invalid input
- **WHEN** a user narrows the main window or collapses the inspector while it holds an invalid dimension draft
- **THEN** the main actions remain accessible and reopening the inspector restores the exact draft and error without altering the project

#### Scenario: Localized settings at enlarged scale
- **WHEN** Settings is shown at its reference size with pt-BR and 130 percent scale
- **THEN** section navigation, field labels, validation, and Done remain readable and reachable, using scrolling or adaptation where necessary

#### Scenario: Welcome reference layout
- **WHEN** Welcome is displayed at 1100 by 700 in either supported language
- **THEN** project-opening actions, template entries, recovery decisions, recent projects, language, and preferences are reachable without overlapping primary controls

### Requirement: Application preferences SHALL persist separately from project edits

Settings General SHALL expose interface language, navigation hints, inverse scroll-to-zoom, material-color tint, and interface scale choices of 90, 100, 115, and 130 percent. These app preferences SHALL save automatically to the platform's local configuration directory, survive restart, apply across projects, and never enter a `.pmcab` project file. Changing them SHALL not dirty the project, change its revision or undo history, or invalidate manufacturing results. Navigation hints SHALL control contextual status guidance; inverse zoom SHALL reverse scroll zoom direction; material tint SHALL control scene shading without changing saved material colors; interface scale SHALL affect the actual interface. Settings SHALL distinguish app preference persistence from undoable project changes such as grid spacing, kerf, and costs, with accurate footer guidance. General SHALL retain access to recovery-folder inspection and explicit snapshot-cleanup controls.

#### Scenario: Preferences survive restart independently of a project
- **WHEN** a user selects pt-BR, disables hints and tint, enables inverse zoom, chooses 115 percent, and restarts the application
- **THEN** those choices are restored even when opening another project, with unchanged project dirty state, revision, undo history, and manufacturing freshness

#### Scenario: Project grid spacing remains a project edit
- **WHEN** a user changes grid spacing in Settings and then changes interface scale
- **THEN** grid spacing commits through project validation and undo while the scale change persists as an app preference and adds no project undo entry

### Requirement: Redesigned unit selectors SHALL preserve presentation-only semantics and supported units

Settings and redesigned measurement controls SHALL retain mm, cm, m, in, and ft presentation choices, including access to m and ft beyond the compact handoff segments. Input SHALL retain supported explicit suffixes and fractional-inch forms independent of selected display unit. Changing display units SHALL not enter project undo history, advance project revision, change canonical quantities, or invalidate manufacturing results. When a field first becomes dirty, its entry display unit and input locale SHALL be captured. Subsequent display-unit or interface-language changes SHALL preserve its entered text, interpretation, validation, physical proposal, and any still-applicable rounding consent; unsuffixed text remains in its captured unit, explicit suffixes override that unit, and subsequent edits to the same draft retain its captured parsing rules while clearing consent. Pristine fields MAY reformat from exact committed quantities and new drafts SHALL use the new settings. Neither preference change SHALL accept a pending edit. Export-unit selection SHALL remain independently controllable from interface display units.

#### Scenario: Select metres or feet
- **WHEN** a user selects m and then ft through the redesigned unit controls
- **THEN** measurements change presentation using those units while canonical dimensions, project revision, and undo history remain unchanged

#### Scenario: Switch units with a pending conversion
- **WHEN** a user changes display units while a `1/64 in` draft awaits rounding confirmation
- **THEN** the entered quantity remains pending, no value is silently committed, and explicit confirmation remains required for its rounded result

#### Scenario: Switch units and language with unsuffixed pending text
- **WHEN** a user enters unsuffixed `1,5` in a pt-BR millimetre field and switches the display unit to centimetres and interface language to English before accepting the edit
- **THEN** the unchanged draft still represents 1.5 mm under its captured pt-BR parsing rules, its validation and any applicable rounding consent remain intact, and no project value or history changes until an explicit acceptance; a newly started draft uses the new settings

### Requirement: Shortcuts help and redesigned assets SHALL work offline in both languages

Help → Shortcuts and Settings Shortcuts SHALL provide localized, discoverable documentation of active platform keyboard controls, including camera navigation, frame selection, tool use, snap bypass, preview cancellation, dialog interaction, workspace switching, command search, and preferences. Camera navigation SHALL remain usable by keyboard and trackpad without the removed legacy Orbit/Pan/Zoom buttons or a three-button mouse. All redesigned labels, tooltips, validation, hints, dialogs, settings, and shortcut explanations SHALL be available in English and pt-BR, with readable English fallback instead of raw keys. The native desktop package SHALL bundle its icons, Noto Sans regular and semibold, JetBrains Mono regular, font licenses, and other required rendering/help assets so the complete redesigned interface operates without network access, webviews, remote fonts, or an installed browser runtime. Language changes SHALL preserve user-authored text and pending drafts and SHALL remain independent of export language.

#### Scenario: Disconnected localized interface
- **WHEN** the installed application launches without network access in pt-BR
- **THEN** every workspace, modal, Welcome, Settings, and shortcut-help surface renders with bundled fonts and icons and readable localized text without downloading assets

#### Scenario: Discover camera controls without legacy buttons
- **WHEN** a trackpad or keyboard-only user opens Help → Shortcuts
- **THEN** the documented controls let them orbit, pan, zoom, and frame selection and explain Alt bypass and Escape cancellation without requiring a middle mouse button
