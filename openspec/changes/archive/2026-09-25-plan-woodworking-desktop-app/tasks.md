# Tasks

The completed implementation tasks below have automated and documented evidence. The first-release target is macOS arm64. The user explicitly removed Linux release acceptance and waived interactive/independent clean-machine macOS validation for this change; those checks are **not recorded as passed**. See `docs/release-checklist.md` for tested evidence and unverified limitations. Proposed module names in design.md are organizational guidance, not new public API contracts.

## 1. Native application and headless foundation

- [x] 1.1 Introduce a library/desktop binary boundary in the existing Rust package, select and lock compatible eframe/egui/wgpu and core serialization dependencies, and verify `cargo check --all-targets` plus a headless library test run without GPU initialization.
- [x] 1.2 Build the macOS Metal native shell, high-DPI-aware layout and unsupported-adapter diagnostics; verify build-host process startup and headless resize/close behavior. Actual window interaction and high-DPI rendering remain unverified by the waived GUI check. The existing Linux backend is experimental.
- [x] 1.3 Add CI formatting, Clippy and headless test jobs; verify `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` on the macOS release host, and document local build prerequisites. Linux CI remains supplemental, not first-release acceptance.
- [x] 1.4 Add the 3D callback with depth-tested boxes, outlines, axes and grid; verify depth/overlay scene fixtures in automated tests. A visible Metal-window inspection remains unverified by the waived GUI check.

## 2. Measurements, localization and project identity

- [x] 2.1 Implement checked integer manufacturing lengths, exact unit conversion, 0.001 mm manufacturing precision and separate bounded f64 spatial poses; verify mm/cm/m/in/ft, mixed inch fractions, overflow, coordinate bounds, zero/negative dimensions, half-grid centred resize and the 1/64 inch rounding-confirmation scenario with unit tests.
- [x] 2.2 Implement ungrouped comma/dot input, parsed-value previews and locale-aware display without focus/blur rounding mutations; verify ambiguous mixed separators are rejected and English/pt-BR parse identical physical values.
- [x] 2.3 Add single-currency integer money values, missing-versus-zero prices and explicit currency relabelling/replacement; verify changing UI language never converts values and mixed-currency inputs are rejected.
- [x] 2.4 Add English and pt-BR translation resources, fallback behaviour and font assets with recorded licenses; verify translation-key coverage, pluralized errors and Portuguese glyph display, and document input grammar and supported units in both languages.
- [x] 2.5 Define stable project/object IDs and versioned records for materials, boards, assemblies, stock, allocations and hardware references; verify duplicate names do not merge objects and invalid/dangling references are detected by headless validation.

## 3. Atomic editing and local persistence

- [x] 3.1 Implement validated edit transactions with undo/redo, redo-branch invalidation, transient previews and consistent revision tracking; verify a cancelled preview creates no undo entry and batch edits undo in one action.
- [x] 3.2 Implement version-1 `.pmcab` JSON serialization with catalog snapshots and validation of references, cycles and numeric values; verify cross-locale round trips preserve IDs and quantities, and newer/invalid documents do not replace unsaved work.
- [x] 3.3 Implement same-directory temporary writes, flush/atomic replacement and Save As cancellation; verify injected permission/storage/write failures preserve the last saved file and the in-memory dirty revision.
- [x] 3.4 Implement separate autosave recovery after the configured inactivity interval and recover/discard/defer choices; verify interrupted-session recovery never overwrites explicit saves and uncommitted previews are absent.
- [x] 3.5 Document file portability, session-local undo, save/recovery and compatibility handling in English and pt-BR; verify an offline versioned-file save/reopen round trip and headless multi-architecture serialization. The native desktop file controls have automated macOS tests; user-driven picker and cross-platform GUI transfer remain unverified by waiver.

## 4. Boards and precise dimension editing

- [x] 4.1 Implement materials and New board with local length/width and material-derived thickness; verify positive/finite validation and that cancelled/invalid creation leaves no partial board or allocation.
- [x] 4.2 Implement explicit material property changes with existing-dependent preservation/update choices and measured stock values never updated implicitly; verify compatibility keys use identity plus effective thickness, 18-to-15 mm changes affect only confirmed boards, preserved board/stock allocations survive save/load and incompatible allocations are flagged.
- [x] 4.3 Implement lengthwise plywood grain defaults and length/width/unrestricted overrides; verify overrides survive assembly rotations, material-default edits and save/load.
- [x] 4.4 Implement start/centre/end anchored dimension changes in local coordinates; verify end-face preservation inside rotated parent assemblies and symmetric centre-anchored thickness changes.
- [x] 4.5 Implement independent board duplication and atomic multi-selection dimension editing with mixed-value display and de-duplicated target lists; verify fresh IDs, no shared allocations, no propagation to originals and all-or-nothing invalid batches.
- [x] 4.6 Document New board, dimensions versus orientation, grain and resize anchors in both languages; verify a keyboard-only walkthrough creates and edits two independent boards as described.

## 5. Assembly interaction and nested structure

- [x] 5.1 Implement mouse/trackpad/keyboard orbit, pan, zoom, frame selection, perspective and orthographic views; verify navigation controls with automated macOS egui input tests and document shortcuts. Native device interaction remains unverified by the waived GUI check.
- [x] 5.2 Implement ray-based face/part picking, object-list selection, multi-selection and highlighted active targets; verify front/back and occluded-part selection cases using scene fixtures.
- [x] 5.3 Implement numeric poses, drag previews, temporary snapping with bypass, and face-to-face placement with alignment/offset controls; verify cancel restores the pre-edit pose and moving a former snap target does not move the placed part.
- [x] 5.3a Add a visible world-XY grid with project-local configurable spacing (10 mm default), metric/imperial input and backward-compatible persistence; preview grid snaps in Move board mode after preferred face snaps, label the target, and bypass both with Alt. Verify existing and sub-micrometre poses are unchanged on setting edits/load, invalid spacing and cancellation are non-mutating, saving/reopening preserves spacing, and accepting a snap is one undoable pose with no constraint. Document the controls in both languages.
- [x] 5.4 Implement nested grouping/reparenting/ungrouping with world-pose preservation, cycle rejection and one-transform-per-selected-subtree semantics; verify a parent-plus-child 100 mm translation moves the child exactly once.
- [x] 5.5 Implement assembly duplication with fresh descendant IDs and mapped internal references, group visibility and object-list reveal; verify duplicates are independent and hidden boards remain in fabrication demand.
- [x] 5.6 Add scoped/frame-labelled measurements for body-only versus overall selections and focus-safe shortcuts; verify feet inside bounds are not double-added, Delete in text fields never deletes a part, and modal dialogs block scene edits.
- [x] 5.7 Document assembly versus mechanical relationships and precision-placement workflows in both languages; verify a cabinet-like assembly can be built without invoking a cabinet preset.

## 6. Stock records and cutting feasibility engine

- [x] 6.1 Implement individual project-local stock pieces, quantity expansion, priority ordering, ownership/prices, grain and trim settings; verify two same-sized offcuts remain distinct and never reserve stock in another project.
- [x] 6.2 Implement validated full-span cut trees with full-in-stock kerf bands, trim operations, exact part leaves and offcut/waste leaves; verify 205 = 100 + 5 + 100, failure at 204 mm, exact-stock zero-cut cases, 100-from-105 edge shaving, explicit 100-from-103 limitation, four-edge trim loss without double-counted corners, and rejection of trim allowances below kerf.
- [x] 6.3 Implement parent-before-child numbered operations with reference edge, retained dimension, kerf side and intermediate identities; verify each input exists when referenced and every physical split contributes exactly one cut.
- [x] 6.4 Implement cut-tree area accounting and stock/part compatibility checks; verify property-based fixtures conserve area exactly and reject material/thickness/grain mismatch and part/kerf overlap.
- [x] 6.5 Implement bounded recursive guillotine witness reconstruction for fixed placements, including isolation cuts for a lone part; verify known valid arrangements and non-slicing layouts, and distinguish budget exhaustion from proven rule violations.
- [x] 6.6 Document kerf, total-loss trim allowances, full-span-only cuts, no stacking and shop review assumptions in both languages; verify worked examples reproduce the engine's dimensions and counts.

## 7. Linked allocation and direct sheet editing

- [x] 7.1 Implement deterministic first-fit on part creation/duplication using declared stock order and compatible orientations; verify existing placements never move, purchased stock is not invented, and a failed attempt leaves a usable unallocated board.
- [x] 7.2 Build the linked 2D sheet/3D selection views with grain arrows, part labels and conflict overlays; verify clicking a part highlights the same identity and sheet moves never alter assembly poses.
- [x] 7.3 Add sheet drag/numeric placement, permitted rotations, cross-sheet transfer, unallocation and locks with cut-witness previews, including atomic multi-placement repair; verify two independent conflicts can be repaired in one session, invalid intermediate previews can be cancelled, explicit unallocation is never trapped by unrelated conflicts and locked placements require unlocking.
- [x] 7.4 Implement invalidation on board/material/grain/stock/kerf/trim edits while preserving visible draft positions; verify a locked resized shelf can remain conflicted and one undo restores its previous feasible state.
- [x] 7.5 Add global allocation diagnostics and targeted resolution actions for hidden and visible boards; verify all required parts are counted once and a user can locate and resolve an unallocated hidden part.
- [x] 7.6 Document first-fit ordering, direct placement, conflict states and locks in both languages; verify a create-resize-conflict-resolve-save-reopen walkthrough preserves both assembly and allocation state.

## 8. Cost estimates and explicit optimization

- [x] 8.1 Implement full-sheet purchase expense plus flat physical-cut charges, with owned stock excluded from new purchase spending; verify the 200 + 6 x 5 = 230 example and missing-price versus explicit-zero cases.
- [x] 8.2 Implement deterministic bounded candidate generation with stock/part orderings and guillotine splits; verify every accepted candidate has complete compatible allocations and an independently revalidated cut witness.
- [x] 8.3 Implement lowest-spending, fewest-cuts and least-unused-area ranking with documented tie-breaks; verify incomplete assignments/prices cannot win a cheapest-complete claim and utilization/offcut summaries match cut-tree accounting.
- [x] 8.4 Preserve locked positions during optimization and retain the feasible incumbent; verify candidates cannot move a locked part or silently replace the current layout when no improvement is found.
- [x] 8.5 Integrate worker cancellation, immutable input tokens, comparative previews and one-transaction acceptance; verify edits during search make results stale and cancellation leaves history and placements unchanged.
- [x] 8.6 Exercise a documented 100-part/10-stock performance fixture with a five-second search budget and 250 ms cancellation target on test machines; record timings and verify the UI remains interactive rather than claiming universal timing guarantees.
- [x] 8.7 Document cost exclusions, heuristic limits, objective definitions and candidate acceptance in both languages; verify UI claims use 'best found' and unknown cost never appears as zero.

## 9. Draft and shop-ready outputs

- [x] 9.1 Implement manufacturing fingerprints and immutable export snapshots with successful-export records; verify dimension/kerf/price/label changes invalidate the relevant output while camera, selection, hiding and motion previews do not.
- [x] 9.2 Implement shop-ready gating against complete current allocations and cut witnesses, plus draft issue summaries; verify hidden/unallocated/conflicted boards block shop-ready but can appear in a prominently marked draft.
- [x] 9.3 Implement PDF stock summaries, parts lists, sheet drawings, grain arrows, cut numbering, intermediate-piece references, costs and assumptions using embedded fonts; verify extracted values/labels match the source snapshot and physical cut count.
- [x] 9.4 Add pagination, declared drawing scale, not-a-template warnings, kerf-side notation and repeated draft watermarks; visually review dense and simple fixtures in both languages for legibility, missing glyphs and ambiguous dimensions.
- [x] 9.5 Implement independent export language/units and atomic output writing with overwrite confirmation; verify Portuguese output from an English UI and failed writes preserve prior files without adding a success record.
- [x] 9.6 Document the shop handoff and stale-revision workflow in both languages; verify an end-to-end cabinet fixture can be designed, allocated, exported, edited and flagged out of date without altering the previously exported PDF.

## 10. Curated hardware and approximate motion

- [x] 10.1 Visually verify the FGVTN Click 3D Slow Reta/Calço 0 candidate's exact kit/plate pairing, overlay applicability, door thickness, cup diameter/depth/edge reference, plate-position references and 105-degree limit against manufacturer documentation; deliver a field-to-source/revision and rights review record, explicitly marking optional missing fastener dimensions and stopping for clarification if any mandatory baseline field cannot be verified.
- [x] 10.2 Implement the structured catalog and pinned project snapshots using only verified factual data and redistributable assets; verify offline reopening ignores external catalog changes and an explicit update revalidates dependent installations.
- [x] 10.3 Implement simple foot/reference hardware with dimensioned geometry and separate hardware lists; verify it affects selected overall bounds but never stock demand or wooden cutting charges.
- [x] 10.4 Implement door/mounting-board selection, compatible hinge/plate configuration, installation parameter previews and annotation placement; verify unsupported thickness, unavailable dimensions and references outside board bounds produce clear warnings rather than invented drilling instructions.
- [x] 10.5 Implement one explicit joint per moving door root with coherent hinge instances, cycle prevention and dependency cleanup; verify a door and handle move together while the mounting board remains fixed, including nested assemblies and undoable deletion.
- [x] 10.6 Implement angle-bounded fixed-axis approximate previews with persistent disclosure and restoration of closed pose; verify previews do not mutate measurements, allocations, saved closed poses or cutting revisions and never report verified clearance.
- [x] 10.7 Integrate hardware references/lists into persistence and workshop output with their own staleness checks and limitations; verify adding a foot or changing a hinge identifier makes the packet outdated without changing wood-cut validity, and invalid numeric installation diagrams are omitted with explicit notices from an otherwise valid cutting packet.
- [x] 10.8 Document the supported catalog entry, missing data, hinge-count/load limits and approximate-motion caveat in both languages; verify the in-app guidance and exported references match the verified record.

## 11. Integrated release acceptance

- [x] 11.1 Run the complete rectangular-cabinet workflow, including feet, door hardware, duplicate boards, a conflicting resize, locked allocations, cost optimization and bilingual shop output; verify every capability's acceptance scenarios are linked to passing automated tests or recorded manual checks.
- [x] 11.2 Validate the macOS arm64 desktop workflow with in-process egui tests of file controls, recovery, stale-worker rejection and failed writes, plus build-host process-start smoke; verify pre-commit failures preserve prior files. Record hardware and explicitly label native picker, Metal viewport, high-DPI, normal close and independent clean-machine interaction as **not tested** under the user's GUI-validation waiver. Linux is outside this release scope.
- [x] 11.3 Package the macOS arm64 `.app` with Noto font license, available dependency notices, embedded catalog facts and runtime prerequisites; verify architecture, plist, ad-hoc bundle seal, license inventory and build-host process startup. Record Gatekeeper rejection, metadata-only license exceptions and the **waived, unverified** clean-machine startup; do not claim a supported Linux artifact.
- [x] 11.4 Run `cargo fmt --check`, `cargo check --offline --locked --all-targets`, strict Clippy, `cargo test --offline --locked`, package-script tests and `openspec validate plan-woodworking-desktop-app --strict`; record the passing build-host results and all waived platform/GUI limitations in the release checklist. Completion under these revised criteria is **not** independent verification, shop approval or production certification.
