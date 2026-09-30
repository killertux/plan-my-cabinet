# Spec Delta

## ADDED Requirements

### Requirement: The Drawers template SHALL install drawer slides

The Drawers template SHALL install a pair of catalog slides on every drawer (default FGVTN TT45 Slowmotion, the longest length that fits) and SHALL use the slide's clearance for the box width. The user MAY choose another family or none; with none, the side clearance field applies. When no length fits, the review SHALL say so and generation SHALL be refused.

#### Scenario: Default slides
- **WHEN** a Drawers template 600 × 560 × 720 mm with 500 mm boxes is generated with the defaults
- **THEN** every drawer runs on a pair of 500 mm TT45 slides with no issues

#### Scenario: No slide fits
- **WHEN** the carcass is too shallow for the shortest length
- **THEN** the review reports that no slide fits and nothing is generated
