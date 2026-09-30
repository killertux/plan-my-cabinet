//! CorteCloud part lists ("Carregar arquivo Cortecloud", Serviço Completo).
//!
//! A part is `c` (length) × `l` (width) with edge band names on its four
//! sides and optional machining. CorteCloud's frame, seen from the part's
//! inner face ("i"): corner 0 top left, 1 bottom left, 2 bottom right,
//! 3 top right; C1 is the left long side, C2 the right, L2 the top and L1 the
//! bottom. A hole is placed from a corner: `x` along C from that corner's L
//! side, `y` along L from its C side.
//!
//! Mapping from a board: `c` follows the grain (the board's length for
//! length grain, its width for width grain, the longer side when the grain is
//! free). The inner face is the face with the most holes. Holes are measured
//! from their nearest corner, as CorteCloud's own examples do.
use serde::{Deserialize, Serialize, Serializer};

use super::FormatError;
use crate::domain::{BoardEdge, BoardFace, BoardGrain};
use crate::part_list::{PartGroup, PartList};
use crate::units::Length;

/// A CorteCloud import file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct File {
    pub parts: Vec<Part>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub quantity: u32,
    #[serde(serialize_with = "mm")]
    pub c: f64,
    #[serde(serialize_with = "mm")]
    pub l: f64,
    pub function: String,
    pub complement: Option<String>,
    pub c1: Option<String>,
    pub c2: Option<String>,
    pub l1: Option<String>,
    pub l2: Option<String>,
    pub material: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machining: Option<Machining>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Machining {
    #[serde(serialize_with = "mm")]
    pub x: f64,
    #[serde(serialize_with = "mm")]
    pub y: f64,
    #[serde(serialize_with = "mm")]
    pub z: f64,
    pub start_side: u8,
    pub horizontal_drills: Vec<HorizontalDrill>,
    pub vertical_drills: Vec<VerticalDrill>,
    pub furrow_machining: Option<Furrow>,
    pub furrow_machining_pair: Option<Furrow>,
}

/// A hole into the face of the part.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VerticalDrill {
    #[serde(serialize_with = "mm")]
    pub x: f64,
    #[serde(serialize_with = "mm")]
    pub y: f64,
    pub face: String,
    #[serde(serialize_with = "mm")]
    pub depth: f64,
    pub corner: u8,
    #[serde(serialize_with = "mm")]
    pub diameter: f64,
    /// Through hole.
    pub bolthole: bool,
}

/// A hole into an edge of the part.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HorizontalDrill {
    pub corner: u8,
    /// "XP" on the L sides, "YP" on the C sides.
    pub direction: String,
    #[serde(serialize_with = "mm")]
    pub x: f64,
    #[serde(serialize_with = "mm")]
    pub y: f64,
    #[serde(serialize_with = "mm")]
    pub z: f64,
    pub face: String,
    #[serde(serialize_with = "mm")]
    pub depth: f64,
    #[serde(serialize_with = "mm")]
    pub diameter: f64,
}

/// A groove or rebate along C2.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Furrow {
    pub face: String,
    #[serde(serialize_with = "mm")]
    pub depth: f64,
    #[serde(serialize_with = "mm")]
    pub width: f64,
    #[serde(serialize_with = "mm")]
    pub distance: f64,
}

/// Millimetres, written as integers when whole ("500", not "500.0").
fn mm<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    if value.fract() == 0.0 && value.abs() < 9.0e15 {
        serializer.serialize_i64(*value as i64)
    } else {
        serializer.serialize_f64(*value)
    }
}

fn to_mm(value: i128) -> f64 {
    // Micrometres to millimetres, rounded to the micrometre.
    value as f64 / 1000.0
}

fn length_mm(value: Length) -> f64 {
    to_mm(i128::from(value.micrometres()))
}

/// How a board's local frame lies in CorteCloud's part frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Orientation {
    /// `c` runs along the board's local Y (its width) instead of X.
    pub c_along_y: bool,
    /// The inner face ("i").
    pub inner: BoardFace,
}

impl Orientation {
    pub fn of(group: &PartGroup) -> Self {
        let c_along_y = match group.grain {
            BoardGrain::Length => false,
            BoardGrain::Width => true,
            BoardGrain::Unrestricted => group.width > group.length,
        };
        let on = |face| {
            group
                .machining
                .face_drills
                .iter()
                .filter(|d| d.face == face)
                .count()
        };
        let inner = if on(BoardFace::MinZ) > on(BoardFace::MaxZ) {
            BoardFace::MinZ
        } else {
            BoardFace::MaxZ
        };
        Self { c_along_y, inner }
    }

    /// Seen from the inner face with X (along C) pointing down and Y (along
    /// L) to the right, the board's own frame is either as is or mirrored:
    /// `Y` counts from the far side when the view is mirrored.
    fn mirrored(self) -> bool {
        // Looking at MaxZ from outside, local X down and local Y right is a
        // right-handed view. Swapping the axes or the face mirrors it.
        self.c_along_y == (self.inner == BoardFace::MaxZ)
    }

    /// Board-local XY (µm) to part coordinates (µm): distance along C from
    /// L2, and along L from C1.
    pub fn part_xy(self, local: [i128; 2], length: Length, width: Length) -> [i128; 2] {
        let (u, v, v_size) = if self.c_along_y {
            (local[1], local[0], i128::from(length.micrometres()))
        } else {
            (local[0], local[1], i128::from(width.micrometres()))
        };
        [u, if self.mirrored() { v_size - v } else { v }]
    }

    /// The board edge on each CorteCloud side: `[c1, c2, l1, l2]`.
    pub fn sides(self) -> [BoardEdge; 4] {
        // C sides run along C, at the ends of the L axis; L sides at the ends
        // of the C axis. L2 is where C starts (X = 0), C1 where L starts.
        let (c_min, c_max, l_min, l_max) = if self.c_along_y {
            (
                BoardEdge::MinY,
                BoardEdge::MaxY,
                BoardEdge::MinX,
                BoardEdge::MaxX,
            )
        } else {
            (
                BoardEdge::MinX,
                BoardEdge::MaxX,
                BoardEdge::MinY,
                BoardEdge::MaxY,
            )
        };
        let (c1, c2) = if self.mirrored() {
            (l_max, l_min)
        } else {
            (l_min, l_max)
        };
        [c1, c2, c_max, c_min]
    }
}

/// A hole relative to its nearest corner: `(corner, x, y)` in µm.
pub fn from_nearest_corner(part: [i128; 2], c: i128, l: i128) -> (u8, i128, i128) {
    let [x, y] = part;
    let options = [(0, x, y), (1, c - x, y), (2, c - x, l - y), (3, x, l - y)];
    options
        .into_iter()
        .min_by_key(|(corner, x, y)| (x * x + y * y, *corner))
        .expect("four corners")
}

/// The material as a shop reads it: its name and thickness ("MDF Branco 18").
fn material_label(group: &PartGroup) -> String {
    let thickness = length_mm(group.thickness);
    let number = if thickness.fract() == 0.0 {
        format!("{thickness:.0}")
    } else {
        format!("{thickness}").replace('.', ",")
    };
    let name = group.material.trim();
    if name.split_whitespace().any(|word| word == number) {
        name.to_owned()
    } else {
        format!("{name} {number}")
    }
}

pub fn part(group: &PartGroup) -> Part {
    let orientation = Orientation::of(group);
    let (c, l) = if orientation.c_along_y {
        (group.width, group.length)
    } else {
        (group.length, group.width)
    };
    let (c_um, l_um) = (i128::from(c.micrometres()), i128::from(l.micrometres()));
    let band = |edge: BoardEdge| group.banding[edge.index()].as_ref().map(|b| b.name.clone());
    let [c1, c2, l1, l2] = orientation.sides();
    let vertical_drills: Vec<VerticalDrill> = group
        .machining
        .face_drills
        .iter()
        .map(|drill| {
            let at = orientation.part_xy(drill.at_um, group.length, group.width);
            let (corner, x, y) = from_nearest_corner(at, c_um, l_um);
            VerticalDrill {
                x: to_mm(x),
                y: to_mm(y),
                face: if drill.face == orientation.inner {
                    "i"
                } else {
                    "e"
                }
                .into(),
                depth: length_mm(drill.depth),
                corner,
                diameter: length_mm(drill.diameter),
                bolthole: drill.through,
            }
        })
        .collect();
    let machining = (!group.machining.is_empty()).then(|| Machining {
        x: length_mm(c),
        y: length_mm(l),
        z: length_mm(group.thickness),
        start_side: 0,
        horizontal_drills: Vec::new(),
        vertical_drills,
        furrow_machining: None,
        furrow_machining_pair: None,
    });
    Part {
        quantity: group.quantity() as u32,
        c: length_mm(c),
        l: length_mm(l),
        function: group.name.clone(),
        complement: group.cabinet.clone(),
        c1: band(c1),
        c2: band(c2),
        l1: band(l1),
        l2: band(l2),
        material: material_label(group),
        machining,
    }
}

pub fn file(list: &PartList) -> File {
    File {
        parts: list.groups.iter().map(part).collect(),
    }
}

pub fn render(list: &PartList) -> Result<Vec<u8>, FormatError> {
    let mut bytes = serde_json::to_vec_pretty(&file(list))
        .map_err(|error| FormatError::Encode(error.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

#[cfg(test)]
mod tests;
