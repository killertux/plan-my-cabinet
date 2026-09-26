# Design

## Context

See `proposal.md` for motivation and the eight capability deltas for behaviour. Inspection found only a Rust 2024 package with an empty dependency list and a `Hello, world!` entry point; there are no application tests, storage formats, UI conventions, or existing capability specs to preserve. The existing OpenCode agents and routing script are unrelated and remain untouched.

This design chooses implementable defaults where discovery left engineering details open. It is a staged first-release plan, not authorization to implement during proposal creation. The user's example cabinet is a future acceptance fixture, not a built-in cabinet template or a structurally approved construction.

## Goals / Non-Goals

**Goals:**
- One authoritative dimensioned project model feeding 3D, stock views, and exports.
- Headless validation of measurements, edits, allocation, and manufacturing feasibility.
- Predictable edits with atomic undo, explicit invalidation, and stable part identity.
- Reproducible local files and catalog snapshots; no runtime network dependency.
- A delivery path that exercises the complete workflow before polishing optimization or hardware.

**Non-Goals:**
- A CAD boundary-representation kernel, parametric constraint solver, physics engine, or game-engine entity system in the first release.
- Treating animations as manufacturing or safety evidence.
- Universal minimum-cost packing or support for every shop pricing/equipment rule.
- A public plugin API, live collaboration, cloud inventory, or automatic internet catalog scraping.

## Decisions

### 1. Rust desktop shell with a dedicated viewport

Use `eframe`/`egui` with its `wgpu` backend, a custom depth-tested board renderer integrated through `egui_wgpu::Callback`, and egui drawing for the 2D sheet workspace. Use the same wgpu version as the selected eframe integration, rather than independently selecting conflicting renderer versions. The first-release target is macOS arm64 with Metal. The existing Vulkan/X11/Wayland code and Linux package remain experimental, not release-validated or supported by this change. Display a clear startup diagnostic if no supported graphics adapter is available.

The current documentation inspected on 2026-09-25 describes eframe 0.36.2, egui-wgpu 0.36.2 and its wgpu 30 dependency. Pin a compatible tested family during implementation and commit the application lockfile; these observations are not a promise that untested hardware works. Start with shaded boxes, edge outlines, a grid, axes, selection/anchor highlights, and perspective plus orthographic views. No photorealistic materials are required.

Alternatives: Qt/QML adds a second UI language and integration layer; a game engine adds an ECS and scheduling model not needed for rectangular editing. A custom winit-only shell increases basic UI work. Revisit the selected stack only if the early platform/interaction gate fails, then update this design rather than silently switching architecture.

References: [eframe](https://docs.rs/eframe/0.36.2/eframe/), [egui-wgpu](https://docs.rs/egui-wgpu/0.36.2/egui_wgpu/). Its accessibility integration should remain enabled, but custom viewport accessibility and production packaging must be tested rather than inferred from library support.

### 2. Headless core and thin adapters

Start with a Rust library plus desktop binary in the existing package, organized into modules. Separate crates can follow when boundaries justify them; do not create a large workspace merely to mirror specification names.

```text
Desktop shell / localization / 3D and stock views
                         |
                  Edit commands
                         v
             Authoritative project model
                |                 |
                v                 v
          Spatial queries    Cutting planner
                |                 |
                +--------+--------+
                         v
              Validated snapshot / export

Persistence adapter <--> Versioned document
Catalog adapter     ---> Pinned hardware data
```

Suggested modules: `domain`, `units`, `commands`, `geometry`, `planning`, `hardware`, `persistence`, `export`, `ui`, and `i18n`. Geometry derives from model dimensions; the mesh is never the input to a cut list. Domain/planner tests must not initialize a GPU. No new application code is written as part of this planning change.

### 3. Exact manufacturing lengths, floating spatial transforms

Store manufacturing lengths as checked signed 64-bit integer micrometres (0.001 mm). Positive dimension constraints apply to stock and parts; translations can be signed. Parse decimal numbers and fractional inches as rational/decimal values, convert using exactly 25.4 mm per inch, and round only at the domain boundary. If input is not representable, show the rounded value for confirmation rather than silently changing it. Use checked arithmetic and wider intermediates for areas; reject overflow and non-finite spatial inputs.

Keep board-local axes fixed: X = length, Y = width, Z = thickness. Store rigid assembly poses with f64 translation and normalized quaternion rotation; no scaling or shearing through transforms. Convert to camera-relative f32 for GPU buffers. Spatial snap tolerances are interaction tolerances, not permission to exceed stock dimensions: allocation checks use integer manufacturing coordinates without a fuzzy fit margin.

Quantize explicit numeric position inputs, but not derived poses from rotation, reparenting or centred resizing. Bound world coordinates to +/-1,000,000 mm per axis and check positional preservation to 0.000001 mm and angular preservation to 1e-9 radians. An odd-micrometre dimension change can require a half-micrometre pose shift; keep that f64 shift rather than moving the anchor. Detect out-of-range edits before commit. Manufacturing precision applies to lengths and sheet coordinates, not arbitrary world-space bounding measurements.

Store money as integer minor currency units, one currency per project, with unknown prices represented separately from zero. Currency selection changes formatting/denomination only after explicit confirmation, never an automatic exchange-rate calculation. Separate user precision of display from stored precision; changing display units never rewrites lengths.

Alternative: binary floating lengths are simple but make equality, kerf boundaries, and repeated unit conversion fragile. Full exact rational geometry is unnecessary for freely rotated boxes; use precision where manufacturing depends on it.

### 4. Stable identities and distinct part, stock, and hardware concepts

Use persistent UUIDs, not names or list indices, for references. Main records:

| Record | Key data |
|---|---|
| Project | schema version, identity, document revision, manufacturing revision/fingerprint, units, currency, catalog snapshots |
| Material | composition/grade identity, user label, default thickness, directional-grain default |
| Board | identity, name, material reference, blank length/width/thickness, grain requirement, local pose, parent assembly, resize anchors |
| Assembly | identity, name, rigid local pose, parent identity, children |
| Stock piece | identity, material, measured rectangle, grain, owned/purchase status, price, priority, trim allowances |
| Allocation | board ID, stock ID, integer origin, allowed quarter-turn orientation, lock, derived validation issues |
| Cut tree | stock root, split nodes with kerf, terminal part/offcut/waste rectangles |
| Hardware instance | identity, dimensions/pose or catalog configuration, parent, role |
| Door joint | moving root, stationary mounting board, hinge instances, closed pose, approximate motion parameters |
| Export record | manufacturing fingerprint, settings, language/units, path and time; not authority for current feasibility |

Board blank dimensions and finished rectangular shape coincide initially; expose separate accessors so later machining changes do not make the optimizer use the wrong footprint. Do not introduce unimplemented shape types merely as speculative extension points.

Material identity represents user-declared composition/grade. The allocation key is (material identity, effective thickness), independent of current material defaults. Material thickness edits are explicit transactions showing affected boards; they cannot silently convert existing physical stock. A stock thickness correction changes compatibility, not the dimensions of every referencing board. Creating a board from a material copies its current default thickness into its blank. Preserving dependants leaves their effective thickness/grain snapshots intact; explicitly applying a change updates confirmed boards but not measured stock. Distinct material IDs do not automatically alias, even if their display names match.

Use board grain values length/width/unrestricted and stock values along-X/along-Y/nondirectional/unknown. Constrained boards align their grain axis to directional stock; unrestricted boards or nondirectional stock allow quarter turns. Unknown stock grain blocks constrained boards until resolved. A change of a material's default is applied to existing default-following boards only through the explicit dependant-update transaction.

### 5. Editing is transactional; previews are not document mutations

Commands validate inputs and apply atomic domain changes plus affected validation-state updates. Undo/redo records before/after deltas including allocations and relationships, not rendered meshes. A drag produces transient previews and one command on confirmation. Cancel clears the preview. Batch dimension changes preflight every selected part and either apply together or report errors without partial edits. Dimensional anchors operate in each board's local frame.

When transforming a selection, collapse selected descendants whose ancestor is selected so each physical item transforms once. Reparent using the inverse new-parent world pose to preserve world placement; reject cycles. Assembly visibility is session/display state and never removes stock demand. Deletion reports dependent allocations/joints and removes or detaches them as one reversible transaction.

Face placement first aligns opposing normals, then allows explicit in-plane edge/centre alignment and offsets. It creates a pose, not a permanent relationship. Face snapping uses screen-distance-ranked candidates with a visible target; numeric entry offers a precise alternative to a dense snap field. Add a world-XY placement grid with project-local positive spacing (default 10 mm) and a labelled grid-snap preview during Move board drags. Grid candidates snap the board's world-space origin in X/Y while retaining its Z and orientation. Rank applicable visible face candidates ahead of grid candidates; when no face candidate applies, preview the grid pose. Alt bypasses both. Commit only the final pose as one transaction; do not snap existing boards when changing grid settings. Keep the grid display and snapping based on the same spacing and world origin, including under rotated parent assemblies. Include trackpad-friendly navigation and selectable view presets. Text-field focus suppresses scene shortcuts.

Store the grid spacing as an integer manufacturing length in the project document, not as rounded f64 display text or a global UI preference. Parse metric/imperial input through the existing exact-length path, require confirmation for nonrepresentable values, and reject zero, negative and out-of-range spacing. Add a backward-compatible default for version-1 files without this field so loading an older project changes neither board poses nor the on-disk file until the user explicitly saves. The setting changes the document but not the manufacturing fingerprint: it is an editing aid, not a fabrication dimension. Do not quantize derived sub-micrometre poses except when the user explicitly accepts a snapped movement.

Alternative: mutate the scene each frame and snapshot it for undo. Rejected because it obscures transaction boundaries and makes stock/hardware consistency harder to guarantee.

### 6. Cut trees are feasibility witnesses, not just pictures

Each stock item has a rectangular root and an optional sequence of edge-trim splits before the usable rectangle. A split stores axis, retained child extent, kerf band, and both outputs. Require: input extent = first output + kerf + second output. Zero-width waste output is allowed for an edge shaving pass whose remaining loss equals kerf, but never as usable stock. A board already exactly matching a leaf needs no cut. The full kerf must lie inside the stock: 100 mm from 103 mm with 5 mm kerf is explicitly unsupported, whereas 100 from 105 is supported. Exterior blade overlap is a deferred operation, not physically impossible. Trim allowance means total edge loss including kerf, so a positive allowance below kerf is invalid. Fix trimming order (left, right, bottom, top) for deterministic accounting; four 5 mm trims on a 100 x 100 mm sheet remove 1,900 square mm, since later cuts operate on narrower intermediate pieces.

Every part must end as an exact leaf, never just a rectangle floating inside a larger offcut. Generate any additional trimming cuts required to isolate it. Split traversal yields a parent-before-child sequence with stable intermediate labels. Count one split as one physical pass; do not count per part, each drawn line segment, or both sides of a shared cut. Annotate saw-reference edge, retained-side boundary, and kerf side unambiguously.

For arbitrary manually placed layouts, rebuild a witness by recursive guillotine decomposition. Candidate kerf strips come from part boundaries and stock edges; a strip must span the current region and intersect no part. Try both axes and partitions with memoized states. Empty regions become offcuts; one-part regions still require isolation cuts. If a search budget is exhausted, report 'feasibility not established', not a proof of impossibility. Such a candidate cannot be committed as valid or exported shop-ready.

For partial drafts, validate the placed subset but separately require every project board to be assigned before shop-ready export. Part/stock edits can retain invalid placement records as visible drafts; derived cut trees are discarded or marked stale until a new witness is established.

To avoid trapping a user behind multiple conflicts, sheet editing supports an atomic repair session with several staged moves/unallocations. Intermediate previews can remain invalid; final acceptance requires witnesses for the placed subsets of affected sheets, leaving explicitly unallocated parts as draft issues. Cancel restores all starting records. Explicit unallocation is always available without requiring the remaining sheet to already be valid. This retains strict valid-placement commits without preventing recovery from design-induced invalid layouts.

Alternative: generic rectangle packing followed by cut counting. Rejected because a non-overlapping layout may not be guillotine-cuttable and cannot support credible paid-cut estimates.

### 7. Stable first-fit and bounded explicit optimization

First-fit scans physical stock in the visible user order, then feasible leaf/candidate origins in a deterministic order, trying unrotated then allowed rotated orientations. Existing allocations, including unlocked ones, are fixed for this operation. Reconstruct or extend the witness without changing existing part locations. No fit leaves the part unallocated; purchase stock is a finite user-declared pool, not an unlimited supply silently bought by the algorithm.

Optimization runs on an immutable snapshot in a worker. Search several deterministic orderings (area, longest edge, constrained grain, cost-aware stock order), split orientations, and bounded beam candidates. Locked part placements are immutable obstacles whose stock, position, and orientation must be preserved in reconstructed witnesses; merely preserving a parent cut node is not sufficient. Start with the current complete feasible layout as an incumbent when available so a search never degrades it without choice.

Rank complete feasible candidates by the chosen objective. For cost, compare integer totals, then unused area, then cut count; do not rank incomplete estimates as cheapest. For fewest cuts, tie-break by unused area then complete cost when comparable; for least unused area, tie-break by cut count then complete cost. Finish ties with stable stock/part IDs. Report actual leftover rectangles and distinguish kerf/trim loss from recoverable offcuts; the 'least unused stock area' objective minimizes full used-stock area minus finished-part area (including reusable offcuts). Explain this definition rather than calling it irreversible waste.

Provide cancellation and bounded search duration; all results carry a source document token. Acceptance requires the same relevant-data token and reruns authoritative validation before one undoable commit. A locked invalid allocation or stale source stops acceptance with an explanation, never an automatic unlock.

Initial performance acceptance fixture: 100 rectangular parts over 10 stock pieces; bounded optimization budget of 5 seconds, cancellation observed within 250 ms on the documented test machines. This is a proposed engineering budget, not a universal hardware guarantee. Direct editing must remain available while the worker runs. Record platform hardware and timings in validation results rather than hiding failures behind unspecified benchmarks.

### 8. Hardware is curated data; motion is an explicitly approximate adapter

Start catalog verification with FGVTN Click 3D Slow Reta / Calço 0, candidate kit `51MX153DRV00100`, from printed page 23 of the provided catalog. The extracted text indicates a 35 mm cup, 11.3 mm depth, 15-22 mm door thickness and 105 degree opening. These values are research leads, not a validated shipped record: visually check the source drawing, plate compatibility, supported overlay parameters, and identifiers before committing catalog data.

Store small structured factual records with provenance and revision metadata. Do not bundle the manufacturer's PDF, photos, or CAD files without redistribution rights. Redraw our own reference annotations from verified dimensions; record missing information explicitly. If no minimum configuration can be verified or lawfully shipped, pause the hardware milestone and ask for a substitute source instead of silently delivering a fictional hinge.

Minimum release evidence must cover exact hinge/plate pairing, overlay/inset applicability, door thickness, cup diameter/depth and edge-position reference, plate positioning references and opening limit, each mapped to source page/revision. Optional screw details can remain unavailable; missing cup/plate positioning cannot count as a delivered supported configuration. Record source-to-data checks as reviewable fixtures, separate from approximate motion tests.

Use one approximate fixed-axis door transform per moving door root, derived from the configured mounting side and user-visible reference axis. This is intentionally not the concealed hinge's linkage trajectory. Hinge instances share this door joint and only supply mounting annotations/limits; they do not independently constrain it. Apply motion as a display-only overlay to closed poses, without changing project or cut-plan geometry. The moving subtree must exclude the stationary mounting reference; validate relationship cycles and detachments.

Only verified installation dimensions are shown as numeric drilling references. No hinge-load or quantity recommendation is inferred. Missing fastener data remains unavailable. Pin used entries inside each project; external catalog refresh never retroactively changes a project. Updates are explicit and revalidate dependent references.

Alternatives: reverse-engineer a linkage from a static picture or run rigid-body physics. Rejected as false precision and disproportionate scope. Accurate kinematics can later implement the same joint-preview boundary with validated data.

### 9. Portable versioned document, atomic saves and recovery

Use a single UTF-8 JSON project file with `.pmcab` extension and explicit schema version, serialized with serde/serde_json. It includes catalog snapshots, part/stock data, settings, and user placements. Derived validation/cut trees are caches: rebuild or verify against a stored input fingerprint on load. Reject unknown newer versions, dangling IDs, cycles, invalid numeric values, and unreasonable file sizes before replacing the current document. Never execute project content or fetch URLs merely because a project references them.

Save by serializing to a temporary file in the destination directory, flushing, and atomically replacing the target; preserve the old file on failure and retain the dirty indicator. Keep autosave recovery separately in the application's user-data directory, keyed by project identity/path. Autosave after 30 seconds of inactivity following a committed change; offer recover/discard on reopening rather than silently overwriting a saved file. Do not persist uncommitted drag/input previews. Prompt before closing unsaved work.

History is session-local in the first release, not a permanent event log. A wood-manufacturing fingerprint covers all wooden parts, stock, cutting assumptions, prices, labels, and allocations required by the output. A separate export-content fingerprint extends it with all included hardware identities, quantities, labels, catalog snapshots and installation references. Camera, selection, hiding, and door-preview angle are excluded. A snapshot ID identifies exported geometry/content; export language/units and timestamps are recorded separately. Adding a foot makes an existing hardware-containing packet outdated without invalidating wooden cuts. JSON favours inspectability and migration tests over a database or opaque binary container at this project scale.

### 10. Localized input and versioned PDF output

Use translation resource keys (Fluent via fluent-bundle) for UI/export messages, including plural forms and parameterized errors. Keep user labels, product IDs, and serialized enums independent of translated text. Ship en and pt-BR resources and embedded fonts with suitable redistribution terms. Use independent UI language, numeric display locale, and export language; no network font loading.

Dimension fields accept one decimal separator, either comma or dot, no grouping separators, supported unit suffixes, and fractional inch forms. Thus `1,200` means 1.200 in a dimension field, never 1200; show parsed-unit previews and examples. Reject mixed separator forms such as `1.234,5` with guidance. Parse first, validate exact precision, then format for locale. Currency fields use the same unambiguous parsing policy with currency precision validation.

Build output from a validated immutable snapshot, not screenshots. Use printpdf for vector diagrams/text and a dedicated pagination/layout layer with embedded fonts. The inspected [printpdf documentation](https://docs.rs/printpdf/0.12.8/printpdf/) exposes page operations, units, fonts and text shaping; validate Portuguese glyphs and font embedding early. No HTML/browser runtime is needed.

Generate a stock summary, one or more pages per sheet with cut-tree labels, ordered cutting instructions and part/hardware lists. Split crowded diagrams and lists rather than making labels unreadable. Draft pages carry repeated NOT FOR CUTTING markings and issue summaries. Shop-ready validates all boards, including hidden ones, against a current witness. Missing monetary data is a disclosed estimate limitation, not a geometric manufacturing error. Write atomically and record export success only after completion.

Validate hardware guidance separately. Invalid or unverified numeric installation diagrams are omitted with explicit unresolved-hardware notices, while a valid wood-cut packet can still be released. Approximate-motion warnings do not make invalid drilling references suitable to print as instructions.

## Risks / Trade-offs

- [This is a whole-application change] -> Deliver vertical milestones below; do not mark the first release complete when only its editor works. Split into independently approved changes later only if the user requests it, preserving coverage.
- [Guillotine search is combinatorial] -> Construct valid cut-tree candidates, bound reconstruction/search, retain the incumbent, and disclose unknown feasibility rather than promise an optimum.
- [A valid abstract plan may be impractical for a shop] -> Document no stacking, full-span cuts, kerf/trim and flat pricing; shop-ready means valid under these assumptions, not shop approval or safe handling certification.
- [Source drawings and catalog rights are incomplete] -> Specific verification gate before catalog shipment; no fabricated drilling geometry or redistributed assets without rights.
- [Independent copies are easy to make inconsistent] -> Multi-selection and clear selection summaries rather than implicit linked edits.
- [GPU/platform differences] -> Mac arm64 automated renderer tests and a same-host process-start check do not verify the Metal viewport, high-DPI or user interactions in a native window. The user waived an interactive/clean-machine macOS release check here; record this limitation explicitly. Linux Vulkan/X11/Wayland is outside the revised release scope.
- [Project-file workflow timing] -> The headless multi-architecture save-transfer-reopen check and macOS in-process desktop-file tests provide portability and lifecycle evidence. Native picker, GUI recovery and cross-platform transfer have not been manually witnessed; the user waived these as release gates for this change, not as facts established by the tests.
- [Approximate motion looks authoritative] -> Persistent warning and no verified-clearance badge; prevent preview pose from contaminating manufacturing dimensions.
- [Manufacturing precision exceeds real-world tolerances] -> Explain 0.001 mm is storage resolution, not saw accuracy; no structural or fit guarantees.

## Migration Plan

No production data migration is needed from the current starter application. Introduce schema version 1 and fixture-based round-trip tests; future migrations require explicit version-to-version conversions and preservation of original files. Rollback during development is reverting application changes without rewriting user documents; older binaries must refuse newer formats safely.

Delivery milestones:
1. Headless domain, measurements, edit transactions, persistence, localization skeleton and macOS native viewport implementation.
2. Board editing and nested assemblies connected to stock, manual allocation, cut-tree validation and first-fit; exercise invalidation and undo end to end.
3. Costed optimization, PDF exports, revision tracking and shop-ready gating with a complete cabinet fixture whose construction is documented as test data only.
4. Verified initial hardware data, feet, installation references and approximate motion integrated with the same editing/persistence model.
5. macOS arm64 packaging, headless recovery/failure tests and user documentation in both languages. Linux GUI validation is postponed to a separately scoped change.

The first-release artifact is an ad-hoc-signed macOS arm64 application bundle with declared Metal prerequisites, bundled license notices and embedded resources. The previously assembled Linux archive is experimental, not evidence of a supported Linux release. The macOS artifact was built and started as a process on the build host; interactive and independent clean-machine behavior remain **unverified by explicit user waiver**. Other architectures are not implied. Developer ID signing/notarization and distribution-channel credentials remain release logistics, not a reason to introduce accounts into the application.

## Open Questions

- The exact release signing identity, distribution channel, and oldest supported OS/GPU versions can be chosen after the early platform fixture, without changing the domain or user workflow.
- Additional hinge families after the first verified configuration and exact visual theme are deferred; neither changes first-release acceptance.
- The shop's actual kerf, prices, and trimming practice are user-entered project data, not unresolved algorithm assumptions.
