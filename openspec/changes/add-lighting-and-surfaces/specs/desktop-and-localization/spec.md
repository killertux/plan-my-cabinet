# Spec Delta

## ADDED Requirements

### Requirement: The user SHALL choose how the 3D view is lit

The 3D view's light SHALL follow the camera by default, or stay fixed at a direction the user sets in Settings or by fixing it where the camera is, or be off for even shading. The choice SHALL be a machine-local preference, available from Settings and the viewport's light menu in English and Portuguese. Pictures SHALL always use a fixed studio light.

#### Scenario: Fix the light
- **WHEN** the user chooses "Fix the light here" and then orbits
- **THEN** the light stays where the camera was, and the choice survives a restart

#### Scenario: Old preferences
- **WHEN** a preferences file without lighting loads
- **THEN** the light follows the camera
