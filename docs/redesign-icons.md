# Redesign icons

## Integration

`src/icons.rs` provides the complete 40-icon original handoff set:

- `Icon`, a copyable typed enum; `Icon::ALL: [Icon; 40]` follows README order.
- `Icon::name()` is the stable handoff filename stem, for diagnostics/gallery labels.
- `Icon::source()` returns embedded `egui::ImageSource<'static>` SVG bytes.
- `install_loaders(&egui::Context)` installs the compatible SVG image loader. Call
  during application creation before displaying icons. Repeated calls are safe.
- `icon(Icon, Color32, f32) -> egui::Image<'static>` uses a square logical-point
  size and the supplied tint/opacity. Reference sizes are 11–19 points.

The application declares this module and calls `install_loaders` during startup.
Example at a call site:

```rust,ignore
use crate::icons::{self, Icon};

// During creation:
icons::install_loaders(ctx);

// A real button retains egui keyboard, focus, and accessible button semantics.
// The label must be the localized action, not the enum's asset identifier.
let image = icons::icon(Icon::Save, text_3, 15.0).alt_text(localized_save_label);
let response = ui.add(egui::Button::image(image));
```

The parent shared button helper owns action names, disabled availability, focus,
selection backgrounds and hover tooltips. Use `Button::image_and_text` for visible
labels. Decorative icons can use `ui.add(icons::icon(...))` directly. Do not use
an image's click sense as a replacement for accessible button behavior.

For an exact 40%-opacity tint, use
`Color32::from_rgba_unmultiplied(r, g, b, 102)`. Coordinate this with the shared
disabled painter so opacity is not applied twice. The raster proof tests the
explicit image tint; native button disabling/focus is part of the parent gallery.

## Assets and provenance

The originals are the 40 SVGs in
`design_handoff_egui_redesign/assets/icons/`, described by the handoff README as
original icons created for this redesign. Their embedded C2PA metadata and file
bytes remain unchanged. No separate third-party icon pack or license is introduced.

The product's derived files are `assets/icons/<same-name>.svg`. Derivation:

1. Remove the non-rendering `<metadata>…</metadata>` credential envelope and its
   `xmlns:c2pa` namespace from the **derived copy**. A source signature is not
   represented as authenticating a recolored derivative.
2. Replace the root `stroke="#000"` with `stroke="#fff"`.
3. Preserve all other drawing markup, viewBox, transparent fill, 1.7 stroke width,
   round caps/joins, path commands, and coordinates exactly; add a final newline.

`geometry_matches_original_handoff` compares the entire derived SVG against that
precise normalization of each source and checks exact filename-set coverage. This
is stronger than counting paths or merely checking that SVG parsing succeeds.
The intentional duplicate geometry of `Cube` and `Assembly` is retained.

White-alpha masks are necessary because `egui::Image::tint` multiplies channels:
black strokes remain black when multiplied by amber. Embedded white strokes
produce the requested color while preserving antialiased alpha. egui caches
decoded masks by asset URI and raster size; changing tint does not generate new
colored SVGs/textures. The loader evicts unused size variants per its normal cache
policy.

`egui_extras` is pinned to `=0.36.2`, matching existing `eframe =0.36.2`, with
default features off and only `svg` enabled. This adds `egui_extras`, `enum-map`,
and `enum-map-derive` to the lockfile; it does not upgrade eframe. All icon bytes
are compiled into the application with `include_image!`. No runtime download,
filesystem asset lookup, system font scan, webview, or handoff HTML is involved.

## Repeatable verification

```sh
cargo test --offline --locked --test redesign_icons
cargo test --offline --locked --test redesign_icons -- --ignored --nocapture
```

The normal target verifies source-preserving geometry, exact 40-name coverage,
embedded synchronous byte/image/texture loading, square logical sizing, physical
raster sizes, visible stroke coverage, transparent backgrounds, and white masks
for all 40 icons at all integer reference sizes 11–19 at both 1× and 2× density.
egui's `SizedTexture.size` preserves the SVG's 24×24 source size; decoded raster
dimensions reflect the requested physical size independently.

The explicitly invoked GPU test requires a native adapter and fails if one is
unavailable. It uses the production egui-wgpu renderer, texture upload, shader,
blending and GPU readback, without a window or network access. It checks every
pixel for every icon at 11–19 points in:

| State | Tint |
| --- | --- |
| White control | `#FFFFFF` |
| Normal | `#5A5248` |
| Selected | `#C9731F` |
| Warning | `#B7791F` |
| Disabled | `#5A5248` at alpha 102/255 |

Colors are compared with the independently rendered white-mask control using the
renderer's premultiplied channel multiplication, with a 2-byte rounding tolerance.
Every icon/size must contain visible nonblack ink. Transparent pixels and alpha
are checked too. Dithering is disabled to make comparisons deterministic.

To retain a contact sheet, set `REDESIGN_ICON_RASTER_PPM` to a **new** output file
when running the ignored test. It writes a 1152×960 binary PPM composited on
`#F4F1EC`. Rows follow `Icon::ALL`; the five state blocks follow the table above,
each with sizes 11 through 19 from left to right. The file is created only after
all raster assertions pass and never overwrites an existing file.

Verified offline on 2026-09-26: both ordinary tests passed; GPU test passed on
Apple M5, Metal. Initial contact sheet:
`/private/var/folders/sz/nnw19cc96ts9hmcg2m0l8j_w0000gn/T/opencode/redesign-icons-raster.ppm`.
This is actual shader/readback evidence. A separate native widget-gallery
capture with installed fonts and accessible buttons was reviewed on macOS
arm64/Metal; redesigned workspace comparison remains pending.
