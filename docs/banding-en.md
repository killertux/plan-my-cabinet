# Edge banding

Edge band (edge tape) covers the exposed core of a board's edge. In Plan My
Cabinet only **MDF** and **MDP** boards take banding: plywood, HDF, solid wood
and other materials are left as they are, and the app never lets banding onto
them.

## Bands and materials

- **Edge bands** are listed in the Design outliner, below Materials. Each has
  a name, as your shop lists it ("Fita Branca 1x22"), a thickness, a height and
  a colour. New projects start with the common white and raw bands. Click a
  band to edit it; a band can be removed only when nothing uses it. The list
  shows how many metres of each band the design uses.
- **A material's type and default band** are set in its dialog. *Type* says
  what the sheet is made of (MDF, MDP, HDF, plywood, solid wood, other); files
  from before banding get it from the material's name. Only MDF and MDP offer a
  *Default edge band*: the band automatic banding uses.

## Automatic banding

Every edge is **automatic** until you change it. An automatic edge gets its
material's default band when it is **free**, and no band when another board
sits flat against it: at least half of the edge face covered, with a gap of at
most 0.5 mm. So the front edges of sides, bottom and shelves are banded; the
ends of a bottom that sits between the sides are not; a side's rear edge
against the back is not; doors and drawer fronts are banded all round.

Doors and drawers move, so their boards only join boards that move with them:
a closed drawer front does not hide the carcass edges behind it.

Automatic banding follows the design: move or resize a board and its edges
update. Cabinet templates in white MDF therefore come out banded.

## Changing banding

Select a board in Design. The inspector's **Edge banding** section shows the
board as a rectangle with its four edges:

- Banded edges are drawn in the band colour. **A** marks an automatic edge;
  a dashed outline means automatic, a solid one means set by hand. The front
  edge is marked *front*. Hover an edge to see why it is (or is not) banded.
- **Click an edge** to band it or take its band off. Right-click an edge and
  choose **Back to automatic** to undo your choice for that edge.
- **Band** chooses the band new edges get.
- **Automatic · None · Front · All 4** set all four edges at once.

Select several boards to set them together: each edge shows how many of the
selected boards are banded, and the presets apply to all of them. Boards that
take no banding are skipped and the app says so.

In the 3D view, banded edge faces take the band colour with a thin line along
them. The **Band edges** tool (in the tool strip) lets you click edges right in
the view: the edge under the pointer is outlined, a click flips it, and
Alt-click puts it back to automatic.

Every change is one undo step. Changing a board to a material that takes no
banding removes its banding in the same step, and a message says how many
edges lost their band.

## Sizes

Board sizes stay the **finished** sizes, band included. The shop removes the
band thickness when it cuts (CorteCloud does this itself).

## In the outputs

- The workshop PDF adds an **Edge banding** column to the parts list (for
  example "L1 L2 W1 · White band 1x22": L1 and L2 are the length edges, W1 and
  W2 the width edges; the Portuguese PDF writes C1 C2 L1 L2, as Brazilian cut
  lists do) and an **Edge band** table with the metres of each band,
  plus 10 % to buy.
- The CorteCloud file sends each band on its side of the part.
