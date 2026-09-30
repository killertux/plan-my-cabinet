//! Drawer slide pairs: finding the drawer box and carcass sides, fitting and
//! checking a catalog slide, hole references for the shop, and the drawer's
//! display-only slide-out motion.
//!
//! Hole coordinates are integer micrometres from each board's minimum corner,
//! like hinge references. Only horizontal slides on upright sides are handled.
use std::collections::HashSet;

use uuid::Uuid;

use crate::assembly_edit::world_pose;
use crate::board_frame::{BoardFrame, dot};
use crate::commands::{EditError, ProjectEditor};
use crate::domain::{
    BoardEdge, BoardFace, CatalogReference, DomainError, Project, SlideInstallation,
    SlideMountingSide, SlideSpec,
};
use crate::door_joint::moving_members;
use crate::hardware_catalog;
use crate::units::{Length, Pose};

/// A world box: the pose of its minimum corner and its size in mm.
pub type MemberBox = (Pose, [f64; 3]);

/// Parallel within about 1°.
const PARALLEL: f64 = 0.999;
/// Slack when comparing measured world distances with exact catalog values.
const SLACK_UM: i64 = 1;
/// The drawer member is shorter than the nominal length (the cabinet
/// member), so a slide as long as the box side fits it: 500 mm slides go on
/// 500 mm boxes.
pub const DRAWER_MEMBER_SHORTER: Length = Length::from_micrometres(10_000);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlideIssue {
    MissingPart(Uuid),
    MissingCatalog,
    /// The sides are not upright and parallel, or the drawer does not run
    /// along the carcass sides.
    NotParallel,
    /// The gap between box side and carcass side is outside the slide's range.
    ClearanceOutOfRange {
        side: usize,
        measured: Length,
    },
    /// Setback plus slide length is deeper than the carcass side.
    TooLongForDepth {
        side: usize,
        available: Length,
    },
    /// The drawer member does not fit on the box side.
    LongerThanDrawer {
        side: usize,
        available: Length,
    },
    /// The slide's height does not fit on the box side at this position.
    HeightExceedsBoxSide,
    /// The slide's height falls beyond the carcass side.
    OutsideMount {
        side: usize,
    },
    /// The two slides are not at the same height or depth.
    Misaligned,
}

impl SlideIssue {
    /// Localization key describing the issue.
    pub const fn key(&self) -> &'static str {
        match self {
            Self::MissingPart(_) => "slide-issue-missing-part",
            Self::MissingCatalog => "slide-issue-missing-catalog",
            Self::NotParallel => "slide-issue-not-parallel",
            Self::ClearanceOutOfRange { .. } => "slide-issue-clearance",
            Self::TooLongForDepth { .. } => "slide-issue-too-long",
            Self::LongerThanDrawer { .. } => "slide-issue-longer-than-drawer",
            Self::HeightExceedsBoxSide => "slide-issue-height",
            Self::OutsideMount { .. } => "slide-issue-outside-mount",
            Self::Misaligned => "slide-issue-misaligned",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlideNotice {
    /// The drawer does not open fully.
    PartialExtension { travel: Length },
}

/// Hole references for one side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlideSideReferences {
    pub cabinet_board: Uuid,
    pub cabinet_face: BoardFace,
    pub cabinet_front_edge: BoardEdge,
    pub cabinet_holes_um: Vec<[i128; 3]>,
    /// Hole distances from the carcass side's front edge.
    pub cabinet_hole_distances: Vec<Length>,
    /// Slide centre line above the carcass side's bottom edge.
    pub cabinet_centre_from_bottom: Length,
    pub drawer_board: Uuid,
    pub drawer_face: BoardFace,
    pub drawer_front_edge: BoardEdge,
    pub drawer_holes_um: Vec<[i128; 3]>,
    /// Hole distances from the box side's front edge.
    pub drawer_hole_distances: Vec<Length>,
    pub drawer_centre_from_bottom: Length,
    pub gap: Length,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlideReferences {
    pub product_id: String,
    pub family: String,
    pub length: Length,
    pub travel: Length,
    pub member_height: Length,
    pub clearance: Length,
    pub clearance_minus: Length,
    pub clearance_plus: Length,
    pub setback: Length,
    pub sides: [SlideSideReferences; 2],
    pub source: String,
    pub attribution: String,
    pub printed_page: u16,
    pub pdf_page: u16,
    pub trust: hardware_catalog::Trust,
    pub pack: Option<String>,
    pub rear_fixing: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlideStatus {
    pub id: Uuid,
    pub issues: Vec<SlideIssue>,
    pub notices: Vec<SlideNotice>,
    /// Present whenever the geometry can be measured, even with issues.
    pub references: Option<SlideReferences>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlideFitError {
    MissingPart,
    /// No group of boards that moves together (an assembly) holds the drawer.
    NoDrawerAssembly,
    /// No pair of box sides with a carcass side next to each was found.
    NoSides,
    NotParallel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlideEditError {
    MissingInstallation,
    MissingCatalog,
    NotASlide,
    InvalidDistance,
    /// This drawer already runs on slides.
    DrawerHasSlides,
    Fit(SlideFitError),
    /// Nothing in the family fits the drawer and carcass.
    NoLengthFits,
    InvalidExtension,
}

/// One side, measured: which edges and faces, and the room available.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SideGeometry {
    pub mounting: SlideMountingSide,
    /// Face-to-face gap, box side to carcass side. Negative when they overlap.
    pub gap: Length,
    /// Carcass side depth, measured from its front edge.
    pub cabinet_depth: Length,
    /// Box side length along the pull direction.
    pub drawer_depth: Length,
    /// How far the box side's front edge sits behind the carcass front edge.
    pub drawer_front_behind: Length,
    /// Box side height.
    pub box_height: Length,
}

/// The four boards of a drawer on slides and their measurements.
#[derive(Clone, Debug, PartialEq)]
pub struct Detected {
    pub drawer_root: Uuid,
    pub drawer_sides: [Uuid; 2],
    pub cabinet_sides: [Uuid; 2],
    pub geometry: [SideGeometry; 2],
    /// World direction the drawer opens in.
    pub pull: [f64; 3],
}

impl Detected {
    pub fn sides(&self) -> [SlideMountingSide; 2] {
        self.geometry.map(|g| g.mounting)
    }
}

/// A proposed slide for a drawer: the length that fits, its position and the
/// checks it would get.
#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    pub detected: Detected,
    pub catalog: CatalogReference,
    pub height: Length,
    pub setback: Length,
    pub status: SlideStatus,
}

fn um(mm: f64) -> Length {
    Length::from_micrometres((mm * 1000.0).round() as i64)
}

fn upright(frame: &BoardFrame) -> bool {
    frame.axes[2][2].abs() < 1.0 - PARALLEL
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}

/// The in-plane axis (0 or 1) parallel to `direction`, if any.
fn axis_along(frame: &BoardFrame, direction: [f64; 3]) -> Option<usize> {
    (0..2).find(|&a| dot(frame.axes[a], direction).abs() > PARALLEL)
}

/// The edge at the end of in-plane axis `axis` that lies furthest toward `direction`.
fn end_edge(frame: &BoardFrame, axis: usize, direction: [f64; 3]) -> BoardEdge {
    let (min, max) = if axis == 0 {
        (BoardEdge::MinX, BoardEdge::MaxX)
    } else {
        (BoardEdge::MinY, BoardEdge::MaxY)
    };
    if dot(frame.edge_mid(max), direction) >= dot(frame.edge_mid(min), direction) {
        max
    } else {
        min
    }
}

/// The in-plane axis an end edge sits at the end of.
const fn edge_axis(edge: BoardEdge) -> usize {
    1 - edge.along_axis()
}

const fn at_min(edge: BoardEdge) -> bool {
    matches!(edge, BoardEdge::MinX | BoardEdge::MinY)
}

fn face_toward(frame: &BoardFrame, target: [f64; 3]) -> BoardFace {
    if dot(frame.axes[2], sub(target, frame.centre())) >= 0.0 {
        BoardFace::MaxZ
    } else {
        BoardFace::MinZ
    }
}

fn face_z_mm(frame: &BoardFrame, face: BoardFace) -> f64 {
    match face {
        BoardFace::MinZ => 0.0,
        BoardFace::MaxZ => frame.size[2],
    }
}

/// World pull direction: out of the carcass through its front edge.
fn pull_from(frame: &BoardFrame, front: BoardEdge) -> [f64; 3] {
    let axis = frame.axes[edge_axis(front)];
    if at_min(front) {
        axis.map(|v| -v)
    } else {
        axis
    }
}

/// Board-local point (mm) on a side: `depth` behind the front edge, `height`
/// above the bottom edge, on `face`.
fn side_point(
    frame: &BoardFrame,
    front: BoardEdge,
    bottom: BoardEdge,
    face: BoardFace,
    depth: f64,
    height: f64,
) -> [f64; 3] {
    let mut local = [0.0; 3];
    let (a, v) = (edge_axis(front), edge_axis(bottom));
    local[a] = if at_min(front) {
        depth
    } else {
        frame.size[a] - depth
    };
    local[v] = if at_min(bottom) {
        height
    } else {
        frame.size[v] - height
    };
    local[2] = face_z_mm(frame, face);
    local
}

/// Measure one side from the drawer box side and the carcass side, with the
/// pull direction known.
fn measure_side(
    drawer: &BoardFrame,
    cabinet: &BoardFrame,
    pull: [f64; 3],
) -> Result<SideGeometry, SlideFitError> {
    if !upright(drawer)
        || !upright(cabinet)
        || dot(drawer.axes[2], cabinet.axes[2]).abs() < PARALLEL
    {
        return Err(SlideFitError::NotParallel);
    }
    let (Some(da), Some(ca)) = (axis_along(drawer, pull), axis_along(cabinet, pull)) else {
        return Err(SlideFitError::NotParallel);
    };
    let dv = 1 - da;
    let down = [0.0, 0.0, -1.0];
    let drawer_front_edge = end_edge(drawer, da, pull);
    let drawer_bottom_edge = end_edge(drawer, dv, down);
    let cabinet_front_edge = end_edge(cabinet, ca, pull);
    let drawer_face = face_toward(drawer, cabinet.centre());
    let cabinet_face = face_toward(cabinet, drawer.centre());
    // Gap along the drawer side's normal, from its outer face to the carcass face.
    let outward = {
        let n = drawer.axes[2];
        if drawer_face == BoardFace::MaxZ {
            n
        } else {
            n.map(|v| -v)
        }
    };
    let drawer_face_point = drawer.world([0.0, 0.0, face_z_mm(drawer, drawer_face)]);
    let cabinet_face_point = cabinet.world([0.0, 0.0, face_z_mm(cabinet, cabinet_face)]);
    let gap = dot(sub(cabinet_face_point, drawer_face_point), outward);
    let behind = dot(
        sub(
            cabinet.edge_mid(cabinet_front_edge),
            drawer.edge_mid(drawer_front_edge),
        ),
        pull,
    );
    Ok(SideGeometry {
        mounting: SlideMountingSide {
            cabinet_front_edge,
            cabinet_face,
            drawer_front_edge,
            drawer_bottom_edge,
            drawer_face,
        },
        gap: um(gap),
        cabinet_depth: um(cabinet.size[ca]),
        drawer_depth: um(drawer.size[da]),
        drawer_front_behind: um(behind),
        box_height: um(drawer.size[dv]),
    })
}

fn corners(frame: &BoardFrame) -> Vec<[f64; 3]> {
    (0..8)
        .map(|i| {
            frame.world([
                if i & 1 == 0 { 0.0 } else { frame.size[0] },
                if i & 2 == 0 { 0.0 } else { frame.size[1] },
                if i & 4 == 0 { 0.0 } else { frame.size[2] },
            ])
        })
        .collect()
}

/// The pull direction for a drawer between these boards: along the carcass
/// sides, toward the end the drawer sticks out of (world −Y when unclear).
fn pull_direction(
    project: &Project,
    drawer_root: Uuid,
    drawer_side: &BoardFrame,
    cabinet_side: &BoardFrame,
) -> Option<[f64; 3]> {
    let axis_index = (0..2).find(|&a| {
        dot(cabinet_side.axes[a], [0.0, 0.0, 1.0]).abs() < 1.0 - PARALLEL
            && axis_along(drawer_side, cabinet_side.axes[a]).is_some()
    })?;
    let axis = cabinet_side.axes[axis_index];
    let span = |points: &mut dyn Iterator<Item = [f64; 3]>| {
        points.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            let d = dot(p, axis);
            (lo.min(d), hi.max(d))
        })
    };
    let (side_lo, side_hi) = span(&mut corners(cabinet_side).into_iter());
    let frames: Vec<_> = moving_members(project, drawer_root)
        .into_iter()
        .filter_map(|id| BoardFrame::new(project, id))
        .collect();
    let (drawer_lo, drawer_hi) = span(&mut frames.iter().flat_map(corners));
    let beyond_min = side_lo - drawer_lo;
    let beyond_max = drawer_hi - side_hi;
    let positive = if (beyond_min - beyond_max).abs() > 0.5 {
        beyond_max > beyond_min
    } else {
        dot(axis, [0.0, -1.0, 0.0]) >= 0.0
    };
    Some(if positive { axis } else { axis.map(|v| -v) })
}

/// The drawer that `id` belongs to: an assembly as given, or a board's
/// parent assembly.
pub fn drawer_root(project: &Project, id: Uuid) -> Option<Uuid> {
    if project.assemblies.iter().any(|a| a.id == id) {
        return Some(id);
    }
    project.board(id).and_then(|b| b.parent_id)
}

/// Measure a drawer given its four boards.
pub fn measure(
    project: &Project,
    drawer_root: Uuid,
    drawer_sides: [Uuid; 2],
    cabinet_sides: [Uuid; 2],
) -> Result<Detected, SlideFitError> {
    let frame = |id| BoardFrame::new(project, id).ok_or(SlideFitError::MissingPart);
    let drawers = [frame(drawer_sides[0])?, frame(drawer_sides[1])?];
    let cabinets = [frame(cabinet_sides[0])?, frame(cabinet_sides[1])?];
    let pull = pull_direction(project, drawer_root, &drawers[0], &cabinets[0])
        .ok_or(SlideFitError::NotParallel)?;
    let geometry = [
        measure_side(&drawers[0], &cabinets[0], pull)?,
        measure_side(&drawers[1], &cabinets[1], pull)?,
    ];
    // Left first, as seen from the front looking in (for a front at -Y,
    // left is -X).
    let across = [-pull[1], pull[0], 0.0];
    let order = dot(drawers[0].centre(), across) <= dot(drawers[1].centre(), across);
    let (drawer_sides, cabinet_sides, geometry) = if order {
        (drawer_sides, cabinet_sides, geometry)
    } else {
        (
            [drawer_sides[1], drawer_sides[0]],
            [cabinet_sides[1], cabinet_sides[0]],
            [geometry[1], geometry[0]],
        )
    };
    Ok(Detected {
        drawer_root,
        drawer_sides,
        cabinet_sides,
        geometry,
        pull,
    })
}

/// The nearest board outside the drawer, parallel to `side` and beyond it
/// (away from `other`), whose face overlaps it. Returns (board, gap in mm).
fn outside_partner(
    project: &Project,
    members: &HashSet<Uuid>,
    side: &BoardFrame,
    other: &BoardFrame,
) -> Option<(Uuid, f64)> {
    let away = {
        let n = side.axes[2];
        if dot(n, sub(side.centre(), other.centre())) >= 0.0 {
            n
        } else {
            n.map(|v| -v)
        }
    };
    let side_out = dot(side.centre(), away) + side.size[2] / 2.0;
    project
        .boards
        .iter()
        .filter(|b| !members.contains(&b.id))
        .filter_map(|b| {
            let frame = BoardFrame::new(project, b.id)?;
            if dot(frame.axes[2], side.axes[2]).abs() < PARALLEL {
                return None;
            }
            let inner = dot(frame.centre(), away) - frame.size[2] / 2.0;
            let gap = inner - side_out;
            // The carcass side's face must overlap the box side's face.
            let overlaps = (0..2).all(|a| {
                let axis = side.axes[a];
                let lo = |f: &BoardFrame| {
                    let c = dot(f.centre(), axis);
                    let half: f64 = (0..3)
                        .map(|k| dot(f.axes[k], axis).abs() * f.size[k] / 2.0)
                        .sum();
                    (c - half, c + half)
                };
                let (a0, a1) = lo(side);
                let (b0, b1) = lo(&frame);
                a0.max(b0) < a1.min(b1) - 1.0
            });
            (overlaps && gap > -0.5 && gap < 60.0).then_some((b.id, gap))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

/// Find a drawer's box sides and the carcass sides beside them.
pub fn detect(project: &Project, drawer: Uuid) -> Result<Detected, SlideFitError> {
    let root = drawer_root(project, drawer).ok_or(SlideFitError::NoDrawerAssembly)?;
    let members: HashSet<_> = moving_members(project, root).into_iter().collect();
    let boards: Vec<_> = project
        .boards
        .iter()
        .filter(|b| members.contains(&b.id))
        .filter_map(|b| BoardFrame::new(project, b.id).map(|f| (b.id, f)))
        .filter(|(_, f)| upright(f))
        .collect();
    let mut best: Option<(f64, [Uuid; 2], [Uuid; 2])> = None;
    for (i, (a, fa)) in boards.iter().enumerate() {
        for (b, fb) in &boards[i + 1..] {
            if dot(fa.axes[2], fb.axes[2]).abs() < PARALLEL {
                continue;
            }
            let apart = dot(sub(fb.centre(), fa.centre()), fa.axes[2]).abs();
            if apart < 50.0 {
                continue;
            }
            let (Some((ca, ga)), Some((cb, gb))) = (
                outside_partner(project, &members, fa, fb),
                outside_partner(project, &members, fb, fa),
            ) else {
                continue;
            };
            let score = ga + gb;
            if best.as_ref().is_none_or(|(s, ..)| score < *s) {
                best = Some((score, [*a, *b], [ca, cb]));
            }
        }
    }
    let (_, drawer_sides, cabinet_sides) = best.ok_or(SlideFitError::NoSides)?;
    measure(project, root, drawer_sides, cabinet_sides)
}

/// Where the slide sits on a drawer: the setback and the drawer member's
/// start on the box side.
fn position(spec: &SlideSpec, geometry: &SideGeometry) -> (Length, i64) {
    let behind = geometry.drawer_front_behind.micrometres();
    let setback = spec.front_setback.micrometres().max(behind);
    (Length::from_micrometres(setback), setback - behind)
}

/// Whether one length fits: the room on the carcass side and on the box side.
fn fits(spec: &SlideSpec, setback: Length, geometry: &SideGeometry) -> bool {
    let start = setback.micrometres() - geometry.drawer_front_behind.micrometres();
    setback.micrometres() + spec.length.micrometres()
        <= geometry.cabinet_depth.micrometres() + SLACK_UM
        && start >= -SLACK_UM
        && start + drawer_member(spec) <= geometry.drawer_depth.micrometres() + SLACK_UM
}

fn drawer_member(spec: &SlideSpec) -> i64 {
    (spec.length.micrometres() - DRAWER_MEMBER_SHORTER.micrometres()).max(0)
}

/// Pick the longest length of `family` that fits the drawer, centred on the
/// box side's height.
pub fn suggest(
    project: &Project,
    detected: Detected,
    family: &[CatalogReference],
) -> Result<Suggestion, SlideEditError> {
    let mut candidates: Vec<_> = family
        .iter()
        .filter_map(|entry| entry.slide().map(|spec| (entry, spec)))
        .collect();
    if candidates.is_empty() {
        return Err(SlideEditError::NotASlide);
    }
    candidates.sort_by_key(|(_, spec)| std::cmp::Reverse(spec.length));
    let (entry, setback) = candidates
        .into_iter()
        .find_map(|(entry, spec)| {
            let setbacks = detected.geometry.map(|g| position(spec, &g).0);
            let setback = setbacks[0].max(setbacks[1]);
            detected
                .geometry
                .iter()
                .all(|g| fits(spec, setback, g))
                .then_some((entry, setback))
        })
        .ok_or(SlideEditError::NoLengthFits)?;
    let box_height = detected.geometry[0]
        .box_height
        .min(detected.geometry[1].box_height);
    let height = Length::from_micrometres(box_height.micrometres() / 2);
    let mut catalog = entry.clone();
    let installation = SlideInstallation {
        id: Uuid::new_v4(),
        catalog_id: catalog.id,
        drawer_root_id: detected.drawer_root,
        drawer_sides: detected.drawer_sides,
        cabinet_sides: detected.cabinet_sides,
        sides: detected.sides(),
        height,
        setback,
    };
    let status = if project.catalog.iter().any(|c| c.id == catalog.id) {
        diagnose(project, &installation)
    } else {
        let mut scratch = project.clone();
        catalog.id = installation.catalog_id;
        scratch.catalog.push(catalog.clone());
        diagnose(&scratch, &installation)
    };
    Ok(Suggestion {
        detected,
        catalog,
        height,
        setback,
        status,
    })
}

fn hole_distances(holes: &[crate::domain::SlideHole], start: i64) -> Vec<Length> {
    holes
        .iter()
        .map(|h| Length::from_micrometres(start + h.along.micrometres()))
        .collect()
}

/// Every check for one installation, plus hole references when measurable.
pub fn diagnose(project: &Project, installation: &SlideInstallation) -> SlideStatus {
    let mut issues = Vec::new();
    let mut notices = Vec::new();
    let status = |issues, notices, references| SlideStatus {
        id: installation.id,
        issues,
        notices,
        references,
    };
    for id in installation
        .drawer_sides
        .iter()
        .chain(&installation.cabinet_sides)
    {
        if project.board(*id).is_none() {
            issues.push(SlideIssue::MissingPart(*id));
        }
    }
    let entry = project
        .catalog
        .iter()
        .find(|c| c.id == installation.catalog_id);
    let Some((entry, spec)) = entry.and_then(|e| e.slide().map(|s| (e, s))) else {
        issues.push(SlideIssue::MissingCatalog);
        return status(issues, notices, None);
    };
    if !issues.is_empty() {
        return status(issues, notices, None);
    }
    let Ok(detected) = measure(
        project,
        installation.drawer_root_id,
        installation.drawer_sides,
        installation.cabinet_sides,
    ) else {
        issues.push(SlideIssue::NotParallel);
        return status(issues, notices, None);
    };
    // `measure` may reorder; keep the stored order for references.
    let geometry = if detected.drawer_sides == installation.drawer_sides {
        detected.geometry
    } else {
        [detected.geometry[1], detected.geometry[0]]
    };
    let frames = |ids: [Uuid; 2]| ids.map(|id| BoardFrame::new(project, id));
    let [Some(d0), Some(d1)] = frames(installation.drawer_sides) else {
        return status(issues, notices, None);
    };
    let [Some(c0), Some(c1)] = frames(installation.cabinet_sides) else {
        return status(issues, notices, None);
    };
    let drawers = [d0, d1];
    let cabinets = [c0, c1];
    let (min, max) = (
        spec.clearance.micrometres() - spec.clearance_minus.micrometres(),
        spec.clearance.micrometres() + spec.clearance_plus.micrometres(),
    );
    let setback = installation.setback.micrometres();
    let length = spec.length.micrometres();
    let half = spec.height.micrometres() / 2;
    let height = installation.height.micrometres();
    let mut centre_z = [0.0; 2];
    let mut sides = Vec::new();
    for side in 0..2 {
        let g = geometry[side];
        let gap = g.gap.micrometres();
        if gap < min - SLACK_UM || gap > max + SLACK_UM {
            issues.push(SlideIssue::ClearanceOutOfRange {
                side,
                measured: g.gap,
            });
        }
        if setback + length > g.cabinet_depth.micrometres() + SLACK_UM {
            issues.push(SlideIssue::TooLongForDepth {
                side,
                available: Length::from_micrometres(g.cabinet_depth.micrometres() - setback),
            });
        }
        let start = setback - g.drawer_front_behind.micrometres();
        if start < -SLACK_UM
            || start + drawer_member(spec) > g.drawer_depth.micrometres() + SLACK_UM
        {
            issues.push(SlideIssue::LongerThanDrawer {
                side,
                available: Length::from_micrometres(g.drawer_depth.micrometres() - start.max(0)),
            });
        }
        if side == 0 || !issues.contains(&SlideIssue::HeightExceedsBoxSide) {
            let box_height = g.box_height.micrometres();
            if height - half < -SLACK_UM || height + half > box_height + SLACK_UM {
                issues.push(SlideIssue::HeightExceedsBoxSide);
            }
        }
        let m = g.mounting;
        let drawer = &drawers[side];
        let cabinet = &cabinets[side];
        // The slide centre line at the drawer member's front, in the world.
        let centre_local = side_point(
            drawer,
            m.drawer_front_edge,
            m.drawer_bottom_edge,
            m.drawer_face,
            start as f64 / 1000.0,
            height as f64 / 1000.0,
        );
        let centre = drawer.world(centre_local);
        centre_z[side] = centre[2];
        let cabinet_local = cabinet.local(centre);
        let cv = 1 - edge_axis(m.cabinet_front_edge);
        let up = dot(cabinet.axes[cv], [0.0, 0.0, 1.0]).signum();
        let from_bottom = if up >= 0.0 {
            cabinet_local[cv]
        } else {
            cabinet.size[cv] - cabinet_local[cv]
        };
        let half_mm = half as f64 / 1000.0;
        if from_bottom - half_mm < -0.001 || from_bottom + half_mm > cabinet.size[cv] + 0.001 {
            issues.push(SlideIssue::OutsideMount { side });
        }
        let cabinet_bottom = if up >= 0.0 {
            if cv == 0 {
                BoardEdge::MinX
            } else {
                BoardEdge::MinY
            }
        } else if cv == 0 {
            BoardEdge::MaxX
        } else {
            BoardEdge::MaxY
        };
        let to_um = |p: [f64; 3]| p.map(|v| (v * 1000.0).round() as i128);
        let cabinet_holes_um = spec
            .cabinet_holes
            .iter()
            .map(|h| {
                to_um(side_point(
                    cabinet,
                    m.cabinet_front_edge,
                    cabinet_bottom,
                    m.cabinet_face,
                    (setback + h.along.micrometres()) as f64 / 1000.0,
                    from_bottom + h.offset.micrometres() as f64 / 1000.0,
                ))
            })
            .collect();
        let drawer_holes_um = spec
            .drawer_holes
            .iter()
            .map(|h| {
                to_um(side_point(
                    drawer,
                    m.drawer_front_edge,
                    m.drawer_bottom_edge,
                    m.drawer_face,
                    (start + h.along.micrometres()) as f64 / 1000.0,
                    (height + h.offset.micrometres()) as f64 / 1000.0,
                ))
            })
            .collect();
        sides.push(SlideSideReferences {
            cabinet_board: installation.cabinet_sides[side],
            cabinet_face: m.cabinet_face,
            cabinet_front_edge: m.cabinet_front_edge,
            cabinet_holes_um,
            cabinet_hole_distances: hole_distances(&spec.cabinet_holes, setback),
            cabinet_centre_from_bottom: um(from_bottom),
            drawer_board: installation.drawer_sides[side],
            drawer_face: m.drawer_face,
            drawer_front_edge: m.drawer_front_edge,
            drawer_holes_um,
            drawer_hole_distances: hole_distances(&spec.drawer_holes, start),
            drawer_centre_from_bottom: installation.height,
            gap: g.gap,
        });
    }
    let front_offset = (geometry[0].drawer_front_behind.micrometres()
        - geometry[1].drawer_front_behind.micrometres())
    .abs();
    if (centre_z[0] - centre_z[1]).abs() > 0.5 || front_offset > 500 {
        issues.push(SlideIssue::Misaligned);
    }
    if spec.travel.micrometres() * 10 < spec.length.micrometres() * 9 {
        notices.push(SlideNotice::PartialExtension {
            travel: spec.travel,
        });
    }
    let [left, right]: [SlideSideReferences; 2] = match sides.try_into() {
        Ok(sides) => sides,
        Err(_) => return status(issues, notices, None),
    };
    let references = SlideReferences {
        product_id: entry.product_id.clone(),
        family: spec.family.clone(),
        length: spec.length,
        travel: spec.travel,
        member_height: spec.height,
        clearance: spec.clearance,
        clearance_minus: spec.clearance_minus,
        clearance_plus: spec.clearance_plus,
        setback: installation.setback,
        sides: [left, right],
        source: if entry.revision.is_empty() {
            entry.source.clone()
        } else {
            format!("{}; {}", entry.source, entry.revision)
        },
        attribution: spec.attribution.clone(),
        printed_page: spec.printed_page,
        pdf_page: spec.pdf_page,
        trust: hardware_catalog::trust(entry).unwrap_or(hardware_catalog::Trust::UserSupplied),
        pack: entry
            .origin
            .as_ref()
            .map(|o| format!("{} {}", o.manufacturer, o.pack_version)),
        rear_fixing: spec.rear_fixing.clone(),
    };
    status(issues, notices, Some(references))
}

pub fn diagnose_all(project: &Project) -> Vec<SlideStatus> {
    project
        .slide_installations
        .iter()
        .map(|i| diagnose(project, i))
        .collect()
}

/// The slide installation a drawer (assembly, or any board in it) runs on.
pub fn for_drawer(project: &Project, id: Uuid) -> Option<&SlideInstallation> {
    project.slide_installations.iter().find(|s| {
        s.id == id
            || s.drawer_root_id == id
            || moving_members(project, s.drawer_root_id).contains(&id)
    })
}

/// Structural invariants, checked on load and on every editor transaction.
pub(crate) fn validate_installations(project: &Project) -> Result<(), DomainError> {
    let mut roots = HashSet::new();
    for s in &project.slide_installations {
        let invalid = || DomainError::InvalidSlide(s.id);
        if !project
            .catalog
            .iter()
            .any(|c| c.id == s.catalog_id && c.slide().is_some())
        {
            return Err(invalid());
        }
        let boards: HashSet<_> = s.drawer_sides.iter().chain(&s.cabinet_sides).collect();
        if boards.len() != 4 || boards.iter().any(|id| project.board(**id).is_none()) {
            return Err(invalid());
        }
        if project.board(s.drawer_root_id).is_none()
            && !project.assemblies.iter().any(|a| a.id == s.drawer_root_id)
        {
            return Err(invalid());
        }
        let members: HashSet<_> = moving_members(project, s.drawer_root_id)
            .into_iter()
            .collect();
        if !s.drawer_sides.iter().all(|id| members.contains(id))
            || s.cabinet_sides.iter().any(|id| members.contains(id))
            || s.height.micrometres() < 0
            || s.setback.micrometres() < 0
            || !roots.insert(s.drawer_root_id)
        {
            return Err(invalid());
        }
    }
    Ok(())
}

fn check_inputs(s: &SlideInstallation) -> Result<(), SlideEditError> {
    if s.height.micrometres() < 0 || s.setback.micrometres() < 0 {
        Err(SlideEditError::InvalidDistance)
    } else {
        Ok(())
    }
}

/// Disposable preview of an installation not yet in the project.
pub fn preview(
    project: &Project,
    proposed: &SlideInstallation,
) -> Result<SlideStatus, SlideEditError> {
    check_inputs(proposed)?;
    Ok(diagnose(project, proposed))
}

pub fn create(
    editor: &mut ProjectEditor,
    installation: SlideInstallation,
) -> Result<bool, EditError<SlideEditError>> {
    check_inputs(&installation).map_err(EditError::Command)?;
    editor.transact(|p| {
        if p.slide_installations
            .iter()
            .any(|s| s.drawer_root_id == installation.drawer_root_id)
        {
            return Err(SlideEditError::DrawerHasSlides);
        }
        let entry = p
            .catalog
            .iter()
            .find(|c| c.id == installation.catalog_id)
            .ok_or(SlideEditError::MissingCatalog)?;
        if entry.slide().is_none() {
            return Err(SlideEditError::NotASlide);
        }
        p.slide_installations.push(installation);
        Ok(())
    })
}

/// Pin `catalog` (unless an entry with the same id exists) and install the
/// slides in one undoable step.
pub fn create_with_catalog(
    editor: &mut ProjectEditor,
    catalog: CatalogReference,
    installation: SlideInstallation,
) -> Result<bool, EditError<SlideEditError>> {
    check_inputs(&installation).map_err(EditError::Command)?;
    editor.transact(|p| {
        if catalog.slide().is_none() {
            return Err(SlideEditError::NotASlide);
        }
        if p.slide_installations
            .iter()
            .any(|s| s.drawer_root_id == installation.drawer_root_id)
        {
            return Err(SlideEditError::DrawerHasSlides);
        }
        if !p.catalog.iter().any(|c| c.id == catalog.id) {
            p.catalog.push(catalog);
        }
        p.slide_installations.push(installation);
        Ok(())
    })
}

pub fn update(
    editor: &mut ProjectEditor,
    installation: SlideInstallation,
) -> Result<bool, EditError<SlideEditError>> {
    check_inputs(&installation).map_err(EditError::Command)?;
    editor.transact(|p| {
        if !p
            .catalog
            .iter()
            .any(|c| c.id == installation.catalog_id && c.slide().is_some())
        {
            return Err(SlideEditError::NotASlide);
        }
        let slot = p
            .slide_installations
            .iter_mut()
            .find(|s| s.id == installation.id)
            .ok_or(SlideEditError::MissingInstallation)?;
        *slot = installation;
        Ok(())
    })
}

/// Refit edges and faces after the boards moved.
pub fn refitted(
    project: &Project,
    installation: &SlideInstallation,
) -> Result<SlideInstallation, SlideFitError> {
    let detected = measure(
        project,
        installation.drawer_root_id,
        installation.drawer_sides,
        installation.cabinet_sides,
    )?;
    Ok(SlideInstallation {
        drawer_sides: detected.drawer_sides,
        cabinet_sides: detected.cabinet_sides,
        sides: detected.sides(),
        ..installation.clone()
    })
}

pub fn remove(editor: &mut ProjectEditor, id: Uuid) -> Result<bool, EditError<SlideEditError>> {
    editor.transact(|p| {
        let index = p
            .slide_installations
            .iter()
            .position(|s| s.id == id)
            .ok_or(SlideEditError::MissingInstallation)?;
        p.slide_installations.remove(index);
        Ok(())
    })
}

/// World direction the drawer opens in, from the fitted left carcass side.
pub fn pull(project: &Project, installation: &SlideInstallation) -> Option<[f64; 3]> {
    let frame = BoardFrame::new(project, installation.cabinet_sides[0])?;
    Some(pull_from(&frame, installation.sides[0].cabinet_front_edge))
}

/// How far the drawer may open: the slide's travel.
pub fn max_extension(project: &Project, installation: &SlideInstallation) -> Option<Length> {
    project
        .catalog
        .iter()
        .find(|c| c.id == installation.catalog_id)
        .and_then(CatalogReference::slide)
        .map(|s| s.travel)
}

/// A checked, display-only opening: every member of the drawer translated
/// `extension_mm` along the pull direction. Zero returns the stored poses.
pub fn derived_poses(
    project: &Project,
    installation: &SlideInstallation,
    extension_mm: f64,
) -> Result<Vec<(Uuid, Pose)>, SlideEditError> {
    let travel = max_extension(project, installation).ok_or(SlideEditError::MissingCatalog)?;
    if !extension_mm.is_finite()
        || extension_mm < 0.0
        || extension_mm > travel.micrometres() as f64 / 1000.0 + 1e-9
    {
        return Err(SlideEditError::InvalidExtension);
    }
    let pull =
        pull(project, installation).ok_or(SlideEditError::Fit(SlideFitError::MissingPart))?;
    moving_members(project, installation.drawer_root_id)
        .into_iter()
        .map(|id| {
            let closed = world_pose(project, id)
                .map_err(|_| SlideEditError::Fit(SlideFitError::MissingPart))?;
            if extension_mm == 0.0 {
                return Ok((id, closed));
            }
            let moved = std::array::from_fn(|i| closed.translation_mm[i] + pull[i] * extension_mm);
            Pose::new(moved, closed.rotation)
                .map(|pose| (id, pose))
                .map_err(|_| SlideEditError::InvalidExtension)
        })
        .collect()
}

/// How far a pose override has pulled the drawer out, clamped to the travel.
pub fn extension_in(
    project: &Project,
    installation: &SlideInstallation,
    poses: Option<&std::collections::HashMap<Uuid, Pose>>,
) -> f64 {
    let Some(posed) = poses.and_then(|p| p.get(&installation.drawer_root_id)) else {
        return 0.0;
    };
    let (Ok(closed), Some(pull), Some(travel)) = (
        world_pose(project, installation.drawer_root_id),
        pull(project, installation),
        max_extension(project, installation),
    ) else {
        return 0.0;
    };
    let moved = dot(sub(posed.translation_mm, closed.translation_mm), pull);
    moved.clamp(0.0, travel.micrometres() as f64 / 1000.0)
}

/// The slide members as world boxes, per side: outer (cabinet), intermediate
/// and inner (drawer) member, each `(pose, size)` with local X running back
/// into the carcass, Y up and Z across. Their thicknesses add up to the
/// nominal clearance. `extension_mm` slides the inner member out by that much
/// and the intermediate one by half.
pub fn member_boxes(
    project: &Project,
    installation: &SlideInstallation,
    extension_mm: f64,
) -> Option<[[MemberBox; 3]; 2]> {
    let boxes = boxes(
        project,
        installation,
        extension_mm,
        &[
            (0.40, 1.0, 0.0, 0.0),
            (0.30, 0.80, 5.0, 0.5),
            (
                0.30,
                0.62,
                DRAWER_MEMBER_SHORTER.micrometres() as f64 / 1000.0,
                1.0,
            ),
        ],
    )?;
    let side = |s: usize| [boxes[s][0], boxes[s][1], boxes[s][2]];
    Some([side(0), side(1)])
}

/// The closed slide on each side as one box filling the clearance: what the
/// slide occupies, for scene checks.
pub fn envelopes(project: &Project, installation: &SlideInstallation) -> Option<[MemberBox; 2]> {
    let boxes = boxes(project, installation, 0.0, &[(1.0, 1.0, 0.0, 0.0)])?;
    Some([boxes[0][0], boxes[1][0]])
}

/// Member boxes per side for `members`: (share of the clearance, share of
/// the height, shorter by mm, share of the extension it moves).
fn boxes(
    project: &Project,
    installation: &SlideInstallation,
    extension_mm: f64,
    members: &[(f64, f64, f64, f64)],
) -> Option<[Vec<MemberBox>; 2]> {
    let spec = project
        .catalog
        .iter()
        .find(|c| c.id == installation.catalog_id)?
        .slide()?;
    let pull = pull(project, installation)?;
    let clearance = spec.clearance.micrometres() as f64 / 1000.0;
    let length = spec.length.micrometres() as f64 / 1000.0;
    let height = spec.height.micrometres() as f64 / 1000.0;
    let back = pull.map(|v| -v);
    let up = [0.0, 0.0, 1.0];
    let cross = [
        back[1] * up[2] - back[2] * up[1],
        back[2] * up[0] - back[0] * up[2],
        back[0] * up[1] - back[1] * up[0],
    ];
    let rotation = rotation_from_axes(back, up, cross)?;
    let mut out = [Vec::new(), Vec::new()];
    for (side, boxes) in out.iter_mut().enumerate() {
        let cabinet = BoardFrame::new(project, installation.cabinet_sides[side])?;
        let drawer = BoardFrame::new(project, installation.drawer_sides[side])?;
        let m = installation.sides[side];
        let behind = dot(
            sub(
                cabinet.edge_mid(m.cabinet_front_edge),
                drawer.edge_mid(m.drawer_front_edge),
            ),
            pull,
        );
        let start = installation.setback.micrometres() as f64 / 1000.0 - behind;
        // The slide's front centre on the box side's face...
        let on_drawer = drawer.world(side_point(
            &drawer,
            m.drawer_front_edge,
            m.drawer_bottom_edge,
            m.drawer_face,
            start,
            installation.height.micrometres() as f64 / 1000.0,
        ));
        // ...moved across the gap onto the carcass side's face.
        let outward = {
            let n = drawer.axes[2];
            if m.drawer_face == BoardFace::MaxZ {
                n
            } else {
                n.map(|v| -v)
            }
        };
        let cabinet_face = cabinet.world([0.0, 0.0, face_z_mm(&cabinet, m.cabinet_face)]);
        let gap = dot(sub(cabinet_face, on_drawer), outward);
        let front: [f64; 3] = std::array::from_fn(|k| on_drawer[k] + outward[k] * gap);
        let inward = outward.map(|v| -v);
        // Local Z is `cross`; boxes grow along it from their origin.
        let flipped = dot(cross, inward) < 0.0;
        let mut offset = 0.0;
        for &(thick, tall, short, moves) in members {
            let t = clearance * thick;
            let h = height * tall;
            let across = if flipped { offset + t } else { offset };
            let origin: [f64; 3] = std::array::from_fn(|k| {
                front[k] + pull[k] * moves * extension_mm - up[k] * h / 2.0 + inward[k] * across
            });
            boxes.push((Pose::new(origin, rotation).ok()?, [length - short, h, t]));
            offset += t;
        }
    }
    Some(out)
}

/// The rotation whose local X, Y, Z are these orthonormal, right-handed axes.
fn rotation_from_axes(x: [f64; 3], y: [f64; 3], z: [f64; 3]) -> Option<crate::units::Quaternion> {
    let (m00, m01, m02) = (x[0], y[0], z[0]);
    let (m10, m11, m12) = (x[1], y[1], z[1]);
    let (m20, m21, m22) = (x[2], y[2], z[2]);
    let trace = m00 + m11 + m22;
    let (w, qx, qy, qz) = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        (0.25 * s, (m21 - m12) / s, (m02 - m20) / s, (m10 - m01) / s)
    } else if m00 > m11 && m00 > m22 {
        let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        ((m21 - m12) / s, 0.25 * s, (m01 + m10) / s, (m02 + m20) / s)
    } else if m11 > m22 {
        let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        ((m02 - m20) / s, (m01 + m10) / s, 0.25 * s, (m12 + m21) / s)
    } else {
        let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        ((m10 - m01) / s, (m02 + m20) / s, (m12 + m21) / s, 0.25 * s)
    };
    crate::units::Quaternion::normalized(w, qx, qy, qz).ok()
}
