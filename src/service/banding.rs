//! Edge banding tools: the project's edge bands and the banding of each
//! board edge. Edges are named by the board's own frame: `length_1` and
//! `length_2` run along the length (local min Y and max Y), `width_1` and
//! `width_2` along the width (min X and max X).
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::banding::{BandingError, BandingPreset, band_lengths, band_usage};
use crate::banding_rules::{self, Contact, EdgeState, FRONT};
use crate::board_frame::BoardFrame;
use crate::domain::{BoardEdge, EdgeBand, EdgeBanding, MaterialKind, Project};
use crate::service::dto::{Change, ColorInput, LengthInput, LengthOut, color_hex};
use crate::service::error::{ErrorCode, ServiceError, ServiceResult};
use crate::service::workspace::{Kind, Workspace, object_name};

impl From<BandingError> for ServiceError {
    fn from(error: BandingError) -> Self {
        match error {
            BandingError::MissingBoard(id) => Self::not_found("board", id),
            BandingError::MissingBand(id) => Self::not_found("edge band", id),
            BandingError::MissingMaterial(id) => Self::not_found("material", id),
            BandingError::NotAccepted => Self::new(
                ErrorCode::InvalidArgument,
                "none of these boards takes edge banding: only MDF and MDP materials do",
            ),
            BandingError::EmptyName => Self::invalid("an edge band needs a name").for_field("name"),
            BandingError::BandInUse { boards, materials } => Self::new(
                ErrorCode::Conflict,
                format!(
                    "the band is still used by {} board(s) and {} material(s)",
                    boards.len(),
                    materials.len()
                ),
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EdgeName {
    /// Along the length, local min Y.
    Length1,
    /// Along the length, local max Y.
    Length2,
    /// Along the width, local min X.
    Width1,
    /// Along the width, local max X.
    Width2,
    /// The edges facing the front of the cabinet (world -Y).
    Front,
    All,
}

pub(crate) fn edge_name(edge: BoardEdge) -> &'static str {
    match edge {
        BoardEdge::MinY => "length_1",
        BoardEdge::MaxY => "length_2",
        BoardEdge::MinX => "width_1",
        BoardEdge::MaxX => "width_2",
    }
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BandingValue {
    /// Follow the automatic rule: the material's default band on edges not
    /// joined to another board.
    Auto,
    /// Band the edge (with `band`, else the material's default band).
    On,
    /// No band, whatever the rule says.
    Off,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PresetName {
    /// Every edge automatic.
    Auto,
    None,
    /// Only edges facing the front.
    Front,
    AllFour,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MaterialKindName {
    Mdf,
    Mdp,
    Hdf,
    Plywood,
    SolidWood,
    Other,
}

impl From<MaterialKindName> for MaterialKind {
    fn from(name: MaterialKindName) -> Self {
        match name {
            MaterialKindName::Mdf => Self::Mdf,
            MaterialKindName::Mdp => Self::Mdp,
            MaterialKindName::Hdf => Self::Hdf,
            MaterialKindName::Plywood => Self::Plywood,
            MaterialKindName::SolidWood => Self::SolidWood,
            MaterialKindName::Other => Self::Other,
        }
    }
}

pub(crate) fn kind_name(kind: MaterialKind) -> &'static str {
    match kind {
        MaterialKind::Mdf => "mdf",
        MaterialKind::Mdp => "mdp",
        MaterialKind::Hdf => "hdf",
        MaterialKind::Plywood => "plywood",
        MaterialKind::SolidWood => "solid_wood",
        MaterialKind::Other => "other",
    }
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct SetBoardBandingInput {
    /// Boards by name or id.
    pub boards: Vec<String>,
    /// Either `preset` (all four edges) or `edges` with `value`.
    #[serde(default)]
    pub preset: Option<PresetName>,
    #[serde(default)]
    pub edges: Vec<EdgeName>,
    #[serde(default)]
    pub value: Option<BandingValue>,
    /// The band for `on`, `front` and `all_four`, by name or id. Default:
    /// each board material's default band, else the project's first band.
    #[serde(default)]
    pub band: Option<String>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct CreateEdgeBandInput {
    /// As the shop lists it, e.g. "Fita Branca 1x22".
    pub name: String,
    /// Tape thickness, e.g. 0.45 or 1 mm.
    pub thickness: LengthInput,
    /// Tape height; at least the board thickness, e.g. 22 mm for 18 mm boards.
    pub height: LengthInput,
    /// "#rrggbb" or [r, g, b]. Default white.
    #[serde(default)]
    pub color: Option<ColorInput>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub allow_rounding: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct UpdateEdgeBandInput {
    pub band: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub thickness: Option<LengthInput>,
    #[serde(default)]
    pub height: Option<LengthInput>,
    #[serde(default)]
    pub color: Option<ColorInput>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub allow_rounding: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct EdgeBandRefInput {
    pub band: String,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

/// Banding of one board, per edge: the band, whether it is automatic and
/// what the edge touches.
pub(crate) fn board_banding(project: &Project, id: Uuid) -> Value {
    let Some(states) = banding_rules::board_states(project, id) else {
        return Value::Null;
    };
    let accepts = project
        .board(id)
        .and_then(|b| project.material(b.material_id))
        .is_some_and(|m| m.kind.accepts_banding());
    if !accepts {
        return json!({ "accepted": false });
    }
    let front = BoardFrame::new(project, id)
        .map(|frame| banding_rules::edges_facing(&frame, FRONT))
        .unwrap_or_default();
    let edges: serde_json::Map<String, Value> = BoardEdge::ALL
        .into_iter()
        .map(|edge| {
            let state: EdgeState = states[edge.index()];
            let touching = match state.contact {
                Contact::Free => Value::Null,
                Contact::Partly { board, coverage } | Contact::Joined { board, coverage } => {
                    json!({
                        "board": object_name(project, board),
                        "coverage": (coverage * 100.0).round() / 100.0,
                    })
                }
            };
            (
                edge_name(edge).to_owned(),
                json!({
                    "band": state.band.and_then(|b| project.edge_band(b)).map(|b| b.name.clone()),
                    "setting": match state.setting {
                        EdgeBanding::Auto => "auto",
                        EdgeBanding::On(_) => "on",
                        EdgeBanding::Off => "off",
                    },
                    "joined": state.contact.is_joined(),
                    "touching": touching,
                    "front": front.contains(&edge),
                }),
            )
        })
        .collect();
    json!({ "accepted": true, "edges": edges })
}

fn band_row(project: &Project, band: &EdgeBand, lengths: &[(Uuid, i128)]) -> Value {
    let unit = project.display_unit;
    let (boards, materials) = band_usage(project, band.id);
    let metres = lengths
        .iter()
        .find(|(id, _)| *id == band.id)
        .map_or(0.0, |(_, um)| *um as f64 / 1_000_000.0);
    json!({
        "id": band.id,
        "name": band.name,
        "thickness": LengthOut::new(band.thickness, unit),
        "height": LengthOut::new(band.height, unit),
        "color": color_hex(band.color),
        "default_of_materials": materials.iter().map(|m| object_name(project, *m)).collect::<Vec<_>>(),
        "boards_set_by_hand": boards.len(),
        "banded_metres": (metres * 100.0).round() / 100.0,
    })
}

impl Workspace {
    pub fn list_edge_bands(&self) -> ServiceResult<Value> {
        let project = self.project()?;
        let lengths = band_lengths(project);
        let rows: Vec<_> = project
            .edge_bands
            .iter()
            .map(|band| band_row(project, band, &lengths))
            .collect();
        Ok(json!({ "revision": project.revision, "edge_bands": rows }))
    }

    pub fn create_edge_band(&mut self, input: CreateEdgeBandInput) -> ServiceResult<Change<Value>> {
        let thickness = input
            .thickness
            .positive("thickness", input.allow_rounding)?;
        let height = input.height.positive("height", input.allow_rounding)?;
        let color = input
            .color
            .as_ref()
            .map(ColorInput::resolve)
            .transpose()?
            .unwrap_or(crate::domain::SrgbColor([244, 242, 238]));
        self.change(input.expected_revision, |editor| {
            let id = editor.create_edge_band(&input.name, thickness, height, color)?;
            Ok((
                json!({ "band_id": id }),
                format!("Created edge band '{}'.", input.name.trim()),
            ))
        })
    }

    pub fn update_edge_band(&mut self, input: UpdateEdgeBandInput) -> ServiceResult<Change<Value>> {
        let id = self.resolve(Kind::Band, &input.band)?;
        let mut band = self
            .project()?
            .edge_band(id)
            .cloned()
            .ok_or_else(|| ServiceError::not_found("edge band", id))?;
        if let Some(name) = &input.name {
            band.name.clone_from(name);
        }
        if let Some(t) = &input.thickness {
            band.thickness = t.positive("thickness", input.allow_rounding)?;
        }
        if let Some(h) = &input.height {
            band.height = h.positive("height", input.allow_rounding)?;
        }
        if let Some(color) = &input.color {
            band.color = color.resolve()?;
        }
        let name = band.name.trim().to_owned();
        self.change(input.expected_revision, |editor| {
            editor.update_edge_band(band)?;
            Ok((
                json!({ "band_id": id }),
                format!("Updated edge band '{name}'."),
            ))
        })
    }

    pub fn remove_edge_band(&mut self, input: EdgeBandRefInput) -> ServiceResult<Change<Value>> {
        let id = self.resolve(Kind::Band, &input.band)?;
        let name = object_name(self.project()?, id);
        self.change(input.expected_revision, |editor| {
            editor.remove_edge_band(id)?;
            Ok((
                json!({ "removed": id }),
                format!("Removed edge band '{name}'."),
            ))
        })
    }

    pub fn set_board_banding(
        &mut self,
        input: SetBoardBandingInput,
    ) -> ServiceResult<Change<Value>> {
        let boards = self.resolve_all(Kind::Board, &input.boards)?;
        if boards.is_empty() {
            return Err(ServiceError::invalid("give at least one board").for_field("boards"));
        }
        let project = self.project()?;
        let band = match &input.band {
            Some(reference) => Some(self.resolve(Kind::Band, reference)?),
            None => boards
                .iter()
                .find_map(|id| {
                    project
                        .board(*id)
                        .and_then(|b| project.material(b.material_id))
                        .and_then(|m| m.default_band)
                })
                .or_else(|| project.edge_bands.first().map(|b| b.id)),
        };
        let needs_band = || {
            ServiceError::invalid("there is no edge band to use; create one with create_edge_band")
                .for_field("band")
        };
        let rows = |project: &Project| -> Vec<Value> {
            boards
                .iter()
                .map(|id| {
                    json!({
                        "board": object_name(project, *id),
                        "banding": board_banding(project, *id),
                    })
                })
                .collect()
        };
        match (input.preset, input.value) {
            (Some(_), Some(_)) => Err(ServiceError::invalid(
                "give either a preset, or edges with a value, not both",
            )),
            (Some(preset), None) => {
                let preset = match preset {
                    PresetName::Auto => BandingPreset::Auto,
                    PresetName::None => BandingPreset::None,
                    PresetName::Front => BandingPreset::Front,
                    PresetName::AllFour => BandingPreset::AllFour,
                };
                if matches!(preset, BandingPreset::Front | BandingPreset::AllFour) && band.is_none()
                {
                    return Err(needs_band());
                }
                self.change(input.expected_revision, |editor| {
                    let outcome = editor.apply_banding_preset(&boards, preset, band)?;
                    let project = editor.project();
                    Ok((
                        json!({
                            "boards": rows(project),
                            "skipped": outcome.skipped.iter().map(|id| object_name(project, *id)).collect::<Vec<_>>(),
                        }),
                        format!("Set the banding of {} board(s).", outcome.changed.len()),
                    ))
                })
            }
            (None, Some(value)) => {
                if input.edges.is_empty() {
                    return Err(ServiceError::invalid("name the edges to set").for_field("edges"));
                }
                let value = match value {
                    BandingValue::Auto => EdgeBanding::Auto,
                    BandingValue::Off => EdgeBanding::Off,
                    BandingValue::On => EdgeBanding::On(band.ok_or_else(needs_band)?),
                };
                // Front differs per board, so each board gets its own edges.
                let per_board: Vec<(Uuid, Vec<BoardEdge>)> = boards
                    .iter()
                    .map(|id| {
                        let front = BoardFrame::new(project, *id)
                            .map(|frame| banding_rules::edges_facing(&frame, FRONT))
                            .unwrap_or_default();
                        let mut edges = Vec::new();
                        for name in &input.edges {
                            let chosen: Vec<BoardEdge> = match name {
                                EdgeName::Length1 => vec![BoardEdge::MinY],
                                EdgeName::Length2 => vec![BoardEdge::MaxY],
                                EdgeName::Width1 => vec![BoardEdge::MinX],
                                EdgeName::Width2 => vec![BoardEdge::MaxX],
                                EdgeName::Front => front.clone(),
                                EdgeName::All => BoardEdge::ALL.to_vec(),
                            };
                            for edge in chosen {
                                if !edges.contains(&edge) {
                                    edges.push(edge);
                                }
                            }
                        }
                        (*id, edges)
                    })
                    .collect();
                self.change(input.expected_revision, |editor| {
                    // One undo step for the whole call.
                    let mut skipped = Vec::new();
                    editor.transact(|project| -> Result<(), BandingError> {
                        let mut changed = 0;
                        for (id, edges) in &per_board {
                            let board = project.board(*id).ok_or(BandingError::MissingBoard(*id))?;
                            let accepts = project
                                .material(board.material_id)
                                .is_some_and(|m| m.kind.accepts_banding());
                            if !accepts {
                                skipped.push(*id);
                                continue;
                            }
                            let board = project.board_mut(*id).expect("checked");
                            for edge in edges {
                                board.banding.set(*edge, value);
                            }
                            changed += 1;
                        }
                        if changed == 0 {
                            return Err(BandingError::NotAccepted);
                        }
                        Ok(())
                    })?;
                    let project = editor.project();
                    Ok((
                        json!({
                            "boards": rows(project),
                            "skipped": skipped.iter().map(|id| object_name(project, *id)).collect::<Vec<_>>(),
                        }),
                        format!("Set the banding of {} board(s).", boards.len() - skipped.len()),
                    ))
                })
            }
            (None, None) => Err(ServiceError::invalid(
                "give a preset, or edges with a value (auto, on or off)",
            )),
        }
    }
}
