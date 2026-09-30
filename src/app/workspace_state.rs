//! Project-session navigation state. Object selection remains in the scene;
//! inspector and sheet focus are independent, stable-ID destinations.
use crate::viewport::{Camera, Selection};
use plan_my_cabinet::domain::Project;
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Workspace {
    #[default]
    Design,
    Stock,
    CutPlan,
    Hardware,
    Handoff,
}

impl Workspace {
    pub const fn number(self) -> u8 {
        match self {
            Self::Design => 1,
            Self::Stock => 2,
            Self::CutPlan => 3,
            Self::Hardware => 4,
            Self::Handoff => 5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InspectorTarget {
    Board(Uuid),
    Sheet(Uuid),
    Material(Uuid),
    Installation(Uuid),
    /// A drawer's slide pair.
    Slide(Uuid),
    /// A foot or other hardware item.
    Hardware(Uuid),
    /// A door relationship.
    Door(Uuid),
    /// A pinned catalog model.
    Catalog(Uuid),
}

impl InspectorTarget {
    /// Whether the target still exists in `project`.
    pub(crate) fn exists(self, project: &Project) -> bool {
        match self {
            Self::Board(id) => project.boards.iter().any(|b| b.id == id),
            Self::Sheet(id) => project.stock.iter().any(|s| s.id == id),
            Self::Material(id) => project.materials.iter().any(|m| m.id == id),
            Self::Installation(id) => project.hinge_installations.iter().any(|h| h.id == id),
            Self::Slide(id) => project.slide_installations.iter().any(|s| s.id == id),
            Self::Hardware(id) => project.hardware.iter().any(|h| h.id == id),
            Self::Door(id) => project.door_joints.iter().any(|j| j.id == id),
            Self::Catalog(id) => project.catalog.iter().any(|c| c.id == id),
        }
    }

    /// Hardware inspectors (shown in both Design and Hardware).
    pub(crate) fn is_fitting(self) -> bool {
        matches!(
            self,
            Self::Installation(_)
                | Self::Slide(_)
                | Self::Hardware(_)
                | Self::Door(_)
                | Self::Catalog(_)
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Destination {
    Board(Uuid),
    Sheet(Uuid),
    Material(Uuid),
    Installation(Uuid),
    BoardAllocation(Uuid),
    /// Any hardware inspector, opened in the Hardware workspace.
    Fitting(InspectorTarget),
}

/// These values belong to this project's UI session, never to its document.
/// Workspaces get separate cameras, scroll offsets, filters and panel layout.
#[allow(dead_code)] // Scroll/filter/panels are bound when each workspace surface arrives.
pub(crate) struct ViewState {
    pub camera: Camera,
    pub scroll: f32,
    pub filter: String,
    pub left_panel_open: bool,
    pub inspector_open: bool,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            camera: Camera::default(),
            scroll: 0.0,
            filter: String::new(),
            left_panel_open: true,
            inspector_open: true,
        }
    }
}

pub(crate) struct WorkspaceSession {
    project_id: Uuid,
    pub active: Workspace,
    pub design: ViewState,
    /// Expanded assembly IDs; neither visibility nor selection changes this set.
    pub design_expanded: HashSet<Uuid>,
    pub stock: ViewState,
    /// Material-filtered Stock table state; neither choice edits the document.
    pub stock_material_filter: Option<Uuid>,
    pub stock_global_order: bool,
    pub stock_piece: Option<Uuid>,
    pub stock_drag: Option<Uuid>,
    pub cut_plan: ViewState,
    pub hardware: ViewState,
    pub handoff: ViewState,
    pub inspector: Option<InspectorTarget>,
    pub focused_sheet: Option<Uuid>,
    pub allocation_issue: Option<Uuid>,
    /// One-shot scroll request; persistent focus remains when the user scrolls.
    pub pending_cut_focus: bool,
}

impl WorkspaceSession {
    pub fn belongs_to(&self, project: &Project) -> bool {
        self.project_id == project.id
    }
    pub fn new(project: &Project) -> Self {
        Self {
            project_id: project.id,
            active: Workspace::Design,
            design: ViewState::default(),
            design_expanded: project.assemblies.iter().map(|a| a.id).collect(),
            stock: ViewState::default(),
            stock_material_filter: None,
            stock_global_order: false,
            stock_piece: None,
            stock_drag: None,
            cut_plan: ViewState::default(),
            hardware: ViewState::default(),
            handoff: ViewState::default(),
            inspector: None,
            focused_sheet: None,
            allocation_issue: None,
            pending_cut_focus: false,
        }
    }

    #[allow(dead_code)] // The rail and workspace surfaces follow this foundation.
    pub fn view(&self, workspace: Workspace) -> &ViewState {
        match workspace {
            Workspace::Design => &self.design,
            Workspace::Stock => &self.stock,
            Workspace::CutPlan => &self.cut_plan,
            Workspace::Hardware => &self.hardware,
            Workspace::Handoff => &self.handoff,
        }
    }

    #[allow(dead_code)] // Workspace surfaces bind their controls in a later task.
    pub fn view_mut(&mut self, workspace: Workspace) -> &mut ViewState {
        match workspace {
            Workspace::Design => &mut self.design,
            Workspace::Stock => &mut self.stock,
            Workspace::CutPlan => &mut self.cut_plan,
            Workspace::Hardware => &mut self.hardware,
            Workspace::Handoff => &mut self.handoff,
        }
    }

    #[allow(dead_code)] // The rail and workspace surfaces follow this foundation.
    pub fn switch(&mut self, workspace: Workspace) {
        self.active = workspace;
    }

    /// Reject destinations from an earlier project or removed entities. Names
    /// deliberately play no part in validation, including for equal-name rows.
    pub fn navigate(
        &mut self,
        project: &Project,
        selection: &mut Selection,
        destination: Destination,
    ) -> bool {
        if self.project_id != project.id {
            return false;
        }
        match destination {
            Destination::Board(id) if project.boards.iter().any(|b| b.id == id) => {
                selection.choose(Some(id), false);
                self.inspector = Some(InspectorTarget::Board(id));
                self.active = Workspace::Design;
            }
            Destination::Sheet(id) if project.stock.iter().any(|s| s.id == id) => {
                self.focused_sheet = Some(id);
                self.allocation_issue = None;
                self.pending_cut_focus = true;
                self.inspector = Some(InspectorTarget::Sheet(id));
                self.active = Workspace::CutPlan;
            }
            Destination::Material(id) if project.materials.iter().any(|m| m.id == id) => {
                self.inspector = Some(InspectorTarget::Material(id));
                self.active = Workspace::Stock;
            }
            Destination::Installation(id)
                if project.hinge_installations.iter().any(|h| h.id == id) =>
            {
                self.inspector = Some(InspectorTarget::Installation(id));
                self.active = Workspace::Hardware;
            }
            Destination::Fitting(target) if target.is_fitting() && target.exists(project) => {
                if let InspectorTarget::Hardware(id) = target {
                    selection.choose(Some(id), false);
                }
                self.inspector = Some(target);
                self.active = Workspace::Hardware;
            }
            Destination::BoardAllocation(id) if project.boards.iter().any(|b| b.id == id) => {
                selection.choose(Some(id), false);
                self.inspector = Some(InspectorTarget::Board(id));
                let mut allocations = project.allocations.iter().filter(|a| a.board_id == id);
                self.focused_sheet = match (allocations.next(), allocations.next()) {
                    (Some(allocation), None)
                        if project
                            .stock
                            .iter()
                            .any(|stock| stock.id == allocation.stock_id) =>
                    {
                        Some(allocation.stock_id)
                    }
                    // A multiply allocated board is an issue, not an arbitrary
                    // first sheet; don't represent it as a unique placement.
                    _ => None,
                };
                self.allocation_issue = self.focused_sheet.is_none().then_some(id);
                self.pending_cut_focus = true;
                self.active = Workspace::CutPlan;
            }
            _ => return false,
        }
        true
    }

    /// Undo, deletion, and loaded revisions can remove targets independently of
    /// navigation. Do not retarget by display name or first available object.
    pub fn retain_existing(&mut self, project: &Project) {
        if self.project_id != project.id {
            *self = Self::new(project);
            return;
        }
        self.design_expanded
            .retain(|id| project.assemblies.iter().any(|a| a.id == *id));
        self.inspector = self.inspector.filter(|target| target.exists(project));
        self.focused_sheet = self
            .focused_sheet
            .filter(|id| project.stock.iter().any(|s| s.id == *id));
        self.stock_material_filter = self
            .stock_material_filter
            .filter(|id| project.materials.iter().any(|material| material.id == *id));
        self.stock_piece = self
            .stock_piece
            .filter(|id| project.stock.iter().any(|piece| piece.id == *id));
        self.stock_drag = self
            .stock_drag
            .filter(|id| project.stock.iter().any(|piece| piece.id == *id));
        self.allocation_issue = self
            .allocation_issue
            .filter(|id| project.boards.iter().any(|b| b.id == *id));
    }
}

#[cfg(test)]
mod tests;
