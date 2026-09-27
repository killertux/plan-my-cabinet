# Design

## Context

See `proposal.md` for motivation and scope. The authoritative references are `design_handoff_egui_redesign/README.md`, Main Window **2a**, the seven other HTML references, and ten supplied screenshots. HTML is a design reference, not a product runtime. The handoff overstates how much is already implemented: the shell, palette, Welcome metadata, preferences, PDF preview/options, display colors, and several annotations require real behavior/data additions.

The existing stack is Rust 2024, eframe/egui 0.36.2 and wgpu, with headless domain modules and Fluent localization. `main.rs` owns much of the application orchestration; the existing `*_ui.rs` modules expose useful editing flows but are not independently routed workspaces. `viewport::show_move` currently combines controls, navigation, selection, placement interaction, scene preparation, and the GPU callback. Reuse its rendering foundation and camera mathematics; do not replace it with the HTML's fixed SVG scene.

`ProjectEditor` and placement/repair sessions already provide transactional edits, preview cancellation, and allocation consequences. `project_ui.rs` tags asynchronous picker results and validates loads before replacement. `recovery.rs` associates snapshots with UUID plus canonical path and waits 30 seconds after committed edits. `persistence::prepare_bytes` currently accepts exactly schema version 1. Materials lack display colors, kerf confirmation lacks a timestamp, and export receipts lack packet mode and an explanatory snapshot baseline. Existing PDF generation has its own paginated layout, not the illustrated page design.

Relevant regression baselines include `tests/portability.rs`, `cabinet_assembly.rs`, `allocation_invalidation.rs`, `stock_walkthrough.rs`, `shop_handoff.rs`, `release_cabinet.rs`, and UI/module tests. The supported visual acceptance platform remains macOS arm64/Metal; experimental Linux is not silently promoted to release support.

## Goals / Non-Goals

**Goals**
- Deliver the complete handoff with real actions, data, empty/error states, and measurable native visual fidelity.
- Keep domain commands, precision, cutting witnesses, and export gates authoritative beneath the new presentation.
- Separate document data, derived views, edit drafts, and machine-local preferences so navigation and appearance do not become manufacturing changes.
- Preserve legacy projects and historical provenance without inventing missing information.

**Non-goals**
- A webview, browser runtime, dark theme, new cutting solver, or rewritten physical domain engine.
- Literal cross-renderer pixel identity, hardcoded mockup statistics, or fake interactive controls.
- Persistent parametric cabinet constraints, automatic drawer-slide selection, machining geometry, structural certification, or collision-validated hinge trajectories.
- Cloud recents, shared stock inventory, automatic recovery deletion, or recovery following a copied file implicitly.

## Decisions

### 1. Reference authority and acceptance

Confirmed by the user: faithfully reproduce the design while correcting contradictions and preserving existing capabilities in cohesive additional states. Confirmed reference size is 1440 x 900 logical points at 100% interface scale, Welcome 1100 x 700, and Settings 780 x 560. Use all five rail entries everywhere. Prefer HTML for exact style values, README for interaction intent, and these delta specs for resolved behavior. Reject 1a/1b/1c alternative layouts.

Resolve known contradictions explicitly:
- Utilization, amounts, counts, names, IDs, dates, and version/platform text come from real state, not illustrative literals.
- The HTML projection is orthographic despite its selected Persp chip. Implement genuine perspective/orthographic controls; use orthographic mode when matching that illustrated camera.
- Grid rendering may decimate lines for legibility while actual snapping uses project spacing; indicate the effective display interval when different rather than pretending 100 mm lines are 10 mm.
- Board thickness is an effective per-board value, not always the current material default. The compact readonly field describes provenance accurately; retain advanced direct thickness editing.
- Preserve m/ft input and output units and existing mounting/measurement controls even where the illustrations show fewer choices.

Alternative rejected: treating every screenshot literal as a requirement. It would produce contradictory state and functional regressions.

Create a deterministic reference project with stable UUIDs, representative cabinet geometry/materials/hardware and an unallocated back panel. Its feasible stock layout must be generated/verified by real domain rules, not copied into a fake canvas. Record corrected statistics alongside screenshot comparisons. Review all ten supplied screens, then empty/no-selection, assembly/multi-selection, validation/rounding, command search, repair, optimization comparison/stale, export failure, and legacy/missing recent states. Geometry/panel bounds target a 2-logical-point tolerance against approved references; font weights/sizes and color tokens match their specified values. Text rasterization, shadows, and genuine corrected-state differences are documented exceptions, not blanket exemptions. Native captures and interaction checks are required; headless egui tests alone are insufficient.

### 2. Native theme and shared widgets

Add `theme.rs`, `icons.rs`, and small reusable presentation helpers for section headers, unit fields, chips, segmented controls, buttons, cards, and modal header/body/footer. Keep the approved warm light palette and explicit typography tokens instead of relying on egui defaults. Bundle licensed Noto Sans Regular/Medium/SemiBold and JetBrains Mono weights used by the references, plus license files. Do not fetch Google Fonts at runtime.

Use compatible egui SVG/image and table support where it reduces custom layout risk. Verify crate versions against pinned egui before adding them. The supplied icons have black strokes; multiplicative image tint does not recolor black. Prepare white-alpha masks or explicit colored SVG rasterization with size/color caching, and visually test all states rather than blindly using the README's tint recipe. Keep handoff sources unchanged; derived product assets belong in normal assets during implementation.

Use real interactive widgets with accessible names and focus semantics, even when their visuals are custom-painted. Retain `egui::Modal` input isolation while reproducing the proposed Window-style chrome. Popup-aware Enter/Escape, focus restoration, disabled confirmation and destructive action labels belong in one modal controller.

Alternative rejected: bespoke drawing and key handling for each screen, which would produce subtle visual and input divergence.

### 3. Shell, action registry, state ownership

Introduce `shell.rs`, `actions.rs`, and `workspaces/{design,stock,cut_plan,hardware,handoff}.rs` as proposed module boundaries. Workspaces consume existing commands/services and read models rather than mutating `Project` fields directly.

```
Rail / palette / menu / contextual control
                    |
             Action dispatcher
                    |
          Edit + navigation guard
             /              \
     Session view state   Domain command/preview
                              |
                       Derived read models
                    /         |          \
                 scene      sheets     document
```

One action registry provides stable action IDs, localized labels/keywords, availability reasons, invocation, and focus/modal guards. Palette results for entities retain typed stable IDs and revalidate on activation. Empty query lists useful available actions; search is case-insensitive across names, IDs and localized action labels. Equal names remain distinguishable. Platform command shortcuts map through the dispatcher, not separate direct mutations. Header Export opens Handoff; Projects opens Welcome subject to the same navigation/document guards.

State ownership:

| Lifetime | Contents |
| --- | --- |
| Portable project | Domain data, colors, stable stock aliases, provenance metadata, export receipts |
| App-local persisted | Language, interface scale, hints, inverse zoom, material tint, recents and thumbnail references |
| Project-session view | Active workspace, per-workspace scroll/filter/panel state, selected sheet, camera, cut overlays, palette query |
| Edit session | Typed field drafts, rounding consent, placement/repair preview, pending destination |
| Derived cache | Diagnostics, witnesses, cost summaries, scene buffers, document layout |

Keep object selection separate from typed inspector targets for materials, stock, installations and relationships. Sheet selection does not select every part. Navigation can focus an allocation issue without moving geometry or changing visibility. Switching projects resets session state and rejects stale async results; switching workspaces does not.

### 4. Draft transactions and navigation

Confirmed behavior: preserve view state on navigation; unfinished inspector/HUD drafts require Apply/Discard, repair and pose edits require Accept/Cancel, and the user can Stay. Invalid acceptance is disabled. Hardware motion alone resets to closed when leaving Hardware. Panel collapse retains drafts.

Build a shared typed draft controller on existing parsers, `ProjectEditor`, and placement sessions. It retains original exact values separately from formatted text, field dirty flags, source project/revision/target, validation, and rounding consent. Editing the text clears stale rounding consent. Inspector and HUD bind to the same draft rather than creating competing edits. Enter or explicit Apply commits a valid logical edit once; Escape cancels the active field/session. Focus loss alone does not silently commit. A target/workspace change with a dirty draft opens the resolution prompt. A pristine field or no-op edit creates no transaction.

On the first text edit, capture the entry display unit and input locale with the dirty field. Subsequent display-unit or interface-language changes leave its text, parsing rules, validation, proposed physical quantity, and any still-applicable rounding consent intact until Apply or Discard; further edits to that same draft use the captured rules and clear consent. An explicit unit suffix continues to override the captured display unit. Pristine fields may reformat from their exact committed value in the new presentation without becoming edits, and newly started drafts use the new settings. Neither preference change commits a draft or changes project history.

When a pending destination is resumed, revalidate its target and source revision. Failed Apply/Accept keeps the user at the edit with the error. Project replacement still uses prepared-load and unsaved-file prompts after edit resolution. Active optimization may continue across workspaces with visible shell activity and cancellation, but its result cannot apply without explicit acceptance; source changes make it stale. No navigation implicitly accepts a worker result. Modal actions prevent scene input; overlay controls consume pointer events before the viewport sees them.

Alternative rejected: committing each keystroke or maintaining detached inspector/HUD drafts. Both break undo and precision guarantees.

### 5. Responsive layout and input

Confirmed behavior: reference panel widths at baseline, central growth on larger windows, collapsible/scrollable panels when space is constrained. Use logical-point layout after interface zoom. Determine collapse from required pane/control minimum sizes rather than shrinking fonts. Keep rail/header actions reachable via compact controls, status details in accessible overflow, and side inspectors in reopenable drawers. Keep overlays within the canvas and reposition/stack them before overlap. Stock tables may scroll horizontally while preserving the selected row and priority labels.

Workspace panes own stable scroll IDs. Dialogs scroll their body when necessary while retaining title/actions and focus traversal. Test 1100 x 700 and 900 x 650 effective windows, all four interface scales, long names and pt-BR; require no unreachable actions or lost drafts, not pixel identity at these secondary sizes. App preferences write atomically to the platform configuration directory, report failure without touching project state, and never fall back to the working directory. Display-unit changes retain the established no-revision/no-undo exception to project setting edits.

### 6. Design viewport and editing surfaces

Separate camera/tool commands, canvas interaction, scene generation/GPU rendering, and projected annotations in `viewport.rs`. Reuse existing projection/picking/drag mathematics consistently across real perspective and orthographic modes; camera presets must not alter saved poses. Tools are Navigate, Move and Measure; frame selection and snap options use shared actions. Measure consumes the existing bounds service and exposes selection scope plus frame, including Body/Overall distinctions.

Extend renderer style inputs for warm background, grid/axes, material-based face shading, active amber vs secondary cyan selection, edges, and floor shadow. Draw dimension pills and hardware guides as projected overlays tied to real geometry and correct viewport clipping. The shadow is visual grounding, not physical lighting certification. Verify picking after moving/resizing the canvas. Numeric pose presets produce cancellable proposals through the existing frame/pivot semantics, not hidden direct rotations.

Confirmed pose-preset convention: the board's local X/Y plane is its broad face, with X=length, Y=width and Z=thickness. **Lay flat** absolutely aligns local X/Y/Z with the selected World or Local-parent frame axes. **Stand up** absolutely maps local X to frame +Z, local Y to frame +Y and local Z to frame −X (a −90° frame-Y rotation). **Turn 90° Z** is a relative +90° rotation about the selected frame's +Z axis from the current tentative pose. All three pivot at the board's pose origin, not its centre; the dialog visibly discloses the frame, axis/face result and pivot before acceptance. Presets update the cancellable placement preview and numeric display without quantizing an untouched source quaternion or committing a separate transform.

Design's inspector and floating HUD reuse dimension drafts. Outliner expansion/visibility/selection are distinct; warnings do not make visibility controls inaccessible. Keep assembly, multi-selection, and hardware placeholder editing reachable. The miniature sheet reads the same sheet model as Cut plan and navigates by UUID. Material changes retain explicit preserve/apply choices; first-fit creation previews run against a snapshot and do not reserve stock.

### 7. Stock and Cut plan read models

Use one stock/sheet presentation model for tables, cards, inspector miniature, canvas, and document diagrams so areas/labels agree. All dimensions, offcuts, cut count, kerf bands, conflict positions and witness state derive from domain results. Cache by relevant source state and do not rerun bounded searches for unchanged frames or simple hover.

Technical default: persist unique monotonic stock aliases assigned by initial source (`S` purchased sheet, `O` owned piece) separately from UUID and global priority. Keep an assigned alias through ownership changes, reorder and save/load; ownership is its chip, not inferred from the alias prefix. Assign legacy aliases deterministically by existing priority plus UUID tie-break, without dirtying on open. Duplicate gets a new alias; redo restores it. Keep alias allocation counters so deletion does not cause reuse.

Grouped/filtered drag ordering permutes only the visible slots in the underlying global priority list; invisible entries keep their slots. Show actual global rank, and explain this rule in the table help. All-stock ordering permits cross-material moves through an ungrouped priority-order view so grouping cannot conceal global order. Reordering never repacks committed placements. Retain keyboard move-up/down alternatives for drag accessibility.

Cut plan has one focused sheet, View/Repair modes, independent Cuts/Offcuts/Grain/IDs overlays, fit and sheet navigation. Repair retains numeric placement, transfer, rotate, unlock and unallocate controls through contextual actions, not drag-only interaction. Needs Stock cards select actions from diagnostics: adding stock is not the universal remedy. Witness-backed cut rows highlight exactly their kerf band; label placement avoids collisions and preserves legibility at low zoom. Optimization compare presents committed vs best-found counts, costs/completeness and allocations, with stale/invalid acceptance disabled.

### 8. Shared document layout, receipts and freshness

Confirmed: preview and export share content **and pagination**. Introduce a renderer-independent paginated document model between `ExportSnapshot`/prepared export and `pdf_export.rs`. Pages contain positioned text runs, paths, tables, diagram primitives, links/labels and mandatory notices in physical page units. Resolve wrapping, font metrics, table continuation, A4 page breaks and cut annotations once. Both egui page rendering/thumbnails and PDF serialization consume those resolved pages. Do not independently wrap text in each backend or use an external PDF rasterizer as a runtime dependency.

Preview cache keys include source snapshot, output language/units, mode, section choices, layout version, and font metrics version. Zoom and selected page are view state. Preparation is cancellable; an older prepared packet cannot be presented as current after a relevant edit. Export writes the frozen document shown, with its identified source state, rather than silently recomputing a different page set at file-picker completion. Existing cancellation/overwrite and verified-byte receipt semantics remain intact.

Conservative stale-review rule: changing a packet source or any output control invalidates the previous review immediately, prepares and displays a newly keyed packet, and requires an explicit **Review this preview** acknowledgement before Export becomes available again. If the source or controls change while a save picker is open, reject that picker result and require fresh review rather than writing either an old packet or a silently recomputed one. Picker cancellation retains an otherwise-current review; overwrite refusal and write failure leave receipts unchanged. This rule is intentionally stricter than merely observing one frame of the refreshed preview and prevents a race at picker completion.

Section toggles default on. Project/revision identity, cutting assumptions, safety/scale notices and relevant unresolved issues remain mandatory; omitting a section never bypasses all-board wood validation or conceals invalid included hardware guidance. Preview and export support existing mm/cm/m/in/ft options, with m/ft in an expanded selector if necessary. Group identical parts only when their material, effective dimensions and grain meaning match, and retain individual IDs and allocation references.

New receipts include mode, section choices, fingerprint/layout version, stock alias mapping and a bounded structured comparison baseline for user-facing manufacturing/hardware changes. Derive 'Since then' summaries from actual before/after values; legacy receipts without a baseline get a generic outdated/unknown-details message. Distinguish superseded historical packets from present validity; historical export is never proof that today's project is shop-ready.

Continue separating wood feasibility and included-packet fingerprints. Exclude viewport-only color, camera, visibility, workspace and preferences from manufacturing content hashes. Include changed printed content/metadata where relevant. Version fingerprint algorithms; use legacy computation when possible for old receipts, otherwise report freshness unavailable rather than automatically classifying every old packet as stale. Color edits still increment project editing revision and dirty state, so document source revision and manufacturing-equivalent freshness are distinct concepts.

Use a manufacturing-input key for optimizer validity as well as the source editing revision. If only color changed, revalidate candidate applicability and apply allocation deltas to the current project while preserving its latest colors and metadata; never replace the entire current project with the worker's old snapshot. A manufacturing input change still requires a fresh search. Add regression tests for both paths.

### 9. Welcome, recovery and metadata compatibility

Approved Hardware correction (2026-09-27): positive opening angles move the
door's free edge away from its selected cup face (the inside mounting face).
Derive the directed local-Y hinge axis from both the selected X edge and Z
cup face, then transform that direction into world coordinates. MinX/MinZ
and MaxX/MaxZ use local −Y; the opposite pairings use local +Y. Do not reverse
every door globally. Closed poses and independent mounting datums remain
unchanged; rotated and nested roots use the same world-space transformation.
This corrects handedness only, not concealed-hinge kinematics or collision
validation. Preserve all stored legacy axes and source bytes on load; compare
them against the corrected proposal through relationship review. Affected
relationships cannot start motion until explicitly reviewed/reconfirmed as
one undoable edit. Undo restores the prior relationship and its review state.
Unchanged valid axes remain usable. No new schema version or silent migration
is needed for the existing directed-axis field.

Technical proposal: write schema **2** for the enriched portable document, and add a strict version-1-to-2 in-memory migration before normal validation. This is preferable to keeping version 1 while older binaries silently drop new metadata on save. Legacy source files remain unchanged until explicit save; warn that files saved in the new format require a compatible application. No destructive downgrade is provided. Preserve unknown historical packet modes/dates and nullable comparison baselines. Serialize colors as optional sRGB byte triples, not egui-specific types. Old missing colors use a neutral material appearance.

Persist kerf confirmation timestamp together with the exact confirmed value; changing kerf clears both. Undo/redo restores them together. Migration never manufactures a date. Add bounded validation for aliases, color channels, receipt-baseline entries and hashes within the existing document-size limit. Recovery payload migration uses the same validator.

Local recent entries store path, UUID, last successful access, cached summary and thumbnail key. Register only successful opens/saves. Locate validates and updates the local reference; a genuinely different project is identified, not silently substituted under an old identity. Remove affects only the list entry. Generate a thumbnail from the successfully saved snapshot with a neutral framed camera offscreen; do not capture a half-edited on-screen preview or modify the user's camera. Capture/cache failure is nonfatal after project save.

Use an app-local recovery discovery index with canonical path and UUID; rebuild/validate entries from recovery metadata and registered recents where possible, preserving invalid/unknown records for diagnosis. Do not scan arbitrary user folders. Welcome offers validated newer candidates and enough provenance for an explicit decision; no copied-path adoption. Proposed conservative cleanup policy is a review list with no preselection and explicit per-snapshot selection plus confirmation, not 'delete everything older than N days'. Defer retains candidates. For untitled projects, use a distinct recoverable untitled identity, never pretend there is a saved-file comparison; the Welcome card labels that state honestly. Recovery remains 30-second committed-edit inactivity, excluding drafts and display previews.

Alternative rejected: embedding thumbnails/preferences/recents inside the project, which couples portability to machine paths and makes view changes dirty.

### 10. One-time templates

Confirmed: Base/Wall/Drawers generate independent normal boards in one undoable assembly, not persistent parameters. Confirmed Drawers option A includes carcass, box parts and fronts; no automatic slides, machining, or fit certification.

Use a pure recipe builder that returns a candidate assembly/BOM/poses plus field-specific errors. Its review screen shows all assumptions, generated dimensions, orientation, material thickness, and allocation outlook. Confirm runs one transaction and first-fit attempts in recipe order. Failure of geometry validation produces no partial assembly; insufficient stock produces legitimate unallocated parts, not purchased stock.

Welcome template tiles start a **staged new-project setup**, including on first launch with no project or materials. The setup collects project name, currency and input units, then lets the user create draft materials with name, positive thickness, grain and optional color and assign them to component roles. Multiple roles can share one draft material when appropriate; differently thick roles require explicit choices. Nested material creation returns to the intact template draft. These materials and the candidate project remain uncommitted throughout review; no existing project is used as an implicit material source or modified. With no declared stock in a fresh project, the preview honestly reports its parts as unallocated and directs the user to Stock after generation.

On valid Generate, run existing unfinished-edit and unsaved-project replacement guards if another document remains open behind Welcome. Save failure, picker cancellation, or choosing Stay/Cancel retains that document and the staged setup rather than replacing it. After those guards succeed, create the new project's empty editor and commit its staged materials plus generated assembly as one transaction, then enter Design with the generated assembly selected and the new project unsaved. One undo removes both generated boards and materials created exclusively by that transaction, leaving the new empty project; it does not resurrect the previous document. Cancelling the setup before successful generation discards only its staged data, returns to Welcome, and leaves any previously open document unchanged. Do not add a recent-project entry until explicit save succeeds. This avoids first-run dependence on an existing material library while retaining atomic generation.

Proposed editable starter recipes (construction defaults, not certified joinery):
- **Base:** full-height sides, bottom between sides, front/back top rails, and an overlay back. Overall width/height/depth, rail width, carcass material and back material are visible inputs.
- **Wall:** full-height sides, top/bottom between sides, one adjustable-position wooden shelf, and an overlay back. No wall fixings or load claim.
- **Drawers:** closed carcass like Wall without its shelf, with a chosen number of separate drawer assemblies. Each box has two sides, front/back between sides, an applied bottom, and a separate external front. Box/front/bottom materials are explicit. Side slide clearance, rear clearance, box depth, vertical clearance, front perimeter reveals and inter-front gap are explicit editable inputs. No groove/rabbet/machining geometry is implied by these rectangular blanks.

Treat outer depth as including the overlay back and, for Drawers, the external front thickness; show that datum diagram. Derive carcass internal width from outer width minus both side thicknesses, then drawer-box external width by subtracting both side clearances. Derive front heights from available facade height minus perimeter reveals and inter-front gaps, divided by drawer count. Expose box height and depth and validate them against the opening and user clearances. Derived front/back box lengths subtract both box-side thicknesses; applied bottoms cover the box footprint. Position boxes using the disclosed opening, front and bottom datums. Show any precision rounding for user consent; reject nonpositive results and unsupported world coordinates. These are editable starting arrangements, not immutable construction standards. Default numeric values are presented for review and are not treated as manufacturer guidance.

Alternative rejected: empty drawer bays, which the user explicitly declined, or a live cabinet constraint solver, which is out of scope.

### 11. Delivery and verification

Build in dependency order: compatibility/theme/action/draft foundations; shell and Design; Stock/Cut plan; Hardware/shared modal migration; document/Handoff; Welcome/templates/Settings; full regression and visual review. Integrate the palette with action routing early rather than retrofitting it after buttons disappear. Each stage delivers real behavior; the change is complete only when all stages pass.

Capture task ordering was explicitly approved for correction: create and verify isolated capture tooling against the currently available application surface first; verify the actual redesigned workspace, Welcome and Settings screens as those surfaces are delivered and during final integration. Early tooling captures are baselines, not evidence that an unimplemented reference screen passes.

Use headless tests for command parity, selection routing, transaction boundaries, precision, metadata migration, recovery, template formulas, layout pagination, receipt freshness and stale worker handling. Exercise the existing 100-board workload so the new shell does not regress cached diagnostics or worker cancellation. Compare serialized page-layout models and rasterized exported PDFs against the native preview at multiple zooms; check long names and pt-BR wrapping. Bundle/offline checks must include new fonts/icons and license inventory.

## Risks / Trade-offs

- [Large cross-cutting UI refactor] -> Extract action/draft/read-model seams before moving controls; keep regression fixtures running throughout and do not remove old actions until their new route exists.
- [Default widgets drift from reference] -> Shared visual tokens and screenshot review per screen, not a final cosmetic pass.
- [Native and PDF font metrics diverge] -> Shared shaped text positions/page layout, font-version keys, and rendered-page comparisons; no independently wrapped preview text.
- [Schema migration loses effective properties or receipts] -> Golden v1 fixtures, duplicate-key/size checks, validated migration and read-only opening; keep original files for rollback.
- [Metadata growth exceeds portable file limits] -> Bounded structured receipt baselines, explicit size errors, and no embedded raster thumbnails; never silently prune history.
- [Drafts accidentally commit during layout/routing] -> Central navigation guard, explicit focus ownership, and tests for collapse, target change, modal keys and worker completion.
- [Template appears manufacturing-certified] -> Explicit recipe/clearance review and hardware disclaimer; user-selected materials and all ordinary allocation checks.
- [Literal screenshot conflicts with truthful data] -> Correct the fixture/readouts, record the exception, preserve design composition.
- [Dense screens become unusable under zoom/localization] -> Logical-point minima, collapsible panels, accessible overflow, and real native tests at secondary sizes/scales.

## Migration Plan

1. Add migration fixtures and headless validators before emitting enriched documents. Opening v1 is in-memory only; explicit saves emit v2 atomically with a compatibility notice.
2. Add optional provenance/appearance data and versioned freshness computation. Do not infer historical mode, date, or detailed changes for legacy records.
3. Add local app-state/recovery discovery storage independently of portable project data. Failed local cache/prefs writes must not corrupt project saves.
4. Introduce shell/workspaces progressively against the existing commands. Preserve every action until its replacement route is verified; no temporary placeholder counts as completion.
5. Validate native visuals, workflows, migration, output parity and offline packaged assets, then update bilingual docs and release checklist.
6. Rollback before saving uses the untouched v1 source. After a v2 save, older binaries must refuse the file; use a separately retained v1 copy or remain on the compatible version. Never silently downgrade away colors, aliases, or receipt provenance.
