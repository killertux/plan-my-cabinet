# Proposal

## Why

The current scrolling-sidebar interface obscures related editing workflows and does not deliver the experience specified by `design_handoff_egui_redesign/`. Reproduce that handoff as a functional native application, including the interaction, rendering, persistence, and document features needed to make every pictured control truthful and useful.

## What Changes

- Replace the sidebar-centric presentation with Design, Stock, Cut plan, Hardware, and Handoff workspaces sharing a rail, command search, header, status bar, linked navigation, and selection.
- Match the chosen Main Window **2a** direction and the remaining handoff screens: warm light theme, bundled typography/icons, native styled widgets, inspectors, viewport overlays, sheet canvas, dialogs, Welcome, and Settings. Rejected 1a/1b/1c layouts are not alternative deliverables.
- Add functional command search, workspace shortcuts, inline inspector/HUD editing, bounding-measure tool, pose presets, material colors, stock drag ordering/filtering, and contextual diagnostic navigation. Preserve advanced capabilities omitted from the illustrations.
- Add shared paginated document layout for PDF preview and export, output-section controls, enriched historical receipts, and current-versus-historical status without weakening shop-ready checks.
- Add Welcome/recent-project management, generated thumbnails, one-time Base/Wall/Drawers assembly generators, persistent application preferences, and explicit recovery management. Drawers generates the carcass, drawer boxes, and fronts, with editable construction assumptions and clearances; it does not select slides or certify mechanical fit.
- Preserve unfinished edits with explicit navigation decisions, adapt panels outside the 1440 x 900 reference size, and verify visual fidelity alongside functional regressions in English and pt-BR.
- Evolve persisted metadata compatibly for display colors, confirmation provenance, and receipts; distinguish project dirty state from manufacturing freshness. Existing files must remain loadable without source-file rewriting on open.
- Correct door-preview handedness from the selected hinge edge and cup face so mirrored doors open outward. Preserve legacy relationship data on open; affected stored axes require explicit review/reconfirmation, never silent rewriting. This focused correction does not certify concealed-hinge trajectories or collisions.

## Capabilities

### New Capabilities

- `desktop-workspaces`: Shared five-workspace shell, action palette, cross-workspace routing, edit/navigation lifecycle, and reference-fidelity acceptance.

### Modified Capabilities

- `desktop-and-localization`: Unified modal interaction, adaptive layouts, persistent app preferences, and localized shortcuts/help.
- `assembly-editor`: Design workspace, camera/tool overlays, measurements and pose presets with existing transaction semantics.
- `boards-and-materials`: Shared inline dimension drafts, truthful effective-thickness presentation, creation previews, and persisted manufacturing-neutral colors.
- `stock-allocation`: Stock workspace, priority drag ordering/filtering, stable display labels, sheet navigation, and complete repair actions.
- `cut-planning-and-costs`: Cut-plan visualization, cut-hover linkage, witness-backed statistics, optimization comparison, and dated kerf confirmation.
- `hardware-and-motion`: Hardware workspace inspector/tree and derived annotations with full mounting controls, workspace-scoped motion preview, outward edge/face-based handedness and explicit legacy relationship review.
- `project-foundation`: Welcome, recent projects, recovery management, thumbnails, one-time template generation, and metadata compatibility.
- `workshop-outputs`: Shared preview/export layout, optional sections with mandatory safety content, and richer historical receipts/freshness.

## Impact

- UI integration: `src/main.rs`, `viewport.rs`, `assembly_ui.rs`, `placement_ui.rs`, `stock_ui.rs`, `sheet_ui.rs`, `optimization_ui.rs`, `hardware_ui.rs`, `hinge_ui.rs`, `door_joint_ui.rs`, and `project_ui.rs`.
- Domain/services: existing command and preview mechanisms, `domain.rs`, `persistence.rs`, `recovery.rs`, `export.rs`, and `pdf_export.rs`; new presentation/layout modules and local app-state storage.
- Assets/dependencies: licensed additional Noto Sans/JetBrains Mono weights, handoff SVGs, egui-compatible SVG/table support where appropriate; preserve eframe 0.36.2/wgpu and offline operation. No webview, backend, or browser runtime in the product.
- Tests/docs: deterministic reference fixtures, native screen comparisons and interaction checks, migration/freshness/export-layout tests, and updated bilingual workflow documentation.
- No intentional removal of existing editing/manufacturing capabilities. Full handoff is the delivery scope; staged implementation is not permission to leave screens or visible controls as placeholders. Template assumptions must be visible before generation; recovery cleanup must require explicit selection and confirmation rather than an implicit age-based deletion policy.
