# Proposal

## Why

The 3D view had no real lighting. Each face of a board got a fixed shade by
which side of the board it was, not by where it faced, so a table leg looked
dark straight on and identical parts modelled differently looked different.
Hardware used yet another light. Boards were also one flat colour all round,
while real MDF, MDP and HDF are coated on none, one or both faces and show
their raw core on uncoated faces and unbanded edges.

## What Changes

- **Lighting.** Faces are lit from their world normal by one directional
  light plus a soft sky term, the same formula in the viewport shader and the
  picture rasterizer. The light follows the camera (default), stays fixed
  where the user puts it, or is off (even shading). Chosen in Settings and in
  a light menu on the viewport tool strip; "Fix the light here" fixes it
  where the camera is. A machine-local preference. Pictures (PDF, thumbnails,
  agent pictures) always use a fixed studio light.
- **Coating.** Materials of type MDF, MDP or HDF are coated on none, one side
  or both sides. Coating is part of the material, so different coatings are
  different materials with their own sheets. Older files get it from the name
  ("cru"/"raw" none, "1 face" one side, otherwise both). On a one-side
  material each board's coated face is automatic (toward the front, up, or
  outside the cabinet) or chosen by hand; flip and back to automatic are one
  undo step each.
- **Surfaces.** Coated faces take the material colour; raw faces and
  unbanded edges show the sheet core with a generated texture (MDF fibre,
  MDP chips, HDF fibre, plywood plies, wood grain); banded edges take the
  band colour. Plywood and solid wood faces show grain in the material
  colour.
- **Agents.** Material rows and inputs carry `coating`; board rows carry
  `coating`; new tool `set_board_coated_face`.
- Schema 6.

## Impact

- New `render::lighting`, `render::surface`, `coating_rules`, `coating`.
- `domain` (Material coating, Board coated face), `persistence` (v6),
  `local_preferences` (lighting), `mesh` (lit, textured face vertices),
  `raster`, `hardware_mesh`.
- Desktop: viewport shader and pipelines, controls light menu, Settings
  Lighting group, coating inspector section, material dialogs, actions.
- MCP: `service/coating.rs`, material inputs and rows, board rows.
