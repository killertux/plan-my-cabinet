# Project files and recovery (English)

## Availability

The desktop offers New project, Open project, Save and Save As, with an unsaved
indicator beside the project name/path. Native pickers run asynchronously.
New, Open and window close ask to save, discard or cancel unsaved changes.
Open validates the selected file before asking to discard current work.
Save As requires confirmation before replacing an existing destination.
Undo/redo history starts fresh for each opened or newly created project.

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

After 30 seconds without a new committed edit, the desktop writes a
snapshot of unsaved committed changes. Another committed edit resets that
inactivity clock; uncommitted input/drag previews are never included.
Recovery is stored separately under the application's
**platform user-data directory** (in a `recovery` subfolder), associated with
the project identity and saved file path. It is local to that installation,
not embedded in `.pmcab`, and copying/renaming a project to another path does
not transfer or adopt the old path's recovery snapshot.

When a valid recovery revision is newer than the explicit save, reopening
presents the project/path and saved versus recovery revision for
an explicit choice:

- **Recover:** work from the recovered committed state, still unsaved; save
  explicitly to replace the saved file. Past-session undo history is not
  restored.
- **Discard:** remove that recovery candidate; the explicitly saved file is
  unchanged.
- **Defer:** leave the candidate and saved file untouched for a later choice.

Recovery never silently overwrites the explicit save. A missing, stale,
invalid, or mismatched recovery record cannot automatically replace a project;
an invalid record is retained for diagnosis rather than accepted as a valid
revision. Keep explicit saves for portable backups.

## Compatibility and damaged files

Opening checks the format version and validates the complete project before
replacing the current one. This core supports schema version 1 and refuses
unsupported versions, including files written by a newer incompatible
application. Use a compatible application version for such a file; an older
version must not rewrite it. Incomplete/invalid JSON, files over the 16 MiB
document limit, broken references, cycles, and invalid values fail to load.
The source file and the currently open project (including unsaved edits) stay
untouched on a failed load. Keep the original file and restore a known-good
copy or correct the source outside the app before trying again.
