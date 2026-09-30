# Spec Delta

## ADDED Requirements

### Requirement: The workshop PDF SHALL list edge banding

The parts table SHALL show each part's banded edges and band, and parts that differ only in banding SHALL be listed apart. The packet SHALL include an edge band table with the banded metres of each band and a 10 % allowance. The manufacturing fingerprint SHALL include the effective banding so a banding change makes a receipt stale.

#### Scenario: Banded side
- **WHEN** a side is banded on its front, top and bottom
- **THEN** the Portuguese parts table reads "C1 L1 L2 · Fita Branca 1x22" for it, and the band table lists the band's metres
