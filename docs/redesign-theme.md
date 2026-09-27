# Native theme and widget conventions

The approved palette, sizing and type hierarchy are specified in the
[handoff README](../design_handoff_egui_redesign/README.md) and implemented by
`src/theme_widgets.rs` and [typography](redesign-typography.md). The design
authority is Main Window **2a**, not 1a/1b/1c. `theme_widgets::apply_visuals`
installs the light palette and minimum widget spacing during desktop startup.
The native product uses bundled fonts/icons, not the HTML reference runtime.

Presentation widgets do not mutate a project. `section_header` uppercases the
localized label and applies 0.08 em tracking at 11 points; `card`, `chip`,
`segmented`, `primary_button`, `secondary_button`, `unit_field` and
`icon_button` use native egui responses and the handoff radius/color tokens.
`unit_field` edits text only; parsing, exact original values, rounding consent,
commit and rollback belong to a shared draft controller (task 4.3). Changing
a segment changes the caller's supplied view/draft value only. Callers must use
the returned response for domain actions and supply localized icon names,
including on disabled buttons. Unlabelled icon-only actions are not permitted.

The opt-in gallery renders all tokens and 40 [offline icons](redesign-icons.md)
at a real native size. Choose a fresh destination each run:

```sh
cargo run --locked -- --capture-gallery "$TMPDIR/pmcab-gallery-en"
cargo run --locked -- --capture-gallery "$TMPDIR/pmcab-gallery-pt" \
  --capture-language pt-BR --capture-scale 130 --capture-size 1100x700
sips -s format png "$TMPDIR/pmcab-gallery-en/capture.ppm" \
  --out "$TMPDIR/pmcab-gallery-en/capture.png"
cargo test --locked --bin plan-my-cabinet theme::tests
cargo test --locked --bin plan-my-cabinet theme_widgets::tests
cargo test --locked --bin plan-my-cabinet widget_gallery::tests
cargo test --locked --test redesign_icons -- --ignored
python3 -m unittest discover -s scripts -p test_package_release.py
```

The native image's `manifest.json` labels its surface `widget-gallery` and
`redesign_acceptance: false`. A passing gallery is **not** a match to an
unimplemented workspace or dialog. Its icon button and field cases are
reference presentation specimens, not wired project-edit actions. The gallery
tests verify palette tokens and accessible icon names; subsequent work must
connect the same widgets to the actual transaction and navigation controls.
Compare section labels, fill/stroke, button/chip radius, focus ring and font
hierarchy against 2a and the modal references during product integration.
Capture English and Portuguese at supported scales; record clipping and
renderer-specific deviations rather than assuming a screenshot pass.

### Foundation evidence, 2026-09-26

- Native macOS arm64/Metal gallery at 1440 × 900 logical points, 100%:
  `pmcab-gallery-final-en/capture.png` in the approved session temporary root.
  Reviewed section tracking/alignment, Noto weight differences, mono numbers,
  warm palette, button/segmented/chip/field presentation and all 40 icon shapes.
  The specimen card header was initially centered due to the column layout;
  correcting its explicit left layout produced the reviewed final capture.
- Secondary gallery at 1100 × 700 logical points, 130% and pt-BR capture
  configuration: `pmcab-gallery-pt-130/capture.png` in the same root. This gallery
  currently uses fixed English specimen labels (with Portuguese accented
  typography samples); it is **not** evidence of product pt-BR localization.
  Gallery content scrolls when it exceeds the available logical height.
- Headless tests verify exact palette values, repeated initialization, offline
  glyph/style availability, accessible icon-only names, disabled state and
  keyboard Enter activation after focus. The separate Metal raster test checks
  40 icons across sizes and normal/selected/warning/disabled tint states.
- The macOS arm64 release bundle assembled with `package-release.py` using
  `--locked --offline`; its license directory includes both OFL notices, font
  inventory and provenance. The unbundled release executable completed a
  native 1100 × 700 gallery capture without network configuration. An attempt
  to capture from the ad-hoc signed `.app` executable timed out waiting for
  the macOS screenshot event; packaged **visual** launch acceptance remains
  open and must be retested in an interactive desktop session. Do not treat
  packaging success as that screenshot proof.
- Remaining: focused field ring in each product surface, actual workspace
  controls, all modal migrations and per-screen 2-point geometry comparisons.
  These are not waived by the gallery.

The common [modal controller](redesign-modal.md) now wraps `egui::Modal` with
isolated scene input, a scrollable body beneath persistent title/footer,
invoking-focus restoration and popup-aware Enter/Escape. Grid/kerf editing
uses it first. Remaining dialogs retain their input guards until they migrate
under tasks 7.1–7.5; this gallery is not proof of those modal captures.
