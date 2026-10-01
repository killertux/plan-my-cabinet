use std::collections::HashSet;

use plan_my_cabinet::domain::{Board, BoardGrain, Material, Project};
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::template_recipes::{
    BaseRecipe, CabinetSize, DrawersRecipe, RecipeCandidate, RecipeErrorKind, RecipeMaterial,
    WallRecipe, base, drawers, wall,
};
use plan_my_cabinet::units::Length;
use uuid::Uuid;

fn mm(n: i64) -> Length {
    Length::from_micrometres(n * 1000)
}
fn material(t: i64) -> RecipeMaterial {
    RecipeMaterial {
        id: Uuid::new_v4(),
        thickness: mm(t),
    }
}
fn size() -> CabinetSize {
    CabinetSize {
        width: mm(600),
        depth: mm(560),
        height: mm(720),
    }
}
fn base_input() -> BaseRecipe {
    BaseRecipe {
        size: size(),
        carcass: material(18),
        back: material(6),
        rail_width: mm(80),
    }
}
fn wall_input() -> WallRecipe {
    WallRecipe {
        size: size(),
        carcass: material(18),
        back: material(6),
        shelf_height: mm(320),
    }
}
fn drawers_input() -> DrawersRecipe {
    DrawersRecipe {
        size: size(),
        carcass: material(18),
        back: material(6),
        box_material: material(12),
        box_bottom: material(6),
        external_front: material(19),
        count: 3,
        box_depth: mm(500),
        side_clearance: mm(13),
        rear_clearance: mm(20),
        vertical_clearance: mm(8),
        front_reveal: mm(3),
        front_gap: mm(3),
    }
}
fn board<'a>(c: &'a RecipeCandidate, name: &str) -> &'a Board {
    c.boards.iter().find(|b| b.name == name).unwrap()
}
fn dimensions(b: &Board) -> [Length; 3] {
    b.blank_dimensions()
}

fn assert_bounds_and_identity(c: &RecipeCandidate, front: Length) {
    let mut ids = HashSet::new();
    for a in &c.assemblies {
        assert!(ids.insert(a.id));
    }
    let parents: HashSet<_> = c.assemblies.iter().map(|a| a.id).collect();
    let min_y = -(front.micrometres() as f64) / 1000.0;
    let max = [c.size.width, c.size.depth, c.size.height].map(|v| v.micrometres() as f64 / 1000.0);
    let max = [max[0], max[1] - front.micrometres() as f64 / 1000.0, max[2]];
    let mut observed_min = [f64::INFINITY; 3];
    let mut observed_max = [f64::NEG_INFINITY; 3];
    for b in &c.boards {
        assert!(ids.insert(b.id));
        assert!(parents.contains(&b.parent_id.unwrap()));
        for v in b.blank_dimensions() {
            assert!(v.micrometres() > 0);
        }
        let parent = c
            .assemblies
            .iter()
            .find(|a| Some(a.id) == b.parent_id)
            .unwrap();
        let world = c.assemblies[0]
            .pose
            .compose(parent.pose)
            .unwrap()
            .compose(b.pose)
            .unwrap();
        for x in [0.0, b.length.micrometres() as f64 / 1000.0] {
            for y in [0.0, b.width.micrometres() as f64 / 1000.0] {
                for z in [0.0, b.thickness.micrometres() as f64 / 1000.0] {
                    let p = world.transform_point([x, y, z]).unwrap();
                    for axis in 0..3 {
                        observed_min[axis] = observed_min[axis].min(p[axis]);
                        observed_max[axis] = observed_max[axis].max(p[axis]);
                        let lo = if axis == 1 { min_y } else { 0.0 };
                        assert!(
                            p[axis] >= lo - 1e-7 && p[axis] <= max[axis] + 1e-7,
                            "{} at {p:?}",
                            b.name
                        );
                    }
                }
            }
        }
    }
    for axis in 0..3 {
        let lo = if axis == 1 { min_y } else { 0.0 };
        assert!((observed_min[axis] - lo).abs() < 1e-7);
        assert!((observed_max[axis] - max[axis]).abs() < 1e-7);
    }
    let mut project = Project::new("candidate", Currency::Usd);
    for (id, thickness) in c
        .boards
        .iter()
        .map(|b| (b.material_id, b.thickness))
        .collect::<HashSet<_>>()
    {
        project.materials.push(Material {
            coating: Default::default(),
            default_band: None,
            kind: Default::default(),
            id,
            name: "Chosen".into(),
            default_thickness: thickness,
            default_grain: BoardGrain::Unrestricted,
        });
    }
    project.assemblies = c.assemblies.clone();
    project.boards = c.boards.clone();
    assert!(project.stock.is_empty() && project.allocations.is_empty());
    assert_eq!(project.validate(), Ok(()));
}

#[test]
fn base_and_wall_boms_and_bounds_across_thicknesses() {
    for (t, back_t) in [(12, 3), (18, 6), (25, 12)] {
        let mut b = base_input();
        b.carcass = material(t);
        b.back = material(back_t);
        let c = base(b).unwrap();
        assert_eq!(c.boards.len(), 6);
        assert_eq!(
            c.boards.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(),
            [
                "Left side",
                "Right side",
                "Bottom",
                "Front top rail",
                "Rear top rail",
                "Overlay back"
            ]
        );
        assert_eq!(
            dimensions(board(&c, "Left side")),
            [mm(720), mm(560 - back_t), mm(t)]
        );
        assert_eq!(
            dimensions(board(&c, "Bottom")),
            [mm(600 - 2 * t), mm(560 - back_t), mm(t)]
        );
        assert_eq!(
            dimensions(board(&c, "Front top rail")),
            [mm(600 - 2 * t), mm(80), mm(t)]
        );
        assert_eq!(
            dimensions(board(&c, "Overlay back")),
            [mm(600), mm(720), mm(back_t)]
        );
        assert_bounds_and_identity(&c, Length::ZERO);

        let mut w = wall_input();
        w.carcass = b.carcass;
        w.back = b.back;
        let c = wall(w).unwrap();
        assert_eq!(c.boards.len(), 6);
        assert_eq!(
            dimensions(board(&c, "Shelf")),
            [mm(600 - 2 * t), mm(560 - back_t), mm(t)]
        );
        assert_eq!(board(&c, "Shelf").pose.translation_mm[2], 320.0);
        assert_bounds_and_identity(&c, Length::ZERO);
    }
    let a = base(base_input()).unwrap();
    let b = base(base_input()).unwrap();
    assert!(a.assemblies[0].id != b.assemblies[0].id && a.boards[0].id != b.boards[0].id);
}

#[test]
fn three_drawer_bom_and_formulas() {
    for (carcass, box_t, bottom_t, front_t, side, rear, vertical) in
        [(18, 12, 6, 19, 13, 20, 8), (16, 15, 9, 22, 10, 10, 5)]
    {
        let mut input = drawers_input();
        input.carcass = material(carcass);
        input.box_material = material(box_t);
        input.box_bottom = material(bottom_t);
        input.external_front = material(front_t);
        input.side_clearance = mm(side);
        input.rear_clearance = mm(rear);
        input.vertical_clearance = mm(vertical);
        let c = drawers(input).unwrap();
        assert_eq!(c.assemblies.len(), 4);
        assert_eq!(c.boards.len(), 23); // five carcass + three sets of six
        assert_eq!(
            dimensions(board(&c, "Overlay back")),
            [mm(600), mm(720), mm(6)]
        );
        let width = 600 - 2 * carcass - 2 * side;
        assert_eq!(
            dimensions(board(&c, "Drawer 1 applied bottom")),
            [mm(width), mm(500), mm(bottom_t)]
        );
        let first_bay = ((720 - 2 * carcass) * 1000 + 2) / 3;
        let side_height = Length::from_micrometres(first_bay - (2 * vertical + bottom_t) * 1000);
        assert_eq!(
            dimensions(board(&c, "Drawer 1 box front")),
            [mm(width - 2 * box_t), side_height, mm(box_t)]
        );
        assert_eq!(
            dimensions(board(&c, "Drawer 1 left box side")),
            [side_height, mm(500), mm(box_t)]
        );
        assert_eq!(
            dimensions(board(&c, "Drawer 1 external front")),
            [mm(594), mm(236), mm(front_t)]
        );
        assert_eq!(
            board(&c, "Drawer 1 applied bottom").pose.translation_mm[0],
            (carcass + side) as f64
        );
        assert_eq!(
            board(&c, "Drawer 2 external front").pose.translation_mm[2],
            242.0
        );
        assert_eq!(board(&c, "Left side").width, mm(560 - 6 - front_t));
        assert_bounds_and_identity(&c, mm(front_t));
    }
}

#[test]
fn division_remainder_is_exact_and_stays_within_facade() {
    let mut input = drawers_input();
    input.size.height = Length::from_micrometres(720_001);
    let c = drawers(input).unwrap();
    // Front board length is horizontal width; its width is vertical height.
    let heights: Vec<_> = (1..=3)
        .map(|i| {
            board(&c, &format!("Drawer {i} external front"))
                .width
                .micrometres()
        })
        .collect();
    assert_eq!(heights.iter().sum::<i64>(), 708_001);
    assert!(heights[0] - heights[2] <= 1);
    assert_bounds_and_identity(&c, input.external_front.thickness);
}

#[test]
fn invalid_inputs_report_specific_fields_and_never_build_partial_geometry() {
    let mut b = base_input();
    b.size.width = mm(30);
    assert_eq!(base(b).unwrap_err()[0].field, "carcass_opening_width");
    b = base_input();
    b.rail_width = mm(300);
    assert_eq!(base(b).unwrap_err()[0].field, "rail_width");
    b = base_input();
    b.size.height = mm(20);
    assert_eq!(base(b).unwrap_err()[0].field, "carcass_opening_height");
    let mut w = wall_input();
    w.shelf_height = mm(700);
    assert_eq!(wall(w).unwrap_err()[0].field, "shelf_height");
    let mut d = drawers_input();
    d.side_clearance = mm(300);
    assert_eq!(drawers(d).unwrap_err()[0].field, "box_width");
    d = drawers_input();
    d.box_material.thickness = mm(300);
    assert_eq!(drawers(d).unwrap_err()[0].field, "box_front_back_length");
    d = drawers_input();
    d.rear_clearance = mm(100);
    assert_eq!(drawers(d).unwrap_err()[0].field, "rear_clearance");
    d = drawers_input();
    d.vertical_clearance = mm(120);
    assert_eq!(drawers(d).unwrap_err()[0].field, "box_height");
    d = drawers_input();
    d.front_reveal = mm(400);
    assert_eq!(drawers(d).unwrap_err()[0].field, "front_height");
    d = drawers_input();
    d.count = 0;
    assert_eq!(drawers(d).unwrap_err()[0].field, "count");
    d = drawers_input();
    d.size.depth = mm(0);
    d.side_clearance = mm(-1);
    let errors = drawers(d).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.field == "depth" && e.kind == RecipeErrorKind::NonPositive)
    );
    assert!(
        errors
            .iter()
            .any(|e| e.field == "side_clearance" && e.kind == RecipeErrorKind::Negative)
    );
    d = drawers_input();
    d.size.width = Length::from_micrometres(1_000_000_001);
    assert_eq!(
        drawers(d).unwrap_err()[0].kind,
        RecipeErrorKind::OutOfBounds
    );
    d = drawers_input();
    d.box_material.id = d.carcass.id;
    assert_eq!(drawers(d).unwrap_err()[0].field, "box_thickness");
    d = drawers_input();
    d.box_material = d.carcass;
    assert!(drawers(d).is_ok());
}
