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

Pin the bundled FGVTN Click 3D Slow Reta / H=0 kit `51MX153DRV00100` with plate `52MX15FG11003D` in the catalog, then add one installation per physical hinge. Select distinct door and mounting boards, their local minimum/maximum X reference edges and Z mounting faces, and Y centre positions measured from each board's minimum Y edge. Editing, deleting and catalog updates are undoable. Resizing a board leaves these coordinates unchanged and updates warnings in the installation list.

The documented door thickness is 15–22 mm. Supported cup-edge setback K / overlay R pairs are 3/15, 4/16, 5/17, 6/18 mm. The preview gives a Ø35 mm cup, 11.3 mm recess, with K measured to the **cup edge** (centre at K + 17.5 mm), and H=0 plate holes 32 mm apart at a 37 mm cabinet-front offset. Unsupported settings, thicknesses or out-of-bounds positions show warnings; these are provisional board-local references, not drilling instructions.

**Fastener drilling unavailable:** screw types, pilot diameters and depths, cup-screw locations, safe clearances and hinge count are not verified. Confirm the board orientation and installation at the shop. Source: [FGVTN General Catalog](https://www.fgvtn.com.br/site/novopdf/Catalogo_Geral.pdf), printed p. 23 (PDF p. 14), May 2025 revision reviewed in [source review](hinge-source-review.md). The project pins its catalog snapshot; refresh explicitly to recheck dependent installations.

The reviewed PDF has SHA-256 `e8aafa4f3656a108e8e91dd1681685455a4bf4f644cf01d4ecfa3fd80bf44df2` (catalog metadata modified 2026-09-16). Its drawing shows 48 mm between cup fixing-hole centres, **not** a complete drilling specification; do not infer pilot depth/diameter or screw choice. No verified hinge load rating, recommended quantity, structural fitness, accurate concealed-linkage path, or collision clearance is available. The on-screen and packet warnings remain at every preview angle. The manufacturer's PDF and artwork are not shipped with this app; only factual source-attributed dimensions and original annotations are included. Invalid installation references are omitted from numeric packet guidance without blocking otherwise valid wood cuts.
