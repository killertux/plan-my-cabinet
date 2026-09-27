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
