# Design

## Layers

- `src/service/` (library, no async): `Workspace` owns at most one open `ProjectEditor`, the catalog registry and recent optimizer results. Every tool is a method taking a `Deserialize + JsonSchema` input. Changes go through `Workspace::change`, which checks `expected_revision`, runs the edit and reports the new revision, dirty state and undo steps. Preview/commit pairs and borrow-scoped sessions (`PlacementSession`, `SheetEditSession`, material, dimension and door previews) open and close inside one call; `dry_run` reports without committing.
- `src/render/` (library): the viewport camera, selection/visibility and scene mesh (moved from the app), a deterministic CPU z-buffer rasterizer with an object buffer and depth-tested edges, labelled pictures composed with resvg, and SVG sheet diagrams.
- `src/app/mcp*` (binary): option parsing, a tokio runtime built only on this path, and the rmcp tool router. Tools run on a blocking thread behind one mutex, so calls are serialized.

## Conventions

- Numbers are millimetres; strings go through the app's length parser. Inexact conversions need `allow_rounding`, mirroring the app's rounding consent.
- References accept ids, unique names, 8+ hex id prefixes, stock aliases, and material name + thickness. Ambiguity is an error listing candidates.
- Errors are tool results with `{code, message, hint, details}`; protocol errors are reserved for undecodable input.
- The optimizer blocks for at most `max_seconds`; results are kept by id and must be applied before further manufacturing edits.

## Seeing the result

Pictures are rendered on the CPU with the viewport's colors and shading so they work headless and in CI. The object buffer places numbered callouts on each object's visible pixels and reports what is hidden from the camera. The scene description treats axis-aligned boxes exactly (contacts, gaps, overlap regions) and uses a separating-axis test for rotated ones.
