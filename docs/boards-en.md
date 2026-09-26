# Boards and materials (English)

## Current desktop interface

The left panel has **New material**, **New board**, a material list and a board
list. The center 3D viewport still draws a fixed smoke-test fixture: it does
not display, select, or rotate the boards in the list. The list is the current
representation of the created parts. There is no stock-entry or placement UI
yet; new and duplicated boards appear **Unallocated**. Stock allocation comes
later. The desktop shell also has no project Open/Save controls yet.

## Create and edit

Create a material first with **New material**: enter a name and a positive
thickness, then choose its default grain direction. **New board** asks for a
name, material, local length, and local width. It displays the effective
material and thickness before confirmation. Thickness comes from the
chosen material, rather than a third board-creation input. A newly created
board follows that material's grain default; the initial plywood convention
is grain along local length. Missing, zero, negative, nonfinite, or
unrepresentable dimensions cannot be committed. Cancel or invalid creation
does not create a partial board.

**Length**, **Width**, and **Thickness** mean the board's own local X, Y, and
Z axes. Turning a board or its parent assembly changes its world orientation,
not these labels or its initial rectangular blank dimensions. For example,
a 2300 × 600 × 18 mm side remains 2300 mm long after being turned upright.
The current desktop interface does not yet offer a rotation control.

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
pose by 25 mm along X. It can retain the same displayed name, so use the ID
shown beneath each list entry to distinguish them. Editing one does not edit
the other; the copy does not inherit a stock allocation. Select board-row
checkboxes and use **Edit selected dimensions** to apply one local dimension
to several boards. The dialog shows **Mixed values** if their current values
differ, lists affected boards and individual anchors, and commits all valid
targets together or none if one is invalid.

## Keyboard-only walkthrough in the current shell

Opening a dialog moves focus to its first control. `Tab` (and `Shift+Tab` to
go back) stays inside the dialog until it closes. Press `Space` on a focused button or
checkbox. There are no special New board, duplicate, or edit shortcuts. The
following uses exact millimetre values so no rounding prompt appears; type
the unit suffix to make the input unambiguous. On macOS use `Command+A`, or
on Linux `Ctrl+A`, to replace all text in a focused input.

1. Run `cargo run --locked`. Tab to **New material** and press `Space`. In
   the dialog, **Material name** already has focus: type `Plywood 18`; Tab to
   **Thickness**, type `18 mm`. Leave **Default grain** at **Along length**.
   Press `Return` to create the material. `Escape` would cancel instead.
2. Tab to **New board**, press `Space`; **Board name** has focus: type `Side`.
   The first material is already selected. Tab to **Length**, type `2300 mm`;
   Tab to **Width**, type `600 mm`. Check the displayed effective material and
   18 mm thickness, then press `Return`. The **Project boards** list now has
   `Side`, `2300 × 600 × 18`, and **Unallocated**.
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
