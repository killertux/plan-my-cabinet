//! Drawer slides in the Hardware workspace: the list, the add/edit dialog
//! with live checks, and the inspector with gaps and hole positions.
#[cfg(test)]
use crate::actions::Argument;
use crate::actions::{ActionId as A, Request, Target};
use crate::icons::Icon;
use crate::theme_widgets as tw;
use crate::*;
use plan_my_cabinet::domain::{CatalogReference, SlideInstallation};
use plan_my_cabinet::slide_installation::{self, SlideStatus, Suggestion};

/// Which slide family the dialog uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FamilyKey {
    pub pack: String,
    pub family: String,
}

pub(crate) struct SlideDialog {
    /// The installation being edited.
    editing: Option<Uuid>,
    project_id: Uuid,
    revision: u64,
    drawer: Option<Uuid>,
    family: Option<FamilyKey>,
    /// `None`: the longest that fits.
    length: Option<Length>,
    height: DimensionDraft,
    setback: DimensionDraft,
    error: Option<String>,
    chrome: ModalChrome,
}

fn mm_text(length: Length) -> String {
    let um = length.micrometres();
    if um % 1000 == 0 {
        (um / 1000).to_string()
    } else {
        format!("{}", um as f64 / 1000.0)
    }
}

fn draft(text: String) -> DimensionDraft {
    DimensionDraft {
        text,
        consent: false,
    }
}

/// Slide families: from the loaded packs, then any pinned in the project.
fn families(app: &DesktopApp) -> Vec<(FamilyKey, String)> {
    let language = app.localizer.language().tag();
    let mut out: Vec<(FamilyKey, String)> = app
        .hardware
        .catalogs
        .usable()
        .iter()
        .flat_map(|p| {
            p.slides.iter().map(|f| {
                (
                    FamilyKey {
                        pack: p.id.clone(),
                        family: f.id.clone(),
                    },
                    f.name(language).to_owned(),
                )
            })
        })
        .collect();
    for entry in &app.editor.project().catalog {
        if let (Some(spec), Some(origin)) = (entry.slide(), &entry.origin) {
            let key = FamilyKey {
                pack: origin.pack_id.clone(),
                family: origin.item_id.clone(),
            };
            if !out.iter().any(|(k, _)| *k == key) {
                out.push((key, spec.family.clone()));
            }
        }
    }
    out
}

/// Every length of a family as snapshots (pinned siblings when the pack is
/// not loaded).
fn lengths(app: &DesktopApp, key: &FamilyKey) -> Vec<CatalogReference> {
    let language = app.localizer.language().tag();
    let from_packs = app
        .hardware
        .catalogs
        .slide_lengths(&key.pack, &key.family, language);
    if !from_packs.is_empty() {
        return from_packs;
    }
    app.editor
        .project()
        .catalog
        .iter()
        .filter(|c| {
            c.slide().is_some()
                && c.origin
                    .as_ref()
                    .is_some_and(|o| o.pack_id == key.pack && o.item_id == key.family)
        })
        .cloned()
        .collect()
}

impl SlideDialog {
    pub(crate) fn new(app: &DesktopApp, editing: Option<Uuid>) -> Self {
        let project = app.editor.project();
        let current =
            editing.and_then(|id| project.slide_installations.iter().find(|s| s.id == id));
        let entry = current.and_then(|s| project.catalog.iter().find(|c| c.id == s.catalog_id));
        let family = entry
            .and_then(|e| e.origin.as_ref())
            .map(|o| FamilyKey {
                pack: o.pack_id.clone(),
                family: o.item_id.clone(),
            })
            .or_else(|| {
                let (pack, family) = plan_my_cabinet::hardware_catalog::DEFAULT_SLIDE_FAMILY;
                Some(FamilyKey {
                    pack: pack.into(),
                    family: family.into(),
                })
            });
        // A selected board or assembly suggests the drawer.
        let drawer = current.map(|s| s.drawer_root_id).or_else(|| {
            app.selection
                .active
                .and_then(|id| slide_installation::drawer_root(project, id))
        });
        Self {
            editing,
            project_id: project.id,
            revision: project.revision,
            drawer,
            family,
            length: entry.and_then(CatalogReference::slide).map(|s| s.length),
            height: draft(current.map_or(String::new(), |s| mm_text(s.height))),
            setback: draft(current.map_or(String::new(), |s| mm_text(s.setback))),
            error: None,
            chrome: ModalChrome::new(egui::Id::new("slide-dialog")).width(560.0),
        }
    }

    fn optional(field: &DimensionDraft) -> Result<Option<Length>, ()> {
        if field.text.trim().is_empty() {
            return Ok(None);
        }
        match parse_length(&field.text, Unit::Mm) {
            Ok(value) => match value.conversion {
                Conversion::Exact(v) if v.micrometres() >= 0 => Ok(Some(v)),
                Conversion::NeedsConfirmation(v) if field.consent && v.micrometres() >= 0 => {
                    Ok(Some(v))
                }
                _ => Err(()),
            },
            Err(_) => Err(()),
        }
    }

    /// What would be installed, checked against the current project.
    fn proposal(&self, app: &DesktopApp) -> Result<(Suggestion, SlideInstallation), String> {
        let l = &app.localizer;
        let project = app.editor.project();
        let drawer = self.drawer.ok_or_else(|| l.text("slide-choose-drawer"))?;
        let family = self
            .family
            .as_ref()
            .ok_or_else(|| l.text("slide-choose-family"))?;
        let mut lengths = lengths(app, family);
        if let Some(length) = self.length {
            lengths.retain(|c| c.slide().is_some_and(|s| s.length == length));
        }
        let detected = slide_installation::detect(project, drawer).map_err(|e| {
            l.text(match e {
                slide_installation::SlideFitError::NoDrawerAssembly => "slide-no-drawer-assembly",
                slide_installation::SlideFitError::NoSides => "slide-no-sides",
                _ => "slide-issue-not-parallel",
            })
        })?;
        let suggestion = match slide_installation::suggest(project, detected.clone(), &lengths) {
            Ok(s) => s,
            Err(_) if lengths.len() == 1 => Suggestion {
                catalog: lengths[0].clone(),
                height: Length::from_micrometres(
                    detected.geometry[0]
                        .box_height
                        .min(detected.geometry[1].box_height)
                        .micrometres()
                        / 2,
                ),
                setback: lengths[0].slide().map_or(Length::ZERO, |s| s.front_setback),
                status: SlideStatus {
                    id: Uuid::nil(),
                    issues: Vec::new(),
                    notices: Vec::new(),
                    references: None,
                },
                detected,
            },
            Err(_) => return Err(l.text("slide-no-length-fits")),
        };
        let height = Self::optional(&self.height).map_err(|()| l.text("slide-invalid-length"))?;
        let setback = Self::optional(&self.setback).map_err(|()| l.text("slide-invalid-length"))?;
        let pinned = project.catalog.iter().find(|c| {
            c.product_id == suggestion.catalog.product_id
                && c.item == suggestion.catalog.item
                && c.origin == suggestion.catalog.origin
        });
        let installation = SlideInstallation {
            id: self.editing.unwrap_or_else(Uuid::new_v4),
            catalog_id: pinned.map_or(suggestion.catalog.id, |c| c.id),
            drawer_root_id: suggestion.detected.drawer_root,
            drawer_sides: suggestion.detected.drawer_sides,
            cabinet_sides: suggestion.detected.cabinet_sides,
            sides: suggestion.detected.sides(),
            height: height.unwrap_or(suggestion.height),
            setback: setback.unwrap_or(suggestion.setback),
        };
        Ok((suggestion, installation))
    }
}

fn issue_lines(l: &Localizer, status: &SlideStatus) -> Vec<String> {
    status.issues.iter().map(|i| l.text(i.key())).collect()
}

impl DesktopApp {
    pub(crate) fn show_slide_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.modals.take_slide() else {
            return;
        };
        let fresh = draft.project_id == self.editor.project().id
            && draft.revision == self.editor.project().revision;
        let title = self.localizer.text(if draft.editing.is_some() {
            "slide-edit"
        } else {
            "slide-new"
        });
        let proposal = draft.proposal(self);
        let families = families(self);
        let family_lengths: Vec<Length> = draft
            .family
            .as_ref()
            .map(|f| lengths(self, f))
            .unwrap_or_default()
            .iter()
            .filter_map(|c| c.slide().map(|s| s.length))
            .collect();
        let drawers: Vec<(Uuid, String)> = self
            .editor
            .project()
            .assemblies
            .iter()
            .map(|a| (a.id, a.name.clone()))
            .collect();
        let mut chrome = draft.chrome.detach();
        let l = &self.localizer;
        let project = self.editor.project();
        let result = chrome.show(
            ctx,
            &title,
            ModalActions {
                cancel: &l.text("cancel"),
                confirm: &title,
            },
            |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                hinge_ui::field_label(ui, &l.text("slide-drawer"));
                egui::ComboBox::from_id_salt("slide-dialog-drawer")
                    .width(ui.available_width())
                    .selected_text(
                        draft
                            .drawer
                            .and_then(|id| drawers.iter().find(|(d, _)| *d == id))
                            .map_or_else(|| l.text("slide-choose-drawer"), |(_, n)| n.clone()),
                    )
                    .show_ui(ui, |ui| {
                        for (id, name) in &drawers {
                            combo_option(ui, &mut draft.drawer, Some(*id), name);
                        }
                    });
                ui.add_space(8.0);
                hinge_ui::field_label(ui, &l.text("slide-family"));
                egui::ComboBox::from_id_salt("slide-dialog-family")
                    .width(ui.available_width())
                    .selected_text(
                        draft
                            .family
                            .as_ref()
                            .and_then(|k| families.iter().find(|(f, _)| f == k))
                            .map_or_else(|| l.text("slide-choose-family"), |(_, n)| n.clone()),
                    )
                    .show_ui(ui, |ui| {
                        for (key, name) in &families {
                            if combo_option(ui, &mut draft.family, Some(key.clone()), name)
                                .clicked()
                            {
                                draft.length = None;
                            }
                        }
                    });
                ui.add_space(8.0);
                let gap = 12.0;
                let third = ((ui.available_width() - 2.0 * gap) / 3.0).max(80.0);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    ui.allocate_ui_with_layout(
                        egui::vec2(third, 52.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            hinge_ui::field_label(ui, &l.text("slide-length"));
                            egui::ComboBox::from_id_salt("slide-dialog-length")
                                .width(third)
                                .selected_text(draft.length.map_or_else(
                                    || l.text("slide-longest-that-fits"),
                                    |v| format!("{} mm", mm_text(v)),
                                ))
                                .show_ui(ui, |ui| {
                                    combo_option(
                                        ui,
                                        &mut draft.length,
                                        None,
                                        l.text("slide-longest-that-fits"),
                                    );
                                    for length in &family_lengths {
                                        combo_option(
                                            ui,
                                            &mut draft.length,
                                            Some(*length),
                                            format!("{} mm", mm_text(*length)),
                                        );
                                    }
                                });
                        },
                    );
                    for (key, field, hint) in [
                        ("slide-height", &mut draft.height, "slide-height-hint"),
                        ("slide-setback", &mut draft.setback, "slide-setback-hint"),
                    ] {
                        ui.allocate_ui_with_layout(
                            egui::vec2(third, 52.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                hinge_ui::field_label(ui, &l.text(key));
                                ui.add(
                                    egui::TextEdit::singleline(&mut field.text)
                                        .hint_text(l.text(hint))
                                        .desired_width(third),
                                );
                            },
                        );
                    }
                });
                ui.add_space(10.0);
                let mut valid = fresh;
                match &proposal {
                    Ok((suggestion, installation)) => {
                        let status = if project
                            .catalog
                            .iter()
                            .any(|c| c.id == installation.catalog_id)
                        {
                            slide_installation::diagnose(project, installation)
                        } else {
                            let mut scratch = project.clone();
                            scratch.catalog.push(suggestion.catalog.clone());
                            scratch
                                .slide_installations
                                .retain(|s| s.id != installation.id);
                            slide_installation::diagnose(&scratch, installation)
                        };
                        let d = &suggestion.detected;
                        let name = |id| {
                            project
                                .board(id)
                                .map_or_else(String::new, |b| b.name.clone())
                        };
                        ui.label(
                            egui::RichText::new(format!(
                                "{} · {}",
                                suggestion.catalog.product_id, suggestion.catalog.name
                            ))
                            .size(12.5)
                            .color(tw::TEXT),
                        );
                        for side in 0..2 {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} ↔ {} · {} {} mm",
                                    name(d.drawer_sides[side]),
                                    name(d.cabinet_sides[side]),
                                    l.text("slide-gap"),
                                    mm_text(d.geometry[side].gap)
                                ))
                                .size(11.5)
                                .color(tw::MUTED),
                            );
                        }
                        if status.issues.is_empty() {
                            ui.label(
                                egui::RichText::new(l.text("slide-fits"))
                                    .size(11.5)
                                    .color(tw::OK),
                            );
                        } else {
                            for line in issue_lines(l, &status) {
                                ui.label(egui::RichText::new(line).size(11.5).color(tw::WARN_INK));
                            }
                        }
                    }
                    Err(message) => {
                        valid = false;
                        ui.label(egui::RichText::new(message).size(11.5).color(tw::WARN_INK));
                    }
                }
                if let Some(error) = &draft.error {
                    ui.label(egui::RichText::new(error).size(11.5).color(tw::DANGER));
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(l.text("slide-reference-note"))
                        .size(11.0)
                        .color(tw::FAINT),
                );
                ((), valid)
            },
        );
        if actions::decision(A::CancelDialog, result.action == ModalAction::Cancel) {
            chrome.close(ctx);
            return;
        }
        if actions::decision(A::ConfirmDialog, result.action == ModalAction::Confirm)
            && let Ok((suggestion, installation)) = proposal
        {
            let pinned = self
                .editor
                .project()
                .catalog
                .iter()
                .any(|c| c.id == installation.catalog_id);
            let result = match (draft.editing, pinned) {
                (None, true) => slide_installation::create(&mut self.editor, installation.clone()),
                (None, false) => slide_installation::create_with_catalog(
                    &mut self.editor,
                    suggestion.catalog,
                    installation.clone(),
                ),
                (Some(_), true) => {
                    slide_installation::update(&mut self.editor, installation.clone())
                }
                (Some(_), false) => {
                    let catalog = suggestion.catalog;
                    self.editor.transact(|p| {
                        p.catalog.push(catalog);
                        let slot = p
                            .slide_installations
                            .iter_mut()
                            .find(|s| s.id == installation.id)
                            .ok_or(slide_installation::SlideEditError::MissingInstallation)?;
                        *slot = installation.clone();
                        Ok(())
                    })
                }
            };
            match result {
                Ok(_) => {
                    self.session.inspector = Some(InspectorTarget::Slide(installation.id));
                    chrome.close(ctx);
                    return;
                }
                Err(_) => draft.error = Some(self.localizer.text("slide-save-failed")),
            }
        }
        draft.chrome = chrome;
        self.modals.set_slide(Some(draft));
    }

    /// The "Drawer slides" section of the Hardware panel.
    pub(crate) fn show_slide_list(&mut self, ui: &mut egui::Ui) {
        let project = self.editor.project();
        let rows: Vec<(Uuid, String, String, bool)> = project
            .slide_installations
            .iter()
            .map(|s| {
                let status = slide_installation::diagnose(project, s);
                let name = project
                    .assemblies
                    .iter()
                    .find(|a| a.id == s.drawer_root_id)
                    .map_or_else(|| s.drawer_root_id.to_string(), |a| a.name.clone());
                let code = project
                    .catalog
                    .iter()
                    .find(|c| c.id == s.catalog_id)
                    .map_or_else(String::new, |c| c.product_id.clone());
                (s.id, name, code, status.issues.is_empty())
            })
            .collect();
        if rows.is_empty() {
            return;
        }
        let modal = self.modal_open();
        let mut run = None;
        let mut inspect = None;
        tw::divider(ui);
        let open = egui::Frame::new()
            .inner_margin(egui::Margin {
                left: 14,
                right: 10,
                top: 0,
                bottom: 0,
            })
            .show(ui, |ui| {
                let (open, ()) = tw::collapsible_section_bar(
                    ui,
                    egui::Id::new("hardware-slides-section"),
                    &self.localizer.text("slide-list"),
                    rows.len(),
                    |ui| {
                        let request = Request::new(A::NewSlides);
                        if tw::ghost_icon_sized(
                            ui,
                            Icon::Plus,
                            &A::NewSlides.label(&self.localizer),
                            tw::MUTED,
                            15.0,
                            24.0,
                            self.action_availability(request).is_ok(),
                            false,
                        )
                        .clicked()
                        {
                            run = Some(request);
                        }
                    },
                );
                open
            })
            .inner;
        if open {
            egui::Frame::new()
                .inner_margin(egui::Margin {
                    left: 6,
                    right: 6,
                    top: 0,
                    bottom: 12,
                })
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    for (id, name, code, ok) in &rows {
                        let active = self.session.inspector == Some(InspectorTarget::Slide(*id));
                        let edit = Request::with(A::EditSlides, Target::Slide(*id));
                        let delete = Request::with(A::DeleteSlides, Target::Slide(*id));
                        let motion = Request::with(A::StartMotion, Target::Slide(*id));
                        let (response, ()) = tw::list_row(
                            ui,
                            egui::Id::new(("hardware-slide-row", *id)),
                            28.0,
                            if active {
                                tw::RowState::Active
                            } else {
                                tw::RowState::Normal
                            },
                            !modal,
                            name,
                            |ui| {
                                let hovered = ui.rect_contains_pointer(ui.max_rect());
                                ui.spacing_mut().item_spacing.x = 7.0;
                                ui.add_space(2.0);
                                ui.add(crate::icons::icon(
                                    Icon::Cube,
                                    if active { tw::ACCENT } else { tw::MUTED },
                                    14.0,
                                ));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.spacing_mut().item_spacing.x = 2.0;
                                        if !ok {
                                            ui.add(crate::icons::icon(
                                                Icon::Warning,
                                                tw::WARN,
                                                13.0,
                                            ));
                                        }
                                        if hovered && !modal {
                                            for (request, icon, color) in [
                                                (delete, Icon::Trash, tw::DANGER),
                                                (edit, Icon::Sliders, tw::SECONDARY),
                                                (motion, Icon::Move, tw::SECONDARY),
                                            ] {
                                                if tw::ghost_icon_sized(
                                                    ui,
                                                    icon,
                                                    &request.id.label(&self.localizer),
                                                    color,
                                                    13.0,
                                                    22.0,
                                                    self.action_availability(request).is_ok(),
                                                    false,
                                                )
                                                .clicked()
                                                {
                                                    run = Some(request);
                                                }
                                            }
                                        } else {
                                            ui.label(tw::mono(code, 11.0).color(tw::FAINT));
                                        }
                                        ui.with_layout(
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new(name).size(13.0).color(
                                                            if active {
                                                                tw::ACCENT_INK
                                                            } else {
                                                                tw::TEXT_2
                                                            },
                                                        ),
                                                    )
                                                    .truncate()
                                                    .selectable(false),
                                                );
                                            },
                                        );
                                    },
                                );
                            },
                        );
                        if run.is_none() && response.double_clicked() {
                            run = Some(edit);
                        } else if run.is_none() && response.clicked() {
                            inspect = Some(*id);
                        }
                    }
                });
        }
        if let Some(id) = inspect {
            self.session.inspector = Some(InspectorTarget::Slide(id));
        }
        if let Some(request) = run {
            self.invoke_or_report(request);
        }
    }

    /// Inspector for one drawer's slides: product, gaps, holes and issues.
    pub(crate) fn show_slide_inspector(&mut self, ui: &mut egui::Ui, id: Uuid) {
        let project = self.editor.project();
        let Some(installation) = project.slide_installations.iter().find(|s| s.id == id) else {
            return;
        };
        let l = &self.localizer;
        let status = slide_installation::diagnose(project, installation);
        let entry = project
            .catalog
            .iter()
            .find(|c| c.id == installation.catalog_id);
        let spec = entry.and_then(CatalogReference::slide);
        let name = |id| {
            project
                .board(id)
                .map_or_else(String::new, |b| b.name.clone())
        };
        let drawer = project
            .assemblies
            .iter()
            .find(|a| a.id == installation.drawer_root_id)
            .map_or_else(String::new, |a| a.name.clone());
        let edit = Request::with(A::EditSlides, Target::Slide(id));
        let delete = Request::with(A::DeleteSlides, Target::Slide(id));
        let motion = Request::with(A::StartMotion, Target::Slide(id));
        let mut run = None;
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(14, 14))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 5.0;
                ui.label(tw::semibold(ui, &drawer, 15.0).color(tw::TEXT));
                if let Some(entry) = entry {
                    ui.label(
                        egui::RichText::new(format!("{} · {}", entry.product_id, entry.name))
                            .size(12.0)
                            .color(tw::MUTED),
                    );
                    if let Some(chip) = plan_my_cabinet::hardware_catalog::trust(entry) {
                        catalog_ui::trust_chip(ui, l, Some(chip));
                    }
                }
                if let Some(spec) = spec {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} {} mm · {} {} mm · {} {} mm (+{} / -{})",
                            l.text("slide-length"),
                            mm_text(spec.length),
                            l.text("slide-travel"),
                            mm_text(spec.travel),
                            l.text("slide-clearance"),
                            mm_text(spec.clearance),
                            mm_text(spec.clearance_plus),
                            mm_text(spec.clearance_minus),
                        ))
                        .size(12.0)
                        .color(tw::TEXT_2),
                    );
                }
                ui.add_space(6.0);
                if status.issues.is_empty() {
                    ui.label(
                        egui::RichText::new(l.text("slide-fits"))
                            .size(12.0)
                            .color(tw::OK),
                    );
                } else {
                    for line in issue_lines(l, &status) {
                        ui.label(egui::RichText::new(line).size(12.0).color(tw::WARN_INK));
                    }
                }
                if let Some(references) = &status.references {
                    for (i, side) in references.sides.iter().enumerate() {
                        ui.add_space(6.0);
                        ui.label(
                            tw::semibold(
                                ui,
                                l.text(if i == 0 { "pdf-left" } else { "pdf-right" }),
                                12.5,
                            )
                            .color(tw::TEXT),
                        );
                        let list = |values: &[Length]| {
                            values
                                .iter()
                                .map(|v| mm_text(*v))
                                .collect::<Vec<_>>()
                                .join(", ")
                        };
                        for line in [
                            format!(
                                "{} · {} {} mm",
                                name(side.cabinet_board),
                                l.text("slide-gap"),
                                mm_text(side.gap)
                            ),
                            format!(
                                "{} {} mm · {} {} mm",
                                l.text("pdf-slide-holes-from-front"),
                                list(&side.cabinet_hole_distances),
                                l.text("pdf-slide-centre-line"),
                                mm_text(side.cabinet_centre_from_bottom)
                            ),
                            name(side.drawer_board),
                            format!(
                                "{} {} mm · {} {} mm",
                                l.text("pdf-slide-holes-from-front"),
                                list(&side.drawer_hole_distances),
                                l.text("pdf-slide-centre-line"),
                                mm_text(side.drawer_centre_from_bottom)
                            ),
                        ] {
                            ui.label(egui::RichText::new(line).size(11.5).color(tw::MUTED));
                        }
                    }
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    for request in [motion, edit, delete] {
                        if ui
                            .add_enabled(
                                self.action_availability(request).is_ok(),
                                egui::Button::new(request.id.label(l)),
                            )
                            .clicked()
                        {
                            run = Some(request);
                        }
                    }
                });
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(l.text("slide-reference-note"))
                        .size(11.0)
                        .color(tw::FAINT),
                );
            });
        if let Some(request) = run {
            self.invoke_or_report(request);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plan_my_cabinet::template_setup::{TemplateField, TemplateKind, TemplateSetup};

    fn chest() -> DesktopApp {
        let mut stage = TemplateSetup::new(
            TemplateKind::Drawers,
            "Chest",
            plan_my_cabinet::money::Currency::Brl,
            Unit::Mm,
        );
        stage.seed_standard_materials(Language::En);
        for (field, value) in [
            (TemplateField::Width, 600),
            (TemplateField::Depth, 560),
            (TemplateField::Height, 720),
            (TemplateField::BoxDepth, 500),
            (TemplateField::SideClearance, 13),
            (TemplateField::RearClearance, 20),
            (TemplateField::VerticalClearance, 8),
            (TemplateField::FrontReveal, 3),
            (TemplateField::FrontGap, 3),
        ] {
            stage.dimensions.insert(
                field,
                plan_my_cabinet::template_setup::ProposedLength::new(Conversion::Exact(
                    Length::from_micrometres(value * 1000),
                )),
            );
        }
        stage.drawer_count = Some(2);
        DesktopApp {
            editor: stage.generate().unwrap().editor,
            ..Default::default()
        }
    }

    #[test]
    fn dialog_proposes_the_template_slides_for_a_selected_box_side() {
        let mut app = chest();
        let side = app
            .editor
            .project()
            .boards
            .iter()
            .find(|b| b.name == "Drawer 2 left box side")
            .unwrap()
            .id;
        app.selection.choose(Some(side), false);
        let dialog = SlideDialog::new(&app, None);
        let (suggestion, installation) = dialog.proposal(&app).unwrap();
        assert_eq!(suggestion.catalog.product_id, "0073.045500SX");
        assert_eq!(
            Some(installation.drawer_root_id),
            app.editor
                .project()
                .slide_installations
                .get(1)
                .map(|s| s.drawer_root_id)
        );
    }

    #[test]
    fn drawer_motion_runs_through_the_door_preview_route() {
        let mut app = chest();
        let id = app.editor.project().slide_installations[0].id;
        app.invoke(Request::with(A::StartMotion, Target::Slide(id)))
            .unwrap();
        assert_eq!(app.hardware.door_motion, Some((id, 0.0)));
        app.invoke(
            Request::with(A::SetDoorAngle, Target::Slide(id)).argument(Argument::Angle(250.0)),
        )
        .unwrap();
        assert_eq!(app.hardware.door_motion, Some((id, 250.0)));
        assert!(
            app.invoke(
                Request::with(A::SetDoorAngle, Target::Slide(id)).argument(Argument::Angle(900.0))
            )
            .is_err()
        );
        let poses = door_joint_ui::motion_poses(app.editor.project(), id, 250.0).unwrap();
        assert!(!poses.is_empty());
        app.invoke(Request::new(A::CloseMotion)).unwrap();
        assert!(app.hardware.door_motion.is_none());
    }
}
