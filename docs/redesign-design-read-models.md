# Design presentation projection

`DesignReadModel::build(project, &stock_snapshot, DesignView { selected, active, hidden, expanded })` produces immutable outliner rows, material and stock summaries, and a typed inspector (`None`, `Board`, `Assembly`, `Hardware`, or `Multi`). It is headless: UI code supplies `viewport::Selection`'s ID sets and `WorkspaceSession::design_expanded`. The session initializes assemblies expanded, retains expansion on workspace switches, prunes removed assembly IDs, and resets on project replacement. Expansion never hides rows from the projection; the UI can filter descendants by their ancestors' `expanded` flags while preserving ID search and reveal.

Build `StockReadModel` once after relevant manufacturing/pricing changes and reuse it for Design view-only changes. Its project-wide diagnostics and witness-backed stock information remain the source of truth. This projection does **not** reconstruct cutting witnesses, calculate utilization or infer a missing proof; the UI may look up a board's `stock_id` in the **same** stock snapshot for its miniature and verified utilization. Keep the snapshot paired with its originating project/revision; the projection accepts the cached snapshot rather than recomputing it. The stock row's `part_count` is the number of actual allocation records, not a feasibility claim.

Outliner rows carry UUID, parent, depth, direct and inherited visibility, active/secondary selection, expansion, and a diagnostic. Assembly issue counts include all descendant boards even when hidden or collapsed. Visibility is transient scene state; warnings must not replace the row's reveal/toggle target. Equal names never identify an object. A board inspector includes exact `Length` values for local X/Y/Z blank dimensions and current material default separately; `MatchesCurrentMaterialDefault` denotes equality only, **not** proof that the board dynamically follows material edits. A different stored thickness is reported as `StoredBoardValueDiffers`. Grain is effective board grain plus material/override provenance. Allocation references preserve UUID, stock UUID/alias (when present), exact origin and lock/rotation state; no allocation is represented by an empty list and the global diagnostic. Structural validation rejects dangling material/allocation references before projection.

Local pose is explicitly labelled world (no parent) or parent-assembly frame. Bounds are from `measurements::measure`, with explicit Body/Overall scope and World/Object frame; unavailable or undimensioned hardware remains a `MeasurementError`, not a numeric estimate. Multi-selection includes mixed boards, assemblies and hardware, and the measurement service deduplicates selected descendants. The board-local size and frame-aware bounding size serve different purposes.

At the original read-model checkpoint, UI wiring still needed the 2a tree,
controls, summaries, inspectors and action routes. The checkpoint below records
the subsequent integration and its remaining gaps; task 6.4 is still open.

## Task 6.4 native wiring checkpoint (2026-09-26)

The Design controls now consume `DesignReadModel` for a nested UUID-based tree,
independent assembly expansion/visibility/selection, descendant warning counts,
and material and stock summaries. The inspector has separate empty, board,
assembly, mixed-selection and hardware branches. Board values distinguish the
stored thickness from the current material default (equality does not mean
following it), grain and its override source, local pose frame, Body/Overall
World/Object bounds, allocation origin/lock/rotation and stock identity. It
does not substitute a board's editable local dimensions for bounding extents.

`DesktopApp::design_stock_snapshot` pairs the witness-backed stock projection
with project UUID/revision, rebuilds after an edit, and is discarded on document
replacement even when UUID/revision happen to match. Visibility, expansion and
selection reproject without rebuilding witnesses. The controls use the same
scrollable pane at 256 pt, and the 292 pt inspector reopens as a compact drawer.
The Design viewport now uses the remaining canvas height.

The action dispatcher remains the authority for selection, visibility,
group/reparent/ungroup/duplicate-assembly/transform, batch dimensions, material
preserve/apply dialog, board dimensions/pose/grain/face placement, duplicate,
allocation navigation and guarded delete. Hierarchy actions are in a collapsed
section; existing advanced board controls remain below the summaries. Material
editing uses the original guarded dialog. Tree rows identify objects by UUID
even with identical names; warning counts survive hidden/collapsed descendants.

**Sidebar/inspector follow-up (2026-09-26):** The 256 pt sidebar now uses
~26 pt outliner rows with bundled assembly/board/eye/warning/chevron icons,
distinct active/secondary fills and independent warning and visibility targets.
The button names and hover detail include UUIDs; selecting remains keyboard
operable and Shift/Command additive. Material rows have actual stored sRGB
swatches, current default thickness and board counts, with stock/unallocated
counts in hover detail. Stock shows the immutable alias, actual global priority,
measured three-axis size and explicit owned/purchase source. The fixture has
**four** pieces (S1, O1, S3, S2), not the three in the illustration; all four
and Add sheet are visible at 1440 × 900 without scrolling. The 292 pt board
inspector groups material/grain, local dimensions with independent stored
thickness provenance, local frame/position, allocation and Body/World bounds;
advanced pose/bounds and action routes remain in its separately identified
disclosure. Dimensioned placeholder hardware exposes guarded Edit and Duplicate
here as well as in Hardware. Focused tests check fixture geometry/data and
hardware target/modal guards; the native compact pt-BR capture confirms the
sidebar still scrolls and the inspector drawer remains reachable at 900 × 650
and 130%.

**Still open:** dimensions/pose edits still open modal forms rather than shared
inspector/HUD drafts; the selection HUD, projected dimension pill, miniature
sheet and direct stock preview in the inspector are not present. The 2D stock
sheets button navigates to the board's allocation or issue. The stock summary
reports actual allocation-record counts, not witness utilization. Catalog
hardware remains read-only here and routes to Hardware for specialized editing.
At compact sizes the pane scrolls. The current tree still shows the fixture
doors expanded and has no separate collapsed Hinges group. No 2-point R01
fidelity claim is made.
