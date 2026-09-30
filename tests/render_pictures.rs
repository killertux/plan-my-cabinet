//! Headless pictures: deterministic, correctly sized, and honoring
//! hidden/highlighted objects. Set PMCAB_RENDER_DUMP=DIR to look at them.
use std::collections::HashSet;

use plan_my_cabinet::reference_fixture;
use plan_my_cabinet::render::camera::Projection;
use plan_my_cabinet::render::picture::{PictureRequest, View, render_picture};

fn dump(name: &str, png: &[u8]) {
    if let Some(dir) = std::env::var_os("PMCAB_RENDER_DUMP") {
        std::fs::write(std::path::Path::new(&dir).join(name), png).unwrap();
    }
}

fn png_size(png: &[u8]) -> (u32, u32) {
    assert_eq!(&png[1..4], b"PNG");
    let w = u32::from_be_bytes(png[16..20].try_into().unwrap());
    let h = u32::from_be_bytes(png[20..24].try_into().unwrap());
    (w, h)
}

#[test]
fn pictures_are_deterministic_sized_and_labelled() {
    let project = reference_fixture::project();
    let request = PictureRequest {
        width: 800,
        height: 600,
        ..Default::default()
    };
    let first = render_picture(&project, &request).unwrap();
    let second = render_picture(&project, &request).unwrap();
    assert_eq!(first.png, second.png);
    assert_eq!(png_size(&first.png), (800, 600));
    assert_eq!(
        first.legend.len(),
        project.boards.len() + project.hardware.len()
    );
    assert!(first.legend.iter().filter(|e| e.number.is_some()).count() >= 5);
    dump("iso.png", &first.png);
    for (name, view, projection) in [
        ("front.png", View::Front, Projection::Orthographic),
        ("top.png", View::Top, Projection::Orthographic),
        ("iso_back.png", View::IsoBack, Projection::Perspective),
        ("right.png", View::Right, Projection::Orthographic),
    ] {
        let picture = render_picture(
            &project,
            &PictureRequest {
                view,
                projection,
                width: 640,
                height: 480,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(png_size(&picture.png), (640, 480));
        dump(name, &picture.png);
    }
}

#[test]
fn hidden_objects_disappear_and_highlight_changes_pixels() {
    let project = reference_fixture::project();
    let base = render_picture(&project, &PictureRequest::default()).unwrap();
    let door = project
        .boards
        .iter()
        .find(|b| b.name.to_lowercase().contains("door"))
        .expect("fixture door")
        .id;
    let hidden = render_picture(
        &project,
        &PictureRequest {
            hidden: HashSet::from([door]),
            ..Default::default()
        },
    )
    .unwrap();
    assert_ne!(base.png, hidden.png);
    assert!(hidden.legend.iter().all(|e| e.id != door));
    dump("no_door.png", &hidden.png);
    let highlighted = render_picture(
        &project,
        &PictureRequest {
            highlight: HashSet::from([door]),
            ..Default::default()
        },
    )
    .unwrap();
    assert_ne!(base.png, highlighted.png);
    dump("highlight.png", &highlighted.png);
}

#[test]
fn sheet_diagrams_show_parts_and_numbered_cuts() {
    use plan_my_cabinet::render::sheet::{PartLabels, render_sheet};
    use plan_my_cabinet::stock_read_models::StockReadModel;
    let project = reference_fixture::project();
    let model = StockReadModel::build(&project).unwrap();
    let mut drawn = 0;
    for piece in model.pieces.iter().filter(|p| p.is_used()) {
        let picture = render_sheet(piece, PartLabels::Names, true, 1000).unwrap();
        assert_eq!(picture.width, 1000);
        assert_eq!(picture.legend.len(), piece.parts.len());
        dump(&format!("sheet_{}.png", piece.alias), &picture.png);
        drawn += 1;
    }
    assert!(drawn > 0);
}

#[test]
fn scene_description_reports_contacts_and_intersections() {
    use plan_my_cabinet::read_models::scene_description::{DescribeOptions, describe};
    use plan_my_cabinet::render::camera::Selection;
    let mut project = reference_fixture::project();
    let described = describe(&project, &Selection::default(), DescribeOptions::default());
    println!("{}", described.text);
    assert_eq!(
        described.object_count,
        project.boards.len() + project.hardware.len()
    );
    assert!(described.overlaps.is_empty(), "{:?}", described.overlaps);
    assert!(!described.contacts.is_empty());
    // Push a copy of the first board into the second: they must now intersect.
    let mut copy = project.boards[0].duplicate();
    copy.name = "Intruder".into();
    copy.pose.translation_mm[0] += 5.0;
    project.boards.push(copy);
    let described = describe(&project, &Selection::default(), DescribeOptions::default());
    assert!(
        described
            .overlaps
            .iter()
            .any(|o| o.a_name == "Intruder" || o.b_name == "Intruder")
    );
    assert!(described.text.contains("OVERLAP"));
}

mod hardware {
    use std::collections::{HashMap, HashSet};

    use plan_my_cabinet::catalog_pack::{CatalogRegistry, snapshot_foot};
    use plan_my_cabinet::commands::ProjectEditor;
    use plan_my_cabinet::domain::{BoardGrain, Project};
    use plan_my_cabinet::money::Currency;
    use plan_my_cabinet::render::camera::Projection;
    use plan_my_cabinet::render::picture::{PictureRequest, View, render_picture};
    use plan_my_cabinet::template_setup::{
        MaterialRole, ProposedLength, TemplateField, TemplateKind, TemplateSetup,
    };
    use plan_my_cabinet::units::{Conversion, Length, Pose, Quaternion, Unit};
    use uuid::Uuid;

    use super::dump;

    fn mm(value: i64) -> ProposedLength {
        ProposedLength::new(Conversion::Exact(Length::from_micrometres(value * 1000)))
    }

    fn chest() -> ProjectEditor {
        let mut stage = TemplateSetup::new(TemplateKind::Drawers, "Chest", Currency::Brl, Unit::Mm);
        for (field, value) in [
            (TemplateField::Width, 600),
            (TemplateField::Depth, 560),
            (TemplateField::Height, 720),
            (TemplateField::BoxDepth, 500),
            (TemplateField::SideClearance, 13),
            (TemplateField::RearClearance, 20),
            (TemplateField::VerticalClearance, 8),
            (TemplateField::FrontReveal, 3),
            (TemplateField::FrontGap, 3),
        ] {
            stage.dimensions.insert(field, mm(value));
        }
        let carcass = stage.add_material("Carcass", mm(18), BoardGrain::Length, None);
        let back = stage.add_material("Back", mm(6), BoardGrain::Unrestricted, None);
        let front = stage.add_material("Front", mm(18), BoardGrain::Length, None);
        for (role, id) in [
            (MaterialRole::Carcass, carcass),
            (MaterialRole::Back, back),
            (MaterialRole::Box, carcass),
            (MaterialRole::BoxBottom, back),
            (MaterialRole::ExternalFront, front),
        ] {
            stage.roles.insert(role, id);
        }
        stage.drawer_count = Some(3);
        stage.generate().unwrap().editor
    }

    fn foot_entry(family: &str, code: &str) -> plan_my_cabinet::domain::CatalogReference {
        let registry = CatalogRegistry::bundled();
        let pack = registry.pack("generic-feet").unwrap();
        let family = pack.feet.iter().find(|f| f.id == family).unwrap();
        let variant = family.variants.iter().find(|v| v.code == code).unwrap();
        snapshot_foot(pack, family, variant, "en")
    }

    fn at(x: f64, y: f64, z: f64) -> Pose {
        Pose::new([x, y, z], Quaternion::IDENTITY).unwrap()
    }

    /// The chest raised onto four square chrome feet.
    fn chest_on_feet() -> ProjectEditor {
        let mut editor = chest();
        let root = editor.project().assemblies[0].id;
        editor
            .transact(|p| -> Result<(), ()> {
                p.assemblies[0].pose.translation_mm[2] += 100.0;
                Ok(())
            })
            .unwrap();
        let entry = foot_entry("post-square", "generic-post-square-100");
        let catalog_id = entry.id;
        let mut pin = Some(entry);
        for (i, (x, y)) in [(40.0, 40.0), (560.0, 40.0), (40.0, 490.0), (560.0, 490.0)]
            .into_iter()
            .enumerate()
        {
            editor
                .create_foot(
                    format!("Foot {}", i + 1),
                    catalog_id,
                    pin.take(),
                    Some(root),
                    at(x - 30.0, y - 30.0, 0.0),
                )
                .unwrap();
        }
        editor
    }

    #[test]
    fn feet_and_slides_are_drawn_and_listed() {
        let editor = chest_on_feet();
        let p = editor.project();
        let picture = render_picture(p, &PictureRequest::default()).unwrap();
        dump("chest-feet-iso.png", &picture.png);
        let kinds = |kind: &str| picture.legend.iter().filter(|e| e.kind == kind).count();
        assert_eq!(kinds("foot"), 4);
        assert_eq!(kinds("slide"), 3);
        // From below the feet cover real pixels.
        let below = render_picture(
            p,
            &PictureRequest {
                view: View::Angles {
                    yaw_degrees: 30.0,
                    pitch_degrees: -25.0,
                },
                ..Default::default()
            },
        )
        .unwrap();
        dump("chest-feet-below.png", &below.png);
        assert!(
            below
                .legend
                .iter()
                .filter(|e| e.kind == "foot")
                .all(|e| e.visible_percent > 0.0)
        );
        // Without the fronts and the side facing the camera, the slides show.
        let hidden: HashSet<Uuid> = p
            .boards
            .iter()
            .filter(|b| b.name.contains("external front") || b.name == "Right side")
            .map(|b| b.id)
            .collect();
        let inside = render_picture(
            p,
            &PictureRequest {
                hidden: hidden.clone(),
                ..Default::default()
            },
        )
        .unwrap();
        dump("chest-slides.png", &inside.png);
        assert!(
            inside
                .legend
                .iter()
                .filter(|e| e.kind == "slide")
                .all(|e| e.visible_percent > 0.05)
        );
        // Opening a drawer moves its box and slide members.
        let slide = &p.slide_installations[1];
        let poses: HashMap<Uuid, Pose> =
            plan_my_cabinet::slide_installation::derived_poses(p, slide, 400.0)
                .unwrap()
                .into_iter()
                .collect();
        let open = render_picture(
            p,
            &PictureRequest {
                hidden,
                poses: Some(&poses),
                ..Default::default()
            },
        )
        .unwrap();
        dump("chest-open.png", &open.png);
        assert_ne!(open.png, inside.png);
    }

    #[test]
    fn every_generic_foot_renders_like_its_product() {
        let registry = CatalogRegistry::bundled();
        let pack = registry.pack("generic-feet").unwrap();
        let mut editor = ProjectEditor::new(Project::new("Feet", Currency::Brl)).unwrap();
        let mut x = 0.0;
        for family in &pack.feet {
            let variant = &family.variants[0];
            let entry = snapshot_foot(pack, family, variant, "en");
            let size = family.spec(variant).local_size();
            let id = entry.id;
            editor
                .create_foot(family.id.clone(), id, Some(entry), None, at(x, 0.0, 0.0))
                .unwrap();
            x += size[0].micrometres() as f64 / 1000.0 + 120.0;
        }
        let p = editor.project();
        for (name, view, projection) in [
            ("feet-iso.png", View::Iso, Projection::Perspective),
            ("feet-front.png", View::Front, Projection::Orthographic),
        ] {
            let picture = render_picture(
                p,
                &PictureRequest {
                    view,
                    projection,
                    width: 1600,
                    height: 700,
                    ..Default::default()
                },
            )
            .unwrap();
            dump(name, &picture.png);
            assert_eq!(picture.legend.len(), pack.feet.len());
            assert!(picture.legend.iter().all(|e| e.kind == "foot"));
        }
        // Small feet on their own.
        let small: HashSet<Uuid> = p
            .hardware
            .iter()
            .filter(|h| h.name.starts_with("tapered") || h.name.starts_with("post"))
            .map(|h| h.id)
            .collect();
        let picture = render_picture(
            p,
            &PictureRequest {
                frame: small,
                width: 900,
                height: 500,
                ..Default::default()
            },
        )
        .unwrap();
        dump("feet-small.png", &picture.png);
    }
}
