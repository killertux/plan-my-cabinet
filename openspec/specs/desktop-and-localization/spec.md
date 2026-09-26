# desktop-and-localization Specification

## Purpose

Define an offline macOS arm64 desktop workflow, independent localized presentation and exports, and responsive cancellable background operations. Linux remains experimental outside this first-release acceptance scope.

## Requirements

### Requirement: Desktop workflows SHALL operate offline on macOS arm64

The application SHALL provide the core project, board, assembly, allocation, and local export workflows as a macOS arm64 desktop application. These workflows SHALL not require sign-in or connectivity. A saved versioned project file SHALL reopen locally without changing model values. Native file-picker cancellation SHALL return to the current project without losing edits. Linux builds and cross-platform GUI transfer remain unverified and are not first-release requirements.

#### Scenario: Complete a disconnected workflow
- **WHEN** the macOS application runs without network access
- **THEN** a user can create a project, edit boards, arrange an assembly, review allocations, save, reopen, and generate supported local outputs

#### Scenario: Cancel Save As
- **WHEN** a user cancels the platform file picker during Save As
- **THEN** the current project and its unsaved edits remain available without reporting a successful save

### Requirement: UI and export languages SHALL be independently selectable

The application SHALL provide English and Brazilian Portuguese for user-facing controls, validation messages, status text, and generated explanatory output. Users SHALL be able to choose export language independently of UI language. Language changes SHALL preserve user-entered names, manufacturer identifiers, committed project values, and currency. Missing localized text SHALL fall back to readable English rather than expose a raw translation key.

#### Scenario: Portuguese interface with English handoff
- **WHEN** a user working in pt-BR chooses English for an export
- **THEN** generated headings and instructions are English while the interface remains pt-BR and user-entered part names remain unchanged

#### Scenario: Change interface language during editing
- **WHEN** a user changes the interface from English to pt-BR
- **THEN** the same model values and names remain in the project and only presentation language changes

### Requirement: Numeric input SHALL accept ungrouped decimal comma or dot

Measurement inputs SHALL accept either comma or dot as a decimal separator regardless of UI language, but SHALL not accept digit-grouping separators. A single comma or dot SHALL always mean a decimal separator, never thousands grouping. Inputs containing both separator types, repeated separators, or grouped digits SHALL be rejected with an explanation. Supported unit suffixes SHALL include mm, cm, m, in, and ft, with inch and foot marks accepted as equivalent suffixes. Unsuffixed input SHALL use the visibly selected field unit. Inch inputs SHALL accept proper fractions and mixed fractions such as `3/4 in` and `1 1/2 in`; zero denominators and malformed fractions SHALL be rejected.

#### Scenario: Interpret commas without thousands guessing
- **WHEN** a user enters `1,234 mm` in either language
- **THEN** the input means 1.234 mm rather than 1234 mm

#### Scenario: Reject grouped or ambiguous input
- **WHEN** a user enters `1,234.5 mm`, `1.234,5 mm`, or `1 000 mm`
- **THEN** the input is rejected without changing the committed value and the user is told to omit grouping separators

#### Scenario: Convert an explicit fractional inch value
- **WHEN** a user enters `1 1/2 in` in a field currently displaying millimetres
- **THEN** the proposed committed length is 38.100 mm

#### Scenario: Reject an invalid fraction
- **WHEN** a user enters `3/0 in`
- **THEN** validation identifies the invalid fraction and retains the previous value

### Requirement: Display formatting SHALL be independent of project data

Measurements and money SHALL be formatted for the selected presentation locale while retaining canonical project values and the declared project currency. Changing language, display units, or export formatting SHALL not rewrite numeric project values. Read-only displays MAY use locale grouping, but editable numeric text SHALL be ungrouped and unambiguous. Opening and leaving a field without editing SHALL not commit its rounded display text as a new value.

#### Scenario: Display rounding does not reduce precision
- **WHEN** a stored 12.345 mm dimension is shown as 12.35 mm and the user focuses and leaves the field without making an edit
- **THEN** the stored dimension remains 12.345 mm

#### Scenario: Locale-independent round trip
- **WHEN** a project saved with an English interface is reopened with a pt-BR interface
- **THEN** dimensions, prices, currency, and allocation results remain the same despite different decimal presentation

### Requirement: Precision loss SHALL require explicit confirmation

When input conversion exceeds the project's 0.001 mm linear precision, the UI SHALL show the entered quantity and the rounded result with enough digits and units to make the difference clear, and SHALL require confirmation before committing. Cancelling SHALL preserve the existing value. Batch edits SHALL obtain confirmation for all proposed rounding before committing any target. This confirmation SHALL not make otherwise invalid zero or negative dimensions acceptable.

#### Scenario: Fractional inches require rounding
- **WHEN** a user enters `1/64 in`, equivalent to 0.396875 mm
- **THEN** the UI requests confirmation of 0.397 mm before commitment and cancellation preserves the previous value

### Requirement: Navigation SHALL support keyboard and mouse or trackpad

The scene SHALL offer discoverable orbit, pan, zoom, and frame-selection controls usable with a mouse and with a trackpad, without requiring a three-button mouse. Core dialogs and property fields SHALL support keyboard focus traversal, visible focus, activation, and cancellation. Keyboard controls SHALL provide a way to adjust the view without precision pointer gestures. Automated macOS interaction checks cover these controls, but do not imply that the waived interactive GUI walkthrough or universal accessibility compliance was verified.

#### Scenario: Navigate without a middle mouse button
- **WHEN** a user works with a trackpad lacking a middle button
- **THEN** they can orbit, pan, zoom, and frame the selected board through documented controls

#### Scenario: Complete New board by keyboard
- **WHEN** a user opens New board and uses only the keyboard
- **THEN** they can move focus through its fields, inspect validation feedback, confirm valid input, or cancel with focus returned to the invoking context

### Requirement: Editing focus SHALL prevent shortcut interference

Application shortcuts SHALL respect platform conventions and the active input context. While text or numeric input has focus, typing, text navigation, deletion, and text-edit undo SHALL act on that input rather than trigger scene transforms, delete selected objects, or undo committed project operations. Escape SHALL cancel the active input or preview before invoking unrelated editor actions. Destructive or model-editing shortcuts SHALL not run through an open modal dialog.

#### Scenario: Delete text rather than a selected board
- **WHEN** a board is selected and the user presses Backspace or Delete while editing its name or dimension
- **THEN** the key affects the field text and does not delete the board

#### Scenario: Text undo does not undo the project
- **WHEN** the user invokes the platform undo shortcut while a field contains an uncommitted text edit
- **THEN** it acts on that text edit without undoing a committed board creation or placement

### Requirement: Background work SHALL be cancellable and revision-aware

Long-running allocation searches, optimization, cut validation, and export preparation SHALL expose activity and cancellation while keeping the desktop responsive. Cancelling SHALL preserve the committed project and report cancellation rather than success. Results SHALL identify their source revision and SHALL not overwrite newer edits. A stale result SHALL be discarded or visibly retained only for review; applying it SHALL require recomputation or validation against the current revision. Outputs completed from an older revision SHALL be labelled with that revision rather than presented as current.

#### Scenario: Cancel an optimization
- **WHEN** the user cancels an optimization before accepting its result
- **THEN** existing allocations and undo history remain unchanged and the UI remains usable

#### Scenario: Edit while a worker is running
- **WHEN** a user changes a board after a worker starts and that worker later returns a result for the earlier revision
- **THEN** the result does not overwrite the new board or allocation state and cannot be applied as though it described the current revision
