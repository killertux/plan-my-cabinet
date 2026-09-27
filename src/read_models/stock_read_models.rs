//! Immutable stock and sheet presentation snapshot. Build once per relevant
//! project state (or repair preview), then share its rows across all surfaces.
use uuid::Uuid;

use crate::allocation_diagnostics::{self, BoardDiagnostic};
use crate::cost_estimate::{self, EstimateError, ProjectEstimate};
use crate::cut_tree::{
    AreaAccounting, CutTree, Reconstruction, ReconstructionViolation, reconstruct_witness,
};
use crate::domain::{BoardGrain, DomainError, Project, StockGrain, StockSource};
use crate::money::Money;
use crate::units::Length;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadModelError {
    InvalidProject(DomainError),
    Estimate(EstimateError),
}

/// No metrics are inferred from a placement rectangle without a current proof.
/// An unused sheet is not a zero-cut verified plan: even its trims have not
/// been established as a cutting sequence by this projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SheetProof {
    Unused,
    Verified {
        tree: CutTree,
        accounting: AreaAccounting,
    },
    Violation(ReconstructionViolation),
    SearchExhausted,
}

impl SheetProof {
    pub fn verified(&self) -> Option<(&CutTree, &AreaAccounting)> {
        match self {
            Self::Verified { tree, accounting } => Some((tree, accounting)),
            _ => None,
        }
    }

    pub fn cut_count(&self) -> Option<usize> {
        self.verified().map(|(tree, _)| tree.cut_count())
    }

    /// Exact ratio of proved finished-part area to the entire physical sheet.
    /// Square micrometres cancel; callers format/round only at the UI boundary.
    pub fn utilization(&self) -> Option<(i128, i128)> {
        self.verified()
            .map(|(_, area)| (area.part_area, area.root_area))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartOnStock {
    pub allocation_id: Uuid,
    pub board_id: Uuid,
    pub name: String,
    pub length: Length,
    pub width: Length,
    /// Snapshot of the actual board thickness, not the current material default.
    pub effective_thickness: Length,
    pub effective_grain: BoardGrain,
    pub origin: [Length; 2],
    pub quarter_turn: bool,
    pub locked: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StockPieceReadModel {
    pub id: Uuid,
    pub alias: String,
    pub global_rank: usize,
    pub name: String,
    pub material_id: Uuid,
    pub material_name: String,
    pub length: Length,
    pub width: Length,
    /// The piece's measured thickness, independent of its material's default.
    pub measured_thickness: Length,
    pub material_default_thickness: Length,
    pub grain: StockGrain,
    pub trim: [Length; 4],
    pub usable_extent: [Length; 2],
    pub source: StockSource,
    /// `None` means unknown; `Some(0)` is a known free piece.
    pub price: Option<Money>,
    /// Actual allocation records, including conflicting or duplicate assignments.
    pub parts: Vec<PartOnStock>,
    pub proof: SheetProof,
}

impl StockPieceReadModel {
    pub fn is_used(&self) -> bool {
        !self.parts.is_empty()
    }

    pub fn measured_area(&self) -> i128 {
        i128::from(self.length.micrometres()) * i128::from(self.width.micrometres())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialStockSummary {
    pub id: Uuid,
    pub name: String,
    pub default_thickness: Length,
    pub board_count: usize,
    pub stock_piece_count: usize,
    /// Boards with no allocation record (irrespective of scene visibility).
    pub unallocated_board_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StockReadModel {
    /// Global first-fit order, not grouped visual order.
    pub pieces: Vec<StockPieceReadModel>,
    pub materials: Vec<MaterialStockSummary>,
    /// One diagnosis per board, including hidden and multiply conflicted boards.
    pub boards: Vec<BoardDiagnostic>,
    /// Authoritative whole-project spending, including exclusions and completeness.
    pub estimate: ProjectEstimate,
    pub cut_fee: Option<Money>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StockUsageSummary {
    pub used_purchased_pieces: usize,
    pub consumed_owned_pieces: usize,
    /// Unavailable if any used piece lacks a current witness.
    pub physical_cuts: Option<u64>,
}

impl StockReadModel {
    /// Rebuild on manufacturing or pricing changes; retain this snapshot on hover
    /// and view-only navigation rather than repeating the bounded witness search.
    pub fn build(project: &Project) -> Result<Self, ReadModelError> {
        project.validate().map_err(ReadModelError::InvalidProject)?;
        // Editor/persistence normally assign aliases. A structurally valid legacy
        // in-memory fixture may have none: normalize a private copy, never mutate
        // the source or manufacture labels from names/current ownership in the UI.
        let mut normalized = project.clone();
        normalized
            .assign_missing_stock_aliases()
            .map_err(ReadModelError::InvalidProject)?;
        let project = &normalized;
        let boards = allocation_diagnostics::diagnose(project);
        let estimate = cost_estimate::estimate(project).map_err(ReadModelError::Estimate)?;
        let materials = project
            .materials
            .iter()
            .map(|material| MaterialStockSummary {
                id: material.id,
                name: material.name.clone(),
                default_thickness: material.default_thickness,
                board_count: project
                    .boards
                    .iter()
                    .filter(|b| b.material_id == material.id)
                    .count(),
                stock_piece_count: project
                    .stock
                    .iter()
                    .filter(|s| s.material_id == material.id)
                    .count(),
                unallocated_board_count: project
                    .boards
                    .iter()
                    .filter(|b| {
                        b.material_id == material.id
                            && !project.allocations.iter().any(|a| a.board_id == b.id)
                    })
                    .count(),
            })
            .collect();
        let pieces = project
            .ordered_stock()
            .into_iter()
            .enumerate()
            .map(|(index, stock)| {
                let material = project
                    .materials
                    .iter()
                    .find(|m| m.id == stock.material_id)
                    .expect("validated material reference");
                let parts = project
                    .allocations
                    .iter()
                    .filter(|a| a.stock_id == stock.id)
                    .map(|allocation| {
                        let board = project
                            .boards
                            .iter()
                            .find(|b| b.id == allocation.board_id)
                            .expect("validated board reference");
                        let board_material = project
                            .materials
                            .iter()
                            .find(|m| m.id == board.material_id)
                            .expect("validated board material");
                        PartOnStock {
                            allocation_id: allocation.id,
                            board_id: board.id,
                            name: board.name.clone(),
                            length: board.length,
                            width: board.width,
                            effective_thickness: board.thickness,
                            effective_grain: board.effective_grain(board_material),
                            origin: allocation.origin,
                            quarter_turn: allocation.quarter_turn,
                            locked: allocation.locked,
                        }
                    })
                    .collect::<Vec<_>>();
                let proof = if parts.is_empty() {
                    SheetProof::Unused
                } else {
                    match reconstruct_witness(
                        project,
                        stock.id,
                        project.cutting_kerf,
                        allocation_diagnostics::WITNESS_BUDGET,
                    ) {
                        Reconstruction::Verified { tree, accounting } => {
                            SheetProof::Verified { tree, accounting }
                        }
                        Reconstruction::RuleViolation(reason) => SheetProof::Violation(reason),
                        Reconstruction::BudgetExhausted => SheetProof::SearchExhausted,
                    }
                };
                StockPieceReadModel {
                    id: stock.id,
                    alias: project
                        .stock_alias(stock.id)
                        .expect("assigned alias")
                        .into(),
                    global_rank: index + 1,
                    name: stock.name.clone(),
                    material_id: stock.material_id,
                    material_name: material.name.clone(),
                    length: stock.length,
                    width: stock.width,
                    measured_thickness: stock.thickness,
                    material_default_thickness: material.default_thickness,
                    grain: stock.grain,
                    trim: stock.trim,
                    usable_extent: [
                        Length::from_micrometres(
                            stock.length.micrometres()
                                - stock.trim[0].micrometres()
                                - stock.trim[1].micrometres(),
                        ),
                        Length::from_micrometres(
                            stock.width.micrometres()
                                - stock.trim[2].micrometres()
                                - stock.trim[3].micrometres(),
                        ),
                    ],
                    source: stock.source,
                    price: stock.price,
                    parts,
                    proof,
                }
            })
            .collect();
        Ok(Self {
            pieces,
            materials,
            boards,
            estimate,
            cut_fee: project.cut_fee,
        })
    }

    /// Stock table projection. Visual grouping may reorder these references but
    /// their global ranks remain the underlying first-fit positions.
    pub fn table_rows(&self, material_filter: Option<Uuid>) -> Vec<&StockPieceReadModel> {
        self.pieces
            .iter()
            .filter(|piece| material_filter.is_none_or(|id| piece.material_id == id))
            .collect()
    }

    /// Cut-plan cards, in actual priority order.
    pub fn sheet_cards(&self) -> &[StockPieceReadModel] {
        &self.pieces
    }

    /// Inspector/Design miniature resolves the identical piece and proof.
    pub fn miniature(&self, stock_id: Uuid) -> Option<&StockPieceReadModel> {
        self.pieces.iter().find(|piece| piece.id == stock_id)
    }

    /// Spending amounts and completeness remain in `estimate`; these physical
    /// counts derive from the same used-piece ledger rather than all inventory.
    pub fn usage_summary(&self) -> StockUsageSummary {
        StockUsageSummary {
            used_purchased_pieces: self
                .estimate
                .used_stock
                .iter()
                .filter(|entry| entry.source == StockSource::ToPurchase)
                .count(),
            consumed_owned_pieces: self
                .estimate
                .used_stock
                .iter()
                .filter(|entry| entry.source == StockSource::Owned)
                .count(),
            physical_cuts: self
                .estimate
                .used_stock
                .iter()
                .try_fold(0_u64, |sum, entry| sum.checked_add(entry.cuts?)),
        }
    }
}
