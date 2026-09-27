//! Session-only, UI-independent drafts shared by the inspector and selection HUD.
//! A draft is keyed by physical project/board identity; widgets borrow it, never copy it.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use uuid::Uuid;

use crate::board_dimensions::{
    BatchDimensionError, BoardDimension, BoardSelection, DimensionEditError, SelectionValue,
};
use crate::commands::{EditError, ProjectEditor};
use crate::dimension_input::{InputError, Locale, ParsedDimension, format_length, parse_length};
use crate::domain::DomainError;
use crate::placement::{
    CoordinateFrame, NumericPose, PlacementError, PlacementSession, world_pose,
};
use crate::units::{Anchor, Conversion, Length, Pose, Unit, dimension};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DraftError {
    MissingBoard(Uuid),
    Stale,
    InvalidField {
        axis: usize,
        error: InputError,
    },
    RoundingConsent {
        axis: usize,
        entered: String,
        rounded_mm: String,
    },
    Dimension(DimensionEditError),
    Batch(BatchDimensionError),
    Placement(PlacementError),
    InvalidProject(DomainError),
    RevisionExhausted,
    InvalidRotation(usize),
}

fn edit_error<E>(error: EditError<E>, command: impl FnOnce(E) -> DraftError) -> DraftError {
    match error {
        EditError::Command(reason) => command(reason),
        EditError::InvalidProject(reason) => DraftError::InvalidProject(reason),
        EditError::RevisionExhausted => DraftError::RevisionExhausted,
        _ => DraftError::Stale,
    }
}

#[derive(Clone, Debug)]
pub struct LengthField {
    /// Always the exact original, even when its display is rounded.
    pub committed: Length,
    pub text: Option<String>,
    pub unit: Unit,
    pub locale: Locale,
    pub consent: bool,
    /// Captured on first edit. Presentation changes never reinterpret pending text.
    entry: Option<(Unit, Locale)>,
}

impl LengthField {
    pub fn new(committed: Length, unit: Unit, locale: Locale) -> Self {
        Self {
            committed,
            text: None,
            unit,
            locale,
            consent: false,
            entry: None,
        }
    }

    pub fn display(&self) -> String {
        self.text
            .clone()
            .unwrap_or_else(|| format_length(self.committed, self.unit, self.locale, 2))
    }

    pub fn edit(&mut self, text: impl Into<String>) {
        self.entry.get_or_insert((self.unit, self.locale));
        self.text = Some(text.into());
        self.consent = false;
    }

    pub fn set_presentation(&mut self, unit: Unit, locale: Locale) {
        self.unit = unit;
        self.locale = locale;
    }

    pub fn parsed(&self) -> Option<Result<ParsedDimension, InputError>> {
        self.text
            .as_ref()
            .map(|text| parse_length(text, self.entry.map_or(self.unit, |(unit, _)| unit)))
    }

    fn value(&self, axis: usize, positive: bool) -> Result<Length, DraftError> {
        let Some(text) = &self.text else {
            return Ok(self.committed);
        };
        let (entry_unit, entry_locale) = self.entry.unwrap_or((self.unit, self.locale));
        let parsed = parse_length(text, entry_unit)
            .and_then(|parsed| {
                if positive {
                    dimension(parsed.conversion).map_err(InputError::Unit)?;
                }
                Ok(parsed)
            })
            .map_err(|error| DraftError::InvalidField { axis, error })?;
        match parsed.conversion {
            Conversion::Exact(value) => Ok(value),
            Conversion::NeedsConfirmation(value) if self.consent => Ok(value),
            Conversion::NeedsConfirmation(value) => Err(DraftError::RoundingConsent {
                axis,
                entered: text.clone(),
                rounded_mm: format_length(value, Unit::Mm, entry_locale, 3),
            }),
        }
    }

    pub fn cancel(&mut self) {
        self.text = None;
        self.consent = false;
        self.entry = None;
    }
}

#[derive(Clone, Debug)]
pub struct BoardDraft {
    pub project_id: Uuid,
    pub board_id: Uuid,
    pub revision: u64,
    pub length: LengthField,
    pub width: LengthField,
    pub anchor: Anchor,
}

impl BoardDraft {
    pub fn new(
        editor: &ProjectEditor,
        board_id: Uuid,
        unit: Unit,
        locale: Locale,
    ) -> Result<Self, DraftError> {
        let project = editor.project();
        let board = project
            .boards
            .iter()
            .find(|board| board.id == board_id)
            .ok_or(DraftError::MissingBoard(board_id))?;
        Ok(Self {
            project_id: project.id,
            board_id,
            revision: project.revision,
            length: LengthField::new(board.length, unit, locale),
            width: LengthField::new(board.width, unit, locale),
            anchor: Anchor::Centre,
        })
    }

    pub fn dirty(&self) -> bool {
        self.length.text.is_some() || self.width.text.is_some()
    }

    pub fn cancel(&mut self) {
        self.length.cancel();
        self.width.cancel();
    }

    fn check(&self, editor: &ProjectEditor) -> Result<(), DraftError> {
        if editor.project().id != self.project_id || editor.project().revision != self.revision {
            return Err(DraftError::Stale);
        }
        let board = editor
            .project()
            .boards
            .iter()
            .find(|b| b.id == self.board_id)
            .ok_or(DraftError::MissingBoard(self.board_id))?;
        if board.length != self.length.committed || board.width != self.width.committed {
            return Err(DraftError::Stale);
        }
        Ok(())
    }

    pub fn values(&self) -> Result<[Length; 2], DraftError> {
        Ok([self.length.value(0, true)?, self.width.value(1, true)?])
    }

    /// Preflights both axes using the authoritative local-anchor dimension service.
    /// The temporary editor never becomes the document and creates no public history.
    pub fn preview(&self, editor: &ProjectEditor) -> Result<crate::domain::Board, DraftError> {
        self.check(editor)?;
        let values = self.values()?;
        let original = editor
            .project()
            .board(self.board_id)
            .ok_or(DraftError::MissingBoard(self.board_id))?;
        if values == [original.length, original.width] {
            return Ok(original.clone());
        }
        let mut candidate =
            ProjectEditor::new(editor.project().clone()).map_err(DraftError::InvalidProject)?;
        for (axis, value, before) in [
            (BoardDimension::Length, values[0], original.length),
            (BoardDimension::Width, values[1], original.width),
        ] {
            if value == before {
                continue;
            }
            let preview = candidate
                .preview_board_dimension(self.board_id, axis, value, self.anchor)
                .map_err(DraftError::Dimension)?;
            candidate
                .edit_board_dimension(preview)
                .map_err(|e| edit_error(e, DraftError::Dimension))?;
        }
        candidate
            .project()
            .board(self.board_id)
            .cloned()
            .ok_or(DraftError::MissingBoard(self.board_id))
    }

    /// A valid two-axis proposal creates at most one undo record. Failure retains the draft.
    pub fn accept(&mut self, editor: &mut ProjectEditor) -> Result<bool, DraftError> {
        let result = self.preview(editor)?;
        let id = self.board_id;
        let changed = editor
            .transact(|project| {
                *project
                    .boards
                    .iter_mut()
                    .find(|board| board.id == id)
                    .ok_or(DraftError::MissingBoard(id))? = result;
                Ok(())
            })
            .map_err(|e| edit_error(e, |e| e))?;
        self.revision = editor.project().revision;
        let committed = editor
            .project()
            .board(id)
            .ok_or(DraftError::MissingBoard(id))?;
        self.length.committed = committed.length;
        self.width.committed = committed.width;
        self.cancel();
        Ok(changed)
    }
}

#[derive(Default)]
pub struct EditDrafts {
    boards: HashMap<(Uuid, Uuid), BoardDraft>,
    poses: HashMap<(Uuid, Uuid), PoseDraft>,
}

impl EditDrafts {
    /// Look up an existing edit without creating or replacing a stale draft.
    pub fn existing_board(&self, project_id: Uuid, id: Uuid) -> Option<&BoardDraft> {
        self.boards.get(&(project_id, id))
    }

    pub fn existing_board_mut(&mut self, project_id: Uuid, id: Uuid) -> Option<&mut BoardDraft> {
        self.boards.get_mut(&(project_id, id))
    }

    pub fn existing_pose(&self, project_id: Uuid, id: Uuid) -> Option<&PoseDraft> {
        self.poses.get(&(project_id, id))
    }

    pub fn existing_pose_mut(&mut self, project_id: Uuid, id: Uuid) -> Option<&mut PoseDraft> {
        self.poses.get_mut(&(project_id, id))
    }

    pub fn board(
        &mut self,
        editor: &ProjectEditor,
        id: Uuid,
        unit: Unit,
        locale: Locale,
    ) -> Result<&mut BoardDraft, DraftError> {
        let key = (editor.project().id, id);
        // A pristine formatter may refresh from a newer committed revision.
        // Never silently replace pending text, even if its target still exists.
        if self
            .boards
            .get(&key)
            .is_some_and(|draft| !draft.dirty() && draft.check(editor).is_err())
        {
            self.boards.remove(&key);
        }
        let draft = match self.boards.entry(key) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(BoardDraft::new(editor, id, unit, locale)?),
        };
        draft.check(editor)?;
        draft.length.set_presentation(unit, locale);
        draft.width.set_presentation(unit, locale);
        Ok(draft)
    }

    pub fn cancel_board(&mut self, project_id: Uuid, board_id: Uuid) {
        self.boards.remove(&(project_id, board_id));
    }

    pub fn pose(
        &mut self,
        editor: &ProjectEditor,
        id: Uuid,
        frame: CoordinateFrame,
        unit: Unit,
        locale: Locale,
    ) -> Result<&mut PoseDraft, DraftError> {
        let key = (editor.project().id, id);
        if self.poses.get(&key).is_some_and(|draft| {
            !draft.dirty() && (draft.frame != frame || draft.check(editor).is_err())
        }) {
            self.poses.remove(&key);
        }
        let draft = match self.poses.entry(key) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(PoseDraft::new(editor, id, frame, unit, locale)?),
        };
        // Frame changes require a new proposal rather than reinterpreting pending text.
        if draft.frame != frame {
            return Err(DraftError::Stale);
        }
        draft.check(editor)?;
        for field in &mut draft.position {
            field.set_presentation(unit, locale);
        }
        Ok(draft)
    }

    pub fn cancel_pose(&mut self, project_id: Uuid, board_id: Uuid) {
        self.poses.remove(&(project_id, board_id));
    }

    pub fn clear(&mut self) {
        self.boards.clear();
        self.poses.clear();
    }
}

/// A single target dimension over a deduplicated selection. Mixed values have no
/// fabricated common original; only an explicitly entered value is applied.
pub struct BatchDraft {
    pub project_id: Uuid,
    pub revision: u64,
    pub selection: Vec<BoardSelection>,
    pub dimension: BoardDimension,
    pub original: SelectionValue,
    pub text: Option<String>,
    pub unit: Unit,
    pub locale: Locale,
    pub consent: bool,
    pub anchors: Vec<(Uuid, Anchor)>,
    entry: Option<(Unit, Locale)>,
}

impl BatchDraft {
    pub fn new(
        editor: &ProjectEditor,
        selection: Vec<BoardSelection>,
        dimension: BoardDimension,
        unit: Unit,
        locale: Locale,
    ) -> Result<Self, DraftError> {
        let selected = editor
            .selected_boards(&selection)
            .map_err(DraftError::Batch)?;
        Ok(Self {
            project_id: editor.project().id,
            revision: editor.project().revision,
            selection,
            dimension,
            original: selected.dimensions[dimension.axis()],
            text: None,
            unit,
            locale,
            consent: false,
            entry: None,
            anchors: selected
                .board_ids
                .into_iter()
                .map(|id| (id, Anchor::Centre))
                .collect(),
        })
    }

    pub fn edit(&mut self, text: impl Into<String>) {
        self.entry.get_or_insert((self.unit, self.locale));
        self.text = Some(text.into());
        self.consent = false;
    }
    pub fn set_presentation(&mut self, unit: Unit, locale: Locale) {
        self.unit = unit;
        self.locale = locale;
    }
    pub fn cancel(&mut self) {
        self.text = None;
        self.consent = false;
        self.entry = None;
    }
    pub fn display(&self) -> Option<String> {
        self.text.clone().or_else(|| match self.original {
            SelectionValue::Uniform(value) => Some(format_length(value, self.unit, self.locale, 2)),
            SelectionValue::Mixed => None,
        })
    }

    pub fn before_after(
        &self,
        editor: &ProjectEditor,
    ) -> Result<Vec<(Uuid, Length, Length, Anchor)>, DraftError> {
        self.check(editor)?;
        let value = self.value()?;
        self.anchors
            .iter()
            .map(|(id, anchor)| {
                let old = editor
                    .project()
                    .board(*id)
                    .ok_or(DraftError::MissingBoard(*id))?
                    .blank_dimensions()[self.dimension.axis()];
                Ok((*id, old, value.unwrap_or(old), *anchor))
            })
            .collect()
    }

    fn check(&self, editor: &ProjectEditor) -> Result<(), DraftError> {
        if editor.project().id != self.project_id || editor.project().revision != self.revision {
            return Err(DraftError::Stale);
        }
        let selected = editor
            .selected_boards(&self.selection)
            .map_err(DraftError::Batch)?;
        if selected.board_ids != self.anchors.iter().map(|(id, _)| *id).collect::<Vec<_>>() {
            return Err(DraftError::Stale);
        }
        Ok(())
    }

    fn value(&self) -> Result<Option<Length>, DraftError> {
        let Some(text) = &self.text else {
            return Ok(None);
        };
        let (entry_unit, entry_locale) = self.entry.unwrap_or((self.unit, self.locale));
        let parsed = parse_length(text, entry_unit)
            .and_then(|p| {
                dimension(p.conversion).map_err(InputError::Unit)?;
                Ok(p)
            })
            .map_err(|error| DraftError::InvalidField {
                axis: self.dimension.axis(),
                error,
            })?;
        Ok(Some(match parsed.conversion {
            Conversion::Exact(value) => value,
            Conversion::NeedsConfirmation(value) if self.consent => value,
            Conversion::NeedsConfirmation(value) => {
                return Err(DraftError::RoundingConsent {
                    axis: self.dimension.axis(),
                    entered: text.clone(),
                    rounded_mm: format_length(value, Unit::Mm, entry_locale, 3),
                });
            }
        }))
    }

    pub fn accept(&mut self, editor: &mut ProjectEditor) -> Result<bool, DraftError> {
        self.check(editor)?;
        let Some(value) = self.value()? else {
            return Ok(false);
        };
        if self.anchors.iter().all(|(id, _)| {
            editor
                .project()
                .boards
                .iter()
                .any(|b| b.id == *id && b.blank_dimensions()[self.dimension.axis()] == value)
        }) {
            self.cancel();
            return Ok(false);
        }
        let preview = editor
            .preview_batch_board_dimension(&self.selection, self.dimension, value, &self.anchors)
            .map_err(DraftError::Batch)?;
        editor
            .edit_batch_board_dimension(preview)
            .map_err(|e| edit_error(e, DraftError::Batch))?;
        self.revision = editor.project().revision;
        self.original = SelectionValue::Uniform(value);
        self.cancel();
        Ok(true)
    }
}

/// Pose text is a proposal; `preview` and `accept` use the existing placement
/// service, preserving unedited quaternion components exactly.
pub struct PoseDraft {
    pub project_id: Uuid,
    pub revision: u64,
    pub board_id: Uuid,
    pub frame: CoordinateFrame,
    pub original: Pose,
    pub position: [LengthField; 3],
    pub rotation: [Option<String>; 3],
    pub rotation_degrees: [f64; 3],
}

impl PoseDraft {
    pub fn new(
        editor: &ProjectEditor,
        board_id: Uuid,
        frame: CoordinateFrame,
        unit: Unit,
        locale: Locale,
    ) -> Result<Self, DraftError> {
        let board = editor
            .project()
            .boards
            .iter()
            .find(|b| b.id == board_id)
            .ok_or(DraftError::MissingBoard(board_id))?;
        let pose = match frame {
            CoordinateFrame::LocalParent => board.pose,
            CoordinateFrame::World => {
                world_pose(editor.project(), board_id).map_err(DraftError::Placement)?
            }
        };
        let q = pose.rotation;
        let pitch = 2.0 * (q.w * q.y - q.z * q.x);
        let angles = [
            (2.0 * (q.w * q.x + q.y * q.z))
                .atan2(1.0 - 2.0 * (q.x * q.x + q.y * q.y))
                .to_degrees(),
            pitch.clamp(-1.0, 1.0).asin().to_degrees(),
            (2.0 * (q.w * q.z + q.x * q.y))
                .atan2(1.0 - 2.0 * (q.y * q.y + q.z * q.z))
                .to_degrees(),
        ];
        Ok(Self {
            project_id: editor.project().id,
            revision: editor.project().revision,
            board_id,
            frame,
            original: pose,
            position: pose.translation_mm.map(|mm| {
                LengthField::new(
                    Length::from_micrometres((mm * 1000.0).round() as i64),
                    unit,
                    locale,
                )
            }),
            rotation: [None, None, None],
            rotation_degrees: angles,
        })
    }

    pub fn dirty(&self) -> bool {
        self.position.iter().any(|f| f.text.is_some()) || self.rotation.iter().any(Option::is_some)
    }
    pub fn cancel(&mut self) {
        for field in &mut self.position {
            field.cancel();
        }
        self.rotation = [None, None, None];
    }

    fn check(&self, editor: &ProjectEditor) -> Result<Pose, DraftError> {
        if editor.project().id != self.project_id || editor.project().revision != self.revision {
            return Err(DraftError::Stale);
        }
        let current = match self.frame {
            CoordinateFrame::LocalParent => {
                editor
                    .project()
                    .boards
                    .iter()
                    .find(|b| b.id == self.board_id)
                    .ok_or(DraftError::MissingBoard(self.board_id))?
                    .pose
            }
            CoordinateFrame::World => {
                world_pose(editor.project(), self.board_id).map_err(DraftError::Placement)?
            }
        };
        if current != self.original {
            return Err(DraftError::Stale);
        }
        Ok(current)
    }

    fn input(&self, editor: &ProjectEditor) -> Result<(NumericPose, [bool; 3], bool), DraftError> {
        let current = self.check(editor)?;
        let edited = self.position.each_ref().map(|f| f.text.is_some());
        let mut position = current.translation_mm;
        for (axis, value) in position.iter_mut().enumerate() {
            if edited[axis] {
                *value = self.position[axis].value(axis, false)?.micrometres() as f64 / 1000.0;
            }
        }
        let mut angles = self.rotation_degrees;
        for (axis, angle) in angles.iter_mut().enumerate() {
            if let Some(text) = &self.rotation[axis] {
                *angle = text
                    .trim()
                    .replace(',', ".")
                    .parse::<f64>()
                    .map_err(|_| DraftError::InvalidRotation(axis))?;
                if !angle.is_finite() {
                    return Err(DraftError::InvalidRotation(axis));
                }
            }
        }
        Ok((
            NumericPose {
                position_mm: position,
                rotation_degrees_xyz: angles,
                frame: self.frame,
            },
            edited,
            self.rotation.iter().any(Option::is_some),
        ))
    }

    pub fn preview(&self, editor: &ProjectEditor) -> Result<Pose, DraftError> {
        let (input, position_edited, rotation_edited) = self.input(editor)?;
        if !self.dirty() {
            return world_pose(editor.project(), self.board_id).map_err(DraftError::Placement);
        }
        let mut shadow =
            ProjectEditor::new(editor.project().clone()).map_err(DraftError::InvalidProject)?;
        let mut session =
            PlacementSession::begin(&mut shadow, self.board_id).map_err(DraftError::Placement)?;
        session
            .preview_numeric_edited(input, position_edited, rotation_edited)
            .map_err(DraftError::Placement)
    }

    pub fn accept(&mut self, editor: &mut ProjectEditor) -> Result<bool, DraftError> {
        let (input, position_edited, rotation_edited) = self.input(editor)?;
        if !self.dirty() {
            return Ok(false);
        }
        let original_rotation = editor
            .project()
            .boards
            .iter()
            .find(|board| board.id == self.board_id)
            .ok_or(DraftError::MissingBoard(self.board_id))?
            .pose
            .rotation;
        let mut session =
            PlacementSession::begin(editor, self.board_id).map_err(DraftError::Placement)?;
        session
            .preview_numeric_edited(input, position_edited, rotation_edited)
            .map_err(DraftError::Placement)?;
        let changed = if rotation_edited {
            session
                .accept()
                .map_err(|e| edit_error(e, |_| DraftError::Stale))?
        } else {
            // The placement service validates the complete pose, but quaternion
            // composition can renormalize an unedited rotation by a few ulps.
            session.pause();
            let id = self.board_id;
            editor
                .update_preview(|project| {
                    project
                        .boards
                        .iter_mut()
                        .find(|board| board.id == id)
                        .ok_or(DraftError::MissingBoard(id))?
                        .pose
                        .rotation = original_rotation;
                    Ok(())
                })
                .map_err(|e| {
                    editor.cancel_preview();
                    edit_error(e, |e| e)
                })?;
            let result = editor.commit_preview();
            if result.is_err() {
                editor.cancel_preview();
            }
            result.map_err(|e| edit_error(e, |_| DraftError::Stale))?
        };
        *self = Self::new(
            editor,
            self.board_id,
            self.frame,
            self.position[0].unit,
            self.position[0].locale,
        )?;
        Ok(changed)
    }
}
