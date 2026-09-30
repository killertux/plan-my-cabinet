# Design

## Layers

1. `machining::board_machining(project, options)`: face and edge drills per
   board, plus omissions. Edge drills exist as a type but nothing produces
   them yet (joinery will).
2. `part_list::build(project, options)`: validates the design, groups boards
   and attaches effective banding and drilling. `PartList::fingerprint`
   identifies what a file says.
3. `formats::render(format, project, options)` and `formats::write(...)`:
   the format module turns the part list into bytes; the shared
   `export::write_file` commits atomically and refuses to replace without
   consent.

Adding a format is one `ExportFormat` variant, one module under `formats/`
and its label; the Handoff cards, the `ExportFile` action and the MCP tool
dispatch on the enum.

## CorteCloud frame

CorteCloud's part frame, seen from the inner face: corner 0 top left, 1 bottom
left, 2 bottom right, 3 top right; C1 left long side, C2 right, L2 top, L1
bottom; X along C from the corner's L side, Y along L from its C side. With
`c` along the board's local X and the inner face at MaxZ, the view with X down
and Y right is the board's own right-handed frame; putting `c` along local Y
or the inner face at MinZ mirrors Y. `Orientation` holds this in one place,
with a table test. CorteCloud's own door example (cups at corners 2 and 3,
y 21.5) is reproduced by a golden test; the two example files round-trip
through the serde types.

The first real import is checked by the user (the Handoff note says so): if
CorteCloud shows holes mirrored, only `Orientation::mirrored` changes.

## Receipts

PDF receipts drive the PDF's freshness rules (wood and packet hashes, review
boundary). A part-list file has neither, so it is recorded in
`Project.file_exports` with the part-list fingerprint, and is current while
the design produces the same part list with the same options.
