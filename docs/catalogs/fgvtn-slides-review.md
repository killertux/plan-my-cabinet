# FGVTN drawer slide sheets: source review

Reviewed on 2026-09-30 for the bundled pack `catalogs/fgvtn-slides.toml`. Each
sheet was read from a raster rendering of the actual page, not from text
extraction alone, and every table row was turned into hole positions by
following the drawing's dimension chains.

The PDFs are cited by URL and SHA-256 and are **not** redistributed. The
files in hand were byte-identical to the ones published at the cited URLs on
the review date. Only factual identifiers and dimensions are shipped, with
attribution.

| Sheet (source id) | SHA-256 | What was checked |
|---|---|---|
| TT45 Slowmotion (`tt45-slowmotion`), Apr 2024, p. 46 | `68cf8c4e00f387f6324571613f8386c17017a0b0cd6c7fbbece2fa061b62e31e` | Codes 0073.045350SX–0073.045550SX; A 350–550; 45 kg; 45 high; clearance 12,7 +0,5/−0; F (travel) 335/390/450/500/550; cabinet holes 35, 51 (35+16), 99 (35+64), 35+B, 35+B+C; drawer holes 32 (slot), 48, 57,5, 48+D, 48+E; PHS AA 3,5 screws; rear holes 2 × ø4,5, 7,5 from the rear, 32 apart. |
| TT44 Slowmotion (`tt44-slowmotion`), Jun 2023, p. 73 | `98a69f08d9d4b74cf70ca17ff767f96c0fbf52d53f073470372eee647ccdd9ce` | Same A–F table as TT45; 35 kg; finishes satin zinc (0073.044…), white (0073.044B…), black (0073.044P…); rear holes 25 apart. |
| TT35 Slowmotion (`tt35-slowmotion`), Nov 2023 | `e707e3a523417588dd449c94abcf5d7bb8757cd99be069e8c40513b3a9de46c3` | Codes 0073.035250SX–0073.035550SX; 35 high; 25 kg (10 kg for 250 and 300); travel 250/297/345 then equal to the length; hole groups per the table. |
| TN H45 Slow (`tn-h45-slow`) | `92dc07449cd291e0ccbd48648f7968838de80699e46998bc311bf79305bde800` | Codes 545TN287SX25000–545TN287SX55000; 45 high; 35 kg; travel 200/275 for 250/300, else the length (+0/−3); rear holes 2 × ø4,5, 18 apart, C = 40 (up to 350) or 60 (400 and longer) from the rear, required for the soft close. |
| TT90 Slow (`tt90-slow`) | `5aba69c778e3562a9ecfea00af79d93a13bc0a1a6ed716c39d1db6456b3c8dc1` | Codes 0073.090450SX–0073.090600SX; 52 high ("H52 ref."); 90 kg; clearance 19 ±0,3; travel L − 15 (+0/−3); hole groups from the front at 51 then per the table; countersunk Ø4×15 chipboard or M4/M5 screws; ø5 rear hole, screw required. |

## Inferred values

- **Front setback 2 mm.** The sheets dimension 37 mm from the cabinet front
  to the first cabinet-member hole, which is 35 mm from the member's front.
- **The drawer member is shorter than the nominal length.** No sheet
  dimensions it; slides of the box's length are the usual choice, so the app
  allows 10 mm between the nominal length and the box side.
- **Minimum inside depth** is the setback plus the nominal length.

## Data left out or simplified on purpose

- **TT90 hole groups** are a ø5 centre hole with two ø6 countersunk holes
  16 mm either side. Only the group centres are shipped: whether the ±16 runs
  along the member or across it could not be read with certainty.
- **TT35 rear screw** is shown but not dimensioned; it is only mentioned in
  the rear-fixing note.
- **TT35 250 mm** has no dimensioned holes in the table beyond the front
  group (35, 51, 99 / 32, 48, 57,5).
- **Load ratings** are the manufacturer's per-pair figures and are not
  checked against the drawer.
