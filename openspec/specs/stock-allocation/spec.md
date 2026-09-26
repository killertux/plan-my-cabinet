# stock-allocation Specification

## Purpose

Connect each physical wooden part to project-local rectangular stock without constraining freeform furniture design.

## Requirements

### Requirement: Project-local stock inventory
The system SHALL allow users to enter rectangular sheets and offcuts with unique identities, names, positive dimensions, material and thickness, grain direction, quantity, and owned or to-purchase status. Quantities SHALL represent separately identifiable physical pieces. Stock SHALL NOT be reserved across projects.

#### Scenario: Enter existing offcuts
- **WHEN** a user adds two owned 900 x 240 x 18 mm plywood offcuts
- **THEN** the project contains two individually allocatable stock pieces without adding purchase expense or modifying another project's inventory

### Requirement: Compatible allocations
The system SHALL allocate each wooden part to at most one stock piece of the same material identity and effective thickness. A part SHALL fit within the usable stock rectangle, obey its grain restriction, and not overlap another part or required kerf. Hardware SHALL be excluded. Board grain choices SHALL be local length, local width, or unrestricted. Stock grain choices SHALL be along either stock axis, nondirectional, or unknown. Unrestricted boards SHALL permit either quarter-turn orientation; length/width constrained boards SHALL align that board axis with directional stock grain. Nondirectional stock SHALL permit either orientation. Unknown stock grain SHALL block a grain-constrained allocation until the stock direction or board requirement is explicitly resolved. Assembly rotation SHALL NOT change these rules.

#### Scenario: Reject an incompatible source
- **WHEN** a user attempts to place an 18 mm plywood part on 12 mm MDF stock
- **THEN** placement is rejected with material and thickness explanations and neither allocation nor assembly changes

#### Scenario: Grain prevents a rotated fit
- **WHEN** a board fits a sheet only after a quarter-turn that violates its lengthwise grain requirement
- **THEN** the system does not present that orientation as a valid allocation

#### Scenario: Unknown grain is not unrestricted
- **WHEN** a length-grain board is assigned to stock with unknown grain direction
- **THEN** it remains incompatible until the user confirms a stock direction or explicitly removes the board's restriction

### Requirement: Stable automatic first-fit
On part creation or duplication, the system SHALL attempt allocation in a visible, user-reorderable stock priority order without moving existing allocations. It SHALL search allowed orientations and placements that admit a supported full-span cut sequence. If no placement is found, it SHALL retain the part as unallocated and distinguish search failure from proof that no possible layout exists. It SHALL NOT implicitly create additional purchased stock.

#### Scenario: Existing positions remain stable
- **WHEN** a new part can be inserted on the second stock piece without relocating existing parts
- **THEN** it is allocated there and all previous placements and assembly transforms remain unchanged

#### Scenario: Stock is insufficient
- **WHEN** no available stock piece can accommodate a new part
- **THEN** the new part remains editable in the assembly, is listed as unallocated, and the user can add stock or request optimization

### Requirement: Direct sheet editing and locks
The system SHALL support previewed drag, numeric position entry, allowed quarter-turn rotation, transfer between sheets, explicit unallocation, and placement locking. Valid committed placements SHALL have a supported cutting sequence. Invalid placement previews SHALL explain out-of-bounds, overlap, grain, material, kerf, or cut-sequence conflicts and SHALL NOT replace the last committed placement. A lock SHALL prevent allocator and optimizer relocation; an explicit user move of a locked allocation SHALL require unlocking first.

For a sheet already invalidated by design edits, the system SHALL support an atomic multi-placement repair preview. Users SHALL be able to stage several moves or explicit unallocations while intermediate previews remain invalid. Acceptance SHALL require a feasible witness for all remaining placements on affected sheets; unallocated boards SHALL remain draft issues. Cancel SHALL restore the full pre-repair state. Explicit unallocation SHALL remain available even while other conflicts persist, so repair is never blocked by unrelated invalid placements.

#### Scenario: Collision-free but uncuttable placement
- **WHEN** a proposed manual arrangement has no overlapping parts but no supported full-span cutting sequence can be established
- **THEN** it is not accepted as a valid placement and the preview explains the cutting-method limitation

#### Scenario: Lock a sourced part
- **WHEN** the user locks an allocation and runs optimization
- **THEN** its sheet identity, location, and orientation are preserved in every offered candidate

#### Scenario: Repair two independent conflicts
- **WHEN** a sheet has two independent conflicts and the user stages two corrective moves in one repair preview
- **THEN** the first intermediate preview can remain invalid, the final feasible layout can be accepted as one transaction, and cancelling at either point preserves the original draft placements

### Requirement: Linked views without coupled positions
The system SHALL highlight the same identified part between the assembly, part list, and stock views. Moving a part in the assembly SHALL NOT alter its stock orientation or location; moving an allocation SHALL NOT move its assembly geometry. Hidden assembly parts SHALL still require allocation.

#### Scenario: Reposition furniture only
- **WHEN** the user rotates a cabinet assembly containing allocated boards
- **THEN** the boards retain their sheet locations, grain requirements, and cut-list identities

### Requirement: Design edits invalidate visibly rather than silently repack
After changes to parts, stock, materials, grain, kerf, or trim settings, the system SHALL revalidate affected allocations. It SHALL preserve valid locations and retain invalid allocations visibly as conflicts until explicitly resolved, including locked allocations. It SHALL provide reasons and resolution actions without silently resizing parts, moving other allocations, or treating invalid records as valid cutting assignments.

#### Scenario: A locked shelf grows into another part
- **WHEN** the shelf width increases and its locked sheet location overlaps another allocation
- **THEN** the design edit succeeds, both implicated placements are highlighted with the conflict, and shop-ready export is blocked until resolved

#### Scenario: Undo restores feasibility
- **WHEN** the user undoes the dimension change that caused an allocation conflict
- **THEN** the previous dimension and allocation state are restored together and validation is recomputed
