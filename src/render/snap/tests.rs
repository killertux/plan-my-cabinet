use super::*;
use crate::render::camera::Preset;

const RECT: egui::Rect = egui::Rect {
    min: egui::Pos2::ZERO,
    max: egui::pos2(800.0, 600.0),
};

/// Orthographic front view: screen right is world +X, screen up is world +Z,
/// and the eye looks along +Y.
fn front() -> Camera {
    let mut camera = Camera {
        target: [50.0, 0.0, 50.0],
        distance: 200.0,
        projection: Projection::Orthographic,
        viewport_aspect: 800.0 / 600.0,
        viewport_height: 600.0,
        ..Camera::default()
    };
    camera.set_preset(Preset::Front);
    camera
}

fn boxed(id: u128, at: [f64; 3], size: [f64; 3]) -> SceneBox {
    SceneBox {
        id: Uuid::from_u128(id),
        pose: Pose::new(at, Quaternion::IDENTITY).unwrap(),
        size,
        anchored: true,
    }
}

/// A 100 × 20 × 100 panel whose front face is the plane Y = 0.
fn panel() -> SceneBox {
    boxed(1, [0.0; 3], [100.0, 20.0, 100.0])
}

fn screen(camera: &Camera, point: [f64; 3]) -> egui::Pos2 {
    camera.project(point, RECT).unwrap()
}

#[test]
fn corner_within_aperture_snaps_exactly_and_anchors_locally() {
    let camera = front();
    let boxes = [panel()];
    let pointer = screen(&camera, [0.0, 0.0, 100.0]) + egui::vec2(5.0, 4.0);
    let snap = snap_point(&boxes, &camera, RECT, pointer).unwrap();
    assert_eq!(snap.kind, SnapKind::Endpoint);
    assert_eq!(snap.point, [0.0, 0.0, 100.0]);
    assert_eq!(
        snap.anchor,
        Some(Anchor {
            id: Uuid::from_u128(1),
            local_mm: [0.0, 0.0, 100.0]
        })
    );
}

#[test]
fn midpoint_and_face_centre_win_only_when_the_pointer_is_on_them() {
    let camera = front();
    let boxes = [panel()];
    let mid = snap_point(
        &boxes,
        &camera,
        RECT,
        screen(&camera, [50.0, 0.0, 100.0]) + egui::vec2(2.0, 0.0),
    )
    .unwrap();
    assert_eq!(
        (mid.kind, mid.point),
        (SnapKind::Midpoint, [50.0, 0.0, 100.0])
    );
    assert!(mid.edge.is_some_and(|e| e[0].abs() == 1.0));
    let centre = snap_point(&boxes, &camera, RECT, screen(&camera, [50.0, 0.0, 50.0])).unwrap();
    assert_eq!(
        (centre.kind, centre.point),
        (SnapKind::FaceCenter, [50.0, 0.0, 50.0])
    );
}

#[test]
fn edges_then_faces_take_over_away_from_points() {
    let camera = front();
    let boxes = [panel()];
    let edge = snap_point(
        &boxes,
        &camera,
        RECT,
        screen(&camera, [0.0, 0.0, 30.0]) + egui::vec2(3.0, 0.0),
    )
    .unwrap();
    assert_eq!(edge.kind, SnapKind::OnEdge);
    assert_eq!((edge.point[0], edge.point[1]), (0.0, 0.0));
    assert!((edge.point[2] - 30.0).abs() < 0.5, "{:?}", edge.point);
    let face = snap_point(&boxes, &camera, RECT, screen(&camera, [20.0, 0.0, 70.0])).unwrap();
    assert_eq!(face.kind, SnapKind::OnFace);
    assert!(face.point[1].abs() < 1e-9 && (face.point[0] - 20.0).abs() < 0.5);
    assert_eq!(face.anchor.map(|a| a.id), Some(Uuid::from_u128(1)));
}

#[test]
fn hidden_corners_are_skipped_for_the_nearer_box() {
    let camera = front();
    // A small block in front of the panel's top-left corner.
    let cover = boxed(2, [-10.0, -50.0, 80.0], [30.0, 10.0, 30.0]);
    let boxes = [panel(), cover];
    let snap = snap_point(&boxes, &camera, RECT, screen(&camera, [0.0, 0.0, 100.0])).unwrap();
    assert_ne!(snap.point, [0.0, 0.0, 100.0]);
    assert_eq!(snap.kind, SnapKind::OnFace);
    assert_eq!(snap.anchor.map(|a| a.id), Some(Uuid::from_u128(2)));
    assert!(!unobstructed(&boxes, &camera, [0.0, 0.0, 100.0]));
    assert!(unobstructed(&boxes, &camera, [100.0, 0.0, 100.0]));
}

#[test]
fn perspective_corners_snap_like_orthographic_ones() {
    let mut camera = Camera {
        target: [50.0, 10.0, 50.0],
        distance: 400.0,
        viewport_aspect: 800.0 / 600.0,
        ..Camera::default()
    };
    camera.set_preset(Preset::Isometric);
    let boxes = [panel()];
    let corner = [100.0, 0.0, 100.0];
    let snap = snap_point(
        &boxes,
        &camera,
        RECT,
        screen(&camera, corner) + egui::vec2(-3.0, 3.0),
    )
    .unwrap();
    assert_eq!((snap.kind, snap.point), (SnapKind::Endpoint, corner));
}

#[test]
fn first_point_off_the_scene_lands_on_the_floor() {
    let mut camera = front();
    camera.target = [0.0; 3];
    camera.set_preset(Preset::Top);
    let snap = measure_target(&[], &camera, RECT, RECT.center(), Constraint::default());
    assert_eq!(snap.kind, SnapKind::Ground);
    assert_eq!(snap.point[2], 0.0);
    assert!(snap.point[0].abs() < 1e-6 && snap.point[1].abs() < 1e-6);
}

#[test]
fn second_point_infers_the_axis_through_the_first() {
    let camera = front();
    let from = [0.0, 0.0, 0.0];
    let pointer = screen(&camera, [0.0, 0.0, 60.0]) + egui::vec2(6.0, 0.0);
    let snap = measure_target(
        &[],
        &camera,
        RECT,
        pointer,
        Constraint {
            from: Some(from),
            ..Default::default()
        },
    );
    assert_eq!((snap.kind, snap.axis), (SnapKind::OnAxis, Some(2)));
    assert_eq!((snap.point[0], snap.point[1]), (0.0, 0.0));
    assert!((snap.point[2] - 60.0).abs() < 0.5);
    // A feature on the axis keeps its kind and gains the axis tag.
    let on_edge = measure_target(
        &[panel()],
        &camera,
        RECT,
        screen(&camera, [0.0, 0.0, 60.0]) + egui::vec2(3.0, 0.0),
        Constraint {
            from: Some(from),
            ..Default::default()
        },
    );
    assert_eq!((on_edge.kind, on_edge.axis), (SnapKind::OnEdge, Some(2)));
}

#[test]
fn locked_axis_projects_hovered_features_onto_its_line() {
    let camera = front();
    let from = [200.0, 0.0, 0.0];
    let locked = Constraint {
        from: Some(from),
        lock: Some([0.0, 0.0, 1.0]),
        free: false,
    };
    let corner = [100.0, 0.0, 100.0];
    let snap = measure_target(&[panel()], &camera, RECT, screen(&camera, corner), locked);
    assert_eq!(snap.kind, SnapKind::Endpoint);
    assert_eq!(snap.point, [200.0, 0.0, 100.0]);
    assert_eq!(snap.source, Some(corner));
    assert_eq!(snap.axis, Some(2));
    // Without a feature the point slides along the locked line.
    let free_air = measure_target(
        &[],
        &camera,
        RECT,
        screen(&camera, [150.0, 0.0, 40.0]),
        locked,
    );
    assert_eq!(free_air.kind, SnapKind::OnAxis);
    assert_eq!((free_air.point[0], free_air.point[1]), (200.0, 0.0));
    assert!((free_air.point[2] - 40.0).abs() < 0.5);
}

#[test]
fn alt_ignores_features_and_inference() {
    let camera = front();
    let corner = [0.0, 0.0, 100.0];
    let pointer = screen(&camera, corner) + egui::vec2(4.0, 4.0);
    let free = Constraint {
        from: Some([0.0; 3]),
        lock: None,
        free: true,
    };
    let snap = measure_target(&[panel()], &camera, RECT, pointer, free);
    assert_eq!(snap.kind, SnapKind::OnFace);
    assert_eq!(snap.axis, None);
    assert_ne!(snap.point, corner);
}
