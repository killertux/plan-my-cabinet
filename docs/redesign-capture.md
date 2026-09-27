# Isolated native redesign capture

The desktop's opt-in capture mode renders an in-memory
[deterministic reference fixture](redesign-reference-fixture.md), isolated from
personal projects. The original baseline tooling in task 1.2 is not itself a
pass for the later Design, Stock, Cut plan, Hardware, Handoff, Welcome or
Settings screens. Current screen selectors are listed by `--help`; native
visual acceptance and pointer/keyboard workflows remain due in tasks 6.8,
8.6, 9.7, 10.5, 11.8, 12.6, 14.5 and 15.2–15.3.

## Run on macOS arm64 with a native graphics session

Choose a **new directory** under an existing writable temporary parent:

```sh
cargo run --locked -- --capture-baseline "$TMPDIR/pmcab-baseline-a"
cargo run --locked -- --capture-baseline "$TMPDIR/pmcab-baseline-b"
shasum -a 256 "$TMPDIR/pmcab-baseline-a/capture.ppm" "$TMPDIR/pmcab-baseline-b/capture.ppm"
```

Arguments after `--` are application arguments. Supported options:

- `--capture-size 1440x900`: logical content size, default 1440 x 900. Bounds are
  640–3840 by 480–2160 points. This changes window size, not workspace state.
- `--capture-scale 90|100|115|130`: interface zoom, default 100. Native window
  dimensions account for zoom so the recorded content size stays explicit.
- `--capture-language en|pt-BR`: UI language, default en.

For example, capture Settings at its reference size:

```sh
cargo run --locked -- --capture-baseline "$TMPDIR/pmcab-settings-small" \
  --capture-settings general --capture-size 780x560 \
  --capture-scale 130 --capture-language pt-BR
```

Use `--capture-workspace design|stock|cut-plan|hardware|handoff` for project
surfaces, `--capture-gallery` for the widget gallery, and
`--capture-settings cutting|grid|costs|general|shortcuts|about` for Settings.
`--capture-welcome empty` selects an isolated first-run Welcome at 1100 × 700
when combined with `--capture-size 1100x700`; it does not load the reference
project into the Welcome state or read the user's recents. It cannot be combined
with another screen selector. Populated, missing-file and recovery Welcome
states still need dedicated isolated selectors and native captures before
task 12.6 or the Welcome part of 15.2 can pass. The Handoff fixture pre-reviews a packet **only for static
capture**, not as evidence of user approval. It writes `reviewed-preview.pdf`
from the same frozen document shown in the preview for development comparison;
that write neither opens a save picker nor creates an export receipt. A static
capture alone cannot prove pointer/keyboard reachability or file-picker safety.

The process requests a native GPU screenshot after 12 warmup frames, writes
`capture.ppm` and `manifest.json`, and exits. The manifest records requested
logical and actual raster dimensions, pixels per point, zoom, locale, fixed
camera, stable selection/visibility IDs, a hash of the **effective in-memory
editor fixture after compatibility/alias normalization**, platform and app
version. `renderer-diagnostic.json` records the actual wgpu target format and
whether pinned egui-wgpu supports screenshot readback from it. The hash can
change when a portable schema evolves without changing
fixture geometry; compare the recorded manifest, not an old hardcoded digest.
The native Metal screenshot request is retried at bounded frame intervals if
the first callback is lost during initial surface setup. A capture succeeds
only when an actual screenshot event arrives and its logical dimensions match;
a timeout does not create an approval artifact.
It labels the surface `current-application-baseline` and sets
`redesign_acceptance: false`. A missing screenshot or incorrectly sized native
window causes a failed process rather than a false success. Close-before-capture
also fails. The frame limit is not a wall-clock watchdog; use a test-runner timeout
when automating on an unavailable or hung graphics session.

PPM is a lossless RGB output requiring no runtime encoder dependency. Convert it
to PNG for viewing/comparisons with the macOS tool:

```sh
sips -s format png "$TMPDIR/pmcab-baseline-a/capture.ppm" \
  --out "$TMPDIR/pmcab-baseline-a/capture.png"
```

For a development-only frozen-page review, capture Handoff separately with
`--capture-page 1` through the actual page count. Every Handoff capture writes
the exact frozen `reviewed-preview.pdf` alongside its PPM; rasterize the PDF
**outside the app**, for example with `pdftoppm -r 96 -png reviewed-preview.pdf
page`, then compare each PDF page with the native page region at the same
physical scale. The product neither shells out to `pdftoppm` nor uses a PDF
viewer for preview, and this diagnostic PDF is not a recorded export. The
six-page foreground comparison and its layout/antialiasing caveats are in
[redesign-acceptance.md](redesign-acceptance.md).

## Isolation and determinism

- The output root must not exist; neither existing directories nor symlinks are
  reused. Capture files are opened with create-new semantics.
- The project is constructed in memory, never loaded from a user path. Capture
  session app data points inside the fresh output root and no recovery store is
  attached to a saved file.
- Keyboard, pointer, accessibility and dropped-file input is filtered before
  egui processes it, so accidental input cannot invoke Save/Open/Export. Widgets
  remain visually enabled; capture mode is not an interactive editing session.
- Animation time, orthographic camera, shelf selection and hidden doors are
  fixed. Font/layout/rendering changes legitimately change the image hash.
- No claim of identical pixels across different OS/font/GPU versions is made.
  Compare repeat captures on the same host/source/fixture configuration and
  record differences rather than adopting a changed baseline silently.
- The renderer bounds offscreen texture allocation to the device limit and
  visible canvas. Small/translated legacy UI may still clip controls; surviving
  capture is not adaptive-layout acceptance for the future redesign.

## Screenshot delivery troubleshooting

Run the **same unbundled binary** with fresh output roots from a foreground
logged-in macOS graphics session, then compare with the failing launch
context. Earlier capture attempts in a non-interactive context requested the
root screenshot at frames 12, 72, 132, 192 and 252 but received no event;
both a capture-only Metal device poll and a temporary 1200-frame retry still
timed out. They produced neither PPM nor manifest and were not visual passes.
The recorded root window was unfocused, but that observation alone does not
establish why readback failed. If the interactive retry fails, instrument the
native render path to distinguish a hidden/unavailable surface, capture
requests not reaching paint, an unsupported surface format, GPU map failure
and a completed image not reaching the input queue. Do not approve a blank,
headless or resized substitute or merely extend the frame limit. A later
foreground capture succeeded with `Bgra8Unorm` and yielded all five workspace,
empty Welcome and three Settings PPMs; see the 2026-09-27 foreground review at
the top of [redesign-acceptance.md](redesign-acceptance.md). This does not
explain every earlier timeout, and the visual deviations remain open.

## Verification recorded on 2026-09-26

- `cargo test --locked --test redesign_reference`: 4 passed.
- `cargo test --locked --bin plan-my-cabinet capture::tests`: 3 passed, covering
  CLI validation, file overwrite refusal, exact screenshot bytes and input isolation.
- Two actual macOS/Metal English 1440 x 900 native captures at 100%, recorded
  at 2880 x 1800 pixels / 2 pixels per point, had identical PPM SHA-256
  `caf9172f4f058ba6e534a6bd12c7c217fb5f6e19cad2a6367e4a18fe8f0147b7`
  before the visible-canvas allocation guard was added. This is a tooling baseline,
  not a redesign visual approval. Temporary evidence directories were
  `pmcab-redesign-baseline-1440-a` and `pmcab-redesign-baseline-1440-b` under the
  session's approved temporary root; these captures are not checked into git.
- A real 780 x 560 / pt-BR / 130% run exposed an oversized texture request in the
  existing viewport. After bounding the visible canvas and device allocation,
  the native capture succeeded. A regression test covers texture-size bounds.
- Repeated 1440 x 900 captures after the allocation guard and expanded manifest
  (`pmcab-redesign-baseline-final-a` / `-final-b`) also matched byte-for-byte:
  PPM SHA-256 `40af3182fd84f5fe2da97c31beb8fecb04330dbcb6e7c72322197986b8ea5f78`.
- `cargo test --locked` after the guard: 178 library, 65 desktop and 14 integration
  tests passed. Native reference-screen comparisons remain pending.

Follow the complete [acceptance matrix](redesign-acceptance.md) for redesign
screens and record the tested git revision (including dirty state) with final
acceptance evidence; the baseline manifest alone does not identify source changes.
