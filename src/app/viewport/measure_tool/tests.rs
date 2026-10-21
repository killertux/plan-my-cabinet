use super::*;
use plan_my_cabinet::domain::Board;
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::units::{Pose, Quaternion};

/// One 100 × 100 × 20 board at the origin, seen from the top: screen right
/// is world +X and screen up is world +Y.
struct Scene {
    ctx: egui::Context,
    project: Project,
    camera: Camera,
    selection: Selection,
    tool: MoveTool,
    rect: egui::Rect,
    texts: Vec<(String, egui::Pos2)>,
}

impl Scene {
    fn new() -> Self {
        let mut project = Project::new("Measure", Currency::Brl);
        project.boards.push(Board {
            coated_face: Default::default(),
            banding: Default::default(),
            id: Uuid::from_u128(7),
            name: "Top".into(),
            material_id: Uuid::new_v4(),
            length: Length::from_micrometres(100_000),
            width: Length::from_micrometres(100_000),
            thickness: Length::from_micrometres(20_000),
            grain_override: None,
            parent_id: None,
            pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
        });
        let mut camera = Camera {
            target: [50.0, 50.0, 10.0],
            distance: 400.0,
            projection: Projection::Orthographic,
            ..Camera::default()
        };
        camera.set_preset(Preset::Top);
        let mut scene = Self {
            ctx: egui::Context::default(),
            project,
            camera,
            selection: Selection::default(),
            tool: MoveTool {
                mode: ToolMode::Measure,
                ..Default::default()
            },
            rect: egui::Rect::NOTHING,
            texts: Vec::new(),
        };
        scene.frame(vec![]);
        scene.frame(vec![]);
        scene
    }

    fn frame(&mut self, events: Vec<egui::Event>) {
        let mut selection_proposed = false;
        let output = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 700.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    self.rect = ui.available_rect_before_wrap();
                    let interaction = show_move(
                        ui,
                        &mut self.camera,
                        &self.project,
                        &mut self.selection,
                        &mut self.tool,
                        false,
                        Language::En,
                        false,
                        true,
                        None,
                        None,
                    );
                    selection_proposed = interaction.selection.is_some();
                });
            },
        );
        assert!(!selection_proposed, "Measure clicks must not select");
        self.texts = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some((text.galley.text().to_owned(), text.pos)),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
    }

    fn at(&self, point: [f64; 3]) -> egui::Pos2 {
        self.camera.project(point, self.rect).unwrap()
    }

    fn click(&mut self, pos: egui::Pos2) {
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.frame(vec![egui::Event::PointerMoved(pos)]);
        self.frame(vec![button(true)]);
        self.frame(vec![button(false)]);
        self.frame(vec![]);
    }

    fn key(&mut self, key: egui::Key) {
        self.frame(vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
    }

    fn text(&self, wanted: &str) -> Option<egui::Pos2> {
        self.texts
            .iter()
            .find(|(text, _)| text == wanted)
            .map(|(_, pos)| *pos)
    }
}

#[test]
fn two_snapped_clicks_keep_a_measurement_without_selecting_or_editing() {
    let mut scene = Scene::new();
    let original = scene.project.clone();
    scene.click(scene.at([0.0, 0.0, 20.0]) + egui::vec2(4.0, -3.0));
    assert!(scene.tool.measure.in_progress());
    scene.click(scene.at([100.0, 0.0, 20.0]) + egui::vec2(-3.0, 2.0));
    assert!(!scene.tool.measure.in_progress());
    let kept = scene.tool.measure.kept();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].kinds, [SnapKind::Endpoint, SnapKind::Endpoint]);
    assert_eq!(kept[0].a.resolve(&scene.project), Some([0.0, 0.0, 20.0]));
    assert!(scene.text("100 mm").is_some(), "{:?}", scene.texts);
    assert!(scene.text("ΔX 100 mm").is_some());
    assert!(scene.text("ΔY 0 mm").is_some());
    assert!(scene.selection.ids.is_empty());
    assert_eq!(scene.project, original);
}

#[test]
fn kept_measurements_follow_their_board() {
    let mut scene = Scene::new();
    scene.click(scene.at([0.0, 0.0, 20.0]));
    scene.click(scene.at([100.0, 100.0, 20.0]));
    scene.project.boards[0].pose = Pose::new([10.0, 0.0, 0.0], Quaternion::IDENTITY).unwrap();
    scene.frame(vec![]);
    let kept = scene.tool.measure.kept()[0];
    assert_eq!(kept.b.resolve(&scene.project), Some([110.0, 100.0, 20.0]));
    scene.project.boards.clear();
    scene.frame(vec![]);
    assert!(scene.tool.measure.kept().is_empty());
}

#[test]
fn axis_keys_toggle_and_escape_cancels_then_clears() {
    let mut scene = Scene::new();
    scene.frame(vec![egui::Event::PointerMoved(scene.rect.center())]);
    scene.key(egui::Key::Z);
    assert_eq!(scene.tool.measure.axis_lock(), Some(2));
    assert!(scene.text("Z · locked").is_some());
    scene.key(egui::Key::X);
    assert_eq!(scene.tool.measure.axis_lock(), Some(0));
    scene.key(egui::Key::X);
    assert_eq!(scene.tool.measure.axis_lock(), None);

    scene.click(scene.at([0.0, 0.0, 20.0]));
    scene.click(scene.at([100.0, 0.0, 20.0]));
    scene.click(scene.at([0.0, 100.0, 20.0]));
    assert!(scene.tool.measure.in_progress());
    scene.key(egui::Key::Escape);
    assert!(!scene.tool.measure.in_progress());
    assert_eq!(scene.tool.measure.kept().len(), 1);
    scene.key(egui::Key::Escape);
    assert!(scene.tool.measure.kept().is_empty());

    scene.click(scene.at([0.0, 0.0, 20.0]));
    scene.click(scene.at([100.0, 0.0, 20.0]));
    scene.key(egui::Key::Delete);
    assert!(scene.tool.measure.kept().is_empty());
}

#[test]
fn readout_docks_bottom_left_clear_of_the_camera_bar_and_snap_chip() {
    let mut scene = Scene::new();
    scene.click(scene.at([0.0, 0.0, 20.0]));
    scene.click(scene.at([100.0, 0.0, 20.0]));
    let distance = scene.text("Distance").expect("measure panel");
    let chip = scene
        .texts
        .iter()
        .find(|(text, _)| text.starts_with("Face + grid"))
        .map(|(_, pos)| *pos)
        .expect("snap chip");
    let iso = scene.text("Iso").expect("camera bar");
    assert!(distance.y > scene.rect.center().y, "{distance:?}");
    assert!(distance.x < scene.rect.center().x);
    assert!(chip.y < scene.rect.top() + 60.0 && iso.y < scene.rect.top() + 60.0);
    // The old top-right bounding-box card is gone from the canvas.
    assert!(
        !scene
            .texts
            .iter()
            .any(|(text, _)| text.contains("X × Y × Z"))
    );
}

#[test]
fn lengths_use_the_display_unit_and_locale() {
    assert_eq!(length_text(642.5, Unit::Mm, Locale::En), "642.5 mm");
    assert_eq!(length_text(642.5, Unit::Mm, Locale::PtBr), "642,5 mm");
    assert_eq!(length_text(600.0, Unit::Mm, Locale::En), "600 mm");
    assert_eq!(length_text(254.0, Unit::Inch, Locale::PtBr), "10 in");
    assert_eq!(length_text(12.345, Unit::Cm, Locale::En), "1.235 cm");
}
