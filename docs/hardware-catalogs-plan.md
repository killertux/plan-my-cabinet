# Hardware catalogs plan

Goal: hardware data (hinges first; drawer slides, handles and so on later) comes from
**external catalog files, one pack per manufacturer**, instead of Rust constants. The app
can load, validate and try out each pack. The first real pack is FGVTN, built from the
seven hinge sheets in `fichas_dobradicas_fgvtn.zip`.

## Where we are today

- `src/catalog/hardware_catalog.rs` hard-codes **one** kit, FGVTN Click 3D Slow Reta /
  Calço 0 (`51MX153DRV00100` + plate `52MX15FG11003D`).
- `is_verified()` means "equals that one Rust baseline, field for field".
- `Project::validate` rejects any `verified_hinge` that differs from the baseline.
- `VerifiedHinge` models an **overlay-only** hinge: a single K→R table, one plate
  height, a 32 mm plate pitch and a 37 mm front offset.
- `hinge_installation::diagnose` and `door_joint::opening_limit` read those facts.
- The project stores a pinned snapshot (`CatalogReference`), so reopening a project
  offline never changes its hardware. **We keep this principle.**
- `docs/hinge-source-review.md` sets the rights rule: bundle facts and citations only,
  never the manufacturer PDFs or drawings. **We keep this too.**

About 17 files touch `CatalogReference`, `VerifiedHinge` or `hardware_catalog::*`. Most of
them are `hinge_ui.rs` (14), `export.rs` (9) and `door_joint.rs`.

## What the FGVTN sheets contain

All seven are single-page PDFs. Each has three arm variants: **Reta** (full overlay),
**Curva** (half overlay) and **Alta** (inset). Every variant has a K table: R (overlay)
for Reta and Curva, F (gap) for Alta.

| Family | Codes (Reta / Curva / Alta) | Cup Ø×depth | Door | K | Opening | H (plate) | Reta R | Curva R | Alta F |
|---|---|---|---|---|---|---|---|---|---|
| Easy TN Click Slow Inox Calço Duplo | 51MX15TNSS000CD / 008CD / 015CD | 35×11,3 | 14–20 | 3–6 | ? | 0 | 13,14,15,16 | 3,6,7,8 ⚠ | 3,2,1,– |
| FGVTN MS Slow Calço Fixo | 51MS15XFG0100BF / 0108BF / 0115BF | 35×9,5 | 12–20 | 3–6 | 105° | 4 / 2 / 2 | 13,14,15,16 | 5,5 6,5 7,5 8,5 | 4,3,2,1 |
| TN Inox | 51MS15TNSS00050 / 08050 / 15050 | 35×11,3 | 16–26 | 3–7 | 110° | 0 | 14–18 | 6–10 | 3,2,1,–,– |
| TN Inox Slowmotion | 51MS15TNSX00050 / 08050 / 15050 | 35×11,3 | 16–20 | 3–7 | 110° | 2 / 2 / 0 | 14–18 | 6–10 | 4,3,2,1,– |
| TN MS Slow Calço Fixo | 51MS15XTN2200BF / 2208BF / 2215BF | 35×11,3 | 16–20 | 3–6 | 110° | 0 ⚠ | 14–17 | 6–9 | 4,3,2,1 |
| TN MS Slow Calço Fixo Caneco 9,8 | 51MS15XTN4900BF / 4908BF / 4915BF | 35×9,8 | 12–18 | 3–6 | 110° | 2 / 2 / 0 | 12–15 | 4–7 | 5,4,3,2 |
| TN MS Slow Inox Calço Fixo | 51MS15XTNSS00CF / 08CF / 15CF | 35×11,3 ⚠ | 15–20 | 3–6 | 105° | 2 / 0 / 0 | 13–16 | 5,5–8,5 | –,1,2,3 ⚠ |

⚠ These values need a visual review before the pack can be marked `reviewed`:
- **Easy TN, Curva, K=3 → 3:** the jump from 3 to 6 breaks the +1 step every other table has.
- **MS Slow Inox, Alta F:** F rises with K (–,1,2,3). On every other sheet it falls.
- **MS Slow Inox, cup:** the sheet prints "ø3,5x11,3". This is obviously ø35.
- **TN MS Slow Calço Fixo:** H=0 is inferred from "Calço 0 mm" and is not printed next to
  each drawing.
- **Easy TN:** no opening angle appears in the extracted text.

Other shared facts:
- Plate front reference: 37 mm (inset: 37+E).
- Adjustment ranges: vertical ±2 to ±3,5, frontal ±3 and overlay ±2,5 to ±3 mm.
- Cup-screw spacing: 48 or 52 mm, with a 5,5 mm offset.
- Plate hole pitch: 32 mm (calço fixo) or 52 mm (calço duplo).
- Screws: only MS Slow Inox names them (Ø4×16) and TN Inox Slowmotion (PHS 4×15).

The SHA-256 of each downloaded PDF has been computed and goes into the pack as the source
fingerprint.

## Design

### 1. Pack file format (TOML, one file per manufacturer line)

TOML allows comments, which suits hand-authored data a woodworker might edit.
- It adds the `toml` crate. JSON would need no new dependency but has no comments.
- Lengths are written in **mm as decimals** (`5.5`).
- Loading converts them to exact `Length` micrometres and rejects anything finer than 1 µm.

```toml
schema = 1
id = "fgvtn"                 # stable pack id, [a-z0-9-]
manufacturer = "FGVTN"
country = "BR"
version = "2026-09-27"       # pack revision, bumped on any data change
review = { status = "reviewed", by = "…", date = "2026-09-27" }  # or "draft"

[[sources]]
id = "ms-slow-calco-fixo"
url = "https://www.fgvtn.com.br/site/novopdf/Dobradica_FGVTN_MS_Slow_Calco_Fixo_Fev_2024.pdf"
sha256 = "3575e766…4c41"
revision = "Fev 2024 (DS_CATALOGO_GERAL_FGVTN_JAN24 p. 91)"

[[hinges]]                   # later: [[drawer_slides]], [[handles]] …
id = "ms-slow-calco-fixo"
name = { pt-BR = "Dobradiça FGVTN MS Slow Calço Fixo", en = "FGVTN MS Slow fixed-plate hinge" }
source = "ms-slow-calco-fixo"
soft_close = true
finish = "nickel"
mounting = "fixed_plate"     # clip | slide_on | fixed_plate
opening_degrees = 105
cup = { diameter = 35, depth = 9.5, screw_spacing = 48 }
door_thickness = { min = 12, max = 20 }
plate = { front_offset = 37, hole_pitch = 32 }
adjustment = { vertical = 3.5, frontal = 3, overlay = 3 }
fasteners = "unavailable"    # or { screw = "Ø4×16", pilot_depth = … }

  [[hinges.variants]]
  arm = "full_overlay"       # Reta
  code = "51MS15XFG0100BF"
  plate_height = 4
  k_table = [[3, 13], [4, 14], [5, 15], [6, 16]]      # K → R

  [[hinges.variants]]
  arm = "half_overlay"       # Curva
  code = "51MS15XFG0108BF"
  plate_height = 2
  k_table = [[3, 5.5], [4, 6.5], [5, 7.5], [6, 8.5]]

  [[hinges.variants]]
  arm = "inset"              # Alta — values are F (gap), plate at 37 + E
  code = "51MS15XFG0115BF"
  plate_height = 2
  k_table = [[3, 4], [4, 3], [5, 2], [6, 1]]
```

**Where the packs live:**
- **Bundled:** `catalogs/*.toml` in the repo, compiled in with `include_str!`. This covers
  FGVTN and the migrated Click 3D Slow.
- **User:** `<user data dir>/catalogs/*.toml`. The app offers "Open catalogs folder",
  "Import pack…" and "Reload".
- If a user pack has the same `id` as a bundled one, the user pack shadows it, with a
  visible note.
- Nothing is fetched from the network.

Side fix: `project_ui::user_data_dir` uses "Plan My Cabinet" while `main.rs` uses
"PlanMyCabinet". Settle on one before adding a third user of that directory.

### 2. Rust model (`src/catalog/pack/`)

- **`schema.rs`:** serde structs that mirror the file (raw mm as `f64`/decimal).
- **`load.rs`:** parse, convert units, and collect **all** problems (not first-error)
  into `Vec<PackIssue { path: "hinges[1].variants[2].k_table[0]", severity, key }>`.
  - Errors: duplicate ids or codes, min > max, K table not strictly increasing in K, K
    outside the door, cup depth ≥ min door thickness, unknown source, sha256 not 64 hex
    digits.
  - Warnings: non-monotonic R/F (catches both ⚠ tables above), missing opening angle,
    fasteners unavailable, `review.status = draft`.
- **`registry.rs`:** `CatalogRegistry { packs: Vec<LoadedPack { origin: Bundled|User(path),
  content_sha256, issues, pack }> }`. It is loaded once at startup, reloads on request, and
  is owned by `DesktopApp` (not by `Project`).
- **Kind enum for the future:** `CatalogItem::Hinge(HingeSpec)`. `DrawerSlide(SlideSpec)`
  is added later, without touching the loader framework: one new `[[drawer_slides]]`
  table, one validator and one project snapshot variant.

### 3. Project snapshot and migration

`CatalogReference` stays the project-owned pinned copy. It gets these fields:

```rust
pub struct CatalogReference {
    pub id: Uuid,
    pub name: String,
    pub origin: CatalogOrigin,      // pack id, pack version, pack sha256, item id, variant code
    pub trust: Trust,               // Reviewed | UserSupplied | Reference (no numbers)
    pub item: PinnedItem,           // PinnedItem::Hinge(HingeFacts { arm, plate_height, k_table, cup, … })
    // legacy fields kept for v-old reading only
}
```

- **Validation changes meaning.** Today it asks "equals the Rust baseline". It becomes
  "the pinned facts are internally consistent", using the same rules as the pack
  validator.
  - A project must open on a machine that does not have the user's pack.
  - Trust is recorded at the moment of pinning and re-checked against the registry when
    the pack is present. The UI shows "pack changed, update available" and never updates
    silently.
- **Schema version bump plus migration:**
  - An old `verified_hinge` equal to the Click 3D Slow baseline becomes `Trust::Reviewed`
    with `arm = full_overlay`.
  - Anything else becomes `Trust::Reference`.
  - The existing tests (`legacy_v1_reference_has_no_verified_claim`, offline reopen,
    atomic undoable update) move to the new functions.
- **Update from catalog** replaces `update_from_builtin`. It shows a field diff, then
  re-diagnoses dependent installations in one transaction, as it does today.

### 4. Geometry: arm types

- **Full and half overlay** (Reta, Curva) use today's formula.
  - Cup centre at K + Ø/2 from the door edge.
  - Plate at `front_offset` on the mounting board.
  - The table gives R.
  - Only the data changes. `plate_height` is already a field.
- **Inset** (Alta) is new.
  - The door sits between the panels.
  - The plate moves to `front_offset + E`, where E is the door's setback from the
    cabinet front.
  - The table gives F, the gap between the door edge and the panel.
  - `diagnose` needs an `InstallationIssue::UnsupportedGap`.
  - The door-joint axis also changes.
  - Until this phase lands, inset variants load and validate. They show a "reference
    only — inset geometry not supported yet" note and produce no drilling numbers.
- The door-thickness range, cup depth versus thickness, and the opening limit all move
  from `VerifiedHinge` to per-family `HingeFacts`. `door_joint::opening_limit` reads the
  pinned facts instead of calling `is_verified`.

### 5. "Validate and test" in the app

- **Catalogs panel** (in the Hardware workspace, and also reachable from Settings):
  - Lists packs as rows: manufacturer, version, origin (bundled/user), status badge
    (Reviewed / User / Draft / Errors) and item count.
  - Clicking a row shows its issues, each with the TOML path and a plain-language
    message.
  - Buttons: Reload, Open folder, Import pack….
- **Hinge test bench** (a dialog opened from any catalog row):
  - Pick family → arm → K, then enter a door thickness and a panel thickness.
  - It shows R or F, the cup centre, plate hole positions, any issues, and our own simple
    side-view drawing (we never copy the manufacturer's).
  - "Apply to selected door" goes through the normal hinge-installation flow.
- **Hinge picker** in the existing hinge dialog becomes Manufacturer → Family → Arm → K
  (with R/F shown). It is filtered to the families that fit the selected door thickness.
- **CLI for pack authors and CI:** `plan-my-cabinet --check-catalog file.toml` prints the
  same issues and exits non-zero on errors.
- **`cargo test`** loads every `catalogs/*.toml`. It asserts zero errors and zero
  warnings, except for warnings allowed per line in the pack itself
  (`allow = ["non_monotonic_gap"]`, each with a reason comment).
- **PDF and export disclosures** name the pack, version, trust level and source for each
  hinge. User-supplied data gets its own disclosure line in both languages.

## Status (2026-09-27)

Phases 1–6 are done. Phase 6 (2026-09-30) added `[[drawer_slides]]` and
`[[feet]]` tables, `catalogs/fgvtn-slides.toml` (5 FGVTN/TN families, reviewed in
`docs/catalogs/fgvtn-slides-review.md`), `catalogs/generic-feet.toml`
(status `generic`), slide installations with fit checks and hole references,
the app-written `user-models.toml`, and project format v4.

- **Packs and loader:** `src/catalog/catalog_pack.rs` (TOML, exact mm
  decimals, every problem with its path, `allow` with reasons, a registry of
  bundled + user packs, user packs shadowing bundled ids). Bundled:
  `catalogs/fgvtn.toml`. CLI: `plan-my-cabinet --check-catalog FILE...`.
- **Trust is derived, never stored:** `hardware_catalog::trust` is
  `Reviewed` while a pinned record equals a bundled reviewed record, and
  `UserSupplied` for any other coherent record (full measurements, labelled
  on screen and in the PDF). `hardware_catalog::facts` gives guidance only
  for internally consistent facts (`catalog_pack::facts_are_consistent`).
- **Project format v3:** `CatalogReference.origin`, `VerifiedHinge.arm`,
  `HingeInstallation.inset_depth`, all defaulting exactly for v1/v2 files.
  Saving an older file asks first (older releases cannot open v3).
- **FGVTN:** 7 sheets + the earlier Click 3D Slow kit, 8 families, 21 variants
  (6 inset). Review in `docs/catalogs/fgvtn-review.md`; two doubtful items
  are left out (Easy TN Curva K=3, MS Slow Inox Alta).
- **Inset geometry:** plate at front offset + E, K/F table, new
  `InstallationIssue::InsetShallowerThanDoor`, door-motion axis on the outside
  face. The hinge dialog asks for E when the chosen kit is inset.
- **UI:** "Add hinge from catalog…" opens the catalog browser (packs with
  status and problems, hinge facts, arm, test bench running the real
  `diagnose` via `hinge_installation::bench`, Reload / Import / Open folder).
  The pinned card shows trust, arm and pack. Update from catalog uses the
  loaded packs, including user ones.
- **Both PDFs** print the pack, the user-data label, the real plate height,
  K/R or K/F, and 37 + E for inset (`InstallationReferences::source_line` /
  `settings_line`, shared by both writers).

## Phases

Each phase ends green: `bintests.py`, `cargo test --lib --tests`, clippy and
`cargo fmt --check`.

1. **Pack format and loader, with no behaviour change.**
   - Add the `toml` dependency and `catalogs/`.
   - Write the Click 3D Slow data as the first bundled pack.
   - Build the loader, validator and registry, plus the `--check-catalog` CLI.
   - `builtin_hinge()` now reads from the registry. The tests prove the result equals
     today's constant.
2. **Generalized hinge facts and project migration.**
   - Add `HingeFacts`, arm types, `Trust` and `CatalogOrigin`.
   - Add the schema bump and the migration.
   - `diagnose`, `opening_limit`, export and `hinge_ui` read pinned facts.
   - `is_verified` and the Rust constants are removed.
3. **FGVTN pack.**
   - Transcribe the 7 families and 21 codes.
   - Do a visual review of each sheet (the same method as `hinge-source-review.md`),
     written up as `docs/catalogs/fgvtn-review.md`.
   - Resolve the ⚠ items.
   - Only then set `review.status = "reviewed"`.
   - Overlay variants give full guidance. Inset variants are reference-only.
4. **Catalogs panel, test bench, picker and user packs.**
   - Add the user catalog folder, import and reload, and "update available" diffs.
   - Add i18n in both languages.
   - Add docs `hardware-en.md` / `hardware-pt-BR.md`, including a "write your own pack"
     section.
5. **Inset geometry** (Alta).
   - Plate at 37+E, gap F, a new installation issue, and changes to the door-joint axis.
   - Inset variants then get full guidance.
6. **Later:** `[[drawer_slides]]` (lengths, load, side clearance, and the mounting hole
   pattern for the drilling sheet), using the same loader, registry, panel and pinning.

## Decisions (confirmed 2026-09-27)

1. **User packs get full measurements.** A user pack that validates cleanly produces
   numeric guidance.
   - The screen and the PDF show which facts came from the pack (the reference parts:
     pack, version, source, codes).
   - They also label user-supplied data as such.
2. **TOML.**
3. **FGVTN is bundled** in the app (facts and citations only; no PDFs).
4. **Inset (Alta) hinges are in scope.** Phase 5 (inset geometry) is part of this work,
   not deferred.
