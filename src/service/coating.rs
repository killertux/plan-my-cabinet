//! Coating tools: a material's coating (none, one side, both sides) and,
//! on one-side coated materials, which broad face of a board is coated.
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::coating::{CoatedFaceEdit, CoatingError};
use crate::coating_rules::{self, Coated, Facing};
use crate::domain::{BoardFace, Coating, Project};
use crate::service::dto::Change;
use crate::service::error::{ErrorCode, ServiceError, ServiceResult};
use crate::service::workspace::{Kind, Workspace, object_name};

impl From<CoatingError> for ServiceError {
    fn from(error: CoatingError) -> Self {
        match error {
            CoatingError::MissingBoard(id) => Self::not_found("board", id),
            CoatingError::MissingMaterial(id) => Self::not_found("material", id),
            CoatingError::NotOneSided => Self::new(
                ErrorCode::InvalidArgument,
                "none of these boards is on a material coated on one side",
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CoatingName {
    /// Raw on both faces ("MDF cru").
    None,
    /// Coated on one face ("1 face").
    OneSide,
    BothSides,
}

impl From<CoatingName> for Coating {
    fn from(name: CoatingName) -> Self {
        match name {
            CoatingName::None => Self::None,
            CoatingName::OneSide => Self::OneSide,
            CoatingName::BothSides => Self::BothSides,
        }
    }
}

pub(crate) fn coating_name(coating: Coating) -> &'static str {
    match coating {
        Coating::None => "none",
        Coating::OneSide => "one_side",
        Coating::BothSides => "both_sides",
    }
}

fn face_name(face: BoardFace) -> &'static str {
    match face {
        BoardFace::MinZ => "min_z",
        BoardFace::MaxZ => "max_z",
    }
}

/// A board's coating, for board rows.
pub(crate) fn board_coating(project: &Project, id: Uuid) -> Value {
    let Some(state) = coating_rules::board_state(project, id) else {
        return Value::Null;
    };
    let coated = match state.coated {
        Coated::Raw => "none",
        Coated::Both => "both",
        Coated::One(face) => face_name(face),
    };
    json!({
        "coated": coated,
        "face_choosable": state.choosable,
        "automatic": state.automatic.map(|facing| match facing {
            Facing::Front => "facing front",
            Facing::Up => "facing up",
            Facing::Outside => "facing outside the cabinet",
            Facing::Right => "facing +X (no cabinet)",
        }),
    })
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CoatedFaceName {
    /// Back to the automatic face (front, up, or outside the cabinet).
    Auto,
    MinZ,
    MaxZ,
    /// The other face from the one coated now.
    Flip,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct SetBoardCoatedFaceInput {
    /// Boards by name or id.
    pub boards: Vec<String>,
    pub face: CoatedFaceName,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

impl Workspace {
    pub fn set_board_coated_face(
        &mut self,
        input: SetBoardCoatedFaceInput,
    ) -> ServiceResult<Change<Value>> {
        let boards = self.resolve_all(Kind::Board, &input.boards)?;
        if boards.is_empty() {
            return Err(ServiceError::invalid("give at least one board").for_field("boards"));
        }
        let edit = match input.face {
            CoatedFaceName::Auto => CoatedFaceEdit::Auto,
            CoatedFaceName::MinZ => CoatedFaceEdit::Face(BoardFace::MinZ),
            CoatedFaceName::MaxZ => CoatedFaceEdit::Face(BoardFace::MaxZ),
            CoatedFaceName::Flip => CoatedFaceEdit::Flip,
        };
        self.change(input.expected_revision, |editor| {
            let outcome = editor.set_coated_face(&boards, edit)?;
            let project = editor.project();
            let rows: Vec<Value> = boards
                .iter()
                .map(|id| {
                    json!({
                        "board": object_name(project, *id),
                        "coating": board_coating(project, *id),
                    })
                })
                .collect();
            let skipped: Vec<String> = outcome
                .skipped
                .iter()
                .map(|id| object_name(project, *id))
                .collect();
            Ok((
                json!({ "boards": rows, "skipped_not_one_sided": skipped }),
                format!("Set the coated face of {} board(s).", outcome.changed.len()),
            ))
        })
    }
}
