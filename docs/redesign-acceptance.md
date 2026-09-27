# Desktop redesign acceptance

### Review ownership

**User batch 1 results:** user reported launch and legacy upgrade Cancel/accept
working as expected (upgrade was observed by the user; no screenshot retained).
User screenshots 1–3 show the populated Drawers Design and Stock at wide/compact
sizes, with linked selection and the compact Inspector entry. User reports
Stock scrolling/edit-cancel and kerf acknowledgement/cancel/confirm passing;
screenshot 4 shows the acknowledged kerf dialog. This closes those individual
interaction checks, not reference-composition acceptance. The user subsequently
explicitly confirmed Save As → quit → reopen (step 6) also passed.
Screenshot review found duplicate `mm` in the kerf label and Design's off-by-one
stock ranks; both are corrected with rendered-label regressions. Design HUD
value clipping and the previously recorded reference differences remain open.

**User batch 2 results:** all four steps reported passing: invalid Design
dimension → navigation prompt with Apply disabled → Stay preserves draft →
Discard permits navigation without the edit; O1/Left side selection and repair
Stay/cancellation preserve placement; multi-page draft PDF export/review with
shop-ready still blocked by unallocated boards; overwrite cancellation preserves
the earlier PDF without an additional successful receipt. Attached screenshot 1
shows the Unfinished edit prompt, focused Stay and disabled Apply. Screenshot 2
shows the selected 720 × 524 mm Left side at origin on O1, two numbered cuts and
1715 × 1220 / 720 × 691 mm offcuts in repair mode. PDF behavior is user-reported;
no Batch 2 PDF was attached for independent inspection. These are interaction
passes, not full Cut plan visual-reference acceptance or optimizer acceptance.

**User batch 3 results:** user reports 3.2 keyboard-only New board (invalid
Enter, Tab/Shift-Tab containment and dropdown/parent Escape handling), 3.3 modal
background scroll/drag isolation, and 3.4 Portuguese/130% preference persistence
passing. The attached image shows the localized Nova peça dialog, visible
Material focus, comma-decimal thickness, and disabled Criar peça with incomplete
inputs. For 3.1, five screenshot filenames were sent as text, but the workspace
images were initially not attached. The user subsequently attached all five
workspace images. Review found status-bar labels overlapping at 130% across
all workspaces, clipped Cut plan repair actions/optimizer heading, a Handoff
page wider than its visible viewport, clipped Design HUD values, and misleading
Hardware missing-selection guidance when there are zero installations. These
make 3.1 a layout failure requiring correction/retest, not an interaction failure
in 3.2–3.4. The English PDF is expected because export language is independent.

Follow-up fixes use measured localized status text to select the More menu,
wrapping for issue actions/headings, bounded control-scroll content, wider HUD
numeric fields with localized thickness, truthful empty Hardware guidance, and
default whole-page fitting with an explicit Fit page control after manual zoom.
Automated regressions cover status overflow, empty Hardware and fitted A4 bounds
at small sizes. The follow-up screenshot results below supersede the pending
status for those individual fixes; other recorded reference differences remain open.

The follow-up `dist/manual-review-2/Plan My Cabinet.app` built successfully.
Full `cargo test --locked`, warnings-denied all-target Clippy, formatting,
diff whitespace checks and strict change validation passed. Full test log:
`…/opencode/batch3-full-tests.log`. This package includes the Batch 1 label/rank
fixes and Batch 3 responsive fixes. Reuse
`dist/manual-review/test-home` to retain the user's test preferences and recents.

**User batch 3 follow-up (`manual-review-2`):** six screenshots (17:15:21,
17:15:43, 17:15:54, 17:16:16, 17:16:25 and 17:17:10) show the status bar without
overlap, fully visible Cut plan repair buttons, a fitted Handoff page and the
correct empty Hardware message. They also expose a Handoff count-card regression
(hinge label squeezed into a vertical column) and continued inspector/optimizer
text clipping. No board is selected, so the dimension HUD fix is not yet verified.
The user explicitly reports that clicking empty Design canvas and pressing **F**
centres the whole cabinet correctly; this interaction passes and does not need
another retest. Manual camera position is not treated as a framing defect.

Count cards now allocate columns before laying out localized labels, with a
stacked fallback when complete words cannot fit. Docked and drawer inspectors
share bounded wrapping inside their scroll containers; wide tables retain
horizontal scrolling. Measurement choices stack and optimizer actions wrap.
New headless regressions check count-label word wrapping and painted bounds at
180–320 points, and the optimizer heading after a populated sheet inspector at
240–320 points, in English and Portuguese. Native confirmation is pending.

The `dist/manual-review-3/Plan My Cabinet.app` follow-up package built successfully.
Formatting, all-target check, warnings-denied all-target Clippy, full Rust tests,
diff whitespace and strict OpenSpec validation passed; the full test log is
`…/opencode/batch3-cards-full-tests.log`. Bundle plist and deep/strict signature
verification passed. This is automated/package evidence, not native acceptance
of the new card/wrapping layouts. The previous packages and isolated HOME remain
untouched. Overall OpenSpec progress stays at 77/87 pending full task acceptance.

**User `manual-review-3` screenshot retest:** all three targeted layout checks
pass at Portuguese/130%. Selected Left side shows complete HUD values `720,00`
and `524,00` with `18,00 mm` thickness. Cut plan shows the optimizer heading
wrapped within the inspector and both Search/Cancel controls fully visible.
Handoff shows readable count cards (23 parts, 3 stock pieces, 0 installations),
four-page navigation and a fitted full page. This is screenshot evidence, not
an assertion that the search was run or all reference geometry now matches.
The remaining stretched letter spacing in the count headings comes from egui
column justification. A source-only follow-up uses non-justified card contents;
the bilingual narrow-card regression now also rejects justified text and passes.
It is not yet in `manual-review-3`; no rebuild is required for the next Hardware
interaction batch. Other reference-composition differences remain open.

For subsequent Hardware acceptance, create a disposable, current-schema fixture:

```sh
cargo run --locked --example write_reference_fixture -- \
  "dist/manual-review/test-data/Hardware review.pmcab"
```

This uses the same deterministic nine-board cabinet, four pinned installations
and two stored relationships as the capture tests. The left relationship is
previewable; the right remains blocked by the intentionally out-of-bounds
upper cup. It refuses
existing destinations, uses the normal serializer/load validator/save path, and
does not read or change app preferences. The unallocated back and approximate,
non-certified fixed-axis motion remain intentional. No manual hinge setup is needed.
The fixture was generated successfully at the path above. A second writer run
refused the existing destination and preserved its SHA-256
`a04881d8cdff979e56cb3671261d114b08ee88994a234f3bc83152ca75458056`.

**Hardware batch blocker found by the user:** the `manual-review-3` screenshot
shows H1 selected with valid mounting details but disabled Visualizar abertura,
both relationships marked for review, and red review annotations. No motion
interaction pass is claimed. A repeated serialize/load/preview regression
reproduced the false left-door review: serde_json's default float parser shifted
a derived axis by one ULP while relationship review correctly used exact checks.
An older fixture test had tolerated this axis discrepancy without checking loaded
motion availability. Enable serde_json `float_roundtrip` and require full project
equality, unchanged serialized bytes, clean-on-open state and working left-door
motion over three save/reopen cycles. The intentionally invalid right cup remains
blocked, and all seven existing handedness/legacy review/undo tests still pass.
No review tolerance, silent axis repair or automatic reconfirmation was added.

The original Hardware review file already contains a parser-rounded axis; keep it
unchanged rather than rewriting it on open. A fresh `Hardware review v2.pmcab`
will be supplied with the corrected parser package. Source-only Handoff letter
spacing is included in that next build. Full gates/package and native retest are
pending at this checkpoint; the previous 77/87 acceptance status is unchanged.

**Hardware precision-fix package ready:** all-target check, warnings-denied
all-target Clippy, full Rust tests, formatting, whitespace and strict OpenSpec
validation passed (`…/opencode/hardware-roundtrip-full-tests.log`). Fresh fixture
`dist/manual-review/test-data/Hardware review v2.pmcab` was generated without
overwriting the earlier fixture. `dist/manual-review-4/Plan My Cabinet.app`
built successfully and passed plist/deep strict signature verification. User
native motion retest remains pending; use this package with the v2 fixture,
not the earlier parser-rounded copy. Bilingual Hardware guides describe exact
round trips and explicit review for previously saved rounded axes.

**User `manual-review-4` Hardware result:** user reports the motion batch works;
the attached image independently shows Door left at `60° / 105°`, opening
outward with H1 selected, a stationary cabinet, readable fixed-axis/display-only
disclaimer, and the intentional upper-right cup warning retained. Close and
workspace-reset behavior are user-reported rather than separately captured.
The screenshot also exposes incorrect angle-scale layout: 0/45/90/105 labels
were simply spaced from the left instead of aligned to the slider values.

Source follow-up anchors tick marks to egui's actual handle-travel range and
positions labels proportionally to the configured limit on both the canvas HUD
and expanded controls. Endpoint labels stay inside the card; crowded intermediate
labels use another row. A regression compares the scale with the painted slider
thumb for circle/rectangular handles, widths 150/300/436 and limits
60/90/105/110/180 degrees, verifies no label overlap and unchanged angle. All five
Hardware UI tests pass, including bilingual disclosures and Closed restoration.
This correction is not yet in the user's `manual-review-4` build; motion passes
are retained and no repeated opening test is required solely for scale labels.

The user requested replacing assistant-driven computer-use testing with short,
user-executed instruction batches and returned screenshots/results. Future native
acceptance is recorded from those results; required checks are not waived.
The first handoff build is `dist/manual-review/Plan My Cabinet.app`, including
the explicit kerf-confirmation modal. Disposable legacy and Drawers copies are
in `dist/manual-review/test-data/`; an isolated HOME is available at
`dist/manual-review/test-home/`. Build, focused kerf/Settings lifecycle tests,
warnings-denied Clippy, formatting and strict OpenSpec validation passed before
handoff. This is not yet a user-tested visual or interaction pass.

## Latest evidence and remaining reference corrections (2026-09-27)

The earlier modal-safety package completed a populated, four-page draft export, native
overwrite refusal, Save As and recent reopen with the current receipt. See the
newest [release walkthrough](release-checklist.md) for paths, file hash and
limitations. Its native schema-upgrade selection blocker was subsequently
resolved by user batch 1, as recorded above; it is no longer a current blocker.
This does not complete 15.1 or 15.5.

Independent review of the latest available captures found the following
**unapproved differences**, in logical points. These remain corrective work,
not renderer exceptions. The six dialog images predate the subsequent chrome
update and all need recapture.

| Reference | Evidence directory under `…/opencode/` | Remaining correction / evidence gap |
| --- | --- | --- |
| R01 Design | `apply-design-framed-offset/` | Scene begins around y139 rather than y46 because of legacy toolbar rows; outliner row pitch ~32 rather than 26; inspector miniature displaced below y720; dimension HUD fields clip. Camera variant lacks retained-default provenance. |
| R02 Cut plan | `apply-cut-plan-final-b/` | Sheet is ~66 pt too narrow and 33 pt too low; unselected parts remain green rather than warm-neutral; thin rails lack readable name/dimension callouts; statistics need 2×2 cards; Needs stock action clips. |
| R03 Stock | `stock-scoped-scroll-0927/`, `stock-narrow-trims-0927/` | Baseline table still horizontally hides actions; selected-row amber treatment missing; material list displaced by heading controls; table row pitch ~32 rather than 40. Narrow trim-stack fix supersedes old trim clipping, but baseline recapture and native wheel/drag remain pending. |
| R04 Dialogs | `modal-reference-0927-{board,position,face,resize,material,unsaved}/` | Six real drafts now captured separately. Later header/icon/Escape/footer/width corrections require recapture. Position/face retain extra workspace rows; board preview needs valid exact-expression state; resize capture must show mixed/rounding state; context headers and body group geometry remain incomplete. |
| R05 Handoff | `apply-handoff-final/`, `apply-handoff-pt-page-{1,3,6}/` | Export is below issue-list fold, not in a fixed footer; language label clips; thumbnails ~96 rather than 62 wide; preview background and page bounds differ; Before you send guidance is missing. Old fixture captures need regeneration after handedness fix. |
| R06 Hardware | `hardware-handedness-c/` | Outward direction passes separately. Model starts around y150 rather than y228 and runs behind HUD; editable inspector fields/segments remain missing; slider track/thumb/ticks, catalog-card height, axis color and hinge labels differ. |
| R07 Welcome | `apply-welcome-empty-final/` | Empty state does not represent populated/recovery reference. Left split ~350 rather than 340; logo/template artwork missing; populated, missing-file and recovery captures still required. |
| R08 Cutting settings | `apply-settings-cutting-final/` | Kerf field ~101×28 rather than 140×34; status wraps; confirmation card displaced; checkbox presentation and heading weight differ. New confirmation modal must also receive native review. |
| R09 Costs settings | `apply-settings-costs-final/` | Fee/currency control geometry differs; estimate card ~226 rather than 348 wide; prose replaces aligned label/value rows. Preserve actual 13 cuts and unknown costs. |
| R10 General settings | `apply-settings-general-final/`, `apply-settings-general-pt130/` | Circular checkboxes instead of 32×18 switches; language/scale segment tracks absent; scale row ~35 pt too low; footer hint centered instead of left aligned. |

Shared workspace differences: left-pane separators are ~18 pt too far right,
right-pane separators ~5 pt too far left, and the status strip is ~17 rather
than 26 pt high. Rail logo/header composition and centered 440×30 command
search remain to align. Capture manifests currently mark
`redesign_acceptance: false`; none of these observations grants acceptance.

## Approved handedness correction and native Stock follow-up

The approved task 10.6 is implemented: MinX/MinZ and MaxX/MaxZ use local −Y,
the other edge/face pairs local +Y, transformed through the door's world pose.
Existing axes are retained on load and compared through the existing review
gate; reconfirmation is explicit and undoable. Seven focused relationship
tests pass, including all four pairings in rotated/nested roots, exact closed
poses, unchanged stationary members, legacy serialization equality, blocked
stale motion, reconfirmation and undo/redo. The English and pt-BR Hardware
guides now explain this compatibility path.

Actual native Metal capture `…/opencode/hardware-handedness-c/` shows the
selected left door at 60° opening outward, with the fixed-axis disclaimer and
105° endpoint retained. This passes the direction retest, not the remaining
R06 composition/interaction acceptance under 10.5 and 15.2.

The subsequent native Drawers walkthrough created 23 boards, declared an
owned 2440 × 1220 × 18 mm sheet, selected it, clicked the horizontal scrollbar
track to reach the table's right-hand actions, and opened/committed Edit piece
using a real pointer (`sky_click`). Spending cards and global rank controls
stayed stationary while the table scrolled. `app_post` scrolling/drag did not
establish a wheel/drag pass. Saved test data is retained at
`…/opencode/native-handedness-home/Drawers.pmcab`; a full path entered in the
native Save name field was interpreted as a colon-separated name in Documents,
so that test-created file was moved to the evidence directory afterward.

The same walkthrough exposed that initial template framing was discarded by
the next project-session synchronization. The fix synchronizes before setting
workspace cameras and defers each fit until its real canvas is allocated.
The extended Drawers lifecycle test passes through repeated Design → Hardware
→ Design frames with the generated center retained and no document edits.
The post-fix native generation retest passed: a fresh Drawers setup generated
23 boards and opened with the whole cabinet centered in Design, then Hardware.
The earlier failed generated-camera view is superseded. Native Locate also
validated the moved `Drawers.pmcab`, refreshed its 23-board/1-sheet metadata
and corrected the recent path without implicitly opening the project.

## Hardware composition follow-up — historical pre-correction evidence

The Hardware pane now uses a full-height scene with floating camera controls,
an inspected-door motion mode/card, compact pinned catalog and hinge rows,
unselected warning summary, and a compact diagnostic inspector. Placeholder
hardware and other advanced routes remain in a disclosure; independent door
and mount coordinates, full supported pairs, source snapshot and board-local
references remain available. New bilingual overlay tests exercise camera
pointer isolation, canvas height, bounded motion disclosure and Closed without
document mutation. The existing inspector test opens the details disclosure
and retains all its invalid-evidence/coordinate assertions.

Native Metal captures `…/opencode/hardware-composition-a/` (closed) and
`…/opencode/hardware-composition-b/` (60-degree preview) show the composition
improvement, **not reference acceptance**. The latter exposed the former
domain mismatch: the fixture's left door opened inward. `door_joint::preview`
previously always used transformed local +Y as the directed axis; here that is world +Z.
At +60 degrees the free edge (397,0,z) moves to (198.5,+343.812,z), inside the
carcass (+Y). The approved HTML instead uses a negative Y component, outward.
This is not a renderer discrepancy or permission to claim collision validity.

Changing the stored axis convention affects `needs_review` and confirmation
of existing relationships; changing only `derived_poses` changes existing
projects' display semantics. A global sign reversal is wrong for paired
left/right doors. The subsequent approved correction and explicit legacy
review policy are implemented and verified above. Tasks 10.5/15.2 remain open;
this does not waive remaining
panel geometry, editable inspector, camera framing or native interaction work.
The post-composition gate passed formatting, all-target check, warnings-denied
Clippy and the full sequential suite (218 library / 313 desktop unit tests
plus integration targets), diff check and strict OpenSpec validation. No
remaining task is marked complete from that automated gate.

## 2026-09-27 packaged native follow-up — partial

An arm64 ad-hoc signed bundle at
`…/opencode/apply-package-upgrade-0927/Plan My Cabinet.app` passed plist and
deep signature validation and opened offline in an isolated `HOME`. Its native
Save sheet wrote a new empty `.pmcab`; Welcome showed the populated recent,
and selecting that recent reopened the same project. Moving that **test** file
away showed the missing-file Locate/Remove state; Remove cleared only the
recent entry. The test file was retained under isolated temporary data and
the temporary Dev copy cleared. Opening the schema-1 golden document through
the native Open sheet worked. A missing schema-upgrade warning was discovered:
the earlier bundle silently wrote schema 2 on Save, so a new common modal now
warns before the first legacy Save/Save As. The rebuilt bundle displayed this
notice and native Cancel preserved the original schema-1 byte hash. A focused
test confirms explicit acceptance writes schema 2 atomically; native acceptance
and PDF export/write are still pending. None of this substitutes for the ten
reference comparisons or the complete end-to-end workflow.

The updated 1440 × 900 Stock Metal capture at
`…/opencode/stock-scoped-scroll-0927/` has spending cards within the center
pane after giving the wide table its own horizontal scroll. The updated
Hardware capture at `…/opencode/hardware-vertical-0927/` wraps catalog and
tree rows. A later 1280 × 875 capture at `…/opencode/hardware-narrow-0927/`
showed the camera's closed assembly within the canvas; a subsequently removed
revision-string abbreviation still needs recapture. The reference-level
geometry and motion state are not accepted. The final bundle's Metal Hardware
capture is `…/opencode/hardware-post-trims-0927/`. A paired 510/770-point
Stock layout test covers card clip bounds in both locales; actual horizontal
pointer-scroll reachability and R02/R06 comparison remain open.

The final offline bundle also displayed a live one-page draft Handoff preview,
required **Review this preview**, and wrote a PDF through the native Save
sheet. The receipt stayed current, and Poppler confirmed an A4 page with
embedded fonts and draft/kerf text. Evidence is at
`…/opencode/native-final-home-0927/exported-draft.pdf`. This does not cover
the populated multi-page packet, pt-BR native preview, overwrite refusal or
the complete project-to-export journey.

Latest post-fix validation: `cargo fmt --check`, `cargo check --locked
--all-targets`, warnings-denied all-target Clippy, `cargo test --locked
-- --test-threads=1` (218 library and 311 binary unit tests plus integration
targets), `git diff --check`, and strict OpenSpec validation passed. The
Stock inspector's four trim actions have a bilingual compact-width regression
test; the legacy upgrade test also checks cancellation of a pending Open.

## Native accessibility walkthrough — 2026-09-27, partial

With `open-computer-use` 0.3.5 connected to OpenCode V2 and macOS Accessibility
and Screen Recording granted, a foreground isolated-home Metal run exercised
Welcome → Drawers setup → nested new material → all five role assignments →
dimension/review → 23-board generation → Design → Stock → a real 2400 × 1200 ×
18 mm owned piece, plus a native Save picker cancellation. No file was saved
to the user's Documents or registered as recent. Native AccessKit initially
crashed on the wizard Project→Materials transition and again on closing its
nested material editor (`Focused ID … is not in the node list`). Clearing the
removed body focus and restoring the parent button on the next frame fixes
both observed crashes; a fresh run traversed the same steps without a panic.
The focused-node regression and template/modal tests pass. The generated
assembly initially filled/clipped the compact Design canvas; template commit
now requests a selection fit on the Design and Hardware cameras, but the new
automatic framing still needs a post-build native retest. The 1280 × 875 Stock
screenshot shows the table's right columns and spending cards clipped in the
center pane; Hardware clips long left-panel labels and renders the generated
assembly partly outside the canvas. Scrolling the Stock scrollbar through
accessibility did not move the table in this run; keyboard focus did reach
stock modal fields and its fixed footer. 8.6, 10.5, 15.2 and 15.3 remain
open. The Save sheet opened and cancelled without
altering the unsaved project; writing and reopening through the native picker,
upgrade prompt, and PDF export remain unverified. The full sequential Rust
suite, formatting, all-target check and strict Clippy pass after these fixes.

## 2026-09-27 Cut plan sheet checkpoint

Latest post-selection-fill validation: `cargo test --locked -- --test-threads=1`
passed 218 library and 308 binary tests plus all integration targets;
`cargo fmt --check`, `cargo check --locked --all-targets`, strict all-target
Clippy, `git diff --check`, and strict OpenSpec validation also pass. The
Design viewport now paints the active board face warm without recolouring
secondary selected material; its camera default remains unchanged. The
recaptured Design view still needs R01 geometry/type/icon comparison and the
native two-point/invalid-draft interaction review before task 6.8 or 15.2.

The native Metal Cut plan fixture at
`/private/var/folders/sz/nnw19cc96ts9hmcg2m0l8j_w0000gn/T/opencode/apply-cut-plan-final-b/capture.png`
shows a fitted sheet with real millimetre rulers, named/dimensioned parts, a
highlighted selected part, grain arrows, reusable-offcut hatching and all nine
numbered witness cut bands. The host's separate inspector retains cut rows;
the headless sheet suite covers witness/kerf geometry, conflicts, low-zoom
callouts, overlay toggles, selection and row-hover linkage (29 passing tests).
The two independent Advanced disclosures no longer share an egui widget ID.
This validates task 9.2, **not** the complete Cut plan reference or the
repair/compare native walkthrough required by 9.7 and 15.2.

## 2026-09-27 Settings task 14.5 checkpoint

Foreground Metal captures of Cutting, Costs & currency and General at
780 × 560/en/100% are at `…/opencode/apply-settings-{cutting,costs,general}-final/`;
the enlarged pt-BR General capture is at
`…/opencode/apply-settings-general-pt130/`. All retain a visible Done footer,
section navigation and readable body at those sizes. Ten Settings interaction
tests cover section bounds, focus, popup keys, child return and Done/Escape;
dimension-input tests cover captured unsuffixed locale/unit, explicit suffix,
rounding consent and pristine reformatting. The bilingual input, project and
viewport guides now describe these controls. The 780 × 560 screenshots still
have typography/spacing differences from the handoff; task 15.2's two-point
geometry and token acceptance is **not** implied by completing 14.5.

## 2026-09-27 Stock/Design/Hardware/Handoff recapture

Updated native fixture captures at `…/opencode/apply-{design,stock,hardware,handoff}-final/`
and the Stock pt-BR capture at `…/opencode/apply-stock-pt-final/` supersede
the earlier partial images below. Stock now has a grouped table, rank/alias
distinction, an HDF missing-stock route and a separate editable inspector;
filtered/global drag and action tests preserve hidden slots and allocations.
Its table and inspector still clip substantive controls in the reference
frame and differ in hierarchy/spacing from R03, so **8.6/15.2 remain open**.
The Design HUD has moved to a compact bottom strip and Hardware has a framed
cabinet with real reference annotations, but R01 and R06 still need alignment
and native interaction sequences. Handoff has its separate export-history pane
and six-page preview but its controls and text density still differ from R05.

## 2026-09-27 Handoff task 11.8 checkpoint

The native Handoff reference capture is at `…/opencode/apply-handoff-final/`.
The pt-BR A4 multi-page capture selected pages 1, 3 and 6 at
`…/opencode/apply-handoff-pt-page-{1,3,6}/`; all three produced an identical
six-page `reviewed-preview.pdf` SHA-256
`dc2d55a28e86e22da2b9029b4222bb17a2cfaf9239460e9bfb93f8a6d3589f77`.
`pdffonts` reports three embedded CID TrueType faces with Unicode maps. Seven
shared-document/PDF tests check all dense pages and two zoom levels; five
reviewed-packet tests cover the exact frozen export, cancellation, picker
failure and receipt behavior; `shop_handoff` passes. The bilingual guides now
explain preview/history, page navigation and print-size review. R05 token and
geometry approval, native file-picker operation and offline package export
remain in 15.2/15.5, not this functional/page-parity checkpoint.

## Native foreground capture and frozen-document review — 2026-09-27

A foreground logged-in macOS/Metal run now delivers screenshot callbacks. The
isolated capture records a `Bgra8Unorm` target, which is supported by pinned
egui-wgpu's readback path. The gallery, all five workspaces at 1440 × 900,
empty Welcome at 1100 × 700, and three Settings sections at 780 × 560 produced
real PPM images and manifests. The reproducible outputs are under
`/private/var/folders/sz/nnw19cc96ts9hmcg2m0l8j_w0000gn/T/opencode/native-review-20260927/`;
the PNGs there are the 2× Retina captures downsampled to logical size for
comparison. Previous timeout records below describe earlier attempts and do
not apply to this successful foreground run. No static capture alone approves
native interaction.
Two additional 900 × 650 gallery captures from the same binary/config had
identical PPM SHA-256
`172a48aa1ac21459b6f277904fe93ee88aeffe39dd8c4bcb74375cee03987b8d`;
this demonstrates tooling repeatability, **not** repeatability or acceptance
of all redesigned screens after their next reflow.

**Reference comparison remains a failure, not a renderer exception.** The
Design outliner starts/ends about 17 points farther right than the reference;
its large lower-left selection card covers the scene instead of the bottom
strip, the camera controls are hidden, and the cabinet is about 16% too large.
Stock places cost summaries above the primary table and renders the inspector
as prose rather than the reference form. Cut plan begins the focused sheet near
`y=697`, clipping its bottom below the status bar; the small side/rail parts
have ID disks but no readable name/dimension callouts. Hardware's selected
installation and motion controls are not visible in the capture. Handoff lacks
the separate right history pane and its export controls are below the fold.
Empty Welcome's split is near `x=816` instead of `x=340`, wrapping language
controls character by character. Settings section and footer bounds differ
from the 780 × 560 references; General's two recovery actions run together.
All of these must be reflowed and recaptured before R01–R10, 6.8, 8.6, 9.2,
9.7, 10.5, 12.6, 14.5 or 15.2 can pass. Corrected physical counts/prices,
unknown fee and confirmation date, empty recents/receipts, and the six-page
packet remain truthful data differences; do not copy the mockup literals.

**Frozen PDF/native page parity (functional 11.4):** capture-only Handoff now
writes `reviewed-preview.pdf` from the exact `ReviewedPacket::document()` shown
by the native preview, without triggering an export or receipt. Its six A4
pages were independently rasterized at 96 dpi with `pdftoppm` for review, and
native capture selected pages 1–6 with `--capture-page`. A crop of each native
page (`x=504..987`, `y=86..770` in the 1440 × 900 logical image) was compared
to the corresponding PDF page resized to 483 × 684. Text, diagram lines,
markers, page breaks and notices align in all six pairs; coarse dark-mask
intersection/union is respectively **0.747, 0.776, 0.699, 0.774, 0.687,
0.770**. The first and sixth capture-side PDF files have identical SHA-256
`dc2d55a28e86e22da2b9029b4222bb17a2cfaf9239460e9bfb93f8a6d3589f77`;
`pdffonts` confirms all three CID TrueType faces embedded. The differences are
glyph rasterization/antialiasing, not a missing
section or a shifted page. Dense en/pt-BR tests additionally compare every
positioned glyph and page count at two preview zooms; focused UI tests verify
thumbnail/page selection, zoom without repagination, and independent wheel
scrolling of page and rail. No external viewer is used by the application.
Dense native pt-BR raster, the Handoff reference composition, and offline
packaged export still belong to 11.8/15.x; this evidence does not close them.

**Dialog integration scope:** the behavioral/translation work in 7.1, 7.2,
7.3 and 7.5 is checked independently of visual approval. New board/material,
numeric/face placement, batch resize, material preserve/apply, hierarchy,
delete, overwrite and unsaved-work prompts keep their real drafts, validation,
atomic undo and cancellation. The shared three-action footer now distinguishes
Save/Discard/Cancel and Apply/Discard/Stay rather than hiding a destructive
decision in body text. Focused owner tests exercise invalid Enter, child-popup
Enter/Escape, nested material return and blocked background actions; a host
navigation prompt test verifies an invalid draft cannot follow its destination
on Enter and Escape preserves the draft. The **six individual native dialog
captures, remainder of the modal inventory, and R04 comparison are still open
in 7.4/15.2**. The 2026-09-27 notes below predate this reconciliation.

## 2026-09-27 apply checkpoint (not Design approval)

The isolated 1440 × 900 English Design capture at
`/private/var/folders/sz/nnw19cc96ts9hmcg2m0l8j_w0000gn/T/opencode/current-state-design-20260927-0947/`
rendered at 2880 × 1800 on Metal. A separate 900 × 650 pt-BR / 130% capture
at `…/opencode/responsive-900-pt130-apply/` shows header overflow and a
reopenable inspector while a board draft remains present. The responsive
headless test traverses header/status menus, both drawers, camera overflow
and HUD actions across the reference and secondary sizes and all four scales;
the current `cabinet_assembly`, `redesign_reference` and 19 `assembly_ui`
tests pass. The six bilingual Design/board/viewport guides now explain the
workspace routes and shared draft behavior.

This is **not** a 6.8 or R01 acceptance: the selected-board HUD remains much
larger and higher than the reference's bottom strip, camera/tool controls take
multiple lines, the 3D fixture differs in selected-face appearance, and a
native empty/hidden/multi-selection/invalid-draft pointer walkthrough is not
yet recorded. The viewport help line now truncates with full hover text at
compact widths; recapture after that change before visual approval. Do not
turn the headless tests or the static manifest's `redesign_acceptance: false`
into an approved two-point geometry comparison.

The same checkout's Stock capture at
`…/opencode/stock-apply-1440/capture.png` confirms real aliases, global ranks,
material filtering, incomplete spending and a selected piece's inspector.
However, the grouped table still needs horizontal scrolling at the reference
size, its inspector/summary composition differs substantially from the Stock
reference, and native filter/drag/keyboard interaction is not yet recorded.
Task 8.6 therefore remains open despite the bilingual guide and headless
subset-reorder tests.

An updated Hardware Metal capture at `…/opencode/hardware-apply-1440/capture.png`
does include live board-local cup/plate/axis guides and the separate reference
diagram. Focused installation, relationship and deletion tests pass. It is not
R05/10.5 acceptance: the reference camera crops the cabinet and long catalog
metadata and relationship labels are cut off in the side panes. The native
selection/edit/motion walkthrough remains to be performed after that layout
is corrected.

## Native integration checkpoint — 2026-09-26 (not reference approval)

**2026-09-27 continuation:** Welcome now mounts the local recent-project host
and staged Base/Wall/Drawers setup. Focused headless tests cover validated
Open/Locate/Remove, unsaved replacement prompts, template cancellation, one
generated-material/assembly undo, insufficient-stock disclosure, and retaining
both the original document and setup after save failure or picker cancellation.
Hardware's duplicate catalog/door lists were consolidated; reference-hardware
removal now goes through a validated action and explicit modal. Cut plan gained
witness-backed overlays and small-part callouts; Handoff has issue-specific Fix
routes and live packet counts. None of these checks substitutes for native
reference approval. Three isolated Metal screenshot attempts (`pmcab-hardware-v3-1440`,
`pmcab-hardware-v3b-1440`, `pmcab-gallery-probe`) failed with “Native screenshot
did not arrive within 300 frames” and produced no PPM/manifest; the capture
failure needs diagnosis before checking native fidelity tasks. Warnings-denied
Clippy, formatting, all-target check, focused Welcome/template/receipt/Cut
plan tests, and a full 229-test binary suite passed after integration. A
subsequent library run exposed one unsupported `←` in new template labels;
replacing it with text made the targeted bundled-font test pass. The first
combined full-suite attempts timed out during compilation. A later complete
sequential run is recorded below; it supersedes that provisional gate status.

**2026-09-27 capture diagnosis:** a later isolated gallery retry again timed
out at 300 frames. Temporary instrumentation (removed after the test) showed
Screenshot commands requested at frames 12, 72, 132, 192 and 252 on the root
viewport, with no `Event::Screenshot` delivered. The native viewport reported
the requested 900 × 650 content rectangle and `focused: false`; visibility was
unknown (`None`), which eframe treats as visible. A prior capture on the same
Metal target succeeded, so this is not evidence that the new workspaces match
the references or that the backend is categorically incapable. Diagnose GPU
readback/window focus/surface behavior in an interactive desktop session before
claiming the ten reference comparisons.

**2026-09-27 consolidated headless gate:** `cargo fmt --check`,
`cargo check --locked --all-targets`, warnings-denied all-target Clippy and
`cargo test --locked -- --test-threads=1` passed on the dirty macOS arm64 tree;
the full suite reported one previously ignored test and no failures. Strict
OpenSpec validation and `git diff --check` also passed. The prescribed
100-board/10-stock fixture then observed a **5.0009 s** worker completion at
its 5 s search deadline, **1.52 ms** cancellation (below the 250 ms target),
**229.73 ms** for five full workspace frames during the worker, **110.92 ms**
for the first densely allocated frame, and **76.38 ms** for five unchanged
dense frames. Compared with the 2026-09-25 baseline in
[`development.md`](development.md#optimization-performance-fixture-task-86),
cancellation remains under target and the headless frames improved. These are
debug-profile observations, **not native Metal pointer/scroll acceptance**;
the capture/readback failure and the unfinished visual/interaction matrix
remain explicit.

**2026-09-27 integration gate after Handoff/Hardware changes:** the full
sequential `cargo test --locked -- --test-threads=1` suite, all-target check,
warnings-denied Clippy, formatting, strict OpenSpec validation, and diff
whitespace check passed. The Handoff Shop-ready card now blocks hidden wood
issues even with all optional sections off, exposes a reachable board-specific
Fix action above the output controls, and the palette cannot start a stale
picker after section changes. A verified wood plan can select Shop-ready with
pt-BR/ft output independently of English/mm interface settings. Hardware's
installation-specific projected cup/plate/axis guide is mounted only in its
workspace and follows the display-pose geometry; headless host and projection
tests pass. These are functional gates, **not approval of native layout**.
One additional capture experiment polled the Metal device during capture-only
frames and still timed out after 300 frames without an image; that exploratory
poll was removed. A longer 1200-frame screenshot retry also timed out and its
temporary timeout change was reverted. Cut-sequence rows now sit beside the
canvas within its central pane and headless tests exercise simultaneous hover,
but the shell's separate Cut plan inspector still shows a duplicate placeholder.
At that checkpoint task 9.3 remained open until the shell panes were
consolidated. Native reference interaction still requires its separate R04
review.

**Repair host guard (later 2026-09-27):** Accept now remains disabled while a
numeric or target draft has not been staged, or any affected sheet lacks a
verified witness; failed staging preserves the draft. Numeric entry keeps its
captured unit across display-unit changes. Focused sheet tests cover live drag
ghosts, transfer, quarter turn, lock/unlock, unallocation, invalid intermediates,
Escape cancellation and a single undo after acceptance. At the workspace host,
an invalid preview prompts with Commit disabled and Stay/Cancel preserves or
discards explicitly; a valid transfer to a separate physical sheet prompts,
commits once on Accept, changes workspace and undoes back to the original
allocation. This closes functional 9.4, not the native Cut plan visual or
pointer/trackpad walkthrough in 9.7/15.3.

**Later integration of that pane:** at 1440 × 900 the Cut plan central canvas
now uses its full shell width and the witness statistics/sequence render in
the pre-existing 308-point right inspector rather than a duplicate in-canvas
column. Hover state is tied to the focused stock ID and repaints its exact
cut band on the following frame; a host-frame test sees sequence and numbered
sheet marker simultaneously. The standalone narrow-pane fallback still stacks
and scrolls both. This closes functional task 9.3, but not native R04 review.
Optimization's right inspector now presents the current-versus-best metrics
and per-part changes vertically, with a larger scrollable comparison for
review and acceptance. The palette's Start/Cancel/Review actions no longer
advertise availability and then fail at invocation; review routes to Cut plan
without auto-applying. Cross-workspace shell activity, color-only acceptance
preserving newer metadata, manufacturing staleness, cancellation, lock/no-result
and one-undo application have focused tests. Tasks 9.5/9.6 are functional
completions, not a native visual pass. The post-integration all-target check,
warnings-denied Clippy, format check, strict OpenSpec validation, whitespace
check and complete sequential `cargo test --locked -- --test-threads=1` passed;
the test transcript is
`/private/var/folders/sz/nnw19cc96ts9hmcg2m0l8j_w0000gn/T/opencode/redesign-all-tests-20260927-3.log`
(including 203 library and 266 desktop unit tests at that point; one existing
integration test remained ignored). The transcript predates later palette-route
and dialog/repair edits, so rerun the gate at release integration.

**2026-09-27 additional headless coverage:** New board/Material now use the
shared modal chrome, with staged swatches and a nonmutating first-fit preview;
the focused tests cover multiple edits, popup keys, roundoff consent, nested
material return and a stock-state change before confirmation. Their native
reference appearance remains unreviewed (7.1 open). The Cut plan optimizer
exposes a scrollable full comparison and a compact inspector counterpart; a
host guard blocks shortcuts/project actions during its review. Its focused
objective/stale/undo and bilingual compact tests passed at that earlier
checkpoint; the later inspector/host integration above closed functional 9.5,
not native R04. PDF layout now separates egui's zero-advance
ligature continuations within the already-measured run width; dense English
and pt-BR tests compare the positioned native-preview glyphs with all PDF
cursors and preserve page counts. The shaped-glyph metrics/cache version was
bumped to 2 so an older prepared packet cannot be mistaken for the revised
placement. A pixel-level native-vs-PDF raster check has
not been completed at that checkpoint; the foreground six-page comparison above
subsequently closes functional 11.4.

**Placement and resize dialog integration:** Position/Numeric pose, Place face
to face, and Resize N boards now use the shared centered chrome. Headless
pointer/key tests in both 1440 × 900 and 900 × 650 exercise invalid Enter,
inexact-value consent/reset, footer acceptance, Escape cancellation and undo;
the batch form shows mixed current and per-board before/after values. The
English and Portuguese viewport/input guides name the actual footer actions
and current Grid route. The six reference-dialog native captures and visual
review have not been done; 7.2/7.4/7.5 remain open pending their respective
acceptance boundaries.

**Additional dialog boundary:** the host's advanced board-dimension,
board-material assignment, material preserve/apply, and PDF overwrite forms
now use the shared scrollable modal chrome. Focused tests verify invalid
resize/unchosen material decisions cannot submit, exact preview acceptance
commits one undo, and Escape cancellation restores focus. The PDF overwrite
dialog revalidates the exact reviewed packet on confirmation; declining or a
stale packet produces no receipt. A newly added regression also verifies that
leaving the overwrite modal does not consume an in-flight `Writing` export
activity on the next frame. Other three-action navigation/project prompts and
Settings' specialized presentation still need their inventory review; neither
the six reference-dialog captures nor the native picker/write workflow have
been approved.

**Welcome selector preparation:** `--capture-welcome empty` now mounts the
actual Welcome with a first-run empty project and recents restricted to the
new capture directory; a headless host test checks template tiles, empty-state
copy and no fixture board. The manifest identifies `welcome-empty` rather than
claiming a cabinet fixture was shown. This is not a delivered native screenshot
and does not cover populated, missing-file or recovery cards; 12.6 and 15.2
remain open.

Under the approved OpenCode temporary root, isolated Metal captures with PPM,
PNG and manifest include `pmcab-stock-v4-1440` and `pmcab-stock-v5-900-pt`
(material list moved to the left pane, estimate cards and explicit currency/fee
controls); `pmcab-cut-v2-1440` (priority sheet cards, deduplicated Needs stock
and the selected physical sheet); `pmcab-hardware-v2-1440` (selected hinge
inspector and board-local reference diagram); `pmcab-handoff-v3-1440` (vertical
thumbnail rail and selected page from the frozen positioned document); and
`pmcab-settings-v2-780-pt` and `pmcab-settings-about-780-en` (native 780 × 560
Settings surfaces including a 130% Portuguese case). The Settings capture CLI
now accepts `--capture-settings cutting|grid|costs|general|shortcuts|about` and
the manifest identifies the section. The Handoff capture deliberately starts
with a pre-reviewed fixture packet **only for static visual inspection**, not
as evidence of a user's acknowledgement.

These captures are *not* native pointer/keyboard acceptance, two-point geometry
approval, or full PDF raster parity. Cut plan still has crowded part labels and
significant reference composition differences; Stock editing is modal and
requires table scrolling at compact sizes; Hardware did not yet project hinge
references in the live viewport **at this capture checkpoint** (the later
projection implementation above has not been recaptured). The PDF cover is visible but only a two-page
geometry spot-check has been made against exported rasters. The Settings modal's sidebar
and footer were adjusted after the initial compact capture; Done stays within
the window, but focus, recovery cleanup and Welcome/no-project access need
end-to-end verification. Focused tests exercise sheet/issue routing, stock
currency and ordering, hardware diagnostics, Settings intents and unit history,
reviewed-packet races, and evidence-based receipt cards. `cargo fmt --check`,
all-target check and warnings-denied Clippy passed at this checkpoint. A full
test run passed the library and binary suites and many integration suites, but
the tool's 120-second timeout interrupted it during recovery discovery; do not
interpret that partial run as a completed release gate.

**Frozen-page raster spot-check (2026-09-26):** a one-off development-only
renderer produced six A4 pages from the fixture's same `ReviewedPacket` source;
the temporary example was removed after use. `pdfinfo` reported six pages.
`pdftoppm` (development review only, **not** a runtime dependency) rasterized
pages 1 and 3 to `pmcab-review-raster-page{1,3}.png` at 152.4 dpi under the
approved temporary root. Native Metal captures `pmcab-handoff-v3-1440` (page 1)
and `pmcab-handoff-page3-1440` (sheet diagram, `--capture-page 3`) used the
same 1440 × 900 fixture and frozen pagination. After scaling the PDF A4 raster
to the 966 × 1366-pixel native page rectangle, both pages' dark-ink masks
aligned with **zero horizontal and one vertical pixel** offset in a ±5-pixel
search; threshold-180 Jaccard overlap was 0.639 and 0.595, respectively.
Anti-aliasing and glyph rasterization differ; this is a geometry/content
spot-check, not pixel identity or a full six-page/long-pt-BR approval. Both
renderers also expose irregular intra-word spacing from the positioned-glyph
strategy (notably `unverified`), which required investigation before claiming a
polished shop packet. A later ligature-position fix and complete six-page
comparison are recorded at the top of this file; 11.8 remains open. No PDF viewer is needed by
the application; the external tool was used only for development inspection.

## Status and authority

This is the task 1.3 acceptance plan for
[`redesign-desktop-workspaces`](../openspec/changes/redesign-desktop-workspaces/proposal.md),
not a record of completed redesign validation. **As of 2026-09-26, the shared
five-route shell foundation exists, but its redesigned workspace content and
screen acceptance are unfinished. Baseline capture tooling and a
native widget gallery are available, but no redesigned-screen checks are
claimed passed here.** The matrices below identify required evidence; their
screen/workflow acceptance results remain **pending** until a dated run and
artifacts are recorded. Foundation unit and native gallery checks have separate
evidence in the linked guides. Existing regression suites are starting points,
not proof that the new UI is implemented.

**Shell foundation review (2026-09-26, not an R01 approval):** an isolated
1440 × 900 native baseline capture at 100% showed the five rail entries and
real fixture state. It exposed a duplicate egui widget ID at the Design
sidebar's repeated Advanced header and repeated `mm` suffixes in status and
inspector; both were corrected and a fresh native capture no longer shows
them. The viewport still occupies only the upper portion of its central area,
and the reference HUD/inspector composition is missing. These are pending
Design work, not renderer exceptions. The first screenshot request timed out;
bounded retries produced an actual 2880 × 1800 Metal capture on frame 15.

**Viewport styling review (2026-09-26, partial R01 evidence):** three isolated
native 1440 × 900 / 100% / en / orthographic captures at 2880 × 1800 show
saved-color shading, neutral tint-off shading, and a hidden selected shelf.
Directories under the approved OpenCode temporary root are
`viewport-task63-final-{on,off,hidden}`; each contains `capture.ppm`,
`manifest.json`, and `viewport-state.json` with the effective colored fixture
hash `813823a1a5890266125c12179f4487c4062b2b2964605cea4ca0805737202260`.
The `manifest.json` hash instead identifies the original uncolored fixture;
the sidecar is authoritative for these view variants. Against
[`Main Window 2a`](../design_handoff_egui_redesign/screenshots/01-design-workspace-2a.png),
the warm background, perspective-like orthographic cabinet, material contrast,
grid/axes and grounding shadow are directionally present. The native selection
is an amber *edge* on the shelf rather than the reference's amber selected
face; the floor shadow is a soft rectangular bounds patch rather than an
illustrated cast silhouette. The selected shelf is near the cabinet back in
this actual fixture, so the visible selection edge is a narrow front line.
The viewport still ends around the upper half of Design instead of filling
the workspace, and the reference's HUD, outliner composition, dimension pill
and miniature stock preview are missing. These are unresolved layout and
workspace tasks, not renderer-specific approvals or a passing R01 result.
The neutral screenshot visibly removes the HDF brown without removing saved
color metadata; the hidden screenshot removes the shelf edge/mesh while the
outliner offers Reveal. The scripted native transient snap captures described
below now supplement automated snap-priority tests. This review was performed
on local macOS arm64 Metal with a dirty working tree; no clean commit/source
revision or formal two-point geometry approval is asserted.

**Design 6.4 integration review (2026-09-26, partial R01/M02 evidence):**
isolated native Metal captures `design-task64-reviewed-1440` (en, 1440 × 900,
100%) and `design-task64-reviewed-compact` (pt-BR, 900 × 650, 130%) under the
approved OpenCode temporary root contain `capture.ppm`, converted `capture.png`
and `manifest.json`. The reference fixture shows the 256 pt outliner and 292 pt
inspector at baseline, active Shelf, warning on unallocated Back panel and its
parent, real materials/stock counts, and viewport filling the central height.
The compact capture puts the tree at the top of the scrollable controls and
offers the labeled Inspector drawer in the header. Focused native tests cover
hidden/collapsed diagnostics, reveal, equal names, selection cardinality,
action/modal guards and revision cache invalidation. These are deterministic
capture and unit evidence, **not** a manual drawer/eye/pointer interaction or
R01/M02 approval. Compared with 2a, the inspector is denser, the tree uses
native buttons instead of compact icon rows, stock detail sits below the fold,
and the selected-board HUD, projected dimension pill and miniature sheet are
still missing. Board/pose edits remain dialogs rather than shared inline
drafts. See [Design read-model wiring](redesign-design-read-models.md#task-64-native-wiring-checkpoint-2026-09-26).

**Design sidebar/inspector follow-up (2026-09-26, partial R01/M02 evidence):**
new isolated macOS Metal images under the approved OpenCode temporary root are
`design-task64-sidebar-verified-1440/capture.png` (en, 1440 × 900, 100%,
2880 × 1800 raster) and `design-task64-sidebar-verified-900-pt130/capture.png`
(pt-BR, 900 × 650 effective, 130%, 2340 × 1690 raster), with PPM and manifest
beside each PNG. The prior baseline is `design-task64-reviewed-1440/capture.png`.
The 1440 capture shows actual swatches (neutral white, oak, HDF), a compact
outliner with separate warning/eye buttons and active shelf, material
thickness/board counts (18 · 6; 18 · 2; 3 · 1), all **four** fixture stock
pieces (S1, O1, S3, S2) with global ranks, measured size and explicit source,
and Add sheet visible before the sidebar fold. The reference 2a depicts three
stock rows, but the fourth is real fixture inventory and must not be silently
omitted. The selected-board inspector now groups readable local values and
independent stored-thickness provenance at the retained 292 pt width; advanced
editing is in its accessible disclosure, and placeholder hardware has guarded
Edit/Duplicate routes. The compact capture shows the tree and three materials
before the fold and the existing Inspector drawer entry; stock remains reachable
by scrolling. Focused tests cover warning/visibility/expansion/selection,
swatches and effective counts, aliases/source/rank and hardware guards. This
is scripted capture and headless evidence only. Compared to 2a, header/viewport
tool placement, the expanded Doors grouping, selection face shading, HUD,
projected dimension pill, shared drafts and miniature sheet remain visibly
different or absent. No R01 two-point geometry or M02 manual interaction
approval is asserted.

**Shared board/pose draft and HUD checkpoint (2026-09-26, pre-6.5 capture):**
the isolated macOS Metal fixture capture at
`pmcab-hud-gMXph9/capture/capture.png` under the approved OpenCode temporary
root (with PPM and manifest) shows the selected Shelf's inspector length/width
fields and a lower-left floating HUD borrowing one app-session draft. The HUD
shows the real effective thickness and routes to advanced face placement,
duplication, hide and delete. Static frames do **not** test
typing, selection clicks, prompt decisions, live compact reflow, or reference
geometry. Subsequent headless UI and transaction/navigation tests cover shared
invalid text, guarded selection and incompatible actions, one-undo Apply,
inspector-local numeric pose fields, frozen entry unit/locale across display
changes, pending pose frame resolution, panel collapse and project/shortcut
guards. These **later pose and keyboard changes were not present in this
capture**. Subsequently implemented projected length/width annotations and
the clickable, UUID-routed stock miniature also **were not in this capture**.
They have headless projection, paint, pointer and navigation tests, but no
updated native image or screen/pointer approval yet. The HUD placement and
proportions have not been approved against Main Window 2a; task 6.8 and native
interaction approval remain open.

An additional pt-BR 900 × 650 / 130% static capture at
`pmcab-hud-compact-oRqkgU/capture/capture.png` shows the Inspector drawer
entry and the HUD's wrapped actions within the canvas. The card occupies a
large part of the compact viewport and has not passed pointer/keyboard,
drawer-opening, or overlay-collision acceptance; it is layout evidence only.

**Post-6.5 native layout inspection (2026-09-26, partial R01/M02):** isolated
Metal captures `pmcab-post65-1440/capture.png` (en, 1440 × 900, 100%),
`pmcab-post65-labels-900/capture.png` (pt-BR, 900 × 650, 130%) and
`pmcab-post65-shell-1100-pt/capture.png` (pt-BR, 1100 × 700, 100%) are under
the approved OpenCode temporary root, with PPM and manifest beside each PNG.
They show real projected 764/537 mm Shelf labels and the shared-model S1
miniature in the baseline inspector, and visible length/width pills above the
compact HUD after collision adjustment. The compact HUD now has a persistent
action footer with face placement, duplicate and More (hide/delete), while
its long/invalid field body may scroll; opening a collapsed pane suppresses
the HUD without losing its draft. At 1100 pt-BR the header/status use localized
overflow without visible clipping or raw Fluent keys. These are static screen
and headless geometry/pointer/navigation observations, **not** manual drawer,
menu, typing, keyboard or complete two-point reference approval. The HUD still
occupies substantial viewport space and the Design composition differs from
Main Window 2a; tasks 5.4, 6.8 and 15.2 remain pending review.

**Measure and pose-preset checkpoint (2026-09-26, automated only):** the
Measure tool now uses the existing Body/Overall and selected-frame bounds in
the viewport and inspector; focused tests cover rotated/nested selections,
hidden descendants, real hardware extents and honest unavailable results.
The Position dialog's Stand up, Lay flat and relative +90° Z controls use the
approved board-pose-origin pivot, show the selected frame/orientation and
tentative Body extents, and retain exact preview quaternions through untouched
Euler text. Service and modal tests cover repeated previews, local/World
frames, invalid input, Escape cancellation, one-transaction acceptance and
undo. The static captures above predate these controls; no actual native
Measure/preset interaction or dialog comparison is approved yet. Those remain
part of tasks 6.8, 7.2 and 15.2–15.3.

Fresh default Design captures after this integration are
`pmcab-post67-design-1440` (en/1440 × 900/100%) and
`pmcab-post67-design-900-pt` (pt-BR/900 × 650/130%). Both manifests identify
the native frame-15 fixture render and `redesign_acceptance: false`. The
default view does not open the numeric dialog or activate Measure; these files
establish only that the integrated shell renders, not preset/Measure visuals
or interaction success. A follow-up regression also found and fixed two
precision cases: untouched half-grid pose origins remain exact through a
preset and frame switch, and a frame-only round trip under a rotated parent
does not create a tentative model edit.

**Secondary-size scale sweep (2026-09-26, static only):** the isolated Metal
capture command completed for each 90/100/115/130% choice at both 1100 × 700
and 900 × 650 logical points. The eight directories are
`pmcab-responsive-{1100x700,900x650}-{90-en,100-pt-BR,115-pt-BR,130-pt-BR}`
under the approved OpenCode temporary root; each contains `capture.ppm` and
`manifest.json`, with selected cases converted to PNG for inspection. Manifests
record frame 15, the requested language and logical size, pixel densities of
1.8/2.0/2.3/2.6 respectively, and `redesign_acceptance: false`. This is
evidence that the default reference fixture renders at those configurations,
**not** that header/status menus, inspector drawers, keyboard focus, invalid
drafts, HUD overlap or primary actions have passed scale-by-scale native
interaction. Task 5.4 and final task 15.3 remain open.

**Stock workspace checkpoint (2026-09-26, partial S03):** the opt-in capture
command now accepts an explicit workspace and records it in the manifest. Its
isolated Metal fixture captures `pmcab-stock-v2-1440` (en, 1440 × 900, 100%)
and `pmcab-stock-v2-900-pt` (pt-BR, 900 × 650, 130%) contain PPM, PNG and
manifest under the approved OpenCode temporary root. They show the real four
pieces across material groups, stable aliases separate from ranks, three
material counts, the empty HDF stock warning, a selected S1 inspector and an
incomplete rather than fabricated spending total. At 900 pt-BR the Stock
inspector collapses to a header drawer and the table is vertically and
horizontally scrollable. The current composition still differs substantially
from the handoff: material filters duplicate the left material list, the
table is wide, its action columns are off-screen until horizontal scrolling,
and editing remains modal rather than inline in the Stock inspector. This is
not a reference approval for 8.2/8.4. Focused pointer-driven egui tests do
exercise grouped and global drag, scoped keyboard/button routes, hidden-slot
preservation, global-position movement, undo and unchanged allocations for
8.3. Native pointer drag and drawer/menu interaction are still pending final
acceptance; the captures themselves filter interactive input.

**Task 5.4 pointer-driven egui follow-up (2026-09-26, partial M19 evidence):**
`cargo test --locked --bin plan-my-cabinet responsive_shell_drawers_menus_and_invalid_draft_survive_all_scales_and_locales`
passes for 1100 × 700 and 900 × 650 effective windows at each of 90/100/115/130%
in **both** en and pt-BR, plus 1440 × 900/100% in both languages (18
configurations). The test sends pointer press/release and Escape to the full
workspace UI, locates actual accessible button bounds, opens header and status
overflow in compact layouts, reopens/closes the inspector and controls drawers
where collapsed, opens viewport camera presets and the HUD action menu, and
checks that an invalid shared board draft and project data survive. Drawer
widths now follow their workspace's preferred pane widths, and a drawer state
is cleared when its pane becomes inline again. This is interactive **headless
egui** evidence, not native Metal pointer/keyboard observation; status popup
content and all actions were not individually activated, nor were long-name,
manual scroll/focus, or transient overlay collisions approved. The existing
native scale sweep above remains static. No complete M19/R01 or task 5.4
approval is claimed. During this follow-up `cargo check --locked --all-targets`
passed, but the full `cargo test --locked` failed in
`stock_ui::tests::stock_list_exposes_alias_name_rank_and_identity_to_accessibility`
and warnings-denied Clippy failed on `src/stock_ui.rs`'s collapsible `if`;
these were concurrent Stock workspace changes outside this task's edit scope.
`cargo fmt --check` initially reported concurrent formatting in
`src/stock_ui.rs`; a subsequent run passed after that file was formatted.

**Snap-control observation (2026-09-26, partial M06 evidence):** isolated
native Metal baseline captures in the approved temporary root at
`viewport-task66-static-1440` (English, 1440 × 900, 100%) and
`viewport-task66-static-900-pt130` (pt-BR, 900 × 650, 130%) show the default
face+grid chip with the actual fixture's 10 mm project spacing, including
reachability in the compact toolbar. Capture mode filters pointer/keyboard
events and fixes the selection/camera before its screenshot; those original
static captures could not show transient face-source/target or grid-target
highlights. An actual interactive drag and opened popover remain **missing**;
static captures and focused tests alone are not M06 approval.

**Task 6.3 transient snap visual evidence (2026-09-26, not R01/M06 approval):**
`viewport-task63-snap-face-final` and `viewport-task63-snap-grid-final` under
the approved OpenCode temporary root contain native Metal `capture.ppm` and
converted `capture.png`, `manifest.json` and `snap-state.json`. Both are en,
orthographic, 1440 × 900 logical at 100%, 2880 × 1800 raster / 2 pixels per
point, with `redesign_acceptance: false`. Their effective colored-fixture hash
is `813823a1a5890266125c12179f4487c4062b2b2964605cea4ca0805737202260`;
the original uncolored manifest hash remains separate. Source shelf
`72656465-7369-476e-8000-000000007f3a` and target bottom
`72656465-7369-476e-8000-000000005b17` are recorded with modes, eligible
face identities, free and chosen poses. Face shows cyan Source and magenta
Target guides; grid-only shows the magenta XY crosshair without face guides.
Both modes select face first; disabling face selects grid; Alt gives the free
pose with no candidate. The capture-only `MoveDrag` is resolved by the same
viewport candidate path and only overrides render geometry; no project,
revision, undo, app preference or production path is changed. Focused viewport
and capture tests provide the **automated interaction checks**, while these
images provide **scripted native transient visual evidence**. There was no
manual pointer interaction or opened popover captured. Full reference Design
composition and M06 interactive review remain pending. See
[viewport boundaries](redesign-viewport-boundaries.md#task-63-native-transient-snap-evidence-2026-09-26).

**Contrast follow-up:** the first transient captures exposed unreadable white
drag-status text against the pale canvas. The annotation now paints a clipped
dark status chip; fresh `viewport-task63-snap-{face,grid}-contrast` Metal
captures (1440 × 900 logical, 2880 × 1800 raster, frame 15) show the legible
status alongside the same source/target guides or grid crosshair. Their
sidecars still record real candidate selection, face-over-grid priority,
Alt bypass and `redesign_acceptance: false`. These remain scripted visual
evidence, not manual-drag or full Design-screen approval.

**Responsive shell observation (2026-09-26, not M19/R01 approval):** isolated
native Metal captures were produced at 1440 × 900/en/100%, 1100 × 700/pt-BR/115%,
and 900 × 650/pt-BR/130% using `--capture-baseline` and the reference fixture
(`redesign-reference-v1`, manifest hash
`900594f446536db092b666013f88d8b00b50549f8942771ae9ae7fbce7e35357`).
The artifacts are `pmcab-shell-5-4-{1440,1100-pt115,900-pt130}/capture.{ppm,png}`
and `manifest.json` under the approved OpenCode temporary root. The 900 × 650
capture records 2340 × 1690 raster pixels at ~2.6 pixels per logical point.
After correcting the compact labels, the 900 × 650 case was recaptured in
`pmcab-shell-5-4-900-pt130-final` and visibly shows “Inspetor” and “Mais” in
the header and “Mais” in the status strip, without header clipping.
After adding horizontal pane scrolling, the same configuration was recaptured
in `pmcab-shell-5-4-900-pt130-scroll`; the controls, camera rows and scene
remain visible and the narrow layout does not acquire a horizontal viewport
offset in its default state.
At baseline the 60-point rail, 256-point controls, 292-point inspector,
46-point header and 26-point status are present; the full status and direct
header actions are visible. At 1100 pt-BR the inspector remains alongside the
controls and viewport; that capture predates the corrected compact labels and
must be recaptured before localized overflow approval. At 900 pt-BR
the inspector collapses and has a labeled reopen button; the pane and camera
controls still render with a central scene. These are observational captures
of the default state, **not** interaction evidence for the drawer, status
popover or invalid-draft retention, and they do not demonstrate full Design
fidelity. The selected shelf is shown with the old inspector and no HUD. The
reference's projected overlay stack, revised inspector and viewport expansion
remain due in Design tasks 6.4–6.6 and native R01/M19 review. Pure shell-width
tests include the 90/100/115/130 scale stress matrix; the native images cover
only the configurations named above. Targeted shell and draft-retention tests
passed. The run was initially blocked by concurrent viewport/test edits and
in-progress export formatting. **After those edits settled**, the full
`cargo fmt --check`, `cargo check --locked --all-targets`, `cargo test --locked`,
warnings-denied all-target Clippy, diff check and strict OpenSpec validation
passed at this checkpoint (one pre-existing ignored test). This is regression
evidence, not drawer interaction or visual approval.

Read the [design](../openspec/changes/redesign-desktop-workspaces/design.md),
[tasks](../openspec/changes/redesign-desktop-workspaces/tasks.md), and
[handoff README](../design_handoff_egui_redesign/README.md). Use the HTML for exact
style values, the README for interaction intent, and the delta specs linked below
for resolved behavior. Main Window **2a** is the approved direction; 1a/1b/1c are
rejected explorations. Historical GUI waivers in [release evidence](release-checklist.md)
do **not** waive screenshot fidelity or native checks for this change.

## Capture and evidence contract

- Task 1.1 supplies the deterministic cabinet fixture: stable identities,
  representative geometry/materials/hardware, a genuinely unallocated back panel,
  and feasible sheet placements verified by the real domain witness. Record its
  version/hash and state modifications with each capture. Never use mock numbers
  or a fixed illustration in place of scene or sheet rendering.
- Native baseline tooling and verified invocation are documented in
  [redesign-capture.md](redesign-capture.md). It captures the current application;
  redesigned screen/state selectors and their acceptance remain pending. Use
  isolated fixture, configuration, recents, recovery, and output locations so
  captures do not modify production projects or user-local data.
- Baseline: macOS arm64, actual Metal rendering, English UI, 100% interface scale.
  Workspace content is 1440 × 900 logical points; Welcome is 1100 × 700; Settings
  is 780 × 560. Record native display scale/pixels-per-point and raster dimensions
  separately. Compare normalized logical bounds, not an unexplained Retina pixel
  size. Record camera/projection, selected stable IDs, focus, scroll/filter,
  overlays, modal state, packet settings, and fixture clock/provenance inputs.
- Use actual orthographic projection to match the illustrated Design camera,
  despite the HTML's selected Persp chip. Separately verify real perspective
  rendering and picking. Hardware motion must use the configured relationship
  limit; dimensions, counts, prices, dates, versions, and receipt history must
  come from the seeded state or be honestly unavailable.
- For every comparison retain the native PNG, reference filename, bounds/diff
  review, interaction steps and observations, source revision, host/OS/GPU,
  capture setup, reviewer/date, and explicit pass/fail/blocked outcome. For
  headless evidence retain exact test command, test names, output, source revision,
  and observed outcome. A blank artifact field means pending, never passed.
- Target panel/control geometry within **2 logical points** of the approved
  reference and match specified color, type size/weight, icons, spacing and radii.
  Check the 60-point rail, 46-point header, 26-point status bar, and each screen's
  pane widths. Record bounded, reviewed exceptions for rasterization/shadows or
  truthful corrected data with reason and affected region. These are not blanket
  exemptions for layout drift, absent controls, missing states or placeholders.
- Keep a correction ledger beside each review: illustrated value, actual value,
  derivation/witness or provenance, affected references, and reviewer decision.
  Include utilization/cut counts/loss/costs, effective thickness provenance,
  displayed versus snapping grid interval, all five rail entries (including Stock),
  unsupported hardware evidence, and unknown historical metadata. Do not invent
  a confirmation/export date to resemble the reference.

## Ten-screen comparison matrix

PNG sizes below were read from the supplied files' PNG headers. Links point to
the existing reference assets, **not** native acceptance artifacts. Each row's
capture and headless check is required and currently pending. Settings rows
represent the modal content bounds; capture enough surrounding native context
as additional evidence to verify isolation and placement.

| ID / reference path | Supplied pixels / intended logical size | Intended data and state | Required native capture and interaction evidence | Required headless evidence |
| --- | --- | --- | --- | --- |
| R01 [01-design-workspace-2a.png](../design_handoff_egui_redesign/screenshots/01-design-workspace-2a.png); [Main Window.dc.html](../design_handoff_egui_redesign/designs/Main%20Window.dc.html), 2a | 1440 × 900 / 1440 × 900 | Deterministic cabinet, allocated shelf active inside its assembly, back panel unallocated, orthographic illustrated camera, real material/stock summaries. | 256-point outliner and 292-point inspector; five-entry rail, native shaded scene, selection dimension pill, HUD, snap popover and miniature sheet. Edit through HUD and inspector; follow shelf to its sheet. | Shared draft/one undo/exact quantities; typed selection routing; camera/picking agreement; hidden descendants; Body/Overall measurement and snap/preset invariants. |
| R02 [02-cut-plan.png](../design_handoff_egui_redesign/screenshots/02-cut-plan.png); [Cut Plan.dc.html](../design_handoff_egui_redesign/designs/Cut%20Plan.dc.html) | 1440 × 900 / 1440 × 900 | Focus the shelf's allocated sheet with a current witness; all overlays enabled; back panel issue visible; actual costs and metrics. | 256/308-point side panes; rulers, labels, kerf, hatching, cut markers; hover a sequence row and select the matching part; switch sheet and enter Repair. | Physical sequence/input/output/reference-edge correspondence; full area accounting; diagnostics; all-affected-sheet repair; optimization comparison/cancel/stale behavior. |
| R03 [03-stock-and-materials.png](../design_handoff_egui_redesign/screenshots/03-stock-and-materials.png); [Stock and Materials.dc.html](../design_handoff_egui_redesign/designs/Stock%20and%20Materials.dc.html) | 1440 × 900 / 1440 × 900 | Grouped inventory with selected piece, purchased and owned pieces, unused stock, material without stock and unknown cut fee. | 256/316-point side panes; table/card hierarchy, trims diagram, warning row and incomplete estimate. Exercise filter, scoped drag, keyboard reorder, global position and Open in cut plan. | Alias persistence/non-reuse independent of ownership/rank; subset reorder preserves hidden slots and placements; exact trim/grain edits; unknown versus zero costs. |
| R04 [04-dialogs.png](../design_handoff_egui_redesign/screenshots/04-dialogs.png); [Dialogs.dc.html](../design_handoff_egui_redesign/designs/Dialogs.dc.html) | **1568 × 1112 reference sheet** / six separate modals over a 1440 × 900 workspace | New board with real first-fit preview; Position tentative pose; Place face to face with selected faces; Resize N boards with mixed values/rounding; New material with color; Unsaved changes on dirty work. | Capture **all six** individually at reference modal widths (400–520 points), compare each reference region, retain backdrop/context. Exercise Tab/Shift-Tab, valid Enter, popup Enter/Escape, cancellation and restored focus. | Modal input isolation; first-fit preview never reserves stock; nested material cancellation; rounding consent reset; atomic batch/placement commit and rollback; save/discard/cancel protections. |
| R05 [05-shop-handoff.png](../design_handoff_egui_redesign/screenshots/05-shop-handoff.png); [Shop Handoff.dc.html](../design_handoff_egui_redesign/designs/Shop%20Handoff.dc.html) | 1440 × 900 / 1440 × 900 | Draft selected, Shop-ready blocked by real back-panel issue, actual prepared A4 pages, independent output language/units, all sections initially on. History only from genuine fixture receipts with known provenance. | 300/300-point side panes; page thumbnails, preview, readiness/Fix and history. Compare native pages with rendered exported PDF at multiple zooms, including long pt-BR content; use actual page count rather than illustrated four. | Shared positioned-page/pagination parity; mandatory content with all sections off; frozen reviewed snapshot export; failure creates no receipt; historical unknowns and independent manufacturing/hardware freshness. |
| R06 [06-hardware.png](../design_handoff_egui_redesign/screenshots/06-hardware.png); [Hardware.dc.html](../design_handoff_egui_redesign/designs/Hardware.dc.html) | 1440 × 900 / 1440 × 900 | Pinned catalog, real door relationship/installation selected; valid display-only opening (60° if supported), actual K/R/reference data and limitations. | 268/316-point side panes; linked tree, projected cup/plate/axis guides, diagram, motion HUD. Scrub, choose Closed, leave/return; inspect sources offline. | Independent mounting coordinates/faces, valid/unsupported evidence, local-to-display annotation derivation, coherent moving root, no manufacturing edits, exit resets motion, guarded relationship drafts. |
| R07 [07-welcome.png](../design_handoff_egui_redesign/screenshots/07-welcome.png); [Welcome.dc.html](../design_handoff_egui_redesign/designs/Welcome.dc.html) | 1100 × 700 / 1100 × 700 | Isolated real recent entries, missing-file entry, validated newer recovery candidate and saved-state thumbnails or honest placeholders; actual build version. | 340-point left column, recovery comparison/actions, recent rows, language/preferences and Base/Wall/Drawers tiles. Exercise Locate/Remove, Recover/Decide later/Discard and native Open. | Successful-open/save registration only; canonical path/UUID recovery association, untouched files on cancel/remove, nonfatal thumbnail failure, template first-run bootstrap and replacement guards. |
| R08 [08-settings-cutting.png](../design_handoff_egui_redesign/screenshots/08-settings-cutting.png); [Settings.dc.html](../design_handoff_egui_redesign/designs/Settings.dc.html), Cutting | 780 × 560 / 780 × 560 | Cutting selected, exact fixture kerf, dated confirmation only if recorded; actual cutting-model limitations and examples link. | 210-point section list; confirmation explanation, field, model and Done. Change/reconfirm kerf, undo; exercise examples/help. | Confirmation bound to exact value/date; change clears provenance and revalidates without resizing; undo/redo restores; legacy date unavailable. |
| R09 [09-settings-costs.png](../design_handoff_egui_redesign/screenshots/09-settings-costs.png); [Settings.dc.html](../design_handoff_egui_redesign/designs/Settings.dc.html), Costs & currency | 780 × 560 / 780 × 560 | Unknown cut fee, actual currency/prices and incomplete estimate, Costs section selected. | Unknown placeholder, Free (0), Change currency flow and estimate card; keyboard/Done behavior and footer semantics. | Unknown distinct from known zero; explicit relabel/replace without conversion; validation/cancel/undo and spending completeness. |
| R10 [10-settings-general.png](../design_handoff_egui_redesign/screenshots/10-settings-general.png); [Settings.dc.html](../design_handoff_egui_redesign/designs/Settings.dc.html), General | 780 × 560 / 780 × 560 | General selected, explicit baseline language/hints/inverse zoom/material tint/100% scale; isolated app-local recovery/configuration. | Toggle actual hint/zoom/tint behavior; restart persistence; recovery-folder and cleanup review; all four scales and pt-BR adaptation. | Atomic config/fallback/write errors; no project revision/history/freshness changes; cleanup unselected files preserved; preferences available without a project. |

## Missing-state matrix

These states are required additions to the ten happy-path illustrations. All
rows are **pending native and behavioral evidence**. Capture each distinct
visual state and record the sequence that reaches it; assertions alone cannot
establish its legibility or accessibility.

| ID | State / surface | Native evidence to capture | Behavioral acceptance |
| --- | --- | --- | --- |
| M01 | Empty project; no selection; no stock/materials/receipts | All five workspaces and first-run Welcome show useful creation routes, no sample content or single-board HUD. | Navigation leaves project unchanged; no fake counts, receipts or witnesses. |
| M02 | Assembly/multi-selection, hidden board, long/duplicate names | Expanded/hidden outliner, distinct active/secondary selection, appropriate inspector and batch resize; retained warning/visibility controls. | Hierarchy operations remain reachable; descendant deduplication, typed identities, reveal action and hidden manufacturing demand preserved. |
| M03 | Invalid draft, exact untouched value, rounding consent | HUD/inspector share text/errors; advanced effective-thickness provenance; mixed per-board resize preview. | Untouched 12.345 mm stays exact; `1/64 in` requires 0.397 mm consent; new proposal clears consent; one acceptance gives one undo; cancel preserves allocations. |
| M04 | Dirty navigation, target change, panel collapse | Apply/Discard/Stay and Accept/Cancel/Stay with invalid acceptance disabled; reopen collapsed panel. | Preserve text/validation/consent/scroll; revalidate resumed target/revision; failed apply stays; text Delete/Undo never reaches scene. |
| M05 | Command palette empty/no-match/duplicate/removed/blocked targets | Grouped results, identity context, reason for unavailable actions and visible keyboard focus. | Command-K, arrows, Enter/Escape and focus restore; same action guards as controls; removed targets do not act on unrelated objects. |
| M06 | Camera, tools, snaps, pose presets and missing extents | Genuine perspective/ortho, Iso/Front/Right/Top; Body/Overall and frame labels; snap-only modes/Alt bypass; cancellable preset frame/pivot. | Render/pick agreement after resize; no camera edits; face precedence; spacing never quantizes existing poses; unsupported hardware extents reported unavailable. |
| M07 | Modal errors, popup/nested layers, destructive decisions | Six reference dialogs plus stock, fee/currency/kerf/grid, preserve/apply, hierarchy, hardware/relationship, delete/overwrite and recovery dialogs. | Tab boundary; popup consumes Enter/Escape once; no background pointer/scroll/shortcut effects; affected-object disclosure and cancel restoration. |
| M08 | Filtered stock, global priority, unused piece, invalid trims | Empty material row, Unknown grain, exact measured thickness, overflow/long names and incomplete/free costs. | Scoped reorder keeps hidden slots fixed; global move available by keyboard; aliases survive quantity/duplicate/delete/undo/save/load without reuse; no automatic repack. |
| M09 | Conflict, absent/stale witness, proof exhaustion | Retained conflict locations, deduplicated issue rows including hidden boards, truthful unavailable metrics and unused sheet. | Distinguish material/thickness/grain/bounds/overlap/kerf/trim/missing/duplicate/cut-model/search-limit reasons; contextual Add/Reveal/Repair, never false verified/zero metrics. |
| M10 | Repair invalid intermediate state and cancellation | Drag ghosts plus numeric transfer/rotation/unallocation/lock controls and affected-sheet diagnostics; leave prompt. | Stage multiple operations; consent on numeric rounding; validate all affected sheets before accept; Escape/Cancel restores whole session; Stay retains drafts. |
| M11 | Optimization busy/no complete result/unchanged/compare/stale | All three objectives, progress/source, shell activity across workspaces, current-versus-best metrics and disabled stale acceptance. | Cancel and late results never apply; one-transaction accept; locked placements respected; incomplete costs cannot claim lowest spend; manufacturing edits stale, color-only edits preserve newer metadata. |
| M12 | Hardware unsupported/missing evidence/invalid relationship | Warned and standalone installations/reference hardware, independent mounting inputs, unavailable diagram values, actual motion endpoint. | Offline snapshots only; no fabricated presets/drilling; references invalidated by board edits; cycle/self-reference rejection; dirty relationship Stay; atomic detach/deletion. |
| M13 | Handoff busy/stale/all sections off/dense pages | Cancellable preparation, refreshed preview, mandatory issues with detail off, long en/pt-BR multipage packet at multiple zooms and all units including m/ft. | Same positioned pages/content as PDF; zoom never repaginates; hidden unallocated board blocks Shop-ready; output language independent; reviewed frozen packet and write verification. |
| M14 | Export cancel/overwrite refusal/write failure; legacy/history | Actionable failure, unchanged receipt list; unknown mode/date/sections/baseline; superseded versus outdated; supported Since then text. | No receipt before successful complete write; exported files immutable; appearance dirty state distinct from manufacturing freshness; included hardware stales only dependent guidance. |
| M15 | Recents empty/missing/invalid; recovery saved/untitled/copied path | Locate failure, unknown metadata, thumbnail placeholder; saved-versus-recovery comparison or honest untitled identity; cleanup with no preselection. | Remove affects only recent entry; save survives thumbnail failure; defer retains candidates; recovered work unsaved/Save As; no cross-path adoption, arbitrary scan or automatic age deletion; partial cleanup failures preserve unselected files. |
| M16 | Base/Wall/Drawers first-run setup, invalid geometry and replacement failure | Material-role creation, nested material cancel, datum/clearance/BOM review, three-drawer boxes/fronts, unallocated result and Stock route. | Positive dimensions/formulas; no implicit purchase; staged setup leaves old document intact; Save/picker failure retains setup; one undo includes materials/assembly, fresh IDs, no recent before save, no parametric propagation or fit certification. |
| M17 | Legacy v1, unsupported future file, metadata bounds | Honest neutral colors/unknown provenance, save-upgrade notice and non-destructive errors. | Exact IDs/quantities/poses/allocations/prices/pinned snapshots/receipts preserved; in-memory migration no rewrite/dirty-on-open; strict bounds/document-size errors; atomic explicit v2 save and rollback limits. |
| M18 | Settings missing reference sections/no project/config errors | Grid & units, Shortcuts, About, preferences without project; actual platform/version; malformed-config/write error; Done/Escape and cleanup. | mm/cm/m/in/ft no-revision/no-undo presentation; pending quantity/consent survives language/unit change; project edits undoable, app settings atomic and separate. |
| M19 | Responsive/localized/disabled/offline | 1100 × 700 and 900 × 650 effective workspace windows; Welcome/Settings reference sizes and constrained areas; en/pt-BR at 90/100/115/130%; normal/selected/warning/disabled icons and typography gallery. | No unreachable controls/overlay collisions/lost drafts/raw translation keys; keyboard/trackpad camera without middle button; offline bundled fonts/icons/licenses, visible focus and accessible icon names. |

## Delta capability coverage (all nine)

Each spec link is the authoritative scenario list. The checks here cover every
requirement family; implementations must attach scenario-specific test names and
native observations rather than treating a capability row as a blanket pass.
Existing suites named below have not been rerun as evidence for task 1.3.

| Delta capability | Visual / behavioral acceptance mapping | Existing regression anchors; additional headless coverage required |
| --- | --- | --- |
| [desktop-workspaces](../openspec/changes/redesign-desktop-workspaces/specs/desktop-workspaces/spec.md) | R01–R06, M01–M05, M11, M19: five-entry truthful shell, linked typed context, edit resolution, action parity/search, complete native fidelity. | Add action inventory/availability, selection/view-state lifetime, pending-navigation/async guards and empty-state tests. |
| [desktop-and-localization](../openspec/changes/redesign-desktop-workspaces/specs/desktop-and-localization/spec.md) | R04, R07–R10, M03–M07, M18–M19: unified modal boundary, adaptive reachability, persistent preferences, units, localized offline help/assets. | Existing desktop/localization tests via full suite; add popup/focus/input-isolation, collapse retention, atomic config/error, unit/language draft preservation and key parity checks. |
| [assembly-editor](../openspec/changes/redesign-desktop-workspaces/specs/assembly-editor/spec.md) | R01, R04, M02, M06: outliner/inspector/HUD, camera, frame-aware measurement, independent snaps, cancellable pose presets. | [cabinet_assembly.rs](../tests/cabinet_assembly.rs), [release_cabinet.rs](../tests/release_cabinet.rs); add projection/picking, selection/overlay, measurement and one-transaction preset checks. |
| [boards-and-materials](../openspec/changes/redesign-desktop-workspaces/specs/boards-and-materials/spec.md) | R01, R04, M03, M07, M17: shared exact drafts, effective thickness/advanced editor, nonmutating first-fit creation preview, manufacturing-neutral colors. | [cabinet_assembly.rs](../tests/cabinet_assembly.rs), [allocation_invalidation.rs](../tests/allocation_invalidation.rs); add draft/rounding, preview-current-state, color save/load/undo/freshness tests. |
| [stock-allocation](../openspec/changes/redesign-desktop-workspaces/specs/stock-allocation/spec.md) | R02–R03, M08–M10: inventory/inspector, aliases, scoped/global reorder, diagnostic navigation, complete repair and honest costs. | [stock_walkthrough.rs](../tests/stock_walkthrough.rs), [allocation_invalidation.rs](../tests/allocation_invalidation.rs); add alias non-reuse/migration, filtered-slot ordering and cross-workspace repair checks. |
| [cut-planning-and-costs](../openspec/changes/redesign-desktop-workspaces/specs/cut-planning-and-costs/spec.md) | R02, R08–R09, M09–M11: witness canvas, linked physical sequence/statistics, all optimizer outcomes, exact kerf provenance. | [optimization_performance.rs](../tests/optimization_performance.rs), [shop_handoff.rs](../tests/shop_handoff.rs); add read-model/witness consistency, hover identity, confirmation undo and manufacturing-key candidate applicability. |
| [hardware-and-motion](../openspec/changes/redesign-desktop-workspaces/specs/hardware-and-motion/spec.md) | R06, M02, M06, M12: pinned catalog/tree, full mounting inspector, derived annotations, scoped display motion, relationship transactions. | [release_cabinet.rs](../tests/release_cabinet.rs) plus existing hardware/relationship unit tests; add annotation projection, independent coordinates, actual-limit/exit reset, navigation draft and removal checks. |
| [project-foundation](../openspec/changes/redesign-desktop-workspaces/specs/project-foundation/spec.md) | R07, M15–M17: recents/optional thumbnails, explicit recovery, complete one-time templates/first-run bootstrap, compatible metadata. | [portability.rs](../tests/portability.rs), [release_cabinet.rs](../tests/release_cabinet.rs); add v1 golden migration, local recents/recovery failure cases, recipe formulas/BOM and atomic staged-material generation/replacement tests. |
| [workshop-outputs](../openspec/changes/redesign-desktop-workspaces/specs/workshop-outputs/spec.md) | R05, M13–M14: mandatory printable content/optional sections, readiness/output controls, shared pagination, honest receipts and independent freshness. | [shop_handoff.rs](../tests/shop_handoff.rs), [release_cabinet.rs](../tests/release_cabinet.rs); add positioned-page parity/dense layout, frozen export/error, bounded historical baselines and color/hardware freshness tests. |

## Verification by implementation stage

Run focused behavioral tests as each stage lands and record their actual results.
Pair each visual delivery with native comparisons at that stage; do not postpone
all fidelity work to a final cosmetic review. Headless egui tests, process starts,
builds, and HTML screenshots cannot substitute for native Metal captures.

| Tasks / stage | Required checks before accepting that stage |
| --- | --- |
| 1: fixture/capture foundations | Stable IDs, real witness/accounting, documented corrected literals; isolated native tooling verified against the current application surface. Redesigned workspace, Welcome and Settings captures are verified after those screens exist (tasks 6.8, 12.6, 14.5 and 15.2), as approved in the task-order correction. See the separate fixture and capture evidence records; this matrix alone is not a pass. |
| 2: compatibility | Golden v1/migration/metadata/freshness tests; exact save/load/undo and source-byte preservation; native compatibility/unknown-provenance states (M17). |
| 3–4: theme/actions/drafts | Widget/icon/font gallery, modal and action parity, draft/precision/navigation tests; native keyboard/focus, tokens, all 40 icons and bundled font weights (M03–M07, M19). |
| 5–6: shell/Design | Selection/view lifetime, projection/picking/drag/snap/measurement tests and existing cabinet regressions; native R01 plus palette, empty/multi/hidden/invalid and responsive states. |
| 7: dialogs | Atomic preview/commit/cancel/rounding tests and modal inventory; six R04 captures and nested-popup/destructive/remaining-dialog checks. |
| 8–9: Stock/Cut plan | Existing allocation/stock/performance suites plus aliases, reorder, witness metrics, repair and optimizer freshness; native R02–R03 and M08–M11. |
| 10: Hardware | Existing installation/relationship/deletion tests plus annotation and exit-reset cases; native R06 and M12 with offline source access. |
| 11: Handoff/documents | Shared layout/pagination, mandatory sections, receipts and export-failure tests; native R05, dense en/pt-BR preview versus rasterized PDF and M13–M14. |
| 12–13: Welcome/templates | Atomic persistence/recovery and recipe/first-run/replacement tests; native R07, picker and recovery flows, complete Base/Wall/Drawers review (M15–M17). |
| 14: Settings | Config/restart/units/kerf/cost/localization tests; native R08–R10 plus missing sections, all scales, pt-BR and no-project access (M18–M19). |
| 15: integrated/release | Create/template → Design → Stock → repair/optimize → Hardware → preview/export → save/reopen; all R/M rows reviewed, all delta scenarios evidenced, offline packaged macOS arm64 launch/assets/pickers and performance observations recorded. |

Final automated gates, as required by task 15.4:

```sh
cargo fmt --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

Also run the existing dense-sheet and optimization fixtures documented in
[development](development.md#optimization-performance-fixture-task-86), keeping
the 100-board workload. Record first/warm frames and cancellation observations
against that baseline on the tested machine; test hang guards are not interactive
performance acceptance thresholds. Record native frame/pointer responsiveness
separately. Use the existing [packaging instructions](development.md#offline-release-artifacts-task-113)
for the actual release build, verifying new fonts/icons/licenses offline rather
than assuming the old package inventory covers them. Linux remains experimental.

## Result recording and completion gate

For each R/M ID, attach an evidence entry with: tested source revision and fixture
identity; native capture paths and environment; headless command/log paths;
expected/actual behavior; measured geometry/token deviations; corrected-data
ledger; reviewer/date; result and follow-up. Keep failed/blocked/not-run entries
visible until resolved. Renderer exceptions require bounded recorded review and
cannot waive screenshot fidelity. Completion requires all ten references (and
six individual R04 modals), missing states, nine capability scenario families,
native interactions, automated gates and offline package checks. The presence
of this plan does not complete those gates.

Task 1.3 documentation validation: all ten reference PNGs and their eight HTML
sources are present; PNG dimensions were read from their headers. Local links in
this document and the two entry-point additions are checked for existing targets.
This is inventory/link validation only, not a screenshot review or a native pass.
