# Architecture and UX improvement plan

Source: the architecture review of September 2026. The work comes in four phases. Each phase
ends with all tests green (`bintests.py` + `cargo test --lib --tests`), clippy clean, and
its own commit(s) on `master`. No phase changes domain rules, the project file format or
command semantics.

## Phase 1 — Error handling (small) — done

What was done (differs slightly from the original plan):
- `src/toasts.rs` holds the queue. `invoke_or_report` / `report_edit` replace every
  discarded action or edit result.
- The panic guard wraps the frame in `catch_unwind`, flushes recovery immediately and
  re-raises the panic, instead of a global hook.
- Autosave for untitled projects is new: before this, never-saved projects had no
  recovery at all. The snapshot is removed on save, on New/Open, and on a deliberate
  close.
- `Project::board/material/stock_piece(_mut)` lookups replace the repeated
  `iter().find().unwrap()`.


Goal: no silent failures, no crash without a recovery save, no new production `unwrap`.

1. **User-visible feedback channel.**
   - Add a `Toasts` queue to `DesktopApp` (`push_error(key, args)`, `push_info`).
   - Render the queue as a floating stack at the bottom right of the canvas. Each toast
     auto-dismisses after 5 s, and the × closes it.
   - i18n keys are in both languages.
2. **Stop swallowing results.** Every `let _ = …` on an edit or action becomes a handled
   result. `Unavailable` goes to a toast with its localized reason, and `EditError` goes to
   a toast with a generic "change was rejected" message plus the reason where one exists.
   - `actions.rs`: undo/redo, reorder stock, remove hinge, add builtin catalog.
   - The UI modules that call `self.invoke(request)`: go through a single
     `invoke_or_report(request)` helper.
3. **Panic hook.**
   - `std::panic::set_hook` writes a recovery snapshot of the last committed project
     (shared through an `Arc<Mutex<Option<Project>>>` updated after each commit) through
     `recovery`.
   - It then chains to the default hook.
   - On the next start, the existing recovery flow offers the snapshot.
4. **Remove the unwraps that are only protected by a separate check.** Parse once into
   validated values and use `let … else`:
   - `sheet_ui.rs` (repair place action and the cache reads)
   - `placement_ui.rs` (face placement coordinates)
   - `template_setup_ui.rs` (material proposal, the default entries)
   - `edit_drafts.rs`, `stock_commands.rs`, `board_dimensions.rs`, `first_fit.rs`,
     `placement.rs`
5. **Lint guard.**
   - Add `#![cfg_attr(not(test), warn(clippy::unwrap_used))]` to `lib.rs` and `main.rs`.
   - Fix or justify every remaining site. An `expect("<invariant>")` is allowed when a
     validated invariant guarantees it.
   - Fix the 5 existing clippy warnings.

## Phase 2 — Dialogs as one modal state (medium) — done

What was done:
- `src/modals.rs`: `Modals { active: Option<Modal> }` with generated typed accessors
  (`hinge()`, `hinge_mut()`, `take_hinge()`, `set_hinge(..)`). Opening a second dialog
  trips a debug assertion; the full test suite never hits it.
- `other_modal_open` checks `modals.is_open()` instead of 16 fields.
- Chromes stay where they were: the app-level chromes persist for focus restore.
  Dialogs that owned an `Option<ModalChrome>` now own a plain `ModalChrome` and use
  `ModalChrome::detach()` during `show`.
- The cut fee dialog is a real `CutFeeDialog` holding its own chrome. It no longer keeps
  an `Arc<Mutex<ModalChrome>>` in egui temp data.


Goal: replace the ~20 `Option<XDialog>` fields, the matching `ModalChrome` fields and the
`other_modal_open` chain with a single source of truth.

1. **`enum Modal { Board(CreationDialog), MaterialEdit(..), BoardMaterial(..),
   BoardDimension(..), Batch(..), Placement(..), Grid(..), KerfConfirmation(..), Stock(..),
   CutFee(..), Currency(..), Assembly(..), Hardware(..), Hinge(..), Door(..),
   Removal(..) }`.**
   - Each variant owns its `ModalChrome`, which removes `chrome.take().expect(..)`.
2. **`ModalHost { active: Option<Modal>, suspended: Option<Modal> }`.**
   - `suspended` covers the existing nested case: a board-creation dialog opens
     material creation, which then returns to it.
3. **Behaviour.**
   - `other_modal_open()` becomes `modal.active.is_some() || <non-dialog modes>`.
   - One `show_modal(ctx)` dispatches on the variant.
   - Opening a dialog while another is active is refused, which is what the guards do
     today.
4. **Scope.** Settings, the palette, the navigation prompt and the template setup keep
   their own state; they are not form dialogs.
5. **Tests.** Keep all focus, restore and guard tests unchanged. They are the safety net.

## Phase 3 — Structure (large, step by step)

Goal: clear boundaries and smaller files. The build and tests stay green after every step.

1. **Group the UI into a `src/ui/` folder in the app crate.** Move `*_ui.rs`,
   `command_palette`, `workspace_shell`, `modal_chrome`, `capture`, `welcome_host` and
   `viewport/` there.
   - Move `settings_ui`, `welcome_ui`, `theme`, `theme_widgets` and `icons` out of the
     library into `ui/` as well. The library then no longer needs egui.
   - `material_presets` stays in the library.
   - Today `icons`, `theme` and `theme_widgets` are compiled twice: as `pub mod` in the
     library and again as `mod` in `main.rs`. That gives two distinct copies of every
     type and static. After the move, only one copy exists.
2. **Group the library modules.**

   | Folder | Modules |
   |---|---|
   | `domain/` | domain, units, money, measurements, placement math |
   | `editing/` | commands, `*_commands`, edit_drafts, sheet_edit, assembly_edit, material_changes, board_dimensions |
   | `optimize/` | candidate_generation, candidate_ranking, cut_tree, first_fit, optimization_worker, allocation_diagnostics |
   | `output/` | export, pdf_export, document_layout, workshop_document, receipt read models, cost_estimate |
   | `storage/` | persistence, recovery, recent_projects, local_preferences |
   | `catalog/` | hardware_catalog, hinge_installation, door_joint, template_* |

   - Re-export the old paths during the move so the integration tests keep compiling.
     Then update the imports and drop the re-exports.
3. **Split `DesktopApp` into state plus per-workspace controllers.**

   ```
   AppState    { editor, selection, localizer, preferences, toasts, modal: ModalHost, … }
   DesignUi    { camera state, drafts, measurement, move tool }
   StockUi     { … }
   CutPlanUi   { sheet_repair, optimizer ui }
   HardwareUi  { door motion, catalog notice }
   HandoffUi   { export_* fields }
   ```

   - Each controller exposes `show(&mut self, ui, state: &mut AppState)`.
   - Mutations go through `state.invoke(Request)`; controllers never edit the project
     directly.
   - Move the fields in groups (Handoff first: it is the most self-contained).
4. **Break up the large files by panel** (outliner, inspector, HUD, dialogs):
   `sheet_ui.rs`, `stock_ui.rs`, `assembly_ui.rs`, `hinge_ui.rs`, `main.rs`. Move inline UI
   tests into `tests.rs` submodules next to the code.
5. **Smaller fixes.**
   - Cap undo history at 200 snapshots.
   - Replace the JSON-serialized preview cache key (`sheet_ui::diagnostics_key`) with a
     preview generation counter kept by `ProjectEditor`.
   - Give the bin tests a shared `test_ctx()` that installs the fonts.
   - Give the `release_cabinet` optimizer deadline headroom under load.

## Phase 4 — UX

1. **Automatic re-optimization.**
   - When the design or stock changes, start the optimizer after a 600 ms debounce.
     Cancel a running job when a newer revision arrives.
   - The Cut plan shows an "out of date / optimizing…" chip instead of requiring Start.
   - Keep manual Start as "Re-run".
2. **Empty states that act.** Each workspace with a missing prerequisite shows one short
   line plus the primary button:
   - Cut plan without stock: "Add sheets".
   - Cut plan without boards: "Go to Design".
   - Hardware without doors: "Add door".
   - Handoff with issues: "Fix in Cut plan".
3. **Direct manipulation in the 3D view.**
   - The selected board gets face handles: dragging a face changes that dimension along
     its axis, with snapping to the grid.
   - The change is a preview until release, and one undo step after.
   - It uses the existing preview/commit path (`ProjectEditor` preview).
4. **Rework leftovers.**
   - Allow Save during the door motion preview: save the committed project, not the
     preview.
   - Restore the "go to workspace" buttons in the Position dialogs.
   - PDF sheet diagrams: label small parts through a numbered leader list and avoid
     crossing callouts.

## Order and checkpoints

- **Phase 1:** commit.
- **Phase 2:** commit.
- **Phase 3:** one commit per step, 3.1 to 3.5.
- **Phase 4:** one commit per item.

After each checkpoint, capture the screenshots to confirm there is no visual regression.
