//! Pinned facts for catalog hardware other than hinges: drawer slides and
//! furniture feet. Lengths are exact micrometres, like every project length.
//!
//! A foot's local frame starts at the minimum corner of its bounding box with Z
//! up; the mounting plate (what touches the furniture) is at Z = height. That
//! is the placeholder convention, so every box-based path treats both alike.
use serde::{Deserialize, Serialize};

use crate::domain::SrgbColor;
use crate::units::Length;

/// Hardware facts that are not hinges; hinge snapshots keep `verified_hinge`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CatalogItem {
    Slide(SlideSpec),
    Foot(FootSpec),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlideExtension {
    #[default]
    Full,
    Partial,
    Over,
}

/// One screw position on a slide member: `along` from the member's front end,
/// `offset` up (+) or down (−) from its centre line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlideHole {
    pub along: Length,
    #[serde(default)]
    pub offset: Length,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diameter: Option<Length>,
}

/// One purchasable slide pair (one length of a family), side mounted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlideSpec {
    /// Display family name, e.g. "TT45 Slowmotion".
    pub family: String,
    /// Nominal length: the closed cabinet member.
    pub length: Length,
    /// How far the drawer member runs out past the closed position.
    pub travel: Length,
    /// Member height (the profile seen from the front).
    pub height: Length,
    /// Gap between drawer box side and cabinet side, per side.
    pub clearance: Length,
    pub clearance_minus: Length,
    pub clearance_plus: Length,
    /// Cabinet member front, behind the cabinet front edge.
    pub front_setback: Length,
    #[serde(default)]
    pub extension: SlideExtension,
    #[serde(default)]
    pub soft_close: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity_kg: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish: Option<String>,
    #[serde(default)]
    pub cabinet_holes: Vec<SlideHole>,
    #[serde(default)]
    pub drawer_holes: Vec<SlideHole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rear_fixing: Option<String>,
    #[serde(default)]
    pub printed_page: u16,
    #[serde(default)]
    pub pdf_page: u16,
    /// SHA-256 of the cited manufacturer sheet; empty for user models.
    #[serde(default)]
    pub source_sha256: String,
    #[serde(default)]
    pub attribution: String,
}

/// A cross-section: round or rectangular (X by Y).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Section {
    Round { diameter: Length },
    Rect { width: Length, depth: Length },
}

impl Section {
    /// Footprint along X and Y.
    pub const fn size(self) -> [Length; 2] {
        match self {
            Self::Round { diameter } => [diameter, diameter],
            Self::Rect { width, depth } => [width, depth],
        }
    }

    pub const fn is_round(self) -> bool {
        matches!(self, Self::Round { .. })
    }
}

/// Levelling glide under a post or frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Glide {
    pub diameter: Length,
    pub height: Length,
}

/// The foot families. Dimensions are the product's own, so the 3-D model reads
/// like the real thing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FootShape {
    /// A solid frustum (the small plastic feet), round or square.
    Tapered {
        top: Section,
        bottom: Section,
        height: Length,
    },
    /// Tube with a top mounting plate and an optional glide: chrome furniture
    /// feet and straight table legs.
    Post {
        tube: Section,
        plate: Section,
        plate_thickness: Length,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        glide: Option<Glide>,
        height: Length,
    },
    /// A closed tube frame in the X–Z plane (industrial table legs): a
    /// rectangle when both widths match, a trapezoid or a V otherwise.
    Frame {
        height: Length,
        top_width: Length,
        bottom_width: Length,
        /// Tube size in the frame plane.
        tube_width: Length,
        /// Tube size across the frame (the frame's depth).
        tube_depth: Length,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        crossbar_height: Option<Length>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        glide: Option<Glide>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FootSpec {
    pub shape: FootShape,
    pub color: SrgbColor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish: Option<String>,
    /// Levelling travel; zero when the foot is fixed.
    #[serde(default)]
    pub adjustment: Length,
    /// Screw positions on the mounting face, relative to its centre (X, Y).
    #[serde(default)]
    pub mounting_holes: Vec<[Length; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_kg: Option<u16>,
    #[serde(default)]
    pub attribution: String,
}

fn max(a: Length, b: Length) -> Length {
    if a >= b { a } else { b }
}

impl FootShape {
    pub const fn height(&self) -> Length {
        match self {
            Self::Tapered { height, .. }
            | Self::Post { height, .. }
            | Self::Frame { height, .. } => *height,
        }
    }

    /// Bounding box along local X, Y, Z.
    pub fn local_size(&self) -> [Length; 3] {
        match self {
            Self::Tapered {
                top,
                bottom,
                height,
                ..
            } => {
                let [tx, ty] = top.size();
                let [bx, by] = bottom.size();
                [max(tx, bx), max(ty, by), *height]
            }
            Self::Post {
                tube,
                plate,
                glide,
                height,
                ..
            } => {
                let [mut x, mut y] = tube.size();
                let [px, py] = plate.size();
                x = max(x, px);
                y = max(y, py);
                if let Some(glide) = glide {
                    x = max(x, glide.diameter);
                    y = max(y, glide.diameter);
                }
                [x, y, *height]
            }
            Self::Frame {
                height,
                top_width,
                bottom_width,
                tube_depth,
                glide,
                ..
            } => {
                let depth = glide.map_or(*tube_depth, |g| max(*tube_depth, g.diameter));
                [max(*top_width, *bottom_width), depth, *height]
            }
        }
    }

    /// Internally coherent dimensions: everything positive and the parts fit
    /// inside the height and width they claim.
    pub fn is_consistent(&self) -> bool {
        let positive = |values: &[Length]| values.iter().all(|v| v.micrometres() > 0);
        let section = |s: &Section| positive(&s.size());
        match self {
            Self::Tapered {
                top,
                bottom,
                height,
                ..
            } => section(top) && section(bottom) && positive(&[*height]),
            Self::Post {
                tube,
                plate,
                plate_thickness,
                glide,
                height,
            } => {
                let glide_height = glide.map_or(0, |g| g.height.micrometres());
                section(tube)
                    && section(plate)
                    && positive(&[*plate_thickness, *height])
                    && glide.is_none_or(|g| positive(&[g.diameter, g.height]))
                    && plate_thickness.micrometres() + glide_height < height.micrometres()
            }
            Self::Frame {
                height,
                top_width,
                bottom_width,
                tube_width,
                tube_depth,
                crossbar_height,
                glide,
            } => {
                let glide_height = glide.map_or(0, |g| g.height.micrometres());
                positive(&[*height, *top_width, *bottom_width, *tube_width, *tube_depth])
                    && glide.is_none_or(|g| positive(&[g.diameter, g.height]))
                    && 2 * tube_width.micrometres() < top_width.micrometres()
                    && tube_width.micrometres() <= bottom_width.micrometres()
                    && 2 * tube_width.micrometres() + glide_height < height.micrometres()
                    && crossbar_height.is_none_or(|c| {
                        c.micrometres() > tube_width.micrometres() + glide_height
                            && c.micrometres() < height.micrometres() - 2 * tube_width.micrometres()
                    })
            }
        }
    }
}

impl FootSpec {
    pub fn local_size(&self) -> [Length; 3] {
        self.shape.local_size()
    }

    pub fn is_consistent(&self) -> bool {
        let size = self.local_size();
        self.shape.is_consistent()
            && self.adjustment.micrometres() >= 0
            && self.mounting_holes.iter().all(|[x, y]| {
                2 * x.micrometres().abs() <= size[0].micrometres()
                    && 2 * y.micrometres().abs() <= size[1].micrometres()
            })
    }
}

/// Largest over-extension any listed slide offers beyond its length.
pub const MAX_OVER_TRAVEL: Length = Length::from_micrometres(100_000);

impl SlideSpec {
    pub fn is_consistent(&self) -> bool {
        let positive = [self.length, self.travel, self.height, self.clearance]
            .iter()
            .all(|v| v.micrometres() > 0);
        let holes = |holes: &[SlideHole], length: Length| {
            holes.iter().all(|h| {
                (0..=length.micrometres()).contains(&h.along.micrometres())
                    && 2 * h.offset.micrometres().abs() <= self.height.micrometres()
                    && h.diameter.is_none_or(|d| d.micrometres() > 0)
            })
        };
        positive
            && self.clearance_minus.micrometres() >= 0
            && self.clearance_plus.micrometres() >= 0
            && self.clearance_minus < self.clearance
            && self.front_setback.micrometres() >= 0
            && self.travel.micrometres()
                <= self.length.micrometres() + MAX_OVER_TRAVEL.micrometres()
            && holes(&self.cabinet_holes, self.length)
            && holes(&self.drawer_holes, self.length)
            && (self.source_sha256.is_empty()
                || (self.source_sha256.len() == 64
                    && self.source_sha256.bytes().all(|b| b.is_ascii_hexdigit())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mm(value: i64) -> Length {
        Length::from_micrometres(value * 1000)
    }

    fn square_post() -> FootShape {
        FootShape::Post {
            tube: Section::Rect {
                width: mm(30),
                depth: mm(30),
            },
            plate: Section::Rect {
                width: mm(60),
                depth: mm(60),
            },
            plate_thickness: mm(2),
            glide: Some(Glide {
                diameter: mm(35),
                height: mm(10),
            }),
            height: mm(100),
        }
    }

    #[test]
    fn post_box_is_the_widest_part_by_the_height() {
        assert_eq!(square_post().local_size(), [mm(60), mm(60), mm(100)]);
        assert!(square_post().is_consistent());
    }

    #[test]
    fn frame_box_uses_the_wider_bar() {
        let frame = FootShape::Frame {
            height: mm(710),
            top_width: mm(500),
            bottom_width: mm(400),
            tube_width: mm(30),
            tube_depth: mm(30),
            crossbar_height: None,
            glide: None,
        };
        assert_eq!(frame.local_size(), [mm(500), mm(30), mm(710)]);
        assert!(frame.is_consistent());
    }

    #[test]
    fn parts_taller_than_the_foot_are_inconsistent() {
        let FootShape::Post {
            tube, plate, glide, ..
        } = square_post()
        else {
            unreachable!()
        };
        let short = FootShape::Post {
            tube,
            plate,
            plate_thickness: mm(2),
            glide,
            height: mm(12),
        };
        assert!(!short.is_consistent());
    }

    #[test]
    fn items_serialize_with_a_kind_tag() {
        let item = CatalogItem::Foot(FootSpec {
            shape: square_post(),
            color: SrgbColor([200, 204, 208]),
            finish: None,
            adjustment: mm(10),
            mounting_holes: Vec::new(),
            load_kg: None,
            attribution: String::new(),
        });
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(json["kind"], "foot");
        assert_eq!(json["shape"]["kind"], "post");
        assert_eq!(serde_json::from_value::<CatalogItem>(json).unwrap(), item);
    }
}
