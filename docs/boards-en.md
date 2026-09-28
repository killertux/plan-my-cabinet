# Boards and materials (English)

## Design workspace and advanced board controls

Design's outliner lists real boards and their parent assemblies; selecting a row
highlights the same physical part in the native 3D view. The inspector shows its
material, effective thickness and provenance, local dimensions, grain, pose,
allocation state and stock preview. The single-board selection HUD exposes
inline length/width edits; advanced board/material, thickness, hierarchy and
placement controls remain accessible from the inspector and object actions.
The shared header offers project navigation and Save. New or duplicated boards
are separate physical parts; any board without a stock placement is shown as
**Unallocated** rather than hidden from allocation diagnostics. See
[Assemblies and hierarchy](assembly-en.md) for placement and [Stock](stock-en.md)
for stock/allocation operations.

## Create and edit

Create a material first with **New material**: enter a name and a positive
thickness, then choose its default grain direction and optional display swatch.
The swatch is visual metadata, not a stock or cutting-property change. **New
board** asks for a
name, material, local length, and local width. It displays the effective
material and thickness before confirmation. A first-fit stock outlook is only
a preview, not an allocation or reservation; confirmation rechecks the current
material and stock. You can open New material from a board draft and return to
that intact draft if you cancel the nested form. Thickness comes from the
chosen material, rather than a third board-creation input. A newly created
board follows that material's grain default; the initial plywood convention
is grain along local length. Missing, zero, negative, nonfinite, or
unrepresentable dimensions cannot be committed. Cancel or invalid creation
does not create a partial board.

**Length**, **Width**, and **Thickness** mean the board's own local X, Y, and
Z axes. Turning a board or its parent assembly changes its world orientation,
not these labels or its initial rectangular blank dimensions. For example,
a 2300 × 600 × 18 mm side remains 2300 mm long after being turned upright.
Use **Numeric pose** (including the cancellable orientation presets), face
placement or Move board to change pose without renaming these local axes.

Each board's **Board grain** menu has **Follow material default**, **Along
length**, **Along width**, and **Unrestricted**. The latter three are explicit
board overrides, measured along local axes even after rotation; an override
survives a later change to the material default. The list shows **Effective
grain (local board axis)**. Changing grain does not silently relocate any
stock placement.

**Edit dimensions** selects one local axis and a **Resize anchor**. **Keep
start face** fixes that axis's minimum-coordinate face, **Keep end face** its
maximum-coordinate face, and **Keep centre** its midpoint; only the opposite
face moves for start/end, while centre moves both faces equally. Anchors stay
local even in a rotated parent assembly. The proposed value and anchor are
shown before **Confirm**. An invalid edit leaves the committed dimensions
alone. Changing thickness here changes this board's effective thickness, not
the material's default or a measured stock piece.

**Assign material** previews the board's resulting effective thickness and
offers a thickness anchor. **Edit material** previews affected boards and
requires **Preserve existing boards** or **Apply to all affected boards**.
Preserve keeps their previous effective thickness and grain state while new
assignments use the revised default; apply changes affected boards explicitly
with the chosen thickness anchor. Measured stock thickness never follows a
material-default edit implicitly. Compatibility depends on material identity
*and* each board's and stock piece's effective thickness. A resulting
allocation conflict is reported, not repaired by a silent reassignment.

**Duplicate board** copies dimensions, material and grain settings but creates
a distinct physical part with a new ID. The desktop action offsets the copy's
pose by 25 mm along X and selects it. The copy keeps the original's name
until you rename it: click the name at the top of the inspector (or press
`F2`, or choose **Rename** in the Outliner's right-click menu), type the new
name and press `Enter`. `Escape` keeps the old name. Assemblies and hardware
are renamed the same way, and a rename is one undo step. Editing one does not edit
the other; the copy does not inherit a stock allocation. Select board-row
checkboxes and use **Edit selected dimensions** to apply one local dimension
to several boards. The dialog shows **Mixed values** if their current values
differ, lists affected boards and individual anchors, and commits all valid
targets together or none if one is invalid.

## Inspector and HUD edits

In **Design**, select the board in the outliner or the scene. With no selection
the inspector does not pretend a board is active; with several selected objects
it shows their actual shared/batch context rather than one arbitrary board.
Hidden boards remain selectable from the outliner and still require stock.

With one board selected, edit **Length** or **Width** in either the inspector or
the floating HUD. Both surfaces show the same pending text, local resize
anchor, validation and rounding consent; changing surfaces does not create a
second edit. Untouched rounded display text retains the exact committed
quantity. Enter or **Apply** accepts a valid draft as one undoable edit; Escape
or **Discard** cancels it. Leaving an invalid or unconfirmed rounded draft for
another target/workspace offers Apply (disabled until valid), Discard or Stay;
collapsing the inspector keeps the draft. Editing the text clears previous
rounding consent; merely losing focus does not commit. The current display unit
and input locale are captured on the first text edit, so changing unit or
language while it is dirty does not reinterpret it. An explicit unit suffix
still takes precedence. Advanced **Edit dimensions**, including anchored
thickness changes and multi-board **Edit selected dimensions**, remain available;
the HUD's thickness is a read-only *effective board value*, which may differ
from the material's current default.

For an allocated board, the inspector's clickable sheet miniature shows its
actual stock piece, trims and part footprints, highlighting that board. Open
it to navigate to the same board's allocation in Cut plan. An unallocated board
instead leads to its allocation issue. This navigation does not reposition the
board or select every board on the sheet; unfinished drafts receive the same
Apply/Discard/Stay decision. The miniature is a stock sketch, not an
independent cut-feasibility certificate.

## Keyboard-only walkthrough using advanced board controls

Opening a dialog moves focus to its first control. `Tab` (and `Shift+Tab` to
go back) stays inside the dialog until it closes. Press `Space` on a focused button or
checkbox. `F2` renames the selected board; there are no special New board,
duplicate, or edit shortcuts. The
following uses exact millimetre values so no rounding prompt appears; type
the unit suffix to make the input unambiguous. On macOS use `Command+A`, or
on Linux `Ctrl+A`, to replace all text in a focused input.

With a project open, `⌘/Ctrl+1–5` switches workspaces; it does not create objects.
Use `⌘/Ctrl+K` and search for **New material** or **New board** to open those forms.

1. Run `cargo run --locked`. On Welcome, Tab to **New project** and press
   `Space`. In **Design**, Tab to **New material** and press `Space`; if the
   left controls are collapsed, reopen **Design** from the header first. In
   the dialog, **Material name** already has focus: type `Plywood 18`; Tab to
   **Thickness**, type `18 mm`. Leave **Default grain** at **Along length**.
   Press `Return` to create the material. `Escape` would cancel instead.
2. Tab to **New board**, press `Space`; **Board name** has focus: type `Side`.
   The first material is already selected. Tab to **Length**, type `2300 mm`;
   Tab to **Width**, type `600 mm`. Check the displayed effective material and
   18 mm thickness, then press `Return`. `Side` appears in **Outliner**.
   In the left controls, expand the lower **Advanced view and language**
   section containing board actions to see its ID, `2300 × 600 × 18`, and
   **Unallocated**, and to reach the duplicate/edit controls used below.
3. Tab to the **Duplicate board** button belonging to `Side` and press
   `Space`. There are now two `Side` rows with different IDs. Find the second
   row by its position and ID; its dimensions still read `2300 × 600 × 18`
   and it is **Unallocated**.
4. Tab to **Edit dimensions** in the *second* row and press `Space`. **Local
   axis** starts at **Length** and **Resize anchor** starts at **Keep centre**.
   Tab to the **Length** input, press `Command+A`/`Ctrl+A`, and type `2200 mm`.
   Tab to **Confirm** and press `Space`. Verify the first row remains
   `2300 × 600 × 18` while the copy reads `2200 × 600 × 18`.
5. Tab to **Edit dimensions** in the *first* row and press `Space`; replace
   its **Length** input with `2400 mm` the same way, then focus **Confirm**
   and press `Space`. The two rows now read `2400 × 600 × 18` and
   `2200 × 600 × 18`: each board was edited independently. `Escape` cancels
   either edit dialog without committing it.

For a non-exact input such as `1/64 in`, the field displays the proposed
rounded 0.397 mm value and a rounding-confirmation checkbox. Focus that
checkbox and press `Space` to consent before confirming; typing a different
value clears consent. Merely tabbing away never accepts rounding. See
[Measurement entry](input-en.md) for units and decimal syntax.
