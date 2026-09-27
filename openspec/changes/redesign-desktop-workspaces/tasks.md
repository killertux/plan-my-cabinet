# Tasks

## 1. Reference fixtures and acceptance harness

- [x] 1.1 Create a deterministic cabinet fixture matching the handoff's geometry, materials, hardware and unallocated-back-panel state; verify stable IDs, real witness-backed sheet metrics, and documented corrections to illustrative numbers.
- [x] 1.2 Add isolated native capture tooling for fixed logical size, scale, locale, camera and fixture state; verify configuration, deterministic fixture setup, capture output and isolation from production project data using the currently available application surface. Actual redesigned workspace, Welcome and Settings screen verification remains in tasks 6.8, 12.6, 14.5 and 15.2 after those surfaces exist.
- [x] 1.3 Record the ten-screen comparison matrix and missing-state matrix in development/release documentation; verify each handoff screen and each delta capability has an identified visual or behavioral acceptance check.

## 2. Portable metadata and compatibility foundations

- [x] 2.1 Add version-1 golden documents and a validated in-memory migration to schema 2; verify preserved exact quantities, IDs, effective material properties, allocations, pinned hardware and legacy receipts with portability tests and no source rewrite or dirty-on-open.
- [x] 2.2 Add optional portable sRGB material colors and undoable color commands with neutral fallback; verify save/load/undo and unchanged manufacturing values and readiness for color-only edits.
- [x] 2.3 Add stable stock aliases and non-reuse counters separate from UUID/priority, including deterministic legacy assignment; verify quantity creation, duplication, ownership change, reorder, deletion, undo/redo and save/load preserve the intended labels.
- [x] 2.4 Add dated kerf confirmation metadata bound to its exact value; verify new confirmation, invalidation on kerf change, undo/redo restoration and unknown legacy dates.
- [x] 2.5 Add optional versioned receipt mode, inclusion settings, alias mapping and bounded comparison-baseline metadata; verify legacy unknown fields, strict validation, document-size failures and lossless historical receipt loading.
- [x] 2.6 Separate manufacturing-input freshness from appearance/edit revision and version output hashes; verify color-only edits preserve output/optimizer applicability, real manufacturing edits stale results, and accepted candidates retain newer presentation metadata.
- [x] 2.7 Document schema compatibility, save-upgrade notice and rollback limits in both project guides; verify unsupported newer files and failed migration preserve current work and original bytes.

## 3. Theme, fonts, icons and modal primitives

- [x] 3.1 Bundle licensed Noto Sans and JetBrains Mono weights needed by the references and register typography tokens; verify accents, numbers, shortcuts and each weight offline, with font licenses included in packaging inputs.
- [x] 3.2 Integrate handoff SVG assets with correct recoloring and compatible pinned egui support; verify all 40 icons render at reference sizes in normal, selected, warning and disabled states rather than staying black under tint.
- [x] 3.3 Implement shared palette/spacing/radius tokens and section, card, chip, segmented, unit-field and button widgets; verify widget-gallery captures against reference tokens and keyboard/accessible names on icon-only actions.
- [x] 3.4 Add shared modal chrome around isolated modal behavior, including scrolling, focus restoration and popup-aware keys; verify Tab/Shift-Tab, Enter, Escape, nested popup events and blocked background scene input with UI tests.
- [x] 3.5 Document theme usage, asset licensing and modal interaction conventions; verify the documented gallery/manual checks run and no product runtime depends on remote fonts or the handoff HTML runtime.

## 4. Action routing, drafts and session state

- [x] 4.1 Extract a typed action registry and enumerate current sidebar/dialog actions before relocation; verify an action-coverage test maps each existing capability to an accessible route and availability reason.
- [x] 4.2 Separate workspace view state, scene selection and typed inspector targets; verify board/sheet/material/installation routing, duplicate names and project replacement do not mix identities or implicitly select every board on a sheet.
- [x] 4.3 Implement shared typed drafts for inspector/HUD dimensions and pose fields using current parsing and transaction services; verify exact untouched values, mixed batch values, captured unit/locale for dirty text across preference switches, preserved applicable rounding consent, consent reset on edit, one-commit undo and cancellation.
- [x] 4.4 Implement pending-navigation resolution for Apply/Discard/Stay and Accept/Cancel/Stay; verify invalid acceptance stays put, panel collapse retains drafts, target changes are guarded and resumed destinations revalidate identity/revision.
- [x] 4.5 Route shortcuts through input/modal/action guards and preserve project-load/save protections; verify text deletion/undo never reaches the scene and stale picker/worker results cannot replace current work.
- [x] 4.6 Document draft commit keys, navigation prompts and state lifetimes in bilingual input/project guides; verify examples agree with automated navigation and transaction tests.

## 5. Shared shell and command palette

- [x] 5.1 Build five-entry rail, header, status bar and workspace routing with platform command-1 through command-5; verify all entries exist consistently and workspace changes preserve selection, scroll/filter and view state without model edits.
- [x] 5.2 Connect Projects, Save, undo/redo and Export plus rail issue badges to real actions; verify Export opens Handoff and shell kerf/spending/issue displays reflect incomplete and invalid actual data.
- [x] 5.3 Implement grouped command/entity search, keyboard result navigation and focus restoration; verify empty/no-result queries, equal names, removed targets, unavailable actions and modal/draft guards.
- [x] 5.4 Implement minimum-width-based panel collapse, accessible header/status overflow and overlay reflow; verify reference dimensions plus 1100 x 700 and 900 x 650 effective windows at 90/100/115/130 percent retain reachable actions and drafts.
- [x] 5.5 Add localized shell/palette strings and shortcut-help documentation as routes are introduced; verify en/pt-BR key parity and capture the reference shell without missing translation keys or clipped controls.

## 6. Design workspace and viewport

- [x] 6.1 Separate viewport controls, canvas interaction, scene rendering and annotation projection while reusing camera mathematics; verify existing picking/drag tests and overlay pointer capture after canvas resizing.
- [x] 6.2 Add actual perspective/orthographic controls and Iso/Front/Right/Top presets, frame selection, and Navigate/Move tool routing; verify render/pick agreement and unchanged project poses/history for all camera controls.
- [x] 6.3 Implement warm scene styling, material face shading, active/secondary selection edges, grid/axes and floor shadow; verify native fixture captures including tint-off, hidden boards and snap-highlight precedence.
- [x] 6.4 Build the outliner, material/stock summaries and single/assembly/multi-selection inspectors; verify visibility and expansion are independent, advanced hierarchy/transform actions remain reachable, and effective thickness/provenance is truthful.
- [x] 6.5 Bind the selection HUD and inspector to shared drafts and add projected dimensions plus clickable miniature sheet; verify editing either surface shares validation and one undo, while sheet navigation focuses the same allocation or issue.
- [x] 6.6 Add independent face/grid snap popover, temporary Alt bypass and scale-aware grid display; verify only enabled assistance contributes candidates, face priority is preserved and spacing never quantizes existing poses.
- [x] 6.7 Add Measure tool using current Body/Overall/frame-aware bounds and cancellable pose presets; verify rotated/nested selection, missing hardware extents, preset frame/pivot disclosure and cancel restoration.
- [ ] 6.8 Update bilingual assembly/viewport/board guides and localize new inspector/help text; verify reference Design capture and empty, hidden, multi-selection and invalid-draft interaction checks alongside existing cabinet/assembly tests.

## 7. Creation and editing dialogs

- [x] 7.1 Migrate New board and New material to shared modal widgets, including color swatches and nonmutating first-fit preview; verify repeated edits/cancel create no allocations, consent is required, and confirm uses current stock state.
- [x] 7.2 Migrate Position, Place face to face and Resize N boards to the reference patterns; verify frame/axis/offset feedback, per-board before/after values, rounding consent and atomic accept/cancel semantics.
- [x] 7.3 Migrate unsaved/delete/overwrite, material preserve/apply, grouping/reparenting and duplicate-assembly dialogs without dropping fields; verify affected-object disclosure and original cancellation/destructive safeguards.
- [ ] 7.4 Finish the dialog inventory with stock, currency/fee/kerf/grid, hardware, relationship and recovery prompts; verify every modal uses the common isolation/focus contract and capture the six reference examples.
- [x] 7.5 Update bilingual dialog/input documentation and translations; verify nested material creation returns to the intact board draft and Enter/Escape do not fall through popups into submission or scene actions.

## 8. Stock workspace

- [x] 8.1 Build reusable stock/sheet read models with aliases, global rank, allocation usage and witness-aware summaries; verify table/card/miniature statistics agree and Unknown grain, effective thickness and missing price remain distinct.
- [x] 8.2 Build material filter/list, grouped stock table, empty-material issue rows and priority-order view; verify initial handoff composition and real counts for filtered, empty and long-name cases.
- [x] 8.3 Implement scoped drag ordering and accessible global-position/keyboard alternatives through undoable commands; verify hidden slots remain fixed for subset reorder, global ordering is available, and allocations never move automatically.
- [x] 8.4 Build stock inspector size/grain/trims/ownership/price fields and allocated-part navigation; verify invalid trims and unconfirmed rounding preserve state, quantity creation uses unique identities, and Open in cut plan targets the correct piece.
- [x] 8.5 Connect cut-fee, currency-change and spending summary controls with unknown/zero distinctions; verify incomplete costs and currency relabel/replace flows through existing stock/cost regression tests.
- [ ] 8.6 Update bilingual stock documentation including alias and grouped-priority semantics; verify native Stock reference capture and keyboard/filter/reorder workflows with en/pt-BR content.

## 9. Cut plan, repair and optimization

- [x] 9.1 Build sheet cards/switcher and deduplicated Needs stock diagnostics with contextual Add/Reveal/Repair actions; verify hidden/conflicted boards remain represented and unused sheets do not inherit another sheet's metrics.
- [x] 9.2 Render the focused sheet with rulers, adaptive part labels, grain/ID toggles, hatched offcuts, real-width kerf bands and cut markers; verify witness correspondence, conflict positions, low-zoom legibility and shared selection.
- [x] 9.3 Build witness-backed stats/cut-sequence inspector and hover linkage; verify actual reference edges, kerf sides, input/output IDs, trim accounting and distinction between violation and proof exhaustion.
- [x] 9.4 Integrate View/Repair drag ghosts plus numeric placement, rotation, transfer, lock/unlock and unallocation; verify intermediate invalid states, all-affected-sheet acceptance, Escape cancellation and explicit workspace-leave decisions.
- [x] 9.5 Build optimization search/progress/cancel and current-versus-best comparison; verify all objectives, metrics, locked placements, incomplete costs, no-result cases and one-transaction candidate acceptance.
- [x] 9.6 Integrate revision/manufacturing-key checks and cross-workspace activity reporting; verify color-only changes are safely preserved on acceptance, manufacturing edits require fresh search, and cancelled/stale workers never auto-apply.
- [ ] 9.7 Update bilingual stock/cutting guidance and worked examples, then capture Cut plan/repair/compare states; verify existing allocation-invalidation, stock-walkthrough and optimization performance fixtures remain valid without changed solver mathematics.

## 10. Hardware workspace

- [x] 10.1 Build pinned catalog/source access and door/hinge tree with standalone/reference hardware routes; verify offline snapshots, warning selection and add/edit/remove/group actions remain accessible.
- [x] 10.2 Build installation inspector and reference diagram from domain diagnostics, retaining independent door/mount coordinates, edges and faces; verify actual supported K/R values, invalid thickness and unavailable evidence are never fabricated.
- [x] 10.3 Add projected axis, cup/plate markers, labels and connectors to the live viewport; verify annotations follow board-local references and display poses and remain visually distinct from machining geometry.
- [x] 10.4 Build configured-limit motion slider/HUD and Closed control with approximate-motion disclosures; verify coherent moving assemblies, unchanged manufacturing state, guarded unfinished relationships and closed reset on Hardware exit.
- [ ] 10.5 Update bilingual hardware documentation and localize inspector/source guidance; verify native Hardware reference capture plus existing installation, relationship, deletion and invalid-reference tests.
- [x] 10.6 Correct outward door-preview handedness from hinge edge and cup face without a global sign reversal; verify all mirrored edge/face combinations, rotated/nested roots, unchanged exact closed/stationary poses, preserved legacy load bytes and axes, blocked stale motion until explicit reconfirmation, undo restoration of review state, and the native 60-degree Hardware reference retest. Update bilingual compatibility guidance.

## 11. Shared document layout and Handoff

- [x] 11.1 Introduce a renderer-independent paginated document model and shared font metrics/shaped text positioning; verify deterministic page geometry, wrapping, repeated context and long en/pt-BR names with layout tests.
- [x] 11.2 Implement the reference cover, stock/parts tables, grouped-identical-part identity lists, estimates and mandatory notices in that model; verify grouping does not merge incompatible material/grain parts and pagination omits no labels.
- [x] 11.3 Port sheet diagrams, cut-step continuations and hardware sections to the shared model; verify kerf/reference-edge/scale labels and omission of invalid numeric hardware guidance using existing shop-handoff fixtures.
- [x] 11.4 Make PDF serialization and native preview consume the same positioned pages; verify identical page counts/content placement, zoom-independent pagination and matching rendered pages without an external runtime PDF viewer.
- [x] 11.5 Build Handoff packet cards, readiness/Fix routes, independent output language/units and section toggles; verify all-off sections retain mandatory safety content and hidden unallocated boards still block Shop-ready.
- [x] 11.6 Add cancellable revision-aware preparation/cache and export the reviewed frozen packet; verify output changes refresh preview, stale preparation is labelled, picker cancellation/overwrite refusal/write failures create no receipt.
- [x] 11.7 Build receipt cards with recorded mode/sections/date/hash, supersession and evidence-based Since then summaries; verify legacy unknown fields, included-hardware changes and manufacturing-neutral colors have correct distinct statuses.
- [x] 11.8 Update bilingual shop-handoff guidance and capture the Handoff reference plus dense multi-page previews; verify offline embedded fonts, PDF/preview page parity and existing write-verification/non-destructive export tests.

## 12. Welcome, recents and recovery

- [x] 12.1 Add bounded atomic local recent-project storage and successful-open/save registration; verify startup Welcome, filtering, actual metadata, missing-file Locate validation and Remove without deleting project/recovery files.
- [x] 12.2 Add post-save offscreen thumbnails keyed to saved state with placeholder fallback; verify preview geometry is excluded, user camera is unchanged, and capture/cache failure does not invalidate successful save.
- [x] 12.3 Add validated recovery discovery/indexing for registered saved and untitled project identities; verify UUID/canonical-path matching, no arbitrary-folder scan, unknown metadata and version-1 snapshot migration.
- [x] 12.4 Build recovery comparison card and Recover/Decide later/Discard actions; verify deferred candidates persist, recovered work remains unsaved, untitled recovery uses Save As and copied paths never adopt another recovery silently.
- [x] 12.5 Add recovery-folder access and no-preselection cleanup review with explicit deletion confirmation; verify cancellation and partial deletion failures preserve unselected snapshots and every saved project file.
- [ ] 12.6 Update bilingual project/recovery guides and capture Welcome with empty, populated, missing-file and recovery states; verify native file-picker/unsaved-work integration and atomic-save portability regressions.

## 13. One-time assembly templates

- [x] 13.1 Implement pure Base and Wall recipe builders using the design's disclosed carcass/back conventions and chosen materials; verify BOM, local board dimensions/world bounds, positive geometry and no implicit stock purchasing with table-driven tests.
- [x] 13.2 Implement Drawers generation including carcass, box sides/fronts/backs/bottoms and separate fronts, with explicit count/material/clearance inputs; verify three-drawer BOM, overall bounds and derived dimension formulas across thickness and clearance combinations.
- [x] 13.3 Add staged template project/material setup and preview/review modal with datum diagrams, generated dimensions, first-fit outlook, validation and rounding consent; verify first-run users can create and assign component materials, nested material cancellation retains the template draft, and invalid geometry or setup cancellation changes no existing project state.
- [x] 13.4 Resolve existing document/edit replacement guards before committing staged materials and generation as one transaction in a new unsaved project with fresh IDs, stable part order and independent allocation attempts; verify save failure/picker cancellation retains the existing document and setup, undo/redo restores generated materials and assembly together, insufficient stock leaves parts unallocated, and later edits do not propagate parametrically.
- [x] 13.5 Connect Welcome template tiles and document bootstrap, cancellation, recipes and clearances in both languages; verify Drawers from empty first-run Welcome creates its advertised materials and parts, selects the assembly in Design, offers Stock for allocation, creates no recent entry before save, and explicitly excludes slide selection, machining and validated mechanical fit.

## 14. Settings and app-local preferences

- [x] 14.1 Add validated atomic platform configuration storage for language, hints, inverse zoom, material tint and scale; verify restart persistence, malformed-config fallback, write-error reporting and absence from `.pmcab`/project history.
- [x] 14.2 Build Cutting, Grid & units and Costs & currency Settings sections using existing commands; verify dated kerf confirmation, worked examples, unknown/free fee distinction, currency flow, and no-revision/no-undo display-unit switching including m/ft.
- [x] 14.3 Build General preferences and recovery links plus contextual footer; verify all four scales, actual scroll-direction/tint/hint behavior and preferences access when no project is open.
- [x] 14.4 Build Shortcuts and About with real version/platform/source information and Help routing; verify keyboard-only camera navigation, platform shortcuts and readable offline content in both languages.
- [x] 14.5 Update bilingual input/project/viewport guidance and capture all three Settings references; verify enlarged pt-BR layout, pending unsuffixed and explicit-suffix input across unit/language changes, captured parsing and rounding consent, pristine reformatting, and Done/Escape behavior retain exact values and drafts.

## 15. End-to-end acceptance and release integration

- [ ] 15.1 Execute create/template -> Design -> Stock -> repair/optimize -> Hardware -> preview/export -> save/reopen workflows with cross-workspace prompts; verify every new scenario and preserved capability has passing automated or recorded native evidence.
- [ ] 15.2 Use the isolated capture tooling to verify repeatable 1440 x 900 redesigned workspace, 1100 x 700 Welcome and 780 x 560 Settings captures, compare all ten native references with the supplied screenshots, and review missing-state captures; verify documented 2-point geometry targets, token/type/icon fidelity, truthful corrected data and no placeholder controls, recording approved renderer-specific exceptions.
- [ ] 15.3 Exercise supported scales, secondary window sizes, keyboard/trackpad navigation, long names, en/pt-BR and actual Metal rendering; verify no unreachable actions, overlay collisions, lost drafts or misleading focus/selection states.
- [x] 15.4 Run `cargo fmt --check`, `cargo check --locked --all-targets`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`, and existing dense-sheet/optimization fixtures; verify passing results and record cancellation/frame observations against the existing baseline.
- [ ] 15.5 Package and launch the macOS arm64 app offline, verify bundled new assets/licenses, native pickers, schema upgrade notice and preview/export output; update the release checklist with actual results and explicitly retain unverified-platform limits.
