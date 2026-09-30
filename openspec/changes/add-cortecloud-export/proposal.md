# Proposal

## Why

CorteCloud lets a shop cut, band and drill parts from its own stock. It
imports a JSON part list ("Carregar arquivo Cortecloud", Serviço Completo).
Typing the design into CorteCloud again is slow and error-prone, and more
formats will follow, so exports need a shared shape rather than another
one-off.

Depends on `add-edge-banding` (bands per side).

## What Changes

- **Format registry.** `ExportFormat` (workshop PDF, CorteCloud JSON) with id,
  extension, label, file name and gate. Part-list formats render from a
  format-neutral **part list**: boards grouped by name, cabinet, material,
  size, grain, banding and holes, with quantities.
- **Machining read model.** Face holes per board in board-local coordinates,
  from hinges and slides that pass the same checks as the PDF's guidance.
  Screw holes without a pilot size are left out and listed, unless the user
  gives a pilot size for the export. Slide holes may carry a catalog depth.
- **CorteCloud mapping.** `c` follows the grain; bands on C1, C2, L1, L2;
  holes from the nearest corner on the inner face (the face with most holes);
  no machining key without holes; sizes in millimetres matching the machining
  block.
- **Gate.** Only a valid design with boards; no review, sheets or kerf.
- **Handoff.** Format cards; for CorteCloud a summary, a Left out list with
  Fix links, the pilot option, import steps, the last export's freshness, a
  parts preview, and Export for CorteCloud… with overwrite confirmation.
- **Receipts.** Part-list exports are recorded apart from PDF receipts
  (`file_exports`), with a part-list fingerprint; undo keeps them.
- **MCP.** `get_part_list` and `export_design`.

## Impact

- New `output/formats` (+ `cortecloud`), `output/part_list`, `output/machining`; `export::write_file`; `Project.file_exports`.
- Desktop: `file_export_ui`, Handoff options/footer/preview, actions `SetExportFormat` and `ExportFile`.
- MCP: `service/exports.rs`.
