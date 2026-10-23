# Tasks

## 1. Library

- [x] 1.1 `render::snap`: scene boxes, shared box hit test, occlusion.
- [x] 1.2 Corner, midpoint, face-centre, edge and face snaps; floor and view-plane fallbacks.
- [x] 1.3 Axis inference, locked-direction projection, Alt free placement.

## 2. Desktop

- [x] 2.1 Viewport picking shares `snap::nearest_hit`.
- [x] 2.2 Measure tool state, clicks, X/Y/Z, Shift, Alt, Esc, Delete; no selection changes.
- [x] 2.3 Overlay: markers, snap tooltip, guides, distance pills, bottom-left readout card.
- [x] 2.4 Remove the canvas bounding-box card; inspector keeps scope and frame.
- [x] 2.5 Strings in English and Portuguese; shortcut list and status hint.

## 3. Verification

- [x] 3.1 Snap unit tests (ortho and perspective, occlusion, inference, locks, Alt).
- [x] 3.2 Headless viewport tests (click flow, anchors, keys, panel placement, formatting).
- [ ] 3.3 Manual check in both languages at narrow and wide window sizes.

## 4. Phase 3 polish

### 4.1 Real fitting shapes

- [x] Expose exact fitting edges and triangles with their anchor object from `render::hardware_mesh` for feet, hinges and slides.
- [x] Snap to real fitting corners, midpoints and edges, and hit their real surfaces for face snaps and occlusion.
- [x] Anchor points on fittings to the moving object; follow edits and drop the measurement when the fitting is deleted.
- [x] Unit tests with a shaped foot, a hinge part and an anchored measurement.

### 4.2 Object-axis and perpendicular locks

- [x] Cycle X / Y / Z from world axis to the first point's object axis to off.
- [x] Press P to lock perpendicular to the face under the first point; clear the other lock when either is chosen.
- [x] Name local and perpendicular locks in the readout without colouring them as world axes.
- [x] Unit tests for rotated local axes, perpendicular projection and lock cycling.

### 4.3 Copy the measured value

- [x] Cmd/Ctrl+C copies the in-progress or last kept distance in the display unit and UI language.
- [x] Keep copying side-effect free and show a short confirmation in the readout.
- [x] Unit test the clipboard command and the formatted value.

### 4.4 Presentation and verification

- [x] English and Portuguese strings, shortcut hint and viewport documentation.
- [x] `cargo fmt --check`, `cargo check --locked --all-targets`, `cargo clippy --locked --all-targets -- -D warnings`, and the test suite pass.
