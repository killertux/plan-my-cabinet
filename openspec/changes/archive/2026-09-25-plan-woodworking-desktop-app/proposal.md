# Proposal

## Why

Amateur woodworkers need to design furniture freely while knowing whether its parts can actually be produced from available wood, with realistic saw losses and cutting costs. Plan My Cabinet currently contains only a Rust starter application; this change establishes a first usable desktop woodworking planner connecting precise 3D assemblies to stock allocation and shop-ready cutting instructions, rather than imposing predefined cabinet designs.

## What Changes

- Deliver a Rust-native, offline-capable desktop application for **macOS arm64** in this first release, with local project files and no required account or backend. Support English and Brazilian Portuguese, metric defaults, imperial input, and localized measurements and monetary estimates without changing underlying project values. Linux implementation and a preliminary package may exist, but Linux is **not a supported or release-validated target** in this change.
- Create rectangular wooden parts through a **New board** dialog with exact length, width, and material-derived thickness. Keep dimensions editable with explicit start/centre/end resize anchors. Default plywood grain to the board's length, with per-part overrides.
- Let users assemble parts freely using dragging with temporary face and configurable grid snapping, face-based placement, and numeric position/rotation controls. Give each project a visible placement grid with user-adjustable spacing. Provide independent duplicates, multi-selection editing, and nested assemblies that move their contents together without linking dimensions. Ordinary placement does not implicitly create persistent constraints.
- Keep the assembly and sheet views linked through the same physical parts: selecting a part highlights its allocation, but moving its allocation does not move it in the furniture. Support project-local rectangular sheets and manually entered rectangular offcuts, distinguished by material, thickness, dimensions, grain, and owned versus to-purchase status.
- Automatically allocate new parts using first-fit placement that respects material, grain, configurable saw kerf, and the supported cutting method without rearranging existing allocations. Keep parts that cannot fit in the design and clearly mark them unallocated. Allow direct sheet-layout editing and placement locks, with explicit optimization previews rather than silent repacking.
- Restrict first-version cutting plans to full-span straight cuts across the current rectangular workpiece, producing rectangular pieces with kerf removed. Generate a feasible cut sequence, not merely non-overlapping packed rectangles. Optimize with monetary estimates, cut count, and material usage visible; do not claim a globally optimal solution without proof.
- Estimate new spending from full purchase prices of used to-purchase sheets plus planned cutting charges. Owned sheets and offcuts add no new purchase expense, but their consumption remains visible. Treat pricing as an estimate dependent on shop assumptions rather than a binding quotation.
- Permit design edits that invalidate allocations while preserving visible, actionable conflicts. Distinguish draft layouts from shop-ready exports, blocking the latter until all required wooden parts are allocated and the supported cutting checks pass. Provide a printable handoff with labelled parts, sheet layouts, cutting sequence, kerf assumptions, and cost/cut-count estimates; identify the exported revision when the design subsequently changes.
- Represent feet with simple hardware placeholders excluded from wood allocation. Add a small curated catalog of real hinge-and-mounting-plate configurations, manufacturer-based installation references, and approximate door-opening simulation clearly labelled as such. Approximate motion must not claim verified physical clearance or structural suitability. Keep hardware relationships distinct from ordinary grouping and snapping.

### First-version boundaries

Rectangular parts and rectangular stock are sufficient initially. Defer freeform shaping, notches, holes as machined solids, angled cuts, irregular stock/defect regions, shared inventory, cloud collaboration, and accurate hinge kinematics. Preserve an extension path separating a part's stock blank from its future finished shape. Cabinet generators are not the foundation or a first-version requirement.

The representative workflow is a plywood cabinet cut by a shop using a track saw and charging per cut. The discussed example has a body 820 mm wide, 600 mm deep, and 2300 mm high, 18 mm main panels, doors, and plastic feet. These are example values, not application defaults or fixed construction rules; body height excludes feet, and overall dimensions must remain distinguishable. Door/back construction, foot height, and exact hinge selection remain undecided.

## Capabilities

### New Capabilities

- `project-foundation`: Local project persistence, measurement semantics, stable part identity, undo/redo, and recovery behaviour.
- `boards-and-materials`: Dimensioned rectangular boards, materials, thickness, grain, anchored resizing, independent copies, and batch dimension editing.
- `assembly-editor`: 3D navigation and selection, precise placement and snapping, multi-selection transforms, and nested assemblies.
- `stock-allocation`: Project-local stock and offcuts, grain-aware first-fit allocation, linked views, manual layout editing, locks, and allocation invalidation.
- `cut-planning-and-costs`: Kerf-aware full-span cut sequences, layout feasibility, deliberate optimization, material consumption, and estimated purchase/cutting costs.
- `hardware-and-motion`: Simple feet, curated hinge/plate data, installation references, explicit mechanical relationships, and labelled approximate motion.
- `workshop-outputs`: Parts lists, labelled sheet layouts and cut instructions, draft/shop-ready validation gates, and exported-plan revision identification.
- `desktop-and-localization`: Rust-native macOS arm64 experience, offline operation, English/Brazilian Portuguese UI and outputs, and locale-aware input/display.

### Modified Capabilities

None. The project has no existing capability specifications.

## Impact

- Future implementation will expand the bare Rust application and add desktop UI, rendering, persistence, geometry, planning, localization, and export dependencies. There are no existing application APIs or saved-file compatibility contracts to break. Existing OpenCode/OpenSpec command routing is outside this change's scope.
- The accompanying design selects eframe/egui with wgpu, a headless woodworking model, integer manufacturing lengths, a versioned local document, and bounded cut-tree-based optimization. Headless checks and a macOS build-host process-start smoke test provide implementation evidence; the user explicitly waived interactive and separate clean-machine GUI release validation for this change. This is not a claim of verified GUI behavior or production certification.
- The [FGVTN general catalog](https://www.fgvtn.com.br/site/novopdf/Catalogo_Geral.pdf), including the Click 3D Slow family on printed page 23, is an initial hardware research source, not an already validated dataset. Visually verify installation diagrams and exact product/plate combinations before encoding them; catalog opening angles alone do not establish motion trajectories. Core offline operation must not depend on fetching this catalog.
- Design, eight capability specifications, and staged implementation tasks accompany this proposal. The implemented board-to-assembly-to-stock-to-export workflow is exercised by automated tests; deferred platform and interactive checks remain recorded in the release checklist rather than being represented as passing tests.
