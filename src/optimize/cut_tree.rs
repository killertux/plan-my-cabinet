//! Headless rectangular guillotine geometry. Coordinates and extents are exact
//! manufacturing lengths; kerf is a full-span band inside its input piece.
use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::domain::{BoardGrain, Project, StockGrain};
use crate::units::Length;

pub type PieceId = usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

/// The low or high edge along the cut's axis, relative to the input piece.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Low,
    High,
}

impl Axis {
    fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rectangle {
    pub origin: [Length; 2],
    pub extent: [Length; 2],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CutKind {
    Offcut,
    Waste,
    Part(Uuid),
    Split {
        axis: Axis,
        kerf: Length,
        first: PieceId,
        second: PieceId,
        retained: PieceId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CutNode {
    pub rectangle: Rectangle,
    pub kind: CutKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CutError {
    InvalidDimension,
    InvalidKerf,
    InvalidTrim,
    UnknownPiece,
    NotAvailable,
    InsufficientExtent,
    /// A desired edge loss is positive but smaller than a full, in-stock kerf.
    SubKerfEdge,
    PartSizeMismatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitResult {
    pub first: PieceId,
    pub second: PieceId,
}

/// One full-span blade pass. `retained_extent` is measured from
/// `reference_edge` of `input` to the finished edge of `retained_output`;
/// the kerf lies on `kerf_side` of that finished edge. Both output IDs are
/// stable tree node IDs (including a zero-width waste output after shaving).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CutOperation {
    pub number: usize,
    pub input: PieceId,
    pub axis: Axis,
    pub reference_edge: Edge,
    pub retained_extent: Length,
    pub kerf_side: Edge,
    pub retained_output: PieceId,
    pub outputs: SplitResult,
}

/// Node IDs remain stable across splits; only available, positive offcuts may
/// be split or promoted to finished parts. Zero-area outputs are waste leaves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CutTree {
    nodes: Vec<CutNode>,
    kerf: Length,
    usable: PieceId,
}

impl CutTree {
    pub fn new(extent: [Length; 2], kerf: Length) -> Result<Self, CutError> {
        if extent.iter().any(|v| v.micrometres() <= 0) {
            return Err(CutError::InvalidDimension);
        }
        if kerf.micrometres() <= 0 {
            return Err(CutError::InvalidKerf);
        }
        Ok(Self {
            nodes: vec![CutNode {
                rectangle: Rectangle {
                    origin: [Length::ZERO; 2],
                    extent,
                },
                kind: CutKind::Offcut,
            }],
            kerf,
            usable: 0,
        })
    }

    pub fn root(&self) -> PieceId {
        0
    }

    pub fn usable(&self) -> PieceId {
        self.usable
    }

    pub fn kerf(&self) -> Length {
        self.kerf
    }

    pub fn node(&self, id: PieceId) -> Option<&CutNode> {
        self.nodes.get(id)
    }

    pub fn nodes(&self) -> &[CutNode] {
        &self.nodes
    }

    pub fn cut_count(&self) -> usize {
        self.nodes
            .iter()
            .filter(|node| matches!(node.kind, CutKind::Split { .. }))
            .count()
    }

    /// Preorder traversal: an operation can only consume the root stock or
    /// an output introduced by an earlier operation. IDs do not depend on
    /// traversal order or subsequent changes to leaf classification.
    pub fn operations(&self) -> Vec<CutOperation> {
        let mut operations = Vec::with_capacity(self.cut_count());
        self.collect_operations(self.root(), &mut operations);
        operations
    }

    fn collect_operations(&self, input: PieceId, operations: &mut Vec<CutOperation>) {
        let node = &self.nodes[input];
        let CutKind::Split {
            axis,
            first,
            second,
            retained,
            ..
        } = node.kind
        else {
            return;
        };
        let retain_first = retained == first;
        operations.push(CutOperation {
            number: operations.len() + 1,
            input,
            axis,
            reference_edge: if retain_first { Edge::Low } else { Edge::High },
            retained_extent: self.nodes[retained].rectangle.extent[axis.index()],
            kerf_side: if retain_first { Edge::High } else { Edge::Low },
            retained_output: retained,
            outputs: SplitResult { first, second },
        });
        self.collect_operations(first, operations);
        self.collect_operations(second, operations);
    }

    /// `first_extent` is measured from the current piece's low-coordinate edge.
    /// The second output is the exact remainder after the full kerf. Either
    /// output may have zero extent only as a terminal waste edge shaving.
    pub fn split(
        &mut self,
        piece: PieceId,
        axis: Axis,
        first_extent: Length,
    ) -> Result<SplitResult, CutError> {
        self.split_retaining(
            piece,
            axis,
            first_extent,
            if first_extent == Length::ZERO {
                Edge::High
            } else {
                Edge::Low
            },
        )
    }

    /// Choose which output's finished boundary is dimensioned. `Low` retains
    /// the first output, `High` the second; the split geometry is unchanged.
    pub fn split_retaining(
        &mut self,
        piece: PieceId,
        axis: Axis,
        first_extent: Length,
        retained_edge: Edge,
    ) -> Result<SplitResult, CutError> {
        let input = self.nodes.get(piece).ok_or(CutError::UnknownPiece)?;
        if input.kind != CutKind::Offcut {
            return Err(CutError::NotAvailable);
        }
        let i = axis.index();
        let width = i128::from(input.rectangle.extent[i].micrometres());
        let first = i128::from(first_extent.micrometres());
        let kerf = i128::from(self.kerf.micrometres());
        let second = width - first - kerf;
        if first >= 0 && first < width && second < 0 {
            return Err(CutError::SubKerfEdge);
        }
        if first < 0 || second < 0 || first + second == 0 {
            return Err(CutError::InsufficientExtent);
        }
        let mut first_rect = input.rectangle;
        first_rect.extent[i] = first_extent;
        let mut second_rect = input.rectangle;
        second_rect.origin[i] = Length::from_micrometres(
            (i128::from(second_rect.origin[i].micrometres()) + first + kerf) as i64,
        );
        second_rect.extent[i] = Length::from_micrometres(second as i64);
        let result = SplitResult {
            first: self.nodes.len(),
            second: self.nodes.len() + 1,
        };
        if (retained_edge == Edge::Low && first == 0)
            || (retained_edge == Edge::High && second == 0)
        {
            return Err(CutError::InvalidDimension);
        }
        self.nodes[piece].kind = CutKind::Split {
            axis,
            kerf: self.kerf,
            first: result.first,
            second: result.second,
            retained: if retained_edge == Edge::Low {
                result.first
            } else {
                result.second
            },
        };
        for rect in [first_rect, second_rect] {
            self.nodes.push(CutNode {
                kind: if rect.extent[i] == Length::ZERO {
                    CutKind::Waste
                } else {
                    CutKind::Offcut
                },
                rectangle: rect,
            });
        }
        Ok(result)
    }

    /// Only an exact rectangle can become a finished part; no perimeter cut is
    /// generated when its boundaries already coincide with the stock/offcut.
    pub fn finish_part(
        &mut self,
        piece: PieceId,
        part: Uuid,
        extent: [Length; 2],
    ) -> Result<(), CutError> {
        let node = self.nodes.get_mut(piece).ok_or(CutError::UnknownPiece)?;
        if node.kind != CutKind::Offcut {
            return Err(CutError::NotAvailable);
        }
        if node.rectangle.extent != extent {
            return Err(CutError::PartSizeMismatch);
        }
        node.kind = CutKind::Part(part);
        Ok(())
    }

    pub fn discard(&mut self, piece: PieceId) -> Result<(), CutError> {
        let node = self.nodes.get_mut(piece).ok_or(CutError::UnknownPiece)?;
        if node.kind != CutKind::Offcut {
            return Err(CutError::NotAvailable);
        }
        node.kind = CutKind::Waste;
        Ok(())
    }

    /// Trim losses are total edge allowances *including* the kerf. Cuts are
    /// fixed left, right, bottom, top; each later cut spans only the retained
    /// rectangle, so corners cannot be charged twice.
    pub fn with_trims(
        extent: [Length; 2],
        kerf: Length,
        trim: [Length; 4],
    ) -> Result<Self, CutError> {
        let mut tree = Self::new(extent, kerf)?;
        let k = i128::from(kerf.micrometres());
        let losses = trim.map(|v| i128::from(v.micrometres()));
        if losses.iter().any(|&v| v < 0) {
            return Err(CutError::InvalidTrim);
        }
        if losses.iter().any(|&v| v > 0 && v < k) {
            return Err(CutError::SubKerfEdge);
        }
        if losses[0] + losses[1] >= i128::from(extent[0].micrometres())
            || losses[2] + losses[3] >= i128::from(extent[1].micrometres())
        {
            return Err(CutError::InvalidTrim);
        }
        for (edge, &loss) in losses.iter().enumerate() {
            if loss == 0 {
                continue;
            }
            let axis = if edge < 2 { Axis::X } else { Axis::Y };
            let low_edge = edge == 0 || edge == 2;
            let current = tree.usable;
            let width =
                i128::from(tree.nodes[current].rectangle.extent[axis.index()].micrometres());
            let first = if low_edge { loss - k } else { width - loss };
            let split = tree.split_retaining(
                current,
                axis,
                Length::from_micrometres(first as i64),
                if low_edge { Edge::High } else { Edge::Low },
            )?;
            let (waste, retained) = if low_edge {
                (split.first, split.second)
            } else {
                (split.second, split.first)
            };
            if tree.nodes[waste].kind == CutKind::Offcut {
                tree.discard(waste)?;
            }
            tree.usable = retained;
        }
        Ok(tree)
    }
}

/// Disjoint areas in square micrometres. Trim loss includes both the blade
/// bands and the discarded edge strips; `kerf_loss` excludes trim passes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AreaAccounting {
    pub root_area: i128,
    pub part_area: i128,
    pub offcut_area: i128,
    pub waste_area: i128,
    pub kerf_loss: i128,
    pub trim_loss: i128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WitnessError {
    UnknownStock(Uuid),
    InvalidStock(Uuid),
    InvalidTree { node: PieceId, reason: &'static str },
    AreaMismatch(AreaAccounting),
    MissingAllocation(Uuid),
    DuplicatePart(Uuid),
    UnexpectedPart(Uuid),
    MissingPart(Uuid),
    UnknownBoard(Uuid),
    MaterialMismatch(Uuid),
    ThicknessMismatch(Uuid),
    GrainMismatch(Uuid),
    InvalidBoard(Uuid),
    PlacementMismatch(Uuid),
}

fn area(rect: Rectangle) -> i128 {
    i128::from(rect.extent[0].micrometres()) * i128::from(rect.extent[1].micrometres())
}

/// Independently verify the tree topology, full-span bands, and the exact
/// partition of its root. `trim` denotes edge loss *including* blade width.
pub fn account_tree(tree: &CutTree, trim: [Length; 4]) -> Result<AreaAccounting, WitnessError> {
    let invalid = |node, reason| WitnessError::InvalidTree { node, reason };
    let root = tree
        .nodes
        .first()
        .ok_or(invalid(0, "missing root"))?
        .rectangle;
    if root.origin != [Length::ZERO; 2]
        || root.extent.iter().any(|v| v.micrometres() <= 0)
        || tree.kerf.micrometres() <= 0
    {
        return Err(invalid(0, "invalid root or kerf"));
    }
    let mut result = AreaAccounting {
        root_area: area(root),
        ..AreaAccounting::default()
    };
    let mut visited = HashSet::new();
    let mut parts = HashSet::new();
    // Validate prescribed edge trims before treating any cuts as ordinary cuts.
    let mut current = 0;
    let mut trim_nodes = HashSet::new();
    let mut trim_waste = HashSet::new();
    let k = i128::from(tree.kerf.micrometres());
    for (edge, loss) in trim.into_iter().enumerate() {
        let loss = i128::from(loss.micrometres());
        if loss < 0 || (loss > 0 && loss < k) {
            return Err(invalid(current, "invalid trim allowance"));
        }
        if loss == 0 {
            continue;
        }
        let parent = tree
            .nodes
            .get(current)
            .ok_or(invalid(current, "missing trim input"))?;
        let axis = if edge < 2 { Axis::X } else { Axis::Y };
        let low = edge == 0 || edge == 2;
        let width = i128::from(parent.rectangle.extent[axis.index()].micrometres());
        if loss >= width {
            return Err(invalid(current, "trim consumes usable stock"));
        }
        let CutKind::Split {
            axis: actual,
            kerf,
            first,
            second,
            retained,
        } = parent.kind
        else {
            return Err(invalid(current, "missing trim split"));
        };
        let (waste, next) = if low {
            (first, second)
        } else {
            (second, first)
        };
        if actual != axis || kerf != tree.kerf || retained != next {
            return Err(invalid(
                current,
                "trim axis, kerf or retained child mismatch",
            ));
        }
        let expected_waste = loss - k;
        let waste_node = tree
            .nodes
            .get(waste)
            .ok_or(invalid(current, "missing trim waste"))?;
        if i128::from(waste_node.rectangle.extent[axis.index()].micrometres()) != expected_waste
            || !matches!(waste_node.kind, CutKind::Waste)
        {
            return Err(invalid(waste, "trim strip must be waste of exact size"));
        }
        trim_nodes.insert(current);
        trim_waste.insert(waste);
        current = next;
    }
    if tree.usable != current {
        return Err(invalid(current, "usable node does not follow trims"));
    }
    let mut stack = vec![0];
    while let Some(id) = stack.pop() {
        if !visited.insert(id) {
            return Err(invalid(id, "shared child or cycle"));
        }
        let node = tree.nodes.get(id).ok_or(invalid(id, "missing child"))?;
        let r = node.rectangle;
        if r.extent.iter().any(|v| v.micrometres() < 0) {
            return Err(invalid(id, "negative rectangle extent"));
        }
        match node.kind {
            CutKind::Part(part) => {
                if r.extent.iter().any(|v| v.micrometres() <= 0) {
                    return Err(invalid(id, "zero-area part"));
                }
                if !parts.insert(part) {
                    return Err(WitnessError::DuplicatePart(part));
                }
                result.part_area += area(r);
            }
            CutKind::Offcut => {
                if r.extent.iter().any(|v| v.micrometres() <= 0) {
                    return Err(invalid(id, "zero-area offcut"));
                }
                result.offcut_area += area(r);
            }
            CutKind::Waste => {
                if r.extent.iter().filter(|v| v.micrometres() == 0).count() > 1 {
                    return Err(invalid(id, "degenerate waste"));
                }
                if trim_waste.contains(&id) {
                    result.trim_loss += area(r);
                } else {
                    result.waste_area += area(r);
                }
            }
            CutKind::Split {
                axis,
                kerf,
                first,
                second,
                retained,
            } => {
                if r.extent.iter().any(|v| v.micrometres() <= 0) {
                    return Err(invalid(id, "cannot split a zero-area piece"));
                }
                let a = axis.index();
                let c = tree
                    .nodes
                    .get(first)
                    .ok_or(invalid(id, "missing first output"))?
                    .rectangle;
                let d = tree
                    .nodes
                    .get(second)
                    .ok_or(invalid(id, "missing second output"))?
                    .rectangle;
                let blade = i128::from(kerf.micrometres());
                let coord = |v: Length| i128::from(v.micrometres());
                if first == second
                    || first == id
                    || second == id
                    || (retained != first && retained != second)
                    || blade <= 0
                    || kerf != tree.kerf
                    || coord(c.extent[a]) + blade + coord(d.extent[a]) != coord(r.extent[a])
                    || coord(c.origin[a]) != coord(r.origin[a])
                    || coord(d.origin[a]) != coord(r.origin[a]) + coord(c.extent[a]) + blade
                    || [c, d].iter().any(|child| {
                        child.origin[1 - a] != r.origin[1 - a]
                            || child.extent[1 - a] != r.extent[1 - a]
                    })
                    || [c, d].iter().any(|child| child.extent[a].micrometres() < 0)
                    || [first, second].iter().any(|&child| {
                        tree.nodes[child].rectangle.extent[a] == Length::ZERO
                            && !matches!(tree.nodes[child].kind, CutKind::Waste)
                    })
                    || tree.nodes[retained].rectangle.extent[a].micrometres() <= 0
                {
                    return Err(invalid(
                        id,
                        "split outputs overlap, escape input, or violate kerf",
                    ));
                }
                let band = blade * coord(r.extent[1 - a]);
                if trim_nodes.contains(&id) {
                    result.trim_loss += band;
                } else {
                    result.kerf_loss += band;
                }
                stack.extend([first, second]);
            }
        }
    }
    if visited.len() != tree.nodes.len() {
        return Err(invalid(0, "unreachable nodes"));
    }
    if result.part_area
        + result.offcut_area
        + result.waste_area
        + result.kerf_loss
        + result.trim_loss
        != result.root_area
    {
        return Err(WitnessError::AreaMismatch(result));
    }
    Ok(result)
}

/// Certify one stock's placed subset. Other unallocated boards remain drafts;
/// this does not declare the whole project ready for manufacture.
pub fn validate_witness(
    tree: &CutTree,
    project: &Project,
    stock_id: Uuid,
) -> Result<AreaAccounting, WitnessError> {
    let stock = project
        .stock
        .iter()
        .find(|s| s.id == stock_id)
        .ok_or(WitnessError::UnknownStock(stock_id))?;
    if stock.length.micrometres() <= 0
        || stock.width.micrometres() <= 0
        || stock.thickness.micrometres() <= 0
        || tree.nodes.first().map(|n| n.rectangle.extent) != Some([stock.length, stock.width])
    {
        return Err(WitnessError::InvalidStock(stock_id));
    }
    let accounting = account_tree(tree, stock.trim)?;
    let mut assignments = HashMap::new();
    for allocation in project
        .allocations
        .iter()
        .filter(|a| a.stock_id == stock_id)
    {
        if project
            .allocations
            .iter()
            .filter(|other| other.board_id == allocation.board_id)
            .count()
            > 1
        {
            return Err(WitnessError::DuplicatePart(allocation.board_id));
        }
        if assignments
            .insert(allocation.board_id, allocation)
            .is_some()
        {
            return Err(WitnessError::DuplicatePart(allocation.board_id));
        }
    }
    let mut seen = HashSet::new();
    for node in &tree.nodes {
        let CutKind::Part(id) = node.kind else {
            continue;
        };
        let allocation = assignments
            .get(&id)
            .ok_or(WitnessError::UnexpectedPart(id))?;
        let board = project
            .boards
            .iter()
            .find(|b| b.id == id)
            .ok_or(WitnessError::UnknownBoard(id))?;
        if board.material_id != stock.material_id {
            return Err(WitnessError::MaterialMismatch(id));
        }
        if board.thickness != stock.thickness {
            return Err(WitnessError::ThicknessMismatch(id));
        }
        if board.length.micrometres() <= 0
            || board.width.micrometres() <= 0
            || board.thickness.micrometres() <= 0
            || allocation.origin.iter().any(|v| v.micrometres() < 0)
        {
            return Err(WitnessError::InvalidBoard(id));
        }
        let material = project
            .materials
            .iter()
            .find(|m| m.id == board.material_id)
            .ok_or(WitnessError::MaterialMismatch(id))?;
        let grain = board.effective_grain(material);
        let axis = match grain {
            BoardGrain::Unrestricted => None,
            BoardGrain::Length => Some(if allocation.quarter_turn {
                StockGrain::AlongY
            } else {
                StockGrain::AlongX
            }),
            BoardGrain::Width => Some(if allocation.quarter_turn {
                StockGrain::AlongX
            } else {
                StockGrain::AlongY
            }),
        };
        if axis.is_some_and(|required| {
            stock.grain != StockGrain::Nondirectional && stock.grain != required
        }) {
            return Err(WitnessError::GrainMismatch(id));
        }
        let extent = if allocation.quarter_turn {
            [board.width, board.length]
        } else {
            [board.length, board.width]
        };
        if node.rectangle.origin != allocation.origin || node.rectangle.extent != extent {
            return Err(WitnessError::PlacementMismatch(id));
        }
        seen.insert(id);
    }
    for id in assignments.keys() {
        if !seen.contains(id) {
            return Err(WitnessError::MissingPart(*id));
        }
    }
    Ok(accounting)
}

/// A budget counts visited search states, including states reached by failed
/// branches. Only fully explored states may establish `NoSlicing`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reconstruction {
    Verified {
        tree: CutTree,
        accounting: AreaAccounting,
    },
    RuleViolation(ReconstructionViolation),
    BudgetExhausted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReconstructionViolation {
    Cut(CutError),
    Witness(WitnessError),
    NoSlicing,
}

#[derive(Clone, Copy)]
struct PlacedPart {
    id: Uuid,
    rect: Rectangle,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct SearchState {
    bounds: [i64; 4],
    parts: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SearchResult {
    Found,
    Impossible,
    Exhausted,
    Cancelled,
}

fn ends(rect: Rectangle, axis: usize) -> (i128, i128) {
    let low = i128::from(rect.origin[axis].micrometres());
    (low, low + i128::from(rect.extent[axis].micrometres()))
}

#[allow(clippy::too_many_arguments)] // Recursive state plus a cooperative cancellation predicate.
fn reconstruct_region(
    tree: &mut CutTree,
    piece: PieceId,
    indices: &[usize],
    parts: &[PlacedPart],
    remaining: &mut usize,
    failed: &mut HashSet<SearchState>,
    axis_order: [Axis; 2],
    cancelled: &impl Fn() -> bool,
) -> SearchResult {
    if cancelled() {
        return SearchResult::Cancelled;
    }
    let rect = tree.nodes[piece].rectangle;
    let state = SearchState {
        bounds: [
            rect.origin[0].micrometres(),
            rect.origin[1].micrometres(),
            rect.extent[0].micrometres(),
            rect.extent[1].micrometres(),
        ],
        parts: indices.to_vec(),
    };
    if failed.contains(&state) {
        return SearchResult::Impossible;
    }
    if *remaining == 0 {
        return SearchResult::Exhausted;
    }
    *remaining -= 1;
    if indices.is_empty() {
        return SearchResult::Found;
    }
    if indices.len() == 1 && parts[indices[0]].rect == rect {
        let part = parts[indices[0]];
        tree.finish_part(piece, part.id, part.rect.extent)
            .expect("exact offcut");
        return SearchResult::Found;
    }

    let kerf = i128::from(tree.kerf.micrometres());
    let mut exhausted = false;
    for axis in axis_order {
        let a = axis.index();
        let (low, high) = ends(rect, a);
        let mut candidates = vec![low, high - kerf];
        for &index in indices {
            let (start, end) = ends(parts[index].rect, a);
            candidates.extend([start - kerf, end]);
        }
        candidates.sort_unstable();
        candidates.dedup();
        for start in candidates {
            if cancelled() {
                return SearchResult::Cancelled;
            }
            if start < low || start + kerf > high || start + kerf == high && start == low {
                continue;
            }
            let mut first_parts = Vec::new();
            let mut second_parts = Vec::new();
            let mut crosses = false;
            for &index in indices {
                let (part_low, part_high) = ends(parts[index].rect, a);
                if part_high <= start {
                    first_parts.push(index);
                } else if part_low >= start + kerf {
                    second_parts.push(index);
                } else {
                    crosses = true;
                    break;
                }
            }
            if crosses || (first_parts.is_empty() && second_parts.is_empty()) {
                continue;
            }
            // The state is copied only for a valid full-span strip. Roll it
            // back if either child fails, so alternatives never share cuts.
            let mut attempt = tree.clone();
            let retained = if start == low { Edge::High } else { Edge::Low };
            let Ok(split) = attempt.split_retaining(
                piece,
                axis,
                Length::from_micrometres((start - low) as i64),
                retained,
            ) else {
                continue;
            };
            let left = reconstruct_region(
                &mut attempt,
                split.first,
                &first_parts,
                parts,
                remaining,
                failed,
                axis_order,
                cancelled,
            );
            if left == SearchResult::Cancelled {
                return SearchResult::Cancelled;
            }
            if left == SearchResult::Impossible {
                continue;
            }
            let right = reconstruct_region(
                &mut attempt,
                split.second,
                &second_parts,
                parts,
                remaining,
                failed,
                axis_order,
                cancelled,
            );
            if right == SearchResult::Cancelled {
                return SearchResult::Cancelled;
            }
            if left == SearchResult::Found && right == SearchResult::Found {
                *tree = attempt;
                return SearchResult::Found;
            }
            exhausted |= left == SearchResult::Exhausted || right == SearchResult::Exhausted;
        }
    }
    if exhausted {
        SearchResult::Exhausted
    } else {
        failed.insert(state);
        SearchResult::Impossible
    }
}

/// Rebuild a cut witness for the fixed allocations on one physical stock.
/// Does not move or change any project allocation. An empty stock is valid.
pub fn reconstruct_witness(
    project: &Project,
    stock_id: Uuid,
    kerf: Length,
    budget: usize,
) -> Reconstruction {
    reconstruct_witness_ordered(project, stock_id, kerf, budget, [Axis::X, Axis::Y])
}

/// Choose the first split direction without changing the witness rules.
/// Both orders still explore both axes and certify the resulting tree.
pub(crate) fn reconstruct_witness_ordered(
    project: &Project,
    stock_id: Uuid,
    kerf: Length,
    budget: usize,
    axis_order: [Axis; 2],
) -> Reconstruction {
    reconstruct_witness_cancellable(project, stock_id, kerf, budget, axis_order, &|| false)
        .expect("synchronous witness cannot be cancelled")
}

pub(crate) fn reconstruct_witness_cancellable(
    project: &Project,
    stock_id: Uuid,
    kerf: Length,
    budget: usize,
    axis_order: [Axis; 2],
    cancelled: &impl Fn() -> bool,
) -> Result<Reconstruction, ()> {
    let result = reconstruct_witness_inner(project, stock_id, kerf, budget, axis_order, cancelled);
    if cancelled() { Err(()) } else { Ok(result) }
}

fn reconstruct_witness_inner(
    project: &Project,
    stock_id: Uuid,
    kerf: Length,
    budget: usize,
    axis_order: [Axis; 2],
    cancelled: &impl Fn() -> bool,
) -> Reconstruction {
    if cancelled() {
        return Reconstruction::BudgetExhausted;
    }
    use ReconstructionViolation::{Cut, Witness};
    let Some(stock) = project.stock.iter().find(|s| s.id == stock_id) else {
        return Reconstruction::RuleViolation(Witness(WitnessError::UnknownStock(stock_id)));
    };
    let mut tree = match CutTree::with_trims([stock.length, stock.width], kerf, stock.trim) {
        Ok(tree) => tree,
        Err(error) => return Reconstruction::RuleViolation(Cut(error)),
    };
    if stock.thickness.micrometres() <= 0 {
        return Reconstruction::RuleViolation(Witness(WitnessError::InvalidStock(stock_id)));
    }
    let usable = tree.nodes[tree.usable].rectangle;
    let mut parts = Vec::new();
    let mut seen = HashSet::new();
    for allocation in project
        .allocations
        .iter()
        .filter(|a| a.stock_id == stock_id)
    {
        if cancelled() {
            return Reconstruction::BudgetExhausted;
        }
        let id = allocation.board_id;
        if !seen.insert(id)
            || project
                .allocations
                .iter()
                .filter(|a| a.board_id == id)
                .count()
                > 1
        {
            return Reconstruction::RuleViolation(Witness(WitnessError::DuplicatePart(id)));
        }
        let Some(board) = project.boards.iter().find(|b| b.id == id) else {
            return Reconstruction::RuleViolation(Witness(WitnessError::UnknownBoard(id)));
        };
        if board.material_id != stock.material_id {
            return Reconstruction::RuleViolation(Witness(WitnessError::MaterialMismatch(id)));
        }
        if board.thickness != stock.thickness {
            return Reconstruction::RuleViolation(Witness(WitnessError::ThicknessMismatch(id)));
        }
        if board.length.micrometres() <= 0
            || board.width.micrometres() <= 0
            || board.thickness.micrometres() <= 0
            || allocation.origin.iter().any(|v| v.micrometres() < 0)
        {
            return Reconstruction::RuleViolation(Witness(WitnessError::InvalidBoard(id)));
        }
        let Some(material) = project.materials.iter().find(|m| m.id == board.material_id) else {
            return Reconstruction::RuleViolation(Witness(WitnessError::MaterialMismatch(id)));
        };
        let required = match board.effective_grain(material) {
            BoardGrain::Unrestricted => None,
            BoardGrain::Length => Some(if allocation.quarter_turn {
                StockGrain::AlongY
            } else {
                StockGrain::AlongX
            }),
            BoardGrain::Width => Some(if allocation.quarter_turn {
                StockGrain::AlongX
            } else {
                StockGrain::AlongY
            }),
        };
        if required
            .is_some_and(|grain| stock.grain != StockGrain::Nondirectional && stock.grain != grain)
        {
            return Reconstruction::RuleViolation(Witness(WitnessError::GrainMismatch(id)));
        }
        let rect = Rectangle {
            origin: allocation.origin,
            extent: if allocation.quarter_turn {
                [board.width, board.length]
            } else {
                [board.length, board.width]
            },
        };
        if (0..2).any(|axis| {
            ends(rect, axis).0 < ends(usable, axis).0 || ends(rect, axis).1 > ends(usable, axis).1
        }) {
            return Reconstruction::RuleViolation(Witness(WitnessError::PlacementMismatch(id)));
        }
        parts.push(PlacedPart { id, rect });
    }
    // Stable regardless of the order in which allocations were created.
    parts.sort_by_key(|p| p.id);
    for (i, a) in parts.iter().enumerate() {
        if cancelled() {
            return Reconstruction::BudgetExhausted;
        }
        for b in &parts[i + 1..] {
            if (0..2).all(|axis| {
                ends(a.rect, axis).0 < ends(b.rect, axis).1
                    && ends(b.rect, axis).0 < ends(a.rect, axis).1
            }) {
                return Reconstruction::RuleViolation(ReconstructionViolation::NoSlicing);
            }
        }
    }
    // A lone blank cannot acquire an exact edge if the edge allowance is
    // positive but smaller than the blade (even after perpendicular cuts).
    if parts.len() == 1 {
        for axis in 0..2 {
            let (part_low, part_high) = ends(parts[0].rect, axis);
            let (stock_low, stock_high) = ends(usable, axis);
            let k = i128::from(kerf.micrometres());
            if (part_low > stock_low && part_low - stock_low < k)
                || (part_high < stock_high && stock_high - part_high < k)
            {
                return Reconstruction::RuleViolation(Cut(CutError::SubKerfEdge));
            }
        }
    }
    let mut remaining = budget;
    let indices: Vec<_> = (0..parts.len()).collect();
    let mut failed = HashSet::new();
    let usable_piece = tree.usable;
    match reconstruct_region(
        &mut tree,
        usable_piece,
        &indices,
        &parts,
        &mut remaining,
        &mut failed,
        axis_order,
        cancelled,
    ) {
        SearchResult::Found if cancelled() => Reconstruction::BudgetExhausted,
        SearchResult::Found => match validate_witness(&tree, project, stock_id) {
            Ok(accounting) => Reconstruction::Verified { tree, accounting },
            Err(error) => Reconstruction::RuleViolation(Witness(error)),
        },
        SearchResult::Impossible => {
            Reconstruction::RuleViolation(ReconstructionViolation::NoSlicing)
        }
        SearchResult::Exhausted => Reconstruction::BudgetExhausted,
        SearchResult::Cancelled => Reconstruction::BudgetExhausted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Allocation, Board, Material, Stock, StockSource};
    use crate::money::Currency;
    use crate::units::{Pose, Quaternion};

    fn mm(value: i64) -> Length {
        Length::from_micrometres(value * 1_000)
    }

    #[test]
    fn bilingual_stock_guide_examples_match_cut_tree() {
        let no_trim = [Length::ZERO; 4];
        let mut two = CutTree::new([mm(205), mm(100)], mm(5)).unwrap();
        let outputs = two.split(two.root(), Axis::X, mm(100)).unwrap();
        for piece in [outputs.first, outputs.second] {
            two.finish_part(piece, Uuid::new_v4(), [mm(100), mm(100)])
                .unwrap();
        }
        assert_eq!(
            two.node(outputs.second).unwrap().rectangle.origin[0],
            mm(105)
        );
        assert_eq!(two.cut_count(), 1);
        let areas = account_tree(&two, no_trim).unwrap();
        assert_eq!(areas.root_area, 20_500_000_000);
        assert_eq!(areas.part_area, 20_000_000_000);
        assert_eq!(areas.kerf_loss, 500_000_000);
        assert_eq!(areas.offcut_area + areas.waste_area + areas.trim_loss, 0);

        let mut short = CutTree::new([mm(204), mm(100)], mm(5)).unwrap();
        let outputs = short.split(0, Axis::X, mm(100)).unwrap();
        assert_eq!(
            short.node(outputs.second).unwrap().rectangle.extent[0],
            mm(99)
        );
        assert_eq!(
            short.finish_part(outputs.second, Uuid::new_v4(), [mm(100), mm(100)]),
            Err(CutError::PartSizeMismatch)
        );

        let mut exact = CutTree::new([mm(100), mm(50)], mm(5)).unwrap();
        exact
            .finish_part(0, Uuid::new_v4(), [mm(100), mm(50)])
            .unwrap();
        assert_eq!(exact.cut_count(), 0);
        assert_eq!(account_tree(&exact, no_trim).unwrap().kerf_loss, 0);

        let mut shave = CutTree::new([mm(105), mm(50)], mm(5)).unwrap();
        let outputs = shave.split(0, Axis::X, mm(100)).unwrap();
        assert_eq!(shave.node(outputs.second).unwrap().kind, CutKind::Waste);
        assert_eq!(
            shave.node(outputs.second).unwrap().rectangle.extent[0],
            Length::ZERO
        );
        shave
            .finish_part(outputs.first, Uuid::new_v4(), [mm(100), mm(50)])
            .unwrap();
        assert_eq!(shave.cut_count(), 1);
        let areas = account_tree(&shave, no_trim).unwrap();
        assert_eq!(
            (areas.root_area, areas.part_area, areas.kerf_loss),
            (5_250_000_000, 5_000_000_000, 250_000_000)
        );
        let mut sub_kerf = CutTree::new([mm(103), mm(50)], mm(5)).unwrap();
        assert_eq!(
            sub_kerf.split(0, Axis::X, mm(100)),
            Err(CutError::SubKerfEdge)
        );

        let seven = CutTree::with_trims(
            [mm(100), mm(100)],
            mm(5),
            [mm(7), Length::ZERO, Length::ZERO, Length::ZERO],
        )
        .unwrap();
        assert_eq!(seven.cut_count(), 1);
        assert_eq!(seven.node(1).unwrap().rectangle.extent[0], mm(2));
        assert_eq!(
            seven.node(seven.usable()).unwrap().rectangle.extent[0],
            mm(93)
        );

        let trims = [mm(5); 4];
        let mut four = CutTree::with_trims([mm(100), mm(100)], mm(5), trims).unwrap();
        let usable = four.node(four.usable()).unwrap().rectangle;
        assert_eq!(usable.origin, [mm(5), mm(5)]);
        assert_eq!(usable.extent, [mm(90), mm(90)]);
        four.finish_part(four.usable(), Uuid::new_v4(), [mm(90), mm(90)])
            .unwrap();
        assert_eq!(four.cut_count(), 4);
        assert_eq!(four.operations().len(), 4);
        let areas = account_tree(&four, trims).unwrap();
        assert_eq!(
            (areas.root_area, areas.part_area, areas.trim_loss),
            (10_000_000_000, 8_100_000_000, 1_900_000_000)
        );
        assert_eq!(areas.offcut_area + areas.waste_area + areas.kerf_loss, 0);
    }

    fn fixture() -> (Project, CutTree) {
        let mut project = Project::new("test", Currency::Brl);
        let material = Material {
            default_band: None,
            kind: Default::default(),
            id: Uuid::new_v4(),
            name: "ply".into(),
            default_thickness: mm(18),
            default_grain: BoardGrain::Length,
        };
        let board = Board {
            banding: Default::default(),
            id: Uuid::new_v4(),
            name: "part".into(),
            material_id: material.id,
            length: mm(100),
            width: mm(50),
            thickness: mm(18),
            grain_override: None,
            parent_id: None,
            pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
        };
        let stock = Stock {
            id: Uuid::new_v4(),
            name: "stock".into(),
            material_id: material.id,
            length: mm(105),
            width: mm(50),
            thickness: mm(18),
            grain: StockGrain::AlongX,
            source: StockSource::Owned,
            price: None,
            priority: 0,
            trim: [Length::ZERO; 4],
        };
        project.allocations.push(Allocation {
            id: Uuid::new_v4(),
            board_id: board.id,
            stock_id: stock.id,
            origin: [Length::ZERO; 2],
            quarter_turn: false,
            locked: false,
        });
        project.materials.push(material);
        project.boards.push(board);
        project.stock.push(stock);
        let mut tree = CutTree::new([mm(105), mm(50)], mm(5)).unwrap();
        let split = tree.split(0, Axis::X, mm(100)).unwrap();
        tree.finish_part(split.first, project.boards[0].id, [mm(100), mm(50)])
            .unwrap();
        (project, tree)
    }

    fn placed(project: &mut Project, x: i64, y: i64, length: i64, width: i64) {
        let mut board = project.boards[0].clone();
        board.id = Uuid::new_v4();
        board.length = mm(length);
        board.width = mm(width);
        project.allocations.push(Allocation {
            id: Uuid::new_v4(),
            board_id: board.id,
            stock_id: project.stock[0].id,
            origin: [mm(x), mm(y)],
            quarter_turn: false,
            locked: false,
        });
        project.boards.push(board);
    }

    #[test]
    fn reconstructs_slicing_layout_with_exact_part_leaves() {
        let (mut project, _) = fixture();
        project.stock[0].length = mm(205);
        project.stock[0].width = mm(105);
        project.allocations[0].origin = [mm(0), mm(0)];
        placed(&mut project, 105, 0, 100, 50);
        placed(&mut project, 0, 55, 100, 50);
        placed(&mut project, 105, 55, 100, 50);
        assert_eq!(
            reconstruct_witness(&project, project.stock[0].id, mm(5), 1),
            Reconstruction::BudgetExhausted
        );
        let original = project.allocations.clone();
        let Reconstruction::Verified { tree, accounting } =
            reconstruct_witness(&project, project.stock[0].id, mm(5), 1000)
        else {
            panic!("expected a verified witness");
        };
        assert_eq!(tree.cut_count(), 3);
        assert_eq!(accounting.part_area, 20_000_000_000);
        assert_eq!(project.allocations, original);
        assert_eq!(
            validate_witness(&tree, &project, project.stock[0].id),
            Ok(accounting)
        );
    }

    #[test]
    fn cancellation_is_polled_inside_recursive_witness_search() {
        use std::cell::Cell;
        let (project, _) = fixture();
        let calls = Cell::new(0);
        let result = reconstruct_witness_cancellable(
            &project,
            project.stock[0].id,
            project.cutting_kerf,
            1000,
            [Axis::X, Axis::Y],
            &|| {
                calls.set(calls.get() + 1);
                calls.get() >= 6
            },
        );
        assert_eq!(result, Err(()));
        assert!(calls.get() >= 6);
    }

    #[test]
    fn non_slicing_pinwheel_and_budget_are_distinct() {
        let (mut project, _) = fixture();
        project.stock[0].length = mm(310);
        project.stock[0].width = mm(310);
        project.allocations[0].origin = [mm(0), mm(0)];
        project.boards[0].length = mm(205);
        project.boards[0].width = mm(100);
        placed(&mut project, 210, 0, 100, 205);
        placed(&mut project, 105, 210, 205, 100);
        placed(&mut project, 0, 105, 100, 205);
        placed(&mut project, 105, 105, 100, 100);
        let id = project.stock[0].id;
        assert_eq!(
            reconstruct_witness(&project, id, mm(5), 1000),
            Reconstruction::RuleViolation(ReconstructionViolation::NoSlicing)
        );
        assert_eq!(
            reconstruct_witness(&project, id, mm(5), 0),
            Reconstruction::BudgetExhausted
        );
    }

    #[test]
    fn lone_part_is_isolated_with_edge_shaving_and_trims() {
        let (mut project, _) = fixture();
        let id = project.stock[0].id;
        let Reconstruction::Verified { tree, .. } = reconstruct_witness(&project, id, mm(5), 100)
        else {
            panic!("105 mm must shave");
        };
        assert_eq!(tree.cut_count(), 1);
        project.stock[0].length = mm(103);
        assert_eq!(
            reconstruct_witness(&project, id, mm(5), 100),
            Reconstruction::RuleViolation(ReconstructionViolation::Cut(CutError::SubKerfEdge))
        );
        project.stock[0].length = mm(105);
        project.stock[0].width = mm(105);
        project.stock[0].trim = [mm(5), mm(5), mm(5), mm(5)];
        project.boards[0].length = mm(95);
        project.boards[0].width = mm(95);
        project.allocations[0].origin = [mm(5), mm(5)];
        let Reconstruction::Verified { tree, accounting } =
            reconstruct_witness(&project, id, mm(5), 100)
        else {
            panic!("trimmed part must fit exactly");
        };
        assert_eq!(tree.cut_count(), 4);
        assert_eq!(accounting.kerf_loss, 0);
        assert!(accounting.trim_loss > 0);
        project.stock[0].trim[0] = mm(3);
        assert_eq!(
            reconstruct_witness(&project, id, mm(5), 100),
            Reconstruction::RuleViolation(ReconstructionViolation::Cut(CutError::SubKerfEdge))
        );
    }

    #[test]
    fn reconstruction_checks_orientation_and_compatibility_before_search() {
        let (mut project, _) = fixture();
        let id = project.stock[0].id;
        project.allocations[0].quarter_turn = true;
        assert_eq!(
            reconstruct_witness(&project, id, mm(5), 0),
            Reconstruction::RuleViolation(ReconstructionViolation::Witness(
                WitnessError::GrainMismatch(project.boards[0].id)
            ))
        );
        project.stock[0].grain = StockGrain::Nondirectional;
        project.stock[0].length = mm(50);
        project.stock[0].width = mm(100);
        let Reconstruction::Verified { tree, .. } = reconstruct_witness(&project, id, mm(5), 20)
        else {
            panic!("rotated exact stock must be cut-free");
        };
        assert_eq!(tree.cut_count(), 0);
        project.boards[0].thickness = mm(12);
        assert_eq!(
            reconstruct_witness(&project, id, mm(5), 20),
            Reconstruction::RuleViolation(ReconstructionViolation::Witness(
                WitnessError::ThicknessMismatch(project.boards[0].id)
            ))
        );
    }

    #[test]
    fn reconstruction_respects_205_versus_204_kerf_boundary() {
        let (mut project, _) = fixture();
        project.stock[0].length = mm(205);
        project.stock[0].width = mm(50);
        placed(&mut project, 105, 0, 100, 50);
        let id = project.stock[0].id;
        let Reconstruction::Verified { tree, .. } = reconstruct_witness(&project, id, mm(5), 100)
        else {
            panic!("two blanks separated by the full kerf must fit");
        };
        assert_eq!(tree.cut_count(), 1);
        project.stock[0].length = mm(204);
        assert_eq!(
            reconstruct_witness(&project, id, mm(5), 100),
            Reconstruction::RuleViolation(ReconstructionViolation::Witness(
                WitnessError::PlacementMismatch(project.boards[1].id)
            ))
        );
    }

    #[test]
    fn exhaustive_rectangular_partitions_conserve_area() {
        for width in 2..25 {
            for height in 1..12 {
                for blade in 1..width {
                    for first in 0..width - blade {
                        let mut tree = CutTree::new([mm(width), mm(height)], mm(blade)).unwrap();
                        let split = tree.split(0, Axis::X, mm(first)).unwrap();
                        if first > 0 {
                            tree.finish_part(split.first, Uuid::new_v4(), [mm(first), mm(height)])
                                .unwrap();
                        }
                        let a = account_tree(&tree, [Length::ZERO; 4]).unwrap();
                        assert_eq!(
                            a.root_area,
                            a.part_area + a.offcut_area + a.waste_area + a.kerf_loss + a.trim_loss
                        );
                        assert_eq!(a.kerf_loss, i128::from(blade * height) * 1_000_000);
                    }
                }
            }
        }
        let trim = CutTree::with_trims([mm(100); 2], mm(5), [mm(7), mm(8), mm(9), mm(10)]).unwrap();
        let a = account_tree(&trim, [mm(7), mm(8), mm(9), mm(10)]).unwrap();
        assert_eq!(a.trim_loss, a.root_area - a.offcut_area);
        assert_eq!(a.kerf_loss, 0);
        let four = CutTree::with_trims([mm(100); 2], mm(5), [mm(5); 4]).unwrap();
        let a = account_tree(&four, [mm(5); 4]).unwrap();
        assert_eq!(a.trim_loss, 1_900_000_000);
        assert_eq!(a.offcut_area, 8_100_000_000);
        assert!(account_tree(&four, [Length::ZERO; 4]).is_err());
    }

    #[test]
    fn witness_rejects_incompatible_drafts_and_tampered_geometry() {
        let (mut project, tree) = fixture();
        let stock = project.stock[0].id;
        assert!(validate_witness(&tree, &project, stock).is_ok());
        project.boards[0].material_id = Uuid::new_v4();
        assert!(matches!(
            validate_witness(&tree, &project, stock),
            Err(WitnessError::MaterialMismatch(_))
        ));
        project.boards[0].material_id = project.stock[0].material_id;
        project.boards[0].thickness = mm(12);
        assert!(matches!(
            validate_witness(&tree, &project, stock),
            Err(WitnessError::ThicknessMismatch(_))
        ));
        project.boards[0].thickness = mm(18);
        project.stock[0].grain = StockGrain::Unknown;
        assert!(matches!(
            validate_witness(&tree, &project, stock),
            Err(WitnessError::GrainMismatch(_))
        ));
        project.stock[0].grain = StockGrain::AlongY;
        assert!(matches!(
            validate_witness(&tree, &project, stock),
            Err(WitnessError::GrainMismatch(_))
        ));
        project.stock[0].grain = StockGrain::AlongX;
        project.allocations[0].quarter_turn = true;
        assert!(matches!(
            validate_witness(&tree, &project, stock),
            Err(WitnessError::GrainMismatch(_))
        ));
        project.stock[0].grain = StockGrain::Nondirectional;
        assert!(matches!(
            validate_witness(&tree, &project, stock),
            Err(WitnessError::PlacementMismatch(_))
        ));
        project.allocations[0].quarter_turn = false;
        project.allocations[0].origin[0] = mm(1);
        assert!(matches!(
            validate_witness(&tree, &project, stock),
            Err(WitnessError::PlacementMismatch(_))
        ));
        project.allocations[0].origin[0] = Length::ZERO;
        let mut duplicate = project.allocations[0].clone();
        duplicate.id = Uuid::new_v4();
        duplicate.stock_id = Uuid::new_v4();
        project.allocations.push(duplicate);
        assert!(matches!(
            validate_witness(&tree, &project, stock),
            Err(WitnessError::DuplicatePart(_))
        ));
        let (_, mut bad) = fixture();
        bad.nodes[2].rectangle.origin[0] = mm(99); // blade overlaps output
        assert!(matches!(
            account_tree(&bad, [Length::ZERO; 4]),
            Err(WitnessError::InvalidTree { .. })
        ));
        bad.nodes[2].rectangle.origin[0] = mm(105);
        bad.nodes[1].rectangle.extent[0] = mm(101);
        assert!(matches!(
            account_tree(&bad, [Length::ZERO; 4]),
            Err(WitnessError::InvalidTree { .. })
        ));
    }

    #[test]
    fn two_parts_fit_at_205_but_not_204() {
        let mut tree = CutTree::new([mm(205), mm(100)], mm(5)).unwrap();
        let halves = tree.split(0, Axis::X, mm(100)).unwrap();
        let ids = [Uuid::new_v4(), Uuid::new_v4()];
        for (piece, part) in [(halves.first, ids[0]), (halves.second, ids[1])] {
            tree.finish_part(piece, part, [mm(100), mm(100)]).unwrap();
        }
        assert_eq!(tree.cut_count(), 1);
        assert_eq!(
            tree.node(halves.second).unwrap().rectangle.origin[0],
            mm(105)
        );
        let mut short = CutTree::new([mm(204), mm(100)], mm(5)).unwrap();
        let result = short.split(0, Axis::X, mm(100)).unwrap();
        assert_eq!(
            short.finish_part(result.second, ids[1], [mm(100), mm(100)]),
            Err(CutError::PartSizeMismatch)
        );
        assert_eq!(
            short.node(result.second).unwrap().rectangle.extent[0],
            mm(99)
        );
    }

    #[test]
    fn exact_stock_requires_no_pass_even_if_kerf_exceeds_stock() {
        let mut tree = CutTree::new([mm(2), mm(1)], mm(5)).unwrap();
        tree.finish_part(0, Uuid::new_v4(), [mm(2), mm(1)]).unwrap();
        assert_eq!(tree.cut_count(), 0);
        assert_eq!(
            tree.split(0, Axis::X, Length::ZERO),
            Err(CutError::NotAvailable)
        );
    }

    #[test]
    fn edge_shaving_consumes_full_kerf_without_zero_usable_piece() {
        let mut tree = CutTree::new([mm(105), mm(50)], mm(5)).unwrap();
        let split = tree.split(0, Axis::X, mm(100)).unwrap();
        assert_eq!(tree.node(split.second).unwrap().kind, CutKind::Waste);
        assert_eq!(
            tree.node(split.second).unwrap().rectangle.extent[0],
            Length::ZERO
        );
        assert_eq!(
            tree.finish_part(split.second, Uuid::new_v4(), [mm(1), mm(50)]),
            Err(CutError::NotAvailable)
        );
        tree.finish_part(split.first, Uuid::new_v4(), [mm(100), mm(50)])
            .unwrap();
        let mut insufficient = CutTree::new([mm(103), mm(50)], mm(5)).unwrap();
        assert_eq!(
            insufficient.split(0, Axis::X, mm(100)),
            Err(CutError::SubKerfEdge)
        );
        assert_eq!(
            CutTree::with_trims(
                [mm(103), mm(50)],
                mm(5),
                [Length::ZERO, mm(3), Length::ZERO, Length::ZERO]
            ),
            Err(CutError::SubKerfEdge)
        );
    }

    #[test]
    fn trims_in_order_charge_each_corner_once() {
        let tree = CutTree::with_trims([mm(100), mm(100)], mm(5), [mm(5); 4]).unwrap();
        assert_eq!(tree.cut_count(), 4);
        let usable = tree.node(tree.usable()).unwrap().rectangle;
        assert_eq!(usable.origin, [mm(5), mm(5)]);
        assert_eq!(usable.extent, [mm(90), mm(90)]);
        // Each kerf is full-span on the piece available at that stage:
        // 500 + 500 + 450 + 450 = 1,900 square mm.
        let loss: i128 = tree
            .nodes()
            .iter()
            .filter_map(|node| {
                let CutKind::Split { axis, kerf, .. } = node.kind else {
                    return None;
                };
                let transverse = node.rectangle.extent[1 - axis.index()].micrometres();
                Some(i128::from(kerf.micrometres()) * i128::from(transverse))
            })
            .sum();
        assert_eq!(loss, 1_900_000_000);
        let larger = CutTree::with_trims(
            [mm(100), mm(100)],
            mm(5),
            [mm(7), Length::ZERO, Length::ZERO, Length::ZERO],
        )
        .unwrap();
        assert_eq!(
            larger.node(larger.usable()).unwrap().rectangle.extent[0],
            mm(93)
        );
        assert_eq!(larger.node(1).unwrap().kind, CutKind::Waste);
        assert_eq!(larger.node(1).unwrap().rectangle.extent[0], mm(2));
    }

    #[test]
    fn rejects_invalid_splits_and_trim_allowances() {
        let mut tree = CutTree::new([mm(100), mm(100)], mm(5)).unwrap();
        assert_eq!(tree.split(0, Axis::X, mm(96)), Err(CutError::SubKerfEdge));
        assert_eq!(
            tree.split(0, Axis::X, mm(-1)),
            Err(CutError::InsufficientExtent)
        );
        assert_eq!(tree.split(0, Axis::X, mm(95)).unwrap().second, 2);
        assert_eq!(tree.split(2, Axis::Y, mm(10)), Err(CutError::NotAvailable));
        assert_eq!(
            CutTree::with_trims(
                [mm(100); 2],
                mm(5),
                [mm(4), Length::ZERO, Length::ZERO, Length::ZERO]
            ),
            Err(CutError::SubKerfEdge)
        );
        assert_eq!(
            CutTree::with_trims(
                [mm(100); 2],
                mm(5),
                [mm(50), mm(50), Length::ZERO, Length::ZERO]
            ),
            Err(CutError::InvalidTrim)
        );
        assert_eq!(
            CutTree::new([mm(100); 2], Length::ZERO),
            Err(CutError::InvalidKerf)
        );
    }

    #[test]
    fn nested_strip_operations_are_preorder_and_reference_existing_outputs() {
        let mut tree = CutTree::new([mm(205), mm(105)], mm(5)).unwrap();
        let strips = tree.split(0, Axis::X, mm(100)).unwrap();
        // Create the right strip's cut first to ensure numbering follows the
        // physical parent-before-child traversal, not node mutation order.
        let right = tree.split(strips.second, Axis::Y, mm(50)).unwrap();
        let left = tree.split(strips.first, Axis::Y, mm(50)).unwrap();
        let ops = tree.operations();
        assert_eq!(ops.len(), tree.cut_count());
        assert_eq!(
            ops.iter().map(|op| op.number).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(
            ops.iter().map(|op| op.input).collect::<Vec<_>>(),
            [0, strips.first, strips.second]
        );
        assert_eq!(ops[0].outputs, strips);
        assert_eq!(ops[1].outputs, left);
        assert_eq!(ops[2].outputs, right);
        assert_eq!(ops[1].axis, Axis::Y);
        assert_eq!(ops[1].reference_edge, Edge::Low);
        assert_eq!(ops[1].retained_extent, mm(50));
        assert_eq!(ops[1].kerf_side, Edge::High);
        let mut available = vec![tree.root()];
        for op in ops {
            assert!(available.contains(&op.input));
            assert!(!available.contains(&op.retained_output));
            assert!(!available.contains(&op.outputs.first));
            assert!(!available.contains(&op.outputs.second));
            available.retain(|&id| id != op.input);
            available.extend([op.outputs.first, op.outputs.second]);
        }
    }

    #[test]
    fn trim_operations_dimension_the_retained_side_from_correct_edges() {
        let tree =
            CutTree::with_trims([mm(100), mm(100)], mm(5), [mm(7), mm(8), mm(9), mm(10)]).unwrap();
        let ops = tree.operations();
        assert_eq!(ops.len(), 4);
        assert_eq!(ops.len(), tree.cut_count());
        for (index, (axis, edge, dimension, kerf_side)) in [
            (Axis::X, Edge::High, mm(93), Edge::Low),
            (Axis::X, Edge::Low, mm(85), Edge::High),
            (Axis::Y, Edge::High, mm(91), Edge::Low),
            (Axis::Y, Edge::Low, mm(81), Edge::High),
        ]
        .into_iter()
        .enumerate()
        {
            let op = ops[index];
            assert_eq!(op.number, index + 1);
            assert_eq!(
                (op.axis, op.reference_edge, op.retained_extent, op.kerf_side),
                (axis, edge, dimension, kerf_side)
            );
            assert_eq!(
                op.retained_output,
                if edge == Edge::High {
                    op.outputs.second
                } else {
                    op.outputs.first
                }
            );
            if index > 0 {
                assert_eq!(op.input, ops[index - 1].retained_output);
            }
        }
        assert_eq!(ops.last().unwrap().retained_output, tree.usable());
    }

    #[test]
    fn shaving_and_exact_stock_have_no_phantom_operations() {
        let mut exact = CutTree::new([mm(100), mm(50)], mm(5)).unwrap();
        exact
            .finish_part(0, Uuid::new_v4(), [mm(100), mm(50)])
            .unwrap();
        assert!(exact.operations().is_empty());
        let mut shave = CutTree::new([mm(105), mm(50)], mm(5)).unwrap();
        let outputs = shave.split(0, Axis::X, mm(100)).unwrap();
        let op = shave.operations()[0];
        assert_eq!(op.number, 1);
        assert_eq!(op.outputs, outputs);
        assert_eq!(op.retained_output, outputs.first);
        assert_eq!(
            (op.reference_edge, op.retained_extent, op.kerf_side),
            (Edge::Low, mm(100), Edge::High)
        );
        assert_eq!(shave.node(outputs.second).unwrap().kind, CutKind::Waste);
        assert_eq!(shave.cut_count(), 1);
        let mut opposite = CutTree::new([mm(105), mm(50)], mm(5)).unwrap();
        let outputs = opposite.split(0, Axis::X, Length::ZERO).unwrap();
        let op = opposite.operations()[0];
        assert_eq!(
            (
                op.reference_edge,
                op.retained_extent,
                op.kerf_side,
                op.retained_output
            ),
            (Edge::High, mm(100), Edge::Low, outputs.second)
        );
        assert_eq!(op.outputs.first, outputs.first);
        assert_eq!(opposite.node(outputs.first).unwrap().kind, CutKind::Waste);
    }
}
