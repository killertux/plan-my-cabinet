use super::*;
use crate::domain::{Board, BoardGrain, EdgeBand, Material, MaterialKind, SrgbColor};
use crate::i18n::Language;
use crate::money::Currency;
use crate::template_setup::{ProposedLength, TemplateField, TemplateKind, TemplateSetup};
use crate::units::{Conversion, Length, Pose, Quaternion, Unit};

fn mm(n: i64) -> Length {
    Length::from_micrometres(n * 1000)
}

fn template(kind: TemplateKind) -> Project {
    let mut stage = TemplateSetup::new(kind, "Cabinet", Currency::Brl, Unit::Mm);
    let fields = [
        (TemplateField::Width, 600),
        (TemplateField::Depth, 560),
        (TemplateField::Height, 720),
        (TemplateField::RailWidth, 80),
        (TemplateField::ShelfHeight, 320),
        (TemplateField::BoxDepth, 500),
        (TemplateField::SideClearance, 13),
        (TemplateField::RearClearance, 20),
        (TemplateField::VerticalClearance, 8),
        (TemplateField::FrontReveal, 3),
        (TemplateField::FrontGap, 3),
    ];
    for (field, value) in fields {
        if kind.fields().contains(&field) {
            stage
                .dimensions
                .insert(field, ProposedLength::new(Conversion::Exact(mm(value))));
        }
    }
    stage.seed_standard_materials(Language::En);
    if kind == TemplateKind::Drawers {
        stage.drawer_count = Some(2);
    }
    stage.generate().unwrap().editor.project().clone()
}

/// Which edges are banded, per board name, in `BoardEdge::ALL` order
/// (MinX, MaxX, MinY, MaxY).
fn banded(project: &Project) -> Vec<(String, [bool; 4])> {
    let states = effective(project);
    project
        .boards
        .iter()
        .map(|b| (b.name.clone(), states[&b.id].map(|s| s.band.is_some())))
        .collect()
}

fn row<'a>(rows: &'a [(String, [bool; 4])], name: &str) -> [bool; 4] {
    rows.iter().find(|(n, _)| n == name).unwrap().1
}

#[test]
fn automatic_banding_bands_the_free_edges_of_every_template() {
    // Sides: MinX bottom, MaxX top, MinY front, MaxY rear (against the back).
    let base = banded(&template(TemplateKind::Base));
    assert_eq!(row(&base, "Left side"), [true, true, true, false]);
    assert_eq!(row(&base, "Right side"), [true, true, true, false]);
    // The bottom sits between the sides and on the back: only its front shows.
    assert_eq!(row(&base, "Bottom"), [false, false, true, false]);
    assert_eq!(row(&base, "Rear top rail"), [false, false, true, false]);
    // The HDF back takes no banding.
    assert_eq!(row(&base, "Overlay back"), [false; 4]);

    let wall = banded(&template(TemplateKind::Wall));
    for name in ["Bottom", "Top", "Shelf"] {
        assert_eq!(row(&wall, name), [false, false, true, false], "{name}");
    }

    let chest = banded(&template(TemplateKind::Drawers));
    // A closed drawer front does not hide the carcass edges behind it.
    assert_eq!(row(&chest, "Bottom"), [false, false, true, false]);
    assert_eq!(row(&chest, "Top"), [false, false, true, false]);
    assert_eq!(row(&chest, "Left side"), [true, true, true, false]);
    assert_eq!(row(&chest, "Drawer 1 external front"), [true; 4]);
    // Box sides show their top edge and rear; front and bottom are joined.
    assert_eq!(
        row(&chest, "Drawer 1 left box side"),
        [false, true, false, true]
    );
}

#[test]
fn a_material_without_a_default_band_or_banding_leaves_auto_edges_bare() {
    let mut project = template(TemplateKind::Base);
    for material in &mut project.materials {
        material.default_band = None;
    }
    assert!(
        banded(&project)
            .iter()
            .all(|(_, edges)| *edges == [false; 4])
    );
    let states = effective(&project);
    let side = project
        .boards
        .iter()
        .find(|b| b.name == "Left side")
        .unwrap();
    // The contact is still reported, for the inspector to explain.
    assert!(states[&side.id][3].contact.is_joined());
    assert_eq!(states[&side.id][2].contact, Contact::Free);
}

#[test]
fn manual_overrides_win_over_the_rule() {
    let mut project = template(TemplateKind::Base);
    let band = project.edge_bands[0].id;
    let side = project
        .boards
        .iter()
        .position(|b| b.name == "Left side")
        .unwrap();
    project.boards[side]
        .banding
        .set(BoardEdge::MinY, EdgeBanding::Off);
    project.boards[side]
        .banding
        .set(BoardEdge::MaxY, EdgeBanding::On(band));
    project.validate().unwrap();
    let states = effective(&project)[&project.boards[side].id];
    assert_eq!(states[2].band, None);
    assert_eq!(states[3].band, Some(band));
    assert!(states[3].is_manual() && states[3].contact.is_joined());
}

/// Two 18 mm boards: `a` lies flat, `b` stands against `a`'s MaxX edge face
/// with `gap` mm between them, covering `cover` mm of the 100 mm edge.
fn pair(gap: f64, cover: i64) -> (Project, Uuid) {
    let mut project = Project::new("Pair", Currency::Brl);
    let band = Uuid::new_v4();
    project.edge_bands.push(EdgeBand {
        id: band,
        name: "Band".into(),
        thickness: mm(1),
        height: mm(22),
        color: SrgbColor([255; 3]),
    });
    let material = Uuid::new_v4();
    project.materials.push(Material {
        id: material,
        name: "MDF".into(),
        default_thickness: mm(18),
        default_grain: BoardGrain::Unrestricted,
        kind: MaterialKind::Mdf,
        default_band: Some(band),
    });
    let board = |name: &str, size: [i64; 3], at: [f64; 3]| Board {
        id: Uuid::new_v4(),
        name: name.into(),
        material_id: material,
        length: mm(size[0]),
        width: mm(size[1]),
        thickness: mm(size[2]),
        grain_override: None,
        parent_id: None,
        pose: Pose::new(at, Quaternion::IDENTITY).unwrap(),
        banding: Default::default(),
    };
    // `a`: 300 × 100 × 18. Its MaxX edge face is x = 300, y 0..100, z 0..18.
    let a = board("a", [300, 100, 18], [0.0; 3]);
    // `b`: 18 thick along x, `cover` deep along y, tall along z.
    let b = board("b", [18, cover, 200], [300.0 + gap, 0.0, -50.0]);
    let id = a.id;
    project.boards.extend([a, b]);
    project.validate().unwrap();
    (project, id)
}

#[test]
fn half_the_face_within_half_a_millimetre_joins_an_edge() {
    let max_x = |project: &Project, id| effective(project)[&id][1];
    let (project, a) = pair(0.0, 50);
    assert!(max_x(&project, a).contact.is_joined());
    assert_eq!(max_x(&project, a).band, None);
    let (project, a) = pair(0.0, 49);
    assert!(
        matches!(max_x(&project, a).contact, Contact::Partly { coverage, .. } if (coverage - 0.49).abs() < 1e-9)
    );
    assert!(max_x(&project, a).band.is_some());
    let (project, a) = pair(0.5, 100);
    assert!(max_x(&project, a).contact.is_joined());
    let (project, a) = pair(0.6, 100);
    assert_eq!(max_x(&project, a).contact, Contact::Free);
    // A slight overlap is still a joint.
    let (project, a) = pair(-0.3, 100);
    assert!(max_x(&project, a).contact.is_joined());
    // The other edges of `a` stay free.
    let (project, a) = pair(0.0, 100);
    let states = effective(&project)[&a];
    assert!(
        [0, 2, 3]
            .iter()
            .all(|&i| states[i].contact == Contact::Free)
    );
}

#[test]
fn front_edges_are_those_facing_minus_y() {
    let project = template(TemplateKind::Base);
    let side = project
        .boards
        .iter()
        .find(|b| b.name == "Left side")
        .unwrap();
    let frame = BoardFrame::new(&project, side.id).unwrap();
    assert_eq!(edges_facing(&frame, FRONT), [BoardEdge::MinY]);
    let back = project
        .boards
        .iter()
        .find(|b| b.name == "Overlay back")
        .unwrap();
    let frame = BoardFrame::new(&project, back.id).unwrap();
    assert!(edges_facing(&frame, FRONT).is_empty());
}
