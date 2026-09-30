# Spec Delta

## ADDED Requirements

### Requirement: Agents SHALL use and create slides and feet

The MCP server SHALL offer tools to list slide and foot catalogs, pin models, create new slide and foot models (optionally saved to the user catalog), plan and install slides on a drawer with checks and hole references, place and update feet (with a mounting-face anchor), list them, and render a drawer pulled out.

#### Scenario: A chest on feet
- **WHEN** an agent generates a Drawers template, raises it by 100 mm and adds four `generic-post-square-100` feet at the corners with the top-centre anchor
- **THEN** the scene description lists four feet and six slide envelopes with no overlaps, and `render_drawer_opening` returns a picture with the drawer pulled out
