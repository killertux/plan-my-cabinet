# Shared document layout (tasks 11.1–11.4/11.6 backend)

`src/document_layout.rs` builds deterministic, renderer-independent A4 pages in
physical millimetres with a top-left origin. `DocumentBuilder` takes localized
strings from a frozen snapshot (`PageContext`), emits mandatory notices, and
flows paragraphs and whole table rows. `section` and `table` headings repeat on
continuations; project/revision/packet context, draft marking and the safety
footer repeat on every page. `diagram` reserves physical space and `box_at`,
`path`, `label_at`, and `link` allow witness-derived geometry and annotations. The
page model contains text, notices, styled paths/boxes and links. Each `TextRun` has
per-character shaped baseline origins and advances plus measured bounds; the
renderers must consume these positions rather than independently wrapping.

`FontMetrics` creates a headless egui font context with the same bundled Noto
Sans and JetBrains Mono static faces used by the native theme. It invokes
egui's harfrust-backed layout at a fixed 1 pixel per point, then converts
typographic points to millimetres (25.4/72). This is independent of preview
zoom and OS-installed fonts. The six faces are embedded at compile time.
Whitespace wraps first; oversized identifiers split by Unicode scalar so no
name or ID is silently elided. Unsupported glyphs and impossible geometry
produce an error. Text bounds and row/page reservations are checked before
placement. `LAYOUT_VERSION` and `FONT_METRICS_VERSION` are explicit inputs for
the prepared-document cache key; update them when rules or fonts change.

## Frozen workshop cover and lists (task 11.2)

`src/workshop_document.rs::build_workshop_document(prepared, sections)` takes
`PreparedExport` and an **explicit** `ReceiptSections` selection. It never
reads a live editor. It adds a cover with project UUID/revision, draft or
shop-ready mode, output language/units/currency and actual kerf, cutting
assumptions, material/stock scope, and the not-a-cutting-template footer.
Prepared wood and hardware issues, unknown fee/used-stock price, withheld
guidance, approximate-hinge warnings, incomplete estimates and optional-section
omissions are notices regardless of toggles; draft stamps and the safety footer
repeat on every page. The cover is positioned before the issue list, which can
flow onto subsequent pages. Hidden boards are included because preparation
diagnoses all boards, independent of viewport visibility.

With Parts list & costs on, material and stock tables include effective stock
thickness, source, grain, price and edge trims. Finished wood parts group by
name, material identity, length/width/thickness and **effective** grain;
quantity rows are followed by every board UUID and its allocation UUID plus
stock alias/UUID, or an unallocated label. Hardware is never counted as wood.
Costs count only verified prepared witnesses, used to-purchase stock, and the
recorded per-cut fee. Unknown costs do not become zero; totals require complete
wood readiness, allocations, witnesses and all amounts. With costing detail
off, a short amount/unknown summary remains. All-off still has a cover, scope,
issues, safety and limitations. Tables/identity paragraphs flow via
`DocumentBuilder` with repeated headings; layout failures return an error
instead of dropping a row or label.

The existing `prepare_export` freezes **default** `ReceiptSections` in its
snapshot. `ReviewedPacket::prepare` instead binds an explicit selection to the
snapshot before building its document; non-default selections must use the
reviewed writer, never the legacy exporter.

`cargo test --locked --test workshop_document` covers grouping by distinct
material/dimensions/grain, individual UUID/allocation mapping, all-off mandatory
content and deterministic dense en/pt-BR pagination without omitted labels.

## Witness diagrams and validated hardware (task 11.3)

With Sheet diagrams + cut steps on, each verified stock witness starts on a
fresh A4 page. The sheet heading includes the stable alias and UUID; scale,
actual size, kerf, edge trims, trim area and other blade loss are derived from
the prepared witness. Trim allowances **include** their blade bands. Finished
parts are outlined at a uniform physical scale; full-span blade bands use
their exact witness widths. Each cut has a numbered, keyed diagram callout.
The part legend maps witness P numbers to full names, board UUIDs, allocation
UUIDs and stock aliases. Operations print their actual input/output P numbers,
retained edge and extent, and blade side; waste/offcut nodes remain identified.
The section and scale/kerf/trim context repeat on instruction continuations.
Turning this section off removes both diagrams and their cut sequence while
retaining the cover's safety, issue and omitted-section disclosures.

With Hinge references on, hardware appears separately from wooden parts.
Placeholder dimensions are reference envelope dimensions, not installation
instructions. Numeric K/R, cup/plate data, board-local coordinates, faces,
edges and source citations come **only** from prepared validated installation
guidance. Missing/unverified/invalid references and incoherent joints receive
the actual prepared withholding reasons, without drill coordinates. Fastener
details are explicitly unavailable even for valid guidance. Turning hardware
detail off still leaves its prepared issue and approximate-motion notices in
the mandatory summary.

`cargo test --locked --test document_diagrams --test workshop_document`
exercises dense bilingual continuation pages, witness operation/part identities,
trim passes, section omission and valid/invalid hinge evidence. The current
`pdf_export.rs::render_pdf` still owns the legacy production export layout. The
new `render_document_pdf` and `paint_document_page` entry points below consume
this shared page model; production routing is pending.

## Document-backed PDF and native page painter (task 11.4 backend)

`pdf_export::render_document_pdf(&Document)` serializes each existing page in
order, converting top-left millimetres to PDF bottom-left coordinates. It embeds
the bundled Noto Sans and JetBrains Mono faces and positions each character at
the supplied `TextRun.glyphs` origin. It does not wrap, paginate, measure or
reconstruct diagrams. Per-character `WriteText` operations retain the subset
font's Unicode map; the `printpdf` 0.8.2 `WriteCodepoints` route was checked and
produced corrupt round-tripped text. Paths honor closure, color and physical
stroke width; boxes honor fills and strokes (including the exact kerf bands).
`page:N` (one-based) and HTTPS links become PDF annotations; unsupported
destinations return an error instead of silently disappearing.

`pdf_export::paint_document_page(painter, page, origin, points_per_mm)` renders
that page in native egui, with its supplied glyph origins, paths, fills and
strokes. The caller installs the bundled native theme fonts, clips to its page
rectangle, draws the paper background and uses the page's link bounds for hit
testing. Use `points_per_mm = 72/25.4 × zoom` for physical 100% page scale;
zoom only changes drawing scale, never the document or page count. The painter
lays out one scalar at a time to obtain its raster glyph, then places that
glyph at the model origin; it never lays out a whole line or reflows a page.

`cargo test --locked --test shared_document_pdf` exercises a dense 28-part
bilingual witness packet: page count, text extraction, every glyph origin,
path/box counts, embedded fonts and blade bands are checked using printpdf's
in-process PDF parser. A headless egui pass paints a small page at two zooms
without changing the source document. No external PDF viewer is required.
printpdf 0.8.2 rounds the PDF media box to whole points (less than 0.2 mm
from the model's exact A4 dimensions); primitive coordinates themselves are
checked within 0.003 mm of the positioned document.
Legacy `render_pdf(prepared)` and `export::write_pdf` remain on their existing
path, preserving receipt/write-verification behavior. The legacy writer rejects
a prepared snapshot with non-default sections rather than recording a false
section choice.

## Frozen reviewed export (task 11.6 foundation)

`export::ReviewedPacket::prepare[_cancellable](project, mode, settings, sections)`
freezes one normalized `PreparedExport` with selected receipt sections and its
`Document` as a private pair. It returns read-only `document()`, `snapshot()`,
issues and `key()` accessors. Preparation checks cancellation before/after
manufacturing preparation and after page layout; a cancelled/blocked layout
cannot be published. The key includes project UUID, editing revision,
manufacturing wood/packet fingerprint, packet mode, language, units, three
section choices, layout version and font metrics version. `matches_source`
allows Handoff to label stale results, discard stale workers and require a
refreshed preview before export. Color-only changes invalidate the review key
through the editing revision while leaving manufacturing receipt freshness
unchanged. Zoom/current page are view state, not key inputs.

`export::write_reviewed_pdf(&packet, destination, overwrite, cancelled)`
serializes **that packet's document** through `render_document_pdf` without
rerunning preparation or pagination after the picker. The shared writer retains
destination/overwrite rechecks, same-directory atomic commit, cancellation
before rendering and committing, verified-byte hash, actual completion time and
receipt evidence. Receipt mode, selected sections and layout version come from
the bound snapshot (layout version 2 for reviewed pages; legacy layout 1).
No receipt is returned on picker cancellation, overwrite refusal, write error
or uncertain post-commit durability. A successful receipt must still be added
to the live project and saved by the Handoff integration.

`cargo test --locked --test reviewed_packet` covers all-off mandatory content,
source/key changes, cancellation and failed destinations, verified PDF page
count, recorded sections and color-only freshness. The prepared snapshot is
not exposed mutably; consumers keep the packet for the picker round trip.

**Remaining integration:** Handoff must retain/cache the reviewed packet by
its key, label stale preparation and paint selectable pages/thumbnails from
`document()`. On picker completion, require current-source review and pass the
same packet to `write_reviewed_pdf`, then persist its receipt. Add native
capture/raster comparisons at multiple zooms against serialized pages before
checking off 11.4/11.6/11.8.

**Shaping limit:** `PositionedGlyph` records a Unicode scalar, advance and
position, but not the shaped glyph ID or selected fallback font. The serializer
rejects characters absent from the declared bundled face instead of emitting
a mismatched fallback. Complex substitutions/ligatures or a symbol that egui
actually draws from a fallback cannot be guaranteed visually identical with
this model; add the glyph ID and resolved face to the shared layout if such
content becomes required for printable packets. The current bilingual Latin
fixtures and dimensional symbols serialize and round-trip without this limit.

`cargo test --locked --test document_layout` checks repeatable page count and
geometry, long bilingual names, per-glyph bounds, notices, continuation
context and failures instead of clipped oversize content. PDF/native rendering
comparisons remain part of task 11.4.
