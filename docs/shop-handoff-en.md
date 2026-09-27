# Shop handoff and exported revisions

The Handoff rail opens a live paginated A4 preview beside its packet controls
and separate export history. Select any page thumbnail, use Previous/Next or
zoom without changing the six-page example's pagination. The preview and PDF
consume the same frozen, positioned document and bundled fonts; neither an
internet font service nor an external PDF viewer is needed by the app. At
small preview zoom, inspect the PDF at print size before sending it—screenshots
of the thumbnail column are not cutting instructions.

The page initially fits the available preview area, including after resizing
or changing interface scale. Adjusting Zoom switches to manual magnification;
use **Fit page** to show the entire page again. Enlarged pages scroll independently
of the thumbnail strip. Fitting and zooming do not alter PDF dimensions or content.

## Review boundary and optional sections

The Handoff readiness card gives issue-specific **Fix in workspace** routes.
An unallocated hidden board still blocks Shop-ready; a missing price or cut
fee makes the estimate incomplete, but does not by itself invalidate a verified
wood plan. Parts list & costs, Sheet diagrams + cut steps, and Hinge references
all begin included. Switching any off removes only optional detail: the packet
still identifies the project, assumptions, omissions, unresolved wood and
hardware issues, incomplete estimates and safety limitations. Diagrams and
their cut steps are one indivisible choice. A4 page thumbnails and page count
come from the prepared document; page navigation and zoom do not repaginate it.

After the project source or *any* output choice (packet mode, language, units,
sections) changes, wait for the refreshed pages and select **Review this
preview** explicitly. Export stays unavailable until that exact packet has
been acknowledged. A save-picker result returned after a source/control change
cannot write an old or silently regenerated packet. Cancelling the picker
retains an otherwise current review; overwrite refusal or write failure
creates no success receipt. A successful receipt records its actual mode,
sections, language, units, filename, completion time and hash. Historical
cards distinguish an older packet's supersession from its content freshness;
**Since then** details require recorded comparison evidence. Legacy receipts
with no mode, date or comparison baseline label those values unavailable,
not guessed. A material-color-only edit can make the project unsaved without
staling manufacturing output.

1. Finish the assembly and identify every physical part by its name **and individual ID**. Duplicate shelves have separate IDs even when the parts list combines their dimensions as a quantity; match IDs between the object list, sheet placement and PDF part key. Allocate hidden boards too. Review material, measured stock thickness and grain, ownership, trims and prices. Confirm the shop's actual blade kerf in the project after setting it; changing kerf clears confirmation. Check the shop's trim practice, full-span cut capability, no-stacking assumption, workholding and flat fee per physical cut. The model does not certify a safe or practical cutting procedure.
2. Explicitly choose **Draft** for an unresolved review copy or **Shop-ready** for cutting. Draft pages say **DRAFT / NOT FOR CUTTING** and list outstanding issues. Shop-ready requires current confirmed kerf, compatible allocations for *all* boards and a verified full-span cut sequence for each used sheet. Resolve unallocated/conflicted/search-budget issues before retrying; missing prices or cut fee disclose an incomplete estimate but do not block feasible cuts. A blank price/rate is **unknown**, never zero; entered `0` is a known zero. Costs exclude taxes, delivery, setup and stacking discounts.
3. Choose the **PDF's language and units separately** from the interface language and project display unit; check its currency and converted dimensions. User-written names and IDs are not translated. Review each stock summary, finished rectangular blank dimension, grain arrow, trim and numbered cut **C1, C2…**. In the sheet legend **P0, P1…** denote intermediate/input/output pieces, not board IDs; each finished part maps back to its full board ID. Follow operations in number order: take the named input P-piece, use the stated X-/X+ or Y-/Y+ reference edge, measure the retained extent, keep the named output on the stated side and allow the separately stated kerf on its indicated side. A cut position is not the finished part size. Count one physical pass per split, including trims. Confirm physical order, blade side, grain and handling with the shop. Diagrams declare scale and are **not cutting templates**: review numerical instructions and diagrams manually, never measure the print as a template.
4. Pick a destination. Cancel leaves no PDF or success receipt; an existing destination needs explicit overwrite confirmation. For a revision sent to a shop, prefer a new filename so the previous packet remains available. After the completed write, the receipt stores project ID, document revision, wood/packet fingerprints, language, units, path, timestamp and PDF hash. A failed write does not create a successful receipt. Save the `.pmcab` project to persist the receipt.
5. After changing a label, dimension, stock, kerf, allocation or price, check **out of date** status against the last receipt; changing included hardware makes the packet stale separately from wood feasibility. A different document revision alone (for example an editing-grid change) does not stale the manufacturing fingerprint; camera and visibility also do not. Subsequent edits leave the exported PDF untouched: do not send it as the current plan. Resolve new allocation conflicts, reconfirm a changed kerf, prepare a fresh shop-ready snapshot and export to a new path; record and save the new receipt. Changing PDF language/units for another packet is an explicit new export, not an automatic rewrite of the previous file.

For an assembly example made from individual boards rather than a cabinet preset, see [Assemblies and hierarchy](assembly-en.md). For first-fit, kerf and estimate details, see [Project stock](stock-en.md).

**Confirming shop kerf:** the confirmation action in Handoff, Settings → Cutting,
or the command palette opens a review dialog; opening it does not confirm anything.
Check the displayed exact value with your shop, tick the acknowledgement, then
choose **Confirm kerf**. Enter on the checkbox only toggles it. Cancel or Escape
leaves the project untouched, and a changed project requires reopening the review.
Confirmation records today's date for that value as one undoable edit; it does
not certify a plan or cutting safety. Changing the kerf clears that confirmation.
