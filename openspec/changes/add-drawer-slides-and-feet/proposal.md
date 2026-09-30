# Proposal

## Why

Hinges are the only real hardware today. Drawers have a fixed "slide clearance" and no slides; feet are grey placeholder boxes. Makers need catalog drawer slides that are checked against the drawer and cabinet (gaps, depth, height) with hole positions for the shop, and feet that look like the product so they can judge the piece. An agent working through MCP must be able to use both and to create new models.

## What Changes

- Catalog packs gain `[[drawer_slides]]` and `[[feet]]` tables and a `generic` review status. Bundled: `fgvtn-slides.toml` (FGVTN/TN TT45, TT44, TT35, H45, TT90, reviewed against the manufacturer's sheets) and `generic-feet.toml` (plastic cones, chrome posts, straight legs, industrial frames).
- Project format v4: pinned slide and foot facts in `CatalogReference.item`; slide installations join a drawer (assembly) to two carcass sides; feet are catalog hardware items.
- Slide checks: side clearance with tolerance, depth, drawer member length, height, alignment; hole references on both boards; a display-only slide-out motion.
- Rendering: feet with the product's shape (frustums, posts with plate and glide, tube frames) and slides as three telescoping members, in the viewport, pictures and scene description.
- The Drawers template installs a slide pair per drawer (default TT45) and uses the slide's clearance.
- Shop PDFs: hardware purchase list, slide hole references, feet descriptions; manufacturing fingerprint v5.
- Desktop: Drawer slides list, dialog and inspector; Foot mode in the hardware dialog; drawer preview reuses the door preview; slide models in the catalog browser; slide choice in template setup.
- MCP: catalog listing, pinning, `create_slide_model` / `create_foot_model` (optionally saved to `user-models.toml`), `suggest_slides`, `add_slides`, `update_slide`, `remove_slide`, `list_slides`, `add_foot`, `update_foot`, `list_feet`, `render_drawer_opening`.

## Capabilities

### Modified Capabilities

- `hardware-and-motion`: drawer slides, catalog feet, generic packs, user models.
- `workshop-outputs`: purchase list and slide references; fingerprint v5.
- `project-foundation`: Drawers template slides.
- `agent-interface`: slide and foot tools.

## Impact

- `toml` gains its `display` feature (writing the user pack).
- Older app versions cannot open v4 projects; saving an older file asks first (existing behaviour).
