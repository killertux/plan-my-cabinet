//! The parts a shop cuts, independent of any file format: boards grouped by
//! everything that makes two parts interchangeable (name, cabinet, material,
//! size, grain, banding and drilling), with quantities. It needs only a valid
//! design: sheets, cut plans, kerf and prices play no part, because the shop
//! nests the parts on its own stock.
use serde_json::json;
use uuid::Uuid;

use crate::banding_rules;
use crate::domain::{BoardGrain, DomainError, Project};
use crate::machining::{BoardMachining, MachiningOptions, Omission, board_machining};
use crate::units::Length;

/// A band on one edge, with what the shop needs to know about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BandRef {
    pub id: Uuid,
    pub name: String,
    pub thickness: Length,
    pub height: Length,
}

/// Identical parts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartGroup {
    pub name: String,
    /// The cabinet (top assembly) the parts belong to.
    pub cabinet: Option<String>,
    pub material_id: Uuid,
    pub material: String,
    pub length: Length,
    pub width: Length,
    pub thickness: Length,
    pub grain: BoardGrain,
    /// In `BoardEdge::ALL` order (MinX, MaxX, MinY, MaxY).
    pub banding: [Option<BandRef>; 4],
    pub machining: BoardMachining,
    pub boards: Vec<Uuid>,
}

impl PartGroup {
    pub fn quantity(&self) -> usize {
        self.boards.len()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartList {
    pub project_name: String,
    pub groups: Vec<PartGroup>,
    /// Drilling left out, with why.
    pub omissions: Vec<Omission>,
}

impl PartList {
    pub fn part_count(&self) -> usize {
        self.groups.iter().map(PartGroup::quantity).sum()
    }

    /// A hash of everything a part-list file says, for receipts: two lists
    /// with the same fingerprint produce the same parts.
    pub fn fingerprint(&self) -> String {
        use sha2::{Digest, Sha256};
        let groups: Vec<_> = self
            .groups
            .iter()
            .map(|g| {
                json!([
                    g.name,
                    g.cabinet,
                    g.material,
                    g.length,
                    g.width,
                    g.thickness,
                    g.grain,
                    g.banding
                        .iter()
                        .map(|b| b.as_ref().map(|b| json!([b.name, b.thickness, b.height])))
                        .collect::<Vec<_>>(),
                    g.machining
                        .face_drills
                        .iter()
                        .map(|d| json!([d.face, d.at_um, d.diameter, d.depth, d.through]))
                        .collect::<Vec<_>>(),
                    g.quantity(),
                ])
            })
            .collect();
        format!(
            "{:x}",
            Sha256::digest(json!(["part-list-v1", groups]).to_string().as_bytes())
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PartListBlocked {
    /// The design has no boards.
    NoBoards,
    InvalidDesign(DomainError),
}

/// Group the project's boards into parts.
pub fn build(project: &Project, options: &MachiningOptions) -> Result<PartList, PartListBlocked> {
    project.validate().map_err(PartListBlocked::InvalidDesign)?;
    if project.boards.is_empty() {
        return Err(PartListBlocked::NoBoards);
    }
    let banding = banding_rules::effective(project);
    let (mut machining, omissions) = board_machining(project, options);
    let mut groups: Vec<PartGroup> = Vec::new();
    for board in &project.boards {
        let Some(material) = project.material(board.material_id) else {
            continue;
        };
        let bands = banding
            .get(&board.id)
            .map_or([None; 4], banding_rules::bands)
            .map(|band| {
                band.and_then(|id| project.edge_band(id)).map(|b| BandRef {
                    id: b.id,
                    name: b.name.clone(),
                    thickness: b.thickness,
                    height: b.height,
                })
            });
        let mut drills = machining.remove(&board.id).unwrap_or_default();
        // Holes in a stable order, so identical parts compare equal.
        drills
            .face_drills
            .sort_by_key(|d| (d.face as u8, d.at_um, d.diameter, d.depth));
        let candidate = PartGroup {
            name: board.name.clone(),
            cabinet: cabinet_name(project, board.parent_id),
            material_id: material.id,
            material: material.name.clone(),
            length: board.length,
            width: board.width,
            thickness: board.thickness,
            grain: board.effective_grain(material),
            banding: bands,
            machining: drills,
            boards: vec![board.id],
        };
        match groups.iter_mut().find(|g| same_part(g, &candidate)) {
            Some(group) => group.boards.push(board.id),
            None => groups.push(candidate),
        }
    }
    Ok(PartList {
        project_name: project.name.clone(),
        groups,
        omissions,
    })
}

fn same_part(a: &PartGroup, b: &PartGroup) -> bool {
    let holes = |g: &PartGroup| {
        g.machining
            .face_drills
            .iter()
            .map(|d| (d.face, d.at_um, d.diameter, d.depth, d.through))
            .collect::<Vec<_>>()
    };
    a.name == b.name
        && a.cabinet == b.cabinet
        && a.material_id == b.material_id
        && [a.length, a.width, a.thickness] == [b.length, b.width, b.thickness]
        && a.grain == b.grain
        && a.banding == b.banding
        && holes(a) == holes(b)
        && a.machining.edge_drills.len() == b.machining.edge_drills.len()
}

/// The top assembly above a board: the cabinet it is part of.
fn cabinet_name(project: &Project, parent: Option<Uuid>) -> Option<String> {
    let mut current = parent?;
    loop {
        let assembly = project.assemblies.iter().find(|a| a.id == current)?;
        match assembly.parent_id {
            Some(next) => current = next,
            None => return Some(assembly.name.clone()),
        }
    }
}
