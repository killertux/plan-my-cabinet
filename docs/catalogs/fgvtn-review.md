# FGVTN hinge sheets: source review

Reviewed on 2026-09-27 for the bundled pack `catalogs/fgvtn.toml`. Each sheet
was read from a raster rendering of the actual page, not from text extraction
alone. The Click 3D Slow kit keeps its earlier review in
[hinge-source-review.md](../hinge-source-review.md).

The PDFs are cited by URL and SHA-256 and are **not** redistributed. Only
factual identifiers and dimensions are shipped, with attribution. The rights
rule in the earlier review applies unchanged.

| Sheet (source id) | SHA-256 | What was checked |
|---|---|---|
| Easy TN Click Slow Inox Calço Duplo (`easy-tn-click-slow-inox-calco-duplo`), p. 25, 2021-07-01 | `4143fa79…ed10` | Codes 51MX15TNSS000CD/008CD/015CD; cup ø35×11,3; door 14–20; H=0 on all three drawings; table K 3–6 → Reta 13–16, Curva 3/6/7/8, Alta 3/2/1/–. No opening-angle icon. Plate vertical pitch not dimensioned (columns 12 mm apart). |
| FGVTN MS Slow Calço Fixo (`ms-slow-calco-fixo`), Feb 2024, p. 91 | `3575e766…4c41` | Codes 51MS15XFG0100BF/0108BF/0115BF; cup Ø35×9,5; K 3–6; door 12–20; 105°; plate pitch 32, cup screws 48; H: Reta 4, Curva 2, Alta 2; Reta 13–16, Curva 5,5–8,5, Alta 4–1. |
| TN Inox (`tn-inox`), Aug 2023 | `89562c74…f692` | Codes 51MS15TNSS00050/08050/15050; cup Ø35×11,3; K 3–7; door 16–26; 110°; H=0; plate pitch 32, cup screws 52 / 5,5; Reta 14–18, Curva 6–10, Alta F 3/2/1/–/–. |
| TN Inox Slowmotion (`tn-inox-slowmotion`), p. 52 | `e8131030…e674` | Codes 51MS15TNSX00050/08050/15050; slide-on; 110°; K 3–7; door 16–20; plate pitch 32; Reta H=2 14–18, Curva H=2 6–10, Alta H=0 F 4/3/2/1/–; PHS 4×15 screws. |
| TN MS Slow Calço Fixo (`tn-ms-slow-calco-fixo`), p. 16, 2021-11-09 | `65de8c33…c287` | Codes 51MS15XTN2200BF/2208BF/2215BF; cup 11,3 (ø35 on drawing); door 16–20; 110°; "Calço 0 mm" (drawings print no H, so H=0 is taken from this note); Reta 14–17, Curva 6–9, Alta 4–1. Plate vertical pitch not dimensioned (columns 14 mm apart). |
| TN MS Slow Calço Fixo Caneco 9,8 (`tn-ms-slow-calco-fixo-caneco-9-8`), Apr 2026 | `0f8eabbd…2638` | Codes 51MS15XTN4900BF/4908BF/4915BF; cup Ø35×9,8; door 12–18; 110°; Reta H=2 12–15, Curva H=2 4–7, Alta H=0 5–2. Plate vertical pitch not dimensioned. |
| TN MS Slow Inox Calço Fixo (`tn-ms-slow-inox-calco-fixo`), Jul 2023, p. 122 | `05721c0e…5b00` | Codes 51MS15XTNSS00CF/08CF/15CF; cup printed "ø3,5x11,3" (drawing: Ø35); 105°; K 3–6; door 15–20; Ø4×16 screws; Reta H=2 13–16, Curva H=0 5,5–8,5, Alta H=0 –/1/2/3. Plate vertical pitch not dimensioned. |

Full hashes are in the pack's `[[sources]]` entries.

## Data left out on purpose

- **Easy TN Curva, K = 3.** The sheet prints R = 3, then 6, 7, 8. Every other
  table steps by 1 mm, so K = 3 → 3 is very likely a misprint of 5. The row
  is omitted; K = 4–6 remain.
- **TN MS Slow Inox Alta (51MS15XTNSS15CF).** The printed gap column
  (–, 1, 2, 3) grows with K. On every other FGVTN inset table it shrinks. The
  whole variant is omitted until the manufacturer confirms it.
- **Plate vertical pitch** is not dimensioned on four sheets. Those families
  have no `hole_pitch`, so the app gives the plate centre line only and shows
  the pitch as "not documented".
- **Easy TN opening angle** is not printed. Door motion preview is unavailable
  for that family (accepted in the pack with `allow`).

## Interpretation

- **E (inset).** Each Alta drawing dimensions the plate at "E + 37" from the
  cabinet side's front edge, with E measured to the door's inside face. The
  app asks for E and places the plate at the front offset plus E.
- **Adjustment ranges** (vertical, frontal, overlay) are recorded where the
  sheet gives a symmetric ± value; asymmetric ones (+2/−3) are kept in notes.
- **Not verified or not supplied:** pilot hole diameters and depths, cup screw
  positions for drilling, load ratings, hinge count and linkage paths. The
  on-screen and PDF warnings about fasteners still apply.
