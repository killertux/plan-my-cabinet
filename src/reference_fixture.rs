//! Deterministic, opt-in cabinet for redesign captures and regression checks.
//! See `docs/redesign-reference-fixture.md` for handoff corrections and datums.
use uuid::Uuid;

use crate::domain::{
    Allocation, Assembly, Board, BoardEdge, BoardFace, BoardGrain, HingeInstallation,
    HingeMountingSide, Material, Project, Stock, StockGrain, StockSource,
};
use crate::money::{Currency, Money};
use crate::units::{Length, Pose, Quaternion};
use crate::{door_joint, hardware_catalog};

const fn id(n: u128) -> Uuid {
    Uuid::from_u128(0x72656465_7369_476e_8000_000000000000 | n)
}

pub const PROJECT_ID: Uuid = id(1);
pub const CARCASS_ID: Uuid = id(2);
pub const DOORS_ID: Uuid = id(3);
pub const WHITE_ID: Uuid = id(0x10);
pub const OAK_ID: Uuid = id(0x11);
pub const HDF_ID: Uuid = id(0x12);
pub const LEFT_SIDE_ID: Uuid = id(0x12ad);
pub const RIGHT_SIDE_ID: Uuid = id(0x40e2);
pub const BOTTOM_ID: Uuid = id(0x5b17);
pub const FRONT_RAIL_ID: Uuid = id(0x6c03);
pub const BACK_RAIL_ID: Uuid = id(0x6c9e);
pub const SHELF_ID: Uuid = id(0x7f3a);
pub const BACK_ID: Uuid = id(0x91c0);
pub const LEFT_DOOR_ID: Uuid = id(0xa711);
pub const RIGHT_DOOR_ID: Uuid = id(0xa7f4);
pub const WHITE_STOCK_ID: Uuid = id(0x20);
pub const OWNED_STOCK_ID: Uuid = id(0x21);
pub const SPARE_STOCK_ID: Uuid = id(0x22);
pub const OAK_STOCK_ID: Uuid = id(0x23);
pub const CATALOG_ID: Uuid = id(0x30);
pub const HINGE_IDS: [Uuid; 4] = [id(0x31), id(0x32), id(0x33), id(0x34)];
pub const LEFT_JOINT_ID: Uuid = id(0x35);
pub const RIGHT_JOINT_ID: Uuid = id(0x36);
pub const ALLOCATION_IDS: [Uuid; 8] = [
    id(0x40),
    id(0x41),
    id(0x42),
    id(0x43),
    id(0x44),
    id(0x45),
    id(0x46),
    id(0x47),
];

fn mm(n: i64) -> Length {
    Length::from_micrometres(n * 1000)
}

fn pose(at: [f64; 3], rotation: Quaternion) -> Pose {
    Pose::new(at, rotation).expect("fixed reference pose")
}

/// A fresh independent project with stable identities and closed door poses.
/// Allocations are fixed reference inputs, verified by the ordinary cut solver
/// in the integration tests; no cut tree or derived statistics are fabricated.
pub fn project() -> Project {
    let mut p = Project::new("Kitchen base 800", Currency::Brl);
    p.id = PROJECT_ID;
    p.confirmed_shop_kerf = Some(p.cutting_kerf);
    for (id, name, thickness, grain) in [
        (WHITE_ID, "MDF White", 18, BoardGrain::Length),
        (OAK_ID, "MDF Oak veneer", 18, BoardGrain::Length),
        (HDF_ID, "HDF", 3, BoardGrain::Unrestricted),
    ] {
        p.materials.push(Material {
            coating: crate::domain::Coating::infer(name),
            default_band: None,
            kind: crate::domain::MaterialKind::infer(name),
            id,
            name: name.into(),
            default_thickness: mm(thickness),
            default_grain: grain,
        });
    }
    for (id, name) in [(CARCASS_ID, "Carcass"), (DOORS_ID, "Doors")] {
        p.assemblies.push(Assembly {
            id,
            name: name.into(),
            parent_id: None,
            pose: pose([0.; 3], Quaternion::IDENTITY),
        });
    }
    // Side local X is depth, Y is height, Z is thickness, so mounting Y
    // references use the same vertical axis as the doors.
    let side = Quaternion::normalized(0.5, 0.5, 0.5, 0.5).expect("constant unit quaternion");
    let vertical = Quaternion::normalized(1., 1., 0., 0.).expect("constant quaternion");
    for (id, name, material_id, size, at, rotation, grain_override) in [
        (
            LEFT_SIDE_ID,
            "Left side",
            WHITE_ID,
            [560, 720, 18],
            [0., 0., 0.],
            side,
            Some(BoardGrain::Width),
        ),
        (
            RIGHT_SIDE_ID,
            "Right side",
            WHITE_ID,
            [560, 720, 18],
            [782., 0., 0.],
            side,
            Some(BoardGrain::Width),
        ),
        (
            BOTTOM_ID,
            "Bottom",
            WHITE_ID,
            [764, 560, 18],
            [18., 0., 0.],
            Quaternion::IDENTITY,
            None,
        ),
        (
            FRONT_RAIL_ID,
            "Top rail front",
            WHITE_ID,
            [764, 100, 18],
            [18., 0., 702.],
            Quaternion::IDENTITY,
            None,
        ),
        (
            BACK_RAIL_ID,
            "Top rail back",
            WHITE_ID,
            [764, 100, 18],
            [18., 460., 702.],
            Quaternion::IDENTITY,
            None,
        ),
        (
            SHELF_ID,
            "Shelf",
            WHITE_ID,
            [764, 537, 18],
            [18., 20., 350.],
            Quaternion::IDENTITY,
            None,
        ),
        (
            BACK_ID,
            "Back panel",
            HDF_ID,
            [764, 684, 3],
            [18., 560., 18.],
            vertical,
            None,
        ),
        (
            LEFT_DOOR_ID,
            "Door left",
            OAK_ID,
            [397, 716, 18],
            [0., 0., 2.],
            vertical,
            Some(BoardGrain::Width),
        ),
        (
            RIGHT_DOOR_ID,
            "Door right",
            OAK_ID,
            [397, 716, 18],
            [403., 0., 2.],
            vertical,
            Some(BoardGrain::Width),
        ),
    ] {
        p.boards.push(Board {
            coated_face: Default::default(),
            banding: Default::default(),
            id,
            name: name.into(),
            material_id,
            length: mm(size[0]),
            width: mm(size[1]),
            thickness: mm(size[2]),
            grain_override,
            parent_id: Some(if material_id == OAK_ID {
                DOORS_ID
            } else {
                CARCASS_ID
            }),
            pose: pose(at, rotation),
        });
    }
    for (priority, (id, name, material_id, size, grain, source)) in [
        (
            WHITE_STOCK_ID,
            "MDF White",
            WHITE_ID,
            [2750000, 1830000, 18000],
            StockGrain::AlongX,
            StockSource::ToPurchase,
        ),
        (
            OWNED_STOCK_ID,
            "Owned offcut",
            WHITE_ID,
            [900000, 600000, 18200],
            StockGrain::Unknown,
            StockSource::Owned,
        ),
        (
            SPARE_STOCK_ID,
            "Spare MDF White",
            WHITE_ID,
            [2750000, 1830000, 18000],
            StockGrain::AlongX,
            StockSource::ToPurchase,
        ),
        (
            OAK_STOCK_ID,
            "MDF Oak veneer",
            OAK_ID,
            [2750000, 1830000, 18000],
            StockGrain::AlongX,
            StockSource::ToPurchase,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        p.stock.push(Stock {
            id,
            name: name.into(),
            material_id,
            length: Length::from_micrometres(size[0]),
            width: Length::from_micrometres(size[1]),
            thickness: Length::from_micrometres(size[2]),
            grain,
            source,
            price: (source == StockSource::ToPurchase)
                .then(|| Money::new(Currency::Brl, 28990).expect("constant price")),
            priority: priority as u32,
            trim: [Length::ZERO; 4],
        });
    }
    // The fixture is a new schema-2 document, so pin the shop labels that the
    // handoff actually depicts rather than embedding label text in stock names.
    // Priority remains independently ordered for first-fit and is not an alias.
    p.stock_aliases.insert(WHITE_STOCK_ID, "S1".into());
    p.stock_aliases.insert(OWNED_STOCK_ID, "O1".into());
    p.stock_aliases.insert(SPARE_STOCK_ID, "S3".into());
    p.stock_aliases.insert(OAK_STOCK_ID, "S2".into());
    p.next_stock_s_alias = 4;
    p.next_stock_o_alias = 2;
    for (allocation_id, (board_id, stock_id, origin, quarter_turn)) in
        ALLOCATION_IDS.into_iter().zip([
            (LEFT_SIDE_ID, WHITE_STOCK_ID, [0, 0], true),
            (RIGHT_SIDE_ID, WHITE_STOCK_ID, [725, 0], true),
            (BOTTOM_ID, WHITE_STOCK_ID, [1450, 0], false),
            (FRONT_RAIL_ID, WHITE_STOCK_ID, [769, 565], false),
            (BACK_RAIL_ID, WHITE_STOCK_ID, [769, 670], false),
            (SHELF_ID, WHITE_STOCK_ID, [0, 565], false),
            (LEFT_DOOR_ID, OAK_STOCK_ID, [0, 0], true),
            (RIGHT_DOOR_ID, OAK_STOCK_ID, [721, 0], true),
        ])
    {
        p.allocations.push(Allocation {
            id: allocation_id,
            board_id,
            stock_id,
            origin: origin.map(mm),
            quarter_turn,
            locked: false,
        });
    }
    let mut catalog = hardware_catalog::builtin_hinge();
    catalog.id = CATALOG_ID;
    p.catalog.push(catalog);
    for (index, (door_board_id, mounting_board_id, door_y, mount_y)) in [
        (LEFT_DOOR_ID, LEFT_SIDE_ID, 100, 102),
        (LEFT_DOOR_ID, LEFT_SIDE_ID, 616, 618),
        (RIGHT_DOOR_ID, RIGHT_SIDE_ID, 100, 102),
        // The handoff's warned cup is 12 mm from the door's upper edge.
        // The plate uses an independent valid Y reference.
        (RIGHT_DOOR_ID, RIGHT_SIDE_ID, 704, 700),
    ]
    .into_iter()
    .enumerate()
    {
        p.hinge_installations.push(HingeInstallation {
            id: HINGE_IDS[index],
            door_board_id,
            mounting_board_id,
            catalog_id: CATALOG_ID,
            side: HingeMountingSide {
                door_edge: if index < 2 {
                    BoardEdge::MinX
                } else {
                    BoardEdge::MaxX
                },
                door_face: BoardFace::MinZ,
                mount_front_edge: BoardEdge::MinX,
                mount_face: if index < 2 {
                    BoardFace::MaxZ
                } else {
                    BoardFace::MinZ
                },
            },
            door_y: mm(door_y),
            mount_y: mm(mount_y),
            cup_edge_setback: mm(4),
            overlay: mm(16),
            inset_depth: Default::default(),
        });
    }
    for (id, root, mount, hinges) in [
        (
            LEFT_JOINT_ID,
            LEFT_DOOR_ID,
            LEFT_SIDE_ID,
            HINGE_IDS[..2].to_vec(),
        ),
        (
            RIGHT_JOINT_ID,
            RIGHT_DOOR_ID,
            RIGHT_SIDE_ID,
            HINGE_IDS[2..].to_vec(),
        ),
    ] {
        p.door_joints.push(
            door_joint::preview(&p, id, root, mount, hinges)
                .expect("fixed reference relationship")
                .joint,
        );
    }
    p.validate().expect("valid reference project");
    p
}
