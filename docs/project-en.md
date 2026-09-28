# Project files and recovery (English)

Settings → General stores interface language, scale, hints, inverse zoom and
material tint on this computer, outside the portable project. **Done** or
Escape closes Settings; neither action saves a project or accepts a pending
dimension draft. Project cutting/grid/cost changes still use validated,
undoable commands, whereas display-unit and interface-language changes do not
alter revision, undo history or manufacturing freshness.

## Availability

The desktop offers New project, Open project, Save and Save As, with an unsaved
indicator beside the project name/path. Native pickers run asynchronously.
New, Open and window close ask to save, discard or cancel unsaved changes.
Open validates the selected file before asking to discard current work.
Save As requires confirmation before replacing an existing destination.
New project first asks for the project's name, currency and input unit
(the same choices as a template's first step); Cancel creates nothing.
To rename the open project, click its name in the header and choose
**Rename project…** (also in ⌘K). Renaming is one undoable edit.
Undo/redo history starts fresh for each opened or newly created project.

## Pending edits, navigation and shortcuts

### Start with a cabinet template

Welcome offers **Base**, **Wall**, and **Drawers** even on the first launch,
without an existing project or material library. Each tile starts a staged
new-project setup: choose a name, currency and input units; create materials
with thickness, grain and optional display color; then assign those materials
to the component roles. Roles may share a material where appropriate. Backing
out of a nested New material form preserves the template setup. The review
shows the construction assumptions, board dimensions, front/back depth datums,
clearances and first-fit outlook before **Generate project**. Invalid geometry
or unconfirmed numeric rounding blocks generation.

**Base** starts with full-height sides, a bottom and front/rear top rails;
**Wall** has top/bottom, a shelf and an overlay back. **Drawers** includes a
carcass, separate box sides/fronts/backs/applied bottoms and exterior fronts.
Set its drawer count, box depth, side and rear clearances, vertical clearance,
front reveals and gaps in the setup. These are independent editable boards,
not ongoing parametric constraints. They do not select slides, supply
machining instructions, or certify structural or mechanical fit.

The setup does not alter the open project. **Cancel setup** returns to Welcome
without creating a project or a recent entry. Generate first asks how to
resolve unfinished edits and unsaved work in any project behind Welcome;
cancelled/failed Save leaves both the old project and setup available. On
success, Design opens the new unsaved project with its generated assembly
selected. All staged materials and parts form one undoable transaction. With
no stock declared, its parts are honestly unallocated; use **Open Stock** to
add measured physical sheets and allocate them. A recent entry is registered
only after a successful explicit project save.

The Design inspector and single-board HUD share a session-only length/width
draft; inline numeric pose fields have their own pending proposal. Enter or
Apply accepts valid dimensions in one undoable edit; Enter or Accept accepts
valid inspector pose fields. Escape or Discard cancels dimensions; Escape or
Cancel preview cancels pose editing. Focus changes do not save or accept a
draft. See
[Measurement entry](input-en.md) for unit/locale capture, exact values and
rounding consent. The advanced thickness editor and Place face to face action
remain available separately.

Changing workspaces, changing an inspector/selection target or starting an
incompatible action while an edit is unfinished asks **Apply / Discard / Stay**
for fields or **Accept / Cancel preview / Stay** for a pose/repair preview.
Acceptance is unavailable for invalid input, unconfirmed rounding or an invalid repair;
failed validation keeps the draft and its location for correction. Stay keeps
both; Discard/Cancel leaves the committed model as it was before the draft
and proceeds. The destination is checked again after acceptance, so a removed
target or stale project/revision cannot be silently selected. Collapsing and
reopening a panel retains its unfinished draft and validation without a
navigation decision. Workspace switches keep session view context such as
selection, panel state, filters and scroll, rather than creating model edits.

Save and project replacement resolve a pending edit first; a successful Apply
can then continue to Save or to the usual New/Open/close unsaved-project
decision. Save does not capture uncommitted text or previews. New/Open/close
still ask about unsaved **committed** changes, and a prepared Open is validated
before replacing the current project. Cancelling a file picker or a failed
save does not replace current work. On actual project replacement, pending
drafts, navigation, selection and per-project view state are cleared; they are
not portable `.pmcab` data or recovered by autosave. A completed edit is
undoable within the current project session; a discarded draft has no undo
entry.

Platform **Command-S** (Control-S where applicable) saves, **Command-Z**
undoes and **Command-Shift-Z** redoes committed project edits. These project
shortcuts do not run while a text field, modal or popup owns keyboard input;
the field's own deletion/undo remains text editing rather than deleting or
undoing a scene object. Shortcut actions use the same availability and pending-
draft resolution checks as their visible controls. Save, Undo and Redo first
ask for an edit decision when a draft is pending; Stay keeps that draft and
does not run the shortcut action. Undo/redo otherwise acts on committed
history, not pending text.

## Portable, offline projects

A `.pmcab` is a single UTF-8 JSON file with an explicit format version. It
currently stores materials, boards, assemblies, stock, allocations, stock
prices, display units, currency, hardware instances, and pinned catalog values.
Cutting settings, door joints and installation relationships are also stored.
For data in the schema, the receiving computer
does not need an account, network connection, original machine's file paths,
or a catalog download to reopen and edit the saved design. Later catalog
revisions do not silently replace values pinned in an existing project.
Transfer the **saved `.pmcab` file**, not a recovery snapshot or an exported
cutting document. Undo history is only for the current editing session; it
does not travel with the file and starts fresh after opening or recovery.

### Move a saved file between macOS arm64 computers

1. On the source computer, finish/commit the edits to keep. Explicitly save
   the project to a `.pmcab` file, or use Save As to choose a new `.pmcab` path.
   Confirm that saving succeeds and that no unsaved changes remain. An
   autosave is not a substitute for this step.
2. Copy that file to a USB drive or another file-transfer medium, then copy
   it to a writable folder on the destination computer. Keep a copy of the
   source file until the transfer is verified. Only the `.pmcab` file is
   needed for the project data described above.
3. On the destination macOS arm64 installation, open the copied `.pmcab`
   with a compatible version of the application. Check the design, stock
   placements, prices, and pinned hardware values. Edit offline and explicitly
   save the local copy. To take those edits back, repeat the save-and-copy
   process in the other direction; the two copies do not synchronize.

Linux GUI operation and cross-platform file transfer are experimental and
outside this first release's support and validation scope. A headless file
transfer fixture does not establish that the Linux desktop application works.

Save As cancellation leaves the project, current file path, and unsaved status
unchanged. If a save fails before replacement, the last successfully saved
file stays intact and the in-memory edits remain unsaved; retry or save to a
different writable location. A rare failure after replacement while syncing
the destination directory is reported as **durability uncertain**: the new
bytes may already be at the path, but the edit is not marked saved. Verify the
file before relying on it or transferring it. Do not close with unsaved edits
assuming that an attempted or cancelled save succeeded.

## Autosave and interruption recovery

### Welcome and local recent entries

Welcome filters the projects successfully opened or saved on **this
computer**. A recent entry shows only cached metadata actually available:
project name, path, part/stock counts, last use and qualified export/recovery
status. An absent thumbnail uses a placeholder. These paths and thumbnails are
local conveniences, not contents of a portable `.pmcab`. A missing-file entry
offers **Locate…**, which validates the replacement candidate before changing
the recent entry or open project, and **Remove from recents**, which removes
only the list entry—never the saved project, snapshots or exports. Returning
to Welcome resolves unfinished edits but retains the current document without
asking to save committed changes. Opening a different recent project still
checks unfinished edits and unsaved committed work before replacing it.
A project is not listed merely because its
template setup was previewed or an attempted save failed.

Settings → General opens the recovery folder for inspection and offers
**Review recovery snapshots**. The cleanup list includes registered saved
and untitled identities, status, available dates and invalid records for
diagnosis. Nothing is selected automatically: choose each snapshot for
deletion, review the affected filenames and explicitly confirm. Cancelling
leaves all snapshots intact; partial deletion failure does not erase any
unselected snapshot or saved project. There is no automatic age-based cleanup
and the application does not scan unrelated user folders.

Keyboard confirmation follows the visibly focused action. Cleanup and overwrite
prompts initially focus **Cancel**; recovery prompts focus **Decide later**.
Enter on that action cancels/defers without deleting, replacing or recovering.
Use Tab to reach the affirmative action before pressing Enter. Escape cancels
the current confirmation; returning from a cleanup confirmation retains the
review rather than sending keyboard focus to the underlying workspace.

After 30 seconds without a new committed edit, the desktop writes a
snapshot of unsaved committed changes. Another committed edit resets that
inactivity clock; uncommitted input/drag previews are never included.
Recovery is stored separately under the application's
**platform user-data directory** (in a `recovery` subfolder), associated with
the project identity and saved file path. It is local to that installation,
not embedded in `.pmcab`, and copying/renaming a project to another path does
not transfer or adopt the old path's recovery snapshot.

Welcome lists registered recovery candidates. For a valid snapshot newer
than an explicit save, its card identifies the project UUID and canonical
saved path, the saved and recovered revisions, and the snapshot timestamp
when known. Unknown metadata stays unknown. Choose explicitly:

- **Recover edits:** work from the recovered committed state, still unsaved; save
  explicitly to replace the saved file. Past-session undo history is not
  restored.
- **Discard snapshot:** remove that recovery candidate; the explicitly saved file is
  unchanged.
- **Decide later:** leave the candidate and saved file untouched for a later choice.

Recovery never silently overwrites the explicit save. A missing, stale,
invalid, or mismatched recovery record cannot automatically replace a project;
an invalid record is retained for diagnosis rather than accepted as a valid
revision. Keep explicit saves for portable backups.

An **untitled** recovery card names its unsaved identity without inventing a
saved path or revision. Recovering opens an unsaved project that requires
**Save As** to create a portable `.pmcab`. If another document remains behind
Welcome, recovering it first asks for the normal unfinished-edit and unsaved-
project decisions; cancelling that decision retains both documents and the
snapshot. **Decide later** preserves the snapshot for the next Welcome visit.

## Compatibility and damaged files

Opening checks the format version and validates the complete project before
replacing the current one. This version reads schema versions 1 and 2; a
schema-1 file is validated and migrated **in memory**. Opening does not rewrite
it or mark the project dirty. The first explicit Save or Save As asks for
confirmation of the schema upgrade; Cancel leaves the original bytes untouched.
Confirming writes schema 2
atomically, including any new appearance or provenance metadata. Once saved
in schema 2, an older version that only reads schema 1 cannot open the new
file. Retain a separate untouched copy of the version-1 file if rollback to
that older application may be necessary. There is no destructive downgrade
operation and reopening with an older binary must never strip the new fields.
Legacy kerf confirmations retain their value but have an **unknown date**;
only a new explicit confirmation records a date. Changing the blade-kerf value
clears both confirmation and date. Material display colors and stock labels
are portable in schema 2; neither display color nor its absence changes
manufacturing quantities or shop readiness.

The application refuses unsupported versions, including files written by a
newer incompatible application. Use a compatible version for such a file; an older
version must not rewrite it. Incomplete/invalid JSON, files over the 16 MiB
document limit, broken references, cycles, and invalid values fail to load.
The source file and the currently open project (including unsaved edits) stay
untouched on a failed load. Keep the original file and restore a known-good
copy or correct the source outside the app before trying again.
