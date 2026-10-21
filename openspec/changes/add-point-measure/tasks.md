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

## 4. Later

- [ ] 4.1 Vertices of foot and fitting meshes.
- [ ] 4.2 Local-axis lock (press an axis key twice) and perpendicular-to-face.
- [ ] 4.3 Copy the measured value.
