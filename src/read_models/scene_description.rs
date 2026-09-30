//! A textual account of the built geometry for readers who cannot see the 3D
//! view: where every board is, how it is oriented, which boards touch, and —
//! most importantly — which ones intersect or leave small gaps.
use std::collections::HashSet;

use serde::Serialize;
use uuid::Uuid;

use crate::domain::Project;
use crate::render::camera::{Selection, board_corners, box_corners};
use crate::units::Pose;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DescribeOptions {
    /// Faces closer than this count as touching; boxes must interpenetrate by
    /// more than this to count as overlapping.
    pub tolerance_mm: f64,
    /// Report facing faces that are apart by at most this much.
    pub gap_report_mm: f64,
}

impl Default for DescribeOptions {
    fn default() -> Self {
        Self {
            tolerance_mm: 0.5,
            gap_report_mm: 20.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Aabb {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ObjectGeometry {
    pub id: Uuid,
    pub name: String,
    /// "board" or "hardware".
    pub kind: &'static str,
    pub parent: Option<String>,
    pub material: Option<String>,
    /// Board length, width, thickness (hardware: its three dimensions).
    pub size_mm: [f64; 3],
    pub aabb: Aabb,
    /// World direction of each local axis (x = length, y = width, z = thickness)
    /// such as "+X", "-Z", or "rotated".
    pub local_axes: [String; 3],
    pub orientation: String,
    /// Relative placement inside the overall bounds, e.g. "left, front, bottom".
    pub position: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Contact {
    pub a: Uuid,
    pub a_name: String,
    /// Board-local face of `a` (e.g. "+x"), when `a` is axis aligned.
    pub a_face: String,
    pub b: Uuid,
    pub b_name: String,
    pub b_face: String,
    /// World axis of the shared face normal.
    pub axis: &'static str,
    pub area_mm2: f64,
    /// Size of the shared patch along its two in-plane world axes.
    pub patch_mm: [f64; 2],
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Overlap {
    pub a: Uuid,
    pub a_name: String,
    pub b: Uuid,
    pub b_name: String,
    /// Smallest translation that separates the two, in millimetres.
    pub penetration_mm: f64,
    /// Intersection box when both are axis aligned.
    pub region: Option<Aabb>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Gap {
    pub a: Uuid,
    pub a_name: String,
    pub b: Uuid,
    pub b_name: String,
    pub axis: &'static str,
    pub distance_mm: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SceneDescription {
    pub object_count: usize,
    pub overall: Option<Aabb>,
    pub objects: Vec<ObjectGeometry>,
    pub contacts: Vec<Contact>,
    pub overlaps: Vec<Overlap>,
    pub gaps: Vec<Gap>,
    /// Objects that touch nothing (only when there are several objects).
    pub floating: Vec<String>,
    /// Pairs involving a rotated (not axis-aligned) object: contacts and gaps
    /// are not computed for them; overlaps are.
    pub rotated: Vec<String>,
    /// A compact English account of all of the above.
    pub text: String,
}

struct Solid {
    id: Uuid,
    name: String,
    corners: [[f64; 3]; 8],
    /// World direction of local x, y, z (unit vectors).
    axes: [[f64; 3]; 3],
    size: [f64; 3],
    aabb: Aabb,
    /// For each local axis, the aligned world axis index and sign.
    aligned: Option<[(usize, f64); 3]>,
}

const AXIS: [&str; 3] = ["X", "Y", "Z"];

fn solid(id: Uuid, name: String, pose: Pose, size: [f64; 3]) -> Option<Solid> {
    let corners = box_corners(pose, size)?;
    solid_from(id, name, corners, pose, size)
}

fn solid_from(
    id: Uuid,
    name: String,
    corners: [[f64; 3]; 8],
    pose: Pose,
    size: [f64; 3],
) -> Option<Solid> {
    let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]].map(|v| pose.rotation.rotate(v));
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for c in corners {
        for i in 0..3 {
            min[i] = min[i].min(c[i]);
            max[i] = max[i].max(c[i]);
        }
    }
    let aligned = axes
        .iter()
        .map(|a| {
            (0..3)
                .find(|&k| (a[k].abs() - 1.0).abs() < 1e-6)
                .map(|k| (k, a[k].signum()))
        })
        .collect::<Option<Vec<_>>>()
        .map(|v| [v[0], v[1], v[2]]);
    Some(Solid {
        id,
        name,
        corners,
        axes,
        size,
        aabb: Aabb { min, max },
        aligned,
    })
}

fn round1(v: f64) -> f64 {
    let r = (v * 10.0).round() / 10.0;
    if r == 0.0 { 0.0 } else { r }
}

fn fmt(v: f64) -> String {
    let r = round1(v);
    if r.fract() == 0.0 {
        format!("{r:.0}")
    } else {
        format!("{r:.1}")
    }
}

/// The board-local face of `s` whose outward normal is world `axis` * `sign`.
fn local_face(s: &Solid, axis: usize, sign: f64) -> String {
    let Some(aligned) = s.aligned else {
        return "?".into();
    };
    for (local, (world, dir)) in aligned.iter().enumerate() {
        if *world == axis {
            let outward = if dir * sign > 0.0 { "+" } else { "-" };
            return format!("{outward}{}", ["x", "y", "z"][local]);
        }
    }
    "?".into()
}

fn orientation(s: &Solid, kind: &str) -> (String, [String; 3]) {
    let names: [String; 3] = match s.aligned {
        Some(a) => a.map(|(k, sign)| format!("{}{}", if sign > 0.0 { "+" } else { "-" }, AXIS[k])),
        None => std::array::from_fn(|i| {
            let v = s.axes[i];
            format!("({:.2}, {:.2}, {:.2})", v[0], v[1], v[2])
        }),
    };
    if kind != "board" {
        return (kind.into(), names);
    }
    let Some(a) = s.aligned else {
        return ("rotated (not aligned with the world axes)".into(), names);
    };
    let thickness_axis = a[2].0;
    let length_axis = a[0].0;
    let text = match thickness_axis {
        2 => format!(
            "horizontal panel (lying flat), length along {}",
            AXIS[length_axis]
        ),
        0 => format!(
            "vertical side panel (thickness along X), length along {}",
            AXIS[length_axis]
        ),
        _ => format!(
            "vertical front/back panel (thickness along Y), length along {}",
            AXIS[length_axis]
        ),
    };
    (text, names)
}

fn position_words(b: &Aabb, overall: &Aabb) -> String {
    let word = |i: usize, low: &str, high: &str| {
        let span = overall.max[i] - overall.min[i];
        if span <= 1e-6 {
            return None;
        }
        let centre = ((b.min[i] + b.max[i]) / 2.0 - overall.min[i]) / span;
        if centre < 0.33 {
            Some(low.to_owned())
        } else if centre > 0.67 {
            Some(high.to_owned())
        } else {
            None
        }
    };
    let words: Vec<String> = [
        word(0, "left", "right"),
        word(1, "front", "back"),
        word(2, "bottom", "top"),
    ]
    .into_iter()
    .flatten()
    .collect();
    if words.is_empty() {
        "centre".into()
    } else {
        words.join(", ")
    }
}

/// Separating-axis overlap depth of two boxes, `None` when separated.
fn sat_penetration(a: &Solid, b: &Solid) -> Option<f64> {
    let mut candidates: Vec<[f64; 3]> = Vec::with_capacity(15);
    candidates.extend(a.axes);
    candidates.extend(b.axes);
    for u in a.axes {
        for v in b.axes {
            let c = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let n = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
            if n > 1e-9 {
                candidates.push(c.map(|x| x / n));
            }
        }
    }
    let project = |s: &Solid, axis: [f64; 3]| {
        let values = s
            .corners
            .map(|c| c[0] * axis[0] + c[1] * axis[1] + c[2] * axis[2]);
        (
            values.iter().copied().fold(f64::INFINITY, f64::min),
            values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        )
    };
    let mut depth = f64::INFINITY;
    for axis in candidates {
        let (a0, a1) = project(a, axis);
        let (b0, b1) = project(b, axis);
        let overlap = a1.min(b1) - a0.max(b0);
        if overlap <= 0.0 {
            return None;
        }
        depth = depth.min(overlap);
    }
    Some(depth)
}

pub fn describe(
    project: &Project,
    selection: &Selection,
    options: DescribeOptions,
) -> SceneDescription {
    let tol = options.tolerance_mm.max(0.0);
    let mut solids = Vec::new();
    let mut objects = Vec::new();
    let parent_name = |parent: Option<Uuid>| {
        parent.and_then(|id| {
            project
                .assemblies
                .iter()
                .find(|a| a.id == id)
                .map(|a| a.name.clone())
        })
    };
    let mut kinds = Vec::new();
    for board in &project.boards {
        if !selection.visible(project, board.id) {
            continue;
        }
        let size = board
            .blank_dimensions()
            .map(|v| v.micrometres() as f64 / 1000.0);
        let Some(pose) = crate::render::camera::world_pose(project, board) else {
            continue;
        };
        let Some(corners) = board_corners(project, board) else {
            continue;
        };
        if let Some(s) = solid_from(board.id, board.name.clone(), corners, pose, size) {
            let material = project.material(board.material_id).map(|m| m.name.clone());
            kinds.push(("board", parent_name(board.parent_id), material));
            solids.push(s);
        }
    }
    for hardware in &project.hardware {
        let Some(dimensions) = project.hardware_dimensions(hardware) else {
            continue;
        };
        let kind = if project.foot_spec(hardware).is_some() {
            "foot"
        } else {
            "hardware"
        };
        if !selection.visible(project, hardware.id) {
            continue;
        }
        let Ok(pose) = crate::assembly_edit::world_pose(project, hardware.id) else {
            continue;
        };
        let size = dimensions.map(|v| v.micrometres() as f64 / 1000.0);
        if let Some(s) = solid(hardware.id, hardware.name.clone(), pose, size) {
            kinds.push((kind, parent_name(hardware.parent_id), None));
            solids.push(s);
        }
    }
    for slide in &project.slide_installations {
        if !slide
            .drawer_sides
            .iter()
            .all(|id| selection.visible(project, *id))
        {
            continue;
        }
        let Some(envelopes) = crate::slide_installation::envelopes(project, slide) else {
            continue;
        };
        let code = project
            .catalog
            .iter()
            .find(|c| c.id == slide.catalog_id)
            .map_or("", |c| c.product_id.as_str());
        let drawer = project
            .assemblies
            .iter()
            .find(|a| a.id == slide.drawer_root_id)
            .map(|a| a.name.clone())
            .or_else(|| project.board(slide.drawer_root_id).map(|b| b.name.clone()))
            .unwrap_or_default();
        for (side, (pose, size)) in envelopes.into_iter().enumerate() {
            let name = format!(
                "{drawer} slide {} ({code})",
                if side == 0 { "left" } else { "right" }
            );
            if let Some(s) = solid(slide.id, name, pose, size) {
                kinds.push(("slide", Some(drawer.clone()), None));
                solids.push(s);
            }
        }
    }
    let overall = (!solids.is_empty()).then(|| {
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for s in &solids {
            for i in 0..3 {
                min[i] = min[i].min(s.aabb.min[i]);
                max[i] = max[i].max(s.aabb.max[i]);
            }
        }
        Aabb { min, max }
    });
    for (s, (kind, parent, material)) in solids.iter().zip(&kinds) {
        let (orientation, local_axes) = orientation(s, kind);
        objects.push(ObjectGeometry {
            id: s.id,
            name: s.name.clone(),
            kind,
            parent: parent.clone(),
            material: material.clone(),
            size_mm: s.size.map(round1),
            aabb: Aabb {
                min: s.aabb.min.map(round1),
                max: s.aabb.max.map(round1),
            },
            local_axes,
            orientation,
            position: overall
                .as_ref()
                .map_or_else(|| "centre".into(), |o| position_words(&s.aabb, o)),
        });
    }

    let mut contacts = Vec::new();
    let mut overlaps = Vec::new();
    let mut gaps = Vec::new();
    let mut rotated = Vec::new();
    for i in 0..solids.len() {
        for j in i + 1..solids.len() {
            let (a, b) = (&solids[i], &solids[j]);
            let both_aligned = a.aligned.is_some() && b.aligned.is_some();
            if !both_aligned {
                if let Some(depth) = sat_penetration(a, b)
                    && depth > tol
                {
                    overlaps.push(Overlap {
                        a: a.id,
                        a_name: a.name.clone(),
                        b: b.id,
                        b_name: b.name.clone(),
                        penetration_mm: round1(depth),
                        region: None,
                    });
                }
                rotated.push(format!("{} / {}", a.name, b.name));
                continue;
            }
            // Signed overlap per world axis (negative = separation).
            let overlap: [f64; 3] = std::array::from_fn(|k| {
                a.aabb.max[k].min(b.aabb.max[k]) - a.aabb.min[k].max(b.aabb.min[k])
            });
            if overlap.iter().all(|o| *o > tol) {
                let region = Aabb {
                    min: std::array::from_fn(|k| round1(a.aabb.min[k].max(b.aabb.min[k]))),
                    max: std::array::from_fn(|k| round1(a.aabb.max[k].min(b.aabb.max[k]))),
                };
                overlaps.push(Overlap {
                    a: a.id,
                    a_name: a.name.clone(),
                    b: b.id,
                    b_name: b.name.clone(),
                    penetration_mm: round1(overlap.iter().copied().fold(f64::INFINITY, f64::min)),
                    region: Some(region),
                });
                continue;
            }
            for k in 0..3 {
                let others = [(k + 1) % 3, (k + 2) % 3];
                if !others.iter().all(|&o| overlap[o] > tol) {
                    continue;
                }
                let separation = -overlap[k];
                // a below b along k when a's max is near b's min.
                let a_low =
                    (a.aabb.max[k] - b.aabb.min[k]).abs() <= (b.aabb.max[k] - a.aabb.min[k]).abs();
                if separation.abs() <= tol {
                    let (sa, sb) = if a_low { (1.0, -1.0) } else { (-1.0, 1.0) };
                    let patch = [overlap[others[0]], overlap[others[1]]];
                    contacts.push(Contact {
                        a: a.id,
                        a_name: a.name.clone(),
                        a_face: local_face(a, k, sa),
                        b: b.id,
                        b_name: b.name.clone(),
                        b_face: local_face(b, k, sb),
                        axis: AXIS[k],
                        area_mm2: round1(patch[0] * patch[1]),
                        patch_mm: patch.map(round1),
                    });
                } else if separation > tol && separation <= options.gap_report_mm {
                    gaps.push(Gap {
                        a: a.id,
                        a_name: a.name.clone(),
                        b: b.id,
                        b_name: b.name.clone(),
                        axis: AXIS[k],
                        distance_mm: round1(separation),
                    });
                }
            }
        }
    }
    let touching: HashSet<Uuid> = contacts
        .iter()
        .flat_map(|c| [c.a, c.b])
        .chain(overlaps.iter().flat_map(|o| [o.a, o.b]))
        .collect();
    let floating = if solids.len() > 1 {
        solids
            .iter()
            .filter(|s| !touching.contains(&s.id))
            .map(|s| s.name.clone())
            .collect()
    } else {
        Vec::new()
    };
    let text = narrative(
        &objects,
        overall.as_ref(),
        &contacts,
        &overlaps,
        &gaps,
        &floating,
    );
    SceneDescription {
        object_count: objects.len(),
        overall: overall.map(|o| Aabb {
            min: o.min.map(round1),
            max: o.max.map(round1),
        }),
        objects,
        contacts,
        overlaps,
        gaps,
        floating,
        rotated,
        text,
    }
}

fn narrative(
    objects: &[ObjectGeometry],
    overall: Option<&Aabb>,
    contacts: &[Contact],
    overlaps: &[Overlap],
    gaps: &[Gap],
    floating: &[String],
) -> String {
    let mut out = String::new();
    let Some(overall) = overall else {
        return "The scene is empty.".into();
    };
    let size: [f64; 3] = std::array::from_fn(|i| overall.max[i] - overall.min[i]);
    out.push_str(&format!(
        "{} objects. Overall {} (X, width) × {} (Y, depth) × {} (Z, height) mm, from ({}, {}, {}) to ({}, {}, {}). The front is -Y, Z is up.\n",
        objects.len(),
        fmt(size[0]),
        fmt(size[1]),
        fmt(size[2]),
        fmt(overall.min[0]),
        fmt(overall.min[1]),
        fmt(overall.min[2]),
        fmt(overall.max[0]),
        fmt(overall.max[1]),
        fmt(overall.max[2]),
    ));
    for o in objects {
        let touches: Vec<&str> = contacts
            .iter()
            .filter_map(|c| {
                if c.a == o.id {
                    Some(c.b_name.as_str())
                } else if c.b == o.id {
                    Some(c.a_name.as_str())
                } else {
                    None
                }
            })
            .collect();
        out.push_str(&format!(
            "- {}{}{}: {} × {} × {} mm, {}; x {}..{}, y {}..{}, z {}..{} ({}){}\n",
            o.name,
            o.parent
                .as_ref()
                .map_or(String::new(), |p| format!(" [in {p}]")),
            o.material
                .as_ref()
                .map_or(String::new(), |m| format!(" ({m})")),
            fmt(o.size_mm[0]),
            fmt(o.size_mm[1]),
            fmt(o.size_mm[2]),
            o.orientation,
            fmt(o.aabb.min[0]),
            fmt(o.aabb.max[0]),
            fmt(o.aabb.min[1]),
            fmt(o.aabb.max[1]),
            fmt(o.aabb.min[2]),
            fmt(o.aabb.max[2]),
            o.position,
            if touches.is_empty() {
                String::new()
            } else {
                format!("; touches {}", touches.join(", "))
            }
        ));
    }
    if overlaps.is_empty() {
        out.push_str("No overlaps: no two objects intersect.\n");
    } else {
        out.push_str(&format!(
            "{} OVERLAP(S) — these objects intersect, which is a design error for sheet parts:\n",
            overlaps.len()
        ));
        for o in overlaps {
            out.push_str(&format!(
                "- {} ∩ {} by {} mm{}\n",
                o.a_name,
                o.b_name,
                fmt(o.penetration_mm),
                o.region.as_ref().map_or(String::new(), |r| format!(
                    " (region x {}..{}, y {}..{}, z {}..{})",
                    fmt(r.min[0]),
                    fmt(r.max[0]),
                    fmt(r.min[1]),
                    fmt(r.max[1]),
                    fmt(r.min[2]),
                    fmt(r.max[2])
                ))
            ));
        }
    }
    if !gaps.is_empty() {
        out.push_str("Small gaps between facing objects:\n");
        for g in gaps {
            out.push_str(&format!(
                "- {} / {}: {} mm along {}\n",
                g.a_name,
                g.b_name,
                fmt(g.distance_mm),
                g.axis
            ));
        }
    }
    if !floating.is_empty() {
        out.push_str(&format!(
            "Touching nothing (floating): {}.\n",
            floating.join(", ")
        ));
    }
    out
}
