# Plan My Cabinet — agent guide

## The model

- **Project**: name, currency (BRL or USD), display unit, grid, saw kerf (default 5 mm), cut fee.
- **Material**: name, default thickness, grain rule (`length`, `width`, `unrestricted`), color. The standard set (White/Raw MDF 6–25 mm, HDF 3 mm, …) has known sheet sizes, so `add_needed_sheets` can buy sheets for it. For your own materials declare sheets with `create_stock`.
- **Board**: a rectangular part. Local X = length, Y = width, Z = thickness. Thickness is copied from the material when created. A board belongs to at most one assembly.
- **Assembly**: a rigid group with its own pose. Moving it moves its contents.
- **Hardware placeholder**: a dimensioned box (foot, handle, appliance) for context; never cut.
- **Stock piece**: a sheet or offcut with measured size, grain (`along_x` = along its length), trims, `owned` or `to_purchase`, price and a fill priority. Aliases S1, S2… (and O1… for offcuts).
- **Placement**: a board on a stock piece at an origin (from the sheet's bottom-left corner), optionally turned 90°, optionally locked. At most one per board.
- **Hinge**: a catalog hinge joining a door board to the board it hangs from (its *mount*), at a distance along the door's hinge edge, with K (cup edge setback) and R (overlay) — or F (gap) and E (depth) for inset hinges.
- **Door**: the relationship that lets a door board (or assembly) swing on its hinges.
- **Drawer slides**: a pair of catalog slides (one length of a family, e.g. FGVTN TT45 500 mm) joining a drawer (an assembly of box boards) to the two carcass sides beside it. The gap between box side and carcass side must match the slide's clearance (12.7 +0.5/−0 mm for most; 19 ±0.3 for TT90).
- **Foot**: a catalog foot placed like hardware (never cut): tapered plastic feet, chrome posts with a plate and a glide, straight table legs, industrial tube frames. Drawn with the product's real shape.

## Coordinates

World X runs to the right, Y toward the back, Z up. The front of a cabinet faces -Y.
Templates build the carcass with its front at Y = 0, the left side's outer face at X = 0 and the bottom on Z = 0. Everything that sits in front of the carcass (overlay doors, drawer fronts) has negative Y.

Rotations are degrees, applied about the board's own X, then Y, then Z (the app's numeric position dialog). Common orientations:

| Part | Rotation | Result |
|---|---|---|
| bottom, top, shelf | none (`lay_flat`) | length along X, width along Y, thickness up |
| side panel | `preset: stand_up` (ry −90) | length up Z, width along Y, thickness toward −X (a side at x = 18 fills x 0..18) |
| back panel, door, drawer front | rx 90 | length along X, width up Z, thickness toward −Y (a door at y = 0 fills y −18..0) |

## Placing parts without arithmetic

`place_board_on_face` moves `board` so its `source_face` lies on `target`'s `target_face`:

- faces are `-x`/`+x` (length ends), `-y`/`+y` (width edges), `-z`/`+z` (broad faces), in each board's own axes;
- `source_align`/`target_align` choose which corner of each face lines up (`start`, `centre`, `end` along the two remaining axes, in increasing axis order);
- `offset_mm` shifts along the target face, `gap` moves away from it.

Example — a shelf resting on a bottom panel, 400 mm up:
`{"board":"Shelf","target":"Bottom","source_face":"-z","target_face":"+z","gap":400}`.

After placing, run `describe_scene`: it confirms contacts ("Shelf touches Left side, Right side") and flags overlaps.

## Lengths and money

Numbers are millimetres. Strings may carry a unit: `"600"`, `"60 cm"`, `"0.6 m"`, `"23 5/8 in"`, `"2'"`, `"18,5"`. A value that is not a whole micrometre (many inch values) needs `allow_rounding: true`. Money: `"12.34"` or `12.34` in the project currency; `null` means unknown (not free).

## Edge banding

Only MDF and MDP boards take edge band (a material's `kind` says what it is;
`list_materials` shows `takes_banding`). Every board edge is **automatic** by
default: it gets its material's `default_band` when it is free, and no band
when another board sits flat against it (at least half of the edge face, gap
0.5 mm or less). Doors and drawers only join boards that move with them.
`get_board` shows each edge (`length_1`, `length_2` along the length,
`width_1`, `width_2` along the width) with its band, whether it is `auto`,
`on` or `off`, what it touches and whether it faces the front.

1. `list_edge_bands`; `create_edge_band` with the name the shop uses
   ("Fita Branca 1x22"), thickness and height.
2. `update_material` with `default_band` so automatic edges get it.
3. Change only the exceptions with `set_board_banding`: a `preset`
   (`auto`, `none`, `front`, `all_four`) or `edges` with `value`
   (`auto`, `on`, `off`). Boards that take no banding are skipped.

Sizes stay finished sizes, band included; the shop deducts the tape.

## Coating

MDF, MDP and HDF sheets are bought coated on `none`, `one_side` or
`both_sides` (`coating` in `list_materials`, `create_material`,
`update_material`). Coating belongs to the material: "MDF Branco 1 face" and
"MDF Branco" are two materials with their own stock, and the cut plan never
mixes them. On a one-side material each board's `coating` row says which
broad face is coated; automatically the face toward the front (doors, backs),
up (bottoms, shelves) or outside the cabinet (sides). Change it with
`set_board_coated_face` (`auto`, `min_z`, `max_z`, `flip`). In pictures, raw
faces and unbanded edges show the sheet's core; banded edges show the band.

## Ordering parts from a shop (CorteCloud)

`get_part_list` previews the parts a shop would cut: identical boards grouped
with a quantity, the cabinet, material, size, banding and holes (hinge cups;
screw pilots when sized), plus what drilling is left out and why.
`export_design` writes the CorteCloud file (`format: "cortecloud-json"`). It
needs only a valid design: no sheets, cut plan or kerf, because the shop nests
the parts. Pass `screw_pilot` (diameter and depth) to have hinge-plate and
slide screws drilled, and `include` (`banding`, `hinge_holes`,
`slide_holes`, all true by default) to leave any of them out, e.g. only the
boards. The user imports the file in CorteCloud with
*Serviço Completo › Carregar arquivo Cortecloud* and links materials and bands.

## Cut planning

1. Boards are placed on stock automatically when created, if a sheet of their material and thickness has room.
2. `suggest_sheets` says what is missing; `add_needed_sheets` buys standard sheets and places everything that waits.
3. `auto_place` with `fill_gaps` places waiting boards; `replan` repacks everything not locked (usually onto fewer sheets).
4. `get_diagnostics` lists boards not validly placed and sheets without a valid cut sequence, plus a to-do list.
5. `optimize_cut_plan` searches for a cheaper plan or one with fewer cuts; apply it right away with `apply_optimization`.
6. `edit_sheet` places boards by hand (all-or-nothing, validated): `{"ops":[{"op":"place","board":"Door","stock":"S2","x":10,"y":10},{"op":"lock","board":"Door","locked":true}]}`.
7. `render_sheets` shows each sheet with numbered cuts.

All cuts are straight, full-length (guillotine) cuts and include the kerf. Grain rules are respected: a `length`-grain board must lie along an `along_x` sheet unless turned onto an `along_y` sheet.

## Hinges and doors

1. Place the door next to the side it hangs from (overlay doors cover the side's front edge).
2. `suggest_hinges {"door":"Left door"}` finds the mount, hinge edge and positions without changing anything.
3. `add_hinges {"door":"Left door"}` pins the bundled hinge if none is pinned, chooses the count (2 up to 900 mm, 3 up to 1500 …), positions (100 mm from each end), and the K/R pair whose R equals the side thickness.
4. `create_door {"moving":"Left door"}` turns it into a swinging door; `render_door_opening {"door":"Left door","angle_degrees":90}` shows it open.
5. Pictures draw each hinge as a cup in the door and a plate on the side, and list it in the legend ("Left door hinge 1"). A hinge with a problem is amber; one whose references can't be placed is not drawn. `describe_scene` leaves hinges out, since a cup sits inside the door.

## Drawer slides

1. `generate_template {"kind":"drawers"}` already installs slides on every drawer (default TT45 Slowmotion, the longest length that fits; `slides: {"slide":"tt90-slow"}` for heavy drawers, `{"none": true}` for none). The box is two clearances narrower than the opening.
2. For a drawer you built: group its boards (`group_objects`), then `suggest_slides {"drawer":"Drawer"}` (read-only: detected sides, gaps, chosen length) and `add_slides {"drawer":"Drawer"}`. Issues explain a wrong gap, a slide longer than the carcass or box, or a bad height.
3. `list_slides` gives hole distances from each board's front edge for the shop; `update_slide {"slide":"Drawer","length":450}` changes the length.
4. `render_drawer_opening {"drawer":"Drawer 1","fraction":1}` shows it open with the slide members.

## Feet

Feet are placed, not cut, and they do not lift the furniture: raise it first (`transform_objects {"objects":["Chest"],"translate_mm":[0,0,100]}` for 100 mm feet), then
`add_foot {"model":"generic-post-square-100","parent":"Chest","anchor":"top_center","positions":[[40,40,100],[560,40,100],[40,500,100],[560,500,100]]}`.
With `anchor: top_center` each position is the centre of the foot's mounting face (the underside of the bottom); the default anchor is the foot's box corner on the floor. `list_hardware_catalog {"kind":"foot"}` lists the models; `describe_scene` confirms the feet touch the bottom and stand at z = 0.

## New hardware models

`create_foot_model` takes a shape — `tapered` (top/bottom section, height: plastic feet), `post` (tube, plate, plate_thickness, optional glide, height: chrome feet and straight legs), `frame` (top_width, bottom_width, tube_width, tube_depth, optional crossbar_height and glide: industrial legs; a smaller bottom_width makes a trapezoid) — plus color and mounting holes. Sections are `{"diameter": d}` or `{"width": w, "depth": d}`.
`create_slide_model` takes the profile height, clearance `{nominal, minus, plus}`, and lengths with travel and holes. Both pin the model to the project; `save_to_catalog: true` also writes it to the user catalog (`user-models.toml`) so the app and later projects offer it.

## Worked example: base cabinet with two doors

```
generate_template {"kind":"base","name":"Kitchen base 800","dimensions":{"width":800,"depth":580,"height":720}}
create_boards {"boards":[
  {"name":"Left door","material":"White MDF 18","length":397,"width":716,"pose":{"x":0,"y":0,"z":2,"rx":90},"parent":"Kitchen base 800"},
  {"name":"Right door","material":"White MDF 18","length":397,"width":716,"pose":{"x":403,"y":0,"z":2,"rx":90},"parent":"Kitchen base 800"}]}
describe_scene {}
render_views {"views":[{"view":"iso"},{"view":"front","projection":"orthographic"},{"view":"iso","hidden":["Left door","Right door"]}]}
add_needed_sheets {}
get_diagnostics {}
set_stock_prices {"prices":[{"stock":"all","price":"320.00"}]}
set_project_settings {"cut_fee":"4.50","confirm_shop_kerf":true}
optimize_cut_plan {"objective":"lowest_new_spending"}
apply_optimization {"search_id":"<from the search>","candidate":0}
render_sheets {}
add_hinges {"door":"Left door"}
add_hinges {"door":"Right door"}
create_door {"moving":"Left door"}
create_door {"moving":"Right door"}
render_door_opening {"door":"Left door","angle_degrees":90}
save_project {"path":"/Users/me/Documents/kitchen-base.pmcab"}
```

## Worked example: a bookshelf by hand

```
new_project {"name":"Bookshelf","seed_standard_materials":false}
create_material {"name":"Plywood 18","thickness":18,"grain":"length"}
create_boards {"boards":[
  {"name":"Left side","material":"Plywood 18","length":1800,"width":300,"pose":{"x":18},"preset":"stand_up"},
  {"name":"Right side","material":"Plywood 18","length":1800,"width":300,"pose":{"x":800},"preset":"stand_up"},
  {"name":"Bottom","material":"Plywood 18","length":782,"width":300,"pose":{"x":18,"z":0}},
  {"name":"Top","material":"Plywood 18","length":782,"width":300,"pose":{"x":18,"z":1782}},
  {"name":"Shelf","material":"Plywood 18","length":782,"width":300,"pose":{"x":18,"z":600}}]}
duplicate_board {"board":"Shelf","offset_mm":[0,0,400],"count":2,"name":"Shelf"}
group_objects {"objects":["Left side","Right side","Bottom","Top","Shelf","Shelf 1","Shelf 2"],"name":"Bookshelf"}
describe_scene {}
create_stock {"material":"Plywood 18","length":2440,"width":1220,"grain":"along_x","price":"410.00","quantity":2}
get_diagnostics {}
render_views {"views":[{"view":"iso"},{"view":"front","projection":"orthographic"}]}
save_project {"path":"/Users/me/Documents/bookshelf.pmcab"}
```

## Errors

Failures return `{code, message, hint, details}`. Common codes: `no_open_project`, `unsaved_changes` (save or pass `discard_changes`), `not_found` / `ambiguous_ref` (use ids), `invalid_length`, `rounding_required`, `invalid_pose`, `material_in_use`, `allocated_stock`, `invalid_placement` (see `details.sheets`), `locked`, `stale_search` (optimize again), `revision_mismatch`, `installation_error`, `joint_error`, `template_error`, `catalog_error` (a new model with invalid values; the message names the field), `conflict` (a drawer that already has slides), `io`.
