# Release acceptance evidence

## Desktop redesign — acceptance pending

**Hardware precision-fix follow-up (`dist/manual-review-4`):** user testing
exposed falsely blocked left-door motion after opening the supplied fixture.
The default JSON float parser shifted a derived axis by one ULP. Enabling
round-trip float parsing fixes save/reopen without changing exact review guards
or silently rewriting legacy axes. Full Rust tests, formatting, all-target check,
warnings-denied Clippy, whitespace and strict OpenSpec validation passed; log:
`…/opencode/hardware-roundtrip-full-tests.log`. The arm64 package built and passed
plist/deep strict signature verification. It includes the count-card letter-spacing
fix. A fresh `Hardware review v2.pmcab` is ready in `dist/manual-review/test-data`;
the original remains untouched. Native motion acceptance is still pending.

The subsequent user motion batch passed with the v2 fixture. Its screenshot
shows outward 60° left-door preview, H1 selection and the approximate-motion
disclosure; the user reports the batch works. Angle tick labels were visibly
mispositioned, so their proportional layout is corrected in source with focused
geometry/overlap regressions. That correction postdates `manual-review-4` and
still needs inclusion in a later package. Other reference differences remain open.

**Latest responsive follow-up (`dist/manual-review-3`):** the full Rust suite,
formatting, all-target check, warnings-denied all-target Clippy, whitespace check
and strict OpenSpec validation passed. The macOS arm64 bundle built successfully;
plist lint and deep/strict ad-hoc signature verification passed. Log:
`…/opencode/batch3-cards-full-tests.log`. Native retest is pending for the new
localized count-card layout and inspector wrapping; the user already confirmed
F-to-centre works in Design. The earlier `manual-review-2` screenshots confirm
status overflow, visible repair buttons, fitted PDF and truthful empty Hardware
guidance. The ready-made Hardware fixture is
`dist/manual-review/test-data/Hardware review.pmcab`; its generator refuses
overwrites. These results do not close remaining reference-fidelity or release
acceptance tasks, and do not establish notarization or other-platform support.

The subsequent three user screenshots pass the targeted `manual-review-3`
Design HUD, Cut plan optimizer wrapping and readable Handoff count-card checks
at pt-BR/130%. A minor count-label letter-spacing correction and its passing
regression are source-only after this package. Full reference-fidelity acceptance
is still pending; the next user batch covers the prepared Hardware fixture.

**User-run manual-review build, batch 1:** the user confirmed successful launch,
native legacy Open, upgrade Cancel, then accepted upgrade and save. The missing
upgrade screenshot does not invalidate this explicitly reported interaction;
it is user-observed evidence, not an assistant capture. The former native-driver
upgrade-selection blocker is resolved by user testing. The user also reports
passing Stock scrolling/edit-cancel and the explicit kerf-confirmation flow,
with four attached Design/Stock/kerf screenshots. The next reply explicitly
confirmed step 6 Save As → quit → reopen passed. User batch 2 also passed
pending-edit protection, repair navigation/cancellation, populated multi-page
draft PDF export/review and overwrite cancellation without another receipt.
Two screenshots show the disabled-Apply prompt and selected repair part/cuts.
The Batch 2 PDF was not attached; its result is user-observed evidence.

### Latest packaged walkthrough (2026-09-27, modal safety build)

`…/opencode/apply-package-modal-0927/Plan My Cabinet.app` was rebuilt after
the modal safety changes. Before packaging, formatting, all-target check,
warnings-denied Clippy and the full Rust suite passed (log:
`…/opencode/modal-complete-suite.log`). With the isolated
`…/opencode/native-handedness-home` and real Metal/AppKit:

- Reopened the saved Drawers workflow (23 boards, four assemblies, three owned
  sheets), reviewed all four Handoff pages and exported through the system Save
  sheet as `Drawers reviewed packet.pdf`. Shop-ready remained disabled because
  22 boards are unallocated and kerf remains unconfirmed; this was a **draft**.
- Poppler reports four A4 pages, 734005 bytes. Independent PDF reading confirms
  all 23 parts, unresolved issues, the allocated left-side sheet drawing and its
  two physical cuts, and DRAFT / NOT FOR CUTTING on every page. Receipt hash and
  actual file SHA-256 match:
  `4c76a804941e6a2a43d1d5e35c05579d4e61b823bedb304fff0e8489f9636cd8`.
- Exporting to the same destination opened the explicit Replace PDF prompt.
  Enter on visibly focused **Cancel** reported cancellation and preserved that
  exact hash. There is one successful receipt, not a second cancelled receipt.
- Native **Save As** wrote `Drawers packaged Save As.pmcab`; compared with
  `Drawers workflow.pmcab`, only `export_records` differs. Native Welcome recent
  reopen retained all counts and showed **Last export is current** in Handoff.
  The package then closed normally without an unsaved-work prompt.
- Accepted schema upgrade is **still pending**. The Open sheet appeared, but
  the computer-use driver could not select the legacy test file in its icon
  view. Cancel worked and the copied schema-1 bytes remain unchanged. This is
  an automation limitation, not a passed upgrade or diagnosed product defect.

This bundle predates the subsequent explicit kerf-confirmation modal; rebuild
again before final release acceptance. Task 15.5 remains open. The following
entries are historical checkpoints, not claims about the newest source.

2026-09-27 follow-up: rebuilt the arm64 signed bundle after the native
accessibility and Stock/Hardware fixes. It passed `plutil -lint` and deep
`codesign --verify --strict`; offline native Save/Welcome recent/reopen,
missing-file display/Remove, native legacy Open, and the **new** schema-1
upgrade notice's Cancel path were exercised with isolated app data and
temporary files (the Dev test copies were cleared). The original legacy
bytes were unchanged after Cancel. PDF picker/export, explicit native
upgrade acceptance, full workflow and visual reference comparisons remain
unverified; 15.5 is not complete.
The later `…/opencode/apply-package-post-trims-0927/` bundle includes the
Stock trim-stack and Hardware revision-label corrections; plist/signature
verification and its 1280 × 875 Metal Hardware capture passed. In an isolated
`HOME` its native Handoff preview displayed a one-page draft, the explicit
review enabled Export PDF, and the system Save sheet wrote a PDF. The app
reported the current receipt; `pdfinfo` verified one A4 page (143604 bytes),
`pdffonts` found three embedded CID TrueType faces and `pdftotext` found the
draft title/kerf warning. The PDF was moved to
`…/opencode/native-final-home-0927/exported-draft.pdf`, and the temporary
Dev test document cleared. This was an **empty** draft, not a populated
multi-page design; accepted schema upgrade, Save As, full cross-workspace
workflow and clean-machine checks are still pending.

The 2026-09-27 interactive Metal walkthrough opened and cancelled the native
Save sheet in an isolated-home **development** build after generating a Drawers
assembly and stock piece. It found and fixed two AccessKit focused-ID crashes
in the template wizard; the bundle used **then** predates those fixes. It did not
save/reopen, open the schema-upgrade notice, or write a PDF with the packaged
binary, so 15.5 remains pending rather than inferred from this smoke check.

**2026-09-27 current checkout package smoke:** `python3
scripts/package-release.py macos-arm64 --out-dir …/opencode/apply-package-final`
completed from the current source. The ad-hoc signed bundle is arm64 Mach-O;
`plutil -lint` and `codesign --verify --deep --strict` pass. Its Licenses
directory contains 487 files including bundled font notices and dependency
inventory. The **packaged executable**, not the unbundled development binary,
produced a real Metal 900 × 650 pt-BR Design capture and manifest at
`…/opencode/apply-packaged-capture/` with 2× pixels and no project file opened.
This verifies offline embedded rendering/assets on this host, not a network-off
clean-machine run. The newer package follow-up above covers some of the native
picker and schema-1 checks, but not Save As, accepted upgrade or PDF write;
task 15.5 remains open. The existing ad-hoc signature is not
notarization or a Developer ID distribution signature; Linux is unverified and
not a supported release target.

The [redesign acceptance matrix](redesign-acceptance.md) tracks all ten reference
screens, missing states, nine delta capabilities and per-stage evidence gates.
Baseline and widget-gallery capture setup, the five-route shell, and functional
Welcome/Settings surfaces are available. The redesigned responsive/native
interaction matrix and full ten-screen comparison are unfinished. Earlier
isolated Metal attempts timed out, but a later foreground logged-in session
produced all five workspace, empty Welcome and three Settings captures. The
native comparison **found material layout and clipping failures** in Design,
Stock, Cut plan, Hardware, Handoff, Welcome and Settings; see the foreground
review at the top of the acceptance matrix. The full sequential
headless gate and measured optimizer workload are recorded in the matrix, but
no native redesigned-screen acceptance is claimed. On 2026-09-27 all five
offline packaging/license unit tests passed; a new bundle has not yet been
built or interactively launched from this checkout. The historical waivers
below concern the earlier change and do
not waive native screenshot fidelity or interaction checks for this redesign.
The 2026-09-26 foundation evidence (native gallery, offline icon raster,
typography and licensing checks) is in [redesign-theme.md](redesign-theme.md).
The offline macOS bundle assembled successfully, but its ad-hoc signed binary's
capture timed out; an unbundled release binary captured the gallery. This is
not a packaged visual or redesigned-screen pass.

On 2026-09-27 the post-Handoff/Hardware/Cut plan host gate passed formatting,
all-target check, warnings-denied Clippy and the full sequential Rust suite;
see the timestamped transcript and limitations in the acceptance matrix.
New board/Material modal, witness inspector and optimizer comparison tests
were added, but no native visual approval follows from them. At that checkpoint,
capture retries still received no native screenshot event even after 1200
frames; the temporary retry was removed. The later successful foreground run
used a supported `Bgra8Unorm` target and real screenshot callbacks, not a
headless frame. Reflow and recapture the material reference deviations before
claiming native acceptance.

**Revised acceptance scope (2026-09-26):** macOS arm64 only. The user removed
Linux release acceptance and explicitly waived interactive and separate
clean-machine macOS GUI validation. All 65 revised OpenSpec implementation
tasks are checked. This means **implementation/checklist complete under the
revised criteria**, not that a native GUI, independent machine, shop process
or production distribution has been certified. The historical Linux build
notes below describe experimental artifacts only.

## Task 11.1 — integrated cabinet (2026-09-25)

- [x] Local macOS offline checks passed for the 11.1 fixture (177 library unit, 54 desktop unit, 10 integration tests across seven suites, zero doc tests). Later desktop-file integration raised the full-suite counts to 178 library and 58 desktop tests; the final commands are recorded below. The targeted `cargo test --offline --test release_cabinet` also passed with Poppler text extraction available.
- [x] `cargo test --offline --test release_cabinet`: `rectangular_cabinet_release_walkthrough` passes. A 2300 mm-high rectangular body, independently duplicated shelf, four 100 mm feet, pinned FGVTN hinge and explicit door joint share one editable project. The body and overall heights measure 2300 and 2400 mm; feet do not enter the five-board cutting demand. A 90° opening leaves the document untouched.
- [x] The shelf resize creates an overlap involving a locked duplicate. Shop-ready export is refused until the sheet repair explicitly unlocks, moves and relocks the copy. Original duplicate dimensions remain independent. The repaired state has no allocation conflicts.
- [x] A bounded lowest-new-spending worker returns ranked complete candidates. Each offered candidate is independently revalidated and retains the locked allocation; the first candidate is explicitly accepted. Shop-ready English and pt-BR PDFs are written from that revision, with a verified hinge reference and four stock witnesses. PDF bytes and differing output bytes are checked; where Poppler `pdftotext` is installed, both localized warnings/headings, the catalog kit ID and the duplicated part ID are checked in extracted text.
- [x] Save/reopen of the `.pmcab` file preserves boards, allocations, hardware, catalog snapshot, hinge installation, door joint and export receipts; shop-ready preparation still succeeds after reopening. This is a headless local-file round trip, not a cross-platform GUI transfer.

## Capability acceptance trace

The scenario headings in the change's eight `spec.md` files are exercised by these automated test suites and documented manual walkthroughs. The integrated test above ties the manufacturing/hardware path together; narrower edge cases remain in their focused suites.

| Capability | Automated evidence | Manual evidence / outstanding check |
| --- | --- | --- |
| Project foundation | `tests/portability.rs`, `tests/stock_walkthrough.rs`, `tests/release_cabinet.rs`; unit tests in `src/commands.rs`, `src/persistence.rs`, `src/recovery.rs`, `src/units.rs`, `src/money.rs`, `src/domain.rs` | `docs/project-en.md`, `docs/project-pt-BR.md` describe file portability, recovery and compatibility. User-driven GUI recovery remains unverified by waiver; cross-platform GUI transfer is outside release scope. |
| Boards and materials | `tests/cabinet_assembly.rs`, `tests/allocation_invalidation.rs`, `tests/release_cabinet.rs`; unit tests in `src/board_commands.rs`, `src/board_dimensions.rs`, `src/material_changes.rs` | Keyboard New board and precision controls: `docs/boards-en.md`, `docs/boards-pt-BR.md`; GUI keyboard run remains to be recorded on release hosts. |
| Assembly editor | `tests/cabinet_assembly.rs`, `tests/release_cabinet.rs`; unit tests in `src/assembly_edit.rs`, `src/placement.rs`, `src/measurements.rs` | `docs/assembly-en.md`, `docs/assembly-pt-BR.md`, `docs/viewport-en.md`, `docs/viewport-pt-BR.md`; native macOS trackpad/viewport interaction remains unverified by waiver. |
| Stock allocation | `tests/stock_walkthrough.rs`, `tests/allocation_invalidation.rs`, `tests/release_cabinet.rs`; unit tests in `src/first_fit.rs`, `src/sheet_edit.rs`, `src/allocation_diagnostics.rs` | `docs/stock-en.md`, `docs/stock-pt-BR.md`; interactive sheet drag/selection and conflict overlays need GUI confirmation. |
| Cut planning and costs | `tests/optimization_performance.rs`, `tests/shop_handoff.rs`, `tests/release_cabinet.rs`; unit tests in `src/cut_tree.rs`, `src/candidate_generation.rs`, `src/candidate_ranking.rs`, `src/cost_estimate.rs`, `src/optimization_worker.rs` | Shop assumptions and optimizer limits: `docs/stock-en.md`, `docs/stock-pt-BR.md`. Native GUI responsiveness remains unverified by waiver. |
| Hardware and motion | `tests/release_cabinet.rs`; unit tests in `src/hardware_catalog.rs`, `src/hinge_installation.rs`, `src/door_joint.rs`, `src/measurements.rs` | Manufacturer field/source review: `docs/hinge-source-review.md`; constraints in `docs/hardware-en.md`, `docs/hardware-pt-BR.md`. Visual motion and installation legibility remain manual. |
| Workshop outputs | `tests/shop_handoff.rs`, `tests/release_cabinet.rs`; unit tests in `src/export.rs`, `src/pdf_export.rs` | `docs/shop-handoff-en.md`, `docs/shop-handoff-pt-BR.md`; printed dense/simple PDF legibility and shop review remain manual. Poppler extraction is conditional in the integration tests. |
| Desktop and localization | `tests/portability.rs`, `tests/release_cabinet.rs`; unit tests in `src/i18n.rs`, `src/dimension_input.rs`, `src/viewport.rs`, `src/main.rs` | `docs/input-en.md`, `docs/input-pt-BR.md`, `docs/viewport-en.md`, `docs/viewport-pt-BR.md`; native macOS keyboard, DPI and offline GUI workflows remain unverified by waiver. Linux is experimental. |

The section above records task 11.1 headless evidence. Revised tasks 11.2–11.4 use the macOS automated/build-host evidence and explicitly record waived checks below.

## Task 11.3 — packaging evidence (2026-09-25)

- [x] Built `dist/Plan My Cabinet.app` on macOS 26.5 (25F71), Apple M5 arm64, integrated 8-core Metal 4 GPU. `file`/`lipo` report thin arm64 Mach-O; `plutil -lint` and `codesign --verify --deep --strict` pass for the ad-hoc sealed bundle; `spctl --assess` rejects it. Inspected `Contents/MacOS/plan-my-cabinet`, `Contents/Info.plist` and five files in `Contents/Resources/Licenses/` (Noto OFL, Rust inventory, catalog provenance and field review, runtime). Launched the bundled executable with a temporary empty `HOME` in this host's desktop session: it stayed running after 4 seconds without stderr and was terminated by SIGTERM for the smoke check. This does not verify normal window close, full UI operation or a separate clean machine.
- [x] On this macOS host, `cargo fmt --check`, `cargo check --locked --offline --all-targets`, `cargo clippy --locked --offline --all-targets -- -D warnings`, and `cargo test --locked --offline` passed (178 library, 58 desktop, seven integration suites). Packager Python syntax check passed.
- [x] Reassembling the macOS bundle from the same cached release binary produced identical SHA-256 hashes for all eight files, including the ad-hoc signature. This is same-host packaging repeatability, not cross-machine binary reproducibility.
- [x] An earlier Linux x86_64 build used a network-disabled `rust:1.95` container under macOS arm64 emulation. Its archive SHA-256 was `4386180f3a5d5cea8931337162430bfed1681520e637ec356ca1282c714abfcf`; it included only five license/provenance/runtime files and predates the dependency-text update below. Its `ldd` resolved `libgcc_s`, `libm`, `libc` and the ELF loader. This historical checksum and file count do **not** describe the current archive. Vulkan/windowing/portal libraries may load dynamically and require target-machine checks; container inspection is **not** Linux desktop startup or clean-machine validation.
- **Waived / not performed:** separate clean-machine macOS arm64 startup, native file operations and window close. Linux X11/Wayland clean-machine checks are outside this release scope. The build-host process-start check is not a substitute for either.
- The package's Noto Sans Regular and Fluent resources are embedded. The reviewed FGVTN kit/plate facts are Rust constants pinned into project snapshots; manufacturer PDF/artwork is excluded. Full Noto SIL OFL and a resolved Rust dependency license inventory accompany each artifact. Inventory entries are license expressions, not the full license text of each Rust crate.
- First-release runtime target: Metal-capable macOS arm64 desktop. Only a macOS 26.5/M5 build-host process start has been observed for this package; no minimum OS, GPU model or driver release is certified. The macOS bundle is sealed with an ad-hoc signature, not a Developer ID signature or notarization (Gatekeeper rejects it). The historical Linux tarball is experimental, unsigned and not a supported release artifact.

### Dependency notice packaging update (2026-09-25)

- The packager now places source-shipped root `LICENSE`/`LICENCE`/`COPYING`/`NOTICE`/`COPYRIGHT`/`UNLICENSE` files (including named variants) and any Cargo `license_file` under `licenses/dependencies/<crate>-<version>/` relative to the artifact's license directory. The macOS location is `Contents/Resources/Licenses/`; Linux uses `share/doc/plan-my-cabinet/`. Original filenames and bytes are retained. `rust-dependencies.txt` maps each normal/build dependency to the actual copied paths, its upstream license declaration and source. The Noto OFL remains `NotoSans-OFL.txt`; manufacturer PDF/artwork is not packaged.
- An upstream crate can declare an SPDX expression without shipping a standalone license file in its published crate. Such rows are explicitly marked `METADATA-ONLY (no source-shipped text)` in the inventory; **the expression is not a substitute for the full text**, and the package does not claim those texts are present. For full terms, consult that crate's upstream source. The packager fails on missing license declarations, empty or unsafe license files, unsafe destination components and conflicting texts at the same output path. It does not silently fetch network texts or synthesize a purported notice.
- On this host's offline cache, the macOS arm64 normal/build closure resolves 294 dependencies: 479 copied files and 43 metadata-only exceptions. Two staged bundles assembled from the same cached release binary contain the same 487 file paths and SHA-256 digests (including the ad-hoc signature); `codesign --verify --deep --strict` passes. Linux package evidence for the current source and copied dependency texts is recorded below; the earlier five-file archive SHA above applies only to the historical artifact.
- Validation for this update: Python compile and two focused unit tests (source bytes, repeatability, exceptions, traversal and symlink rejection), `cargo fmt --check`, `cargo check --locked --offline --all-targets` and `cargo test --locked --offline` passed.

### Refreshed Linux x86_64 archive (2026-09-25)

- [x] Rebuilt `dist/plan-my-cabinet-0.1.0-linux-x86_64.tar.gz` from the current checkout, including `src/project_ui.rs`, with `python3 scripts/package-release.py linux-x86_64` in the cached `rust:1.95` image forced to `linux/amd64` under macOS arm64 emulation. The container used `--network none`, an offline/locked Cargo build, the host's prefetched Cargo registry and a separate Linux target directory. Rust 1.95.0 finished the release build at 2026-09-25 21:35:16 -0300; the archive was written at 21:35:20 -0300. The source-tree hashes captured during the build still matched afterward; `src/project_ui.rs` SHA-256 was `df8e04604c9c010985d45a3d76dd0eb40d3a8c55f29995000d3107c004371926`, `src/main.rs` was `4e3e70b7df2b774846edf970baa599ef4eb987311267b57d4d1114e7a1b72f6b`, and `Cargo.lock` was `876bc49b0a87f60de5177a9d5f9b53af6eb20e742e3d63405654b5eeb5a200b0`.
- [x] Archive SHA-256: `7cc229c4279c2d6698bb6acda4804c8c79c7b1273da42b82a1d4da8abf96b69b`. The tar has 1,056 entries (680 regular files): `bin/plan-my-cabinet`, README, XDG launcher, AppStream metadata, Noto OFL, catalog provenance/review, runtime instructions, Rust inventory and **671 copied dependency license/notice files** under `share/doc/plan-my-cabinet/dependencies/`. The inventory covers 393 normal/build dependencies, with 25 marked `METADATA-ONLY`; those entries do not contain full upstream license text. The older five-file count and `4386180f…` SHA above do not apply to this archive.
- [x] `file` reports ELF 64-bit LSB x86-64 PIE, dynamically linked with `/lib64/ld-linux-x86-64.so.2`; the ELF header has class 2, little-endian and machine 62. The executable extracted from the archive has SHA-256 `c7029f42fe4f0ddc7000086bef614be2ab562591a262d3e0437bdb49ccfa16cd`, identical to the just-built release binary. Container `ldd` resolves `libgcc_s`, `libm`, `libc` and the loader. Python packager syntax and three focused unit tests, plus `cargo fmt --check`, passed. These are build, archive and dependency inspections under emulation, **not** a Linux GUI runtime or clean-machine test.

## Revised macOS acceptance and waived platform checks (tasks 11.2–11.4)

### macOS arm64 current-source follow-up (2026-09-25)

- Linux release support was **removed from this change by the user**. The historical Linux archive notes above are retained as experimental build evidence, not as supported-platform claims. Revised tasks 11.2–11.4 are checked against automated macOS/build-host evidence; native interactive and separate clean-machine macOS validation were explicitly waived, not passed.
- Rebuilt `dist/Plan My Cabinet.app` via `python3 scripts/package-release.py macos-arm64` with Cargo `--locked --offline --release --target aarch64-apple-darwin` from the current tree. Host: MacBook Air Mac17,3, Apple M5 (10 CPU cores), 16 GB RAM; macOS 26.5 build 25F71, Darwin 25.5.0. Built-in Apple M5 GPU has 8 cores, Metal 4 support; built-in Liquid Retina 2560 × 1664 display. `system_profiler` reports no separate GPU driver version or external display; this is the build host, not a clean-machine certification.
- `file` and `lipo -archs` identify the staged executable as thin arm64 Mach-O; `plutil -lint` accepts `Info.plist` with `CFBundleExecutable=plan-my-cabinet`, `CFBundleIdentifier=org.planmycabinet.PlanMyCabinet`, `CFBundleShortVersionString=CFBundleVersion=0.1.0`, `CFBundlePackageType=APPL`. `codesign --verify --deep --strict` passes for the resource-sealed ad-hoc signature (no TeamIdentifier); `spctl --assess --type execute` rejects it. This is unsigned by Developer ID and unnotarized.
- `Contents/Resources/Licenses/` contains the Noto OFL, Rust inventory, catalog provenance, hinge review and runtime notice, plus 479 copied crate license/notice files (484 regular files total); 43 crate inventory rows explicitly say `METADATA-ONLY (no source-shipped text)`. These rows are not full license texts and require upstream consultation for full terms.
- macOS-gated headless desktop tests exercise New/Save As collision and explicit replacement/Open/dirty close, OS close-request cancellation at the egui viewport boundary, and failed background PDF write preserving an existing output and receipt state. Existing `src/main.rs` and `src/project_ui.rs` tests also exercise invalid Open, dirty New cancel/discard, recovery defer/recover and stale picker results; `src/optimization_worker.rs`/`src/optimization_ui.rs` cover stale/cancelled optimization and `src/export.rs` covers pre-commit PDF failure. These are in-process checks; they do not assert a visible window, a successful AppKit file dialog or user-driven close behavior.
- Final rebuilt executable SHA-256: `a5fba32a3a4a26b6044f1b21a5232238bee69bd070799349ea7017ae20c1fad2`. With an empty temporary `HOME` and the build host's existing desktop session, the staged executable remained alive after four seconds with zero stdout/stderr bytes, then exited on an intentional SIGTERM; this is process-start evidence only. After the changes, `cargo fmt --check`, `cargo check --locked --offline --all-targets`, `cargo clippy --locked --offline --all-targets -- -D warnings`, `cargo test --locked --offline` (178 library, 61 desktop, 10 integration, zero doc tests) and the three packager Python unit tests passed.
- **Waived but unverified:** interactive macOS arm64 clean-machine launch without a toolchain/cache or network, Gatekeeper handling, actual Metal viewport/overlays/font rendering and display scaling, native Open/Save As/PDF dialogs, user-driven recovery/dirty close/stale optimizer/failed export and normal window close. Prior `.pmcab` and PDF byte preservation is tested headlessly, not through a native GUI. Cross-platform GUI transfer is outside this change's scope.

- [x] Desktop New/Open/Save/Save As, dirty-close prompting and local recovery choices are now connected in `src/project_ui.rs`; headless egui tests check cancellation, failure, overwrite protection, stale picker results and recovery. The same-host packaged macOS executable remained alive for four seconds with an empty temporary `HOME`, with no stdout/stderr, then was terminated deliberately. This is **not** a visual check of a window or a successful native file picker operation.
- [x] On the macOS build host, `cargo fmt --check`, `cargo check --offline --locked --all-targets`, `cargo clippy --offline --locked --all-targets -- -D warnings`, `cargo test --offline --locked`, the three package-script tests, and `openspec validate plan-woodworking-desktop-app --strict` pass (178 library, 61 desktop and seven integration test binaries). These results and all waived validation limitations are recorded here. Completion under revised criteria is not independent verification, shop approval or production certification; `/opsx-verify` remains an optional independent review before archiving.
