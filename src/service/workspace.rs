//! The agent's session: at most one open project, the loaded hinge catalogs,
//! and recent optimizer results. Every tool is a method on [`Workspace`].
use std::collections::VecDeque;
use std::path::PathBuf;

use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use crate::candidate_ranking::Objective;
use crate::catalog_pack::CatalogRegistry;
use crate::commands::ProjectEditor;
use crate::domain::Project;
use crate::i18n::Language;
use crate::optimization_worker::CompletedSearch;
use crate::service::dto::Change;
use crate::service::error::{ErrorCode, ServiceError, ServiceResult};

#[derive(Clone, Debug, Default)]
pub struct WorkspaceConfig {
    /// Where user catalog packs (`*.toml`) live; default `<user data>/catalogs`.
    pub catalog_dir: Option<PathBuf>,
    /// Recent-projects index location; default is the desktop app's.
    pub user_data_dir: Option<PathBuf>,
    pub language: Language,
}

pub struct Document {
    pub editor: ProjectEditor,
    /// `None` until saved once.
    pub path: Option<PathBuf>,
}

pub(crate) struct StoredSearch {
    pub id: Uuid,
    pub objective: Objective,
    pub result: Box<CompletedSearch>,
    pub revision: u64,
}

pub struct Workspace {
    pub(crate) document: Option<Document>,
    pub(crate) catalogs: CatalogRegistry,
    pub(crate) catalog_dir: Option<PathBuf>,
    pub(crate) user_data_dir: Option<PathBuf>,
    pub(crate) language: Language,
    pub(crate) searches: VecDeque<StoredSearch>,
}

/// Kinds of objects a reference can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Board,
    Assembly,
    Hardware,
    /// A board, assembly or hardware item.
    Object,
    Material,
    Stock,
    Hinge,
    Door,
    Catalog,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Self::Board => "board",
            Self::Assembly => "assembly",
            Self::Hardware => "hardware item",
            Self::Object => "board, assembly or hardware item",
            Self::Material => "material",
            Self::Stock => "stock piece",
            Self::Hinge => "hinge",
            Self::Door => "door",
            Self::Catalog => "catalog entry",
        }
    }
}

impl Workspace {
    pub fn new(config: WorkspaceConfig) -> Self {
        let user_data_dir = config
            .user_data_dir
            .or_else(crate::user_dirs::user_data_dir);
        let catalog_dir = config
            .catalog_dir
            .or_else(|| user_data_dir.as_ref().map(|d| d.join("catalogs")));
        Self {
            document: None,
            catalogs: CatalogRegistry::load(catalog_dir.as_deref()),
            catalog_dir,
            user_data_dir,
            language: config.language,
            searches: VecDeque::new(),
        }
    }

    pub fn document(&self) -> ServiceResult<&Document> {
        self.document.as_ref().ok_or_else(no_project)
    }

    pub fn editor(&self) -> ServiceResult<&ProjectEditor> {
        Ok(&self.document()?.editor)
    }

    pub fn editor_mut(&mut self) -> ServiceResult<&mut ProjectEditor> {
        Ok(&mut self.document.as_mut().ok_or_else(no_project)?.editor)
    }

    pub fn project(&self) -> ServiceResult<&Project> {
        Ok(self.editor()?.project())
    }

    /// Run one change and wrap its result in the common envelope. The number
    /// of undo steps is the number of revisions the change added.
    pub fn change<T: Serialize>(
        &mut self,
        expected_revision: Option<u64>,
        edit: impl FnOnce(&mut ProjectEditor) -> ServiceResult<(T, String)>,
    ) -> ServiceResult<Change<T>> {
        let editor = self.editor_mut()?;
        let before = editor.project().revision;
        if let Some(expected) = expected_revision
            && expected != before
        {
            return Err(ServiceError::new(
                ErrorCode::RevisionMismatch,
                format!("the project is at revision {before}, not {expected}"),
            )
            .hint("Read the current state again before changing it.")
            .details(json!({ "expected": expected, "current": before })));
        }
        let (result, summary) = edit(editor)?;
        let after = editor.project().revision;
        Ok(Change {
            committed: true,
            changed: after != before,
            revision: after,
            dirty: editor.is_dirty(),
            undo_steps: (after - before) as usize,
            summary,
            warnings: Vec::new(),
            result,
        })
    }

    /// A dry run: report what would happen without changing anything.
    pub fn preview_only<T: Serialize>(
        &self,
        result: T,
        summary: String,
    ) -> ServiceResult<Change<T>> {
        let editor = self.editor()?;
        Ok(Change {
            committed: false,
            changed: false,
            revision: editor.project().revision,
            dirty: editor.is_dirty(),
            undo_steps: 0,
            summary,
            warnings: Vec::new(),
            result,
        })
    }

    /// Resolve a reference: a UUID, a unique id prefix of 8+ hex digits, a
    /// stock alias ("S1", "O2"), or a unique exact (then case-insensitive) name.
    pub fn resolve(&self, kind: Kind, reference: &str) -> ServiceResult<Uuid> {
        resolve(self.project()?, kind, reference)
    }

    pub fn resolve_all(&self, kind: Kind, references: &[String]) -> ServiceResult<Vec<Uuid>> {
        references.iter().map(|r| self.resolve(kind, r)).collect()
    }
}

fn no_project() -> ServiceError {
    ServiceError::new(ErrorCode::NoOpenProject, "no project is open")
        .hint("Call new_project, open_project or generate_template first.")
}

/// (id, name) candidates of a kind.
pub(crate) fn candidates(project: &Project, kind: Kind) -> Vec<(Uuid, String)> {
    let boards = || project.boards.iter().map(|b| (b.id, b.name.clone()));
    let assemblies = || project.assemblies.iter().map(|a| (a.id, a.name.clone()));
    let hardware = || project.hardware.iter().map(|h| (h.id, h.name.clone()));
    match kind {
        Kind::Board => boards().collect(),
        Kind::Assembly => assemblies().collect(),
        Kind::Hardware => hardware().collect(),
        Kind::Object => boards().chain(assemblies()).chain(hardware()).collect(),
        Kind::Material => project
            .materials
            .iter()
            .map(|m| (m.id, m.name.clone()))
            .collect(),
        Kind::Stock => project
            .stock
            .iter()
            .map(|s| (s.id, s.name.clone()))
            .collect(),
        Kind::Hinge => project
            .hinge_installations
            .iter()
            .map(|h| (h.id, String::new()))
            .collect(),
        // A door is named after the board or assembly that moves.
        Kind::Door => project
            .door_joints
            .iter()
            .map(|j| {
                let name = project
                    .board(j.moving_root_id)
                    .map(|b| b.name.clone())
                    .or_else(|| {
                        project
                            .assemblies
                            .iter()
                            .find(|a| a.id == j.moving_root_id)
                            .map(|a| a.name.clone())
                    })
                    .unwrap_or_default();
                (j.id, name)
            })
            .collect(),
        Kind::Catalog => project
            .catalog
            .iter()
            .map(|c| (c.id, c.name.clone()))
            .collect(),
    }
}

pub(crate) fn resolve(project: &Project, kind: Kind, reference: &str) -> ServiceResult<Uuid> {
    let reference = reference.trim();
    let all = candidates(project, kind);
    if let Ok(id) = Uuid::parse_str(reference) {
        if all.iter().any(|(candidate, _)| *candidate == id) {
            return Ok(id);
        }
        // A door can also be named by its moving board or assembly id.
        if kind == Kind::Door
            && let Some(joint) = project.door_joints.iter().find(|j| j.moving_root_id == id)
        {
            return Ok(joint.id);
        }
        return Err(ServiceError::not_found(kind.label(), reference));
    }
    if kind == Kind::Stock
        && let Some((id, _)) = project
            .stock_aliases
            .iter()
            .find(|(_, alias)| alias.eq_ignore_ascii_case(reference))
    {
        return Ok(*id);
    }
    let unique = |matches: Vec<&(Uuid, String)>| -> Option<ServiceResult<Uuid>> {
        match matches.len() {
            0 => None,
            1 => Some(Ok(matches[0].0)),
            _ => Some(Err(ServiceError::new(
                ErrorCode::AmbiguousRef,
                format!("'{reference}' matches {} {}s", matches.len(), kind.label()),
            )
            .hint("Use the id instead of the name.")
            .details(json!({
                "candidates": matches
                    .iter()
                    .map(|(id, name)| json!({
                        "id": id,
                        "name": project.material(*id).map_or_else(|| name.clone(), material_label),
                        "parent": parent_name(project, *id),
                    }))
                    .collect::<Vec<_>>()
            })))),
        }
    };
    if reference.len() >= 8 && reference.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        let lower = reference.to_ascii_lowercase();
        if let Some(found) = unique(
            all.iter()
                .filter(|(id, _)| id.to_string().starts_with(&lower))
                .collect(),
        ) {
            return found;
        }
    }
    if let Some(found) = unique(all.iter().filter(|(_, name)| name == reference).collect()) {
        return found;
    }
    if let Some(found) = unique(
        all.iter()
            .filter(|(_, name)| !name.is_empty() && name.eq_ignore_ascii_case(reference))
            .collect(),
    ) {
        return found;
    }
    // Materials also answer to "<name> <thickness>" ("White MDF 18", "HDF 3 mm").
    if kind == Kind::Material {
        let labelled: Vec<(Uuid, String)> = project
            .materials
            .iter()
            .map(|m| (m.id, material_label(m)))
            .collect();
        let wanted = reference.to_lowercase().replace("mm", "").replace(',', ".");
        let wanted = wanted.split_whitespace().collect::<Vec<_>>().join(" ");
        if let Some(found) = unique(
            labelled
                .iter()
                .filter(|(_, label)| label.to_lowercase() == wanted)
                .collect(),
        ) {
            return found;
        }
    }
    Err(ServiceError::not_found(kind.label(), reference))
}

/// "White MDF 18" — a material's name with its thickness in millimetres.
pub(crate) fn material_label(material: &crate::domain::Material) -> String {
    let mm = material.default_thickness.micrometres() as f64 / 1000.0;
    format!("{} {mm}", material.name)
}

pub(crate) fn parent_name(project: &Project, id: Uuid) -> Option<String> {
    let parent = project
        .boards
        .iter()
        .find(|b| b.id == id)
        .and_then(|b| b.parent_id)
        .or_else(|| {
            project
                .assemblies
                .iter()
                .find(|a| a.id == id)
                .and_then(|a| a.parent_id)
        })
        .or_else(|| {
            project
                .hardware
                .iter()
                .find(|h| h.id == id)
                .and_then(|h| h.parent_id)
        })?;
    project
        .assemblies
        .iter()
        .find(|a| a.id == parent)
        .map(|a| a.name.clone())
}

/// Display name of any object id.
pub(crate) fn object_name(project: &Project, id: Uuid) -> String {
    project
        .boards
        .iter()
        .find(|b| b.id == id)
        .map(|b| b.name.clone())
        .or_else(|| {
            project
                .assemblies
                .iter()
                .find(|a| a.id == id)
                .map(|a| a.name.clone())
        })
        .or_else(|| {
            project
                .hardware
                .iter()
                .find(|h| h.id == id)
                .map(|h| h.name.clone())
        })
        .or_else(|| {
            project
                .materials
                .iter()
                .find(|m| m.id == id)
                .map(|m| m.name.clone())
        })
        .or_else(|| project.stock_alias(id).map(str::to_owned))
        .unwrap_or_else(|| id.to_string())
}
