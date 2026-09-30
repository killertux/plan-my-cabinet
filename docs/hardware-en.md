# Hinge installation references

## Hardware workspace

The Hardware panel starts with **Add hardware ▾** and then has one section per
kind. The sections are always shown, even when empty, and each has its own
**+**:

| Section | Lists | **+** adds |
|---|---|---|
| **Doors & hinges** | Each door with its hinges, then hinges that are not on a door | A door (on the selected board) or a single hinge |
| **Drawers & slides** | One row per drawer with its slide code | Slides on the selected drawer |
| **Feet & legs** | Catalog feet | A foot under the selected cabinet |
| **Other hardware** | Sized boxes for handles, rails and the like | A 100 mm box you can resize and move |
| **Catalog models** | Every model pinned to the project: hinges, slides and feet, with how many items use each | Opens the catalog browser |

Adding creates the item at once, as one undo step, and opens it in the
inspector so you can adjust it:

- **Foot.** It uses the last foot model you used, goes under the selected
  cabinet 20 mm in from its corner, and hangs its height below the floor. The
  inspector offers **Raise <cabinet> by N mm**.
- **Slides.** They go on the drawer of the selection, with the last slide
  family you used (TT45 by default). If that drawer already has slides, they
  are opened instead.
- **Door.** The selected board is hung from the nearest cabinet side, using its
  loose hinges or a standard set.
- **Hinge.** It joins the selected board's door.

A picker opens only when something must be chosen first: for example, adding
slides with nothing selected, or a foot when no foot model is available.

Every item opens in an editable inspector, both here and in **Design**. Click
its row, its link under a board or group in Design, or (for slides and hinges)
the item itself in the 3D view. The inspectors are:

| Item | What you can change |
|---|---|
| Feet and other hardware | Model, parent group, world position and rotation. Other hardware also has its dimensions. |
| Slides | Model and length, which apply at once; height and setback; **Refit slides** |
| Hinges | Mounting inspector below |
| Doors | Moving part, the board it hangs from, which hinges it uses, opening limit and **Reconfirm**. **+** adds another hinge. |
| Catalog models | Facts, source and where the model is used. **Remove model** works only when nothing uses it. |

Typed values follow the usual rule: **Enter** or **Apply** saves, **Escape**
or **Discard** reverts, and clicking elsewhere never saves. Leaving an item
with unsaved values asks first.

A warning follows the affected installation, not another hinge with a
similar name. Removing a referenced board or hardware object requires the
affected-relationship confirmation.

Selecting an installation connects its tree row, mounting inspector and
projected viewport reference. The dashed axis, cup/plate markers and connector
follow board-local references and any *display-only* open-door pose. They are
not solid machining geometry; invalid or missing evidence is flagged and does
not become a verified drilling location. The inspector retains separate door
and mounting-board Y coordinates, edges, faces and K/R settings, even when a
quick preset is available. The fixed-axis door slider displays the actual
supported opening endpoint and tick labels from the pinned verified catalog;
the bundled reviewed kit currently supports 105°. Its angle HUD is labelled
**Display only**. **Exit preview** closes the door, and successfully leaving
Hardware also restores the saved closed appearance without a project edit.
An unfinished relationship or installation form blocks starting motion until
you explicitly resolve it; invalid references disable motion rather than
inventing a path.

## Door relationships

### Opening direction and older projects

Positive preview angles open the free edge away from the selected cup face.
Both the X hinge edge and Z cup face determine the direction, so mirrored
doors do not share one global rotation sign. This also applies to rotated,
nested assemblies; the stationary board and saved closed poses stay fixed.

Older relationships may have the former axis direction. Opening a file does
not rewrite its bytes or change that stored axis. An affected relationship
shows **Relationship needs review** and cannot start motion. Under the door's
**Door actions**, choose **Edit door relationship**, check its boards, hinges
and proposed axis, then confirm. Cancel preserves the old relationship;
confirmation is one undoable edit. Undo restores the old axis and review
requirement. Relationships whose direction already agrees remain usable.
This is a handedness correction, not a verified hinge path or collision check.

Save/reopen preserves floating-point poses and axes exactly, so an unchanged
valid relationship does not require review merely because it was reopened.
Earlier builds could round a derived axis while reading JSON. If that rounded
value was subsequently saved, it is still preserved on open: review and explicitly
confirm the relationship as above rather than silently repairing stored data.
Real installation warnings continue to block motion independently of this fix.

The selected door's floating **Preview opening / Closed** controls start at
0° or exit the disposable preview. The bottom card adjusts the angle;
**Closed** restores the saved appearance without editing the project.
Installation **Edit** and **Delete** are in the inspector and row context menu.
Expand **Full datums, supported pairs and source** for the complete references;
reference hardware and additional object actions are under **Advanced view and language**.

After installing the hinges, choose **Add door relationship**. Select a moving board or assembly root, a stationary mounting board and compatible existing installations. The preview lists the moving subtree (including nested handles), fixed mount, closed-pose axis and installation warnings. Confirm creates one undoable relationship without moving the closed design. Edit changes its hinge selection; deletion removes the relationship in one undo step. Cycles, self-mounts and duplicate moving roots or hinge assignments are rejected. Select a board or assembly and use **Delete selected board / assembly** to review dependent allocations, installations and relationships before removing the subtree in one undoable action. Cancel leaves the project unchanged. Review warnings after board/hinge changes; the reference axis is not verified clearance or drilling guidance.

Pin the bundled FGVTN Click 3D Slow Reta / H=0 kit `51MX153DRV00100` with plate `52MX15FG11003D` in the catalog, then add one installation per physical hinge. Select distinct door and mounting boards. A new hinge starts on the selected board, and the cabinet side is picked for you: the nearest board at right angles to the door, preferring the door's long edges. Its position defaults to the next free standard spot (100 mm from each end, then between). With **Line up with the cabinet automatically** on (the default), the app works out the door's hinge edge, the cup face, the side's front edge and plate face, and the plate position from where the two boards sit in the model, so the plate always meets the cup at the same height. Turn it off, or when the boards aren't parallel, to pick the edges (X-/X+ run along the board's Y, Y-/Y+ along its X), faces and plate position yourself. Positions are measured along the hinge edge from its minimum end.

In the hinge inspector, changing **Position** moves the cup and brings the plate with it. **Line up plate with cup** appears when the plate is out of line with the cup (for example after moving a board), and **Space hinges evenly** puts all hinges on the door 100 mm from each end and the rest evenly between, in one undo step. The inspector also says how many hinges a door of that size usually takes (2 up to 900 mm, 3 up to 1500 mm, 4 up to 2000 mm, then 5). Sliding hinges along the door doesn't ask you to review the door relationship again.

Editing, deleting and catalog updates are undoable. Resizing a board leaves these coordinates unchanged and updates warnings in the installation list.

The documented door thickness is 15–22 mm. Supported cup-edge setback K / overlay R pairs are 3/15, 4/16, 5/17, 6/18 mm. The preview gives a Ø35 mm cup, 11.3 mm recess, with K measured to the **cup edge** (centre at K + 17.5 mm), and H=0 plate holes 32 mm apart at a 37 mm cabinet-front offset. Unsupported settings, thicknesses or out-of-bounds positions show warnings; these are provisional board-local references, not drilling instructions.

**Fastener drilling unavailable:** screw types, pilot diameters and depths, cup-screw locations, safe clearances and hinge count are not verified. Confirm the board orientation and installation at the shop. Source: [FGVTN General Catalog](https://www.fgvtn.com.br/site/novopdf/Catalogo_Geral.pdf), printed p. 23 (PDF p. 14), May 2025 revision reviewed in [source review](hinge-source-review.md). The project pins its catalog snapshot; refresh explicitly to recheck dependent installations.

The reviewed PDF has SHA-256 `e8aafa4f3656a108e8e91dd1681685455a4bf4f644cf01d4ecfa3fd80bf44df2` (catalog metadata modified 2026-09-16). Its drawing shows 48 mm between cup fixing-hole centres, **not** a complete drilling specification; do not infer pilot depth/diameter or screw choice. No verified hinge load rating, recommended quantity, structural fitness, accurate concealed-linkage path, or collision clearance is available. The on-screen and packet warnings remain at every preview angle. The manufacturer's PDF and artwork are not shipped with this app; only factual source-attributed dimensions and original annotations are included. Invalid installation references are omitted from numeric packet guidance without blocking otherwise valid wood cuts.

## Catalog packs

Hinge data comes from **catalog packs**: one TOML file per manufacturer, with
the facts of each hinge and the source they were read from. **Add hinge from
catalog…** opens the catalog browser:

- **Packs** lists every pack with a status: **Reviewed** (bundled with the app
  and checked against the cited sheets), **User data** (your own file),
  **Draft**, or **Errors**. A pack with errors is listed with every problem
  and its place in the file, but offers nothing to add.
- **Hinge** and **Arm** choose a family and its full overlay (Reta), half
  overlay (Curva) or inset (Alta) variant. The facts show the product and
  plate codes, cup, door range, plate height H, plate hole pitch and front
  offset, opening angle and source.
- **Test bench** runs the same installation checks as a project on a sample
  door: pick a K pair from the table, type the door and side thickness (and
  E for inset) and see whether it fits, where the cup and plate centres fall,
  or which check fails.
- **Add to project** pins the chosen variant into the project. The project
  keeps its own copy: editing or deleting the pack later never changes a
  saved project. **Update from catalog** replaces a pinned record explicitly
  (one undoable edit) and rechecks its installations.

Records from your own packs give full measurements. The card, inspector and
PDF label them **User data** and name the pack; check them against the
manufacturer's sheet before drilling. A pinned record whose numbers do not
agree with each other (for example a cup deeper than the thinnest door, or a
K table out of order) gives no numeric guidance.

### Inset hinges (Alta)

An inset door sits between the cabinet sides. Its table gives the gap **F**
between door edge and side for each K, instead of an overlay R. The hinge
dialog asks for **E**: the distance from the side's front edge to the door's
inside face (the door thickness when the door is flush with the front). The
plate sits at the front offset plus E (for example 37 + 18 = 55 mm). An E
smaller than the door thickness is flagged, because the door would stand
proud of the front. Door motion previews an inset door about its outside
front edge.

### Writing your own pack

Put `.toml` files in your catalog folder (**Open folder** in the browser;
on macOS `~/Library/Application Support/Plan My Cabinet/catalogs`) or use
**Import pack…**, then **Reload**. Lengths are millimetres; decimals such as
`9.8` are exact. The bundled `catalogs/fgvtn.toml` is a complete example.

```toml
schema = 1
id = "acme"                    # lowercase letters, digits and '-'
manufacturer = "Acme"
version = "2026-10-01"         # change it whenever the data changes
review = { status = "draft" }  # or "reviewed"

[[sources]]
id = "acme-sheet"
title = "Acme hinge sheet"
url = "https://example.com/hinge.pdf"
sha256 = "…64 hex digits of the PDF you read…"
revision = "Oct 2026"
printed_page = 12              # optional
pdf_page = 1                   # optional

[[hinges]]
id = "soft-110"
name = { en = "Acme soft-close 110°", pt-BR = "Acme amortecida 110°" }
source = "acme-sheet"
soft_close = true
mounting = "clip"              # clip | slide_on | fixed_plate (optional)
opening_degrees = 110          # optional; without it door motion is unavailable
cup = { diameter = 35, depth = 11.5 }
door_thickness = { min = 16, max = 22 }
plate = { front_offset = 37, hole_pitch = 32 }   # hole_pitch optional
fasteners = "Ø4×16 screws"     # optional text
notes = "Anything else from the sheet"            # optional
# allow = [{ warning = "not-monotonic", reason = "As printed" }]

  [[hinges.variants]]
  arm = "full_overlay"         # full_overlay | half_overlay | inset
  code = "AC-110-FO"
  plate_code = "AC-PL-0"       # optional
  plate_height = 0
  k_table = [[3, 14], [4, 15], [5, 16]]   # [K, R]; for inset [K, F]
```

The loader reports every problem at once. Errors (the pack cannot be used)
include unknown fields, invalid ids, duplicate codes, lengths finer than
1 µm, a thickness range the wrong way round, a cup as deep as the thinnest
door, and K values that do not increase. Warnings (the pack stays usable)
include uneven R/F steps, a missing opening angle, a draft pack, and a user
pack that replaces a bundled one with the same id. A warning can be accepted
for one hinge with `allow`, but only with a reason.

To check a pack without opening the app, run
`plan-my-cabinet --check-catalog my-pack.toml`. It prints each problem with
its place in the file and exits non-zero on errors.

# Drawer slides

A drawer slide pair joins a drawer to the two cabinet sides beside it. The
drawer is a group of boards (an assembly): the **Drawers** template makes one
group per drawer; for a drawer you built yourself, group its boards first.

Select a drawer (or one of its boards) and press **+** in **Drawers &
slides**. The slides are added at once and open in the inspector, where the
model, length, height and setback can be changed. With nothing selected, the
slide dialog opens: pick the drawer and a slide model. The app finds the box sides
and the cabinet sides next to them, measures the gaps, and picks the longest
length that fits. It shows the product code and whether it fits. You can
choose a length, the height on the box side (centred by default) and the
setback from the cabinet front (2 mm by default).

The checks are:

- **Side clearance.** The gap between each box side and cabinet side must be
  within the slide's clearance. For most slides this is 12.7 mm, +0.5/−0, so
  the box is 25.4 mm narrower than the opening. The TT90 needs 19 ±0.3 mm.
- **Depth.** Setback plus slide length must fit the cabinet side, and the
  drawer member must fit on the box side. A 500 mm slide goes on a 500 mm box.
- **Height.** The slide must fit on the box side and on the cabinet side.
- **Alignment.** Both slides must be at the same height and depth.

The **Drawer slides** list in the Hardware panel shows each drawer with its
product code. The inspector lists the gaps and, for each side, the hole
distances from the front edge and the height of the slide's centre line. Hole
positions come from the manufacturer's sheet; check them before drilling.

**Preview** pulls the drawer out on its slides (a slider in millimetres, up
to the slide's travel). It is display only, like the door preview.

The **Drawers** template installs slides on every drawer. Choose the slide
model in the setup (default FGVTN TT45 Slowmotion) or **None**. With a
slide, the side clearance comes from the slide and the side clearance field
is ignored.

The bundled `catalogs/fgvtn-slides.toml` has these FGVTN / TN full-extension
soft-close slides, from the manufacturer's sheets
(see [the review](catalogs/fgvtn-slides-review.md)):

| Model | Lengths | Load | Height | Side clearance |
|---|---|---|---|---|
| TT45 Slowmotion (0073.045500SX …) | 350–550 mm | 45 kg | 45 mm | 12.7 +0.5/−0 |
| TT44 Slowmotion (zinc, white, black) | 350–550 mm | 35 kg | 45 mm | 12.7 +0.5/−0 |
| TT35 Slowmotion | 250–550 mm | 25 kg | 35 mm | 12.7 +0.5/−0 |
| TN H45 Slow | 250–550 mm | 35 kg | 45 mm | 12.7 +0.5/−0 |
| TT90 Slow (heavy duty) | 450–600 mm | 90 kg | 52 mm | 19 ±0.3 |

The shop PDF lists the slides to buy ("0073.045500SX … — 3 pairs") and, for
each drawer, the hole distances on both boards of each side.

# Feet

Feet are catalog hardware. Select the cabinet and press **+** in **Feet &
legs**: the foot goes under it, 20 mm in from the corner, and opens in the
inspector. There you can change the model (the mounting face stays where it
is), the group it belongs to and its world position. You can also drag it in
the 3D view with **Move board**. The position is the corner of the foot's box on the
floor; the mounting face is on top. Feet are not cut from stock and they do
not lift the furniture. While a foot reaches below the floor, the inspector
offers **Raise <cabinet> by N mm**.

Feet are drawn with the product's shape, so you can see how the piece will
look: a plastic cone, a chrome post with its flange and levelling glide, an
industrial tube frame. The bundled `catalogs/generic-feet.toml` has typical
sizes of common products. These are **generic reference dimensions**, not a
manufacturer's sheet, and they are labelled that way on screen and in the PDF:

| Model | Size |
|---|---|
| Plastic tapered foot 40 mm (black, white) | Ø50 → Ø30 × 40 mm, one screw |
| Square chrome adjustable foot 60/100/120/150/200 mm | 60 × 60 flange, 32 × 32 tube, Ø38 glide, 10 mm levelling |
| Round chrome adjustable foot 80/100 mm | Ø60 flange, Ø32 tube, Ø38 glide |
| Industrial frame leg 750 × 500 | closed frame of 30 × 30 tube |
| Reinforced industrial frame leg 750 × 600 | closed frame of 50 × 30 tube |
| Industrial trapezoid leg 710 × 500 | 500 at the top, 400 at the floor |
| Straight table leg 710 mm | 40 × 40 tube, 100 × 100 plate, 30 mm levelling |

The shop PDF lists the feet to buy with their size and finish.

## Writing slide and foot models

Slides and feet go in catalog packs like hinges, in `[[drawer_slides]]` and
`[[feet]]` tables. A pack may hold any mix of hinges, slides and feet.

```toml
[[drawer_slides]]
id = "acme-45"
name = { en = "Acme 45 full-extension slide", pt-BR = "Corrediça Acme 45" }
source = "acme-sheet"          # required in a reviewed pack
height = 45
clearance = { nominal = 12.7, minus = 0, plus = 0.5 }
front_setback = 2              # slide front behind the cabinet front
extension = "full"             # full | partial | over
soft_close = true
capacity_kg = 45
rear_fixing = "Screw at the rear end"   # optional text

  [[drawer_slides.variants]]
  code = "A45-500"
  length = 500                 # the closed cabinet member
  travel = 500                 # how far it opens
  cabinet_holes = [35, 51, 99, 259, 275]   # mm from the member's front end
  drawer_holes = [32, 48, 57.5, { along = 240, offset = 8, diameter = 4.5 }]

[[feet]]
id = "cone-40"
name = { en = "Plastic cone 40 mm" }
shape = { kind = "tapered", height = 40, top = { diameter = 50 }, bottom = { diameter = 30 } }
color = "#1e1e1e"
mounting_holes = [[0, 0]]      # X, Y from the centre of the mounting face

  [[feet.variants]]
  code = "CONE-40-BLK"

  [[feet.variants]]
  code = "CONE-40-WHT"
  color = "#eeeeea"            # optional overrides: color, finish, height
```

Foot shapes:

- `tapered`: a solid cone or pyramid. `top` and `bottom` are sections
  (`{ diameter = … }` or `{ width = …, depth = … }`) and `height`.
- `post`: a tube with a top plate and an optional glide: `tube`, `plate`,
  `plate_thickness`, `glide = { diameter, height }`, `height`.
- `frame`: a closed tube frame: `top_width`, `bottom_width` (smaller for a
  trapezoid, equal to `tube_width` for a V), `tube_width`, `tube_depth`,
  optional `crossbar_height` and `glide`, `height`.

Packs with `review = { status = "generic" }` hold typical sizes without a
manufacturer's sheet (no `[[sources]]` needed). Models created by an AI agent
with `save_to_catalog` are written to `user-models.toml` in your catalog
folder; use **Reload** to see them.
