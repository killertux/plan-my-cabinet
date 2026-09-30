# Proposal

## Why

Shops band the exposed edges of MDF and MDP parts, and ordering parts (for
example through CorteCloud) needs the band on each side. The app had no
banding at all, and no idea which materials take it.

## What Changes

- **Material type.** Materials say what they are made of: MDF, MDP, HDF,
  plywood, solid wood or other. Only MDF and MDP take banding. Older files get
  the type from the material's name.
- **Edge bands.** A project keeps its bands (name as the shop lists it,
  thickness, height, colour). New projects and templates start with the common
  white and raw bands; white MDF and MDP band in white by default.
- **Automatic banding.** Every board edge is automatic unless set by hand: it
  gets its material's default band when it is free, and none when another
  board sits flat against at least half of it (gap at most 0.5 mm). Moving
  parts (doors, drawers) only join boards that move with them. Manual On/Off
  per edge overrides the rule.
- **Editing.** A Banding section in the board inspector (edge diagram, band
  choice, presets Automatic, None, Front, All 4), the same for several boards,
  a Band edges tool in the 3D view, an Edge bands list with a dialog, and type
  and default band in the material dialogs. Each change is one undo step.
  Changing to a material without banding drops its banding in the same step.
- **Outputs.** The workshop PDF gets a banding column and an edge band table
  (metres, +10 %). The receipt fingerprint (version 6) includes banding. MCP
  tools list, create, update and remove bands and set board banding; board
  and material rows show banding.
- Schema 5.

## Impact

- `domain` (Material kind and default band, EdgeBand, Board banding), `persistence` (v5 migration).
- New `banding_rules`, `banding`; `workshop_document`, `export` (fingerprint v6), mesh and raster rendering.
- Desktop: `banding_ui`, `assembly_ui`, `board_dialogs`, viewport Band tool, actions.
- MCP: `service/banding.rs`, material inputs.
