use crate::actions::{ActionId as A, Request, Target};
use crate::icons::Icon;
use crate::theme_widgets as tw;
use crate::*;
use plan_my_cabinet::domain::{
    BoardEdge, BoardFace, CatalogReference, HingeInstallation, HingeMountingSide, Project,
};
use plan_my_cabinet::hinge_installation::{self, FitError, InstallationIssue, InstallationStatus};

pub(crate) struct HingeDialog {
    id: Option<Uuid>,
    new_id: Uuid,
    project_id: Uuid,
    revision: u64,
    door: Option<Uuid>,
    mount: Option<Uuid>,
    catalog: Option<Uuid>,
    side: HingeMountingSide,
    values: [String; 4],
    /// E for inset arms: side front edge to the door's inside face.
    inset_depth: String,
    /// Derive the edges, faces and plate Y from the model.
    auto: bool,
    /// Suggest the door position until it is typed (new hinges only).
    door_y_auto: bool,
    /// Suggest the cabinet side from the door until one is picked.
    mount_auto: bool,
    fit_error: Option<FitError>,
    error: bool,
    chrome: ModalChrome,
}

/// Draft text for a committed length: exact, without needless zeros.
fn mm(value: Length) -> String {
    assembly_ui::short_length(value, Locale::En)
}

fn distance(text: &str) -> Option<Length> {
    let value = parse_length(text, Unit::Mm).ok()?.conversion.exact()?;
    (value.micrometres() >= 0).then_some(value)
}

fn locale(localizer: &Localizer) -> Locale {
    if localizer.language() == Language::En {
        Locale::En
    } else {
        Locale::PtBr
    }
}

/// Display text for a length in mm (`100`, `11.3`), localized decimal mark.
pub(crate) fn short_mm(localizer: &Localizer, value: Length) -> String {
    assembly_ui::short_length(value, locale(localizer))
}

fn short_um(localizer: &Localizer, value: i128) -> String {
    short_mm(
        localizer,
        Length::from_micrometres(value.clamp(i64::MIN as i128, i64::MAX as i128) as i64),
    )
}

fn edge_label(localizer: &Localizer, edge: BoardEdge) -> String {
    localizer.text(match edge {
        BoardEdge::MinX => "hinge-min-x",
        BoardEdge::MaxX => "hinge-max-x",
        BoardEdge::MinY => "hinge-min-y",
        BoardEdge::MaxY => "hinge-max-y",
    })
}

fn face_label(localizer: &Localizer, face: BoardFace) -> String {
    localizer.text(match face {
        BoardFace::MinZ => "hinge-min-z",
        BoardFace::MaxZ => "hinge-max-z",
    })
}

fn short_edge(localizer: &Localizer, edge: BoardEdge) -> String {
    localizer.text(match edge {
        BoardEdge::MinX => "hardware-edge-min-x",
        BoardEdge::MaxX => "hardware-edge-max-x",
        BoardEdge::MinY => "hardware-edge-min-y",
        BoardEdge::MaxY => "hardware-edge-max-y",
    })
}

fn short_face(localizer: &Localizer, face: BoardFace) -> String {
    localizer.text(match face {
        BoardFace::MinZ => "hardware-face-min-z",
        BoardFace::MaxZ => "hardware-face-max-z",
    })
}

fn board_name(project: &Project, id: Uuid) -> &str {
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
}

/// The tree, inspector and 3D labels share one ordinal, never a UUID prefix.
pub(crate) fn hinge_ordinal(project: &Project, id: Uuid) -> usize {
    project
        .hinge_installations
        .iter()
        .position(|h| h.id == id)
        .unwrap_or(0)
        + 1
}

fn hinge_name(localizer: &Localizer, project: &Project, id: Uuid) -> String {
    format!(
        "{} {}",
        localizer.text("hardware-hinge"),
        hinge_ordinal(project, id)
    )
}

fn short_hw_id(id: Uuid) -> String {
    let hex = id.simple().to_string();
    format!("hw-{}", &hex[hex.len() - 4..])
}

/// "FGVTN Click 3D Slow Reta / Calço 0 (complete kit)" → "FGVTN Click 3D Slow Reta".
fn kit_short_name(name: &str) -> &str {
    name.split(" / ").next().unwrap_or(name).trim()
}

/// "May 2025 catalog; modified …; SHA-256 …" → "May 2025".
fn short_revision(revision: &str) -> String {
    let first = revision.split(';').next().unwrap_or(revision).trim();
    first
        .strip_suffix(" catalog")
        .unwrap_or(first)
        .trim()
        .to_owned()
}

fn mono_font(ui: &egui::Ui, size: f32, weight: crate::theme::Typeface) -> egui::FontId {
    if tw::weights_available(ui) {
        egui::FontId::new(size, weight.family())
    } else {
        egui::FontId::monospace(size)
    }
}

/// 12px muted label placed above a dialog field.
pub(crate) fn field_label(ui: &mut egui::Ui, text: &str) {
    ui.add(
        egui::Label::new(egui::RichText::new(text).size(12.0).color(tw::MUTED))
            .selectable(false)
            .truncate(),
    );
}

/// Frameless 11.5px link-style action with an accessible name.
fn small_link(ui: &mut egui::Ui, text: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(tw::medium(ui, text, 11.5).color(tw::ACCENT_DARK)).frame(false),
    )
}

fn row_icon(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    color: egui::Color32,
    enabled: bool,
) -> egui::Response {
    tw::ghost_icon_sized(ui, icon, label, color, 13.0, 22.0, enabled, false)
}

/// Warning callout with a leading `warning` icon. `white` uses the card fill
/// (tree summaries); otherwise the `warn_bg` fill (inspector notices).
pub(crate) fn warning_callout<R>(
    ui: &mut egui::Ui,
    white: bool,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    tw::warn_callout()
        .fill(if white { tw::CARD } else { tw::WARN_BG })
        .inner_margin(egui::Margin::symmetric(10, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 9.0;
                ui.add_space(0.0);
                ui.vertical(|ui| {
                    ui.add_space(2.0);
                    ui.add(crate::icons::icon(Icon::Warning, tw::WARN, 14.0));
                });
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    body(ui)
                })
                .inner
            })
            .inner
        })
        .inner
}

pub(crate) fn warn_text(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    ui.add(
        egui::Label::new(
            egui::RichText::new(text.into())
                .size(12.0)
                .color(egui::Color32::from_rgb(92, 62, 16)),
        )
        .wrap(),
    )
}

/// A field-looking chooser that opens the installation dialog.
fn chooser_field(ui: &mut egui::Ui, text: &str, hint: &str, enabled: bool) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(width, 28.0),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, text));
    let response = response.on_hover_text(hint);
    if ui.is_rect_visible(rect) {
        let hovered = enabled && response.hovered();
        ui.painter().rect(
            rect,
            6.0,
            if hovered { tw::HOVER_ROW } else { tw::APP },
            egui::Stroke::new(
                1.0,
                if response.has_focus() {
                    tw::FOCUS
                } else {
                    tw::BORDER_SOFT
                },
            ),
            egui::StrokeKind::Inside,
        );
        let chevron = egui::Rect::from_center_size(
            rect.right_center() - egui::vec2(14.0, 0.0),
            egui::Vec2::splat(11.0),
        );
        crate::icons::icon(Icon::ChevDown, tw::MUTED, 11.0).paint_at(ui, chevron);
        let text_rect = egui::Rect::from_min_max(
            rect.min + egui::vec2(8.0, 0.0),
            rect.max - egui::vec2(26.0, 0.0),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(text_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        child.add(
            egui::Label::new(egui::RichText::new(text).size(13.0).color(tw::TEXT))
                .truncate()
                .selectable(false),
        );
    }
    response
}

/// The documented K/R pairs as one segmented control. Returns the chosen pair.
pub(crate) fn pair_segmented(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    pairs: &[(Length, Length)],
    current: (Length, Length),
    enabled: bool,
) -> Option<(Length, Length)> {
    let mut chosen = None;
    let width = ui.available_width();
    let (outer, _) = ui.allocate_exact_size(egui::vec2(width, 30.0), egui::Sense::hover());
    ui.painter().rect_filled(outer, 7.0, tw::VIEWPORT);
    let inner = outer.shrink(2.0);
    let count = pairs.len().max(1) as f32;
    let cell = (inner.width() - 2.0 * (count - 1.0)) / count;
    for (index, &(k, r)) in pairs.iter().enumerate() {
        let rect = egui::Rect::from_min_size(
            inner.min + egui::vec2(index as f32 * (cell + 2.0), 0.0),
            egui::vec2(cell, inner.height()),
        );
        let selected = (k, r) == current;
        let label = format!("{} / {}", short_mm(localizer, k), short_mm(localizer, r));
        let response = ui.interact(
            rect,
            ui.id().with(("hinge-pair", index)),
            if enabled {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, enabled, selected, &label)
        });
        if selected {
            ui.painter().rect_filled(
                rect.translate(egui::vec2(0.0, 1.0)),
                5.0,
                egui::Color32::from_rgba_unmultiplied(60, 45, 25, 22),
            );
            ui.painter().rect_filled(rect, 5.0, tw::PANEL);
        } else if enabled && response.hovered() {
            ui.painter().rect_filled(rect, 5.0, tw::HOVER_ROW);
        }
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            &label,
            mono_font(
                ui,
                12.0,
                if selected {
                    crate::theme::Typeface::MonoMedium
                } else {
                    crate::theme::Typeface::Mono
                },
            ),
            if selected { tw::TEXT } else { tw::SECONDARY },
        );
        if response.clicked() && !selected {
            chosen = Some((k, r));
        }
    }
    chosen
}

/// Small painted cup/plate reference (not to scale, not a drilling template).
fn cup_plate_diagram(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    installation: &HingeInstallation,
    r: &hinge_installation::InstallationReferences,
) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(88.0, 110.0), egui::Sense::hover());
    response.on_hover_text(localizer.text("hinge-diagram-not-scale"));
    let p = ui.painter_at(rect.expand(1.0));
    let ink = tw::ACCENT_DARK;
    p.rect(
        rect,
        0.0,
        egui::Color32::from_rgb(239, 230, 214),
        egui::Stroke::new(1.5, egui::Color32::from_rgb(156, 144, 126)),
        egui::StrokeKind::Inside,
    );
    let o = rect.min;
    let cup = o + egui::vec2(18.0, 34.0);
    p.circle(
        cup,
        14.0,
        egui::Color32::from_rgb(244, 194, 122),
        egui::Stroke::new(1.5, ink),
    );
    p.line_segment(
        [o + egui::vec2(4.0, 4.0), o + egui::vec2(4.0, 18.0)],
        egui::Stroke::new(1.0, ink),
    );
    p.text(
        o + egui::vec2(36.0, 29.0),
        egui::Align2::LEFT_CENTER,
        format!("Ø{}", short_mm(localizer, r.cup_diameter)),
        egui::FontId::monospace(10.0),
        ink,
    );
    p.line_segment(
        [o + egui::vec2(1.0, 53.0), o + egui::vec2(5.0, 53.0)],
        egui::Stroke::new(1.0, ink),
    );
    p.text(
        o + egui::vec2(5.0, 57.0),
        egui::Align2::LEFT_TOP,
        format!("K {}", short_mm(localizer, installation.cup_edge_setback)),
        egui::FontId::monospace(9.5),
        tw::MUTED,
    );
    for y in [72.0, 94.0] {
        p.circle_filled(o + egui::vec2(38.0, y), 2.2, tw::MUTED);
    }
    p.text(
        o + egui::vec2(46.0, 83.0),
        egui::Align2::LEFT_CENTER,
        if r.plate_hole_pitch == Length::ZERO {
            "—".to_owned()
        } else {
            short_mm(localizer, r.plate_hole_pitch)
        },
        egui::FontId::monospace(9.5),
        tw::MUTED,
    );
}

/// "32 mm apart", or "not documented" when the source gives no pitch.
fn plate_pitch_text(
    localizer: &Localizer,
    r: &hinge_installation::InstallationReferences,
) -> String {
    if r.plate_hole_pitch == Length::ZERO {
        localizer.text("catalogs-unknown")
    } else {
        format!(
            "{} {}",
            short_mm(localizer, r.plate_hole_pitch),
            localizer.text("hardware-holes-apart")
        )
    }
}

/// Front offset, or "37 + E 18 = 55" for an inset arm.
fn plate_front_text(
    localizer: &Localizer,
    r: &hinge_installation::InstallationReferences,
) -> String {
    if r.arm.is_inset() {
        format!(
            "{} + E {} = {}",
            short_mm(localizer, r.plate_front_offset),
            short_mm(localizer, r.inset_depth),
            short_mm(
                localizer,
                Length::from_micrometres(
                    r.plate_front_offset.micrometres() + r.inset_depth.micrometres()
                )
            )
        )
    } else {
        short_mm(localizer, r.plate_front_offset)
    }
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

fn verified_facts(entry: &CatalogReference) -> Option<&plan_my_cabinet::domain::VerifiedHinge> {
    entry
        .verified_hinge
        .as_ref()
        .filter(|_| hardware_catalog::is_verified(entry))
}

/// Pinned-catalog summary card: name, mono SKUs, door range and revision.
fn catalog_card(ui: &mut egui::Ui, localizer: &Localizer, entry: &CatalogReference) {
    egui::Frame::new()
        .fill(tw::APP)
        .corner_radius(8)
        .inner_margin(10)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.add(crate::icons::icon(Icon::Hinge, tw::MUTED, 15.0));
                ui.add(
                    egui::Label::new(
                        tw::medium(ui, kit_short_name(&entry.name), 13.0).color(tw::TEXT),
                    )
                    .truncate()
                    .selectable(false),
                )
                .on_hover_text(&entry.name);
            });
            ui.add(
                egui::Label::new(
                    tw::mono(
                        format!(
                            "{} · {} {}",
                            entry.product_id,
                            localizer.text("hardware-plate-word"),
                            entry.plate_id.as_deref().unwrap_or("—")
                        ),
                        10.5,
                    )
                    .color(tw::FAINT),
                )
                .wrap(),
            );
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                crate::catalog_ui::trust_chip(ui, localizer, hardware_catalog::trust(entry));
                if let Some(facts) = hardware_catalog::facts(entry) {
                    tw::chip(
                        ui,
                        &crate::catalog_ui::arm_label(localizer, facts.arm),
                        tw::VIEWPORT,
                        tw::MUTED,
                    );
                }
                if let Some(origin) = &entry.origin {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} · {}",
                            origin.manufacturer, origin.pack_version
                        ))
                        .size(11.0)
                        .color(tw::FAINT),
                    );
                }
            });
            let revision = format!(
                "{} {}",
                localizer.text("hardware-rev-short"),
                short_revision(&entry.revision)
            );
            if let Some(facts) = verified_facts(entry) {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "H={} · {} {}–{} mm",
                            short_mm(localizer, facts.plate_height),
                            localizer.text("hardware-door-word"),
                            short_mm(localizer, facts.door_thickness_min),
                            short_mm(localizer, facts.door_thickness_max)
                        ))
                        .size(11.0)
                        .color(tw::MUTED),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&revision).size(11.0).color(tw::MUTED),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&entry.revision);
                    });
                });
            } else {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(localizer.text("hinge-evidence-unavailable"))
                            .size(11.5)
                            .color(tw::WARN_INK),
                    )
                    .wrap(),
                );
                ui.add(
                    egui::Label::new(egui::RichText::new(&revision).size(11.0).color(tw::MUTED))
                        .truncate(),
                )
                .on_hover_text(&entry.revision);
            }
        });
}

impl HingeDialog {
    pub(crate) fn new(app: &DesktopApp, id: Option<Uuid>) -> Self {
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
        // A new hinge starts on the selected board, hung from its likely side.
        let door = old.map(|i| i.door_board_id).or_else(|| {
            app.selection
                .active
                .filter(|id| boards.iter().any(|b| b.id == *id))
                .or_else(|| boards.first().map(|b| b.id))
        });
        let mount = old.map(|i| i.mounting_board_id).or_else(|| {
            door.and_then(|door| hinge_installation::likely_mount(project, door))
                .or_else(|| boards.iter().find(|b| Some(b.id) != door).map(|b| b.id))
        });
        let auto = old.is_none_or(|old| {
            hinge_installation::fitted(project, old)
                .is_ok_and(|fit| fit.side == old.side && fit.mount_y == old.mount_y)
        });
        let mut dialog = Self {
            id,
            new_id: Uuid::new_v4(),
            project_id: project.id,
            revision: project.revision,
            door,
            mount,
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
            inset_depth: old.map_or_else(
                || {
                    old.map(|i| i.door_board_id)
                        .or_else(|| boards.first().map(|b| b.id))
                        .and_then(|id| boards.iter().find(|b| b.id == id))
                        .map_or_else(|| "18".into(), |door| mm(door.thickness))
                },
                |i| mm(i.inset_depth),
            ),
            auto,
            door_y_auto: old.is_none(),
            mount_auto: old.is_none(),
            fit_error: None,
            error: false,
            chrome: ModalChrome::new(egui::Id::new("hinge-dialog"))
                .first_focus(egui::Id::new(("hinge-value-field", 0)))
                .width(560.0),
        };
        dialog.refit(project);
        dialog
    }

    /// Re-derive what is automatic: the side for a new door, the door
    /// position, then the edges, faces and plate Y from the model.
    fn refit(&mut self, project: &Project) {
        self.fit_error = None;
        if !self.auto {
            return;
        }
        let (Some(door), Some(mount)) = (self.door, self.mount) else {
            self.fit_error = Some(FitError::MissingPart);
            return;
        };
        let Some(board) = project.boards.iter().find(|b| b.id == door) else {
            self.fit_error = Some(FitError::MissingPart);
            return;
        };
        if self.door_y_auto {
            let probe = |y: Length| hinge_installation::fit(project, door, mount, y).ok();
            let side = [board.width, board.length]
                .iter()
                .find_map(|half| probe(Length::from_micrometres(half.micrometres() / 2)));
            if let Some((side, _)) = side {
                let taken: Vec<_> = project
                    .hinge_installations
                    .iter()
                    .filter(|h| h.door_board_id == door && Some(h.id) != self.id)
                    .map(|h| h.door_y)
                    .collect();
                let edge =
                    hinge_installation::edge_length(side.door_edge, board.length, board.width);
                self.values[0] = mm(hinge_installation::next_position(edge, &taken));
            }
        }
        let Some(door_y) = distance(&self.values[0]) else {
            return;
        };
        match hinge_installation::fit(project, door, mount, door_y) {
            Ok((side, mount_y)) => {
                self.side = side;
                self.values[1] = mm(mount_y);
            }
            Err(error) => self.fit_error = Some(error),
        }
    }

    #[cfg(test)]
    pub(crate) fn catalog_id(&self) -> Option<Uuid> {
        self.catalog
    }

    /// Test seam: K, the table value (R or F) and E as typed.
    #[cfg(test)]
    pub(crate) fn set_values(&mut self, k: &str, value: &str, inset_depth: &str) {
        self.values[2] = k.into();
        self.values[3] = value.into();
        self.inset_depth = inset_depth.into();
    }

    pub(crate) fn proposed(&self, app: &DesktopApp) -> Option<HingeInstallation> {
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
        let inset = project
            .catalog
            .iter()
            .find(|c| c.id == catalog)
            .and_then(hardware_catalog::facts)
            .is_some_and(|facts| facts.arm.is_inset());
        let inset_depth = if inset {
            distance(&self.inset_depth)?
        } else {
            Length::ZERO
        };
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
            inset_depth,
        })
    }
}

pub(crate) fn issue_key(issue: &InstallationIssue) -> &'static str {
    match issue {
        InstallationIssue::MissingPart(_) => "hinge-missing-part",
        InstallationIssue::MissingCatalog(_) | InstallationIssue::MissingVerifiedCatalog => {
            "hinge-missing-catalog"
        }
        InstallationIssue::UnsupportedThickness => "hinge-thickness-warning",
        InstallationIssue::UnsupportedOverlay => "hinge-overlay-warning",
        InstallationIssue::CupOutsideDoor => "hinge-cup-warning",
        InstallationIssue::PlateOutsideMount => "hinge-plate-warning",
        InstallationIssue::InsetShallowerThanDoor => "hinge-inset-depth-warning",
    }
}

/// Mono label/value grid used by the inspector card and the dialog summary.
/// Values keep their full width; labels truncate (with a tooltip) if needed.
fn value_grid(
    ui: &mut egui::Ui,
    _id: impl std::hash::Hash,
    rows: &[(String, String, egui::Color32)],
) {
    let font = egui::FontId::monospace(11.5);
    let value_width = rows
        .iter()
        .map(|(_, value, _)| {
            ui.painter()
                .layout_no_wrap(value.clone(), font.clone(), tw::TEXT)
                .size()
                .x
        })
        .fold(0.0_f32, f32::max);
    let width = ui.available_width();
    let label_width = (width - value_width - 8.0).max(24.0);
    ui.spacing_mut().item_spacing.y = 5.0;
    for (label, value, color) in rows {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 17.0), egui::Sense::hover());
        let mut left = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_min_size(
                    rect.min,
                    egui::vec2(label_width, rect.height()),
                ))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        left.add(
            egui::Label::new(egui::RichText::new(label).size(12.0).color(tw::MUTED))
                .truncate()
                .selectable(false),
        );
        ui.painter().text(
            egui::pos2(rect.right() - value_width, rect.center().y),
            egui::Align2::LEFT_CENTER,
            value,
            font.clone(),
            *color,
        );
    }
}

/// Stacked label-over-value rows for longer datums.
fn value_list(ui: &mut egui::Ui, rows: &[(String, String, egui::Color32)]) {
    for (label, value, color) in rows {
        ui.add_space(3.0);
        ui.label(egui::RichText::new(label).size(11.5).color(tw::MUTED));
        ui.add(egui::Label::new(tw::mono(value, 12.0).color(*color)).wrap());
    }
}

/// Door-thickness row: documented range check with an `ok` tick.
fn thickness_row(
    localizer: &Localizer,
    project: &Project,
    installation: &HingeInstallation,
) -> Option<(String, String, egui::Color32)> {
    let door = project
        .boards
        .iter()
        .find(|b| b.id == installation.door_board_id)?;
    let facts = project
        .catalog
        .iter()
        .find(|c| c.id == installation.catalog_id)
        .and_then(verified_facts);
    let supported = facts.is_some_and(|f| {
        door.thickness >= f.door_thickness_min && door.thickness <= f.door_thickness_max
    });
    let value = short_mm(localizer, door.thickness);
    Some((
        localizer.text("hardware-door-thk"),
        if supported {
            format!("{value} ✓")
        } else {
            value
        },
        if supported { tw::OK } else { tw::WARN_INK },
    ))
}

/// Compact dialog preview: warnings, numeric references and the schematic.
fn status_ui(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    project: &Project,
    status: &InstallationStatus,
    proposed: &HingeInstallation,
) {
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
    let catalog = project.catalog.iter().find(|c| c.id == proposed.catalog_id);
    if !status.issues.is_empty()
        || catalog.is_some_and(|entry| !hardware_catalog::is_verified(entry))
    {
        warning_callout(ui, false, |ui| {
            for issue in &status.issues {
                warn_text(ui, localizer.text(issue_key(issue)));
            }
            if catalog.is_some_and(|entry| !hardware_catalog::is_verified(entry)) {
                warn_text(ui, localizer.text("hinge-evidence-unavailable"));
            }
            if !status.issues.is_empty() {
                ui.label(
                    egui::RichText::new(localizer.text("hinge-reference-withheld"))
                        .size(11.5)
                        .color(tw::MUTED),
                );
            }
        });
    }
    if status.issues.is_empty()
        && let Some(r) = &status.references
    {
        field_label(ui, &localizer.text("hinge-reference-diagram"));
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            reference_diagram(ui, localizer, project, proposed, r);
            ui.vertical(|ui| {
                let to = |v: i128| short_um(localizer, v);
                value_grid(
                    ui,
                    "hinge-dialog-references",
                    &[
                        (
                            localizer.text("hardware-cup"),
                            format!(
                                "Ø{} × {}",
                                short_mm(localizer, r.cup_diameter),
                                short_mm(localizer, r.cup_depth)
                            ),
                            tw::TEXT,
                        ),
                        (
                            localizer.text("hardware-datum-cup"),
                            format!(
                                "X {} · Y {} · Z {}",
                                to(r.cup_center_um[0]),
                                to(r.cup_center_um[1]),
                                to(r.cup_center_um[2])
                            ),
                            tw::TEXT,
                        ),
                        (
                            localizer.text("hardware-plate-holes"),
                            format!(
                                "X {} · Y {} / {}",
                                to(r.plate_hole_centers_um[0][0]),
                                to(r.plate_hole_centers_um[0][1]),
                                to(r.plate_hole_centers_um[1][1])
                            ),
                            tw::TEXT,
                        ),
                        (
                            localizer.text("hardware-plate-front"),
                            plate_front_text(localizer, r),
                            tw::TEXT,
                        ),
                    ],
                );
            });
        });
    }
    ui.horizontal_wrapped(|ui| {
        ui.add(crate::icons::icon(Icon::Warning, tw::WARN, 12.0));
        ui.label(
            egui::RichText::new(localizer.text("hardware-fasteners-title"))
                .size(11.5)
                .color(tw::WARN_INK),
        )
        .on_hover_text(localizer.text("hardware-fasteners-detail"));
        if catalog.is_some_and(hardware_catalog::is_verified) {
            ui.hyperlink_to(
                egui::RichText::new(localizer.text("hinge-source-review"))
                    .size(11.5)
                    .color(tw::ACCENT_DARK),
                hardware_catalog::SOURCE_URL,
            );
        }
    });
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
    let width = 200.0_f32.min(ui.available_width()).max(80.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 150.0), egui::Sense::hover());
    response.on_hover_text(localizer.text("hinge-diagram-not-scale"));
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 8.0, tw::APP);
    let ink = tw::MUTED;
    let accent = tw::ACCENT;
    let board_w = width - 24.0;
    let point = |board: &plan_my_cabinet::domain::Board, origin: egui::Pos2, center: [i128; 3]| {
        let x = center[0] as f32 / board.length.micrometres().max(1) as f32;
        let y = center[1] as f32 / board.width.micrometres().max(1) as f32;
        egui::pos2(origin.x + x * board_w, origin.y + y * 40.0)
    };
    let draw_board = |origin: egui::Pos2| {
        let area = egui::Rect::from_min_size(origin, egui::vec2(board_w, 40.0));
        p.rect(
            area,
            2.0,
            tw::CARD,
            egui::Stroke::new(1.0, tw::BORDER_STRONG),
            egui::StrokeKind::Inside,
        );
    };
    let door_origin = rect.min + egui::vec2(12.0, 22.0);
    let mount_origin = rect.min + egui::vec2(12.0, 96.0);
    draw_board(door_origin);
    draw_board(mount_origin);
    let cup = point(door, door_origin, r.cup_center_um);
    let plate_a = point(mount, mount_origin, r.plate_hole_centers_um[0]);
    let plate_b = point(mount, mount_origin, r.plate_hole_centers_um[1]);
    p.circle(
        cup,
        7.0,
        egui::Color32::from_rgb(244, 194, 122),
        egui::Stroke::new(1.5, tw::ACCENT_DARK),
    );
    p.circle_filled(plate_a, 2.5, accent);
    p.circle_filled(plate_b, 2.5, accent);
    p.line_segment([plate_a, plate_b], egui::Stroke::new(1.0, accent));
    for (origin, key, edge, face) in [
        (
            door_origin,
            "hardware-door",
            installation.side.door_edge,
            r.cup_face,
        ),
        (
            mount_origin,
            "hardware-cabinet",
            installation.side.mount_front_edge,
            r.plate_face,
        ),
    ] {
        p.text(
            origin - egui::vec2(0.0, 4.0),
            egui::Align2::LEFT_BOTTOM,
            format!(
                "{} · {} · {}",
                localizer.text(key),
                short_edge(localizer, edge),
                short_face(localizer, face)
            ),
            egui::FontId::proportional(10.5),
            ink,
        );
    }
}

/// Per-installation draft text for the inspector's Y fields (session only).
fn y_draft_id(id: Uuid, index: usize) -> egui::Id {
    egui::Id::new(("hinge-inspector-y", id, index))
}

impl DesktopApp {
    pub(crate) fn show_pinned_catalog(&mut self, ui: &mut egui::Ui) {
        let project = self.editor.project();
        let browse_id = egui::Id::new(("hardware-browse-snapshots", project.id));
        let mut browse = ui.data(|d| d.get_temp::<bool>(browse_id)).unwrap_or(false);
        let entries = project.catalog.clone();
        let selected = match self.session.inspector {
            Some(InspectorTarget::Installation(id)) => {
                project.hinge_installations.iter().find(|h| h.id == id)
            }
            _ => None,
        };
        let pinned = selected
            .map_or_else(
                || entries.first(),
                |hinge| entries.iter().find(|entry| entry.id == hinge.catalog_id),
            )
            .cloned();
        let modal = self.modal_open();
        let mut run = None;
        egui::Frame::new()
            .inner_margin(egui::Margin {
                left: 14,
                right: 10,
                top: 0,
                bottom: 0,
            })
            .show(ui, |ui| {
                tw::section_bar(ui, &self.localizer.text("hardware-pinned-catalog"), |ui| {
                    if !entries.is_empty()
                        && small_link(ui, &self.localizer.text("catalog-browse"), true)
                            .on_hover_text(self.localizer.text("catalog-heading"))
                            .clicked()
                    {
                        browse = !browse;
                    }
                });
            });
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(8, 0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if let Some(entry) = &pinned {
                    catalog_card(ui, &self.localizer, entry);
                } else {
                    egui::Frame::new()
                        .fill(tw::APP)
                        .corner_radius(8)
                        .inner_margin(10)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                egui::RichText::new(self.localizer.text("hinge-missing-catalog"))
                                    .size(12.0)
                                    .color(tw::WARN_INK),
                            );
                            ui.add_space(4.0);
                            if tw::icon_text_button(
                                ui,
                                Icon::Plus,
                                &self.localizer.text("catalog-add"),
                                false,
                                !modal,
                            )
                            .clicked()
                            {
                                run = Some(Request::new(A::AddCatalog));
                            }
                        });
                }
                if browse && !entries.is_empty() {
                    ui.add_space(8.0);
                    for entry in &entries {
                        ui.push_id(entry.id, |ui| {
                            egui::Frame::new()
                                .fill(tw::CARD)
                                .stroke(egui::Stroke::new(1.0, tw::BORDER_SOFT))
                                .corner_radius(8)
                                .inner_margin(10)
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.spacing_mut().item_spacing.y = 4.0;
                                    snapshot_details(ui, &self.localizer, entry);
                                    ui.horizontal(|ui| {
                                        if tw::icon_text_button(
                                            ui,
                                            Icon::Redo,
                                            &self.localizer.text("catalog-update"),
                                            false,
                                            !modal,
                                        )
                                        .clicked()
                                        {
                                            run = Some(Request::with(
                                                A::UpdateCatalog,
                                                Target::Catalog(entry.id),
                                            ));
                                        }
                                    });
                                });
                            ui.add_space(6.0);
                        });
                    }
                    if tw::icon_text_button(
                        ui,
                        Icon::Plus,
                        &self.localizer.text("catalog-add"),
                        false,
                        !modal,
                    )
                    .clicked()
                    {
                        run = Some(Request::new(A::AddCatalog));
                    }
                    if let Some(notice) = &self.hardware.catalog_update_notice {
                        ui.label(egui::RichText::new(notice).size(11.5).color(tw::MUTED));
                    }
                }
            });
        ui.add_space(12.0);
        tw::divider(ui);
        ui.data_mut(|d| d.insert_temp(browse_id, browse));
        if let Some(request) = run {
            self.invoke_or_report(request);
        }
    }

    fn hardware_add_menu(&mut self, ui: &mut egui::Ui) {
        let mut run = None;
        for action in [
            A::NewHinge,
            A::NewDoor,
            A::NewSlides,
            A::NewFoot,
            A::NewHardware,
            A::AddCatalog,
        ] {
            let request = Request::new(action);
            let (icon, key) = match action {
                A::NewHinge => (Icon::Hinge, "hinge-new"),
                A::NewDoor => (Icon::Door, "door-add"),
                A::NewSlides => (Icon::Layers, "slide-new"),
                A::NewFoot => (Icon::Cube, "foot-new"),
                A::NewHardware => (Icon::Cube, "hardware-new"),
                _ => (Icon::Plus, "catalog-add"),
            };
            if ui
                .add_enabled(
                    self.action_availability(request).is_ok(),
                    egui::Button::image_and_text(
                        crate::icons::icon(icon, tw::MUTED, 14.0),
                        self.localizer.text(key),
                    )
                    .frame(false),
                )
                .clicked()
            {
                run = Some(request);
            }
        }
        if let Some(request) = run {
            ui.close();
            self.invoke_or_report(request);
        }
    }

    fn show_door_group_row(
        &mut self,
        ui: &mut egui::Ui,
        id: Uuid,
        first_hinge: Option<Uuid>,
    ) -> bool {
        let project = self.editor.project();
        let Some(door) = project.door_joints.iter().find(|d| d.id == id) else {
            return false;
        };
        let name = board_name(project, door.moving_root_id).to_owned();
        let mount = board_name(project, door.mounting_board_id).to_owned();
        let review = plan_my_cabinet::door_joint::needs_review(project, door);
        let collapsed_id = egui::Id::new(("hardware-door-collapsed", id));
        let mut collapsed = ui
            .data(|d| d.get_temp::<bool>(collapsed_id))
            .unwrap_or(false);
        let modal = self.modal_open();
        let mut args = FluentArgs::new();
        args.set("board", mount.clone());
        let on = self.localizer.format("hardware-door-on", Some(&args));
        let label = format!("{name} → {mount}");
        let edit = Request::with(A::EditDoor, Target::Door(id));
        let delete = Request::with(A::DeleteDoor, Target::Door(id));
        let preview = Request::with(A::StartMotion, Target::Door(id));
        let mut run = None;
        let (response, ()) = tw::list_row(
            ui,
            egui::Id::new(("hardware-door-row", id)),
            28.0,
            tw::RowState::Normal,
            !modal,
            &label,
            |ui| {
                let hovered = ui.rect_contains_pointer(ui.max_rect());
                if tw::ghost_icon_sized(
                    ui,
                    if collapsed {
                        Icon::ChevRight
                    } else {
                        Icon::ChevDown
                    },
                    &self.localizer.text(if collapsed {
                        "hardware-expand"
                    } else {
                        "hardware-collapse"
                    }),
                    tw::MUTED,
                    11.0,
                    16.0,
                    true,
                    false,
                )
                .clicked()
                {
                    collapsed = !collapsed;
                }
                ui.add(crate::icons::icon(Icon::Door, tw::MUTED, 14.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if hovered && !modal {
                        if row_icon(
                            ui,
                            Icon::Trash,
                            &A::DeleteDoor.label(&self.localizer),
                            tw::DANGER,
                            self.action_availability(delete).is_ok(),
                        )
                        .clicked()
                        {
                            run = Some(delete);
                        }
                        if row_icon(
                            ui,
                            Icon::Sliders,
                            &A::EditDoor.label(&self.localizer),
                            tw::SECONDARY,
                            self.action_availability(edit).is_ok(),
                        )
                        .clicked()
                        {
                            run = Some(edit);
                        }
                    } else {
                        ui.add(
                            egui::Label::new(egui::RichText::new(&on).size(11.0).color(tw::FAINT))
                                .truncate()
                                .selectable(false),
                        );
                    }
                    if review {
                        ui.add(crate::icons::icon(Icon::Warning, tw::WARN, 13.0))
                            .on_hover_text(self.localizer.text("door-review"));
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(tw::medium(ui, &name, 13.0).color(tw::TEXT))
                                .truncate()
                                .selectable(false),
                        );
                    });
                });
            },
        );
        let response = response.on_hover_text(&label);
        response.context_menu(|ui| {
            for (request, key) in [
                (preview, "door-motion-start"),
                (edit, "door-edit"),
                (delete, "door-delete"),
            ] {
                if ui
                    .add_enabled(
                        self.action_availability(request).is_ok(),
                        egui::Button::new(self.localizer.text(key)),
                    )
                    .clicked()
                {
                    run = Some(request);
                    ui.close();
                }
            }
        });
        if run.is_none() {
            if response.double_clicked() {
                run = Some(edit);
            } else if response.clicked()
                && let Some(first) = first_hinge
            {
                self.navigate_session(Destination::Installation(first));
            }
        }
        ui.data_mut(|d| d.insert_temp(collapsed_id, collapsed));
        if let Some(request) = run {
            self.invoke_or_report(request);
        }
        !collapsed
    }

    fn show_hinge_tree_row(&mut self, ui: &mut egui::Ui, id: Uuid, indent: f32) {
        let project = self.editor.project();
        let Some(installation) = project
            .hinge_installations
            .iter()
            .find(|h| h.id == id)
            .cloned()
        else {
            let _ = tw::list_row(
                ui,
                egui::Id::new(("hardware-hinge-row", id)),
                28.0,
                tw::RowState::Normal,
                false,
                &self.localizer.text("hinge-missing-part"),
                |ui| {
                    ui.add_space(indent);
                    ui.add(crate::icons::icon(Icon::Warning, tw::WARN, 13.0));
                    ui.label(
                        egui::RichText::new(self.localizer.text("hinge-missing-part"))
                            .size(12.0)
                            .color(tw::WARN_INK),
                    );
                },
            );
            return;
        };
        let name = hinge_name(&self.localizer, project, id);
        let label = format!(
            "{name} · {} → {} · Y {} / {} mm",
            board_name(project, installation.door_board_id),
            board_name(project, installation.mounting_board_id),
            short_mm(&self.localizer, installation.door_y),
            short_mm(&self.localizer, installation.mount_y)
        );
        let issues = hinge_installation::diagnose(project, &installation).issues;
        let issue_text = issues
            .iter()
            .map(|issue| self.localizer.text(issue_key(issue)))
            .collect::<Vec<_>>()
            .join("\n");
        let active = self.session.inspector == Some(InspectorTarget::Installation(id));
        let modal = self.modal_open();
        let y = format!("Y {}", short_mm(&self.localizer, installation.door_y));
        let edit = Request::with(A::EditHinge, Target::Hinge(id));
        let delete = Request::with(A::DeleteHinge, Target::Hinge(id));
        let mut run = None;
        let (response, ()) = tw::list_row(
            ui,
            egui::Id::new(("hardware-hinge-row", id)),
            28.0,
            if active {
                tw::RowState::Active
            } else {
                tw::RowState::Normal
            },
            !modal,
            &name,
            |ui| {
                let hovered = ui.rect_contains_pointer(ui.max_rect());
                ui.spacing_mut().item_spacing.x = 7.0;
                ui.add_space(indent);
                let (dot, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                ui.painter().circle_filled(
                    dot.center(),
                    4.0,
                    if active {
                        tw::ACCENT
                    } else {
                        egui::Color32::from_rgb(156, 144, 126)
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    if issues.is_empty() {
                        ui.label(tw::mono(&y, 11.0).color(if active {
                            egui::Color32::from_rgb(138, 84, 24)
                        } else {
                            tw::FAINT
                        }));
                    } else {
                        ui.add(crate::icons::icon(Icon::Warning, tw::WARN, 13.0))
                            .on_hover_text(&issue_text);
                    }
                    if hovered && !modal {
                        if row_icon(
                            ui,
                            Icon::Trash,
                            &A::DeleteHinge.label(&self.localizer),
                            tw::DANGER,
                            self.action_availability(delete).is_ok(),
                        )
                        .clicked()
                        {
                            run = Some(delete);
                        }
                        if row_icon(
                            ui,
                            Icon::Sliders,
                            &A::EditHinge.label(&self.localizer),
                            tw::SECONDARY,
                            self.action_availability(edit).is_ok(),
                        )
                        .clicked()
                        {
                            run = Some(edit);
                        }
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let text = if active {
                            tw::medium(ui, &name, 13.0).color(tw::ACCENT_INK)
                        } else {
                            egui::RichText::new(&name).size(13.0).color(tw::TEXT_2)
                        };
                        ui.add(egui::Label::new(text).truncate().selectable(false));
                    });
                });
            },
        );
        let response = response.on_hover_text(&label);
        response.context_menu(|ui| {
            for (request, key) in [(edit, "hinge-edit"), (delete, "hinge-delete")] {
                if ui
                    .add_enabled(
                        self.action_availability(request).is_ok(),
                        egui::Button::new(self.localizer.text(key)),
                    )
                    .clicked()
                {
                    run = Some(request);
                    ui.close();
                }
            }
        });
        if run.is_none() {
            if response.double_clicked() {
                run = Some(edit);
            } else if response.clicked() {
                self.navigate_session(Destination::Installation(id));
            }
        }
        if let Some(request) = run {
            self.invoke_or_report(request);
        }
    }

    pub(crate) fn show_hinge_list(&mut self, ui: &mut egui::Ui) {
        let modal = self.modal_open();
        let (doors, standalone) = installation_groups(self.editor.project());
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
                    egui::Id::new("hardware-doors-section"),
                    &self.localizer.text("hardware-doors"),
                    doors.len() + standalone.len(),
                    |ui| {
                        let plus = tw::ghost_icon_sized(
                            ui,
                            Icon::Plus,
                            &self.localizer.text("hardware-add-menu"),
                            tw::MUTED,
                            15.0,
                            24.0,
                            !modal,
                            false,
                        );
                        egui::Popup::menu(&plus)
                            .align(egui::RectAlign::BOTTOM_END)
                            .show(|ui| {
                                ui.set_min_width(220.0);
                                self.hardware_add_menu(ui);
                            });
                    },
                );
                open
            })
            .inner;
        if open {
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(6, 0))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    if doors.is_empty() && standalone.is_empty() {
                        egui::Frame::new()
                            .inner_margin(egui::Margin::symmetric(8, 4))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(self.localizer.text("hardware-no-hinges"))
                                        .size(12.0)
                                        .color(tw::FAINT),
                                );
                            });
                    }
                    for (id, children) in doors {
                        let expanded = self.show_door_group_row(ui, id, children.first().copied());
                        if expanded {
                            for child in children {
                                self.show_hinge_tree_row(ui, child, 16.0);
                            }
                        }
                    }
                    if !standalone.is_empty() {
                        egui::Frame::new()
                            .inner_margin(egui::Margin {
                                left: 8,
                                right: 8,
                                top: 8,
                                bottom: 2,
                            })
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(self.localizer.text("hardware-unassigned"))
                                        .size(11.0)
                                        .color(tw::FAINT),
                                );
                            });
                        for id in standalone {
                            self.show_hinge_tree_row(ui, id, 2.0);
                        }
                    }
                });
        }
        let project = self.editor.project();
        let mut warnings: Vec<String> = project
            .door_joints
            .iter()
            .filter(|door| plan_my_cabinet::door_joint::needs_review(project, door))
            .map(|door| {
                format!(
                    "{}: {}",
                    board_name(project, door.moving_root_id),
                    self.localizer.text("door-review")
                )
            })
            .collect();
        warnings.extend(project.hinge_installations.iter().flat_map(|hinge| {
            let name = hinge_name(&self.localizer, project, hinge.id);
            hinge_installation::diagnose(project, hinge)
                .issues
                .into_iter()
                .map(move |issue| (name.clone(), issue))
                .map(|(name, issue)| format!("{name}: {}", self.localizer.text(issue_key(&issue))))
        }));
        if !warnings.is_empty() {
            egui::Frame::new()
                .inner_margin(egui::Margin {
                    left: 8,
                    right: 8,
                    top: 12,
                    bottom: 0,
                })
                .show(ui, |ui| {
                    warning_callout(ui, true, |ui| {
                        for warning in warnings {
                            warn_text(ui, warning);
                        }
                    });
                });
        }
        ui.add_space(12.0);
    }

    /// Fixed footer of the Hardware controls pane: "+ Hinge" and "+ Door".
    pub(crate) fn show_hardware_footer(&mut self, ui: &mut egui::Ui) {
        let mut run = None;
        let gap = 6.0;
        let width = ((ui.available_width() - gap) / 2.0).max(40.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (action, icon, key) in [
                (A::NewHinge, Icon::Plus, "hardware-add-hinge"),
                (A::NewDoor, Icon::Door, "hardware-add-door"),
            ] {
                let request = Request::new(action);
                let enabled = self.action_availability(request).is_ok();
                ui.allocate_ui_with_layout(
                    egui::vec2(width, 32.0),
                    egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                    |ui| {
                        if tw::icon_text_button(ui, icon, &self.localizer.text(key), false, enabled)
                            .on_hover_text(action.label(&self.localizer))
                            .clicked()
                        {
                            run = Some(request);
                        }
                    },
                );
            }
        });
        if let Some(request) = run {
            self.invoke_or_report(request);
        }
    }

    /// Commit one inspector edit through the installation update path: one
    /// validated, undoable transaction or no change at all.
    fn apply_installation_edit(
        &mut self,
        id: Uuid,
        edit: impl FnOnce(&mut HingeInstallation),
    ) -> bool {
        let Some(mut installation) = self
            .editor
            .project()
            .hinge_installations
            .iter()
            .find(|i| i.id == id)
            .cloned()
        else {
            return false;
        };
        let before = installation.clone();
        edit(&mut installation);
        installation != before && hinge_installation::update(&mut self.editor, installation).is_ok()
    }

    /// One Y field in the inspector. Returns a validated value to commit.
    #[allow(clippy::too_many_arguments)]
    fn inspector_y_field(
        &self,
        ui: &mut egui::Ui,
        id: Uuid,
        index: usize,
        label: &str,
        committed: Length,
        suffix: &str,
        enabled: bool,
    ) -> Option<Length> {
        let draft_id = y_draft_id(id, index);
        let field_id = egui::Id::new(("hinge-inspector-y-field", id, index));
        let mut text = ui
            .data(|d| d.get_temp::<String>(draft_id))
            .unwrap_or_else(|| short_mm(&self.localizer, committed));
        let invalid = distance(&text).is_none();
        let mut result = None;
        tw::prop_row(ui, label, 84.0, |ui| {
            let width = ui.available_width();
            let response = tw::value_field(
                ui,
                field_id,
                label,
                &mut text,
                width,
                Some(suffix),
                None,
                enabled,
                invalid,
            );
            if response.changed() {
                ui.data_mut(|d| d.insert_temp(draft_id, text.clone()));
            }
            if response.lost_focus() {
                let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
                if !escape && let Some(value) = distance(&text).filter(|v| *v != committed) {
                    result = Some(value);
                }
                ui.data_mut(|d| d.remove::<String>(draft_id));
            }
        });
        result
    }

    /// Hardware inspector hook: call with the session's selected Installation ID.
    pub(crate) fn show_selected_installation_inspector(&mut self, ui: &mut egui::Ui, id: Uuid) {
        let project = self.editor.project();
        let Some(installation) = project
            .hinge_installations
            .iter()
            .find(|i| i.id == id)
            .cloned()
        else {
            egui::Frame::new().inner_margin(14).show(ui, |ui| {
                ui.label(
                    egui::RichText::new(self.localizer.text("hinge-selection-missing"))
                        .size(12.5)
                        .color(tw::MUTED),
                );
            });
            return;
        };
        let localizer = &self.localizer;
        let title = hinge_name(localizer, project, id);
        let door_name = board_name(project, installation.door_board_id).to_owned();
        let mount_name = board_name(project, installation.mounting_board_id).to_owned();
        let subline = format!("{} · {door_name} → {mount_name}", short_hw_id(id));
        let status = hinge_installation::diagnose(project, &installation);
        let entry = project
            .catalog
            .iter()
            .find(|c| c.id == installation.catalog_id)
            .cloned();
        let facts = entry.as_ref().and_then(verified_facts).cloned();
        let thickness = thickness_row(localizer, project, &installation);
        let edit = Request::with(A::EditHinge, Target::Hinge(id));
        let delete = Request::with(A::DeleteHinge, Target::Hinge(id));
        let edit_enabled = self.action_availability(edit).is_ok();
        let delete_enabled = self.action_availability(delete).is_ok();
        let editable = !self.modal_open();
        let mut run = None;
        let mut commit_y: [Option<Length>; 2] = [None, None];
        let mut commit_pair = None;
        let refit = hinge_installation::fitted(project, &installation).ok();
        let lined_up = refit
            .as_ref()
            .map_or(installation.mount_y == installation.door_y, |f| {
                f.side == installation.side && f.mount_y == installation.mount_y
            });
        // Spacing: offered once the door has two hinges off the standard spots.
        let door_hinges: Vec<_> = project
            .hinge_installations
            .iter()
            .filter(|h| h.door_board_id == installation.door_board_id)
            .map(|h| h.door_y)
            .collect();
        let edge = project
            .boards
            .iter()
            .find(|b| b.id == installation.door_board_id)
            .map(|door| {
                hinge_installation::edge_length(
                    installation.side.door_edge,
                    door.length,
                    door.width,
                )
            });
        let spacing = edge.filter(|&edge| {
            let mut current = door_hinges.clone();
            current.sort();
            current.len() >= 2
                && current != hinge_installation::standard_positions(edge, current.len())
        });
        let recommended = edge
            .map(hinge_installation::recommended_count)
            .filter(|&count| door_hinges.len() < count);
        enum Placement {
            LineUp,
            Space,
        }
        let mut placement = None;

        egui::Frame::new()
            .inner_margin(egui::Margin {
                left: 14,
                right: 10,
                top: 14,
                bottom: 12,
            })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let (tile, _) =
                        ui.allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::hover());
                    ui.painter().rect_filled(tile, 8.0, tw::ACCENT_BG);
                    crate::icons::icon(Icon::Hinge, egui::Color32::from_rgb(154, 91, 18), 18.0)
                        .paint_at(
                            ui,
                            egui::Rect::from_center_size(tile.center(), egui::Vec2::splat(18.0)),
                        );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        if tw::ghost_icon_sized(
                            ui,
                            Icon::Trash,
                            &A::DeleteHinge.label(localizer),
                            tw::MUTED,
                            15.0,
                            28.0,
                            delete_enabled,
                            false,
                        )
                        .clicked()
                        {
                            run = Some(delete);
                        }
                        ui.add_space(6.0);
                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.add(
                                egui::Label::new(tw::semibold(ui, &title, 15.0).color(tw::TEXT))
                                    .truncate()
                                    .selectable(false),
                            );
                            ui.add(
                                egui::Label::new(tw::mono(&subline, 11.0).color(tw::FAINT))
                                    .truncate(),
                            )
                            .on_hover_text(&subline);
                        });
                    });
                });
            });
        tw::divider(ui);

        egui::Frame::new()
            .inner_margin(egui::Margin {
                left: 14,
                right: 14,
                top: 2,
                bottom: 14,
            })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                tw::inspector_heading(ui, &localizer.text("hardware-mounting"), |_| {});
                ui.spacing_mut().item_spacing.y = 5.0;
                let hint = A::EditHinge.label(localizer);
                tw::prop_row(ui, &localizer.text("hardware-door"), 84.0, |ui| {
                    if chooser_field(
                        ui,
                        &format!(
                            "{door_name} · {}",
                            short_edge(localizer, installation.side.door_edge)
                        ),
                        &format!(
                            "{} · {} · {}",
                            edge_label(localizer, installation.side.door_edge),
                            face_label(localizer, installation.side.door_face),
                            hint
                        ),
                        edit_enabled,
                    )
                    .clicked()
                    {
                        run = Some(edit);
                    }
                });
                tw::prop_row(ui, &localizer.text("hardware-cabinet"), 84.0, |ui| {
                    if chooser_field(
                        ui,
                        &format!(
                            "{mount_name} · {}",
                            short_face(localizer, installation.side.mount_face)
                        ),
                        &format!(
                            "{} · {} · {}",
                            edge_label(localizer, installation.side.mount_front_edge),
                            face_label(localizer, installation.side.mount_face),
                            hint
                        ),
                        edit_enabled,
                    )
                    .clicked()
                    {
                        run = Some(edit);
                    }
                });
                commit_y[0] = self.inspector_y_field(
                    ui,
                    id,
                    0,
                    &localizer.text("hardware-position-y"),
                    installation.door_y,
                    &localizer.text(if installation.side.door_edge.along_axis() == 1 {
                        "hardware-from-y-edge"
                    } else {
                        "hardware-from-x-edge"
                    }),
                    editable,
                );
                let plate_differs =
                    !lined_up || ui.data(|d| d.get_temp::<String>(y_draft_id(id, 1)).is_some());
                if plate_differs {
                    commit_y[1] = self.inspector_y_field(
                        ui,
                        id,
                        1,
                        &localizer.text("hardware-plate-y"),
                        installation.mount_y,
                        "mm",
                        editable,
                    );
                }
                let line_up = !lined_up && refit.is_some();
                if line_up || spacing.is_some() || recommended.is_some() {
                    ui.add_space(2.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                        if line_up
                            && tw::secondary_button_enabled(
                                ui,
                                &localizer.text("hardware-line-up"),
                                editable,
                            )
                            .on_hover_text(localizer.text("hardware-line-up-hint"))
                            .clicked()
                        {
                            placement = Some(Placement::LineUp);
                        }
                        if spacing.is_some()
                            && tw::secondary_button_enabled(
                                ui,
                                &localizer.text("hardware-space-evenly"),
                                editable,
                            )
                            .on_hover_text(localizer.text("hardware-space-evenly-hint"))
                            .clicked()
                        {
                            placement = Some(Placement::Space);
                        }
                    });
                    if let Some(count) = recommended {
                        let mut args = FluentArgs::new();
                        args.set("count", count);
                        ui.label(
                            egui::RichText::new(
                                localizer.format("hardware-recommended-count", Some(&args)),
                            )
                            .size(11.5)
                            .color(tw::MUTED),
                        );
                    }
                }

                ui.add_space(4.0);
                tw::inspector_heading(ui, &localizer.text("hardware-setback-overlay"), |ui| {
                    ui.label(egui::RichText::new("mm").size(11.0).color(tw::MUTED));
                });
                let current = (installation.cup_edge_setback, installation.overlay);
                if let Some(facts) = &facts {
                    let pairs: Vec<_> = facts
                        .overlay_by_cup_edge
                        .iter()
                        .map(|p| (p.cup_edge_setback, p.overlay))
                        .collect();
                    commit_pair = pair_segmented(ui, localizer, &pairs, current, editable);
                } else {
                    ui.label(
                        tw::mono(
                            format!(
                                "{} / {}",
                                short_mm(localizer, current.0),
                                short_mm(localizer, current.1)
                            ),
                            12.0,
                        )
                        .color(tw::TEXT),
                    );
                }

                ui.add_space(12.0);
                let references = status
                    .references
                    .as_ref()
                    .filter(|_| status.issues.is_empty());
                if let Some(r) = references {
                    tw::card().inner_margin(12).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 10.0;
                            cup_plate_diagram(ui, localizer, &installation, r);
                            ui.vertical(|ui| {
                                let mut rows = vec![
                                    (
                                        localizer.text("hardware-cup"),
                                        format!(
                                            "Ø{} × {}",
                                            short_mm(localizer, r.cup_diameter),
                                            short_mm(localizer, r.cup_depth)
                                        ),
                                        tw::TEXT,
                                    ),
                                    (
                                        localizer.text("hardware-cup-edge"),
                                        short_um(
                                            localizer,
                                            i128::from(installation.cup_edge_setback.micrometres())
                                                + i128::from(r.cup_diameter.micrometres()) / 2,
                                        ),
                                        tw::TEXT,
                                    ),
                                    (
                                        localizer.text("hardware-plate-holes"),
                                        plate_pitch_text(localizer, r),
                                        tw::TEXT,
                                    ),
                                    (
                                        localizer.text("hardware-plate-front"),
                                        plate_front_text(localizer, r),
                                        tw::TEXT,
                                    ),
                                ];
                                rows.extend(thickness.clone());
                                value_grid(ui, ("installation-reference", id), &rows);
                            });
                        });
                    });
                } else {
                    warning_callout(ui, false, |ui| {
                        for issue in &status.issues {
                            warn_text(ui, localizer.text(issue_key(issue)));
                        }
                        if entry.is_some() && facts.is_none() {
                            warn_text(ui, localizer.text("hinge-evidence-unavailable"));
                        }
                        ui.label(
                            egui::RichText::new(localizer.text("hinge-reference-withheld"))
                                .size(11.5)
                                .color(tw::MUTED),
                        );
                    });
                    if let Some(row) = &thickness {
                        ui.add_space(8.0);
                        tw::card().inner_margin(12).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            value_grid(
                                ui,
                                ("installation-thickness", id),
                                std::slice::from_ref(row),
                            );
                        });
                    }
                }

                ui.add_space(12.0);
                warning_callout(ui, false, |ui| {
                    ui.add(
                        egui::Label::new(
                            tw::semibold(ui, localizer.text("hardware-fasteners-title"), 12.0)
                                .color(egui::Color32::from_rgb(92, 62, 16)),
                        )
                        .wrap(),
                    )
                    .on_hover_text(localizer.text("hardware-fasteners-detail"));
                });

                ui.add_space(8.0);
                egui::CollapsingHeader::new(
                    egui::RichText::new(localizer.text("hardware-reference-details"))
                        .size(12.0)
                        .color(tw::MUTED),
                )
                .id_salt(("installation-details", id))
                .show(ui, |ui| {
                    let to = |v: i128| short_um(localizer, v);
                    let mut rows = Vec::new();
                    if let Some(r) = references {
                        rows.push((
                            localizer.text("hardware-datum-cup"),
                            format!(
                                "X {} · Y {} · Z {}",
                                to(r.cup_center_um[0]),
                                to(r.cup_center_um[1]),
                                to(r.cup_center_um[2])
                            ),
                            tw::TEXT,
                        ));
                        rows.push((
                            localizer.text("hardware-plate-holes"),
                            format!(
                                "X {} · Y {} / {} · Z {}",
                                to(r.plate_hole_centers_um[0][0]),
                                to(r.plate_hole_centers_um[0][1]),
                                to(r.plate_hole_centers_um[1][1]),
                                to(r.plate_hole_centers_um[0][2])
                            ),
                            tw::TEXT,
                        ));
                    }
                    if let Some(door) = project
                        .boards
                        .iter()
                        .find(|b| b.id == installation.door_board_id)
                    {
                        rows.push((
                            localizer.text("hinge-current-thickness"),
                            format!("{} mm", short_mm(localizer, door.thickness)),
                            tw::TEXT,
                        ));
                    }
                    if let Some(facts) = &facts {
                        rows.push((
                            localizer.text("hinge-supported-thickness"),
                            format!(
                                "{}–{} mm",
                                short_mm(localizer, facts.door_thickness_min),
                                short_mm(localizer, facts.door_thickness_max)
                            ),
                            tw::TEXT,
                        ));
                        rows.push((
                            format!(
                                "{} (K/{})",
                                localizer.text("hinge-supported-pairs"),
                                crate::catalog_ui::table_letter(facts.arm)
                            ),
                            facts
                                .overlay_by_cup_edge
                                .iter()
                                .map(|p| {
                                    format!(
                                        "{}/{}",
                                        short_mm(localizer, p.cup_edge_setback),
                                        short_mm(localizer, p.overlay)
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join(" · "),
                            tw::TEXT,
                        ));
                    }
                    if let Some(entry) = &entry {
                        rows.push((
                            localizer.text("hardware-revision"),
                            entry
                                .revision
                                .split(';')
                                .next()
                                .unwrap_or("")
                                .trim()
                                .to_owned(),
                            tw::TEXT,
                        ));
                    }
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    value_list(ui, &rows);
                });
            });

        tw::divider(ui);
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(14, 12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some(entry) = &entry {
                        let link = ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.hyperlink_to(
                                    egui::RichText::new(localizer.text("hinge-source-review"))
                                        .size(11.5)
                                        .color(tw::ACCENT_DARK),
                                    &entry.source,
                                );
                                let source = facts.as_ref().map_or_else(
                                    || localizer.text("hinge-recorded-source"),
                                    |f| f.attribution.clone(),
                                );
                                ui.with_layout(
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(format!(
                                                    "{}: {source}",
                                                    localizer.text("hardware-source")
                                                ))
                                                .size(11.5)
                                                .color(tw::MUTED),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(&entry.source);
                                    },
                                );
                            },
                        );
                        let _ = link;
                    }
                });
            });

        if let Some(request) = run {
            self.invoke_or_report(request);
        } else if let Some((k, r)) = commit_pair {
            self.apply_installation_edit(id, |i| {
                i.cup_edge_setback = k;
                i.overlay = r;
            });
        } else if let Some(value) = commit_y[0] {
            // The plate follows the cup.
            let _ = hinge_installation::move_to(&mut self.editor, id, value);
        } else if let Some(placement) = placement {
            let _ = match placement {
                Placement::LineUp => refit
                    .map(|refit| hinge_installation::update(&mut self.editor, refit).map(|_| true))
                    .unwrap_or(Ok(false)),
                Placement::Space => {
                    hinge_installation::space_evenly(&mut self.editor, installation.door_board_id)
                }
            };
        } else if let Some(value) = commit_y[1] {
            self.apply_installation_edit(id, |i| i.mount_y = value);
        }
    }

    pub(crate) fn show_hinge_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.modals.take_hinge() else {
            return;
        };
        let current = draft.project_id == self.editor.project().id
            && draft.revision == self.editor.project().revision;
        let title = self.localizer.text(if draft.id.is_some() {
            "hinge-edit"
        } else {
            "hinge-new"
        });
        let mut chrome = draft.chrome.detach();
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
                ui.spacing_mut().item_spacing.y = 4.0;
                let gap = 12.0;
                let half = ((ui.available_width() - gap) / 2.0).max(80.0);
                let project = self.editor.project();
                let localizer = &self.localizer;
                let (door_before, mount_before) = (draft.door, draft.mount);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for (key, selected) in [
                        ("hinge-door", &mut draft.door),
                        ("hinge-mount", &mut draft.mount),
                    ] {
                        ui.allocate_ui_with_layout(
                            egui::vec2(half, 52.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                field_label(ui, &localizer.text(key));
                                let name = selected
                                    .and_then(|id| project.boards.iter().find(|b| b.id == id))
                                    .map_or("—", |b| b.name.as_str());
                                let chooser = egui::ComboBox::from_id_salt(key)
                                    .width(half)
                                    .selected_text(name)
                                    .show_ui(ui, |ui| {
                                        for board in &project.boards {
                                            crate::combo_option(
                                                ui,
                                                selected,
                                                Some(board.id),
                                                &board.name,
                                            );
                                        }
                                    });
                                if key == "hinge-door" {
                                    first_control = Some(chooser.response.id);
                                }
                            },
                        );
                    }
                });
                if draft.mount != mount_before {
                    draft.mount_auto = false;
                }
                if draft.door != door_before && draft.mount_auto {
                    draft.mount = draft
                        .door
                        .and_then(|door| hinge_installation::likely_mount(project, door));
                }
                draft.refit(project);
                ui.add_space(8.0);
                field_label(ui, &localizer.text("hinge-kit"));
                let name = draft
                    .catalog
                    .and_then(|id| project.catalog.iter().find(|c| c.id == id))
                    .map_or("—", |c| c.name.as_str());
                egui::ComboBox::from_id_salt("hinge-kit")
                    .width(ui.available_width())
                    .selected_text(name)
                    .show_ui(ui, |ui| {
                        for entry in &project.catalog {
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
                ui.add_space(8.0);
                let edge_combo = |ui: &mut egui::Ui, key: &str, edge: &mut BoardEdge| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(half, 52.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            field_label(ui, &localizer.text(key));
                            egui::ComboBox::from_id_salt(key)
                                .width(half)
                                .selected_text(edge_label(localizer, *edge))
                                .show_ui(ui, |ui| {
                                    for value in BoardEdge::ALL {
                                        crate::combo_option(
                                            ui,
                                            edge,
                                            value,
                                            edge_label(localizer, value),
                                        );
                                    }
                                });
                        },
                    );
                };
                let face_combo = |ui: &mut egui::Ui, key: &str, face: &mut BoardFace| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(half, 52.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            field_label(ui, &localizer.text(key));
                            egui::ComboBox::from_id_salt(key)
                                .width(half)
                                .selected_text(face_label(localizer, *face))
                                .show_ui(ui, |ui| {
                                    for value in [BoardFace::MinZ, BoardFace::MaxZ] {
                                        crate::combo_option(
                                            ui,
                                            face,
                                            value,
                                            face_label(localizer, value),
                                        );
                                    }
                                });
                        },
                    );
                };
                if ui
                    .checkbox(&mut draft.auto, localizer.text("hinge-auto-fit"))
                    .on_hover_text(localizer.text("hinge-auto-fit-hint"))
                    .changed()
                {
                    draft.refit(project);
                }
                if let Some(error) = draft.fit_error.filter(|_| draft.auto) {
                    warn_text(
                        ui,
                        localizer.text(match error {
                            FitError::MissingPart => "hinge-fit-missing",
                            FitError::NotParallel => "hinge-fit-not-parallel",
                            FitError::OutsideMount => "hinge-fit-outside",
                        }),
                    );
                }
                let fitted = draft.auto && draft.fit_error.is_none();
                ui.add_space(4.0);
                ui.add_enabled_ui(!fitted, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        edge_combo(ui, "hinge-door-edge", &mut draft.side.door_edge);
                        face_combo(ui, "hinge-door-face", &mut draft.side.door_face);
                    });
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        edge_combo(ui, "hinge-front-edge", &mut draft.side.mount_front_edge);
                        face_combo(ui, "hinge-mount-face", &mut draft.side.mount_face);
                    });
                });
                if let Some(entry) = draft
                    .catalog
                    .and_then(|id| project.catalog.iter().find(|c| c.id == id))
                    .filter(|c| hardware_catalog::is_verified(c))
                    && let Some(facts) = &entry.verified_hinge
                {
                    ui.add_space(8.0);
                    field_label(ui, &localizer.text("hardware-setback-overlay"));
                    let pairs: Vec<_> = facts
                        .overlay_by_cup_edge
                        .iter()
                        .map(|p| (p.cup_edge_setback, p.overlay))
                        .collect();
                    let current = (
                        distance(&draft.values[2]).unwrap_or(Length::from_micrometres(-1)),
                        distance(&draft.values[3]).unwrap_or(Length::from_micrometres(-1)),
                    );
                    if let Some((k, r)) = pair_segmented(ui, localizer, &pairs, current, true) {
                        draft.values[2] = mm(k);
                        draft.values[3] = mm(r);
                    }
                    ui.label(
                        egui::RichText::new(localizer.text("hinge-pair-presets"))
                            .size(11.0)
                            .color(tw::FAINT),
                    );
                }
                ui.add_space(8.0);
                let quarter = ((ui.available_width() - 3.0 * 8.0) / 4.0).max(60.0);
                let mut invalid_key = None;
                let inset = draft
                    .catalog
                    .and_then(|id| project.catalog.iter().find(|c| c.id == id))
                    .and_then(hardware_catalog::facts)
                    .is_some_and(|facts| facts.arm.is_inset());
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    for (index, key) in
                        ["hinge-door-y", "hinge-mount-y", "hinge-k", "hinge-overlay"]
                            .iter()
                            .enumerate()
                    {
                        ui.allocate_ui_with_layout(
                            egui::vec2(quarter, 52.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                let short = localizer.text(match index {
                                    0 => "hardware-dialog-door-y",
                                    1 => "hardware-plate-y",
                                    2 => "hardware-dialog-k",
                                    _ if inset => "hardware-dialog-f",
                                    _ => "hardware-dialog-r",
                                });
                                field_label(ui, &short);
                                let invalid = distance(&draft.values[index]).is_none();
                                if invalid {
                                    invalid_key = Some(*key);
                                }
                                let response = tw::value_field(
                                    ui,
                                    egui::Id::new(("hinge-value-field", index)),
                                    &localizer.text(key),
                                    &mut draft.values[index],
                                    quarter,
                                    Some("mm"),
                                    None,
                                    !(index == 1 && fitted),
                                    invalid,
                                )
                                .on_hover_text(localizer.text(key));
                                if index == 0 && response.changed() {
                                    draft.door_y_auto = false;
                                }
                            },
                        );
                    }
                });
                if inset {
                    ui.add_space(4.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(2.0 * quarter + 8.0, 52.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            field_label(ui, &localizer.text("hinge-inset-depth"));
                            let invalid = distance(&draft.inset_depth).is_none();
                            if invalid {
                                invalid_key = Some("hinge-inset-depth");
                            }
                            tw::value_field(
                                ui,
                                egui::Id::new(("hinge-value-field", 4)),
                                &localizer.text("hinge-inset-depth"),
                                &mut draft.inset_depth,
                                quarter,
                                Some("mm"),
                                None,
                                true,
                                invalid,
                            )
                            .on_hover_text(localizer.text("hinge-inset-depth-hint"));
                        },
                    );
                }
                if let Some(key) = invalid_key {
                    ui.label(
                        egui::RichText::new(format!(
                            "{}: {}",
                            localizer.text(key),
                            localizer.text("hinge-distance-invalid")
                        ))
                        .size(11.5)
                        .color(tw::DANGER),
                    );
                }
                // A door position typed this frame moves the plate before preview.
                draft.refit(project);
                ui.add_space(8.0);
                let proposed = current.then(|| draft.proposed(self)).flatten();
                if let Some(ref proposed) = proposed {
                    if let Ok(status) = hinge_installation::preview(project, proposed) {
                        status_ui(ui, localizer, project, &status, proposed);
                    }
                } else {
                    ui.label(
                        egui::RichText::new(localizer.text("hinge-invalid"))
                            .size(11.5)
                            .color(tw::DANGER),
                    );
                }
                if draft.error {
                    ui.label(
                        egui::RichText::new(localizer.text("hinge-invalid"))
                            .size(11.5)
                            .color(tw::DANGER),
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
        draft.chrome = chrome;
        self.modals.set_hinge(Some(draft));
    }
}

/// Snapshot details shown when browsing pinned catalog records.
fn snapshot_details(ui: &mut egui::Ui, localizer: &Localizer, entry: &CatalogReference) {
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
    ui.add(egui::Label::new(tw::medium(ui, &entry.name, 12.5).color(tw::TEXT)).wrap());
    ui.label(
        tw::mono(
            format!(
                "{} · {}",
                entry.product_id,
                entry.plate_id.as_deref().unwrap_or("—")
            ),
            10.5,
        )
        .color(tw::FAINT),
    );
    let small = |ui: &mut egui::Ui, text: String| {
        ui.add(egui::Label::new(egui::RichText::new(text).size(11.0).color(tw::MUTED)).wrap());
    };
    small(
        ui,
        format!(
            "{} {}",
            localizer.text("hardware-rev-short"),
            short_revision(&entry.revision)
        ),
    );
    if let Some(facts) = verified_facts(entry) {
        small(
            ui,
            format!(
                "K/R {}",
                facts
                    .overlay_by_cup_edge
                    .iter()
                    .map(|p| format!(
                        "{}/{}",
                        short_mm(localizer, p.cup_edge_setback),
                        short_mm(localizer, p.overlay)
                    ))
                    .collect::<Vec<_>>()
                    .join(" · ")
            ),
        );
        small(
            ui,
            format!("{} · PDF {}", facts.attribution, facts.pdf_page),
        );
    } else {
        ui.label(
            egui::RichText::new(localizer.text("hinge-evidence-unavailable"))
                .size(11.0)
                .color(tw::WARN_INK),
        );
    }
    ui.hyperlink_to(
        egui::RichText::new(localizer.text("hinge-source-review"))
            .size(11.0)
            .color(tw::ACCENT_DARK),
        &entry.source,
    );
}
#[cfg(test)]
mod tests;
