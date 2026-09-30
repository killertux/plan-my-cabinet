# Design

## Model

- `CatalogReference.item: Option<CatalogItem>` holds `Slide(SlideSpec)` or `Foot(FootSpec)`; hinges keep `verified_hinge`. One pinned record per purchasable code (one slide length). Trust is derived as for hinges, plus `Generic` for records equal to a bundled generic pack.
- `SlideInstallation { catalog_id, drawer_root_id, drawer_sides[2], cabinet_sides[2], sides[2], height, setback }`. The pull direction is derived from the carcass side's fitted front edge, so no closed pose is stored; misplacement shows up as issues.
- Feet are `Hardware` with `HardwareKind::Catalog`; `Project::hardware_dimensions` gives placeholders and feet one box for bounds, picking, measuring and checks. A foot's box starts at its minimum corner with Z up; the mounting face is on top.

## Slides

- Detection: upright parallel pairs of boards in the drawer whose outer faces each have a parallel carcass board within 60 mm; the pair with the smallest total gap wins. Measure: gaps along the normal, carcass depth from its front edge, box length, how far the box front sits behind the carcass front.
- Fit: longest length with setback + L ≤ carcass depth and start + (L − 10 mm) ≤ box length (the drawer member is shorter than the nominal length). Height: centred on the box side.
- References: board-local µm points on the fitted faces, and distances from each board's front edge for the shop.
- Motion: every member of the drawer translates along the pull direction; slide member boxes follow the drawer pose (inner by e, intermediate by e/2).

## Packs and user models

- Flat raw TOML structs with `deny_unknown_fields`; shapes validated by hand so issues carry paths. `generic` packs need no sources.
- `user_pack::save_models` upserts into `<catalog dir>/user-models.toml` (a `draft` pack), refusing to overwrite an invalid file and re-validating before writing.

## Rendering

- `render::hardware_mesh` builds triangles with normal-based shading and rim-only edges; the CPU raster and the GPU viewport draw them with no shader change.
