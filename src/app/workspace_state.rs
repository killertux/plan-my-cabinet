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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Destination {
    Board(Uuid),
    Sheet(Uuid),
    Material(Uuid),
    Installation(Uuid),
    BoardAllocation(Uuid),
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
        self.inspector = self.inspector.filter(|target| match target {
            InspectorTarget::Board(id) => project.boards.iter().any(|b| b.id == *id),
            InspectorTarget::Sheet(id) => project.stock.iter().any(|s| s.id == *id),
            InspectorTarget::Material(id) => project.materials.iter().any(|m| m.id == *id),
            InspectorTarget::Installation(id) => {
                project.hinge_installations.iter().any(|h| h.id == *id)
            }
        });
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
mod tests {
    use super::*;
    use plan_my_cabinet::money::Currency;
    use plan_my_cabinet::reference_fixture;

    #[test]
    fn identity_routing_keeps_sheet_focus_and_scene_selection_independent() {
        let mut project = reference_fixture::project();
        let first = project.boards[0].id;
        let second = project.boards[1].id;
        project.boards[1].name = project.boards[0].name.clone();
        let original = project.clone();
        let material = project.materials[0].id;
        let installation = project.hinge_installations[0].id;
        let sheet = project.stock[0].id;
        let mut selection = Selection::default();
        let mut state = WorkspaceSession::new(&project);

        assert!(state.navigate(&project, &mut selection, Destination::Board(second)));
        assert_eq!(selection.active, Some(second));
        assert_eq!(state.inspector, Some(InspectorTarget::Board(second)));
        assert!(state.navigate(&project, &mut selection, Destination::Sheet(sheet)));
        assert_eq!(selection.ids.len(), 1);
        assert_eq!(selection.active, Some(second));
        assert_eq!(state.inspector, Some(InspectorTarget::Sheet(sheet)));
        assert!(state.navigate(&project, &mut selection, Destination::Material(material)));
        assert_eq!(selection.active, Some(second));
        assert!(state.navigate(
            &project,
            &mut selection,
            Destination::Installation(installation)
        ));
        assert_eq!(
            state.inspector,
            Some(InspectorTarget::Installation(installation))
        );
        assert_eq!(selection.active, Some(second));
        assert!(state.navigate(&project, &mut selection, Destination::Board(first)));
        assert_eq!(selection.ids.len(), 1);
        assert_eq!(selection.active, Some(first));
        assert!(!state.navigate(&project, &mut selection, Destination::Board(Uuid::new_v4())));
        assert_eq!(selection.active, Some(first));
        assert_eq!(project, original);
    }

    #[test]
    fn allocation_route_focuses_actual_piece_or_issue_without_selecting_other_parts() {
        let project = reference_fixture::project();
        let allocated = project.allocations[0].board_id;
        let stock = project.allocations[0].stock_id;
        let unallocated = project
            .boards
            .iter()
            .find(|b| !project.allocations.iter().any(|a| a.board_id == b.id))
            .unwrap()
            .id;
        let mut selection = Selection::default();
        let mut state = WorkspaceSession::new(&project);
        assert!(state.navigate(
            &project,
            &mut selection,
            Destination::BoardAllocation(allocated)
        ));
        assert_eq!(state.focused_sheet, Some(stock));
        assert_eq!(state.allocation_issue, None);
        assert!(state.pending_cut_focus);
        assert_eq!(selection.ids.len(), 1);
        assert!(state.navigate(
            &project,
            &mut selection,
            Destination::BoardAllocation(unallocated)
        ));
        assert_eq!(state.focused_sheet, None);
        assert_eq!(state.allocation_issue, Some(unallocated));
        assert_eq!(selection.ids.len(), 1);

        let mut conflicted = project.clone();
        let mut duplicate = conflicted.allocations[0].clone();
        duplicate.id = Uuid::new_v4();
        duplicate.stock_id = conflicted.stock.last().unwrap().id;
        conflicted.allocations.push(duplicate);
        assert!(state.navigate(
            &conflicted,
            &mut selection,
            Destination::BoardAllocation(allocated)
        ));
        assert_eq!(state.focused_sheet, None);
        assert_eq!(state.allocation_issue, Some(allocated));
        assert_eq!(selection.ids, [allocated].into_iter().collect());
    }

    #[test]
    fn equal_material_and_sheet_names_do_not_alias_or_resurrect_removed_targets() {
        let mut project = reference_fixture::project();
        let first_material = project.materials[0].id;
        let mut other_material = project.materials[0].clone();
        other_material.id = Uuid::new_v4();
        project.materials.push(other_material.clone());
        let first_sheet = project.stock[0].id;
        let mut other_sheet = project.stock[0].clone();
        other_sheet.id = Uuid::new_v4();
        project.stock.push(other_sheet.clone());

        let mut selection = Selection::default();
        let mut state = WorkspaceSession::new(&project);
        assert!(state.navigate(
            &project,
            &mut selection,
            Destination::Material(other_material.id)
        ));
        assert_eq!(
            state.inspector,
            Some(InspectorTarget::Material(other_material.id))
        );
        assert_ne!(
            state.inspector,
            Some(InspectorTarget::Material(first_material))
        );
        assert!(state.navigate(&project, &mut selection, Destination::Sheet(other_sheet.id)));
        assert_eq!(state.focused_sheet, Some(other_sheet.id));
        assert_ne!(state.focused_sheet, Some(first_sheet));
        assert!(selection.ids.is_empty());

        project.stock.retain(|s| s.id != other_sheet.id);
        state.retain_existing(&project);
        assert_eq!(state.focused_sheet, None);
        assert_eq!(state.inspector, None);
        assert!(!state.navigate(&project, &mut selection, Destination::Sheet(other_sheet.id)));
    }

    #[test]
    fn workspace_views_survive_navigation_but_project_replacement_discards_them() {
        let project = reference_fixture::project();
        let mut state = WorkspaceSession::new(&project);
        state.stock.filter = "oak".into();
        state.stock.scroll = 105.0;
        state.stock.left_panel_open = false;
        state.stock.inspector_open = false;
        state.switch(Workspace::Hardware);
        state.switch(Workspace::Stock);
        assert_eq!(state.view(Workspace::Stock).filter, "oak");
        assert_eq!(state.view(Workspace::Stock).scroll, 105.0);
        assert!(!state.view(Workspace::Stock).left_panel_open);
        assert!(!state.view(Workspace::Stock).inspector_open);

        let other = Project::new("other", Currency::Brl);
        let mut selection = Selection::default();
        assert!(!state.navigate(
            &other,
            &mut selection,
            Destination::Board(project.boards[0].id)
        ));
        state.retain_existing(&other);
        assert_eq!(state.active, Workspace::Design);
        assert!(state.stock.filter.is_empty());
        assert_eq!(state.stock.scroll, 0.0);
        assert!(state.stock.left_panel_open);
        assert_eq!(state.inspector, None);
        assert_eq!(state.focused_sheet, None);
    }

    #[test]
    fn design_expansion_is_session_only_pruned_and_reset_on_replacement() {
        let mut project = reference_fixture::project();
        let assembly = project.assemblies[0].id;
        let other = Project::new("other", Currency::Brl);
        let mut state = WorkspaceSession::new(&project);
        assert!(state.design_expanded.contains(&assembly));
        state.design_expanded.remove(&assembly);
        let document = project.clone();
        state.switch(Workspace::Stock);
        state.switch(Workspace::Design);
        assert!(!state.design_expanded.contains(&assembly));
        assert_eq!(project, document);
        state.design_expanded.insert(assembly);
        project.assemblies.retain(|a| a.id != assembly);
        // Pruning only consults assembly identities; no document edit is made by the session.
        state.retain_existing(&project);
        assert!(!state.design_expanded.contains(&assembly));
        state.design_expanded.insert(assembly);
        state.retain_existing(&other);
        assert!(state.design_expanded.is_empty());
    }
}
