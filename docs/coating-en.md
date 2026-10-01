# Coating and how sheets look

MDF, MDP and HDF sheets are sold **coated** (melamine, laminate) on both
faces, on one face, or raw. Plan My Cabinet keeps this on the material, and
the 3D view shows it.

## Coating belongs to the material

Set **Coating** (*None (raw)*, *One side*, *Both sides*) in the material's
dialog; it is offered for MDF, MDP and HDF. Because it is part of the
material, "MDF Branco 1 face" and "MDF Branco" are two materials: each has its
own sheets in Stock, and the cut plan only cuts a part from a sheet of its own
material. Files from before coatings get it from the material's name: "Cru"
or "Raw" is uncoated, "1 face", "uma face" or "one side" is one side, anything
else both sides.

## Which face is coated

On a material coated on one side, every board shows its coating where it is
seen. Automatically:

- boards across the cabinet (doors, fronts, backs) are coated toward the
  front;
- lying boards (bottoms, shelves, tops) are coated on top;
- boards along the cabinet (sides, dividers) are coated outside.

The board inspector's **Coating** section says which face is coated and why.
**Flip coated face** chooses the other face by hand; **Back to automatic**
returns to the rule. Each is one undo step, and works on several selected
boards at once. The face never changes which sheet a part is cut from.

## What the 3D view shows

- Coated faces: the material colour.
- Raw faces and edges without band: the sheet's core, with its texture: MDF
  fibre, MDP chips, HDF fibre, plywood plies or wood grain.
- Banded edges: the band's colour (see [Edge banding](banding-en.md)).

Plywood and solid wood faces show wood grain in the material colour. With
*Tint boards by material colour* off, every face is neutral.
