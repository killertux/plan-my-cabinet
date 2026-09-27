# Handoff: Plan My Cabinet — egui UI redesign

## Overview
This is a redesign of the Plan My Cabinet desktop UI (Rust, `eframe =0.36.2`, wgpu). Today the app has one long scrolling sidebar holding the project, settings, materials, stock, costs and optimization, next to the 3D viewport and 2D sheets. The redesign splits that into **workspaces**. Each one has a consistent shell: a workspace rail, a header with command search, three panes and a status bar.

The redesign changes no domain logic. Every control maps to an existing feature documented in `docs/*-en.md` (boards, stock, viewport, assembly, hardware, shop handoff, project/recovery). Items marked **[NEW]** are proposed features.

Repo: `killertux/plan-my-cabinet` (branch `master`).

## About the design files
The files in `designs/` are **design references built in HTML**. They show the intended look and behavior; they are not code to port. Recreate them in the existing Rust/egui app, using egui panels, `Frame`s, widgets and the custom painter (for the viewport and sheet canvas). Don't embed a webview.

To view them, open any `designs/*.dc.html` in a browser. `support.js` and `icons/` must stay beside them.

## Fidelity
**High fidelity.** Colors, type sizes, spacing, radii and copy are final. Match them in egui as closely as egui allows. Where egui can't match exactly (e.g. CSS box-shadow blur), use the closest egui `Shadow`.

**Chosen direction: `2a`** in `Main Window.dc.html` (top section). It uses the 1b layout with the 1a palette. `1a`, `1b` and `1c` in the same file are rejected explorations; ignore them except as context. The "Reference · current app" section at the bottom recreates today's UI.

---

## Design tokens

### Colors (light, warm)
| Token | Hex | Use |
|---|---|---|
| `bg_app` | `#F4F1EC` | window background, rail, input fill |
| `bg_panel` | `#FBFAF7` | side panels, header, status bar, floating toolbars |
| `bg_viewport` | `#ECE8E1` | 3D viewport / sheet canvas background; also inactive segmented track & secondary buttons |
| `bg_card` | `#FFFFFF` | cards inside panels, focused input |
| `border` | `#DDD7CD` | panel separators |
| `border_soft` | `#E6E0D6` | input and toolbar strokes, inner dividers |
| `border_strong` | `#D4CDC1` | HUD / sheet outlines |
| `text` | `#2A2520` | primary text; **also primary button fill** |
| `text_2` | `#3A332C` | list rows |
| `text_3` | `#5A5248` | secondary text, icon buttons |
| `muted` | `#6E655A` | labels |
| `faint` | `#8C8276` | section headers, units, IDs |
| `disabled` | `#B5AC9F` | separators "/", disabled |
| `accent_bg` | `#F6E3CB` | selected row / active rail item / active chip fill |
| `accent_ink` | `#5C3309` | text on accent_bg |
| `accent` | `#C9731F` | unsaved dot, selected icon tint, slider fill |
| `accent_dark` | `#8A4F0E` / `#9A5B12` | icon on accent_bg, links |
| `focus_stroke` | `#D9A45E` | focused input stroke (+ 3px ring in `accent_bg`) |
| `sel_fill` | `#F4C27A` / `#EDB05E` / `#D9963F` | selected board in 3D (top / front / side faces) |
| `ok` | `#3F7D4E` | check icons, "confirmed" |
| `ok_bg` | `#E4EEE3` (ink `#3F6B45`) | "owned" chip |
| `warn` | `#B7791F` | warning icon |
| `warn_ink` | `#8A5A12` / `#5C3E10` | warning text |
| `warn_bg` | `#FCF4E7` (stroke `#EBD2B0`) | warning callouts |
| `danger` | `#B4412F` | destructive text (Discard, Delete) |
| `kerf` | `#C4453A` | cut lines, X axis |
| `axis_y` / `axis_z` | `#4E9A57` / `#3E6FC4` | Y / Z axes and field prefixes |
| `snap_source` / `snap_target` | `#4FB8D6` / `#E0559F` | face-to-face highlight (keeps the existing cyan/pink meaning) |
| wood (white MDF) | `#F4F2EE` / `#E4E1DA` / `#D3CFC6`, edge `#8C8272` | 3D face shading top/front/side |
| wood (oak) | `#E2C59C` / `#CFAE80` / `#B8966A`, edge `#7A5A36` | |
| wood (HDF) | `#B69A78` / `#A48865` / `#8E7456` | |
| grid | `#DDD7CC`, major every 5th `#CFC7B9` | viewport grid |

### Typography
- **UI:** Noto Sans (already bundled at `assets/fonts/NotoSans-Regular.ttf`). Add a **Medium/SemiBold** weight (NotoSans-SemiBold.ttf, OFL) as a second family, e.g. `FontFamily::Name("semibold")`, because egui has no synthetic bold.
- **Numbers, IDs, dimensions, shortcuts:** JetBrains Mono Regular (OFL). Register it as `FontFamily::Monospace`.
- Sizes (egui points = CSS px here):

| Style | Size | Weight | Use |
|---|---|---|---|
| Title | 15 | semibold | inspector title, dialog title |
| Heading | 20 | semibold | page title (e.g. "Project stock") |
| Body | 13 | regular | default |
| Small | 12 / 11.5 | regular | hints, status bar (11.5) |
| Section | 10.5–11 | semibold, uppercase, +0.08em tracking | "OUTLINER", "DIMENSIONS" |
| Mono | 12–13 | regular | values; 11 for IDs |
| Rail label | 9.5 | regular (active: medium) | under rail icons |

egui has no letter-spacing, so for section headers write the text uppercase and add `extra_letter_spacing` via a `LayoutJob` if needed.

### Spacing, radii and strokes
- Base unit 2px. Common values: 2, 4, 6, 8, 10, 12, 14, 16, 18.
- Panel padding 14px horizontal, 12–16px vertical. Row heights: tree 26, property 28, table 40, toolbar buttons 28–34.
- Radii: inputs 6, buttons 7, cards/toolbars 9, dialogs 12, rail items 8, chips 9 (pill), segmented inner 5.
- Strokes are 1px. Focus is 1px `focus_stroke` plus a 3px outer ring in `accent_bg`: draw a second rect expanded 3px behind the widget.
- Shadows: floating toolbar/HUD `0 8 24 rgba(60,45,25,.16)`, dialog `0 18 44 rgba(60,45,25,.18)`. Use `egui::Shadow { offset:[0,8], blur:24, spread:0, color: Color32::from_rgba_unmultiplied(60,45,25,40) }`.

### Suggested theme setup (verify names against egui 0.36)
```rust
pub fn apply_theme(ctx: &egui::Context) {
    use egui::{Color32 as C, Stroke, CornerRadius};
    let hex = |s: &str| C::from_hex(s).unwrap();
    let mut v = egui::Visuals::light();
    v.panel_fill = hex("#FBFAF7");
    v.window_fill = hex("#FBFAF7");
    v.extreme_bg_color = hex("#F4F1EC");      // text edit background
    v.faint_bg_color = hex("#F4F1EC");        // striped rows
    v.window_stroke = Stroke::new(1.0, hex("#DDD7CD"));
    v.window_corner_radius = CornerRadius::same(12);
    v.selection.bg_fill = hex("#F6E3CB");
    v.selection.stroke = Stroke::new(1.0, hex("#8A4F0E"));
    v.hyperlink_color = hex("#9A5B12");
    v.warn_fg_color = hex("#B7791F");
    v.error_fg_color = hex("#B4412F");
    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0, hex("#E6E0D6"));
    w.noninteractive.fg_stroke = Stroke::new(1.0, hex("#2A2520"));
    w.inactive.weak_bg_fill = hex("#ECE8E1");  // button fill
    w.inactive.bg_fill = hex("#F4F1EC");
    w.inactive.bg_stroke = Stroke::new(1.0, hex("#E6E0D6"));
    w.inactive.fg_stroke = Stroke::new(1.0, hex("#3A332C"));
    w.hovered.weak_bg_fill = hex("#E4DFD6");
    w.hovered.bg_stroke = Stroke::new(1.0, hex("#D4CDC1"));
    w.active.weak_bg_fill = hex("#DDD7CD");
    w.open.weak_bg_fill = hex("#ECE8E1");
    for s in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        s.corner_radius = CornerRadius::same(6);
    }
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.interact_size.y = 28.0;
        s.spacing.window_margin = egui::Margin::same(0); // dialogs draw their own header/body/footer frames
    });
}
```
Keep the existing light theme only. Dark mode is out of scope; 1b was a dark exploration and was rejected.

---

## Shell (every workspace)
Everything below comes from `Main Window.dc.html#2a`. The design window is 1440×900.

1. **Workspace rail**: `SidePanel::left("rail")`, exact width 60, fill `bg_app`, right stroke `border`.
   - Top: logo tile 32×32, radius 8, fill `text`, icon `board` 18px tinted `#F4C27A`. 12px gap below it.
   - Items, each 48 wide, padding 7/5, radius 8, 19px icon above a 9.5px label: **Design** (`cube`), **Stock** (`material`) [NEW rail entry], **Cut plan** (`sheet`), **Hardware** (`hinge`), **Handoff** (`export`). Active: fill `accent_bg`, icon and text `#8A4F0E`, label medium. Inactive: `muted`.
   - A 7px `accent` dot at top-right of an item means it has open issues (Cut plan when a part is unallocated).
   - Bottom, 40×40 icon-only: `globe` (language), `sliders` (preferences).
2. **Header**: `TopBottomPanel::top`, height 46, fill `bg_panel`, bottom stroke `border`, padding 0 12 0 16.
   - Left: "Projects" (muted) / "Kitchen base 800" (semibold), then a 7px `accent` unsaved dot and "unsaved" (11.5, muted). Hide the dot when saved.
   - Center: **command search**, 440×30, fill `bg_app`, stroke `border_soft`, radius 7. Search icon 14, placeholder "Search commands, parts, materials…", a `⌘K` keycap (11 mono, 1px stroke, radius 4). It opens a command palette [NEW] listing every action and every board, material or stock piece by name/ID.
   - Right: undo / redo icon buttons 32×32 (redo at 40% alpha when unavailable), **Save** (secondary: fill `bg_viewport`, stroke `border_soft`), **Export** (primary: fill `text`, text `bg_panel`, semibold, `export` icon 15).
3. **Status bar**: `TopBottomPanel::bottom`, height 26, fill `bg_panel`, top stroke `border`, 11.5px muted, 18px gaps, no wrap. Content by workspace:
   - Always: kerf state (`check` + "Kerf 5 mm confirmed" in `ok`, or a warning if not confirmed).
   - The workspace's own facts (grid, units, counts).
   - Issues (`warning` + text in `warn_ink`).
   - Right-aligned: "Est. new spending **BRL 579,80** + cut fee unknown", with the value in mono `text`.
4. **Body**: a 3-column layout. Use `SidePanel::left` (256–300), `SidePanel::right` (292–316) and a `CentralPanel`. Widths are listed per screen below. Panel fills are `bg_panel`; the central area is `bg_viewport` or `bg_app`.

Section header pattern: height 36, padding 0 10 0 14, 11px semibold uppercase `muted`, optional trailing icon buttons (`search`, `plus`) at 14–15px. Separate sections with a 1px `border` line and a 12px top margin.

Segmented control: track fill `bg_viewport`, radius 7, padding 2. Selected segment: fill `bg_panel`, radius 5, a 1px shadow, text medium. Others: `text_3`. Build it with `ui.horizontal` plus custom-painted `selectable_label`s.

Input field: height 28–34, fill `bg_app`, stroke `border_soft`, radius 6, padding 0 8–10. Value in mono on the left, unit on the right in `faint` 12px. Focused: fill `bg_card`, stroke `focus_stroke`, 3px ring. Read-only/derived values (e.g. thickness from material) use a **dashed** `border_strong` stroke with no fill and a "material" suffix.

Axis-tagged fields: X/Y/Z prefix letter in `kerf`/`axis_y`/`axis_z` (11px semibold), or a 2px bottom stroke in the axis color (2a inspector).

Chips: pill, 11px, padding 1–2 / 7–8. To purchase: `accent_bg` / `#7A4410`. Owned: `ok_bg` / `#3F6B45`. Out of date: `warn_bg` / `#8A520A`. Draft: `bg_viewport` / `text_3`.

---

## Screens

### 1. Design workspace (`Main Window.dc.html#2a`)
Columns: left 256, center fluid, right 292.

- **Left panel**
  - **OUTLINER**: the assembly tree (existing "Objects · hierarchy"). Rows 26 high, radius 5; children indented 28. Each row: 13px `board` icon (or 14px `assembly` for groups, with an 11px chevron), name, and a trailing visibility icon (`eye` tinted `#C9C1B4`; `eyeoff` `muted` when hidden). Hidden groups show their name in `faint`. A row with an allocation issue shows `warning` in `warn` instead of the eye.
    - The **active** board uses fill `accent_bg`, text `accent_ink` medium, icon `accent`. This replaces the old `*` prefix.
    - Multi-selected rows: fill `#FBF1E3`. Shift/⌘ behavior is unchanged.
    - "Hinges (4)" is a collapsed group row.
  - **MATERIALS**: rows 28 high, 14px color swatch (radius 3), name, and a right-aligned mono "18 · 6" (thickness · board count).
  - **STOCK**: rows with a mono ID (`S1` in `accent` when it holds the selection), mono dimensions, and "to buy"/"owned" in `faint`.
- **Center: 3D viewport** (the existing wgpu scene; restyle only)
  - Clear color `bg_viewport`, grid `#DDD7CC` with every 5th line `#CFC7B9`, axes red/green/blue at 70% alpha, soft floor shadow under the model.
  - Board fill uses the material's display color, with face shading of 1.0 / 0.93 / 0.86 for top/front/side and a 1px darker edge. The selected board uses `sel_fill` with a 1.6px `#8A4F0E` edge. Other multi-selected boards: keep cyan.
  - Dimension label on the selected board: a dark `text` pill with `bg_panel` mono text "764 × 537 × 18".
  - **Floating tool strip**, left, vertically centered: `bg_panel` frame, radius 10, padding 4. 34×34 buttons: Navigate (`orbit`, active: fill `text`, icon `bg_panel`), Move board (`move`), Measure (`measure`) [NEW tool mode for bounding dimensions], a divider, then Frame selection (`frame`).
  - **Top center**: segmented Iso | Front | Right | Top, and a second one Persp | Ortho.
  - **Top right**: snap chip with `magnet` 15px in `accent`, "Faces + grid 10 mm". Clicking it opens a popover to toggle face snap, grid snap and the grid spacing. This replaces "Edit grid spacing".
  - **Bottom center HUD** (only when one board is selected): `bg_panel`, 1px `border_strong`, radius 11, shadow. It holds the board name (semibold `accent_ink`), three inline mono dimension fields (L focused, W, T read-only) with "×" separators and "mm", a divider, and 30×30 icon buttons: Place face to face (`place`), Duplicate (`duplicate`), Hide (`eyeoff`), Delete (`trash` in `danger`).
  - The old Orbit/Pan/Zoom buttons are gone. Keyboard (arrows, +/−) and a Help → Shortcuts sheet replace them. The drag hint moves into the status bar ("Move: drag a board · Alt bypasses snaps · Esc cancels") while Move mode is active.
- **Right panel: inspector** for the active selection
  - Header: 15px icon, name 15 semibold, mono ID right-aligned; sub-line "Board in Carcass · allocated on S1". Padding 14/12, bottom stroke.
  - Sections use label/value rows (label column 88px, row gap 4, rows 28 high):
    - **BOARD**: Material dropdown (swatch + name) and Grain dropdown ("Material · length" / Length / Width / Any).
    - **DIMENSIONS**: Length, Width, Thickness (derived, dashed), Anchor segmented Start | Centre | End. This replaces the Edit dimensions dialog for a single board. Multi-select opens the Resize dialog.
    - **TRANSFORM**: frame dropdown "World ▾" / "Local · <parent>"; Position as 3 mono fields with axis-colored bottom strokes; Rotation as 3 fields in °.
    - **STOCK**: a sheet preview card (below), then "Sheet S1 at 0, 565" and "Placement: 🔒 Unlocked".
    - **MEASURE**: "Bounding 764 × 537 × 18".
  - **Sheet preview card** (moved here from the viewport, per the review): white card, 1px `border_soft`, radius 9, padding 10, margin 0 14 10. Title row "S1 · MDF White" with a mono utilization "71%". Below it, the sheet drawn at aspect 2750:1830: other parts in `#E2DBCF` with a `#BFB5A5` stroke, the selected part in `#F4C27A` with a 1.5px `#9A5B12` stroke. Clicking it opens the Cut plan focused on that part.

### 2. Cut plan (`Cut Plan.dc.html`)
Columns: left 256, center fluid (sheet canvas), right 308. This replaces the "2D stock sheets", "Edit sheet / Repair session", "Optimize stock layout" and "Allocation issues" blocks.

- **Left panel**
  - **SHEETS · PRIORITY ORDER**: cards (padding 8, radius 8). Each has a 60×40 thumbnail, a mono ID plus material name, a mono "2750×1830 · 6 parts", and a 4px utilization bar with a %. The selected card uses `accent_bg`. Owned offcuts show an "owned" chip; unused pieces show a dashed thumbnail.
  - **NEEDS STOCK**: an issue card (white, `#EBD2B0` stroke) per unallocated or conflicted board. It shows the name and mono dims, the reason (existing diagnostic text), and actions: primary "Add HDF sheet" (pre-fills material and thickness) and secondary "Reveal" (existing Select and reveal).
  - Footer: "+ Sheet or offcut".
- **Center: sheet canvas** (egui `Painter`)
  - Toolbar (height 52): a sheet switcher `‹ S1 MDF White 18 · 2750 × 1830 ›`; mode segmented **View** | **Repair** (Repair = the existing repair session); toggles Cuts / Offcuts / Grain / IDs; "Fit".
  - The sheet is scaled to fit, with 24px padding. Sheet fill `#F7F5F0`, 1px `#BFB5A5` outline and the panel shadow. Dimension rulers above (2750) and to the left (1830) are mono 11 `muted` with 1px lines.
  - Parts: fill `#E9E3D7`, 1px `#9C907E`, name (12.5 medium) plus mono dims centered. If the drawn height is under 40px, put name and dims on **one line**.
  - Selected part: fill `#F4C27A`, 2px `#8A4F0E` stroke, name, dims, ID, and a small grain glyph at bottom-right.
  - Reusable offcuts: 135° hatch (`#EFEBE3` with a 1px `#E7E2D8` line every 7px) and a mono size label on a `#F7F5F0` pill.
  - Kerf: a filled band of true kerf width, `kerf` at 85% alpha, minimum 1.6px.
  - Cut markers: 22px circles, fill `bg_panel`, 1.5px `kerf` stroke, 10px mono semibold `#A5362C` text "C1…C9". Place each near the start of its cut; if it would overlap a part label, put it outside the part.
  - Legend, bottom-left: Part · Reusable offcut · Kerf 5 mm · Conflict. Conflict style: fill `#FBE3E0`, dashed `kerf` stroke. It replaces the red/crossed shapes; keep the recorded positions.
  - Repair mode: drag unlocked parts; draw a staged ghost with feasibility color; toolbar shows "Accept repair" / "Cancel repair" (Esc).
- **Right panel**
  - Header: "Sheet S1" + ownership chip; sub-line "MDF White 18 · grain along X · no trims · BRL 289,90".
  - A 2×2 stat grid (cards `bg_app`, radius 7): Utilization, Physical cuts, Reusable offcuts (m²), Kerf loss (m²).
  - **CUT SEQUENCE** + "✓ Verified full-span" (`ok`). One row per cut: a 24×20 badge (fill `#FBE9E6`, text `#A5362C`), the operation ("Rip full sheet at Y 560"), and mono details ("P0 → P1 2750×560 · P2 2750×1265"). Hovering a row highlights its cut on the canvas.
  - **OPTIMIZE ALL SHEETS**: objective dropdown (Lowest new spending / Fewest physical cuts / Least unused stock area), primary "⚡ Search", status "current = best found", and a disclaimer (11px `faint`). While searching, the button becomes "Cancel search". Results open a compare view (current vs best found) with "Accept best found".

### 3. Stock & materials (`Stock and Materials.dc.html`) [NEW workspace]
Columns: left 256, center fluid (`bg_app`), right 316.

- **Left panel**
  - **MATERIALS**: rows with a 10×28 swatch bar, name, and mono "18 mm · 6 boards · 3 pcs". A material with boards but no stock shows `warning`. "All stock" at the top clears the filter.
  - Below the list, a summary of the selected material (thickness, default grain, used by) with "Edit material…", which opens the existing preserve/apply dialog.
- **Center**
  - Page title "Project stock" (20 semibold) with the sub-line "Sheets and offcuts you own or plan to buy. First-fit tries them top to bottom."
  - Top right: a "Cut fee **unknown** Set" chip and the primary "+ Add sheet or offcut".
  - The table is a card (`bg_panel`, radius 10). Use `egui_extras::TableBuilder` with columns 30 (drag handle `⋮⋮`) / 40 (#) / 60 (ID) / fluid (Measured L × W × T) / 84 (Grain) / 60 (Trims) / 108 (Ownership chip) / 84 (Price, right-aligned) / 110 (On plan: bar + "6 parts" / "unused" / "spare"). Row height 40.
  - Material group header rows are 30 high, fill `bg_app`. Selected row: `accent_bg`. Dragging a row reorders priority; this replaces Move up/down.
  - A material with no stock gets an inline `warn_bg` row with its reason and "+ Add HDF sheet".
  - Three summary cards below: To purchase (used), Cutting (e.g. "12 cuts × ?"), Owned pieces consumed. Then a disclaimer line.
- **Right panel**: inspector for the selected piece. MEASURED SIZE (L/W/T); GRAIN segmented Along X | Along Y | None | Unknown; EDGE TRIMS as 4 fields laid out around a hatched "usable W × H" rectangle; COST (Owned | To purchase + price "BRL / piece", blank = unknown); an "On the plan" card with the parts list and "Open in cut plan →".

### 4. Dialogs (`Dialogs.dc.html`)
Every modal uses one `egui::Window` pattern: `.collapsible(false).resizable(false).title_bar(false)`, a centered anchor, and a dimmed backdrop (`rgba(42,37,32,.25)`). Width 400–520. It has three internal frames:

- **Header**: padding 16/18, bottom stroke. A 34×34 icon tile (fill `accent_bg`, icon `#9A5B12`), title 15 semibold, one-line context in 12px `muted`, and an `Esc` keycap at right.
- **Body**: padding 16/18, 14px vertical gap between field groups. Labels are 12px `muted` above the fields.
- **Footer**: padding 12/18, fill `bg_app`, top stroke. On the left, an 11.5px `faint` hint. On the right, "Cancel" (secondary) and a primary button **named for the action**, with a `⏎` hint at 60% alpha.

Focus starts in the first field; Tab stays inside the dialog; Esc cancels; Enter confirms when the form is valid.

The six dialogs:
1. **New board**: Name, Material (swatch + thickness), Length · X, Width · Y, Thickness (derived, dashed), Grain segmented. A live parse line under any unit expression (`= 539.750 mm exact` in `ok`). A preview strip shows the dims and the first-fit result ("Fits S1 at X 1538 · Y 565").
2. **Position** (Numeric pose): frame segmented World | Local · <parent>; Position XYZ; Rotation XYZ; quick presets "Stand up" / "Lay flat" / "Turn 90° Z" [NEW]; a derived-extents readout. Primary "Apply". Previews live.
3. **Place face to face**: Source face → Target board and face (each with a cyan/pink swatch); per-axis Start | Centre | End with an offset; gap between faces with a result line ("Shelf bottom lands at Z 350"). Primary "Place".
4. **Resize N boards** (batch edit and rounding): Axis and Keep-fixed segmented; value field with "current: mixed (…)"; a rounding callout in `warn_bg` with a required checkbox "Use the rounded value" (typing a different value clears it); a per-board before → after list. Primary "Resize 3 boards".
5. **New material**: Name, Thickness, Default grain; Display colour swatches [NEW: viewport tint only].
6. **Unsaved changes**: a compact alert ("Save changes to <name>?"). "Discard" (text, `danger`) sits left, then Cancel and primary "Save ⌘S".

Reuse the same pattern for all other existing dialogs: grid spacing, kerf (with its confirm checkbox), cut fee, stock piece, group/reparent, duplicate assembly, hardware, door relationship, delete confirmation, and overwrite confirmation.

### 5. Shop handoff (`Shop Handoff.dc.html`)
Columns: left 300, center fluid (fill `#E2DDD4`), right 300.

- **Left panel**
  - **PACKET TYPE**: two radio cards. Draft (selected: 1.5px `text` stroke, white fill). Shop-ready (disabled: `bg_app` fill with a `lock` icon), with its checklist inline: ✓ Kerf confirmed, ✓ Cut sequence verified, ⚠ "Back panel unallocated" + a **Fix** link that jumps to Cut plan › Needs stock.
  - **PDF OUTPUT**: Language dropdown, Units segmented mm | cm | in, Include checkboxes (Parts list & costs, Sheet diagrams + cut steps, Hinge references), and an info note "Interface stays in English…".
  - Footer: primary full-width "Export draft PDF…" (height 38; the label changes with the packet type) and a hint "use a new file name for every revision".
- **Center**: a preview toolbar ("Preview · 4 pages · A4", zoom − 86% +), a vertical 62×88 page thumbnail strip, and the current page rendered at A4 aspect (520×735 at 86%) on white with a shadow. Page 1 content follows the existing PDF export: title block, a DRAFT stamp (1.5px `kerf` border), an issues box, a Sheets table, a Parts table (identical parts combined with their ID list), the estimate, and the disclaimers. The preview must be generated from the same data as `pdf_export.rs`.
- **Right panel**: **EXPORT HISTORY** receipt cards. Filename, an "out of date" chip, "Shop-ready · pt-BR · mm · date", a "Since then: …" diff line, and a mono hash. Older receipts are muted with "superseded". Then **BEFORE YOU SEND** guidance, and a footer note "Save the project after exporting to keep the receipt."

### 6. Hardware (`Hardware.dc.html`)
Columns: left 268, center viewport, right 316.

- **Left panel**
  - **PINNED CATALOG**: a card with the kit name, mono SKUs, "H=0 · door 15–22 mm", and the revision. "Browse" opens the catalog snapshots, which replaces the old collapsing header.
  - **DOORS**: each door relationship is a group row (`door` icon, "on Left side"); its hinges are child rows with a colored dot and a mono "Y 100". A hinge with a warning shows `warning`, and its warning text sits in a callout below the list.
  - Footer: "+ Hinge", "+ Door".
- **Center: viewport**
  - Top-left segmented Motion preview | Closed. Top-right Iso | Front | Top.
  - The hinge axis is drawn as a dashed `snap_target` pink line. Cups are 9px dots (the selected one in `accent`), linked to their plates by dashed lines, with a dark label pill "H1".
  - Bottom HUD (460 wide): door name, mono angle "60°", a slider 0–105° (track `border_soft`, fill `accent`, 18px knob with a 2px `accent` stroke), tick labels 0/45/90/105, and the text "Display only… the saved pose stays closed." Leaving the workspace or choosing Closed restores the closed pose.
- **Right panel**: inspector for one installation.
  - Header "Hinge 1" + "hw-… · Door left → Left side".
  - MOUNTING: Door + reference edge, Cabinet board + face, Position Y (mm from Y− edge).
  - SETBACK K / OVERLAY R segmented 3/15 | 4/16 | 5/17 | 6/18.
  - A reference card: a small painted cup/plate diagram plus a label/value grid (Cup Ø35 × 11.3, Cup ctr · edge 21.5, Plate holes 32 apart, Plate · front 37, Door thk 18 ✓).
  - A `warn_bg` callout "Fastener drilling not available…".
  - Footer with the source and a "Source review" link.

### 7. Welcome & recovery (`Welcome.dc.html`)
Shown at launch or when no project is open. Window 1100×700.

- **Left column** (340, `bg_panel`, padding 28/24): logo 42×42, the app name 17 semibold, and the mono version. Primary "New project ⌘N" (height 40) and secondary "Open project… ⌘O". **START FROM A TEMPLATE** [NEW]: Base / Wall / Drawers tiles. Bottom: Language dropdown and Preferences ⌘,.
- **Right column** (padding 24/28)
  - **Recovery card**: shown when a valid snapshot is newer than the saved file. White, `#EBD2B0` stroke, radius 12. Title "Unsaved work found for <project>", then a saved vs recovery revision comparison (two tiles, the recovery tile tinted `warn_bg`). Buttons: primary **Recover edits**, secondary **Decide later** (= Defer), text **Discard snapshot** in `danger`, and a note "Recovered work stays unsaved until you save". This maps to the existing Recover / Discard / Defer.
  - **Recent projects**: a list card. Each row has a 64×46 thumbnail (render a small offscreen viewport capture on save [NEW]; show a hatched placeholder until then), the name, the mono path (ellipsized), stats, a status chip (Recovery available / Shop-ready sent / Draft) and a relative date. A missing file shows a dashed thumbnail and "Locate…" / "Remove".

### 8. Settings (`Settings.dc.html`)
A modal window, 780×560, using the dialog pattern (no header; Done in the footer). It opens from the rail's `sliders` icon or ⌘,.

- **Left section list** (210, fill `bg_app`, right stroke `border_soft`, padding 16/10).
  - A **PROJECT** group label with the project name (11.5 `muted`, ellipsized) under it, then Cutting (`cut`), Grid & units (`grid`), Costs & currency (`sheet`).
  - An **APP** group: General (`globe`), Shortcuts (`command`), About (`list`).
  - Rows are 32 high, radius 7, with a 15px icon. The active row uses fill `accent_bg`, text `accent_ink`. A 7px `accent` dot marks a section with an open issue (e.g. cut fee unknown).
- **Content** (padding 22/26). Title 17 semibold with a 12.5 `muted` description. Below it, a 2-column grid: label column 150 (label 13 medium + 11.5 `faint` hint), then controls. Separate groups with 1px `#EDE8E0` rules.
  - **Cutting**: kerf field (140 wide) + "✓ Confirmed with shop · <date>" (`ok`), a white card with the confirmation checkbox and its explanation (changing the kerf clears it and re-checks placements), and a read-only "Cutting model" bullet list with a "Worked examples →" link.
  - **Grid & units** (not drawn): grid spacing field (replaces "Edit grid spacing") and a display-unit segmented mm | cm | in.
  - **Costs & currency**: fee-per-cut field. Blank shows the placeholder "unknown" in `disabled` with the "BRL" suffix; a "Free (0)" quick chip sets a known zero. A currency field with "Change…", which opens the existing replace-prices/relabel flow. A read-only estimate card in `warn_bg` when incomplete.
  - **General**: interface language segmented English | Português (BR); Recovery info with "Show recovery folder" / "Clear old snapshots…"; Viewport toggles (navigation hints in the status bar, invert scroll-to-zoom [NEW], tint by material colour [NEW]); interface scale 90/100/115/130% [NEW, maps to `ctx.set_zoom_factor`].
  - Toggle switch: 32×18 pill, on = fill `text`, off = `border`, with a 14px `bg_panel` knob.
- **Footer**: hint "Project changes apply immediately and can be undone" (or "App settings save automatically") and a primary "Done Esc".
- Project edits commit through the existing command/undo system, one edit per change. App settings live in the platform config dir and never go into the `.pmcab`.

---

## Interactions & behavior
- **Selection is shared** across the Outliner, 3D viewport, sheet canvas, stock table and command palette (the existing linked-selection behavior). Switching workspace keeps the selection.
- **Workspace switching**: rail click or ⌘1–⌘5 [NEW]. Each workspace remembers its panel scroll positions.
- **Command palette** [NEW]: ⌘K opens a centered 560-wide window with a search field and grouped results (Actions / Boards / Materials / Stock). ↑↓ moves, ⏎ runs, Esc closes. Actions include every existing button (New board, Group selection, Frame selection, Start optimization, Export…).
- **Hover**: buttons use `hovered.weak_bg_fill` `#E4DFD6`; list rows use a `#F4F1EC` fill; icon buttons get a hover fill `bg_viewport` at radius 6–7.
- **Disabled**: 40% alpha (e.g. Redo). Locked options (Shop-ready) keep full-contrast text in `faint`, add a `lock` icon, and explain why inline.
- **Validation**: invalid numeric input gets a red 1px stroke (`#B4412F`) and an inline 11.5px message under the field; the primary button is disabled. Rounding needs the explicit checkbox (as today).
- **Stale states**: the export receipt shows "out of date"; the optimizer result shows "stale — search again" in `warn_ink`.
- There are no animations beyond egui defaults. The door slider updates the preview live.

## State (additions to the existing app state)
- `workspace: enum { Design, Stock, CutPlan, Hardware, Handoff }`
- `command_palette: Option<PaletteState { query, selected_index }>`
- `cut_plan_view: { sheet_id, mode: View|Repair, show_cuts, show_offcuts, show_grain, show_ids, zoom }`
- `stock_filter: Option<MaterialId>`
- `door_preview: Option<{ relationship_id, angle_deg }>` (display only, never saved)
- `material.display_color: Option<Color32>` [NEW, saved in `.pmcab`; needs a format-version decision]
- Everything else (selection, dialogs, repair session, optimization worker, receipts) already exists. Keep the current logic and restyle it.

## Suggested code organization
- `src/theme.rs`: `apply_theme`, color constants, font registration, and helpers `section_header`, `segmented`, `unit_field`, `chip`, `icon_button`, `primary_button`, `card_frame`.
- `src/icons.rs`: load the SVGs with `egui_extras` (`features = ["svg"]`) via `egui_extras::install_image_loaders` + `include_image!`. Tint with `Image::tint(color)`: the SVGs are black strokes, so tinting recolors them. Sizes 11–19px.
- `src/shell.rs`: rail, header, status bar, command palette.
- `src/workspaces/{design,stock,cut_plan,hardware,handoff}.rs`: split the current `main.rs` sidebar into these, reusing the logic in `stock_ui.rs`, `sheet_ui.rs`, `optimization_ui.rs`, `hinge_ui.rs`, `door_joint_ui.rs`, `hardware_ui.rs`, `project_ui.rs` and `placement_ui.rs`.
- Add all new strings to the existing Fluent bundles (en + pt-BR) through `i18n.rs`. The copy in the designs is the English source.

## Assets
- `assets/icons/*.svg`: 40 original stroke icons (24×24 viewBox, stroke 1.7, round caps and joins, black; tint at runtime) created for this redesign. They are `orbit, move, measure, frame, board, assembly, material, sheet, cut, hinge, export, undo, redo, eye, eyeoff, lock, plus, grid, magnet, search, sliders, warning, check, chevdown, chevright, duplicate, trash, save, folder, place, axes, bolt, layers, globe, door, cube, dots, list, command, grain`.
- Fonts: Noto Sans Regular (already in the repo); add Noto Sans SemiBold and JetBrains Mono Regular (both OFL, bundle with license files in `assets/fonts/`).
- No raster images. Project thumbnails on Welcome are generated at runtime [NEW].

## Files in this bundle
- `designs/Main Window.dc.html`: the Design workspace. **Implement section 2a.** 1a/1b/1c are explorations; the bottom section is the current UI for reference.
- `designs/Cut Plan.dc.html`: the Cut plan workspace.
- `designs/Stock and Materials.dc.html`: the Stock workspace.
- `designs/Dialogs.dc.html`: the six dialog patterns.
- `designs/Shop Handoff.dc.html`: the Handoff workspace.
- `designs/Hardware.dc.html`: the Hardware workspace.
- `designs/Welcome.dc.html`: Welcome and recovery.
- `designs/Settings.dc.html`: the Settings window (Cutting, Costs & currency, General).
- `designs/support.js`, `designs/icons/`: needed to open the HTML files in a browser.
- `assets/icons/`: the icon set to copy into the repo.

## Screenshots
Static captures of each screen at 1×, in `screenshots/`. They're for quick reference; the HTML files are the source of truth for exact values.
- `01-design-workspace-2a.png`: Design workspace (chosen 2a)
- `02-cut-plan.png`, `03-stock-and-materials.png`, `04-dialogs.png`, `05-shop-handoff.png`, `06-hardware.png`, `07-welcome.png`
- `08-settings-cutting.png`, `09-settings-costs.png`, `10-settings-general.png`

## Suggested implementation order
1. Theme, fonts, icons and shared widgets (`theme.rs`, `icons.rs`).
2. The shell: rail, header, status bar, and moving the existing sidebar content into workspaces unchanged.
3. The Design workspace layout (Outliner, inspector, viewport overlays).
4. The Cut plan canvas and cut sequence.
5. The Stock workspace table.
6. The dialog pattern applied to all existing dialogs.
7. Handoff, Hardware and Welcome.
8. The command palette.
9. The [NEW] features: Measure tool, display colors, pose presets, templates, thumbnails.
