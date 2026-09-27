# Native redesign typography

## Integration and usage

`src/theme.rs` provides `install_fonts(&eframe::egui::Context)` for egui
0.36.2. The desktop now calls this during application creation, before the
first frame, in place of the previous regular-only font setup.
This module embeds all six static TTF faces with `include_bytes!`; neither font
loading nor packaging requires a runtime network request or system-installed font.

Use `egui::RichText::new(text).font(theme::TITLE.font_id())`, or the registered
`TextStyle::Name("Title".into())`. The token's public `size` and `typeface` also
support painter/LayoutJob use. Use `Typeface::family()` to select a genuine weight
at another reference-specific size. `RichText::strong()` changes color, not font
weight; use an explicit medium/semibold token instead.

| Token / named style | Points | Face | Intended use |
| --- | ---: | --- | --- |
| `HEADING` / Heading | 20 | Noto Sans 600 | Stock page title |
| `PAGE_TITLE` / PageTitle | 17 | Noto Sans 600 | Settings / Welcome title |
| `TITLE` / Title | 15 | Noto Sans 600 | Inspector / dialog title |
| `BODY` / Body | 13 | Noto Sans 400 | Default copy |
| `BODY_MEDIUM`, `BUTTON` / BodyMedium, Button | 13 | Noto Sans 500 | Selected rows / controls |
| `PRIMARY_BUTTON` / PrimaryButton | 13 | Noto Sans 600 | Primary actions |
| `PART_LABEL` / PartLabel | 12.5 | Noto Sans 500 | Sheet part names |
| `SMALL` / Small | 12 | Noto Sans 400 | Hints |
| `STATUS` / Status | 11.5 | Noto Sans 400 | Status / footer |
| `CAPTION` / Caption | 11 | Noto Sans 400 | Chips / secondary notes |
| `SECTION`, `SECTION_COMPACT` / Section, SectionCompact | 11 / 10.5 | Noto Sans 600 | Uppercase section labels |
| `MONO` / Monospace | 13 | JetBrains Mono 400 | Values / dimensions |
| `MONO_SMALL` / MonoSmall | 12 | JetBrains Mono 400 | Compact dimensions |
| `MONO_MEDIUM` / MonoMedium | 12 | JetBrains Mono 500 | Emphasized IDs / values |
| `MONO_ID`, `SHORTCUT` / MonoId, Shortcut | 11 | JetBrains Mono 400 | IDs / shortcut keycaps |
| `CUT_MARKER` / CutMarker | 10 | JetBrains Mono 600 | Cut sequence markers |
| `RAIL`, `RAIL_ACTIVE` / Rail, RailActive | 9.5 | Noto Sans 400 / 500 | Rail labels |

Heading, Body, Button, Small and Monospace are egui built-in text styles. All
other names in the table are registered custom styles. Sizes are logical points;
interface zoom handles 90/100/115/130%, without changing these constants. For
uppercase section text, set `TextFormat.extra_letter_spacing` to
`token.size * SECTION_TRACKING_EM` (0.08 em). Uppercase the localized display
label, never user-authored names.

`FontFamily::Proportional` starts with Noto Sans Regular; `Monospace` starts with
JetBrains Mono Regular. Named families `noto-medium`, `noto-semibold`,
`jetbrains-medium`, and `jetbrains-semibold` start with their own 500/600-weight
faces. Noto families fall back first to the corresponding JetBrains Mono weight
for missing arrows, minus and macOS modifier symbols. Each family then preserves
egui's embedded default fallback chain. Missing symbols can therefore use a
fallback's metrics (or weight for the general egui fallbacks); primary Latin text
and numbers use the selected actual face.

## Upstream provenance and licenses

Downloaded from the official upstream repositories on 2026-09-26. These are
unmodified static upstream binaries, not renamed regular faces or locally
generated variable-font instances. The existing Noto Sans Regular was retained
and verified byte-for-byte against the same pinned upstream file.

- **Noto Sans:** official archived distribution
  [`notofonts/noto-fonts`](https://github.com/notofonts/noto-fonts/tree/ffebf8c1ee449e544955a7e813c54f9b73848eac),
  commit `ffebf8c1ee449e544955a7e813c54f9b73848eac`.
  Binary paths: `hinted/ttf/NotoSans/NotoSans-{Regular,Medium,SemiBold}.ttf`.
  Upstream `LICENSE` is the existing `assets/fonts/OFL.txt` (Copyright 2018 The
  Noto Project Authors). This archived release was chosen to match the existing
  regular face, rather than mixing generations of Noto metrics.
- **JetBrains Mono:** official
  [`JetBrains/JetBrainsMono` v2.304](https://github.com/JetBrains/JetBrainsMono/tree/cd5227bd1f61dff3bbd6c814ceaf7ffd95e947d9),
  commit `cd5227bd1f61dff3bbd6c814ceaf7ffd95e947d9`.
  Binary paths: `fonts/ttf/JetBrainsMono-{Regular,Medium,SemiBold}.ttf`.
  Upstream `OFL.txt` is `assets/fonts/JetBrainsMono-OFL.txt` (Copyright 2020 The
  JetBrains Mono Project Authors).

For a pinned raw download, use
`https://raw.githubusercontent.com/<repository>/<commit>/<upstream-path>`.
Both families are SIL OFL 1.1. Redistribution includes their copyright notices
and full license texts. The release script ships `NotoSans-OFL.txt`,
`JetBrainsMono-OFL.txt`, this provenance document and a generated `fonts.txt`
inventory in both platform license directories. Font binaries are embedded in
the executable after application integration.

### SHA-256 verification

| File in `assets/fonts/` | SHA-256 |
| --- | --- |
| NotoSans-Regular.ttf | `b85c38ecea8a7cfb39c24e395a4007474fa5a4fc864f6ee33309eb4948d232d5` |
| NotoSans-Medium.ttf | `7bbe267354704c6ad18bde24b1dbc756c8e4380ca1c3f3c25c45ec5c4471510b` |
| NotoSans-SemiBold.ttf | `87a8b90ece1e89746b544e4e086f85a3710e41485a8078f9be874837dfad45d5` |
| JetBrainsMono-Regular.ttf | `a0bf60ef0f83c5ed4d7a75d45838548b1f6873372dfac88f71804491898d138f` |
| JetBrainsMono-Medium.ttf | `31c92d01a8a08528b718a43addf0ad3df0af2ca4b7b3290a452f70f358e14d3d` |
| JetBrainsMono-SemiBold.ttf | `1b3bfa1ed5665a4ce3f9feb68d2d4e40e70bf8b4b7d9a3edd418f321b4e166a0` |
| OFL.txt | `0dab92d0544f7b233403f14b84a663bdbfa746982eda629e7f4f9ffe1b036feb` |
| JetBrainsMono-OFL.txt | `30f0c136e3c88e422d0791acd97238870f9054a9729bc34cf2ff0d4ed8cac4ad` |

## Verification and pending native checks

Module unit tests check actual OS/2 weights 400/500/600, static/nonduplicate
faces, pt-BR accents and common dimensional symbols in every primary face, monospaced
numeric advances, family precedence, registered styles and offline shortcut
glyph coverage through egui's fallback chains. After application module
integration run `cargo test --locked --bin plan-my-cabinet theme::tests`.
Release notice checks: `python3 -m unittest discover -s scripts -p test_package_release.py`.

Startup integration and a native offline gallery capture were exercised on
macOS arm64/Metal on 2026-09-26 using `--capture-gallery` at 1440 × 900. The
gallery rendered all six faces, pt-BR accents, numeric tokens and shortcut
glyphs without missing-character boxes. Unit and packaging-inventory tests
passed. **Pending:** packaged offline launch and all redesigned-screen visual
acceptance. A packaged ad-hoc signed app was built offline with the OFL files,
but its native capture attempt timed out waiting for the screenshot event;
the unbundled release binary did capture. Packaged visual verification remains
pending. The gallery is not a passing screenshot of the new workspaces.

For continuing visual acceptance:

1. Capture every token and all six faces in a native typography gallery, showing
   `Ação · Dimensões · Português (BR)`, `764 × 537 × 18`, `BRL 579,80`, `S1`,
   `C1`, and platform shortcut keys. Check that regular/medium/semibold are
   visibly distinct and that glyphs have no missing-character boxes.
2. Compare native Metal captures with Main Window 2a and all reference screens
   at 100% scale: title/body hierarchy, rail label weights, mono alignment,
   section tracking, baseline/line-height and clipping. Record rasterization
   differences explicitly; do not waive size or weight discrepancies.
3. Repeat en/pt-BR at 90/115/130%, including long labels and narrow layouts.
4. Launch the packaged app disconnected from the network on a machine without
   these fonts installed. Check the same labels/shortcuts and both bundled OFL
   texts plus the six-face inventory.
