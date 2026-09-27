# Measurement entry (English)

**Settings** has Cutting, Grid & units, Costs & currency, General, Shortcuts
and About sections. The footer's **Done** or Escape closes Settings without
accepting an unfinished child editor. App-only preferences save automatically;
grid, kerf and costs remain explicit project edits. Enlarged interfaces scroll
their settings body while retaining section navigation and Done. Enter/Escape
in a child popup affects that popup first, not the Settings parent.

Manufacturing dimensions are stored at 0.001 mm resolution. You may enter
ungrouped decimals using either a dot or comma, regardless of the interface
language. `1,234 mm` means **1.234 mm**, not 1,234 mm. Do not enter digit-grouping
spaces or both separator types; use `1200 mm` or `1.2 m` instead. A field with
no unit suffix uses its displayed unit.

Supported suffixes: `mm`, `cm`, `m`, `in`, `ft`, plus `"` for inches and `'` for
feet. Inch fractions such as `3/4 in` and `1 1/2 in` are accepted; `3/0 in` is
not. The input preview shows the exact converted value or a suggested rounded
value. A rounded suggestion requires explicit confirmation. Merely focusing
and leaving a field never commits its rounded display text to the project.

Unit and interface-language switches change presentation only; they never
rescale a board or convert a project price. A project uses one currency; a
currency change requires explicit replacement prices or a confirmed relabel
without an exchange-rate conversion.

## Design inspector and selection HUD drafts

For one selected board, the inspector and bottom selection HUD edit **the same**
pending local length and width, with a shared anchor, validation and rounding
consent. Correcting text in either surface updates the other; the two fields
are accepted together as one undoable edit. The inspector also has inline
numeric position (X/Y/Z) and rotation (X°/Y°/Z°) drafts with a World or
Local-parent frame. Position text uses the same measurement parsing and
rounding rules; rotation is entered in degrees. These are proposals until
accepted. The compact thickness readout describes the board's effective
thickness; use the inspector's advanced dimension editor to change thickness
with an anchor and validation. The existing Place face to face route remains
available for placement.

With a field focused, **Enter** or **Apply** commits valid length/width text;
**Escape** or **Discard** cancels it. For inline pose, use **Enter** or
**Accept** to commit, and **Escape** or **Cancel preview** to restore the prior
pose.
Invalid text or an unconfirmed rounded conversion cannot be accepted. For
example, `1/64 in` converts to approximately 0.396875 mm: the proposed
0.397 mm needs your explicit rounding consent before Apply/Accept. Editing
that text again clears its consent. Focus changes alone do not commit; merely
focusing and leaving a rounded display such as 12.35 mm for an exact
12.345 mm value leaves 12.345 mm stored and adds no undo step. Cancellation also
leaves the project and its allocations unchanged.

The first edit to each measurement field captures its entry unit and input
locale. If you type unsuffixed `1,5` while viewing millimetres in pt-BR, then
switch to centimetres and English, the pending text still means **1.5 mm**;
it is neither reformatted nor accepted. An explicit suffix such as `1/64 in`
continues to override the captured display unit. Validation, the physical
proposal and any consent still applicable to unchanged text survive the
switch. Further edits to that pending field keep its original parsing context
but reset consent. Pristine fields may reformat from their exact stored values;
new drafts use the new presentation settings. Switching units or language
alone creates no measurement edit or undo entry.

Leaving the workspace, changing the selected board/inspector target or
starting an incompatible action while a draft is pending opens a resolution
prompt: **Apply / Discard / Stay** for dimension fields and **Accept / Cancel
preview / Stay** for a pose or repair preview. Apply/Accept validates and then
continues to the destination; Discard/Cancel preview continues without that
edit; Stay retains the edit and current location. Invalid input or missing rounding consent
disables acceptance, so choose Stay to correct it or Discard/Cancel preview to
leave. Closing or collapsing the inspector preserves pending text, errors and consent
for reopening; it is not a commit or a cancellation.

## Creation, placement and batch-resize dialogs

**New board** and **New material** use centered, isolated forms. Material
swatches affect appearance only; the board's first-fit outlook is a
nonmutating preview, not a reservation. You may revise fields repeatedly and
cancel without creating a board, allocation or material. A board confirmation
validates the current material/stock again, so a preview from before a stock
change cannot authorize stale placement. Opening New material from a board
draft and cancelling it returns to that intact board form. An inexact entered
dimension requires explicit rounding consent; changing the proposal clears it.

**Numeric pose** and **Place face to face** preview changes without modifying
the project. The first has a World/Local-parent frame and XYZ position/rotation;
the second identifies both boards/faces, in-plane alignment/offset and outward
gap. The footer's **Apply** or **Place** commits one undoable pose; **Cancel**
or Escape discards the preview. **Resize N boards** distinguishes mixed
current values, shows each board's before/after value and anchor, and applies
the validated change to all boards in one undoable edit. Its invalid or
unconfirmed rounded proposal cannot be accepted. The modal boundary keeps
scene drags, project shortcuts and background scroll from acting behind these
forms; Enter cannot submit a parent form when an open child popup consumes it,
and Escape dismisses the active popup/edit layer first.

## Workspace and command-search shortcuts

Use **⌘** on macOS or **Ctrl** on other supported desktops for the shortcuts
below. With a project open, **⌘/Ctrl+1–5** switches to Design, Stock, Cut plan,
Hardware and Handoff respectively. **⌘/Ctrl+K** opens Search commands from the
workspace. The header also offers Search commands, Save, undo/redo, and Export;
Export opens Handoff to review output preparation.

In Search commands, type to find actions or boards, materials,
stock pieces (including stock aliases), hinge installations and door
relationships by name or ID. An empty query lists available actions. Use
**Up/Down** to select a result, **Enter** to activate it, or **Escape** to close
the palette and return focus. A disabled result gives a reason; selecting an
entity navigates to its identified target. Pending edits may require the
Apply/Discard/Stay or Accept/Cancel preview/Stay decision described above.

**⌘/Ctrl+S** saves the project, **⌘/Ctrl+Z** undoes and
**⌘/Ctrl+Shift+Z** redoes when those actions are available. Project and
workspace shortcuts do not take over focused text input or modal dialogs and
popups. Search commands also stays closed while another modal or file operation
owns the interaction. For a focused dimension or pose field, use its own
Enter/Escape handling described above instead.
