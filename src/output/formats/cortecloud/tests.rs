use super::*;
use crate::domain::BoardFace;
use crate::machining::{BoardMachining, DrillSource, FaceDrill};
use crate::part_list::{BandRef, PartGroup};
use uuid::Uuid;

fn mm(n: f64) -> Length {
    Length::from_micrometres((n * 1000.0).round() as i64)
}

#[test]
fn the_example_files_parse_and_write_back_unchanged() {
    for source in [
        include_str!("../../../../tests/fixtures/cortecloud/exemplo.json"),
        include_str!("../../../../tests/fixtures/cortecloud/exemplo-2.json"),
    ] {
        let parsed: File = serde_json::from_str(source).unwrap();
        let original: serde_json::Value = serde_json::from_str(source).unwrap();
        let written = serde_json::to_value(&parsed).unwrap();
        assert_eq!(written, original);
    }
}

#[test]
fn sides_and_axes_follow_the_grain_and_the_inner_face() {
    use BoardEdge::*;
    // [c1, c2, l1, l2] for each way a board can lie.
    for (c_along_y, inner, sides) in [
        (false, BoardFace::MaxZ, [MinY, MaxY, MaxX, MinX]),
        (false, BoardFace::MinZ, [MaxY, MinY, MaxX, MinX]),
        (true, BoardFace::MinZ, [MinX, MaxX, MaxY, MinY]),
        (true, BoardFace::MaxZ, [MaxX, MinX, MaxY, MinY]),
    ] {
        let orientation = Orientation { c_along_y, inner };
        assert_eq!(orientation.sides(), sides, "{orientation:?}");
        // A point near C1 and L2 in part terms sits at those board edges.
        let (length, width) = (mm(700.0), mm(400.0));
        let near = |edge: BoardEdge| match edge {
            MinX => 0,
            MaxX => i128::from(length.micrometres()),
            MinY => 0,
            MaxY => i128::from(width.micrometres()),
        };
        let mut local = [0i128; 2];
        for edge in [sides[0], sides[3]] {
            local[1 - edge.along_axis()] = near(edge);
        }
        assert_eq!(
            orientation.part_xy(local, length, width),
            [0, 0],
            "{orientation:?}"
        );
    }
}

#[test]
fn holes_are_measured_from_their_nearest_corner() {
    let (c, l) = (300_000, 300_000);
    assert_eq!(
        from_nearest_corner([10_000, 20_000], c, l),
        (0, 10_000, 20_000)
    );
    assert_eq!(
        from_nearest_corner([290_000, 20_000], c, l),
        (1, 10_000, 20_000)
    );
    assert_eq!(
        from_nearest_corner([200_000, 278_500], c, l),
        (2, 100_000, 21_500)
    );
    assert_eq!(
        from_nearest_corner([100_000, 278_500], c, l),
        (3, 100_000, 21_500)
    );
}

fn door() -> PartGroup {
    let band = BandRef {
        id: Uuid::nil(),
        name: "Fita Branca 1x22".into(),
        thickness: mm(1.0),
        height: mm(22.0),
    };
    let cup = |x: f64| FaceDrill {
        face: BoardFace::MinZ,
        at_um: [(x * 1000.0) as i128, 21_500],
        diameter: mm(35.0),
        depth: mm(12.0),
        through: false,
        source: DrillSource::HingeCup(Uuid::nil()),
    };
    PartGroup {
        name: "Porta".into(),
        cabinet: Some("Balcão 800".into()),
        material_id: Uuid::nil(),
        material: "MDF Branco".into(),
        length: mm(716.0),
        width: mm(397.0),
        thickness: mm(18.0),
        grain: BoardGrain::Length,
        banding: std::array::from_fn(|_| Some(band.clone())),
        machining: BoardMachining {
            face_drills: vec![cup(100.0), cup(616.0)],
            edge_drills: Vec::new(),
        },
        boards: vec![Uuid::nil(), Uuid::from_u128(1)],
    }
}

#[test]
fn a_door_with_hinge_cups_matches_cortecloud_s_own_door_example() {
    let written = serde_json::to_value(part(&door())).unwrap();
    // As in example 2: the cups sit 21.5 mm from C2, 100 mm from the ends,
    // measured from corners 2 and 3, on the inner face.
    assert_eq!(
        written,
        serde_json::json!({
            "quantity": 2,
            "c": 716,
            "l": 397,
            "function": "Porta",
            "complement": "Balcão 800",
            "c1": "Fita Branca 1x22",
            "c2": "Fita Branca 1x22",
            "l1": "Fita Branca 1x22",
            "l2": "Fita Branca 1x22",
            "material": "MDF Branco 18",
            "machining": {
                "x": 716, "y": 397, "z": 18, "startSide": 0,
                "horizontalDrills": [],
                "verticalDrills": [
                    { "x": 100, "y": 21.5, "face": "i", "depth": 12, "corner": 3,
                      "diameter": 35, "bolthole": false },
                    { "x": 100, "y": 21.5, "face": "i", "depth": 12, "corner": 2,
                      "diameter": 35, "bolthole": false }
                ],
                "furrowMachining": null,
                "furrowMachiningPair": null
            }
        })
    );
}

#[test]
fn width_grain_turns_the_part_and_its_holes() {
    let mut group = door();
    group.grain = BoardGrain::Width;
    group.banding = [None, None, None, None];
    group.banding[BoardEdge::MinY.index()] = door().banding[0].clone();
    let part = part(&group);
    assert_eq!((part.c, part.l), (397.0, 716.0));
    let machining = part.machining.unwrap();
    assert_eq!(
        (machining.x, machining.y, machining.z),
        (397.0, 716.0, 18.0)
    );
    // The hinge edge (MinY) now runs along L.
    assert_eq!(
        [part.c1, part.c2, part.l1, part.l2].map(|b| b.is_some()),
        [false, false, false, true]
    );
    for drill in machining.vertical_drills {
        assert_eq!(drill.x, 21.5);
        assert_eq!(drill.y, 100.0);
    }
}

#[test]
fn parts_without_holes_have_no_machining_and_names_keep_their_thickness() {
    let mut group = door();
    group.machining = BoardMachining::default();
    group.material = "MDF Branco 18 TX".into();
    let part = part(&group);
    assert!(part.machining.is_none());
    assert_eq!(part.material, "MDF Branco 18 TX");
    let json = serde_json::to_string(&part).unwrap();
    assert!(!json.contains("machining"), "{json}");
}
