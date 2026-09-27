//! Board-local hinge references. The modal owns an uncommitted draft.
use super::*;
use crate::actions::{ActionId as A, Request, Target};
use plan_my_cabinet::domain::{
    BoardEdge, BoardFace, CatalogReference, HingeInstallation, HingeMountingSide, Project,
};
use plan_my_cabinet::hinge_installation::{self, InstallationIssue, InstallationStatus};

pub(super) struct HingeDialog {
    id: Option<Uuid>,
    new_id: Uuid,
    project_id: Uuid,
    revision: u64,
    door: Option<Uuid>,
    mount: Option<Uuid>,
    catalog: Option<Uuid>,
    side: HingeMountingSide,
    values: [String; 4],
    error: bool,
    chrome: Option<ModalChrome>,
}

fn mm(value: Length) -> String {
    format!("{:.3}", value.micrometres() as f64 / 1000.0)
}

fn distance(text: &str) -> Option<Length> {
    let value = parse_length(text, Unit::Mm).ok()?.conversion.exact()?;
    (value.micrometres() >= 0).then_some(value)
}

fn edge_label(localizer: &Localizer, edge: BoardEdge) -> String {
    localizer.text(match edge {
        BoardEdge::MinX => "hinge-min-x",
        BoardEdge::MaxX => "hinge-max-x",
    })
}

fn face_label(localizer: &Localizer, face: BoardFace) -> String {
    localizer.text(match face {
        BoardFace::MinZ => "hinge-min-z",
        BoardFace::MaxZ => "hinge-max-z",
    })
}

/// A relationship owns its listed installation IDs, not every installation on
/// the same board. Keep broken references visible as warnings in the tree.
fn installation_groups(project: &Project) -> (Vec<(Uuid, Vec<Uuid>)>, Vec<Uuid>) {
    let mut assigned = std::collections::HashSet::new();
    let doors = project
        .door_joints
        .iter()
        .map(|door| {
            let children = door.hinge_installation_ids.clone();
            assigned.extend(children.iter().copied());
            (door.id, children)
        })
        .collect();
    let standalone = project
        .hinge_installations
        .iter()
        .filter(|hinge| !assigned.contains(&hinge.id))
        .map(|hinge| hinge.id)
        .collect();
    (doors, standalone)
}

fn snapshot_card(ui: &mut egui::Ui, localizer: &Localizer, entry: &CatalogReference) {
    ui.group(|ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        ui.set_max_width(ui.available_width());
        ui.strong(&entry.name);
        ui.monospace(format!(
            "{} · {}",
            entry.product_id,
            entry.plate_id.as_deref().unwrap_or("—")
        ));
        ui.small(format!(
            "{}: {}",
            localizer.text("hinge-snapshot"),
            entry.revision
        ));
        if hardware_catalog::is_verified(entry) {
            if let Some(facts) = &entry.verified_hinge {
                ui.small(format!(
                    "H={} mm · {}: {}–{} mm",
                    mm(facts.plate_height),
                    localizer.text("hinge-supported-thickness"),
                    mm(facts.door_thickness_min),
                    mm(facts.door_thickness_max)
                ));
                ui.small(format!(
                    "{}: {} mm",
                    localizer.text("hinge-supported-pairs"),
                    facts
                        .overlay_by_cup_edge
                        .iter()
                        .map(|pair| format!("{}/{}", mm(pair.cup_edge_setback), mm(pair.overlay)))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                ui.small(format!(
                    "{}: {} · {} {}, PDF {}",
                    localizer.text("hinge-source"),
                    facts.attribution,
                    localizer.text("hinge-printed-page"),
                    facts.printed_page,
                    facts.pdf_page
                ));
            }
        } else {
            ui.colored_label(
                crate::theme_widgets::WARN_INK,
                localizer.text("hinge-evidence-unavailable"),
            );
        }
        ui.small(format!(
            "{}: {}",
            localizer.text("hinge-recorded-source"),
            entry.source
        ));
        ui.hyperlink_to(localizer.text("hinge-source-review"), &entry.source);
    });
}

impl HingeDialog {
    pub(super) fn new(app: &DesktopApp, id: Option<Uuid>) -> Self {
        let project = app.editor.project();
        let old = id.and_then(|id| project.hinge_installations.iter().find(|i| i.id == id));
        let boards = &project.boards;
        let catalog = old.map(|i| i.catalog_id).or_else(|| {
            project
                .catalog
                .iter()
                .find(|c| hardware_catalog::is_verified(c))
                .map(|c| c.id)
        });
        let initial_pair = catalog
            .and_then(|id| project.catalog.iter().find(|c| c.id == id))
            .filter(|c| hardware_catalog::is_verified(c))
            .and_then(|c| c.verified_hinge.as_ref())
            .and_then(|facts| facts.overlay_by_cup_edge.first());
        Self {
            id,
            new_id: Uuid::new_v4(),
            project_id: project.id,
            revision: project.revision,
            door: old
                .map(|i| i.door_board_id)
                .or_else(|| boards.first().map(|b| b.id)),
            mount: old
                .map(|i| i.mounting_board_id)
                .or_else(|| boards.get(1).map(|b| b.id)),
            catalog,
            side: old.map_or(
                HingeMountingSide {
                    door_edge: BoardEdge::MinX,
                    door_face: BoardFace::MinZ,
                    mount_front_edge: BoardEdge::MinX,
                    mount_face: BoardFace::MinZ,
                },
                |i| i.side,
            ),
            values: old.map_or_else(
                || {
                    [
                        "50".into(),
                        "50".into(),
                        initial_pair.map_or(String::new(), |p| mm(p.cup_edge_setback)),
                        initial_pair.map_or(String::new(), |p| mm(p.overlay)),
                    ]
                },
                |i| {
                    [
                        mm(i.door_y),
                        mm(i.mount_y),
                        mm(i.cup_edge_setback),
                        mm(i.overlay),
                    ]
                },
            ),
            error: false,
            chrome: Some(
                ModalChrome::new(egui::Id::new("hinge-dialog"))
                    .first_focus(egui::Id::new(("hinge-value-field", 0)))
                    .width(560.0),
            ),
        }
    }

    fn proposed(&self, app: &DesktopApp) -> Option<HingeInstallation> {
        let project = app.editor.project();
        let (door, mount, catalog) = (self.door?, self.mount?, self.catalog?);
        if door == mount
            || !project.boards.iter().any(|b| b.id == door)
            || !project.boards.iter().any(|b| b.id == mount)
            || !project
                .catalog
                .iter()
                .any(|c| c.id == catalog && hardware_catalog::is_verified(c))
        {
            return None;
        }
        let [door_y, mount_y, k, overlay] = self.values.each_ref().map(|v| distance(v));
        Some(HingeInstallation {
            id: self.id.unwrap_or(self.new_id),
            door_board_id: door,
            mounting_board_id: mount,
            catalog_id: catalog,
            side: self.side,
            door_y: door_y?,
            mount_y: mount_y?,
            cup_edge_setback: k?,
            overlay: overlay?,
        })
    }
}

pub(super) fn issue_key(issue: &InstallationIssue) -> &'static str {
    match issue {
        InstallationIssue::MissingPart(_) => "hinge-missing-part",
        InstallationIssue::MissingCatalog(_) | InstallationIssue::MissingVerifiedCatalog => {
            "hinge-missing-catalog"
        }
        InstallationIssue::UnsupportedThickness => "hinge-thickness-warning",
        InstallationIssue::UnsupportedOverlay => "hinge-overlay-warning",
        InstallationIssue::CupOutsideDoor => "hinge-cup-warning",
        InstallationIssue::PlateOutsideMount => "hinge-plate-warning",
    }
}

fn compact_installation_status(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    project: &Project,
    installation: &HingeInstallation,
) {
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
    crate::theme_widgets::section_header(ui, &localizer.text("hardware-mounting"));
    let name = |id| {
        project
            .boards
            .iter()
            .find(|b| b.id == id)
            .map_or("—", |b| b.name.as_str())
    };
    egui::Grid::new(("installation-mounting", installation.id))
        .num_columns(2)
        .min_col_width(82.0)
        .max_col_width((ui.available_width() - 96.0).max(90.0))
        .show(ui, |ui| {
            for (key, value) in [
                (
                    "hardware-door",
                    format!(
                        "{} · {}",
                        name(installation.door_board_id),
                        edge_label(localizer, installation.side.door_edge)
                    ),
                ),
                (
                    "hardware-cabinet",
                    format!(
                        "{} · {}",
                        name(installation.mounting_board_id),
                        face_label(localizer, installation.side.mount_face)
                    ),
                ),
                (
                    "hardware-door-position",
                    format!("{} mm", mm(installation.door_y)),
                ),
                (
                    "hardware-mount-position",
                    format!("{} mm", mm(installation.mount_y)),
                ),
                (
                    "hardware-setback-overlay",
                    format!(
                        "{} / {} mm",
                        mm(installation.cup_edge_setback),
                        mm(installation.overlay)
                    ),
                ),
            ] {
                ui.small(localizer.text(key));
                ui.add(egui::Label::new(value).wrap());
                ui.end_row();
            }
        });
    let status = hinge_installation::diagnose(project, installation);
    for issue in &status.issues {
        ui.colored_label(
            crate::theme_widgets::WARN_INK,
            localizer.text(issue_key(issue)),
        );
    }
    if status.issues.is_empty() {
        if let Some(r) = &status.references {
            crate::theme_widgets::card().show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(72.0, 98.0), egui::Sense::hover());
                    let painter = ui.painter_at(rect);
                    painter.rect_filled(rect, 0, egui::Color32::from_rgb(240, 232, 212));
                    let center = rect.left_top() + egui::vec2(27.0, 34.0);
                    painter.circle_filled(center, 13.0, egui::Color32::from_rgb(246, 197, 121));
                    painter.circle_stroke(
                        center,
                        13.0,
                        egui::Stroke::new(1.0, crate::theme_widgets::WARN_INK),
                    );
                    for y in [68.0, 84.0] {
                        painter.circle_filled(
                            rect.left_top() + egui::vec2(46.0, y),
                            2.0,
                            crate::theme_widgets::WARN_INK,
                        );
                    }
                    ui.vertical(|ui| {
                        for (key, value) in [
                            (
                                "hardware-cup",
                                format!("Ø{} × {} mm", mm(r.cup_diameter), mm(r.cup_depth)),
                            ),
                            (
                                "hardware-cup-edge",
                                format!(
                                    "{:.3} mm",
                                    (installation.cup_edge_setback.micrometres() as f64
                                        + r.cup_diameter.micrometres() as f64 * 0.5)
                                        / 1000.0
                                ),
                            ),
                            (
                                "hardware-plate-pitch",
                                format!("{} mm", mm(r.plate_hole_pitch)),
                            ),
                            (
                                "hardware-plate-front",
                                format!("{} mm", mm(r.plate_front_offset)),
                            ),
                        ] {
                            ui.small(format!("{}: {value}", localizer.text(key)));
                        }
                    });
                });
                ui.small(localizer.text("hinge-diagram-not-scale"));
            });
        }
    } else {
        ui.colored_label(
            crate::theme_widgets::WARN_INK,
            localizer.text("hinge-reference-withheld"),
        );
    }
    egui::Frame::new()
        .fill(crate::theme_widgets::WARN_BG)
        .inner_margin(10)
        .corner_radius(6)
        .show(ui, |ui| {
            ui.colored_label(
                crate::theme_widgets::WARN_INK,
                localizer.text("hinge-fasteners-unavailable"),
            );
            ui.small(localizer.text("hinge-provisional"));
        });
    if let Some(entry) = project
        .catalog
        .iter()
        .find(|entry| entry.id == installation.catalog_id)
    {
        if let Some(facts) = entry
            .verified_hinge
            .as_ref()
            .filter(|_| hardware_catalog::is_verified(entry))
        {
            ui.small(format!(
                "{} · {} {}",
                facts.attribution,
                localizer.text("hinge-printed-page"),
                facts.printed_page
            ));
        } else {
            ui.colored_label(
                crate::theme_widgets::WARN_INK,
                localizer.text("hinge-evidence-unavailable"),
            );
        }
        ui.hyperlink_to(localizer.text("hinge-source-review"), &entry.source);
    }
}

fn status_ui(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    project: &Project,
    status: &InstallationStatus,
    proposed: &HingeInstallation,
) {
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
    for issue in &status.issues {
        ui.colored_label(
            crate::theme_widgets::WARN_INK,
            localizer.text(issue_key(issue)),
        );
    }
    let catalog = project.catalog.iter().find(|c| c.id == proposed.catalog_id);
    if let Some(entry) = catalog {
        ui.label(format!(
            "{}: {} / {}",
            localizer.text("hinge-kit-identifiers"),
            entry.product_id,
            entry.plate_id.as_deref().unwrap_or("—")
        ));
        ui.add(
            egui::Label::new(format!(
                "{}: {}",
                localizer.text("hinge-snapshot"),
                entry.revision
            ))
            .wrap(),
        );
        if hardware_catalog::is_verified(entry) {
            if let Some(facts) = &entry.verified_hinge {
                let settings = facts
                    .overlay_by_cup_edge
                    .iter()
                    .map(|pair| format!("{}/{}", mm(pair.cup_edge_setback), mm(pair.overlay)))
                    .collect::<Vec<_>>()
                    .join(", ");
                ui.add(
                    egui::Label::new(format!(
                        "{}: {settings} mm",
                        localizer.text("hinge-supported-pairs")
                    ))
                    .wrap(),
                );
                ui.label(format!(
                    "{}: {}–{} mm",
                    localizer.text("hinge-supported-thickness"),
                    mm(facts.door_thickness_min),
                    mm(facts.door_thickness_max)
                ));
            }
        } else {
            ui.label(format!(
                "{}: {}",
                localizer.text("hinge-recorded-source"),
                entry.source
            ));
            ui.colored_label(
                crate::theme_widgets::WARN_INK,
                localizer.text("hinge-evidence-unavailable"),
            );
        }
    }
    if status.issues.is_empty() {
        if let Some(r) = &status.references {
            let to_mm = |v: i128| v as f64 / 1000.0;
            ui.separator();
            ui.strong(localizer.text("hinge-reference-diagram"));
            reference_diagram(ui, localizer, project, proposed, r);
            ui.label(format!(
                "{}: Ø{} × {} mm; K={} mm; X={:.3}, Y={:.3}, Z={:.3} mm",
                localizer.text("hinge-cup-reference"),
                mm(r.cup_diameter),
                mm(r.cup_depth),
                mm(proposed.cup_edge_setback),
                to_mm(r.cup_center_um[0]),
                to_mm(r.cup_center_um[1]),
                to_mm(r.cup_center_um[2])
            ));
            ui.label(format!(
                "{}: H={} mm; {} / {} mm; X={:.3}, Y={:.3} / {:.3}, Z={:.3} mm",
                localizer.text("hinge-plate-reference"),
                mm(r.plate_height),
                mm(r.plate_hole_pitch),
                mm(r.plate_front_offset),
                to_mm(r.plate_hole_centers_um[0][0]),
                to_mm(r.plate_hole_centers_um[0][1]),
                to_mm(r.plate_hole_centers_um[1][1]),
                to_mm(r.plate_hole_centers_um[0][2])
            ));
            ui.label(format!(
                "{}: {} — {} ({} {}, PDF {})",
                localizer.text("hinge-source"),
                r.attribution,
                r.source,
                localizer.text("hinge-printed-page"),
                r.printed_page,
                r.pdf_page
            ));
            ui.hyperlink_to(
                localizer.text("hinge-source-review"),
                hardware_catalog::SOURCE_URL,
            );
        }
    } else {
        ui.colored_label(
            crate::theme_widgets::WARN_INK,
            localizer.text("hinge-reference-withheld"),
        );
        if let Some(entry) = catalog.filter(|e| hardware_catalog::is_verified(e))
            && let Some(facts) = &entry.verified_hinge
        {
            ui.label(format!(
                "{}: {} ({} {}, PDF {})",
                localizer.text("hinge-source"),
                facts.attribution,
                localizer.text("hinge-printed-page"),
                facts.printed_page,
                facts.pdf_page
            ));
            ui.hyperlink_to(
                localizer.text("hinge-source-review"),
                hardware_catalog::SOURCE_URL,
            );
        }
    }
    ui.colored_label(
        crate::theme_widgets::WARN_INK,
        localizer.text("hinge-fasteners-unavailable"),
    );
    ui.small(localizer.text("hinge-provisional"));
}

/// A schematic of the selected board-local datums, never a drilling template.
/// Coordinates are read from the diagnostic, not inferred from the pictured kit.
fn reference_diagram(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    project: &Project,
    installation: &HingeInstallation,
    r: &hinge_installation::InstallationReferences,
) {
    let Some(door) = project
        .boards
        .iter()
        .find(|b| b.id == installation.door_board_id)
    else {
        return;
    };
    let Some(mount) = project
        .boards
        .iter()
        .find(|b| b.id == installation.mounting_board_id)
    else {
        return;
    };
    let width = ui.available_width().clamp(80.0, 340.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 184.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 6.0, egui::Color32::from_rgb(244, 241, 236));
    let ink = egui::Color32::from_rgb(90, 82, 72);
    let accent = egui::Color32::from_rgb(201, 115, 31);
    let point = |board: &plan_my_cabinet::domain::Board, origin: egui::Pos2, center: [i128; 3]| {
        let x = center[0] as f32 / board.length.micrometres() as f32;
        let y = center[1] as f32 / board.width.micrometres() as f32;
        egui::pos2(origin.x + x * (width - 40.0), origin.y + y * 55.0)
    };
    let draw_board = |origin: egui::Pos2| {
        let area = egui::Rect::from_min_size(origin, egui::vec2(width - 40.0, 55.0));
        p.rect_filled(area, 2.0, egui::Color32::WHITE);
        for (a, b) in [
            (area.left_top(), area.right_top()),
            (area.right_top(), area.right_bottom()),
            (area.right_bottom(), area.left_bottom()),
            (area.left_bottom(), area.left_top()),
        ] {
            p.line_segment([a, b], egui::Stroke::new(1.0, ink));
        }
    };
    let door_origin = rect.min + egui::vec2(20.0, 20.0);
    let mount_origin = rect.min + egui::vec2(20.0, 105.0);
    draw_board(door_origin);
    draw_board(mount_origin);
    let cup = point(door, door_origin, r.cup_center_um);
    let plate_a = point(mount, mount_origin, r.plate_hole_centers_um[0]);
    let plate_b = point(mount, mount_origin, r.plate_hole_centers_um[1]);
    p.circle_stroke(cup, 7.0, egui::Stroke::new(2.0, accent));
    p.circle_filled(cup, 2.0, accent);
    p.circle_filled(plate_a, 3.0, accent);
    p.circle_filled(plate_b, 3.0, accent);
    p.line_segment([plate_a, plate_b], egui::Stroke::new(1.0, accent));
    p.line_segment(
        [
            cup,
            egui::pos2((plate_a.x + plate_b.x) / 2.0, (plate_a.y + plate_b.y) / 2.0),
        ],
        egui::Stroke::new(1.0, ink),
    );
    p.text(
        rect.min + egui::vec2(22.0, 76.0),
        egui::Align2::LEFT_TOP,
        format!(
            "{} · {} / {}",
            localizer.text("hinge-door"),
            edge_label(localizer, installation.side.door_edge),
            face_label(localizer, r.cup_face)
        ),
        egui::FontId::proportional(10.0),
        ink,
    );
    p.text(
        rect.min + egui::vec2(22.0, 162.0),
        egui::Align2::LEFT_TOP,
        format!(
            "{} · {} / {}",
            localizer.text("hinge-mount"),
            edge_label(localizer, installation.side.mount_front_edge),
            face_label(localizer, r.plate_face)
        ),
        egui::FontId::proportional(10.0),
        ink,
    );
    ui.small(localizer.text("hinge-diagram-not-scale"));
}

impl DesktopApp {
    pub(super) fn show_pinned_catalog(&mut self, ui: &mut egui::Ui) {
        let project = self.editor.project();
        let browse_id = ui.make_persistent_id(("hardware-browse-snapshots", project.id));
        let mut browse = ui.data(|d| d.get_temp::<bool>(browse_id)).unwrap_or(false);
        ui.horizontal_wrapped(|ui| {
            ui.strong(self.localizer.text("catalog-heading"));
            if ui.button(self.localizer.text("catalog-browse")).clicked() {
                browse = !browse;
            }
        });
        let entries = project.catalog.clone();
        if entries.is_empty() {
            ui.colored_label(
                crate::theme_widgets::WARN_INK,
                self.localizer.text("hinge-missing-catalog"),
            );
        } else {
            // Prefer the selected installation's exact saved snapshot; never
            // substitute a packaged revision when its source is missing.
            let selected = match self.session.inspector {
                Some(InspectorTarget::Installation(id)) => {
                    project.hinge_installations.iter().find(|h| h.id == id)
                }
                _ => None,
            };
            let pinned = selected.map_or_else(
                || entries.first(),
                |hinge| entries.iter().find(|entry| entry.id == hinge.catalog_id),
            );
            if let Some(entry) = pinned {
                crate::theme_widgets::card().show(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    ui.strong(&entry.name);
                    ui.small(format!(
                        "{} · {}",
                        entry.product_id,
                        entry.plate_id.as_deref().unwrap_or("—")
                    ));
                    if hardware_catalog::is_verified(entry) {
                        if let Some(facts) = &entry.verified_hinge {
                            ui.small(format!(
                                "H={} mm · {}–{} mm",
                                mm(facts.plate_height),
                                mm(facts.door_thickness_min),
                                mm(facts.door_thickness_max)
                            ));
                        }
                    } else {
                        ui.colored_label(
                            crate::theme_widgets::WARN_INK,
                            self.localizer.text("hinge-evidence-unavailable"),
                        );
                    }
                    ui.add(
                        egui::Label::new(egui::RichText::new(&entry.revision).small()).truncate(),
                    )
                    .on_hover_text(&entry.revision);
                });
            } else {
                ui.colored_label(
                    crate::theme_widgets::WARN_INK,
                    self.localizer.text("hinge-missing-catalog"),
                );
            }
        }
        if browse {
            ui.separator();
            ui.small(self.localizer.text("catalog-heading"));
            for entry in &entries {
                ui.push_id(entry.id, |ui| {
                    snapshot_card(ui, &self.localizer, entry);
                    if ui
                        .add_enabled(
                            !self.modal_open(),
                            egui::Button::new(self.localizer.text("catalog-update")),
                        )
                        .clicked()
                    {
                        let _ =
                            self.invoke(Request::with(A::UpdateCatalog, Target::Catalog(entry.id)));
                    }
                });
            }
            if ui
                .add_enabled(
                    !self.modal_open(),
                    egui::Button::new(self.localizer.text("catalog-add")),
                )
                .clicked()
            {
                let _ = self.invoke(Request::new(A::AddCatalog));
            }
            if let Some(notice) = &self.catalog_update_notice {
                ui.label(notice);
            }
        }
        ui.data_mut(|d| d.insert_temp(browse_id, browse));
    }

    fn show_hinge_tree_row(&mut self, ui: &mut egui::Ui, id: Uuid) {
        let Some(installation) = self
            .editor
            .project()
            .hinge_installations
            .iter()
            .find(|h| h.id == id)
            .cloned()
        else {
            ui.colored_label(
                crate::theme_widgets::WARN_INK,
                self.localizer.text("hinge-missing-part"),
            );
            return;
        };
        let project = self.editor.project();
        let name = |id| {
            project
                .boards
                .iter()
                .find(|b| b.id == id)
                .map_or("—", |b| b.name.as_str())
        };
        let label = format!(
            "{} → {} · Y {} / {} mm",
            name(installation.door_board_id),
            name(installation.mounting_board_id),
            mm(installation.door_y),
            mm(installation.mount_y)
        );
        let issues = hinge_installation::diagnose(project, &installation).issues;
        let ordinal = project
            .hinge_installations
            .iter()
            .position(|h| h.id == id)
            .unwrap_or(0)
            + 1;
        let short = format!(
            "{} {ordinal} · Y {}{}",
            self.localizer.text("hardware-hinge"),
            mm(installation.door_y),
            if issues.is_empty() { "" } else { " ⚠" }
        );
        let row = ui
            .add_enabled(
                !self.modal_open(),
                egui::Button::new(short)
                    .min_size(egui::vec2(ui.available_width(), 28.0))
                    .wrap_mode(egui::TextWrapMode::Wrap)
                    .selected(self.session.inspector == Some(InspectorTarget::Installation(id))),
            )
            .on_hover_text(label);
        if row.clicked() {
            self.navigate_session(Destination::Installation(id));
        }
        row.context_menu(|ui| {
            if ui
                .add_enabled(
                    !self.modal_open(),
                    egui::Button::new(self.localizer.text("hinge-edit")),
                )
                .clicked()
            {
                let _ = self.invoke(Request::with(A::EditHinge, Target::Hinge(id)));
            }
            if ui
                .add_enabled(
                    !self.modal_open(),
                    egui::Button::new(self.localizer.text("hinge-delete")),
                )
                .clicked()
            {
                let _ = self.invoke(Request::with(A::DeleteHinge, Target::Hinge(id)));
            }
        });
    }

    pub(super) fn show_hinge_list(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        crate::theme_widgets::section_header(ui, &self.localizer.text("door-joints"));
        let (doors, standalone) = installation_groups(self.editor.project());
        for (id, children) in doors {
            let Some(door) = self
                .editor
                .project()
                .door_joints
                .iter()
                .find(|d| d.id == id)
            else {
                continue;
            };
            let project = self.editor.project();
            let name = |id| {
                project
                    .boards
                    .iter()
                    .find(|b| b.id == id)
                    .map(|b| b.name.as_str())
                    .or_else(|| {
                        project
                            .assemblies
                            .iter()
                            .find(|a| a.id == id)
                            .map(|a| a.name.as_str())
                    })
                    .unwrap_or("—")
            };
            let label = format!(
                "{} → {}",
                name(door.moving_root_id),
                name(door.mounting_board_id)
            );
            let compact = if label.chars().count() > 27 {
                format!("{}…", label.chars().take(26).collect::<String>())
            } else {
                label.clone()
            };
            let reviewed = plan_my_cabinet::door_joint::needs_review(project, door);
            ui.push_id(id, |ui| {
                egui::CollapsingHeader::new(compact)
                    .default_open(true)
                    .show(ui, |ui| {
                        if reviewed {
                            ui.colored_label(
                                crate::theme_widgets::WARN_INK,
                                self.localizer.text("door-review"),
                            );
                        }
                        for child in children {
                            self.show_hinge_tree_row(ui, child);
                        }
                        ui.menu_button(self.localizer.text("hardware-door-actions"), |ui| {
                            if ui
                                .add_enabled(
                                    !self.modal_open(),
                                    egui::Button::new(self.localizer.text("door-edit")),
                                )
                                .clicked()
                            {
                                let _ = self.invoke(Request::with(A::EditDoor, Target::Door(id)));
                            }
                            if ui
                                .add_enabled(
                                    !self.modal_open(),
                                    egui::Button::new(self.localizer.text("door-delete")),
                                )
                                .clicked()
                            {
                                let _ = self.invoke(Request::with(A::DeleteDoor, Target::Door(id)));
                            }
                        });
                    })
                    .header_response
                    .on_hover_text(label);
            });
        }
        if !standalone.is_empty() {
            ui.separator();
            ui.strong(self.localizer.text("hinge-list"));
            for id in standalone {
                self.show_hinge_tree_row(ui, id);
            }
        }
        // Unselected installation problems must remain visible: selection is
        // inspection, not a filter on safety diagnostics.
        let warnings: Vec<_> = self
            .editor
            .project()
            .hinge_installations
            .iter()
            .enumerate()
            .flat_map(|(index, hinge)| {
                hinge_installation::diagnose(self.editor.project(), hinge)
                    .issues
                    .into_iter()
                    .map(move |issue| (index + 1, issue))
            })
            .collect();
        if !warnings.is_empty() {
            egui::Frame::new()
                .fill(crate::theme_widgets::WARN_BG)
                .corner_radius(6)
                .inner_margin(10)
                .show(ui, |ui| {
                    for (ordinal, issue) in warnings {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!(
                                    "{} {ordinal}: {}",
                                    self.localizer.text("hardware-hinge"),
                                    self.localizer.text(issue_key(&issue))
                                ))
                                .color(crate::theme_widgets::WARN_INK),
                            )
                            .wrap(),
                        );
                    }
                });
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !self.modal_open(),
                    egui::Button::new(self.localizer.text("hinge-new")),
                )
                .clicked()
            {
                let _ = self.invoke(Request::new(A::NewHinge));
            }
            if ui
                .add_enabled(
                    !self.modal_open(),
                    egui::Button::new(self.localizer.text("door-add")),
                )
                .clicked()
            {
                let _ = self.invoke(Request::new(A::NewDoor));
            }
        });
    }

    /// Hardware inspector hook: call with the session's selected Installation ID.
    /// The same diagnostic card is used by the uncommitted edit preview below.
    pub(super) fn show_selected_installation_inspector(&mut self, ui: &mut egui::Ui, id: Uuid) {
        let Some(installation) = self
            .editor
            .project()
            .hinge_installations
            .iter()
            .find(|i| i.id == id)
            .cloned()
        else {
            ui.label(self.localizer.text("hinge-selection-missing"));
            return;
        };
        let ordinal = self
            .editor
            .project()
            .hinge_installations
            .iter()
            .position(|h| h.id == id)
            .unwrap_or(0)
            + 1;
        ui.heading(format!(
            "{} {ordinal}",
            self.localizer.text("hardware-hinge")
        ));
        ui.horizontal_wrapped(|ui| {
            for action in [A::EditHinge, A::DeleteHinge] {
                let request = Request::with(action, Target::Hinge(id));
                if actions::button(
                    ui,
                    &self.localizer,
                    request,
                    self.action_availability(request),
                )
                .clicked()
                {
                    let _ = self.invoke(request);
                }
            }
        });
        let project = self.editor.project();
        compact_installation_status(ui, &self.localizer, project, &installation);
        egui::CollapsingHeader::new(self.localizer.text("hardware-reference-details"))
            .id_salt(("installation-details", id))
            .show(ui, |ui| {
                for (key, board_id) in [
                    ("hinge-door", installation.door_board_id),
                    ("hinge-mount", installation.mounting_board_id),
                ] {
                    let name = project
                        .boards
                        .iter()
                        .find(|b| b.id == board_id)
                        .map_or("—", |b| b.name.as_str());
                    ui.label(format!("{}: {name}", self.localizer.text(key)));
                }
                if let Some(door) = project
                    .boards
                    .iter()
                    .find(|b| b.id == installation.door_board_id)
                {
                    ui.label(format!(
                        "{}: {} mm",
                        self.localizer.text("hinge-current-thickness"),
                        mm(door.thickness)
                    ));
                }
                for (key, value) in [
                    (
                        "hinge-door-edge",
                        edge_label(&self.localizer, installation.side.door_edge),
                    ),
                    (
                        "hinge-door-face",
                        face_label(&self.localizer, installation.side.door_face),
                    ),
                    (
                        "hinge-front-edge",
                        edge_label(&self.localizer, installation.side.mount_front_edge),
                    ),
                    (
                        "hinge-mount-face",
                        face_label(&self.localizer, installation.side.mount_face),
                    ),
                    ("hinge-door-y", format!("{} mm", mm(installation.door_y))),
                    ("hinge-mount-y", format!("{} mm", mm(installation.mount_y))),
                    (
                        "hinge-k",
                        format!("{} mm", mm(installation.cup_edge_setback)),
                    ),
                    ("hinge-overlay", format!("{} mm", mm(installation.overlay))),
                ] {
                    ui.label(format!("{}: {value}", self.localizer.text(key)));
                }
                let status = hinge_installation::diagnose(project, &installation);
                status_ui(ui, &self.localizer, project, &status, &installation);
            });
    }

    pub(super) fn show_hinge_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.hinge_dialog.take() else {
            return;
        };
        let current = draft.project_id == self.editor.project().id
            && draft.revision == self.editor.project().revision;
        let title = self.localizer.text(if draft.id.is_some() {
            "hinge-edit"
        } else {
            "hinge-new"
        });
        let mut chrome = draft.chrome.take().expect("hinge modal controller");
        let opening = !chrome.is_active();
        let mut first_control = None;
        let result = chrome.show(
            ctx,
            &title,
            ModalActions {
                cancel: &self.localizer.text("cancel"),
                confirm: &title,
            },
            |ui| {
                for (key, selected) in [
                    ("hinge-door", &mut draft.door),
                    ("hinge-mount", &mut draft.mount),
                ] {
                    let name = selected
                        .and_then(|id| self.editor.project().boards.iter().find(|b| b.id == id))
                        .map_or("—", |b| b.name.as_str());
                    let chooser = egui::ComboBox::from_label(self.localizer.text(key))
                        .selected_text(name)
                        .show_ui(ui, |ui| {
                            for board in &self.editor.project().boards {
                                crate::combo_option(ui, selected, Some(board.id), &board.name);
                            }
                        });
                    if key == "hinge-door" {
                        first_control = Some(chooser.response.id);
                    }
                }
                let name = draft
                    .catalog
                    .and_then(|id| self.editor.project().catalog.iter().find(|c| c.id == id))
                    .map_or("—", |c| c.name.as_str());
                egui::ComboBox::from_label(self.localizer.text("hinge-kit"))
                    .selected_text(name)
                    .show_ui(ui, |ui| {
                        for entry in &self.editor.project().catalog {
                            if hardware_catalog::is_verified(entry) {
                                crate::combo_option(
                                    ui,
                                    &mut draft.catalog,
                                    Some(entry.id),
                                    &entry.name,
                                );
                            }
                        }
                    });
                for (key, edge) in [
                    ("hinge-door-edge", &mut draft.side.door_edge),
                    ("hinge-front-edge", &mut draft.side.mount_front_edge),
                ] {
                    egui::ComboBox::from_label(self.localizer.text(key))
                        .selected_text(self.localizer.text(if *edge == BoardEdge::MinX {
                            "hinge-min-x"
                        } else {
                            "hinge-max-x"
                        }))
                        .show_ui(ui, |ui| {
                            crate::combo_option(
                                ui,
                                edge,
                                BoardEdge::MinX,
                                self.localizer.text("hinge-min-x"),
                            );
                            crate::combo_option(
                                ui,
                                edge,
                                BoardEdge::MaxX,
                                self.localizer.text("hinge-max-x"),
                            );
                        });
                }
                for (key, face) in [
                    ("hinge-door-face", &mut draft.side.door_face),
                    ("hinge-mount-face", &mut draft.side.mount_face),
                ] {
                    egui::ComboBox::from_label(self.localizer.text(key))
                        .selected_text(self.localizer.text(if *face == BoardFace::MinZ {
                            "hinge-min-z"
                        } else {
                            "hinge-max-z"
                        }))
                        .show_ui(ui, |ui| {
                            crate::combo_option(
                                ui,
                                face,
                                BoardFace::MinZ,
                                self.localizer.text("hinge-min-z"),
                            );
                            crate::combo_option(
                                ui,
                                face,
                                BoardFace::MaxZ,
                                self.localizer.text("hinge-max-z"),
                            );
                        });
                }
                if let Some(entry) = draft
                    .catalog
                    .and_then(|id| self.editor.project().catalog.iter().find(|c| c.id == id))
                    .filter(|c| hardware_catalog::is_verified(c))
                    && let Some(facts) = &entry.verified_hinge
                {
                    ui.label(self.localizer.text("hinge-pair-presets"));
                    ui.horizontal_wrapped(|ui| {
                        for pair in &facts.overlay_by_cup_edge {
                            let k = mm(pair.cup_edge_setback);
                            let r = mm(pair.overlay);
                            if ui.button(format!("{k}/{r} mm")).clicked() {
                                draft.values[2] = k;
                                draft.values[3] = r;
                            }
                        }
                    });
                }
                for (index, key) in ["hinge-door-y", "hinge-mount-y", "hinge-k", "hinge-overlay"]
                    .iter()
                    .enumerate()
                {
                    ui.horizontal(|ui| {
                        ui.label(self.localizer.text(key));
                        ui.add(
                            egui::TextEdit::singleline(&mut draft.values[index])
                                .id(egui::Id::new(("hinge-value-field", index))),
                        );
                    });
                    if distance(&draft.values[index]).is_none() {
                        ui.colored_label(
                            crate::theme_widgets::WARN_INK,
                            format!(
                                "{}: {}",
                                self.localizer.text(key),
                                self.localizer.text("hinge-distance-invalid")
                            ),
                        );
                    }
                }
                let proposed = current.then(|| draft.proposed(self)).flatten();
                if let Some(ref proposed) = proposed {
                    if let Ok(status) = hinge_installation::preview(self.editor.project(), proposed)
                    {
                        status_ui(
                            ui,
                            &self.localizer,
                            self.editor.project(),
                            &status,
                            proposed,
                        );
                    }
                } else {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        self.localizer.text("hinge-invalid"),
                    );
                }
                if draft.error {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        self.localizer.text("hinge-invalid"),
                    );
                }
                ((), proposed.is_some())
            },
        );
        if opening && let Some(id) = first_control {
            ctx.memory_mut(|m| m.request_focus(id));
        }
        if actions::decision(A::CancelDialog, result.action == ModalAction::Cancel) {
            chrome.close(ctx);
            return;
        }
        if actions::decision(A::ConfirmDialog, result.action == ModalAction::Confirm) {
            if let Some(proposed) = draft.proposed(self) {
                let result = if draft.id.is_some() {
                    hinge_installation::update(&mut self.editor, proposed)
                } else {
                    hinge_installation::create(&mut self.editor, proposed)
                };
                if result.is_ok() {
                    chrome.close(ctx);
                    return;
                }
            }
            draft.error = true;
        }
        draft.chrome = Some(chrome);
        self.hinge_dialog = Some(draft);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plan_my_cabinet::board_commands::{NewBoard, NewMaterial};
    use plan_my_cabinet::domain::BoardGrain;

    fn fixture() -> DesktopApp {
        let mut app = DesktopApp::default();
        let material = app
            .editor
            .create_material(NewMaterial {
                name: "Plywood".into(),
                thickness: Length::from_micrometres(18_000),
                grain: BoardGrain::Unrestricted,
            })
            .unwrap();
        for name in ["Door", "Side"] {
            app.editor
                .create_board(NewBoard {
                    name: name.into(),
                    material_id: material,
                    length: Length::from_micrometres(100_000),
                    width: Length::from_micrometres(100_000),
                    pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
                })
                .unwrap();
        }
        hardware_catalog::add_builtin(&mut app.editor).unwrap();
        app
    }

    #[test]
    fn invalid_draft_and_escape_leave_project_untouched() {
        let mut app = fixture();
        let before = app.editor.project().clone();
        let mut draft = HingeDialog::new(&app, None);
        assert!(draft.proposed(&app).is_some());
        draft.values[0] = "invalid".into();
        assert!(draft.proposed(&app).is_none());
        draft.values[0] = "-1".into();
        assert!(draft.proposed(&app).is_none());
        draft.values[0] = "50".into();
        draft.mount = draft.door;
        assert!(draft.proposed(&app).is_none());
        app.hinge_dialog = Some(draft);
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_hinge_dialog(ui.ctx())
        })
        .drop_without_applying_deltas();
        assert_eq!(app.editor.project(), &before);
        ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| app.show_hinge_dialog(ui.ctx()),
        )
        .drop_without_applying_deltas();
        assert!(app.hinge_dialog.is_none());
        assert_eq!(app.editor.project(), &before);
    }

    #[test]
    fn valid_and_unsupported_preview_and_refresh() {
        let mut app = fixture();
        let mut draft = HingeDialog::new(&app, None);
        let proposed = draft.proposed(&app).unwrap();
        assert!(
            hinge_installation::preview(app.editor.project(), &proposed)
                .unwrap()
                .issues
                .is_empty()
        );
        draft.values[3] = "18".into();
        let unsupported =
            hinge_installation::preview(app.editor.project(), &draft.proposed(&app).unwrap())
                .unwrap();
        assert!(
            unsupported
                .issues
                .contains(&InstallationIssue::UnsupportedOverlay)
        );
        assert!(unsupported.references.is_none());
        let id = proposed.catalog_id;
        hinge_installation::create(&mut app.editor, proposed).unwrap();
        let (_, statuses) =
            hardware_catalog::update_from_builtin_with_status(&mut app.editor, id).unwrap();
        assert_eq!(statuses.len(), 1);
        assert!(statuses[0].issues.is_empty());
        app.editor
            .transact(|p| -> Result<(), ()> {
                p.boards[0].width = Length::from_micrometres(20_000);
                Ok(())
            })
            .unwrap();
        assert!(
            hinge_installation::diagnose_all(app.editor.project())[0]
                .issues
                .contains(&InstallationIssue::CupOutsideDoor)
        );
        let (_, statuses) =
            hardware_catalog::update_from_builtin_with_status(&mut app.editor, id).unwrap();
        assert!(
            statuses[0]
                .issues
                .contains(&InstallationIssue::CupOutsideDoor)
        );
        assert_eq!(
            app.editor.project().hinge_installations[0].door_y,
            Length::from_micrometres(50_000)
        );
    }

    fn inspector_text(app: &mut DesktopApp, id: Uuid) -> String {
        let ctx = egui::Context::default();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600.0, 3000.0),
                )),
                ..Default::default()
            },
            |ui| {
                app.show_selected_installation_inspector(ui, id);
            },
        );
        let label = app.localizer.text("hardware-reference-details");
        let point = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap();
        for pressed in [true, false] {
            output.drop_without_applying_deltas();
            output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600.0, 3000.0),
                    )),
                    events: vec![
                        egui::Event::PointerMoved(point),
                        egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ui| app.show_selected_installation_inspector(ui, id),
            );
        }
        let text = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        output.drop_without_applying_deltas();
        text
    }

    fn tree_text(app: &mut DesktopApp) -> String {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_pinned_catalog(ui);
            app.show_hinge_list(ui);
        });
        let text = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        output.drop_without_applying_deltas();
        text
    }

    #[test]
    fn offline_tree_groups_by_membership_and_tracks_selected_warning() {
        let mut app = fixture();
        let first = HingeDialog::new(&app, None).proposed(&app).unwrap();
        let first_id = first.id;
        hinge_installation::create(&mut app.editor, first).unwrap();
        let second = HingeDialog::new(&app, None).proposed(&app).unwrap();
        let second_id = second.id;
        hinge_installation::create(&mut app.editor, second).unwrap();
        let boards = &app.editor.project().boards;
        let proposal = plan_my_cabinet::door_joint::preview(
            app.editor.project(),
            Uuid::new_v4(),
            boards[0].id,
            boards[1].id,
            vec![first_id],
        )
        .unwrap();
        plan_my_cabinet::door_joint::confirm(&mut app.editor, proposal).unwrap();
        let (doors, standalone) = installation_groups(app.editor.project());
        assert_eq!(doors.len(), 1);
        assert_eq!(doors[0].1, vec![first_id]);
        assert_eq!(standalone, vec![second_id]);
        assert!(app.navigate_session(Destination::Installation(second_id)));
        assert_eq!(
            app.session.inspector,
            Some(InspectorTarget::Installation(second_id))
        );
        assert_eq!(app.session.active, Workspace::Hardware);
        app.editor
            .transact(|p| -> Result<(), ()> {
                p.boards[0].thickness = Length::from_micrometres(14_000);
                Ok(())
            })
            .unwrap();
        let text = tree_text(&mut app);
        assert!(text.contains(hardware_catalog::KIT_ID));
        assert!(text.contains(hardware_catalog::PLATE_ID));
        assert!(text.contains("May 2025 catalog"));
        assert!(text.contains("Unsupported door thickness"));
        assert!(text.contains("Door → Side"));
        assert!(text.contains("Hinge installations"));
    }

    #[test]
    fn legacy_snapshot_is_labelled_without_bundled_evidence() {
        let mut app = fixture();
        app.editor
            .transact(|p| -> Result<(), ()> {
                p.catalog[0].verified_hinge = None;
                Ok(())
            })
            .unwrap();
        let text = tree_text(&mut app);
        assert!(text.contains(hardware_catalog::KIT_ID));
        assert!(text.contains("Verified installation evidence unavailable"));
        assert!(!text.contains("Documented K/R pairs"));
    }

    #[test]
    fn selected_inspector_discloses_only_supported_derived_references() {
        let mut app = fixture();
        let mut draft = HingeDialog::new(&app, None);
        draft.values[0] = "70".into();
        draft.values[1] = "40".into();
        draft.side.door_edge = BoardEdge::MaxX;
        draft.side.door_face = BoardFace::MaxZ;
        draft.side.mount_front_edge = BoardEdge::MaxX;
        draft.side.mount_face = BoardFace::MinZ;
        let proposed = draft.proposed(&app).unwrap();
        assert_eq!(proposed.id, draft.proposed(&app).unwrap().id);
        let id = proposed.id;
        hinge_installation::create(&mut app.editor, proposed).unwrap();
        let text = inspector_text(&mut app, id);
        assert!(text.contains("70.000 mm"));
        assert!(text.contains("40.000 mm"));
        assert!(text.contains("3.000/15.000, 4.000/16.000"), "{text}");
        assert!(text.contains("79.500")); // cup X from the opposite door edge
        assert!(text.contains("63.000")); // plate X from the opposite front edge
        assert!(text.contains("May 2025 catalog"));
        assert!(text.contains("Fastener drilling unavailable"));

        app.editor
            .transact(|p| -> Result<(), ()> {
                p.hinge_installations[0].overlay = Length::from_micrometres(17_000);
                Ok(())
            })
            .unwrap();
        let text = inspector_text(&mut app, id);
        assert!(text.contains("Unsupported K / overlay pair"));
        assert!(!text.contains("Board-local reference diagram"));
        assert!(!text.contains("79.500"));
        app.editor.undo().unwrap();

        app.editor
            .transact(|p| -> Result<(), ()> {
                p.boards[0].thickness = Length::from_micrometres(14_000);
                Ok(())
            })
            .unwrap();
        let text = inspector_text(&mut app, id);
        assert!(text.contains("Actual door thickness: 14.000 mm"));
        assert!(text.contains("Numeric installation guidance unavailable"));
        assert!(!text.contains("79.500"));
        assert!(!text.contains("Board-local reference diagram"));

        app.editor
            .transact(|p| -> Result<(), ()> {
                p.catalog[0].verified_hinge = None;
                Ok(())
            })
            .unwrap();
        let text = inspector_text(&mut app, id);
        assert!(text.contains("Verified installation evidence unavailable"));
        assert!(!text.contains("Documented K/R pairs"));
        assert!(!text.contains("Ø35.000"));
    }

    #[test]
    fn independent_draft_coordinates_and_pairs_never_commit_on_preview() {
        let app = fixture();
        let before = app.editor.project().clone();
        let mut draft = HingeDialog::new(&app, None);
        draft.values[0] = "60".into();
        draft.values[1] = "45".into();
        draft.values[2] = "6".into();
        draft.values[3] = "18".into();
        draft.side.mount_face = BoardFace::MaxZ;
        let status =
            hinge_installation::preview(app.editor.project(), &draft.proposed(&app).unwrap())
                .unwrap();
        assert!(status.issues.is_empty());
        assert_eq!(
            status.references.unwrap().plate_hole_centers_um[0][1],
            29_000
        );
        draft.values[3] = "17".into();
        let status =
            hinge_installation::preview(app.editor.project(), &draft.proposed(&app).unwrap())
                .unwrap();
        assert!(
            status
                .issues
                .contains(&InstallationIssue::UnsupportedOverlay)
        );
        assert!(status.references.is_none());
        draft.values[0] = "1/64 in".into();
        assert!(draft.proposed(&app).is_none());
        assert_eq!(app.editor.project(), &before);
    }
}
