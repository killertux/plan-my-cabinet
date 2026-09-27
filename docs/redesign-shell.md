# Shared workspace shell (tasks 5.2–5.4)

The five rail entries route through `request_navigation`, preserving the current
project's selection and workspace view state. A Cut plan badge counts one issue
per board from the global allocation diagnostics, including unallocated,
conflicted and proof-budget-unknown boards whether or not they are visible in
the scene. Cut plan's issue list exposes the affected boards, diagnostic reasons
and Select/reveal, Repair and stock actions. The badge is never derived solely
from allocation-record counts. Repair previews have a separate diagnostics
cache; the shell describes the committed project.

The header's Projects menu exposes the existing New/Open/Save/Save As actions.
New and Open retain the prepared-load, native picker, unsaved-work and overwrite
prompts. Save uses the same guarded file action. Undo and Redo use the editor's
real history and are disabled without available history or during a modal.
Export invokes the Handoff navigation action; it does not open a PDF picker or
create an export receipt. A pending invalid dimension or placement edit receives
the navigation Apply/Discard/Stay or Accept/Cancel/Stay prompt before leaving.
Handoff's own reviewed export action retains preparation and PDF write guards.

Kerf status displays the current value and confirmation; the hover text gives
the recorded date, or explicitly unavailable date for a legacy confirmation.
The status bar identifies the current workspace's counts and global issue
breakdown, including hidden issue boards. Spending comes from the authoritative
cost estimate, cached for the committed project/revision. It only displays a
numeric total when the estimate is complete, every board has a valid diagnosis,
and the allocation count agrees. Unknown fee or used purchase price, unresolved
cut proof show **Incomplete estimate**; invalid project data or overflow use
the separate invalid-estimate explanation rather than a numeric zero or a
misleading subtotal. The optimizer/search and PDF
output show cross-workspace activity labels without accepting results on a
workspace switch.

The header Search commands button and platform ⌘K/Ctrl+K open a native command
palette when no conflicting dialog, picker, pending navigation or text input has
keyboard ownership. The blank search shows available commands. Nonblank search
matches localized action labels/keywords and entity names, aliases and full
UUIDs without case sensitivity. Results group actions, boards, materials, stock
pieces, hinge installations and door relationships. A row always includes its
UUID (and the stock alias) to distinguish equal names. Disabled command rows
explain their live availability; unmatched queries show a no-results message.
Up/Down cycle the rows, Enter invokes the current enabled row, and Escape
closes and restores the invoking widget focus. Nested popup keys remain with
the popup. Commands use the same action dispatcher as visible controls, and
entity rows use the pending-navigation guard. Unfinished previews disable
incompatible immediate commands with a reason while navigation results offer
the appropriate resolution prompt. Removed targets and newly blocked actions
are rechecked on activation; a result never resolves by name or
silently accepts an edit. Relationship rows open the existing relationship
editor after successful Hardware navigation. Search does not alter the project
or add a history step until a real command is invoked.

# Responsive shell layout (task 5.4)

Pane sizes are calculated in egui logical points after interface zoom, using
the width remaining after the 60-point rail. The Design baseline keeps a
256-point controls pane and 292-point inspector, with 46-point header and
26-point status. Other workspace pairs are Stock 256/316, Cut plan 256/308,
Hardware 268/316, Handoff 300/300. A 420-point minimum central pane plus
12-point inter-pane gaps determines collapse: inspector first, then controls.
Collapsed panes have header buttons to reopen them as bounded, bidirectionally
scrollable floating drawers; opening one never calls an edit command or changes
the project. The existing per-workspace scroll IDs and session drafts/selection
are retained; horizontal controls and both inspector scroll offsets are carried
between docked and drawer render trees and reset on project replacement. A
drawer blocks scene drag/selection beneath it. The compact
header groups Save, Undo, Redo, Export and command search in a labeled menu,
while Projects and drawer toggles remain direct. The compact status exposes its
complete existing details in a scrollable overflow menu rather than shortening
labels or shrinking fonts. These menus use the existing localized strings and
the same availability guards as direct controls.
The Stock content and existing pane controls can also scroll horizontally for
long names or dense rows instead of losing rightmost actions.

At the current native shell stage, camera controls are still rendered in rows
above the Design viewport and the 3D canvas only uses part of its central
height. The projected HUD, snap popover and redesigned Design inspector are
not mounted (tasks 6.4–6.6). Their overlay collision rules and shared inspector
draft guarantees cannot yet be certified through task 5.4; the current inspector
offers contextual links to the existing dimension dialog. The narrow static
captures cannot exercise drawer clicks or focused invalid drafts because
capture mode intentionally filters input.

The dedicated Welcome/recents screen and reviewed native Handoff preview
belong to later tasks (12.1 and 11.5–11.6). The Projects menu remains the
live project-navigation entry until Welcome is built. Optimization progress
is polled at app level across workspaces; only Cut plan presents and accepts
the candidate after explicit review.
