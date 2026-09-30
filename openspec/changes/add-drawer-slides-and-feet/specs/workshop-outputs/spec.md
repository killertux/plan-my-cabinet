# Spec Delta

## ADDED Requirements

### Requirement: Packets SHALL list the hardware to buy and slide references

Both PDFs SHALL list hinges, slide pairs and feet by product code with quantities, and SHALL print, for each drawer without issues, the slide's hole distances from the front edge and centre-line heights on each board, its provenance and the rear-fixing note.

#### Scenario: Three drawers on TT45
- **WHEN** a three-drawer chest on 500 mm TT45 slides is exported
- **THEN** each PDF lists "0073.045500SX" with 3 pairs and gives cabinet holes 37, 53, 101, 261, 277 mm from the front edge

### Requirement: Slide and foot edits SHALL make exports stale

The manufacturing fingerprint (version 5) SHALL include slide installations with their pinned facts and board positions, and feet with their pinned facts. Older fingerprint versions SHALL still verify historical receipts.

#### Scenario: Move a slide
- **WHEN** a slide's height changes after an export
- **THEN** the packet fingerprint changes while the wood fingerprint does not
