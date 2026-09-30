# Development builds

Plan My Cabinet is a Rust 2024 desktop application. Its first-release target
is macOS arm64 with Metal, a headless library and a native egui/wgpu executable.
Use Rust 1.95 or newer. The existing Linux backend/package is experimental,
not a supported or release-validated desktop target.

```sh
cargo fmt --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --lib  # does not start a graphics device
cargo test --locked
cargo run --locked
```

On macOS, test with an arm64 computer and Metal-capable driver. Experimental
Linux builds use the Vulkan backend with eframe's X11/Wayland window integration. Install
system Vulkan drivers, an active desktop session (X11 or Wayland), and any
native shared libraries required by your distribution's windowing stack.
Build success alone does not establish that window creation or graphics work.
If no suitable graphics adapter is available, the executable reports a
startup error on stderr. The UI uses the platform's display scale factor;
verify high-DPI window resizing on actual target hardware.

Some PDF tests read the exported text back with `pdftotext` from poppler
(`brew install poppler` on macOS, `poppler-utils` on Debian/Ubuntu).

GitHub Actions runs format, all-target checks, strict Clippy, headless library
tests, and all tests on macOS and Ubuntu. Interactive GUI checks are manual.

## MCP server (`--mcp`)

`plan-my-cabinet --mcp` serves the agent API over stdio with `rmcp`. The layers:

- `src/service/`: the transport-free agent API. Each tool is a `Workspace`
  method with a `Deserialize + JsonSchema` input and a serializable result or a
  `ServiceError { code, message, hint, details }`. Library errors convert there
  with exhaustive matches, so a new error variant fails to compile until it has
  a message.
- `src/render/`: the camera, scene mesh and a CPU rasterizer shared by the
  viewport, the saved thumbnail and agent pictures; sheet diagrams are SVG
  rendered with resvg. No GPU is needed, so it runs in CI.
- `src/read_models/scene_description.rs`: contacts, overlaps and gaps in words.
- `src/app/mcp.rs`, `src/app/mcp/server.rs`: the thin rmcp adapter, the
  server instructions (`instructions.md`) and guide (`guide.md`).

Nothing on the `--mcp` path may write to stdout: it carries the protocol. Use
stderr for diagnostics.

Tests: `tests/service_cabinet.rs` drives the API end to end with JSON inputs,
`tests/render_pictures.rs` checks pictures and the scene description, and
`tests/mcp_stdio.rs` spawns the binary and speaks JSON-RPC. Set
`PMCAB_RENDER_DUMP=/some/dir` to write the test pictures there. For manual
checks: `npx @modelcontextprotocol/inspector target/debug/plan-my-cabinet --mcp`.

## Desktop redesign acceptance

See [redesign acceptance](redesign-acceptance.md) for the ten-screen reference
inventory, missing-state matrix, all nine delta capabilities and per-stage tests.
Capture setup, shell and screens are unfinished; native redesign checks remain
pending. Earlier GUI waivers do not waive this change's screenshot fidelity.

## Offline release artifacts (task 11.3)

From a checkout with the committed `Cargo.lock`, Rust 1.95+ and all native
build prerequisites installed, run on the matching architecture:

```sh
python3 scripts/package-release.py macos-arm64
python3 scripts/package-release.py linux-x86_64
```

`macos-x86_64` and `windows-x86_64` are also accepted. `--archive` additionally
writes the macOS bundle as `plan-my-cabinet-<version>-macos-<arch>.tar.gz`; the
Windows target writes a `.zip` with `plan-my-cabinet.exe` and `Licenses/`.
Pushing a `v<version>` tag runs `.github/workflows/release.yml`, which builds
all four packages (Windows with a static C runtime), and publishes them with
`SHA256SUMS` as a GitHub release for `install.sh` / `install.ps1`.

The script uses `cargo build --locked --offline --release --target` and refuses
to package a binary whose format/architecture does not match the requested
artifact. It requires dependencies already cached; cross-builds require the
Rust target **and** native target linkers/libraries. It writes the unsigned
`dist/Plan My Cabinet.app` on macOS or
`dist/plan-my-cabinet-0.1.0-linux-x86_64.tar.gz` on Linux. Pass `--out-dir`
to stage elsewhere. The Linux archive contains `bin/plan-my-cabinet`, an XDG
`.desktop` launcher and AppStream metadata under `share/`.
For desktop registration, install `bin/` into the PATH and `share/` into the
corresponding share prefix; the executable can also be run directly from the
extracted archive. The macOS executable lives in `Contents/MacOS/`.

The Noto font (including its PDF embedding), English/pt-BR translations and
reviewed factual catalog record are compiled into the executable via
`include_bytes!`, `include_str!`, and Rust code. License/attribution files
are separately visible in `Contents/Resources/Licenses/` or
`share/doc/plan-my-cabinet/`: Noto's full SIL OFL, a target-specific Rust
dependency license inventory derived from offline Cargo metadata, catalog
provenance and field-to-source review, and runtime instructions. No manufacturer
PDF/artwork is packaged.
`SOURCE_DATE_EPOCH` controls tar/gzip metadata timestamps (default 0); file
order and ownership are normalized. Reproducible packaging assumes the same
locked sources, Rust/linker toolchain and build inputs; binary bit-for-bit
reproducibility across different build machines is not established.

The macOS app is ad-hoc signed for bundle integrity, but has no Developer ID
signature or notarization, so Gatekeeper can block it. Linux needs
a Vulkan-capable GPU/driver, X11 or Wayland session, native windowing libraries
and a working XDG Desktop Portal backend for the file picker. The archive does
not bundle distribution shared libraries; use `ldd` on a target Linux host.
Neither a successful build nor archive inspection proves clean-machine GUI
startup; see `docs/release-checklist.md` for tested combinations and gaps.

## Optimization performance fixture (task 8.6)

`tests/support/performance_fixture.rs` constructs exactly 100 numbered
100 × 50 × 18 mm boards and ten individual 1050 × 60 × 18 mm owned sheets,
all with stable UUIDs, lengthwise grain and a 5 mm kerf. Ten boards fit per
sheet at 105 mm origin intervals; the tenth ends at 1045 mm, leaving the
5 mm edge band for isolation. The worker starts with no allocations and the
desktop dense-sheet check explicitly places all 100. Neither test scales down
the workload. Run:

```sh
cargo test --locked --test optimization_performance -- --nocapture
cargo test --locked --bin plan-my-cabinet large_fixture_workspace_frames_during_worker -- --nocapture
cargo test --locked --bin plan-my-cabinet dense_sheet_first_and_warm_frames -- --nocapture
```

The integration test uses the UI's actual 10,000 placement / 20,000 witness
state / beam-width 8 bounds and a five-second worker deadline. It measures
completion and cancellation from `cancel()` to completion separately. The
headless egui tests render the full workspace, including sheets, alongside
the live worker and then a densely allocated sheet. They check interactive
frame production without a graphics device; desktop pointer/scroll and GPU
latency remain manual checks. The 30-second completion and two-second
cancellation assertions are CI hang guards, **not** performance pass thresholds;
observations are printed and should be recorded per machine. The target for
cancel acknowledgement is under 250 ms on the machine being tested.

Observed 2026-09-25, macOS 26.5 (25F71), arm64 Apple M5, 10 CPU cores,
16 GiB memory, Rust debug test profile: worker completion **5.001 s** with a
5 s search deadline (1 ms scheduling/polling overhead), cancellation
**1.27 ms** (250 ms target met), five full workspace frames while searching
**258.74 ms** total, first dense allocated workspace frame **162.62 ms**,
and five unchanged dense frames **83.45 ms** total. Before coalescing progress
updates, completion was 7.36–7.59 s because thousands of queued progress
messages preceded the completion notification; this was a UI responsiveness
blocker, now addressed by reporting at 256-placement intervals. These are
observations from headless egui tests on this host, not universal timing
guarantees or a Metal desktop interaction measurement. Committed diagnostics
are cached by project ID/revision; preview diagnostics compare their input
snapshot, so unchanged UI frames do not redo bounded cut-witness searches.

## PDF page review (task 9.4)

On 2026-09-25, rendered the simple two-part and dense 28-part fixtures in
English and pt-BR to A4 raster previews using `pdftoppm` and reviewed the
cover, layout, continuation and final pages. Simple pages show distinct part
outlines and a numbered cut; dense layouts have staggered C1–C27 callouts with
leaders, a keyed part legend and paginated instructions. Scale and NOT A
CUTTING TEMPLATE appear on layout continuations; draft markings repeat on
each page. Accents in Portuguese names and headings rendered without missing
glyphs. The views are legible at normal PDF zoom; long stock and individual
part IDs wrap to avoid clipping. This is a visual review of representative
fixtures, not a guarantee for arbitrary user names or printers. Remaining
English phrases in Portuguese issue and piece-detail text are tracked for
the independent localization/output task 9.5.
An earlier macOS startup/resize/close check was reported on 2026-09-25.
The packaged build-host process-start check and automated egui tests do not
verify a visible Metal window, native dialogs or high-DPI interaction; those
checks were explicitly waived for this change. A headless Linux ARM64
container build also passed, but Linux desktop operation is not supported or
release-validated here.

The app bundles Noto Sans Regular from the Noto Project (SIL Open Font License
1.1, `assets/fonts/OFL.txt`) for English and Brazilian Portuguese interface
glyphs. Its source is
`https://github.com/googlefonts/noto-fonts/tree/main/hinted/ttf/NotoSans`.
Headless tests check that the bundled font covers all printed resource glyphs.
User-facing measurement guidance is in `docs/input-en.md` and
`docs/input-pt-BR.md`. Project portability and recovery guidance is in
`docs/project-en.md` and `docs/project-pt-BR.md`; those pages distinguish the
headless persistence tests from the desktop file controls. Native file-picker
and recovery interactions have not been manually witnessed.
Board creation and editing guidance is in `docs/boards-en.md` and
`docs/boards-pt-BR.md`, including the current left-panel keyboard walkthrough.

The headless `.pmcab` transfer fixture in `tests/portability.rs` was saved on
macOS arm64, reopened and edited in a network-disabled Linux arm64 container,
then reopened and compared on macOS arm64 (2026-09-25). Run its `write`,
`read`, and `verify` roles in that order with a shared `PMCAB_TRANSFER_DIR`;
the Linux `read` role runs with `--network none` and
`cargo test --offline --locked --test portability`. Fetch dependencies before
disconnecting the build container. The Linux x86_64 container toolchain
crashed under this host's x86 emulation before tests could start, so this
does not verify Linux x86_64 or any desktop GUI transfer. Cross-platform GUI
transfer is outside this first-release acceptance scope.

## 3D viewport and navigation smoke test (tasks 1.4, 5.1)

On a Metal-capable macOS arm64 desktop, run `cargo run --locked`. Create a
material and two boards, then duplicate one board to make overlapping geometry.
The center viewport shows their shaded rectangular solids, dark outlines,
grid, and red X, green Y, blue Z axes. At their crossing, visible front faces
and edges should hide geometry behind them. Look for flicker, missing surfaces
or grid lines drawn over front faces. Select a board checkbox and use **Frame
selection/scene**; clear selection and frame the whole project. Exercise all
four view presets, perspective/orthographic projection, primary drag orbit,
secondary drag and Shift+primary drag pan, trackpad scroll/pinch zoom, and the
keyboard controls documented in `docs/viewport-en.md`. Repeat without a
middle mouse button. Resize the window and change display scaling if available;
the picture should follow the center pane, keep its proportions, and never
cover the left panel. Open and cancel **New board** to confirm surrounding UI
remains usable; close the window normally. Record macOS version, CPU/GPU,
display scale, and any rendering or interaction failures. A passing compile or
headless test cannot replace this visual check.

For a future Linux support change, repeat this procedure on a Linux x86_64 Vulkan desktop
in both X11 and Wayland sessions. Record distribution, GPU/driver, compositor,
session type, display scaling and any differences. Compilation and headless
tests do not verify interactive navigation or Metal/Vulkan rendering.
