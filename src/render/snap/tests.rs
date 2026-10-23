use super::*;
use crate::domain::{Hardware, HardwareKind};
use crate::money::Currency;
use crate::render::camera::Preset;
use crate::units::Length;
use crate::units::Quaternion;

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
    let id = Uuid::from_u128(id);
    let pose = Pose::new(at, Quaternion::IDENTITY).unwrap();
    SceneBox {
        id,
        pose,
        size,
        anchor: Some(AnchorBase {
            id,
            pose,
            owner: None,
        }),
        shape: None,
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
            local_mm: [0.0, 0.0, 100.0],
            owner: None,
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
        lock: Some(Lock {
            direction: [0.0, 0.0, 1.0],
            axis: Some(2),
            local: false,
            perpendicular: false,
        }),
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

/// A pyramid whose apex is well inside its 100 × 100 × 40 envelope.
fn pyramid() -> SceneBox {
    let base = [
        [0.0, 0.0, 0.0],
        [100.0, 0.0, 0.0],
        [100.0, 100.0, 0.0],
        [0.0, 100.0, 0.0],
    ];
    let apex = [50.0, 50.0, 40.0];
    let edges = (0..4)
        .flat_map(|i| [[base[i], base[(i + 1) % 4]], [base[i], apex]])
        .collect();
    let mut tris = Vec::new();
    for i in 0..4 {
        tris.push(ShapeTri {
            points: [base[i], base[(i + 1) % 4], apex],
            normal: [0.0, 0.0, 1.0],
        });
    }
    for i in [0, 2] {
        tris.push(ShapeTri {
            points: [base[i], base[(i + 1) % 4], base[(i + 2) % 4]],
            normal: [0.0, 0.0, -1.0],
        });
    }
    let mut shaped = boxed(9, [0.0; 3], [100.0, 100.0, 40.0]);
    shaped.shape = Some(Shape { edges, tris });
    shaped
}

#[test]
fn shaped_fittings_snap_to_their_real_vertices_not_the_envelope() {
    let mut camera = front();
    camera.target = [50.0, 50.0, 20.0];
    let boxes = [pyramid()];
    let apex = [50.0, 50.0, 40.0];
    let snap = snap_point(
        &boxes,
        &camera,
        RECT,
        screen(&camera, apex) + egui::vec2(3.0, -2.0),
    )
    .unwrap();
    assert_eq!((snap.kind, snap.point), (SnapKind::Endpoint, apex));
    // The envelope corner that is not part of the drawn shape does not win.
    assert_ne!(snap.point, [100.0, 100.0, 40.0]);
    assert_eq!(snap.anchor.map(|a| a.local_mm), Some(apex));
}

#[test]
fn local_axis_lock_follows_the_object_rotation() {
    let mut camera = front();
    camera.target = [0.0; 3];
    let direction = [
        45.0_f64.to_radians().cos(),
        45.0_f64.to_radians().sin(),
        0.0,
    ];
    let snap = measure_target(
        &[],
        &camera,
        RECT,
        screen(&camera, direction.map(|v| v * 60.0)),
        Constraint {
            from: Some([0.0; 3]),
            lock: Some(Lock {
                direction,
                axis: Some(0),
                local: true,
                perpendicular: false,
            }),
            free: false,
        },
    );
    assert_eq!(snap.kind, SnapKind::OnAxis);
    assert_eq!(snap.axis, Some(0));
    assert!(snap.local_axis);
    assert!((snap.point[0] - direction[0] * 60.0).abs() < 0.5);
    assert!((snap.point[1] - direction[1] * 60.0).abs() < 0.5);
}

#[test]
fn perpendicular_lock_projects_onto_the_face_normal() {
    let mut camera = front();
    camera.target = [100.0, 0.0, 50.0];
    let snap = measure_target(
        &[],
        &camera,
        RECT,
        screen(&camera, [100.0, 0.0, 80.0]),
        Constraint {
            from: Some([100.0, 0.0, 50.0]),
            lock: Some(Lock {
                direction: [0.0, 0.0, 1.0],
                axis: None,
                local: false,
                perpendicular: true,
            }),
            free: false,
        },
    );
    assert_eq!(snap.kind, SnapKind::OnAxis);
    assert!(snap.perpendicular);
    assert_eq!((snap.point[0], snap.point[1]), (100.0, 0.0));
    assert!((snap.point[2] - 80.0).abs() < 0.5);
}

#[test]
fn anchored_hardware_points_follow_their_object_and_die_with_an_owner() {
    let id = Uuid::from_u128(42);
    let mut project = Project::new("Anchors", Currency::Brl);
    project.hardware.push(Hardware {
        id,
        name: "Foot".into(),
        parent_id: None,
        pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
        kind: HardwareKind::Placeholder {
            dimensions: [Length::from_micrometres(10_000); 3],
        },
    });
    let point = MeasurePoint::On(Anchor {
        id,
        local_mm: [10.0, 0.0, 0.0],
        owner: None,
    });
    assert_eq!(point.resolve(&project), Some([10.0, 0.0, 0.0]));
    project.hardware[0].pose = Pose::new([100.0, 0.0, 0.0], Quaternion::IDENTITY).unwrap();
    assert_eq!(point.resolve(&project), Some([110.0, 0.0, 0.0]));
    let owned = MeasurePoint::On(Anchor {
        id,
        local_mm: [10.0, 0.0, 0.0],
        owner: Some(Uuid::from_u128(43)),
    });
    assert_eq!(owned.resolve(&project), None);
}

#[test]
fn a_catalog_foot_uses_its_drawn_rim_for_snapping() {
    use crate::catalog_pack::{CatalogRegistry, snapshot_foot};
    use crate::commands::ProjectEditor;

    let registry = CatalogRegistry::bundled();
    let pack = registry.pack("generic-feet").unwrap();
    let family = pack
        .feet
        .iter()
        .find(|f| f.id == "tapered-round-40")
        .unwrap();
    let variant = family
        .variants
        .iter()
        .find(|v| v.code == "generic-tapered-round-40-black")
        .unwrap();
    let entry = snapshot_foot(pack, family, variant, "en");
    let catalog_id = entry.id;
    let mut editor = ProjectEditor::new(Project::new("Foot", Currency::Brl)).unwrap();
    let foot = editor
        .create_foot(
            "Foot".into(),
            catalog_id,
            Some(entry),
            None,
            Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
        )
        .unwrap();
    let project = editor.project().clone();
    let boxes = scene_boxes(&project, &Selection::default());
    let foot_box = boxes.iter().find(|b| b.id == foot).unwrap();
    assert!(foot_box.shape.is_some());

    // The 50 mm top disc's rim vertex at +X, not the envelope's (50, 50).
    let mut camera = Camera {
        target: [25.0, 25.0, 20.0],
        distance: 400.0,
        projection: Projection::Orthographic,
        viewport_aspect: 800.0 / 600.0,
        viewport_height: 600.0,
        ..Camera::default()
    };
    camera.set_preset(Preset::Top);
    let rim = [50.0, 25.0, 40.0];
    let snap = snap_point(
        &boxes,
        &camera,
        RECT,
        screen(&camera, rim) + egui::vec2(2.0, 1.0),
    )
    .unwrap();
    assert_eq!((snap.kind, snap.point), (SnapKind::Endpoint, rim));
    assert_eq!(snap.anchor.map(|a| a.id), Some(foot));
}
