# Spec Delta

## Purpose

Produce explainable, feasible rectangular cutting sequences and compare their material use, paid cuts, and estimated new spending.

## ADDED Requirements

### Requirement: Full-span cutting feasibility
Every valid plan SHALL consist of straight cuts spanning the current rectangular workpiece parallel to its edges, accounting for the configured positive kerf and yielding identified rectangular outputs. Each finished part SHALL correspond to one terminal rectangle of its exact blank dimensions, without double-counting shared boundaries. Remaining rectangles SHALL be identified as offcuts or waste. Stopped cuts, stacking, interior cutouts, and arbitrary-angle sheet cuts SHALL be unsupported in the first version.

The first version SHALL require the full kerf band to lie inside the current workpiece. A kerf band exactly consuming a remaining edge allowance SHALL be allowed with no usable output on that side. Removing less than one kerf width by overlapping the blade outside the stock SHALL be unsupported and explicitly identified as a cutting-model limitation, not a physical impossibility.

#### Scenario: Exact kerf boundary
- **WHEN** two 100 mm-wide parts are cut side by side across stock of width 205 mm with a 5 mm kerf
- **THEN** the transverse split fits exactly, while equivalent stock of width 204 mm fails that arrangement

#### Scenario: Part equals usable stock
- **WHEN** a part exactly matches a stock rectangle and no edge trimming is configured
- **THEN** that stock yields the part without an artificial perimeter cut or kerf deduction

#### Scenario: Sub-kerf edge allowance
- **WHEN** a 100 mm-wide part must be isolated from a 103 mm-wide piece using a 5 mm kerf
- **THEN** the proposed isolation is rejected with the full-in-stock-kerf limitation explained, while isolation from a 105 mm-wide piece is supported as one edge-shaving cut

### Requirement: Configurable cutting assumptions
The system SHALL expose kerf and non-negative edge-trimming allowances in project cutting settings, initially proposing 5 mm kerf and zero trim with a prompt to confirm shop assumptions. Trim allowances SHALL denote total loss from each original edge including kerf; positive allowances smaller than kerf SHALL be rejected. Required trimming operations SHALL be included in the sequence and cut count. Changes SHALL trigger revalidation, not silently shrink requested part dimensions.

#### Scenario: Increased kerf breaks a plan
- **WHEN** kerf increases beyond the space reserved between parts
- **THEN** the plan becomes invalid with an explanation and the requested finished dimensions remain unchanged

### Requirement: Numbered physical cut sequence
The system SHALL identify every cut's input piece, reference edge, distance to the retained finished edge, kerf side, and resulting pieces. Each operation SHALL refer only to stock or outputs already available from preceding operations. One physical pass SHALL count as one paid cut, without assuming stacked cutting. A boundary already provided by stock SHALL NOT create a billable cut.

#### Scenario: Cut a strip into parts
- **WHEN** a sheet is split into strips and one strip is subsequently crosscut
- **THEN** the crosscut references that strip's identity, follows the split that created it, and adds one to the cut count

### Requirement: Transparent monetary estimates
The system SHALL support a single configurable currency per project, a non-negative flat charge per physical cut, and a non-negative purchase price per stock item marked to-purchase. Estimated new spending SHALL equal full prices of used to-purchase items plus cutting charges on all used stock. Owned stock SHALL add zero new material expense. Unknown prices SHALL be distinct from zero; incomplete estimates SHALL be labelled and SHALL NOT support a lowest-cost claim. Taxes, delivery, setup fees, and stack discounts SHALL be excluded and disclosed. Currency changes SHALL NOT imply exchange-rate conversion.

#### Scenario: Use owned wood and one purchased sheet
- **WHEN** a plan uses an owned offcut, one purchased sheet priced at 200 currency units, and six cuts priced at 5 each
- **THEN** estimated new spending is 230, with 200 material and 30 cutting expense shown separately

#### Scenario: Sheet price is missing
- **WHEN** an allocated to-purchase sheet has no price
- **THEN** known cutting expense remains visible but the total is labelled incomplete rather than treating the sheet as free

### Requirement: Explicit optimization with alternatives
The system SHALL provide explicit optimization objectives for lowest estimated new spending, fewest cuts, and least wasted area within the declared stock pool. Complete allocation and valid cutting sequences SHALL be prerequisites for ranking complete solutions; a cheaper incomplete result SHALL NOT be presented as a complete optimum. Candidates SHALL preserve locks, respect grain/kerf/trim, show sheets used, cut count, offcuts, and cost completeness, and require acceptance before replacing current placements. Cost ties SHALL prefer less wasted area, then fewer cuts. The system SHALL describe heuristic results as best found, not guaranteed global optima.

The area objective and cost tie-break SHALL mean the full area of used stock minus finished-part area, including reusable offcuts, and SHALL be labelled "least unused stock area" to distinguish it from irreversible kerf/trim loss. Unused stock items SHALL not enter this sum. Recoverable area and irreversible loss SHALL also be reported separately.

#### Scenario: Preview a cheaper plan
- **WHEN** optimization finds a valid lower-cost arrangement
- **THEN** the existing plan remains unchanged until acceptance and the preview shows the cost and placement differences

#### Scenario: Cancel or stale result
- **WHEN** optimization is cancelled or relevant project data changes before its result is accepted
- **THEN** no candidate is silently committed and a stale result requires a fresh search

#### Scenario: Reusable leftovers still count toward unused area
- **WHEN** two complete plans yield 8,000 square mm of parts, one using 10,000 square mm of stock with 1,000 loss and 1,000 offcut, and another using 12,000 with 500 loss and 3,500 offcut
- **THEN** the least-unused-area objective prefers the first plan's 2,000 unused area over the second's 4,000, while displaying both plans' distinct recoverable and loss areas

### Requirement: Distinguish validity from search exhaustion
The system SHALL distinguish verified feasible plans, detected rule violations, and a search ending without a feasible plan. It SHALL preserve the user's current plan if optimization cannot improve it or cannot find a complete solution. Partial allocations SHALL remain useful draft information, not be labelled shop-ready.

#### Scenario: Search budget expires
- **WHEN** the optimizer reaches its resource budget without finding a complete plan
- **THEN** it reports that no complete plan was found within the budget rather than declaring the project impossible

### Requirement: Material accounting
For each used stock piece, the system SHALL report finished-part area, recoverable rectangular offcut area, and loss from kerf and trimming without double counting. Utilization SHALL use full consumed stock area as its denominator and identify the definition used. No cutting plan SHALL certify structural safety or shop acceptance.

#### Scenario: Account for a complete sheet
- **WHEN** a feasible plan is summarized
- **THEN** finished parts, offcuts, and losses sum to the original used stock area within the document's measurement precision

#### Scenario: Trim corners are not counted twice
- **WHEN** all four edges of a 100 x 100 mm piece are trimmed by 5 mm including a 5 mm kerf, in left/right/bottom/top order
- **THEN** four cuts leave a 90 x 90 mm usable rectangle and 1,900 square mm of loss, not 2,000
