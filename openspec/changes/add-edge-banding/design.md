# Design

## Model

`Board.banding` holds one `EdgeBanding` per edge (`Auto`, `On(band)`, `Off`),
omitted from files while all automatic. The effective band of an edge is
derived (`banding_rules::effective`) so it follows geometry without edits.
Validation refuses `On` on a material that takes no banding; every
transaction first drops such banding, so no edit path can leave it behind.

## The automatic rule

For each edge face (edge length × thickness, from the board frame), boards
square to this one are boxed in its local frame. A board joins the edge when
its box reaches the thin slab outside the face (gap ≤ 0.5 mm) without being
buried in the board, and covers at least 50 % of the face. Boards moving with
a door joint or a slide installation only join boards of the same moving part.
The rule is exact for square boards and ignores boards at an odd angle.

## Sizes

Board dimensions remain finished sizes, band included. Cut size deduction is
the shop's (CorteCloud deducts it itself), so the optimiser and PDF sizes do
not change.

## UI

The inspector's edge diagram flips the effective state of an edge as a manual
override and collapses back to automatic when the override equals the rule.
The Band tool reuses the viewport's face hit: an edge face, or the broad face
within 25 mm of an edge, picks that edge.
