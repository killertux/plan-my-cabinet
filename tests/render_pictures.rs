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
