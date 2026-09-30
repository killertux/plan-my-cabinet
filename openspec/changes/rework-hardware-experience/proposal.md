# Proposal

## Why

After 0.5.0 the Hardware workspace still reads as an afterthought:

- The only "+" lives in the Doors section, and its empty state talks about hinges even when you want a foot.
- Selecting a foot shows nothing in the inspector.
- Slides are read-only, and panel rows behave three different ways (one of them skips the unsaved-draft guard).
- The pinned catalog card assumes every model is a hinge.
- In 3D you can't click slides or hinges, hinges aren't drawn at all, and Move drags boards even in Hardware.

## What Changes

- **Panel by kind.** An "Add hardware" menu, then five always-visible sections: Doors & hinges, Drawers & slides, Feet & legs, Other hardware, Catalog models. Each has its own "+" and an empty state that says what to select. Catalog models replace the pinned hinge card.
- **Add, then edit.**
  - Add creates the item at once from the selection and sensible defaults: last foot model, last slide family, standard hinge set. Each add is one undo step, and the new item opens in the inspector.
  - A picker opens only when a choice is missing.
- **Editable inspectors in Hardware and Design.**
  - Feet and other hardware: model, parent, world position and rotation, and dimensions. There is also "Raise the cabinet" when a foot sits below the floor.
  - Slides: model, length, height and setback, plus Refit.
  - Doors: moving part, mount, hinges and opening limit, plus Reconfirm.
  - Catalog models: facts, usage, and Remove when unused.
  - Text fields follow the draft rules (Enter/Apply commit, Escape reverts, focus loss never commits), and leaving an unsaved draft asks first.
- **Viewport.**
  - Slides and hinges can be clicked.
  - Hinges are drawn as a cup and a plate. These are representational and never drilling geometry.
  - Move drags feet and other hardware, and only hardware while in Hardware. Dragging is off while a position draft is unsaved.
- New actions `RefitSlides` and `RemoveCatalog`, with the reason `CatalogInUse`. Edit actions open the inspector.
- Pictures list hinges in their legend.

## Capabilities

### Modified Capabilities

- `hardware-and-motion`: this change replaces the pinned catalog card and Browse access described in the pending `redesign-desktop-workspaces` requirement "Hardware workspace tree and catalog access". That requirement is not archived yet, so this delta adds requirements instead of modifying it. Archive `redesign-desktop-workspaces` first.

## Impact

- The project format is unchanged, and MCP tools and service transactions are unchanged.
- Screenshots of the Hardware workspace need a refresh.
