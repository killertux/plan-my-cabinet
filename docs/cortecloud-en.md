# Ordering parts through CorteCloud

[CorteCloud](https://cortecloud.com) lets a shop cut, band and drill your
parts from its own stock. Plan My Cabinet writes the part list CorteCloud
imports, so you don't retype it.

## Exporting

1. Open **Handoff** and choose **CorteCloud** under *Format*.
2. Under **Include in the file**, untick what the shop should not do:
   **Edge banding**, **Hinge holes** or **Drawer slide holes**. With all three
   off the file lists only the boards. A kind the design doesn't have is shown
   unticked. What you leave out here is not listed under *Left out*.
3. Check **What the file lists**: parts and lines (identical parts share a line
   with a quantity), banded parts and metres of band, drilled parts and holes,
   and the parts per material.
4. Read **Left out**. Drilling is sent only for hardware the workshop PDF
   would also guide:
   - hinges or slides with issues, and doors that need review, are left out
     (**Fix** takes you to them);
   - screw holes with no pilot size are left out. Catalogs rarely size the
     pilots of hinge plates and slides; tick **Ask the shop to drill screw
     pilots** and give a diameter and depth to include them.
5. Press **Export for CorteCloud…** and choose where to save. A file that
   exists is replaced only after you confirm.

The centre of Handoff shows the parts exactly as the file lists them. The
export needs only a valid design: sheets, the cut plan, kerf and prices are
not used, because the shop nests the parts on its own sheets. Exporting is not
an edit; the last export is remembered and Handoff tells you when the design
has changed since.

## Importing in CorteCloud

In CorteCloud: **Novo serviço › Serviço Completo › Carregar arquivo
Cortecloud**, pick the file, then link each material and band to the shop's
stock. Check the preview of the first parts you import: holes and bands should
be where Plan My Cabinet shows them.

## What goes in the file

| CorteCloud field | From the design |
|---|---|
| `c` × `l` | Finished length and width. `c` follows the grain; with no grain rule, the longer side. |
| `quantity` | How many identical parts: same name, cabinet, material, size, grain, banding and holes. |
| `function` | The board's name. |
| `complement` | The cabinet (top assembly) the board belongs to. |
| `material` | The material's name and thickness, e.g. "MDF Branco 18". |
| `c1` `c2` `l1` `l2` | The band on each side, by name, or empty. |
| `machining` | Face holes: hinge cups and, when sized, screw pilots, each from its nearest corner, with depth, diameter and through or not. Parts without holes have no machining. |

The face with most holes is the inner face (`i`). Edge holes, grooves and
rebates are not modelled yet, so the file has none.

## For agents

The MCP tools `get_part_list` and `export_design` do the same; see the agent
guide.
