//! Faces are lit by the direction they face in the room, not by which side
//! of the board they are: two identical boxes modelled differently look the
//! same, and under a headlight the face toward the camera is the bright one.
use plan_my_cabinet::domain::{Board, MaterialKind, Project};
use plan_my_cabinet::reference_fixture as fixture;
use plan_my_cabinet::render::camera::{Camera, Projection, Selection};
use plan_my_cabinet::render::lighting::Light;
use plan_my_cabinet::render::raster::{SceneStyle, fit_camera, render_scene};
use plan_my_cabinet::units::{Pose, Quaternion};

/// One shelf at the origin and a copy turned upside down (180° about X) and
/// moved so it fills the same kind of box further along X.
fn twins() -> (Project, [[f64; 3]; 2]) {
    let mut project = fixture::project();
    let shelf: Board = project
        .boards
        .iter()
        .find(|b| b.name == "Shelf")
        .unwrap()
        .clone();
    let size = shelf
        .blank_dimensions()
        .map(|d| d.micrometres() as f64 / 1000.0);
    for material in &mut project.materials {
        material.kind = MaterialKind::Other;
    }
    let mut a = shelf.clone();
    a.parent_id = None;
    a.pose = Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap();
    let mut b = a.duplicate();
    b.pose = Pose::new(
        [size[0] + 200.0, size[1], size[2]],
        Quaternion::normalized(0.0, 1.0, 0.0, 0.0).unwrap(),
    )
    .unwrap();
    project.boards = vec![a, b];
    project.allocations.clear();
    project.hinge_installations.clear();
    project.door_joints.clear();
    project.validate().unwrap();
    let centre = |x0: f64| [x0 + size[0] / 2.0, size[1] / 2.0, size[2]];
    (project, [centre(0.0), centre(size[0] + 200.0)])
}

fn pixel(project: &Project, camera: &Camera, light: Light, at: [f64; 3]) -> [u8; 3] {
    let style = SceneStyle {
        width: 400,
        height: 300,
        material_tint: false,
        show_shadow: false,
        supersample: 1,
        light,
        ..Default::default()
    };
    let raster = render_scene(project, camera, &Selection::default(), &style).unwrap();
    let rect = eframe::egui::Rect::from_min_size(
        eframe::egui::Pos2::ZERO,
        eframe::egui::vec2(400.0, 300.0),
    );
    let p = camera.project(at, rect).unwrap();
    let i = (p.y as usize * raster.width + p.x as usize) * 4;
    [raster.rgba[i], raster.rgba[i + 1], raster.rgba[i + 2]]
}

#[test]
fn identical_boxes_look_identical_however_they_were_modelled() {
    let (project, centres) = twins();
    let mut camera = Camera {
        projection: Projection::Orthographic,
        ..Default::default()
    };
    // Looking down from the front-right: both top faces face up.
    camera.yaw = -0.8;
    camera.pitch = 0.9;
    let points: Vec<[f64; 3]> = project
        .boards
        .iter()
        .filter_map(|b| plan_my_cabinet::render::camera::board_corners(&project, b))
        .flatten()
        .collect();
    fit_camera(&mut camera, &points, 400, 300, 0.9);
    for light in [Light::studio(), Light::headlight(&camera), Light::OFF] {
        let top = |c: [f64; 3]| pixel(&project, &camera, light, c);
        assert_eq!(top(centres[0]), top(centres[1]), "{light:?}");
    }
}

#[test]
fn the_headlight_brightens_the_face_you_look_at() {
    let (project, centres) = twins();
    let mut camera = Camera {
        projection: Projection::Orthographic,
        ..Default::default()
    };
    camera.pitch = 1.2;
    let points: Vec<[f64; 3]> = project
        .boards
        .iter()
        .filter_map(|b| plan_my_cabinet::render::camera::board_corners(&project, b))
        .flatten()
        .collect();
    fit_camera(&mut camera, &points, 400, 300, 0.9);
    // From above, the headlight lights the tops more than the studio light,
    // which comes in from the front at an angle.
    let lit = pixel(&project, &camera, Light::headlight(&camera), centres[0]);
    let studio = pixel(&project, &camera, Light::studio(), centres[0]);
    assert!(lit[0] > studio[0], "{lit:?} {studio:?}");
}
