//! Edge band records and per-edge banding edits. Every command is one editor
//! transaction, so one undo step.
use uuid::Uuid;

use crate::banding_rules::{self, FRONT};
use crate::board_frame::BoardFrame;
use crate::commands::{EditError, ProjectEditor};
use crate::domain::{BoardEdge, EdgeBand, EdgeBanding, MaterialKind, Project, SrgbColor};
use crate::units::Length;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BandingError {
    MissingBoard(Uuid),
    MissingBand(Uuid),
    MissingMaterial(Uuid),
    /// None of the boards is on a material that takes banding.
    NotAccepted,
    EmptyName,
    /// Boards and materials still use the band.
    BandInUse {
        boards: Vec<Uuid>,
        materials: Vec<Uuid>,
    },
}

/// Quick settings for all four edges of one or more boards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BandingPreset {
    /// Every edge follows the automatic rule.
    Auto,
    /// No edge is banded.
    None,
    /// Only the edges facing the front of the cabinet.
    Front,
    AllFour,
}

/// What a banding edit did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BandingOutcome {
    pub changed: Vec<Uuid>,
    /// Boards left alone because their material takes no banding.
    pub skipped: Vec<Uuid>,
}

/// Drop banding from boards whose material no longer takes it. Runs inside
/// every transaction, so changing a board's material (or a material's kind)
/// can never leave banding behind.
pub(crate) fn strip_unaccepted(project: &mut Project) {
    let refused: Vec<Uuid> = project
        .materials
        .iter()
        .filter(|m| !m.kind.accepts_banding())
        .map(|m| m.id)
        .collect();
    for board in &mut project.boards {
        if refused.contains(&board.material_id) {
            for value in &mut board.banding.0 {
                if matches!(value, EdgeBanding::On(_)) {
                    *value = EdgeBanding::Auto;
                }
            }
        }
    }
}

/// Number of banded edges on each board, for telling the user what a change
/// removed.
pub fn banded_edges(project: &Project) -> usize {
    banding_rules::effective(project)
        .values()
        .map(|states| states.iter().filter(|s| s.band.is_some()).count())
        .sum()
}

fn accepts(project: &Project, board: Uuid) -> Result<bool, BandingError> {
    let board = project
        .board(board)
        .ok_or(BandingError::MissingBoard(board))?;
    Ok(project
        .material(board.material_id)
        .is_some_and(|m| m.kind.accepts_banding()))
}

impl ProjectEditor {
    /// Set `edges` of every board in `boards` to `value`. Boards whose
    /// material takes no banding are skipped.
    pub fn set_edge_banding(
        &mut self,
        boards: &[Uuid],
        edges: &[BoardEdge],
        value: EdgeBanding,
    ) -> Result<BandingOutcome, EditError<BandingError>> {
        let mut outcome = BandingOutcome::default();
        self.transact(|project| {
            if let EdgeBanding::On(band) = value
                && project.edge_band(band).is_none()
            {
                return Err(BandingError::MissingBand(band));
            }
            for &id in boards {
                if !accepts(project, id)? {
                    outcome.skipped.push(id);
                    continue;
                }
                let board = project.board_mut(id).expect("checked");
                for &edge in edges {
                    board.banding.set(edge, value);
                }
                outcome.changed.push(id);
            }
            if outcome.changed.is_empty() && !boards.is_empty() {
                return Err(BandingError::NotAccepted);
            }
            Ok(())
        })?;
        Ok(outcome)
    }

    /// Flip whether an edge is banded, as the user sees it. The edge becomes
    /// a manual override, unless the new state is what the automatic rule
    /// gives, in which case it goes back to automatic. `band` is the band a
    /// newly banded edge gets.
    pub fn toggle_edge_banding(
        &mut self,
        board: Uuid,
        edge: BoardEdge,
        band: Uuid,
    ) -> Result<EdgeBanding, EditError<BandingError>> {
        let project = self.project();
        if !accepts(project, board).map_err(EditError::Command)? {
            return Err(EditError::Command(BandingError::NotAccepted));
        }
        let states = banding_rules::board_states(project, board)
            .ok_or(EditError::Command(BandingError::MissingBoard(board)))?;
        let current = states[edge.index()];
        let automatic = {
            let mut probe = project.clone();
            probe
                .board_mut(board)
                .expect("board exists")
                .banding
                .set(edge, EdgeBanding::Auto);
            banding_rules::board_states(&probe, board).expect("board exists")[edge.index()].band
        };
        let wanted = if current.band.is_some() {
            None
        } else {
            Some(band)
        };
        let value = if wanted == automatic {
            EdgeBanding::Auto
        } else {
            wanted.map_or(EdgeBanding::Off, EdgeBanding::On)
        };
        self.set_edge_banding(&[board], &[edge], value)?;
        Ok(value)
    }

    /// Apply a preset to every edge of `boards`. `band` is the band for
    /// `Front` and `AllFour`.
    pub fn apply_banding_preset(
        &mut self,
        boards: &[Uuid],
        preset: BandingPreset,
        band: Option<Uuid>,
    ) -> Result<BandingOutcome, EditError<BandingError>> {
        let mut outcome = BandingOutcome::default();
        let fronts: Vec<(Uuid, Vec<BoardEdge>)> = boards
            .iter()
            .map(|&id| {
                let front = BoardFrame::new(self.project(), id)
                    .map(|frame| banding_rules::edges_facing(&frame, FRONT))
                    .unwrap_or_default();
                (id, front)
            })
            .collect();
        self.transact(|project| {
            let on = match (preset, band) {
                (BandingPreset::Front | BandingPreset::AllFour, Some(band)) => {
                    if project.edge_band(band).is_none() {
                        return Err(BandingError::MissingBand(band));
                    }
                    EdgeBanding::On(band)
                }
                (BandingPreset::Front | BandingPreset::AllFour, None) => {
                    return Err(BandingError::MissingBand(Uuid::nil()));
                }
                _ => EdgeBanding::Off,
            };
            for (id, front) in &fronts {
                if !accepts(project, *id)? {
                    outcome.skipped.push(*id);
                    continue;
                }
                let board = project.board_mut(*id).expect("checked");
                for edge in BoardEdge::ALL {
                    let value = match preset {
                        BandingPreset::Auto => EdgeBanding::Auto,
                        BandingPreset::None => EdgeBanding::Off,
                        BandingPreset::AllFour => on,
                        BandingPreset::Front if front.contains(&edge) => on,
                        BandingPreset::Front => EdgeBanding::Off,
                    };
                    board.banding.set(edge, value);
                }
                outcome.changed.push(*id);
            }
            if outcome.changed.is_empty() && !boards.is_empty() {
                return Err(BandingError::NotAccepted);
            }
            Ok(())
        })?;
        Ok(outcome)
    }

    pub fn create_edge_band(
        &mut self,
        name: &str,
        thickness: Length,
        height: Length,
        color: SrgbColor,
    ) -> Result<Uuid, EditError<BandingError>> {
        let id = Uuid::new_v4();
        let name = name.trim().to_owned();
        self.transact(|project| {
            if name.is_empty() {
                return Err(BandingError::EmptyName);
            }
            project.edge_bands.push(EdgeBand {
                id,
                name,
                thickness,
                height,
                color,
            });
            Ok(())
        })?;
        Ok(id)
    }

    pub fn update_edge_band(&mut self, band: EdgeBand) -> Result<bool, EditError<BandingError>> {
        self.transact(|project| {
            let name = band.name.trim().to_owned();
            if name.is_empty() {
                return Err(BandingError::EmptyName);
            }
            let slot = project
                .edge_bands
                .iter_mut()
                .find(|b| b.id == band.id)
                .ok_or(BandingError::MissingBand(band.id))?;
            *slot = EdgeBand { name, ..band };
            Ok(())
        })
    }

    /// Remove a band nothing uses.
    pub fn remove_edge_band(&mut self, id: Uuid) -> Result<bool, EditError<BandingError>> {
        self.transact(|project| {
            let index = project
                .edge_bands
                .iter()
                .position(|b| b.id == id)
                .ok_or(BandingError::MissingBand(id))?;
            let (boards, materials) = band_usage(project, id);
            if !boards.is_empty() || !materials.is_empty() {
                return Err(BandingError::BandInUse { boards, materials });
            }
            project.edge_bands.remove(index);
            Ok(())
        })
    }

    /// Set what a material is made of and the band its free edges get.
    pub fn set_material_banding(
        &mut self,
        material: Uuid,
        kind: MaterialKind,
        default_band: Option<Uuid>,
    ) -> Result<bool, EditError<BandingError>> {
        self.transact(|project| {
            if let Some(band) = default_band
                && project.edge_band(band).is_none()
            {
                return Err(BandingError::MissingBand(band));
            }
            let slot = project
                .materials
                .iter_mut()
                .find(|m| m.id == material)
                .ok_or(BandingError::MissingMaterial(material))?;
            slot.kind = kind;
            slot.default_band = default_band;
            Ok(())
        })
    }
}

/// Boards with the band set by hand, and materials using it as default.
pub fn band_usage(project: &Project, id: Uuid) -> (Vec<Uuid>, Vec<Uuid>) {
    let boards = project
        .boards
        .iter()
        .filter(|b| b.banding.bands().any(|band| band == id))
        .map(|b| b.id)
        .collect();
    let materials = project
        .materials
        .iter()
        .filter(|m| m.default_band == Some(id))
        .map(|m| m.id)
        .collect();
    (boards, materials)
}

/// Total band length per band, in micrometres, over every banded edge.
pub fn band_lengths(project: &Project) -> Vec<(Uuid, i128)> {
    let states = banding_rules::effective(project);
    let mut totals: Vec<(Uuid, i128)> = project.edge_bands.iter().map(|b| (b.id, 0)).collect();
    for board in &project.boards {
        let Some(edges) = states.get(&board.id) else {
            continue;
        };
        for edge in BoardEdge::ALL {
            if let Some(band) = edges[edge.index()].band
                && let Some(total) = totals.iter_mut().find(|(id, _)| *id == band)
            {
                total.1 += i128::from(
                    crate::board_frame::edge_length(edge, board.length, board.width).micrometres(),
                );
            }
        }
    }
    totals
}
