# Design

## Lighting

`Light::brightness(normal) = 0.40 + 0.50·max(0, n·l) + 0.12·(0.5 + 0.5·n.z)`;
with the light off `0.80 + 0.16·(0.5 + 0.5·n.z)`. Colours are shaded toward a
light grey, never black. `lighting::wgsl()` formats the same constants into
the viewport shader, so the two renderers cannot drift.

Face vertices carry position, world normal, base colour, UV in millimetres
and a surface index (12 floats). Lines and the floor shadow keep position and
colour and have their own unlit pipeline. Lighting is per pixel, from the
interpolated normal, on the GPU and in the rasterizer (perspective-correct).

The headlight sits just above and right of the eye. "Fix the light here"
stores the headlight's angles (azimuth from the front, elevation) as a fixed
light. The preference is machine-local (`preferences.json`), defaulting to
follow camera; files without it load the default.

## Coating

`Material.coating` (`None`, `OneSide`, `BothSides`, default both) applies to
MDF, MDP and HDF only. Stock and allocation already match by material, so a
coating difference is a material difference. `Board.coated_face`
(`Auto`, `MinZ`, `MaxZ`) matters only for one-side materials.
`coating_rules` decides automatic faces from the board's world frame: across
the cabinet (normal mostly ±Y) the front face; lying (±Z) the upper face;
along the cabinet (±X) the face away from the middle of its top assembly.

## Surfaces

Five tileable 128 × 128 grayscale detail maps are generated at start-up from
seeded value noise and cellular chips (no image files). They multiply a base
colour: the fixed core colour per kind for raw faces and edges, the material
colour for wood grain. The GPU gets them as a mipmapped texture array; the
rasterizer samples the same bytes bilinearly.
