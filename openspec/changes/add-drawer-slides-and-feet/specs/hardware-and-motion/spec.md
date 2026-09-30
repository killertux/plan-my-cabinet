# Spec Delta

## ADDED Requirements

### Requirement: Catalog drawer slides SHALL be installed and checked on drawers

A slide pair SHALL join a drawer (a board group) to the two carcass sides beside it. The system SHALL check each side gap against the slide's clearance and tolerance, the slide length against the carcass depth behind the setback and against the box side, the slide height on both boards, and the alignment of the two slides, and SHALL give hole positions on both boards from the pinned facts.

#### Scenario: Template drawer with TT45
- **WHEN** a 600 mm Drawers template with 500 mm boxes gets TT45 slides
- **THEN** the box is 25.4 mm narrower than the opening, each gap measures 12.7 mm, the 500 mm length is chosen, there are no issues, and the first carcass hole is 37 mm from the front edge

#### Scenario: Wrong gap
- **WHEN** a box side is moved 1 mm toward the middle
- **THEN** that side reports a clearance issue with the measured 13.7 mm gap and its references are withheld from the shop PDF

### Requirement: A drawer SHALL slide out in a display-only preview

The preview SHALL translate every member of the drawer along the pull direction up to the slide's travel and SHALL never change the saved project.

#### Scenario: Open a drawer fully
- **WHEN** a drawer on 500 mm TT45 slides is previewed at 500 mm
- **THEN** its external front moves 500 mm toward the front, the drawer members of the slides follow, the cabinet members stay, and the project revision is unchanged

### Requirement: Catalog feet SHALL be drawn with the product's shape

Feet SHALL be catalog hardware positioned like placeholders, never cut from stock, drawn as tapered solids, posts with plate and glide, or tube frames within their box, and included in bounds, picking, measurements and the scene description.

#### Scenario: Square chrome feet under a chest
- **WHEN** four 100 mm square chrome feet are placed under a chest raised by 100 mm
- **THEN** pictures show each foot's flange, tube and glide, the scene description lists them touching the bottom, and no sheet stock is used

### Requirement: Generic packs SHALL be labelled

Records from a pack with review status `generic` SHALL be shown as generic reference dimensions on screen and in exports.

#### Scenario: Generic foot in the PDF
- **WHEN** a project with a foot from `generic-feet.toml` is exported
- **THEN** the foot line says the dimensions are generic references

### Requirement: New slide and foot models SHALL be validated like pack records

Models created in the app or by an agent SHALL pass the pack loader's rules, SHALL be pinned as user-supplied, and MAY be saved to the app-owned `user-models.toml`, which SHALL not be overwritten when it is invalid.

#### Scenario: A post without plate thickness
- **WHEN** a foot model of kind `post` omits `plate_thickness`
- **THEN** it is refused with the path `shape.plate_thickness` and nothing is pinned or written
