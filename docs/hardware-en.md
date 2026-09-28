# Hinge installation references

## Hardware workspace

The pinned catalog card shows the project's saved kit and plate IDs, reviewed
revision and source; **Browse snapshots** examines the project's own records
offline without refreshing them. The relationship tree lists each door with
its stationary board and child hinge installations. Unassigned installations
and dimensioned reference hardware remain available separately for selection,
editing, positioning, duplication and removal. A warning follows the affected
installation, not another hinge with a similar name. Removing a referenced
board or hardware object requires the affected-relationship confirmation.

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
