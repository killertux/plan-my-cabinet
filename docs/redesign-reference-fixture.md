# Redesign reference fixture

Task 1.1's reusable entry point is
`plan_my_cabinet::reference_fixture::project() -> Project`, implemented in
`src/reference_fixture.rs`. It constructs independent, structurally validated
current-schema project data without disk access. Public UUID constants identify every
record (including `ALLOCATION_IDS` and `HINGE_IDS`). All UUIDs use the fixed
`72656465-7369-476e-8000-…` namespace. The board UUID suffixes preserve the
handoff's short labels where available. Capture setup should select `SHELF_ID`,
focus `WHITE_STOCK_ID`, and hide the `DOORS_ID` assembly for the Design reference;
Hardware can use `LEFT_JOINT_ID` and `HINGE_IDS[0]` with a 60° display preview.
Visibility and camera setup belong to the capture session.

## Reference authority and geometry

Sources are the chosen 2a scene's `boxes` in `Main Window.dc.html`, the closed
solids in `Hardware.dc.html`, the white layout in `Cut Plan.dc.html`, and the
four physical rows in `Stock and Materials.dc.html`, all under
`design_handoff_egui_redesign/designs/`. The fixture contains nine wood boards:
six white MDF 18 mm, two oak-veneer MDF 18 mm doors, and one HDF 3 mm back.

World X is cabinet width, Y is depth, and Z is height. All following coordinates
are millimetres; integration tests measure the actual transformed geometry.

| Board | Minimum world XYZ | Maximum world XYZ |
| --- | --- | --- |
| Left side | 0, 0, 0 | 18, 560, 720 |
| Right side | 782, 0, 0 | 800, 560, 720 |
| Bottom | 18, 0, 0 | 782, 560, 18 |
| Front rail | 18, 0, 702 | 782, 100, 720 |
| Back rail | 18, 460, 702 | 782, 560, 720 |
| Shelf | 18, 20, 350 | 782, 557, 368 |
| Back | 18, 557, 18 | 782, 560, 702 |
| Left door, closed | 0, -18, 2 | 397, 0, 718 |
| Right door, closed | 403, -18, 2 | 800, 0, 718 |

The carcass measures **800 × 560 × 720**; including closed doors gives
**800 × 578 × 720**. The shelf is **764 × 537 × 18** at **18, 20, 350**.
The back is **764 × 684 × 3**, inset into the actual handoff geometry.

**Local-axis correction:** hinge annotations use board-local Y for height.
Sides therefore have local blanks **560 × 720 × 18**, with local X along depth,
Y along height, and Z along thickness. Doors use **397 × 716 × 18**, with local
Y along height. Their explicit Width grain and quarter-turned allocations keep
the long/vertical grain aligned with sheet X. This is the same physical geometry
and grain as the handoff's sometimes transposed **720 × 560** / **716 × 397**
labels; the inspector must report the actual local-axis convention.

## Stock and witnessed layout

Global stock order is **S1, O1, S3, S2**, matching the Stock table. These are
persisted fixture stock aliases, separate from the stock names and priority;
the reference's S2/S3 order is intentional rather than renumbered by rank.

| Name | Material | Measured L × W × T (mm) | Ownership | Grain | Used parts |
| --- | --- | --- | --- | --- | ---: |
| S1 | White MDF | 2750 × 1830 × 18 | To purchase | Along X | 6 |
| O1 | White MDF | 900 × 600 × **18.2** | Owned | **Unknown** | 0 |
| S3 | White MDF | 2750 × 1830 × 18 | To purchase | Along X | 0 |
| S2 | Oak MDF | 2750 × 1830 × 18 | To purchase | Along X | 2 |

All edge trims are zero. Each to-purchase sheet costs BRL 289.90; the owned
offcut price is unknown. O1's measured thickness is intentionally preserved,
not rounded to its material's 18 mm default. S3 is an unused spare. There is no
HDF stock and the back remains unallocated.

S1 origins are left side `(0,0)`, right side `(725,0)`, bottom `(1450,0)`,
shelf `(0,565)`, front rail `(769,565)`, and back rail `(769,670)`.
S2's doors have 716 × 397 sheet footprints at `(0,0)` and `(721,0)`.
All eight allocations are unlocked. The inconsistent Design miniature that
places oak doors on the white sheet is corrected to these material-compatible
allocations.

The existing `reconstruct_witness` solver with the normal **20,000-state budget**
and **5 mm kerf** proves every allocated sheet. `validate_witness` independently
checks each returned tree, grain, allocations and conserved area. Display cuts,
kerf bands and offcut rectangles from that returned tree, including its own
operation order and piece IDs; the illustrated sequence is not a stored proof.

| Sheet | Part area (m²) | Utilization | Physical cuts | Recoverable area (m²) | Kerf loss (m²) |
| --- | ---: | ---: | ---: | ---: | ---: |
| S1 | 1.797308 | **35.714019%** | **9** | **3.185262** | **0.049930** |
| O1, unused | — | — | — | 0.540000, whole piece | — |
| S3, unused | — | — | — | 5.032500, whole piece | — |
| S2 | 0.568504 | **11.296652%** | **4** | **4.438536** | **0.025460** |

Unused sheets have known physical area but no current cutting witness; their
utilization, cut count and loss are not represented as verified zeroes. Trim
loss and non-kerf waste in the **used** witnesses are zero. Each full sheet
has 5.032500 m² area.
The Design miniature's **71%** becomes **36%** rounded; S2 rounds to **11%**.
Total physical cuts are **13**, correcting the illustrative **12**. These
numbers are regression expectations for the current solver's real witness,
not a request to force its search order or claim an optimal cut sequence.

Used purchased stock totals **BRL 579.80**. Owned pieces consumed: **0**.
The cut fee and cutting charge are **unknown**, and the total estimate is
**incomplete**, additionally because the back still needs stock. Do not charge
the unused spare or display the material subtotal as a complete total.

## Hardware and readiness

Four `HingeInstallation` records reference the bundled verified FGVTN kit
`51MX153DRV00100` / plate `52MX15FG11003D`; the project pins the real bundled
source revision, hash and printed page 23 / PDF page 14. Two `DoorJoint`
relationships are derived by the existing preview API. There are no duplicate
generic hardware records for those same four physical hinges.

All hinges use the supported **K=4 / R=16** pair, Ø35 × 11.3 cup, 32 mm plate
pitch and 37 mm front offset. H1/H2 door-local Y values are 100/616 mm, with
independent mounting Y values 102/618 mm to account for the door's 2 mm rise.
H3 uses 100/102. H4 uses door Y **704** (12 mm from the 716 mm door's upper edge)
and independent mount Y **700**, reproducing the real **CupOutsideDoor** warning
without introducing a second plate-outside-mount warning. The fixture's
bottom-origin Y convention corrects the handoff's reversed top/bottom labels;
H1 is at world Z 102 and H2 at Z 618. The warned H4 relationship needs review
and cannot preview motion. The valid left relationship supports the 60° preview
with its actual 105° limit. Display preview leaves serialized data unchanged.
Fastener drilling remains unavailable in the verified source data.

Diagnostics contain exactly **eight allocated-valid boards and one unallocated
back**, plus the separate **one hardware cup warning**. Draft export prepares;
Shop-ready export is blocked by the back. Kerf is explicitly confirmed at
5 mm in this synthetic fixture, with no invented confirmation date. The fixture
has no receipts, recovery history, recent paths, or invented timestamps. Colors
remain absent under the current domain schema; white/oak/HDF are real material
identities and names ready for the later appearance work.

## Verification

Run `cargo test --locked --test redesign_reference`. Four tests cover stable
construction/serialized bytes and persistence, every board's world bounds,
real sheet witnesses/accounting/costs/readiness, and pinned hardware/relationships
including the warned hinge and read-only motion. Manufacturing quantities and
IDs round-trip exactly. The existing JSON parser may change derived floating
axis coordinates by one ULP; round-trip axis comparison uses a 1e-9 tolerance
and does not alter the domain or serializer. Repeated fresh construction is
byte-for-byte deterministic.
