//! One-time, project-independent cabinet recipes. All coordinates are exact micrometres:
//! world X = width, Y = depth (front at 0), Z = height. Board X/Y/Z remain
//! length/width/thickness. No stock, allocation, or project is created here.
use uuid::Uuid;

use crate::domain::{Assembly, Board};
use crate::units::{Length, Pose, Quaternion, WORLD_LIMIT_MM};

const LIMIT: i128 = (WORLD_LIMIT_MM as i128) * 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecipeMaterial {
    pub id: Uuid,
    pub thickness: Length,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CabinetSize {
    pub width: Length,
    /// Includes overlay back and (for Drawers) the external front.
    pub depth: Length,
    pub height: Length,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BaseRecipe {
    pub size: CabinetSize,
    pub carcass: RecipeMaterial,
    pub back: RecipeMaterial,
    pub rail_width: Length,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WallRecipe {
    pub size: CabinetSize,
    pub carcass: RecipeMaterial,
    pub back: RecipeMaterial,
    /// Height of the shelf's underside from the cabinet bottom.
    pub shelf_height: Length,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawersRecipe {
    pub size: CabinetSize,
    pub carcass: RecipeMaterial,
    pub back: RecipeMaterial,
    pub box_material: RecipeMaterial,
    pub box_bottom: RecipeMaterial,
    pub external_front: RecipeMaterial,
    pub count: usize,
    pub box_depth: Length,
    /// Clearance on each side of each box, inside the carcass opening.
    pub side_clearance: Length,
    /// Minimum free space behind a box, before the overlay back.
    pub rear_clearance: Length,
    /// Space above and below a box within its equal-height opening.
    pub vertical_clearance: Length,
    /// Reveal at both X edges and both Z edges of the full facade.
    pub front_reveal: Length,
    pub front_gap: Length,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecipeErrorKind {
    NonPositive,
    Negative,
    Geometry,
    OutOfBounds,
    TooMany,
    MaterialConflict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecipeError {
    /// Stable input/derived field key for a caller's form error.
    pub field: &'static str,
    pub kind: RecipeErrorKind,
}

#[derive(Clone, Debug)]
pub struct RecipeCandidate {
    pub assemblies: Vec<Assembly>,
    /// Ordered independent physical parts; also the ungrouped BOM / allocation order.
    pub boards: Vec<Board>,
    pub size: CabinetSize,
}

fn um(value: Length) -> i128 {
    i128::from(value.micrometres())
}

fn error(field: &'static str, kind: RecipeErrorKind) -> RecipeError {
    RecipeError { field, kind }
}

fn derived(value: i128, field: &'static str) -> Result<Length, RecipeError> {
    if value <= 0 {
        Err(error(field, RecipeErrorKind::Geometry))
    } else if value > LIMIT || value > i64::MAX as i128 {
        Err(error(field, RecipeErrorKind::OutOfBounds))
    } else {
        Ok(Length::from_micrometres(value as i64))
    }
}

fn validate(
    size: CabinetSize,
    materials: &[(&'static str, RecipeMaterial)],
    positive: &[(&'static str, Length)],
    nonnegative: &[(&'static str, Length)],
) -> Result<(), Vec<RecipeError>> {
    let mut errors = Vec::new();
    for (field, value) in [
        ("width", size.width),
        ("depth", size.depth),
        ("height", size.height),
    ]
    .into_iter()
    .chain(materials.iter().map(|(field, m)| (*field, m.thickness)))
    .chain(positive.iter().copied())
    {
        if um(value) <= 0 {
            errors.push(error(field, RecipeErrorKind::NonPositive));
        } else if um(value) > LIMIT {
            errors.push(error(field, RecipeErrorKind::OutOfBounds));
        }
    }
    for &(field, value) in nonnegative {
        if um(value) < 0 {
            errors.push(error(field, RecipeErrorKind::Negative));
        } else if um(value) > LIMIT {
            errors.push(error(field, RecipeErrorKind::OutOfBounds));
        }
    }
    for (index, (field, material)) in materials.iter().enumerate() {
        if materials[..index].iter().any(|(_, previous)| {
            previous.id == material.id && previous.thickness != material.thickness
        }) {
            errors.push(error(field, RecipeErrorKind::MaterialConflict));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn pose(x: i128, y: i128, z: i128, rotation: Quaternion) -> Pose {
    // Called only after checked positive input, derived dimensions and bounds.
    Pose::from_explicit(
        [x, y, z].map(|n| Length::from_micrometres(n as i64)),
        rotation,
    )
    .expect("validated recipe pose")
}

fn identity() -> Quaternion {
    Quaternion::IDENTITY
}
fn upright_side() -> Quaternion {
    Quaternion::normalized(1.0, 0.0, -1.0, 0.0).expect("constant quaternion")
}
fn upright_face() -> Quaternion {
    Quaternion::normalized(1.0, 1.0, 0.0, 0.0).expect("constant quaternion")
}

impl RecipeCandidate {
    fn new(name: &str, size: CabinetSize) -> Self {
        Self {
            assemblies: vec![Assembly {
                id: Uuid::new_v4(),
                name: name.into(),
                parent_id: None,
                pose: pose(0, 0, 0, identity()),
            }],
            boards: Vec::new(),
            size,
        }
    }

    fn board(
        &mut self,
        name: impl Into<String>,
        material: RecipeMaterial,
        dimensions: [i128; 2],
        origin: [i128; 3],
        rotation: Quaternion,
        parent_id: Uuid,
    ) {
        self.boards.push(Board {
            id: Uuid::new_v4(),
            name: name.into(),
            material_id: material.id,
            length: Length::from_micrometres(dimensions[0] as i64),
            width: Length::from_micrometres(dimensions[1] as i64),
            thickness: material.thickness,
            grain_override: None,
            parent_id: Some(parent_id),
            pose: pose(origin[0], origin[1], origin[2], rotation),
        });
    }
}

/// Sides span the full height; the bottom and rails sit between them. The back
/// overlays the carcass at the rear, within the declared overall depth.
pub fn base(input: BaseRecipe) -> Result<RecipeCandidate, Vec<RecipeError>> {
    validate(
        input.size,
        &[
            ("carcass_thickness", input.carcass),
            ("back_thickness", input.back),
        ],
        &[("rail_width", input.rail_width)],
        &[],
    )?;
    let w = um(input.size.width);
    let h = um(input.size.height);
    let t = um(input.carcass.thickness);
    let d = um(input.size.depth) - um(input.back.thickness);
    let inner = derived(w - 2 * t, "carcass_opening_width").map_err(|e| vec![e])?;
    derived(d, "carcass_depth").map_err(|e| vec![e])?;
    derived(h - 2 * t, "carcass_opening_height").map_err(|e| vec![e])?;
    if um(input.rail_width) * 2 > d {
        return Err(vec![error("rail_width", RecipeErrorKind::Geometry)]);
    }
    let mut c = RecipeCandidate::new("Base", input.size);
    let root = c.assemblies[0].id;
    c.board(
        "Left side",
        input.carcass,
        [h, d],
        [t, 0, 0],
        upright_side(),
        root,
    );
    c.board(
        "Right side",
        input.carcass,
        [h, d],
        [w, 0, 0],
        upright_side(),
        root,
    );
    c.board(
        "Bottom",
        input.carcass,
        [um(inner), d],
        [t, 0, 0],
        identity(),
        root,
    );
    c.board(
        "Front top rail",
        input.carcass,
        [um(inner), um(input.rail_width)],
        [t, 0, h - t],
        identity(),
        root,
    );
    c.board(
        "Rear top rail",
        input.carcass,
        [um(inner), um(input.rail_width)],
        [t, d - um(input.rail_width), h - t],
        identity(),
        root,
    );
    c.board(
        "Overlay back",
        input.back,
        [w, h],
        [0, um(input.size.depth), 0],
        upright_face(),
        root,
    );
    // Upright face's thickness points toward the front (negative Y).
    Ok(c)
}

/// The shelf height is measured to its underside and must clear both fixed panels.
pub fn wall(input: WallRecipe) -> Result<RecipeCandidate, Vec<RecipeError>> {
    validate(
        input.size,
        &[
            ("carcass_thickness", input.carcass),
            ("back_thickness", input.back),
        ],
        &[("shelf_height", input.shelf_height)],
        &[],
    )?;
    let w = um(input.size.width);
    let h = um(input.size.height);
    let t = um(input.carcass.thickness);
    let d = um(input.size.depth) - um(input.back.thickness);
    let inner = derived(w - 2 * t, "carcass_opening_width").map_err(|e| vec![e])?;
    derived(d, "carcass_depth").map_err(|e| vec![e])?;
    derived(h - 2 * t, "carcass_opening_height").map_err(|e| vec![e])?;
    if um(input.shelf_height) <= t || um(input.shelf_height) + t >= h - t {
        return Err(vec![error("shelf_height", RecipeErrorKind::Geometry)]);
    }
    let mut c = RecipeCandidate::new("Wall", input.size);
    let root = c.assemblies[0].id;
    c.board(
        "Left side",
        input.carcass,
        [h, d],
        [t, 0, 0],
        upright_side(),
        root,
    );
    c.board(
        "Right side",
        input.carcass,
        [h, d],
        [w, 0, 0],
        upright_side(),
        root,
    );
    c.board(
        "Bottom",
        input.carcass,
        [um(inner), d],
        [t, 0, 0],
        identity(),
        root,
    );
    c.board(
        "Top",
        input.carcass,
        [um(inner), d],
        [t, 0, h - t],
        identity(),
        root,
    );
    c.board(
        "Shelf",
        input.carcass,
        [um(inner), d],
        [t, 0, um(input.shelf_height)],
        identity(),
        root,
    );
    c.board(
        "Overlay back",
        input.back,
        [w, h],
        [0, um(input.size.depth), 0],
        upright_face(),
        root,
    );
    Ok(c)
}

/// Complete rectangular drawer boxes. Equal-height openings and facades are
/// divided exactly on the micrometre grid: first drawers receive the remainder
/// (at most one micrometre extra each); no manufacturing rounding is hidden.
pub fn drawers(input: DrawersRecipe) -> Result<RecipeCandidate, Vec<RecipeError>> {
    validate(
        input.size,
        &[
            ("carcass_thickness", input.carcass),
            ("back_thickness", input.back),
            ("box_thickness", input.box_material),
            ("box_bottom_thickness", input.box_bottom),
            ("front_thickness", input.external_front),
        ],
        &[("box_depth", input.box_depth)],
        &[
            ("side_clearance", input.side_clearance),
            ("rear_clearance", input.rear_clearance),
            ("vertical_clearance", input.vertical_clearance),
            ("front_reveal", input.front_reveal),
            ("front_gap", input.front_gap),
        ],
    )?;
    if input.count == 0 {
        return Err(vec![error("count", RecipeErrorKind::NonPositive)]);
    }
    if input.count > 1000 {
        return Err(vec![error("count", RecipeErrorKind::TooMany)]);
    }
    let n = input.count as i128;
    let w = um(input.size.width);
    let h = um(input.size.height);
    let t = um(input.carcass.thickness);
    let ft = um(input.external_front.thickness);
    let bt = um(input.box_material.thickness);
    let bottom = um(input.box_bottom.thickness);
    let bd = um(input.box_depth);
    let d = um(input.size.depth) - um(input.back.thickness) - ft;
    let inner = derived(w - 2 * t, "carcass_opening_width").map_err(|e| vec![e])?;
    let opening = derived(h - 2 * t, "carcass_opening_height").map_err(|e| vec![e])?;
    derived(d, "carcass_depth").map_err(|e| vec![e])?;
    let box_width =
        derived(um(inner) - 2 * um(input.side_clearance), "box_width").map_err(|e| vec![e])?;
    let cross = derived(um(box_width) - 2 * bt, "box_front_back_length").map_err(|e| vec![e])?;
    if bd + um(input.rear_clearance) > d {
        return Err(vec![error("rear_clearance", RecipeErrorKind::Geometry)]);
    }
    derived(bd - 2 * bt, "box_inner_depth").map_err(|e| vec![e])?;
    let bay = um(opening) / n;
    let box_height =
        derived(bay - 2 * um(input.vertical_clearance), "box_height").map_err(|e| vec![e])?;
    derived(um(box_height) - bottom, "box_side_height").map_err(|e| vec![e])?;
    let available = derived(
        h - 2 * um(input.front_reveal) - (n - 1) * um(input.front_gap),
        "front_height",
    )
    .map_err(|e| vec![e])?;
    derived(um(available) / n, "front_height").map_err(|e| vec![e])?;
    let front_width =
        derived(w - 2 * um(input.front_reveal), "front_width").map_err(|e| vec![e])?;

    let mut c = RecipeCandidate::new("Drawers", input.size);
    let root = c.assemblies[0].id;
    c.board(
        "Left side",
        input.carcass,
        [h, d],
        [t, 0, 0],
        upright_side(),
        root,
    );
    c.board(
        "Right side",
        input.carcass,
        [h, d],
        [w, 0, 0],
        upright_side(),
        root,
    );
    c.board(
        "Bottom",
        input.carcass,
        [um(inner), d],
        [t, 0, 0],
        identity(),
        root,
    );
    c.board(
        "Top",
        input.carcass,
        [um(inner), d],
        [t, 0, h - t],
        identity(),
        root,
    );
    c.board(
        "Overlay back",
        input.back,
        [w, h],
        [0, um(input.size.depth) - ft, 0],
        upright_face(),
        root,
    );
    let left = t + um(input.side_clearance);
    let mut bay_start = t;
    let mut front_start = um(input.front_reveal);
    for i in 0..input.count {
        let bay_h = bay + if (i as i128) < um(opening) % n { 1 } else { 0 };
        let box_z = bay_start + um(input.vertical_clearance);
        let side_h = bay_h - 2 * um(input.vertical_clearance) - bottom;
        let drawer_id = Uuid::new_v4();
        c.assemblies.push(Assembly {
            id: drawer_id,
            name: format!("Drawer {}", i + 1),
            parent_id: Some(root),
            pose: pose(0, 0, 0, identity()),
        });
        c.board(
            format!("Drawer {} left box side", i + 1),
            input.box_material,
            [side_h, bd],
            [left + bt, 0, box_z + bottom],
            upright_side(),
            drawer_id,
        );
        c.board(
            format!("Drawer {} right box side", i + 1),
            input.box_material,
            [side_h, bd],
            [left + um(box_width), 0, box_z + bottom],
            upright_side(),
            drawer_id,
        );
        // Front/back members span between sides. Upright face thickness runs toward -Y.
        c.board(
            format!("Drawer {} box front", i + 1),
            input.box_material,
            [um(cross), side_h],
            [left + bt, bt, box_z + bottom],
            upright_face(),
            drawer_id,
        );
        c.board(
            format!("Drawer {} box back", i + 1),
            input.box_material,
            [um(cross), side_h],
            [left + bt, bd, box_z + bottom],
            upright_face(),
            drawer_id,
        );
        c.board(
            format!("Drawer {} applied bottom", i + 1),
            input.box_bottom,
            [um(box_width), bd],
            [left, 0, box_z],
            identity(),
            drawer_id,
        );
        let front_h = um(available) / n
            + if (i as i128) < um(available) % n {
                1
            } else {
                0
            };
        c.board(
            format!("Drawer {} external front", i + 1),
            input.external_front,
            [um(front_width), front_h],
            [um(input.front_reveal), 0, front_start],
            upright_face(),
            drawer_id,
        );
        bay_start += bay_h;
        front_start += front_h + um(input.front_gap);
    }
    Ok(c)
}
