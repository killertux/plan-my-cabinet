# Stock/sheet read model integration

Build `StockReadModel::build(&project)` from the committed project or a repair
preview. It validates the portable document and returns `ReadModelError` rather
than hiding invalid dimensions/prices. Keep the resulting immutable snapshot for
the current manufacturing/pricing state; navigation, filter changes and hover
should use that snapshot instead of repeating bounded witness searches. Rebuild
after allocation, stock, board, material, kerf, ownership or price/fee changes.

- `table_rows(Some(material_id))` gives filtered borrowed pieces; `None` gives
  global priority order. Grouping can rearrange references for display without
  changing `global_rank` (1-based). `sheet_cards()` and `miniature(stock_id)`
  reference the **same** `StockPieceReadModel` instances.
- `alias` is the persistent project alias. For a valid, alias-free in-memory
  legacy project, the builder normalizes a private copy in priority/UUID order;
  it never writes the project. UUID remains the navigation identity; ownership
  is `source`, regardless of alias prefix. The reference fixture pins its
  depicted S1/S2/S3/O1 labels independently of priority and keeps label text
  out of stock names; display the alias, not a name-derived guess.
- Grouped or filtered priority controls can call
  `ProjectEditor::reorder_stock_subset(id, target, visible_ids)`, where the IDs
  are the current displayed subset in global priority order and `target` is a
  zero-based position in that subset. It validates stale/duplicate members and
  permutes only their occupied global slots, leaving other pieces in place.
  `reorder_stock(id, global_target)` remains the explicit global-rank route.
  The drag and keyboard UI for these commands is still task 8.3.
- `parts` are physical allocation records, including conflicting ones; each
  retains board/allocation IDs, original coordinates/orientation and effective
  board thickness/grain. `measured_thickness` is the stock piece's own measured
  thickness, separate from `material_default_thickness`. `StockGrain::Unknown`
  remains distinct from `Nondirectional`.
- `SheetProof::Verified` carries the current `CutTree` and independently
  accounted area. `cut_count()` and `utilization()` return `None` for unused,
  violated or search-exhausted pieces. The latter yields an exact numerator and
  denominator of square micrometres, for final UI formatting; tree operations
  and accounting supply cut/trim/offcut data without recreating geometry.
- `boards` holds one domain diagnosis per board, independent of visibility;
  `materials` includes empty materials and unallocated board counts.
  `estimate` is the domain spending/completeness result, `cut_fee` preserves
  unknown versus known free, and `usage_summary()` counts physically used
  purchased/owned pieces and exposes witnessed total cuts or `None`.

Tests: `cargo test --locked --test stock_read_models` checks the reference
fixture, invalid/empty/pricing distinctions, and count/identity invariants over
allocation subsets and priority rotations.
